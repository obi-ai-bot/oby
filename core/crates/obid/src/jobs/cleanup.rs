//! The cleanup pass (docs/runner-api.md, `cleanup`): git's daily tidy of each clone, a settled
//! job's worktree removed once it holds nothing that exists nowhere else, and an ended job's build
//! outputs freed (CLEANUP-1, CLEANUP-2). It never writes a job's branch or its commits.

use super::*;

impl Jobs {
    /// Lets git tidy each clone once a day ([`Jobs::tidy_clone`]).
    pub(super) async fn tidy_clones(self: Arc<Self>) {
        loop {
            tokio::time::sleep(TIDY_EVERY).await;
            let repos: Vec<String> = self
                .config
                .repos
                .get()
                .iter()
                .map(|r| r.repo.clone())
                .collect();
            for repo in repos {
                match self.tidy_clone(&repo).await {
                    Ok(true) => tracing::info!("{repo}: obid's clone tidied (git gc --auto)"),
                    Ok(false) => {}
                    Err(e) => tracing::info!("{repo}: obid's clone wasn't tidied: {e}"),
                }
            }
        }
    }

    /// `git gc --auto` in obid's clone of `repo`, which runs no maintenance by itself
    /// (git::ensure_clone): only while no job uses the repository, under the lock that gets one
    /// ready. False when it didn't run.
    pub(super) async fn tidy_clone(&self, repo: &str) -> Result<bool, String> {
        let clone = git::clone_path(&self.config.home, repo);
        if !clone.join(".git").is_dir() {
            return Ok(false);
        }
        let preparing = self.preparing(repo);
        let Ok(_held) = preparing.try_lock() else {
            return Ok(false);
        };
        let busy = self
            .lock()
            .jobs
            .values()
            .any(|job| job.repo == repo && job.status.is_active());
        if busy {
            return Ok(false);
        }
        // git's own threshold, for this run only (the clone's gc.auto is 0), and in the foreground,
        // so it ends while the lock is held.
        git::git(
            &clone,
            &[
                "-c",
                "gc.auto=6700",
                "-c",
                "gc.autoDetach=false",
                "gc",
                "--auto",
                "--quiet",
            ],
        )
        .await
        .map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// The cleanup pass (docs/runner-api.md, `cleanup`): removes the worktree of every settled job
    /// ([`settled_in`]) whose worktree holds nothing that exists nowhere else
    /// ([`Jobs::worktree_holds_work`]), through obid's clone, and keeps the rest in `status.cleanup.kept`.
    /// `cleared` is when Clear done was last pressed, `wanted` the bytes the disk lacks
    /// ([`disk_wanted`]). A job that ended ([`at_rest`]) loses its builds now
    /// ([`Jobs::free_build_outputs`]). Answers how many worktrees it removed or freed space in.
    pub async fn clean_worktrees(&self, now: i64, cleared: Option<i64>, wanted: u64) -> usize {
        let settled: Vec<Job> = {
            let state = self.lock();
            state
                .jobs
                .values()
                .filter(|job| settled_in(&state, job, now, cleared))
                .filter(|job| self.worktree_path(&job.id).exists())
                .cloned()
                .collect()
        };
        let (mut removed, mut kept) = (0, Vec::new());
        let still_settled = |id: &str| {
            let state = self.lock();
            state
                .jobs
                .get(id)
                .filter(|j| settled_in(&state, j, now, cleared))
                .cloned()
        };
        for job in settled {
            if self.shutting_down.load(Ordering::Relaxed) {
                break;
            }
            // Under the lock a job's worktree is made with, checked again: one that runs again
            // meanwhile waits, then makes it afresh from its branch.
            let preparing = self.preparing(&job.repo);
            let _preparing = preparing.lock().await;
            let Some(job) = still_settled(&job.id) else {
                continue;
            };
            if let Some(why) = self.worktree_holds_work(&job).await {
                kept.push(KeptWorktree { job: job.id, why });
                continue;
            }
            if still_settled(&job.id).is_none() {
                continue;
            }
            let clone = self.clone_for(&job);
            // Its HOME first: once the worktree is gone, so is what its runs kept there.
            let _ = std::fs::remove_dir_all(self.temp_path(&job.id));
            remove_worktree(&clone, &self.worktree_path(&job.id)).await;
            tracing::info!("job {}: its worktree was cleaned up", job.id);
            self.update(&job.id, |j| j.worktree = None);
            removed += 1;
        }
        // Every job that ended frees its builds once (CLEANUP-1); on a disk low on space, every
        // job not queued or running again, oldest first, until there's enough (CLEANUP-2).
        let mut built: Vec<(i64, String, String)> = {
            let state = self.lock();
            state
                .jobs
                .values()
                .filter(|job| at_rest(&state, job) && (wanted > 0 || job.builds_left()))
                .filter(|job| self.worktree_path(&job.id).exists())
                .map(|job| (job.finished.unwrap_or(0), job.id.clone(), job.repo.clone()))
                .collect()
        };
        built.sort();
        let mut freed = 0;
        for (_, id, repo) in built {
            if self.shutting_down.load(Ordering::Relaxed) {
                break;
            }
            let preparing = self.preparing(&repo);
            let _preparing = preparing.lock().await;
            let left = {
                let state = self.lock();
                let job = state.jobs.get(&id).filter(|j| at_rest(&state, j));
                job.map(Job::builds_left)
            };
            if left.is_none_or(|left| !left && freed >= wanted) || !self.worktree_path(&id).exists()
            {
                continue;
            }
            // Tried once per end, freed or not: a worktree git can't answer for stays as it is.
            let bytes = match self.free_build_outputs(&id).await {
                Ok(bytes) => bytes,
                Err(why) => {
                    tracing::info!("job {id}: its build outputs stay: {why}");
                    0
                }
            };
            if bytes > 0 {
                tracing::info!("job {id}: its build outputs went, {bytes} bytes");
                removed += 1;
            }
            freed += bytes;
            if bytes > 0 || left == Some(true) {
                let freed = Some(crate::job::BuildFreed { bytes, at: now });
                self.update(&id, |j| j.build_freed = freed);
            }
        }
        let mut cleaned = self.cleaned.lock().unwrap_or_else(|e| e.into_inner());
        (cleaned.last_run, cleaned.kept) = (Some(now), kept);
        if freed > 0 {
            let why = match wanted {
                0 => "Jobs ended.".into(),
                _ => format!("The disk had {} GB free.", (30 * GB - wanted) / GB),
            };
            cleaned.freed = Some(Freed { bytes: freed, why });
        }
        removed
    }

    /// Why the worktree of `job`, settled, must stay, or none: what's in it may exist nowhere
    /// else. A folder that isn't a git worktree (an agent can delete its `.git`) unless it's empty,
    /// artifacts that weren't all published (`.obi/artifacts` is never a change git sees),
    /// uncommitted changes (untracked files too, ignored ones not), or a HEAD that is neither where
    /// the job started, nor what it pushed, nor on any of the clone's remote branches. git runs in
    /// the job's sandbox, as for `keep_wip`: the agent could write the worktree's `.git` and point
    /// it at a repository whose filters would run. Whatever can't be answered keeps it too. A
    /// settled job's branch in the clone holds its commits, and obid makes the worktree again from
    /// there when the job runs again.
    async fn worktree_holds_work(&self, job: &Job) -> Option<String> {
        let worktree = self.worktree_path(&job.id);
        if std::fs::symlink_metadata(worktree.join(".git")).is_err() {
            let empty = std::fs::read_dir(&worktree).is_ok_and(|mut d| d.next().is_none());
            return (!empty).then(|| "It isn't a git worktree.".into());
        }
        if !job.artifact_problems.is_empty() {
            return Some("Some of its artifacts weren't published.".into());
        }
        if let Err(why) = self.own_git_dirs(&job.id).await {
            return Some(why);
        }
        let status = ["status", "--porcelain", "--untracked-files=normal"];
        match self.sandboxed_git(&job.id, &status, None).await {
            Ok(changes) if changes.is_empty() => {}
            Ok(_) => return Some("It has uncommitted changes.".into()),
            Err(e) => {
                return Some(format!(
                    "obid can't tell whether it has uncommitted changes: {e}"
                ));
            }
        }
        let head = match self
            .sandboxed_git(&job.id, &["rev-parse", "--verify", "--quiet", "HEAD"], None)
            .await
        {
            Ok(head) => String::from_utf8_lossy(&head).trim().to_owned(),
            Err(e) => return Some(format!("obid can't read its HEAD: {e}")),
        };
        if !git::is_sha(&head) {
            return Some("Its HEAD isn't a commit.".into());
        }
        if job.start_commit.as_deref() == Some(head.as_str())
            || job.pushed_head() == Some(head.as_str())
        {
            return None;
        }
        let clone = self.clone_for(job);
        match git::git(
            &clone,
            &[
                "for-each-ref",
                "--count=1",
                "--contains",
                &head,
                "refs/remotes",
            ],
        )
        .await
        {
            Ok(refs) if !refs.is_empty() => None,
            _ => Some("It has commits that aren't on the remote.".into()),
        }
    }

    /// Deletes the build outputs in a job's worktree ([`build_outputs`]: a build makes them again)
    /// and its HOME's caches (those of its own folders it takes: CLEANUP-1), and answers how many
    /// bytes that freed. Only folders git ignores: never the job's work, which is its commits, nor
    /// its login copies. git lists them and `rm` takes them in the job's sandbox, so a link the
    /// agent left leads nowhere outside its worktree.
    async fn free_build_outputs(&self, id: &str) -> Result<u64, String> {
        let ignored = [
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ];
        let listed = self.sandboxed_git(id, &ignored, None).await?;
        let (worktree, home) = (self.worktree_path(id), self.temp_path(id));
        let mut outputs = build_outputs(&worktree, &String::from_utf8_lossy(&listed));
        let entries = std::fs::read_dir(&home).into_iter().flatten().flatten();
        let listed: String = (entries.map(|e| format!("{}/\0", e.file_name().display()))).collect();
        outputs.extend(
            build_outputs(&home, &listed)
                .into_iter()
                .map(|dir| home.join(dir)),
        );
        if outputs.is_empty() {
            return Ok(0);
        }
        // The walks off the runtime's threads: a target/ can hold a million files.
        let size = |worktree: PathBuf, outputs: Vec<PathBuf>| async move {
            tokio::task::spawn_blocking(move || {
                outputs
                    .iter()
                    .map(|dir| crate::api::disk_usage(&worktree.join(dir)))
                    .sum::<u64>()
            })
            .await
            .unwrap_or(0)
        };
        let before = size(worktree.clone(), outputs.clone()).await;
        let mut rm: Vec<OsString> = vec!["/bin/rm".into(), "-rf".into(), "--".into()];
        rm.extend(outputs.iter().map(|dir| dir.clone().into_os_string()));
        let removed = self
            .sandboxed(id, &rm, None, SANDBOXED_GIT_LIMIT, usize::MAX)
            .await;
        // What went is freed, all of it or not: an rm that stopped partway says why in the log.
        let freed = before.saturating_sub(size(worktree, outputs).await);
        if let Err(why) = removed {
            tracing::info!("job {id}: {freed} bytes of its build outputs went, not all: {why}");
        }
        Ok(freed)
    }

    /// Waits until a job may have settled (it ended, was dismissed or archived, or Clear done was
    /// pressed), or `most` passed: answers whether one may have.
    pub async fn wait_to_clean(&self, most: Duration) -> bool {
        tokio::time::timeout(most, self.cleaning.notified())
            .await
            .is_ok()
    }

    /// Wakes the cleanup pass: Clear done may have settled jobs.
    pub fn clean_soon(&self) {
        self.cleaning.notify_one();
    }

    /// What the last cleanup pass did.
    pub fn cleaned(&self) -> Cleaned {
        self.cleaned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}
