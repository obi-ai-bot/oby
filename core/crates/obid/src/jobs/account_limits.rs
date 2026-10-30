//! Application orchestration for the account-limit contract (#1563).
//!
//! The durable decision store is `crate::account_limits`; this adapter joins its decisions to
//! jobs, provider usage, and the scheduler. Clients only render and sign the offered choices.

use super::*;

impl Jobs {
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
                Choice::Pending => job.reason.clone().unwrap_or_else(|| "This account reached its usage limit. Choose what Oby should do with the affected jobs.".into()),
                Choice::Resume => "Waiting for the account's usage limit. Oby resumes this job once fresh usage confirms room.".into(),
                Choice::Stop => "You kept this job stopped after its account reached its usage limit.".into(),
                Choice::Move => "You chose another account. Oby is moving this job with its work and context.".into(),
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
        for decision in self.limit_decisions.list() {
            self.sync_limit_answer(&decision);
            if decision.choice == crate::account_limits::Choice::Move {
                self.apply_limit_move(&decision).await;
                continue;
            }
            if decision.choice != crate::account_limits::Choice::Resume {
                continue;
            }
            // A disconnected source cannot produce a fresh usage report. Reconcile it before
            // waiting for usage, otherwise an accepted choice would disappear indefinitely.
            if !self.limit_source_connected(&decision) {
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
            if !self.verified_limit_room(
                &decision.account,
                now,
                decision.reset_at.unwrap_or(decision.created_at),
            ) {
                continue;
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
                match self.apply_limit_job(&decision, id, None) {
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

    fn limit_source_connected(&self, decision: &crate::account_limits::Decision) -> bool {
        let home = login_home();
        Tool::from_id(&decision.agent).is_some_and(|tool| {
            self.accounts(tool).iter().any(|account| {
                account.id == decision.account && signed_in(tool, account, home.as_deref())
            })
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

    async fn limit_destinations(&self, decision: &crate::account_limits::Decision) -> Vec<Value> {
        let jobs: Vec<Job> = decision
            .jobs
            .iter()
            .filter_map(|id| self.get(id))
            .filter(|j| {
                j.account_limit_blocked && j.dismissed.is_none() && j.status == Status::Interrupted
            })
            .collect();
        if jobs.is_empty() || jobs.iter().any(|j| j.obi.is_some() || j.read_only()) {
            return Vec::new();
        }
        let Some(source_tool) = Tool::from_id(&decision.agent) else {
            return Vec::new();
        };
        if source_tool == Tool::Claude && self.config.claude_own_sandbox.contains(&decision.account)
        {
            return Vec::new();
        }
        let now = obi_protocol::time::now();
        let home = login_home();
        let mut result = Vec::new();
        for tool in Tool::ALL {
            for account in self.accounts(tool) {
                if account.id == decision.account
                    || self.being_changed(&account.id)
                    || !signed_in(tool, &account, home.as_deref())
                    || !self.verified_limit_room(&account.id, now, now.saturating_sub(600))
                    || self.own_sandbox(tool, &account)
                {
                    continue;
                }
                if tool == Tool::Claude && self.claude_login_ready(Some(&account)).await.is_err() {
                    continue;
                }
                result.push(json!({"kind":"move", "account":account.id, "agent":tool.id(), "name": account.name}));
            }
        }
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
            let mut options = self.limit_destinations(&decision).await;
            let source_connected = self.limit_source_connected(&decision);
            if options.is_empty() && source_connected {
                options.push(json!({"kind":"resume"}));
            }
            options.push(json!({"kind":"stop"}));
            let snapshot = json!({"jobs":jobs,"options":options}).to_string();
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
    fn apply_limit_job(
        self: &Arc<Self>,
        decision: &crate::account_limits::Decision,
        id: &str,
        target: Option<(Tool, crate::config::Account)>,
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
        if target.is_none() {
            let home = login_home();
            let available = self
                .accounts(from_tool)
                .into_iter()
                .find(|a| a.id == decision.account)
                .is_some_and(|a| {
                    !self.being_changed(&a.id) && signed_in(from_tool, &a, home.as_deref())
                });
            if !available {
                return Err("The original account is no longer connected.".into());
            }
        }
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
            let mut next = job.clone();
            if let Some((tool, account)) = target {
                if next.obi.is_some() || next.read_only() {
                    return Err("This review cannot change accounts.".into());
                }
                let from = format!("{} · {}", from_tool.name(), from_name);
                let to = format!("{} · {}", tool.name(), account.name);
                let heard = handoff(&next, &from);
                next.heard.insert(0, heard);
                next.agent = tool.id().to_owned();
                next.account = account.id;
                next.named = true;
                next.session = None;
                if tool != from_tool {
                    next.model = None;
                }
                next.moves.push(crate::job::Move {
                    from: from.clone(),
                    to: to.clone(),
                    why: Some("Your account-limit choice".into()),
                    date: obi_protocol::time::now(),
                });
                next.changed(format!(
                    "Moved from {from} to {to} by your account-limit choice"
                ));
                let login = self.login_path(id);
                forget_login_secrets(&login);
                remove_inside(&login, ".claude.json");
                remove_inside(&login, "cli-config.json");
            }
            hear(&mut next, crate::brain::actions::TRY_AGAIN);
            next_round(&mut next);
            next.asked = true;
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
            let destinations = self.limit_destinations(decision).await;
            let target_account = destinations
                .iter()
                .find(|v| v["account"] == target)
                .and_then(|v| Tool::from_id(v["agent"].as_str()?))
                .and_then(|tool| {
                    self.accounts(tool)
                        .into_iter()
                        .find(|a| a.id == target)
                        .map(|a| (tool, a))
                });
            let result = match target_account {
                Some(to) => self.apply_limit_job(decision, id, Some(to)),
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
