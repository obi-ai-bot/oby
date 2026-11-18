//! obid's own git in a job's worktree, in its sandbox and pinned to the git folders obid checked:
//! its head, its commits kept in the clone, rebasing them onto the base, and work left
//! uncommitted. It never pushes.

use super::*;

impl Jobs {
    /// The job `id`'s worktree head, read through the git folders obid checked, when nothing is
    /// left uncommitted there; else none. Its new commits are still in the job's own object
    /// folder, so what needs them runs in its sandbox, as [`Jobs::keep_commits`]' packing does.
    async fn clean_head(&self, id: &str) -> Option<String> {
        let worktree = self.worktree_path(id);
        let (admin, common) = self.own_git_dirs(id).await.ok()?;
        let folders = git_folders(&admin, &common, &worktree);
        let head = git::pinned(
            &worktree,
            &folders,
            &["rev-parse", "--verify", "--quiet", "HEAD"],
        )
        .await
        .ok()
        .filter(|h| git::is_sha(h))?;
        let left = self
            .sandboxed_git_within(
                id,
                &["status", "--porcelain"],
                None,
                SANDBOXED_GIT_LIMIT,
                CHANGED_MAX,
            )
            .await
            .ok()?;
        if !left.is_empty() {
            tracing::info!("job {id}: its check passed with work left uncommitted");
        }
        left.is_empty().then_some(head)
    }

    /// What a passing check of the job `id` with `command` counts for at merge (`Job::checked`):
    /// the clean head it passed on, and why it can't stand for the Mac's checks, from the files its
    /// change touches against its base (crate::merge::check_apart).
    pub(super) async fn checked(&self, id: &str, command: &str) -> Option<crate::job::Checked> {
        let commit = self.clean_head(id).await?;
        let job = self.get(id)?;
        let changed = self.changed_files(&job, &commit).await;
        Some(crate::job::Checked {
            apart: crate::merge::check_apart(&job, command, changed.as_deref()),
            commit,
        })
    }

    /// The files `head` changes against the job's base as the clone has it
    /// (`origin/<base>...<head>`), read in its sandbox; none when they can't be read.
    pub(super) async fn changed_files(&self, job: &Job, head: &str) -> Option<Vec<String>> {
        let base = job
            .base
            .clone()
            .or_else(|| self.repo(&job.repo).map(|r| r.default_branch.clone()))
            .filter(|b| git::is_branch_name(b))?;
        let range = format!("refs/remotes/origin/{base}...{head}");
        let names = self
            .sandboxed_git_within(
                &job.id,
                &["diff", "--name-only", "--no-renames", "-z", &range, "--"],
                None,
                SANDBOXED_GIT_LIMIT,
                CHANGED_MAX,
            )
            .await
            .ok()?;
        Some(
            names
                .split(|b| *b == 0)
                .filter(|f| !f.is_empty())
                .map(|f| String::from_utf8_lossy(f).into_owned())
                .collect(),
        )
    }

    /// Takes what an owner's job committed into its clone and moves its branch there: the
    /// worktree's HEAD commit, which it answers.
    ///
    /// In the sandbox the job's HEAD is detached and its new objects are in its own object folder
    /// (Spec::git_env); the clone's refs and objects were never writable. The objects the clone
    /// doesn't have are packed inside the job's sandbox again (so git reads nothing the job
    /// couldn't, whatever links or alternates it left in its object folder), indexed into the
    /// clone outside it ([`git::take_pack`] checks every object), and then the branch moves.
    pub(super) async fn keep_commits(&self, id: &str) -> Result<String, String> {
        let job = self.get(id).ok_or("the job is gone")?;
        let worktree = self.worktree_path(id);
        if !worktree.join(".git").exists() {
            return Err("the job has no worktree".into());
        }
        let (admin, common) = self.own_git_dirs(id).await?;
        let folders = git_folders(&admin, &common, &worktree);
        let clone = common
            .parent()
            .ok_or("the clone's git directory has no folder")?
            .to_path_buf();
        let head = git::pinned(
            &worktree,
            &folders,
            &["rev-parse", "--verify", "--quiet", "HEAD"],
        )
        .await
        .map_err(|e| e.to_string())?;
        if !git::is_sha(&head) {
            return Err(format!("the worktree's HEAD isn't a commit: {head}"));
        }
        let pack = if git::has_commit(&clone, &head).await {
            None
        } else {
            let objects = self
                .sandboxed_git(
                    id,
                    &[
                        "rev-list",
                        "--objects",
                        &head,
                        "--not",
                        "--branches",
                        "--remotes",
                        "--tags",
                    ],
                    None,
                )
                .await?;
            Some(
                self.sandboxed_git(id, &["pack-objects", "--stdout", "--quiet"], Some(objects))
                    .await?,
            )
        };
        // Into the clone under the lock a publish fetches the base with (#628): a pack and a
        // branch landing mid-fetch can fail it.
        let fetching = self.preparing(&job.repo);
        let _fetching = fetching.lock().await;
        if let Some(pack) = pack {
            git::take_pack(&clone, pack)
                .await
                .map_err(|e| e.to_string())?;
        }
        git::move_branch(&clone, &job.branch, &head)
            .await
            .map_err(|e| e.to_string())?;
        // PUBLISHING-2: others checked on another commit can't publish it; this one checks again.
        let moved: Vec<String> = self
            .lock()
            .jobs
            .values()
            .filter(|j| {
                j.repo == job.repo
                    && j.id != id
                    && same_branch(&j.branch, &job.branch)
                    && j.checked_head.as_ref().is_some_and(|c| *c != head)
            })
            .map(|j| j.id.clone())
            .collect();
        for other in moved {
            self.update(&other, |j| {
                j.checked_head = None;
                j.publish_error = Some(crate::publish::branch_moved(&j.branch));
            });
        }
        Ok(head)
    }

    /// The job's git folders where obid makes them (`clone/.git/worktrees/<id>`, beside `git
    /// worktree add home/worktrees/<id>`, and `clone/.git`), as the variables that pin a git call in
    /// its worktree there ([`git_folders`]): for calls that can't wait on [`Jobs::own_git_dirs`].
    /// No pointer is read at all; folders that aren't the job's make the call fail.
    pub(super) fn made_git_folders(&self, job: &Job) -> Vec<(&'static str, String)> {
        let common = git::clone_path(&self.config.home, &job.repo).join(".git");
        let admin = common.join("worktrees").join(&job.id);
        git_folders(&admin, &common, &self.worktree_path(&job.id))
    }

    /// [`Jobs::own_git_dirs`] as the variables that pin obid's own git calls in the worktree there
    /// ([`git::pinned`], [`git_folders`]).
    pub(super) async fn pinned_git(&self, id: &str) -> Result<Vec<(&'static str, String)>, String> {
        let (admin, common) = self.own_git_dirs(id).await?;
        Ok(git_folders(&admin, &common, &self.worktree_path(id)))
    }

    /// Runs git in the job's worktree inside its sandbox, without the network, so git reads and
    /// runs nothing the job couldn't (no hooks, no fsmonitor): its output.
    pub(super) async fn sandboxed_git(
        &self,
        id: &str,
        args: &[&str],
        input: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, String> {
        self.sandboxed_git_within(id, args, input, SANDBOXED_GIT_LIMIT, usize::MAX)
            .await
    }

    /// [`Self::sandboxed_git`], ended after `limit`, and refused when it prints more than `max`
    /// bytes.
    pub(super) async fn sandboxed_git_within(
        &self,
        id: &str,
        args: &[&str],
        input: Option<Vec<u8>>,
        limit: Duration,
        max: usize,
    ) -> Result<Vec<u8>, String> {
        let mut argv: Vec<OsString> = vec![
            "git".into(),
            "-c".into(),
            "core.hooksPath=/dev/null".into(),
            "-c".into(),
            "core.fsmonitor=false".into(),
        ];
        argv.extend(args.iter().map(OsString::from));
        self.sandboxed(id, &argv, input, limit, max).await
    }

    /// Runs `argv` in the job's sandbox, in its worktree, as its git runs there
    /// ([`Self::sandboxed_git`]): no network, and nothing to write but the worktree and the job's
    /// own git folders.
    pub(super) async fn sandboxed(
        &self,
        id: &str,
        argv: &[OsString],
        input: Option<Vec<u8>>,
        limit: Duration,
        max: usize,
    ) -> Result<Vec<u8>, String> {
        let sandbox = self.sandbox.as_ref().map_err(String::clone)?;
        let worktree = self.worktree_path(id);
        let (admin, common) = self.own_git_dirs(id).await?;
        // The job's own object folder, which git writes to here, as a run makes it: an Obi-to-Obi
        // review's worktree has none.
        own_objects(&admin)?;
        let temp = self.temp_path(id);
        prepare_folder(&temp).map_err(|e| e.to_string())?;
        let mut spec = self.spec(&worktree, &admin, &common, &temp, true, Tool::Claude);
        spec.network = false;
        spec.read_only.clear();
        let mut env = process::sandbox_env(&process::tools_path(), &temp, &spec.git_env());
        // Each call's own `TMPDIR` inside the job's, so the exit its macOS watcher records there
        // is its own: two of the job's git calls at once (two reads of its changes) never clear or
        // overwrite each other's, and a call that passed is never read as killed (#2529).
        let own = temp.join(format!(".git-{}", uuid::Uuid::new_v4()));
        prepare_folder(&own).map_err(|e| e.to_string())?;
        if let Some((_, dir)) = env.iter_mut().find(|(name, _)| name == "TMPDIR") {
            *dir = own.clone().into_os_string();
        }
        let out = sandboxed_output(
            &sandbox.wrap(&spec, argv),
            &env,
            &worktree,
            input,
            limit,
            max,
        )
        .await;
        let _ = std::fs::remove_dir_all(&own);
        out
    }

    /// Puts the job's own commits on its base as the remote has it now: the base fetched into
    /// obid's clone, and when it moved (or was force-pushed) since the job's commits started at
    /// `start`, those commits (`start..HEAD`) rebased onto it in the job's sandbox, HEAD detached at
    /// the result as before; `start_commit` is the new base from then on. Only a branch obid made for the job
    /// ([`Job::own_branch`]) that started from the base (never a pull request's or another job's, whose commits
    /// aren't all its own), with nothing left uncommitted. A conflict aborts the rebase and says
    /// which files: its agent gets one round to rebase it ([`rebase_prompt`]). A base that can't
    /// be fetched leaves it as it is.
    pub(super) async fn onto_base(&self, id: &str, start: &str) -> Onto {
        let Some(job) = self.get(id) else {
            return Onto::Same;
        };
        // A rebase its agent left half done (conflicts unresolved, its commits partly applied) is
        // no rebase: aborted first, before anything else can stop here (a fetch that fails), so
        // nothing half done is ever checked or published. For a job obid rebases, it counts as its
        // round's failure.
        let half = self.abort_half_rebase(id).await;
        let Some(repo) = self.repo(&job.repo) else {
            return Onto::Same;
        };
        let base = job.base.clone().unwrap_or(repo.default_branch.clone());
        if !crate::merge::own_branch(&job, &job.branch)
            || job.pr.is_some()
            || job.start_from.is_some()
            || job.review_of.is_some()
            || !git::is_branch_name(&base)
            || !git::is_sha(start)
        {
            return Onto::Same;
        }
        if let Some(files) = half {
            return Onto::Conflict(files);
        }
        let clone = self.clone_for(&job);
        let tracking = format!("refs/remotes/origin/{base}");
        {
            let preparing = self.preparing(&repo.repo);
            let _fetching = preparing.lock().await;
            if let Err(e) = git::fetch(
                &clone,
                &repo.remote,
                &[format!("+refs/heads/{base}:{tracking}")],
            )
            .await
            {
                tracing::info!("job {id}: {base} didn't fetch, so it isn't rebased: {e}");
                return Onto::Same;
            }
            Box::pin(self.taint_unless_ours(
                &clone,
                &repo,
                &tracking,
                Some(&base),
                &format!("the branch {base}, fetched as a job's base"),
            ))
            .await;
        }
        let Ok(now) = git::git(
            &clone,
            &["rev-parse", "--verify", &format!("{tracking}^{{commit}}")],
        )
        .await
        else {
            return Onto::Same;
        };
        let on = |commit: &str| {
            let commit = commit.to_owned();
            async move {
                self.sandboxed_git(id, &["merge-base", "--is-ancestor", &commit, "HEAD"], None)
                    .await
                    .is_ok()
            }
        };
        if on(&now).await {
            if now == start {
                return Onto::Same;
            }
            // Its commits sit on the base as it is now (its agent rebased them, or its base only
            // moved forward under them since): they start there from now on. Not when the base went
            // back under where they started: the commits it dropped aren't the job's, and go below.
            let went_back = git::git(&clone, &["merge-base", "--is-ancestor", &now, start])
                .await
                .is_ok();
            if !went_back || !on(start).await {
                self.update(id, |job| job.start_commit = Some(now.clone()));
                return Onto::Rebased(now);
            }
        }
        if self.uncommitted(id).await {
            return Onto::Same;
        }
        // What it rewrites: recorded again after, only when all of it was in the record.
        let rewrote: Vec<String> =
            Box::pin(self.sandboxed_git(id, &["rev-list", &format!("{start}..HEAD")], None))
                .await
                .map(|listed| {
                    String::from_utf8_lossy(&listed)
                        .lines()
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_else(|_| vec![String::new()]);
        let (name, email) = &self.identity();
        let (name, email) = (format!("user.name={name}"), format!("user.email={email}"));
        let mut args = git::LFS_POINTERS.to_vec();
        args.extend([
            "-c",
            &name,
            "-c",
            &email,
            "-c",
            "commit.gpgsign=false",
            "rebase",
            "--quiet",
            "--no-autostash",
            "--onto",
            &now,
            start,
        ]);
        let rebased = self.sandboxed_git(id, &args, None).await;
        if rebased.is_ok() {
            Box::pin(self.record_own(id, &now, &rewrote)).await;
            tracing::info!("job {id}: {base} moved, so its commits were rebased onto {now}");
            self.update(id, |job| {
                job.start_commit = Some(now.clone());
                job.changed(format!("Rebased onto {base} as it is now, by Oby"));
            });
            return Onto::Rebased(now);
        }
        let files = self.unmerged(id).await;
        let _ = self.sandboxed_git(id, &["rebase", "--abort"], None).await;
        tracing::info!("job {id}: {base} moved, and its commits don't rebase onto it");
        Onto::Conflict(files)
    }

    /// A rebase left in progress in the job's worktree (its `rebase-merge` or `rebase-apply`, or
    /// unmerged paths), aborted: the files it left in conflict, or none when there was none.
    async fn abort_half_rebase(&self, id: &str) -> Option<Vec<String>> {
        let (admin, _) = self.own_git_dirs(id).await.ok()?;
        let unmerged = self.unmerged(id).await;
        if !admin.join("rebase-merge").exists()
            && !admin.join("rebase-apply").exists()
            && unmerged.is_empty()
        {
            return None;
        }
        let _ = self.sandboxed_git(id, &["rebase", "--abort"], None).await;
        tracing::info!("job {id}: its agent left a rebase half done: aborted");
        Some(unmerged)
    }

    /// The files a rebase in the job's worktree left with conflicts, as git in its sandbox says.
    async fn unmerged(&self, id: &str) -> Vec<String> {
        self.sandboxed_git(id, &["diff", "--name-only", "--diff-filter=U"], None)
            .await
            .map(|out| {
                String::from_utf8_lossy(&out)
                    .lines()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// What the agent left uncommitted when its job ended without commits to publish (stopped, at
    /// a limit, failed, or done with none of its own): one [`WIP_MESSAGE`] commit of all of it, in
    /// the job's sandbox, so keep_commits keeps it on the job's branch (#606). It never counts as
    /// the job's: its next run takes it back out ([`Self::unkeep_wip`]).
    pub(super) async fn keep_wip(&self, id: &str) -> Result<(), String> {
        let changes = self
            .sandboxed_git(
                id,
                &["status", "--porcelain", "--untracked-files=all"],
                None,
            )
            .await?;
        if changes.is_empty() {
            return Ok(());
        }
        let head =
            Box::pin(self.sandboxed_git(id, &["rev-parse", "--verify", "HEAD"], None)).await?;
        self.sandboxed_git(id, &["add", "--all"], None).await?;
        let (name, email) = &self.identity();
        self.sandboxed_git(
            id,
            &[
                "-c",
                &format!("user.name={name}"),
                "-c",
                &format!("user.email={email}"),
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "--no-verify",
                "--message",
                WIP_MESSAGE,
            ],
            None,
        )
        .await?;
        // What its agent left: the job's own work, as its turns' commits are.
        Box::pin(self.record_own(id, String::from_utf8_lossy(&head).trim(), &[])).await;
        Ok(())
    }

    /// Whether the job's worktree holds changes nobody committed, as git sees them in its sandbox
    /// (obid's own `.obi/` folders aside): false when git can't say.
    pub(super) async fn uncommitted(&self, id: &str) -> bool {
        self.sandboxed_git(
            id,
            &["status", "--porcelain", "--untracked-files=all"],
            None,
        )
        .await
        .is_ok_and(|changes| !changes.is_empty())
    }

    /// Whether deleting the job `id` loses work its commit count doesn't show (TASKS-11): obid's
    /// own `wip:` commit at its worktree's HEAD, or changes nobody committed. True when git can't
    /// say within 15 seconds (it runs in the owner's Delete request); a worktree that's gone loses
    /// nothing.
    pub async fn unsaved_work(&self, id: &str) -> bool {
        if !self.worktree_path(id).join(".git").exists() {
            return false;
        }
        let limit = Duration::from_secs(15);
        // Any change at all is enough: one byte of it, never an untracked folder listed whole.
        let status = self
            .sandboxed_git_within(
                id,
                &["status", "--porcelain", "--untracked-files=normal"],
                None,
                limit,
                0,
            )
            .await;
        if status.is_err() {
            return true;
        }
        self.sandboxed_git_within(
            id,
            &["log", "-1", "--format=%B", "HEAD"],
            None,
            limit,
            WIP_MESSAGE.len() + 64,
        )
        .await
        .is_ok_and(|message| String::from_utf8_lossy(&message).trim() == WIP_MESSAGE)
    }
}
