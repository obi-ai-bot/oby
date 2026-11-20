//! The cleanup pass (docs/runner-api.md, `cleanup`): git's daily tidy of each clone, a settled
//! job's worktree removed once it holds nothing that exists nowhere else (CLEANUP-7: not even an
//! edit `git status` doesn't show), and an ended job's build outputs freed (CLEANUP-1,
//! CLEANUP-2). It never writes a job's branch or its commits.

use super::*;

/// What [`Jobs::worktree_holds_work`] says of a worktree whose edits `git status` doesn't show
/// (CLEANUP-7).
pub(super) const HIDDEN_CHANGES: &str = "It has uncommitted changes git status doesn't show.";

/// The entries of `git ls-files --stage -v -z` flagged skip-worktree (`S`) or assume-unchanged (a
/// lowercase tag), which `git status` doesn't look at: each one's mode, object id and path, a
/// submodule's left out (its own repository says what it holds).
fn flagged(listed: &[u8]) -> Vec<(&[u8], &[u8], &[u8])> {
    listed
        .split(|&b| b == 0)
        .filter_map(|entry| {
            let (&tag, rest) = entry.split_first()?;
            let tab = rest.iter().position(|&b| b == b'\t')?;
            let (fields, path) = (&rest[..tab], &rest[tab + 1..]);
            let mut fields = fields.split(|&b| b == b' ').filter(|f| !f.is_empty());
            let (mode, object) = (fields.next()?, fields.next()?);
            (tag == b'S' || tag.is_ascii_lowercase())
                .then_some((mode, object, path))
                .filter(|_| mode != b"160000")
        })
        .collect()
}

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
    /// ([`free::settled_in`]) whose worktree holds nothing that exists nowhere else
    /// ([`Jobs::worktree_holds_work`]), through obid's clone ([`Jobs::free_worktrees`]), and keeps
    /// the rest in `status.cleanup.kept`.
    /// `cleared` is when Clear done was last pressed, `wanted` the bytes the disk lacks
    /// ([`disk_wanted`]). A job that ended ([`at_rest`]) loses its builds now
    /// ([`Jobs::free_build_outputs`]). Answers how many worktrees it removed or freed space in.
    pub async fn clean_worktrees(&self, now: i64, cleared: Option<i64>, wanted: u64) -> usize {
        let (mut removed, kept) = self.free_worktrees(now, cleared).await;
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
    pub(super) async fn worktree_holds_work(&self, job: &Job) -> Option<String> {
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
            Ok(_) => return Some(UNCOMMITTED.into()),
            Err(e) => {
                return Some(format!(
                    "obid can't tell whether it has uncommitted changes: {e}"
                ));
            }
        }
        match self.hides_changes(&job.id).await {
            Ok(false) => {}
            Ok(true) => return Some(HIDDEN_CHANGES.into()),
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

    /// Whether the job's worktree holds an edit `git status` doesn't show (CLEANUP-7): a tracked
    /// file flagged skip-worktree or assume-unchanged ([`flagged`]: an agent's `git update-index`,
    /// a sparse checkout), there on disk and not what git holds for it. Status and `git diff`
    /// never look at those files, as T3 Code found (pingdotgg/t3code fe565c8958). A flagged file
    /// that isn't on disk loses nothing; one obid can't compare (a link, a folder, a name with a
    /// newline) counts as changed. git runs in the job's sandbox, as for the status before it.
    async fn hides_changes(&self, id: &str) -> Result<bool, String> {
        use std::os::unix::ffi::OsStrExt;
        let listed = self
            .sandboxed_git(id, &["ls-files", "--stage", "-v", "-z"], None)
            .await?;
        let worktree = self.worktree_path(id);
        let (mut paths, mut held) = (Vec::new(), Vec::new());
        for (mode, object, path) in flagged(&listed) {
            let on_disk =
                std::fs::symlink_metadata(worktree.join(std::ffi::OsStr::from_bytes(path)));
            match on_disk {
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) => {}
                Ok(meta)
                    if meta.is_file() && mode.starts_with(b"1006") && !path.contains(&b'\n') =>
                {
                    paths.extend_from_slice(path);
                    paths.push(b'\n');
                    held.push(object);
                }
                _ => return Ok(true),
            }
        }
        if held.is_empty() {
            return Ok(false);
        }
        let hashed = self
            .sandboxed_git(id, &["hash-object", "--stdin-paths"], Some(paths))
            .await?;
        let on_disk: Vec<&[u8]> = hashed
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .collect();
        Ok(on_disk.len() != held.len() || on_disk.iter().zip(&held).any(|(disk, git)| disk != git))
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

#[cfg(test)]
mod tests {
    use super::super::tests::{cleanup_git, cleanup_home, ended, with_pr};
    use super::*;

    /// CLEANUP-7 (docs/contracts/cleanup.md): a worktree whose edit `git status` doesn't show,
    /// because its file is marked skip-worktree or assume-unchanged, stays on the pass, in
    /// `status.cleanup.kept` with why. A flagged file that is as git holds it, or not on disk at
    /// all (a sparse checkout's), keeps nothing: that worktree goes.
    #[tokio::test]
    async fn cleanup_7_edits_git_status_hides_keep_the_worktree() {
        let (root, clone, _, jobs) = cleanup_home("hidden", true);
        std::fs::write(clone.join("README.md"), "base\n").unwrap();
        std::fs::write(clone.join("notes.md"), "notes\n").unwrap();
        cleanup_git(&clone, &["add", "README.md", "notes.md"]);
        cleanup_git(&clone, &["commit", "-q", "-m", "files"]);
        let base = cleanup_git(&clone, &["rev-parse", "HEAD"]);
        cleanup_git(&clone, &["update-ref", "refs/remotes/origin/main", &base]);
        let now = obi_protocol::time::now();
        let job = |id: &str, change: &dyn Fn(&Path)| {
            let (job, tree) = ended(&jobs, &clone, &base, id, now);
            change(&tree);
            jobs.lock()
                .jobs
                .insert(id.into(), with_pr(job, "MERGED", now));
            tree
        };
        let flag = |tree: &Path, flag: &str, file: &str| {
            cleanup_git(tree, &["update-index", flag, file]);
        };
        let skipped = job("skipped", &|tree| {
            std::fs::write(tree.join("README.md"), "mine\n").unwrap();
            flag(tree, "--skip-worktree", "README.md");
        });
        let assumed = job("assumed", &|tree| {
            std::fs::write(tree.join("notes.md"), "mine\n").unwrap();
            flag(tree, "--assume-unchanged", "notes.md");
        });
        let unchanged = job("unchanged", &|tree| {
            flag(tree, "--skip-worktree", "README.md");
            flag(tree, "--assume-unchanged", "notes.md");
        });
        let sparse = job("sparse", &|tree| {
            flag(tree, "--skip-worktree", "README.md");
            std::fs::remove_file(tree.join("README.md")).unwrap();
        });
        let status = cleanup_git(&skipped, &["status", "--porcelain"]);
        assert_eq!(
            status, "",
            "git status shows the edit, so this proves nothing"
        );

        jobs.clean_worktrees(now, None, 0).await;
        assert!(
            skipped.join("README.md").exists(),
            "the skip-worktree edit was lost"
        );
        assert!(
            assumed.join("notes.md").exists(),
            "the assume-unchanged edit was lost"
        );
        assert!(
            !unchanged.exists(),
            "a flagged file as git holds it kept its worktree"
        );
        assert!(
            !sparse.exists(),
            "a flagged file not on disk kept its worktree"
        );
        let mut kept: Vec<(String, String)> = (jobs.cleaned().kept.into_iter())
            .map(|k| (k.job, k.why))
            .collect();
        kept.sort();
        let said = |job: &str| (job.to_owned(), HIDDEN_CHANGES.to_owned());
        assert_eq!(kept, vec![said("assumed"), said("skipped")]);
        assert!(jobs.get("skipped").unwrap().worktree_freed.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// CLEANUP-7: which `git ls-files --stage -v -z` entries `git status` doesn't look at.
    #[test]
    fn cleanup_7_flagged_reads_skip_worktree_and_assume_unchanged() {
        let id = "e69de29bb2d1d6434b8b29ae775ad8c2b48c5391";
        let listed = format!(
            "H 100644 {id} 0\tplain\0S 100644 {id} 0\tskipped\0h 100755 {id} 0\tsome dir/assumed\0s 160000 {id} 0\tmodule\0"
        );
        let paths: Vec<&[u8]> = flagged(listed.as_bytes())
            .into_iter()
            .map(|(_, _, p)| p)
            .collect();
        assert_eq!(paths, vec![&b"skipped"[..], &b"some dir/assumed"[..]]);
    }
}
