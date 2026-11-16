//! Another Oby's review on this Mac (docs/contracts/teammate-review.md, TEAMREVIEW-3 and 4): what
//! this Oby has to start one, the job it starts, and the result of one with no check to run. It
//! owns the review job's start and that result; the reply and the publish gate are the exchange's
//! (`sphere::Link::review_ended`), and it never writes the exchange.

use super::*;

use crate::sphere::review_start::Can;

impl Jobs {
    /// What this Oby has for a review of `repo` (TEAMREVIEW-3): a repository the owner's jobs would
    /// take (listed, or one of GitHub's gh can push to), a Claude Code account routing would pick
    /// for background work, and the job sandbox. Reads only obid's memory, so it may run under the
    /// Link's lock.
    pub fn can_review(&self, repo: &str) -> Can {
        Can {
            pushable: self.repo(repo).is_some(),
            account: self
                .pick_for(Some(Tool::Claude.id()), None, &Work::new(None, None, true))
                .is_ok(),
            sandbox: self.sandbox.is_ok(),
        }
    }

    /// Starts the review another Obi asked for (runner-api.md, "obid only: Obi-to-Obi review jobs",
    /// step 1): a `review` job on an account routing picks, titled from typed fields only.
    pub fn start_obi_review(self: &Arc<Self>, request: ObiReview) -> Result<Job, NotMade> {
        let body: ReviewRequest = serde_json::from_value(request.body.clone())
            .map_err(|e| Refused(format!("The request isn't a review_request: {e}")))?;
        // A repository the owner's own jobs take: listed, or one of GitHub's gh can push to.
        let repo = self
            .repo(&body.repo)
            .ok_or_else(|| Refused(format!("{} isn't a repository obid knows.", body.repo)))?;
        // Claude Code's: the review's answer is its structured output (`--json-schema`).
        let (_, account) =
            self.pick_for(Some(Tool::Claude.id()), None, &Work::new(None, None, true))?;
        let id = self.new_id();
        let repo_name = body
            .repo
            .rsplit('/')
            .next()
            .unwrap_or(&body.repo)
            .to_owned();
        let task = format!(
            "Review {} for {}: {} at {} into {}.",
            body.repo,
            request.from_owner,
            body.head_branch,
            &body.head_sha[..7.min(body.head_sha.len())],
            body.base_branch
        );
        let mut job = Job::new(
            id,
            body.repo.clone(),
            "review".into(),
            task,
            "claude".into(),
            account.id,
        );
        job.title = Some(format!("Review for {}: {repo_name}", request.from_owner));
        job.branch = body.head_branch.clone();
        job.base = Some(body.base_branch.clone());
        job.pr = body.pr;
        job.check = repo.check.clone();
        job.mode = Some("bypass".into());
        job.round = Some(1);
        job.obi = Some(ObiRef {
            correlation_id: request.correlation_id.clone(),
            from: request.from.clone(),
            from_owner: request.from_owner.clone(),
            request_id: request.request_id.clone(),
        });
        job.obi_request = Some(json!({
            "from": request.from, "from_owner": request.from_owner, "type": "review_request", "body": request.body
        }));
        job.phase = Some(Phase::Prepare);
        job.step = format!("Getting {repo_name} ready");
        let job = self.insert(job).map_err(NotMade::Unkept)?;
        // The first review in one of GitHub's repositories makes it one of obid's (crate::repos).
        if let Err(why) = self.config.repos.keep(&repo) {
            tracing::warn!("{why}");
        }
        self.schedule();
        Ok(job)
    }

    /// A review with no check to run (TEAMREVIEW-4): it can't vouch that the tests pass, so no
    /// agent runs and the result is `failed`, saying so, through the exchange's publish gate. It
    /// ends done in its round `ours` (`end_done`).
    pub(super) async fn review_without_a_check(
        self: &Arc<Self>,
        id: &str,
        ours: u32,
        request: &ReviewRequest,
    ) {
        let body = review::no_check_result(request);
        self.update(id, |job| {
            job.review_result = Some(body.clone());
            job.approvable = false;
            job.step = "Scanning the result for secrets and sending it".into();
        });
        self.review_ended(id, true).await;
        self.end_done(id, ours, 0, None, |job| {
            job.summary = format!(
                "Review for {}: failed, 0 finding(s), no check to run.",
                job.obi
                    .as_ref()
                    .map(|o| o.from_owner.as_str())
                    .unwrap_or("another Oby")
            );
            job.review_result = Some(body);
        });
    }

    /// A review another Obi asked for, steps 2 to 7 (runner-api.md, "obid only: Obi-to-Obi review
    /// jobs"). The publish gate and the reply are the exchange's, once the job has its result.
    pub(super) async fn run_obi_review(
        self: &Arc<Self>,
        id: &str,
        signal: &watch::Receiver<Signal>,
    ) -> Result<(), String> {
        let job = self.get(id).ok_or("the job is gone")?;
        // It starts over from a fresh worktree whatever a restart cut: nothing to pick up.
        if job.cut.is_some() {
            self.update(id, |job| job.cut = None);
        }
        let quoted = job.obi_request.clone().ok_or("the job has no request")?;
        let request: ReviewRequest = serde_json::from_value(quoted["body"].clone())
            .map_err(|e| format!("the request isn't a review_request: {e}"))?;
        // With no check to run it can't vouch that the tests pass (TEAMREVIEW-4): no agent either.
        if job.check.is_none() {
            self.review_without_a_check(id, job.current_round(), &request)
                .await;
            return Ok(());
        }
        if let Err(why) = &self.sandbox {
            return Err(format!(
                "obid can't make the job sandbox on this machine, so it ran nothing: {why}"
            ));
        }
        let repo = self
            .repo(&request.repo)
            .ok_or_else(|| format!("{} isn't a repository obid knows.", request.repo))?
            .clone();
        if !git::is_branch_name(&request.head_branch)
            || !git::is_branch_name(&request.base_branch)
            || !git::is_sha(&request.head_sha)
        {
            return Err("The request's branches or commit aren't ones obid can fetch.".into());
        }

        // 2. The head (and the base, to compare with), into obid's clone; a fresh worktree at head_sha.
        self.update(id, |job| {
            job.step = format!("Fetching {}", request.head_branch)
        });
        let preparing = self.preparing(&repo.repo);
        let prepared = preparing.lock().await;
        let head_ref = format!(
            "+refs/heads/{0}:refs/remotes/origin/{0}",
            request.head_branch
        );
        // A clone of its own, apart from the owner's: no owner's job's worktree sees what another
        // Oby asked to have reviewed (crate::secretspec).
        let clone = git::ensure_clone(&self.received_home(), &repo.repo, &repo.remote, &[head_ref])
            .await
            .map_err(|e| {
                format!(
                    "Couldn't fetch {} from {}: {e}",
                    request.head_branch, repo.repo
                )
            })?;
        let base_ref = format!(
            "+refs/heads/{0}:refs/remotes/origin/{0}",
            request.base_branch
        );
        // A request to approve the change on GitHub is reviewed only against its base as fetched:
        // without it, the review would miss what the change brings into the branch it approves.
        let approving = request.approve_on_host == Some(true);
        if let Err(e) = git::fetch(&clone, &repo.remote, &[base_ref]).await {
            tracing::info!(
                "job {id}: the base {} didn't fetch: {e}",
                request.base_branch
            );
            if approving {
                return Err(format!(
                    "Couldn't fetch the base branch {} from {}, so obid didn't review a change it was asked to approve: {e}",
                    request.base_branch, repo.repo
                ));
            }
        }
        if !git::has_commit(&clone, &request.head_sha).await {
            let _ = git::fetch(
                &clone,
                &repo.remote,
                std::slice::from_ref(&request.head_sha),
            )
            .await;
        }
        if !git::has_commit(&clone, &request.head_sha).await {
            return Err(format!(
                "Couldn't fetch {} from {}: the remote doesn't have it.",
                request.head_sha, repo.repo
            ));
        }
        let worktree = self.worktree_path(id);
        // A restart: start again from a fresh one.
        remove_worktree(&clone, &worktree).await;
        std::fs::create_dir_all(worktree.parent().expect("worktrees have a folder"))
            .map_err(|e| format!("obid can't make its worktrees folder: {e}"))?;
        let mut args: Vec<&std::ffi::OsStr> =
            git::LFS_POINTERS.iter().map(|a| a.as_ref()).collect();
        args.extend::<[&std::ffi::OsStr; 6]>([
            "worktree".as_ref(),
            "add".as_ref(),
            "--quiet".as_ref(),
            "--detach".as_ref(),
            worktree.as_os_str(),
            request.head_sha.as_ref(),
        ]);
        git::run(&clone, &args, &[])
            .await
            .map_err(|e| format!("obid couldn't make the job's worktree: {e}"))?;
        let worktree_text = worktree.to_string_lossy().into_owned();
        self.update(id, |job| {
            job.made_worktree(worktree_text);
            job.start_commit = Some(request.head_sha.clone());
        });
        // A link the branch commits that leads out of the worktree fails the review before the
        // agent starts: Claude Code reads inside its working folder, links followed, without
        // asking obid, and the job sandbox lets the agent read its own account folder.
        let links = git::committed_links(&clone, &request.head_sha)
            .await
            .map_err(|e| format!("obid couldn't list the branch's links: {e}"))?;
        if let Some(out) = links
            .iter()
            .find(|link| crate::policy::link_leads_out(&worktree, link))
        {
            return Err(format!(
                "The branch commits a link that leads out of the review's worktree ({:?}), so obid didn't review it: its agent would read through the link.",
                out.to_string_lossy()
            ));
        }
        // The change, for the agent, which can't run git: from the base as fetched to the head.
        let base = format!("refs/remotes/origin/{}", request.base_branch);
        let mut change = match git::change(&clone, &base, &request.head_sha).await {
            Ok(diff) => diff,
            Err(e) if approving => {
                return Err(format!(
                    "obid couldn't compare the head with the base branch, so it didn't review a change it was asked to approve: {e}"
                ));
            }
            Err(e) => {
                tracing::info!("job {id}: no diff against the base: {e}");
                b"obid couldn't compare the head with the base branch (it didn't fetch, or they share no history). Review the files as they are in this worktree.\n".to_vec()
            }
        };
        if change.len() > review::DIFF_MAX {
            change.truncate(review::DIFF_MAX);
            change.extend_from_slice(b"\n[obid cut the diff here: it is longer than 8 MiB]\n");
        }
        // The repository's rules as the base has them, never as the change writes them.
        let mut rules = Vec::new();
        for name in ["AGENTS.md", "CLAUDE.md"] {
            let blob = format!("{base}:{name}");
            rules.push((
                name,
                git::git(&clone, &["cat-file", "blob", &blob]).await.ok(),
            ));
        }
        let rules = review::base_rules(&request.base_branch, &rules);
        drop(prepared);

        // 3. The request, as data, in the worktree.
        // The worktree holds what the other Obi's branch commits: a `.obi` or `.obi/inputs` that
        // is a link out of it fails the job, and nothing is written (crate::obi_dir).
        let inputs = crate::obi_dir::folder(&worktree, "inputs")
            .map_err(|e| format!("obid can't make .obi/inputs: {e}"))?;
        let text = serde_json::to_vec_pretty(&quoted).map_err(|e| e.to_string())?;
        let file = Path::new(review::REQUEST_FILE)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("the request file has a name");
        crate::obi_dir::write_new(&inputs, file, &crate::vault::for_agent_bytes(&text)?)
            .map_err(|e| format!("obid can't write the request: {e}"))?;
        let file = Path::new(review::DIFF_FILE)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("the diff file has a name");
        crate::obi_dir::write_new(&inputs, file, &crate::vault::for_agent_bytes(&change)?)
            .map_err(|e| format!("obid can't write the diff: {e}"))?;
        crate::obi_dir::write_new(
            &inputs,
            review::RULES_FILE,
            &crate::vault::for_agent_bytes(rules.as_bytes())?,
        )
        .map_err(|e| format!("obid can't quote the base's rules: {e}"))?;

        // 4. The agent, with a fixed prompt, in the sandbox.
        let (admin, common) = self.own_git_dirs(id).await?;
        let temp = self.temp_path(id);
        let (tool, account) = self.job_account(&job)?;
        let spec = self.spec(&worktree, &admin, &common, &temp, false, tool);
        let prompt = review::prompt(&request);
        let flags: Vec<OsString> = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-prompt-tool",
            "stdio",
            "--permission-mode",
            "default",
            "--setting-sources",
            "",
            "--strict-mcp-config",
            // No CLAUDE.md is loaded as instructions: the change's own is data (`--setting-sources
            // ""` keeps settings out, not instructions), and the base's rules are quoted.
            "--restricted",
            "--json-schema",
            review::OUTPUT_SCHEMA,
        ]
        .iter()
        .map(OsString::from)
        .collect();
        // Only the tools obid allows a review, given at the start: Claude Code doesn't ask before
        // every call (it runs read-only shell commands and reads in its working folder without
        // asking), so obid's answers alone can't keep a tool from running. `--tools` takes the
        // arguments after it up to the next option, so it goes last, with one value.
        let mut flags = flags;
        flags.extend([
            OsString::from("--tools"),
            OsString::from(crate::policy::REVIEW_TOOLS.join(",")),
        ]);
        if tool == Tool::Claude {
            self.claude_login_ready(Some(&account)).await?;
        }
        if tool == Tool::Codex {
            Box::pin(self.chatgpt_ready(id, &account)).await?;
        }
        let job = self.get(id).unwrap_or(job);
        let launch = self.launch(&job, &spec, tool, flags, &account, None)?;
        if *signal.borrow() != Signal::Run {
            return Ok(());
        }
        self.update(id, |job| {
            if job.status == Status::Queued {
                job.status = Status::Running;
            }
            job.started.get_or_insert_with(obi_protocol::time::now);
            job.phase = Some(Phase::Work);
            job.step = "Reviewing".into();
        });
        // A review another Obi asked for only reads: no MCP server.
        let turn = self
            .turn(
                id,
                tool,
                launch,
                &prompt,
                signal,
                Gate::ObiReview,
                ROUND_LIMIT.into(),
                None,
            )
            .await?;
        if *signal.borrow() != Signal::Run {
            return Ok(());
        }
        if turn.timed_out {
            return Err("The review ran past 45 minutes, the most a round may take.".into());
        }
        if !turn.succeeded() {
            // Masked as the stderr note is (#1648): a CLI's error can carry a token.
            let first = crate::detail::mask(turn.error().lines().next().unwrap_or("").trim());
            let first = first.trim();
            return Err(if first.is_empty() {
                format!(
                    "Claude Code stopped without finishing the review ({}).{}",
                    turn.exit,
                    self.agent_note(id)
                )
            } else {
                format!("Claude Code couldn't finish the review: {first}")
            });
        }
        let folders = git_folders(&admin, &common, &worktree);
        let head = git::pinned(&worktree, &folders, &["rev-parse", "HEAD"])
            .await
            .map_err(|e| e.to_string())?;
        if head != request.head_sha {
            return Err("The review made a commit: a review only reads.".into());
        }

        // 5. Its review.
        let output = review::parse_output(
            turn.result
                .as_ref()
                .expect("a turn that succeeded has its result"),
        )?;

        // 6. The check, in the same sandbox, with no network.
        let tests_pass = match &job.check {
            Some(command) => {
                self.update(id, |job| {
                    job.phase = Some(Phase::Check);
                    job.step = format!("Running {command}");
                });
                let run = self.check(id, command, &head, &spec, signal).await?;
                if *signal.borrow() != Signal::Run {
                    return Ok(());
                }
                run.passed
            }
            None => false,
        };

        // 7. The result; 8. the publish gate and the reply, the exchange's, before the job ends.
        let body = review::result(&request, &output, tests_pass);
        let findings = body["findings"].as_array().map_or(0, Vec::len);
        let outcome = body["outcome"].as_str().unwrap_or("").replace('_', " ");
        let approvable = tests_pass && !review::any_blocker(&output);
        self.update(id, |job| {
            job.review_result = Some(body.clone());
            job.approvable = approvable;
            job.step = "Scanning the result for secrets and sending it".into();
        });
        self.review_ended(id, true).await;
        self.update(id, |job| {
            job.status = Status::Done;
            job.step = String::new();
            job.commits = Some(0);
            job.summary = format!(
                "Review for {}: {outcome}, {findings} finding(s), {}.",
                job.obi
                    .as_ref()
                    .map(|o| o.from_owner.as_str())
                    .unwrap_or("another Oby"),
                match (&job.check, tests_pass) {
                    (None, _) => "no check to run",
                    (Some(_), true) => "tests pass",
                    (Some(_), false) => "tests fail",
                }
            );
            job.review_result = Some(body);
            job.finished = Some(obi_protocol::time::now());
        });
        Ok(())
    }
}
