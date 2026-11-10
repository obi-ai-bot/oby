//! Moving a job to another tool, account or model: `POST /v1/jobs/{id}/move` (LIMITS-16,
//! docs/contracts/account-limits.md) and `oby_change_job`. The worktree and branch stay; a job that
//! has run goes on in a new session there, told what it's taking over ([`carry_over`]).

use super::*;

/// Who stopped a running job to move it: its run starts over on the new choice.
const MOVING: &str = "Moved to another account";

/// Where a job goes, as `POST /v1/jobs/{id}/move`'s body says it: each left out follows from the
/// rest (docs/runner-api.md).
#[derive(Default, serde::Deserialize)]
pub struct Moving {
    pub agent: Option<String>,
    /// An account's id or name; `auto` or none for Routing's pick.
    pub account: Option<String>,
    /// A model's id, or `default` for the account's own.
    pub model: Option<String>,
    /// The owner's reason, which the agent taking over hears.
    pub why: Option<String>,
}

impl Jobs {
    /// LIMITS-16: a queued job starts on the new choice; a running one is stopped and, once its run
    /// and all it started ended, its round starts over there; one that ended runs its next round
    /// there. A move that can't go leaves it as it was.
    pub async fn move_to(
        self: &Arc<Self>,
        id: &str,
        to: Moving,
        source: Source,
    ) -> Option<Result<Job, Refused>> {
        let job = self.get(id)?;
        let (tool, account) = match self.destination(&job, &to) {
            Ok(found) => found,
            Err(refused) => return Some(Err(refused)),
        };
        let (busy, running) = {
            let state = self.lock();
            let busy = state.wrapping.contains(id) || state.publishing.contains(id);
            (busy, state.workers.contains_key(id))
        };
        let why = if busy {
            "is wrapping up or publishing: move it once it's done"
        } else if !signed_in(tool, &account, login_home().as_deref()) {
            "isn't signed in there"
        } else if (self.usage.get(&account.id))
            .full_until_at(obi_protocol::time::now())
            .is_some()
        {
            "can't go there: that account is at its usage limit"
        } else {
            ""
        };
        if !why.is_empty() {
            return Some(Err(Refused(format!("It {why}."))));
        }
        if running {
            self.stop(id, MOVING);
            while self.lock().workers.contains_key(id) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        let moved = self.change(id, &to, "by you", source)?;
        if running && moved.is_ok() {
            self.update(id, |job| {
                if job.stopped_by.as_deref() == Some(MOVING) {
                    let (round, rounds) = (job.round, job.rounds);
                    hear(job, crate::brain::actions::TRY_AGAIN);
                    next_round(job);
                    (job.round, job.rounds) = (round, rounds);
                }
            });
        }
        self.schedule();
        Some(moved.map(|job| self.get(id).unwrap_or(job)))
    }

    /// Where `to` sends `job`: with no account named, its own while the tool stays.
    fn destination(
        &self,
        job: &Job,
        to: &Moving,
    ) -> Result<(Tool, crate::config::Account), Refused> {
        let refused = |why: String| Err(Refused(why));
        if job.obi.is_some() || job.read_only() {
            return refused("A review picks its own tool and account: it can't be moved.".into());
        }
        let model = to.model.as_deref().filter(|m| *m != "default");
        if let Some(model) = model.filter(|m| !crate::settings::valid_model(m)) {
            return refused(format!("{model} isn't a model."));
        }
        let account = (to.account.as_deref().map(str::trim))
            .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case("auto"));
        if to.agent.is_none() && account.is_none() && to.model.is_none() {
            return refused("Say which tool, account or model.".into());
        }
        // A model names its tool: Claude Code's start `claude-`, else the job's other tool or Codex.
        let by_model = model.map(|m| match m.starts_with("claude-") {
            true => Tool::Claude.id(),
            false if job.agent != Tool::Claude.id() => job.agent.as_str(),
            false => Tool::Codex.id(),
        });
        let agent = (to.agent.as_deref()).or(account.xor(Some(by_model.unwrap_or(&job.agent))));
        let background = job.background();
        let (tool, chosen) = match crate::accounts::find(&self.config.agents.get(), &job.account) {
            Some(own) if account.is_none() && agent == Some(own.0.id()) => own,
            _ => self.pick_for(agent, account, background)?,
        };
        if background && self.waits_for_yes(tool, &chosen) {
            return refused(format!("I need {}.", consent_words(&chosen.name)));
        }
        if model.is_some_and(|m| m.starts_with("claude-") != (tool == Tool::Claude)) {
            return refused(format!(
                "That's a model of another tool than {}.",
                tool.name()
            ));
        }
        let same_model = (to.model.as_deref()).is_none_or(|m| job.model.as_deref() == Some(m));
        if tool.id() == job.agent && chosen.id == job.account && same_model {
            return refused(format!(
                "It's on {} · {} already.",
                tool.name(),
                chosen.name
            ));
        }
        Ok((tool, chosen))
    }

    /// Moves a job no run holds (`oby_change_job` takes only the tool and account), adding the move
    /// and `by` to it: its next round starts a new session there, and what it waited for on the
    /// account it leaves goes.
    pub fn change(
        self: &Arc<Self>,
        id: &str,
        to: &Moving,
        by: &str,
        source: Source,
    ) -> Option<Result<Job, Refused>> {
        let job = self.get(id)?;
        let blocked =
            || Refused("Changing an account-limit job waits for the owner's signed choice.".into());
        if job.account_limit_blocked && !source.approves_limit() {
            return Some(Err(blocked()));
        }
        if self.lock().workers.contains_key(id) {
            return Some(Err(Refused(
                "It's still running: stop it first, then change it and continue it.".into(),
            )));
        }
        let (tool, chosen) = match self.destination(&job, to) {
            Ok(found) => found,
            Err(refused) => return Some(Err(refused)),
        };
        let from = crate::accounts::find(&self.config.agents.get(), &job.account)
            .map_or(job.account.clone(), |(t, a)| {
                format!("{} · {}", t.name(), a.name)
            });
        let place = format!("{} · {}", tool.name(), chosen.name);
        let named = (to.account.as_deref().map(str::trim))
            .is_some_and(|a| !a.is_empty() && !a.eq_ignore_ascii_case("auto"));
        let tuning = crate::settings::load(&self.config.home).tuning;
        let offered = self.model_lists.to_check(tool, &chosen.id);
        let had_run = job.session.is_some();
        let mut refused = false;
        let changed = self.update(id, |job| {
            if job.account_limit_blocked && !source.approves_limit() {
                refused = true;
                return;
            }
            if had_run {
                let (because, why) = ("was moved to you by the owner", "Moved by you".into());
                let said = Carried {
                    because,
                    why,
                    by: by.into(),
                };
                carry_over(job, tool, &chosen, &tuning, &offered, &from, said);
            } else {
                on_tool(job, tool);
                job.account = chosen.id.clone();
                tune(job, &tuning, &offered);
                job.changed(format!("Moved from {from} to {place} {by}"));
                let (date, why) = (obi_protocol::time::now(), None);
                let (from, place) = (from.clone(), place.clone());
                job.moves.push(crate::job::Move {
                    from,
                    to: place,
                    why,
                    date,
                });
            }
            if let Some(model) = to.model.as_deref() {
                job.model = (model != "default").then(|| model.to_owned());
                job.model_from_account = false;
                tune(job, &tuning, &offered);
            }
            if let Some(last) = job.moves.last_mut() {
                last.why = to.why.clone();
            }
            if let Some(why) = &to.why {
                job.heard
                    .push(format!("The owner moved this job to you, saying: {why}"));
            }
            job.named = named;
            // The owner moved it: their move answers its account-limit card (LIMITS-16).
            (job.account_limit_blocked, job.account_limit_episode) = (false, None);
            if job.resumes_at.is_some() || job.waiting.as_ref().is_some_and(Waiting::for_room) {
                (job.resumes_at, job.waiting) = (None, None);
                job.step = "Starting".into();
            }
        })?;
        if had_run {
            self.forget_moved_login(id);
        }
        Some(if refused { Err(blocked()) } else { Ok(changed) })
    }
}
