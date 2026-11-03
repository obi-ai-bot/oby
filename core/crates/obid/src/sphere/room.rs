//! The Sphere's room (docs/contracts/rooms.md; spec `protocol.md` section 9.2, `sphere-api.md` section
//! 6.5): what a post is, sealing one and reading one, and what obid keeps of the room, in
//! `home/sphere/room.json` (0600), which goes when obid leaves its Sphere. The link's side, following
//! and sending, is in `link/room.rs`.
//!
//! A post's words are data: they reach a model only as a quoted file, and no post starts, changes or
//! publishes anything.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use obi_protocol::keys::{self, Keypair};
use obi_protocol::seal::{self, SealKey};
use obi_protocol::{CLOCK_SKEW_SECONDS, envelope, jcs, json as strict, schema, time};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::store;

/// A post's closed kinds.
pub const KINDS: [&str; 5] = ["working_on", "done", "found", "need", "say"];
/// The longest a post's words may be, in characters.
pub const MAX_TEXT: usize = 2000;
/// The longest a whole post may be, its copies included.
pub const MAX_BYTES: usize = 131_072;
/// What a post's words are sealed under (`protocol.md` section 9.2).
const COPY_TYPE: &str = "room_post";
/// How many posts obid keeps. ponytail: the whole file is written for each change, so 1,000 and not
/// the Sphere's 5,000; an append-only log if a room outgrows that.
const KEPT: usize = 1000;

/// Not sent yet: it waits in the queue, its signed bytes in `wire`.
pub const WAITING: &str = "waiting";
/// The Sphere has it.
pub const SENT: &str = "sent";
/// The Sphere won't take it, now or later: `error` says why.
pub const REFUSED: &str = "refused";

/// A post as obid keeps it, words opened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub id: String,
    pub from: String,
    pub owner: String,
    pub kind: String,
    pub by: String,
    pub created_at: String,
    #[serde(default)]
    pub reply_to: Option<String>,
    pub text: String,
    #[serde(default)]
    pub about: Vec<Value>,
    /// The Sphere's place for it, once it has it.
    #[serde(default)]
    pub seq: Option<u64>,
    #[serde(default)]
    pub recorded_at: Option<String>,
    pub state: String,
    #[serde(default)]
    pub error: Option<String>,
    /// The job it's about: its `outgoing` lists it once sent. Never in the API's answer.
    #[serde(default)]
    pub job: Option<String>,
    /// The post as signed, while it waits: a retry sends these same bytes. Never in the API's answer.
    #[serde(default)]
    pub wire: Option<Value>,
    /// The sharing rule it went out by itself under, none for what the owner said or asked for: it
    /// must still hold when it's sent. Never in the API's answer.
    #[serde(default)]
    pub under: Option<Under>,
}

/// Which sharing rule a post obid wrote by itself goes under.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Under {
    /// The repositories it tells of.
    pub repos: Vec<String>,
    /// Whose `need` it answers: none for a job's post.
    #[serde(default)]
    pub asker: Option<String>,
}

impl Post {
    /// As the runner API shows it.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "seq": self.seq,
            "recordedAt": self.recorded_at,
            "from": self.from,
            "owner": self.owner,
            "kind": self.kind,
            "by": self.by,
            "createdAt": self.created_at,
            "replyTo": self.reply_to,
            "text": self.text,
            "about": self.about,
            "state": self.state,
            "error": self.error,
        })
    }
}

/// What obid keeps of the room, and what it knows of the Sphere's.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Room {
    /// The highest `seq` read.
    #[serde(default)]
    pub after: u64,
    #[serde(default)]
    pub posts: Vec<Post>,
    /// Whose `need` obid took on to answer by itself, and when, for the last hour: what the
    /// self-answer limits count, also after a restart, whatever posts have gone since.
    #[serde(default)]
    pub self_answers: Vec<(String, i64)>,
    /// Whether the Sphere has a room (`404`: it doesn't), once obid asked.
    #[serde(skip)]
    pub available: Option<bool>,
    /// Which membership this is for: a read or a post under way when obid left its Sphere finds
    /// another here, and keeps nothing.
    #[serde(skip)]
    pub epoch: u64,
    /// obid left its Sphere, and hasn't joined another: nothing is written.
    #[serde(skip)]
    pub closed: bool,
}

fn path(home: &Path) -> PathBuf {
    home.join("sphere").join("room.json")
}

impl Room {
    /// What's kept, or an empty room. A file that doesn't read loses only the room's history: obid
    /// reads on from the start of what the Sphere keeps.
    pub fn open(home: &Path) -> Room {
        std::fs::read(path(home))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// It's for no membership now: `home/sphere` went, and nothing brings it back.
    pub fn close(&mut self) {
        *self = Room {
            epoch: self.epoch + 1,
            closed: true,
            ..Room::default()
        };
    }

    /// Keeps it, the oldest posts past [`KEPT`] gone, never one still waiting.
    pub fn save(&mut self, home: &Path) {
        if self.closed {
            return;
        }
        let since = time::now() - 3600;
        self.self_answers.retain(|(_, at)| *at > since);
        let over = self.posts.len().saturating_sub(KEPT);
        if over > 0 {
            let mut left = over;
            self.posts.retain(|p| {
                let gone = left > 0 && p.state != WAITING;
                left -= usize::from(gone);
                !gone
            });
        }
        let saved = path(home)
            .parent()
            .map_or(Ok(()), store::make_private_dir)
            .and_then(|()| {
                let text = serde_json::to_vec(&self).map_err(std::io::Error::other)?;
                store::write_private(&path(home), &text)
            });
        if let Err(e) = saved {
            tracing::error!("can't keep the room: {e}");
        }
    }

    /// The posts above `after`, and every one not sent yet: in the Sphere's order, then those.
    pub fn since(&self, after: u64) -> Vec<&Post> {
        let mut posts: Vec<&Post> = self
            .posts
            .iter()
            .filter(|p| p.seq.is_none_or(|seq| seq > after))
            .collect();
        posts.sort_by_key(|p| p.seq.unwrap_or(u64::MAX));
        posts
    }

    /// Whether a post of job `job`'s, of this kind, is kept.
    pub fn job_posted(&self, job: &str, kind: &str) -> bool {
        self.posts
            .iter()
            .any(|p| p.kind == kind && p.job.as_deref() == Some(job))
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Post> {
        self.posts.iter_mut().find(|p| p.id == id)
    }
}

/// A new post: what the words are, before they're sealed.
#[derive(Debug, Clone)]
pub struct New<'a> {
    pub kind: &'a str,
    pub by: &'a str,
    pub text: &'a str,
    pub about: &'a [Value],
    pub reply_to: Option<&'a str>,
}

/// What a copy's seal opens to: `{"text", "about"?}`, as canonical JSON's value.
fn words(text: &str, about: &[Value]) -> Value {
    let mut words = json!({"text": text});
    if !about.is_empty() {
        words["about"] = json!(about);
    }
    words
}

/// Why `about` can't go with a post, when it can't.
pub fn about_problem(about: &[Value]) -> Option<&'static str> {
    (!about.is_empty() && !schema::is_valid("common#/$defs/refs", &json!(about)))
        .then_some("about is a list of at most 10 references: repo, pr, issue or job.")
}

/// The header each copy's key is bound to (`protocol.md` section 9.2).
fn header(sphere: &str, id: &str, from: &str, to: &str) -> Map<String, Value> {
    let Value::Object(header) =
        json!({"sphere": sphere, "id": id, "from": from, "to": to, "type": COPY_TYPE})
    else {
        unreachable!("a header is an object")
    };
    header
}

/// The post, its words sealed for each of `members` (Obi id and seal key) apart, signed by `key`.
pub fn sealed(
    new: &New,
    id: &str,
    sphere: &str,
    from: &str,
    members: &[(String, [u8; 32])],
    key: &Keypair,
    now: i64,
) -> Result<Value, String> {
    let opened = jcs::canonical_bytes(&words(new.text, new.about)).map_err(|e| e.to_string())?;
    let opened = String::from_utf8(opened).map_err(|e| e.to_string())?;
    let copies = members
        .iter()
        .map(|(to, seal_key)| {
            let (seal, text) = seal::seal_text(&header(sphere, id, from, to), seal_key, &opened)?;
            Ok(json!({"to": to, "seal": seal, "text": text}))
        })
        .collect::<Result<Vec<Value>, String>>()?;
    let mut unsigned = json!({
        "id": id, "sphere": sphere, "from": from, "kind": new.kind, "by": new.by,
        "created_at": time::format(now), "key_id": key.key_id(), "copies": copies,
    });
    if let Some(reply_to) = new.reply_to {
        unsigned["reply_to"] = json!(reply_to);
    }
    let Value::Object(unsigned) = unsigned else {
        unreachable!("a post is an object")
    };
    envelope::sign(&unsigned, key).map_err(|e| e.to_string())
}

/// `wire` signed again with `key`, its words and id as they were (the Sphere: `unknown_key`, a key
/// that rotated while it waited).
pub fn signed_again(wire: &Value, key: &Keypair) -> Result<Value, String> {
    let mut unsigned = wire.as_object().cloned().ok_or("a post is an object")?;
    unsigned.insert("key_id".into(), json!(key.key_id()));
    envelope::sign(&unsigned, key).map_err(|e| e.to_string())
}

/// Why this post, as the Sphere served it, isn't to be read: its shape, its Sphere, its date, or its
/// signature by `public_key` (the key its `from`'s trusted card signs, by `key_id`). Names checks,
/// never words.
pub fn problem(
    post: &Value,
    sphere: &str,
    now: i64,
    public_key: impl Fn(&str, &str) -> Option<String>,
) -> Option<&'static str> {
    let Some(map) = post.as_object() else {
        return Some("it isn't an object");
    };
    const FIELDS: [&str; 10] = [
        "id",
        "sphere",
        "from",
        "kind",
        "by",
        "created_at",
        "reply_to",
        "key_id",
        "copies",
        "signature",
    ];
    if map.keys().any(|k| !FIELDS.contains(&k.as_str())) {
        return Some("a field a post doesn't have");
    }
    let text = |key: &str| post[key].as_str().unwrap_or("");
    let valid = |def: &str, value: &Value| schema::is_valid(&format!("common#/$defs/{def}"), value);
    if !valid("uuid", &post["id"])
        || !valid("obi_id", &post["from"])
        || !valid("timestamp", &post["created_at"])
        || !valid("key_id", &post["key_id"])
        || !valid("signature", &post["signature"])
    {
        return Some("a field isn't what it must be");
    }
    if map.contains_key("reply_to") && !valid("uuid", &post["reply_to"]) {
        return Some("its reply_to isn't a post's id");
    }
    if text("sphere") != sphere {
        return Some("it's for another Sphere");
    }
    if !KINDS.contains(&text("kind")) {
        return Some("a kind that isn't one");
    }
    let by = text("by");
    if !matches!(by, "obi" | "owner") || (text("kind") == "say" && by != "owner") {
        return Some("who wrote it can't be that");
    }
    let Some(copies) = post["copies"].as_array().filter(|c| !c.is_empty()) else {
        return Some("it has no copies");
    };
    let mut seen = std::collections::HashSet::new();
    for copy in copies {
        let shaped = copy.as_object().is_some_and(|c| {
            c.len() == 3 && c.contains_key("to") && c.contains_key("seal") && c.contains_key("text")
        }) && valid("obi_id", &copy["to"])
            && valid("seal", &copy["seal"])
            && valid("sealed_text", &copy["text"]);
        if !shaped {
            return Some("a copy that isn't one");
        }
        if !seen.insert(copy["to"].as_str()) {
            return Some("two copies for one member");
        }
    }
    let ahead = time::parse(text("created_at")).is_none_or(|t| t > now + CLOCK_SKEW_SECONDS);
    if ahead {
        return Some("created_at is ahead of the clock");
    }
    let Some(public_key) = public_key(text("from"), text("key_id")) else {
        return Some("its key isn't its sender's current one");
    };
    let signed = envelope::signing_input(post)
        .is_ok_and(|input| keys::verify_bytes(&public_key, &input, text("signature")));
    if !signed {
        return Some("its signature doesn't verify");
    }
    None
}

/// The words of `me`'s copy of a post that passed [`problem`]: `Ok(None)` when it has none (a member
/// who joined later, or with no seal key then) or it doesn't open with a key of `me`'s; `Err` when
/// what it opens to breaks the shape. Names checks, never words.
pub fn open(
    post: &Value,
    me: &str,
    own: &HashMap<String, SealKey>,
) -> Result<Option<(String, Vec<Value>)>, &'static str> {
    let copies = post["copies"].as_array().map_or(&[][..], Vec::as_slice);
    let Some(copy) = copies.iter().find(|c| c["to"] == me) else {
        return Ok(None);
    };
    let Some(key) = copy["seal"]["key_id"].as_str().and_then(|id| own.get(id)) else {
        return Ok(None);
    };
    let (sphere, id, from) = (
        post["sphere"].as_str().unwrap_or(""),
        post["id"].as_str().unwrap_or(""),
        post["from"].as_str().unwrap_or(""),
    );
    let Ok(opened) = seal::open_text(
        &header(sphere, id, from, me),
        &copy["seal"],
        copy["text"].as_str().unwrap_or(""),
        key,
    ) else {
        return Ok(None);
    };
    let Ok(value) = strict::from_str(&opened) else {
        return Err("it opens to something that isn't JSON");
    };
    if jcs::canonical_bytes(&value).ok().as_deref() != Some(opened.as_bytes()) {
        return Err("it opens to JSON that isn't canonical");
    }
    let shaped = value
        .as_object()
        .is_some_and(|m| m.keys().all(|k| k == "text" || k == "about"));
    let text = value["text"].as_str().unwrap_or("");
    if !shaped
        || text.is_empty()
        || text.chars().count() > MAX_TEXT
        || !schema::is_valid("common#/$defs/block_text", &value["text"])
    {
        return Err("its words aren't what a post's may be");
    }
    let about = match value.get("about") {
        None => Vec::new(),
        Some(about) => {
            let list = about.as_array().cloned().unwrap_or_default();
            if list.is_empty() || about_problem(&list).is_some() {
                return Err("its about isn't a list of references");
            }
            list
        }
    };
    Ok(Some((text.to_owned(), about)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> Keypair {
        Keypair::from_seed(&[7; 32])
    }

    fn new<'a>(kind: &'a str, by: &'a str) -> New<'a> {
        New {
            kind,
            by,
            text: "hello",
            about: &[],
            reply_to: None,
        }
    }

    const ID: &str = "8a6c1f4e-5d2b-4c7a-9e3f-1b2c3d4e5f60";

    fn post(members: &[(String, [u8; 32])]) -> Value {
        sealed(&new("need", "obi"), ID, "s", "obi-a", members, &key(), 1000).unwrap()
    }

    /// The problem [`problem`] finds in `post`, `obi-a`'s key being `key()`.
    fn found(post: &Value) -> Option<&'static str> {
        problem(post, "s", 1000, |from, id| {
            (from == "obi-a" && id == key().key_id()).then(|| key().public_key())
        })
    }

    /// `post` with `change` made, signed again by its sender: what only its signature can't catch.
    fn signed_with(post: &Value, change: impl Fn(&mut Value)) -> Value {
        let mut changed = post.clone();
        change(&mut changed);
        signed_again(&changed, &key()).unwrap()
    }

    /// ROOMS-8: a post that fails verification is dropped, whoever changed it and however.
    #[test]
    fn rooms_8_a_post_that_fails_verification_is_refused_by_name() {
        let me = SealKey::generate().unwrap();
        let good = post(&[("obi-b".into(), *me.public())]);
        assert_eq!(found(&good), None);
        // Changed under its signature: every member covers it.
        for (field, value) in [
            ("kind", json!("say")),
            ("by", json!("owner")),
            ("created_at", json!("2020-01-01T00:00:00Z")),
            ("from", json!("obi-c")),
            ("copies", json!([])),
        ] {
            let mut changed = good.clone();
            changed[field] = value;
            assert!(found(&changed).is_some(), "{field}");
        }
        // Signed by its sender, and still no post.
        for (change, why) in [
            (json!("yell"), "kind"),
            (json!("other-sphere"), "sphere"),
            (json!("somebody"), "by"),
            (json!("2030-01-01T00:00:00Z"), "created_at"),
        ] {
            let bad = signed_with(&good, |p| p[why] = change.clone());
            assert!(found(&bad).is_some(), "{why}");
        }
        assert!(found(&signed_with(&good, |p| p["extra"] = json!(1))).is_some());
        assert!(
            found(&signed_with(&good, |p| {
                let copy = p["copies"][0].clone();
                p["copies"] = json!([copy, copy]);
            }))
            .is_some(),
            "two copies for one member"
        );
        // `say` is the owner's alone.
        let said = signed_with(&good, |p| p["kind"] = json!("say"));
        assert!(found(&said).is_some());
        // An old key, or another's.
        let old = problem(&good, "s", 1000, |_, _| None);
        assert_eq!(old, Some("its key isn't its sender's current one"));
        let other = problem(&good, "s", 1000, |_, _| {
            Some(Keypair::from_seed(&[8; 32]).public_key())
        });
        assert_eq!(other, Some("its signature doesn't verify"));
    }

    /// A copy sealed by hand with `words` for `me`, on a post signed as usual.
    fn with_words(me: &SealKey, opened: &str) -> Value {
        let good = post(&[("obi-b".into(), *me.public())]);
        let (seal, text) =
            seal::seal_text(&header("s", ID, "obi-a", "obi-b"), me.public(), opened).unwrap();
        signed_with(&good, |p| {
            p["copies"] = json!([{"to": "obi-b", "seal": seal, "text": text}]);
        })
    }

    /// ROOMS-8: what a copy opens to must be a post's words: anything else is dropped.
    #[test]
    fn rooms_8_a_post_whose_opened_json_breaks_the_shape_is_dropped() {
        let me = SealKey::generate().unwrap();
        let own: HashMap<String, SealKey> = [(me.key_id(), me.clone())].into();
        let good = with_words(&me, r#"{"text":"hi"}"#);
        assert_eq!(
            open(&good, "obi-b", &own),
            Ok(Some(("hi".to_owned(), Vec::new())))
        );
        let long = format!(r#"{{"text":"{}"}}"#, "x".repeat(MAX_TEXT + 1));
        for bad in [
            "not json",
            r#"{"text":"hi","text":"twice"}"#,
            r#"{ "text":"not canonical" }"#,
            r#"{"prompt":"x","text":"hi"}"#,
            r#"{"text":""}"#,
            r#"{"text":7}"#,
            r#"{"about":[],"text":"hi"}"#,
            r#"{"about":[{"kind":"url","url":"x"}],"text":"hi"}"#,
            "{\"text\":\"a\\u202eb\"}",
            long.as_str(),
        ] {
            assert!(open(&with_words(&me, bad), "obi-b", &own).is_err(), "{bad}");
        }
    }

    /// ROOMS-15: a member with no copy, or a copy that won't open, reads nothing and no error.
    #[test]
    fn rooms_15_a_member_with_no_copy_or_one_that_wont_open_skips_it() {
        let (them, me) = (SealKey::generate().unwrap(), SealKey::generate().unwrap());
        let own: HashMap<String, SealKey> = [(me.key_id(), me.clone())].into();
        let theirs = post(&[("obi-b".into(), *them.public())]);
        assert_eq!(open(&theirs, "obi-late", &own), Ok(None), "no copy");
        assert_eq!(open(&theirs, "obi-b", &own), Ok(None), "another key's");
        // Another member's copy put under `me`'s name doesn't open: its key is bound to its `to`.
        let moved = {
            let good = post(&[("obi-c".into(), *me.public())]);
            signed_with(&good, |p| p["copies"][0]["to"] = json!("obi-b"))
        };
        assert_eq!(open(&moved, "obi-b", &own), Ok(None));
    }

    /// The Sphere's side reads what this seals (`spec/protocol.md` section 9.2): the signing input is
    /// an envelope's, and a copy's words are one canonical JSON value.
    #[test]
    fn a_post_is_signed_as_an_envelope_and_sealed_for_each_member_apart() {
        let (a, b) = (SealKey::generate().unwrap(), SealKey::generate().unwrap());
        let about = [json!({"kind": "repo", "repo": "acme/app"})];
        let made = sealed(
            &New {
                about: &about,
                reply_to: Some(ID),
                ..new("found", "obi")
            },
            ID,
            "s",
            "obi-a",
            &[("obi-a".into(), *a.public()), ("obi-b".into(), *b.public())],
            &key(),
            1000,
        )
        .unwrap();
        assert_eq!(found(&made), None);
        assert_eq!(made["reply_to"], ID);
        assert!(!made.to_string().contains("hello"));
        assert_ne!(
            made["copies"][0]["seal"]["enc"],
            made["copies"][1]["seal"]["enc"]
        );
        let own: HashMap<String, SealKey> = [(b.key_id(), b.clone())].into();
        assert_eq!(
            open(&made, "obi-b", &own),
            Ok(Some(("hello".to_owned(), about.to_vec())))
        );
        // A key that rotated: the same words, signed again.
        let again = signed_again(&made, &Keypair::from_seed(&[8; 32])).unwrap();
        assert_eq!(again["copies"], made["copies"]);
        assert_eq!(again["id"], made["id"]);
        assert_ne!(again["signature"], made["signature"]);
    }

    #[test]
    fn what_is_kept_keeps_what_waits_and_loses_the_oldest() {
        let home = std::env::temp_dir().join(format!("obid-room-{}", uuid::Uuid::new_v4()));
        let one = |n: usize, state: &str| Post {
            id: format!("p{n}"),
            from: "a".into(),
            owner: "o".into(),
            kind: "say".into(),
            by: "owner".into(),
            created_at: String::new(),
            reply_to: None,
            text: "t".into(),
            about: Vec::new(),
            seq: (state == SENT).then_some(n as u64 + 1),
            recorded_at: None,
            state: state.into(),
            error: None,
            job: None,
            wire: None,
            under: None,
        };
        let mut room = Room::default();
        room.posts.push(one(0, WAITING));
        for n in 1..KEPT + 5 {
            room.posts.push(one(n, SENT));
        }
        room.after = 99;
        room.save(&home);
        let kept = Room::open(&home);
        assert_eq!(kept.after, 99);
        assert_eq!(kept.posts.len(), KEPT);
        assert_eq!(kept.posts[0].id, "p0");
        assert_eq!(kept.posts[1].id, "p6");
        // Leaving: nothing is written, whatever was under way.
        room.close();
        std::fs::remove_dir_all(&home).unwrap();
        room.save(&home);
        assert!(!home.exists());
        let mut kept = kept;
        // In the Sphere's order, what's not sent last.
        let since = kept.since((KEPT + 3) as u64);
        assert_eq!(
            since.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            vec![
                format!("p{}", KEPT + 3),
                format!("p{}", KEPT + 4),
                "p0".into()
            ]
        );
        // What the self-answer limits count outlives the posts, for its hour.
        let now = time::now();
        kept.self_answers = vec![("old".into(), now - 4000), ("ernesto".into(), now - 10)];
        for n in KEPT + 5..KEPT + 20 {
            kept.posts.push(one(n, SENT));
        }
        kept.save(&home);
        let reopened = Room::open(&home);
        assert_eq!(
            reopened.self_answers,
            vec![("ernesto".to_owned(), now - 10)]
        );
        assert_eq!(reopened.posts.len(), KEPT);
        let _ = std::fs::remove_dir_all(home);
    }
}
