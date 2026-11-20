//! Running the repository's check for a job (PUBLISHING-5 to 8) and, when it fails, on the base,
//! to tell a failure the change made from one the base has ([`BaseFailure`]). It never pushes.

use super::*;

impl Jobs {
    /// The lock of the clone job `id`'s checks are checked out from (`Jobs::preparing`): a check's
    /// checkout is added and removed under it, as a job's own worktree is (#2626).
    pub(super) fn checkout_lock(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.preparing(&self.get(id).map(|job| job.repo).unwrap_or_default())
    }

    /// Removes job `id`'s own check checkout and what its checks built there, once its worker
    /// ended: never another job's (cargo could run another checkout's binary).
    pub(super) async fn drop_checkout(&self, id: &str) {
        let at = self.check_home(id);
        let Some(job) = self.get(id).filter(|_| at.exists()) else {
            return;
        };
        let common = self.clone_for(&job).join(".git");
        let name = format!("check-{id}");
        let check = Spec {
            worktree: at.join(&name),
            git_admin: Some(common.join("worktrees").join(&name)),
            git_common: Some(common),
            ..Spec::default()
        };
        let lock = self.checkout_lock(id);
        if let Err(why) = crate::secretspec::remove_checkout(&check, &at, &lock).await {
            tracing::warn!("job {id}: {why}");
        }
    }

    /// Where the job's worktree is as its check starts (PUBLISHING-5): its HEAD (empty when git
    /// can't say), and whether anything isn't committed (untracked files too, ignored ones not).
    /// Dirty when git can't say. The check never writes there: it runs in a checkout of its own.
    pub(super) async fn before_check(&self, id: &str) -> CheckStart {
        let dirty = !self
            .sandboxed_git(
                id,
                &["status", "--porcelain", "--untracked-files=all"],
                None,
            )
            .await
            .is_ok_and(|changes| changes.is_empty());
        CheckStart {
            head: self.worktree_head(id).await,
            dirty,
        }
    }

    /// The commit a passing check passed on (PUBLISHING-5): the worktree's HEAD before it, when
    /// nothing was uncommitted then and HEAD was still there after it; else why it didn't check
    /// them.
    pub(super) async fn checked_commit(
        &self,
        id: &str,
        start: CheckStart,
    ) -> Result<String, String> {
        if start.dirty {
            return Err(CHECKED_UNCOMMITTED.into());
        }
        let after = self.worktree_head(id).await;
        if !git::is_sha(&start.head) || after != start.head {
            return Err(CHECK_MOVED_HEAD.into());
        }
        Ok(start.head)
    }

    /// What the check said when it stopped `job`, for the round the owner's words started: its
    /// last run, in the round before this one, failed. None otherwise, or with its log gone.
    pub(super) fn stopping_check(&self, job: &Job) -> Option<String> {
        let command = job.check.as_deref()?;
        let last = job.checks.last()?;
        let failed = last.steps.iter().any(|s| s.passed == Some(false));
        if !failed || last.round + 1 != job.current_round() {
            return None;
        }
        let output = std::fs::read_to_string(self.log_path(&job.id, "check.log")).ok()?;
        Some(check_failed(command, &output, &last.failing))
    }

    /// After `command` failed (`output`) in `round`: the next round's message, or how the job stops.
    /// A failure the agent says (`said`) the task asked for (a test meant to fail) ends the rounds:
    /// another round would undo what the owner asked. It isn't a pass, so nothing goes out: only
    /// checks that pass publish (AGENTS.md), whatever the agent says, and nothing publishes it by
    /// hand either: the owner's words decide. Past its last round, or with no number named on the
    /// same failure as the round before (RECOVERY-9), it stops at a limit.
    pub(super) fn after_failed_check(
        &self,
        id: &str,
        job: &Job,
        command: &str,
        output: &str,
        said: &str,
        round: u32,
    ) -> Result<String, End> {
        if said.trim_start().starts_with(FAILS_AS_ASKED) {
            return Err(End::needs_you(
                format!(
                    "`{command}` fails, as the agent says the task asked for, so nothing was pushed. The branch is kept: tell it what you want."
                ),
                None,
            ));
        }
        let checks = self.get(id).map(|j| j.checks).unwrap_or_default();
        // With no number named, the same failure as the round before's stops it (RECOVERY-9).
        if round >= job.max_rounds() || job.rounds.is_none() && crate::facts::repeats(&checks) {
            let limit = format!("`{command}` still fails after {round} round(s)");
            self.update(id, |job| job.limit = Some(limit.clone()));
            return Err(End::needs_you(
                format!("Stopped at a limit: {limit}. The branch is kept and nothing was pushed."),
                Some(crate::job::ONE_MORE_ROUND),
            ));
        }
        let quoted: String = job.task.lines().map(|l| format!("> {l}\n")).collect();
        Ok(format!(
            "{}\n\nThe task, as the owner wrote it:\n\n{quoted}\nIf this failure is what the task asks for (a test meant to fail, say), change nothing: start the first line of your summary with `{FAILS_AS_ASKED}` and say so. Otherwise fix what it shows, run it again in the foreground, commit, and finish with a short summary.",
            check_failed(
                command,
                output,
                checks.last().map_or(&[], |c| &c.failing[..])
            )
        ))
    }

    /// Runs the repository's check on commit `head` in the job sandbox (in a checkout of its own,
    /// [`Jobs::base_checkout`]), with no network but, for an owner's job run here, loopback, its
    /// limit counted from when it starts.
    pub(super) async fn checking_with_turn(
        &self,
        id: &str,
        command: &str,
        head: &str,
        spec: &Spec,
        signal: &watch::Receiver<Signal>,
    ) -> Result<CheckRun, String> {
        let sandbox = self
            .sandbox
            .as_ref()
            .map_err(|why| format!("obid can't run the check: {why}"))?;
        // Whether the job's agent had a network of its own: a boxed agent's way off a Mac is obid's
        // egress proxy (`confines_egress`, as each launch sets `proxied`); elsewhere, or in the
        // own lane, it isn't.
        let raw_network = spec.network && (!sandbox.confines_egress() || self.ran_unproxied(id));
        let spec = self.check_spec(id, spec);
        // Each step between its top-level `&&`s timed, its tests counted, live (#734).
        let round = self.get(id).map_or(1, |job| job.current_round());
        let marker = format!("obid-step-{}", random_check_id()?);
        // The secrets its repository declares on its base branch, for an owner's job only.
        // Boxed, as each of the secrets' steps below: they'd grow every job's future on the stack.
        let (mut declared, gate) = Box::pin(self.declared_secrets(id, &spec, raw_network)).await;
        // Every check runs in a checkout obid makes of the commit (the gate's, with values), apart
        // from everything the job's sandbox can write, with nothing ignored or untracked, and gone
        // after it (crate::secretspec); the base's own check script and `.gitleaks.toml` in it
        // (PUBLISHING-6). Without that, nothing counts as checked (PUBLISHING-8).
        // Its own checkout, kept across its checks with what they built, and gone with its worker
        // ([`Jobs::drop_checkout`]); with the owner's secrets, one for this check alone.
        let kept = gate.is_none();
        let at = match kept {
            true => self.check_home(id),
            false => self
                .config
                .home
                .join("checks")
                .join(format!("{id}-{}", random_check_id()?)),
        };
        let base_spec = spec.clone();
        let (spec, notes, scripts_from) =
            match Box::pin(self.checkout(id, command, head, &spec, gate, &at, signal)).await {
                Ok(check) => check,
                Err(why) => {
                    tracing::info!("job {id}: nothing counts as checked: {why}");
                    self.update(id, |job| job.checked = None);
                    return Ok(CheckRun {
                        passed: false,
                        output: why.clone(),
                        unchecked: Some(why),
                    });
                }
            };
        let watch = Arc::new(Mutex::new(
            crate::facts::CheckWatch::new(marker, crate::facts::steps(command))
                .redacting(declared.secrets()),
        ));
        let record = |ended| {
            let record = watch
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record(round, ended);
            // Sent only when something changed: a running step's time isn't in it.
            if ended.is_none()
                && self
                    .get(id)
                    .is_some_and(|job| job.checks.last() == Some(&record))
            {
                return;
            }
            self.update(id, |job| match job.checks.last_mut() {
                Some(last) if ended.is_some() || last.steps.iter().any(|s| s.running) => {
                    *last = record
                }
                _ => job.checks.push(record),
            });
        };
        record(None);
        let script = watch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .script();
        let argv: Vec<OsString> = vec!["/bin/sh".into(), "-c".into(), script.into()];
        let argv = sandbox.wrap(&spec, &argv);
        let mut extra = spec.git_env();
        extra.extend(process::show_rust(&spec.temp));
        // One check at a time: its build takes what the running agents leave it.
        extra.extend(self.build_env(id, true));
        let mut env = process::sandbox_env(&process::tools_path(), &spec.temp, &extra);
        declared.apply(&mut env);
        let secrets = declared.secrets();
        let group = self.recording(id, &spec);
        let running = run_check(
            &argv,
            &env,
            &spec.worktree,
            CHECK_QUIET,
            unjudged::limit(),
            signal.clone(),
            Some(watch.clone()),
            Some(&secrets),
            Some(&group),
        );
        tokio::pin!(running);
        let mut tick = tokio::time::interval(CHECK_TICK);
        tick.tick().await;
        let run = loop {
            tokio::select! {
                run = &mut running => break run,
                _ = tick.tick() => record(None),
            }
        };
        // With the owner's secrets, what it wrote goes with its own checkout, which no job's
        // sandbox ever reads.
        let lock = self.checkout_lock(id);
        if !kept
            && let Err(why) = Box::pin(crate::secretspec::remove_checkout(&spec, &at, &lock)).await
        {
            tracing::warn!("job {id}: {why}");
        }
        record(Some(run.as_ref().is_ok_and(|run| run.passed)));
        let mut run = run.map_err(|e| format!("obid couldn't run the check: {e}"))?;
        // A check the machine failed, not the code, judged nothing: no fix round for it, the
        // owner's Publish by hand (PUBLISHING-17). Read from the check's own output, before obid's
        // notes join it, and never when its runner named a test that failed.
        let tests_failed = self
            .get(id)
            .is_some_and(|job| job.checks.last().is_some_and(|c| !c.failing.is_empty()));
        // Nor one whose line says it couldn't judge it, `couldn't judge: <why>` (PUBLISHING-22).
        if !run.passed && run.unchecked.is_none() && !tests_failed {
            run.unchecked = unjudged::why(command, &run.output);
        }
        // A first failure the base already has isn't the change's: no fix round for it, the
        // owner's Publish by hand (PUBLISHING-18). Still holding the check's turn.
        if !run.passed
            && run.unchecked.is_none()
            && round == 1
            && self.base_check
            // With the owner's secrets the job's check ran with values the base run wouldn't get.
            && kept
            && let Some(why) = Box::pin(self.fails_on_base(
                id,
                command,
                head,
                &scripts_from,
                &base_spec,
                signal,
                &run.output,
            ))
            .await
        {
            run.unchecked = Some(why);
        }
        // What a `mac` workflow's merge counts (crate::merge): a pass, on the clean head it ran.
        let checked = match run.passed {
            true => self.checked(id, command).await,
            false => None,
        };
        self.update(id, |job| {
            job.checked = checked;
            if let Some(last) = job.checks.last_mut().filter(|_| !run.passed) {
                last.signature = Some(crate::facts::signature(last, &run.output));
            }
        });
        run.output = declared.noted(&format!("{}\n{}", notes.join("\n"), run.output));
        let _ = std::fs::write(self.log_path(id, "check.log"), &run.output);
        Ok(run)
    }

    /// Why job `id`'s failed first check of `command` on `head` (its `output`, its scripts as the
    /// base had them at `scripts_from`) isn't the change's doing (PUBLISHING-18): the same command,
    /// run once on the commit its work started from, failed the same way: the same signature, or
    /// the same step with every test it named, when those names are its whole failure. Only for a
    /// job started at its base's tip (no pull request, first job's branch or review), not someone
    /// else's code (its run would feed every job's [`Jobs::base_runs`]), whose task names none of
    /// its failing tests, never once it stopped. Each run is kept; one that judged nothing isn't.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn fails_on_base(
        &self,
        id: &str,
        command: &str,
        head: &str,
        scripts_from: &str,
        spec: &Spec,
        signal: &watch::Receiver<Signal>,
        output: &str,
    ) -> Option<String> {
        if *signal.borrow() != Signal::Run {
            return None;
        }
        let job = self.get(id).filter(|job| {
            job.pr.is_none()
                && job.start_from.is_none()
                && job.review_of.is_none()
                && job.someone_elses().is_none()
        })?;
        let start = job.start_commit.clone().filter(|c| git::is_sha(c))?;
        let mine = BaseFailure::of(job.checks.last()?, output);
        if crate::facts::names_any(&job.task, &mine.failing) {
            return None;
        }
        let paths = self.changed_files(&job, head).await.unwrap_or_default();
        let paths = paths.join("\n");
        let key = |from: &str| format!("{}\0{start}\0{from}\0{command}\0{paths}", job.repo);
        let runs = || {
            self.base_runs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        let kept = runs().get(&key(scripts_from)).cloned();
        let base = match kept {
            Some(base) => base,
            None => {
                // Nothing changed: its own run is the base's.
                let (base, from) = match start == head {
                    true => (Some(mine.clone()), scripts_from.to_owned()),
                    false => {
                        Box::pin(self.run_on_base(id, command, &start, &paths, spec, signal))
                            .await?
                    }
                };
                runs().insert(key(&from), base.clone());
                // Its scripts as the base had them later than the job's: no like for like.
                if from != scripts_from {
                    return None;
                }
                base
            }
        }?;
        if !base.same_as(&mine) {
            return None;
        }
        let what = match mine.failing.as_slice() {
            [] => "the same way".to_owned(),
            tests => format!("on {}", tests.join(", ")),
        };
        Some(unjudged::by_hand(format!(
            "`{command}` already fails on the base {start:.7} as it fails on this change's {head:.7}, {what}"
        )))
    }

    /// Job `id`'s check `command` run on `start` (PUBLISHING-18) with no declared secrets, told the
    /// job's changed `paths` (`OBY_CHECK_PATHS`, for a check that runs only what changed), under
    /// `home/checks`, removed after: how it failed (`None`: passed), with its scripts' base commit;
    /// `None` when it judged nothing (it couldn't run, ran past its limit, or the job stopped).
    async fn run_on_base(
        &self,
        id: &str,
        command: &str,
        start: &str,
        paths: &str,
        spec: &Spec,
        signal: &watch::Receiver<Signal>,
    ) -> Option<(Option<BaseFailure>, String)> {
        let sandbox = self.sandbox.as_ref().ok()?;
        let at = self
            .config
            .home
            .join("checks")
            .join(format!("{id}-base-{}", random_check_id().ok()?));
        let marker = format!("obid-step-{}", random_check_id().ok()?);
        let (check, _, from) =
            match Box::pin(self.checkout(id, command, start, spec, None, &at, signal)).await {
                Ok(check) => check,
                Err(why) => {
                    tracing::info!("job {id}: no check on the base {start}: {why}");
                    return None;
                }
            };
        tracing::info!("job {id}: running its check on the base {start}");
        // A watch of its own, never the job's: it reads the base's steps and failing tests.
        let watch = Arc::new(Mutex::new(crate::facts::CheckWatch::new(
            marker,
            crate::facts::steps(command),
        )));
        let script = watch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .script();
        let argv = sandbox.wrap(&check, &["/bin/sh".into(), "-c".into(), script.into()]);
        let mut extra = check.git_env();
        extra.extend(process::show_rust(&check.temp));
        extra.extend(self.build_env(id, true));
        extra.push(("OBY_CHECK_PATHS", paths.into()));
        let env = process::sandbox_env(&process::tools_path(), &check.temp, &extra);
        let group = self.recording(id, &check);
        let run = run_check(
            &argv,
            &env,
            &check.worktree,
            CHECK_QUIET,
            CHECK_LIMIT,
            signal.clone(),
            Some(watch.clone()),
            None,
            Some(&group),
        )
        .await;
        let lock = self.checkout_lock(id);
        if let Err(why) = Box::pin(crate::secretspec::remove_checkout(&check, &at, &lock)).await {
            tracing::warn!("job {id}: {why}");
        }
        let run = run
            .ok()
            .filter(|run| run.unchecked.is_none() && *signal.borrow() == Signal::Run)?;
        if run.passed {
            return Some((None, from));
        }
        let record = watch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .record(1, Some(false));
        Some((Some(BaseFailure::of(&record, &run.output)), from))
    }

    /// The checkout job `id`'s check of `command` runs in, at `at` (PUBLISHING-6): of `head`, the
    /// commit its worktree was at when the check started, with the base's own copy of the check's
    /// scripts ([`crate::merge::check_scripts`], each naming a file at the commit or on the base)
    /// and of `.gitleaks.toml` (none when the base has none), frozen there with the folders they're
    /// in. The base is the job's when it's the repository's default branch or the one its
    /// remembered workflow merges into, else the default branch: never a branch obid pushed
    /// (`oby/…`) or any other, which a task could have written. As obid's clone has it when the
    /// check starts. The secrets' `gate`, when it passed, passed `head`. With what obid notes atop
    /// the check's output; else why nothing counts as checked (PUBLISHING-8), nothing left at `at`.
    pub(super) async fn base_checkout(
        &self,
        id: &str,
        command: &str,
        head: &str,
        spec: &Spec,
        gate: Option<(String, String)>,
        at: &Path,
    ) -> Result<(Spec, Vec<String>, String), String> {
        let job = self.get(id).ok_or("the job is gone")?;
        if !git::is_sha(head) {
            return Err(unchecked(
                "obid couldn't read the commit its worktree is at",
            ));
        }
        if gate.as_ref().is_some_and(|(_, gated)| gated != head) {
            return Err(CHECK_MOVED_HEAD.into());
        }
        let repo = self
            .repo(&job.repo)
            .ok_or_else(|| unchecked("obid doesn't know its repository's default branch"))?;
        let merges_into = crate::memory::workflow(&self.config.home, &job.repo)
            .and_then(|m| m.workflow)
            .map(|w| w.base);
        // A job on another base (stacked on `oby/a`) gets where its work merges into.
        let base = job
            .base
            .clone()
            .filter(|b| *b == repo.default_branch || Some(b) == merges_into.as_ref())
            .or(merges_into.filter(|b| !b.starts_with("oby/")))
            .unwrap_or(repo.default_branch.clone());
        if !git::is_branch_name(&base) || base.starts_with("oby/") {
            return Err(unchecked(format!(
                "{base} isn't a branch obid takes a check from"
            )));
        }
        let clone = self.clone_for(&job);
        // The job's own base was fetched as the check started (Jobs::onto_base); another isn't.
        if job.base.as_ref().is_some_and(|b| *b != base) {
            let tracking = format!("refs/remotes/origin/{base}");
            let preparing = self.preparing(&repo.repo);
            let _fetching = preparing.lock().await;
            if let Err(e) = git::fetch(
                &clone,
                &repo.remote,
                &[format!("+refs/heads/{base}:{tracking}")],
            )
            .await
            {
                return Err(unchecked(format!("obid couldn't fetch {base}: {e}")));
            }
            Box::pin(self.taint_unless_ours(
                &clone,
                &repo,
                &tracking,
                Some(&base),
                &format!("the branch {base}, fetched for a job's check"),
            ))
            .await;
        }
        let base_commit = git::git(
            &clone,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{base}^{{commit}}"),
            ],
        )
        .await
        .map_err(|_| unchecked(format!("obid's clone doesn't have {base}")))?;
        let lock = self.checkout_lock(id);
        let check = crate::secretspec::fresh_checkout(
            spec,
            &base_commit,
            head,
            at,
            &repo.check_keep,
            &lock,
        )
        .await
        .map_err(unchecked);
        let mut check = match check {
            Ok(check) => check,
            Err(why) => {
                let _ = std::fs::remove_dir_all(at);
                return Err(why);
            }
        };
        // Its home is the job's, as before, with the caches its agent filled (cargo's registry,
        // npm's): the check has no network to fetch them. One with the owner's secrets gets a
        // home of its own, which no job's sandbox ever wrote.
        // ponytail: a cache the agent wrote can still steer the check (a `~/.cargo/config.toml`);
        // a home rebuilt from the base's lockfiles when that matters.
        if gate.is_none() {
            check.temp = spec.temp.clone();
        }
        let scripts: Vec<String> = crate::merge::check_scripts(command).into_iter().collect();
        let at_commit: Vec<bool> = scripts
            .iter()
            .map(|p| check.worktree.join(p).symlink_metadata().is_ok())
            .collect();
        let mut paths = scripts.clone();
        paths.push(".gitleaks.toml".into());
        let made =
            crate::secretspec::from_base(&clone, &base_commit, &check.worktree, &paths).await;
        let missing = match &made {
            Ok(had) => scripts
                .iter()
                .zip(at_commit)
                .find(|(path, at_commit)| *at_commit && !had.contains(path))
                .map(|(path, _)| {
                    unchecked(format!(
                        "its check runs {path} as {base} has it, and {base} has no {path}"
                    ))
                }),
            Err(why) => Some(unchecked(why.clone())),
        };
        if let Some(why) = missing {
            if let Err(e) = crate::secretspec::remove_checkout(&check, at, &lock).await {
                tracing::warn!("job {id}: {e}");
            }
            return Err(why);
        }
        let had = made.unwrap_or_default();
        // What the check runs as its base has it can't be rewritten while it runs (a shell reads
        // its script as it goes), nor moved out from under that with a folder it's in.
        for path in &had {
            check.frozen.push(check.worktree.join(path));
            let mut folder = std::path::PathBuf::new();
            for part in path
                .split('/')
                .collect::<Vec<_>>()
                .split_last()
                .map_or(&[][..], |(_, f)| f)
            {
                folder.push(part);
                let fixed = check.worktree.join(&folder);
                if !check.fixed.contains(&fixed) {
                    check.fixed.push(fixed);
                }
            }
        }
        let ran: Vec<&String> = scripts.iter().filter(|p| had.contains(p)).collect();
        let note = match ran.as_slice() {
            [] => format!(
                "obid: no script found in this check, so the commit's own files run (`.gitleaks.toml` is {base}'s)."
            ),
            ran => format!(
                "obid: this check runs {} as {base} has it, on the commit alone.",
                ran.iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        Ok((check, vec![note], base_commit))
    }
}

/// What the agent hears of a check that failed: the command, the tests that failed (the end alone
/// can be all one stack, with the failures above it gone: #1790) and the end of its output, the
/// secrets obid knows taken out, in a fence of its own so nothing the output says can close it and
/// pose as the owner's words. A vault that can't be read sends none of it.
pub(super) fn check_failed(
    command: &str,
    output: &str,
    failing: &[crate::facts::Failing],
) -> String {
    let named: String = failing
        .iter()
        .map(|f| format!("Failed: {}\n{}\n", f.test, f.message))
        .collect();
    // Names and messages come from what the check printed: bounded as the rest is.
    let named: String = named.chars().take(4_000).collect();
    let said = format!("{named}{}", tail(output, 8_000));
    let output = match crate::vault::for_agent(&said) {
        Ok(text) => crate::secrets::known().redact(&text),
        Err(why) => format!("(obid left it out: {why})"),
    };
    let nonce = crate::brain::briefing::nonce();
    format!(
        "`{command}` failed after your last round. Its output, from the end:\n\n{}\n{}",
        crate::brain::briefing::fence_rule(&nonce, "the check's output"),
        crate::brain::briefing::fence(&nonce, &output)
    )
}

/// The worktree as a check starts ([`Jobs::before_check`]).
pub(super) struct CheckStart {
    pub(super) head: String,
    dirty: bool,
}

/// What a failed check run failed on, to tell whether a base fails as a job does (PUBLISHING-18).
#[derive(Debug, Clone)]
pub(super) struct BaseFailure {
    signature: String,
    /// The step that failed.
    step: String,
    /// Its failing tests' names.
    failing: Vec<String>,
    /// Whether those names are the whole of its failure ([`crate::facts::named`]).
    named: bool,
}

impl BaseFailure {
    fn of(record: &crate::facts::CheckRecord, output: &str) -> BaseFailure {
        let failed = record.steps.iter().find(|s| s.passed == Some(false));
        BaseFailure {
            signature: crate::facts::signature(record, output),
            step: failed.map(|s| s.command.clone()).unwrap_or_default(),
            named: crate::facts::named(record, output),
            failing: record.failing.iter().map(|f| f.test.clone()).collect(),
        }
    }

    /// Whether the base (this) fails as the job's run `job` does.
    fn same_as(&self, job: &BaseFailure) -> bool {
        self.signature == job.signature
            || (job.named
                && self.step == job.step
                && self.failing.len() < crate::facts::FAILING_MAX
                && job.failing.iter().all(|t| self.failing.contains(t)))
    }
}

/// How a run of the check went: by default, not at all (the job was stopped).
#[derive(Debug, Clone, Default)]
pub struct CheckRun {
    pub passed: bool,
    /// Its output, stdout and stderr together, from the end.
    pub output: String,
    /// Why it didn't run, so nothing counts as checked, though the job ends done and its owner
    /// may publish it by hand (PUBLISHING-8).
    pub unchecked: Option<String>,
}

/// Runs a check's command line: its output together, ended with everything it started when it
/// does nothing for `quiet`, runs `limit` or the job stops (PUBLISHING-29).
///
/// `secrets` (the values its repository declares, crate::secretspec) are taken out of its output
/// before it is cut to size or kept. When only its end was kept, its first line, which a value or
/// a word may have been cut in, goes too (RECOVERY-9).
#[allow(clippy::too_many_arguments)]
pub async fn run_check(
    argv: &[OsString],
    env: &[(OsString, OsString)],
    cwd: &Path,
    quiet: Duration,
    limit: Duration,
    mut signal: watch::Receiver<Signal>,
    watch: Option<Arc<Mutex<crate::facts::CheckWatch>>>,
    secrets: Option<&crate::secrets::Secrets>,
    group: Option<&crate::groups::Recording>,
) -> std::io::Result<CheckRun> {
    let (reader, writer) = std::io::pipe()?;
    clear_recorded_exit(env);
    let mut command = crate::process::command(argv, env, cwd)?;
    command
        .stdin(std::process::Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer);
    if let Some(group) = group {
        crate::groups::take(group)
            .await
            .map_err(std::io::Error::other)?;
    }
    // No process group set here: the command line is the job sandbox's (`Sandbox::wrap`), whose
    // first program starts a session of its own, and so a process group whose id is the child's.
    let mut child = command.spawn()?;
    // The writing ends live in the child now: the reader ends when it (and what it started) does.
    drop(command);
    let pgid = child.id().unwrap_or(0);
    let _metered = crate::meter::check_root(pgid, cwd);
    let _recorded = match group {
        Some(group) => Some(crate::groups::record(group, pgid).await),
        None => None,
    };
    // Dropped part way (its job's task panicked, #914), what it started ends too, not only the
    // first process `kill_on_drop` ends.
    struct Group(u32);
    impl Drop for Group {
        fn drop(&mut self) {
            process::signal_group(self.0, nix::sys::signal::Signal::SIGKILL);
        }
    }
    let _group = Group(pgid);
    let fed = watch.clone();
    let heard = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hearing = heard.clone();
    let reading = tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let feed = |bytes: &[u8]| {
            if let Some(watch) = &fed {
                watch
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .feed(bytes);
            }
        };
        let mut kept: Vec<u8> = Vec::new();
        let mut cut = false;
        let mut buffer = [0u8; 8192];
        loop {
            match std::io::Read::read(&mut reader, &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    hearing.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                    feed(&buffer[..n]);
                    kept.extend_from_slice(&buffer[..n]);
                    if kept.len() > 2 * CHECK_OUTPUT_MAX {
                        kept.drain(..kept.len() - CHECK_OUTPUT_MAX);
                        cut = true;
                    }
                }
            }
        }
        // The last line, when it has no newline of its own.
        feed(b"\n");
        (kept, cut)
    });
    // What its output ends with and why it judged nothing, when obid stopped it.
    let mut stopped = None;
    let status = tokio::select! {
        status = child.wait() => status.ok(),
        idle = tokio::time::timeout(limit, unjudged::idle(pgid, &heard, quiet)) => {
            stopped = Some(idle.unwrap_or_else(|_| unjudged::too_long(limit)));
            process::end_group(pgid, process::STOPPING).await;
            child.wait().await.ok()
        }
        _ = async { while signal.changed().await.is_ok() && *signal.borrow() == Signal::Run {} } => {
            process::end_group(pgid, process::STOPPING).await;
            child.wait().await.ok()
        }
    };
    // Whatever it left running would hold the output open.
    process::signal_group(pgid, nix::sys::signal::Signal::SIGKILL);
    let (mut kept, cut) = tokio::time::timeout(Duration::from_secs(5), reading)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    if cut {
        // With no newline before its end, all of it stays, unless a declared secret may be in it.
        let declared = secrets.is_some_and(|secrets| !secrets.is_empty());
        let none = if declared { kept.len() } else { 0 };
        let body = kept.strip_suffix(b"\n").unwrap_or(&kept);
        let start = body.iter().position(|&b| b == b'\n');
        kept.drain(..start.map_or(none, |at| at + 1));
    }
    if let Some(secrets) = secrets.filter(|secrets| !secrets.is_empty()) {
        kept = secrets.redact(&String::from_utf8_lossy(&kept)).into_bytes();
    }
    let mut output = String::from_utf8_lossy(&kept).into_owned();
    if let Some(watch) = &watch {
        let watch = watch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        output = output
            .split_inclusive('\n')
            .filter(|line| !watch.is_marker(line))
            .collect();
    }
    if output.len() > CHECK_OUTPUT_MAX {
        let end = tail(&output, CHECK_OUTPUT_MAX);
        let start = end.strip_suffix('\n').unwrap_or(end).find('\n');
        output = end[start.map_or(0, |at| at + 1)..].to_owned();
    }
    if let Some((said, _)) = &stopped {
        if !output.ends_with('\n') && !output.is_empty() {
            output.push('\n');
        }
        output.push_str(said);
    }
    let passed = stopped.is_none() && status.is_some_and(|ended| sandbox_succeeded(&ended, env));
    // A run obid stopped judged nothing: no fix round for it, the owner's Publish by hand
    // (PUBLISHING-16). A stop never sets `stopped`.
    let unchecked = stopped.map(|(_, why)| unchecked(why));
    Ok(CheckRun {
        passed,
        output,
        unchecked,
    })
}

pub(super) fn random_check_id() -> Result<String, String> {
    let mut bytes = [0u8; 4];
    getrandom::fill(&mut bytes).map_err(|e| format!("the system's random source failed: {e}"))?;
    Ok(hex::encode(bytes))
}
