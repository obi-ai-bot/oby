//! A profile is offered the Mac's own logins no profile holds, and is deleted with everything it
//! kept becoming the default's (docs/contracts/profiles.md, PROFILES-43 to 45). Each test is named
//! after the contract's scenario it proves.
mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

const DEFAULT: &str = "3f0c2a4e-9b1d-4c8e-a7f2-5d6e8b9c0a1f";
const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const UNKNOWN: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const TWO: &str = "22222222-2222-4222-8222-222222222222";

fn agent(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/agents/{name}"))
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

/// Claude Code's, Codex's and Cursor's own logins on the test's Mac, with a stand-in `codex` on
/// the PATH: the folder of the stand-in.
fn logins_on_the_mac(folder: &Path) -> PathBuf {
    use base64::Engine;
    use std::os::unix::fs::PermissionsExt;
    let bin = folder.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("codex"), "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(bin.join("codex"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = folder.join("env-home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(home.join(".claude/.credentials.json"), "{}").unwrap();
    let ends = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let claims =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json!({"exp": ends}).to_string());
    let login = json!({"tokens": {"access_token": format!("e30.{claims}.c2ln"), "refresh_token": (["r", "1"].concat())}});
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(home.join(".codex/auth.json"), login.to_string()).unwrap();
    let cursor = home.join(if cfg!(target_os = "macos") {
        ".cursor/auth.json"
    } else {
        ".config/cursor/auth.json"
    });
    std::fs::create_dir_all(cursor.parent().unwrap()).unwrap();
    // Built at runtime, so a secret scan doesn't take the test's values for credentials.
    let token = json!({"accessToken": (["fake", "-access"].concat())});
    std::fs::write(&cursor, token.to_string()).unwrap();
    bin
}

/// `availableLogins` as `profile` sees them: agent, id.
fn offered(obid: &Obid, profile: Option<&str>) -> Vec<(String, String)> {
    let status = as_profile(obid, profile, "GET", "/v1/status", None);
    (status.body["availableLogins"].as_array().unwrap().iter())
        .map(|a| {
            (
                a["agent"].as_str().unwrap().into(),
                a["id"].as_str().unwrap().into(),
            )
        })
        .collect()
}

/// `otherProfileLogins` as `profile` sees them: agent, the holding profile's id and name.
fn held_elsewhere(obid: &Obid, profile: Option<&str>) -> Vec<(String, String, String)> {
    let status = as_profile(obid, profile, "GET", "/v1/status", None);
    let list = status.body["otherProfileLogins"].as_array();
    let list = list.unwrap_or_else(|| panic!("no otherProfileLogins: {}", status.text));
    list.iter()
        .map(|a| {
            let text = |key: &str| a[key].as_str().unwrap().to_owned();
            (text("agent"), text("profile"), text("profileName"))
        })
        .collect()
}

/// PROFILES-43: a profile is offered the Mac's own logins that no profile holds, and is told which
/// profile holds the others; one taken into a profile is that profile's alone.
#[test]
fn profiles_43_a_profile_is_offered_the_macs_logins_no_profile_holds() {
    let folder = Folder::new("profiles-43");
    let remote = folder.0.join("remote.git");
    let mut config = config(&folder.0, &remote, &agent("fake-claude"));
    config["agents"]["cursor"] = json!({"bin": agent("fake-cursor"), "accounts": []});
    let bin = logins_on_the_mac(&folder.0);
    let obid = start(&folder.0, &config, Some(&bin));
    let made = call(
        &obid,
        "POST",
        "/v1/profiles",
        Some(&json!({"name": "Acme", "color": "#7a4a8c"})),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let b = made.body["id"].as_str().unwrap().to_owned();
    let default = call(&obid, "GET", "/v1/profiles", None).body[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Codex's and Cursor's were listed in the default at the first start; Claude Code's (the
    // configuration has `stub`) is held by no profile.
    assert_eq!(
        offered(&obid, Some(&b)),
        [("claude".into(), "default".into())]
    );
    let theirs = |agent: &str| (agent.to_owned(), default.clone(), "Default".to_owned());
    assert_eq!(
        held_elsewhere(&obid, Some(&b)),
        [theirs("codex"), theirs("cursor")]
    );
    assert!(held_elsewhere(&obid, None).is_empty());

    let body = json!({"agent": "claude", "default": true});
    let taken = as_profile(&obid, Some(&b), "POST", "/v1/accounts", Some(&body));
    assert_eq!(taken.status, 201, "{}", taken.text);
    assert!(offered(&obid, None).is_empty());
    assert!(offered(&obid, Some(&b)).is_empty());
    assert_eq!(
        held_elsewhere(&obid, None),
        [("claude".into(), b.clone(), "Acme".into())]
    );
    assert_eq!(
        held_elsewhere(&obid, Some(&b)),
        [theirs("codex"), theirs("cursor")]
    );
}

/// A stopped obid's home with the default profile and profile B, conversation [`TWO`] made in B.
fn two_profiles(name: &str) -> (Folder, PathBuf) {
    let folder = Folder::new(name);
    let home = folder.0.join("home");
    std::fs::create_dir_all(home.join("profiles").join(B)).unwrap();
    let registry = json!({"profiles": [
        {"id": DEFAULT, "name": "Default", "color": "#3a5a8c", "default": true},
        {"id": B, "name": "Acme", "color": "#7a4a8c", "default": false}]});
    std::fs::write(home.join("profiles.json"), registry.to_string()).unwrap();
    let dir = home.join("conversations");
    std::fs::create_dir_all(&dir).unwrap();
    let turn = json!({"id": format!("{}0", &TWO[..35]), "who": "user", "text": "plan the acme launch",
        "at": "2026-11-15T10:00:00Z", "conversation": TWO, "jobs": []});
    let thread = json!({"id": TWO, "title": "Acme launch", "pinned": false, "profile": B,
        "created": "2026-11-15T10:00:00Z", "sequence": 1, "turns": [turn], "foreign": null});
    std::fs::write(dir.join(format!("{TWO}.json")), thread.to_string()).unwrap();
    (folder, home)
}

fn obid_in(folder: &Folder) -> (Obid, Value) {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = agent("fake-harness");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    (start(&folder.0, &config, None), config)
}

/// PROFILES-44: a deleted profile's conversations, projects, helpers, secrets and accounts are the
/// default's from then on, across a restart; nothing of them is deleted.
#[test]
fn profiles_44_a_deleted_profiles_things_become_the_defaults() {
    let (folder, home) = two_profiles("profiles-44");
    let (obid, config) = obid_in(&folder);
    let project = |profile, name: &str| {
        let made = as_profile(
            &obid,
            profile,
            "POST",
            "/v1/projects",
            Some(&json!({"name": name})),
        );
        assert_eq!(made.status, 201, "{}", made.text);
        made.body["id"].as_str().unwrap().to_owned()
    };
    let ours = project(None, "Oby");
    let theirs = project(Some(B), "Acme app");
    let helper = json!({"name": "Acme inbox", "color": "#3a5a8c", "instructions": "Sort my inbox.",
        "tool": "claude", "connectors": []});
    let helper = as_profile(&obid, Some(B), "POST", "/v1/helpers", Some(&helper));
    assert_eq!(helper.status, 201, "{}", helper.text);
    let helper = helper.body["id"].as_str().unwrap().to_owned();
    let value = json!({"value": format!("acme-{}", uuid::Uuid::new_v4().simple())});
    let put = signed_as(
        &obid,
        Some(B),
        "PUT",
        "/v1/obid/vault/acme-key",
        Some(&value),
    );
    assert_eq!(put.status, 204, "{}", put.text);
    let body = json!({"agent": "claude", "name": "Acme Claude"});
    let account = as_profile(&obid, Some(B), "POST", "/v1/accounts", Some(&body));
    assert_eq!(account.status, 201, "{}", account.text);
    let account = account.body["id"].as_str().unwrap().to_owned();

    let gone = call(&obid, "DELETE", &format!("/v1/profiles/{B}"), None);
    assert_eq!(gone.status, 204, "{}", gone.text);
    let listed = call(&obid, "GET", "/v1/profiles", None).body;
    assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed[0]["id"], DEFAULT);
    let kept: Value =
        serde_json::from_slice(&std::fs::read(home.join("profiles.json")).unwrap()).unwrap();
    assert_eq!(kept["merged"], json!([B]));
    assert!(!home.join("profiles").join(B).exists());
    let refused = as_profile(&obid, Some(B), "GET", "/v1/conversations", None);
    assert_eq!(refused.status, 400, "{}", refused.text);

    let check = |obid: &Obid| {
        assert!(ids(obid, None, "/v1/conversations", None).contains(&TWO.to_owned()));
        assert_eq!(
            ids(obid, None, "/v1/projects", None),
            [ours.clone(), theirs.clone()]
        );
        assert_eq!(
            ids(obid, None, "/v1/helpers", None),
            std::slice::from_ref(&helper)
        );
        let names = call(obid, "GET", "/v1/obid/vault", None).body["names"].clone();
        assert_eq!(names, json!(["acme-key"]));
        let accounts = ids(obid, None, "/v1/status", Some("accounts"));
        assert!(accounts.contains(&account) && accounts.contains(&"stub".to_owned()));
    };
    check(&obid);
    let mut obid = obid;
    assert!(obid.stop().success());
    let obid = start(&folder.0, &config, None);
    check(&obid);
}

/// PROFILES-45: the default profile, an id no profile has, and any profile while `profiles.json`
/// didn't read, are never deleted, and nothing is written.
#[test]
fn profiles_45_the_default_or_an_unknown_profile_is_not_deleted() {
    let (folder, home) = two_profiles("profiles-45");
    let (obid, config) = obid_in(&folder);
    let made = as_profile(
        &obid,
        Some(B),
        "POST",
        "/v1/projects",
        Some(&json!({"name": "Acme"})),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let registry = std::fs::read(home.join("profiles.json")).unwrap();
    let projects = std::fs::read(home.join("profiles").join(B).join("projects.json")).unwrap();
    let unchanged = || {
        assert_eq!(std::fs::read(home.join("profiles.json")).unwrap(), registry);
        let file = home.join("profiles").join(B).join("projects.json");
        assert_eq!(std::fs::read(file).unwrap(), projects);
        assert!(!home.join("projects.json").exists());
    };
    let default = call(&obid, "DELETE", &format!("/v1/profiles/{DEFAULT}"), None);
    assert_eq!(default.status, 409, "{}", default.text);
    assert!(default.body["error"].is_string());
    let unknown = call(&obid, "DELETE", &format!("/v1/profiles/{UNKNOWN}"), None);
    assert_eq!(unknown.status, 404, "{}", unknown.text);
    unchanged();

    let mut obid = obid;
    assert!(obid.stop().success());
    std::fs::write(home.join("profiles.json"), "not json").unwrap();
    let obid = start(&folder.0, &config, None);
    let unreadable = call(&obid, "DELETE", &format!("/v1/profiles/{B}"), None);
    assert_eq!(unreadable.status, 409, "{}", unreadable.text);
    assert_eq!(
        std::fs::read(home.join("profiles.json")).unwrap(),
        b"not json"
    );
    assert!(home.join("profiles").join(B).join("projects.json").exists());
}
