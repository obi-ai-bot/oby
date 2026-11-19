//! The owner's own job: its branch, its rounds of the agent and the check, its commits counted,
//! and the end its stages lead to (docs/ARCHITECTURE.md, "State ownership"). It publishes only
//! through jobs/publishing.rs.

use super::*;

impl Jobs {
    /// The owner's own work: a branch, rounds of the agent and the check, its commits counted.
    pub(super) async fn run_owner_job(
        self: &Arc<Self>,
        id: &str,
        signal: &watch::Receiver<Signal>,
    ) -> Result<(), End> {
        let mut job = self.get(id).ok_or("the job is gone")?;
        let mut ours = job.current_round(); // the round it last launched, all it ends (TASKS-22)
        if let Err(why) = &self.sandbox {
            return Err(End::Failed(format!(
                "obid can't make the job sandbox on this machine, so it ran nothing: {why}"
            )));
        }
        let repo = self
            .repo(&job.repo)
            .ok_or_else(|| format!("{} isn't a repository obid knows.", job.repo))?
            .clone();
        let base = job
            .base
            .clone()
            .unwrap_or_else(|| repo.default_branch.clone());
        let worktree = self.worktree_path(id);
        let stages = Stages::new(self.log_path(id, "stages.log"));
        stages.begin("prepare");

        // Only a job that could get the owner's secrets asks GitHub (crate::secretspec).
        // A job on an issue asks once its issue is quoted, below.
        if !worktree.join(".git").exists()
            && job.not_owners_words().is_none()
            && job.issue.is_none()
            && job.pr.is_some()
        {
            Box::pin(self.authors_as_it_starts(&job, None, "as it started")).await;
        }
        let preparing = self.preparing(&repo.repo);
        let mut prepared = preparing.lock().await;
        if !worktree.join(".git").exists() {
            // A clone someone else's commits came into is made again before a new job uses it;
            // Oby's review only reads it, and finds the commit it reads below either way.
            if job.clone_root.is_none()
                && !job.apart
                && job.obi.is_none()
                && job.review_of.is_none()
            {
                Box::pin(self.remake_marked_clone(&job, &repo)).await;
            }
            // That may have moved its task's clone aside, as it stood: a follow-up that starts from
            // its task's branch carries on there, with that branch and its commits, as one started
            // after the move does (PUBLISHING-15).
            if job.clone_root.is_none()
                && job.start_from.is_some()
                && let Some(root) = moved_clone_of(&job, |of| self.get(of))
            {
                self.update(id, |job| job.clone_root = Some(root.clone()));
                job.clone_root = Some(root);
            }
            let mut refspecs = vec![format!("+refs/heads/{base}:refs/remotes/origin/{base}")];
            if let Some(pr) = job.pr {
                refspecs.push(format!("+refs/pull/{pr}/head:refs/remotes/origin/pr/{pr}"));
            }
            let ready = self.ready_clone(id, signal, &job, &repo, &refspecs, &preparing, prepared);
            let Some((clone, held)) = Box::pin(ready).await? else {
                return Ok(());
            };
            prepared = held;
            // Someone else's code, or a reach past the registries (a follow-up of either), marks
            // the owner's clone for every check from now on (crate::secretspec).
            if !job.apart
                && job.obi.is_none()
                && let Some(why) = job.someone_elses().or(job.reached.clone())
            {
                git::taint(&clone, &format!("job {id}: {why}")).await;
            }
            Box::pin(self.taint_unless_ours(
                &clone,
                &repo,
                &format!("refs/remotes/origin/{base}"),
                Some(&base),
                &format!("the branch {base}, fetched as a job's base"),
            ))
            .await;
            if let Some(pr) = job.pr {
                let theirs = Box::pin(self.taint_unless_ours(
                    &clone,
                    &repo,
                    &format!("refs/remotes/origin/pr/{pr}"),
                    None,
                    &format!("pull request #{pr}'s head"),
                ))
                .await;
                // The owner's review of commits obid didn't record reads someone else's code: it
                // asks for its reads and stays in Oby's box (MERGING-36).
                if theirs && job.read_only() && job.review_of.is_none() {
                    let why = format!("pull request #{pr}'s head has commits obid didn't record");
                    self.mark(id, why, false);
                    job = self.get(id).ok_or("the job is gone")?;
                }
            }
            // Oby's review of published work reads the very commit obid pushed, whatever its branch
            // has since.
            let pushed = job
                .start_commit
                .as_ref()
                .filter(|_| job.review_of.is_some());
            // Its clone may be one made again since the author pushed (a marked clone moves
            // aside, #1662): the commit comes from origin as obid pushed it, held to the same
            // rule as any fetch into the owner's clone.
            if let Some(sha) = pushed
                && sha.len() == 40
                && sha.bytes().all(|b| b.is_ascii_hexdigit())
                && git::git(&clone, &["cat-file", "-e", &format!("{sha}^{{commit}}")])
                    .await
                    .is_err()
            {
                git::fetch(&clone, &repo.remote, std::slice::from_ref(sha))
                    .await
                    .map_err(|e| format!("Couldn't fetch {sha} from {}: {e}", repo.repo))?;
                tracing::info!("job {id}: fetched {sha}, which its clone didn't have");
                Box::pin(self.taint_unless_ours(
                    &clone,
                    &repo,
                    sha,
                    None,
                    &format!("the commit {sha} Oby's review reads"),
                ))
                .await;
            }
            let start = match (pushed, &job.start_from, job.pr) {
                (Some(sha), _, _) => sha.clone(),
                // The owner's review reads its pull request's head as just fetched (MERGING-37).
                (None, _, Some(pr)) if job.read_only() => format!("refs/remotes/origin/pr/{pr}"),
                (None, Some(branch), _) => format!("refs/heads/{branch}"),
                (None, None, Some(pr)) => format!("refs/remotes/origin/pr/{pr}"),
                (None, None, None) => format!("refs/remotes/origin/{base}"),
            };
            let start_commit = git::git(
                &clone,
                &["rev-parse", "--verify", &format!("{start}^{{commit}}")],
            )
            .await
            .map_err(|e| format!("Couldn't find {start} in {}: {e}", repo.repo))?;
            std::fs::create_dir_all(worktree.parent().expect("worktrees have a folder"))
                .map_err(|e| format!("obid can't make its worktrees folder: {e}"))?;
            let exists = git::git(
                &clone,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{}", job.branch),
                ],
            )
            .await
            .is_ok();
            if !exists {
                git::git(
                    &clone,
                    &["branch", "--no-track", "--", &job.branch, &start_commit],
                )
                .await
                .map_err(|e| format!("obid couldn't make the job's branch: {e}"))?;
                // A branch the owner named that no remote has is this job's own, as an `oby/` one
                // is (PUBLISHING-13); one that's there, or a remote that didn't say, stays theirs.
                if !job.branch.starts_with("oby/")
                    && let Ok(target) = self.publish_target(&job)
                {
                    let taken = crate::publish::branch_taken(&job.branch, &target).await;
                    let _ = std::fs::remove_dir_all(&target.scratch);
                    if crate::publish::free_to_make(&taken) {
                        self.update(id, |job| job.made_branch = true);
                    }
                }
            }
            // Detached at the branch: the sandbox never writes the clone's refs, so obid moves the
            // branch to the job's commits itself (git::keep_commits). A review of a pull request at
            // the head it reads, never a branch of that name the clone kept from before (MERGING-37).
            let branch_ref = match job.pr {
                Some(_) if job.read_only() => start_commit.clone(),
                _ => format!("refs/heads/{}", job.branch),
            };
            let mut args: Vec<&std::ffi::OsStr> =
                git::LFS_POINTERS.iter().map(|a| a.as_ref()).collect();
            args.extend::<[&std::ffi::OsStr; 6]>([
                "worktree".as_ref(),
                "add".as_ref(),
                "--quiet".as_ref(),
                "--detach".as_ref(),
                worktree.as_os_str(),
                branch_ref.as_ref(),
            ]);
            git::run(&clone, &args, &[])
                .await
                .map_err(|e| format!("obid couldn't make the job's worktree: {e}"))?;
            let mut missing: Vec<String> = Vec::new();
            if git::uses_lfs(&clone, &start_commit).await {
                missing.push("its Git LFS files are pointers".into());
            }
            // Filled by obid, never by the agent (SETUP-1).
            if git::has_submodules(&clone, &start_commit).await {
                self.update(id, |job| {
                    job.step = format!("Getting {}'s submodules", job.repo_name())
                });
                let (remote, file) = (&repo.remote, self.config.allow_file_remotes);
                let who = format!("job {id}");
                let fill = git::fill_submodules(&clone, &worktree, remote, file, &who, signal);
                missing.extend(Box::pin(fill).await);
            }
            let worktree_text = worktree.to_string_lossy().into_owned();
            self.update(id, |job| {
                job.made_worktree(worktree_text);
                job.start_commit.get_or_insert(start_commit);
                job.base = Some(base.clone());
                if !missing.is_empty() {
                    job.step = format!(
                        "Getting {} ready: {}, so what needs them won't work",
                        job.repo_name(),
                        missing.join(" and ")
                    );
                }
            });
        }
        drop(prepared);
        // A Stop while its submodules filled.
        if *signal.borrow() != Signal::Run {
            return Ok(());
        }
        let job = self.get(id).ok_or("the job is gone")?;
        let mut start_commit = job.start_commit.clone().unwrap_or_default();
        // Where its branch was when this run started: a job whose own publish failed is tried
        // again by itself only when this run made new commits (runner-api.md, "Job").
        let clone = self.clone_for(&job);
        self.offer_check(&job, &clone, &start_commit).await;
        let before_head = git::git(
            &clone,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{}", job.branch),
            ],
        )
        .await
        .ok();
        if let Some(paths) = &job.attachments {
            let (copied, gone) = files_at_work::copy_inputs(&self.config.home, &worktree, paths)?;
            self.update(id, |job| {
                job.attachments = (!copied.is_empty()).then_some(copied);
                job.attachments_gone = (!gone.is_empty()).then_some(gone);
            });
        }
        // The issue it works from, as GitHub has it now, read with obid's own gh (the owner's
        // login), quoted as data in `.obi/inputs/`, on every run: its agent has no GitHub login to
        // read it itself (ISSUETASK-1). Who wrote it is read after the quote, so a word anyone
        // else added by then marks the job (ISSUETASK-4). One that can't be read fails a job the
        // owner asked for by the issue alone; any other works from its task, and says so
        // (ISSUETASK-2).
        if let Some(issue) = job.issue {
            let inputs = crate::obi_dir::folder(&worktree, "inputs")
                .map_err(|e| format!("obid can't make .obi/inputs: {e}"))?;
            let file = crate::github_work::quoted_name(&job.repo, issue);
            match crate::github_work::quote("assigned", &job.repo, issue).await {
                Ok((quoted, read)) => {
                    let text = crate::secrets::known().redact(&crate::vault::for_agent(&quoted)?);
                    crate::obi_dir::write_new(&inputs, &file, text.as_bytes()).map_err(|e| {
                        format!("obid couldn't quote issue #{issue} for the agent: {e}")
                    })?;
                    self.update(id, |job| job.issue_unread = None);
                    if job.foreign.is_none() || job.foreign_by_api {
                        Box::pin(self.authors_as_it_starts(&job, Some(&read), "as it started"))
                            .await;
                    }
                }
                Err(why) => {
                    let _ = std::fs::remove_file(inputs.join(&file));
                    let unread = format!(
                        "obid couldn't read issue #{issue} of {} on GitHub ({why})",
                        job.repo
                    );
                    if only_the_issue(&job.request) {
                        return Err(format!(
                            "{unread}, so its agent never saw it and nothing was started. Sign gh in to an account that can read it, then ask again."
                        )
                        .into());
                    }
                    self.update(id, |job| job.issue_unread = Some(unread));
                    // Its agent never saw the issue, but who wrote it is read as it starts all
                    // the same: a word of someone else's there still gives its check nothing.
                    if job.foreign.is_none() || job.foreign_by_api {
                        Box::pin(self.authors_as_it_starts(&job, None, "as it started")).await;
                    }
                }
            }
        }
        // Oby's review of published work reads what its author was asked and the repository's
        // rules as the base has them, quoted (crate::review): never the rules the change itself
        // writes, which its author could steer the review with. A Claude Code job gets the rules
        // too (#612): under `--setting-sources ""`, which keeps the repository's settings out,
        // Claude Code loads no CLAUDE.md (measured with 2.1.302), nor under an own job's `user`
        // (LANES-5). Codex and Cursor read the worktree's own rules files themselves.
        let reviews = job.review_of.is_some();
        let mut rules_quoted = false;
        // The change Oby's own review judges (PERMISSIONS-14), frozen in its worktree.
        let mut own_diff: Option<Vec<u8>> = None;
        if reviews || job.agent == Tool::Claude.id() {
            let mut files = Vec::new();
            for name in ["AGENTS.md", "CLAUDE.md"] {
                let blob = format!("refs/remotes/origin/{base}:{name}");
                files.push((
                    name,
                    git::git(&clone, &["cat-file", "blob", &blob]).await.ok(),
                ));
            }
            rules_quoted = !reviews && files.iter().any(|(_, text)| text.is_some());
            let mut quoted = Vec::new();
            if reviews {
                // The issue it worked from (MERGING-18). Whoever wrote on it by now counts as they
                // would for the author: a stranger's comment the reviewer reads means two reviews
                // (MERGING-14), and the reviewer is told, below. The file is always obid's: one a
                // commit brought never stands as the quote.
                let author = job.review_of.as_deref().and_then(|of| self.get(of));
                let issue_text = match author.as_ref().and_then(|a| Some((a, a.issue?))) {
                    None => "This work isn't from an issue.".to_owned(),
                    Some((author, issue)) => {
                        match crate::github_work::quote("assigned", &job.repo, issue).await {
                            Ok((text, read)) => {
                                // MERGING-33a: a stranger's word ends an API mark's exception.
                                if author.foreign.is_none() || author.foreign_by_api {
                                    Box::pin(self.authors_as_it_starts(
                                        author,
                                        Some(&read),
                                        "as its review started",
                                    ))
                                    .await;
                                }
                                text
                            }
                            Err(why) => {
                                tracing::warn!(
                                    "job {id}: its review goes without issue #{issue}: {why}"
                                );
                                format!("obid couldn't read issue #{issue} on GitHub.")
                            }
                        }
                    }
                };
                quoted.push((review::ISSUE_FILE, issue_text));
                // Whether the author is untrusted, and why: never what it read (MERGING-13).
                let author = job.review_of.as_deref().and_then(|of| self.get(of));
                let untrusted = author.as_ref().and_then(Job::untrusted_why);
                // What the owner asked, and every word they sent it after (#1659, TASKCONV-3,
                // MERGING-33).
                let asked = match &author {
                    Some(author) => review::asked_of(author, |id| self.get(id)),
                    None => review::Asked {
                        task: job.task.clone(),
                        request: job.request.clone(),
                        ..review::Asked::default()
                    },
                };
                quoted.push((
                    review::ASKED_FILE,
                    review::asked(&asked, untrusted.as_deref())
                        + &review::reached_note(author.as_ref().and_then(|a| a.reached.as_deref())),
                ));
                // The security paths its head changes against its base, from the commit it reads
                // (MERGING-15): none read counts as touching one.
                let changed = match &job.start_commit {
                    Some(head) => Box::pin(self.changed_files(&job, head)).await,
                    None => None,
                };
                let check = author.as_ref().and_then(|a| a.check.as_deref());
                let touched = changed.map(|changed| crate::merge::security_paths(&changed, check));
                self.update(id, |job| job.security_paths = touched);
                // What it changes, read by obid outside the box with no git settings but the
                // clone's, before anything runs there: what the review's commands run can't move
                // it. None read, no review.
                let head = job
                    .start_commit
                    .as_deref()
                    .ok_or("obid doesn't know the head to review, so it didn't review it")?;
                let mut change = Box::pin(git::change(
                    &clone,
                    &format!("refs/remotes/origin/{base}"),
                    head,
                ))
                .await
                .map_err(|e| {
                    format!(
                        "obid couldn't compare the head with {base}, so it didn't review it: {e}"
                    )
                })?;
                if change.len() > review::DIFF_MAX {
                    change.truncate(review::DIFF_MAX);
                    change.extend_from_slice(
                        b"\n[obid cut the diff here: it is longer than 8 MiB]\n",
                    );
                }
                own_diff = Some(change);
            }
            if reviews || rules_quoted {
                quoted.push((review::RULES_FILE, review::base_rules(&base, &files)));
            }
            if !quoted.is_empty() {
                let inputs = crate::obi_dir::folder(&worktree, "inputs")
                    .map_err(|e| format!("obid can't make .obi/inputs: {e}"))?;
                for (file, text) in quoted {
                    let text = crate::secrets::known().redact(&crate::vault::for_agent(&text)?);
                    crate::obi_dir::write_new(&inputs, file, text.as_bytes())
                        .map_err(|e| format!("obid couldn't quote {file} for the agent: {e}"))?;
                }
                if let Some(change) = &own_diff {
                    let file = Path::new(review::DIFF_FILE)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .expect("the diff file has a name");
                    crate::obi_dir::write_new(
                        &inputs,
                        file,
                        &crate::vault::for_agent_bytes(change)?,
                    )
                    .map_err(|e| format!("obid can't write the diff: {e}"))?;
                }
            }
        }
        let (admin, common) = self.own_git_dirs(id).await?;
        let temp = self.temp_path(id);
        let (tool, account) = self.job_account(&job)?;
        // An account already reported full needs an owner decision before this job runs.
        let usage = self.usage.get(&account.id);
        if job.obi.is_none()
            && let Some(until) = usage.full_until_at(obi_protocol::time::now())
        {
            return Err(End::AccountLimit {
                reason: format!(
                    "{} hit its usage limit. Choose what Oby should do with the affected jobs.",
                    account.name
                ),
                // A five-hour limit's waits (LIMITS-5). Any other's asks with no reset, as it did,
                // and so does a ChatGPT login's, whatever time it holds from before it was one.
                reset: Some(until)
                    .filter(|at| usage.five_hour_limit(*at, obi_protocol::time::now()))
                    .filter(|_| crate::chatgpt::marker_of(&account).is_none()),
            });
        }
        let job = self.get(id).ok_or("the job is gone")?;
        let mut spec = self.spec(&worktree, &admin, &common, &temp, true, tool);
        // The diff Oby's own review judges, and every input obid quoted for it (what was asked,
        // the issue, the base's rules), stay as obid wrote them, nor can their folders move.
        if own_diff.is_some() {
            spec.frozen.push(worktree.join(review::DIFF_FILE));
            if let Ok(inputs) = std::fs::read_dir(worktree.join(".obi/inputs")) {
                spec.frozen
                    .extend(inputs.flatten().map(|entry| entry.path()));
            }
            spec.fixed
                .extend([worktree.join(".obi"), worktree.join(".obi/inputs")]);
        }
        // HEAD detached at the branch (a worktree from before may be on it), and the folder the
        // job's new objects go to (Spec::git_env).
        let folders = git_folders(&admin, &common, &worktree);
        if let Ok(head) = git::pinned(
            &worktree,
            &folders,
            &["rev-parse", "--verify", "--quiet", "HEAD"],
        )
        .await
            && git::pinned(&worktree, &folders, &["symbolic-ref", "--quiet", "HEAD"])
                .await
                .is_ok()
        {
            git::pinned(
                &worktree,
                &folders,
                &["update-ref", "--no-deref", "HEAD", &head],
            )
            .await
            .map_err(|e| format!("obid couldn't detach the job's HEAD: {e}"))?;
        }
        if spec.own_objects().is_some() {
            own_objects(&admin)?;
        }
        // The work an earlier run left uncommitted is the agent's to commit again, never counted
        // as the job's commits.
        self.unkeep_wip(id, &start_commit)
            .await
            .map_err(|e| format!("obid couldn't take its wip commit back out: {e}"))?;
        if tool == Tool::Cursor {
            // What the repository lets Cursor run without asking obid: a card, as its hooks are
            // (BYPASS-9); Bypass answers it.
            let allowed = acp::cursor::repository_allowlist(&worktree);
            if !allowed.is_empty() {
                self.allow_repository_config(id, &job.repo, &allowed, true, signal)
                    .await?;
            }
            let hooks = acp::cursor::repository_hooks(&worktree);
            if !hooks.is_empty() {
                self.allow_repository_config(id, &job.repo, &hooks, false, signal)
                    .await?;
            }
        }

        let words = Box::pin(self.heard_handing_over(id, &worktree)).await?;
        // A restart cut it short while it ran its check or published ("After any restart"): it
        // picks up there (below), and builds no prompt it won't send (nor asks Tetrix or the
        // teammates for one). Words the owner sent meanwhile are its next turn instead.
        let cut = self.get(id).and_then(|j| j.cut);
        let picks_up = cut.is_some() && words.is_empty();
        // What cut its last run, once (RECOVERY-2): none when a stop's own run picks this up.
        let was_cut = job.recovery.cut.clone();
        if cut.is_some() || was_cut.is_some() {
            self.update(id, |job| (job.cut, job.recovery.cut) = (None, None));
        }
        // Its first turn goes to its own session only, when it's told to carry on there.
        let mut session_only = was_cut.is_some();
        // What obid says first on the next turn when it carries a cut run on by itself
        // (RECOVERY-1 to 3): `message` stays what was last sent, since the cut may have lost it.
        let mut cut_note: Option<String> = None;
        // The first message, when this run built it: a new session needs it (RECOVERY-8).
        let mut first_built: Option<String> = None;
        let mut message = if picks_up {
            String::new()
        } else if !words.is_empty() && job.session.is_some() {
            // A round the owner asked for: the agent picks its session up and hears their words,
            // after the check's output when a failing check stopped it: One more round and Try
            // again alike.
            let words = words.join("\n\n");
            let words = match self.stopping_check(&job) {
                Some(failed) => format!("{failed}\n\n{words}"),
                None => words,
            };
            match &was_cut {
                Some(what) => {
                    let note = continue_message(what, None);
                    match job.recovery.sent.as_deref() {
                        Some(sent) => format!("{}\n\n{words}", after_cut(&note, sent)),
                        None => format!("{note}\n\n{words}"),
                    }
                }
                None => words,
            }
        } else if let Some(sent) = job.recovery.sent.clone().filter(|_| was_cut.is_some()) {
            // What the run obid went down with was sent: the owner's words among it are nowhere else.
            cut_note = was_cut.as_deref().map(|what| continue_message(what, None));
            sent
        } else {
            let first = self
                .first_message(id, &job, &worktree, &base, rules_quoted)
                .await?;
            first_built = Some(first.clone());
            let mut message = first;
            if job.session.is_some() {
                let what = was_cut.as_deref().unwrap_or(WENT_DOWN);
                message = format!("{}\n\n{message}", continue_message(what, None));
            }
            if !words.is_empty() {
                message = format!("{message}\n\n{}", words.join("\n\n"));
            }
            message
        };
        if !Box::pin(self.setup(id, &job.repo, &spec, &mut message, signal)).await {
            return Ok(());
        }
        // What Oby's own review must leave as it was given, past the repository's setup line: the
        // code it runs could move what it reads (PERMISSIONS-14, [`REVIEW_MOVED`]).
        let reviewed_tracked = match job.review_of {
            Some(_) => Box::pin(self.sandboxed_git(id, &TRACKED, None)).await.ok(),
            None => None,
        };
        // What the secret scan found the last time it blocked publishing: the same again stops.
        let mut blocked_before: Option<Vec<crate::publish::Found>> = None;
        // The decision check gave its one round (crate::decisions), in this run or before a restart.
        let mut decided_before = self
            .get(id)
            .and_then(|j| j.tetrix)
            .and_then(|t| t.decisions)
            .is_some_and(|d| d.asked);
        // The turn only the owner's words started after the work's: its reply leaves the job's
        // summary to the work (#699).
        let mut noted = false;
        // The owner's own words were among them (never only obid's rebase or commit prompt, Try
        // again's or what Oby read): their reply is added to the summary, so whoever reads it
        // sees they were heard (#1661).
        let mut steered = false;
        // No clock ends a job that makes progress: only the run's safety ceiling (`jobCeilingHours`).
        // It counts every run's time, restarts and tries again included (`Job::ran_secs`), kept as
        // each turn starts and ends; a check's waits to run again aren't (PUBLISHING-31).
        // ponytail: a turn obid's own crash cuts off isn't counted; keep it every few minutes if
        // crash loops show up.
        let ran_before = self.ran_before(id);
        let began = std::time::Instant::now();
        let ran = || unjudged::unwaited(id, ran_before + began.elapsed());
        // Nothing outside the agent is waited on (the Mac checks went with the strip).
        let waiting: crate::conn::Waiting = Arc::new(|| false);
        // obid told the agent once to wait for what it left running (`WAIT_HERE`), and once to
        // commit what it left uncommitted (`COMMIT_HERE`).
        let (mut told_to_wait, mut told_to_commit) = (false, false);
        // What its last passing check passed on, or why it didn't check its commits (PUBLISHING-5).
        let mut checked: Option<Result<String, String>> = None;
        // Why nothing counted as checked though its commits are kept for Publish by hand
        // (PUBLISHING-8).
        let mut by_hand: Option<String> = None;
        // How the transcript opens the next turn, when it's obid's words that start it.
        let mut opening: Option<String> = None;
        let mut renew: Option<(String, &str)> = None;
        // Its base moved and its commits didn't rebase onto it: its agent had its one round to do it.
        let mut rebase_round = false;
        // The round a failed check gave it, counted when its turn launches.
        let mut round_due = false;
        // Its first launch's tries again, and whether this run is one (RECOVERY-14).
        let (mut launch_tries, mut relaunch) = (0, false);
        // Its agent's last `GOAL MET` or `GOAL NOT MET: …` line, for a job asked for one
        // ([`asks_goal_line`]), and whether a turn that ended its last round without one was told
        // to say it (TASKS-19).
        let (mut goal, mut goal_nudged): (Option<String>, bool) = (None, false);
        // Resume at the interrupted stage, with research excluded from checks (PUBLISHING-28).
        let mut to_publish = false;
        if picks_up {
            self.update(id, |job| {
                job.status = Status::Running;
                job.started.get_or_insert_with(obi_protocol::time::now);
                job.phase = cut;
            });
            let job = self.get(id).ok_or("the job is gone")?;
            // On its base as it is now, as any check is: a conflict is its agent's round.
            let onto = match job.repository_check() {
                Some(_) => Box::pin(self.onto_base(id, &start_commit)).await,
                None => Onto::Same,
            };
            match (job.repository_check().cloned(), onto) {
                (None, _) => {
                    to_publish = true;
                    // Its last GOAL line, from the replies it kept, so a restart holds what its
                    // agent said wasn't met (TASKS-19).
                    // ponytail: `summaries` is per turn, not per round, so this may hold an earlier
                    // round's NOT MET after a restart: that fails safe (nothing goes out); keep the
                    // loop's own `goal` on the job if it ever costs a publish.
                    if asks_goal_line(&job) {
                        goal = job
                            .summaries
                            .iter()
                            .flatten()
                            .rev()
                            .find_map(|s| goal_line(s))
                            .map(str::to_owned);
                    }
                }
                (Some(_), Onto::Conflict(files)) => {
                    rebase_round = true;
                    message = rebase_prompt(&job, &start_commit, &files);
                    noted = true;
                    opening = Some(format!(
                        "Oby told {} to rebase onto the moved base",
                        tool.name()
                    ));
                }
                (Some(command), onto) => {
                    if let Onto::Rebased(base) = onto {
                        start_commit = base;
                    }
                    tracing::info!("job {id}: a restart cut it short, so its check runs again");
                    self.update(id, |job| {
                        job.phase = Some(Phase::Check);
                        job.step = format!("Running {command}");
                    });
                    stages.begin("check");
                    let before = self.before_check(id).await;
                    let run =
                        Box::pin(self.check(id, &command, &before.head, &spec, signal)).await?;
                    if *signal.borrow() != Signal::Run {
                        return Ok(());
                    }
                    if run.passed || run.unchecked.is_some() {
                        checked = Some(self.checked_commit(id, before).await);
                        by_hand = run.unchecked;
                        let (words, owners) = self.last_turn(id);
                        to_publish = words.is_empty();
                        if !to_publish {
                            message = words.join("\n\n");
                            noted = true;
                            steered = owners;
                        }
                    } else {
                        let round = job.current_round();
                        message = self.after_failed_check(
                            id,
                            &job,
                            &command,
                            &run.output,
                            &job.summary,
                            round,
                        )?;
                        round_due = true;
                    }
                }
            }
        }
        'rounds: loop {
            loop {
                if std::mem::take(&mut to_publish) {
                    break;
                }
                // LANES-18: a Codex session made in the other lane's folder can't go on in this one.
                if let Some(note) = self.other_lane(id, tool) {
                    renew = Some((note, "its mode changed its sandbox"));
                }
                let job = self.get(id).ok_or("the job is gone")?;
                // A new session has heard nothing: obid's note, all it's first told, what it was last sent.
                if let Some((note, why)) = renew.take() {
                    let first = match first_built.take() {
                        Some(first) => first,
                        None => {
                            self.first_message(id, &job, &worktree, &base, rules_quoted)
                                .await?
                        }
                    };
                    message = if message.contains(&first) {
                        format!("{note}\n\n{message}")
                    } else {
                        format!("{note}\n\n{first}\n\n{message}")
                            .trim_end()
                            .to_owned()
                    };
                    opening = Some(format!(
                        "Oby started {} in a new session: {why}",
                        tool.name()
                    ));
                }
                // A failed check's next round counts once its turn launches (below), so a restart
                // before then runs the same round's check again.
                let round = job.current_round() + u32::from(round_due);
                let mut flags: Vec<OsString> = [
                    "-p",
                    "--input-format",
                    "stream-json",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    // The words as the agent writes them, for the transcript (Event::Delta).
                    "--include-partial-messages",
                    // Each line obid writes, back as it's taken: the owner's words reach it mid-turn
                    // (crate::steer, TASKCONV-33).
                    "--replay-user-messages",
                    "--permission-prompt-tool",
                    "stdio",
                    "--permission-mode",
                    if Self::mode_of(&job) == Mode::Plan {
                        "plan"
                    } else {
                        "default"
                    },
                    // A worktree's path and git status out of the prompt's cached part, so one job
                    // reuses what another's start wrote (TASKS-15; claude 2.1.98).
                    "--exclude-dynamic-system-prompt-sections",
                    "--max-turns",
                    MAX_TURNS,
                ]
                .iter()
                .map(OsString::from)
                .collect();
                // The own lane loads every settings source, the owner's and the repository's, as
                // their terminal does (the owner's decision of 2026-11-16, LANES-23); the foreign
                // lane loads none (LANES-5, LANES-10).
                let own = Self::own_lane(&job, tool);
                if !own {
                    flags.extend(
                        ["--setting-sources", "", "--strict-mcp-config"].map(OsString::from),
                    );
                }
                // On every run: a resumed one without them has no rules.
                if let Some(rules) = standing_rules(&job) {
                    flags.extend(["--append-system-prompt".into(), rules.into()]);
                }
                let model = job
                    .model
                    .as_deref()
                    .unwrap_or_else(|| crate::settings::default_model(tool));
                // An own run whose login shell names a model (LANES-4) runs on it as written, unless
                // the job or its account picked one.
                let named = Self::own_lane(&job, tool)
                    && job.model.is_none()
                    && tokio::task::spawn_blocking(process::login_env)
                        .await
                        .ok()
                        .flatten()
                        .is_some_and(|env| env.iter().any(|(name, _)| name == "ANTHROPIC_MODEL"));
                if !named {
                    flags.extend(["--model".into(), model.into()]);
                }
                if let Some(effort) = &job.effort {
                    flags.extend(["--effort".into(), effort.into()]);
                }
                if let Some(session) = &job.session {
                    flags.extend(["--resume".into(), session.into()]);
                }
                // Oby's own servers, from a file in the job's own folder, and one permission rule
                // per tool each allows, each its own argument (last, since --allowedTools takes
                // every argument after it). Signed in to Tetrix, obid's own Tetrix server, with
                // Oby's login (crate::tetrix).
                let tetrix = self.tetrix_mcp(&job);
                // Oby's own server, where Codex and Cursor ask the owner (`oby_ask_owner`); Claude
                // Code asks with its own tool. Connected apps went with the strip to a dispatcher.
                let broker = match tool {
                    Tool::Claude => None,
                    Tool::Codex | Tool::Cursor => self.ask_mcp(id),
                };
                // Being given Tetrix's tools leaves the job the owner's; what a call answers marks
                // it where it's read (MERGING-30, the owner's decision of 2026-11-06).
                let mut mcp_rules = Vec::new();
                if broker.is_some() {
                    mcp_rules.push(if tool == Tool::Cursor {
                        format!("mcp__oby__{}", permissions::ASK_OWNER)
                    } else {
                        crate::connections::BROKER_RULE.into()
                    });
                }
                if tetrix.is_some() {
                    self.tetrix_used(id, |_| {});
                    mcp_rules.extend(
                        crate::tetrix::TOOLS
                            .iter()
                            .map(|tool| format!("mcp__tetrix__{tool}")),
                    );
                }
                if (broker.is_some() || tetrix.is_some()) && tool == Tool::Claude {
                    let file = mcp_config_path(own, &spec.temp, &self.login_path(id))?;
                    let mut config = json!({"mcpServers": {}});
                    if let Some((url, token)) = broker.as_ref().filter(|_| tool == Tool::Claude) {
                        config["mcpServers"]["oby"] = json!({"type":"http","url":url,"headers":{"Authorization":format!("Bearer {token}")}});
                    }
                    if let Some((url, token)) = tetrix.as_ref().filter(|_| tool == Tool::Claude) {
                        config["mcpServers"]["tetrix"] = json!({"type":"http","url":url,"headers":{"Authorization":format!("Bearer {token}")}});
                    }
                    // An own run's first turn waits for none of the owner's servers, so obid's are
                    // each `alwaysLoad`: connected before it (LANES-24, `agent::MCP_STARTUP_WAIT`).
                    // A boxed run (`--strict-mcp-config`) has only obid's, and waits for them.
                    if own && let Some(servers) = config["mcpServers"].as_object_mut() {
                        for server in servers.values_mut() {
                            server["alwaysLoad"] = json!(true);
                        }
                    }
                    let text = serde_json::to_vec(&config).map_err(|e| e.to_string())?;
                    let _ = std::fs::remove_file(&file);
                    crate::store::write_private(&file, &text)
                        .map_err(|e| format!("obid can't write the agent's MCP config: {e}"))?;
                    flags.extend(["--mcp-config".into(), file.into_os_string()]);
                    if !mcp_rules.is_empty() {
                        flags.push("--allowedTools".into());
                        flags.extend(mcp_rules.iter().map(OsString::from));
                    }
                }
                // A job run on a teammate's Mac uses the company API key, not this account's login.
                if tool == Tool::Claude
                    && !Self::own_lane(&job, tool)
                    && let Err(why) = self.claude_login_ready(Some(&account)).await
                {
                    return Err(why.into());
                }
                if tool == Tool::Codex
                    && let Err(why) = Box::pin(self.chatgpt_ready(id, &account)).await
                {
                    return Err(End::needs_you(why, Some(crate::job::TRY_AGAIN)));
                }
                let left = self
                    .config
                    .job_ceiling
                    .saturating_sub(Duration::from_secs(ran()));
                // Boxed in here: seven stalls' futures by value made a debug build's frame of this
                // function 300 KB bigger, past tokio's 2 MiB worker stack with what runs under it.
                let stall = |why| {
                    Box::pin(self.stall(
                        id,
                        why,
                        tool,
                        &spec,
                        flags.clone(),
                        &account,
                        broker.as_ref(),
                        signal,
                        &mcp_rules,
                    ))
                };
                if left.is_zero() {
                    return Err(stall(Stall::Ceiling).await);
                }
                if spent(&job) >= job.budget_from.unwrap_or(0) + self.config.job_token_budget {
                    return Err(stall(Stall::Budget).await);
                }
                // Words that came while it was cut go with its next try.
                let told = match cut_note.take() {
                    Some(note) => {
                        session_only = true;
                        let (words, owners) = self.take_words(id);
                        if !words.is_empty() {
                            message = [message.as_str(), &words.join("\n\n")]
                                .into_iter()
                                .filter(|m| !m.is_empty())
                                .collect::<Vec<_>>()
                                .join("\n\n");
                            noted = true;
                            steered = steered || owners;
                        }
                        after_cut(&note, &message)
                    }
                    None => message.clone(),
                };
                // ACP has no system prompt: its rules go with each prompt (TASKS-15).
                let told = match standing_rules(&job).filter(|_| tool == Tool::Cursor) {
                    Some(rules) => format!("{rules}\n\n{told}"),
                    None => told,
                };
                // Why it couldn't start is kept on the account in plain words (ACCOUNTS-19).
                let launch = self
                    .launch(&job, &spec, tool, flags.clone(), &account, broker.as_ref())
                    .map_err(|why| {
                        let plain = self.cant_start(&account, &why);
                        tracing::info!("job {id}: {} didn't start: {why}", tool.name());
                        format!(
                            "{} couldn't start on {}: {plain}.",
                            tool.name(),
                            account.name
                        )
                    })?;
                self.started_on(&account.id);
                if *signal.borrow() != Signal::Run {
                    return Ok(());
                }
                self.update(id, |job| {
                    job.recovery.sent = Some(message.clone());
                    job.stalled = None;
                    job.ran_secs = Some(ran());
                    if job.status == Status::Queued {
                        job.status = Status::Running;
                    }
                    job.started.get_or_insert_with(obi_protocol::time::now);
                    job.phase = Some(Phase::Work);
                    job.round = Some(round);
                    if told_to_wait && message == WAIT_HERE {
                        job.step = "Oby: wait for it in the foreground".into();
                    } else if message == COMMIT_HERE {
                        job.step = "Oby: commit your work".into();
                    } else if !job.step.starts_with("You: ") {
                        job.step = format!("Starting {}", tool.name());
                    }
                });
                ours = round;
                self.stamp_version(id, tool);
                round_due = false;
                stages.begin(&format!("agent round {round}"));
                // A launch the network cut goes again in the same transcript turn (RECOVERY-14).
                if !std::mem::take(&mut relaunch) {
                    self.transcribed(
                        id,
                        self.transcripts.start(
                            id,
                            &opening.take().unwrap_or_else(|| match round {
                                1 => format!("Started on {}", tool.name()),
                                n => format!("Round {n} on {}", tool.name()),
                            }),
                        ),
                    );
                    if round == 1 {
                        self.transcribed(id, self.transcripts.asked(id, &job.request));
                    }
                }
                // `agent.log` keeps every run: only what this one prints says why it ended.
                let log_from =
                    std::fs::metadata(self.log_path(id, "agent.log")).map_or(0, |log| log.len());
                // Read just before the run, so only its agent's commits count, never obid's own
                // rebase between rounds: a run that moved it got somewhere (RECOVERY-1).
                let head_before = self.worktree_head(id).await;
                // A Claude Code or Codex turn takes the owner's words as they come (TASKCONV-33, 36).
                let steer = matches!(tool, Tool::Claude | Tool::Codex).then(|| self.steer_open(id));
                let turn = self
                    .turn(
                        id,
                        tool,
                        launch,
                        &told,
                        signal,
                        Gate::Owner {
                            mcp: &mcp_rules,
                            park: true,
                            session_only: std::mem::take(&mut session_only),
                        },
                        crate::conn::Limit {
                            most: left,
                            quiet: Some(self.config.stall_after),
                            watch: Some(worktree.clone()),
                            waiting: Some(waiting.clone()),
                        },
                        Some(&ran),
                    )
                    .await;
                if let Some(steer) = &steer {
                    self.steer_close(id, steer);
                }
                let turn = turn?;
                self.update(id, |job| job.ran_secs = Some(ran()));
                if *signal.borrow() != Signal::Run {
                    return Ok(());
                }
                // LANES-18: its mode left Bypass under an own Codex run, which was ended: boxed now.
                if !turn.succeeded() && self.left_own_lane(id, tool) {
                    continue;
                }
                if launch_tries < RETRY_WAITS.len()
                    && let Some(cut) = self.offline(id, tool, &turn, log_from)
                {
                    launch_tries += 1;
                    if !Box::pin(self.wait_queued(id, signal, &cut, launch_tries)).await? {
                        return Ok(());
                    }
                    relaunch = true;
                    continue;
                }
                // Tries count failures in a row: a run that moved the head, or a turn that ends well
                // (below), gives them back. The ceiling, budget and loop detector still bound it.
                let head = self.worktree_head(id).await;
                if git::is_sha(&head) && git::is_sha(&head_before) && head != head_before {
                    self.update(id, |job| {
                        job.recovery.errors = 0;
                        job.recovery.stalls = 0;
                    });
                }
                if turn.over_budget {
                    return Err(stall(Stall::Budget).await);
                }
                if turn.looped {
                    return Err(stall(Stall::Loop).await);
                }
                if turn.timed_out {
                    if !turn.quiet {
                        return Err(stall(Stall::Ceiling).await);
                    }
                    // RECOVERY-3: stopped and continued in its session, no model asked; idle,
                    // twice at most (RECOVERY-12).
                    let text = Stall::Quiet.text(&self.config);
                    let tries = if turn.idle { IDLE_TRIES } else { STALL_TRIES };
                    let Some(next) = self.stall_try(id, &text, tries) else {
                        return Err(stall(Stall::Quiet).await);
                    };
                    cut_note = Some(next);
                    opening = Some(format!("Oby continued {}: {text}", tool.name()));
                    continue;
                }
                // The job's summary keeps the agent's words without any secret obid holds, as its
                // transcript does: a login copy's token, a job's MCP token, the vault's.
                let said = crate::secrets::known().redact(&turn.last_said());
                if !turn.succeeded() {
                    // What OpenAI said, when the run went through the relay: Codex's own words
                    // drop its code (crate::chatgpt_relay).
                    let refusal = self.chatgpt.get().and_then(|chatgpt| chatgpt.runs.take(id));
                    let subtype = turn.result.as_ref().and_then(|r| r.get("subtype"));
                    if subtype.and_then(Value::as_str) == Some("error_max_turns") {
                        // RECOVERY-4: the same session goes on, counted with RECOVERY-3's.
                        let text = Stall::Turns.text(&self.config);
                        let Some(next) = self.stall_try(id, &text, STALL_TRIES) else {
                            return Err(stall(Stall::Turns).await);
                        };
                        // 200 turns in, what it was sent reached it.
                        message.clear();
                        cut_note = Some(next);
                        opening = Some(format!("Oby continued {}: {text}", tool.name()));
                        continue;
                    }
                    // RECOVERY-8: too long for its model, Claude Code's own compaction having failed
                    // by now: a new session goes on from the branch, with obid's note, before anyone
                    // is told. Trying again in that one could only fail the same way.
                    if turn.cause() == Some(agent::Cause::TooLong)
                        && let Some(note) = self.new_session(id)
                    {
                        renew = Some((note, "its last was too long for its model"));
                        continue;
                    }
                    // What the CLI said ended the turn, never the agent's own last words, which can
                    // quote anyone. Masked as the stderr note is (#1648): it goes into the job's step
                    // and reason.
                    let error = crate::secrets::known().redact(turn.error());
                    let first = crate::detail::mask(error.lines().next().unwrap_or("").trim());
                    // A passing error that still ends the turn is the agent's: Claude Code and
                    // Codex retried it themselves, so it isn't sent again (RECOVERY-19).
                    // As its windows are now: one past its reset is empty again.
                    let usage = self.usage.get(&job.account);
                    let at = obi_protocol::time::now();
                    let full = usage.full_until_at(at).is_some() || usage.window_full(at);
                    // The CLI ended with no result, its session there to go on in: what it last
                    // said isn't why, its exit and its stderr are.
                    let session = self.get(id).is_some_and(|j| j.session.is_some());
                    let no_result = turn.result.is_none() && session;
                    let ended = format!(
                        "{} stopped without finishing ({}).{}",
                        tool.name(),
                        turn.exit,
                        self.agent_note_from(id, log_from)
                    );
                    let limit_said = turn.resets.is_some()
                        || full
                        || turn.says_limit(&ended)
                        || turn.says_limit(&first);
                    // A limit the account's usage or the tool reports is #1563's, whatever cut it.
                    use crate::chatgpt_relay::After;
                    // On a ChatGPT login only the relay's refusal says it's a limit: a usage read
                    // doesn't exist, and OpenAI's limit names no time (the contract).
                    let chatgpt = crate::chatgpt::marker_of(&account).is_some();
                    // RECOVERY-1: one denylist, the tool's own fields first (RECOVERY-7). OpenAI's
                    // refusal through the relay went back to Codex, which retried it (RECOVERY-19).
                    let of = match &refusal {
                        Some(_) => 0,
                        None if !chatgpt && limit_said => 0,
                        None => {
                            let words = if no_result { &ended } else { &error };
                            // KEYCHAIN-10: a boxed run's copy that ended, never a revoked login.
                            let ended = tool == Tool::Claude
                                && !Self::own_lane(&job, tool)
                                && !login_copy_lasts(&self.login_path(id));
                            retries(turn.cause(), words, session, no_result, ended)
                        }
                    };
                    let recovery = self.get(id).map(|j| j.recovery).unwrap_or_default();
                    let tries = recovery.errors;
                    if tries < of && !recovery.spent() {
                        // RECOVERY-1: running, in its same session and worker, so Stop works, no
                        // card shows and the tries count with what came before them.
                        let error = match &refusal {
                            Some(refusal) => refusal.words.as_str(),
                            None if no_result || first.is_empty() => ended.as_str(),
                            None => first.as_str(),
                        };
                        let attempt = tries + 1;
                        self.update(id, |job| {
                            job.recovery.errors = attempt;
                            job.recovery.tries += 1;
                        })
                        .ok_or("the job couldn't be kept")?;
                        let ran_now = |job: &mut Job| job.ran_secs = Some(ran());
                        let of = (attempt, of);
                        if !self
                            .wait_to_try(id, signal, error, CUT_WAITS[tries], of, ran_now)
                            .await
                        {
                            return Ok(());
                        }
                        if session {
                            cut_note =
                                Some(continue_message("an error ended your turn", Some(error)));
                            opening = Some(format!(
                                "Oby continued {}: an error cut its run",
                                tool.name()
                            ));
                        }
                        continue;
                    }
                    let mut reason = if first.is_empty() { ended } else { first };
                    // OpenAI refused the Codex login, as Codex itself says (never the agent's own
                    // words): [`codex_refused_copy`] ends it, or says run codex once when the copy
                    // already expired (ACCOUNTS-18).
                    if let Some(refusal) = &refusal {
                        reason = refusal.words.clone();
                    } else if tool == Tool::Codex
                        && let Some(error) = codex_error(&reason).filter(|e| codex_login_refused(e))
                    {
                        let login = self.login_path(id);
                        let copy = match account.config_dir {
                            Some(_) => login.join("auth.json"),
                            None => login.join(".codex/auth.json"),
                        };
                        reason = codex_refused_copy(&copy, &account.name, error);
                    }
                    // A usage limit, a login, a crash: what's done is kept, and Try again picks the
                    // session up (after the reset, the sign-in). Past a usage limit's reset, obid
                    // tries again by itself; a reset already gone by is a stale one, and arms
                    // nothing that could loop. Not a stall: #773's budget and ceiling still hold,
                    // since the Try-again words go through `Jobs::message` as the owner's would.
                    let now = obi_protocol::time::now();
                    let limited = match &refusal {
                        Some(refusal) => refusal.after == After::Limit,
                        None if chatgpt => false,
                        None => turn.resets.is_some() || full || turn.says_limit(&reason),
                    };
                    let reset = turn
                        .resets
                        .or_else(|| {
                            turn.says_limit(&reason)
                                .then(|| crate::limits::reset_in(&reason, now))
                                .flatten()
                        })
                        // Else when the account's own usage says it's full until (#745). OpenAI's
                        // limit names no time, and an hour obid inferred never reaches the card.
                        .or_else(|| limited.then(|| usage.full_until_at(now)).flatten())
                        .filter(|_| !chatgpt);
                    // The account is full whatever the tool said, but for one model's (USAGE-13), so
                    // nothing else goes there until it resets; the job goes on where there's room.
                    if limited {
                        self.limit_reached(id, &account.id, reset, &turn);
                    }
                    if limited && job.obi.is_none() {
                        return Err(End::AccountLimit {
                            reason,
                            reset: reset.filter(|at| *at > now),
                        });
                    }
                    if tries < of && recovery.spent() {
                        reason = crate::job::capped(&reason);
                    }
                    // Its login was refused: the account can't run until that changes (ACCOUNTS-19).
                    if cant_run::login_refused(turn.cause(), &reason) {
                        self.cant_start(&account, &reason);
                    }
                    let resumes = None;
                    return Err(End::NeedsYou {
                        reason,
                        next: Some(crate::job::TRY_AGAIN),
                        resumes,
                    });
                }
                self.update(id, |job| {
                    job.recovery.sent = None;
                    job.recovery.errors = 0;
                    job.recovery.stalls = 0;
                    // A turn that said nothing (its last message a tool call, say) keeps the
                    // earlier round's summary and adds no empty one to the rounds' (#764).
                    if said.trim().is_empty() {
                        if !noted {
                            job.summary = job
                                .summaries
                                .as_ref()
                                .and_then(|s| s.last())
                                .cloned()
                                .unwrap_or_default();
                        }
                        return;
                    }
                    if !noted || job.summary.is_empty() {
                        job.summary = said.clone();
                    } else if steered {
                        job.summary =
                            format!("{}\n\n{}{}", job.summary, crate::job::STEERED, said.trim());
                    }
                    job.summaries
                        .get_or_insert_with(Vec::new)
                        .push(said.clone());
                });
                noted = false;
                steered = false;
                self.note_diff(id, &start_commit, round).await;
                // It asked the owner (`oby_ask_owner`) and ended its turn: it waits for the answer,
                // which starts its next turn, the owner's words that came meanwhile with it.
                if let Some(call) = self.lock().asked.remove(id) {
                    return Err(End::Asked(Asked::Question(call)));
                }
                // Its answer came first and ended this run (PERMISSIONS-23): the next run hears it.
                if *signal.borrow() != Signal::Run {
                    return Ok(());
                }
                if self.ends_on_plan(id, round, &said) {
                    return Err(End::Asked(Asked::Plan(said)));
                }
                // Words the owner sent while it worked: the agent hears them now, in another turn of
                // this round, before the check.
                let (words, owners) = self.take_words(id);
                if !words.is_empty() {
                    message = words.join("\n\n");
                    noted = true;
                    steered = owners;
                    continue;
                }
                // It ended its turn to wait for something still running: once, it's told to wait
                // in the foreground; again, it needs the owner, never "nothing committed".
                let waits = if tool == Tool::Claude {
                    turn.left_running
                } else {
                    waits_for_later(&said)
                };
                if waits {
                    if !told_to_wait {
                        told_to_wait = true;
                        message = WAIT_HERE.into();
                        opening = Some(format!(
                            "Oby told {} to wait for its work in the foreground",
                            tool.name()
                        ));
                        continue;
                    }
                    return Err(End::needs_you(
                        format!(
                            "Stopped waiting: {} The agent ended its turn to wait for work still running, and nothing wakes it once its turn is over. The branch is kept: Try again picks its session up.",
                            one_line(
                                said.lines()
                                    .find(|l| !l.trim().is_empty())
                                    .unwrap_or("")
                                    .trim(),
                                200
                            )
                        ),
                        Some(crate::job::TRY_AGAIN),
                    ));
                }
                let job = self.get(id).ok_or("the job is gone")?;
                let more_rounds = round < job.max_rounds();
                // What it's checked on, and publishes, is on its base as it is now.
                if job.repository_check().is_some() {
                    match Box::pin(self.onto_base(id, &start_commit)).await {
                        Onto::Same => {}
                        Onto::Rebased(base) => start_commit = base,
                        Onto::Conflict(files) if !rebase_round => {
                            rebase_round = true;
                            message = rebase_prompt(&job, &start_commit, &files);
                            // Its reply leaves the job's summary to the work's.
                            noted = true;
                            message = self.reopened(id, message);
                            opening = Some(format!(
                                "Oby told {} to rebase onto the moved base",
                                tool.name()
                            ));
                            continue;
                        }
                        Onto::Conflict(files) => return Err(not_rebased(&job, &files)),
                    }
                }
                if let Some(command) = job.repository_check() {
                    self.update(id, |job| {
                        job.phase = Some(Phase::Check);
                        job.step = format!("Running {command}");
                    });
                    stages.begin("check");
                    let before = self.before_check(id).await;
                    let run =
                        Box::pin(self.check(id, command, &before.head, &spec, signal)).await?;
                    if *signal.borrow() != Signal::Run {
                        return Ok(());
                    }
                    if run.passed || run.unchecked.is_some() {
                        checked = Some(self.checked_commit(id, before).await);
                        by_hand = run.unchecked;
                        // The last turn, unless the owner's words came meanwhile: those are the next.
                        let (words, owners) = self.last_turn(id);
                        if words.is_empty() {
                            break;
                        }
                        (message, noted, steered) = (words.join("\n\n"), true, owners);
                        continue;
                    }
                    message =
                        self.after_failed_check(id, &job, command, &run.output, &said, round)?;
                } else if asks_goal_line(&job) {
                    // A reply with no line (to obid's commit or rebase prompt, say) keeps the
                    // round's last; a new round starts with none (below).
                    if let Some(line) = goal_line(&said) {
                        goal = Some(line.to_owned());
                    }
                    if goal.as_deref() == Some("GOAL MET")
                        || (!more_rounds && (goal.is_some() || goal_nudged))
                    {
                        // The last turn, unless the owner's words came meanwhile: those are the next.
                        let (words, owners) = self.last_turn(id);
                        if words.is_empty() {
                            break;
                        }
                        (message, noted, steered) = (words.join("\n\n"), true, owners);
                        // Their words decide: the reply to them says the goal again.
                        goal = None;
                        continue;
                    }
                    message = "Your last line didn't say `GOAL MET`: carry on with what's left, then end with `GOAL MET` or `GOAL NOT MET: <what's left>`.".into();
                    // Its last round with no line: told once, spending no round (TASKS-19).
                    if !more_rounds {
                        goal_nudged = true;
                        // Its reply leaves the job's summary to the work's.
                        noted = true;
                    }
                } else {
                    // The last turn, unless the owner's words came meanwhile: those are the next.
                    let (words, owners) = self.last_turn(id);
                    if words.is_empty() {
                        break;
                    }
                    (message, noted, steered) = (words.join("\n\n"), true, owners);
                    continue;
                }
                // What obid declined last round, for a tool whose answers carry no words (ACP).
                if !turn.notes.is_empty() {
                    message = format!("{}\n\n{message}", turn.notes.join("\n"));
                }
                // The last round's nudge (just above) spends no round.
                if goal_nudged && !more_rounds {
                    continue;
                }
                round_due = true;
                // The next round's own line decides, never this one's (TASKS-19).
                goal = None;
            }

            // With no check to run on it, it's put on its base as it is now before it publishes.
            if let Some(job) = self.get(id).filter(|j| j.check.is_none()) {
                match Box::pin(self.onto_base(id, &start_commit)).await {
                    Onto::Same => {}
                    Onto::Rebased(base) => start_commit = base,
                    Onto::Conflict(files) if !rebase_round => {
                        rebase_round = true;
                        message = rebase_prompt(&job, &start_commit, &files);
                        noted = true;
                        message = self.reopened(id, message);
                        opening = Some(format!(
                            "Oby told {} to rebase onto the moved base",
                            tool.name()
                        ));
                        continue 'rounds;
                    }
                    Onto::Conflict(files) => return Err(not_rebased(&job, &files)),
                }
            }
            // What it made to be looked at, listed on the job before it ends.
            stages.begin("artifacts");
            self.publish_artifacts(id).await;
            stages.begin("keep commits");
            let head = Box::pin(self.keep_commits(id)).await?;
            // What its check passed on (PUBLISHING-1, PUBLISHING-5): all that publishing will push,
            // by itself or by hand, while its branch is still there. One with no check: its head.
            let verified = match (
                self.get(id).and_then(|j| j.repository_check().cloned()),
                checked.take(),
            ) {
                (None, _) => Ok(head.clone()),
                (Some(_), Some(Ok(commit))) if commit == head => Ok(commit),
                (Some(_), Some(Err(why))) => Err(why),
                (Some(_), _) => Err(CHECK_MOVED_HEAD.to_owned()),
            };
            self.update(id, |job| job.checked_head = verified.clone().ok());
            let job = self.get(id).ok_or("the job is gone")?;
            let folders = self.pinned_git(id).await.unwrap_or_default();
            let commits = self.counted(&job, &worktree, &folders, &head).await;
            // Work it left uncommitted, none committed or checked with them: once, it's told to
            // commit it, then the check runs on what it committed. Before the job's end is told to
            // anyone.
            if COMMITTING.contains(&job.kind.as_str())
                && (commits == 0 || verified.is_err())
                && !told_to_commit
                && self.uncommitted(id).await
            {
                told_to_commit = true;
                message = COMMIT_HERE.into();
                opening = Some(format!("Oby told {} to commit its work", tool.name()));
                // Its reply leaves the job's summary to the work's.
                noted = true;
                message = self.reopened(id, message);
                continue 'rounds;
            }
            self.teammates_ended(id, &worktree, &start_commit).await;
            self.tetrix_ended(id, &worktree, &start_commit).await;
            let job = self.get(id).ok_or("the job is gone")?;
            // Nothing committed: done, as Claude Code is when its model stops, and the agent's
            // summary says why. A round the owner asked for says nothing of it: their words decide.
            if COMMITTING.contains(&job.kind.as_str()) && commits == 0 && !job.asked {
                self.end_done(id, ours, 0, job.pushed, |job| {
                    job.reason = Some(NOTHING_COMMITTED.into());
                });
                return Ok(());
            }
            // A review publishes nothing: one that committed is done, its review standing, its
            // commits counted and kept on its branch, never published.
            if job.kind == "review" && commits > 0 {
                self.end_done(id, ours, commits, job.pushed, |job| {
                    job.reason = Some(crate::publish::REVIEW_COMMITTED.into());
                });
                self.offer_owner_review(id).await;
                return Ok(());
            }
            // Oby's own review ran code (PERMISSIONS-14): its verdict counts only on the commit
            // and the tracked files it was given, as they were when it started.
            if job.review_of.is_some()
                && (job.start_commit.as_deref() != Some(head.as_str())
                    || reviewed_tracked.is_none()
                    || Box::pin(self.sandboxed_git(id, &TRACKED, None)).await.ok()
                        != reviewed_tracked)
            {
                self.end_done(id, ours, 0, job.pushed, |job| {
                    job.reason = Some(REVIEW_MOVED.into());
                });
                self.offer_owner_review(id).await;
                return Ok(());
            }
            // Its agent says the goal isn't met: its commits are kept and nothing goes out by
            // itself, though the owner's Publish still sends them (TASKS-19, PUBLISHING-9).
            if let Some(left) = goal.as_deref().filter(|l| l.starts_with("GOAL NOT MET"))
                && commits > 0
            {
                let left = one_line(&crate::job::readable(left), 300);
                let stop = if left.ends_with(['.', '!', '?']) {
                    ""
                } else {
                    "."
                };
                return Err(End::needs_you(
                    format!(
                        "{left}{stop} The branch is kept and nothing was pushed: tell it what's left, or Publish it as it is."
                    ),
                    None,
                ));
            }
            // Carrying only commits whose pull request was closed unmerged: Publish is the owner's.
            if let Some(first) = self.first_turned_down(&job)
                && count_commits(
                    &worktree,
                    &self.pinned_git(id).await.unwrap_or_default(),
                    &start_commit,
                    &head,
                )
                .await
                    == 0
            {
                self.end_done(id, ours, commits, job.pushed, |job| {
                    job.reason = Some(format!(
                        "The pull request of {first} was closed without merging, so Oby didn't publish its commits again: Publish opens a new pull request with them."
                    ));
                });
                return Ok(());
            }
            let pushed = self.still_pushed(&job, &head, commits).await;
            // Its check didn't pass on its commits alone: done, nothing published, and why.
            if let Err(why) = &verified {
                self.end_done(id, ours, commits, pushed, |job| {
                    job.reason = Some(why.clone())
                });
                return Ok(());
            }
            if let Some(why) = by_hand.take()
                && job.kind != "research"
            {
                self.end_done(id, ours, commits, pushed, |job| job.reason = Some(why));
                return Ok(());
            }
            // The job as it would end, which is what gets published.
            let mut ended = job.clone();
            ended.status = Status::Done;
            ended.commits = Some(commits);
            ended.pushed = pushed;
            // One whose own publish failed isn't tried again by itself, unless this run made new
            // commits; one a restart cut short (its publish never ran) carries on and publishes.
            let made_now = before_head.as_deref() != Some(head.as_str());
            // A job a standing rule started (#1314), or one whose conversation read something
            // untrusted, publishes by itself like any other, after its checks and the secret scan
            // (the owner's decision of 2026-10-24, replacing 2026-10-23's by hand), and merging
            // still waits for a review.
            if !ended.can_publish() || (!made_now && job.publish_error.is_some()) {
                // Still waiting for Publish by hand: why its own publish failed still holds.
                let waiting = ended.can_publish();
                self.end_done(id, ours, commits, pushed, |job| {
                    if !waiting {
                        job.publish_error = None;
                    }
                });
                if job.kind == "review" {
                    self.offer_owner_review(id).await;
                }
                return Ok(());
            }
            // Before it goes out: what it changes against earlier sessions' decisions. One round
            // to put it back or say why; what's left goes into the pull request, never a block.
            let conflicts = self
                .decision_check(id, &head, &start_commit, decided_before, signal)
                .await;
            // Tetrix may take a while: stopped meanwhile, nothing more.
            if *signal.borrow() != Signal::Run {
                return Ok(());
            }
            if !conflicts.is_empty() && !decided_before {
                decided_before = true;
                let quoted = crate::decisions::quoted(&conflicts, &crate::brain::briefing::nonce());
                let written = crate::obi_dir::folder(&worktree, "inputs").and_then(|dir| {
                    crate::obi_dir::write_new(&dir, crate::decisions::FILE, quoted.as_bytes())
                });
                // Without the quoted file the round can't be told what was decided: it goes out
                // as it is, the conflicts listed in the pull request.
                if let Err(e) = written {
                    tracing::warn!("job {id}: the decisions weren't quoted: {e}");
                    self.update(id, |job| {
                        if let Some(d) = job.tetrix.as_mut().and_then(|t| t.decisions.as_mut()) {
                            d.asked = false;
                        }
                    });
                } else {
                    message = crate::decisions::prompt(&conflicts);
                    message = self.reopened(id, message);
                    self.update(id, |job| {
                        job.phase = Some(Phase::Work);
                        job.publish_step = None;
                        job.round = Some(job.current_round() + 1);
                        job.step = format!(
                            "Round {} of {}: a change goes against an earlier decision",
                            job.current_round(),
                            job.max_rounds().max(job.current_round())
                        );
                    });
                    continue 'rounds;
                }
            }
            self.update(id, |job| {
                job.commits = Some(commits);
                job.pushed = pushed;
                job.phase = Some(Phase::Publish);
                job.publish_step = Some(crate::publish::Step::Scan);
                job.step = "Publishing".into();
            });
            // Publish times its own steps (PublishHooks).
            stages.end();
            use crate::publish::Failure;
            let mut published = self
                .publish_job(id, Some(signal.clone()), |_, _| Ok(()), &Default::default())
                .await;
            // A try whose pull request failed pushed (#627): counted at once, so neither a stop
            // nor a later try's other failure forgets it.
            let mut went_out = false;
            let mut count_push = |published: &Result<Option<String>, Failure>| {
                if !went_out && matches!(published, Err(Failure::PullRequest(_))) {
                    went_out = true;
                    self.update(id, |job| job.pushed = Some(commits));
                }
            };
            count_push(&published);
            // GitHub or the remote didn't take it: published again after each wait, the waits
            // a round's transient errors get (capped by the stall window, as theirs are).
            for (tried, wait) in RETRY_WAITS.iter().enumerate() {
                let Err(failure) = &published else { break };
                if !failure.may_pass_later() || *signal.borrow() != Signal::Run {
                    break;
                }
                let wait = (*wait).min(self.config.stall_after);
                let step = format!(
                    "Publishing again in {} ({} of {}): {}",
                    spell(wait),
                    tried + 1,
                    RETRY_WAITS.len(),
                    one_line(&failure.line(), 120)
                );
                tracing::info!("job {id}: {step}");
                self.update(id, |job| job.step = step.clone());
                let mut stop = signal.clone();
                tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    _ = stop.wait_for(|s| *s != Signal::Run) => return Ok(()),
                }
                published = self
                    .publish_job(id, Some(signal.clone()), |_, _| Ok(()), &Default::default())
                    .await;
                count_push(&published);
            }
            if *signal.borrow() != Signal::Run {
                return Ok(());
            }
            let pushed = if went_out { Some(commits) } else { pushed };
            match published {
                Ok(Some(url)) => {
                    self.end_done(id, ours, commits, Some(commits), |job| {
                        job.was_published(url);
                    });
                    self.follow.notify_one();
                    // Published by itself, Oby reviews it; published by hand, the owner does.
                    self.start_review(id);
                }
                // Pushed to a remote that has no pull requests: that's all of it.
                Ok(None) => {
                    self.end_done(id, ours, commits, Some(commits), |job| {
                        job.publish_error = None
                    });
                }
                // Pushed, and only the pull request failed: the commits count as out (#627).
                Err(failure @ Failure::PullRequest(_)) => {
                    let why = failure.line();
                    self.end_done(id, ours, commits, Some(commits), |job| {
                        job.publish_error = Some(why)
                    });
                }
                Err(Failure::AlreadyOnBase(base)) => {
                    let line = Failure::AlreadyOnBase(base).line();
                    self.end_done(id, ours, 0, pushed, |job| {
                        job.summary = format!("{line}\n\n{}", job.summary);
                        job.publish_error = None;
                    });
                }
                Err(Failure::Blocked(found)) => {
                    let job = self.get(id).ok_or("the job is gone")?;
                    // One bounded round to rewrite the commits that hold it: the first block gets it
                    // even with no rounds left (as Claude Code fixes what a hook refused and commits
                    // again), later ones while rounds are left and it found something else.
                    if (blocked_before.is_none() || job.current_round() < job.max_rounds())
                        && blocked_before.as_ref() != Some(&found)
                        && found.iter().all(crate::publish::Found::fixable)
                    {
                        let base = job
                            .base
                            .clone()
                            .unwrap_or_else(|| repo.default_branch.clone());
                        message = crate::publish::fix_prompt(&found, &base);
                        blocked_before = Some(found);
                        message = self.reopened(id, message);
                        self.update(id, |job| {
                            job.phase = Some(Phase::Work);
                            job.publish_step = None;
                            job.round = Some(job.current_round() + 1);
                            job.step = format!(
                                "Round {} of {}: the secret scan blocked publishing",
                                job.current_round(),
                                job.max_rounds().max(job.current_round())
                            );
                        });
                        continue 'rounds;
                    }
                    // Its fix round couldn't clear it: the commits stay unpublished on the branch. Try
                    // again rewrites them once more; a finding no round can fix is for a person.
                    let fixable = found.iter().all(crate::publish::Found::fixable);
                    return Err(End::needs_you(
                        crate::publish::blocked(&found),
                        fixable.then_some(crate::job::TRY_AGAIN),
                    ));
                }
                Err(Failure::Stopped) => return Ok(()),
                Err(
                    failure @ (Failure::Remote(_)
                    | Failure::Failed(_)
                    | Failure::Busy
                    | Failure::Refused(_)),
                ) => {
                    let why = failure.line();
                    self.end_done(id, ours, commits, pushed, |job| {
                        job.publish_error = Some(why)
                    });
                }
            }
            return Ok(());
        }
    }
}
