//! Application orchestration for the account-limit contract (#1563).
//!
//! The durable decision store is `crate::account_limits`; this adapter joins its decisions to
//! jobs, provider usage, and the scheduler. Clients only render and sign the offered choices.

use super::*;

/// How long after its reset (or its question, with none known) an approved Resume on an account
/// that has reported no usage since waits before it runs again anyway (#1645).
const UNREAD_RESUME: i64 = 30 * 60;

/// An approved Resume waiting for room.
const RESUME_WAIT: &str = "Waiting for the account's usage limit. Oby resumes this job once fresh usage confirms room, or 30 minutes after the reset when the account reports none.";

/// An approved Resume on a ChatGPT login: OpenAI's limit names no time, and neither does Oby.
const RESUME_WAIT_NO_TIME: &str = "Waiting for ChatGPT's usage limit. It doesn't say when it ends, so Oby tries again by itself, and you can check Manage usage.";

/// An approved Resume its account's weekly reserve alone holds.
const RESERVE_WAIT: &str = "Waiting for the account's weekly reserve: it's past 80% of its week. Lift its reserve to resume now.";

/// How long after a five-hour limit's reset its job resumes by itself, as Claude's desktop app
/// does (LIMITS-5). The tool's clock and the Mac's may differ.
const RESUME_AFTER_RESET: i64 = 90;

/// How long a job waits for a limit whose end nobody knows before its account is tried again.
const UNKNOWN_RESET: i64 = 60 * 60;

/// What cut its run, as the session that resumes hears it (`continue_message`).
const RESET_CAME: &str = "your account hit its usage limit, and that limit has reset";

/// What a card from before LIMITS-15 asked, while it's still shown.
const LIMIT_QUESTION: &str =
    "This account reached its usage limit. Choose what Oby should do with the affected jobs.";

/// A job's line while it waits for room (LIMITS-15), saying when its own account's limit ends.
/// Proposed words: Sangam's to change.
fn resuming_at(at: i64, zone: &jiff::tz::TimeZone) -> String {
    let time = jiff::Timestamp::from_second(at)
        .map(|t| {
            t.to_zoned(zone.clone())
                .strftime("%a %-I:%M %p")
                .to_string()
        })
        .unwrap_or_else(|_| "the reset".into());
    format!("Every account is at its limit: resuming when one has room, at the latest {time}")
}

/// `job` waits, `queued`, until `at` or an account with room (LIMITS-5, LIMITS-15): `cap`, as the
/// apps know it (their job decodes no other kind), with no slot, no card, and Stop as for any
/// queued job.
pub(super) fn wait_for_reset(job: &mut Job, at: i64) {
    let line = resuming_at(at, &jiff::tz::TimeZone::system());
    job.status = Status::Queued;
    job.resumes_at = Some(at);
    job.waiting = Some(Waiting {
        kind: "cap".into(),
        text: line.clone(),
        reserved: None,
        tool: None,
    });
    job.step = line;
}

/// A run a limit stopped starts its wait, counted (LIMITS-9), unless Stop came as the run ended:
/// nothing undoes it (LIMITS-6). Whether the owner asked for its round stays.
pub(super) fn start_wait(job: &mut Job, at: i64) {
    if !job.status.is_active() {
        return;
    }
    job.recovery.limit_resumes += 1;
    job.publish_step = None;
    wait_for_reset(job, at);
    job.changed(format!("A usage limit stopped it. {}", job.step));
}

/// Where a job a limit stopped goes by itself (LIMITS-15): the tool and account, where it was, and
/// what its line says.
pub(super) struct Failover(Tool, crate::config::Account, String, String);

impl Failover {
    /// `job` goes on there in a new session, the same round, as a resume does (LIMITS-12).
    pub(super) fn apply(&self, job: &mut Job, jobs: &Jobs) {
        let Failover(tool, account, from, line) = self;
        let by = "by itself (Account failover)".into();
        let said = Carried {
            because: "which reached its limit",
            why: line.clone(),
            by,
        };
        let tuning = crate::settings::load(&jobs.config.home).tuning;
        let offered = jobs.model_lists.to_check(*tool, &account.id);
        carry_over(job, *tool, account, &tuning, &offered, from, said);
        let (round, rounds) = (job.round, job.rounds);
        hear(job, crate::brain::actions::TRY_AGAIN);
        next_round(job);
        (job.round, job.rounds, job.publish_step) = (round, rounds, None);
        job.step = line.clone();
    }
}

impl Jobs {
    /// Where job `id`, stopped by its account's limit, moves by itself, if anywhere (LIMITS-15):
    /// with `accountFailover` on, to the account with the most room of any tool, unless the owner
    /// named its account. A ChatGPT login shows no room, so only the owner's click puts work there.
    pub(super) async fn failover_for(&self, id: &str) -> Option<Failover> {
        let job = (self.get(id)).filter(|j| !j.named && self.account_failover())?;
        let (was, from) = crate::accounts::find(&self.config.agents.get(), &job.account)?;
        let (tool, account) = (self
            .destinations(&job.agent, &job.account, std::slice::from_ref(&job))
            .await)
            .into_iter()
            .find(|(_, a)| crate::chatgpt::marker_of(a).is_none())?;
        let (limited, took) = match tool == was {
            true => (from.name.as_str(), account.name.as_str()),
            false => (was.name(), tool.name()),
        };
        let line = format!("{limited} reached its limit, so {took} took over");
        Some(Failover(
            tool,
            account,
            format!("{} · {}", was.name(), from.name),
            line,
        ))
    }

    /// A pending card from before LIMITS-15 resolves itself: each job moves where there's room, or
    /// else waits as a job a limit stops now does.
    async fn settle_pending(self: &Arc<Self>, decision: &crate::account_limits::Decision) {
        if decision.choice != crate::account_limits::Choice::Pending {
            return;
        }
        let _execution = self.limit_execution.lock().await;
        for id in &decision.jobs {
            let settled = match self.failover_for(id).await {
                Some(failover) => self.apply_limit_job(decision, id, None, Some(&failover)),
                None => {
                    let Some(job) = self.get(id) else { continue };
                    let at = self.limit_wait_until(&job.account, job.account_limit_reset);
                    let mut waits = false;
                    self.update(id, |job| {
                        waits = job.account_limit_blocked
                            && job.status == Status::Interrupted
                            && job.dismissed.is_none()
                            && job.account_limit_episode.as_deref() == Some(&decision.id);
                        if waits {
                            let (round, rounds) = (job.round, job.rounds);
                            next_round(job);
                            (job.round, job.rounds) = (round, rounds);
                            wait_for_reset(job, at);
                        }
                    });
                    waits.then_some(()).ok_or(String::new())
                }
            };
            if settled.is_ok()
                && let Err(error) = self.limit_decisions.applied(&decision.id, id)
            {
                tracing::error!("can't keep account-limit receipt for {id}: {error}");
            }
        }
    }

    /// Jobs waiting for room take an account that has it now, by itself (LIMITS-15).
    async fn fail_over_waits(self: &Arc<Self>) {
        let waiting: Vec<String> = (self.lock().jobs.values())
            .filter(|j| j.status == Status::Queued && j.resumes_at.is_some())
            .map(|j| j.id.clone())
            .collect();
        for id in waiting {
            let Some(failover) = self.failover_for(&id).await else {
                continue;
            };
            self.update(&id, |job| {
                if job.status == Status::Queued && job.resumes_at.is_some() {
                    failover.apply(job, self);
                }
            });
            self.forget_moved_login(&id);
            self.schedule();
        }
    }

    /// When a job `account`'s limit stopped resumes there at the latest: 90 seconds after the reset
    /// its tool said (LIMITS-5), else the one obid knows, else in an hour.
    pub(super) fn limit_wait_until(&self, account: &str, reset: Option<i64>) -> i64 {
        let now = obi_protocol::time::now();
        let at = (reset.filter(|at| *at > now))
            .or_else(|| self.usage.get(account).free_at(now))
            .unwrap_or(now + UNKNOWN_RESET);
        at.max(now) + RESUME_AFTER_RESET
    }

    /// Resumes the jobs whose reset came, in their own sessions: one each [`RESET_TICK`], so a Mac
    /// that slept through several resets doesn't start them all at once (LIMITS-8). An account
    /// that's still held, or gone, waits again, with no card (LIMITS-11, LIMITS-15).
    pub(super) fn resume_limit_waits(self: &Arc<Self>) {
        let now = obi_protocol::time::now();
        let mut due: Vec<(i64, String)> = self
            .lock()
            .jobs
            .values()
            .filter(|j| j.status == Status::Queued)
            .filter_map(|j| Some((j.resumes_at.filter(|at| *at <= now)?, j.id.clone())))
            .collect();
        due.sort();
        let waits = |job: &Job| job.status == Status::Queued && job.resumes_at.is_some();
        for (_, id) in due {
            let Some(job) = self.get(&id) else { continue };
            // A review of a head its job pushed past: its verdict wouldn't count (`reviewed`), so
            // it spends no turn, and stopping it starts the newer head's (LIMITS-14).
            if job
                .review_of
                .as_deref()
                .and_then(|of| self.get(of))
                .is_some_and(|author| job.read_another_head(&author))
            {
                self.stop(&id, "Stopped: its job pushed a newer head to review");
                continue;
            }
            let gone = !self.limit_source_is_connected(&job.agent, &job.account);
            if gone || self.usage.get(&job.account).held(now).is_some() {
                let at = match gone {
                    true => now + UNKNOWN_RESET,
                    false => self.limit_wait_until(&job.account, None),
                };
                self.update(&id, |job| {
                    if waits(job) {
                        wait_for_reset(job, at);
                    }
                });
                continue;
            }
            self.update(&id, |job| {
                if waits(job) {
                    // The same round going on, as after a restart (RECOVERY-2).
                    let (round, rounds) = (job.round, job.rounds);
                    next_round(job);
                    (job.round, job.rounds) = (round, rounds);
                    job.step = "Starting".into();
                    // A job that never started has no session to tell: it gets its first prompt.
                    if job.session.is_some() {
                        job.recovery.cut = Some(RESET_CAME.into());
                        job.recovery.sent.get_or_insert_default();
                    }
                }
            });
            self.changed.notify_waiters();
            self.schedule();
            return;
        }
    }

    /// A persisted answer remains visible after its question closes, including after a restart
    /// between saving the answer and updating its jobs. This mirror never authorizes execution.
    pub(super) fn sync_limit_answer(&self, decision: &crate::account_limits::Decision) {
        use crate::account_limits::Choice;
        for id in &decision.jobs {
            let Some(job) = self.get(id).filter(|job| {
                job.account_limit_blocked
                    && job.status == Status::Interrupted
                    && job.dismissed.is_none()
                    && job.account_limit_episode.as_deref() == Some(&decision.id)
                    && job.account_limit_choice != Some(decision.choice)
            }) else {
                continue;
            };
            let reason = match decision.choice {
                Choice::Pending => job.reason.clone().unwrap_or_else(|| LIMIT_QUESTION.into()),
                Choice::Resume => self.resume_wait(&decision.account).into(),
                Choice::Stop => {
                    "You kept this job stopped after its account reached its usage limit.".into()
                }
                Choice::Move => {
                    "You chose another account. Oby is moving this job with its work and context."
                        .into()
                }
            };
            self.update(id, |job| {
                if job.account_limit_episode.as_deref() == Some(&decision.id) {
                    job.account_limit_choice = Some(decision.choice);
                    job.reason = Some(reason);
                    job.next_step = None;
                }
            });
        }
    }

    pub(super) async fn run_approved_limit_decisions(self: &Arc<Self>) {
        let now = obi_protocol::time::now();
        self.fail_over_waits().await;
        for decision in self.limit_decisions.list() {
            self.sync_limit_answer(&decision);
            self.settle_pending(&decision).await;
            if decision.choice == crate::account_limits::Choice::Move {
                self.apply_limit_move(&decision).await;
                continue;
            }
            if decision.choice != crate::account_limits::Choice::Resume {
                continue;
            }
            // A disconnected source cannot produce a fresh usage report. Reconcile it before
            // waiting for usage, otherwise an accepted choice would disappear indefinitely.
            if !self.limit_source_is_connected(&decision.agent, &decision.account) {
                for id in &decision.jobs {
                    self.reask_limit_job(
                        &decision,
                        id,
                        "The original account is no longer connected.",
                    );
                }
                continue;
            }
            if decision
                .reset_at
                .is_some_and(|at| now < at.saturating_add(RESET_SLACK))
            {
                continue;
            }
            {
                let mut reads = self.usage_reads();
                if reads.limit_refreshed.insert(decision.id.clone()) {
                    reads.asked.remove(&decision.account);
                }
            }
            self.want_usage();
            let since = decision.reset_at.unwrap_or(decision.created_at);
            if !self.verified_limit_room(&decision.account, now, since) {
                // #1645: an account with no fresh reading (Cursor and API keys never report, a
                // read can fail) resumes on time instead of waiting forever, unless what obid last
                // read of it still holds it (a week at its reserve). A wait says why.
                let usage = self.usage.get(&decision.account);
                let fresh = [usage.five_hour_observed_at, usage.seven_day_observed_at]
                    .into_iter()
                    .flatten()
                    .any(|at| at >= since && now.saturating_sub(at) <= 600);
                let on_time = !fresh
                    && usage.held(now).is_none()
                    && now >= since.saturating_add(UNREAD_RESUME);
                if !on_time {
                    let why = if usage.only_reserved(now) {
                        RESERVE_WAIT
                    } else {
                        self.resume_wait(&decision.account)
                    };
                    self.say_limit_wait(&decision, why);
                    continue;
                }
            }
            for id in &decision.jobs {
                if decision.applied.contains(id) {
                    continue;
                }
                let Some(job) = self.get(id) else { continue };
                if job.dismissed.is_some()
                    || !job.account_limit_blocked
                    || job.status != Status::Interrupted
                {
                    continue;
                }
                match self.apply_limit_job(&decision, id, None, None) {
                    Ok(()) => {
                        if let Err(error) = self.limit_decisions.applied(&decision.id, id) {
                            tracing::error!("can't keep account-limit receipt for {id}: {error}");
                        }
                    }
                    Err(why) => self.reask_limit_job(&decision, id, &why),
                }
            }
        }
    }

    /// Says why an approved Resume still waits, on each of its jobs whose words differ.
    fn say_limit_wait(&self, decision: &crate::account_limits::Decision, why: &str) {
        for id in &decision.jobs {
            if self.get(id).is_some_and(|job| {
                job.account_limit_episode.as_deref() == Some(&decision.id)
                    && job.account_limit_blocked
                    && job.account_limit_choice == Some(crate::account_limits::Choice::Resume)
                    && job.dismissed.is_none()
                    && job.status == Status::Interrupted
                    && job.reason.as_deref() != Some(why)
            }) {
                self.update(id, |job| job.reason = Some(why.to_owned()));
            }
        }
    }

    fn limit_source_is_connected(&self, agent: &str, id: &str) -> bool {
        let home = login_home();
        Tool::from_id(agent).is_some_and(|tool| {
            self.accounts(tool)
                .iter()
                .any(|account| account.id == id && signed_in(tool, account, home.as_deref()))
        })
    }

    /// Record a stopped job's durable decision. A failed write leaves it stopped and is retried
    /// from the per-job marker on the next reconciliation or restart.
    pub(super) fn record_account_limit(&self, id: &str) {
        let Some(job) = self
            .get(id)
            .filter(|j| j.account_limit_blocked && j.dismissed.is_none())
        else {
            return;
        };
        if job.account_limit_episode.as_ref().is_some_and(|episode| {
            self.limit_decisions
                .list()
                .iter()
                .any(|d| &d.id == episode && d.jobs.contains(&job.id))
        }) {
            return;
        }
        let Some(nonce) = job.account_limit_nonce.as_deref() else {
            return;
        };
        match self.limit_decisions.record(
            &job.agent,
            &job.account,
            job.account_limit_reset,
            id,
            nonce,
        ) {
            Ok(decision) => {
                self.update(id, |j| j.account_limit_episode = Some(decision.id.clone()));
                self.events
                    .send("limitDecision", json!({"id": decision.id}));
            }
            Err(error) => {
                tracing::error!("can't keep account limit decision for {id}: {error}");
                self.update(id, |j| j.reason = Some("Oby could not save your account-limit decision. The job remains stopped; restart Oby after resolving its storage problem.".into()));
            }
        }
    }

    /// What an approved Resume on `account` says while it waits.
    fn resume_wait(&self, account: &str) -> &'static str {
        let on_chatgpt = Tool::ALL
            .into_iter()
            .flat_map(|tool| self.accounts(tool))
            .find(|a| a.id == account)
            .is_some_and(|a| crate::chatgpt::marker_of(&a).is_some());
        if on_chatgpt {
            RESUME_WAIT_NO_TIME
        } else {
            RESUME_WAIT
        }
    }

    /// A ChatGPT login reports no usage (docs/contracts/chatgpt-sign-in.md), so it can never show
    /// room: it's offered while nothing obid knows holds it, and it moves only on the owner's click.
    fn chatgpt_has_room(&self, account: &crate::config::Account, now: i64) -> bool {
        crate::chatgpt::marker_of(account).is_some()
            && self.usage.get(&account.id).full_until_at(now).is_none()
    }

    /// Only current, provider-reported windows constitute evidence that an account has room.
    fn verified_limit_room(&self, account: &str, now: i64, since: i64) -> bool {
        let usage = self.usage.get(account);
        usage
            .five_hour
            .is_some_and(|used| used < crate::usage::NEARLY_FULL)
            && usage.seven_day.is_some_and(|used| {
                used < crate::usage::NEARLY_FULL
                    && (used < crate::usage::RESERVE || usage.lifted_until(now).is_some())
            })
            && usage
                .five_hour_observed_at
                .is_some_and(|at| at >= since && at <= now && now.saturating_sub(at) <= 600)
            && usage
                .seven_day_observed_at
                .is_some_and(|at| at >= since && at <= now && now.saturating_sub(at) <= 600)
            && usage.full_until_at(now).is_none()
    }

    async fn limit_destinations(
        &self,
        decision: &crate::account_limits::Decision,
    ) -> Vec<(Tool, crate::config::Account)> {
        let jobs: Vec<Job> = decision
            .jobs
            .iter()
            .filter_map(|id| self.get(id))
            .filter(|j| {
                j.account_limit_blocked && j.dismissed.is_none() && j.status == Status::Interrupted
            })
            .collect();
        self.destinations(&decision.agent, &decision.account, &jobs)
            .await
    }

    /// Where `jobs`, stopped on `agent`'s account `from`, may move: a signed-in account of any
    /// tool with room, the most room first (LIMITS-15), never one in its own sandbox, so work in
    /// Oby's box stays in it.
    async fn destinations(
        &self,
        agent: &str,
        from: &str,
        jobs: &[Job],
    ) -> Vec<(Tool, crate::config::Account)> {
        // A review can't change accounts, but it doesn't take Move away from the tasks beside it
        // (#1645): a Move asks again about the review alone (`apply_limit_move`).
        if jobs.iter().all(|j| j.obi.is_some() || j.read_only()) {
            return Vec::new();
        }
        let Some(source_tool) = Tool::from_id(agent) else {
            return Vec::new();
        };
        if source_tool == Tool::Claude && self.config.claude_own_sandbox.iter().any(|a| a == from) {
            return Vec::new();
        }
        // Work nobody's watching doesn't move onto a ChatGPT login without the owner's yes
        // (CHATGPT-20). Reviews stay where they are, so they don't count.
        let background = jobs
            .iter()
            .any(|j| j.background() && j.obi.is_none() && !j.read_only());
        let now = obi_protocol::time::now();
        let home = login_home();
        let mut result = Vec::new();
        for tool in Tool::ALL {
            for account in self.accounts(tool) {
                if account.id == from
                    || self.being_changed(&account.id)
                    || !signed_in(tool, &account, home.as_deref())
                    || (background && !self.takes_background(&account))
                    || !(self.verified_limit_room(&account.id, now, now.saturating_sub(600))
                        || self.chatgpt_has_room(&account, now))
                {
                    continue;
                }
                if tool == Tool::Claude && self.claude_login_ready(Some(&account)).await.is_err() {
                    continue;
                }
                result.push((tool, account));
            }
        }
        // The fuller of its two windows; one that reports none (a ChatGPT login) last.
        let used = |id: &str| {
            let usage = self.usage.get(id);
            (usage.five_hour.zip(usage.seven_day)).map_or(1.0, |(five, week)| five.max(week))
        };
        result.sort_by(|(_, a), (_, b)| used(&a.id).total_cmp(&used(&b.id)));
        result
    }

    pub async fn limit_decisions(self: &Arc<Self>) -> Vec<Value> {
        self.want_usage();
        // Recover a job persisted just before the decision file was written.
        let blocked: Vec<String> = self
            .lock()
            .jobs
            .values()
            .filter(|j| {
                j.account_limit_blocked && j.dismissed.is_none() && j.status == Status::Interrupted
            })
            .map(|j| j.id.clone())
            .collect();
        for id in blocked {
            self.record_account_limit(&id);
        }
        for decision in self.limit_decisions.list() {
            self.sync_limit_answer(&decision);
        }
        let mut result = Vec::new();
        for decision in self
            .limit_decisions
            .list()
            .into_iter()
            .filter(|d| d.choice == crate::account_limits::Choice::Pending)
        {
            let jobs: Vec<Value> = decision
                .jobs
                .iter()
                .filter_map(|id| self.get(id))
                .filter(|j| {
                    j.account_limit_blocked
                        && j.dismissed.is_none()
                        && j.status == Status::Interrupted
                })
                .map(|j| json!({"id":j.id,"title":j.title}))
                .collect();
            if jobs.is_empty() {
                continue;
            }
            let mut options: Vec<Value> = (self.limit_destinations(&decision).await)
                .into_iter()
                .map(|(tool, a)| json!({"kind":"move", "account":a.id, "agent":tool.id(), "name": a.name}))
                .collect();
            let source_connected =
                self.limit_source_is_connected(&decision.agent, &decision.account);
            if options.is_empty() && source_connected {
                options.push(json!({"kind":"resume"}));
            }
            options.push(json!({"kind":"stop"}));
            // The revision the owner signs follows the jobs shown, not the options (#1645): a
            // usage read landing after they looked changes the options, and their answer is
            // refused only when the option they chose is gone (`answer_limit_decision`).
            let snapshot = json!({"jobs":jobs}).to_string();
            let (decision, problem) = match self.limit_decisions.refresh(&decision.id, &snapshot) {
                Ok(saved) => (saved, (!source_connected).then_some("The original account isn't connected. Connect it or choose another available account.")),
                Err(error) => {
                    tracing::error!("can't keep account-limit options: {error}");
                    // Fail closed while retaining the visible question. No option may be
                    // approved until its exact scope can be kept durably.
                    options.clear();
                    (
                        decision,
                        Some(
                            "Oby couldn't save this decision. Your jobs remain stopped. Restore disk space or access and try again.",
                        ),
                    )
                }
            };
            result.push(json!({"id":decision.id,"revision":decision.revision,"account":decision.account,"agent":decision.agent,"resetAt":decision.reset_at,"jobs":jobs,"options":options,"problem":problem}));
        }
        result
    }

    pub async fn answer_limit_decision(
        self: &Arc<Self>,
        id: &str,
        revision: u64,
        choice: &str,
        account: Option<&str>,
    ) -> Result<Value, String> {
        let view = self
            .limit_decisions()
            .await
            .into_iter()
            .find(|v| v["id"] == id)
            .ok_or("Decision not found or no longer pending.")?;
        if view["revision"].as_u64() != Some(revision) {
            return Err("The decision changed; refresh and choose again.".into());
        }
        let offered = view["options"].as_array().is_some_and(|options| {
            options.iter().any(|option| {
                option["kind"] == choice
                    && (choice != "move" || option["account"].as_str() == account)
            })
        });
        if !offered || (choice != "move" && account.is_some()) {
            return Err("That option is no longer available; refresh and choose again.".into());
        }
        // A limit can arrive or a job can be dismissed between rendering and answering. Check
        // the exact scope under the job lock while claiming the decision revision.
        let decision = {
            let state = self.lock();
            let decision = self
                .limit_decisions
                .list()
                .into_iter()
                .find(|d| d.id == id)
                .ok_or("Decision not found.")?;
            let mut actual: Vec<String> = state
                .jobs
                .values()
                .filter(|j| {
                    j.account_limit_blocked
                        && j.status == Status::Interrupted
                        && j.dismissed.is_none()
                        && j.agent == decision.agent
                        && j.account == decision.account
                        && (j.account_limit_episode.as_deref() == Some(id)
                            || j.account_limit_episode.is_none())
                })
                .map(|j| j.id.clone())
                .collect();
            let mut shown: Vec<String> = view["jobs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|j| j["id"].as_str().map(str::to_owned))
                .collect();
            actual.sort();
            shown.sort();
            if actual != shown {
                return Err("The affected jobs changed; refresh and choose again.".into());
            }
            self.limit_decisions.answer(id, revision, choice, account)?
        };
        self.sync_limit_answer(&decision);
        self.events.send("limitDecision", json!({"id":id}));
        if choice == "move" {
            self.apply_limit_move(&decision).await;
        }
        Ok(json!({"id":id,"choice":choice,"revision":decision.revision}))
    }

    /// Apply a decision to one job in one persisted state transition. In particular the handoff,
    /// new account and queued status cannot be separated by an obid crash.
    pub(super) fn apply_limit_job(
        self: &Arc<Self>,
        decision: &crate::account_limits::Decision,
        id: &str,
        target: Option<(Tool, crate::config::Account)>,
        failover: Option<&Failover>,
    ) -> Result<(), String> {
        let from_tool = Tool::from_id(&decision.agent).ok_or("The old tool is unavailable.")?;
        let from_name = self
            .accounts(from_tool)
            .into_iter()
            .find(|a| a.id == decision.account)
            .map_or_else(|| decision.account.clone(), |a| a.name);
        if target.is_some()
            && from_tool == Tool::Claude
            && self.config.claude_own_sandbox.contains(&decision.account)
        {
            return Err(
                "This job uses its account's own sandbox and cannot be moved safely.".into(),
            );
        }
        if target.is_none()
            && failover.is_none()
            && (self.being_changed(&decision.account)
                || !self.limit_source_is_connected(&decision.agent, &decision.account))
        {
            return Err("The original account is no longer connected.".into());
        }
        let tuning = crate::settings::load(&self.config.home).tuning;
        let offered = target
            .as_ref()
            .map(|(tool, account)| self.model_lists.to_check(*tool, &account.id));
        let changed = {
            let mut state = self.lock();
            if state.publishing.contains(id) {
                return Err("The job is publishing.".into());
            }
            let job = state.jobs.get(id).ok_or("The job no longer exists.")?;
            if job.status != Status::Interrupted
                || job.dismissed.is_some()
                || !job.account_limit_blocked
                || job.account_limit_episode.as_deref() != Some(&decision.id)
            {
                return Err("The job changed or was dismissed.".into());
            }
            if past_safety(job) {
                return Err("The job needs a separate safety confirmation.".into());
            }
            if state
                .jobs
                .values()
                .any(|j| j.follow_up_of.as_deref() == Some(id) && j.branch == job.branch)
            {
                return Err("A follow-up now carries the job's branch.".into());
            }
            // Resumed, it would move a branch another job works on now (PUBLISHING-4).
            if let Some(why) = branch_busy(&state.jobs, job) {
                return Err(why);
            }
            let mut next = job.clone();
            next.revision += 1;
            if let Some(failover) = failover {
                failover.apply(&mut next, self);
                self.forget_moved_login(id);
            } else if let Some((tool, account)) = target {
                if next.obi.is_some() || next.read_only() {
                    return Err("This review cannot change accounts.".into());
                }
                let from = format!("{} · {}", from_tool.name(), from_name);
                carry_over(
                    &mut next,
                    tool,
                    &account,
                    &tuning,
                    offered.as_deref().unwrap_or(&[]),
                    &from,
                    Carried {
                        because: "which reached its limit",
                        why: "Your account-limit choice".into(),
                        by: "by your account-limit choice".into(),
                    },
                );
                next.named = true;
                self.forget_moved_login(id);
            }
            if failover.is_none() {
                hear(&mut next, crate::brain::actions::TRY_AGAIN);
                next_round(&mut next);
                next.asked = true;
            }
            self.store
                .save(&next)
                .map_err(|e| format!("Couldn't save the job: {e}"))?;
            state.jobs.insert(id.to_owned(), next.clone());
            next
        };
        self.events.send("job", changed.to_json());
        self.changed.notify_waiters();
        self.schedule();
        Ok(())
    }

    /// What a job leaves behind of its old account's login when it moves ([`carry_over`]).
    pub(super) fn forget_moved_login(&self, id: &str) {
        let login = self.login_path(id);
        forget_login_secrets(&login);
        remove_inside(&login, ".claude.json");
        remove_inside(&login, "cli-config.json");
    }

    /// Persist the failed job's fresh episode first. A crash before the old receipt is saved can
    /// then recover the new question from the job instead of hiding it forever.
    fn reask_limit_job(&self, decision: &crate::account_limits::Decision, id: &str, why: &str) {
        let changed = {
            let mut state = self.lock();
            let Some(job) = state.jobs.get(id) else {
                return;
            };
            if job.account_limit_episode.as_deref() != Some(&decision.id)
                || job.status != Status::Interrupted
                || job.dismissed.is_some()
            {
                return;
            }
            let mut next = job.clone();
            next.revision += 1;
            next.reason = Some(format!("{why} Choose what Oby should do with this job."));
            next.account_limit_episode = None;
            next.account_limit_nonce = Some(uuid::Uuid::new_v4().to_string());
            next.account_limit_choice = None;
            if let Err(error) = self.store.save(&next) {
                tracing::error!("can't keep account-limit failure for {id}: {error}");
                return;
            }
            state.jobs.insert(id.to_owned(), next.clone());
            next
        };
        self.events.send("job", changed.to_json());
        self.record_account_limit(id);
        if let Err(error) = self.limit_decisions.applied(&decision.id, id) {
            tracing::error!("can't keep account-limit failure receipt for {id}: {error}");
        }
    }

    async fn apply_limit_move(self: &Arc<Self>, decision: &crate::account_limits::Decision) {
        let _execution = self.limit_execution.lock().await;
        let Some(target) = decision.destination.as_deref() else {
            return;
        };
        for id in &decision.jobs {
            if decision.applied.contains(id) {
                continue;
            }
            let Some(job) = self.get(id) else { continue };
            if !job.account_limit_blocked
                || job.account_limit_episode.as_deref() != Some(&decision.id)
                || job.dismissed.is_some()
                || job.status != Status::Interrupted
            {
                let _ = self.limit_decisions.applied(&decision.id, id);
                continue;
            }
            if job.obi.is_some() || job.read_only() {
                self.reask_limit_job(decision, id, "A review stays on its account.");
                continue;
            }
            let destinations = self.limit_destinations(decision).await;
            let target_account = destinations.into_iter().find(|(_, a)| a.id == target);
            let result = match target_account {
                Some(to) => self.apply_limit_job(decision, id, Some(to), None),
                None => Err("The chosen account is no longer available.".into()),
            };
            match result {
                Ok(()) => {
                    if let Err(error) = self.limit_decisions.applied(&decision.id, id) {
                        tracing::error!("can't keep account-limit receipt for {id}: {error}");
                    }
                }
                Err(why) => {
                    self.reask_limit_job(decision, id, &why);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIMITS-5: the job's line says the local time it resumes.
    #[test]
    fn limits_5_the_line_says_the_time_in_the_macs_zone() {
        let at = 1_793_691_381; // 2026-11-03T07:36:21Z
        let utc = jiff::tz::TimeZone::UTC;
        let line = "Every account is at its limit: resuming when one has room, at the latest";
        assert_eq!(resuming_at(at, &utc), format!("{line} Tue 7:36 AM"));
        let sao_paulo = jiff::tz::TimeZone::get("America/Sao_Paulo").unwrap();
        assert_eq!(resuming_at(at, &sao_paulo), format!("{line} Tue 4:36 AM"));
    }

    /// LIMITS-6: a Stop that lands as the run ends, once obid read that it wasn't stopped, stays.
    #[test]
    fn limits_6_a_stop_as_the_run_ends_isnt_undone() {
        let mut job = Job::new(
            "aaaa0601".into(),
            "o/r".into(),
            "build".into(),
            "Look around".into(),
            "claude".into(),
            "stub".into(),
        );
        job.status = Status::Stopped;
        start_wait(&mut job, 1_793_691_381);
        assert_eq!(job.status, Status::Stopped);
        assert_eq!(job.resumes_at, None);
        assert_eq!(job.recovery.limit_resumes, 0);

        job.status = Status::Running;
        job.asked = true;
        start_wait(&mut job, 1_793_691_381);
        assert_eq!(job.status, Status::Queued);
        assert_eq!(job.resumes_at, Some(1_793_691_381));
        assert_eq!(job.recovery.limit_resumes, 1);
        assert!(job.asked, "whether the owner asked for its round stays");
    }
}
