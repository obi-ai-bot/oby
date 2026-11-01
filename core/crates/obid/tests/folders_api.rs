//! Working folders at obid's API boundary (docs/contracts/folders.md): `status.folders`, the folders
//! `POST /v1/plans` adds to a conversation, kept for later words and replaced with `PATCH`, every
//! path checked and canonicalized, and the places Oby never works in refused with nothing kept.
//! Each test is named after the contract's scenario it proves. Through the gateway: gateway_api.rs.
mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

fn harness_obid(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    start_at_home(folder, &config)
}

/// The test's own home (obid's `HOME` in tests): the person's home its obid checks folders against.
fn home_of(folder: &Folder) -> PathBuf {
    folder.0.join("env-home")
}

/// obid with the test's home as the person's only one (`OBID_PERSON_HOMES`), so the folders it
/// picks there pass wherever the test runs: never this Mac's own home, whose `~/Library` or
/// `/private/tmp` may hold cargo's folders (as in Oby's own jobs).
fn start_at_home(folder: &Folder, config: &Value) -> Obid {
    let home = home_of(folder);
    std::fs::create_dir_all(&home).unwrap();
    start_with(
        &folder.0,
        config,
        None,
        &[("OBID_PERSON_HOMES", home.to_str().unwrap())],
    )
}

/// Folders the owner picks, in the test's home.
fn picked(folder: &Folder, names: &[&str]) -> Vec<PathBuf> {
    let root = home_of(folder).join("Projects");
    let _ = std::fs::remove_dir_all(&root);
    names
        .iter()
        .map(|name| {
            let path = root.join(name);
            std::fs::create_dir_all(&path).unwrap();
            path.canonicalize().unwrap()
        })
        .collect()
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

fn plan(obid: &Obid, body: Value) -> Value {
    let answer = call(obid, "POST", "/v1/plans", Some(&body));
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

fn folders_of(obid: &Obid, id: &str) -> Value {
    let answer = call(obid, "GET", "/v1/conversations", None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("no conversation {id} in {}", answer.text))["folders"]
        .clone()
}

#[test]
fn folders_6_a_conversation_keeps_its_folders_for_later_words() {
    let folder = Folder::new("folders-6");
    let obid = harness_obid(&folder);
    let [api, web] = <[PathBuf; 2]>::try_from(picked(&folder, &["api", "web"])).unwrap();
    let status = call(&obid, "GET", "/v1/status", None);
    assert_eq!(status.body["folders"], true, "{}", status.text);

    let first = plan(
        &obid,
        json!({"said": "rename the User type everywhere", "new": true,
               "folders": [text(&api), text(&web), format!("{}/", text(&api))]}),
    );
    let id = first["conversation"].as_str().unwrap().to_owned();
    // The same folder twice is kept once, in the order picked.
    assert_eq!(folders_of(&obid, &id), json!([text(&api), text(&web)]));

    // Later words with no folders keep them.
    plan(
        &obid,
        json!({"said": "and the tests too", "conversation": id}),
    );
    assert_eq!(folders_of(&obid, &id), json!([text(&api), text(&web)]));

    // PATCH replaces the list; an empty one clears it.
    let patched = call(
        &obid,
        "PATCH",
        &format!("/v1/conversations/{id}"),
        Some(&json!({"folders": [text(&web)]})),
    );
    assert_eq!(patched.status, 200, "{}", patched.text);
    assert_eq!(patched.body["folders"], json!([text(&web)]));
    assert_eq!(folders_of(&obid, &id), json!([text(&web)]));
    let cleared = call(
        &obid,
        "PATCH",
        &format!("/v1/conversations/{id}"),
        Some(&json!({"folders": []})),
    );
    assert_eq!(cleared.body["folders"], json!([]), "{}", cleared.text);
}

#[test]
fn folders_9_refused_paths_keep_nothing() {
    let folder = Folder::new("folders-9");
    let remote = fixture(&folder.0.join("fixture"));
    let [login, file_parent, agent, keys] = <[PathBuf; 4]>::try_from(picked(
        &folder,
        &["login", "parent", "agent/claude-real", "keys/ssh-real"],
    ))
    .unwrap();
    let mut config = config(&folder.0, &remote, &stub_agent());
    config["agents"]["claude"]["accounts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "other", "name": "Other", "configDir": login}));
    // The person's ~/.claude and ~/.ssh are links to elsewhere: where they lead is guarded too.
    let env_home = home_of(&folder);
    std::fs::create_dir_all(env_home.join("Library")).unwrap();
    std::os::unix::fs::symlink(&agent, env_home.join(".claude")).unwrap();
    std::os::unix::fs::symlink(&keys, env_home.join(".ssh")).unwrap();
    let obid = start_at_home(&folder, &config);
    let file = file_parent.join("notes.txt");
    std::fs::write(&file, "notes").unwrap();
    let home = text(&env_home);
    let obid_home = folder.0.join("home");
    let holding = folder.0.file_name().unwrap().to_str().unwrap();
    // On macOS the test's own folders are in /private/var, the system's.
    let ours = |name: &str| {
        if cfg!(target_os = "macos") {
            format!("\"{name}\" is where your Mac keeps its own files")
        } else {
            format!("\"{name}\" holds Oby's own files")
        }
    };

    let mut refused = vec![
        ("/".to_owned(), "\"/\" is your whole disk".to_owned()),
        (home.clone(), "is your whole home folder".to_owned()),
        (
            format!("{home}/Library"),
            "\"Library\" is where your Mac keeps its own settings".to_owned(),
        ),
        (text(&obid_home), ours("home")),
        (text(&folder.0), ours(holding)),
        (text(&login), "\"login\" holds an agent's login".to_owned()),
        (text(&login.join("..")), "holds an agent's login".to_owned()),
        (
            text(&agent),
            "\"claude-real\" holds an agent's login".to_owned(),
        ),
        (
            text(agent.parent().unwrap()),
            "\"agent\" holds an agent's login".to_owned(),
        ),
        (
            text(&keys),
            "\"ssh-real\" holds your keys or logins".to_owned(),
        ),
        (
            text(keys.parent().unwrap()),
            "\"keys\" holds your keys or logins".to_owned(),
        ),
        (
            "/usr/local".to_owned(),
            "\"local\" is where your Mac keeps its own files".to_owned(),
        ),
        (
            text(&file),
            "\"notes.txt\" is a file, not a folder.".to_owned(),
        ),
        (
            "Projects/api".to_owned(),
            "\"api\" isn't a folder's full path.".to_owned(),
        ),
        (
            text(&file_parent.join("gone")),
            "\"gone\" isn't at".to_owned(),
        ),
    ];
    if cfg!(target_os = "macos") {
        refused.push((
            format!("{home}/LIBRARY"),
            "\"LIBRARY\" is where your Mac keeps".to_owned(),
        ));
    }
    for (path, why) in refused {
        let answer = call(
            &obid,
            "POST",
            "/v1/plans",
            Some(&json!({"said": "tidy up", "new": true, "folders": [path]})),
        );
        assert_eq!(answer.status, 422, "{path}: {}", answer.text);
        let error = answer.body["error"].as_str().unwrap();
        assert!(error.contains(&why), "{path}: {error}");
    }
    // Nothing of a refused request happened: no conversation.
    let list = call(&obid, "GET", "/v1/conversations", None);
    assert_eq!(list.body, json!([]), "{}", list.text);
}

#[test]
fn folders_10_a_link_is_kept_as_the_folder_it_points_to() {
    let folder = Folder::new("folders-10");
    let obid = harness_obid(&folder);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["api"])).unwrap();
    let work = api.parent().unwrap().join("work");
    std::os::unix::fs::symlink(&api, &work).unwrap();

    let first = plan(
        &obid,
        json!({"said": "look at it", "new": true, "folders": [text(&work)]}),
    );
    let id = first["conversation"].as_str().unwrap();
    assert_eq!(folders_of(&obid, id), json!([text(&api)]));
}

#[test]
fn folders_22_the_same_words_twice_add_nothing() {
    let folder = Folder::new("folders-22");
    let obid = harness_obid(&folder);
    let [api, web] = <[PathBuf; 2]>::try_from(picked(&folder, &["api", "web"])).unwrap();
    let words = "11111111-2222-4333-8444-555555555555";

    let first = plan(
        &obid,
        json!({"said": "tidy up", "new": true, "id": words, "folders": [text(&api)]}),
    );
    let id = first["conversation"].as_str().unwrap().to_owned();
    let again = plan(
        &obid,
        json!({"said": "tidy up", "new": true, "id": words, "folders": [text(&web)]}),
    );
    assert_eq!(again["repeated"], true, "{again}");
    assert_eq!(again["conversation"], id.as_str());
    assert_eq!(folders_of(&obid, &id), json!([text(&api)]));
    // A retry whose folder is refused now is still the same words: repeated, never a 422.
    let gone = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "tidy up", "new": true, "id": words, "folders": ["/usr"]})),
    );
    assert_eq!(gone.status, 200, "{}", gone.text);
    assert_eq!(gone.body["repeated"], true, "{}", gone.text);
    assert_eq!(folders_of(&obid, &id), json!([text(&api)]));
}

#[test]
fn folders_a_conversation_works_in_at_most_20() {
    let folder = Folder::new("folders-most");
    let obid = harness_obid(&folder);
    let names: Vec<String> = (0..21).map(|n| format!("f{n}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let all: Vec<String> = picked(&folder, &names).iter().map(|p| text(p)).collect();
    let at_most = "A conversation works in at most 20 folders.";

    let too_many = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "tidy up", "new": true, "folders": all})),
    );
    assert_eq!(too_many.status, 422, "{}", too_many.text);
    assert_eq!(too_many.body["error"], at_most);

    let first = plan(
        &obid,
        json!({"said": "tidy up", "new": true, "folders": all[..20]}),
    );
    let id = first["conversation"].as_str().unwrap().to_owned();
    let one_more = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "and this", "conversation": id, "folders": [all[20]]})),
    );
    assert_eq!(one_more.status, 422, "{}", one_more.text);
    assert_eq!(one_more.body["error"], at_most);
    let patched = call(
        &obid,
        "PATCH",
        &format!("/v1/conversations/{id}"),
        Some(&json!({"folders": all})),
    );
    assert_eq!(patched.status, 422, "{}", patched.text);
    assert_eq!(patched.body["error"], at_most);
    assert_eq!(folders_of(&obid, &id), json!(all[..20]));
}
