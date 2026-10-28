//! Decisions and inbox item lifecycle: Confirm/Start/Question answering, device-confirmed actions,
//! offer expiry, and persistent state saves.

use std::sync::Arc;

use axum::http::StatusCode;
use obi_protocol::time;
use serde_json::{Value, json};

use super::*;

impl Link {
    // -----------------------------------------------------------------------------------------
    // Decisions and the inbox

    /// `GET /v1/obid/decisions`, oldest first.
    pub fn decisions_json(&self) -> Value {
        Value::Array(
            self.lock()
                .decisions
                .iter()
                .map(Decision::to_json)
                .collect(),
        )
    }

    /// `POST /v1/obid/decisions` with `{"type": "host_approval", "fromOwner", "repo"}` (`repo` may
    /// be `*`): the owner's standing permission to approve `fromOwner`'s pull requests there on
    /// GitHub when Oby's review passes waits for their own Confirm in the inbox (#776), which keeps
    /// it: an API token alone never does. The `confirm` item. Every other decision is made by an
    /// answer in the inbox.
    pub fn propose_approval(&self, body: &[u8]) -> Result<Value, ApiError> {
        let map = object(body, &["type", "fromOwner", "repo"])?;
        let invalid = |m: &str| api(StatusCode::UNPROCESSABLE_ENTITY, m);
        if text_field(&map, "type")? != approval::HOST_APPROVAL {
            return Err(invalid(
                "type is host_approval: every other decision is made by an answer in the inbox.",
            ));
        }
        let owner = text_field(&map, "fromOwner")?;
        if !is_slug(owner) {
            return Err(invalid("fromOwner is a Sphere owner's id."));
        }
        let repo = text_field(&map, "repo")?;
        if repo != "*" && !crate::github::is_repo(repo) {
            return Err(invalid("repo is owner/name, or * for every repository."));
        }
        let item = approval::remember_item(owner, repo, None, time::now());
        Ok(self.add_confirm(item).to_json())
    }

    /// `DELETE /v1/obid/decisions/{id}`: `false` for one obid doesn't have.
    pub fn forget(&self, id: &str) -> bool {
        let decisions = {
            let mut state = self.lock();
            let before = state.decisions.len();
            state.decisions.retain(|d| d.id != id);
            if state.decisions.len() == before {
                return false;
            }
            state.decisions.clone()
        };
        self.save_decisions(&decisions);
        true
    }

    /// `GET /v1/inbox` (`done` false: open and put off, what needs the owner first, then FYI,
    /// then ignore, each the longest-waiting first) or `?state=done` (the latest settled first, at
    /// most `limit`).
    pub fn inbox_json(&self, done: bool, limit: usize) -> Value {
        self.settle_stale_starts();
        let state = self.lock();
        let mut items: Vec<&Item> = state
            .inbox
            .values()
            .filter(|i| i.done.is_some() == done)
            .collect();
        let sorts = self.sorts(&state.inbox);
        if done {
            items.sort_by(|a, b| b.done.cmp(&a.done).then_with(|| b.id.cmp(&a.id)));
            items.truncate(limit);
        } else {
            needs_you_first(&mut items, &sorts);
        }
        let shown = items.into_iter().map(|i| {
            let mut shown = i.to_json();
            shown["triage"] = sorts.get(i).into();
            shown
        });
        Value::Array(shown.collect())
    }

    /// Settles an open item with no answer: what it asked about is gone (a shell command whose
    /// turn ended). Nothing for an item that's done already.
    pub fn settle(&self, id: &str) {
        let settled = {
            let mut state = self.lock();
            state
                .inbox
                .get_mut(id)
                .filter(|i| i.done.is_none())
                .map(|item| {
                    item.done = Some(time::now());
                    item.clone()
                })
        };
        if let Some(item) = settled {
            self.save_item(&item);
        }
    }

    /// The open items of a job that's gone are done, with no answer.
    pub fn settle_job(&self, job: &str) {
        let now = time::now();
        let settled: Vec<Item> = {
            let mut state = self.lock();
            state
                .inbox
                .values_mut()
                .filter(|i| i.job == job && i.done.is_none())
                .map(|i| {
                    i.done = Some(now);
                    i.clone()
                })
                .collect()
        };
        for item in &settled {
            self.save_item(item);
        }
    }

    /// Something Oby's brain planned that persists or widens trust waits in the inbox for the
    /// owner's own `Confirm` (#546): the brain never carries it out itself. A conversation's shell
    /// keeps its `permission` items here too.
    pub fn add_confirm(&self, item: Item) -> Item {
        self.lock().inbox.insert(item.id.clone(), item.clone());
        self.save_item(&item);
        item
    }

    /// [`Link::add_confirm`], unless a `confirm` item with the same action and target is still
    /// open: that one stands, so a caller asking again and again adds nothing (crate::jobs:
    /// `confirm_past_safety`'s job, `offer_check`'s repository).
    pub fn add_confirm_once(&self, item: Item) -> Item {
        let what = |i: &Item| {
            i.confirm
                .as_ref()
                .map(|c| (c.action.clone(), c.target.clone()))
        };
        {
            let mut state = self.lock();
            if let Some(open) = state
                .inbox
                .values()
                .find(|i| i.kind == "confirm" && i.done.is_none() && what(i) == what(&item))
            {
                return open.clone();
            }
            state.inbox.insert(item.id.clone(), item.clone());
        }
        self.save_item(&item);
        item
    }

    /// [`Link::add_confirm`] in place of every open `confirm` item with the same action and
    /// target, which are done unanswered: the owner asked for this one (`POST /v1/repos/check`),
    /// so only it stands.
    pub fn add_confirm_instead(&self, item: Item) -> Item {
        let what = |i: &Item| {
            i.confirm
                .as_ref()
                .map(|c| (c.action.clone(), c.target.clone()))
        };
        let closed: Vec<Item> = {
            let mut state = self.lock();
            let now = time::now();
            let closed = state
                .inbox
                .values_mut()
                .filter(|i| i.kind == "confirm" && i.done.is_none() && what(i) == what(&item))
                .map(|i| {
                    i.done = Some(now);
                    i.clone()
                })
                .collect();
            state.inbox.insert(item.id.clone(), item.clone());
            closed
        };
        for old in closed.iter().chain([&item]) {
            self.save_item(old);
        }
        item
    }

    /// `POST /v1/inbox/{id}/answer` on a `confirm` item: `Confirm` carries out what its title
    /// says; `Cancel`, done or later do nothing.
    pub(super) async fn answer_confirm(
        self: &Arc<Self>,
        mut item: Item,
        answer: Option<String>,
        later: Option<i64>,
        signed_at: Option<i64>,
    ) -> Result<Value, ApiError> {
        if let Some(at) = later {
            item.later = Some(at);
        } else {
            if let Some(answer) = &answer {
                if answer != inbox::CONFIRM && answer != inbox::CANCEL {
                    return Err(api(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        format!("The answer is {} or {}.", inbox::CONFIRM, inbox::CANCEL),
                    ));
                }
                // Cancel on Oby's ask for a repository's check (#1062): with none kept there, it's
                // No check, remembered, so Oby never asks about it again; with one, that stays.
                if answer == inbox::CANCEL
                    && let Some(repo) = item
                        .confirm
                        .as_ref()
                        .filter(|c| c.action == inbox::action::SET_CHECK)
                        .map(|c| c.target.clone())
                    && self
                        .config
                        .repos
                        .find(&repo)
                        .is_some_and(|r| r.check.is_none())
                    && let Err(e) = crate::settings::no_check(&self.config.home, &repo, true)
                {
                    tracing::warn!("No check for {repo} isn't remembered: {e}");
                }
                if answer == inbox::CONFIRM {
                    let done = match item.confirm.clone() {
                        Some(confirmed) => self.carry_out(*confirmed, signed_at).await,
                        None => Err(api(StatusCode::CONFLICT, "That item says nothing to do.")),
                    };
                    // What it was for moved on (the permission was answered, the job ended): the
                    // item is closed with why, never left open to fail again.
                    if let Err(why) = done {
                        item.done = Some(time::now());
                        item.answer = Some(answer.clone());
                        item.text = format!("{} It couldn't happen: {}", item.text, why.message);
                        self.lock().inbox.insert(item.id.clone(), item.clone());
                        self.save_item(&item);
                        return Err(why);
                    }
                }
            }
            item.done = Some(time::now());
            item.answer = answer;
        }
        self.lock().inbox.insert(item.id.clone(), item.clone());
        self.save_item(&item);
        Ok(item.to_json())
    }

    /// What a confirmed `confirm` item does: one dispatcher, a case per action. `signed_at`: when
    /// the owner's device signed the Confirm.
    pub(super) async fn carry_out(
        self: &Arc<Self>,
        confirmed: inbox::Confirmed,
        signed_at: Option<i64>,
    ) -> Result<(), ApiError> {
        let target = confirmed.target.as_str();
        match confirmed.action.as_str() {
            inbox::action::ALWAYS_ALLOW => {
                use crate::jobs::PermissionRefused;
                let permission = confirmed.payload["permission"].as_str().unwrap_or("");
                match self.jobs.answer_permission(
                    target,
                    permission,
                    crate::permissions::Decision::Always,
                    None,
                ) {
                    Ok(_) => Ok(()),
                    Err(PermissionRefused::NoJob) => {
                        Err(api(StatusCode::CONFLICT, "Its job is gone."))
                    }
                    Err(PermissionRefused::NotWaiting(why)) => Err(api(StatusCode::CONFLICT, why)),
                    Err(PermissionRefused::Refused(why)) => {
                        Err(api(StatusCode::UNPROCESSABLE_ENTITY, why))
                    }
                }
            }
            inbox::action::SET_MODE_BYPASS => {
                if !self.jobs.get(target).is_some_and(|j| j.status.is_active()) {
                    return Err(api(
                        StatusCode::CONFLICT,
                        "That job isn't running any more.",
                    ));
                }
                self.jobs
                    .set_mode(target, crate::permissions::Mode::Bypass)
                    .map(|_| ())
                    .ok_or_else(|| api(StatusCode::CONFLICT, "That job is gone."))
            }
            inbox::action::START_JOB => {
                let new: crate::jobs::NewJob =
                    serde_json::from_value(confirmed.payload["job"].clone())
                        .map_err(|_| api(StatusCode::CONFLICT, "That job's plan was lost."))?;
                self.jobs
                    .create(new)
                    .map(|_| ())
                    .map_err(|e| api(StatusCode::UNPROCESSABLE_ENTITY, e.0))
            }
            // Made before 2026-10-18 for one more round: the words now start a follow-up, as any
            // words for a job that ended do.
            inbox::action::MESSAGE_JOB => {
                let words = confirmed.payload["words"].as_str().unwrap_or("");
                match self
                    .jobs
                    .message(target, words, crate::jobs::Source::Confirmed)
                {
                    Some(Ok(_)) => Ok(()),
                    Some(Err(refused)) => Err(api(StatusCode::UNPROCESSABLE_ENTITY, refused.0)),
                    None => Err(api(StatusCode::CONFLICT, "That job is gone.")),
                }
            }
            inbox::action::DISMISS_JOB => match self.jobs.dismiss(target) {
                Some(Ok(_)) => Ok(()),
                Some(Err(refused)) => Err(api(StatusCode::CONFLICT, refused.0)),
                None => Err(api(StatusCode::CONFLICT, "That job is gone.")),
            },
            inbox::action::ADD_REPOSITORY => {
                // obid's own words of the review request (crate::github_work's typed fields).
                let (repo, number) = (
                    confirmed.payload["repo"].as_str().unwrap_or_default(),
                    confirmed.payload["number"].as_u64().unwrap_or_default(),
                );
                if self
                    .lock()
                    .inbox
                    .get(target)
                    .is_some_and(|i| i.done.is_some())
                {
                    return Err(api(StatusCode::CONFLICT, "That review request is settled."));
                }
                let (job, _) =
                    crate::github_work::start_adding(&self.jobs, &self.config, repo, number)
                        .await?;
                self.github_started(target, &job.id);
                Ok(())
            }
            // The owner's standing permission to approve a teammate's pull requests on GitHub, kept
            // only on their own tap (#776), in place of one for the same owner and repository.
            inbox::action::REMEMBER_APPROVAL => {
                let text = |key: &str| confirmed.payload[key].as_str().unwrap_or("").to_owned();
                let (from_owner, repo) = (text("fromOwner"), text("repo"));
                if !is_slug(&from_owner) || !(repo == "*" || crate::github::is_repo(&repo)) {
                    return Err(api(
                        StatusCode::CONFLICT,
                        "That permission's plan was lost.",
                    ));
                }
                let kept = {
                    let mut state = self.lock();
                    state.decisions.retain(|d| {
                        !(d.kind == approval::HOST_APPROVAL
                            && d.from_owner == from_owner
                            && d.repo == repo)
                    });
                    state.decisions.push(Decision {
                        id: decisions::new_id(),
                        kind: approval::HOST_APPROVAL.into(),
                        from_owner,
                        repo,
                        decision: "accept".into(),
                        created: time::now(),
                        inbox_item: target.to_owned(),
                        ..Decision::default()
                    });
                    state.decisions.clone()
                };
                self.save_decisions(&kept);
                Ok(())
            }
            inbox::action::APPROVE_ON_HOST => {
                self.confirmed_approval(target, &confirmed.payload, signed_at)
                    .await
            }
            inbox::action::CREATE_ROUTINE => {
                let draft: crate::routines::Draft =
                    serde_json::from_value(confirmed.payload["routine"].clone())
                        .map_err(|_| api(StatusCode::CONFLICT, "That routine's plan was lost."))?;
                let routines = self
                    .jobs
                    .routines()
                    .ok_or_else(|| api(StatusCode::CONFLICT, "obid keeps no routines."))?;
                routines
                    .create(&self.jobs, draft, time::now())
                    .map(|_| ())
                    .map_err(|why| api(StatusCode::UNPROCESSABLE_ENTITY, why))
            }
            inbox::action::CHANGE_ROUTINE => {
                use crate::routines::{Change, NotRun};
                let routines = self
                    .jobs
                    .routines()
                    .ok_or_else(|| api(StatusCode::CONFLICT, "obid keeps no routines."))?;
                let gone = || api(StatusCode::CONFLICT, "That routine is gone.");
                // The card named its mode, tool, account and check: one changed since is a routine
                // the owner didn't see.
                let current = routines.get(target).ok_or_else(gone)?;
                let payload = &confirmed.payload;
                if payload["mode"] != serde_json::json!(current.mode)
                    || payload["agent"] != serde_json::json!(current.agent)
                    || payload["account"] != serde_json::json!(current.account)
                    || payload["check"] != serde_json::json!(current.check)
                {
                    return Err(api(
                        StatusCode::CONFLICT,
                        "The routine's mode, tool, account or check changed since Oby asked: ask again.",
                    ));
                }
                let now = time::now();
                let change = |change: Change| match routines.change(&self.jobs, target, change, now)
                {
                    None => Err(gone()),
                    Some(Err(why)) => Err(api(StatusCode::UNPROCESSABLE_ENTITY, why)),
                    Some(Ok(_)) => Ok(()),
                };
                match confirmed.payload["action"].as_str() {
                    Some("resume") => change(Change {
                        enabled: Some(true),
                        ..Default::default()
                    }),
                    Some("reschedule") => change(Change {
                        schedule: serde_json::from_value(confirmed.payload["schedule"].clone())
                            .ok(),
                        zone: confirmed.payload["zone"].as_str().map(str::to_owned),
                        ..Default::default()
                    }),
                    Some("delete") => routines.delete(target).then_some(()).ok_or_else(gone),
                    Some("run") => {
                        // Off the async threads: a run takes the routines' firing lock.
                        let (routines, target) = (routines.clone(), target.to_owned());
                        let ran = tokio::task::spawn_blocking(move || routines.run(&target, now))
                            .await
                            .map_err(|_| {
                                api(StatusCode::INTERNAL_SERVER_ERROR, "The run stopped.")
                            })?;
                        match ran {
                            Ok(_) => Ok(()),
                            Err(NotRun::NoRoutine) => Err(gone()),
                            Err(NotRun::Busy) => Err(api(
                                StatusCode::CONFLICT,
                                "Its previous run is still going.",
                            )),
                            Err(NotRun::Refused(why)) => {
                                Err(api(StatusCode::UNPROCESSABLE_ENTITY, why))
                            }
                        }
                    }
                    _ => Err(api(StatusCode::CONFLICT, "That change was lost.")),
                }
            }
            inbox::action::RETRY_JOB => {
                if self.jobs.get(target).is_some_and(|j| j.status.is_active()) {
                    return Err(api(StatusCode::CONFLICT, "That job is running again."));
                }
                match self.jobs.message(
                    target,
                    crate::brain::actions::TRY_AGAIN,
                    crate::jobs::Source::Confirmed,
                ) {
                    Some(Ok(_)) => Ok(()),
                    Some(Err(refused)) => Err(api(StatusCode::UNPROCESSABLE_ENTITY, refused.0)),
                    None => Err(api(StatusCode::CONFLICT, "That job is gone.")),
                }
            }
            // Proposed against the job as it was: after another round, a move or a new mode,
            // the title no longer says what Confirm would do.
            inbox::action::CONTINUE_JOB | inbox::action::DELETE_JOB | inbox::action::CHANGE_JOB
                if self
                    .jobs
                    .get(target)
                    .is_some_and(|j| j.snapshot() != confirmed.payload["snapshot"]) =>
            {
                Err(api(
                    StatusCode::CONFLICT,
                    "The job changed since Oby asked (another round, a move or a new mode): ask Oby again.",
                ))
            }
            inbox::action::CONTINUE_JOB => {
                let words = confirmed.payload["words"].as_str();
                // Words Oby proposed, from a conversation the Confirm doesn't name (crate::secretspec).
                let foreign = Some("words Oby proposed, confirmed in the inbox".to_owned());
                match self
                    .jobs
                    .continue_job(target, words, "on your Confirm", foreign)
                {
                    Some(Ok(_)) => Ok(()),
                    Some(Err(refused)) => Err(api(StatusCode::CONFLICT, refused.0)),
                    None => Err(api(StatusCode::CONFLICT, "That job is gone.")),
                }
            }
            inbox::action::DELETE_JOB => match self.jobs.delete(target).await {
                Some(Ok(_)) => {
                    self.settle_job(target);
                    Ok(())
                }
                Some(Err(refused)) => Err(api(StatusCode::CONFLICT, refused.0)),
                None => Err(api(StatusCode::CONFLICT, "That job is gone.")),
            },
            inbox::action::CHANGE_JOB => {
                let text = |key: &str| confirmed.payload[key].as_str();
                if let Some(mode) = text("mode") {
                    let mode = crate::permissions::Mode::parse(mode)
                        .ok_or_else(|| api(StatusCode::CONFLICT, "That mode isn't one."))?;
                    self.jobs
                        .set_mode(target, mode)
                        .ok_or_else(|| api(StatusCode::CONFLICT, "That job is gone."))?;
                    self.jobs.note_change(
                        target,
                        format!("Mode changed to {} on your Confirm", mode.title()),
                    );
                }
                if text("agent").is_some() || text("account").is_some() {
                    match self.jobs.change(
                        target,
                        text("agent"),
                        text("account"),
                        "on your Confirm",
                    ) {
                        Some(Ok(_)) => {}
                        Some(Err(refused)) => return Err(api(StatusCode::CONFLICT, refused.0)),
                        None => return Err(api(StatusCode::CONFLICT, "That job is gone.")),
                    }
                }
                Ok(())
            }
            // A repository's check Oby asked about (#1062), kept on the owner's tap: the command
            // made its check, or with none the one it had (`was`) taken out. Either way Oby may
            // ask there again, and No check is forgotten.
            inbox::action::SET_CHECK => {
                let payload = &confirmed.payload;
                let check = match &payload["check"] {
                    Value::String(check) if crate::brain::plan::is_check(check) => {
                        Some(check.as_str())
                    }
                    Value::Null => None,
                    _ => return Err(api(StatusCode::CONFLICT, "That check's plan was lost.")),
                };
                let now = self.config.repos.find(target).and_then(|r| r.check);
                if check.is_none() && now.as_deref() != payload["was"].as_str() {
                    return Err(api(
                        StatusCode::CONFLICT,
                        format!("{target}'s check changed since Oby asked: ask again."),
                    ));
                }
                self.config
                    .repos
                    .set_check(target, check)
                    .map_err(|why| api(StatusCode::CONFLICT, why))?;
                if let Err(e) = crate::settings::no_check(&self.config.home, target, false) {
                    tracing::warn!("No check for {target} isn't forgotten: {e}");
                }
                Ok(())
            }
            // The ones that answer an inbox item, as the owner would have.
            inbox::action::ACCEPT
            | inbox::action::ACCEPT_REMEMBER
            | inbox::action::DECLINE_REMEMBER
            | inbox::action::ANSWER_QUESTION
            | inbox::action::START
            | inbox::action::ANSWER_ITEM => {
                let answer = match confirmed.action.as_str() {
                    inbox::action::ACCEPT => inbox::ACCEPT.to_owned(),
                    inbox::action::ACCEPT_REMEMBER => inbox::ACCEPT_REMEMBER.to_owned(),
                    inbox::action::DECLINE_REMEMBER => inbox::DECLINE_REMEMBER.to_owned(),
                    inbox::action::START => inbox::START.to_owned(),
                    inbox::action::ANSWER_QUESTION => {
                        confirmed.payload["words"].as_str().unwrap_or("").to_owned()
                    }
                    _ => confirmed.payload["answer"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned(),
                };
                // A confirm item never confirms another.
                if self
                    .lock()
                    .inbox
                    .get(target)
                    .is_none_or(|i| i.kind == "confirm")
                {
                    return Err(api(StatusCode::CONFLICT, "That item is gone."));
                }
                let body = json!({"answer": answer}).to_string();
                Box::pin(self.answer(target, body.as_bytes()))
                    .await
                    .map(|_| ())
            }
            other => Err(api(
                StatusCode::CONFLICT,
                format!("obid doesn't know the action {other:?}."),
            )),
        }
    }

    /// A `start` item for a follow-up whose work merged after it was proposed (the first job's pull
    /// request or a follow-up's), or an offer left a day ([`OFFER_SECONDS`]), is done by itself,
    /// with no options, its text saying so above the owner's words. Checked whenever the inbox is
    /// read; nothing starts.
    pub(super) fn settle_stale_starts(&self) {
        let open: Vec<Item> = self
            .lock()
            .inbox
            .values()
            .filter(|i| i.kind == "start" && i.done.is_none())
            .cloned()
            .collect();
        if open.is_empty() {
            return;
        }
        let jobs = self.jobs.list(usize::MAX);
        let now = time::now();
        let mut closed = Vec::new();
        for item in open {
            // An offer nobody started in a day: the phone and the briefing stop asking.
            if expired(&item, now) {
                closed.push(closed_start(item, OFFER_EXPIRED, now));
                continue;
            }
            let Some(of) = item.start.as_ref().and_then(|s| s["follow_up_of"].as_str()) else {
                continue;
            };
            // The work it would follow up merged after it was proposed: the first job's pull
            // request, or one of its follow-ups'. Anything else (a follow-up that ended, even
            // failed) leaves it open, its words and Start as they were.
            let Some(merged) = jobs
                .iter()
                .filter(|j| j.id == of || j.follow_up_of.as_deref() == Some(of))
                .find(|j| {
                    j.pull_request
                        .as_ref()
                        .and_then(|pr| pr.merged_at)
                        .is_some_and(|at| at > item.created)
                })
            else {
                continue;
            };
            let why = format!(
                "Closed by Oby: {}'s work merged after this was proposed.",
                merged.short_title()
            );
            closed.push(closed_start(item, &why, now));
        }
        {
            let mut state = self.lock();
            for item in &closed {
                // Answered meanwhile: the owner's answer stands.
                if state.inbox.get(&item.id).is_some_and(|i| i.done.is_none()) {
                    state.inbox.insert(item.id.clone(), item.clone());
                }
            }
        }
        for item in &closed {
            self.save_item(item);
        }
    }

    /// Closes the open `start` items offered in `conversation`, which was deleted: nothing of it
    /// starts any more. Under the lock, so a Start that claimed one first stands.
    pub fn close_offers(&self, conversation: &str) {
        let now = time::now();
        let closed: Vec<Item> = {
            let mut state = self.lock();
            let open: Vec<Item> = state
                .inbox
                .values()
                .filter(|i| {
                    i.kind == "start"
                        && i.done.is_none()
                        && i.conversation().as_deref() == Some(conversation)
                })
                .cloned()
                .collect();
            open.into_iter()
                .map(|item| {
                    let item = closed_start(item, OFFER_GONE, now);
                    state.inbox.insert(item.id.clone(), item.clone());
                    item
                })
                .collect()
        };
        for item in &closed {
            self.save_item(item);
        }
    }

    /// Work the brain offered (`oby_offer`) waits for the owner's Start, as a `start` item
    /// (Engine/Inbox.swift, `Inbox.proposal`): nothing starts until they say so.
    /// `conversation`: the one it was proposed in, which shows it (`Item::conversation`);
    /// `offer`: `{"id", "what"}`, the card it's on there (`Item::to_json`).
    pub fn propose(
        &self,
        new: &crate::jobs::NewJob,
        title: String,
        text: String,
        conversation: &str,
        offer: Value,
    ) -> Item {
        let mut start = serde_json::to_value(new).unwrap_or_default();
        start["proposedIn"] = json!(conversation);
        start["offer"] = offer;
        let item = Item {
            id: format!("start-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            kind: "start".into(),
            job: String::new(),
            title,
            text,
            options: vec![inbox::START.into(), inbox::CANCEL_START.into()],
            details: Some(vec![
                "Starts it now, as planned.".into(),
                "Drops the plan. Nothing starts; say it another way.".into(),
            ]),
            recommended: Some(0),
            created: time::now(),
            done: None,
            later: None,
            answer: None,
            obi: None,
            plugin: None,
            arrived: Some(inbox::arrival()),
            confirm: None,
            start: Some(Box::new(start)),
            github: None,
            watch: None,
        };
        self.lock().inbox.insert(item.id.clone(), item.clone());
        self.save_item(&item);
        item
    }

    /// Start on a `start` item `answer_as` claimed: queues the job as planned. What couldn't start
    /// leaves it open again.
    pub(super) fn start_item(&self, mut item: Item) -> Result<Value, ApiError> {
        let open_again = |why: ApiError| {
            self.lock().inbox.insert(item.id.clone(), item.clone());
            why
        };
        let new: crate::jobs::NewJob = item
            .start
            .clone()
            .and_then(|s| serde_json::from_value(*s).ok())
            .ok_or_else(|| open_again(api(StatusCode::CONFLICT, "That plan was lost.")))?;
        // A follow-up of a job whose follow-up is working already: that one hears it (the owner's
        // own Start), and the item is done, never a second. Only while it's at work with nothing
        // waiting, checked as the words are taken (`hear_at_work`): a permission or plan would
        // take them as its answer, so the item stays open until then.
        let following = new
            .follow_up_of
            .as_deref()
            .and_then(|of| self.jobs.following(of));
        let job = match following {
            // Heard as they are (crate::secretspec, `Jobs::hear_plan`).
            Some(next) => match self.jobs.hear_plan(&next.id, &new) {
                Some(Ok(job)) => job,
                Some(Err(e)) => {
                    return Err(open_again(api(
                        StatusCode::CONFLICT,
                        format!(
                            "{} (job {}) is following it up already. {} Start it again once it's at work, or tell that job.",
                            next.short_title(),
                            next.id,
                            e.0
                        ),
                    )));
                }
                None => return Err(open_again(api(StatusCode::CONFLICT, "That job is gone."))),
            },
            None => self
                .jobs
                .create(new)
                .map_err(|e| open_again(api(StatusCode::UNPROCESSABLE_ENTITY, e.0)))?,
        };
        item.job = job.id;
        item.done = Some(time::now());
        item.answer = Some(inbox::START.into());
        self.lock().inbox.insert(item.id.clone(), item.clone());
        self.save_item(&item);
        Ok(item.to_json())
    }

    /// The inbox item `id`, whatever its state.
    pub fn item(&self, id: &str) -> Option<Item> {
        self.lock().inbox.get(id).cloned()
    }

    /// The items waiting for the owner now (not done, and not put off past `now`), what needs them
    /// first and each group in the order they came: what the brain is told about the inbox (#546).
    pub fn waiting_items(&self, now: i64) -> Vec<Item> {
        self.settle_stale_starts();
        let state = self.lock();
        let mut items: Vec<Item> = state
            .inbox
            .values()
            .filter(|i| i.done.is_none() && i.later.is_none_or(|later| later <= now))
            .cloned()
            .collect();
        needs_you_first(&mut items, &self.sorts(&state.inbox));
        items
    }

    /// `POST /v1/inbox/{id}/answer`, from anything but the owner's own tap: a `Confirm` on a
    /// `confirm` item is refused (#776). Agents, oby-mcp, the brain and obid's own callers come
    /// this way.
    pub async fn answer(self: &Arc<Self>, id: &str, body: &[u8]) -> Result<Value, ApiError> {
        let answered = self.answer_as(id, body, None).await;
        answered.map(|shown| self.with_triage(shown))
    }

    /// An answer's item with its `triage`, as every item the API shows (crate::triage).
    pub(super) fn with_triage(&self, mut shown: Value) -> Value {
        if let Some(item) = shown["id"].as_str().and_then(|id| self.item(id)) {
            shown["triage"] = self.triage_of(&item).into();
        }
        shown
    }

    /// `POST /v1/inbox/{id}/answer` after a registered device signed a Confirm of item `id`
    /// whose title was `approved` (crate::approval, `oby-confirm`): checked by the route before
    /// this, and here again against the item as it's claimed.
    pub async fn answer_confirmed(
        self: &Arc<Self>,
        id: &str,
        body: &[u8],
        approved: &str,
        signed_at: i64,
    ) -> Result<Value, ApiError> {
        let answered = self.answer_as(id, body, Some((approved, signed_at))).await;
        answered.map(|shown| self.with_triage(shown))
    }

    pub(super) async fn answer_as(
        self: &Arc<Self>,
        id: &str,
        body: &[u8],
        approved: Option<(&str, i64)>,
    ) -> Result<Value, ApiError> {
        let map = object(body, &["answer", "done", "later"])?;
        let one_of = "The body is exactly one of {\"answer\": …}, {\"done\": true} or {\"later\": \"<date>\"}.";
        if map.len() != 1 {
            return Err(api(StatusCode::UNPROCESSABLE_ENTITY, one_of));
        }
        enum Given {
            Answer(String),
            Done,
            Later(i64),
        }
        let given = match map.iter().next() {
            Some((k, Value::String(a))) if k == "answer" => {
                let length = a.chars().count();
                if a.trim().is_empty() || length > 2000 {
                    return Err(api(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "An answer is 1 to 2,000 characters.",
                    ));
                }
                Given::Answer(a.clone())
            }
            Some((k, Value::Bool(true))) if k == "done" => Given::Done,
            Some((k, Value::String(t))) if k == "later" => {
                Given::Later(parse_later(t).ok_or_else(|| {
                    api(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "later is a date, like 2026-10-10T10:12:00Z.",
                    )
                })?)
            }
            _ => return Err(api(StatusCode::UNPROCESSABLE_ENTITY, one_of)),
        };
        // What Oby's brain planned that only the owner may do: carried out on their Confirm.
        // A Confirm claims the item under the lock, so a second one at the same moment finds it
        // done and carries out nothing (review on #552).
        let confirm = {
            let mut state = self.lock();
            let item = state
                .inbox
                .get(id)
                .filter(|i| i.done.is_none() && i.kind == "confirm")
                .cloned();
            let confirming = matches!(&given, Given::Answer(a) if a == inbox::CONFIRM);
            // Only the owner's tap confirms: a device's signature over this item's own title,
            // checked before the item is claimed, so a refused Confirm leaves it open (#776).
            if let Some(open) = &item
                && confirming
                && approved.map(|(title, _)| title) != Some(open.title.as_str())
            {
                return Err(api(StatusCode::FORBIDDEN, NEEDS_YOUR_TAP));
            }
            if item.is_some()
                && confirming
                && let Some(kept) = state.inbox.get_mut(id)
            {
                kept.done = Some(time::now());
            }
            item
        };
        if let Some(item) = confirm {
            return match given {
                Given::Answer(answer) => {
                    let signed_at = approved.map(|(_, at)| at);
                    self.answer_confirm(item, Some(answer), None, signed_at)
                        .await
                }
                Given::Done => self.answer_confirm(item, None, None, None).await,
                Given::Later(at) => self.answer_confirm(item, None, Some(at), None).await,
            };
        }
        // A question's item, answered or declined: the words pass the publish gate first, with
        // no lock held.
        let question = self
            .lock()
            .inbox
            .get(id)
            .filter(|i| i.done.is_none() && i.obi.as_ref().is_some_and(|o| o["type"] == "question"))
            .cloned();
        if let Some(item) = question {
            match given {
                Given::Answer(words) => return self.answer_question(item, Some(words)).await,
                Given::Done => return self.answer_question(item, None).await,
                Given::Later(_) => {}
            }
        }
        // The owner's review (crate::review): Approve posts it on GitHub, with no lock held. The
        // answer claims it first, and the claim (`watch.posting`) is on disk before gh is asked, so
        // a second Approve, or one after obid stopped mid-post, never posts it twice; when nothing
        // was posted it's open again.
        let approving = {
            let mut state = self.lock();
            let item = state
                .inbox
                .get(id)
                .filter(|i| {
                    i.done.is_none()
                        && i.kind == "listen"
                        && i.options.iter().any(|o| o == crate::review::APPROVE)
                })
                .cloned();
            match (item, &given) {
                (Some(item), Given::Answer(a)) if a == crate::review::APPROVE => {
                    if item
                        .watch
                        .as_deref()
                        .is_some_and(|w| !w["posting"].is_null())
                    {
                        return Err(api(
                            StatusCode::CONFLICT,
                            "Oby was posting this review when it stopped, or still is: check the pull request on GitHub, then Dismiss it.",
                        ));
                    }
                    let mut claimed = item.clone();
                    if let Some(watch) = claimed.watch.as_mut() {
                        watch["posting"] = serde_json::json!(time::format(time::now()));
                    }
                    state.inbox.insert(claimed.id.clone(), claimed.clone());
                    Some((item, claimed))
                }
                _ => None,
            }
        };
        if let Some((open, claimed)) = approving {
            // Not on disk: nothing is posted (a stop could then post it twice).
            if let Err(why) = self.keep_item(&claimed) {
                self.lock().inbox.insert(open.id.clone(), open.clone());
                self.save_item(&open);
                return Err(api(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Nothing was posted: obid couldn't keep the claim ({why})."),
                ));
            }
            return match crate::review::post_review_with_gh(&self.config.home, &claimed).await {
                Ok(()) => {
                    let mut item = claimed;
                    item.done = Some(time::now());
                    item.answer = Some(crate::review::APPROVE.into());
                    self.lock().inbox.insert(item.id.clone(), item.clone());
                    self.save_item(&item);
                    Ok(item.to_json())
                }
                Err(why) => {
                    // Open again, unless the owner dismissed it meanwhile.
                    let dismissed = {
                        let mut state = self.lock();
                        let dismissed = state.inbox.get(&open.id).is_some_and(|i| i.done.is_some());
                        if !dismissed {
                            state.inbox.insert(open.id.clone(), open.clone());
                        }
                        dismissed
                    };
                    if !dismissed {
                        self.save_item(&open);
                    }
                    Err(api(
                        StatusCode::BAD_GATEWAY,
                        format!("Nothing was posted: {why}."),
                    ))
                }
            };
        }
        // A merge card (crate::merge): Merge, Merge and remember, or Ask a teammate and remember,
        // claims the item under the lock (done), then acts with no lock held, as Approve does
        // above.
        let merge = {
            let mut state = self.lock();
            let item = state
                .inbox
                .get(id)
                .filter(|i| i.done.is_none() && i.kind == "merge")
                .cloned();
            if item.is_some()
                && matches!(&given, Given::Answer(a)
                    if a == crate::merge::MERGE
                        || a == crate::merge::MERGE_AND_REMEMBER
                        || a == crate::merge::ASK_TEAMMATE)
                && let Some(kept) = state.inbox.get_mut(id)
            {
                kept.done = Some(time::now());
            }
            item
        };
        if let Some(item) = merge {
            return match given {
                Given::Answer(answer) => self.answer_merge(item, Some(answer), None).await,
                Given::Done => self.answer_merge(item, None, None).await,
                Given::Later(at) => self.answer_merge(item, None, Some(at)).await,
            };
        }
        // An offer of GitHub work: its start reads GitHub first, with no lock held, so its start
        // answer claims the item under the lock (done), as Start does: nothing settles it
        // meanwhile, and a second answer finds it done.
        let github = {
            let mut state = self.lock();
            let item = state
                .inbox
                .get(id)
                .filter(|i| i.done.is_none() && i.kind == "github")
                .cloned();
            if let Some(item) = &item
                && matches!(&given, Given::Answer(a) if item.options.first() == Some(a))
                && let Some(kept) = state.inbox.get_mut(id)
            {
                kept.done = Some(time::now());
            }
            item
        };
        // Approving a teammate's pull request on GitHub, asked before the owner's Confirm was needed.
        let host_approval = self
            .lock()
            .inbox
            .get(id)
            .filter(|i| {
                i.done.is_none()
                    && i.obi
                        .as_ref()
                        .is_some_and(|o| o["type"] == approval::HOST_APPROVAL)
            })
            .cloned();
        if let Some(item) = host_approval {
            return match given {
                Given::Answer(answer) => self.answer_host_approval(item, Some(answer), None).await,
                Given::Done => self.answer_host_approval(item, None, None).await,
                Given::Later(at) => self.answer_host_approval(item, None, Some(at)).await,
            };
        }
        if let Some(item) = github {
            return match given {
                Given::Answer(answer) => self.answer_github(item, Some(answer), None).await,
                Given::Done => self.answer_github(item, None, None).await,
                Given::Later(at) => self.answer_github(item, None, Some(at)).await,
            };
        }
        let now = time::now();
        let (item, exchange, decisions) = {
            let mut state = self.lock();
            let item = state
                .inbox
                .get(id)
                .cloned()
                .ok_or_else(|| api(StatusCode::NOT_FOUND, "There's no such inbox item."))?;
            if item.done.is_some() {
                return Err(api(StatusCode::CONFLICT, "That item is done already."));
            }
            if matches!(
                item.kind.as_str(),
                "permission" | "plan" | inbox::QUESTION_KIND
            ) {
                drop(state);
                return match given {
                    Given::Answer(answer) => self.answer_permission_item(item, Some(answer), None),
                    Given::Done => self.answer_permission_item(item, None, None),
                    Given::Later(at) => self.answer_permission_item(item, None, Some(at)),
                };
            }
            // A crashed job's `failed` item (#911): Try again runs it again, Dismiss lets the job
            // go. The answer claims it under the lock, so a second tap carries nothing out.
            if item.kind == "failed" {
                let mut item = item;
                match &given {
                    Given::Later(at) => item.later = Some(*at),
                    Given::Done => item.done = Some(now),
                    Given::Answer(a) if a == crate::job::TRY_AGAIN || a == inbox::DISMISS => {
                        item.answer = Some(a.clone());
                        item.done = Some(now);
                    }
                    Given::Answer(_) => {
                        return Err(api(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "The answer is Try again or Dismiss.",
                        ));
                    }
                }
                state.inbox.insert(item.id.clone(), item.clone());
                drop(state);
                self.save_item(&item);
                let carried = match item.answer.as_deref() {
                    // An approval obid couldn't dismiss: the owner removes it on GitHub; nothing
                    // else is carried out.
                    _ if approval::is_dismissal_item(&item) => return Ok(item.to_json()),
                    // Oby's review that couldn't run: Try again is its review's, never the job's.
                    Some(crate::job::TRY_AGAIN) if inbox::is_review_item(&item) => {
                        self.jobs.review_again(&item.job)
                    }
                    Some(crate::job::TRY_AGAIN) => self.jobs.message(
                        &item.job,
                        crate::brain::actions::TRY_AGAIN,
                        crate::jobs::Source::Owner,
                    ),
                    Some(_) => self.jobs.dismiss(&item.job),
                    None => return Ok(item.to_json()),
                };
                let refused = match carried {
                    Some(Ok(_)) => return Ok(item.to_json()),
                    Some(Err(refused)) => refused.0,
                    None => "That job is gone.".to_owned(),
                };
                item.answer = None;
                item.done = None;
                self.lock().inbox.insert(item.id.clone(), item.clone());
                self.save_item(&item);
                return Err(api(StatusCode::CONFLICT, refused));
            }
            // A `start` item: every answer settles it under the lock, so of two at the same
            // moment only the first counts. A second Start finds it done and starts nothing
            // (review on #552), and Later, Cancel or done never write back a copy read before a
            // Start that started its job (review on #1536). Only Start carries anything out.
            if item.kind == "start" {
                let mut item = item;
                match &given {
                    Given::Answer(a) if a == inbox::START => {
                        // An offer past its day closes instead, though nothing read the inbox
                        // since (`settle_stale_starts`): nothing starts.
                        if let Some(closed) = close_expired(&mut state.inbox, id, now) {
                            drop(state);
                            self.save_item(&closed);
                            return Err(api(StatusCode::CONFLICT, OFFER_EXPIRED));
                        }
                        if let Some(kept) = state.inbox.get_mut(id) {
                            kept.done = Some(now);
                        }
                        drop(state);
                        return self.start_item(item);
                    }
                    Given::Answer(a) if a == inbox::CANCEL_START => {
                        item.done = Some(now);
                        item.answer = Some(a.clone());
                    }
                    Given::Answer(_) => {
                        return Err(api(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            format!("The answer is {} or {}.", inbox::START, inbox::CANCEL_START),
                        ));
                    }
                    Given::Done => item.done = Some(now),
                    Given::Later(at) => item.later = Some(*at),
                }
                state.inbox.insert(item.id.clone(), item.clone());
                drop(state);
                self.save_item(&item);
                return Ok(item.to_json());
            }
            // A `limit` item (#871): Try again carries its job on now, Dismiss lets the job go. The
            // answer claims it under the lock, so a second tap carries nothing out.
            if item.kind == "limit" {
                let mut item = item;
                match &given {
                    Given::Later(at) => item.later = Some(*at),
                    Given::Done => item.done = Some(now),
                    Given::Answer(a) if a == crate::job::TRY_AGAIN || a == inbox::DISMISS => {
                        item.answer = Some(a.clone());
                        item.done = Some(now);
                    }
                    Given::Answer(_) => {
                        return Err(api(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "The answer is Try again or Dismiss.",
                        ));
                    }
                }
                state.inbox.insert(item.id.clone(), item.clone());
                drop(state);
                self.save_item(&item);
                let carried = match item.answer.as_deref() {
                    Some(crate::job::TRY_AGAIN) => self.jobs.message(
                        &item.job,
                        crate::brain::actions::TRY_AGAIN,
                        crate::jobs::Source::Owner,
                    ),
                    Some(_) => self.jobs.dismiss(&item.job),
                    None => return Ok(item.to_json()),
                };
                let refused = match carried {
                    Some(Ok(_)) => return Ok(item.to_json()),
                    Some(Err(refused)) => refused.0,
                    None => "That job is gone.".to_owned(),
                };
                item.answer = None;
                item.done = None;
                self.lock().inbox.insert(item.id.clone(), item.clone());
                self.save_item(&item);
                return Err(api(StatusCode::CONFLICT, refused));
            }
            // Oby's own question about a job event (the event orchestrator's escalation): the
            // owner's answer, or Done, settles it, and nothing else is carried out (#710). It isn't
            // another Obi's request, whose exchange it would look for.
            if item.obi.is_none() && item.id.starts_with("orchestrator-") {
                let mut item = item;
                match &given {
                    Given::Later(at) => item.later = Some(*at),
                    Given::Answer(answer) => {
                        item.answer = Some(answer.clone());
                        item.done = Some(now);
                    }
                    Given::Done => item.done = Some(now),
                }
                state.inbox.insert(item.id.clone(), item.clone());
                drop(state);
                self.save_item(&item);
                return Ok(item.to_json());
            }
            // A plug-in's event and the watcher's notice only say something: Dismiss settles them.
            if matches!(item.kind.as_str(), "pluginEvent" | "watch" | "listen") {
                drop(state);
                return match given {
                    Given::Answer(answer) => self.answer_plugin_event(item, Some(answer), None),
                    Given::Done => self.answer_plugin_event(item, None, None),
                    Given::Later(at) => self.answer_plugin_event(item, None, Some(at)),
                };
            }
            if let Given::Later(at) = &given {
                let mut item = item;
                item.later = Some(*at);
                state.inbox.insert(item.id.clone(), item.clone());
                drop(state);
                self.save_item(&item);
                return Ok(item.to_json());
            }
            let run_request = item
                .obi
                .as_ref()
                .is_some_and(|o| o["type"] == "run_request");
            let answer = match &given {
                Given::Answer(a) => {
                    if !run_request && !inbox::OBI_OPTIONS.contains(&a.as_str()) {
                        return Err(api(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            format!("The answer is one of: {}.", inbox::OBI_OPTIONS.join("; ")),
                        ));
                    }
                    Some(a.clone())
                }
                _ => None,
            };
            let cid = item
                .obi
                .as_ref()
                .and_then(|o| o["correlationId"].as_str())
                .unwrap_or("")
                .to_owned();
            let mut exchange = state
                .exchanges
                .get(&cid)
                .cloned()
                .filter(|e| e.state == "inbox")
                .ok_or_else(|| api(StatusCode::CONFLICT, "That request was settled already."))?;
            // After its expires_at nobody acts on a request (protocol.md section 3): it's settled
            // here, as the clock would at its next pass, and the answer is refused.
            if exchange.expires_at().is_some_and(|t| t <= now) {
                drop(state);
                self.expire();
                return Err(api(
                    StatusCode::CONFLICT,
                    "That request expired before the answer.",
                ));
            }
            // Only a review request is accepted here: a question is answered in words (above).
            if exchange.kind != "review_request" {
                return Err(api(
                    StatusCode::CONFLICT,
                    "That request isn't a review request.",
                ));
            }
            let chosen = answer.as_deref().unwrap_or(inbox::DECLINE);
            let accept = chosen == inbox::ACCEPT || chosen == inbox::ACCEPT_REMEMBER;
            let remember = chosen == inbox::ACCEPT_REMEMBER || chosen == inbox::DECLINE_REMEMBER;
            exchange.decided_by = Some("inbox".into());
            if accept {
                self.accept_locked(&state, &mut exchange, None);
            } else {
                exchange.state = "declined".into();
                let decline = self.reply_locked(
                    &state,
                    &exchange,
                    "decline",
                    json!({"reason": "owner_declined"}),
                );
                exchange.outbox.extend(decline);
            }
            exchange.updated = now;
            let mut decisions = None;
            if remember {
                let decision = Decision {
                    id: decisions::new_id(),
                    kind: exchange.kind.clone(),
                    from_owner: exchange.peer_owner.clone().unwrap_or_default(),
                    repo: exchange.request["body"]["repo"]
                        .as_str()
                        .unwrap_or("*")
                        .to_owned(),
                    decision: if accept { "accept" } else { "decline" }.into(),
                    created: now,
                    inbox_item: item.id.clone(),
                    uses: 0,
                    last_used: None,
                };
                state.decisions.retain(|d| {
                    !(d.kind == decision.kind
                        && d.from_owner == decision.from_owner
                        && d.repo == decision.repo)
                });
                state.decisions.push(decision);
                decisions = Some(state.decisions.clone());
            }
            let mut item = item;
            item.done = Some(now);
            item.answer = answer;
            state.inbox.insert(item.id.clone(), item.clone());
            state.exchanges.insert(cid, exchange.clone());
            (item, exchange, decisions)
        };
        if let Some(decisions) = decisions {
            self.save_decisions(&decisions);
        }
        self.save_item(&item);
        self.save_exchange(&exchange);
        self.outbox.notify_one();
        Ok(item.to_json())
    }
}

impl Link {
    /// `POST /v1/inbox/{id}/answer` on a question's item: `Decline` (or `{"done": true}`) and
    /// `Decline, and remember for this repo` decline it, the second keeping a decline for each
    /// repository it is about (`*` for one about none); any other words are the owner's answer,
    /// `by: owner`, with `choice` when they are exactly one of the question's choices. The words
    /// pass the publish gate first: a possible secret, or a scanner that can't run, is `422`,
    /// nothing is sent, and the item stays open.
    pub(super) async fn answer_question(
        &self,
        item: Item,
        words: Option<String>,
    ) -> Result<Value, ApiError> {
        let invalid = |m: String| api(StatusCode::UNPROCESSABLE_ENTITY, m);
        let cid = item
            .obi
            .as_ref()
            .and_then(|o| o["correlationId"].as_str())
            .unwrap_or("")
            .to_owned();
        let declining = words
            .as_deref()
            .is_none_or(|w| inbox::QUESTION_OPTIONS.contains(&w));
        let mut body = Value::Null;
        if let Some(text) = words.as_deref().filter(|_| !declining) {
            let asked = self
                .lock()
                .exchanges
                .get(&cid)
                .map(|e| e.request["body"].clone())
                .unwrap_or(Value::Null);
            body = json!({"by": "owner", "text": text});
            if let Some(choice) = asked["choices"]
                .as_array()
                .and_then(|choices| choices.iter().position(|c| c == text))
            {
                body["choice"] = json!(choice);
            }
            // Scanned first; the schema's own messages would quote the words.
            let scratch = self.config.home.join("tmp");
            let _ = std::fs::create_dir_all(&scratch);
            let verdict = scan::scan(&every_text(&body), &scratch).await;
            match verdict {
                scan::Verdict::Clean => {}
                scan::Verdict::Found(_) => {
                    return Err(invalid(
                        "The secret scan found a possible secret in the answer: nothing was sent."
                            .into(),
                    ));
                }
                scan::Verdict::CouldNotRun(why) => {
                    return Err(invalid(format!(
                        "The secret scan couldn't run ({why}): nothing was sent."
                    )));
                }
            }
            if !obi_protocol::schema::is_valid("actions/answer", &body) {
                return Err(invalid(
                    "An answer is one line or more of text with no control or bidirectional characters but tab and line breaks: nothing was sent."
                        .into(),
                ));
            }
        }
        let now = time::now();
        let (item, exchange, decisions) = {
            let mut state = self.lock();
            if state.inbox.get(&item.id).is_none_or(|i| i.done.is_some()) {
                return Err(api(StatusCode::CONFLICT, "That item is done already."));
            }
            let mut exchange = state
                .exchanges
                .get(&cid)
                .cloned()
                .filter(|e| e.state == "inbox")
                .ok_or_else(|| api(StatusCode::CONFLICT, "That question was settled already."))?;
            if exchange.expires_at().is_some_and(|t| t <= now) {
                drop(state);
                self.expire();
                return Err(api(
                    StatusCode::CONFLICT,
                    "That question expired before the answer.",
                ));
            }
            exchange.decided_by = Some("inbox".into());
            let mut decisions = None;
            if declining {
                exchange.state = "declined".into();
                let decline = self.reply_locked(
                    &state,
                    &exchange,
                    "decline",
                    json!({"reason": "owner_declined"}),
                );
                exchange.outbox.extend(decline);
                if words.as_deref() == Some(inbox::DECLINE_REMEMBER) {
                    let owner = exchange.peer_owner.clone().unwrap_or_default();
                    let mut repos: Vec<String> = item
                        .obi
                        .as_ref()
                        .and_then(|o| o["repos"].as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    if repos.is_empty() {
                        repos.push("*".into());
                    }
                    for repo in repos {
                        state.decisions.retain(|d| {
                            !(d.kind == "question"
                                && d.from_owner == owner
                                && d.repo.eq_ignore_ascii_case(&repo))
                        });
                        state.decisions.push(Decision {
                            id: decisions::new_id(),
                            kind: "question".into(),
                            from_owner: owner.clone(),
                            repo,
                            decision: "decline".into(),
                            created: now,
                            inbox_item: item.id.clone(),
                            uses: 0,
                            last_used: None,
                        });
                    }
                    decisions = Some(state.decisions.clone());
                }
            } else {
                let Some(answer) = self.reply_locked(&state, &exchange, "answer", body.clone())
                else {
                    return Err(api(
                        StatusCode::CONFLICT,
                        "obid has no key or membership to sign the answer with.",
                    ));
                };
                exchange.outbox.push(answer);
                exchange.result = Some(body);
                exchange.state = "done".into();
            }
            exchange.updated = now;
            let mut item = item;
            item.done = Some(now);
            item.answer = words;
            state.inbox.insert(item.id.clone(), item.clone());
            state.exchanges.insert(cid, exchange.clone());
            (item, exchange, decisions)
        };
        if let Some(decisions) = decisions {
            self.save_decisions(&decisions);
        }
        self.save_item(&item);
        self.save_exchange(&exchange);
        self.outbox.notify_one();
        Ok(item.to_json())
    }
}
