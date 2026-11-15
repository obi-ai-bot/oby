//! Routing: which tool and account a job runs on, an account at its usage limit or weekly
//! reserve, and the waits for an account with room and for the owner's yes to a ChatGPT login
//! (#872, CHATGPT-20). It never writes a job's worktree or branch.

use super::*;

impl Jobs {
    /// The configuration of `tool`'s adapter, when obid has one, with its accounts as they are now
    /// (crate::accounts).
    pub(super) fn agent_config(&self, tool: Tool) -> Option<crate::config::Agent> {
        let agents = self.config.agents.get();
        match tool {
            Tool::Claude => agents.claude,
            Tool::Codex => agents.codex,
            Tool::Cursor => agents.cursor,
        }
    }

    /// `tool`'s accounts: the configured ones, or its own default login (Claude Code's and
    /// Codex's; a Cursor account always has a folder of its own).
    pub(crate) fn accounts(&self, tool: Tool) -> Vec<crate::config::Account> {
        let Some(agent) = self.agent_config(tool) else {
            return Vec::new();
        };
        match tool {
            Tool::Claude => claude_accounts(&agent),
            Tool::Codex if agent.accounts.is_empty() => vec![crate::config::Account {
                id: "codex".into(),
                name: "Codex".into(),
                config_dir: None,
                payer: None,
            }],
            _ => agent.accounts,
        }
    }

    /// The accounts of `tool` Oby thinks with (the brain and conversations), best
    /// first (#874): signed in, not being signed in or removed, and not marked full; those with
    /// room first (a weekly reserve holds no turn, LIMITS-20), in the tool's order, then the rest,
    /// the least of their 5-hour window used first.
    pub fn thinking_accounts(&self, tool: Tool) -> Vec<crate::config::Account> {
        let home = login_home();
        let now = obi_protocol::time::now();
        let mut ranked: Vec<(u8, f64, usize, crate::config::Account)> = self
            .accounts(tool)
            .into_iter()
            .enumerate()
            .filter(|(_, a)| !self.being_changed(&a.id) && signed_in(tool, a, home.as_deref()))
            .filter_map(|(at, account)| {
                let usage = self.usage.get(&account.id);
                usage.full_until_at(now).is_none().then_some(())?;
                let five = usage.five_hour_at(now).unwrap_or(0.0);
                let week = usage.seven_day_at(now).unwrap_or(0.0);
                Some(match usage.held_for(now, false) {
                    // Cursor's figure holds nothing: it only puts the emptier first (USAGE-9).
                    None if usage.advisory => (0, week, at, account),
                    None => (0, 0.0, at, account),
                    Some(_) => (2, five, at, account),
                })
            })
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        ranked
            .into_iter()
            .map(|(_, _, _, account)| account)
            .collect()
    }

    /// Whether `account` takes `background` work now (crate::limits::holds).
    pub fn has_room(&self, account: &str, background: bool) -> bool {
        let work = Work::new(None, None, background);
        (self.holds().held(account, &work, obi_protocol::time::now())).is_none()
    }

    /// Whether `account` takes work nobody's watching ([`Job::background`], CHATGPT-20): any login
    /// does, but a ChatGPT one only with the owner's yes, signed.
    pub(crate) fn takes_background(&self, account: &crate::config::Account) -> bool {
        crate::chatgpt::marker_of(account).is_none()
            || crate::settings::load(&self.config.home)
                .background_consent
                .get(&account.id)
                .is_some_and(|answer| answer.given)
    }

    /// Work nobody's watching waits for the owner's yes to run on this login: a signed-out one
    /// doesn't: it's its sign-in that's missing, as for any job, and the job says that.
    pub(super) fn waits_for_yes(&self, tool: Tool, account: &crate::config::Account) -> bool {
        !self.takes_background(account) && signed_in(tool, account, login_home().as_deref())
    }

    /// Whether `job` is work nobody's watching, on a login that waits for the owner's yes.
    pub(super) fn off_its_login(&self, job: &Job) -> bool {
        job.background()
            && crate::accounts::find(&self.config.agents.get(), &job.account)
                .is_some_and(|(tool, account)| self.waits_for_yes(tool, &account))
    }

    /// The owner answered about a ChatGPT login (`POST /v1/accounts/{id}/background`). A yes: what
    /// waited for it starts. A no: work nobody's watching comes off it now: what's running stops,
    /// and [`Jobs::schedule`] looks at what's queued, under the lock: the settings are written by
    /// now, so whatever it starts from here on it has checked, and what it started before is on
    /// this list.
    pub fn background_answered(self: &Arc<Self>, account: &str, given: bool) {
        if !given {
            let stopping: Vec<String> = {
                let state = self.lock();
                state
                    .jobs
                    .values()
                    .filter(|j| j.account == account && j.background() && j.status.is_active())
                    .filter(|j| state.workers.contains_key(&j.id))
                    .map(|j| j.id.clone())
                    .collect()
            };
            for id in stopping {
                self.stop(&id, BACKGROUND_OFF);
                self.update(&id, |job| job.reason = Some(BACKGROUND_OFF.to_owned()));
            }
        }
        self.schedule();
    }

    /// `tool`'s accounts that are [`signed_in`].
    pub fn signed_in_accounts(&self, tool: Tool) -> Vec<crate::config::Account> {
        let home = login_home();
        self.accounts(tool)
            .into_iter()
            .filter(|a| signed_in(tool, a, home.as_deref()))
            .collect()
    }

    /// Why `tool` has no [`signed_in`] account for conversations: a login that's there but can't
    /// run says what to do about it ([`login_problem`]: Codex's, run codex once), else that none is.
    pub fn none_signed_in(&self, tool: Tool) -> String {
        let home = login_home();
        self.accounts(tool)
            .iter()
            .find_map(|a| login_problem(tool, a, home.as_deref()))
            .unwrap_or_else(|| format!("no {} account is signed in for conversations", tool.name()))
    }

    /// The owner's preferred tool only while it has a signed-in account with room. The caller
    /// retains the planner's automatic choice when this returns none.
    /// The account new tasks go to (`defaultAccount`, #1573), only while routing would put a task
    /// on it: signed in, with room, not being changed. What the brain is told, and what
    /// [`Jobs::preferred_agent`] takes first.
    pub(crate) fn preferred_account(&self) -> Option<(Tool, String)> {
        let id = crate::settings::load(&self.config.home).default_account?;
        let (tool, account) = self.pick(None, None).ok()?;
        (account.id == id && signed_in(tool, &account, login_home().as_deref()))
            .then_some((tool, id))
    }

    /// The account new tasks go to comes first ([`Jobs::preferred_account`]): its tool.
    pub(crate) fn preferred_agent(&self) -> Option<String> {
        if let Some((tool, _)) = self.preferred_account() {
            return Some(tool.id().into());
        }
        let agent = crate::settings::load(&self.config.home).default_agent;
        if agent == "auto" {
            return None;
        }
        let (tool, account) = self.pick(Some(&agent), None).ok()?;
        signed_in(tool, &account, login_home().as_deref()).then_some(agent)
    }

    /// The tool and account a job runs on: the tool named (`claude`, `codex`, `cursor`; `auto` or
    /// none for any), and the account named (by id or name), else the signed-in one with the most
    /// room ([`Jobs::by_room`], USAGE-11; `defaultAccount` first, #1573), else the first account.
    pub(super) fn pick(
        &self,
        agent: Option<&str>,
        named: Option<&str>,
    ) -> Result<(Tool, crate::config::Account), Refused> {
        self.pick_for(agent, named, &Work::new(None, None, false))
    }

    /// [`Jobs::pick`] for `work`, `background` when nobody's watching it: routing then passes over a
    /// login that doesn't take that ([`Jobs::takes_background`]), as it does one without room for
    /// it (LIMITS-21). An account named is the caller's to look at.
    pub(super) fn pick_for(
        &self,
        agent: Option<&str>,
        named: Option<&str>,
        work: &Work,
    ) -> Result<(Tool, crate::config::Account), Refused> {
        let tools: Vec<Tool> = match agent
            .map(str::trim)
            .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case("auto"))
        {
            None => Tool::ALL
                .into_iter()
                .filter(|t| self.agent_config(*t).is_some())
                .collect(),
            Some(id) => {
                let tool = Tool::from_id(id).ok_or_else(|| {
                    Refused(format!("agent is claude, codex or cursor, not {id}."))
                })?;
                if self.agent_config(tool).is_none() {
                    return Err(Refused(format!(
                        "obid has no {} set up: its configuration names no agents.{id}.",
                        tool.name()
                    )));
                }
                vec![tool]
            }
        };
        if tools.is_empty() {
            return Err(Refused(
                "obid has no agent set up: its configuration names no agents.claude, agents.codex or agents.cursor.".into(),
            ));
        }
        let named = named
            .map(str::trim)
            .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("auto"));
        let preferred = named
            .is_none()
            .then(|| crate::settings::load(&self.config.home).default_account)
            .flatten();
        let mut tools = tools;
        if let Some(id) = &preferred {
            tools.sort_by_key(|t| !self.accounts(*t).iter().any(|a| &a.id == id));
        }
        let (home, now) = (login_home(), obi_protocol::time::now());
        let mut first = None;
        let (mut held, mut ready) = (Vec::new(), Vec::new());
        let mut not_yet = None;
        for tool in &tools {
            let mut accounts = self.accounts(*tool);
            if let Some(id) = &preferred {
                accounts.sort_by_key(|a| &a.id != id);
            }
            if let Some(changing) = named.and_then(|named| {
                accounts
                    .iter()
                    .find(|a| (a.id == named || a.name == named) && self.being_changed(&a.id))
            }) {
                return Err(Refused(format!(
                    "{} is being signed in or removed: try again in a moment.",
                    changing.name
                )));
            }
            // Routing skips an account being signed in or removed.
            accounts.retain(|a| !self.being_changed(&a.id));
            match named {
                // Routing skips an account that isn't signed in (#626), and one without room.
                None => {
                    for account in accounts {
                        if !signed_in(*tool, &account, home.as_deref()) {
                            first.get_or_insert((*tool, account));
                        } else if work.background && !self.takes_background(&account) {
                            not_yet.get_or_insert(account.name);
                        } else if let Some((why, _)) = self.holds().held(&account.id, work, now) {
                            held.push(format!("{} {why}", account.name));
                        } else {
                            ready.push((*tool, account));
                        }
                    }
                }
                Some(named) => {
                    if let Some(account) = accounts
                        .into_iter()
                        .find(|a| a.id == named || a.name == named)
                    {
                        return Ok((*tool, account));
                    }
                }
            }
        }
        if let Some(found) = self.by_room(ready, preferred.as_deref()).into_iter().next() {
            return Ok(found);
        }
        if !held.is_empty() {
            return Err(Refused(format!(
                "No account has room: {}.",
                held.join("; ")
            )));
        }
        if let Some(name) = not_yet {
            return Err(Refused(format!("I need {}.", consent_words(&name))));
        }
        // None is signed in: the first, whose tool then says why it can't run.
        if let Some(found) = first {
            return Ok(found);
        }
        Err(Refused(match (named, tools.as_slice()) {
            (Some(named), _) => format!("There's no account {named}."),
            (None, [tool]) => format!(
                "obid has no {} account: agents.{}.accounts names none.",
                tool.name(),
                tool.id()
            ),
            (None, _) => "obid has no account to run the job on.".into(),
        }))
    }

    /// Whether obid moves work past an account's usage limit to another account with room by
    /// itself (`accountFailover`, `PUT /v1/settings/jobs`; [`crate::settings::ACCOUNT_FAILOVER`]
    /// when the owner didn't say).
    pub fn account_failover(&self) -> bool {
        crate::settings::load(&self.config.home).account_failover()
    }

    /// `account` hit its usage limit: it's marked full until `reset` (an hour, when its tool named
    /// none), whatever the tool said, so routing passes it by until then ([`Holds::apply`]).
    pub(crate) fn mark_full(&self, account: &str, reset: Option<i64>, now: i64) {
        self.holds().apply(account, reset, None, now);
    }

    /// When no account takes `background` work only because of weekly reserves (#872, LIMITS-20):
    /// the first of those of `agent`'s tool (any for none or `auto`), under 95% of both windows,
    /// and what the job waits for, naming every one of them for the apps' "Use … anyway".
    pub(super) fn in_reserve(
        &self,
        agent: Option<&str>,
        background: bool,
    ) -> Option<(Tool, crate::config::Account, Option<Waiting>)> {
        let asked = agent
            .map(str::trim)
            .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case("auto"));
        let tools: Vec<Tool> = match asked {
            None => Tool::ALL.to_vec(),
            Some(id) => vec![Tool::from_id(id)?],
        };
        let (home, now) = (login_home(), obi_protocol::time::now());
        let held: Vec<(Tool, crate::config::Account)> = tools
            .into_iter()
            .filter(|t| self.agent_config(*t).is_some())
            .flat_map(|tool| self.accounts(tool).into_iter().map(move |a| (tool, a)))
            .filter(|(tool, a)| {
                !self.being_changed(&a.id)
                    && signed_in(*tool, a, home.as_deref())
                    && self.usage.get(&a.id).only_reserved(now)
                    && (background && self.takes_background(a))
            })
            .collect();
        let (tool, first) = held.first().cloned()?;
        let names: Vec<&str> = held.iter().map(|(_, a)| a.name.as_str()).collect();
        let line = match names.as_slice() {
            [one] => format!("{one} is in its weekly reserve"),
            [rest @ .., last] => {
                format!("{} and {last} are in their weekly reserve", rest.join(", "))
            }
            [] => unreachable!(),
        };
        Some((
            tool,
            first,
            Some(Waiting {
                kind: "limit".into(),
                text: format!("an account with room: {line}"),
                reserved: Some(held.iter().map(|(_, a)| a.id.clone()).collect()),
                tool: asked.map(str::to_owned),
            }),
        ))
    }

    /// The first ChatGPT login of `agent`'s tool the owner hasn't said yes to: where work nobody's
    /// watching waits when nothing else can take it (CHATGPT-20), and what it waits for.
    pub(super) fn first_not_yet(
        &self,
        agent: Option<&str>,
    ) -> Option<(Tool, crate::config::Account, Option<Waiting>)> {
        let asked = agent
            .map(str::trim)
            .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case("auto"));
        if asked.is_some_and(|a| a != Tool::Codex.id()) {
            return None;
        }
        let home = login_home();
        let account = self.accounts(Tool::Codex).into_iter().find(|a| {
            !self.being_changed(&a.id)
                && signed_in(Tool::Codex, a, home.as_deref())
                && !self.takes_background(a)
        })?;
        let wait = consent_wait(&account, asked);
        Some((Tool::Codex, account, Some(wait)))
    }

    /// Job `id`, about to start with nobody watching on a login with no yes ([`Jobs::schedule`]):
    /// the owner took it back, or its next round came after they did. It waits, and
    /// [`Jobs::room_for_waiting`] looks for another account. A review stays on its account, so it's
    /// stopped, and [`Jobs::reviewed`] starts another elsewhere.
    pub(super) fn hold_for_consent(&self, id: &str) {
        let Some(job) = self.get(id) else { return };
        if job.review_of.is_some() || job.obi.is_some() {
            self.stop(id, BACKGROUND_OFF);
            self.update(id, |job| job.reason = Some(BACKGROUND_OFF.to_owned()));
            return;
        }
        let Some((_, account)) = crate::accounts::find(&self.config.agents.get(), &job.account)
        else {
            return;
        };
        let wait = consent_wait(&account, Some(&job.agent));
        self.update(id, |job| {
            job.step = room_step(&wait);
            job.waiting = Some(wait);
        });
    }

    /// Jobs waiting for an account with room (#872) that has one now take it: an account in
    /// their list whose reserve the owner lifted (Use anyway), else the first of their tool with
    /// room. Then they wait for a slot like any queued job.
    pub(super) fn room_for_waiting(&self) {
        let waiting: Vec<Job> = self
            .lock()
            .jobs
            .values()
            .filter(|j| {
                j.status == Status::Queued && j.waiting.as_ref().is_some_and(Waiting::for_room)
            })
            .cloned()
            .collect();
        for job in waiting {
            let background = job.background();
            let listed = job
                .waiting
                .as_ref()
                .and_then(|w| w.reserved.clone())
                .unwrap_or_default();
            let lifted = listed.iter().find_map(|id| {
                Tool::ALL.into_iter().find_map(|tool| {
                    self.accounts(tool)
                        .into_iter()
                        .find(|a| {
                            &a.id == id
                                && self.has_room(id, background)
                                && (!background || self.takes_background(a))
                        })
                        .map(|a| (tool, a))
                })
            });
            // The tool it asked for, else any: never narrowed by where it waits now.
            let asked = job.waiting.as_ref().and_then(|w| w.tool.clone());
            let for_consent = job.waiting.as_ref().is_some_and(Waiting::for_consent);
            let on_its_own = job.named && for_consent;
            let was = crate::accounts::find(&self.config.agents.get(), &job.account);
            let found = if on_its_own {
                // The owner named it, so it goes nowhere else: it starts on their yes.
                was.clone().filter(|(tool, account)| {
                    !self.being_changed(&account.id) && !self.waits_for_yes(*tool, account)
                })
            } else {
                lifted.or_else(|| self.pick_for(asked.as_deref(), None, &Work::of(&job)).ok())
            };
            let Some((tool, account)) = found else {
                // What it waited on is gone, and nothing else takes it: nothing would ever start it.
                if for_consent && was.is_none() {
                    self.stop(&job.id, WAITED_ON_GONE);
                    self.update(&job.id, |job| {
                        job.reason = Some(WAITED_ON_GONE.to_owned());
                    });
                    continue;
                }
                // Still waiting: what holds it as it is now, on the first account that does (the
                // one Start now lifts).
                if !on_its_own
                    && let Some((tool, first, Some(now))) =
                        self.in_reserve(asked.as_deref(), background)
                    && (job.waiting.as_ref() != Some(&now) || job.account != first.id)
                {
                    let tuning = crate::settings::load(&self.config.home).tuning;
                    let offered = self.model_lists.to_check(tool, &first.id);
                    self.update(&job.id, |job| {
                        on_tool(job, tool);
                        job.account = first.id.clone();
                        tune(job, &tuning, &offered);
                        job.step = room_step(&now);
                        job.waiting = Some(now);
                    });
                }
                continue;
            };
            let on = format!("{} · {}", tool.name(), account.name);
            let tuning = crate::settings::load(&self.config.home).tuning;
            let offered = self.model_lists.to_check(tool, &account.id);
            // It has run: its session and its login copy are the old account's (CHATGPT-23).
            let carried = job.session.is_some() && job.account != account.id;
            let from = was.map_or(job.account.clone(), |(tool, account)| {
                format!("{} · {}", tool.name(), account.name)
            });
            self.update(&job.id, |job| {
                if carried {
                    carry_over(
                        job,
                        tool,
                        &account,
                        &tuning,
                        &offered,
                        &from,
                        Carried {
                            because: "which couldn't go on there",
                            why: "It needs your OK to go on there".into(),
                            by: format!("as {from} needs your OK for work I run by myself"),
                        },
                    );
                } else {
                    on_tool(job, tool);
                    job.account = account.id.clone();
                    tune(job, &tuning, &offered);
                }
                job.waiting = None;
                job.step = "Starting".into();
                job.changed(format!("Started when an account had room: {on}"));
            });
            if carried {
                self.forget_moved_login(&job.id);
            }
        }
    }

    /// The owner lifted an account's weekly reserve (`POST /v1/accounts/{id}/reserve`): a job
    /// waiting for room may start on it now (#872).
    pub fn reserve_changed(self: &Arc<Self>) {
        self.schedule();
    }

    /// `POST /v1/jobs/{id}/start` (the apps' Start now, #872): a job waiting for an account with
    /// room starts on its own account, the first of those its weekly reserve held back, that
    /// reserve lifted as Use anyway lifts it. obid makes no other job wait to start.
    pub fn start_now(self: &Arc<Self>, id: &str) -> Option<Result<Job, Refused>> {
        let job = self.get(id)?;
        if job.status != Status::Queued || !job.waiting.as_ref().is_some_and(Waiting::for_room) {
            return Some(Err(Refused(
                "It isn't waiting to start: obid starts every other job as soon as it can.".into(),
            )));
        }
        if job.waiting.as_ref().is_some_and(Waiting::for_consent) {
            return Some(Err(Refused(
                "It's waiting for your OK to use that ChatGPT plan: you can allow it on your Mac, in Settings › Agents.".into(),
            )));
        }
        if let Err(e) = self.usage.lift(&job.account, true) {
            return Some(Err(Refused(format!("obid couldn't lift its reserve: {e}"))));
        }
        self.usage_changed.notify_one();
        self.schedule();
        self.get(id).map(Ok)
    }
}
