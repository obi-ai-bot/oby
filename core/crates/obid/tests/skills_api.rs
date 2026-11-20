//! The owner's agent skills (docs/contracts/skills.md, SKILLS-5 to 12; docs/api/skills.md):
//! `GET /v1/skills`, a picked skill reaching the conversation's agent in its own form, a waiting
//! agent started before an install thrown away, and Restart agent. The agent is the stand-in
//! (tests/agents/fake-harness), which says which skill it was started with; the skills are a
//! fixture in the stub account's own folder.
mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

fn harness() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness")
}

/// A skill in the stub account's own folder (`home/accounts/stub/skills/<name>`).
fn install(folder: &Folder, name: &str, front: &str) {
    let dir = folder.0.join("home/accounts/stub/skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\n{front}\n---\n\nDo it.\n"),
    )
    .unwrap();
}

/// obid with the stand-in as Claude Code, a signed-in stub account holding two skills, and
/// `kept` conversations in its home before it starts.
fn obid(name: &str, kept: &[(&str, Value)]) -> (Folder, Obid) {
    let folder = Folder::new(name);
    let dir = folder.0.join("home/conversations");
    std::fs::create_dir_all(&dir).unwrap();
    for (id, thread) in kept {
        std::fs::write(dir.join(format!("{id}.json")), thread.to_string()).unwrap();
    }
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &harness());
    config["conversation"] = json!({"agent": "claude"});
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    install(
        &folder,
        "write-tests",
        "description: Writes the tests first.",
    );
    install(
        &folder,
        "agent-only",
        "description: Only the agent starts it.\nuser-invocable: false",
    );
    let obid = start_with(&folder.0, &config, None, &[]);
    (folder, obid)
}

// Only the own lane's tests send words, and the own lane is macOS's for now (LANES-12).
#[cfg(target_os = "macos")]
fn said(obid: &Obid, body: Value) -> Answer {
    let answer = call(obid, "POST", "/v1/plans", Some(&body));
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer
}

/// What the stand-in recorded of each conversation turn, oldest first.
fn turns(obid: &Obid) -> Vec<Value> {
    let mut all: Vec<Value> = std::fs::read_dir(obid.home.join("conversations"))
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|entry| {
            std::fs::read_to_string(entry.path().join("config/turns.jsonl"))
                .unwrap_or_default()
                .lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .collect::<Vec<_>>()
        })
        .collect();
    all.sort_by_key(|t| t["at"].as_u64());
    all
}

fn names(answer: &Answer) -> Vec<String> {
    (answer.body["skills"]
        .as_array()
        .cloned()
        .unwrap_or_default())
    .iter()
    .filter_map(|s| s["name"].as_str().map(str::to_owned))
    .collect()
}

/// A conversation of other Obys' questions (ASK-16), kept before obid starts: its Claude Code
/// stays in Oby's box (LANES-12).
fn obys_conversation() -> (String, Value) {
    let id = uuid::Uuid::new_v4().to_string();
    let thread = json!({"id": id, "title": "Questions from other Obys", "pinned": false,
        "created": "2026-11-10T00:00:00Z", "sequence": 1, "turns": [], "foreign": null,
        "readUntrusted": true, "fromObys": true});
    (id, thread)
}

/// SKILLS-5: the skills of the agent the next words would reach, only where they'd run: the
/// owner's own Claude Code lists the account's own; a conversation in Oby's box lists none and
/// says why; a conversation, job or account obid doesn't have is a 404.
#[test]
fn skills_5_the_box_lists_the_agents_skills_where_they_run() {
    let (id, thread) = obys_conversation();
    let (_folder, obid) = obid("skills-5", &[(&id, thread)]);
    let home = call(&obid, "GET", "/v1/skills", None);
    assert_eq!(home.status, 200, "{}", home.text);
    assert_eq!(home.body["tool"], "claude");
    assert_eq!(home.body["account"], "stub");
    // The owner's own conversation, on macOS and Linux alike (LANES-12, LANES-26).
    assert_eq!(names(&home), ["agent-only", "write-tests"], "{}", home.text);
    let skill = &home.body["skills"][1];
    assert_eq!(skill["description"], "Writes the tests first.");
    assert_eq!(skill["userInvocable"], true);
    assert_eq!(skill["source"], "user");
    assert_eq!(home.body["skills"][0]["userInvocable"], false);
    assert!(home.body.get("why").is_none(), "{}", home.text);
    let boxed = call(&obid, "GET", &format!("/v1/skills?conversation={id}"), None);
    assert_eq!(boxed.status, 200, "{}", boxed.text);
    assert!(names(&boxed).is_empty(), "{}", boxed.text);
    assert!(
        boxed.body["why"]
            .as_str()
            .unwrap_or_default()
            .contains("Oby's box"),
        "{}",
        boxed.text
    );
    let account = call(&obid, "GET", "/v1/skills?account=stub", None);
    assert_eq!(
        names(&account),
        ["agent-only", "write-tests"],
        "{}",
        account.text
    );
    for path in [
        "/v1/skills?conversation=00000000-0000-4000-8000-000000000000",
        "/v1/skills?job=nope",
        "/v1/skills?account=nope",
    ] {
        assert_eq!(call(&obid, "GET", path, None).status, 404, "{path}");
    }
    let both = call(&obid, "GET", "/v1/skills?account=stub&job=x", None);
    assert_eq!(both.status, 400, "{}", both.text);
}

/// SKILLS-6 and 8: a picked skill reaches Claude Code as `/name` opening the last text block,
/// with the owner's words after it, and nothing else: words that say `/name` themselves, or a
/// skill the account hasn't, start none, and the answer says the skill wasn't used.
#[cfg(target_os = "macos")]
#[test]
fn skills_6_a_picked_skill_reaches_claude_code_as_slash_name() {
    let (_folder, obid) = obid("skills-6", &[]);
    let first = said(
        &obid,
        json!({"said": "tidy the parser's tests", "new": true, "skill": "write-tests"}),
    );
    assert_eq!(
        first.body["text"], "skill: write-tests words=tidy the parser's tests",
        "{}",
        first.text
    );
    let conversation = first.body["conversation"].as_str().unwrap().to_owned();
    let recorded = turns(&obid);
    assert_eq!(
        recorded[0]["prompt"],
        "/write-tests tidy the parser's tests"
    );
    // A skill the account hasn't (or the agent alone may start): the words go alone, said so.
    for name in ["no-such-skill", "agent-only"] {
        let other = said(
            &obid,
            json!({"said": "SAY plain", "conversation": conversation, "skill": name}),
        );
        let text = other.body["text"].as_str().unwrap_or_default();
        assert!(text.starts_with("plain"), "{}", other.text);
        assert!(
            text.contains(&format!("Your words went without the skill {name}")),
            "{}",
            other.text
        );
    }
    // A skill picked with no words is a message of its own: `/name` alone.
    let alone = said(
        &obid,
        json!({"said": "", "conversation": conversation, "skill": "write-tests"}),
    );
    assert_eq!(
        alone.body["text"], "skill: write-tests words=",
        "{}",
        alone.text
    );
    let none = said(
        &obid,
        json!({"said": "", "conversation": conversation, "skill": "no-such-skill"}),
    );
    assert!(
        none.body["text"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Nothing was sent"),
        "{}",
        none.text
    );
    // Words that name a skill aren't one: no `skill`, no skill call.
    let typed = said(
        &obid,
        json!({"said": "SAY /write-tests is a skill", "conversation": conversation}),
    );
    assert_eq!(
        typed.body["text"], "/write-tests is a skill",
        "{}",
        typed.text
    );
}

/// SKILLS-12: a `skill` that isn't a skill's name is refused, and nothing of the words is kept.
#[test]
fn skills_12_a_skill_that_isnt_a_name_is_refused() {
    let (_folder, obid) = obid("skills-12", &[]);
    for bad in ["../etc", "a b", "/write-tests", ""] {
        let answer = call(
            &obid,
            "POST",
            "/v1/plans",
            Some(&json!({"said": "hi", "new": true, "skill": bad})),
        );
        assert_eq!(answer.status, 422, "{bad}: {}", answer.text);
    }
    assert!(turns(&obid).is_empty());
}

/// SKILLS-9: the Claude Code waiting for a conversation's next turn isn't given it once a skill
/// was installed on its account since it started: the turn starts it again, on the same session.
#[cfg(target_os = "macos")]
#[test]
fn skills_9_a_waiting_agent_started_before_an_install_starts_again() {
    let (folder, obid) = obid("skills-9", &[]);
    let first = said(&obid, json!({"said": "SAY one", "new": true}));
    let conversation = first.body["conversation"].as_str().unwrap().to_owned();
    let again = said(
        &obid,
        json!({"said": "SAY two", "conversation": conversation}),
    );
    assert_eq!(again.body["text"], "two", "{}", again.text);
    let recorded = turns(&obid);
    assert_eq!(
        recorded[0]["id"], recorded[1]["id"],
        "nothing installed: the same one"
    );
    install(&folder, "review-diff", "description: Reads the diff.");
    let third = said(
        &obid,
        json!({"said": "REMEMBER", "conversation": conversation, "skill": "review-diff"}),
    );
    assert!(
        third.body["text"]
            .as_str()
            .unwrap_or_default()
            .starts_with("skill: review-diff"),
        "{}",
        third.text
    );
    let recorded = turns(&obid);
    assert_ne!(recorded[1]["id"], recorded[2]["id"], "{recorded:?}");
    assert!(recorded[2]["resumed"].is_string(), "{recorded:?}");
}

/// SKILLS-10: Restart agent ends the agent waiting for the conversation's next turn; that turn
/// starts it again on the same session. With none waiting it says so; another conversation is a 404.
#[cfg(target_os = "macos")]
#[test]
fn skills_10_restart_agent_starts_the_next_turn_afresh() {
    let (_folder, obid) = obid("skills-10", &[]);
    let first = said(&obid, json!({"said": "SAY one", "new": true}));
    let conversation = first.body["conversation"].as_str().unwrap().to_owned();
    let path = format!("/v1/conversations/{conversation}/restart");
    let restarted = call(&obid, "POST", &path, None);
    assert_eq!(restarted.status, 200, "{}", restarted.text);
    assert_eq!(restarted.body["restarted"], true);
    let again = call(&obid, "POST", &path, None);
    assert_eq!(again.body["restarted"], false, "{}", again.text);
    said(
        &obid,
        json!({"said": "SAY two", "conversation": conversation}),
    );
    let recorded = turns(&obid);
    assert_eq!(recorded.len(), 2, "{recorded:?}");
    assert_ne!(recorded[0]["id"], recorded[1]["id"], "{recorded:?}");
    assert!(recorded[1]["resumed"].is_string(), "{recorded:?}");
    let gone = call(
        &obid,
        "POST",
        "/v1/conversations/00000000-0000-4000-8000-000000000000/restart",
        None,
    );
    assert_eq!(gone.status, 404, "{}", gone.text);
}
