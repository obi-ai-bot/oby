//! Outbound reply queue, envelope sealing & fitting, expiry clock, and revocation settlement.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use obi_protocol::client::ClientError;
use obi_protocol::time;
use serde_json::{Value, json};

use super::*;

impl Link {
    // -----------------------------------------------------------------------------------------
    // Sending replies, and the clock

    /// Starts the sender and the clock, once, when obid has a membership: the clock runs whatever
    /// its state, and the sender sends only while it's `joined`.
    pub(super) fn start_sender(self: &Arc<Self>) {
        if self.sender_running.swap(true, Ordering::SeqCst) {
            return;
        }
        let link = self.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = link.clone().run_sender() => {}
                () = stopped(link.stopping.clone()) => {}
            }
        });
    }

    pub(super) async fn run_sender(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(1);
        loop {
            self.expire();
            // Revoked, nothing is sent: the clock alone goes on, once a minute.
            let wait = if !self.is_joined() {
                Duration::from_secs(60)
            } else if self.send_waiting().await {
                backoff = Duration::from_secs(1);
                Duration::from_secs(2)
            } else {
                let wait = backoff;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                wait
            };
            tokio::select! {
                () = self.outbox.notified() => {}
                () = tokio::time::sleep(wait) => {}
            }
        }
    }

    /// Sends every waiting reply, exchange by exchange, oldest first: `false` when one couldn't
    /// go and should be tried again.
    pub(super) async fn send_waiting(&self) -> bool {
        let mut waiting: Vec<(i64, String)> = self
            .lock()
            .exchanges
            .values()
            .filter(|e| !e.outbox.is_empty())
            .map(|e| (e.updated, e.correlation_id.clone()))
            .collect();
        waiting.sort();
        let mut all = true;
        for (_, cid) in waiting {
            all &= self.flush(&cid).await;
        }
        all
    }

    /// Sends the exchange's waiting replies, in order: `true` once none is left to send. Nothing
    /// leaves while obid isn't `joined` (revoked): what waits stays unsent.
    pub(super) async fn flush(&self, cid: &str) -> bool {
        let _one = self.sending.lock().await;
        // Whether a reply goes sealed is the directory's to say: read it once before the first.
        if self.lock().directory_read == 0
            && let Err(e) = self.read_directory().await
        {
            self.note_error(&e);
            return false;
        }
        loop {
            let next = {
                let state = self.lock();
                if state
                    .membership
                    .as_ref()
                    .is_none_or(|m| m.state != "joined")
                {
                    return false;
                }
                match state.exchanges.get(cid).and_then(|e| e.outbox.first()) {
                    Some(next) => next.clone(),
                    None => return true,
                }
            };
            let expires = next["expires_at"]
                .as_str()
                .and_then(time::parse)
                .unwrap_or(0);
            let first = next["id"].clone();
            if expires <= time::now() {
                self.update_exchange(cid, |e| {
                    // A cancel may have cleared the outbox meanwhile.
                    if e.outbox.first().is_some_and(|f| f["id"] == first) {
                        e.outbox.remove(0);
                    }
                    e.error = Some("A reply expired before the Sphere took it.".into());
                });
                continue;
            }
            // Before its first try a reply is sealed for the peer and fitted to the most a message
            // may be (protocol.md section 9.1); once tried, it goes again as it is, so a retry
            // after an answer that never came sends the same bytes.
            let id = next["id"].as_str().unwrap_or("").to_owned();
            let tried = self
                .lock()
                .exchanges
                .get(cid)
                .and_then(|e| e.tried.as_deref())
                == Some(id.as_str());
            let next = if tried {
                next
            } else {
                match self.prepare_reply(cid, &next) {
                    Ok(prepared) => prepared,
                    Err(why) => {
                        // It waits, never going in the clear instead, for the directory read
                        // before its next try to list a seal key obid trusts.
                        tracing::warn!("sphere: exchange {cid}: {why}");
                        self.update_exchange(cid, |e| e.error = Some(why.clone()));
                        if let Err(e) = self.read_directory().await {
                            self.note_error(&e);
                        }
                        return false;
                    }
                }
            };
            self.update_exchange(cid, |e| {
                if let Some(first) = e.outbox.first_mut() {
                    *first = next.clone();
                }
                if next["type"] == "result" {
                    e.result = Some(next["body"].clone());
                }
                e.tried = Some(id.clone());
            });
            let peer = next["to"].as_str().unwrap_or("").to_owned();
            let sent = match self.token().await {
                Ok(token) => match self.client() {
                    Ok(client) => client.send(&token, &wire(&next)).await,
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };
            // The Sphere checks a message's id before its seal (sphere-api.md 6.1, checks 8 and
            // 11), so these refusals say it kept nothing under this id: the peer's card gained a
            // seal key, or another one. The directory is read again, and the reply prepared afresh.
            let reseal = match &sent {
                Err(ClientError::Refused { code, .. }) if code == "not_sealed" => {
                    self.pins.mark_seals(&peer);
                    true
                }
                Err(ClientError::Refused { code, .. }) if code == "invalid_envelope" => {
                    next.get("seal").is_some()
                }
                _ => false,
            };
            if reseal {
                if let Err(e) = self.read_directory().await {
                    self.note_error(&e);
                    return false;
                }
                let now_for = self
                    .lock()
                    .directory
                    .members
                    .get(&peer)
                    .and_then(|m| m.seal)
                    .map(|key| obi_protocol::seal::key_id_of(&key));
                if next.get("seal").is_none()
                    || now_for.as_deref() != next["seal"]["key_id"].as_str()
                {
                    self.update_exchange(cid, |e| e.tried = None);
                    continue;
                }
            }
            match sent {
                Ok(sent) => {
                    self.keep_receipt(&id, &sent);
                    self.lock().seen.insert(id);
                    self.update_exchange(cid, |e| {
                        // A cancel may have cleared the outbox while it was sent.
                        if e.outbox.first().is_some_and(|f| f["id"] == first) {
                            e.outbox.remove(0);
                        }
                        e.tried = None;
                        e.messages.push(Message {
                            direction: "sent".into(),
                            seq: Some(sent.seq),
                            at: sent.recorded_at.clone(),
                            envelope: next.clone(),
                        });
                    });
                }
                Err(ClientError::Refused {
                    status,
                    code,
                    message,
                }) if status < 500 && status != 401 && status != 429 && status != 408 => {
                    // The Sphere won't take it, now or later: what's left of the exchange can't go.
                    let line = format!(
                        "The Sphere refused a {}: {code}: {message}",
                        next["type"].as_str().unwrap_or("reply")
                    );
                    tracing::warn!("sphere: exchange {cid}: {line}");
                    self.update_exchange(cid, |e| {
                        e.outbox.clear();
                        e.tried = None;
                        e.error = Some(line.clone());
                    });
                    return true;
                }
                Err(e) => {
                    self.refused_token(&e);
                    self.note_error(&e);
                    return false;
                }
            }
        }
    }

    /// The exchange's first waiting reply, `next`, made ready for its first try (protocol.md
    /// section 9.1): sealed for the peer when its trusted card has a seal key, signed again, and its
    /// words shortened until it fits ([`fitted`]). `Err` with why keeps it waiting: a peer known to
    /// seal with no seal key obid trusts now, or a seal that failed. Nothing goes in the clear to an
    /// Obi that seals; a peer whose card has no seal key yet (it may still seal what it sends)
    /// gets words in the clear.
    pub(super) fn prepare_reply(&self, cid: &str, next: &Value) -> Result<Value, String> {
        let (recipient, must_seal, key, peer) = {
            let state = self.lock();
            let exchange = state.exchanges.get(cid).ok_or("the exchange is gone")?;
            let peer = exchange.peer.clone();
            (
                state.directory.members.get(&peer).and_then(|m| m.seal),
                self.pins.seals(&peer),
                state.key.clone().ok_or("obid has no Obi key")?,
                peer,
            )
        };
        if recipient.is_none() && must_seal {
            return Err(format!(
                "A reply waits for a seal key of {peer}'s that obid trusts: it never goes in the clear."
            ));
        }
        let mut unsigned = next.as_object().cloned().unwrap_or_default();
        for member in ["signature", "seal", "sealed"] {
            unsigned.remove(member);
        }
        fitted(next["body"].clone(), |body| {
            let mut unsigned = unsigned.clone();
            unsigned.insert("body".into(), body.clone());
            if let Some(recipient) = &recipient {
                obi_protocol::seal::seal(&mut unsigned, recipient)?;
            }
            let signed =
                obi_protocol::envelope::sign(&unsigned, &key).map_err(|e| e.to_string())?;
            Ok(if recipient.is_some() {
                kept(signed, body.clone())
            } else {
                signed
            })
        })
    }

    /// Changes the exchange `cid`, keeps it and tells the apps.
    pub(super) fn update_exchange(
        &self,
        cid: &str,
        change: impl FnOnce(&mut Exchange),
    ) -> Option<Exchange> {
        let exchange = {
            let mut state = self.lock();
            let exchange = state.exchanges.get_mut(cid)?;
            change(exchange);
            exchange.updated = time::now();
            exchange.clone()
        };
        self.save_exchange(&exchange);
        Some(exchange)
    }

    /// Requests that expired: one waiting in the inbox (or a question the Obi is still answering by
    /// itself) is settled by itself, with no answer, and declined with reason `expired`; one this
    /// Obi sent with no result or answer is `expired`. It runs whatever the membership: a revoked
    /// obid's requests expire too (their declines never go: nothing leaves once revoked).
    pub(super) fn expire(&self) {
        let now = time::now();
        self.settle_open(
            |e| {
                e.expires_at().is_some_and(|t| t <= now)
                    && e.kind != "share_offer"
                    && match e.direction.as_str() {
                        "incoming" => e.state == "inbox" || e.state == "answering",
                        _ => e.state == "sent" || e.state == "accepted",
                    }
            },
            "expired",
            Some("expired"),
            None,
        );
        self.settle_host_approvals(
            |i| approval::ask_expiry(i).flatten().is_none_or(|t| t <= now),
            "It lapsed when the request expired: nothing was approved.",
        );
    }

    /// obid's membership was revoked: no open incoming request can be answered any more. Each one
    /// in the inbox, being answered by itself, or accepted with its review job is settled here,
    /// `declined` with `error` saying why and its inbox item done, its review job stopped; no reply
    /// is built (nothing leaves once revoked) and its peer isn't told. What this Obi sent is left
    /// to the clock.
    pub(super) fn settle_on_revoke(&self) {
        let settled = self.settle_open(
            |e| {
                e.direction == "incoming"
                    && (e.state == "inbox"
                        || e.state == "answering"
                        || (e.state == "accepted" && e.job.is_some()))
            },
            "declined",
            None,
            Some(REVOKED),
        );
        for job in settled.iter().filter_map(|e| e.job.as_deref()) {
            self.jobs.stop(job);
        }
        self.settle_host_approvals(
            |_| true,
            "This Oby left its Obysphere: nothing was approved.",
        );
    }

    /// Settles the requests `due` picks: each goes to `settled` (with `error` when given), an
    /// incoming one with its inbox item done and a decline for `reason` queued when there is one.
    /// Answers what it settled.
    pub(super) fn settle_open(
        &self,
        due: impl Fn(&Exchange) -> bool,
        settled: &str,
        reason: Option<&str>,
        error: Option<&str>,
    ) -> Vec<Exchange> {
        let now = time::now();
        let mut changed = Vec::new();
        let mut items = Vec::new();
        {
            let mut state = self.lock();
            let due: Vec<String> = state
                .exchanges
                .values()
                .filter(|e| due(e))
                .map(|e| e.correlation_id.clone())
                .collect();
            for cid in due {
                let Some(mut exchange) = state.exchanges.get(&cid).cloned() else {
                    continue;
                };
                exchange.state = settled.into();
                exchange.updated = now;
                if let Some(error) = error {
                    exchange.error = Some(error.into());
                }
                if exchange.direction == "incoming" {
                    if let Some(reason) = reason {
                        let decline = self.reply_locked(
                            &state,
                            &exchange,
                            "decline",
                            json!({"reason": reason}),
                        );
                        exchange.outbox.extend(decline);
                    }
                    if let Some(item) = exchange
                        .inbox_item
                        .as_ref()
                        .and_then(|id| state.inbox.get_mut(id))
                        .filter(|i| i.done.is_none())
                    {
                        item.done = Some(now);
                        items.push(item.clone());
                    }
                }
                state.exchanges.insert(cid, exchange.clone());
                changed.push(exchange);
            }
        }
        for item in &items {
            self.save_item(item);
        }
        for exchange in &changed {
            self.save_exchange(exchange);
        }
        changed
    }
}
