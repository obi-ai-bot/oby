//! Following incoming Sphere deliveries: long-polling, envelope verification & unsealing,
//! reply matching, cancel/follow-up/introduction handling, and request triage.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use obi_protocol::client::{ClientError, Delivery};
use obi_protocol::time;
use serde_json::{Value, json};

use super::*;

impl Link {
    // -----------------------------------------------------------------------------------------
    // Following: deliveries

    /// Starts the follower, once.
    pub(super) fn follow(self: &Arc<Self>) {
        if self.following.swap(true, Ordering::SeqCst) {
            return;
        }
        let link = self.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = link.clone().run_follower() => {}
                () = stopped(link.stopping.clone()) => return,
            }
            // Left or revoked: a later join follows again, also one that came while this ended.
            link.following.store(false, Ordering::SeqCst);
            if link.is_joined() {
                link.follow();
            }
        });
    }

    pub(super) async fn run_follower(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(1);
        loop {
            let revoked = self
                .lock()
                .membership
                .as_ref()
                .is_none_or(|m| m.state != "joined");
            if revoked {
                return;
            }
            match self.poll().await {
                Ok(()) => backoff = Duration::from_secs(1),
                Err(e) => {
                    self.note_error(&e);
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                }
            }
        }
    }

    /// One deliveries call: each delivery handled, then acknowledged once what it decided is kept.
    pub(super) async fn poll(self: &Arc<Self>) -> Result<(), ClientError> {
        let stale = time::now() - self.lock().directory_read >= DIRECTORY_EVERY;
        if stale {
            self.read_directory().await?;
        }
        let token = self.token().await?;
        let client = self.client()?;
        let deliveries = match client.deliveries(&token, 50, POLL_WAIT).await {
            Ok(list) => list,
            Err(e) => {
                self.refused_token(&e);
                return Err(e);
            }
        };
        {
            let mut state = self.lock();
            state.last_poll = Some(time::now());
            state.last_error = None;
        }
        if deliveries.is_empty() {
            return Ok(());
        }
        let mut handled = Vec::new();
        for delivery in deliveries {
            let id = delivery.delivery_id.clone();
            self.handle(delivery).await;
            handled.push(id);
        }
        if let Err(e) = client.ack(&token, &handled).await {
            // They come again; the ids already handled make the second time harmless.
            self.refused_token(&e);
            return Err(e);
        }
        Ok(())
    }

    /// The checks of protocol.md section 10, and a sealed message's words opened: `None` for one in
    /// the clear.
    pub(super) fn verify(
        &self,
        delivery: &Delivery,
    ) -> Result<Option<Value>, obi_protocol::Rejection> {
        let state = self.lock();
        let from = delivery.envelope["from"].as_str().unwrap_or("");
        let context = obi_protocol::envelope::Context {
            now: Some(time::now()),
            sphere: state.membership.as_ref().map(|m| m.sphere.clone()),
            receiver: state.membership.as_ref().map(|m| m.obi_id.clone()),
            // A live message is signed with its sender's current key, the one its card signs;
            // older keys only check old chain entries.
            keys: state
                .directory
                .keys
                .iter()
                .filter(|(_, k)| k.valid_until_seq.is_none())
                .map(|(id, k)| (id.clone(), k.clone()))
                .collect(),
            seq: Some(delivery.seq),
            seen_ids: Vec::new(),
            seal_keys: Some(own_seal_keys(&state)),
            // Words in the clear count against a sender that seals only once both cards had a seal
            // key when it wrote them: a message sent before either card gained one, or within the
            // clock skew after (a card is issued a little before the Sphere takes it), is taken.
            sender_seals: state.directory.members.get(from).is_some_and(|m| {
                let created = delivery.envelope["created_at"]
                    .as_str()
                    .and_then(time::parse)
                    .unwrap_or(i64::MAX);
                let since = m.sealed_since.zip(own_sealed_since(&state));
                since.is_some_and(|(theirs, mine)| {
                    created >= theirs.max(mine) + obi_protocol::CLOCK_SKEW_SECONDS
                })
            }),
        };
        obi_protocol::envelope::verify_and_open(&delivery.envelope, &context)
    }

    pub(super) fn drop_message(&self, id: &str, why: &str) {
        // The reason names checks and codes, never the message's text.
        tracing::warn!("sphere: dropped message {id}: {why}");
        let mut state = self.lock();
        state.seen.insert(id.to_owned());
        state.dropped.push(id.to_owned());
        if state.dropped.len() > DROPPED_MAX {
            let old = state.dropped.remove(0);
            state.seen.remove(&old);
        }
    }

    /// One delivery (protocol.md section 11): verified, deduplicated, then a reply matched to its
    /// exchange or a request decided.
    pub(super) async fn handle(self: &Arc<Self>, mut delivery: Delivery) {
        let id = delivery.envelope["id"].as_str().unwrap_or("").to_owned();
        if !id.is_empty() && self.lock().seen.contains(&id) {
            return;
        }
        let mut verdict = self.verify(&delivery);
        if matches!(&verdict, Err(r) if r.code == "unknown_key") {
            // A member who joined or rotated since the directory was read.
            if let Err(e) = self.read_directory().await {
                self.note_error(&e);
            }
            verdict = self.verify(&delivery);
        }
        let opened = match verdict {
            Ok(opened) => opened,
            Err(rejection) => {
                // Its code only: a schema's message may quote the message's text.
                self.drop_message(&id, rejection.code);
                return;
            }
        };
        // Its words opened, as everything that reads them reads them (protocol.md section 9.1).
        if let Some(body) = opened {
            delivery.envelope = kept(delivery.envelope, body);
        }
        let kind = delivery.envelope["type"].as_str().unwrap_or("");
        if obi_protocol::REPLY_TYPES.contains(&kind) {
            self.handle_reply(delivery);
        } else {
            self.handle_request(delivery);
        }
    }

    pub(super) fn received(delivery: &Delivery) -> Message {
        Message {
            direction: "received".into(),
            seq: Some(delivery.seq),
            at: delivery.relayed_at.clone(),
            envelope: delivery.envelope.clone(),
        }
    }

    /// A reply: matched to the outgoing request it names, sent to the reply's sender, keeping the
    /// exchange rules; else dropped.
    pub(super) fn handle_reply(self: &Arc<Self>, delivery: Delivery) {
        let envelope = &delivery.envelope;
        let id = envelope["id"].as_str().unwrap_or("").to_owned();
        let cid = envelope["in_reply_to"].as_str().unwrap_or("");
        let from = envelope["from"].as_str().unwrap_or("");
        let kind = envelope["type"].as_str().unwrap_or("");
        let mut state = self.lock();
        let Some(exchange) = state.exchanges.get_mut(cid).filter(|e| {
            e.peer == from
                && (e.direction == "outgoing"
                    || (obi_protocol::SENDER_REPLY_TYPES.contains(&kind)
                        && e.direction == "incoming"))
        }) else {
            drop(state);
            self.drop_message(&id, "a reply to no request of this Obi's");
            return;
        };
        let earlier: Vec<&Value> = exchange
            .messages
            .iter()
            .skip(1)
            .map(|m| &m.envelope)
            .collect();
        if let Some(problem) =
            obi_protocol::chain::exchange_problem(envelope, Some(&exchange.request), &earlier)
        {
            drop(state);
            self.drop_message(&id, &format!("breaks the exchange rules: {problem}"));
            return;
        }
        // An answer picking a choice its question doesn't have (protocol.md section 11, step 3).
        let choices = exchange.request["body"]["choices"]
            .as_array()
            .map_or(0, Vec::len) as u64;
        if envelope["type"] == "answer"
            && envelope["body"]
                .get("choice")
                .is_some_and(|c| c.as_u64().is_none_or(|c| c >= choices))
        {
            drop(state);
            self.drop_message(&id, "an answer with a choice its question doesn't have");
            return;
        }
        exchange.messages.push(Link::received(&delivery));
        match envelope["type"].as_str().unwrap_or("") {
            "accept" if exchange.state == "sent" => exchange.state = "accepted".into(),
            "decline" => exchange.state = "declined".into(),
            // A result finishes a review or a handoff, an answer a question: either ends the
            // exchange, and is its result.
            "result" | "answer" => {
                exchange.state = "done".into();
                exchange.result = Some(envelope["body"].clone());
            }
            // Its sender called a request obid received off (protocol.md section 5.17): nothing
            // more goes out in that exchange.
            "cancel" => {
                exchange.state = "cancelled".into();
                exchange.outbox.clear();
            }
            _ => {}
        }
        exchange.updated = time::now();
        let exchange = exchange.clone();
        state.seen.insert(id);
        drop(state);
        self.save_exchange(&exchange);
        if kind == "cancel" {
            self.cancelled(&exchange);
        }
    }

    /// A request obid received was called off by its sender (protocol.md section 11, "Calling a
    /// request off"): its work stops, its inbox item is settled, and nothing is approved for it.
    pub(super) fn cancelled(self: &Arc<Self>, exchange: &Exchange) {
        let now = time::now();
        let cid = exchange.correlation_id.clone();
        let (item, posted) = {
            let mut state = self.lock();
            let item = exchange
                .inbox_item
                .as_ref()
                .and_then(|id| state.inbox.get_mut(id))
                .filter(|i| i.done.is_none())
                .map(|i| {
                    i.done = Some(now);
                    i.text = format!("{} Its sender called it off.", i.text);
                    i.clone()
                });
            // An approval obid already posted for it: taken here, so only one of this and
            // [`Link::post_approval`] dismisses it.
            let posted = state
                .exchanges
                .get_mut(&cid)
                .and_then(|e| e.host_approval.take());
            (item, posted)
        };
        if let Some(item) = item {
            self.save_item(&item);
        }
        if let Some(job) = exchange.job.as_deref() {
            self.jobs.stop(job);
        }
        self.settle_host_approvals(
            |i| {
                i.confirm
                    .as_ref()
                    .is_some_and(|c| c.payload["correlationId"] == cid.as_str())
                    || i.obi
                        .as_ref()
                        .is_some_and(|o| o["correlationId"] == cid.as_str())
            },
            "Its sender called the review off: nothing was approved.",
        );
        // The receiver dismisses an approval it posted before the cancel came (section 11).
        if let Some(posted) = posted {
            let link = Arc::clone(self);
            tokio::spawn(async move {
                link.dismiss_approval(&cid, &posted, "its sender called the review off")
                    .await;
            });
        }
    }

    /// Dismisses the approval obid posted on GitHub for exchange `cid` (`posted`, as
    /// [`Exchange::host_approval`] keeps it), saying `why`; one GitHub doesn't let it dismiss is
    /// the owner's to remove, as an inbox item says ([`approval::dismissal_item`]).
    pub(super) async fn dismiss_approval(&self, cid: &str, posted: &Value, why: &str) {
        let text = |key: &str| posted[key].as_str().unwrap_or("").to_owned();
        let (job, repo, url) = (text("job"), text("repo"), text("url"));
        let pr = posted["pr"].as_u64().unwrap_or(0);
        let dismissed = match (crate::github::gh(), posted["id"].as_u64()) {
            (Some(gh), Some(id)) => approval::dismiss(&gh, &repo, pr, id).await,
            (None, _) => Err("gh isn't installed".into()),
            (_, None) => Err("GitHub didn't say which review it was".into()),
        };
        match dismissed {
            Ok(()) => self.jobs.dismissed_on_github(&job, &repo, &url),
            Err(failed) => {
                tracing::error!(
                    "sphere: exchange {cid}: obid approved {repo}#{pr} and couldn't dismiss it ({failed}) though {why}: the owner removes {url} on GitHub"
                );
                let item = approval::dismissal_item(cid, &job, &repo, pr, &url, time::now());
                self.lock().inbox.insert(item.id.clone(), item.clone());
                self.save_item(&item);
            }
        }
    }

    /// Whether incoming exchange `cid` is there and its sender hasn't called it off. Its expiry and
    /// obid's membership are checked apart.
    pub(super) fn not_called_off(&self, cid: &str) -> bool {
        self.lock()
            .exchanges
            .get(cid)
            .is_some_and(|e| e.state != "cancelled")
    }

    /// A request (protocol.md section 11, steps 4 to 6): declined when the card doesn't accept its
    /// type, decided by a remembered decision when one covers it, else put in the owner's inbox
    /// (declined `busy` when the sender's owner has as many waiting there as the limit allows). A
    /// question goes on to [`Link::decide_question`].
    pub(super) fn handle_request(self: &Arc<Self>, delivery: Delivery) {
        let envelope = delivery.envelope.clone();
        let id = envelope["id"].as_str().unwrap_or("").to_owned();
        let from = envelope["from"].as_str().unwrap_or("").to_owned();
        let kind = envelope["type"].as_str().unwrap_or("").to_owned();
        let body = &envelope["body"];
        // A question's repositories, its job references checked against obid's own jobs, before
        // the Link's state is locked.
        let (repos, referenced) = if kind == "question" {
            self.question_repos(body)
        } else {
            (Vec::new(), Vec::new())
        };
        let mut state = self.lock();
        if state.exchanges.contains_key(&id) {
            drop(state);
            self.drop_message(&id, "a request for an exchange obid has already");
            return;
        }
        // Revoked, obid can't answer it: one delivered after the revoke (in the same poll, or a
        // long poll alongside) never reaches the inbox or a rule. Under this lock, so the revoke's
        // settle either sees it or it sees the revoke.
        if state
            .membership
            .as_ref()
            .is_none_or(|m| m.state != "joined")
        {
            drop(state);
            self.drop_message(&id, "obid isn't in its Sphere any more");
            return;
        }
        // The sender's owner comes from the directory, never from the message; a removed member
        // is never admitted.
        let owner = state
            .directory
            .members
            .get(&from)
            .filter(|m| m.active)
            .map(|m| m.owner.clone());
        let Some(owner) = owner else {
            drop(state);
            self.drop_message(&id, "its sender isn't an active member in the directory");
            return;
        };
        let accepts = state
            .membership
            .as_ref()
            .map(Membership::accepts)
            .unwrap_or_default();
        let mut exchange = Exchange::new(
            "incoming",
            envelope.clone(),
            from.clone(),
            Some(owner.clone()),
        );
        exchange.messages.push(Link::received(&delivery));
        state.seen.insert(id.clone());
        let repo = body["repo"].as_str();
        let now = time::now();
        let mut item = None;
        let mut decisions_changed = false;
        let mut answer_by_itself = None;
        // Jobs on this Mac for teammates and shares of API accounts went with compute sharing: a
        // run_request or share_offer is declined like any kind obid doesn't take.
        if !accepts.contains(&kind) || !(kind == "review_request" || kind == "question") {
            exchange.state = "declined".into();
            let decline = self.reply_locked(
                &state,
                &exchange,
                "decline",
                json!({"reason": "out_of_scope"}),
            );
            exchange.outbox.extend(decline);
        } else if kind == "question" {
            match self.decide_question(&mut state, &mut exchange, &repos, &referenced, now) {
                Decided::Settled { decisions } => decisions_changed = decisions,
                Decided::Inbox(made) => item = Some(*made),
                Decided::ByItself(rule) => answer_by_itself = Some(rule),
            }
        } else if let Some(decision) =
            decisions::covering(&state.decisions, &kind, &owner, repo).cloned()
        {
            exchange.decided_by = Some("rule".into());
            exchange.rule = Some(decision.id.clone());
            if let Some(d) = state.decisions.iter_mut().find(|d| d.id == decision.id) {
                d.uses += 1;
                d.last_used = Some(now);
            }
            decisions_changed = true;
            if decision.decision == "accept" {
                self.accept_locked(&state, &mut exchange, Some(&decision.id));
            } else {
                exchange.state = "declined".into();
                let decline = self.reply_locked(
                    &state,
                    &exchange,
                    "decline",
                    json!({"reason": "owner_declined", "by_rule": decision.id}),
                );
                exchange.outbox.extend(decline);
            }
        } else if self.inbox_full(&state, &owner) {
            self.decline_busy(&state, &mut exchange);
        } else {
            let made = inbox::review_request_item(
                &exchange.correlation_id,
                &from,
                &owner,
                body,
                envelope["expires_at"].as_str().unwrap_or(""),
                now,
            );
            exchange.state = "inbox".into();
            exchange.inbox_item = Some(made.id.clone());
            state.inbox.insert(made.id.clone(), made.clone());
            item = Some(made);
        }
        state
            .exchanges
            .insert(exchange.correlation_id.clone(), exchange.clone());
        let decisions = decisions_changed.then(|| state.decisions.clone());
        drop(state);
        if let Some(decisions) = decisions {
            self.save_decisions(&decisions);
        }
        if let Some(item) = item {
            self.save_item(&item);
        }
        self.save_exchange(&exchange);
        self.outbox.notify_one();
        if let Some(rule) = answer_by_itself {
            let link = self.clone();
            let cid = exchange.correlation_id.clone();
            tokio::spawn(async move {
                link.answer_by_itself(cid, rule, repos, referenced).await;
            });
        }
    }

    /// The repositories a question is about (protocol.md 5.2), each once, compared ignoring ASCII
    /// case, and the job references among them that are obid's own jobs. A job reference counts
    /// only when the job is obid's own and runs in the repository it names (5.9): otherwise it is
    /// something obid doesn't know, and names no repository.
    pub(super) fn question_repos(&self, body: &Value) -> (Vec<String>, Vec<String>) {
        let mut repos: Vec<String> = Vec::new();
        let mut referenced = Vec::new();
        for r in body["about"].as_array().into_iter().flatten() {
            let repo = r["repo"].as_str().unwrap_or("");
            if r["kind"] == "job" {
                let job = r["job"].as_str().unwrap_or("");
                let ours = self
                    .jobs
                    .get(job)
                    .is_some_and(|j| j.repo.eq_ignore_ascii_case(repo));
                if !ours {
                    continue;
                }
                referenced.push(job.to_owned());
            }
            if !repo.is_empty() && !repos.iter().any(|r| r.eq_ignore_ascii_case(repo)) {
                repos.push(repo.to_owned());
            }
        }
        (repos, referenced)
    }

    /// Whether the sender's `owner` has as many requests waiting in the inbox as the limit allows.
    pub(super) fn inbox_full(&self, state: &State, owner: &str) -> bool {
        let waiting = state
            .inbox
            .values()
            .filter(|i| {
                i.kind == "obiRequest"
                    && i.done.is_none()
                    && i.obi.as_ref().is_some_and(|o| o["fromOwner"] == owner)
            })
            .count();
        waiting >= state.sharing.limits.inbox_per_owner as usize
    }

    /// Declines with reason `busy`: nothing reaches the inbox.
    pub(super) fn decline_busy(&self, state: &State, exchange: &mut Exchange) {
        exchange.state = "declined".into();
        exchange.decided_by = None;
        exchange.rule = None;
        let decline = self.reply_locked(state, exchange, "decline", json!({"reason": "busy"}));
        exchange.outbox.extend(decline);
    }

    /// A question (protocol.md section 11, step 5 and "Questions"): a remembered decline that
    /// covers each repository it is about declines it; the same question open already is `busy`;
    /// a sharing rule for the asker that covers it, with the self-answer limit not spent, lets the
    /// Obi answer by itself; else it goes to the owner's inbox, or is `busy` when that is full.
    pub(super) fn decide_question(
        &self,
        state: &mut State,
        exchange: &mut Exchange,
        repos: &[String],
        referenced: &[String],
        now: i64,
    ) -> Decided {
        let owner = exchange.peer_owner.clone().unwrap_or_default();
        if self.decline_remembered(state, exchange, repos, now) {
            return Decided::Settled { decisions: true };
        }
        let body = &exchange.request["body"];
        let asked_already = state.exchanges.values().any(|e| {
            e.direction == "incoming"
                && e.kind == "question"
                && e.peer_owner.as_deref() == Some(owner.as_str())
                && (e.state == "inbox" || e.state == "answering")
                && e.request["body"]["text"] == body["text"]
                && e.request["body"].get("about") == body.get("about")
        });
        if asked_already {
            self.decline_busy(state, exchange);
            return Decided::Settled { decisions: false };
        }
        if let Some(rule) = state.sharing.covering(&owner, repos).cloned() {
            let limits = state.sharing.limits;
            state.all_self_answers.retain(|t| *t > now - 3600);
            let all = state.all_self_answers.len();
            let started = state.self_answers.entry(owner.clone()).or_default();
            started.retain(|t| *t > now - 3600);
            if started.len() < limits.self_answers_per_hour as usize
                && all < limits.self_answers_per_hour_total as usize
            {
                // It counts when obid takes it on, whatever the run ends in.
                started.push(now);
                state.all_self_answers.push(now);
                exchange.self_answered = Some(now);
                exchange.state = "answering".into();
                exchange.decided_by = Some("sharing".into());
                exchange.rule = Some(rule.id.clone());
                return Decided::ByItself(rule);
            }
        }
        match self.question_to_inbox(state, exchange, repos, referenced, now) {
            Some(item) => Decided::Inbox(Box::new(item)),
            None => Decided::Settled { decisions: false },
        }
    }

    /// Declines the question with reason `owner_declined` when a remembered decline covers it,
    /// repository by repository (`by_rule` its id, counted as a use): whether it did.
    pub(super) fn decline_remembered(
        &self,
        state: &mut State,
        exchange: &mut Exchange,
        repos: &[String],
        now: i64,
    ) -> bool {
        let owner = exchange.peer_owner.clone().unwrap_or_default();
        let Some(decision) = decisions::covering_question(&state.decisions, &owner, repos).cloned()
        else {
            return false;
        };
        exchange.decided_by = Some("rule".into());
        exchange.rule = Some(decision.id.clone());
        if let Some(d) = state.decisions.iter_mut().find(|d| d.id == decision.id) {
            d.uses += 1;
            d.last_used = Some(now);
        }
        exchange.state = "declined".into();
        let decline = self.reply_locked(
            state,
            exchange,
            "decline",
            json!({"reason": "owner_declined", "by_rule": decision.id}),
        );
        exchange.outbox.extend(decline);
        true
    }

    /// A question obid is answering by itself that a remembered decline now covers (the owner said
    /// "Decline, and remember" while it waited or ran): declined as one that came in covered.
    /// Whether it was. Takes the lock.
    pub(super) fn declined_meanwhile(&self, cid: &str, repos: &[String]) -> bool {
        let (exchange, decisions) = {
            let mut state = self.lock();
            let Some(mut exchange) = state
                .exchanges
                .get(cid)
                .filter(|e| e.state == "answering")
                .cloned()
            else {
                return false;
            };
            let now = time::now();
            if !self.decline_remembered(&mut state, &mut exchange, repos, now) {
                return false;
            }
            exchange.updated = now;
            state.exchanges.insert(cid.to_owned(), exchange.clone());
            (exchange, state.decisions.clone())
        };
        self.save_decisions(&decisions);
        self.save_exchange(&exchange);
        self.outbox.notify_one();
        true
    }

    /// Puts the question in the owner's inbox (`inbox`), or declines it `busy` when the asker's
    /// owner has as many requests waiting there as the limit allows: the item, when one was made.
    pub(super) fn question_to_inbox(
        &self,
        state: &mut State,
        exchange: &mut Exchange,
        repos: &[String],
        referenced: &[String],
        now: i64,
    ) -> Option<Item> {
        let owner = exchange.peer_owner.clone().unwrap_or_default();
        if self.inbox_full(state, &owner) {
            self.decline_busy(state, exchange);
            return None;
        }
        let made = inbox::question_item(exchange, repos, referenced, now);
        exchange.state = "inbox".into();
        exchange.decided_by = None;
        exchange.rule = None;
        exchange.inbox_item = Some(made.id.clone());
        state.inbox.insert(made.id.clone(), made.clone());
        Some(made)
    }

    /// A question the Obi was answering by itself, and couldn't: to the owner's inbox (or `busy`).
    pub(super) fn to_owner(&self, cid: &str) {
        let Some(body) = self
            .lock()
            .exchanges
            .get(cid)
            .map(|e| e.request["body"].clone())
        else {
            return;
        };
        let (repos, referenced) = self.question_repos(&body);
        let (exchange, item) = {
            let mut state = self.lock();
            let Some(mut exchange) = state
                .exchanges
                .get(cid)
                .filter(|e| e.state == "answering")
                .cloned()
            else {
                return;
            };
            let now = time::now();
            let item = self.question_to_inbox(&mut state, &mut exchange, &repos, &referenced, now);
            exchange.updated = now;
            state.exchanges.insert(cid.to_owned(), exchange.clone());
            (exchange, item)
        };
        if let Some(item) = &item {
            self.save_item(item);
        }
        self.save_exchange(&exchange);
        self.outbox.notify_one();
    }

    /// Accepts the request: `accept` (with `by_rule` when a decision made it) waits to be sent, and
    /// the review job starts. When the job can't even start, a failed result follows.
    pub(super) fn accept_locked(&self, state: &State, exchange: &mut Exchange, rule: Option<&str>) {
        let body = match rule {
            Some(rule) => json!({"by_rule": rule}),
            None => json!({}),
        };
        exchange
            .outbox
            .extend(self.reply_locked(state, exchange, "accept", body));
        exchange.state = "accepted".into();
        let request = &exchange.request;
        let started = self.jobs.start_obi_review(ObiReview {
            correlation_id: exchange.correlation_id.clone(),
            request_id: request["id"].as_str().unwrap_or("").to_owned(),
            from: exchange.peer.clone(),
            from_owner: exchange.peer_owner.clone().unwrap_or_default(),
            body: request["body"].clone(),
        });
        match started {
            Ok(job) => exchange.job = Some(job.id),
            Err(refused) => {
                tracing::warn!(
                    "sphere: the review for {} couldn't start: {}",
                    exchange.correlation_id,
                    refused.0
                );
                exchange.error = Some(refused.0);
                let mut failed = failed_result(&exchange.request);
                if let Some(rule) = rule {
                    failed["by_rule"] = json!(rule);
                }
                exchange.result = Some(failed.clone());
                exchange.state = "failed".into();
                exchange
                    .outbox
                    .extend(self.reply_locked(state, exchange, "result", failed));
            }
        }
    }

    /// A reply in `exchange`, built and signed (runner-api.md, "Expiry of what obid sends"):
    /// created now, expiring the later of an hour from now and the request's expiry, at most the
    /// longest a message may live. `None` when obid has no key or membership to sign it with.
    pub(super) fn reply_locked(
        &self,
        state: &State,
        exchange: &Exchange,
        kind: &str,
        mut body: Value,
    ) -> Option<Value> {
        let key = state.key.as_ref()?;
        // A guest job's decline says only unavailable, whatever this Mac's reason (its owner at it,
        // no slot, sharing off, not allowed, no VM now), and names no rule: why stays here, in the
        // log (the owner's decision of 2026-10-21; spec protocol.md section 4, rule 9). Only the
        // guest's own doing keeps its reason.
        // Nor whether a rule or this owner in person said yes or no.
        if exchange.kind == "run_request"
            && let Some(map) = body.as_object_mut()
        {
            map.remove("by_rule");
        }
        if kind == "decline" && exchange.kind == "run_request" {
            let reason = body["reason"].as_str().unwrap_or("other").to_owned();
            let said = match reason.as_str() {
                "expired" | "out_of_scope" | "share_used" => reason.clone(),
                _ => "unavailable".to_owned(),
            };
            tracing::info!(
                "sphere: declined {} ({reason}); the guest is told {said}",
                exchange.correlation_id
            );
            body = json!({"reason": said});
        }
        let membership = state.membership.as_ref()?;
        let now = time::now();
        let request_expires = exchange.expires_at().unwrap_or(now);
        let expires = (now + REPLY_LIFETIME)
            .max(request_expires)
            .min(now + obi_protocol::MAX_LIFETIME_SECONDS);
        let unsigned = json!({
            "protocol": obi_protocol::PROTOCOL,
            "id": new_uuid(),
            "type": kind,
            "from": membership.obi_id,
            "to": exchange.peer,
            "sphere": membership.sphere,
            "correlation_id": exchange.correlation_id,
            "in_reply_to": exchange.correlation_id,
            "created_at": time::format(now),
            "expires_at": time::format(expires),
            "key_id": key.key_id(),
            "body": body,
        });
        let Value::Object(unsigned) = unsigned else {
            unreachable!("an envelope is an object")
        };
        match obi_protocol::envelope::sign(&unsigned, key) {
            Ok(signed) => Some(signed),
            Err(e) => {
                tracing::error!("sphere: can't sign a {kind}: {e}");
                None
            }
        }
    }
}
