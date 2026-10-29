//! obid's link to its Sphere: membership and keys, the Obi-to-Obi client, exchanges, the
//! `obiRequest` inbox item, remembered decisions, and the reply to a review another Obi asked for
//! with its publish gate (docs/runner-api.md, "obid only"; spec `protocol.md`, `identity.md`,
//! `sphere-api.md`).
//!
//! What crosses in from another Obi is a signed, typed action, checked against the spec before
//! anything acts on it (protocol.md section 10). It may become only what section 11 allows: an inbox
//! item, or work a remembered decision covers, and then only the typed reply to that action. Its
//! text is data: it never reaches a prompt, an inbox item's title or text, or a log line.
//!
//! Three tasks run once obid has joined: the follower (deliveries, long-polled), the sender (each
//! exchange's replies in order, again after a failure) and the clock (requests that expire).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use obi_protocol::client::{Client, ClientError};
use obi_protocol::keys::Keypair;
use obi_protocol::seal::SealKey;
use obi_protocol::{DirectoryKey, Keys, time};
use serde_json::{Map, Value, json};
use tokio::sync::{Notify, watch};

use super::answering;
use super::approval;
use super::decisions::{self, Decision, Decisions};
use super::exchange::{Exchange, Exchanges, Message};
use super::inbox::{self, Inbox, Item};
use super::membership::{Files, Membership};
use super::sharing::{self, Rule, Sharing};
use crate::api::ApiError;
use crate::config::Config;
use crate::events::Events;
use crate::job::Job;
use crate::jobs::{Jobs, ObiReview};
use crate::sandbox::Sandbox;
use crate::scan;
use obi_protocol::identity::takes_card_runtime;

mod follower;
mod inbox_actions;
mod reviews;
mod sender;

/// How long a reply obid sends lives, at least (runner-api.md, "Expiry of what obid sends").
const REPLY_LIFETIME: i64 = 3_600;
/// A new token when the one held has less than this left.
const TOKEN_MARGIN: i64 = 60;
/// How long the Sphere may hold a deliveries call.
const POLL_WAIT: u32 = 25;
/// How often the directory is read again while following.
const DIRECTORY_EVERY: i64 = 60;
/// The longest a reply waits to be sent again after a failure.
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// What obid's card says it is: from 0.2.0, an obid that reads a card with a seal key (#1233); from
/// 0.3.0, one that takes `card_runtime` too, a card's `features`, `runtime` and `vendors` (spec
/// identity.md section 2.3), which it says the spec's way, with `+card-runtime` as build metadata.
/// One from before refuses such a card (its schema closes `params`), so obid puts them in a card
/// only once every other member's card says it reads them ([`reads_seal_keys`],
/// [`obi_protocol::identity::takes_card_runtime`]).
const OBID_VERSION: &str = "0.3.0+card-runtime";

/// The runtime obid's card declares (identity.md section 2.3): Oby, at the version its card says,
/// without the build metadata.
const RUNTIME_ID: &str = "oby";
const RUNTIME_VERSION: &str = "0.3.0";
/// The additions obid takes, as its card lists them once it may (identity.md section 2.3): it reads
/// a card's runtime, vendors, classes and topics, verifies a chain with follow-ups, cancels and
/// introductions, reads a policy's class rules, and stops what a received request's cancel calls
/// off. Its card accepts no introduction, says no topics, and claims no conformance class until a
/// report backs one.
const FEATURES: [&str; 7] = [
    "cancel",
    "card_classes",
    "card_runtime",
    "card_topics",
    "follow_up",
    "introduction",
    "policy_classes",
];
/// How often obid reads the Sphere's `GET /v0.1/features` again: for a token exchange, and, once
/// its team takes `card_runtime`, for a Sphere that didn't list it (at once when the team first does).
const FEATURES_EVERY: i64 = 600;
/// How long obid waits for the Sphere's `GET /v0.1/features`; no answer by then counts as none.
const FEATURES_WAIT: Duration = Duration::from_secs(10);

/// Whether obid is still a member through `client`'s server: what it learnt of one server says
/// nothing of the next.
fn same_server(state: &State, client: &Client) -> bool {
    state
        .client
        .as_ref()
        .is_some_and(|c| c.base() == client.base())
}

/// The model vendors obid's card declares (identity.md section 2.3): those whose models write what
/// it sends another member. A review, and a question it answers by itself, run on Claude Code; an
/// answer the owner confirms may be drafted by the conversation's agent.
fn vendors(config: &Config) -> Vec<&'static str> {
    // The conversation's agent is Claude Code or Codex (configuration refuses Cursor there).
    if config.conversation.agent == crate::agent::Tool::Codex {
        vec!["anthropic", "openai"]
    } else {
        vec!["anthropic"]
    }
}

/// Whether a card's `version` is an obid's that reads a card with a seal key: 0.2.0 or later.
fn reads_seal_keys(version: &str) -> bool {
    let parts: Vec<u64> = version
        .split(['.', '-', '+'])
        .take(3)
        .map_while(|p| p.parse().ok())
        .collect();
    parts.len() == 3 && (parts[0], parts[1]) >= (0, 2)
}

/// The scopes a delegation must grant for obid to follow and answer.
const NEEDED_SCOPES: [&str; 2] = ["receive", "reply"];
/// The most model runs answering questions by themselves at once, across every sender.
const ANSWER_RUNS: usize = 2;
/// The most dropped message ids obid remembers, to acknowledge them again without a second look.
const DROPPED_MAX: usize = 10_000;
/// An open incoming request's `error` when obid's membership was revoked under it.
const REVOKED: &str = "obid's membership was revoked: settled here, and its peer wasn't told.";

/// One member of the directory, once its card passed the checks (identity.md 2.2).
#[derive(Debug, Clone)]
struct Member {
    owner: String,
    active: bool,
    accepts: Vec<String>,
    /// The key its words are sealed for, from its trusted card (protocol.md section 9.1), and that
    /// card's `issued_at`: what it sent before then may come in the clear.
    seal: Option<[u8; 32]>,
    sealed_since: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct Directory {
    members: HashMap<String, Member>,
    keys: Keys,
}

struct State {
    key: Option<Arc<Keypair>>,
    /// The seal key other Obis seal this Obi's words for (crate::sphere::membership).
    seal: Option<Arc<SealKey>>,
    /// Whether obid may put its seal key in a card (identity.md section 2): its Sphere's health says
    /// `seal`, and every other active member's card says an obid that reads one. From the last
    /// directory read.
    sphere_seals: bool,
    team_reads_seal_keys: bool,
    /// Whether obid may put `features`, `runtime` and `vendors` in a card (identity.md section 2.3):
    /// its Sphere's `GET /v0.1/features` lists `card_runtime`, and every other active member takes
    /// it. From the last directory read.
    sphere_takes_card_runtime: bool,
    team_takes_card_runtime: bool,
    /// When obid last asked the Sphere's `GET /v0.1/features`, while it didn't list `card_runtime`.
    features_read: i64,
    /// The Sphere's `GET /v0.1/features` and when obid read it (an answer that isn't the list counts
    /// as none): the token exchange and the card's runtime both read it from here.
    features: Option<(Instant, Vec<String>)>,
    /// The Sphere refused an actor assertion that named its URL, and took one naming it alone:
    /// obid names it alone from then on, while it is a member.
    url_aud_refused: bool,
    membership: Option<Membership>,
    delegation: Option<String>,
    client: Option<Client>,
    /// The access token and when it expires.
    token: Option<(String, i64)>,
    directory: Directory,
    directory_read: i64,
    last_poll: Option<i64>,
    last_error: Option<String>,
    exchanges: HashMap<String, Exchange>,
    inbox: HashMap<String, Item>,
    decisions: Vec<Decision>,
    /// The owner's sharing rules and the per-sender limits.
    sharing: Sharing,
    /// When each sender's owner's questions started an answer the Obi wrote by itself, within the
    /// last hour: the self-answer limit (protocol.md section 11, "Rate limits").
    self_answers: HashMap<String, Vec<i64>>,
    /// The same, every sender's together: the overall hourly cap.
    all_self_answers: Vec<i64>,
    /// Every envelope id obid has handled: the messages of its exchanges, and the ones it dropped.
    seen: HashSet<String>,
    dropped: Vec<String>,
    /// Requests the Council's convener sent whose post didn't answer (`504`): the envelope as
    /// signed, by its id, so that a repeated call posts the same bytes again (an Obysphere answers a
    /// byte-identical envelope with its first receipt, sphere-api.md section 6.1, check 8) rather
    /// than send the seat a second request. Kept while obid runs.
    unconfirmed: HashMap<String, Value>,
}

/// `sealing` in `GET /v1/obid/sphere`: `on` once the card carries obid's seal key; `card_needed`
/// when the owner should sign a new card (to say obid reads seal keys or a card's runtime, or to add
/// its own seal key or its runtime once every member reads them); `waiting` until the Sphere or a
/// teammate's obid can take one.
fn sealing(state: &State, vendors: &[&str]) -> &'static str {
    let version = state
        .membership
        .as_ref()
        .and_then(|m| m.card["version"].as_str())
        .unwrap_or("");
    if runtime_card_needed(state, vendors) {
        "card_needed"
    } else if !own_seal_keys(state).is_empty() {
        "on"
    } else if !reads_seal_keys(version) || (state.sphere_seals && state.team_reads_seal_keys) {
        "card_needed"
    } else {
        "waiting"
    }
}

/// Whether the card obid holds should be signed again for its runtime (identity.md section 2.3): it
/// doesn't say an obid that takes `card_runtime` yet, or it could carry `features`, `runtime` and
/// `vendors` now and doesn't.
fn runtime_card_needed(state: &State, vendors: &[&str]) -> bool {
    let Some(card) = state.membership.as_ref().map(|m| &m.card) else {
        return false;
    };
    let params = obi_protocol::card::params(card);
    // A card that carries them, saying another runtime or other vendors than obid would now (its
    // owner changed the conversation's agent, say), is signed again.
    let stale = carries_runtime(state)
        && params.is_some_and(|p| {
            p.get("runtime") != Some(&json!({"id": RUNTIME_ID, "version": RUNTIME_VERSION}))
                || p.get("vendors") != Some(&json!(vendors))
                || p.get("features") != Some(&json!(FEATURES))
        });
    !takes_card_runtime(card)
        || stale
        || (!carries_runtime(state)
            && state.sphere_takes_card_runtime
            && state.team_takes_card_runtime)
}

/// Whether the card obid holds lists `card_runtime`, and so carries its runtime and vendors.
fn carries_runtime(state: &State) -> bool {
    state
        .membership
        .as_ref()
        .and_then(|m| obi_protocol::card::params(&m.card))
        .is_some_and(|p| {
            obi_protocol::identity::lists_feature(p, obi_protocol::identity::CARD_RUNTIME)
        })
}

/// When this Obi's card first carried its seal key: its `issued_at`, while it carries it.
fn own_sealed_since(state: &State) -> Option<i64> {
    if own_seal_keys(state).is_empty() {
        return None;
    }
    let params = obi_protocol::card::params(&state.membership.as_ref()?.card)?;
    params.get("issued_at")?.as_str().and_then(time::parse)
}

/// This Obi's seal keys for check 11 (protocol.md section 9.1): its seal key once its card carries
/// it, none before (`sealing` is `card_needed`).
fn own_seal_keys(state: &State) -> HashMap<String, SealKey> {
    let carried = state
        .membership
        .as_ref()
        .and_then(|m| obi_protocol::card::params(&m.card))
        .and_then(obi_protocol::seal::card_key);
    match &state.seal {
        Some(seal) if carried.as_ref() == Some(seal.public()) => {
            [(seal.key_id(), SealKey::clone(seal))].into()
        }
        _ => HashMap::new(),
    }
}

/// The chain as this Obi knows it for itself (spec sphere-api.md 7.1, "No rollback, no fork"), in
/// `home/sphere/chain.json` (0600): the last head it verified, and the seq and hash the Sphere
/// answered for each of its own sends. A chain that is shorter than that head, has another hash at
/// it, or lacks one of these sends at its seq is a rollback or a fork.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct ChainKept {
    head: Option<(u64, String)>,
    /// By envelope id: (seq, hash).
    sends: std::collections::BTreeMap<String, (u64, String)>,
}

impl ChainKept {
    fn path(home: &std::path::Path) -> std::path::PathBuf {
        home.join("sphere").join("chain.json")
    }

    fn open(home: &std::path::Path) -> ChainKept {
        std::fs::read(Self::path(home))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, home: &std::path::Path) {
        let path = Self::path(home);
        let saved = path
            .parent()
            .map_or(Ok(()), crate::store::make_private_dir)
            .and_then(|()| {
                crate::store::write_private(&path, &serde_json::to_vec(self).unwrap_or_default())
            });
        if let Err(e) = saved {
            tracing::error!("can't keep what obid saw of the chain: {e}");
        }
    }

    /// What's wrong with `entries` (the whole chain, from seq 1) against what this Obi saw: a
    /// `rollback`, a `fork`, or nothing.
    fn problem(&self, entries: &[Value]) -> Option<&'static str> {
        let at = |seq: u64| entries.get((seq as usize).wrapping_sub(1));
        if let Some((seq, hash)) = &self.head {
            match at(*seq) {
                None => return Some("rollback"),
                Some(e) if e["hash"] != hash.as_str() => return Some("fork"),
                _ => {}
            }
        }
        for (id, (seq, hash)) in &self.sends {
            let ok = at(*seq)
                .is_some_and(|e| e["hash"] == hash.as_str() && e["envelope"]["id"] == id.as_str());
            if !ok {
                return Some("fork");
            }
        }
        None
    }
}

impl Link {
    /// Keeps the seq and hash the Sphere answered for a send of this Obi's, under the id of the
    /// envelope sent, `id`: never under one the Sphere named (Client::send refuses a receipt that
    /// names another envelope).
    fn keep_receipt(&self, id: &str, sent: &obi_protocol::client::Sent) {
        let mut kept = self.chain.lock().unwrap();
        kept.sends
            .insert(id.to_owned(), (sent.seq, sent.entry_hash.clone()));
        kept.save(&self.config.home);
    }
}

/// An audit entry as typed fields only: never a text the message carries.
fn audit_entry(entry: &Value) -> Value {
    let e = &entry["envelope"];
    let b = &e["body"];
    let mut out = json!({"seq": entry["seq"], "recordedAt": entry["recorded_at"], "type": e["type"], "from": e["from"],
                         "to": e["to"], "correlationId": e["correlation_id"]});
    for (key, value) in [
        ("repo", &b["repo"]),
        ("reason", &b["reason"]),
        ("state", &b["state"]),
        ("outcome", &b["outcome"]),
        ("costCents", &b["cost_cents"]),
        (
            "grant",
            if b["grant"].is_null() {
                &b["account"]["grant"]
            } else {
                &b["grant"]
            },
        ),
        ("share", &b["share"]),
        ("usedCents", &b["used_cents"]),
        ("limitCents", &b["limit_cents"]),
    ] {
        if !value.is_null() {
            out[key] = value.clone();
        }
    }
    out
}

/// The names the Sphere's secrets are known by in `crate::secrets`.
const OBI_KEY_SECRET: &str = "obi-key";
const SEAL_KEY_SECRET: &str = "seal-key";
const DELEGATION_SECRET: &str = "sphere-delegation";
const SPHERE_TOKEN_SECRET: &str = "sphere-token";

/// The Obi key, as obid's log layer takes it out: its hex text (`keys/obi.key`) and its 32
/// bytes, each in every common encoding (crate::secrets).
fn learn_key(key: &Keypair) {
    let seed = key.seed();
    let text = hex::encode(seed);
    crate::secrets::known().learn_forms(OBI_KEY_SECRET, &[text.as_bytes(), &seed]);
}

/// The seal key's private half, taken out the same way: its hex text (`keys/seal.key`) and bytes.
fn learn_seal_key(key: &SealKey) {
    let private = key.private_bytes();
    let text = hex::encode(private);
    crate::secrets::known().learn_forms(SEAL_KEY_SECRET, &[text.as_bytes(), private]);
}

/// What obid keeps of a sealed envelope it sent or opened (docs/runner-api.md, `messages`): the
/// envelope as signed with its words opened in `body`, and the body as signed in `sealed`. So
/// everything that reads a message's words reads them as before.
fn kept(mut signed: Value, opened: Value) -> Value {
    let sealed = std::mem::replace(&mut signed["body"], opened);
    signed["sealed"] = sealed;
    signed
}

/// An envelope obid keeps ([`kept`]), as its sender signed it.
fn wire(kept: &Value) -> Value {
    let mut wire = kept.clone();
    if let Some(sealed) = wire.as_object_mut().and_then(|m| m.remove("sealed")) {
        wire["body"] = sealed;
    }
    wire
}

/// The reply `make` builds from `body`, its words shortened until it fits the most a message may be
/// (protocol.md section 3; sealed, words are about a third longer): every summary, finding title
/// and finding detail longer than 100 characters halved, as often as it takes. A review's result that still doesn't
/// fit goes `failed`, with no text; any other reply goes as it is, and the Sphere says why it can't.
fn fitted(
    mut body: Value,
    make: impl Fn(&Value) -> Result<Value, String>,
) -> Result<Value, String> {
    loop {
        let envelope = make(&body)?;
        let size = obi_protocol::jcs::canonical_bytes(&wire(&envelope))
            .map_err(|e| e.to_string())?
            .len();
        if size <= obi_protocol::MAX_ENVELOPE_BYTES {
            return Ok(envelope);
        }
        if halve_texts(&mut body) {
            continue;
        }
        if body["for"] != "review_request" {
            return Ok(envelope);
        }
        let mut failed = json!({"for": "review_request", "outcome": "failed", "repo": body["repo"],
                                "head_sha": body["head_sha"], "findings": []});
        if let Some(rule) = body.get("by_rule") {
            failed["by_rule"] = rule.clone();
        }
        return make(&failed);
    }
}

/// Halves every summary, finding title and finding detail of `body` longer than 100 characters:
/// whether one was.
fn halve_texts(body: &mut Value) -> bool {
    let mut halved = false;
    let mut halve = |text: &mut Value| {
        let Some(words) = text.as_str() else { return };
        let count = words.chars().count();
        if count > 100 {
            *text = json!(format!(
                "{}…",
                words.chars().take(count / 2).collect::<String>()
            ));
            halved = true;
        }
    };
    if let Some(summary) = body.get_mut("summary") {
        halve(summary);
    }
    let findings = body.get_mut("findings").and_then(Value::as_array_mut);
    for finding in findings.into_iter().flatten() {
        for text in ["title", "detail"] {
            if let Some(text) = finding.get_mut(text) {
                halve(text);
            }
        }
    }
    halved
}

/// The link. One per obid; one Sphere per obid in this version.
pub struct Link {
    config: Config,
    files: Files,
    exchanges: Exchanges,
    inbox: Inbox,
    decisions: Decisions,
    sharing: sharing::Kept,
    /// The owner keys this Obi trusts, pinned the first time it saw each (crate::sphere::pins).
    pins: super::pins::Pins,
    /// What the quick model said of the inbox's items, and when it was asked (crate::triage).
    pub(crate) triage: crate::triage::Kept,
    /// What the model that answers a question by itself runs in: the job sandbox.
    sandbox: Result<Sandbox, String>,
    /// At most this many of those runs at once, whoever asked.
    answering: tokio::sync::Semaphore,
    jobs: Arc<Jobs>,
    events: Events,
    state: Mutex<State>,
    /// Woken when a reply waits to be sent.
    outbox: Notify,
    /// One send at a time, so each exchange's replies go in order.
    sending: tokio::sync::Mutex<()>,
    /// One save of an item or exchange at a time, so what reaches the disk and the apps never goes
    /// back to an older copy.
    saving: Mutex<()>,
    /// One token exchange at a time.
    tokening: tokio::sync::Mutex<()>,
    /// One key, card or join call at a time.
    joining: tokio::sync::Mutex<()>,
    /// Why a key file that's there couldn't be read when obid started: no key is made over it.
    unread_key: Option<String>,
    /// The chain as this Obi knows it for itself: no rollback, no fork (crate::sphere::link::ChainKept).
    chain: Mutex<ChainKept>,
    following: AtomicBool,
    /// Whether the sender and the clock run: from obid's first membership on (the sender sends
    /// only while it's `joined`).
    sender_running: AtomicBool,
    stopping: watch::Receiver<bool>,
    /// The phone relay (crate::relay): told at once when this Obi's membership is revoked.
    relay: std::sync::OnceLock<Arc<crate::relay::Relay>>,
    /// Who obid acts for in its Sphere ([`Link::owner`]).
    owner: Mutex<String>,
}

fn api(status: StatusCode, message: impl Into<String>) -> ApiError {
    ApiError::new(status, message)
}

/// A client error as the API answers it: `502` with the Sphere's code and message, `504` when it
/// couldn't be reached.
fn from_client(error: &ClientError) -> ApiError {
    match error {
        ClientError::Unreachable(_) => api(StatusCode::GATEWAY_TIMEOUT, error.to_string()),
        _ => api(StatusCode::BAD_GATEWAY, error.to_string()),
    }
}

/// What the app needs of the delegation obid holds to sign the next one before it ends (#1215): its
/// scope and when it ends. Never the JWT itself.
fn delegation_json(delegation: &str) -> Value {
    let claims = obi_protocol::jwt::parse(delegation).map(|p| p.claims);
    let claims = claims.as_ref();
    json!({
        "scope": claims.and_then(|c| c["scope"].as_str()),
        "expiresAt": claims.and_then(|c| c["exp"].as_i64()).map(time::format),
    })
}

/// When a request obid sends expires: a lifetime from the moment it's built, or an instant.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Expiry {
    In(i64),
    At(i64),
}

fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Resolves once obid is stopping.
pub(super) async fn stopped(mut stopping: watch::Receiver<bool>) {
    while !*stopping.borrow() {
        if stopping.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// A Sphere's base URL: `https://`, a host and maybe a port, nothing after it. Plain `http://`
/// only on this machine (a local Sphere, the e2e harness's), so the delegation and every token
/// cross the network encrypted.
fn is_base_url(url: &str) -> bool {
    let (rest, plain) = match url.strip_prefix("https://") {
        Some(rest) => (rest, false),
        None => match url.strip_prefix("http://") {
            Some(rest) => (rest, true),
            None => return false,
        },
    };
    let valid = !rest.is_empty()
        && rest.len() <= 255
        && rest
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'));
    valid && (!plain || is_loopback(rest))
}

/// `host[:port]` names this machine: `localhost`, `127.x.x.x` or `[::1]`.
fn is_loopback(authority: &str) -> bool {
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.split(':').next().unwrap_or(""),
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// What `POST /v1/obid/sphere/join` takes as `invitation`.
const INVITATION_SHAPE: &str = "invitation is {\"id\", \"secret\"}, and {\"signin\": {\"state\", \"code\"}} for one limited to a domain.";

/// An id the Sphere makes (an invitation id) or a secret: what may go in a path or a body.
fn is_token_text(text: &str) -> bool {
    (1..=256).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
}

fn is_slug(text: &str) -> bool {
    obi_protocol::schema::is_valid("common#/$defs/slug", &json!(text))
}

/// Reads exactly the keys `allowed` from a JSON object body: `422` for anything else.
pub(super) fn object(body: &[u8], allowed: &[&str]) -> Result<Map<String, Value>, ApiError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|e| api(StatusCode::BAD_REQUEST, format!("The body isn't JSON: {e}")))?;
    let Value::Object(map) = value else {
        return Err(api(
            StatusCode::UNPROCESSABLE_ENTITY,
            "The body is a JSON object.",
        ));
    };
    if let Some(unknown) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(api(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{unknown} isn't a field this endpoint takes."),
        ));
    }
    Ok(map)
}

fn text_field<'a>(map: &'a Map<String, Value>, key: &str) -> Result<&'a str, ApiError> {
    map.get(key).and_then(Value::as_str).ok_or_else(|| {
        api(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{key} is a string, and it's required."),
        )
    })
}

/// A later time as the inbox takes it: a protocol timestamp, with or without fractional seconds.
fn parse_later(text: &str) -> Option<i64> {
    if let Some(t) = time::parse(text) {
        return Some(t);
    }
    // 2026-10-10T10:12:00.000Z
    let (whole, rest) = text.split_once('.')?;
    let digits = rest.strip_suffix('Z')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    time::parse(&format!("{whole}Z"))
}

/// What needs the owner first (crate::triage, with what waits for the model), then FYI, then
/// ignore; each in the order they came, also within one second (Item::arrival_order).
fn needs_you_first(items: &mut [impl std::borrow::Borrow<Item>], sorts: &crate::triage::Sorts) {
    items.sort_by_cached_key(|i| {
        let i: &Item = i.borrow();
        let rank = crate::triage::rank(sorts.get(i));
        (rank, i.arrival_order(), i.id.clone())
    });
}

impl Link {
    /// Opens what obid keeps of its Sphere, its exchanges, inbox and decisions. A key that can't
    /// be read is left out and said (the log, `lastError`), never a reason not to start: the Sphere
    /// is optional. No new key is made over it ([`Link::make_key`]): that would lose the Obi's
    /// identity and its paired phones.
    pub fn open(
        config: Config,
        jobs: Arc<Jobs>,
        events: Events,
        stopping: watch::Receiver<bool>,
    ) -> Arc<Link> {
        let files = Files::new(&config.home);
        // What compute sharing kept in obid's home (the guest image, guest jobs' disks, its log)
        // went with it, once.
        for folder in ["vm", "g"] {
            let _ = std::fs::remove_dir_all(config.home.join(folder));
        }
        let _ = std::fs::remove_file(config.home.join("sharing-log.jsonl"));
        let mut unread = None;
        let key = match files.load_key() {
            Ok(key) => key.map(Arc::new),
            Err(e) => {
                unread = Some(format!("obid can't read its Obi key: {e}"));
                None
            }
        };
        let seal = match files.load_seal_key() {
            Ok(seal) => seal.map(Arc::new),
            Err(e) => {
                unread.get_or_insert(format!("obid can't read its seal key: {e}"));
                None
            }
        };
        if let Some(why) = &unread {
            tracing::error!("sphere: {why}");
        }
        let membership = files.load_membership();
        let delegation = files.load_delegation();
        // What obid's log layer takes out of plug-ins' lines (crate::logs).
        let secrets = crate::secrets::known();
        if let Some(key) = &key {
            learn_key(key);
        }
        if let Some(seal) = &seal {
            learn_seal_key(seal);
        }
        if let Some(delegation) = &delegation {
            secrets.learn(DELEGATION_SECRET, delegation);
        }
        let exchanges = Exchanges::open(&config.home);
        let inbox = Inbox::open(&config.home);
        let decisions = Decisions::open(&config.home);
        let sharing = sharing::Kept::open(&config.home);
        let pins = super::pins::Pins::open(&config.home);
        let triage = crate::triage::Kept::open(&config.home);
        let chain = ChainKept::open(&config.home);
        let sandbox = std::env::current_exe()
            .map_err(|e| format!("obid can't find its own executable: {e}"))
            .and_then(|obid| Sandbox::detect(&obid));
        let mut seen = HashSet::new();
        let mut by_id = HashMap::new();
        for exchange in exchanges.load_all() {
            seen.extend(exchange.envelope_ids().map(str::to_owned));
            by_id.insert(exchange.correlation_id.clone(), exchange);
        }
        let now = time::now();
        let items = inbox
            .load_all()
            .into_iter()
            .map(|mut i| {
                // What its job still shows waits on: a restart kept the agent's question (#858),
                // or the call obid parked the job for (crate::jobs, `Jobs::park`). A job shows
                // no other card once it's open.
                let kept = jobs
                    .get(&i.job)
                    .and_then(|j| j.permission)
                    .is_some_and(|p| inbox::permission_item_id(&i.job, &p.id) == i.id);
                // A permission waited on an agent that ended with obid: its job's agent asks again
                // when it runs, so the old item is done by itself.
                // An older obid's offer about Oby's own pull request (`Address it`, `Fix CI` on
                // its red head): nothing starts those any more, so it's done by itself too; the
                // follower asks the owner about that pull request instead (crate::merge). So is a
                // plug-in's event: obid has no plug-ins any more, and its offers start nothing.
                let own_pr_offer = i.kind == "github"
                    && i.github
                        .as_deref()
                        .is_some_and(|g| g["list"] == "comments" || g["list"] == "checks");
                if (matches!(
                    i.kind.as_str(),
                    "permission" | "plan" | inbox::QUESTION_KIND
                ) && !kept
                    || own_pr_offer
                    || i.kind == "pluginEvent"
                    // A teammate's job for this Mac, or a share they used: compute sharing is
                    // gone, and nothing answers those any more.
                    || i.obi.as_ref().is_some_and(|o| {
                        o["type"] == "run_request" || o["type"] == "share_used"
                    }))
                    && i.done.is_none()
                {
                    i.done = Some(now);
                    if let Err(e) = inbox.save(&i) {
                        tracing::error!("can't keep inbox item {}: {e}", i.id);
                    }
                }
                (i.id.clone(), i)
            })
            .collect();
        let client = membership
            .as_ref()
            .and_then(|m| Client::new(&m.url, &m.sphere).ok());
        // The owner it joined as (an Oby that joined before `home/owner` existed keeps it now), else
        // the one it joined a Sphere as before it left, else config's.
        let joined_as = membership
            .as_ref()
            .and_then(|m| obi_protocol::card::params(&m.card)?.get("owner")?.as_str())
            .map(str::to_owned);
        if let Some(owner) = &joined_as
            && files.load_owner().is_none()
            && let Err(e) = files.save_owner(owner)
        {
            tracing::error!("can't keep the owner obid joined as: {e}");
        }
        let owner = joined_as
            .or_else(|| files.load_owner())
            .unwrap_or_else(|| config.owner.clone());
        let state = State {
            key,
            seal,
            sphere_seals: false,
            team_reads_seal_keys: false,
            sphere_takes_card_runtime: false,
            team_takes_card_runtime: false,
            features: None,
            url_aud_refused: false,
            features_read: 0,
            membership,
            delegation,
            client,
            token: None,
            directory: Directory::default(),
            directory_read: 0,
            last_poll: None,
            last_error: unread.clone(),
            exchanges: by_id,
            inbox: items,
            decisions: decisions.load(),
            sharing: sharing.load(),
            self_answers: HashMap::new(),
            all_self_answers: Vec::new(),
            seen,
            dropped: Vec::new(),
            unconfirmed: HashMap::new(),
        };
        let link = Arc::new(Link {
            config,
            files,
            exchanges,
            inbox,
            decisions,
            sharing,
            pins,
            triage,
            sandbox,
            answering: tokio::sync::Semaphore::new(ANSWER_RUNS),
            jobs,
            events,
            state: Mutex::new(state),
            outbox: Notify::new(),
            sending: tokio::sync::Mutex::new(()),
            saving: Mutex::new(()),
            tokening: tokio::sync::Mutex::new(()),
            joining: tokio::sync::Mutex::new(()),
            unread_key: unread,
            chain: Mutex::new(chain),
            following: AtomicBool::new(false),
            sender_running: AtomicBool::new(false),
            stopping,
            relay: std::sync::OnceLock::new(),
            owner: Mutex::new(owner),
        });
        // The inbox hears of the permissions jobs wait on. Jobs tells it with none of its own
        // locks held, and never while this Link's state is locked (only `start_obi_review` is
        // called under it, and an Obi-to-Obi review never asks the owner).
        link.jobs.set_link(Arc::downgrade(&link));
        let hook = Arc::downgrade(&link);
        link.jobs.on_permission(Arc::new(move |note| {
            if let Some(link) = hook.upgrade() {
                link.permission_note(note);
            }
        }));
        let hook = Arc::downgrade(&link);
        link.jobs.on_confirm(Arc::new(move |item| {
            if let Some(link) = hook.upgrade() {
                link.add_confirm_once(item);
            }
        }));
        let hook = Arc::downgrade(&link);
        link.jobs.on_review_end(Arc::new(move |job, done| {
            let link = hook.upgrade();
            Box::pin(async move {
                if let Some(link) = link {
                    link.review_ended(job, done).await;
                }
            })
        }));
        link
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Starts following the Sphere when obid is in one, and the clock (with the sender, which sends
    /// only while joined) once it has a membership. A membership revoked before this start settles
    /// what's still open, as at the revoke.
    pub fn start(self: &Arc<Self>) {
        let membership = self.lock().membership.as_ref().map(|m| m.state.clone());
        if membership.as_deref().is_some_and(|s| s != "joined") {
            self.settle_on_revoke();
        }
        self.reconcile();
        if membership.as_deref() == Some("joined") {
            self.follow();
        }
        if membership.is_some() {
            self.start_sender();
        }
    }

    /// Whether obid is in a Sphere now.
    fn is_joined(&self) -> bool {
        self.lock()
            .membership
            .as_ref()
            .is_some_and(|m| m.state == "joined")
    }

    /// After a restart: an accepted request whose job has ended without a reply gets its failed
    /// result, and one whose job is gone too. A question the Obi was answering by itself goes to
    /// the owner: its run ended with obid.
    fn reconcile(self: &Arc<Self>) {
        // What a run cut short left: its folder, with a copied login in it.
        let _ = std::fs::remove_dir_all(self.config.home.join("answers"));
        {
            // The self-answer limits count the last hour across restarts: every run obid took
            // on, whatever it ended in (an answer, the owner's inbox, `busy`).
            let mut state = self.lock();
            let since = time::now() - 3600;
            let started: Vec<(String, i64)> = state
                .exchanges
                .values()
                .filter_map(|e| {
                    e.self_answered
                        .filter(|at| *at > since)
                        .map(|at| (e.peer_owner.clone().unwrap_or_default(), at))
                })
                .collect();
            for (owner, at) in started {
                state.self_answers.entry(owner).or_default().push(at);
                state.all_self_answers.push(at);
            }
        }
        let answering: Vec<String> = self
            .lock()
            .exchanges
            .values()
            .filter(|e| e.direction == "incoming" && e.state == "answering")
            .map(|e| e.correlation_id.clone())
            .collect();
        for cid in answering {
            self.to_owner(&cid);
        }
        let waiting: Vec<(String, Option<String>)> = self
            .lock()
            .exchanges
            .values()
            .filter(|e| e.direction == "incoming" && e.state == "accepted" && !e.ended())
            .map(|e| (e.correlation_id.clone(), e.job.clone()))
            .collect();
        for (cid, job) in waiting {
            let ended = match job.as_deref().and_then(|id| self.jobs.get(id)) {
                Some(job) => !job.status.is_active(),
                None => true,
            };
            if ended {
                let link = self.clone();
                tokio::spawn(async move { link.reply_failed(&cid, None).await });
            }
        }
    }

    /// A permission waits for the owner: its item, which settles any other open item of the job's
    /// (one open item per job). Or one stopped waiting: its item is done, with the owner's answer
    /// or none.
    fn permission_note(&self, note: crate::jobs::PermissionNote) {
        use crate::jobs::PermissionNote;
        let now = time::now();
        let mut changed = Vec::new();
        {
            let mut state = self.lock();
            match note {
                PermissionNote::Waiting(job, permission) => {
                    let item = inbox::permission_item(&job, &permission, now);
                    for other in state.inbox.values_mut() {
                        if other.job == job.id && other.done.is_none() && other.id != item.id {
                            other.done = Some(now);
                            changed.push(other.clone());
                        }
                    }
                    if !state.inbox.contains_key(&item.id) {
                        state.inbox.insert(item.id.clone(), item.clone());
                        changed.push(item);
                    }
                }
                PermissionNote::Settled {
                    job,
                    permission,
                    answer,
                } => {
                    let id = inbox::permission_item_id(&job, &permission);
                    if let Some(item) = state.inbox.get_mut(&id).filter(|i| i.done.is_none()) {
                        item.done = Some(now);
                        item.answer = answer;
                        changed.push(item.clone());
                    }
                }
            }
        }
        for item in &changed {
            self.save_item(item);
        }
    }

    pub fn use_relay(&self, relay: Arc<crate::relay::Relay>) {
        let _ = self.relay.set(relay);
    }

    /// What a read of the owner's GitHub work found (crate::github_work): a `github` item for each
    /// of `new` that never had one, open or done; and, with `live` (the inbox ids of every offer
    /// the feed holds now, when no list was cut), each open `github` item that isn't one any more
    /// (its work left the feed, or its pull request's head moved or isn't red now) is done by
    /// itself, with no answer.
    pub fn github_work(
        &self,
        new: &[crate::github_work::Item],
        live: Option<&std::collections::HashSet<String>>,
    ) {
        let now = time::now();
        let known: Vec<bool> = new
            .iter()
            .map(|work| self.config.repos.find(&work.repo).is_some())
            .collect();
        let mut changed = Vec::new();
        {
            let mut state = self.lock();
            if let Some(live) = live {
                for item in state.inbox.values_mut() {
                    if item.kind == "github" && item.done.is_none() && !live.contains(&item.id) {
                        item.done = Some(now);
                        changed.push(item.clone());
                    }
                }
            }
            for (work, known) in new.iter().zip(known) {
                let item = inbox::github_item(work, known, now);
                if !state.inbox.contains_key(&item.id) {
                    state.inbox.insert(item.id.clone(), item.clone());
                    changed.push(item);
                }
            }
        }
        for item in &changed {
            self.save_item(item);
        }
    }

    /// Items obid made (the owner's reviews ready to post, crate::jobs): each one that never was in
    /// the inbox, open or done.
    pub fn add_items(&self, items: Vec<Item>) {
        let mut changed = Vec::new();
        {
            let mut state = self.lock();
            for item in items {
                if !state.inbox.contains_key(&item.id) {
                    state.inbox.insert(item.id.clone(), item.clone());
                    changed.push(item);
                }
            }
        }
        for item in &changed {
            self.save_item(item);
        }
    }

    /// The job `job`'s `limit` item now (#871): `item`, when there is one, added, and the job's other
    /// open ones done, with no answer. None when it runs again.
    pub fn limit_items(&self, job: &str, item: Option<Item>) {
        let stale: Vec<String> = self
            .lock()
            .inbox
            .values()
            .filter(|i| i.kind == "limit" && i.job == job && i.done.is_none())
            .map(|i| i.id.clone())
            .collect();
        for id in stale {
            self.settle(&id);
        }
        if let Some(item) = item {
            self.add_items(vec![item]);
        }
    }

    /// Whether a usage-limit decision for this job is still the owner's to answer.
    pub fn has_open_limit_item(&self, job: &str) -> bool {
        self.lock()
            .inbox
            .values()
            .any(|item| item.kind == "limit" && item.job == job && item.done.is_none())
    }

    /// The job `job`'s item about Oby's review that couldn't run now (`inbox::review_item`): `item`,
    /// when there is one, added, and the job's other open ones done, with no answer.
    pub fn review_items(&self, job: &str, item: Option<Item>) {
        let keep = item.as_ref().map(|i| i.id.clone());
        let stale: Vec<String> = self
            .lock()
            .inbox
            .values()
            .filter(|i| {
                inbox::is_review_item(i)
                    && i.job == job
                    && i.done.is_none()
                    && Some(&i.id) != keep.as_ref()
            })
            .map(|i| i.id.clone())
            .collect();
        for id in stale {
            self.settle(&id);
        }
        if let Some(item) = item {
            self.add_items(vec![item]);
        }
    }

    /// The job `job`'s merge card now (crate::merge): `card`, added unless the owner answered it
    /// before (they aren't asked again about that head and reason); one the follower settled by
    /// itself is offered again. Every other open `merge` item of the job is done, with no answer:
    /// what it asked about is gone, for now.
    pub fn merge_items(&self, job: &str, card: Option<Item>) {
        let keep = card.as_ref().map(|c| c.id.clone());
        let stale: Vec<String> = self
            .lock()
            .inbox
            .values()
            .filter(|i| {
                i.kind == "merge"
                    && i.job == job
                    && i.done.is_none()
                    && Some(&i.id) != keep.as_ref()
            })
            .map(|i| i.id.clone())
            .collect();
        for id in stale {
            self.settle(&id);
        }
        if let Some(card) = card {
            let settled = self
                .lock()
                .inbox
                .get(&card.id)
                .is_some_and(|i| i.done.is_some() && i.answer.is_none());
            if settled {
                self.lock().inbox.insert(card.id.clone(), card.clone());
                self.save_item(&card);
            } else {
                self.add_items(vec![card]);
            }
        }
    }

    /// `POST /v1/inbox/{id}/answer` on a `merge` card (crate::merge): Merge, or Merge and
    /// remember, merges it (`Jobs::merge_by_owner`), the item claimed (done) first so a second tap
    /// merges nothing, and open again when it didn't merge; Ask a teammate, and remember, saves
    /// the repository's workflow and asks a teammate's Oby (`Jobs::ask_teammate_and_remember`),
    /// claimed the same way, its outcome the answer's `note`; Leave it open, Not now, Dismiss or
    /// done settle it, and nothing is sent; later puts it off.
    async fn answer_merge(
        &self,
        mut item: Item,
        answer: Option<String>,
        later: Option<i64>,
    ) -> Result<Value, ApiError> {
        if let Some(at) = later {
            item.later = Some(at);
            self.lock().inbox.insert(item.id.clone(), item.clone());
            self.save_item(&item);
            return Ok(item.to_json());
        }
        let asking = answer.as_deref() == Some(crate::merge::ASK_TEAMMATE);
        let merging = matches!(
            answer.as_deref(),
            Some(crate::merge::MERGE | crate::merge::MERGE_AND_REMEMBER)
        );
        if let Some(answer) = &answer
            && !item.options.contains(answer)
        {
            // Unclaim: the answer isn't one of its options.
            if merging || asking {
                self.lock().inbox.insert(item.id.clone(), item.clone());
            }
            return Err(api(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("The answer is one of: {}.", item.options.join("; ")),
            ));
        }
        if merging || asking {
            let head = crate::merge::item_head(&item.id)
                .unwrap_or_default()
                .to_owned();
            let merged = if asking {
                // The owner's yes: remembered for the repository, then a teammate's Oby is asked.
                self.jobs
                    .ask_teammate_and_remember(&item.job, &head)
                    .await
                    .map(Some)
                    .map_err(|why| format!("Nothing was asked: {why}"))
            } else {
                let remember = answer.as_deref() == Some(crate::merge::MERGE_AND_REMEMBER);
                self.jobs
                    .merge_by_owner(&item.job, &head, remember)
                    .await
                    .map_err(|why| format!("Not merged: {why}"))
            };
            if let Err(why) = &merged {
                // Open again, unless it was settled meanwhile.
                let open = {
                    let mut state = self.lock();
                    let claimed = state
                        .inbox
                        .get(&item.id)
                        .is_some_and(|i| i.done.is_some() && i.answer.is_none());
                    if claimed {
                        state.inbox.insert(item.id.clone(), item.clone());
                    }
                    claimed
                };
                if open {
                    self.save_item(&item);
                }
                return Err(api(StatusCode::CONFLICT, why.clone()));
            }
            // Merged, or a teammate asked: done, whatever came after it (a workflow it couldn't
            // remember, or what came of the ask, is `note`).
            if let Ok(Some(note)) = merged {
                item.done = Some(time::now());
                item.answer = answer;
                self.lock().inbox.insert(item.id.clone(), item.clone());
                self.save_item(&item);
                let mut shown = item.to_json();
                shown["note"] = serde_json::json!(note);
                return Ok(shown);
            }
        }
        item.done = Some(time::now());
        // The owner's own done is the card's way of leaving it (Leave it open, Dismiss): an
        // answered card isn't offered again, as one the follower settled is.
        item.answer = answer.or_else(|| item.options.last().cloned());
        self.lock().inbox.insert(item.id.clone(), item.clone());
        self.save_item(&item);
        Ok(item.to_json())
    }

    /// Work started from the feed (`POST /v1/github/work/{id}/start`): its open `github` item `id`
    /// is done, answered with its start option, `job` the job on it.
    pub fn github_started(&self, id: &str, job: &str) {
        let item = {
            let mut state = self.lock();
            let Some(item) = state.inbox.get_mut(id).filter(|i| i.done.is_none()) else {
                return;
            };
            item.done = Some(time::now());
            item.answer = item.options.first().cloned();
            item.job = job.to_owned();
            item.clone()
        };
        self.save_item(&item);
    }

    /// `POST /v1/inbox/{id}/answer` on a `github` item: `Review it`, `Fix it` or `Fix CI` starts the
    /// work as `POST /v1/github/work/{id}/start` does; an error leaves the item open. `Dismiss` or
    /// done settle it, later puts it off. Nothing is sent to GitHub. A review in a repository obid
    /// doesn't work in yet waits for the owner's signed Confirm instead, which adds it (#776,
    /// #1118): an API token alone (an agent, oby-mcp) never widens what obid clones.
    async fn answer_github(
        &self,
        mut item: Item,
        answer: Option<String>,
        later: Option<i64>,
    ) -> Result<Value, ApiError> {
        if let Some(at) = later {
            item.later = Some(at);
        } else {
            if let Some(answer) = &answer {
                let start = item.options.first().cloned().unwrap_or_default();
                if *answer != start && answer != inbox::DISMISS {
                    return Err(api(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        format!("The answer is {start} or {}.", inbox::DISMISS),
                    ));
                }
                if *answer == start {
                    let work = item.github.as_deref().cloned().unwrap_or_default();
                    let list = work["list"].as_str().unwrap_or_default();
                    let repo = work["repo"].as_str().unwrap_or_default();
                    let number = work["number"].as_u64().unwrap_or_default();
                    if list == "reviews" && self.config.repos.find(repo).is_none() {
                        // `answer` claimed it: it stays open for the Confirm.
                        self.lock().inbox.insert(item.id.clone(), item.clone());
                        return Err(self.confirm_adding(&item.id, repo, number));
                    }
                    let started = crate::github_work::start_work(
                        &self.jobs,
                        &self.config,
                        list,
                        repo,
                        number,
                    )
                    .await;
                    match started {
                        Ok((job, _)) => item.job = job.id,
                        // `answer` claimed it; what couldn't start leaves it open again.
                        Err(why) => {
                            self.lock().inbox.insert(item.id.clone(), item.clone());
                            return Err(why);
                        }
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

    /// The `confirm` item that adds `repo` and starts Oby's review of its pull request `number`
    /// (#1118), for the GitHub offer `offer` (its inbox id, whether that item was made or not); the
    /// one still open for it stands. Answers the error a start asked for this way answers: only the
    /// owner's signed Confirm adds a repository (#776), from the feed and the inbox alike.
    pub fn confirm_adding(&self, offer: &str, repo: &str, number: u64) -> ApiError {
        let item = inbox::confirm_item(
            "",
            format!("Add {repo} and review #{number}"),
            format!(
                "Adds {repo} to the repositories Oby works in, with no check, then starts Oby's review of pull request #{number}. Nothing is posted on GitHub. It happens only if you confirm."
            ),
            inbox::Confirmed {
                action: inbox::action::ADD_REPOSITORY.into(),
                target: offer.into(),
                payload: json!({"repo": repo, "number": number}),
            },
            time::now(),
        );
        let added = {
            let mut state = self.lock();
            let open = state.inbox.values().any(|i| {
                i.kind == "confirm"
                    && i.done.is_none()
                    && i.confirm.as_ref().is_some_and(|c| {
                        c.action == inbox::action::ADD_REPOSITORY && c.target == offer
                    })
            });
            if !open {
                state.inbox.insert(item.id.clone(), item.clone());
            }
            !open
        };
        if added {
            self.save_item(&item);
        }
        api(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Adding {repo} waits for your Confirm in the inbox."),
        )
    }

    /// `POST /v1/inbox/{id}/answer` on a `pluginEvent` item: `Dismiss` or `{"done": true}` settles
    /// it, `later` puts it off. Nothing is sent anywhere.
    fn answer_plugin_event(
        &self,
        mut item: Item,
        answer: Option<String>,
        later: Option<i64>,
    ) -> Result<Value, ApiError> {
        if let Some(answer) = &answer
            && answer != inbox::DISMISS
        {
            let options = if item.kind == "pluginEvent" {
                item.options.join(" or ")
            } else {
                inbox::DISMISS.to_owned()
            };
            return Err(api(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("The answer is {options}."),
            ));
        }
        match later {
            Some(at) => item.later = Some(at),
            None => {
                item.done = Some(time::now());
                item.answer = answer;
            }
        }
        self.lock().inbox.insert(item.id.clone(), item.clone());
        self.save_item(&item);
        Ok(item.to_json())
    }

    /// `POST /v1/inbox/{id}/answer` on a `permission` item: the owner's answer goes to the job (an
    /// answer of their own words is a no, with those words for the agent); done or later only
    /// settles or puts off the item.
    fn answer_permission_item(
        &self,
        item: Item,
        answer: Option<String>,
        later: Option<i64>,
    ) -> Result<Value, ApiError> {
        let now = time::now();
        let Some(answer) = answer else {
            let mut item = item;
            match later {
                Some(at) => item.later = Some(at),
                None => item.done = Some(now),
            }
            // Done without an answer: a shell command waiting for it doesn't run.
            if item.done.is_some() && crate::brain::shell::is_item(&item) {
                crate::brain::shell::answered(&item.id, None);
            }
            self.lock().inbox.insert(item.id.clone(), item.clone());
            self.save_item(&item);
            return Ok(item.to_json());
        };
        // A command of a conversation's shell: the owner's answer goes to it, and only once.
        if crate::brain::shell::is_item(&item) {
            if answer == crate::permissions::ALWAYS_ALLOW
                && !item
                    .options
                    .iter()
                    .any(|o| o == crate::permissions::ALWAYS_ALLOW)
            {
                return Err(api(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "It has no rule to save: allow it once instead.",
                ));
            }
            if !crate::brain::shell::answered(&item.id, Some(answer.clone())) {
                self.settle(&item.id);
                return Err(api(
                    StatusCode::CONFLICT,
                    "Its command isn't waiting any more: its turn ended.",
                ));
            }
            let mut item = item;
            item.answer = Some(answer);
            item.done = Some(now);
            self.lock().inbox.insert(item.id.clone(), item.clone());
            self.save_item(&item);
            return Ok(item.to_json());
        }
        let permission = item
            .id
            .strip_prefix(&format!("{}-permission-", item.job))
            .unwrap_or("")
            .to_owned();
        use crate::jobs::PermissionRefused;
        // The agent's question: an option or the owner's own words are its answer (#858).
        let answered = if item.kind == inbox::QUESTION_KIND {
            self.jobs.answer_question(&item.job, &permission, &answer)
        } else {
            let (decision, words) = crate::permissions::inbox_decision(&answer);
            self.jobs
                .answer_permission(&item.job, &permission, decision, words.as_deref())
        };
        match answered {
            Ok(_) => {}
            Err(PermissionRefused::NoJob) => {
                return Err(api(StatusCode::CONFLICT, "Its job is gone."));
            }
            Err(PermissionRefused::NotWaiting(why)) => return Err(api(StatusCode::CONFLICT, why)),
            Err(PermissionRefused::Refused(why)) => {
                return Err(api(StatusCode::UNPROCESSABLE_ENTITY, why));
            }
        }
        let settled = self.lock().inbox.get(&item.id).cloned().unwrap_or(item);
        Ok(settled.to_json())
    }

    /// Keeps the exchange and tells the apps, as it is now: a caller's copy can be older than what
    /// another change has put there since (docs/runner-api.md, "GET /v1/events").
    fn save_exchange(&self, exchange: &Exchange) {
        let _saving = self.saving.lock().unwrap_or_else(|e| e.into_inner());
        let now = self.lock().exchanges.get(&exchange.correlation_id).cloned();
        let exchange = now.as_ref().unwrap_or(exchange);
        if let Err(e) = self.exchanges.save(exchange) {
            tracing::error!("can't keep exchange {}: {e}", exchange.correlation_id);
        }
        self.events.send("exchange", exchange.to_json());
    }

    /// Keeps the item and tells the apps, as it is now (as `save_exchange`).
    fn save_item(&self, item: &Item) {
        if let Err(e) = self.keep_item(item) {
            tracing::error!("can't keep inbox item {}: {e}", item.id);
        }
    }

    /// [`Link::save_item`], saying whether it was kept on disk.
    fn keep_item(&self, item: &Item) -> Result<(), String> {
        let _saving = self.saving.lock().unwrap_or_else(|e| e.into_inner());
        let now = self.lock().inbox.get(&item.id).cloned();
        let item = now.as_ref().unwrap_or(item);
        let kept = self.inbox.save(item).map_err(|e| e.to_string());
        self.events.send("inbox", self.shown(item));
        kept
    }

    /// The inbox's sorts as they are now (crate::triage), over `inbox`.
    fn sorts<'a>(&'a self, inbox: &'a HashMap<String, Item>) -> crate::triage::Sorts<'a> {
        crate::triage::Sorts::of(&self.triage, inbox.values(), time::now())
    }

    /// An item's sort now (crate::triage). Never with this Link's state locked.
    pub fn triage_of(&self, item: &Item) -> &'static str {
        let state = self.lock();
        self.sorts(&state.inbox).get(item)
    }

    /// An item as the API shows it: with its `triage`.
    pub fn shown(&self, item: &Item) -> Value {
        let mut shown = item.to_json();
        shown["triage"] = self.triage_of(item).into();
        shown
    }

    /// Open items waiting for the quick model (crate::triage), the longest-waiting first.
    pub fn unsorted(&self) -> Vec<Item> {
        let state = self.lock();
        let sorts = self.sorts(&state.inbox);
        let mut items: Vec<Item> = state
            .inbox
            .values()
            .filter(|i| sorts.waits(i))
            .cloned()
            .collect();
        items.sort_by_key(Item::arrival_order);
        items
    }

    /// The model's sorts (crate::triage::sort), kept for good: the apps hear each item again, now
    /// sorted. Nothing about the item changes but that.
    pub fn sorted(&self, sorts: &[(String, &str)]) {
        self.retell(&self.triage.keep(sorts));
    }

    /// The apps hear these items again, as they are now: their sort changed, nothing else.
    pub fn retell(&self, ids: &[String]) {
        let _saving = self.saving.lock().unwrap_or_else(|e| e.into_inner());
        for id in ids {
            if let Some(item) = self.item(id) {
                self.events.send("inbox", self.shown(&item));
            }
        }
    }

    fn save_sharing(&self, sharing: &Sharing) {
        if let Err(e) = self.sharing.save(sharing) {
            tracing::error!("can't keep the sharing rules: {e}");
        }
    }

    fn save_decisions(&self, decisions: &[Decision]) {
        if let Err(e) = self.decisions.save(decisions) {
            tracing::error!("can't keep the remembered decisions: {e}");
        }
    }

    fn note_error(&self, error: impl std::fmt::Display) {
        let line = error.to_string();
        tracing::warn!("sphere: {line}");
        self.lock().last_error = Some(line);
    }

    // -----------------------------------------------------------------------------------------
    // Membership: GET /v1/obid/sphere, POST …/key, …/card, …/join

    /// `GET /v1/obid/sphere`.
    pub fn membership_json(&self) -> Value {
        let state = self.lock();
        let key = state.key.as_ref();
        let joined = state.membership.as_ref();
        json!({
            "state": joined.map_or("none", |m| m.state.as_str()),
            "url": joined.map(|m| &m.url),
            "sphere": joined.map(|m| &m.sphere),
            "obiId": joined.map(|m| &m.obi_id),
            "owner": self.owner(),
            "keyId": key.map(|k| k.key_id()),
            "publicKey": key.map(|k| k.public_key()),
            "sealKeyId": state.seal.as_ref().map(|s| s.key_id()),
            "sealPublicKey": state.seal.as_ref().map(|s| s.public_key()),
            "sealing": joined.map(|_| sealing(&state, &vendors(&self.config))),
            "accepts": joined.map(Membership::accepts),
            "card": joined.map(|m| &m.card),
            "delegation": state.delegation.as_deref().map(delegation_json),
            "tokenExpiresAt": state.token.as_ref().map(|t| time::format(t.1)),
            "lastPoll": state.last_poll.map(time::format),
            "lastError": state.last_error,
        })
    }

    /// Whether obid keeps a membership, joined or revoked.
    pub fn has_membership(&self) -> bool {
        self.lock().membership.is_some()
    }

    /// `DELETE /v1/obid/sphere`: obid leaves its Sphere, joined or revoked, so it can join another
    /// or the same one again (#1214). The app revokes the Obi there first, when the Sphere answers:
    /// that takes the owner's own token, which obid never holds. What's open is settled as at a
    /// revoke, and `home/sphere` goes (the membership, the delegation, what obid saw of the chain,
    /// the owner keys it pinned), and the Obi key with it: a Sphere never takes a key twice, so the
    /// next join makes a new one. The exchanges and the inbox stay.
    pub async fn leave(&self) -> Result<Value, ApiError> {
        let _one = self.joining.lock().await;
        // No send under way, and nothing left to send: an exchange's waiting replies were for this
        // Sphere, never the next one's.
        let _sending = self.sending.lock().await;
        if self.has_membership() {
            self.settle_on_revoke();
        }
        let waiting: Vec<String> = self
            .lock()
            .exchanges
            .values()
            .filter(|e| !e.outbox.is_empty())
            .map(|e| e.correlation_id.clone())
            .collect();
        for cid in waiting {
            self.update_exchange(&cid, |e| e.outbox.clear());
        }
        self.files.forget().map_err(|e| {
            api(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("obid couldn't forget its Obysphere: {e}"),
            )
        })?;
        {
            let mut state = self.lock();
            state.key = None;
            state.seal = None;
            state.membership = None;
            state.delegation = None;
            state.client = None;
            state.token = None;
            state.directory = Directory::default();
            state.directory_read = 0;
            state.last_poll = None;
            state.last_error = None;
            // What obid learnt of this Sphere and its team says nothing of the next one.
            state.sphere_seals = false;
            state.team_reads_seal_keys = false;
            state.sphere_takes_card_runtime = false;
            state.team_takes_card_runtime = false;
            state.features_read = 0;
            state.features = None;
            state.url_aud_refused = false;
        }
        *self.chain.lock().unwrap_or_else(|e| e.into_inner()) = ChainKept::default();
        self.pins.forget_all();
        // The phone relay's socket stops serving phones now.
        if let Some(relay) = self.relay.get() {
            relay.wake();
        }
        Ok(self.membership_json())
    }

    /// `POST /v1/obid/sphere/key`: the Obi key, made the first time.
    pub async fn make_key(&self) -> Result<Value, ApiError> {
        let _one = self.joining.lock().await;
        self.no_key_over_unread()?;
        let existing = self.lock().key.clone();
        let key = match existing {
            Some(key) => key,
            None => {
                let key = Arc::new(self.files.make_key().map_err(|e| {
                    api(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("obid couldn't keep its key: {e}"),
                    )
                })?);
                learn_key(&key);
                self.lock().key = Some(key.clone());
                key
            }
        };
        let seal = self.seal_key()?;
        Ok(json!({"keyId": key.key_id(), "publicKey": key.public_key(),
                  "sealKeyId": seal.key_id(), "sealPublicKey": seal.public_key()}))
    }

    /// `409` while a key file obid couldn't read at start is there: a new key would replace it.
    fn no_key_over_unread(&self) -> Result<(), ApiError> {
        match &self.unread_key {
            Some(why) => Err(api(
                StatusCode::CONFLICT,
                format!(
                    "{why}. Oby makes no new key over it, which would end this Obi and its paired phones: fix the file, or move it out of obid's home, then restart obid."
                ),
            )),
            None => Ok(()),
        }
    }

    /// Whether a card's seal key, when it has one, is obid's own.
    fn carries_own_seal_key(&self, params: &Map<String, Value>) -> bool {
        let Some(public) = params.get("seal_public_key") else {
            return true;
        };
        let own = self.lock().seal.as_ref().map(|s| s.public_key());
        own.is_some_and(|own| public == own.as_str())
            && obi_protocol::seal::card_key(params).is_some()
    }

    /// The seal key, made the first time: with the Obi key, or for the card of an Obi that joined
    /// before obid sealed. Under `joining`.
    fn seal_key(&self) -> Result<Arc<SealKey>, ApiError> {
        if let Some(seal) = self.lock().seal.clone() {
            return Ok(seal);
        }
        self.no_key_over_unread()?;
        let seal = Arc::new(self.files.make_seal_key().map_err(|e| {
            api(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("obid couldn't keep its seal key: {e}"),
            )
        })?);
        learn_seal_key(&seal);
        self.lock().seal = Some(seal.clone());
        Ok(seal)
    }

    /// `GET /v1/obid/sphere/pins`: the owner keys this Obi trusts, each with the fingerprint owners
    /// compare, and any changed key waiting for this owner's OK.
    pub fn pins_json(&self) -> Value {
        self.pins.to_json()
    }

    /// `POST /v1/obid/sphere/pins/{owner}` with `{"fingerprint"}`: the owner compared `owner`'s new
    /// key with them and trusts it. The directory is read again.
    pub async fn accept_pin(&self, owner: &str, body: &[u8]) -> Result<Value, ApiError> {
        let map = object(body, &["fingerprint"])?;
        let seen = text_field(&map, "fingerprint")?;
        let pinned = self
            .pins
            .accept(owner, seen, time::now())
            .map_err(|m| api(StatusCode::UNPROCESSABLE_ENTITY, m))?;
        if self.lock().membership.is_some()
            && let Err(e) = self.read_directory().await
        {
            self.note_error(&e);
        }
        Ok(pinned)
    }

    /// `POST /v1/obid/sphere/card`: the card the owner is to sign, around obid's own key.
    pub async fn card(&self, body: &[u8]) -> Result<Value, ApiError> {
        let _one = self.joining.lock().await;
        let map = object(
            body,
            &[
                "url",
                "sphere",
                "obiId",
                "name",
                "ownerKey",
                "accepts",
                "description",
            ],
        )?;
        let invalid = |m: String| api(StatusCode::UNPROCESSABLE_ENTITY, m);
        let (key, card_seq) = {
            let state = self.lock();
            let key = state.key.clone().ok_or_else(|| {
                api(
                    StatusCode::CONFLICT,
                    "There's no Obi key yet: POST /v1/obid/sphere/key first.",
                )
            })?;
            (
                key,
                state.membership.as_ref().map_or(1, |m| m.card_seq() + 1),
            )
        };
        let url = text_field(&map, "url")?.trim_end_matches('/');
        if !is_base_url(url) {
            return Err(invalid(format!(
                "url is the Sphere's base URL, https (http only on this machine), like https://sphere.example.com, not {url:?}."
            )));
        }
        let sphere = text_field(&map, "sphere")?;
        let obi_id = text_field(&map, "obiId")?;
        let name = text_field(&map, "name")?;
        let owner_key = text_field(&map, "ownerKey")?;
        let description = match map.get("description") {
            None | Some(Value::Null) => obi_protocol::identity::DEFAULT_DESCRIPTION,
            Some(Value::String(d)) => d.as_str(),
            Some(_) => return Err(invalid("description is a string.".into())),
        };
        let mut accepts: Vec<String> = match map.get("accepts") {
            Some(Value::Array(list)) => list
                .iter()
                .map(|a| a.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| invalid("accepts is a list of request types.".into()))?,
            _ => {
                return Err(invalid(
                    "accepts is a list of request types, and it's required.".into(),
                ));
            }
        };
        // The frozen Mac app still asks for run_request and share_offer: they went with compute
        // sharing, so the card never offers them (teammates would send work this Mac declines).
        accepts.retain(|a| !matches!(a.as_str(), "run_request" | "share_offer"));
        if accepts.is_empty() {
            return Err(invalid("accepts names at least one request type.".into()));
        }
        if let Some(other) = accepts
            .iter()
            .find(|a| !matches!(a.as_str(), "review_request" | "question"))
        {
            return Err(invalid(format!(
                "obid takes review_request and question in this version, not {other}."
            )));
        }
        if obi_protocol::keys::key_id_of(owner_key).is_none() {
            return Err(invalid(
                "ownerKey is an Ed25519 public key, base64url.".into(),
            ));
        }
        if owner_key == key.public_key() {
            return Err(invalid(
                "ownerKey is the owner's key, not the Obi's.".into(),
            ));
        }
        let (key_id, public_key, issued_at) =
            (key.key_id(), key.public_key(), time::format(time::now()));
        // Its seal key goes in only once the Sphere and every other member can take a card with
        // one (identity.md section 2): a joining Obi's card has none, and a later one may.
        if self.is_joined()
            && let Err(e) = self.read_directory().await
        {
            self.note_error(&e);
        }
        // A card that carries it already never drops it: teammates know this Obi seals, and would
        // hold what they send it.
        let ready = {
            let state = self.lock();
            !own_seal_keys(&state).is_empty()
                || (state.membership.is_some() && state.sphere_seals && state.team_reads_seal_keys)
        };
        let seal = ready.then(|| self.seal_key()).transpose()?;
        let seal = seal.as_ref().map(|s| (s.key_id(), s.public_key()));
        // Its runtime and vendors go in the same way (identity.md section 2.3): once the Sphere and
        // every other member take `card_runtime`, and in every card after the first that has them.
        let runtime = {
            let state = self.lock();
            carries_runtime(&state)
                || (state.membership.is_some()
                    && state.sphere_takes_card_runtime
                    && state.team_takes_card_runtime)
        };
        let vendors = vendors(&self.config);
        let runtime = runtime.then_some(obi_protocol::identity::CardRuntime {
            features: &FEATURES,
            id: RUNTIME_ID,
            version: RUNTIME_VERSION,
            vendors: &vendors,
        });
        let owner = self.owner();
        let unsigned = obi_protocol::identity::unsigned_card(&obi_protocol::identity::CardFields {
            sphere_url: url,
            sphere,
            obi_id,
            owner: &owner,
            owner_key,
            name,
            description,
            version: OBID_VERSION,
            key_id: &key_id,
            public_key: &public_key,
            accepts: &accepts,
            card_seq,
            issued_at: &issued_at,
            seal: seal.as_ref().map(|(id, key)| (id.as_str(), key.as_str())),
            runtime,
        });
        let problems = obi_protocol::identity::unsigned_card_problems(&unsigned);
        if !problems.is_empty() {
            return Err(invalid(format!(
                "That card wouldn't pass the spec's checks: {}",
                problems.join("; ")
            )));
        }
        Ok(json!({"card": unsigned}))
    }

    /// `POST /v1/obid/sphere/join`: checks the owner-signed card and the delegation, redeems the
    /// invitation, exchanges a token, reads the directory and starts following.
    pub async fn join(self: &Arc<Self>, body: &[u8]) -> Result<Value, ApiError> {
        let _one = self.joining.lock().await;
        let map = object(body, &["url", "sphere", "invitation", "card", "delegation"])?;
        let invalid = |m: String| api(StatusCode::UNPROCESSABLE_ENTITY, m);
        let key = {
            let state = self.lock();
            if let Some(m) = &state.membership {
                return Err(api(
                    StatusCode::CONFLICT,
                    format!("obid has joined {} already ({}).", m.sphere, m.state),
                ));
            }
            state.key.clone().ok_or_else(|| {
                api(
                    StatusCode::CONFLICT,
                    "There's no Obi key yet: POST /v1/obid/sphere/key first.",
                )
            })?
        };
        let url = text_field(&map, "url")?.trim_end_matches('/').to_owned();
        if !is_base_url(&url) {
            return Err(invalid(format!(
                "url is the Sphere's base URL, https (http only on this machine), like https://sphere.example.com, not {url:?}."
            )));
        }
        let sphere = text_field(&map, "sphere")?.to_owned();
        if !is_slug(&sphere) {
            return Err(invalid(format!("{sphere:?} isn't a Sphere id.")));
        }
        let invitation = map
            .get("invitation")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid(INVITATION_SHAPE.into()))?;
        if invitation
            .keys()
            .any(|k| !matches!(k.as_str(), "id" | "secret" | "signin"))
        {
            return Err(invalid(INVITATION_SHAPE.into()));
        }
        let invitation_id = text_field(invitation, "id")?;
        let secret = text_field(invitation, "secret")?;
        if !is_token_text(invitation_id) || !is_token_text(secret) {
            return Err(invalid(
                "The invitation's id and secret are what the Sphere gave, unchanged.".into(),
            ));
        }
        // The owner's sign-in with the Sphere, for an invitation limited to a domain: passed on
        // to the redeem as it is, and kept nowhere.
        let signin = match invitation.get("signin") {
            None => None,
            Some(Value::Object(s))
                if s.len() == 2
                    && ["state", "code"]
                        .iter()
                        .all(|k| s.get(*k).and_then(Value::as_str).is_some_and(is_token_text)) =>
            {
                invitation.get("signin")
            }
            Some(_) => return Err(invalid(INVITATION_SHAPE.into())),
        };
        let card = map
            .get("card")
            .filter(|c| c.is_object())
            .ok_or_else(|| invalid("card is the agent card, signed by the owner.".into()))?
            .clone();
        let delegation = text_field(&map, "delegation")?.to_owned();
        let now = time::now();
        obi_protocol::card::verify(
            &card,
            &obi_protocol::card::Context {
                now: Some(now),
                bound_owner_key: None,
            },
        )
        .map_err(|r| invalid(format!("The card doesn't pass the spec's checks: {r}")))?;
        let params = obi_protocol::card::params(&card).expect("a verified card has its params");
        let param = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or("");
        if param("key_id") != key.key_id() || param("public_key") != key.public_key() {
            return Err(invalid("The card doesn't carry obid's own key.".into()));
        }
        if !self.carries_own_seal_key(params) {
            return Err(invalid(
                "The card carries a seal key that isn't obid's.".into(),
            ));
        }
        if param("sphere") != sphere {
            return Err(invalid(format!(
                "The card is for the Sphere {}, not {sphere}.",
                param("sphere")
            )));
        }
        let owner = self.owner();
        if param("owner") != owner {
            return Err(invalid(format!(
                "The card's owner is {}, not obid's owner {owner}.",
                param("owner"),
            )));
        }
        let obi_id = param("obi_id").to_owned();
        let expected_url = format!("{url}/v0.1/spheres/{sphere}/members/{obi_id}");
        if card["url"].as_str() != Some(expected_url.as_str()) {
            return Err(invalid(format!("The card's url isn't {expected_url}.")));
        }
        let owner_key = param("owner_key").to_owned();
        obi_protocol::identity::check_delegation(
            &delegation,
            &obi_protocol::identity::DelegationCheck {
                owner_key: &owner_key,
                owner: &owner,
                sphere: &sphere,
                sphere_url: &url,
                obi_id: &obi_id,
                now,
                needs: &NEEDED_SCOPES,
            },
        )
        .map_err(|why| invalid(format!("The delegation doesn't pass: {why}.")))?;

        let client = Client::new(&url, &sphere).map_err(|e| from_client(&e))?;
        client
            .redeem(invitation_id, secret, &owner_key, &card, signin)
            .await
            .map_err(|e| from_client(&e))?;
        let membership = Membership {
            state: "joined".into(),
            url,
            sphere,
            obi_id,
            card,
            joined_at: now,
        };
        crate::secrets::known().learn(DELEGATION_SECRET, &delegation);
        let kept = self
            .files
            .save_membership(&membership)
            .and_then(|()| self.files.save_delegation(&delegation))
            .and_then(|()| self.files.save_owner(&owner));
        {
            let mut state = self.lock();
            state.membership = Some(membership);
            state.delegation = Some(delegation);
            state.client = Some(client);
            state.token = None;
            state.features = None;
            state.url_aud_refused = false;
        }
        if let Err(e) = kept {
            self.note_error(format!("obid couldn't keep its membership: {e}"));
        }
        let read = self.read_directory().await;
        self.follow();
        self.start_sender();
        if let Err(e) = read {
            self.note_error(&e);
            return Err(from_client(&e));
        }
        Ok(self.membership_json())
    }

    // -----------------------------------------------------------------------------------------
    // Talking to the Sphere

    fn client(&self) -> Result<Client, ClientError> {
        self.lock()
            .client
            .clone()
            .ok_or_else(|| ClientError::Unreachable("obid hasn't joined a Sphere".into()))
    }

    /// The access token, exchanged again when the one held has less than a minute left.
    async fn token(&self) -> Result<String, ClientError> {
        let fresh = |state: &State| {
            state
                .token
                .as_ref()
                .filter(|(_, expires)| expires - time::now() > TOKEN_MARGIN)
                .map(|(t, _)| t.clone())
        };
        if let Some(token) = fresh(&self.lock()) {
            return Ok(token);
        }
        let _one = self.tokening.lock().await;
        let (client, delegation, key, membership) = {
            let state = self.lock();
            if let Some(token) = fresh(&state) {
                return Ok(token);
            }
            let missing = || ClientError::Unreachable("obid hasn't joined a Sphere".into());
            (
                state.client.clone().ok_or_else(missing)?,
                state.delegation.clone().ok_or_else(missing)?,
                state.key.clone().ok_or_else(missing)?,
                state.membership.clone().ok_or_else(missing)?,
            )
        };
        let now = time::now();
        // The actor assertion names the server too when the Sphere takes that (identity.md 4.1).
        let refused = self.lock().url_aud_refused;
        let binds = !refused
            && self
                .sphere_features(&client, |at| {
                    at.elapsed() < Duration::from_secs(FEATURES_EVERY.unsigned_abs())
                })
                .await
                .iter()
                .any(|f| f == obi_protocol::identity::SPHERE_URL_AUD);
        let by_url = binds
            .then(|| obi_protocol::identity::url_audience(client.base(), &membership.sphere))
            .flatten();
        let actor = |aud: &str| {
            obi_protocol::identity::actor_assertion_for(
                &membership.obi_id,
                aud,
                &key,
                now,
                &new_uuid(),
            )
            .map_err(|e| ClientError::Protocol(e.to_string()))
        };
        let plain = obi_protocol::identity::audience(&membership.sphere);
        let token = match by_url {
            Some(aud) => match client.token(&delegation, &actor(&aud)?).await {
                // A Sphere whose own address isn't the one obid joined by refuses its URL: the
                // same exchange naming the Sphere alone tells, and if it passes, obid names it
                // alone from then on.
                Err(ClientError::Refused { code, .. }) if code == "invalid_request" => {
                    let token = client.token(&delegation, &actor(&plain)?).await?;
                    let mut state = self.lock();
                    if same_server(&state, &client) {
                        state.url_aud_refused = true;
                    }
                    token
                }
                answer => answer?,
            },
            None => client.token(&delegation, &actor(&plain)?).await?,
        };
        crate::secrets::known().learn(SPHERE_TOKEN_SECRET, &token.access_token);
        self.lock().token = Some((token.access_token.clone(), now + token.expires_in));
        Ok(token.access_token)
    }

    /// The Sphere's `GET /v0.1/features`: the answer obid kept, if `fresh` says when it was read
    /// is recent enough, else read again. An answer that isn't the list (an Obysphere from before
    /// it, an error) counts as none.
    async fn sphere_features(
        &self,
        client: &Client,
        fresh: impl FnOnce(Instant) -> bool,
    ) -> Vec<String> {
        if let Some((at, list)) = self.lock().features.clone()
            && fresh(at)
        {
            return list;
        }
        let at = Instant::now();
        // Bounded well inside the token's last minute, which a token exchange may be waiting on.
        let list = tokio::time::timeout(FEATURES_WAIT, client.features())
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        let mut state = self.lock();
        // Kept only for the server it was read from: obid may have left and joined another since.
        if same_server(&state, client) {
            state.features = Some((at, list.clone()));
        }
        list
    }

    /// The Sphere's client while obid is a member (`joined`): the phone relay's (crate::relay).
    pub fn relay_client(&self) -> Option<Client> {
        let state = self.lock();
        state.membership.as_ref().filter(|m| m.state == "joined")?;
        state.client.clone()
    }

    /// Whether obid is a member of an Obysphere now (`joined`).
    pub fn in_sphere(&self) -> bool {
        self.lock()
            .membership
            .as_ref()
            .is_some_and(|m| m.state == "joined")
    }

    /// An access token for the phone relay's calls and socket, and when it ends (Unix seconds).
    pub async fn relay_token(&self) -> Result<(String, i64), ClientError> {
        let token = self.token().await?;
        let ends = self
            .lock()
            .token
            .as_ref()
            .filter(|(held, _)| *held == token)
            .map_or(time::now() + TOKEN_MARGIN, |(_, ends)| *ends);
        Ok((token, ends))
    }

    /// The Sphere refused a relay call: a refused token is forgotten, as for any other call.
    pub fn relay_refused(&self, error: &ClientError) {
        self.refused_token(error);
    }

    /// What to do with a refusal of the token: forget it, so the next call exchanges a new one.
    fn refused_token(&self, error: &ClientError) {
        if matches!(error, ClientError::Refused { status: 401, .. }) {
            self.lock().token = None;
        }
    }

    /// Reads the directory and keeps every member whose card passes the checks (identity.md 2.2,
    /// with the owner key the Sphere bound), with every key it ever had.
    async fn read_directory(&self) -> Result<(), ClientError> {
        let began = Instant::now();
        let token = self.token().await?;
        let members = match self.client()?.members(&token).await {
            Ok(members) => members,
            Err(e) => {
                self.refused_token(&e);
                return Err(e);
            }
        };
        let now = time::now();
        let mut directory = Directory::default();
        for member in &members {
            let obi = member["obi"].as_str().unwrap_or("");
            let owner = member["owner"].as_str().unwrap_or("");
            let card = &member["card"];
            let checked = obi_protocol::card::verify(
                card,
                &obi_protocol::card::Context {
                    now: Some(now),
                    bound_owner_key: member["owner_key"].as_str().map(str::to_owned),
                },
            );
            let params = obi_protocol::card::params(card);
            let matches = params.is_some_and(|p| p["obi_id"] == obi && p["owner"] == owner);
            if let Err(why) = checked {
                tracing::warn!(
                    "sphere: {obi}'s card doesn't pass the checks ({}): not trusted",
                    why.code
                );
                continue;
            }
            if !matches {
                tracing::warn!("sphere: {obi}'s card names another Obi or owner: not trusted");
                continue;
            }
            // Only a card signed with the owner key pinned here, whatever the Sphere bound.
            let owner_key = params.and_then(|p| p["owner_key"].as_str()).unwrap_or("");
            if !self.pins.check(owner, owner_key, now) {
                continue;
            }
            // Never an older card than one trusted before (a key rotated away coming back).
            let card_seq = params.and_then(|p| p["card_seq"].as_u64()).unwrap_or(0);
            if !self.pins.check_card(obi, card_seq) {
                continue;
            }
            // The Obi key is the one the card signs: a current key the Sphere lists must be it.
            // Older keys (with `valid_until_seq`) only check entries signed before they ended.
            let (card_id, card_key) = (
                params.and_then(|p| p["key_id"].as_str()).unwrap_or(""),
                params.and_then(|p| p["public_key"].as_str()).unwrap_or(""),
            );
            let keys: Vec<&Value> = member["keys"].as_array().into_iter().flatten().collect();
            let current: Vec<&&Value> = keys
                .iter()
                .filter(|k| k["valid_until_seq"].as_u64().is_none())
                .collect();
            if current.len() != 1
                || current[0]["key_id"] != card_id
                || current[0]["public_key"] != card_key
            {
                tracing::warn!(
                    "sphere: the Sphere lists a key for {obi} that its card doesn't sign ({}): not trusted",
                    current
                        .iter()
                        .filter_map(|k| k["key_id"].as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                continue;
            }
            self.pins.keep_key(obi, card_id, card_key);
            for key in keys {
                let (Some(id), Some(public_key)) =
                    (key["key_id"].as_str(), key["public_key"].as_str())
                else {
                    continue;
                };
                let older = key["valid_until_seq"].as_u64().is_some();
                if older && (id == card_id || !self.pins.knows_key(obi, id, public_key)) {
                    // Only a key a card obid trusted signed: the Sphere can't add one of its own.
                    tracing::warn!(
                        "sphere: the Sphere lists an older key for {obi} no card obid trusted signed: not used"
                    );
                    continue;
                }
                directory.keys.insert(
                    id.to_owned(),
                    DirectoryKey {
                        obi: obi.to_owned(),
                        public_key: public_key.to_owned(),
                        ranged: true,
                        valid_from_seq: key["valid_from_seq"].as_u64().unwrap_or(1),
                        valid_until_seq: key["valid_until_seq"].as_u64(),
                    },
                );
            }
            // An Obi whose trusted card had a seal key is sent nothing in the clear again.
            if params.and_then(obi_protocol::seal::card_key).is_some() {
                self.pins.mark_seals(obi);
            }
            directory.members.insert(
                obi.to_owned(),
                Member {
                    owner: owner.to_owned(),
                    active: member["status"] == "active",
                    accepts: params
                        .and_then(|p| p.get("accepts"))
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                    seal: params.and_then(obi_protocol::seal::card_key),
                    sealed_since: params
                        .filter(|p| obi_protocol::seal::card_key(p).is_some())
                        .and_then(|p| p.get("issued_at")?.as_str().and_then(time::parse)),
                },
            );
        }
        // Whether obid may put its seal key in a card (identity.md section 2): every other active
        // member's obid reads one, whatever obid trusts of it, and the Sphere takes one (asked
        // until it says so).
        let me = self.lock().membership.as_ref().map(|m| m.obi_id.clone());
        let others = || {
            members
                .iter()
                .filter(|m| m["status"] == "active" && me.as_deref() != m["obi"].as_str())
        };
        let team_reads_seal_keys =
            others().all(|m| reads_seal_keys(m["card"]["version"].as_str().unwrap_or("")));
        // And whether it may put its runtime in one: every other member takes `card_runtime`, and
        // so does the Sphere (asked, as for sealing, until it says so).
        let team_takes_card_runtime = others().all(|m| takes_card_runtime(&m["card"]));
        let (known_seals, known_runtime, features_read, team_was_ready) = {
            let state = self.lock();
            (
                state.sphere_seals,
                state.sphere_takes_card_runtime,
                state.features_read,
                state.team_takes_card_runtime,
            )
        };
        let sphere_seals = known_seals
            || self
                .client()?
                .health()
                .await
                .is_ok_and(|h| h["seal"] == true);
        // The additions it takes are at their own endpoint, never in health (spec README,
        // "Versions"): asked only once the team takes card_runtime, when it first does, and then
        // every 10 minutes until the Sphere lists it.
        let ask = !known_runtime
            && team_takes_card_runtime
            && (!team_was_ready || now - features_read >= FEATURES_EVERY);
        // An answer read since this read began (its token exchange's) is fresh enough.
        let sphere_takes_card_runtime = known_runtime
            || (ask
                && self
                    .sphere_features(&self.client()?, |at| at >= began)
                    .await
                    .iter()
                    .any(|x| x == obi_protocol::identity::CARD_RUNTIME));
        let revoked = {
            let mut state = self.lock();
            let revoked = me
                .and_then(|me| members.iter().find(|m| m["obi"] == me.as_str()).cloned())
                .is_some_and(|m| m["status"] == "revoked");
            state.directory = directory;
            state.directory_read = now;
            state.team_reads_seal_keys = team_reads_seal_keys;
            state.sphere_seals = sphere_seals;
            state.team_takes_card_runtime = team_takes_card_runtime;
            state.sphere_takes_card_runtime = sphere_takes_card_runtime;
            if ask {
                state.features_read = now;
            }
            if revoked && let Some(m) = state.membership.as_mut() {
                m.state = "revoked".into();
            }
            revoked.then(|| state.membership.clone()).flatten()
        };
        if let Some(membership) = revoked {
            let _ = self.files.save_membership(&membership);
            // Nothing goes to the Sphere any more: what's open is settled here, its peers not told.
            self.settle_on_revoke();
            // The relay's socket stops serving phones now, not at its next check.
            if let Some(relay) = self.relay.get() {
                relay.wake();
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // Sharing rules and limits: GET /v1/obid/sharing, POST …/rules, DELETE …/rules/{id},
    // PUT …/limits

    /// `GET /v1/obid/sharing`.
    pub fn sharing_json(&self) -> Value {
        self.lock().sharing.to_json()
    }

    /// `POST /v1/obid/sharing/rules`: a new rule, kept after the others.
    pub fn add_rule(&self, body: &[u8]) -> Result<Value, ApiError> {
        let value: Value = serde_json::from_slice(body)
            .map_err(|e| api(StatusCode::BAD_REQUEST, format!("The body isn't JSON: {e}")))?;
        let rule = sharing::rule_from(&value, time::now())
            .map_err(|why| api(StatusCode::UNPROCESSABLE_ENTITY, why))?;
        {
            let mut state = self.lock();
            if state.sharing.rules.len() >= sharing::MAX_RULES {
                return Err(api(
                    StatusCode::CONFLICT,
                    format!("obid keeps at most {} sharing rules.", sharing::MAX_RULES),
                ));
            }
            state.sharing.rules.push(rule.clone());
            // Kept under the lock, so an older copy never overwrites a newer one.
            self.save_sharing(&state.sharing);
        }
        Ok(rule.to_json())
    }

    /// `DELETE /v1/obid/sharing/rules/{id}`: `false` for one obid doesn't have.
    pub fn forget_rule(&self, id: &str) -> bool {
        let mut state = self.lock();
        let before = state.sharing.rules.len();
        state.sharing.rules.retain(|r| r.id != id);
        if state.sharing.rules.len() == before {
            return false;
        }
        self.save_sharing(&state.sharing);
        true
    }

    /// `PUT /v1/obid/sharing/limits`.
    pub fn set_limits(&self, body: &[u8]) -> Result<Value, ApiError> {
        let value: Value = serde_json::from_slice(body)
            .map_err(|e| api(StatusCode::BAD_REQUEST, format!("The body isn't JSON: {e}")))?;
        let limits = sharing::limits_from(&value)
            .map_err(|why| api(StatusCode::UNPROCESSABLE_ENTITY, why))?;
        let mut state = self.lock();
        state.sharing.limits = limits;
        self.save_sharing(&state.sharing);
        Ok(limits.to_json())
    }
}

impl Link {
    /// Who obid acts for in its Sphere: the owner it joined as while it keeps a membership, else
    /// config's `owner`, or the GitHub login it adopted ([`Link::adopt_github_owner`]).
    pub(crate) fn owner(&self) -> String {
        self.owner.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Until obid first joins a Sphere, its owner becomes `github`, the login gh is signed in as,
    /// when config's `owner` is still the Mac's login name obid wrote at its first start (#1214): a
    /// Sphere binds one owner key per owner id, and two people's Macs often share a login name
    /// (or have one that isn't an id, `john.doe`). An owner the person set themselves stays, and so
    /// does the one obid ever joined as (`home/owner`, kept after a leave): the Sphere would refuse
    /// that person's rejoin under another id ("this account joined as another owner").
    pub fn adopt_github_owner(&self, github: &str) {
        let github = github.to_ascii_lowercase();
        let login = nix::unistd::User::from_uid(nix::unistd::getuid())
            .ok()
            .flatten()
            .map(|user| user.name);
        if self.lock().membership.is_some()
            || self.files.load_owner().is_some()
            || login.as_deref() != Some(self.config.owner.as_str())
            || !is_slug(&github)
        {
            return;
        }
        *self.owner.lock().unwrap_or_else(|e| e.into_inner()) = github;
    }

    /// `GET /v1/obid/sphere/audit`: the Sphere's audit chain, checked here end to end against the
    /// directory's keys, and the entries this Obi sent or got, as metadata only (#810): who, which
    /// Obi, the repository, the kind, the times, the host's use and the cost; no message's text.
    pub async fn audit_json(&self) -> Result<Value, ApiError> {
        let membership = self
            .lock()
            .membership
            .clone()
            .filter(|m| m.state == "joined")
            .ok_or_else(|| api(StatusCode::CONFLICT, "obid hasn't joined a Sphere."))?;
        self.read_directory().await.map_err(|e| from_client(&e))?;
        let token = self.token().await.map_err(|e| from_client(&e))?;
        let client = self.client().map_err(|e| from_client(&e))?;
        // ponytail: the whole chain each time, at most MAX_AUDIT entries; from a checked head
        // once chains grow long.
        const MAX_AUDIT: usize = 100_000;
        let mut entries: Vec<Value> = Vec::new();
        let mut head = json!({"seq": 0, "hash": obi_protocol::ZERO_HASH});
        loop {
            let page = client
                .audit(&token, entries.len() as u64 + 1, 1000)
                .await
                .map_err(|e| from_client(&e))?;
            head = page["head"].clone();
            let got = page["entries"].as_array().cloned().unwrap_or_default();
            let done = got.is_empty()
                || entries.len() + got.len() >= head["seq"].as_u64().unwrap_or(0) as usize;
            entries.extend(got);
            if entries.len() > MAX_AUDIT {
                return Err(api(
                    StatusCode::BAD_GATEWAY,
                    "The Sphere's chain is longer than obid checks here (100,000 entries).",
                ));
            }
            if done {
                break;
            }
        }
        let chain = json!({"sphere": membership.sphere, "entries": entries});
        let checked =
            obi_protocol::chain::verify(&chain, &self.lock().directory.keys).and_then(|()| {
                // The head the Sphere names is the chain's last entry.
                let last = entries
                    .last()
                    .map_or((0, obi_protocol::ZERO_HASH.to_owned()), |e| {
                        (
                            e["seq"].as_u64().unwrap_or(0),
                            e["hash"].as_str().unwrap_or("").to_owned(),
                        )
                    });
                if head["seq"].as_u64() == Some(last.0) && head["hash"].as_str() == Some(&last.1) {
                    Ok(())
                } else {
                    Err(obi_protocol::Rejection::new(
                        "head_mismatch",
                        "the Sphere's head isn't the chain's last entry",
                    ))
                }
            });
        // The chain checks out by itself; it must also be the one this Obi saw before.
        let problem = match &checked {
            Err(rejection) => Some(rejection.code.to_owned()),
            Ok(()) => {
                let mut kept = self.chain.lock().unwrap();
                let found = kept.problem(&entries);
                if found.is_none() {
                    kept.head = Some((
                        head["seq"].as_u64().unwrap_or(0),
                        head["hash"].as_str().unwrap_or("").to_owned(),
                    ));
                    kept.save(&self.config.home);
                }
                found.map(str::to_owned)
            }
        };
        let me = membership.obi_id.as_str();
        let mine: Vec<Value> = entries
            .iter()
            .filter(|e| e["envelope"]["from"] == me || e["envelope"]["to"] == me)
            .map(audit_entry)
            .collect();
        Ok(
            json!({"verified": problem.is_none(), "problem": problem, "head": head, "entries": mine}),
        )
    }

    /// `GET /v1/obid/sphere/members`: the other active members of the Sphere whose cards obid
    /// trusts (identity.md 2.2, its pinned owner keys), as typed fields only: who they are, what
    /// they take, whether obid seals its words for them, and whether the owner compared their
    /// owner's key. The directory is read fresh, so a teammate who just joined shows; when it
    /// can't be read, the last read answers. `409` before joining.
    pub async fn members_json(&self) -> Result<Value, ApiError> {
        let joined = self
            .lock()
            .membership
            .as_ref()
            .is_some_and(|m| m.state == "joined");
        if !joined {
            return Err(api(StatusCode::CONFLICT, "obid hasn't joined a Sphere."));
        }
        if let Err(e) = self.read_directory().await {
            self.note_error(&e);
        }
        let state = self.lock();
        let me = state
            .membership
            .as_ref()
            .map(|m| m.obi_id.clone())
            .unwrap_or_default();
        let mut list: Vec<Value> = state
            .directory
            .members
            .iter()
            .filter(|(obi, m)| **obi != me && m.active)
            .map(|(obi, m)| {
                let mut accepts = m.accepts.clone();
                accepts.sort();
                json!({"obi": obi, "owner": m.owner, "accepts": accepts,
                       "sealed": m.seal.is_some(), "verified": self.pins.verified(&m.owner)})
            })
            .collect();
        list.sort_by(|a, b| {
            (a["owner"].as_str(), a["obi"].as_str()).cmp(&(b["owner"].as_str(), b["obi"].as_str()))
        });
        Ok(json!(list))
    }

    /// `PUT /v1/obid/sphere/membership`: a new card and/or delegation the owner signed (to take
    /// `run_request`, or a delegation with `send:run_request`, or a fresh one), checked as at
    /// joining. The card goes to the Sphere first; the delegation is kept and the token renewed.
    pub async fn renew(self: &Arc<Self>, body: &[u8]) -> Result<Value, ApiError> {
        let _one = self.joining.lock().await;
        let map = object(body, &["card", "delegation"])?;
        let invalid = |m: String| api(StatusCode::UNPROCESSABLE_ENTITY, m);
        if map.is_empty() {
            return Err(invalid(
                "The body has a card, a delegation, or both.".into(),
            ));
        }
        let (key, membership) = {
            let state = self.lock();
            let membership = state
                .membership
                .clone()
                .filter(|m| m.state == "joined")
                .ok_or_else(|| api(StatusCode::CONFLICT, "obid hasn't joined a Sphere."))?;
            (
                state
                    .key
                    .clone()
                    .ok_or_else(|| api(StatusCode::CONFLICT, "There's no Obi key."))?,
                membership,
            )
        };
        let now = time::now();
        let owner_key = obi_protocol::card::params(&membership.card)
            .and_then(|p| {
                p.get("owner_key")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        let owner = self.owner();
        let mut card = None;
        if let Some(new) = map.get("card") {
            obi_protocol::card::verify(
                new,
                &obi_protocol::card::Context {
                    now: Some(now),
                    bound_owner_key: Some(owner_key.clone()),
                },
            )
            .map_err(|r| invalid(format!("The card doesn't pass the spec's checks: {r}")))?;
            let params = obi_protocol::card::params(new).expect("a verified card has its params");
            let param = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or("");
            if param("key_id") != key.key_id()
                || param("obi_id") != membership.obi_id
                || param("sphere") != membership.sphere
                || param("owner") != owner
            {
                return Err(invalid(
                    "The card isn't for this Obi, its key, Sphere and owner.".into(),
                ));
            }
            if !self.carries_own_seal_key(params) {
                return Err(invalid(
                    "The card carries a seal key that isn't obid's.".into(),
                ));
            }
            // A card that listed card_runtime never drops it (identity.md section 2.3): teammates
            // know what runs this Obi, and its runtime is obid's own.
            let held = obi_protocol::card::params(&membership.card);
            let listed = |p: Option<&Map<String, Value>>| {
                p.is_some_and(|p| {
                    obi_protocol::identity::lists_feature(p, obi_protocol::identity::CARD_RUNTIME)
                })
            };
            if listed(held) && !listed(Some(params)) {
                return Err(invalid(
                    "The card drops card_runtime, which the card it replaces listed.".into(),
                ));
            }
            if params.get("runtime").is_some() && params["runtime"]["id"] != RUNTIME_ID {
                return Err(invalid(format!(
                    "The card's runtime isn't obid's ({RUNTIME_ID})."
                )));
            }
            if params.get("card_seq").and_then(Value::as_u64).unwrap_or(0) <= membership.card_seq()
            {
                return Err(invalid(format!(
                    "The card's card_seq must be more than {}.",
                    membership.card_seq()
                )));
            }
            card = Some(new.clone());
        }
        let delegation = match map.get("delegation") {
            None => None,
            Some(Value::String(d)) => {
                obi_protocol::identity::check_delegation(
                    d,
                    &obi_protocol::identity::DelegationCheck {
                        owner_key: &owner_key,
                        owner: &owner,
                        sphere: &membership.sphere,
                        sphere_url: &membership.url,
                        obi_id: &membership.obi_id,
                        now,
                        needs: &NEEDED_SCOPES,
                    },
                )
                .map_err(|why| invalid(format!("The delegation doesn't pass: {why}.")))?;
                Some(d.clone())
            }
            Some(_) => {
                return Err(invalid(
                    "delegation is the owner's delegation, a JWT.".into(),
                ));
            }
        };
        if let Some(d) = &delegation {
            crate::secrets::known().learn(DELEGATION_SECRET, d);
            self.files.save_delegation(d).map_err(|e| {
                api(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("obid couldn't keep the delegation: {e}"),
                )
            })?;
            let mut state = self.lock();
            state.delegation = Some(d.clone());
            state.token = None;
        }
        if let Some(card) = card {
            let token = self.token().await.map_err(|e| from_client(&e))?;
            self.client()
                .map_err(|e| from_client(&e))?
                .update_card(&token, &membership.obi_id, &card)
                .await
                .map_err(|e| from_client(&e))?;
            let membership = {
                let mut state = self.lock();
                if let Some(m) = state.membership.as_mut() {
                    m.card = card;
                }
                state.membership.clone()
            };
            if let Some(m) = membership {
                let _ = self.files.save_membership(&m);
            }
        }
        if let Err(e) = self.read_directory().await {
            self.note_error(&e);
        }
        Ok(self.membership_json())
    }
}

/// What [`Link::decide_question`] decided.
enum Decided {
    /// Declined (`decisions`: a remembered decision was used).
    Settled { decisions: bool },
    /// In the owner's inbox: its item, boxed (an item is far larger than the rest).
    Inbox(Box<Item>),
    /// The Obi answers it by itself, under this sharing rule.
    ByItself(Rule),
}

/// A failed review result: no findings, no text.
fn failed_result(request: &Value) -> Value {
    let body = &request["body"];
    json!({"for": "review_request", "outcome": "failed", "repo": body["repo"], "head_sha": body["head_sha"], "findings": []})
}

/// Every string in a reply's body, each with where it is (`finding 1 file`, `summary`,
/// `condition 2 kind`): the publish gate scans all of them, the agent's paths too, not only the
/// fields meant as prose (ARCHITECTURE.md section 4: nothing leaves until the scan has passed on
/// everything in it).
pub(crate) fn every_text(body: &Value) -> Vec<(String, String)> {
    fn walk(value: &Value, place: String, out: &mut Vec<(String, String)>) {
        match value {
            Value::String(text) => out.push((place, text.clone())),
            Value::Array(items) => {
                let one = match place.as_str() {
                    "findings" => "finding",
                    "acceptance" => "condition",
                    other => other,
                }
                .to_owned();
                for (i, item) in items.iter().enumerate() {
                    walk(item, format!("{one} {}", i + 1), out);
                }
            }
            Value::Object(fields) => {
                for (key, field) in fields {
                    let here = if place.is_empty() {
                        key.clone()
                    } else {
                        format!("{place} {key}")
                    };
                    walk(field, here, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(body, String::new(), &mut out);
    out
}

/// Why obid's approval on GitHub may not stand, read again right after it was posted.
enum Recheck {
    /// The pull request changed: its head, or the code owners' approvals next to it.
    Changed(String),
    /// `gh` couldn't read it.
    Unread(String),
}

/// Why a Confirm without the owner's tap is refused (#776).
pub const NEEDS_YOUR_TAP: &str =
    "Only your tap can confirm this: confirm it in Oby on your Mac or iPhone.";

/// How long an offer (`oby_offer`) stays open: a day, then Oby closes it ([`OFFER_EXPIRED`]).
const OFFER_SECONDS: i64 = 24 * 3600;
const OFFER_EXPIRED: &str =
    "Closed by Oby: offered a day ago and never started. Ask Oby again to start it.";
const OFFER_GONE: &str = "Closed by Oby: the conversation it was offered in was deleted.";

/// Whether `item` is work the brain offered (`oby_offer`) a day or more before `now`.
fn expired(item: &Item, now: i64) -> bool {
    item.start.as_ref().is_some_and(|s| s["offer"].is_object())
        && now - item.created >= OFFER_SECONDS
}

/// Closes `inbox`'s item `id` when it's an offer past its day ([`expired`]), answering it closed;
/// none otherwise. The caller holds the lock `inbox` is under.
fn close_expired(inbox: &mut HashMap<String, Item>, id: &str, now: i64) -> Option<Item> {
    let item = inbox.get(id).filter(|i| expired(i, now))?.clone();
    let closed = closed_start(item, OFFER_EXPIRED, now);
    inbox.insert(id.to_owned(), closed.clone());
    Some(closed)
}

/// A `start` item done by Oby itself, `why` above its words, with no options: nothing starts.
fn closed_start(mut item: Item, why: &str, now: i64) -> Item {
    item.text = format!("{why}\n\n{}", inbox::unfenced(&item.text));
    item.options = vec![];
    item.details = None;
    item.recommended = None;
    item.done = Some(now);
    item
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use obi_protocol::client::Delivery;

    use super::*;

    /// The publish gate scans every string of the reply: the agent's paths as much as its prose.
    #[test]
    fn the_gate_reads_every_text_of_a_result() {
        let body = json!({"for": "review_request", "outcome": "approved", "repo": "o/r",
            "head_sha": "a".repeat(40), "summary": "S",
            "findings": [{"id": "f-1", "severity": "nit", "file": "src/F1.js", "line": 3, "title": "T1", "detail": "D1"},
                         {"id": "f-2", "severity": "minor", "file": "src/F2.js", "line": 4, "title": "T2", "detail": "D2", "end_line": 5}],
            "acceptance": [{"index": 0, "kind": "tests_pass", "met": true}]});
        let texts: std::collections::BTreeMap<String, String> =
            every_text(&body).into_iter().collect();
        for (place, text) in [
            ("finding 1 file", "src/F1.js"),
            ("finding 1 title", "T1"),
            ("finding 1 detail", "D1"),
            ("finding 2 file", "src/F2.js"),
            ("finding 2 title", "T2"),
            ("finding 2 detail", "D2"),
            ("summary", "S"),
            ("repo", "o/r"),
            ("condition 1 kind", "tests_pass"),
        ] {
            assert_eq!(
                texts.get(place).map(String::as_str),
                Some(text),
                "{place}: {texts:?}"
            );
        }
        // Numbers and booleans aren't text; all 16 strings are there.
        assert_eq!(texts.len(), 16, "{texts:?}");
    }

    #[test]
    fn base_urls() {
        assert!(is_base_url("http://127.0.0.1:8787"));
        assert!(is_base_url("https://sphere.example.com"));
        assert!(!is_base_url("http://127.0.0.1:8787/v0.1"));
        assert!(!is_base_url("ftp://x"));
        assert!(!is_base_url("http://"));
        assert!(!is_base_url("http://a?b"));
        // Plain http only on this machine.
        assert!(is_base_url("http://localhost:8787"));
        assert!(is_base_url("http://[::1]:8787"));
        assert!(is_base_url("http://127.0.0.2"));
        assert!(!is_base_url("http://obysphere.deskree.com"));
        assert!(!is_base_url("http://10.0.0.5:8787"));
        assert!(!is_base_url("http://169.254.169.254"));
        assert!(!is_base_url("http://127.0.0.1.nip.io"));
        assert!(is_base_url("https://obysphere.deskree.com"));
    }

    #[test]
    fn later_dates() {
        assert_eq!(
            parse_later("2026-10-10T10:12:00Z"),
            time::parse("2026-10-10T10:12:00Z")
        );
        assert_eq!(
            parse_later("2026-10-10T10:12:00.123Z"),
            time::parse("2026-10-10T10:12:00Z")
        );
        assert_eq!(parse_later("tomorrow"), None);
        assert_eq!(parse_later("2026-10-10T10:12:00.Z"), None);
    }

    #[test]
    fn a_failed_result_has_no_text_and_passes_the_schema() {
        let request = json!({"body": {"repo": "obi-e2e/fixture-app", "head_sha": "6d20146f366b9fac8907b5066c30ee39aaec09b3"}});
        let mut body = failed_result(&request);
        assert!(obi_protocol::schema::is_valid("actions/result", &body));
        body["by_rule"] = json!("d-3f9a0c1b2e4d");
        assert!(obi_protocol::schema::is_valid("actions/result", &body));
    }

    /// A review that is too big once sealed is shortened until it fits, and still opens; one that
    /// can't fit, whatever is shortened, goes `failed`, with no text (protocol.md section 9.1).
    #[test]
    fn a_review_too_big_once_sealed_is_shortened_to_fit() {
        let key = SealKey::generate().unwrap();
        let signer = Keypair::from_seed_string("obid fit test");
        let findings: Vec<Value> = (0..50)
            .map(|i| json!({"id": format!("f-{i:016x}"), "severity": "minor", "file": "src/a.js", "line": i + 1,
                            "title": format!("Finding {i}"), "detail": "Ünïcode détail. ".repeat(250)}))
            .collect();
        let body = json!({"for": "review_request", "outcome": "approved", "repo": "o/r",
            "head_sha": "6d20146f366b9fac8907b5066c30ee39aaec09b3", "findings": findings,
            "acceptance": [{"index": 0, "kind": "no_blocker_findings", "met": true}],
            "summary": "s".repeat(4000), "by_rule": "d-3f9a0c1b2e4d"});
        let unsigned = json!({"protocol": "obi/0.1", "id": "8a6c1f4e-5d2b-4c7a-9e3f-1b2c3d4e5f60", "type": "result",
            "from": "obi-a", "to": "obi-b", "sphere": "s", "correlation_id": "9a6c1f4e-5d2b-4c7a-9e3f-1b2c3d4e5f60",
            "in_reply_to": "9a6c1f4e-5d2b-4c7a-9e3f-1b2c3d4e5f60", "created_at": "2026-10-26T12:00:00Z",
            "expires_at": "2026-10-26T13:00:00Z", "key_id": signer.key_id()});
        let make = |body: &Value| -> Result<Value, String> {
            let mut unsigned = unsigned.as_object().unwrap().clone();
            unsigned.insert("body".into(), body.clone());
            obi_protocol::seal::seal(&mut unsigned, key.public())?;
            let signed =
                obi_protocol::envelope::sign(&unsigned, &signer).map_err(|e| e.to_string())?;
            Ok(kept(signed, body.clone()))
        };
        assert!(
            obi_protocol::jcs::canonical_bytes(&wire(&make(&body).unwrap()))
                .unwrap()
                .len()
                > obi_protocol::MAX_ENVELOPE_BYTES
        );
        let fit = fitted(body, make).unwrap();
        let sent = wire(&fit);
        assert!(
            obi_protocol::jcs::canonical_bytes(&sent).unwrap().len()
                <= obi_protocol::MAX_ENVELOPE_BYTES
        );
        let opened = obi_protocol::seal::open(&sent, &key).unwrap();
        assert_eq!(opened, fit["body"]);
        assert!(obi_protocol::schema::is_valid("actions/result", &opened));
        assert_eq!(opened["outcome"], "approved");
        assert_eq!(opened["findings"].as_array().unwrap().len(), 50);
        let detail = opened["findings"][0]["detail"].as_str().unwrap();
        assert!(detail.starts_with("Ünïcode détail.") && detail.ends_with('…'));
        // Whatever is shortened, it doesn't fit: a text-free `failed`, its rule kept.
        let huge = fitted(fit["body"].clone(), |body| {
            let pad = if body["outcome"] == "failed" {
                0
            } else {
                70_000
            };
            Ok(json!({"body": body, "pad": "x".repeat(pad)}))
        })
        .unwrap();
        assert_eq!(
            huge["body"],
            json!({"for": "review_request", "outcome": "failed", "repo": "o/r",
                   "head_sha": "6d20146f366b9fac8907b5066c30ee39aaec09b3", "findings": [], "by_rule": "d-3f9a0c1b2e4d"})
        );
    }

    #[test]
    fn bodies_take_only_their_fields() {
        assert!(object(br#"{"to": "x"}"#, &["to"]).is_ok());
        assert_eq!(
            object(br#"{"to": "x", "prompt": "y"}"#, &["to"])
                .err()
                .unwrap()
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            object(b"[", &["to"]).err().unwrap().status,
            StatusCode::BAD_REQUEST
        );
    }

    /// Approving a teammate's pull request on GitHub takes the owner's own tap on that exchange's
    /// ask, at the commit its request named, signed within two minutes, while the request is open
    /// and obid is in its Sphere: no API answer, no other exchange's tap, no tap on an ask for
    /// another commit, no stale one, none after the expiry or a revoke. Every refusal here comes
    /// before gh is asked anything.
    #[tokio::test]
    async fn only_the_owners_fresh_tap_on_that_exchange_at_that_commit_approves() {
        let home = std::env::temp_dir().join(format!("obid-link-host-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "wellington"}).to_string(),
        )
        .unwrap();
        let events = Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (_stop, stopping) = watch::channel(false);
        let link = Arc::new(Link::open(config, jobs, events, stopping));
        let now = time::now();
        const HEAD: &str = "6d20146f366b9fac8907b5066c30ee39aaec09b3";
        const OLDER: &str = "1111111111111111111111111111111111111111";
        // A repository GitHub doesn't have: were anything to reach gh, it would find nothing.
        let request = |head: &str| {
            json!({"repo": "obi-x5-test/none", "pr": 12, "base_branch": "main", "head_sha": head,
                   "approve_on_host": true})
        };
        // The exchange as obid keeps it, its result sent (`asked`), and the ask in the inbox.
        let ask = |cid: &str, head: &str, asked_at: &str, expires: i64| {
            let envelope = json!({"id": cid, "correlation_id": cid, "type": "review_request",
                                  "expires_at": time::format(expires), "body": request(head)});
            let mut exchange = Exchange::new(
                "incoming",
                envelope,
                "obi-ernesto".into(),
                Some("ernesto".into()),
            );
            exchange.state = "done".into();
            link.lock().exchanges.insert(cid.into(), exchange);
            let item = approval::item(
                cid,
                "obi-ernesto",
                "ernesto",
                &request(asked_at),
                &time::format(expires),
                "",
                now,
            );
            link.lock().inbox.insert(item.id.clone(), item.clone());
            item
        };
        let membership = |state: &str| Membership {
            state: state.into(),
            url: "http://127.0.0.1:1".into(),
            sphere: "team".into(),
            obi_id: "obi-wellington".into(),
            card: json!({}),
            joined_at: 0,
        };
        link.lock().membership = Some(membership("joined"));
        let confirm = json!({"answer": inbox::CONFIRM}).to_string();
        let tap = |item: &Item, at: i64| {
            let (link, item, confirm) = (link.clone(), item.clone(), confirm.clone());
            async move {
                link.answer_confirmed(&item.id, confirm.as_bytes(), &item.title, at)
                    .await
            }
        };

        // The ask names the exchange (its id) and the commit (its title), which the device signs.
        let item = ask("c-1", HEAD, HEAD, now + 3600);
        assert_eq!(item.id, "obi-approve-c-1");
        assert_eq!(item.kind, "confirm");
        assert_eq!(
            item.title,
            "Approve ernesto's pull request #12 on obi-x5-test/none at 6d20146 on GitHub, as you"
        );
        assert_eq!(item.options, [inbox::CONFIRM, inbox::CANCEL]);
        // No device, no approval: the API token alone is refused and the ask stays open.
        let refused = link.answer(&item.id, confirm.as_bytes()).await.unwrap_err();
        assert_eq!(refused.status, StatusCode::FORBIDDEN);
        assert!(link.item(&item.id).unwrap().done.is_none());
        // A tap signed 121 seconds ago is stale.
        let refused = tap(&item, now - 121).await.unwrap_err();
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.message);
        assert!(refused.message.starts_with("Nothing was approved"));

        // The ask carries another commit than the exchange's request: it counts for nothing.
        let older = ask("c-2", HEAD, OLDER, now + 3600);
        let refused = tap(&older, now).await.unwrap_err();
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.message);
        // One exchange's tap, carried over to another exchange's ask, approves nothing.
        let first = ask("c-3", HEAD, HEAD, now + 3600);
        let second = ask("c-4", HEAD, HEAD, now + 3600);
        let carried = link
            .confirmed_approval(
                &first.id,
                &second.confirm.as_ref().unwrap().payload,
                Some(now),
            )
            .await
            .unwrap_err();
        assert_eq!(carried.status, StatusCode::CONFLICT);
        assert_eq!(
            carried.message,
            "Nothing was approved: that request is gone."
        );
        // No device's signature at all: nothing either.
        let unsigned = link
            .confirmed_approval(&first.id, &first.confirm.as_ref().unwrap().payload, None)
            .await
            .unwrap_err();
        assert_eq!(unsigned.status, StatusCode::CONFLICT);

        // After the expiry, a tap approves nothing, and the clock settles the ask.
        let late = ask("c-late", HEAD, HEAD, now - 1);
        let refused = tap(&late, now).await.unwrap_err();
        assert_eq!(
            refused.message,
            "That request lapsed: nothing was approved."
        );
        let waiting = ask("c-waiting", HEAD, HEAD, now - 1);
        link.expire();
        assert!(link.item(&waiting.id).unwrap().done.is_some());
        // Revoked: a tap approves nothing, and the revoke settles every open ask.
        let open = ask("c-open", HEAD, HEAD, now + 3600);
        let other = ask("c-other", HEAD, HEAD, now + 3600);
        link.lock().membership = Some(membership("revoked"));
        let refused = tap(&open, now).await.unwrap_err();
        assert_eq!(
            refused.message,
            "That request lapsed: nothing was approved."
        );
        link.settle_on_revoke();
        let settled = link.item(&other.id).unwrap();
        assert!(settled.done.is_some() && settled.answer.is_none());
        assert!(
            settled
                .text
                .ends_with("This Oby left its Obysphere: nothing was approved.")
        );

        // An ask obid made before the owner's tap was needed: Accept approves nothing, and the
        // ask comes back, the same id, as the Confirm only the owner's tap answers.
        link.lock().membership = Some(membership("joined"));
        let legacy = Item {
            id: approval::item_id("c-legacy"),
            kind: "obiRequest".into(),
            job: String::new(),
            title: "Approve ernesto's pull request on GitHub?".into(),
            text: String::new(),
            options: vec![
                inbox::ACCEPT.into(),
                inbox::ACCEPT_REMEMBER.into(),
                inbox::DECLINE.into(),
            ],
            details: None,
            recommended: None,
            created: now,
            done: None,
            later: None,
            answer: None,
            obi: Some(
                json!({"correlationId": "c-legacy", "from": "obi-ernesto", "fromOwner": "ernesto",
                             "type": approval::HOST_APPROVAL, "repo": "obi-x5-test/none", "pr": 12,
                             "baseBranch": "main", "headSha": HEAD, "expiresAt": time::format(now + 3600)}),
            ),
            plugin: None,
            arrived: None,
            confirm: None,
            start: None,
            github: None,
            watch: None,
        };
        link.lock().inbox.insert(legacy.id.clone(), legacy.clone());
        let accept = json!({"answer": inbox::ACCEPT}).to_string();
        let refused = link
            .answer(&legacy.id, accept.as_bytes())
            .await
            .unwrap_err();
        assert_eq!(refused.status, StatusCode::FORBIDDEN);
        let now_asked = link.item(&legacy.id).unwrap();
        assert_eq!(now_asked.kind, "confirm");
        assert!(now_asked.done.is_none());
        assert_eq!(
            now_asked.confirm.unwrap().action,
            inbox::action::APPROVE_ON_HOST
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn a_teammate_is_asked_once_and_never_after_changes_were_asked_for() {
        use approval::Teammate;
        let home = std::env::temp_dir().join(format!("obid-link-teammate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "wellington"}).to_string(),
        )
        .unwrap();
        let events = Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (_stop, stopping) = watch::channel(false);
        let link = Link::open(config, jobs, events, stopping);
        let head = "6d20146f366b9fac8907b5066c30ee39aaec09b3";
        let change = approval::Change {
            repo: "acme/app",
            pr: 12,
            base: "dev",
            head_branch: "oby/fix-12",
            head,
            goal: "Fix the thing",
        };
        assert_eq!(
            link.ask_teammate(&change).await,
            Teammate::NeedsYou(
                "Oby isn't in an Obysphere, so no teammate's Oby can approve it. Invite a teammate who can push to acme/app to an Obysphere with you, or get their approval on GitHub yourself.".into()
            )
        );
        let member = |owner: &str| Member {
            owner: owner.into(),
            active: true,
            accepts: vec!["review_request".into()],
            seal: None,
            sealed_since: None,
        };
        let asked = |state: &str, result: Option<Value>| {
            let body = approval::request_body(&change);
            let request = json!({"id": "c-zoe", "correlation_id": "c-zoe", "type": "review_request",
                                 "expires_at": time::format(time::now() + 3600), "body": body});
            let mut e = Exchange::new("outgoing", request, "obi-zoe".into(), Some("zoe".into()));
            e.state = state.into();
            e.result = result;
            e
        };
        {
            let mut state = link.lock();
            state.membership = Some(Membership {
                state: "joined".into(),
                url: "http://127.0.0.1:1".into(),
                sphere: "team".into(),
                obi_id: "obi-wellington".into(),
                card: json!({}),
                joined_at: 0,
            });
            state
                .directory
                .members
                .insert("obi-wellington".into(), member("wellington"));
        }
        assert_eq!(
            link.ask_teammate(&change).await,
            Teammate::NeedsYou("No teammate's Oby in the Obysphere takes reviews. Invite a teammate who can push to acme/app to your Obysphere, or get their approval on GitHub yourself.".into()),
            "never itself"
        );
        let set = |e: Exchange| {
            let mut state = link.lock();
            state
                .directory
                .members
                .insert("obi-zoe".into(), member("zoe"));
            state.exchanges.insert(e.correlation_id.clone(), e);
        };
        set(asked("accepted", None));
        assert_eq!(
            link.ask_teammate(&change).await,
            Teammate::Waiting(
                "Waiting for zoe's Oby to review it and approve it on GitHub.".into()
            )
        );
        set(asked("done", Some(json!({"outcome": "changes_requested"}))));
        assert_eq!(
            link.ask_teammate(&change).await,
            Teammate::NeedsYou("zoe's Oby reviewed it and asked for changes.".into())
        );
        set(asked(
            "done",
            Some(json!({"outcome": "approved", "host_review": "unable"})),
        ));
        assert_eq!(
            link.ask_teammate(&change).await,
            Teammate::NeedsYou("No teammate's Oby could approve it on GitHub. Invite a teammate who can push to acme/app to your Obysphere, or get their approval on GitHub yourself.".into()),
            "zoe couldn't, and nobody else can"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn a_save_keeps_and_tells_what_is_there_now() {
        let home = std::env::temp_dir().join(format!("obid-link-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "wellington"}).to_string(),
        )
        .unwrap();
        let events = Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (_stop, stopping) = watch::channel(false);
        let link = Link::open(config, jobs, events.clone(), stopping);
        let mut heard = events.subscribe();

        let request = json!({"id": "c-1", "correlation_id": "c-1", "type": "review_request",
            "body": {"repo": "o/r"}});
        let mut arrived = Exchange::new("incoming", request.clone(), "obi-ernesto".into(), None);
        arrived.state = "inbox".into();
        let item = inbox::review_request_item(
            "c-1",
            "obi-ernesto",
            "ernesto",
            &request["body"],
            "2099-01-01T09:00:00Z",
            time::now(),
        );
        let mut answered = item.clone();
        answered.done = Some(time::now());
        answered.answer = Some(inbox::DECLINE.into());
        let mut declined = arrived.clone();
        declined.state = "declined".into();
        {
            let mut state = link.lock();
            state.inbox.insert(item.id.clone(), answered.clone());
            state
                .exchanges
                .insert(declined.correlation_id.clone(), declined.clone());
        }
        // The answer saves first; the arrival, slower, saves its older copies after.
        link.save_item(&answered);
        link.save_exchange(&declined);
        link.save_item(&item);
        link.save_exchange(&arrived);

        let mut told = Vec::new();
        while let Ok(event) = heard.try_recv() {
            told.push((event.name, event.data["state"].clone()));
        }
        assert_eq!(
            told,
            [
                ("inbox", json!("done")),
                ("exchange", json!("declined")),
                ("inbox", json!("done")),
                ("exchange", json!("declined")),
            ]
        );
        let kept = Inbox::open(&home).load_all();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].answer.as_deref(), Some(inbox::DECLINE));
        let kept = Exchanges::open(&home).load_all();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].state, "declined");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A link, its home under `name`: with a membership in `kept` (`joined`, `revoked`) when
    /// given, as kept from before a restart.
    fn sender_link(
        name: &str,
        kept: Option<&str>,
    ) -> (Arc<Link>, std::path::PathBuf, watch::Sender<bool>) {
        let home = std::env::temp_dir().join(format!("obid-link-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        if let Some(state) = kept {
            Files::new(&home)
                .save_membership(&membership(state))
                .unwrap();
        }
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "wellington"}).to_string(),
        )
        .unwrap();
        let events = Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (stop, stopping) = watch::channel(false);
        let link = Link::open(config, jobs, events, stopping);
        (link, home, stop)
    }

    fn membership(state: &str) -> Membership {
        Membership {
            state: state.into(),
            url: "http://127.0.0.1:1".into(),
            sphere: "s".into(),
            obi_id: "obi-wellington".into(),
            card: json!({}),
            joined_at: time::now(),
        }
    }

    /// The app reads the delegation's scope and end, to sign the next before it ends (#1215); never
    /// the JWT.
    #[tokio::test]
    async fn the_membership_says_when_the_delegation_ends() {
        let (link, home, _stop) = sender_link("delegation-ends", Some("joined"));
        assert!(link.membership_json()["delegation"].is_null());
        let key = Keypair::from_seed(&[7; 32]);
        let jwt = obi_protocol::jwt::sign(
            &json!({"scope": "receive reply", "exp": 1_900_000_000}),
            &key,
        )
        .ok()
        .unwrap();
        link.lock().delegation = Some(jwt.clone());
        let shown = link.membership_json();
        assert_eq!(
            shown["delegation"],
            json!({"scope": "receive reply", "expiresAt": time::format(1_900_000_000)})
        );
        assert!(!shown.to_string().contains(&jwt));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Out of a Sphere, start() leaves the sender and the clock off; with a membership, joined or
    /// revoked, start() starts them.
    #[tokio::test]
    async fn the_sender_starts_with_a_membership() {
        for (name, kept, running) in [
            ("sender-out", None, false),
            ("sender-in", Some("joined"), true),
            ("sender-revoked", Some("revoked"), true),
        ] {
            let (link, home, _stop) = sender_link(name, kept);
            link.start();
            assert_eq!(
                link.sender_running.load(Ordering::SeqCst),
                running,
                "{name}"
            );
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    /// Leaving forgets the Sphere, revoked or not: its folder and the Obi key go, the next key is a
    /// new one, and nothing refuses a join any more (#1214).
    #[tokio::test]
    async fn leaving_forgets_the_sphere_and_the_obi_key() {
        let (link, home, _stop) = sender_link("leave", Some("revoked"));
        let before = link.make_key().await.ok().unwrap()["keyId"].clone();
        ChainKept::default().save(&home);
        assert!(home.join("sphere/membership.json").is_file());
        assert!(home.join("keys/obi.key").is_file());

        let mut exchange = Exchange::new(
            "outgoing",
            json!({"correlation_id": "c-waiting", "type": "question"}),
            "obi-ernesto".into(),
            None,
        );
        exchange.outbox.push(json!({"id": "m-1"}));
        link.lock().exchanges.insert("c-waiting".into(), exchange);

        let left = link.leave().await.ok().unwrap();
        assert_eq!(left["state"], "none");
        // What waited to go to that Sphere never goes to the next.
        assert!(link.lock().exchanges["c-waiting"].outbox.is_empty());
        assert!(!link.has_membership());
        assert!(!home.join("sphere").exists());
        assert!(!home.join("keys/obi.key").exists());
        assert_ne!(link.make_key().await.ok().unwrap()["keyId"], before);
        // Again, with nothing to leave: the same answer.
        assert_eq!(link.leave().await.ok().unwrap()["state"], "none");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Until obid joins, the owner it would join as is the GitHub login when config's owner is
    /// still the Mac's login name; an owner set by hand stays, and so does the one it joined as.
    #[tokio::test]
    async fn until_it_joins_the_owner_is_the_github_login() {
        let login = nix::unistd::User::from_uid(nix::unistd::getuid())
            .unwrap()
            .unwrap()
            .name;
        let home = std::env::temp_dir().join(format!("obid-link-owner-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": login}).to_string(),
        )
        .unwrap();
        let events = Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (_stop, stopping) = watch::channel(false);
        let link = Link::open(config, jobs, events, stopping);
        link.adopt_github_owner("not_a_slug");
        assert_eq!(link.owner(), login);
        link.adopt_github_owner("WellPrado");
        assert_eq!(link.owner(), "wellprado");
        assert_eq!(link.membership_json()["owner"], "wellprado");
        let _ = std::fs::remove_dir_all(&home);

        // An owner set by hand ("wellington", not the login name) stays.
        let (link, home, _stop) = sender_link("owner-set", None);
        link.adopt_github_owner("wellprado");
        assert_eq!(link.owner(), "wellington");
        let _ = std::fs::remove_dir_all(&home);

        // A membership kept: the owner its card names, whatever config says; kept in `home/owner`,
        // and still the owner after a leave, and after a restart (#1214).
        let home = std::env::temp_dir().join(format!("obid-link-owner-in-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let mut kept = membership("joined");
        kept.card = json!({"capabilities": {"extensions": [{"params": {"owner": "carol"}}]}});
        Files::new(&home).save_membership(&kept).unwrap();
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": login}).to_string(),
        )
        .unwrap();
        let events = Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (_stop, stopping) = watch::channel(false);
        let link = Link::open(config, jobs, events, stopping);
        link.adopt_github_owner("wellprado");
        assert_eq!(link.owner(), "carol");
        assert_eq!(Files::new(&home).load_owner().as_deref(), Some("carol"));
        link.leave().await.ok().unwrap();
        link.adopt_github_owner("wellprado");
        assert_eq!(link.owner(), "carol");
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": login}).to_string(),
        )
        .unwrap();
        let events = Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (_stop, stopping) = watch::channel(false);
        let again = Link::open(config, jobs, events, stopping);
        again.adopt_github_owner("wellprado");
        assert_eq!(again.owner(), "carol");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A review request from obi-ernesto, `cid` its id, expiring at `expires_at`.
    fn review_request(cid: &str, expires_at: &str) -> Value {
        json!({"id": cid, "correlation_id": cid, "type": "review_request", "from": "obi-ernesto",
            "body": {"repo": "o/r"}, "expires_at": expires_at})
    }

    /// Puts in `link` what can be open at a revoke: a review request in the inbox (`c-open`, its
    /// item's id answered), a question being answered by itself (`c-answering`), an accepted review
    /// whose job runs (`c-accepted`), one whose result waits to be sent (`c-result`), and one this
    /// Obi sent (`c-sent`, expiring at `sent_expires`).
    fn open_exchanges(link: &Link, sent_expires: &str) -> String {
        let later = "2099-01-01T09:00:00Z";
        let item = inbox::review_request_item(
            "c-open",
            "obi-ernesto",
            "ernesto",
            &review_request("c-open", later)["body"],
            later,
            time::now(),
        );
        let exchange = |direction: &str, cid: &str, state: &str, expires: &str| {
            let mut e = Exchange::new(
                direction,
                review_request(cid, expires),
                "obi-ernesto".into(),
                Some("ernesto".into()),
            );
            e.state = state.into();
            e
        };
        let mut open = exchange("incoming", "c-open", "inbox", later);
        open.inbox_item = Some(item.id.clone());
        let answering = exchange("incoming", "c-answering", "answering", later);
        let mut accepted = exchange("incoming", "c-accepted", "accepted", later);
        accepted.job = Some("job-of-c-accepted".into());
        let mut result = exchange("incoming", "c-result", "done", later);
        result
            .outbox
            .push(json!({"id": "r-1", "type": "result", "expires_at": later}));
        let sent = exchange("outgoing", "c-sent", "sent", sent_expires);
        let mut state = link.lock();
        state.inbox.insert(item.id.clone(), item.clone());
        for e in [open, answering, accepted, result, sent] {
            state.exchanges.insert(e.correlation_id.clone(), e);
        }
        item.id
    }

    /// Each open incoming request is settled as a revoke settles it: `declined`, ended, saying why,
    /// with no reply built.
    fn settled_at_revoke(link: &Link, item: &str) {
        let state = link.lock();
        for cid in ["c-open", "c-answering", "c-accepted"] {
            let e = &state.exchanges[cid];
            assert_eq!(e.state, "declined", "{cid}");
            assert!(e.ended(), "{cid}");
            assert_eq!(e.error.as_deref(), Some(REVOKED), "{cid}");
            assert!(e.outbox.is_empty(), "no reply for {cid}");
        }
        assert!(state.inbox[item].done.is_some());
    }

    /// The vendors obid's card declares are those whose models write what it sends another member:
    /// Claude Code's always, and Codex's when the conversation runs on Codex.
    #[test]
    fn the_vendors_follow_the_conversations_agent() {
        let home = std::env::temp_dir();
        let parse = |extra: Value| {
            let mut c = json!({"listen": "127.0.0.1:0", "home": home, "owner": "wellington"});
            for (k, v) in extra.as_object().unwrap() {
                c[k] = v.clone();
            }
            Config::parse(&c.to_string()).unwrap()
        };
        assert_eq!(vendors(&parse(json!({}))), ["anthropic"]);
        assert_eq!(
            vendors(&parse(json!({"conversation": {"agent": "codex"}}))),
            ["anthropic", "openai"]
        );
        assert_eq!(
            vendors(&parse(json!({"conversation": {"agent": "plan"}}))),
            ["anthropic"]
        );
    }

    /// The version obid's card says is its runtime's, with `+card-runtime` as its build metadata:
    /// the two constants move together.
    #[test]
    fn the_cards_version_is_the_runtimes_with_card_runtime() {
        assert_eq!(OBID_VERSION, format!("{RUNTIME_VERSION}+card-runtime"));
        assert!(reads_seal_keys(OBID_VERSION));
        assert!(obi_protocol::identity::takes_card_runtime(
            &json!({"version": OBID_VERSION})
        ));
    }

    /// A card that carries obid's runtime is signed again when it says other vendors than obid
    /// would now (its owner moved the conversation to Codex, say), another runtime version, or
    /// other features.
    #[tokio::test]
    async fn a_card_with_stale_vendors_is_signed_again() {
        let (link, _home, _stop) = sender_link("stale-vendors", Some("joined"));
        let mut state = link.lock();
        let card = |runtime_version: &str, features: Value| {
            json!({"version": OBID_VERSION, "capabilities": {"extensions": [{"params": {
                "features": features,
                "runtime": {"id": RUNTIME_ID, "version": runtime_version},
                "vendors": ["anthropic"],
            }}]}})
        };
        state.membership.as_mut().unwrap().card = card(RUNTIME_VERSION, json!(FEATURES));
        assert!(!runtime_card_needed(&state, &["anthropic"]));
        assert!(runtime_card_needed(&state, &["anthropic", "openai"]));
        state.membership.as_mut().unwrap().card = card("0.2.9", json!(FEATURES));
        assert!(runtime_card_needed(&state, &["anthropic"]));
        // A card from an obid that took fewer features is signed again too.
        state.membership.as_mut().unwrap().card = card(RUNTIME_VERSION, json!(["card_runtime"]));
        assert!(runtime_card_needed(&state, &["anthropic"]));
    }

    /// Leaving a Sphere forgets what obid learnt of it and its team, so the next one is asked again.
    #[tokio::test]
    async fn leaving_forgets_what_the_sphere_and_its_team_take() {
        let (link, _home, _stop) = sender_link("leave-flags", Some("joined"));
        {
            let mut state = link.lock();
            state.sphere_seals = true;
            state.team_reads_seal_keys = true;
            state.sphere_takes_card_runtime = true;
            state.team_takes_card_runtime = true;
            state.features_read = 1;
            state.features = Some((Instant::now(), vec!["sphere_url_aud".into()]));
            state.url_aud_refused = true;
        }
        link.leave().await.unwrap();
        let state = link.lock();
        assert!(!state.sphere_seals && !state.team_reads_seal_keys);
        assert!(!state.sphere_takes_card_runtime && !state.team_takes_card_runtime);
        assert_eq!(state.features_read, 0);
        assert!(state.features.is_none() && !state.url_aud_refused);
    }

    /// The directory says obid's membership was revoked: what's open incoming is settled with no
    /// reply, what waited to be sent never goes, nothing more is built for the accepted review, a
    /// request delivered after it never reaches the inbox, and what this Obi sent is left to the
    /// clock.
    #[tokio::test]
    async fn after_a_revoke_nothing_comes_in_or_goes_out() {
        let (link, home, _stop) = sender_link("revoke", Some("joined"));
        // The membership's card accepts review requests: a late one would reach the inbox.
        let owner = Keypair::from_seed_string("link test owner");
        let obi = Keypair::from_seed_string("link test obi");
        let (owner_key, key_id, public_key) = (owner.public_key(), obi.key_id(), obi.public_key());
        let card = obi_protocol::card::sign(
            &obi_protocol::identity::unsigned_card(&obi_protocol::identity::CardFields {
                sphere_url: "http://127.0.0.1:1",
                sphere: "s",
                obi_id: "obi-wellington",
                owner: "wellington",
                owner_key: &owner_key,
                name: "Wellington's Obi",
                description: obi_protocol::identity::DEFAULT_DESCRIPTION,
                version: "0.1.0",
                key_id: &key_id,
                public_key: &public_key,
                accepts: &["review_request".to_owned()],
                card_seq: 1,
                issued_at: &time::format(time::now()),
                seal: None,
                runtime: None,
            }),
            &owner,
        )
        .unwrap();
        // A Sphere whose directory lists this Obi revoked, and that counts what's sent to it.
        let me = json!({"obi": "obi-wellington", "owner": "wellington", "owner_key": owner_key,
            "owner_key_id": obi_protocol::keys::key_id_of(&owner_key), "status": "revoked",
            "joined_at": "2026-10-01T09:00:00Z", "revoked_at": "2026-10-24T09:00:00Z",
            "card": card, "keys": [{"key_id": key_id, "public_key": public_key,
                                    "valid_from_seq": 1, "valid_until_seq": null}]});
        let sends = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = sends.clone();
        let sphere = axum::Router::new()
            .route(
                "/v0.1/spheres/s/members",
                axum::routing::get(move || {
                    let members = json!({"sphere": "s", "members": [me.clone()]});
                    async move { axum::Json(members) }
                }),
            )
            .route(
                "/v0.1/spheres/s/messages",
                axum::routing::post(move || {
                    counted.fetch_add(1, Ordering::SeqCst);
                    async { StatusCode::SERVICE_UNAVAILABLE }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, sphere).await.unwrap() });
        {
            let mut state = link.lock();
            state.membership.as_mut().unwrap().card = card.clone();
            state.client = Some(Client::new(&url, "s").unwrap());
            state.token = Some(("token".into(), time::now() + 3600));
        }
        let item = open_exchanges(&link, "2099-01-01T09:00:00Z");

        link.read_directory().await.unwrap();
        assert_eq!(link.lock().membership.as_ref().unwrap().state, "revoked");
        settled_at_revoke(&link, &item);
        assert_eq!(link.lock().exchanges["c-sent"].state, "sent");

        // The accepted review's result, built before the revoke, doesn't go; the stopped job's end
        // builds nothing more.
        assert!(!link.flush("c-result").await);
        assert_eq!(link.lock().exchanges["c-result"].outbox.len(), 1);
        link.reply_failed("c-accepted", None).await;
        settled_at_revoke(&link, &item);
        assert_eq!(
            sends.load(Ordering::SeqCst),
            0,
            "nothing reached the Sphere"
        );

        // A request delivered after the revoke, from a member the directory shows active.
        link.lock().directory.members.insert(
            "obi-ernesto".into(),
            Member {
                owner: "ernesto".into(),
                active: true,
                accepts: vec!["review_request".into()],
                seal: None,
                sealed_since: None,
            },
        );
        let inbox_before = link.lock().inbox.len();
        link.handle_request(Delivery {
            delivery_id: "d-late".into(),
            seq: 9,
            relayed_at: time::format(time::now()),
            envelope: review_request("c-late", "2099-01-01T09:00:00Z"),
        });
        let state = link.lock();
        assert!(!state.exchanges.contains_key("c-late"));
        assert_eq!(state.inbox.len(), inbox_before);
        drop(state);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// obid starts with its membership revoked: what was still open is settled as at the revoke,
    /// and the clock still runs, so what this Obi sent expires.
    #[tokio::test]
    async fn a_start_revoked_settles_what_is_open_and_the_clock_runs() {
        let (link, home, _stop) = sender_link("start-revoked", Some("revoked"));
        let item = open_exchanges(&link, "2020-01-01T09:00:00Z");
        link.start();
        settled_at_revoke(&link, &item);
        tokio::time::timeout(Duration::from_secs(5), async {
            while link.lock().exchanges["c-sent"].state != "expired" {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the clock expires what this Obi sent");
        assert!(link.lock().exchanges["c-sent"].outbox.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A key file obid can't read starts obid all the same, says why in `lastError`, and no new key
    /// is made over it: the Obi's identity and its paired phones stay.
    #[tokio::test]
    async fn an_unreadable_key_starts_obid_but_is_never_replaced() {
        let home = std::env::temp_dir().join(format!("obid-link-bad-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("keys")).unwrap();
        std::fs::write(home.join("keys/obi.key"), "not a key").unwrap();
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "wellington"}).to_string(),
        )
        .unwrap();
        let events = Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (_stop, stopping) = watch::channel(false);
        let link = Link::open(config, jobs, events, stopping);
        let said = link.membership_json()["lastError"].clone();
        assert!(
            said.as_str().unwrap().contains("can't read its Obi key"),
            "{said}"
        );
        let refused = link.make_key().await.err().unwrap();
        assert_eq!(refused.status, StatusCode::CONFLICT);
        assert_eq!(
            std::fs::read_to_string(home.join("keys/obi.key")).unwrap(),
            "not a key"
        );
        assert!(!home.join("keys/seal.key").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A Start on an offer past its day closes it under the lock instead, even when nothing read
    /// the inbox since; a fresh offer is left to start.
    #[test]
    fn a_start_on_an_expired_offer_closes_it() {
        let confirmed = inbox::Confirmed {
            action: String::new(),
            target: String::new(),
            payload: Value::Null,
        };
        let mut item = inbox::confirm_item("", "Greeting".into(), "Start it?".into(), confirmed, 0);
        item.kind = "start".into();
        item.confirm = None;
        item.start = Some(Box::new(json!({"repo": "o/app", "offer": {"id": "t1"}})));
        let id = item.id.clone();
        let mut inbox = HashMap::from([(id.clone(), item.clone())]);
        assert!(super::close_expired(&mut inbox, &id, super::OFFER_SECONDS - 1).is_none());
        assert_eq!(inbox[&id], item);
        let closed = super::close_expired(&mut inbox, &id, super::OFFER_SECONDS).unwrap();
        assert_eq!(closed.done, Some(super::OFFER_SECONDS));
        assert!(closed.text.starts_with(super::OFFER_EXPIRED) && closed.options.is_empty());
        assert_eq!(inbox[&id], closed);
        assert!(super::close_expired(&mut inbox, "gone", super::OFFER_SECONDS).is_none());
    }

    /// An offer (`oby_offer`) closes a day after it was made, its words kept under obid's own
    /// sentence and nothing left to answer; any other `start` item stays as it was.
    #[test]
    fn an_offer_closes_after_a_day() {
        let confirmed = inbox::Confirmed {
            action: String::new(),
            target: String::new(),
            payload: Value::Null,
        };
        let mut item = inbox::confirm_item("", "Greeting".into(), "Start it?".into(), confirmed, 0);
        item.kind = "start".into();
        item.confirm = None;
        item.start = Some(Box::new(json!({"repo": "o/app"})));
        assert!(!super::expired(&item, super::OFFER_SECONDS));
        item.start = Some(Box::new(json!({"repo": "o/app", "offer": {"id": "t1"}})));
        assert!(!super::expired(&item, super::OFFER_SECONDS - 1));
        assert!(super::expired(&item, super::OFFER_SECONDS));
        let closed = super::closed_start(item, super::OFFER_EXPIRED, 7);
        assert_eq!(
            closed.text,
            format!("{}\n\nStart it?", super::OFFER_EXPIRED)
        );
        assert!(closed.options.is_empty() && closed.details.is_none());
        assert_eq!(closed.done, Some(7));
    }
}
