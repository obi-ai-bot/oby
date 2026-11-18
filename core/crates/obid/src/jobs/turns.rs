//! Launching a job's agent and running its turns: the session, its stalls ([`Stall`]), its
//! transcript and what its stderr said. It never pushes: publishing is jobs/publishing.rs's.

use super::*;

/// How often the words being written and a running command's lines go out, when they changed
/// (LIVETASK-32, 34): about as often as T3 Code batches its deltas.
const STREAM_EVERY: Duration = Duration::from_millis(100);

impl Jobs {
    /// Runs a job to its end, unless it is stopped or obid shuts down.
    pub(super) async fn work(self: &Arc<Self>, id: &str, signal: watch::Receiver<Signal>) {
        let Some(job) = self.get(id) else { return };
        if job.status != Status::Queued {
            return;
        }
        // Running again: the limit it waited on is no more the owner's to answer.
        if let Some(link) = self.link.get().and_then(std::sync::Weak::upgrade) {
            link.limit_items(id, None);
        }
        // Queued while its worktree gets ready, as the Swift runner has it: running once its agent
        // starts.
        self.update(id, |job| {
            job.waiting = None;
            // The owner's words show until the agent moves on.
            if job.heard.is_empty() {
                job.step = format!("Getting {} ready", job.repo_name());
            }
        });
        let temp = self.temp_path(id);
        let outcome = match prepare_folder(&temp).and_then(|()| {
            prepare_folder(self.log_path(id, "x").parent().expect("a log has a folder"))
        }) {
            Err(e) => Err(End::Failed(format!(
                "obid couldn't make the job's folders: {e}"
            ))),
            Ok(()) if job.obi.is_some() => Box::pin(self.run_obi_review(id, &signal))
                .await
                .map_err(End::Failed),
            Ok(()) => {
                let outcome = Box::pin(self.run_owner_job(id, &signal)).await;
                // Waiting for the owner's answer isn't an end either: kept as a shutdown keeps it.
                let paused =
                    *signal.borrow() == Signal::Shutdown || matches!(outcome, Err(End::Asked(_)));
                // A job that ends any other way than done (failed, stopped) shows what it made
                // too; done published them before it ended. A shutdown isn't an end.
                if !paused {
                    self.publish_artifacts(id).await;
                }
                // Ended without commits to publish, what it left uncommitted is kept as a `wip:`
                // commit (#606). A shutdown isn't an end, and a review keeps nothing.
                let keeps_none = self.get(id).is_some_and(|j| {
                    j.kind == "review" || j.status == Status::Done && j.commits.unwrap_or(0) > 0
                });
                // What it committed itself ends here, before obid's `wip:` commit, which never
                // counts: counted once keep_commits has its objects in the clone.
                let worktree = self.worktree_path(id);
                let own_head = match self.pinned_git(id).await {
                    Ok(folders) => git::pinned(
                        &worktree,
                        &folders,
                        &["rev-parse", "--verify", "--quiet", "HEAD"],
                    )
                    .await
                    .unwrap_or_default(),
                    Err(_) => String::new(),
                };
                if !paused
                    && !keeps_none
                    && let Err(e) = self.keep_wip(id).await
                {
                    tracing::info!("job {id}: its uncommitted work stays in its worktree: {e}");
                }
                // Whatever ended it (a limit, a stop, a failure), what it committed is on its
                // branch, and counted, so a follow-up starts from them and Dismiss warns (#757).
                // Done counted its own.
                match self.keep_commits(id).await {
                    Ok(_) => {
                        let made = match self.get(id) {
                            Some(job) => {
                                let folders = self.pinned_git(id).await.unwrap_or_default();
                                self.counted(&job, &worktree, &folders, &own_head).await
                            }
                            None => 0,
                        };
                        if made > 0 {
                            self.update(id, |job| {
                                if job.status != Status::Done {
                                    job.commits = Some(made);
                                }
                            });
                        }
                    }
                    Err(e) => tracing::info!("job {id}: its commits stay in its worktree: {e}"),
                }
                // A stalled job's note says where its branch is: its own last commit, under obid's
                // `wip:` one when there is one.
                self.update(id, |job| {
                    let commits = job.commits.unwrap_or(0);
                    let mine = job.start_commit.as_deref() != Some(own_head.as_str());
                    if let Some(stalled) = job.stalled.as_mut() {
                        stalled.commits = commits;
                        stalled.commit = (!own_head.is_empty() && mine).then(|| own_head.clone());
                    }
                });
                outcome
            }
        };
        // An owner's job's HOME keeps the package caches its agent filled until the worktree is
        // cleaned up, for the next run's offline check: only the MCP config, with its broker's
        // token, goes now. Another Obi's review keeps nothing.
        if job.obi.is_some() {
            let _ = std::fs::remove_dir_all(&temp);
        } else {
            let _ = std::fs::remove_file(temp.join(MCP_CONFIG_FILE));
        }
        forget_login_secrets(&self.login_path(id));
        let asked = matches!(outcome, Err(End::Asked(_)));
        self.drop_asks(id, *signal.borrow() == Signal::Shutdown, asked);
        // It ended (a shutdown isn't an end): an old crash never counts against a later run.
        if *signal.borrow() != Signal::Shutdown
            && self.get(id).is_some_and(|j| !j.crash_resumes.is_empty())
        {
            self.update(id, |job| job.crash_resumes.clear());
        }
        let stopped = *signal.borrow() != Signal::Run;
        match outcome {
            // The owner stopped it: its exchange gets a failed result. A shutdown leaves it be.
            _ if stopped => {
                if *signal.borrow() == Signal::Stop {
                    self.review_ended(id, false).await;
                }
            }
            Ok(()) => {}
            // Last, once nothing of its run can take the question down again.
            Err(End::Asked(waits)) => self.wait_for_answer(id, waits),
            Err(end) => {
                let review = self.get(id).and_then(|j| {
                    let body: ReviewRequest =
                        serde_json::from_value(j.obi_request?.get("body")?.clone()).ok()?;
                    Some(review::failed_result(&body))
                });
                if job.obi.is_some() {
                    self.update(id, |job| job.review_result = review.clone());
                    self.review_ended(id, false).await;
                }
                // A limit's job moves to an account with room by itself; with none, it waits for
                // one, asking no one (LIMITS-15).
                let failover = match &end {
                    End::AccountLimit { .. } => self.failover_for(id).await,
                    _ => None,
                };
                let resume_at = match &end {
                    End::AccountLimit { reset, .. } if failover.is_none() => {
                        Some(self.limit_wait_until(&job.account, *reset, &Work::of(&job)))
                    }
                    _ => None,
                };
                if let End::AccountLimit { reason, .. } = &end {
                    self.update(id, |job| match (&failover, resume_at) {
                        (Some(failover), _) if job.status.is_active() => failover.apply(job, self),
                        (_, Some(at)) => account_limits::start_wait(job, at),
                        // It moved or waits (LIMITS-15), or Stop came as it ended (LIMITS-6).
                        _ => tracing::info!("job {id}: {reason}"),
                    });
                } else {
                    use crate::job::Command;
                    let at = obi_protocol::time::now();
                    self.apply(id, Command::RunEnded(end.ended(review, at)));
                }
                // The `stalled` event (docs/runner-api.md): the job stays on its own card.
                if let Some(job) = self.get(id).filter(|j| j.stalled.is_some()) {
                    self.events.send("stalled", job.to_json());
                }
            }
        }
        // Oby's review of published work ended: its verdict goes to the job it read. A shutdown
        // isn't an end: it runs again once obid starts, nor is a question it waits on.
        if *signal.borrow() != Signal::Shutdown && !asked {
            self.reviewed(id);
        }
    }

    /// Everything a sandboxed process of this job may and may not touch.
    pub(super) fn spec(
        &self,
        worktree: &Path,
        admin: &Path,
        common: &Path,
        temp: &Path,
        commits: bool,
        tool: Tool,
    ) -> Spec {
        let mut hidden = sandbox::job_hidden(&self.config.home, person_homes());
        hidden.extend(self.config.agents.hidden_folders(&self.config.home));
        let mut read_only = sandbox::tool_roots(
            &process::tools_path(),
            &hidden,
            std::slice::from_ref(&self.config.home),
        );
        read_only.extend(process::rust_reads(
            &hidden,
            std::slice::from_ref(&self.config.home),
        ));
        if let Some(bin) = self.agent_bin(tool) {
            read_only.extend(sandbox::install_root(
                &bin,
                &hidden,
                std::slice::from_ref(&self.config.home),
            ));
            read_only.push(bin);
        }
        Spec {
            worktree: worktree.to_path_buf(),
            git_admin: Some(admin.to_path_buf()),
            git_common: Some(common.to_path_buf()),
            commits,
            temp: temp.to_path_buf(),
            accounts: Vec::new(),
            read_only,
            hidden,
            // An agent keeps the network any agent has (its model's API); the check gets none.
            network: true,
            loopback: false,
            proxied: false,
            frozen: Vec::new(),
            fixed: Vec::new(),
        }
    }

    /// Where a run of job `id` in `spec`'s worktree records its process group (crate::groups).
    pub(super) fn recording(&self, id: &str, spec: &Spec) -> crate::groups::Recording {
        crate::groups::Recording {
            records: crate::groups::dir(&self.config.home),
            job: id.to_owned(),
            git_admin: spec.git_admin.clone(),
        }
    }

    pub(crate) fn agent_bin(&self, tool: Tool) -> Option<PathBuf> {
        self.agent_config(tool)
            .and_then(|agent| process::resolve_bin(&agent.bin))
    }

    /// The agent's command line and environment for one turn of `tool` on `account`. `flags`
    /// follow Claude Code's binary; Codex and Cursor take their adapters' own.
    pub(super) fn launch(
        &self,
        job: &Job,
        spec: &Spec,
        tool: Tool,
        flags: Vec<OsString>,
        account: &crate::config::Account,
        broker: Option<&(String, String)>,
    ) -> Result<Launch, String> {
        if self.being_changed(&account.id) {
            return Err(format!(
                "{} is being signed in or removed, so {} didn't start.",
                account.name,
                tool.name()
            ));
        }
        let sandbox = self
            .sandbox
            .as_ref()
            .map_err(|why| format!("obid can't run this job's agent: {why}"))?;
        let bin = self.agent_bin(tool).ok_or_else(|| {
            let named = self
                .agent_config(tool)
                .map(|c| c.bin.clone())
                .unwrap_or_default();
            format!("obid can't find {}'s binary ({named}).", tool.name())
        })?;
        let mut spec = spec.clone();
        let mut extra = spec.git_env();
        extra.extend(process::no_publish_env(&spec.temp));
        let chatgpt = tool == Tool::Codex && crate::chatgpt::marker_of(account).is_some();
        let own = lanes::launches_own(job, tool, account);
        // A boxed run's only way off the Mac: obid's proxy, which lets its tool's own services
        // through and decides every other site (crate::egress, #616). An own run's network is its
        // CLI's sandbox's (LANES-6).
        if !own && sandbox.confines_egress() {
            let egress = self.egress.get().ok_or_else(|| {
                format!(
                    "obid's egress proxy isn't running, so {} didn't start.",
                    tool.name()
                )
            })?;
            extra.extend(crate::egress::env(&egress.open(&job.id)));
            spec.proxied = true;
        }
        extra.extend(process::show_rust(&spec.temp));
        // Its builds take what memory leaves them next to the other jobs (crate::meter).
        extra.extend(self.build_env(&job.id, false));
        let tetrix = self.tetrix_mcp(job);
        // On a ChatGPT login, no run holds a token: Codex goes through obid's relay, with the job's
        // broker token (crate::chatgpt_relay).
        let relay = chatgpt
            .then(|| {
                if let Some(why) = login_problem(tool, account, login_home().as_deref()) {
                    return Err(why);
                }
                if !signed_in(tool, account, login_home().as_deref()) {
                    return Err(crate::chatgpt::SIGN_IN_AGAIN.to_owned());
                }
                self.connections
                    .get()
                    .map(|connections| connections.open_chatgpt(&job.id))
                    .ok_or_else(|| "obid's relay isn't running, so Codex didn't start.".to_owned())
            })
            .transpose()?;
        if tool == Tool::Codex
            && let Some((_, token)) = broker.or(tetrix.as_ref()).or(relay.as_ref())
        {
            // A secret obid knows since it made it (crate::connections::Connections::job_token).
            extra.push(("OBY_MCP_TOKEN", token.into()));
        }
        if relay.is_some()
            && let Some(chatgpt) = self.chatgpt.get()
        {
            chatgpt.runs.begin(&job.id);
        }
        self.launched(&job.id, own)?;
        // The owner's environment under obid's (LANES-4): a shell that doesn't answer leaves the run
        // obid's allowlist, and the job says so.
        let owners = if own {
            process::login_env().map(|env| owners_for(tool, env))
        } else {
            Some(Vec::new())
        };
        if owners.is_none() {
            self.note_change(&job.id, "Your login shell didn't answer within 10 seconds, so this run has none of its environment.".into());
        }
        // An own Claude Code loads the owner's MCP servers: its first turn waits for none of them,
        // only for obid's own, which are `alwaysLoad` (LANES-24).
        if own && tool == Tool::Claude {
            let (name, value) = crate::agent::MCP_STARTUP_WAIT;
            extra.push((name, value.into()));
        }
        match (&account.config_dir, tool) {
            // In the CLI's own sandbox, with the account's own folder and login: no copy.
            _ if own => extra.extend(own_run_env(
                tool,
                account,
                &spec,
                &self.login_path(&job.id),
            )?),
            // Never the account's own folder (#655): the job's own, a login copy and nothing else.
            (Some(dir), _) => {
                let login = self.login_path(&job.id);
                copy_account_folder(tool, dir, &login).map_err(|e| {
                    format!("obid can't copy {}'s login for this job: {e}", tool.name())
                })?;
                match tool {
                    Tool::Claude => {
                        extra.push(("CLAUDE_CONFIG_DIR", login.clone().into_os_string()))
                    }
                    Tool::Codex => {
                        // A job moved here from a device login keeps no copy of that login.
                        if relay.is_some() {
                            let _ = std::fs::remove_file(login.join("auth.json"));
                        }
                        codex_copy_ready(&login.join("auth.json"), &account.name)?;
                        extra.push(("CODEX_HOME", login.clone().into_os_string()))
                    }
                    Tool::Cursor => {
                        acp::cursor::warn_if_long(&login);
                        extra.extend(acp::cursor::environment(&login));
                    }
                }
                spec.accounts.push(login);
            }
            // Cursor's own login (`~/.cursor`), copied as an account's is: never the person's folder.
            (None, Tool::Cursor) => {
                let login = self.login_path(&job.id);
                copy_own_cursor(&login)?;
                acp::cursor::warn_if_long(&login);
                extra.extend(acp::cursor::environment(&login));
                spec.accounts.push(login);
            }
            (None, _) => {
                // The tool's own login, without the person's real folders: obid copies the login
                // (and only that) into a folder of the job's own, which the agent sees as its
                // ~/.claude and ~/.claude.json (or ~/.codex) through its temporary HOME. What the
                // agent writes there (settings, hooks, MCP servers, a refreshed token) stays there:
                // the person's own ~/.claude, ~/.claude.json and ~/.codex are never writable, so
                // nothing a job runs can plant something the tool runs later outside the sandbox.
                let login = self.login_path(&job.id);
                let names =
                    copy_login(login_home().as_deref(), &login, tool, true).map_err(|e| {
                        format!("obid can't copy {}'s login for this job: {e}", tool.name())
                    })?;
                if tool == Tool::Codex {
                    codex_copy_ready(&login.join(".codex/auth.json"), &account.name)?;
                }
                for name in names {
                    let link = spec.temp.join(name);
                    let _ = std::fs::remove_file(&link);
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(login.join(name), &link)
                        .map_err(|e| format!("obid can't show the job its login: {e}"))?;
                }
                spec.accounts.push(login);
            }
        }
        if COMMITTING.contains(&job.kind.as_str()) || job.obi.is_none() {
            let (name, email) = &self.identity();
            for key in ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"] {
                extra.push((key, name.into()));
            }
            for key in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] {
                extra.push((key, email.into()));
            }
        }
        let mut argv = match tool {
            Tool::Claude => {
                let mut argv: Vec<OsString> = vec![bin.into_os_string()];
                argv.extend(flags);
                if own {
                    let login = self.login_path(&job.id);
                    let secrets = self.secret_places();
                    let settings = own_sandbox_settings(&spec, &login, &extra, &secrets)?;
                    argv.extend(["--tools", OWN_JOB_TOOLS, "--settings"].map(OsString::from));
                    argv.push(settings.into());
                }
                argv
            }
            Tool::Codex => {
                let mut argv = codex::argv(
                    bin,
                    broker.map(|(url, token)| (url.as_str(), token.as_str())),
                    tetrix.as_ref().map(|(url, _)| url.as_str()),
                );
                // No plug-in sync: at every start Codex clones its curated marketplace from
                // github.com, which the egress proxy would ask the owner about (#616), and a job's
                // `CODEX_HOME` has no plug-in in it anyway.
                argv.splice(1..1, ["-c".into(), "features.plugins=false".into()]);
                // Codex itself needs it (its MCP servers, the relay); its commands don't, and real
                // Codex hands them all of its environment, this included (measured 2026-11-03).
                if extra.iter().any(|(name, _)| *name == "OBY_MCP_TOKEN") {
                    argv.splice(
                        1..1,
                        [
                            "-c".into(),
                            r#"shell_environment_policy.exclude=["OBY_MCP_TOKEN"]"#.into(),
                        ],
                    );
                }
                if let Some((base, _)) = &relay {
                    argv.splice(1..1, crate::chatgpt_relay::codex_settings(base));
                }
                if own {
                    argv.splice(1..1, own_codex_settings(&spec)?);
                    // Under `never` Codex asks nobody: Oby's own tools are approved (LANES-21).
                    if broker.is_some() {
                        argv.splice(1..1, codex::OWN_OBY_TOOLS.map(OsString::from));
                    }
                }
                argv
            }
            Tool::Cursor => acp::cursor::argv(bin),
        };
        if job.review_of.is_some() {
            without_project_instructions(tool, &mut argv, &mut extra);
        }
        Ok(Launch {
            own_sandbox: own,
            group: Some(self.recording(&job.id, &spec)),
            argv: if own {
                sandbox.unboxed(&argv)
            } else {
                sandbox.wrap(&spec, &argv)
            },
            env: {
                if tool == Tool::Claude {
                    extra.push((process::CLAUDE_TMPDIR, spec.temp.clone().into_os_string()));
                    extra.push((process::CLAUDE_TODO_TOOLS, "1".into()));
                    // Claude Code's documented switches: no Datadog telemetry, no Sentry reports,
                    // no self-update, in both lanes. Through the egress proxy they were a site like
                    // any other, a card for the owner and a foreign mark (PERMISSIONS-2).
                    // ponytail: Codex and Cursor document no environment switch for theirs (Codex's
                    // is `config.toml`'s `[analytics]`, `[otel]` and `check_for_update_on_startup`,
                    // its OTel off by default), so what they send is still decided as any site is.
                    extra.extend(process::CLAUDE_RUN_SWITCHES.map(|name| (name, "1".into())));
                }
                let mut env = process::sandbox_env(&process::tools_path(), &spec.temp, &extra);
                process::with_owners(&mut env, &owners.unwrap_or_default());
                env
            },
            cwd: spec.worktree.clone(),
            stderr: self.log_path(&job.id, "agent.log"),
        })
    }

    /// The account the brain thinks with: the first of [`Jobs::thinking_accounts`], else the
    /// tool's first [`signed_in`] one, else its first.
    fn brain_account(&self, tool: Tool) -> Option<crate::config::Account> {
        self.thinking_accounts(tool)
            .into_iter()
            .next()
            .or_else(|| self.signed_in_accounts(tool).into_iter().next())
            .or_else(|| self.accounts(tool).into_iter().next())
    }

    /// Before a boxed Claude Code run on `account` (none: the brain's), its Keychain login is
    /// fresh ([`keychain_ready`]). Without a binary or an account it's the launch that says so.
    /// An own run doesn't ask: Claude Code reads and refreshes its own login (LANES-3).
    pub async fn claude_login_ready(
        &self,
        account: Option<&crate::config::Account>,
    ) -> Result<(), String> {
        let account = match account {
            Some(account) => account.clone(),
            None => match self.brain_account(Tool::Claude) {
                Some(account) => account,
                None => return Ok(()),
            },
        };
        if !keychain_on() {
            return file_login_ready(&account);
        }
        match self.agent_bin(Tool::Claude) {
            Some(bin) => keychain_ready(bin, &account, &self.config.home).await,
            None => Ok(()),
        }
    }

    /// The brain's command line when it may read its quoted file (#65): boxed like a job's agent,
    /// so all it sees is `dir` (its working directory, `work.md` in it) and its account's login.
    /// `flags` follow Claude Code's binary; Codex runs `app-server`. The brain thinks with the
    /// tool's first [`signed_in`] account, else its first. Throws why it can't run.
    pub fn brain_launch(
        &self,
        tool: Tool,
        dir: &Path,
        flags: Vec<OsString>,
    ) -> Result<Launch, String> {
        let account = self
            .brain_account(tool)
            .ok_or_else(|| format!("{} has no account.", tool.name()))?;
        let temp = dir.join("home");
        store::make_private_dir(&temp)
            .map_err(|e| format!("obid can't make the brain's folder: {e}"))?;
        self.boxed_launch(Boxed {
            tool,
            account: &account,
            dir,
            temp: &temp,
            login: &dir.join("login"),
            frozen: Vec::new(),
            flags,
            stderr: dir.join("stderr.log"),
            confined: false,
            door: None,
        })
    }

    /// A conversation's agent session (crate::brain::harness), `tool` (Claude Code or Codex) boxed
    /// like a job's agent: it
    /// writes only `room` (its working folder), but not `frozen` in it (the repositories'
    /// checkouts), `temp` (its `HOME` and `TMPDIR`) and `login`, its own copy of the account's
    /// login (its `CLAUDE_CONFIG_DIR` for an account with a folder of its own, which it never
    /// sees); it reads those, its tools and what the system needs, and nothing else of the
    /// machine's: not obid's home, the person's, `/tmp` or other people's folders. Throws why it
    /// can't run.
    #[allow(clippy::too_many_arguments)]
    pub fn conversation_launch(
        &self,
        tool: Tool,
        account: &crate::config::Account,
        room: &Path,
        temp: &Path,
        login: &Path,
        frozen: Vec<PathBuf>,
        flags: Vec<OsString>,
        stderr: PathBuf,
    ) -> Result<Launch, String> {
        self.boxed_launch(Boxed {
            tool,
            account,
            dir: room,
            temp,
            login,
            frozen,
            flags,
            stderr,
            confined: true,
            door: None,
        })
    }

    /// A conversation's Cursor (crate::brain::harness_cursor), boxed as [`Jobs::conversation_launch`]
    /// boxes the others, and its only way off the Mac obid's egress proxy, by the door `door`
    /// ([`Jobs::reach_now`]): Cursor's own reads and edits never ask, so what it read must have
    /// nowhere to go. Throws why it can't run: where the box can't hold it to the proxy, it doesn't.
    #[allow(clippy::too_many_arguments)]
    pub fn conversation_cursor_launch(
        &self,
        account: &crate::config::Account,
        room: &Path,
        temp: &Path,
        login: &Path,
        frozen: Vec<PathBuf>,
        stderr: PathBuf,
        door: &str,
    ) -> Result<Launch, String> {
        self.boxed_launch(Boxed {
            tool: Tool::Cursor,
            account,
            dir: room,
            temp,
            login,
            frozen,
            flags: Vec::new(),
            stderr,
            confined: true,
            door: Some(door),
        })
    }

    /// A conversation's Cursor's turn ended: its door lets nothing through any more.
    pub(crate) fn close_door(&self, door: &str) {
        if let Some(egress) = self.egress.get() {
            egress.close(door);
        }
    }

    /// What [`Jobs::brain_launch`] and [`Jobs::conversation_launch`] run: `tool` in the job
    /// sandbox, working in `dir`, with its account's login.
    pub(super) fn boxed_launch(&self, boxed: Boxed<'_>) -> Result<Launch, String> {
        let Boxed {
            tool,
            account,
            dir,
            temp,
            login,
            frozen,
            flags,
            stderr,
            confined,
            door,
        } = boxed;
        let sandbox = self
            .sandbox
            .as_ref()
            .map_err(|why| format!("obid can't box {}: {why}", tool.name()))?;
        let bin = self
            .agent_bin(tool)
            .ok_or_else(|| format!("obid can't find {}'s binary.", tool.name()))?;
        let temp = temp.to_path_buf();
        let mut hidden = vec![self.config.home.clone()];
        hidden.extend(person_homes());
        hidden.extend(self.config.agents.hidden_folders(&self.config.home));
        let mut read_only = sandbox::tool_roots(
            &process::tools_path(),
            &hidden,
            std::slice::from_ref(&self.config.home),
        );
        read_only.extend(sandbox::install_root(
            &bin,
            &hidden,
            std::slice::from_ref(&self.config.home),
        ));
        read_only.push(bin.clone());
        if confined {
            read_only.extend(sandbox::path_software(&process::tools_path(), &hidden));
        }
        let mut spec = Spec {
            worktree: dir.to_path_buf(),
            git_admin: None,
            git_common: None,
            commits: false,
            temp: temp.clone(),
            accounts: Vec::new(),
            read_only,
            hidden,
            network: true,
            loopback: false,
            proxied: false,
            frozen,
            fixed: Vec::new(),
        };
        let mut extra = process::no_publish_env(&temp);
        match &account.config_dir {
            // Its own folder with a copy of the login, never the account's, and the proxy.
            _ if confined && tool == Tool::Cursor => {
                let egress = self
                    .egress
                    .get()
                    .filter(|_| sandbox.confines_egress())
                    .zip(door)
                    .ok_or("obid can't hold Cursor to its egress proxy here, so a conversation doesn't run on it")?;
                extra.extend(crate::egress::env(&egress.0.open(egress.1)));
                spec.proxied = true;
                match &account.config_dir {
                    Some(from) => copy_account_folder(tool, from, login)
                        .map_err(|e| format!("obid can't copy Cursor's login: {e}"))?,
                    None => copy_own_cursor(login)?,
                }
                acp::cursor::warn_if_long(login);
                extra.extend(acp::cursor::environment(login));
                spec.accounts.push(login.to_path_buf());
            }
            // A conversation never sees the account's own folder (every job's and conversation's
            // transcripts are in it): `login` is its own, with a copy of the login and nothing else.
            // Codex's is its `CODEX_HOME` for any account: none of the person's `~/.codex` either
            // (their config.toml, MCP servers, instructions).
            _ if confined && tool == Tool::Codex => {
                let from = account
                    .config_dir
                    .clone()
                    .or_else(|| login_home().map(|home| home.join(".codex")));
                if let Some(from) = from {
                    copy_account_login(tool, &from, login)
                        .map_err(|e| format!("obid can't copy Codex's login: {e}"))?;
                }
                codex_copy_ready(&login.join("auth.json"), &account.name)?;
                spec.accounts.push(login.to_path_buf());
                extra.push(("CODEX_HOME", login.as_os_str().to_owned()));
            }
            // Never the account's own folder (#655): a copy of its login, nothing else of it.
            Some(shared) => {
                copy_account_login(tool, shared, login)
                    .map_err(|e| format!("obid can't copy {}'s login: {e}", tool.name()))?;
                spec.accounts.push(login.to_path_buf());
                if tool == Tool::Codex {
                    codex_copy_ready(&login.join("auth.json"), &account.name)?;
                    extra.push(("CODEX_HOME", login.as_os_str().to_owned()));
                } else {
                    extra.push(("CLAUDE_CONFIG_DIR", login.as_os_str().to_owned()));
                }
            }
            None => {
                // The tool's own login, copied into a folder of its own as a job's is.
                let names = copy_login(login_home().as_deref(), login, tool, true)
                    .map_err(|e| format!("obid can't copy {}'s login: {e}", tool.name()))?;
                if tool == Tool::Codex {
                    codex_copy_ready(&login.join(".codex/auth.json"), &account.name)?;
                }
                for name in names {
                    let link = temp.join(name);
                    let _ = std::fs::remove_file(&link);
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(login.join(name), &link)
                        .map_err(|e| format!("obid can't show {} its login: {e}", tool.name()))?;
                }
                spec.accounts.push(login.to_path_buf());
            }
        }
        let argv: Vec<OsString> = match tool {
            // The brain never gets the connected-app broker; a conversation's `-c` settings come
            // before `app-server`.
            Tool::Codex => std::iter::once(bin.into_os_string())
                .chain(flags)
                .chain(["app-server".into()])
                .collect(),
            Tool::Cursor => acp::cursor::argv(bin),
            _ => std::iter::once(bin.into_os_string()).chain(flags).collect(),
        };
        let argv = if confined {
            sandbox.wrap_reading_only_its_own(&spec, &argv)
        } else {
            sandbox.wrap(&spec, &argv)
        };
        Ok(Launch {
            own_sandbox: false,
            group: None,
            argv,
            env: {
                if tool == Tool::Claude {
                    extra.push((process::CLAUDE_TMPDIR, spec.temp.clone().into_os_string()));
                }
                process::sandbox_env(&process::tools_path(), &spec.temp, &extra)
            },
            cwd: dir.to_path_buf(),
            stderr,
        })
    }

    /// Runs one turn of the job's tool and follows it on the job: its session and steps. An owner's
    /// job's turn that makes the same failing call [`LOOP_REPEATS`] times in a row is ended, and
    /// comes back `looped`.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn turn(
        &self,
        id: &str,
        tool: Tool,
        launch: Launch,
        prompt: &str,
        signal: &watch::Receiver<Signal>,
        gate: Gate<'_>,
        limit: crate::conn::Limit,
        ran_secs: Option<&(dyn Fn() -> u64 + Sync)>,
    ) -> Result<agent::Turn, String> {
        // What an owner's job's turn commits goes in obid's record of what it made, only while
        // its check would get values (crate::secretspec).
        let records = matches!(gate, Gate::Owner { .. })
            && self
                .get(id)
                .is_some_and(|job| job.someone_elses().is_none());
        // LANES-8: an own run's refs are its own; obid reads the clone's before and after it.
        let own_run = self.own_turn(id, launch.own_sandbox).await?;
        let before = match records {
            true => Box::pin(self.sandboxed_git(id, &["rev-parse", "--verify", "HEAD"], None))
                .await
                .ok(),
            false => None,
        };
        let started = obi_protocol::time::now();
        // Boxed: the turn's future is large, and one more level of it on the stack overflowed it
        // (a guest's turn, crate::sphere::guest).
        let turn =
            Box::pin(self.unrecorded_turn(id, tool, launch, prompt, signal, gate, limit, ran_secs))
                .await;
        if let Some(run) = &own_run {
            run.settle().await?;
        }
        if let Some(before) = before {
            Box::pin(self.record_turn(id, String::from_utf8_lossy(&before).trim(), started)).await;
        }
        turn
    }

    /// [`Jobs::turn`], without the record.
    #[allow(clippy::too_many_arguments)]
    async fn unrecorded_turn(
        &self,
        id: &str,
        tool: Tool,
        launch: Launch,
        prompt: &str,
        signal: &watch::Receiver<Signal>,
        gate: Gate<'_>,
        limit: crate::conn::Limit,
        ran_secs: Option<&(dyn Fn() -> u64 + Sync)>,
    ) -> Result<agent::Turn, String> {
        let prompt = &crate::vault::for_agent(prompt)?;
        let (obi_review, mcp_rules, parks, session_only) = match gate {
            Gate::ObiReview => (true, &[][..], false, false),
            Gate::Owner {
                mcp,
                park,
                session_only,
            } => (false, mcp, park, session_only),
        };
        let worktree = launch.cwd.clone();
        // Where Claude Code keeps what a running command prints (crate::spill, LIVETASK-34).
        let spill_temp = launch
            .env
            .iter()
            .rev()
            .find(|(name, _)| name == process::CLAUDE_TMPDIR)
            .map(|(_, temp)| std::path::PathBuf::from(temp));
        let job = self.get(id).ok_or("the job is gone")?;
        let jobs = self;
        // In Claude Code's own sandbox its tools run unboxed (LANES-14, `own_reach`).
        let own = launch.own_sandbox;
        // obid's fixed rules first, in every mode; then, for the owner's own jobs, the job's mode
        // and the repository's saved rules, which may ask the owner. A review another Obi asked
        // for only reads, and never asks.
        let decide = |name: &str, input: &Value| -> Ruling {
            let outside = own
                .then(|| jobs.own_reach(id, name, input, &worktree))
                .flatten();
            if let Some(Answer::Allow) = outside {
                return Ruling::Now(Answer::Allow);
            }
            match outside.unwrap_or_else(|| {
                // The job's mode as it is now: the owner may change it mid-turn (BYPASS-8). Its
                // background helpers are waited for as its background commands are
                // (`background_tasks_changed`, crate::agent).
                let bypass = jobs
                    .get(id)
                    .is_some_and(|j| Self::mode_of(&j) == Mode::Bypass);
                policy::decide_as(bypass, name, input, &worktree, obi_review, mcp_rules)
            }) {
                Answer::Deny(why) => {
                    if !obi_review {
                        jobs.answered(id, &Call::new(name, input), &worktree, Some(&why));
                    }
                    Ruling::Now(Answer::Deny(why))
                }
                Answer::Allow | Answer::AllowWith(_) if obi_review => Ruling::Now(Answer::Allow),
                Answer::Allow | Answer::AllowWith(_) => jobs.rule_on(id, name, input, &worktree),
            }
        };
        // The turn's own stop: the job's, and a loop's (an owner's job only).
        let mut job_signal = signal.clone();
        let (cut, signal) = watch::channel(*signal.borrow());
        let cut = Arc::new(cut);
        let forward = tokio::spawn({
            let cut = cut.clone();
            async move {
                while job_signal.changed().await.is_ok() {
                    let now = *job_signal.borrow();
                    if cut.send(now).is_err() {
                        break;
                    }
                }
            }
        });
        let signal = &signal;
        let mut looping = Looping::in_worktree(worktree.clone());
        let looped = std::sync::atomic::AtomicBool::new(false);
        // An owner's job's token budget (`jobTokenBudget`), counted from where the owner last let
        // it past: a turn that reaches it is ended, and comes back `over_budget`.
        let budget =
            (!obi_review).then(|| job.budget_from.unwrap_or(0) + self.config.job_token_budget);
        let over_budget = std::sync::atomic::AtomicBool::new(false);
        // What the job used before this turn: each report of the turn's own counts goes on top.
        let before = job.usage.clone().unwrap_or_default();
        let on_event = |event: AgentEvent| {
            let entries = jobs
                .transcripts
                .record(id, &event, &worktree, job.check.as_deref());
            jobs.transcribed(id, entries);
            // Only a tool of a kind obid knows is safe keeps the job the owner's: anything else (a
            // vendor's web search, any MCP server or resource, a helper, a kind obid doesn't know)
            // may bring it someone else's words (crate::secretspec).
            if let AgentEvent::Tool { name, .. } = &event
                && !crate::secretspec::tool_is_known_safe(name)
            {
                let why = format!(
                    "its agent used {name}, which brings it content obid's proxy doesn't see"
                );
                jobs.mark(id, why, false);
            }
            match event {
                AgentEvent::Tokens(tokens) => {
                    jobs.update(id, |job| {
                        // Claude Code's counts are its session's so far, the turns it resumed
                        // included: this turn's own are what grew since the session's last ones.
                        let own = if tool == Tool::Claude {
                            let session = job.session.clone().unwrap_or_default();
                            let own = match &job.usage_seen {
                                Some((seen_in, seen)) if *seen_in == session => tokens.minus(seen),
                                _ => tokens.clone(),
                            };
                            job.usage_seen = Some((session, tokens));
                            own
                        } else {
                            tokens
                        };
                        job.usage = Some(before.plus(&own));
                        if let Some(ran) = ran_secs {
                            job.ran_secs = Some(ran());
                        }
                        if budget.is_some_and(|budget| spent(job) >= budget)
                            && !over_budget.swap(true, Ordering::Relaxed)
                        {
                            let _ = cut.send(Signal::Stop);
                        }
                    });
                }
                AgentEvent::Usage(usage) => {
                    let (was, now) = jobs.usage.record(&job.account, &usage);
                    jobs.share_rise(&job.account, &was, &now);
                    jobs.usage_changed.notify_one();
                }
                AgentEvent::Session(session) => {
                    jobs.update(id, |job| job.session = Some(session));
                }
                AgentEvent::Step(step) | AgentEvent::Denied(step) => {
                    jobs.update(id, |job| {
                        // What it asked with `oby_ask_owner` waits on the owner (PERMISSIONS-22).
                        if !job.question_call.as_ref().is_some_and(|c| c.ended_turn) {
                            job.step = step;
                        }
                        // The run's time so far, kept as it goes: a crash loses little of it.
                        if let Some(ran) = ran_secs {
                            job.ran_secs = Some(ran());
                        }
                    });
                }
                // The calls are the transcript's too; obid watches them for a loop.
                AgentEvent::Tool {
                    id: call,
                    name,
                    input,
                } => {
                    let doing = crate::agent::told_step(&name, &input);
                    jobs.update(id, |job| job.doing = Some(doing));
                    looping.called(call, name, input);
                }
                AgentEvent::ToolDone {
                    id: call, failed, ..
                } => {
                    if looping.done(&call, failed)
                        && !obi_review
                        && !looped.swap(true, Ordering::Relaxed)
                    {
                        let _ = cut.send(Signal::Stop);
                    }
                }
                // How full its session's context is now (USAGE-15): the latest, whatever round
                AgentEvent::Context(context) => {
                    jobs.update(id, |job| job.context_window = Some(context));
                }
                // The words, to-do list and a running command's output are the transcript's.
                AgentEvent::Delta(_)
                | AgentEvent::Output { .. }
                | AgentEvent::Spilled { .. }
                | AgentEvent::Said(_)
                | AgentEvent::Thought { .. }
                | AgentEvent::Thinking { .. }
                | AgentEvent::Plan(_) => {}
                // What Settings shows is an own conversation's (crate::connectors): a job's
                // session has the same owner's, with the job's broker in place of Oby's tools.
                AgentEvent::Connectors(_) => {}
            }
        };
        let plan = Self::mode_of(&job) == Mode::Plan;
        // The job's activity and context follow the transcript, saved and sent at most once a
        // second while the agent works (each call's start and end would be a `job` event).
        let run = async {
            match tool {
                Tool::Claude => {
                    // The owner's words go into it as they come, while its run opened that
                    // (crate::steer, TASKCONV-33).
                    let steer = self.lock().steers.get(id).cloned();
                    agent::run_turn_steered(
                        launch,
                        prompt,
                        limit.clone(),
                        signal.clone(),
                        steer,
                        decide,
                        on_event,
                    )
                    .await
                }
                Tool::Codex => {
                    // A ChatGPT login has no usage to read, and pays by its plan.
                    let chatgpt = self
                        .job_account(&job)
                        .is_ok_and(|(_, account)| crate::chatgpt::marker_of(&account).is_some());
                    let options =
                        codex::Options {
                            resume: job.session.clone(),
                            model: Some(job.model.clone().unwrap_or_else(|| {
                                crate::settings::default_model(tool).to_owned()
                            })),
                            effort: job.effort.clone(),
                            plan,
                            instructions: (!obi_review).then(|| standing_rules(&job)).flatten(),
                            no_commands: false,
                            usage: !chatgpt,
                            priced: if chatgpt {
                                Some("plan".into())
                            } else {
                                self.usage.get(&job.account).priced
                            },
                            chatgpt,
                        };
                    codex::run_turn(
                        launch,
                        prompt,
                        &options,
                        limit.clone(),
                        signal.clone(),
                        decide,
                        on_event,
                    )
                    .await
                }
                Tool::Cursor => {
                    let account = self
                        .job_account(&job)
                        .map(|(_, a)| a.name)
                        .unwrap_or_default();
                    let options = acp::Options {
                        tool: tool.name().into(),
                        account,
                        resume: job.session.clone(),
                        // The answer to a call it was parked for goes to its own session only, as
                        // does a cut run's continuation.
                        resume_only: session_only
                            || job.parked.as_ref().is_some_and(|p| p.answered),
                        model: Some(job.model.clone().unwrap_or_else(|| crate::settings::default_model(tool).to_owned())),
                        plan,
                        plan_mode: "plan".into(),
                        agent_mode: "agent".into(),
                        // Oby's Tetrix server (crate::tetrix) and, for the owner's job, Oby's own
                        // with only `oby_ask_owner`: the only ones Cursor gets.
                        mcp: [("tetrix", self.tetrix_mcp(&job)), ("oby", (!obi_review).then(|| self.ask_mcp(id)).flatten())]
                            .into_iter()
                            .filter_map(|(name, server)| server.map(|(url, token)| json!({"type": "http", "name": name, "url": url,
                                "headers": [{"name": "Authorization", "value": format!("Bearer {token}")}]})))
                            .collect(),
                        quiet_load: false,
                    };
                    acp::run_turn(
                        launch,
                        prompt,
                        &options,
                        limit.clone(),
                        signal.clone(),
                        decide,
                        on_event,
                    )
                    .await
                }
            }
        };
        tokio::pin!(run);
        // The words being written and a running command's lines stream about every tenth of a
        // second (LIVETASK-32, 34); the job's activity once a second.
        let mut streaming = tokio::time::interval(STREAM_EVERY);
        streaming.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut activity =
            tokio::time::interval_at(tokio::time::Instant::now() + ACTIVITY_EVERY, ACTIVITY_EVERY);
        activity.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let ran = loop {
            tokio::select! {
                ran = &mut run => break ran,
                _ = streaming.tick() => {
                    self.transcribed(id, self.transcripts.tick(id, spill_temp.as_deref()));
                }
                _ = activity.tick() => {
                    self.glanced(id);
                    // A call of its agent's that waits for the owner past `parkAfter` parks it.
                    if parks {
                        self.park(id);
                    }
                    if self.left_own_lane(id, tool) {
                        let _ = cut.send(Signal::Stop);
                    }
                }
            }
        };
        self.transcribed(id, self.transcripts.end_turn(id));
        self.glanced(id);
        self.transcripts.forget(id);
        forward.abort();
        let mut turn = ran.map_err(|e| format!("obid couldn't start {}: {e}", tool.name()))?;
        turn.looped = looped.load(Ordering::Relaxed);
        turn.over_budget = over_budget.load(Ordering::Relaxed);
        Ok(turn)
    }

    /// A job gone quiet or at its turn limit (`text`) goes on in its session, while it has tries left
    /// of `tries` (RECOVERY-3, RECOVERY-4, RECOVERY-12): what its agent is told next. Without a session or tries, it stalls.
    pub(super) fn stall_try(&self, id: &str, text: &str, tries: usize) -> Option<String> {
        let job = self.get(id)?;
        job.session.as_ref()?;
        if job.recovery.stalls >= tries || job.recovery.spent() {
            return None;
        }
        let attempt = job.recovery.stalls + 1;
        tracing::info!("job {id}: {text}: continuing it ({attempt} of {tries})");
        self.update(id, |job| {
            job.recovery.stalls = attempt;
            job.recovery.tries += 1;
            job.step = format!("Trying again now ({attempt} of {tries})");
        })?;
        Some(continue_message(text, None))
    }

    /// What a job's agent is first told: its task, and what obid found for it (the repository's
    /// rules, the owner's memories, teammates, Tetrix). The worktree's files it names are written
    /// afresh; a new session (RECOVERY-8) needs all of it again.
    pub(super) async fn first_message(
        &self,
        id: &str,
        job: &Job,
        worktree: &Path,
        base: &str,
        rules_quoted: bool,
    ) -> Result<String, String> {
        let mut message = owner_prompt(job);
        if rules_quoted {
            message = format!("{message}\n\n{}", review::rules_line(base));
        }
        // The owner's memories about them and the job's repository, quoted (#378). Never on a
        // teammate's Mac: only the job's code goes there, never its inputs.
        if let Some(text) =
            crate::memory::for_agent(&self.config.home, &[job.repo.as_str()]).await?
        {
            // A memory found in words that weren't the owner's alone (crate::secretspec).
            if let Some(why) = crate::memory::foreign_among(&self.config.home, &[job.repo.as_str()])
            {
                self.unvet(id, &format!("it read {why}"));
            }
            // One known to come from someone else's words: two reviews merge its pull
            // request (MERGING-14).
            if crate::memory::someone_elses_among(&self.config.home, &[job.repo.as_str()]) {
                self.mark_line(id, |job| job.memory_from_others = true);
            }
            let written = crate::obi_dir::folder(worktree, "inputs").and_then(|dir| {
                crate::obi_dir::write_new(&dir, crate::memory::FILE, text.as_bytes())
            });
            match written {
                Ok(_) => {
                    let path = format!(".obi/inputs/{}", crate::memory::FILE);
                    message = format!("{message}\n\n{}", crate::memory::prompt_line(&path));
                }
                Err(e) => tracing::warn!("job {id}: the owner's memories weren't quoted: {e}"),
            }
        }
        let mut context = Vec::new();
        // Teammates' open pull requests on the files it claimed (crate::teammates).
        if let Some(found) = self
            .teammates_on(job, job.files.as_deref().unwrap_or_default())
            .await
        {
            let prs: Vec<_> = found.iter().map(|(pr, _)| pr.clone()).collect();
            if let Some(line) = crate::teammates::prompt_line(&prs) {
                message = format!("{message}\n\n{line}");
                context.push(("teammates", crate::teammates::context_items(&found)));
            }
            self.update(id, |job| {
                if !job.teammates.as_ref().is_some_and(|t| t.rechecked) {
                    job.teammates = Some(crate::teammates::Teammates {
                        prs,
                        rechecked: false,
                    });
                }
            });
        }
        // What Tetrix knows about the job, asked with Oby's login (crate::tetrix).
        if let (Some(tetrix), Some(connections)) = (self.tetrix_for(job), self.connections.get()) {
            let (items, seen) = tetrix.context(connections, job).await;
            self.tetrix_used(id, |used| used.absorb(seen));
            if !items.is_empty() {
                context.push(("tetrix", items));
            }
        }
        // What they gave, quoted as data in `.obi/context/`: the prompt names only the files.
        if let Some(note) = self.quote_context(id, worktree, &context) {
            message = format!("{message}\n\n{note}");
        }
        if self.tetrix_for(job).is_some() {
            message = format!("{message}\n\n{}", tetrix_line(job));
        }
        Ok(message)
    }

    /// Its conversation was told of how it ended (TASKCONV-18).
    pub fn mark_told(&self, id: &str) {
        self.update(id, |job| job.told = job.finished);
    }

    /// A job whose session grew too long for its model goes on in a new one, once until the owner's
    /// own words (RECOVERY-8): what its agent is told. Past that, the owner is told.
    pub(super) fn new_session(&self, id: &str) -> Option<String> {
        let job = self.get(id)?;
        job.session.as_ref()?;
        if job.recovery.renewed >= NEW_SESSIONS {
            return None;
        }
        tracing::info!("job {id}: its session is too long for its model: starting a new one");
        self.update(id, |job| {
            job.recovery.renewed += 1;
            job.session = None;
            job.changed("Started a new session: its last was too long for its model".into());
        });
        Some(continue_message(
            "your session grew too long for your model, so this is a new one with none of its history. What was done so far is on this branch: read its log and diff first",
            None,
        ))
    }

    /// The job stalled (`why`, docs/runner-api.md `stalled`): its agent writes a handoff note in one
    /// short turn more of its session (or obid writes one from what it saw), the job keeps `limit`
    /// and `stalled` (the note in it), and it ends needing the owner, `Stalled: <why>.` its reason.
    /// Its branch and worktree stay; nothing is published.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn stall(
        &self,
        id: &str,
        why: Stall,
        tool: Tool,
        spec: &Spec,
        mut flags: Vec<OsString>,
        account: &crate::config::Account,
        broker: Option<&(String, String)>,
        signal: &watch::Receiver<Signal>,
        mcp: &[String],
    ) -> End {
        let text = why.text(&self.config);
        tracing::info!("job {id} stalled: {text}");
        let before = self.get(id);
        let step = before.as_ref().map(|j| j.step.clone()).unwrap_or_default();
        self.update(id, |job| job.step = "Writing a handoff note".into());
        let mut note = None;
        if let Some(job) = self.get(id)
            && let Some(session) = job.session.clone()
            && *signal.borrow() == Signal::Run
        {
            if tool == Tool::Claude && !flags.iter().any(|f| f == "--resume") {
                flags.splice(1..1, [OsString::from("--resume"), session.into()]);
            }
            let prompt = format!(
                "obid stopped your work here: {text}. Don't carry on with the task. Commit what's worth keeping, then write a handoff note for whoever picks this up, as your whole reply, under three headings: Done (what works, and is committed), Left (what's still to do, in order), Failed (what went wrong and why, and what you tried). Keep it under 300 words."
            );
            if let Ok(launch) = self.launch(&job, spec, tool, flags, account, broker)
                && let Ok(turn) = self
                    .turn(
                        id,
                        tool,
                        launch,
                        &prompt,
                        signal,
                        Gate::Owner {
                            mcp,
                            park: false,
                            session_only: false,
                        },
                        crate::conn::Limit {
                            most: HANDOFF_LIMIT,
                            quiet: Some(self.config.stall_after),
                            watch: None,
                            waiting: None,
                        },
                        None,
                    )
                    .await
                && turn.succeeded()
                && !turn.text().trim().is_empty()
            {
                note = clean_note(&turn.text());
            }
        }
        // It needs the owner (#725): Try again carries it on in its session (past a safety stall,
        // only once they confirm), and the handoff note is in `stalled`, beside the agent's own
        // summary.
        // Out of continuations in all, it says so (RECOVERY-17).
        let spent = before.as_ref().is_some_and(|j| j.recovery.spent());
        let said = match spent && matches!(why, Stall::Quiet | Stall::Turns) {
            true => crate::job::capped(&text),
            false => format!("Stalled: {text}."),
        };
        let needs_you = || End::needs_you(said.clone(), Some(crate::job::TRY_AGAIN));
        let Some(job) = self.get(id).or(before) else {
            return needs_you();
        };
        let (note, by) = match note {
            Some(note) => (note, "agent"),
            None => {
                let note = |report: &str| {
                    format!(
                        "The agent couldn't write a note, so obid did.\nWhere it was: round {}, {}.\nIts last report: {report}\nWhat stopped it: {text}.",
                        job.current_round(),
                        if step.is_empty() {
                            "no step yet"
                        } else {
                            &step
                        },
                    )
                };
                let report = job.summaries.as_deref().and_then(<[String]>::last);
                let report = report.map_or("none".into(), |r| one_line(r, 600));
                // Its last report is the agent's words: left out when the scan flags them.
                let note = clean_note(&note(&report)).unwrap_or_else(|| note("left out"));
                (note, "obid")
            }
        };
        let stalled = crate::job::Stalled {
            why: why.id().into(),
            text: text.clone(),
            at: obi_protocol::time::now(),
            round: job.current_round(),
            branch: job.branch.clone(),
            commit: None,
            commits: 0,
            session: job.session.is_some(),
            note: note.clone(),
            note_by: by.into(),
        };
        self.update(id, |job| {
            job.limit = Some(text.clone());
            job.stalled = Some(stalled.clone());
        });
        needs_you()
    }

    /// Tells the apps about transcript entries that came or changed, and words added to one as
    /// the agent writes them (`transcript` events).
    pub(super) fn transcribed<C: Into<crate::transcript::Change>>(
        &self,
        id: &str,
        changes: Vec<C>,
    ) {
        for change in changes {
            let data = match change.into() {
                crate::transcript::Change::Entry(entry) => json!({"job": id, "entry": entry}),
                crate::transcript::Change::Append { n, text } => {
                    json!({"job": id, "n": n, "append": text})
                }
                crate::transcript::Change::Tail { n, text } => {
                    json!({"job": id, "n": n, "tail": text})
                }
                crate::transcript::Change::Output { n, text, lines } => {
                    json!({"job": id, "n": n, "output": {"text": text, "lines": lines}})
                }
            };
            self.events.send("transcript", data);
        }
    }

    /// The transcript entry `n` of job `id` with its detail (`GET /v1/jobs/{id}/transcript/{n}`);
    /// `None` for a job or an entry obid doesn't have.
    pub fn transcript_entry(
        &self,
        id: &str,
        n: u64,
    ) -> Option<(crate::transcript::Entry, Option<Value>)> {
        self.get(id)?;
        self.transcripts.detail(id, n)
    }

    /// Keeps the job's `activity` and `context` up to date with its transcript, when they changed.
    fn glanced(&self, id: &str) {
        let (activity, context) = self.transcripts.glance(id);
        let job = self.get(id);
        if job.is_some_and(|j| j.activity != activity || j.context != context) {
            self.update(id, |job| {
                job.activity = activity;
                job.context = context;
            });
        }
    }

    /// The job's transcript entries that came or changed after entry `after`
    /// (`GET /v1/jobs/{id}/transcript`); `None` for a job obid doesn't have.
    pub fn transcript(
        &self,
        id: &str,
        after: u64,
        limit: usize,
    ) -> Option<(Vec<crate::transcript::Entry>, Option<u64>)> {
        self.get(id)?;
        Some(self.transcripts.read(id, after, limit))
    }
}
