//! The agent's permission asks and questions (`AskUserQuestion`, `oby_ask_owner`): the modes, the
//! saved rules, the sites a job's agent reaches through the egress proxy, and the owner's answers,
//! parked calls' included. It answers a call; it never runs one itself.

use super::*;

impl Jobs {
    pub(super) fn note(&self, note: PermissionNote) {
        if let Some(hook) = self.permission_hook.get() {
            hook(note);
        }
    }

    pub(super) fn kept(&self) -> std::sync::MutexGuard<'_, permissions::Kept> {
        self.permissions.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Settings › Permissions: the mode new jobs get.
    pub fn default_mode(&self) -> Mode {
        self.kept().default_mode()
    }

    /// Whether Bypass widens trust here: it does unless it's the owner's own default
    /// (Settings › Permissions), in which case every new job runs in it anyway, so the brain
    /// naming it, continuing a job in it or carrying it on by itself asks nothing wider than
    /// what the owner chose. Only the brain's own choices go through this: a mode the owner
    /// names (`POST /v1/jobs`, `POST /v1/jobs/{id}/mode`) never waits.
    pub fn bypass_widens(&self) -> bool {
        self.default_mode() != Mode::Bypass
    }

    /// The mode a job goes on in once the owner approves its plan: their default, the mode a
    /// new job gets (Bypass too: approving the plan asks nothing wider than any job they start),
    /// or Ask when that default is Plan itself, which the job is leaving.
    pub fn approved_mode(&self) -> Mode {
        match self.default_mode() {
            Mode::Plan => Mode::Ask,
            mode => mode,
        }
    }

    /// Changes the mode new jobs get, and keeps it. Jobs already made keep theirs.
    pub fn set_default_mode(&self, mode: Mode) -> std::io::Result<()> {
        let mut kept = self.kept();
        let mut changed = kept.clone();
        changed.mode = Some(mode.as_str().to_owned());
        permissions::save(&self.config.home, &changed)?;
        *kept = changed;
        Ok(())
    }

    /// The saved rules, oldest first.
    pub fn rules(&self) -> Vec<Rule> {
        self.kept().rules.clone()
    }

    /// Forgets a rule: the rules left, or `None` for a rule obid doesn't have.
    pub fn forget_rule(&self, id: &str) -> Option<std::io::Result<Vec<Rule>>> {
        let mut kept = self.kept();
        if !kept.rules.iter().any(|r| r.id == id) {
            return None;
        }
        let mut changed = kept.clone();
        changed.rules.retain(|r| r.id != id);
        if let Err(e) = permissions::save(&self.config.home, &changed) {
            return Some(Err(e));
        }
        *kept = changed;
        // What the rule let agents reach is decided again (crate::egress).
        if let Some(egress) = self.egress.get() {
            egress.forget(None);
        }
        Some(Ok(kept.rules.clone()))
    }

    /// Saves an allow rule for a repository, once; the prompts waiting now, in every job, follow it.
    pub(crate) fn save_rule(&self, rule: &str, repo: &str) {
        {
            let mut kept = self.kept();
            if kept
                .rules
                .iter()
                .any(|r| r.rule == rule && r.repo == repo && r.decision == "allow")
            {
                return;
            }
            let mut changed = kept.clone();
            changed.rules.push(Rule {
                id: permissions::new_rule_id(),
                rule: rule.to_owned(),
                repo: repo.to_owned(),
                decision: "allow".into(),
                created: obi_protocol::time::now(),
            });
            if let Err(e) = permissions::save(&self.config.home, &changed) {
                tracing::error!("can't keep the permission rule {rule}: {e}");
                return;
            }
            *kept = changed;
        }
        if let Some(egress) = self.egress.get() {
            egress.forget(None);
        }
        let waiting: Vec<String> = self.lock().asks.keys().cloned().collect();
        for id in waiting {
            self.reconsider(&id);
        }
    }

    /// A Cursor job starts with its repository's hooks ([`acp::cursor::repository_hooks`]) only on
    /// the owner's yes, which Cursor would otherwise run as its own children, outside obid's
    /// prompts: a card like any permission's, allowed once, or always for these very hooks in this
    /// repository (the rule `RepositoryHooks(<digest>)`: a changed hook asks again). A no starts
    /// nothing, and says Claude Code and Codex don't run them; a stop ends the wait. The mode
    /// decides first (`permissions::decide`): in Bypass they run without a card, as the owner
    /// chose (secrets too since #1918: the job's box holds); in any other mode one that touches
    /// secrets asks and no rule answers (#1570).
    pub(super) async fn allow_repository_config(
        &self,
        id: &str,
        repo: &str,
        hooks: &[String],
        allowlist: bool,
        signal: &watch::Receiver<Signal>,
    ) -> Result<(), End> {
        let detail = hooks.join("\n");
        let digest = hex::encode(obi_protocol::keys::sha256(detail.as_bytes()));
        let rule = format!("{}({})", permissions::REPOSITORY_HOOKS, &digest[..16]);
        // `.cursor/cli.json`'s entries are what Cursor runs as they are (`Shell(npm test)`).
        let runs: Vec<&str> = hooks
            .iter()
            .map(|h| match allowlist {
                true => h.as_str(),
                false => h.split_once(": ").map_or(h.as_str(), |(_, run)| run),
            })
            .collect();
        // What the hooks run, one command a line, read as the agent's commands are: the mode
        // decides, as `reconsider` does when it changes, and secrets ask in every mode (#1570).
        let call = Call {
            tool: permissions::REPOSITORY_HOOKS.into(),
            command: Some(runs.join("\n")),
            ..Call::default()
        };
        let Some(job) = self.get(id) else {
            return Err(End::Failed("It stopped before Cursor started.".into()));
        };
        let bounds = self.bounds(&job, &self.worktree_path(id));
        let rules = self.rules_for(repo);
        let secret = permissions::reach(&call, &bounds) == Some(permissions::Reach::Secrets);
        match permissions::decide(&call, Self::mode_of(&job), &rules, &bounds) {
            permissions::Verdict::Allow => return Ok(()),
            permissions::Verdict::Deny(why) => {
                return Err(End::Failed(format!("Cursor didn't start: {why}")));
            }
            permissions::Verdict::Ask => {}
        }
        // The owner's Always for these very hooks; never for one that touches secrets.
        if !secret
            && rules
                .iter()
                .any(|r| r.rule == rule && r.decision == "allow")
        {
            return Ok(());
        }
        let title = if allowlist {
            format!(
                "Let Cursor run what this repository's .cursor/cli.json allows: {}",
                runs.join(", ")
            )
        } else {
            format!(
                "Let Cursor run this repository's hooks: {}",
                runs.join(", ")
            )
        };
        let permission = Permission {
            id: permissions::new_id(),
            tool: permissions::REPOSITORY_HOOKS.into(),
            title: if title.chars().count() > 120 {
                format!("{}…", title.chars().take(119).collect::<String>())
            } else {
                title
            },
            detail: detail.chars().take(2000).collect(),
            rule: (!secret).then_some(rule),
            always: (!secret).then(|| match allowlist {
                true => format!("Yes, and don't ask again for these in {repo}"),
                false => format!("Yes, and don't ask again for these hooks in {repo}"),
            }),
            why: Some(if secret {
                permissions::Reach::Secrets.why().into()
            } else if allowlist {
                "This repository's .cursor/cli.json lets Cursor run these without asking obid."
                    .into()
            } else {
                "Cursor runs a repository's hooks itself, outside obid's prompts.".into()
            }),
            question: None,
        };
        let (reply, mut answer) = tokio::sync::oneshot::channel();
        let first = {
            let mut state = self.lock();
            let asks = state.asks.entry(id.to_owned()).or_default();
            asks.push_back(Ask {
                call,
                permission,
                reply,
                input: Value::Null,
                answers: Vec::new(),
                owner: None,
                since: None,
            });
            asks.len() == 1
        };
        if first {
            self.show(id, None);
        }
        let mut signal = signal.clone();
        let answer = loop {
            if *signal.borrow() != Signal::Run {
                return Err(End::Failed("It stopped before Cursor started.".into()));
            }
            tokio::select! {
                answer = &mut answer => break answer,
                changed = signal.changed() => if changed.is_err() {
                    return Err(End::Failed("It stopped before Cursor started.".into()));
                },
            }
        };
        match answer {
            Ok(Answer::Allow | Answer::AllowWith(_)) => Ok(()),
            _ if allowlist => Err(End::Failed(
                "Cursor didn't start: you said no to what this repository's .cursor/cli.json lets it run without asking obid. Claude Code and Codex jobs don't read it: move it to one of them, or start it again and allow it.".into(),
            )),
            _ => Err(End::Failed(
                "Cursor didn't start: you said no to this repository's hooks, which Cursor runs itself, outside obid's prompts. Claude Code and Codex jobs don't run them: move it to one of them, or start it again and allow them.".into(),
            )),
        }
    }

    /// The rules for one repository.
    fn rules_for(&self, repo: &str) -> Vec<Rule> {
        self.kept()
            .rules
            .iter()
            .filter(|r| r.repo == repo)
            .cloned()
            .collect()
    }

    /// The job's mode, as it names it. A job from before modes is in Bypass; a mode this obid
    /// can't read (one a newer obid wrote) asks rather than allow. A review job is
    /// in Plan, whatever it says.
    pub fn mode_of(job: &Job) -> Mode {
        if job.read_only() {
            return Mode::Plan;
        }
        match job.mode.as_deref() {
            None => Mode::Bypass,
            Some(mode) => Mode::parse(mode).unwrap_or(Mode::Ask),
        }
    }

    /// Where the job's sandbox ends, for `permissions::reach`: its worktree, its temporary folder,
    /// its `HOME` (its own login folder for a Cursor account) and its agent's login folders, the
    /// account's own among them (never mounted, still never to be read).
    pub(super) fn bounds(&self, job: &Job, worktree: &Path) -> permissions::Bounds {
        let temp = self.temp_path(&job.id);
        let tool = Tool::from_id(&job.agent);
        let config_dir = tool.and_then(|tool| {
            self.accounts(tool)
                .into_iter()
                .find(|a| a.id == job.account)
                .and_then(|a| a.config_dir)
        });
        let mut logins = vec![
            self.login_path(&job.id),
            temp.join(".claude"),
            temp.join(".claude.json"),
            temp.join(".codex"),
        ];
        logins.extend(config_dir.clone());
        let home = match (tool, &config_dir) {
            (Some(Tool::Cursor), _) => self.login_path(&job.id),
            _ => temp.clone(),
        };
        permissions::Bounds {
            worktree: worktree.to_path_buf(),
            temp: Some(temp),
            home: Some(home),
            logins,
            git: self.made_git_folders(job),
            programs: (tool == Some(Tool::Codex))
                .then(|| self.agent_bin(Tool::Codex))
                .flatten()
                .and_then(|bin| crate::codex::shipped_programs(&bin)),
        }
    }

    /// A prompt obid's fixed rules let through: the job's mode and its repository's rules allow or
    /// refuse it now, or it waits for the owner, shown on the job and in the inbox.
    pub(super) fn rule_on(&self, id: &str, tool: &str, input: &Value, worktree: &Path) -> Ruling {
        let Some(job) = self.get(id) else {
            return Ruling::Now(Answer::Deny("The job is gone.".into()));
        };
        let call = Call::new(tool, input);
        let bounds = self.bounds(&job, worktree);
        // The owner allowed this call while the job was parked for it: its resumed agent's same
        // call goes through, once, as their own yes ([`Jobs::answer_parked`]).
        let allowed = self
            .lock()
            .jobs
            .get_mut(id)
            .and_then(|j| j.parked.as_mut())
            .filter(|p| p.lets_through(tool, input))
            .map(|p| p.allowed = false)
            .is_some();
        if allowed {
            self.update(id, |_| {});
            if let Some(egress) = self.egress.get() {
                egress.let_through(id, &self.sites_opened(&call, &bounds));
            }
            return Ruling::Now(Answer::Allow);
        }
        let mode = Self::mode_of(&job);
        // A job that only reads (#630) never leaves Plan mode, so its plan is no question for
        // the owner: it reports what it found instead.
        if call.tool == "ExitPlanMode" && job.read_only() {
            let why = "This job only reads: stay in Plan mode and end with your findings as your summary.".to_owned();
            self.answered(id, &call, worktree, Some(&why));
            return Ruling::Now(Answer::Deny(why));
        }
        // A plan in Plan mode waits for the owner's own answer (#609): Claude Code's ExitPlanMode
        // and Cursor's create_plan alike (#1008); Codex's is its Plan turn's end (jobs::codex_plan).
        let plan = call.tool == "ExitPlanMode" && mode == Mode::Plan;
        // The agent's question waits for the owner's own answer in every mode, Bypass too (#858):
        // no mode or rule can answer it for them.
        let question = call.tool == permissions::ASK_USER_QUESTION;
        if question && call.questions.is_empty() {
            let why = "Ask at least one question, each with its whole text and its options.";
            self.answered(id, &call, worktree, Some(why));
            return Ruling::Now(Answer::Deny(why.into()));
        }
        let verdict = if plan || question {
            permissions::Verdict::Ask
        } else {
            permissions::decide(&call, mode, &self.rules_for(&job.repo), &bounds)
        };
        match verdict {
            permissions::Verdict::Allow => {
                self.answered(id, &call, worktree, None);
                Ruling::Now(Answer::Allow)
            }
            permissions::Verdict::Deny(why) => {
                self.answered(id, &call, worktree, Some(&why));
                Ruling::Now(Answer::Deny(why))
            }
            // Oby's own review may run a local command its box holds, `cargo test` say (the
            // owner's answer of 2026-11-08, PERMISSIONS-14), where nothing it reaches leaves it.
            permissions::Verdict::Ask if self.review_runs(&job, &call, &bounds) => {
                self.answered(id, &call, worktree, None);
                Ruling::Now(Answer::Allow)
            }
            permissions::Verdict::Ask => match unwatched_review(&job, &call) {
                Some(why) => {
                    self.answered(id, &call, worktree, Some(&why));
                    Ruling::Now(Answer::Deny(why))
                }
                None => Ruling::Ask(
                    self.wait_for_owner(id, call, input, &job.repo, &bounds, None, None)
                        .expect("no limit to wait under"),
                ),
            },
        }
    }

    /// Puts `call` on job `id`'s card for the owner, saying which sites their yes opens; its
    /// answer comes later, and `owner` hears when it's the owner's own. None, and nothing asked,
    /// when `most_sites` of the job's fetches wait for the owner already (counted as it's put).
    #[allow(clippy::too_many_arguments)]
    fn wait_for_owner(
        &self,
        id: &str,
        call: Call,
        input: &Value,
        repo: &str,
        bounds: &permissions::Bounds,
        owner: Option<tokio::sync::oneshot::Sender<()>>,
        most_sites: Option<usize>,
    ) -> Option<tokio::sync::oneshot::Receiver<Answer>> {
        let mut permission = permissions::card(&call, &permissions::new_id(), repo, bounds);
        let sites = self.sites_opened(&call, bounds);
        if !sites.is_empty() {
            permission.detail.push_str(&format!(
                "\n\nAllowing it lets this task reach {} until it ends.",
                sites.join(", ")
            ));
        }
        let (reply, answer) = tokio::sync::oneshot::channel();
        let first = {
            let mut state = self.lock();
            let asks = state.asks.entry(id.to_owned()).or_default();
            let fetches = asks.iter().filter(|a| a.call.tool == "WebFetch").count();
            if most_sites.is_some_and(|most| fetches >= most) {
                return None;
            }
            asks.push_back(Ask {
                call,
                permission,
                reply,
                input: input.clone(),
                answers: Vec::new(),
                since: owner.is_none().then(std::time::Instant::now),
                owner,
            });
            asks.len() == 1
        };
        if first {
            self.show(id, None);
        }
        Some(answer)
    }

    /// What job `id`'s agent reaches through obid's egress proxy (crate::egress, #616) without
    /// a decision of its own: its tool's own services on 443 go through; telemetry to anyone
    /// else, and anything more for another Obi's review (it only reads, and never asks), don't.
    /// None for a site the job decides ([`Jobs::ask_about`]).
    pub(crate) fn reach_now(&self, id: &str, host: &str, port: u16) -> Option<Result<(), String>> {
        let site = crate::egress::site(host, port);
        // A conversation's Cursor: its tool's own services, and nothing else ever.
        if id.starts_with(crate::brain::harness_cursor::DOOR) {
            let own = crate::egress::tools_own(Tool::Cursor, host, port);
            let why = "A conversation on Cursor reaches only Cursor's own services, not";
            return Some(own.then_some(()).ok_or_else(|| format!("{why} {site}.")));
        }
        let Some(job) = self.get(id) else {
            return Some(Err("The job is gone.".into()));
        };
        if let Some(tool) = Tool::from_id(&job.agent)
            && crate::egress::tools_own(tool, host, port)
        {
            if host == "chatgpt.com" && job.withheld.is_none() {
                self.update(id, Job::reached_chatgpt);
            } else if host != "chatgpt.com" && !crate::egress::tools_api(tool, host) {
                let why = format!("its agent reached {site}, past its tool's API");
                self.mark(id, why, false);
            }
            return Some(Ok(()));
        }
        // Telemetry a CLI sends elsewhere (Datadog, Sentry) is a site like any other: the job's
        // rules, its mode or the owner's card decide it (PERMISSIONS-2).
        job.obi
            .is_some()
            .then(|| Err(format!("This job doesn't reach {site}.")))
    }

    /// How job `id` decides a site its agent wants to reach (`host` on `port`), once for its
    /// worker (crate::egress keeps the answer): as a fetch from it (`WebFetch` of
    /// `https://<site>/`). A package registry goes through in Sandbox mode
    /// ([`crate::egress::REGISTRIES`]); else the repository's rules and the job's mode answer, or
    /// the owner's card does (Allow for the rest of the job, Always allow saving the rule too). At
    /// most [`MOST_SITES_ASKED`] wait for the owner at once: another is refused this once.
    pub(crate) fn ask_about(
        &self,
        id: &str,
        host: &str,
        port: u16,
        plain: bool,
    ) -> crate::egress::Asked {
        use crate::egress::Asked;
        let Some(job) = self.get(id) else {
            return Asked::Now(Err("The job is gone.".into()));
        };
        let site = crate::egress::site(host, port);
        // Its card says how it's reached: plain http shows `http://` (PERMISSIONS-2).
        let scheme = if plain { "http" } else { "https" };
        let input = json!({"url": format!("{scheme}://{site}/")});
        let call = Call::new("WebFetch", &input);
        let worktree = self.worktree_path(id);
        let mode = Self::mode_of(&job);
        if port == 443 && mode == Mode::Sandbox && crate::egress::REGISTRIES.contains(&host) {
            self.answered(id, &call, &worktree, None);
            return Asked::Now(Ok(()));
        }
        let bounds = self.bounds(&job, &worktree);
        match permissions::decide(&call, mode, &self.rules_for(&job.repo), &bounds) {
            permissions::Verdict::Allow => {
                self.answered(id, &call, &worktree, None);
                Asked::Now(Ok(()))
            }
            permissions::Verdict::Deny(why) => {
                self.answered(id, &call, &worktree, Some(&why));
                Asked::Now(Err(why))
            }
            permissions::Verdict::Ask if job.review_of.is_some() => {
                let why = unwatched_review(&job, &call).unwrap_or_default();
                self.answered(id, &call, &worktree, Some(&why));
                Asked::Now(Err(why))
            }
            permissions::Verdict::Ask => {
                let (owner, by_owner) = tokio::sync::oneshot::channel();
                match self.wait_for_owner(
                    id,
                    call,
                    &input,
                    &job.repo,
                    &bounds,
                    Some(owner),
                    Some(MOST_SITES_ASKED),
                ) {
                    Some(answer) => Asked::Card { answer, by_owner },
                    None => Asked::Busy(format!(
                        "Other sites are waiting for the owner: ask for {site} again once they answer."
                    )),
                }
            }
        }
    }

    /// The repository's rule that refuses job `id` the site `host` on `port`, said as the
    /// agent hears it: checked at every connection, so a deny saved later stops a site the
    /// owner had let through.
    pub(crate) fn denied_site(&self, id: &str, host: &str, port: u16) -> Option<String> {
        let job = self.get(id)?;
        let call = Call::new(
            "WebFetch",
            &json!({"url": format!("https://{}/", crate::egress::site(host, port))}),
        );
        let worktree = self.worktree_path(id);
        self.rules_for(&job.repo)
            .into_iter()
            .find(|r| r.decision == "deny" && permissions::covers(&r.rule, &call, &worktree, false))
            .map(|r| permissions::rule_refuses(&r.rule, &r.repo))
    }

    /// The sites the owner's yes to `call` lets the job's agent reach through the egress proxy
    /// until the job ends, as its card says: a fetch's site, and the `http(s)://` addresses of
    /// the commands in it that reach the network as obid reads them (not a comment's, nor an
    /// `echo`'s). None where agents aren't proxied.
    fn sites_opened(&self, call: &Call, bounds: &permissions::Bounds) -> Vec<String> {
        if self.egress.get().is_none()
            || permissions::reach(call, bounds) != Some(permissions::Reach::Network)
        {
            return Vec::new();
        }
        let words = match call.tool.as_str() {
            "WebFetch" => call.url.iter().cloned().collect(),
            "Bash" => call
                .command
                .as_deref()
                .map(|command| permissions::network_words(command, bounds))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        crate::egress::sites_named(&words)
    }

    /// Keeps on the job a permission obid answered by its own rules, `refused` with the words the
    /// agent heard, so the owner can see what was decided without them.
    pub fn answered(&self, id: &str, call: &Call, worktree: &Path, refused: Option<&str>) {
        if !permissions::is_noted(&call.tool) {
            return;
        }
        let Some(job) = self.get(id) else { return };
        let entry = permissions::Answered::of(
            call,
            &job.repo,
            &self.bounds(&job, worktree),
            refused,
            obi_protocol::time::now(),
        );
        self.update(id, |job| {
            permissions::keep_answered(&mut job.answered, &mut job.answered_count, entry)
        });
    }

    /// The job shows its first waiting permission, or, with none left, goes on, its step saying
    /// what happened. The inbox hears of it.
    fn show(&self, id: &str, step: Option<String>) {
        // A question's call goes with it, so a restart keeps it (#858).
        let (first, call) = self
            .lock()
            .asks
            .get(id)
            .and_then(|asks| {
                asks.front().map(|a| {
                    let call = a
                        .permission
                        .is_question()
                        .then(|| permissions::QuestionCall {
                            questions: a.call.questions.clone(),
                            answers: a.answers.clone(),
                            ended_turn: false,
                            parked: false,
                        });
                    (a.permission.clone(), call)
                })
            })
            .unzip();
        let call = call.flatten();
        let shown = self.get(id).and_then(|j| j.permission);
        if first.as_ref().map(|p| &p.id) == shown.as_ref().map(|p| &p.id) {
            if let Some(step) = step {
                self.update(id, |job| job.step = step);
            }
            return;
        }
        // The inbox has it before the job shows it: whoever sees the one finds the other.
        if let (Some(job), Some(permission)) = (self.get(id), first.clone()) {
            self.note(PermissionNote::Waiting(Box::new(job), permission));
        }
        self.update(id, |job| {
            job.permission = first.clone();
            job.question_call = call;
            if first.as_ref().is_some_and(Permission::is_plan) {
                job.step = codex_plan::PLAN_WAITS.into();
            } else if first.as_ref().is_some_and(Permission::is_question) {
                job.step = "Waiting for your answer".into();
            } else if first.is_some() {
                job.step = "Waiting for your OK".into();
            } else if let Some(step) = step {
                job.step = step;
            }
        });
    }

    /// The job's waiting prompts after its mode or the rules changed: what's allowed or refused
    /// now is answered, the rest keep waiting.
    pub(super) fn reconsider(&self, id: &str) {
        let Some(job) = self.get(id) else { return };
        let worktree = self.worktree_path(id);
        let bounds = self.bounds(&job, &worktree);
        let rules = self.rules_for(&job.repo);
        let mode = Self::mode_of(&job);
        let mut answered: Vec<(Ask, Answer)> = Vec::new();
        {
            let mut state = self.lock();
            let Some(asks) = state.asks.get_mut(id) else {
                return;
            };
            let mut kept = VecDeque::new();
            while let Some(ask) = asks.pop_front() {
                // A secret, a plan or a question waits for the owner's own answer: a new mode or
                // rule never settles it.
                if ask.permission.is_secret()
                    || ask.permission.is_plan()
                    || ask.permission.is_question()
                {
                    kept.push_back(ask);
                    continue;
                }
                match permissions::decide(&ask.call, mode, &rules, &bounds) {
                    permissions::Verdict::Allow => answered.push((ask, Answer::Allow)),
                    permissions::Verdict::Deny(why) => answered.push((ask, Answer::Deny(why))),
                    permissions::Verdict::Ask => kept.push_back(ask),
                }
            }
            *asks = kept;
        }
        if answered.is_empty() {
            return;
        }
        let step = answered.first().map(|(ask, answer)| match answer {
            Answer::Allow | Answer::AllowWith(_) => format!("Allowed: {}", ask.permission.title),
            Answer::Deny(_) => format!("Refused: {}", ask.permission.title),
        });
        for (ask, answer) in answered {
            let refused = match &answer {
                Answer::Allow | Answer::AllowWith(_) => None,
                Answer::Deny(why) => Some(why.as_str()),
            };
            self.answered(id, &ask.call, &worktree, refused);
            self.note(PermissionNote::Settled {
                job: id.to_owned(),
                permission: ask.permission.id.clone(),
                answer: None,
            });
            let _ = ask.reply.send(answer);
        }
        self.show(id, step);
    }

    /// The owner answers the job's waiting permission: allow it once, always (a rule for the
    /// repository), or no, with their words for the agent.
    pub fn answer_permission(
        self: &Arc<Self>,
        id: &str,
        permission: &str,
        decision: Decision,
        message: Option<&str>,
    ) -> Result<Job, PermissionRefused> {
        self.answer_permission_from(id, permission, decision, message, Source::Owner)
    }

    /// [`Jobs::answer_permission`], with whose words `message` is: a no in anyone's but the owner's
    /// is never told to the agent as theirs (PERMISSIONS-11).
    pub fn answer_permission_from(
        self: &Arc<Self>,
        id: &str,
        permission: &str,
        decision: Decision,
        message: Option<&str>,
        source: Source,
    ) -> Result<Job, PermissionRefused> {
        let job = self.get(id).ok_or(PermissionRefused::NoJob)?;
        let words = message.map(str::trim).filter(|w| !w.is_empty());
        if words.is_some_and(|w| w.chars().count() > 2000) {
            return Err(PermissionRefused::Refused(
                "Tell the agent something of up to 2,000 characters.".into(),
            ));
        }
        if let Some(answered) = self.answer_parked(id, permission, decision, words, source) {
            return answered;
        }
        let asked = self.lock().asks.get(id).is_some_and(|asks| {
            asks.front()
                .is_some_and(|a| a.permission.id == permission && a.permission.is_question())
        });
        let kept = self.kept_ask(id).is_some_and(|p| p.id == permission);
        // A question's no with words of the owner's own (the Mac's and the Watch's typed reply)
        // is its answer, not a refusal (#858).
        if decision == Decision::Deny
            && let Some(words) = words
            && (asked || kept)
        {
            return self.answer_question(id, permission, words);
        }
        // A question a restart kept: a no declines it; nothing else but an answer does.
        if kept {
            return match decision {
                Decision::Deny => self
                    .answer_kept_question(id, permission, None, false)
                    .unwrap_or_else(|| {
                        Err(PermissionRefused::NotWaiting(
                            "That question isn't waiting any more.".into(),
                        ))
                    }),
                _ => Err(PermissionRefused::Refused(QUESTION_NEEDS_AN_ANSWER.into())),
            };
        }
        let ask = {
            let mut state = self.lock();
            let asks = state.asks.get_mut(id);
            let Some(asks) =
                asks.filter(|a| a.front().is_some_and(|a| a.permission.id == permission))
            else {
                return Err(PermissionRefused::NotWaiting(
                    "That permission isn't waiting any more.".into(),
                ));
            };
            if decision == Decision::Always
                && asks.front().is_some_and(|a| a.permission.rule.is_none())
            {
                return Err(PermissionRefused::Refused(
                    "This one has no rule to save: allow it once instead.".into(),
                ));
            }
            // A question is answered with an option or words (`answer_question`); a no declines it.
            if decision != Decision::Deny
                && asks.front().is_some_and(|a| a.permission.is_question())
            {
                return Err(PermissionRefused::Refused(QUESTION_NEEDS_AN_ANSWER.into()));
            }
            asks.pop_front().expect("one waits")
        };
        let plan = ask.permission.is_plan();
        let (said, answer) = owners_answer(&ask.permission, decision, words, source);
        // An approved plan goes on in the owner's default mode ([`Jobs::approved_mode`]), before
        // the agent's next call is judged.
        if plan && matches!(answer, Answer::Allow) {
            let approved = self.approved_mode();
            self.update(id, |job| job.mode = Some(approved.as_str().to_owned()));
        }
        // Its item settles, and its words are noted, before the agent hears the answer (and asks
        // its next).
        self.note(PermissionNote::Settled {
            job: id.to_owned(),
            permission: permission.to_owned(),
            answer: Some(said),
        });
        if let Some(words) = words {
            self.update(id, |job| {
                job.owner_spoke = true;
                job.notes
                    .get_or_insert_with(Vec::new)
                    .push(words.to_owned());
            });
        }
        // The owner's own yes opens the sites it names (a rule's or a mode's never does).
        if matches!(answer, Answer::Allow)
            && let Some(egress) = self.egress.get()
        {
            let bounds = self.bounds(&job, &self.worktree_path(id));
            egress.let_through(id, &self.sites_opened(&ask.call, &bounds));
        }
        if let Some(owner) = ask.owner {
            let _ = owner.send(());
        }
        let _ = ask.reply.send(answer);
        if decision == Decision::Always
            && let Some(rule) = &ask.permission.rule
        {
            self.save_rule(rule, &job.repo);
        }
        self.show(id, Some(answered_step(&ask.permission, decision)));
        Ok(self.get(id).unwrap_or(job))
    }

    /// The owner answers the call job `id` was parked for ([`Jobs::park`]), its card `permission`:
    /// the card settles, and the job is queued again, its session picked up with the answer
    /// ([`permissions::after_park`]); an allow lets the resumed agent's same call through once
    /// ([`Jobs::rule_on`]), an always saves its rule too. `None` when no call of the job's is
    /// parked on that card (a parked question is a kept one: [`Jobs::answer_kept_question`]).
    fn answer_parked(
        self: &Arc<Self>,
        id: &str,
        permission: &str,
        decision: Decision,
        words: Option<&str>,
        source: Source,
    ) -> Option<Result<Job, PermissionRefused>> {
        let approved = self.approved_mode();
        let (card, said) = {
            let mut state = self.lock();
            let job = state
                .jobs
                .get_mut(id)
                .filter(|j| j.status == Status::Running)?;
            let card = job.permission.clone().filter(|p| p.id == permission)?;
            let parked = job.parked.as_mut().filter(|p| !p.answered)?;
            if decision == Decision::Always && card.rule.is_none() {
                return Some(Err(PermissionRefused::Refused(
                    "This one has no rule to save: allow it once instead.".into(),
                )));
            }
            let (said, answer) = owners_answer(&card, decision, words, source);
            job.heard.push(permissions::after_park(parked, &answer));
            // Kept until its run ends: that run must pick its session up (`acp::Options`), and
            // an allow lets the same call through once.
            parked.answered = true;
            parked.allowed = matches!(answer, Answer::Allow);
            // An approved plan goes on in the owner's default mode, as a live one does.
            if parked.allowed && card.is_plan() {
                job.mode = Some(approved.as_str().to_owned());
            }
            if let Some(words) = words {
                job.owner_spoke = true;
                job.notes
                    .get_or_insert_with(Vec::new)
                    .push(words.to_owned());
            }
            job.permission = None;
            job.status = Status::Queued;
            job.phase = Some(Phase::Prepare);
            job.step = answered_step(&card, decision);
            (card, said)
        };
        let job = self.update(id, |_| {})?;
        self.note(PermissionNote::Settled {
            job: id.to_owned(),
            permission: permission.to_owned(),
            answer: Some(said),
        });
        if decision == Decision::Always
            && let Some(rule) = &card.rule
        {
            self.save_rule(rule, &job.repo);
        }
        self.schedule();
        Some(Ok(self.get(id).unwrap_or(job)))
    }

    /// Parks job `id` once its agent's call has waited for the owner for `parkAfter`, so an
    /// unanswered card holds no slot (docs/runner-api.md, "Parked"): the call becomes the card the
    /// job keeps showing, `running` with no agent (a question's as a restart keeps it, #858; a
    /// permission's or plan's in `parked`), its inbox item stays open, and the job's other waits
    /// go. Its worker ends the agent as a shutdown does, its session kept, and gives its slot
    /// back. The owner's answer queues it again ([`Jobs::answer_parked`],
    /// [`Jobs::answer_kept_question`]). Nothing is answered for them.
    pub(super) fn park(&self, id: &str) {
        let (card, shown, others) = {
            let mut state = self.lock();
            let due = state.asks.get(id).and_then(|asks| {
                asks.iter().position(|a| {
                    a.since
                        .is_some_and(|since| since.elapsed() >= self.config.park_after)
                })
            });
            let Some(due) = due else { return };
            let Some(worker) = state.workers.get(id) else {
                return;
            };
            let _ = worker.send(Signal::Shutdown);
            let mut asks = state.asks.remove(id).unwrap_or_default();
            let ask = asks.remove(due).expect("it waits");
            let others: Vec<String> = asks.iter().map(|a| a.permission.id.clone()).collect();
            let call = ask.call.clone();
            let (card, input, answers) = (
                ask.permission.clone(),
                ask.input.clone(),
                ask.answers.clone(),
            );
            // Every reply stays open until the worker ends: the agent's wait ends on its stop,
            // never on a no.
            state
                .held
                .insert(id.to_owned(), asks.into_iter().chain([ask]).collect());
            let Some(job) = state.jobs.get_mut(id) else {
                return;
            };
            let shown = job.permission.as_ref().map(|p| p.id.clone());
            if card.is_question() {
                job.question_call = Some(permissions::QuestionCall {
                    questions: call.questions,
                    answers,
                    ended_turn: false,
                    parked: true,
                });
            } else {
                job.parked = Some(permissions::Parked {
                    tool: call.tool,
                    input,
                    answered: false,
                    allowed: false,
                    ended_turn: false,
                });
            }
            job.permission = Some(card.clone());
            job.changed(format!(
                "Parked by obid: its agent waited {} for your answer, so obid ended it to free its slot. Your answer picks its session up.",
                spell(self.config.park_after)
            ));
            (card, shown, others)
        };
        // What the agent would reach on the way out waits on no card of the job's.
        if let Some(egress) = self.egress.get() {
            egress.close(id);
        }
        for permission in others {
            self.note(PermissionNote::Settled {
                job: id.to_owned(),
                permission,
                answer: None,
            });
        }
        let Some(job) = self.update(id, |_| {}) else {
            return;
        };
        if shown.as_deref() != Some(card.id.as_str()) {
            self.note(PermissionNote::Waiting(Box::new(job), card));
        }
        tracing::info!("job {id}: parked: its agent's call waited for the owner");
    }

    /// The owner answers the agent's waiting question (`AskUserQuestion`, #858): one of its
    /// options by its label, several for a multiple choice, or their own words. With more of the
    /// call's questions left, the next takes its place on a card of its own; after the last, the
    /// agent hears every answer as the tool's (`updatedInput` with `answers`).
    pub fn answer_question(
        self: &Arc<Self>,
        id: &str,
        permission: &str,
        answer: &str,
    ) -> Result<Job, PermissionRefused> {
        self.answer_question_as(id, permission, answer, false)
    }

    /// [`Jobs::answer_question`]; `signed`: the owner's app signed the answer (their typed words,
    /// a Confirm or a device-signed inbox answer), so the question and it are kept with their own
    /// words for Oby's review ([`Job::said`]). An answer no app signed is kept nowhere.
    pub fn answer_question_as(
        self: &Arc<Self>,
        id: &str,
        permission: &str,
        answer: &str,
        signed: bool,
    ) -> Result<Job, PermissionRefused> {
        let job = self.get(id).ok_or(PermissionRefused::NoJob)?;
        // The owner's words reach the agent without the vault's secrets, or not at all.
        let answer = crate::vault::for_agent(answer.trim())
            .map(|clean| crate::secrets::known().redact(&clean))
            .map_err(|why| {
                PermissionRefused::Refused(format!("Couldn't pass your answer on: {why}"))
            })?;
        let answer = answer.as_str();
        self.update(id, |job| job.owner_spoke = true);
        if let Some(answered) = self.answer_kept_question(id, permission, Some(answer), signed) {
            return answered;
        }
        let asked;
        let (settled, done) = {
            let mut state = self.lock();
            let Some(ask) = state
                .asks
                .get_mut(id)
                .and_then(|asks| asks.front_mut())
                .filter(|a| a.permission.id == permission)
            else {
                return Err(PermissionRefused::NotWaiting(
                    "That question isn't waiting any more.".into(),
                ));
            };
            let Some(asking) = ask.permission.question.clone() else {
                return Err(PermissionRefused::Refused(
                    "That isn't a question: allow it or say no.".into(),
                ));
            };
            let Some(said) = permissions::question_answer(&asking.question, answer) else {
                return Err(PermissionRefused::Refused(QUESTION_NEEDS_AN_ANSWER.into()));
            };
            ask.answers.push(said.clone());
            let next = asking.index + 1;
            asked = asking.question.question.clone();
            let settled = (ask.permission.id.clone(), said);
            if next < ask.call.questions.len() {
                ask.permission =
                    permissions::question_card(&ask.call.questions, &permissions::new_id(), next);
                (settled, None)
            } else {
                let ask = state
                    .asks
                    .get_mut(id)
                    .and_then(VecDeque::pop_front)
                    .expect("one waits");
                (settled, Some(ask))
            }
        };
        let (permission, said) = settled;
        if signed {
            self.owner_said(id, answered(&asked, &said));
        }
        self.note(PermissionNote::Settled {
            job: id.to_owned(),
            permission,
            answer: Some(said),
        });
        let step = match done {
            Some(ask) => {
                let updated =
                    permissions::answered_input(&ask.input, &ask.call.questions, &ask.answers);
                let _ = ask.reply.send(Answer::AllowWith(updated));
                "You answered its question".to_owned()
            }
            None => "Waiting for your answer".to_owned(),
        };
        self.show(id, Some(step));
        Ok(self.get(id).unwrap_or(job))
    }

    /// The question a restart kept on job `id` (#858): `running`, its card a question with its
    /// call, and no agent of its waiting on it.
    pub(super) fn kept_ask(&self, id: &str) -> Option<Permission> {
        let state = self.lock();
        if !waits_with_no_agent(&state, id) {
            return None;
        }
        let job = state.jobs.get(id).filter(|j| j.status == Status::Running)?;
        kept_question(job).cloned()
    }

    /// The owner answers a question a restart kept (#858), or declines it (`answer` none): the
    /// next of its call's questions waits in its place, or, after the last or a no, the job's
    /// session is picked up as after any restart, its agent told what it asked and what the owner
    /// said ([`permissions::answered_later`]). `None` when no kept question has that id.
    fn answer_kept_question(
        self: &Arc<Self>,
        id: &str,
        permission: &str,
        answer: Option<&str>,
        signed: bool,
    ) -> Option<Result<Job, PermissionRefused>> {
        let mut qa = None;
        let (next, said, picked_up) = {
            let mut state = self.lock();
            if !waits_with_no_agent(&state, id) {
                return None;
            }
            let job = state
                .jobs
                .get_mut(id)
                .filter(|j| j.status == Status::Running)?;
            let asking = kept_question(job)
                .filter(|p| p.id == permission)?
                .question
                .clone()?;
            let call = job.question_call.as_mut()?;
            let mut next = None;
            let said = match answer {
                Some(answer) => {
                    let Some(said) = permissions::question_answer(&asking.question, answer) else {
                        return Some(Err(PermissionRefused::Refused(
                            QUESTION_NEEDS_AN_ANSWER.into(),
                        )));
                    };
                    call.answers.push(said.clone());
                    qa = signed.then(|| answered(&asking.question.question, &said));
                    next = (asking.index + 1 < call.questions.len()).then(|| {
                        permissions::question_card(
                            &call.questions,
                            &permissions::new_id(),
                            asking.index + 1,
                        )
                    });
                    said
                }
                None => permissions::DENY.to_owned(),
            };
            let picked_up = next.is_none();
            if picked_up {
                job.heard
                    .push(permissions::answered_later(call, answer.is_none()));
                job.permission = None;
                job.question_call = None;
                job.status = Status::Queued;
                job.phase = Some(Phase::Prepare);
                job.step = if answer.is_some() {
                    "You answered its question"
                } else {
                    "You didn't answer its question"
                }
                .into();
            } else {
                job.permission = next.clone();
            }
            (next, said, picked_up)
        };
        if let Some(qa) = qa {
            self.owner_said(id, qa);
        }
        let job = self.update(id, |_| {})?;
        self.note(PermissionNote::Settled {
            job: id.to_owned(),
            permission: permission.to_owned(),
            answer: Some(said),
        });
        if let Some(next) = next {
            self.note(PermissionNote::Waiting(Box::new(job.clone()), next));
        }
        if picked_up {
            self.schedule();
        }
        Some(Ok(self.get(id).unwrap_or(job)))
    }

    /// Job `id`'s agent asks the owner with `oby_ask_owner` (a Codex or Cursor job): what the tool
    /// answers at once. The question is asked once its turn ends ([`End::Asked`]); it can only ask,
    /// never answer or allow anything. A second one in the same turn takes the first's place.
    pub fn ask_owner(&self, id: &str, arguments: &Value) -> Result<&'static str, String> {
        let job = self.get(id).ok_or("The job is gone.")?;
        if !asks_through_oby(&job) {
            return Err(permissions::NO_ONE_TO_ASK.into());
        }
        let call = permissions::ask_owner_call(arguments).ok_or(
            "Ask one question, its whole text in `question` (up to 1,000 characters), with `options` (each a `label`, and a `description` if it helps) when there are a few clear choices.",
        )?;
        self.lock().asked.insert(id.to_owned(), call);
        Ok(
            "Asked the owner. End your turn now with a one-line summary; Oby will resume this session with the answer.",
        )
    }

    /// Changes how much the job asks, from now on: the prompts waiting now that the new mode
    /// allows are answered. A finished job keeps the mode for when it runs again. A review job stays
    /// in Plan.
    pub fn set_mode(&self, id: &str, mode: Mode) -> Option<Job> {
        self.update(id, |job| {
            if !job.read_only() {
                job.mode = Some(mode.as_str().to_owned());
            }
        })?;
        // What the old mode let its agent reach is decided again (crate::egress).
        if let Some(egress) = self.egress.get() {
            egress.forget(Some(id));
        }
        self.reconsider(id);
        self.get(id)
    }

    /// The job's waiting prompts go: nobody will answer them (it was stopped, or ended), a
    /// question a restart kept among them. obid shutting down (`shutdown`) keeps the question the
    /// job shows, and its inbox item, for the next start to keep waiting (#858), and so does a
    /// worker ending for [`Jobs::park`] with the call it parked the job for.
    pub(super) fn drop_asks(&self, id: &str, shutdown: bool) {
        let dropped = {
            let mut state = self.lock();
            // What its agent asked with `oby_ask_owner` in a turn that didn't end well goes too.
            state.asked.remove(id);
            // A parked job's agent ended: what it waited on goes, settled already or kept.
            state.held.remove(id);
            state.asks.remove(id).unwrap_or_default()
        };
        let job = self.get(id);
        let shown = job.as_ref().and_then(|j| j.permission.clone());
        // The call obid parked the job for waits with no agent, as a question does (`Jobs::park`).
        let parked = job
            .as_ref()
            .and_then(|j| j.parked.as_ref())
            .is_some_and(|p| !p.answered);
        let kept = shown
            .as_ref()
            .filter(|p| shutdown && (p.is_question() || parked));
        let mut settled: Vec<&str> = dropped.iter().map(|a| a.permission.id.as_str()).collect();
        // A question a restart kept has no ask of its own.
        if let Some(p) = &shown
            && !settled.contains(&p.id.as_str())
        {
            settled.push(&p.id);
        }
        for permission in settled {
            if kept.is_some_and(|k| k.id == permission) {
                continue;
            }
            self.note(PermissionNote::Settled {
                job: id.to_owned(),
                permission: permission.to_owned(),
                answer: None,
            });
        }
        // It ended (a shutdown isn't an end): a parked call the owner answered goes.
        let ended = !shutdown && job.is_some_and(|j| j.parked.is_some());
        if shown.is_some() && kept.is_none() || ended {
            self.update(id, |job| {
                job.permission = None;
                job.question_call = None;
                if !shutdown {
                    job.parked = None;
                }
            });
        }
    }
}
