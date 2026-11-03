//! The room as the link follows and posts in it (docs/contracts/rooms.md): reading it in the follower's
//! loop, each post checked and its copy opened (crate::sphere::room); a queue of posts with its own
//! sender; a `need` answered by itself under the owner's sharing rules only; a job's `working_on` and
//! `done`; and what the API and the brain's tools ask of it.
//!
//! A post's words are data: logged never, a model's only in a quoted file. Nothing a post says starts,
//! changes or publishes anything.

use std::sync::{Arc, MutexGuard};
use std::time::Duration;

use axum::http::StatusCode;
use obi_protocol::client::ClientError;
use obi_protocol::time;
use serde_json::{Value, json};

use super::*;
use crate::job::Status;
use crate::sphere::room::{New, Post};

/// How many posts one read asks for.
const PAGE: u32 = 50;
/// How many of the newest posts the brain's read shows.
const BRAIN_READS: usize = 50;
/// How long a route waits for its post to reach the Sphere before it answers "not sent yet".
const SEND_NOW: Duration = Duration::from_secs(10);

/// What the brain asks to post (`oby_room_post`), and what its Confirm card keeps.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Asked {
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub about: Vec<Value>,
    #[serde(default)]
    pub reply_to: Option<String>,
}

impl Asked {
    /// Why it can't go, when it can't. The brain posts only what its tool is for: `need` and `found`.
    pub fn problem(&self) -> Option<String> {
        match (self.kind.as_str(), &self.reply_to) {
            ("need", None) | ("found", Some(_)) => {}
            ("need", Some(_)) => return Some("A need answers nothing: no replyTo.".into()),
            ("found", None) => return Some("A found names the need it answers: replyTo.".into()),
            _ => return Some("kind is need or found.".into()),
        }
        words_problem(&self.text, &self.about, self.reply_to.as_deref())
    }
}

/// Why words can't go in a post, when they can't.
fn words_problem(text: &str, about: &[Value], reply_to: Option<&str>) -> Option<String> {
    if answering::clean(text).is_empty() {
        return Some("A post has words.".into());
    }
    if text.chars().count() > room::MAX_TEXT {
        return Some(format!(
            "A post's words are at most {} characters.",
            room::MAX_TEXT
        ));
    }
    if let Some(reply_to) = reply_to
        && !obi_protocol::schema::is_valid("common#/$defs/uuid", &json!(reply_to))
    {
        return Some("replyTo is a post's id.".into());
    }
    room::about_problem(about).map(str::to_owned)
}

/// What a job's post says: its title, its repository and its pull request, nothing else.
fn job_words(job: &Job) -> (String, Value) {
    let pr = job
        .published
        .as_deref()
        .and_then(|url| crate::github::pull_request_number(url, &job.repo))
        .or(job.pr);
    let title = job.short_title();
    match pr {
        Some(pr) => (
            format!("{title} ({}#{pr})", job.repo),
            json!({"kind": "pr", "repo": job.repo, "pr": pr}),
        ),
        None => (
            format!("{title} ({})", job.repo),
            json!({"kind": "repo", "repo": job.repo}),
        ),
    }
}

/// Who the room's words reach by the directory: the Obi and its seal key, each active member with one.
fn seal_members(state: &State) -> Vec<(String, [u8; 32])> {
    let mut members: Vec<(String, [u8; 32])> = state
        .directory
        .members
        .iter()
        .filter(|(_, m)| m.active)
        .filter_map(|(obi, m)| Some((obi.clone(), m.seal?)))
        .collect();
    members.sort();
    members
}

impl Link {
    pub(super) fn room_lock(&self) -> MutexGuard<'_, Room> {
        self.room.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The other active members' owners: never this Obi's own, whose other Obis are theirs too.
    fn other_owners(&self, state: &State) -> Vec<String> {
        let me = state.membership.as_ref().map(|m| m.obi_id.as_str());
        let own = self.owner();
        let mut owners: Vec<String> = state
            .directory
            .members
            .iter()
            .filter(|(obi, m)| m.active && Some(obi.as_str()) != me && m.owner != own)
            .map(|(_, m)| m.owner.clone())
            .collect();
        owners.sort();
        owners.dedup();
        owners
    }

    /// The directory as it is now, or, with the Sphere away, as it was last read: who a post is
    /// sealed for, and who a rule has to cover, is who's a member at sending.
    async fn directory_now(&self) -> Result<(), ClientError> {
        match self.read_directory().await {
            Err(e) if self.lock().directory_read == 0 => Err(e),
            _ => Ok(()),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Following

    /// One look at the room, in the follower's loop: every post above the cursor, read in order. A
    /// Sphere with no room answers `404`, and is asked again every 10 minutes. Whatever goes wrong here
    /// is the room's alone: deliveries go on, and `lastError` doesn't hear of it.
    pub(super) async fn follow_room(self: &Arc<Self>) {
        let now = time::now();
        {
            let room = self.room_lock();
            if room.available == Some(false) && now - room.asked < FEATURES_EVERY {
                return;
            }
        }
        loop {
            let after = self.room_lock().after;
            let page = match self.token().await {
                Ok(token) => match self.client() {
                    Ok(client) => client.room_posts(&token, after, PAGE).await,
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };
            let page = match page {
                Ok(page) => page,
                Err(ClientError::Refused { status: 404, .. }) => {
                    let mut room = self.room_lock();
                    room.available = Some(false);
                    room.asked = now;
                    return;
                }
                Err(e) => {
                    self.refused_token(&e);
                    tracing::debug!("sphere: the room wasn't read: {e}");
                    return;
                }
            };
            self.room_lock().available = Some(true);
            if page.is_empty() {
                return;
            }
            let full = page.len() == PAGE as usize;
            let mut done = true;
            for (seq, at, post) in page {
                if !self.read_room_post(seq, &at, &post).await {
                    done = false;
                    break;
                }
                let mut room = self.room_lock();
                room.after = room.after.max(seq);
            }
            self.room_lock().save(&self.config.home);
            if !full || !done {
                return;
            }
        }
    }

    /// One post the Sphere served: kept when it passes and has a copy for this Obi, dropped and logged
    /// when it doesn't pass, skipped when it has none. `false` when the directory that judges it
    /// couldn't be read: it's read again later, the cursor still before it.
    async fn read_room_post(self: &Arc<Self>, seq: u64, at: &str, post: &Value) -> bool {
        let text = |key: &str| post[key].as_str().unwrap_or("");
        let (id, from, key_id) = (text("id"), text("from"), text("key_id"));
        // Already here. Its own, read back before its answer came: that's its answer. A retry the
        // Sphere took as new past its 5,000: nothing new.
        let kept = self
            .room_lock()
            .posts
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.state == room::WAITING && p.wire.as_ref() == Some(post));
        if let Some(waiting) = kept {
            if waiting {
                self.room_post_sent(id, seq, at);
            }
            return true;
        }
        // A member who joined or rotated since the directory was read.
        let known = {
            let state = self.lock();
            state.directory.members.contains_key(from) && state.directory.keys.contains_key(key_id)
        };
        if !known && let Err(e) = self.read_directory().await {
            self.refused_token(&e);
            return false;
        }
        let read = {
            let state = self.lock();
            let current = |from: &str, id: &str| {
                let key = state.directory.keys.get(id)?;
                (key.obi == from && key.valid_until_seq.is_none()).then(|| key.public_key.clone())
            };
            let (me, sphere) = state
                .membership
                .as_ref()
                .map(|m| (m.obi_id.clone(), m.sphere.clone()))
                .unwrap_or_default();
            let owner = state
                .directory
                .members
                .get(from)
                .filter(|m| m.active)
                .map(|m| m.owner.clone());
            if let Some(why) = room::problem(post, &sphere, time::now(), current) {
                Err(why)
            } else if let Some(owner) = owner {
                room::open(post, &me, &own_seal_keys(&state))
                    .map(|opened| opened.map(|words| (words, owner, me)))
            } else {
                Err("its sender isn't an active member in the directory")
            }
        };
        let ((words, about), owner, me) = match read {
            Ok(Some(read)) => read,
            // A member who joined later, or with no seal key then: nothing of it, and no error.
            Ok(None) => return true,
            Err(why) => {
                // Why names checks: the seq, never an id or a word the post chose.
                tracing::warn!("sphere: dropped the room's post at {seq}: {why}");
                return true;
            }
        };
        let kept = Post {
            id: id.to_owned(),
            from: from.to_owned(),
            owner,
            kind: text("kind").to_owned(),
            by: text("by").to_owned(),
            created_at: text("created_at").to_owned(),
            reply_to: post["reply_to"].as_str().map(str::to_owned),
            text: words,
            about,
            seq: Some(seq),
            recorded_at: Some(at.to_owned()),
            state: room::SENT.into(),
            error: None,
            job: None,
            wire: None,
            self_answered: None,
        };
        self.room_lock().posts.push(kept.clone());
        self.events.send("roomPost", kept.to_json());
        if kept.kind == "need" && kept.from != me {
            let link = self.clone();
            tokio::spawn(async move { link.answer_need(kept).await });
        }
        true
    }

    // -----------------------------------------------------------------------------------------
    // Answering by itself

    /// The rule a `found` to `owner`'s `need` about `repos` is made under: their job-status rule,
    /// while one holds for every other member too, since a post is sealed for them all.
    fn need_rule(&self, state: &State, owner: &str, repos: &[String]) -> Option<Rule> {
        let others = self.other_owners(state);
        (others.iter().any(|o| o == owner) && state.sharing.job_status_for_all(&others, repos))
            .then(|| state.sharing.job_status_rule(owner, repos).cloned())
            .flatten()
    }

    /// A `need` from another member: one `found`, as a question is answered (crate::sphere::answering:
    /// a model run on the owner's account, the words in a quoted file, the same self-answer limits),
    /// from what a sharing rule allows. Under no rule, or past the limit, or when the model can't,
    /// Oby stays silent: nobody is asked and nothing notifies.
    async fn answer_need(self: Arc<Self>, need: Post) {
        let now = time::now();
        // An old one read late (a Mac asleep) is nobody's question any more.
        let fresh = time::parse(&need.created_at).is_some_and(|t| t > now - REPLY_LIFETIME);
        if !fresh {
            return;
        }
        let (repos, referenced) = self.question_repos(&json!({"about": need.about}));
        let rule = {
            let mut state = self.lock();
            let declined = decisions::covering_question(&state.decisions, &need.owner, &repos);
            if declined.is_some() {
                return;
            }
            let Some(rule) = self.need_rule(&state, &need.owner, &repos) else {
                return;
            };
            if !Link::take_self_answer(&mut state, &need.owner, now) {
                return;
            }
            rule
        };
        {
            let mut room = self.room_lock();
            if let Some(kept) = room.get_mut(&need.id) {
                kept.self_answered = Some(now);
            }
            room.save(&self.config.home);
        }
        let Ok(_run) = self.answering.acquire().await else {
            return;
        };
        let (rule, declined) = {
            let state = self.lock();
            // The owner may have changed it while it waited.
            let Some(rule) = self
                .need_rule(&state, &need.owner, &repos)
                .filter(|r| r.id == rule.id)
            else {
                return;
            };
            let declined: Vec<String> = self
                .other_owners(&state)
                .iter()
                .flat_map(|o| decisions::declined_repos(&state.decisions, o))
                .collect();
            (rule, declined)
        };
        let jobs = self.jobs.list(usize::MAX);
        // No results of reviews: they were sent to one owner, and a post goes to everyone.
        let material = answering::material(
            &rule,
            &need.owner,
            &repos,
            &declined,
            &referenced,
            &jobs,
            &[],
        );
        if material.is_empty() {
            return;
        }
        let dir = self.config.home.join("answers").join(&need.id);
        let question = json!({"text": need.text, "about": need.about});
        let written = answering::write(
            &self.config,
            &self.sandbox,
            &dir,
            &need.owner,
            &question,
            &material,
        )
        .await;
        let written = match written {
            Ok(Some(written)) => written,
            Ok(None) => return,
            Err(why) => {
                // Why names checks, never the need's or the model's words.
                tracing::warn!("sphere: the room's need wasn't answered: {why}");
                return;
            }
        };
        // The rule may have gone while the model ran, or a member come whom it doesn't cover: then
        // nothing it wrote goes.
        let _ = self.directory_now().await;
        let still = {
            let state = self.lock();
            self.need_rule(&state, &need.owner, &repos)
                .is_some_and(|r| r.id == rule.id)
        };
        if !still {
            return;
        }
        let new = New {
            kind: "found",
            by: "obi",
            text: &written.text,
            about: &written.refs,
            reply_to: Some(&need.id),
        };
        if let Err(why) = self.post_in_room(&new, None).await {
            tracing::warn!("sphere: the room's need wasn't answered: {why}");
            return;
        }
        let mut state = self.lock();
        if let Some(r) = state.sharing.rules.iter_mut().find(|r| r.id == rule.id) {
            r.uses += 1;
            r.last_used = Some(time::now());
        }
        self.save_sharing(&state.sharing);
    }

    // -----------------------------------------------------------------------------------------
    // Posting

    /// A new post, sealed for each active member with a seal key and signed, in the queue: every
    /// outgoing word through the secret scan first, so a finding, or a scan that can't run, sends
    /// nothing. `job` is the job it's about, once for each kind. `Err` says why, never quoting words.
    pub(crate) async fn post_in_room(
        &self,
        new: &New<'_>,
        job: Option<&str>,
    ) -> Result<Post, String> {
        if let Some(why) = words_problem(new.text, new.about, new.reply_to) {
            return Err(why);
        }
        let text = answering::clean(new.text);
        let scratch = self.config.home.join("tmp");
        let _ = std::fs::create_dir_all(&scratch);
        let outgoing = json!({"text": text, "about": new.about});
        match scan::scan(&every_text(&outgoing), &scratch).await {
            scan::Verdict::Clean => {}
            scan::Verdict::Found(found) => {
                let places: Vec<String> = found.iter().take(3).map(ToString::to_string).collect();
                return Err(format!(
                    "The secret scan found a possible secret in {}: nothing was posted.",
                    places.join(", ")
                ));
            }
            scan::Verdict::CouldNotRun(why) => {
                return Err(format!(
                    "The secret scan couldn't run ({why}): nothing was posted."
                ));
            }
        }
        self.directory_now().await.map_err(|e| e.to_string())?;
        let new = New {
            text: &text,
            ..new.clone()
        };
        let id = new_uuid();
        let (wire, from, owner) = {
            let state = self.lock();
            let membership = state
                .membership
                .as_ref()
                .filter(|m| m.state == "joined")
                .ok_or("This Oby isn't in an Obysphere.")?;
            let key = state.key.as_ref().ok_or("This Oby has no key.")?;
            let members = seal_members(&state);
            if members.is_empty() {
                return Err(
                    "No member of this Obysphere can open a post yet: none has a seal key.".into(),
                );
            }
            let wire = room::sealed(
                &new,
                &id,
                &membership.sphere,
                &membership.obi_id,
                &members,
                key,
                time::now(),
            )?;
            (wire, membership.obi_id.clone(), self.owner())
        };
        let size = obi_protocol::jcs::canonical_bytes(&wire)
            .map_err(|e| e.to_string())?
            .len();
        if size > room::MAX_BYTES {
            // ponytail: refused, so the words stay whole; shorten them by halves if a room gets here.
            return Err("Sealed for every member, that's too long to post: shorten it.".into());
        }
        let post = Post {
            id,
            from,
            owner,
            kind: new.kind.to_owned(),
            by: new.by.to_owned(),
            created_at: wire["created_at"].as_str().unwrap_or("").to_owned(),
            reply_to: new.reply_to.map(str::to_owned),
            text: new.text.to_owned(),
            about: new.about.to_vec(),
            seq: None,
            recorded_at: None,
            state: room::WAITING.into(),
            error: None,
            job: job.map(str::to_owned),
            wire: Some(wire),
            self_answered: None,
        };
        {
            let mut room = self.room_lock();
            if let Some(job) = job
                && room.job_posted(job, new.kind)
            {
                return Err("It's in the room already.".into());
            }
            room.posts.push(post.clone());
            room.save(&self.config.home);
        }
        self.events.send("roomPost", post.to_json());
        self.room_outbox.notify_one();
        Ok(post)
    }

    /// The room's queue, for as long as obid has a membership: posts go as soon as they wait, and
    /// after a failure again, with the same bytes.
    pub(super) async fn run_room_sender(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(1);
        loop {
            let wait = if !self.is_joined() {
                Duration::from_secs(60)
            } else if self.send_room().await {
                backoff = Duration::from_secs(1);
                Duration::from_secs(60)
            } else {
                let wait = backoff;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                wait
            };
            tokio::select! {
                () = self.room_outbox.notified() => {}
                () = tokio::time::sleep(wait) => {}
            }
        }
    }

    /// Sends what waits, oldest first: `true` once nothing waits to be tried again. A `429` waited
    /// as `Retry-After` says (the client's), a Sphere that doesn't answer or answers 5xx: it stays
    /// queued. A key that rotated while it waited: signed again, once. A Sphere that won't take it,
    /// now or later: refused, and said on the post.
    pub(super) async fn send_room(&self) -> bool {
        let _one = self.room_sending.lock().await;
        let mut resigned: Option<String> = None;
        loop {
            if !self.is_joined() {
                return false;
            }
            let next = self
                .room_lock()
                .posts
                .iter()
                .filter(|p| p.state == room::WAITING)
                .find_map(|p| Some((p.id.clone(), p.wire.clone()?)));
            let Some((id, wire)) = next else {
                return true;
            };
            let sent = match self.token().await {
                Ok(token) => match self.client() {
                    Ok(client) => client.room_post(&token, &wire).await,
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };
            match sent {
                Ok((seq, at)) => self.room_post_sent(&id, seq, &at),
                Err(ClientError::Refused { code, .. })
                    if code == "unknown_key" && resigned.as_deref() != Some(&id) =>
                {
                    let key = self.lock().key.clone();
                    let again = key.and_then(|key| room::signed_again(&wire, &key).ok());
                    let Some(again) = again else {
                        return false;
                    };
                    let mut room = self.room_lock();
                    if let Some(post) = room.get_mut(&id) {
                        post.wire = Some(again);
                    }
                    room.save(&self.config.home);
                    resigned = Some(id);
                }
                Err(ClientError::Refused { status: 404, .. }) => {
                    self.room_lock().available = Some(false);
                    self.room_post_refused(&id, "This Obysphere has no room.".into());
                }
                Err(ClientError::Refused {
                    status,
                    code,
                    message,
                }) if status < 500 && !matches!(status, 401 | 408 | 429) => {
                    self.room_post_refused(
                        &id,
                        format!("The Sphere refused it: {code}: {message}"),
                    );
                }
                Err(e) => {
                    self.refused_token(&e);
                    tracing::warn!("sphere: a room post waits: {e}");
                    return false;
                }
            }
        }
    }

    /// The Sphere has it: its place kept, the job's `outgoing` told.
    fn room_post_sent(&self, id: &str, seq: u64, at: &str) {
        let post = {
            let mut room = self.room_lock();
            let Some(post) = room.get_mut(id) else {
                return;
            };
            post.seq = Some(seq);
            post.recorded_at = Some(at.to_owned());
            post.state = room::SENT.into();
            post.error = None;
            post.wire = None;
            let post = post.clone();
            room.save(&self.config.home);
            post
        };
        self.events.send("roomPost", post.to_json());
        if let Some(job) = &post.job {
            self.jobs.room_posted(job, id);
        }
    }

    fn room_post_refused(&self, id: &str, why: String) {
        tracing::warn!("sphere: a room post went nowhere: {why}");
        let post = {
            let mut room = self.room_lock();
            let Some(post) = room.get_mut(id) else {
                return;
            };
            post.state = room::REFUSED.into();
            post.error = Some(why);
            post.wire = None;
            let post = post.clone();
            room.save(&self.config.home);
            post
        };
        self.events.send("roomPost", post.to_json());
    }

    // -----------------------------------------------------------------------------------------
    // A job's posts

    /// A job's status changed (crate::jobs, with none of its locks held, and maybe under this Link's:
    /// `start_obi_review`, so this locks nothing): it started or ended, and the room may hear.
    pub(crate) fn job_moved(self: &Arc<Self>, job: &Job) {
        let kind = match job.status {
            Status::Running => "working_on",
            Status::Done | Status::Failed | Status::Stopped => "done",
            Status::Queued | Status::Interrupted => return,
        };
        // Never a review another Obi asked for: only the owner's own work.
        if job.obi.is_some() || job.repo.is_empty() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (link, id) = (self.clone(), job.id.clone());
        runtime.spawn(async move { link.post_job(&id, kind).await });
    }

    /// Job `id`'s `working_on` or `done`, once each: its title, repository and pull request only, and
    /// only while a sharing rule for every member allows job status for that repository.
    async fn post_job(&self, id: &str, kind: &str) {
        if !self.is_joined() || self.room_lock().available == Some(false) {
            return;
        }
        if self.room_lock().job_posted(id, kind) {
            return;
        }
        let Some(job) = self.jobs.get(id) else {
            return;
        };
        // It ended before this ran: its `done` says all there is.
        if kind == "working_on" && !job.status.is_active() {
            return;
        }
        if let Err(e) = self.directory_now().await {
            tracing::warn!("sphere: job {id} wasn't posted in the room: {e}");
            return;
        }
        let allowed = {
            let state = self.lock();
            state
                .sharing
                .job_status_for_all(&self.other_owners(&state), std::slice::from_ref(&job.repo))
        };
        if !allowed {
            return;
        }
        let (text, about) = job_words(&job);
        let about = [about];
        let new = New {
            kind,
            by: "obi",
            text: &text,
            about: &about,
            reply_to: None,
        };
        if let Err(why) = self.post_in_room(&new, Some(id)).await {
            tracing::warn!("sphere: job {id} wasn't posted in the room: {why}");
        }
    }

    // -----------------------------------------------------------------------------------------
    // The API's and the brain's

    /// Whether the Sphere has a room, asked at once if obid hasn't yet.
    async fn room_known(self: &Arc<Self>) -> bool {
        if self.room_lock().available.is_none() {
            Box::pin(self.follow_room()).await;
        }
        self.room_lock().available != Some(false)
    }

    fn sphere_is(&self, sphere: &str) -> Result<(), ApiError> {
        let ours = self
            .lock()
            .membership
            .as_ref()
            .is_some_and(|m| m.sphere == sphere);
        if ours {
            Ok(())
        } else {
            Err(api(
                StatusCode::NOT_FOUND,
                "This Oby isn't in that Obysphere.",
            ))
        }
    }

    /// `GET /v1/spheres/{sphere}/room?after=`: the room's posts above `after`, and those not sent yet.
    pub async fn room_json(self: &Arc<Self>, sphere: &str, after: u64) -> Result<Value, ApiError> {
        self.sphere_is(sphere)?;
        if !self.room_known().await {
            return Ok(json!({"room": "unavailable", "posts": []}));
        }
        let room = self.room_lock();
        let posts: Vec<Value> = room.since(after).iter().map(|p| p.to_json()).collect();
        Ok(json!({"room": "available", "posts": posts}))
    }

    /// `POST /v1/spheres/{sphere}/room`: the owner's own `say`, which their app signed (the route
    /// checks). The post, sent if the Sphere took it in a few seconds, else `waiting`.
    pub async fn room_say(self: &Arc<Self>, sphere: &str, body: &[u8]) -> Result<Value, ApiError> {
        self.sphere_is(sphere)?;
        let map = object(body, &["text", "about", "replyTo"])?;
        let text = text_field(&map, "text")?;
        let about: Vec<Value> = match map.get("about") {
            None => Vec::new(),
            Some(Value::Array(about)) => about.clone(),
            Some(_) => {
                return Err(api(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "about is a list of references.",
                ));
            }
        };
        let reply_to = match map.get("replyTo") {
            None => None,
            Some(Value::String(id)) => Some(id.as_str()),
            Some(_) => {
                return Err(api(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "replyTo is a post's id.",
                ));
            }
        };
        if !self.room_known().await {
            return Err(api(StatusCode::CONFLICT, "This Obysphere has no room."));
        }
        let new = New {
            kind: "say",
            by: "owner",
            text,
            about: &about,
            reply_to,
        };
        let post = self
            .post_in_room(&new, None)
            .await
            .map_err(|why| api(StatusCode::UNPROCESSABLE_ENTITY, why))?;
        Ok(self.sent_soon(post).await.to_json())
    }

    /// What the brain asked to post (`oby_room_post`, or its Confirm): `need` or `found`, by Oby.
    pub async fn post_asked(self: &Arc<Self>, asked: &Asked) -> Result<Post, String> {
        if let Some(why) = asked.problem() {
            return Err(why);
        }
        if !self.room_known().await {
            return Err("This Obysphere has no room.".into());
        }
        let new = New {
            kind: &asked.kind,
            by: "obi",
            text: &asked.text,
            about: &asked.about,
            reply_to: asked.reply_to.as_deref(),
        };
        let post = self.post_in_room(&new, None).await?;
        Ok(self.sent_soon(post).await)
    }

    /// `post` as it is after a few seconds' try at sending it; the sender goes on either way.
    async fn sent_soon(&self, post: Post) -> Post {
        let _ = tokio::time::timeout(SEND_NOW, self.send_room()).await;
        self.room_lock()
            .posts
            .iter()
            .find(|p| p.id == post.id)
            .cloned()
            .unwrap_or(post)
    }

    /// The newest posts for a model's quoted file, oldest first (`oby_room_read`).
    pub async fn room_for_model(self: &Arc<Self>) -> Result<Vec<Post>, String> {
        if !self.is_joined() {
            return Err("This Oby isn't in an Obysphere.".into());
        }
        if !self.room_known().await {
            return Err("This Obysphere has no room: ask one member at a time instead.".into());
        }
        let room = self.room_lock();
        let posts = room.since(0);
        let skip = posts.len().saturating_sub(BRAIN_READS);
        Ok(posts.into_iter().skip(skip).cloned().collect())
    }
}
