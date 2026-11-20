//! B00-15: the brain's side calls (sorting the inbox, the recap, memory) run on any signed-in
//! account, not only the first Claude Code one (crate::brain::calls; docs/api/inbox.md, "obid
//! only: sorting the inbox"). Codex here is a stand-in that checks the side call's box and answers
//! its schema (tests/agents/fake-codex-side-call); the inbox item only the model sorts is an issue
//! assigned to the owner, from the GitHub work feed's stand-in (tests/agents/gh). Time is faked
//! through obid's hooks (`OBID_TRIAGE_PENDING_SECONDS`, a check-in's kept time), never slept.
mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

const REPO: &str = "obi-e2e/fixture-app";
/// What the stand-in writes for a recap.
const RECAP: &str = "Codex wrote this recap. One item waits for you.";

fn agents() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents")
}

/// obid with Codex (the side calls' stand-in) as its only tool, its account signed in when
/// `signed_in`, and Claude Code configured beside it, never signed in, when `claude`.
fn obid(folder: &Folder, signed_in: bool, claude: bool, env: &[(&str, &str)]) -> (Value, Obid) {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-harness"));
    if !claude {
        config["agents"].as_object_mut().unwrap().remove("claude");
    }
    let account = folder.0.join("home/accounts/codex");
    std::fs::create_dir_all(&account).unwrap();
    if signed_in {
        std::fs::write(account.join("auth.json"), "{}").unwrap();
    }
    config["agents"]["codex"] = json!({"bin": agents().join("fake-codex-side-call"),
        "accounts": [{"id": "cstub", "name": "Codex stub", "configDir": account}]});
    let obid = start_with(&folder.0, &config, Some(&agents()), env);
    (config, obid)
}

/// The GitHub work feed's stand-in's folder, once obid's own first read at its start is done.
fn feed(folder: &Folder, obid: &Obid) -> PathBuf {
    let gh = folder.0.join("env-home/github");
    std::fs::create_dir_all(&gh).unwrap();
    within(30, "obid's first read of GitHub work", || {
        let feed = call(obid, "GET", "/v1/github/work", None);
        (!feed.body["nextCheck"].is_null()).then_some(())
    });
    gh
}

/// An issue newly assigned to the owner, offered in the inbox: the kind only the model sorts.
fn assigned_issue(gh: &Path, obid: &Obid) -> String {
    let issues = |numbers: &[u64]| numbers.iter().map(|n| node(REPO, *n)).collect();
    // The first read on record notes what's there; what comes after is offered.
    write_work(gh, "wellington", vec![], issues(&[640]), vec![]);
    refresh(obid);
    write_work(gh, "wellington", vec![], issues(&[640, 641]), vec![]);
    refresh(obid);
    let items = github_items(obid, "open");
    let item = items
        .iter()
        .find(|i| i["github"]["number"] == 641)
        .unwrap_or_else(|| panic!("no item for the assigned issue in {items:?}"));
    item["id"].as_str().unwrap().to_owned()
}

fn item(obid: &Obid, id: &str) -> Value {
    github_items(obid, "open")
        .into_iter()
        .find(|i| i["id"] == id)
        .unwrap_or_else(|| panic!("no open item {id}"))
}

/// What obid's log says of triage and the brain's calls: why a call had no answer.
fn said(obid: &Obid) -> String {
    obid.log()
        .lines()
        .filter(|l| {
            l.contains("triage")
                || l.contains("brain")
                || l.contains("features")
                || l.contains("Codex")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The brain's records of `kind`'s calls.
fn calls(obid: &Obid, kind: &str) -> Vec<Value> {
    std::fs::read_to_string(obid.home.join("brain-turns.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|record| record["kind"] == kind)
        .flat_map(|record| record["calls"].as_array().cloned().unwrap_or_default())
        .collect()
}

/// TRIAGE-ANY-1: with only a Codex account, a new item only the model sorts is sorted within 2
/// minutes (its time to wait, `pending_seconds`), by Codex: `fyi` is the model's word alone.
#[test]
fn triage_any_1_codex_only_sorts() {
    let folder = Folder::new("triage-any-1");
    let (_, obid) = obid(&folder, true, false, &[]);
    let gh = feed(&folder, &obid);
    let id = assigned_issue(&gh, &obid);
    let sorted = within(120, "Codex sorts the assigned issue", || {
        let item = item(&obid, &id);
        (item["triage"] != "pending").then_some(item)
    });
    assert_eq!(sorted["triage"], "fyi", "{sorted}\n{}", said(&obid));
    let triage = calls(&obid, "triage");
    assert!(
        triage
            .iter()
            .any(|c| c["adapter"] == "codex" && c["outcome"] == "answered"),
        "{triage:?}"
    );
    // What Codex counted, kept with the call.
    let call = triage.iter().find(|c| c["adapter"] == "codex").unwrap();
    assert_eq!(
        (&call["input_tokens"], &call["output_tokens"]),
        (&json!(1000), &json!(30))
    );
}

/// TRIAGE-ANY-2: with no account signed in (Claude Code and Codex both configured, neither signed
/// in), nothing is sent: the item stays `pending` past its time to wait (none here) and a `triage`
/// event says why, until an account signs in.
#[test]
fn triage_any_2_none_says_why() {
    let folder = Folder::new("triage-any-2");
    let (_, obid) = obid(
        &folder,
        false,
        true,
        &[("OBID_TRIAGE_PENDING_SECONDS", "0")],
    );
    let events = follow_events(&obid);
    let gh = feed(&folder, &obid);
    let id = assigned_issue(&gh, &obid);
    let said = events_named(&events, "triage", 30, |got| {
        got.iter().any(|e| e["why"].is_string())
    });
    let why = said.iter().find_map(|e| e["why"].as_str()).unwrap();
    assert!(
        why.contains("no Claude Code or Codex account is signed in"),
        "{why}"
    );
    assert_eq!(said.last().unwrap()["waiting"], 1, "{said:?}");
    // Past its time to wait, still `pending`: never guessed `needs_you`.
    assert_eq!(item(&obid, &id)["triage"], "pending");
    assert!(calls(&obid, "triage").is_empty(), "nothing was sent");
    // Codex signs in: the item is sorted, and the event says nothing waits for a reason.
    std::fs::write(folder.0.join("home/accounts/codex/auth.json"), "{}").unwrap();
    let sorted = within(60, "Codex sorts it once it's signed in", || {
        let item = item(&obid, &id);
        (item["triage"] == "fyi").then_some(item)
    });
    assert_eq!(sorted["triage"], "fyi");
    let cleared = events_named(&events, "triage", 30, |got| {
        got.iter().any(|e| e["why"].is_null())
    });
    assert!(cleared.iter().any(|e| e["why"].is_null()), "{cleared:?}");
}

/// TRIAGE-ANY-3: with only a Codex account, the check-in's recap of what happened is Codex's
/// sentences, not obid's count. Its time is faked: the kept check-in is due as obid starts.
#[test]
fn triage_any_3_recap_on_codex() {
    let folder = Folder::new("triage-any-3");
    let (config, mut obid) = obid(&folder, true, false, &[]);
    let gh = feed(&folder, &obid);
    // Something happened: an item came in.
    assigned_issue(&gh, &obid);
    obid.stop();
    let path = obid.home.join("settings.json");
    let mut settings: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({}));
    let now = obi_protocol::time::now();
    let at = now - 60;
    settings["checkIn"] = json!({"schedule": {"every": "daily",
            "at": format!("{:02}:{:02}", at.rem_euclid(86_400) / 3_600, at.rem_euclid(3_600) / 60)},
        "zone": "UTC", "conversation": "gone", "from": now - 86_400});
    std::fs::write(&path, settings.to_string()).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let obid = start_with(&folder.0, &config, Some(&agents()), &[]);
    let turn = within(60, "the check-in posts Codex's recap", || {
        let settings: Value = serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
        let conversation = settings["checkIn"]["conversation"].as_str()?.to_owned();
        let turns = call(
            &obid,
            "GET",
            &format!("/v1/conversation?id={conversation}"),
            None,
        );
        turns
            .body
            .as_array()?
            .iter()
            .find(|t| t["who"] == "oby")
            .cloned()
    });
    assert_eq!(turn["text"], RECAP, "{turn}\n{}", said(&obid));
    let recap = calls(&obid, "recap");
    assert!(
        recap
            .iter()
            .any(|c| c["adapter"] == "codex" && c["outcome"] == "answered"),
        "{recap:?}"
    );
}
