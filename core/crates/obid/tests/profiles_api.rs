//! Chrome-style profiles (docs/contracts/profiles.md): `GET /v1/profiles`, the upgrade that
//! gives an install with no `profiles.json` its first profile, and creating and renaming one. Each
//! test is named after the contract's scenario it proves.
mod support;

use std::os::unix::fs::PermissionsExt;
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
    assert_eq!(profile["name"], "Default");
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
    assert_eq!(kept["profiles"][0]["name"], "Default");
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

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// Accounts, usage and permissions already in `home`, as an install that has used Oby has them.
fn seed(home: &Path) -> Vec<(&'static str, Vec<u8>)> {
    std::fs::create_dir_all(home.join("accounts/stub")).unwrap();
    let files = vec![
        (
            "accounts.json",
            br#"{"identities":{"stub":{"email":"a@b.dev"}}}"#.to_vec(),
        ),
        ("usage.json", br#"{"stub":{"fiveHour":0.1}}"#.to_vec()),
        ("permissions.json", br#"{"mode":"bypass"}"#.to_vec()),
    ];
    for (name, bytes) in &files {
        std::fs::write(home.join(name), bytes).unwrap();
    }
    files
}

fn account_listed(obid: &Obid) -> bool {
    let status = call(obid, "GET", "/v1/status", None);
    assert_eq!(status.status, 200, "{}", status.text);
    status.body["accounts"]
        .as_array()
        .is_some_and(|a| a.iter().any(|a| a["id"] == "stub"))
}

fn is_lower_uuid(id: &str) -> bool {
    id.len() == 36
        && uuid::Uuid::parse_str(id).is_ok()
        && !id.chars().any(|c| c.is_ascii_uppercase())
}

/// PROFILES-13: a new profile is kept with a lower-case UUID and its own empty folder (0700);
/// the first profile and every other file stay as they were.
#[test]
fn profiles_13_a_new_profile_is_added_with_its_own_folder() {
    let folder = Folder::new("profiles-13");
    let home = folder.0.join("home");
    let seeded = seed(&home);
    let obid = harness_obid(&folder);

    let made = call(
        &obid,
        "POST",
        "/v1/profiles",
        Some(&json!({"name": "  Acme  ", "color": "#7a4a8c"})),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    assert_eq!(
        made.body.as_object().map(|o| o.len()),
        Some(5),
        "{}",
        made.text
    );
    assert_eq!(made.body["name"], "Acme");
    assert_eq!(made.body["color"], "#7a4a8c");
    assert_eq!(made.body["default"], false);
    assert_eq!(made.body["waiting"], false);
    let id = made.body["id"].as_str().unwrap();
    assert!(is_lower_uuid(id), "{id}");

    let list = call(&obid, "GET", "/v1/profiles", None).body;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!(list[0]["default"], true);
    assert_eq!(list[1], made.body);
    let kept: Value =
        serde_json::from_slice(&std::fs::read(home.join("profiles.json")).unwrap()).unwrap();
    assert_eq!(kept["profiles"].as_array().map(Vec::len), Some(2));
    assert_eq!(kept["profiles"][1]["id"], id);
    assert_eq!(mode(&home.join("profiles.json")), 0o600);

    let root = home.join("profiles").join(id);
    assert!(root.is_dir());
    assert_eq!(mode(&root), 0o700);
    assert_eq!(
        std::fs::read_dir(&root).unwrap().count(),
        0,
        "its folder is empty"
    );

    for (name, bytes) in &seeded {
        assert_eq!(&std::fs::read(home.join(name)).unwrap(), bytes, "{name}");
    }
    assert!(account_listed(&obid));
}

/// PROFILES-14: a bad name or colour, a body that isn't a profile, or an unknown key is refused
/// with a reason, and nothing is written.
#[test]
fn profiles_14_a_bad_name_or_colour_writes_nothing() {
    let folder = Folder::new("profiles-14");
    let home = folder.0.join("home");
    let obid = harness_obid(&folder);
    let before = std::fs::read(home.join("profiles.json")).unwrap();

    let long = "a".repeat(41);
    let bodies = [
        Some(json!({"name": "", "color": "#7a4a8c"})),
        Some(json!({"name": "   ", "color": "#7a4a8c"})),
        Some(json!({"name": long, "color": "#7a4a8c"})),
        Some(json!({"name": "Acme", "color": "red"})),
        Some(json!({"name": "Acme", "color": "#12345"})),
        Some(json!({"name": "Acme"})),
        Some(json!({"name": "Acme", "color": "#7a4a8c", "default": true})),
        Some(json!("not json")),
        None,
    ];
    for body in &bodies {
        let answer = call(&obid, "POST", "/v1/profiles", body.as_ref());
        assert_eq!(answer.status, 400, "{body:?}: {}", answer.text);
        let error = answer.body["error"].as_str().unwrap_or_default();
        assert!(
            !error.is_empty() && !error.contains('\n'),
            "{body:?}: {}",
            answer.text
        );
    }
    assert_eq!(std::fs::read(home.join("profiles.json")).unwrap(), before);
    assert!(!home.join("profiles").exists());
    let list = call(&obid, "GET", "/v1/profiles", None).body;
    assert_eq!(list.as_array().map(Vec::len), Some(1));
}

/// PROFILES-15: a profile's name and colour change, and nothing else of it; the default stays the
/// one default, and a refused change writes nothing.
#[test]
fn profiles_15_a_profile_changes_only_its_name_and_colour() {
    let folder = Folder::new("profiles-15");
    let home = folder.0.join("home");
    let obid = harness_obid(&folder);
    let first = call(&obid, "GET", "/v1/profiles", None).body[0].clone();
    let first_id = first["id"].as_str().unwrap();

    let renamed = call(
        &obid,
        "PATCH",
        &format!("/v1/profiles/{first_id}"),
        Some(&json!({"name": "Home"})),
    );
    assert_eq!(renamed.status, 200, "{}", renamed.text);
    assert_eq!(renamed.body["name"], "Home");
    assert_eq!(renamed.body["id"], first["id"]);
    assert_eq!(renamed.body["color"], first["color"]);
    assert_eq!(renamed.body["default"], true);

    let made = call(
        &obid,
        "POST",
        "/v1/profiles",
        Some(&json!({"name": "Acme", "color": "#7a4a8c"})),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap();
    let recoloured = call(
        &obid,
        "PATCH",
        &format!("/v1/profiles/{id}"),
        Some(&json!({"color": "#112233"})),
    );
    assert_eq!(recoloured.status, 200, "{}", recoloured.text);
    let mut expected = made.body.clone();
    expected["color"] = json!("#112233");
    assert_eq!(recoloured.body, expected);

    let before = std::fs::read(home.join("profiles.json")).unwrap();
    for body in [json!({"default": true}), json!({"id": "x"}), json!({})] {
        let answer = call(&obid, "PATCH", &format!("/v1/profiles/{id}"), Some(&body));
        assert_eq!(answer.status, 400, "{body}: {}", answer.text);
    }
    let unknown = format!("/v1/profiles/{}", uuid::Uuid::new_v4());
    let answer = call(&obid, "PATCH", &unknown, Some(&json!({"name": "Gone"})));
    assert_eq!(answer.status, 404, "{}", answer.text);
    assert_eq!(std::fs::read(home.join("profiles.json")).unwrap(), before);

    let list = call(&obid, "GET", "/v1/profiles", None).body;
    let defaults: Vec<_> = (list.as_array().unwrap().iter())
        .filter(|p| p["default"] == true)
        .collect();
    assert_eq!(defaults.len(), 1, "{list}");
    assert_eq!(defaults[0]["id"], first["id"]);
}

/// PROFILES-16: a `profiles.json` that didn't read at start is never written over: creating and
/// renaming are refused.
#[test]
fn profiles_16_an_unreadable_registry_is_never_written_over() {
    let folder = Folder::new("profiles-16");
    let home = folder.0.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("profiles.json"), "not json").unwrap();
    let obid = harness_obid(&folder);

    let list = call(&obid, "GET", "/v1/profiles", None).body;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 1, "{list:?}");
    let id = list[0]["id"].as_str().unwrap();
    let made = call(
        &obid,
        "POST",
        "/v1/profiles",
        Some(&json!({"name": "Acme", "color": "#7a4a8c"})),
    );
    assert_eq!(made.status, 409, "{}", made.text);
    assert!(
        made.body["error"]
            .as_str()
            .is_some_and(|e| e.contains("profiles.json"))
    );
    let renamed = call(
        &obid,
        "PATCH",
        &format!("/v1/profiles/{id}"),
        Some(&json!({"name": "Home"})),
    );
    assert_eq!(renamed.status, 409, "{}", renamed.text);
    assert_eq!(
        std::fs::read(home.join("profiles.json")).unwrap(),
        b"not json"
    );
    assert!(!home.join("profiles").exists());
}

/// PROFILES-18: an install that already has one profile starts as before: the same profile, every
/// file as it was, no `profiles` folder, the account still listed.
#[test]
fn profiles_18_an_install_with_one_profile_behaves_as_before() {
    let folder = Folder::new("profiles-18");
    let home = folder.0.join("home");
    let seeded = seed(&home);
    let profile = json!({
        "id": "3f0c2a4e-9b1d-4c8e-a7f2-5d6e8b9c0a1f",
        "name": "Personal",
        "color": "#3a5a8c",
        "default": true
    });
    let registry = serde_json::to_vec_pretty(&json!({"profiles": [profile]})).unwrap();
    std::fs::write(home.join("profiles.json"), &registry).unwrap();
    let obid = harness_obid(&folder);

    let mut expected = profile.clone();
    expected["waiting"] = json!(false);
    let answer = call(&obid, "GET", "/v1/profiles", None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert_eq!(answer.body, json!([expected]));
    assert_eq!(std::fs::read(home.join("profiles.json")).unwrap(), registry);
    for (name, bytes) in &seeded {
        assert_eq!(&std::fs::read(home.join(name)).unwrap(), bytes, "{name}");
    }
    assert!(!home.join("profiles").exists());
    assert!(account_listed(&obid));
}
