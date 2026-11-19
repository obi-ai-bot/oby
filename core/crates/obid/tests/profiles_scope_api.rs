//! Each profile keeps its own conversations, jobs, helpers, accounts and vault (docs/contracts/
//! profiles.md, B00-40): a request names its profile with `Oby-Profile`, and sees, changes and uses
//! only that profile's. Each test is named after the contract's scenario it proves.
mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

const DEFAULT: &str = "3f0c2a4e-9b1d-4c8e-a7f2-5d6e8b9c0a1f";
const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const UNKNOWN: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const ONE: &str = "11111111-1111-4111-8111-111111111111";
const TWO: &str = "22222222-2222-4222-8222-222222222222";

/// A stopped obid's home with the default profile and profile B, conversation [`ONE`] kept
/// before profiles (the default's) and [`TWO`] made in B.
fn two_profiles(name: &str) -> (Folder, PathBuf) {
    let folder = Folder::new(name);
    let home = folder.0.join("home");
    std::fs::create_dir_all(home.join("profiles").join(B)).unwrap();
    let registry = json!({"profiles": [
        {"id": DEFAULT, "name": "Default", "color": "#3a5a8c", "default": true},
        {"id": B, "name": "Acme", "color": "#7a4a8c", "default": false}]});
    std::fs::write(home.join("profiles.json"), registry.to_string()).unwrap();
    kept_conversation(&home, ONE, "Personal errands", "buy the groceries", None);
    kept_conversation(&home, TWO, "Acme launch", "plan the acme launch", Some(B));
    (folder, home)
}

fn kept_conversation(home: &Path, id: &str, title: &str, said: &str, profile: Option<&str>) {
    let dir = home.join("conversations");
    std::fs::create_dir_all(&dir).unwrap();
    let turn = json!({"id": format!("{}0", &id[..35]), "who": "user", "text": said,
        "at": "2026-11-15T10:00:00Z", "conversation": id, "jobs": []});
    let mut thread = json!({"id": id, "title": title, "pinned": false,
        "created": "2026-11-15T10:00:00Z", "sequence": 1, "turns": [turn], "foreign": null});
    if let Some(profile) = profile {
        thread["profile"] = json!(profile);
    }
    std::fs::write(dir.join(format!("{id}.json")), thread.to_string()).unwrap();
}

fn obid_in(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    start(&folder.0, &config, None)
}

/// `method` to `path` as profile `profile` (`Oby-Profile`), or naming none.
fn as_profile(
    obid: &Obid,
    profile: Option<&str>,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Answer {
    let headers: Vec<(&str, &str)> = profile.map(|p| ("Oby-Profile", p)).into_iter().collect();
    call_with_headers(obid, TOKEN, method, path, body, &headers)
}

/// The owner's signed words, as profile `profile`.
fn signed_as(
    obid: &Obid,
    profile: Option<&str>,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Answer {
    let words = words_header(obid, method, path, body);
    let mut headers = vec![("X-Oby-Words", words.as_str())];
    headers.extend(profile.map(|p| ("Oby-Profile", p)));
    call_with_headers(obid, TOKEN, method, path, body, &headers)
}

/// The ids in a list `path` answers as `profile`, at `key` (none: the list itself).
fn ids(obid: &Obid, profile: Option<&str>, path: &str, key: Option<&str>) -> Vec<String> {
    let answer = as_profile(obid, profile, "GET", path, None);
    assert_eq!(answer.status, 200, "{path}: {}", answer.text);
    let list = key.map_or(&answer.body, |key| &answer.body[key]);
    let list = list
        .as_array()
        .unwrap_or_else(|| panic!("{path}: {}", answer.text));
    list.iter()
        .map(|v| v["id"].as_str().unwrap().to_owned())
        .collect()
}

/// PROFILES-33: a conversation made in one profile is listed, searched, read, renamed, deleted and
/// spoken in only from it, and one made with words in a profile is that profile's.
#[test]
fn profiles_33_a_conversation_is_kept_in_its_own_profile() {
    let (folder, home) = two_profiles("profiles-33");
    let obid = obid_in(&folder);

    assert_eq!(ids(&obid, None, "/v1/conversations", None), [ONE]);
    assert_eq!(ids(&obid, Some(DEFAULT), "/v1/conversations", None), [ONE]);
    assert_eq!(ids(&obid, Some(B), "/v1/conversations", None), [TWO]);
    assert!(ids(&obid, None, "/v1/conversations?q=acme", None).is_empty());
    assert_eq!(ids(&obid, Some(B), "/v1/conversations?q=acme", None), [TWO]);
    assert!(ids(&obid, Some(B), "/v1/conversations?q=groceries", None).is_empty());

    let page = format!("/v1/conversation?id={TWO}");
    assert_eq!(as_profile(&obid, None, "GET", &page, None).status, 404);
    let read = as_profile(&obid, Some(B), "GET", &page, None);
    assert_eq!(read.status, 200, "{}", read.text);
    assert_eq!(read.body[0]["text"], "plan the acme launch");
    let latest = as_profile(&obid, Some(B), "GET", "/v1/conversation", None);
    assert_eq!(latest.body[0]["conversation"], TWO, "{}", latest.text);

    let file = home.join(format!("conversations/{TWO}.json"));
    let before = std::fs::read(&file).unwrap();
    let path = format!("/v1/conversations/{TWO}");
    let renamed = as_profile(
        &obid,
        None,
        "PATCH",
        &path,
        Some(&json!({"title": "Mine now"})),
    );
    assert_eq!(renamed.status, 404, "{}", renamed.text);
    assert_eq!(as_profile(&obid, None, "DELETE", &path, None).status, 404);
    let said = json!({"said": "what's in here?", "conversation": TWO});
    let spoken = as_profile(&obid, None, "POST", "/v1/plans", Some(&said));
    assert_eq!(spoken.status, 404, "{}", spoken.text);
    assert_eq!(
        std::fs::read(&file).unwrap(),
        before,
        "untouched from the default profile"
    );

    let new = json!({"said": "draft the acme press release", "new": true});
    let started = as_profile(&obid, Some(B), "POST", "/v1/plans", Some(&new));
    assert_eq!(started.status, 200, "{}", started.text);
    // B has no account of its own: Oby says so, naming none of the default profile's.
    assert!(!started.text.contains("Stub"), "{}", started.text);
    assert!(
        started.text.contains("no agent account"),
        "{}",
        started.text
    );
    let made = started.body["conversation"].as_str().unwrap().to_owned();
    assert!(ids(&obid, Some(B), "/v1/conversations", None).contains(&made));
    assert!(!ids(&obid, None, "/v1/conversations", None).contains(&made));
    let kept: Value = serde_json::from_slice(
        &std::fs::read(home.join(format!("conversations/{made}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(kept["profile"], B);
}

/// PROFILES-34: a job made in a profile is listed and reached only from it, runs only on that
/// profile's accounts, and its task conversation is that profile's.
#[test]
fn profiles_34_a_job_is_kept_in_its_own_profile_on_its_accounts() {
    let (folder, home) = two_profiles("profiles-34");
    std::fs::write(
        home.join("account-profiles.json"),
        json!({"stub": B}).to_string(),
    )
    .unwrap();
    let obid = obid_in(&folder);
    let job = |account: Option<&str>| {
        let mut job = json!({"repo": "obi-e2e/fixture-app", "kind": "investigate",
            "task": "Find where the launch date is set"});
        if let Some(account) = account {
            job["account"] = json!(account);
        }
        json!({"job": job})
    };

    let refused = as_profile(&obid, None, "POST", "/v1/jobs", Some(&job(None)));
    assert!((400..500).contains(&refused.status), "{}", refused.text);
    let named = as_profile(&obid, None, "POST", "/v1/jobs", Some(&job(Some("stub"))));
    assert!((400..500).contains(&named.status), "{}", named.text);

    let made = as_profile(&obid, Some(B), "POST", "/v1/jobs", Some(&job(None)));
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap().to_owned();
    assert_eq!(made.body["profile"], B, "{}", made.text);
    assert_eq!(made.body["account"], "stub");

    assert!(!ids(&obid, None, "/v1/jobs", None).contains(&id));
    assert!(ids(&obid, Some(B), "/v1/jobs", None).contains(&id));
    let path = format!("/v1/jobs/{id}");
    assert_eq!(as_profile(&obid, None, "GET", &path, None).status, 404);
    let stop = format!("/v1/jobs/{id}/stop");
    assert_eq!(as_profile(&obid, None, "POST", &stop, None).status, 404);
    assert_eq!(as_profile(&obid, Some(B), "GET", &path, None).status, 200);

    let conversation = made.body["conversation"].as_str().unwrap().to_owned();
    assert!(ids(&obid, Some(B), "/v1/conversations", None).contains(&conversation));
    assert!(!ids(&obid, None, "/v1/conversations", None).contains(&conversation));
}

/// PROFILES-35: an account added in a profile is that profile's alone: listed, changed and removed
/// only from it, and the default profile's aren't listed in it.
#[test]
fn profiles_35_an_account_is_kept_in_its_own_profile() {
    let (folder, home) = two_profiles("profiles-35");
    let obid = obid_in(&folder);
    let body = json!({"agent": "claude", "name": "Acme Claude"});
    let added = as_profile(&obid, Some(B), "POST", "/v1/accounts", Some(&body));
    assert_eq!(added.status, 201, "{}", added.text);
    let id = added.body["id"].as_str().unwrap().to_owned();

    let default = ids(&obid, None, "/v1/status", Some("accounts"));
    assert!(
        default.contains(&"stub".to_owned()) && !default.contains(&id),
        "{default:?}"
    );
    assert_eq!(
        ids(&obid, Some(B), "/v1/status", Some("accounts")),
        [id.as_str()]
    );
    let kept: Value =
        serde_json::from_slice(&std::fs::read(home.join("account-profiles.json")).unwrap())
            .unwrap();
    assert_eq!(kept, json!({id.clone(): B}));

    let path = format!("/v1/accounts/{id}");
    let renamed = as_profile(&obid, None, "PATCH", &path, Some(&json!({"name": "Mine"})));
    assert_eq!(renamed.status, 404, "{}", renamed.text);
    assert_eq!(as_profile(&obid, None, "DELETE", &path, None).status, 404);
    let stub = as_profile(
        &obid,
        Some(B),
        "PATCH",
        "/v1/accounts/stub",
        Some(&json!({"name": "X"})),
    );
    assert_eq!(stub.status, 404, "{}", stub.text);
    assert_eq!(
        ids(&obid, Some(B), "/v1/status", Some("accounts")),
        [id.as_str()]
    );

    let removed = as_profile(&obid, Some(B), "DELETE", &path, None);
    assert!(
        removed.status == 204 || removed.status == 200,
        "{}",
        removed.text
    );
    assert!(ids(&obid, Some(B), "/v1/status", Some("accounts")).is_empty());
    let kept: Value =
        serde_json::from_slice(&std::fs::read(home.join("account-profiles.json")).unwrap())
            .unwrap();
    assert_eq!(kept, json!({}));
}

/// PROFILES-36: a helper made in a profile is listed and changed only from it, and can't be made
/// in another profile than the request's.
#[test]
fn profiles_36_a_helper_is_kept_in_its_own_profile() {
    let (folder, _home) = two_profiles("profiles-36");
    let obid = obid_in(&folder);
    let body = json!({"name": "Acme inbox", "color": "#3a5a8c", "instructions": "Sort my inbox.",
        "tool": "claude", "connectors": []});
    let made = as_profile(&obid, Some(B), "POST", "/v1/helpers", Some(&body));
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap().to_owned();
    assert_eq!(made.body["profile"], B);

    assert!(ids(&obid, None, "/v1/helpers", None).is_empty());
    assert_eq!(ids(&obid, Some(B), "/v1/helpers", None), [id.as_str()]);
    let path = format!("/v1/helpers/{id}");
    let changed = as_profile(&obid, None, "PATCH", &path, Some(&json!({"name": "Mine"})));
    assert_eq!(changed.status, 404, "{}", changed.text);
    let conversation = made.body["conversation"].as_str().unwrap().to_owned();
    assert!(!ids(&obid, None, "/v1/conversations", None).contains(&conversation));

    let mut elsewhere = body.clone();
    elsewhere["profile"] = json!(DEFAULT);
    let refused = as_profile(&obid, Some(B), "POST", "/v1/helpers", Some(&elsewhere));
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(ids(&obid, Some(B), "/v1/helpers", None), [id]);
}

/// PROFILES-37: a vault secret put in a profile is listed only in it; another profile can't
/// write over it, remove it or see its name.
#[test]
fn profiles_37_a_vault_secret_is_kept_in_its_own_profile() {
    let (folder, _home) = two_profiles("profiles-37");
    let obid = obid_in(&folder);
    let value = json!({"value": format!("acme-{}", uuid::Uuid::new_v4().simple())});
    let path = "/v1/obid/vault/acme-deploy-key";
    let put = signed_as(&obid, Some(B), "PUT", path, Some(&value));
    assert_eq!(put.status, 204, "{}", put.text);

    let names =
        |profile| as_profile(&obid, profile, "GET", "/v1/obid/vault", None).body["names"].clone();
    assert_eq!(names(Some(B)), json!(["acme-deploy-key"]));
    assert_eq!(names(None), json!([]));
    let other = json!({"value": format!("mine-{}", uuid::Uuid::new_v4().simple())});
    let over = signed_as(&obid, None, "PUT", path, Some(&other));
    assert_eq!(over.status, 409, "{}", over.text);
    let removed = signed_as(&obid, None, "DELETE", path, None);
    assert_eq!(removed.status, 404, "{}", removed.text);
    assert_eq!(names(Some(B)), json!(["acme-deploy-key"]));
}

/// PROFILES-38: an app following events as a profile hears its own conversations and accounts,
/// never another profile's.
#[test]
fn profiles_38_the_event_stream_carries_only_the_profiles_own() {
    let (folder, _home) = two_profiles("profiles-38");
    let obid = obid_in(&folder);
    let in_b = follow_events_as(&obid, &[("Oby-Profile", B)]);
    let in_default = follow_events(&obid);

    let rename = |profile, id: &str, title: &str| {
        let path = format!("/v1/conversations/{id}");
        let answer = as_profile(
            &obid,
            profile,
            "PATCH",
            &path,
            Some(&json!({"title": title})),
        );
        assert_eq!(answer.status, 200, "{}", answer.text);
    };
    rename(None, ONE, "Errands, renamed");
    rename(Some(B), TWO, "Acme, renamed");
    let body = json!({"agent": "claude", "name": "Acme Claude"});
    let added = as_profile(&obid, Some(B), "POST", "/v1/accounts", Some(&body));
    assert_eq!(added.status, 201, "{}", added.text);
    let id = added.body["id"].as_str().unwrap().to_owned();

    let renamed = |events, title: &str| {
        events_named(events, "conversation", 30, |got| {
            got.iter().any(|c| c["title"] == title)
        })
    };
    let heard = renamed(&in_b, "Acme, renamed");
    assert!(heard.iter().all(|c| c["id"] == TWO), "{heard:?}");
    let heard = renamed(&in_default, "Errands, renamed");
    assert!(heard.iter().all(|c| c["id"] == ONE), "{heard:?}");
    let listed = |events, has: bool| {
        events_named(events, "accounts", 30, |got| {
            got.last()
                .is_some_and(|l| l.as_array().unwrap().iter().any(|a| a["id"] == "stub") != has)
        })
    };
    let accounts = listed(&in_b, true);
    let last = accounts.last().unwrap().as_array().unwrap();
    assert_eq!(
        last.iter().map(|a| a["id"].clone()).collect::<Vec<_>>(),
        [json!(id)]
    );
    let accounts = listed(&in_default, false);
    assert!(
        accounts
            .last()
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["id"] != id.as_str())
    );
    // Nothing of the other profile's came meanwhile.
    while let Ok((name, data)) = in_b.recv_timeout(std::time::Duration::from_millis(500)) {
        assert!(!(name == "conversation" && data["id"] == ONE), "{data}");
    }
}

/// PROFILES-39: what an install kept before profiles is the default profile's, unmoved; a new
/// profile starts with none of it.
#[test]
fn profiles_39_what_was_kept_before_is_the_default_profiles() {
    let folder = Folder::new("profiles-39");
    let home = folder.0.join("home");
    kept_conversation(&home, ONE, "Personal errands", "buy the groceries", None);
    let file = home.join(format!("conversations/{ONE}.json"));
    let before = std::fs::read(&file).unwrap();
    let obid = obid_in(&folder);

    assert_eq!(ids(&obid, None, "/v1/conversations", None), [ONE]);
    assert!(ids(&obid, None, "/v1/status", Some("accounts")).contains(&"stub".to_owned()));
    let made = call(
        &obid,
        "POST",
        "/v1/profiles",
        Some(&json!({"name": "Acme", "color": "#7a4a8c"})),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let b = made.body["id"].as_str().unwrap();
    assert!(ids(&obid, Some(b), "/v1/conversations", None).is_empty());
    assert!(ids(&obid, Some(b), "/v1/status", Some("accounts")).is_empty());
    assert!(ids(&obid, Some(b), "/v1/jobs", None).is_empty());
    assert_eq!(std::fs::read(&file).unwrap(), before, "kept as it was");
    assert!(!home.join("account-profiles.json").exists());
    assert!(!home.join("vault-profiles.json").exists());
}

/// PROFILES-40: a request naming a profile this Mac doesn't have is refused on every route, and
/// nothing is made or read.
#[test]
fn profiles_40_an_unknown_profile_is_refused_everywhere() {
    let (folder, home) = two_profiles("profiles-40");
    let obid = obid_in(&folder);
    for (method, path, body) in [
        ("GET", "/v1/status", None),
        ("GET", "/v1/conversations", None),
        ("GET", "/v1/conversation", None),
        ("GET", "/v1/jobs", None),
        ("GET", "/v1/helpers", None),
        ("GET", "/v1/obid/vault", None),
        ("GET", "/v1/events", None),
        (
            "POST",
            "/v1/plans",
            Some(json!({"said": "hello", "new": true})),
        ),
        (
            "POST",
            "/v1/accounts",
            Some(json!({"agent": "claude", "name": "Lost"})),
        ),
    ] {
        let answer = as_profile(&obid, Some(UNKNOWN), method, path, body.as_ref());
        assert_eq!(answer.status, 400, "{method} {path}: {}", answer.text);
        assert!(answer.body["error"].is_string(), "{}", answer.text);
    }
    assert_eq!(ids(&obid, None, "/v1/conversations", None), [ONE]);
    assert!(
        !ids(&obid, None, "/v1/status", Some("accounts"))
            .iter()
            .any(|a| a != "stub")
    );
    assert!(!home.join("account-profiles.json").exists());
}
