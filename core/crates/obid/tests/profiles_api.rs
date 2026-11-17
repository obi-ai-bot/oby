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

/// A conversation kept in a stopped obid's home: no turn, so no answer moves its revision.
const ONE: &str = "11111111-1111-4111-8111-111111111111";
const TWO: &str = "22222222-2222-4222-8222-222222222222";
/// Profile B, as a stopped obid's registry keeps it.
const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

/// `method` to `path` with `profile`, when there's one, as `Oby-Profile`.
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

/// The default profile's id, and a new profile's.
fn two_profiles(obid: &Obid) -> (String, String) {
    let first = call(obid, "GET", "/v1/profiles", None).body[0]["id"].clone();
    let body = json!({"name": "Acme", "color": "#7a4a8c"});
    let made = call(obid, "POST", "/v1/profiles", Some(&body));
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap().to_owned();
    (first.as_str().unwrap().to_owned(), id)
}

/// A project named `name` made in `profile`, answered `201`.
fn made_in(obid: &Obid, profile: Option<&str>, name: &str) -> Value {
    let body = json!({"name": name});
    let answer = as_profile(obid, profile, "POST", "/v1/projects", Some(&body));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body
}

fn listed_in(obid: &Obid, profile: Option<&str>) -> Value {
    let answer = as_profile(obid, profile, "GET", "/v1/projects", None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

/// The ids of the projects kept in `file`.
fn kept_ids(file: &Path) -> Vec<String> {
    let kept: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let projects = kept["projects"].as_array().unwrap().iter();
    projects
        .map(|p| p["id"].as_str().unwrap().to_owned())
        .collect()
}

/// Writes conversation `id` into the home of an obid not started yet, with `keys` over its own.
fn kept_conversation(home: &Path, id: &str, keys: Value) {
    let dir = home.join("conversations");
    std::fs::create_dir_all(&dir).unwrap();
    let mut thread = json!({"id": id, "title": "Kept", "pinned": false,
        "created": "2026-11-15T10:00:00Z", "sequence": 1, "turns": [], "foreign": null});
    for (key, value) in keys.as_object().unwrap() {
        thread[key] = value.clone();
    }
    std::fs::write(dir.join(format!("{id}.json")), thread.to_string()).unwrap();
}

fn conversation(obid: &Obid, id: &str) -> Value {
    let answer = call(obid, "GET", "/v1/conversations", None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    let list = answer.body.as_array().unwrap();
    let found = list.iter().find(|c| c["id"] == id);
    found
        .unwrap_or_else(|| panic!("no conversation {id} in {}", answer.text))
        .clone()
}

/// PROFILES-26: a project made in one profile is listed, changed and deleted only in it.
#[test]
fn profiles_26_a_project_is_kept_in_its_own_profile() {
    let folder = Folder::new("profiles-26");
    let home = folder.0.join("home");
    let obid = harness_obid(&folder);
    let (first, b) = two_profiles(&obid);

    let theirs = made_in(&obid, Some(&b), "Acme app");
    assert_eq!(listed_in(&obid, Some(&b)), json!([theirs]));
    assert_eq!(listed_in(&obid, None), json!([]));
    assert_eq!(listed_in(&obid, Some(&first)), json!([]));
    let ours = made_in(&obid, None, "Oby");
    assert_eq!(listed_in(&obid, Some(&b)), json!([theirs]));

    let file = home.join("profiles").join(&b).join("projects.json");
    let theirs_id = theirs["id"].as_str().unwrap();
    let ours_id = ours["id"].as_str().unwrap();
    assert_eq!(kept_ids(&file), vec![theirs_id]);
    assert_eq!(mode(&file), 0o600);
    assert_eq!(kept_ids(&home.join("projects.json")), vec![ours_id]);

    let before = std::fs::read(&file).unwrap();
    let home_before = std::fs::read(home.join("projects.json")).unwrap();
    let rename = json!({"name": "Moved"});
    for (profile, id) in [(None, theirs_id), (Some(b.as_str()), ours_id)] {
        let path = format!("/v1/projects/{id}");
        let changed = as_profile(&obid, profile, "PATCH", &path, Some(&rename));
        assert_eq!(changed.status, 404, "{}", changed.text);
        let gone = as_profile(&obid, profile, "DELETE", &path, None);
        assert_eq!(gone.status, 404, "{}", gone.text);
    }
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert_eq!(
        std::fs::read(home.join("projects.json")).unwrap(),
        home_before
    );
    assert_eq!(listed_in(&obid, Some(&b)), json!([theirs]));
    assert_eq!(listed_in(&obid, None), json!([ours]));
}

/// PROFILES-27: a conversation joins only a project of the profile the request names.
#[test]
fn profiles_27_a_conversation_joins_only_a_project_of_its_profile() {
    let folder = Folder::new("profiles-27");
    let home = folder.0.join("home");
    std::fs::create_dir_all(home.join("profiles").join(B)).unwrap();
    let registry = json!({"profiles": [
        {"id": "3f0c2a4e-9b1d-4c8e-a7f2-5d6e8b9c0a1f", "name": "Personal", "color": "#3a5a8c",
         "default": true},
        {"id": B, "name": "Acme", "color": "#7a4a8c", "default": false}]});
    std::fs::write(home.join("profiles.json"), registry.to_string()).unwrap();
    // ONE is the default profile's; TWO was made in B (PROFILES-33).
    kept_conversation(&home, ONE, json!({}));
    kept_conversation(&home, TWO, json!({"profile": B}));
    let obid = harness_obid(&folder);
    let b = B.to_owned();
    let project = made_in(&obid, Some(&b), "Acme app");
    let before = conversation(&obid, ONE);
    let path = format!("/v1/conversations/{ONE}");
    let put = json!({"project": project["id"]});

    let refused = as_profile(&obid, None, "PATCH", &path, Some(&put));
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(conversation(&obid, ONE), before);
    assert_eq!(before["project"], Value::Null);

    let path = format!("/v1/conversations/{TWO}");
    let joined = as_profile(&obid, Some(&b), "PATCH", &path, Some(&put));
    assert_eq!(joined.status, 200, "{}", joined.text);
    assert_eq!(joined.body["project"], project["id"]);
    let listed = as_profile(&obid, Some(&b), "GET", "/v1/conversations", None);
    assert_eq!(listed.body[0]["id"], TWO, "{}", listed.text);
    assert_eq!(listed.body[0]["project"], project["id"]);

    let plan = json!({"said": "tidy up", "new": true, "project": project["id"]});
    let started = as_profile(&obid, None, "POST", "/v1/plans", Some(&plan));
    assert_eq!(started.status, 422, "{}", started.text);
    let listed = call(&obid, "GET", "/v1/conversations", None);
    assert_eq!(listed.body.as_array().unwrap().len(), 1, "{}", listed.text);
}

/// PROFILES-28: a request naming a profile this Mac doesn't have, or naming two, is refused on
/// every project route and changes nothing; it never reaches the default profile.
#[test]
fn profiles_28_an_unknown_profile_is_refused_and_changes_nothing() {
    let folder = Folder::new("profiles-28");
    let home = folder.0.join("home");
    let obid = harness_obid(&folder);
    let (_, b) = two_profiles(&obid);
    let ours = made_in(&obid, None, "Oby");
    made_in(&obid, Some(&b), "Acme app");
    let files = [
        home.join("projects.json"),
        home.join("profiles").join(&b).join("projects.json"),
    ];
    let before: Vec<Vec<u8>> = files.iter().map(|f| std::fs::read(f).unwrap()).collect();

    let unknown = "44444444-4444-4444-8444-444444444444";
    let named: [&[&str]; 4] = [&[unknown], &["../x"], &[""], &[&b, &b]];
    let path = format!("/v1/projects/{}", ours["id"].as_str().unwrap());
    let routes = [
        ("GET", "/v1/projects", None),
        ("POST", "/v1/projects", Some(json!({"name": "A"}))),
        ("PATCH", path.as_str(), Some(json!({"name": "A"}))),
        ("DELETE", path.as_str(), None),
    ];
    for values in named {
        let headers: Vec<(&str, &str)> = values.iter().map(|v| ("Oby-Profile", *v)).collect();
        for (method, path, body) in &routes {
            let answer = call_with_headers(&obid, TOKEN, method, path, body.as_ref(), &headers);
            assert_eq!(answer.status, 400, "{values:?} {method}: {}", answer.text);
            let error = answer.body["error"].as_str().unwrap_or_default();
            assert!(!error.is_empty(), "{values:?} {method}: {}", answer.text);
        }
    }
    for (file, bytes) in files.iter().zip(&before) {
        assert_eq!(&std::fs::read(file).unwrap(), bytes, "{}", file.display());
    }
    assert!(!home.join("profiles").join(unknown).exists());
    assert!(!home.join("x").exists());
    assert_eq!(listed_in(&obid, None), json!([ours]));
}

/// PROFILES-29: naming no profile is naming the default, and switching between profiles moves
/// nothing: each keeps exactly the projects made in it.
#[test]
fn profiles_29_no_profile_named_is_the_default_and_moves_nothing() {
    let folder = Folder::new("profiles-29");
    let home = folder.0.join("home");
    let obid = harness_obid(&folder);
    let (first, b) = two_profiles(&obid);

    let (mut ours, mut theirs) = (vec![], vec![]);
    for n in 0..3 {
        let named = if n == 1 { Some(first.as_str()) } else { None };
        ours.push(made_in(&obid, named, &format!("Oby {n}")));
        theirs.push(made_in(&obid, Some(&b), &format!("Acme {n}")));
        assert_eq!(listed_in(&obid, None), json!(ours));
        assert_eq!(listed_in(&obid, Some(&first)), json!(ours));
        assert_eq!(listed_in(&obid, Some(&b)), json!(theirs));
    }
    let ids = |made: &[Value]| -> Vec<String> {
        let ids = made.iter().map(|p| p["id"].as_str().unwrap().to_owned());
        ids.collect()
    };
    assert_eq!(kept_ids(&home.join("projects.json")), ids(&ours));
    let file = home.join("profiles").join(&b).join("projects.json");
    assert_eq!(kept_ids(&file), ids(&theirs));
}

/// PROFILES-30: an install with one profile keeps its projects as before: its `projects.json`
/// untouched by the start, every route as PROJECTS-1, 3 and 8, and no `profiles` folder made.
#[test]
fn profiles_30_one_profile_keeps_projects_as_before() {
    let folder = Folder::new("profiles-30");
    let home = folder.0.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let registry = json!({"profiles": [{"id": "3f0c2a4e-9b1d-4c8e-a7f2-5d6e8b9c0a1f",
        "name": "Personal", "color": "#3a5a8c", "default": true}]});
    std::fs::write(home.join("profiles.json"), registry.to_string()).unwrap();
    let project = json!({"id": "33333333-3333-4333-8333-333333333333", "name": "Oby",
        "folders": ["/b"], "repos": []});
    let kept = serde_json::to_vec_pretty(&json!({"projects": [project]})).unwrap();
    std::fs::write(home.join("projects.json"), &kept).unwrap();
    kept_conversation(
        &home,
        ONE,
        json!({"project": project["id"], "projectFolders": ["/b"]}),
    );
    let obid = harness_obid(&folder);

    assert_eq!(std::fs::read(home.join("projects.json")).unwrap(), kept);
    assert_eq!(listed_in(&obid, None), json!([project]));
    assert_eq!(conversation(&obid, ONE)["project"], project["id"]);
    let other = made_in(&obid, None, "Bare");
    assert_eq!(listed_in(&obid, None), json!([project, other]));
    let path = format!("/v1/projects/{}", project["id"].as_str().unwrap());
    let renamed = call(&obid, "PATCH", &path, Some(&json!({"name": "Oby app"})));
    assert_eq!(renamed.status, 200, "{}", renamed.text);
    assert_eq!(renamed.body["name"], "Oby app");
    let gone = call(&obid, "DELETE", &path, None);
    assert_eq!(gone.status, 204, "{}", gone.text);
    let now = conversation(&obid, ONE);
    assert_eq!(now["project"], Value::Null, "{now}");
    assert_eq!(now["folders"], json!(["/b"]), "{now}");
    assert_eq!(listed_in(&obid, None), json!([other]));
    assert!(!home.join("profiles").exists());
}

/// A stopped obid's home with profile B, whose `projects.json` is `projects`; conversation [`ONE`]
/// in B's project P, and [`TWO`] in a project no profile has.
fn profile_b_home(name: &str, projects: &[u8]) -> (Folder, std::path::PathBuf) {
    let folder = Folder::new(name);
    let home = folder.0.join("home");
    std::fs::create_dir_all(home.join("profiles").join(B)).unwrap();
    let registry = json!({"profiles": [
        {"id": "3f0c2a4e-9b1d-4c8e-a7f2-5d6e8b9c0a1f", "name": "Personal", "color": "#3a5a8c",
         "default": true},
        {"id": B, "name": "Acme", "color": "#7a4a8c", "default": false}]});
    std::fs::write(home.join("profiles.json"), registry.to_string()).unwrap();
    std::fs::write(
        home.join("profiles").join(B).join("projects.json"),
        projects,
    )
    .unwrap();
    kept_conversation(
        &home,
        ONE,
        json!({"project": "33333333-3333-4333-8333-333333333333", "projectFolders": ["/b"]}),
    );
    kept_conversation(
        &home,
        TWO,
        json!({"folders": ["/a"], "project": "00000000-0000-4000-8000-000000000000",
               "projectFolders": ["/b"]}),
    );
    (folder, home)
}

/// PROFILES-31: at start, a conversation in any profile's project gets its folders as they are
/// now, one in a project no profile has leaves it, and an unreadable profile file changes none.
#[test]
fn profiles_31_the_start_reconciles_every_profiles_projects() {
    let project = json!({"id": "33333333-3333-4333-8333-333333333333", "name": "Acme app",
        "folders": ["/c"], "repos": []});
    let kept = json!({"projects": [project]}).to_string();
    let (folder, home) = profile_b_home("profiles-31", kept.as_bytes());
    let obid = harness_obid(&folder);

    let one = conversation(&obid, ONE);
    assert_eq!(one["project"], project["id"], "{one}");
    assert_eq!(one["folders"], json!(["/c"]), "{one}");
    let two = conversation(&obid, TWO);
    assert_eq!(two["project"], Value::Null, "{two}");
    assert_eq!(two["folders"], json!(["/b"]), "{two}");
    assert!(!home.join("projects.json").exists());

    let (folder, home) = profile_b_home("profiles-31-unreadable", b"not json");
    let files = [ONE, TWO].map(|id| home.join(format!("conversations/{id}.json")));
    let before = files.clone().map(|f| std::fs::read(f).unwrap());
    let obid = harness_obid(&folder);
    assert_eq!(conversation(&obid, ONE)["folders"], json!(["/b"]));
    assert_eq!(conversation(&obid, TWO)["folders"], json!(["/b"]));
    for (file, bytes) in files.iter().zip(&before) {
        assert_eq!(&std::fs::read(file).unwrap(), bytes, "{}", file.display());
    }
    let b_file = home.join("profiles").join(B).join("projects.json");
    assert_eq!(std::fs::read(b_file).unwrap(), b"not json");
    assert!(!home.join("projects.json").exists());
}
