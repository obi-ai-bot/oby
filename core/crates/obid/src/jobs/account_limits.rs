//! Jobs at an account's usage limit (docs/contracts/account-limits.md): it owns a job's limit wait
//! (`resumes_at`, `waiting`, the `account_limit_*` fields), joins it to usage and the scheduler,
//! and settles what an older release left at a limit when obid loads it (LIMITS-26, the rule
//! `crate::limits::legacy`'s). It never writes a job's branch (docs/ARCHITECTURE.md, "State
//! ownership").

use super::*;
use crate::limits::holds::{self, Holds};
use crate::limits::legacy::{Choice, Decisions, Left, Upgrade, upgrade};

/// What cut its run, as the session that resumes hears it (`continue_message`).
const RESET_CAME: &str = "your account hit its usage limit, and that limit has reset";

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
        let tuning = crate::settings::load(&jobs.config.home).tuning;
        let offered = jobs.model_lists.to_check(*tool, &account.id);
        // A waiting one that never ran (no session, move or round's report) hands nothing over: it
        // starts there, as a wait for room does.
        let ran = job.moves.len() + job.summaries.as_ref().map_or(0, Vec::len);
        if job.status == Status::Queued && job.session.is_none() && ran == 0 {
            (job.account, job.resumes_at, job.waiting) = (account.id.clone(), None, None);
            on_tool(job, *tool);
            tune(job, &tuning, &offered);
            job.step = "Starting".into();
            let on = format!("{} · {}", tool.name(), account.name);
            return job.changed(format!("Started when an account had room: {on}"));
        }
        let by = "by itself (Account failover)".into();
        let said = Carried {
            because: "which reached its limit",
            why: line.clone(),
            by,
        };
        carry_over(job, *tool, account, &tuning, &offered, from, said);
        let (round, rounds) = (job.round, job.rounds);
        hear(job, crate::brain::actions::TRY_AGAIN);
        if let Ok(change) = transition(job, &crate::job::Command::NextRound) {
            change.apply(job);
        }
        (job.round, job.rounds, job.publish_step) = (round, rounds, None);
        job.step = line.clone();
        jobs.forget_moved_login(&job.id);
    }
}

impl Jobs {
    /// Where job `id`, stopped by its account's limit, moves by itself, if anywhere (LIMITS-15):
    /// with `accountFailover` on, to the account with the most room of any tool, unless the owner
    /// named its account. Only the owner's click puts work on a ChatGPT login (CHATGPT-25).
    pub(super) async fn failover_for(&self, id: &str) -> Option<Failover> {
        let job = (self.get(id)).filter(|j| !j.named && self.account_failover())?;
        let (was, from) = crate::accounts::find(&self.config.agents.get(), &job.account)?;
        let (work, now) = (Work::of(&job), obi_protocol::time::now());
        let held = |a: &str| self.holds().held(a, &work, now).is_some();
        let (tool, account) = (self
            .destinations(&job.agent, &job.account, std::slice::from_ref(&job))
            .await)
            .into_iter()
            .find(|(_, a)| crate::chatgpt::marker_of(a).is_none() && !held(&a.id))?;
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

    /// The one table of what holds each account back (crate::limits::holds).
    pub fn holds(&self) -> Holds<'_> {
        Holds(&self.usage, &self.usage_changed)
    }

    /// Job `id` hit `account`'s limit: the whole account's marks it full until `reset`; one
    /// model's (USAGE-13) holds this job, and that model's family, off it until then, so its moves
    /// never come back to it and, with nowhere left, it waits ([`Jobs::failover_for`], LIMITS-21).
    pub(super) fn limit_reached(&self, id: &str, on: &str, reset: Option<i64>, turn: &agent::Turn) {
        let refused = Work::new(turn.family.as_deref(), Some(id), false);
        let now = obi_protocol::time::now();
        (self.holds()).apply(on, reset, turn.scoped.then_some(&refused), now);
    }

    /// What an older release left at a limit, settled once as obid loads its jobs, after their
    /// holds (LIMITS-13, LIMITS-26, [`upgrade`]): a kept Stop stays stopped, a wait dismissed or
    /// taken over is forgotten, and every other job waits as a limit's job does now, for the tick
    /// to move it where there's room ([`Jobs::fail_over_waits`]). Its decision file goes once every
    /// job is saved; one obid can't read stays, and its jobs go by their own `account_limit_choice`.
    pub(super) fn settle_older_limits(&self) {
        let path = Decisions::path(&self.config.home);
        let file = Decisions::read(&path).unwrap_or_else(|error| {
            tracing::error!("can't read an older release's {}: {error}", path.display());
            None
        });
        let left: Vec<(String, Upgrade, Option<String>)> = {
            let state = self.lock();
            let followed: HashSet<&str> = (state.jobs.values())
                .filter_map(|j| j.follow_up_of.as_deref())
                .collect();
            (state.jobs.values())
                .map(|job| {
                    let episode = job.account_limit_episode.as_deref();
                    let kept = file.as_ref().and_then(|f| f.of(&job.id, episode));
                    let left = Left {
                        interrupted: job.status == Status::Interrupted,
                        blocked: job.account_limit_blocked,
                        dismissed: job.dismissed.is_some(),
                        older_wait: job.resumes_at.is_some(),
                        followed: followed.contains(job.id.as_str()),
                        choice: kept.map(|(_, choice)| choice).or(job.account_limit_choice),
                    };
                    let decision = kept.map(|(id, _)| id.to_owned());
                    (job.id.clone(), upgrade(left), decision)
                })
                .filter(|(_, outcome, _)| *outcome != Upgrade::Untouched)
                .collect()
        };
        let mut saved = true;
        for (id, outcome, decision) in left {
            let Some(job) = self.get(&id) else { continue };
            let kept = match outcome {
                Upgrade::Untouched => continue,
                Upgrade::ForgetWait => self.update(&id, |job| job.resumes_at = None),
                Upgrade::KeepStopped => {
                    let episode = job.account_limit_episode.clone().or(decision);
                    if (job.account_limit_choice, &job.account_limit_episode)
                        == (Some(Choice::Stop), &episode)
                    {
                        continue;
                    }
                    self.update(&id, |job| {
                        (job.account_limit_choice, job.account_limit_episode) =
                            (Some(Choice::Stop), episode);
                    })
                }
                Upgrade::Settle => {
                    let reset = job
                        .account_limit_reset
                        .filter(|_| job.account_limit_blocked);
                    let at = self.limit_wait_until(&job.account, reset, &Work::of(&job));
                    self.update(&id, |job| {
                        let (round, rounds) = (job.round, job.rounds);
                        if let Ok(change) = transition(job, &crate::job::Command::NextRound) {
                            change.apply(job);
                        }
                        (job.round, job.rounds) = (round, rounds);
                        wait_for_reset(job, at);
                    })
                }
            };
            saved &= kept.is_some();
        }
        if file.is_some()
            && saved
            && let Err(error) = std::fs::remove_file(&path)
        {
            tracing::error!(
                "can't remove an older release's {}: {error}",
                path.display()
            );
        }
    }

    /// Jobs waiting for room take an account that has it now, by itself (LIMITS-15).
    pub(super) async fn fail_over_waits(self: &Arc<Self>) {
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
            self.schedule();
        }
    }

    /// When `work` `account`'s limit stopped resumes there at the latest: 90 seconds after the
    /// reset its tool said (LIMITS-5), else the one obid knows for it (LIMITS-20), else in an hour;
    /// never before a hold on its family or on its job ends (LIMITS-21).
    pub(super) fn limit_wait_until(&self, account: &str, reset: Option<i64>, work: &Work) -> i64 {
        let now = obi_protocol::time::now();
        let at = (reset.filter(|at| *at > now))
            .or_else(|| self.usage.get(account).free_at(now, work.background))
            .max(self.holds().scoped(account, work, now))
            .unwrap_or(now + crate::usage::UNKNOWN_RESET);
        holds::release_at(at.max(now))
    }

    /// When every signed-in account of `agent`'s tool (any, for none or `auto`) is at its limit
    /// (LIMITS-18): the one free soonest, for a new job that names none to wait on as a limit's
    /// job waits ([`Jobs::wait_as_vetted`]), its `cap` the mark that it does.
    pub(super) fn no_room(
        &self,
        agent: Option<&str>,
        work: &Work,
    ) -> Option<(Tool, crate::config::Account, Option<Waiting>)> {
        let tools = match agent
            .map(str::trim)
            .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case("auto"))
        {
            None => Tool::ALL.to_vec(),
            Some(id) => vec![Tool::from_id(id)?],
        };
        let (home, now) = (login_home(), obi_protocol::time::now());
        let (tool, account) = tools
            .into_iter()
            .filter(|t| self.agent_config(*t).is_some())
            .flat_map(|tool| self.accounts(tool).into_iter().map(move |a| (tool, a)))
            .filter(|(tool, a)| {
                !self.being_changed(&a.id)
                    && signed_in(*tool, a, home.as_deref())
                    && (!work.background || self.takes_background(a))
                    && self.holds().held(&a.id, work, now).is_some()
            })
            .min_by_key(|(_, a)| self.limit_wait_until(&a.id, None, work))?;
        let wait = Waiting {
            kind: "cap".into(),
            text: String::new(),
            reserved: None,
            tool: None,
        };
        Some((tool, account, Some(wait)))
    }

    /// A new job waits as `vetted` found it must: for room or a yes (#872, CHATGPT-20), or, placed
    /// by [`Jobs::no_room`], `queued` with no slot and no card until its account's reset or another
    /// account with room (LIMITS-18, as LIMITS-5 and 15).
    pub(super) fn wait_as_vetted(&self, job: &mut Job, waiting: Option<Waiting>) {
        match waiting {
            Some(wait) if wait.kind == "cap" => {
                wait_for_reset(
                    job,
                    self.limit_wait_until(&job.account, None, &Work::of(job)),
                );
            }
            Some(wait) => {
                job.step = room_step(&wait);
                job.waiting = Some(wait);
            }
            None => {}
        }
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
            let work = Work::of(&job);
            if gone || self.holds().held(&job.account, &work, now).is_some() {
                let at = match gone {
                    true => now + crate::usage::UNKNOWN_RESET,
                    false => self.limit_wait_until(&job.account, None, &work),
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
                    if let Ok(change) = transition(job, &crate::job::Command::NextRound) {
                        change.apply(job);
                    }
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

    fn limit_source_is_connected(&self, agent: &str, id: &str) -> bool {
        let home = login_home();
        Tool::from_id(agent).is_some_and(|tool| {
            self.accounts(tool)
                .iter()
                .any(|account| account.id == id && signed_in(tool, account, home.as_deref()))
        })
    }

    /// Where `jobs`, stopped on `agent`'s account `from`, may move: a signed-in account of any
    /// tool that nothing obid knows holds (LIMITS-19: one that reports no usage too), the most room
    /// first ([`Jobs::by_room`], LIMITS-15), never one in its own sandbox, so work in Oby's box
    /// stays in it.
    async fn destinations(
        &self,
        agent: &str,
        from: &str,
        jobs: &[Job],
    ) -> Vec<(Tool, crate::config::Account)> {
        // A review can't change accounts.
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
        let (mut result, any) = (Vec::new(), Work::new(None, None, background));
        for tool in Tool::ALL {
            for account in self.accounts(tool) {
                let held = (self.holds()).held(&account.id, &any, now);
                if account.id == from
                    || self.being_changed(&account.id)
                    || !signed_in(tool, &account, home.as_deref())
                    || (background && !self.takes_background(&account))
                    || held.is_some()
                {
                    continue;
                }
                if tool == Tool::Claude && self.claude_login_ready(Some(&account)).await.is_err() {
                    continue;
                }
                result.push((tool, account));
            }
        }
        self.by_room(result, None)
    }

    /// `accounts` the most room first, for a move (LIMITS-15, 19) and a new job (USAGE-11): `first`
    /// (the owner's default) while it's there, then the fullest window each reports now (one reset
    /// since reads empty, USAGE-10) emptiest, those reporting none (an API key, a ChatGPT login,
    /// a Cursor figure) last. Ties keep their order (the tools', then the configuration's). A tool
    /// whose kept version is known bad goes after every other, its order within kept (CLIVERSIONS-8).
    pub(super) fn by_room(
        &self,
        mut accounts: Vec<(Tool, crate::config::Account)>,
        first: Option<&str>,
    ) -> Vec<(Tool, crate::config::Account)> {
        let now = obi_protocol::time::now();
        let rank = |(tool, account): &(Tool, crate::config::Account)| {
            let id = account.id.as_str();
            let ahead = (crate::cli_versions::known_bad(*tool), Some(id) != first);
            let usage = self.usage.get(id);
            let windows = [usage.five_hour_at(now), usage.seven_day_at(now)];
            let used = windows
                .into_iter()
                .flatten()
                .reduce(f64::max)
                .unwrap_or(1.0);
            // A Cursor figure is none beside another tool's; it orders Cursor's own (USAGE-9).
            match usage.advisory {
                true => (ahead, 1.0, used),
                false => (ahead, used, 0.0),
            }
        };
        // ponytail: no NaN is ever kept, so Equal never decides.
        accounts
            .sort_by(|a, b| (rank(a).partial_cmp(&rank(b))).unwrap_or(std::cmp::Ordering::Equal));
        accounts
    }

    /// What a job leaves behind of its old account's login when it moves ([`carry_over`]).
    pub(super) fn forget_moved_login(&self, id: &str) {
        let login = self.login_path(id);
        forget_login_secrets(&login);
        remove_inside(&login, ".claude.json");
        remove_inside(&login, "cli-config.json");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// USAGE-11: a new job on Auto takes the signed-in account with the most room, of any tool, as
    /// a move does (`Jobs::by_room`): fresh room first, one that reports none last; the owner's
    /// default goes first while it has room.
    #[test]
    fn usage_11_a_new_job_takes_the_account_with_the_most_room() {
        let root = std::env::temp_dir().join(format!("obid-usage-11-{}", uuid::Uuid::new_v4()));
        let folder = |id: &str, login: &str| {
            let dir = root.join("accounts").join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(login), "{}").unwrap();
            json!({"id": id, "name": id, "configDir": dir})
        };
        let (c1, c2) = (
            folder("c1", ".credentials.json"),
            folder("c2", ".credentials.json"),
        );
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://example.com/o/r.git"}],
                "agents": {"claude": {"bin": "/usr/bin/true", "accounts": [c1, c2]},
                           "codex": {"bin": "/usr/bin/true", "accounts": [folder("x", "auth.json")]}}})
            .to_string(),
        )
        .unwrap();
        let jobs = Jobs::open(config, crate::events::Events::new(), Err("none".into())).unwrap();
        let picked = || jobs.pick(None, None).unwrap().1.id;
        let read = |five, week| crate::usage::Usage {
            five_hour: Some(five),
            seven_day: Some(week),
            ..Default::default()
        };
        jobs.usage.record("c2", &read(0.5, 0.3));
        jobs.usage.record("x", &read(0.1, 0.2));
        assert_eq!(picked(), "x", "the emptiest fullest window, of any tool");
        jobs.usage.record("x", &read(0.1, 0.6));
        assert_eq!(picked(), "c2", "c1 reports nothing: it comes last");
        crate::settings::update(&jobs.config.home, |s| s.default_account = Some("c1".into()))
            .unwrap();
        assert_eq!(picked(), "c1", "the owner's default while it has room");
        jobs.mark_full("c1", None, obi_protocol::time::now());
        assert_eq!(picked(), "c2");
        // A 5-hour reading whose window reset since counts as empty (USAGE-10).
        let now = obi_protocol::time::now();
        let reset = crate::usage::Usage {
            five_hour_resets: Some(now - 60),
            ..read(0.92, 0.3)
        };
        jobs.usage.record("c2", &reset);
        jobs.usage.record("x", &read(0.6, 0.6));
        assert_eq!(
            picked(),
            "c2",
            "its 5-hour window reset: 30% is its fullest"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// USAGE-9: a Cursor figure doesn't rank Cursor before another tool's account (Cursor stays
    /// last on Auto); it only orders Cursor's own accounts, the emptier first.
    #[test]
    fn usage_9_a_cursor_figure_orders_only_cursors_own_accounts() {
        let root = std::env::temp_dir().join(format!("obid-usage-9-{}", uuid::Uuid::new_v4()));
        let folder = |id: &str, login: &str| {
            let dir = root.join("accounts").join(id);
            std::fs::create_dir_all(dir.join(login).parent().unwrap()).unwrap();
            std::fs::write(dir.join(login), r#"{"apiKey":"k"}"#).unwrap();
            json!({"id": id, "name": id, "configDir": dir})
        };
        let cursor = if cfg!(target_os = "macos") {
            ".cursor/auth.json"
        } else {
            ".config/cursor/auth.json"
        };
        // Claude Code's reading is 20 minutes old: not fresh, as a real Cursor figure never is.
        let at = obi_protocol::time::now() - 1200;
        let usage = json!({"c1": {"fiveHour": 0.3, "sevenDay": 0.3, "fiveHourObservedAt": at,
                "sevenDayObservedAt": at},
            "k1": {"sevenDay": 0.5, "advisory": true}, "k2": {"sevenDay": 0.1, "advisory": true}});
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::write(root.join("home/usage.json"), usage.to_string()).unwrap();
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://example.com/o/r.git"}],
                "agents": {"claude": {"bin": "/usr/bin/true", "accounts": [folder("c1", ".credentials.json")]},
                           "cursor": {"bin": "/usr/bin/true", "accounts": [folder("k1", cursor), folder("k2", cursor)]}}})
            .to_string(),
        )
        .unwrap();
        let jobs = Jobs::open(config, crate::events::Events::new(), Err("none".into())).unwrap();
        let picked = || jobs.pick(None, None).unwrap().1.id;
        assert_eq!(
            picked(),
            "c1",
            "a 10% Cursor figure doesn't beat Claude Code at 30%"
        );
        jobs.mark_full("c1", None, obi_protocol::time::now());
        assert_eq!(picked(), "k2", "the emptier Cursor account first");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// CLIVERSIONS-8: an account of a tool whose kept version is known bad comes after another
    /// tool's, the owner's default and the most room included, and is still taken when it's alone.
    #[test]
    fn cliversions_8_a_known_bad_tool_ranks_last() {
        let root =
            std::env::temp_dir().join(format!("obid-cliversions-8-{}", uuid::Uuid::new_v4()));
        let config = crate::config::Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://example.com/o/r.git"}],
                "agents": {"claude": {"bin": "/usr/bin/true", "accounts": [{"id": "c1", "name": "c1"}]},
                           "codex": {"bin": "/usr/bin/true", "accounts": [{"id": "x", "name": "x"}]}}})
            .to_string(),
        )
        .unwrap();
        let jobs = Jobs::open(config, crate::events::Events::new(), Err("none".into())).unwrap();
        let read = |five| crate::usage::Usage {
            five_hour: Some(five),
            seven_day: Some(five),
            ..Default::default()
        };
        jobs.usage.record("c1", &read(0.1));
        jobs.usage.record("x", &read(0.9));
        let ranked = |first| -> Vec<String> {
            let accounts = [Tool::Claude, Tool::Codex]
                .into_iter()
                .flat_map(|tool| jobs.accounts(tool).into_iter().map(move |a| (tool, a)))
                .collect();
            jobs.by_room(accounts, first)
                .into_iter()
                .map(|(_, a)| a.id)
                .collect()
        };
        assert_eq!(ranked(None), ["c1", "x"], "the most room first");
        crate::cli_versions::tests::KEPT
            .with_borrow_mut(|kept| kept.insert("claude", "2.1.300".into()));
        assert_eq!(ranked(None), ["x", "c1"], "known bad, however much room");
        assert_eq!(
            ranked(Some("c1")),
            ["x", "c1"],
            "known bad, the owner's default too"
        );
        let alone = jobs.by_room(
            vec![(Tool::Claude, jobs.accounts(Tool::Claude).remove(0))],
            None,
        );
        assert_eq!(alone[0].1.id, "c1", "taken when nothing else is there");
        let _ = std::fs::remove_dir_all(&root);
    }

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
