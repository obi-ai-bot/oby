//! B00-15: inbox sorting, memory and the recap think with the signed-in account of any tool with
//! the most room, not only Claude Code's (docs/api/inbox.md, "obid only: sorting the inbox"). Here
//! the only tool is Codex (the stand-in, tests/agents/fake-codex), and an assigned issue from the
//! fake gh is what the model sorts.
mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

const REPO: &str = "obi-e2e/fixture-app";

fn agents() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents")
}

/// obid's configuration with Codex alone, its one account signed in when `signed_in`.
fn codex_only(folder: &Folder, signed_in: bool) -> Value {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-claude"));
    let account = folder.0.join("home/accounts/codex");
    config["agents"] = json!({"codex": {"bin": agents().join("fake-codex"),
        "accounts": [{"id": "cstub", "name": "Codex stub", "configDir": account}]}});
    std::fs::create_dir_all(&account).unwrap();
    if signed_in {
        std::fs::write(account.join("auth.json"), "{}").unwrap();
    }
    config
}

/// An issue assigned to the owner comes in from GitHub (the first read notes what's there, the
/// second offers what's new): its inbox item, which only the model sorts.
fn an_issue_is_assigned(folder: &Folder, obid: &Obid) -> Value {
    let gh = folder.0.join("env-home/github");
    std::fs::create_dir_all(&gh).unwrap();
    write_work(&gh, "wellington", vec![], vec![], vec![]);
    refresh(obid);
    write_work(&gh, "wellington", vec![], vec![node(REPO, 640)], vec![]);
    refresh(obid);
    let items = github_items(obid, "open");
    assert_eq!(items.len(), 1, "{items:?}");
    items[0].clone()
}

/// The open inbox item `id` as `GET /v1/inbox` shows it.
fn item(obid: &Obid, id: &str) -> Value {
    let open = github_items(obid, "open");
    open.into_iter().find(|i| i["id"] == id).unwrap_or_default()
}

/// TRIAGE-ANY-1: with only a Codex account, a new inbox item is sorted within 2 minutes (before it
/// would need the owner unsorted), by Codex, and the call is kept as Codex's.
#[test]
fn triage_any_1_codex_only_sorts() {
    let folder = Folder::new("triage-any-1");
    let obid = start(&folder.0, &codex_only(&folder, true), Some(&agents()));
    let issue = an_issue_is_assigned(&folder, &obid);
    let id = issue["id"].as_str().unwrap().to_owned();
    let sorted = within(60, "Codex sorts the item", || {
        let shown = item(&obid, &id);
        (shown["triage"] != "pending").then_some(shown)
    });
    assert_eq!(sorted["triage"], "fyi", "{sorted}");
    assert_eq!(sorted["triageWhy"], Value::Null, "{sorted}");
    let records = std::fs::read_to_string(obid.home.join("brain-turns.jsonl")).unwrap();
    let triage: Vec<Value> = records
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|r| r["kind"] == "triage")
        .collect();
    assert_eq!(triage[0]["calls"][0]["adapter"], "codex", "{records}");
    assert_eq!(triage[0]["calls"][0]["outcome"], "answered", "{records}");
}

/// TRIAGE-ANY-2: with no account signed in, items stay `pending` past their time to wait (faked:
/// one second) and say why; nothing is guessed, and nothing is sent.
#[test]
fn triage_any_2_none_says_why() {
    let folder = Folder::new("triage-any-2");
    let env = [("OBID_TRIAGE_PENDING_SECONDS", "1")];
    let obid = start_with(
        &folder.0,
        &codex_only(&folder, false),
        Some(&agents()),
        &env,
    );
    let issue = an_issue_is_assigned(&folder, &obid);
    let id = issue["id"].as_str().unwrap().to_owned();
    let created = issue["created"].as_i64().unwrap();
    let shown = within(
        30,
        "the item says why it isn't sorted, past its time",
        || {
            let shown = item(&obid, &id);
            let past = obi_protocol::time::now() - created > 1;
            (past && shown["triageWhy"].is_string()).then_some(shown)
        },
    );
    assert_eq!(shown["triage"], "pending", "{shown}");
    assert_eq!(
        shown["triageWhy"],
        "Oby can't sort it yet: no account of Claude Code, Codex or Cursor is signed in with room to think.",
        "{shown}"
    );
    let records = std::fs::read_to_string(obid.home.join("brain-turns.jsonl")).unwrap_or_default();
    assert!(!records.contains("triage"), "a model was asked: {records}");
}

/// TRIAGE-ANY-3: with only a Codex account, the check-in's recap is written by Codex, not obid's
/// count. The check-in's time is moved to a minute ago while obid is off: it's due as obid starts.
#[test]
fn triage_any_3_recap_on_codex() {
    let folder = Folder::new("triage-any-3");
    let config = codex_only(&folder, true);
    let mut obid = start(&folder.0, &config, Some(&agents()));
    an_issue_is_assigned(&folder, &obid);
    obid.stop();
    let now = obi_protocol::time::now();
    let at = now - 60;
    let path = obid.home.join("settings.json");
    let mut settings: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({}));
    settings["checkIn"] = json!({"schedule": {"every": "daily",
        "at": format!("{:02}:{:02}", at.rem_euclid(86_400) / 3_600, at.rem_euclid(3_600) / 60)},
        "zone": "UTC", "conversation": "gone", "from": now - 86_400});
    std::fs::write(&path, settings.to_string()).unwrap();
    let obid = start(&folder.0, &config, Some(&agents()));
    let said = within(60, "the check-in is posted", || {
        let kept: Value = serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
        let conversation = kept["checkIn"]["conversation"].as_str()?;
        let target = format!("/v1/conversation?id={conversation}");
        let turns = call(&obid, "GET", &target, None).body;
        turns
            .as_array()?
            .iter()
            .find(|t| t["who"] == "oby")
            .cloned()
    });
    assert_eq!(said["text"], "Codex wrote this recap.", "{said}");
}
