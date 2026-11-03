//! An exchange: one request and its replies, as this Obi sees it (docs/runner-api.md, "obid only:
//! Obi-to-Obi exchanges"). Kept one file per exchange in `home/exchanges/` (0600).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::store;

/// How long a question goes unanswered, with no word from the other Oby, before the owner hears so.
pub const QUIET: i64 = 120;

/// One envelope sent or received in an exchange, exactly as signed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// `sent` or `received`.
    pub direction: String,
    /// The Sphere's audit seq, when known.
    pub seq: Option<u64>,
    /// When the Sphere recorded it (`recorded_at`, `relayed_at`).
    pub at: String,
    pub envelope: Value,
}

/// An exchange as obid keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exchange {
    pub correlation_id: String,
    /// `outgoing` or `incoming`.
    pub direction: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub peer: String,
    pub peer_owner: Option<String>,
    pub state: String,
    pub decided_by: Option<String>,
    pub rule: Option<String>,
    pub inbox_item: Option<String>,
    pub job: Option<String>,
    pub publish_scan: Option<Value>,
    pub messages: Vec<Message>,
    pub result: Option<Value>,
    pub error: Option<String>,
    /// Seconds since the epoch.
    pub created: i64,
    pub updated: i64,
    /// The request, whatever its direction: the first message.
    pub request: Value,
    /// Replies built and signed, waiting to be sent, oldest first. Never in the API's answer.
    #[serde(default)]
    pub outbox: Vec<Value>,
    /// When obid took the question on to answer by itself (a model run), whatever the run ended
    /// in: what the self-answer limits count, also after a restart. Never in the API's answer.
    #[serde(default)]
    pub self_answered: Option<i64>,
    /// The id of the outbox's first reply once obid tried to send it: from then on it goes as it
    /// is, never signed again, unless the Sphere refused it in a way that says it kept nothing.
    /// Never in the API's answer.
    #[serde(default)]
    pub tried: Option<String>,
    /// The approval obid posted on GitHub for this incoming review (`job`, `repo`, `pr`, `id`,
    /// `url`), so that a cancel arriving after it dismisses it (protocol.md section 11, "Calling a
    /// request off"). Never in the API's answer.
    #[serde(default)]
    pub host_approval: Option<Value>,
    /// The conversation whose owner asked this question (`oby_ask`, docs/contracts/ask-an-oby.md):
    /// what comes back posts there. Never in the API's answer.
    #[serde(default)]
    pub asked_in: Option<String>,
    /// What the owner knew the person as when they asked: what comes back goes by it, whatever
    /// their card says by then. Never in the API's answer.
    #[serde(default)]
    pub asked_name: Option<String>,
    /// What obid already told its owner about this exchange (crate::brain::ask), so a restart or a
    /// later event never says it twice. Never in the API's answer.
    #[serde(default)]
    pub told: Vec<String>,
    /// The owner's own signed words asked this from their thread, with no model of theirs
    /// (docs/contracts/peer-thread.md): the thread marks its question as theirs. Only here: a
    /// question's body has no `by` in the protocol. Never in the API's answer.
    #[serde(default)]
    pub asked_by_owner: bool,
}

impl Exchange {
    /// A new exchange for `request`.
    pub fn new(
        direction: &str,
        request: Value,
        peer: String,
        peer_owner: Option<String>,
    ) -> Exchange {
        let now = obi_protocol::time::now();
        Exchange {
            correlation_id: request["correlation_id"].as_str().unwrap_or("").to_owned(),
            direction: direction.to_owned(),
            kind: request["type"].as_str().unwrap_or("").to_owned(),
            peer,
            peer_owner,
            state: String::new(),
            decided_by: None,
            rule: None,
            inbox_item: None,
            job: None,
            publish_scan: None,
            messages: Vec::new(),
            result: None,
            error: None,
            created: now,
            updated: now,
            request,
            outbox: Vec::new(),
            self_answered: None,
            tried: None,
            host_approval: None,
            asked_in: None,
            asked_name: None,
            told: Vec::new(),
            asked_by_owner: false,
        }
    }

    /// When the request expires, as seconds since the epoch.
    pub fn expires_at(&self) -> Option<i64> {
        self.request["expires_at"]
            .as_str()
            .and_then(obi_protocol::time::parse)
    }

    /// Whether the exchange has ended: a `decline`, `result` or `answer` was sent or received, or
    /// built and waiting to be sent; or it's `declined` with none (settled here at a revoke, or a
    /// decline the Sphere refused).
    pub fn ended(&self) -> bool {
        self.state == "declined"
            || self
                .messages
                .iter()
                .map(|m| &m.envelope)
                .chain(self.outbox.iter())
                .any(|e| obi_protocol::TERMINAL_TYPES.contains(&e["type"].as_str().unwrap_or("")))
    }

    /// Whether the other Oby said only its person can answer.
    pub fn waiting_on_them(&self) -> bool {
        self.messages.iter().any(|m| {
            m.direction == "received"
                && m.envelope["type"] == "status"
                && m.envelope["body"]["state"] == "waiting_on_owner"
        })
    }

    /// The envelope ids it holds.
    pub fn envelope_ids(&self) -> impl Iterator<Item = &str> {
        self.messages
            .iter()
            .filter_map(|m| m.envelope["id"].as_str())
    }

    /// As the runner API shows it.
    pub fn to_json(&self) -> Value {
        let date = obi_protocol::time::format;
        json!({
            "correlationId": self.correlation_id,
            "direction": self.direction,
            "type": self.kind,
            "peer": self.peer,
            "peerOwner": self.peer_owner,
            "state": self.state,
            "decidedBy": self.decided_by,
            "rule": self.rule,
            "inboxItem": self.inbox_item,
            "job": self.job,
            "publishScan": self.publish_scan,
            "messages": self.messages,
            "result": self.result,
            "error": self.error,
            "created": date(self.created),
            "updated": date(self.updated),
        })
    }

    /// What a member's thread shows of it (docs/contracts/peer-thread.md): one post per message, in
    /// the room's shape, oldest first, typed fields and the words only. `me` is this Oby's owner, and
    /// `verified` whether the owner compared the other's owner key. A reply still in `outbox` isn't
    /// here: it may be sealed already, and `exchange.state` says what's coming. Nor is a request being
    /// sent: a refusal removes it with no event, so it shows once the Sphere has it, or with `error`
    /// once obid doesn't know whether it does.
    pub fn posts(&self, me: &str, verified: bool) -> Vec<Value> {
        let unconfirmed =
            self.error.is_some() && !self.envelope_ids().any(|id| self.request["id"] == id);
        let about = self.about_json();
        let empty = json!([]);
        let post = |direction: &str, seq: Option<u64>, at: Option<&str>, envelope: &Value| {
            let (kind, body) = (envelope["type"].as_str().unwrap_or(""), &envelope["body"]);
            let sent = direction == "sent";
            let waiting = at.is_none();
            json!({
                "id": envelope["id"],
                "seq": seq,
                "recordedAt": at,
                "from": envelope["from"],
                "owner": if sent { Some(me) } else { self.peer_owner.as_deref() },
                "kind": kind,
                "by": if sent && kind == "question" && self.asked_by_owner { "owner" } else { said_by(kind, body) },
                "createdAt": envelope["created_at"],
                "replyTo": envelope["in_reply_to"],
                "text": words_of(kind, body),
                "about": if body["about"].is_array() { &body["about"] } else if body["refs"].is_array() { &body["refs"] } else { &empty },
                "state": if waiting { "waiting" } else { "sent" },
                // Never what a Sphere said: `GET /v1/obid/sphere/exchanges/{id}` has the exchange's own.
                "error": waiting.then_some("Not confirmed yet: the Obysphere didn't answer."),
                "verified": sent || verified,
                "direction": direction,
                "status": (kind == "status").then(|| &body["state"]),
                "reason": (kind == "decline").then(|| &body["reason"]),
                "exchange": about,
            })
        };
        let mut posts = Vec::with_capacity(self.messages.len() + 1);
        if unconfirmed {
            let direction = if self.direction == "outgoing" {
                "sent"
            } else {
                "received"
            };
            posts.push(post(direction, None, None, &self.request));
        }
        posts.extend(
            self.messages
                .iter()
                .map(|m| post(&m.direction, m.seq, Some(&m.at), &m.envelope)),
        );
        posts
    }

    /// What every post of a thread says of its exchange. `quietAt`: when an outgoing question with
    /// no word yet counts as unanswered ([`QUIET`]), which the clock decides, so no event says it.
    fn about_json(&self) -> Value {
        let waiting = self.waiting_on_them();
        let open = matches!(self.state.as_str(), "sent" | "accepted");
        let quiet = self.direction == "outgoing" && self.kind == "question" && open && !waiting;
        json!({
            "id": self.correlation_id,
            "type": self.kind,
            "direction": self.direction,
            "state": self.state,
            "waitingOnThem": waiting && open,
            "quietAt": quiet.then(|| obi_protocol::time::format(self.created + QUIET)),
        })
    }
}

/// Who a message's body says wrote it: its sender's own claim, as `by` on a room's post.
fn said_by(kind: &str, body: &Value) -> &'static str {
    let owners = body["by"] == "owner" || (kind == "decline" && body["reason"] == "owner_declined");
    if owners { "owner" } else { "obi" }
}

/// The words of a message, as someone wrote them: data, shown only as a quote.
fn words_of(kind: &str, body: &Value) -> Value {
    let key = match kind {
        "question" | "answer" => "text",
        "status" | "decline" => "note",
        "review_request" | "handoff" => "goal",
        "result" => "summary",
        _ => return Value::Null,
    };
    body.get(key)
        .filter(|words| words.is_string())
        .cloned()
        .unwrap_or(Value::Null)
}

/// Where exchanges are kept.
pub struct Exchanges {
    dir: PathBuf,
}

impl Exchanges {
    /// `home/exchanges`; one that can't be made is said in the log, and each save then fails.
    pub fn open(home: &Path) -> Exchanges {
        let dir = home.join("exchanges");
        if let Err(e) = store::make_private_dir(&dir) {
            tracing::error!("can't make {}: {e}", dir.display());
        }
        Exchanges { dir }
    }

    /// A correlation id is a lower-case UUID (the envelope's schema checked it): safe as a name.
    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn save(&self, exchange: &Exchange) -> std::io::Result<()> {
        let text = serde_json::to_vec_pretty(exchange).map_err(std::io::Error::other)?;
        store::write_private(&self.path(&exchange.correlation_id), &text)
    }

    pub fn remove(&self, id: &str) {
        let _ = std::fs::remove_file(self.path(id));
    }

    pub fn load_all(&self) -> Vec<Exchange> {
        store::load_dir(&self.dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(direction: &str, kind: &str, body: Value) -> Message {
        Message {
            direction: direction.into(),
            seq: Some(1),
            at: "2026-11-03T12:00:00Z".into(),
            envelope: json!({"id": kind, "type": kind, "from": "obi-sam", "body": body}),
        }
    }

    fn asked(kind: &str, body: Value) -> Exchange {
        let request = json!({"id": "c", "correlation_id": "c", "type": kind, "body": body});
        let mut asked = Exchange::new(
            "outgoing",
            request.clone(),
            "obi-sam".into(),
            Some("sam".into()),
        );
        asked.state = "sent".into();
        asked.created = 1000;
        asked.messages.push(Message {
            direction: "sent".into(),
            seq: Some(1),
            at: "2026-11-03T12:00:00Z".into(),
            envelope: request,
        });
        asked
    }

    /// PEER-1: each message's words, and what it says of itself, whatever its type.
    #[test]
    fn peer_1_each_message_says_its_words_and_no_more() {
        for (kind, body, text, by) in [
            ("question", json!({"text": "Q"}), json!("Q"), "obi"),
            (
                "answer",
                json!({"by": "owner", "text": "A"}),
                json!("A"),
                "owner",
            ),
            (
                "answer",
                json!({"by": "obi", "text": "A"}),
                json!("A"),
                "obi",
            ),
            (
                "status",
                json!({"state": "working", "note": "N"}),
                json!("N"),
                "obi",
            ),
            ("decline", json!({"reason": "busy"}), Value::Null, "obi"),
            (
                "decline",
                json!({"reason": "owner_declined", "note": "N"}),
                json!("N"),
                "owner",
            ),
            (
                "review_request",
                json!({"goal": "G", "summary": "S"}),
                json!("G"),
                "obi",
            ),
            ("handoff", json!({"goal": "G"}), json!("G"), "obi"),
            (
                "result",
                json!({"summary": "S", "findings": ["F"]}),
                json!("S"),
                "obi",
            ),
            ("accept", json!({}), Value::Null, "obi"),
            (
                "share_offer",
                json!({"text": "not a known type's words"}),
                Value::Null,
                "obi",
            ),
        ] {
            let mut e = asked("question", json!({"text": "Q"}));
            e.messages.push(message("received", kind, body));
            let posts = e.posts("me", false);
            assert_eq!(posts[1]["text"], text, "{kind}");
            assert_eq!(posts[1]["by"], by, "{kind}");
            assert_eq!(posts[0]["verified"], true, "this Oby's own");
            assert_eq!(posts[1]["verified"], false, "not compared");
            assert_eq!(e.posts("me", true)[1]["verified"], true, "compared");
        }
    }

    /// PEER-1: where the exchange stands: when a question counts as unanswered, unless it's said why.
    #[test]
    fn peer_1_a_question_says_when_it_counts_as_unanswered() {
        let quiet = |e: &Exchange| e.posts("me", false)[0]["exchange"]["quietAt"].clone();
        let mut e = asked("question", json!({"text": "Q"}));
        assert_eq!(quiet(&e), json!(obi_protocol::time::format(1000 + QUIET)));
        e.messages
            .push(message("received", "status", json!({"state": "working"})));
        assert!(quiet(&e).is_string(), "a word that says nothing of when");
        e.messages.push(message(
            "received",
            "status",
            json!({"state": "waiting_on_owner"}),
        ));
        assert_eq!(quiet(&e), Value::Null);
        assert_eq!(e.posts("me", false)[0]["exchange"]["waitingOnThem"], true);
        e.state = "done".into();
        assert_eq!(
            e.posts("me", false)[0]["exchange"]["waitingOnThem"],
            false,
            "it ended"
        );
        for state in ["done", "declined", "expired"] {
            let mut ended = asked("question", json!({"text": "Q"}));
            ended.state = state.into();
            assert_eq!(quiet(&ended), Value::Null, "{state}");
        }
        assert_eq!(
            quiet(&asked("review_request", json!({"goal": "G"}))),
            Value::Null
        );
    }

    /// PEER-1: a request the Sphere hasn't confirmed is still in the thread, in obid's words, and what
    /// obid keeps for itself isn't.
    #[test]
    fn peer_1_a_request_not_confirmed_shows_as_waiting() {
        let mut e = asked("review_request", json!({"goal": "G"}));
        e.messages.clear();
        assert_eq!(e.posts("me", false), Vec::<Value>::new(), "being sent");
        e.error = Some("Not confirmed yet: what a Sphere said".into());
        e.asked_in = Some("conversation-id".into());
        e.outbox
            .push(json!({"id": "out", "body": {"text": "in the outbox"}}));
        let posts = e.posts("me", false);
        assert_eq!(posts.len(), 1, "what isn't sent yet isn't listed");
        assert_eq!(posts[0]["state"], "waiting");
        assert_eq!(posts[0]["seq"], Value::Null);
        assert_eq!(posts[0]["text"], "G");
        let shown = posts[0].to_string();
        assert!(!shown.contains("what a Sphere said"), "{shown}");
        assert!(!shown.contains("conversation-id"), "{shown}");
    }
}
