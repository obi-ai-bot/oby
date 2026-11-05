//! Chrome-style profiles (docs/contracts/profiles.md): `GET /v1/profiles` and the upgrade that
//! gives an install with no `profiles.json` its first profile. Each test is named after the
//! contract's scenario it proves.
mod support;

use std::path::Path;

use serde_json::{Value, json};
use support::*;

fn harness_obid(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    start(&folder.0, &config, None)
}

/// PROFILES-1: an install with no `profiles.json` gets one first profile that holds everything it
/// already has; nothing moves, renames or signs out, and `GET /v1/profiles` shows it.
#[test]
fn profiles_1_upgrade_keeps_everything_in_the_first_profile() {
    let folder = Folder::new("profiles-1");
    let home = folder.0.join("home");
    std::fs::create_dir_all(home.join("accounts/stub")).unwrap();
    let accounts = br#"{"identities":{"stub":{"email":"a@b.dev"}}}"#;
    let usage = br#"{"stub":{"fiveHour":0.1}}"#;
    let permissions = br#"{"mode":"bypass"}"#;
    std::fs::write(home.join("accounts.json"), accounts).unwrap();
    std::fs::write(home.join("usage.json"), usage).unwrap();
    std::fs::write(home.join("permissions.json"), permissions).unwrap();
    assert!(!home.join("profiles.json").exists());

    let obid = harness_obid(&folder);
    let answer = call(&obid, "GET", "/v1/profiles", None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    let list = answer.body.as_array().expect("a list");
    assert_eq!(list.len(), 1, "{list:?}");
    let profile = &list[0];
    assert_eq!(profile["name"], "Personal");
    assert_eq!(profile["color"], "#3a5a8c");
    assert_eq!(profile["default"], true);
    assert_eq!(profile["waiting"], false);
    assert!(
        profile["id"].as_str().is_some_and(|id| !id.is_empty()),
        "{profile}"
    );
    assert_eq!(
        profile.as_object().map(|o| o.len()),
        Some(5),
        "only id, name, color, default, waiting: {profile}"
    );

    let kept: Value = serde_json::from_slice(&std::fs::read(home.join("profiles.json")).unwrap())
        .expect("profiles.json");
    assert_eq!(kept["profiles"].as_array().map(|a| a.len()), Some(1));
    assert_eq!(kept["profiles"][0]["id"], profile["id"]);
    assert_eq!(kept["profiles"][0]["name"], "Personal");
    assert_eq!(kept["profiles"][0]["default"], true);

    assert_eq!(
        std::fs::read(home.join("accounts.json")).unwrap(),
        accounts,
        "accounts.json untouched by the upgrade"
    );
    assert_eq!(
        std::fs::read(home.join("usage.json")).unwrap(),
        usage,
        "usage.json untouched by the upgrade"
    );
    assert!(
        home.join("accounts/stub").is_dir(),
        "account folder still there: nothing signed out or moved"
    );
    let status = call(&obid, "GET", "/v1/status", None);
    assert_eq!(status.status, 200, "{}", status.text);
    let listed = status.body["accounts"].as_array().unwrap();
    assert!(
        listed.iter().any(|a| a["id"] == "stub"),
        "account still listed: {}",
        status.text
    );
}

/// A second start with the registry already there answers the same profile, and leaves the file.
#[test]
fn profiles_get_returns_the_saved_registry() {
    let folder = Folder::new("profiles-get");
    let home = folder.0.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let registry = json!({
        "profiles": [{
            "id": "11111111-1111-1111-1111-111111111111",
            "name": "Work",
            "color": "#7a4a8c",
            "default": true
        }]
    });
    std::fs::write(
        home.join("profiles.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    let before = std::fs::read(home.join("profiles.json")).unwrap();

    let obid = harness_obid(&folder);
    let answer = call(&obid, "GET", "/v1/profiles", None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert_eq!(
        answer.body,
        json!([{
            "id": "11111111-1111-1111-1111-111111111111",
            "name": "Work",
            "color": "#7a4a8c",
            "default": true,
            "waiting": false
        }])
    );
    assert_eq!(std::fs::read(home.join("profiles.json")).unwrap(), before);
}
