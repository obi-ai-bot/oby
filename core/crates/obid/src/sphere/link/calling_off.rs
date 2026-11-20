//! Calling off a request this Oby sent (docs/contracts/obi-requests.md, OBIREQ-1 to 6): the
//! `cancel` reply built, signed and queued in the exchange's outbox, sent once by the sender as any
//! reply is, and the exchange marked `cancelled`. When it is due is [`calloff`]'s to say; this
//! follows the jobs' events and the owner's Stop to ask it. It never changes a job, nor an
//! exchange it received.

use std::sync::Arc;

use axum::http::StatusCode;
use obi_protocol::time;
use serde_json::{Value, json};

use super::*;
use crate::sphere::calloff;

/// Why a call-off didn't happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotCalledOff {
    /// obid has no such exchange.
    Unknown,
    /// The other Oby asked it: only its sender calls a request off.
    Incoming,
    /// It already ended, or never was a request a `cancel` calls off.
    Ended,
    /// obid has no Obi key or membership to sign a `cancel` with.
    Unsigned,
}

impl Link {
    /// Calls off the request `cid` this Oby sent, for `why` (the log's): a `cancel` in reply to
    /// it goes in the exchange's outbox, which the sender sends once, now or once the Sphere is
    /// back (also after a restart: the outbox is kept with the exchange), and the exchange reads
    /// `cancelled` (OBIREQ-1 to 6). Nothing is sent for one that already ended.
    pub fn call_off(&self, cid: &str, why: &str) -> Result<Exchange, NotCalledOff> {
        let exchange = {
            let mut state = self.lock();
            let exchange = state.exchanges.get(cid).ok_or(NotCalledOff::Unknown)?;
            if exchange.direction != "outgoing" {
                return Err(NotCalledOff::Incoming);
            }
            if !calloff::open(exchange) {
                return Err(NotCalledOff::Ended);
            }
            let cancel = self
                .reply_locked(&state, exchange, "cancel", json!({}))
                .ok_or(NotCalledOff::Unsigned)?;
            let exchange = state.exchanges.get_mut(cid).ok_or(NotCalledOff::Unknown)?;
            exchange.outbox.push(cancel);
            exchange.state = "cancelled".into();
            exchange.updated = time::now();
            exchange.clone()
        };
        tracing::info!("sphere: exchange {cid}: called off: {why}");
        self.save_exchange(&exchange);
        self.outbox.notify_one();
        Ok(exchange)
    }

    /// `POST /v1/obid/sphere/exchanges/{id}/cancel`: the owner calls off a request of theirs
    /// (OBIREQ-5): the exchange; `404` for one obid doesn't have, `409` for one that ended or that
    /// another Oby sent.
    pub fn call_off_by_owner(&self, cid: &str) -> Result<Value, ApiError> {
        match self.call_off(cid, calloff::BY_OWNER) {
            Ok(exchange) => Ok(exchange.to_json()),
            Err(NotCalledOff::Unknown) => {
                Err(api(StatusCode::NOT_FOUND, "There's no such exchange."))
            }
            Err(NotCalledOff::Incoming) => Err(api(
                StatusCode::CONFLICT,
                "Another Oby asked this: only the Oby that asks calls a request off.",
            )),
            Err(NotCalledOff::Ended) => Err(api(
                StatusCode::CONFLICT,
                "It already ended: nothing was sent.",
            )),
            Err(NotCalledOff::Unsigned) => Err(api(
                StatusCode::CONFLICT,
                "obid isn't in an Obysphere with an Obi key: nothing was sent.",
            )),
        }
    }

    /// Calls off each of `due`, `(exchange, why)`; one that ended meanwhile sends nothing.
    fn call_off_all(&self, due: Vec<(String, &'static str)>) {
        for (cid, why) in due {
            if let Err(not) = self.call_off(&cid, why) {
                tracing::debug!("sphere: exchange {cid}: not called off ({not:?})");
            }
        }
    }

    /// Calls off what job `id`'s end calls off ([`calloff::due_for_job`]): it stopped, its pull
    /// request merged or closed, or its head moved on.
    pub(super) fn work_changed(&self, id: &str) {
        let Some(job) = self.jobs.get(id) else { return };
        let due = calloff::due_for_job(&job, self.lock().exchanges.values());
        self.call_off_all(due);
    }

    /// [`Link::work_changed`] for every job: at start, and when obid's events went faster than
    /// they were read.
    fn every_work_changed(&self) {
        let jobs = self.jobs.list(usize::MAX);
        let due: Vec<(String, &'static str)> = {
            let state = self.lock();
            jobs.iter()
                .flat_map(|job| calloff::due_for_job(job, state.exchanges.values()))
                .collect()
        };
        self.call_off_all(due);
    }

    /// Follows the jobs' events for as long as the Link is there, calling off what a job's end
    /// calls off (OBIREQ-1, 3), after looking once at every job: an end while obid was down.
    pub(super) fn watch_work_ending(self: &Arc<Self>) {
        let link = Arc::downgrade(self);
        let mut events = self.events.subscribe();
        self.every_work_changed();
        tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let Some(link) = link.upgrade() else { return };
                        link.every_work_changed();
                        continue;
                    }
                    Err(_) => return,
                };
                if event.name != "job" {
                    continue;
                }
                let Some(link) = link.upgrade() else { return };
                if let Some(id) = event.data["id"].as_str() {
                    link.work_changed(id);
                }
            }
        });
    }

    /// Before asking about `head` of pull request `number` of `repo`, calls off every open request
    /// for its approval at another head (OBIREQ-2): one request per pull request stays open.
    pub(super) fn call_off_other_heads(&self, repo: &str, number: u64, head: &str) {
        let due = calloff::other_heads(self.lock().exchanges.values(), repo, number, head);
        self.call_off_all(
            due.into_iter()
                .map(|cid| (cid, calloff::ASKED_AGAIN))
                .collect(),
        );
    }

    /// The owner stopped the turn under way in `conversation`, which began at `since`: the
    /// questions it asked other Obys are called off (OBIREQ-4).
    pub fn call_off_turn(&self, conversation: &str, since: i64) {
        let due = calloff::asked_in_turn(self.lock().exchanges.values(), conversation, since);
        self.call_off_all(
            due.into_iter()
                .map(|cid| (cid, calloff::TURN_STOPPED))
                .collect(),
        );
    }
}

#[cfg(test)]
mod tests {
    use obi_protocol::keys::Keypair;
    use tokio::sync::watch;

    use super::*;
    use crate::sphere::membership::Membership;

    fn joined_link(name: &str) -> (Arc<Link>, std::path::PathBuf, watch::Sender<bool>) {
        let home = std::env::temp_dir().join(format!("obid-calloff-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "wellington"}).to_string(),
        )
        .unwrap();
        let events = Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (stop, stopping) = watch::channel(false);
        let link = Link::open(config, jobs, events, stopping);
        {
            let mut state = link.lock();
            state.key = Some(Arc::new(Keypair::from_seed_string("calloff test")));
            state.membership = Some(Membership {
                state: "joined".into(),
                url: "http://127.0.0.1:1".into(),
                sphere: "team".into(),
                obi_id: "obi-wellington".into(),
                card: json!({}),
                joined_at: 0,
                revoked: None,
            });
        }
        (link, home, stop)
    }

    fn asked(link: &Link, cid: &str, head: &str) {
        let body = json!({"goal": "G", "repo": "acme/app", "pr": 12, "head_sha": head,
                          "approve_on_host": true});
        let request = json!({"id": cid, "correlation_id": cid, "type": "review_request",
                             "expires_at": time::format(time::now() + 3600), "body": body});
        let mut e = Exchange::new("outgoing", request, "obi-zoe".into(), Some("zoe".into()));
        e.state = "sent".into();
        link.lock().exchanges.insert(cid.into(), e);
    }

    /// OBIREQ-2: asking about the pull request's new head calls off the request at the old one
    /// first, with one `cancel` in reply to it, and leaves the request at the new head alone, so one
    /// request per pull request stays open; a second call-off of it sends nothing.
    #[tokio::test]
    async fn obireq_2_a_new_head_calls_off_the_old_request() {
        let (link, home, _stop) = joined_link("new-head");
        let (old, new) = ("c-old", "c-new");
        asked(&link, old, "a".repeat(40).as_str());
        asked(&link, new, "b".repeat(40).as_str());
        let head = "b".repeat(40);
        let change = approval::Change {
            repo: "Acme/App",
            pr: 12,
            base: "dev",
            head_branch: "oby/fix-12",
            head: &head,
            goal: "Fix it",
        };
        let _ = link.ask_teammate(&change).await;
        let state = link.lock();
        let called_off = &state.exchanges[old];
        assert_eq!(called_off.state, "cancelled");
        assert_eq!(called_off.outbox.len(), 1);
        let cancel = &called_off.outbox[0];
        assert_eq!(cancel["type"], "cancel");
        assert_eq!(cancel["in_reply_to"], old);
        assert_eq!(cancel["to"], "obi-zoe");
        assert_eq!(cancel["body"], json!({}));
        assert_eq!(
            state.exchanges[new].state, "sent",
            "the request at the new head stays"
        );
        assert!(state.exchanges[new].outbox.is_empty());
        drop(state);
        assert_eq!(
            link.call_off(old, calloff::BY_OWNER).unwrap_err(),
            NotCalledOff::Ended,
            "once"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// OBIREQ-4: the owner stops a turn under way: the questions its conversation asked since the
    /// turn began are called off, and no other conversation's, nor one asked before it.
    #[tokio::test]
    async fn obireq_4_a_stopped_turn_calls_off_its_questions() {
        let (link, home, _stop) = joined_link("turn");
        let question = |cid: &str, created: i64, conversation: &str| {
            let request = json!({"id": cid, "correlation_id": cid, "type": "question",
                                 "expires_at": time::format(time::now() + 3600),
                                 "body": {"text": "Q"}});
            let mut e = Exchange::new("outgoing", request, "obi-zoe".into(), Some("zoe".into()));
            e.state = "sent".into();
            e.created = created;
            e.asked_in = Some(conversation.into());
            link.lock().exchanges.insert(cid.into(), e);
        };
        let now = time::now();
        question("this-turn", now, "c1");
        question("earlier-turn", now - 600, "c1");
        question("elsewhere", now, "c2");
        link.call_off_turn("c1", now - 5);
        let state = link.lock();
        assert_eq!(state.exchanges["this-turn"].state, "cancelled");
        assert_eq!(state.exchanges["this-turn"].outbox[0]["type"], "cancel");
        assert_eq!(state.exchanges["earlier-turn"].state, "sent");
        assert_eq!(state.exchanges["elsewhere"].state, "sent");
        drop(state);
        let _ = std::fs::remove_dir_all(&home);
    }
}
