//! Projects at obid's API boundary (docs/contracts/projects.md): `GET`/`POST /v1/projects`,
//! `PATCH`/`DELETE /v1/projects/{id}`, a conversation's `project`, and the start's reconcile. Each
//! test is named after the contract's scenario it proves. Through the gateway and the relay:
//! gateway_api.rs.
mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

/// A conversation kept in a stopped obid's home: no turn, so no answer moves its revision.
const ONE: &str = "11111111-1111-4111-8111-111111111111";
const TWO: &str = "22222222-2222-4222-8222-222222222222";

fn harness_config(folder: &Folder) -> Value {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    config
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// Writes conversation `id` into the home of an obid not started yet, with `keys` over its own.
fn kept_conversation(folder: &Folder, id: &str, keys: Value) {
    let dir = folder.0.join("home/conversations");
    std::fs::create_dir_all(&dir).unwrap();
    let mut thread = json!({"id": id, "title": "Kept", "pinned": false,
        "created": "2026-11-15T10:00:00Z", "sequence": 1, "turns": [], "foreign": null});
    for (key, value) in keys.as_object().unwrap() {
        thread[key] = value.clone();
    }
    std::fs::write(dir.join(format!("{id}.json")), thread.to_string()).unwrap();
}

/// Writes `projects` as the home's `projects.json`, before obid starts.
fn kept_projects(folder: &Folder, projects: Value) {
    std::fs::create_dir_all(folder.0.join("home")).unwrap();
    let file = folder.0.join("home/projects.json");
    std::fs::write(file, json!({ "projects": projects }).to_string()).unwrap();
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

fn projects(obid: &Obid) -> Value {
    let answer = call(obid, "GET", "/v1/projects", None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

/// `POST /v1/projects` with `body`, answered `201`.
fn made(obid: &Obid, body: Value) -> Value {
    let answer = call(obid, "POST", "/v1/projects", Some(&body));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body
}

fn patch(obid: &Obid, path: &str, body: Value) -> Answer {
    call(obid, "PATCH", path, Some(&body))
}

#[test]
fn projects_1_a_project_is_made_listed_and_kept() {
    let folder = Folder::new("projects-1");
    let config = harness_config(&folder);
    let mut obid = start_at_home(&folder, &config);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["api"])).unwrap();
    let link = api.parent().unwrap().join("work");
    std::os::unix::fs::symlink(&api, &link).unwrap();

    let project = made(
        &obid,
        json!({"name": " Oby ", "folders": [text(&link)], "repos": ["obi-ai-bot/oby"]}),
    );
    let id = project["id"].as_str().unwrap().to_owned();
    assert!(
        id.len() == 36 && id.bytes().all(|b| b == b'-' || b.is_ascii_hexdigit()),
        "{project}"
    );
    assert_eq!(id, id.to_lowercase());
    assert_eq!(
        project,
        json!({"id": id, "name": "Oby", "folders": [text(&api)], "repos": ["obi-ai-bot/oby"]})
    );
    assert_eq!(projects(&obid), json!([project]));
    // An empty project: folders and repos left out are none.
    let bare = made(&obid, json!({"name": "Bare"}));
    assert_eq!((&bare["folders"], &bare["repos"]), (&json!([]), &json!([])));

    obid.stop();
    let obid = start_at_home(&folder, &config);
    assert_eq!(projects(&obid), json!([project, bare]));
    let mode = std::fs::metadata(folder.0.join("home/projects.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn projects_2_refused_input_keeps_nothing() {
    let folder = Folder::new("projects-2");
    let config = harness_config(&folder);
    let obid = start_at_home(&folder, &config);
    let names: Vec<String> = (0..21).map(|n| format!("f{n}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let all: Vec<String> = picked(&folder, &names).iter().map(|p| text(p)).collect();
    let ssh = home_of(&folder).join(".ssh");
    std::fs::create_dir_all(&ssh).unwrap();
    let kept = made(&obid, json!({"name": "Kept", "folders": [all[0]]}));

    let refused = [
        (json!({"name": "A", "folders": ["Projects/f0"]}), 422),
        (
            json!({"name": "A", "folders": [format!("{}/gone", all[0])]}),
            422,
        ),
        (json!({"name": "A", "folders": [text(&ssh)]}), 422),
        (
            json!({"name": "A", "folders": [text(&folder.0.join("home"))]}),
            422,
        ),
        (json!({"name": "A", "folders": all}), 422),
        (json!({"name": "A", "repos": ["a/b/c"]}), 422),
        (json!({"name": "A", "repos": ["../x"]}), 422),
        (json!({"name": "  "}), 422),
        (json!({"name": "x".repeat(101)}), 422),
        (json!({"name": "two\nlines"}), 422),
        (json!({"folders": [all[1]]}), 422),
        (json!({"name": "A", "colour": "red"}), 400),
    ];
    for (body, status) in refused {
        let answer = call(&obid, "POST", "/v1/projects", Some(&body));
        assert_eq!(answer.status, status, "{body}: {}", answer.text);
        assert!(answer.body["error"].as_str().is_some(), "{}", answer.text);
    }
    let path = format!("/v1/projects/{}", kept["id"].as_str().unwrap());
    let refused = patch(&obid, &path, json!({"folders": [all[1], "/usr"]}));
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(projects(&obid), json!([kept]));

    // The same folder twice, and a repository in another case, are kept once.
    let once = made(
        &obid,
        json!({"name": "Once", "folders": [all[2], format!("{}/", all[2])],
               "repos": ["obi-ai-bot/oby", "Obi-AI-Bot/Oby"]}),
    );
    assert_eq!(once["folders"], json!([all[2]]));
    assert_eq!(once["repos"], json!(["obi-ai-bot/oby"]));
    // 100 characters is a name.
    made(&obid, json!({"name": "x".repeat(100)}));
}

#[test]
fn projects_3_a_change_reaches_its_conversations() {
    let folder = Folder::new("projects-3");
    let config = harness_config(&folder);
    let [b, c, d] = <[PathBuf; 3]>::try_from(picked(&folder, &["b", "c", "d"])).unwrap();
    let project = json!({"id": "33333333-3333-4333-8333-333333333333", "name": "Oby",
        "folders": [text(&b)], "repos": ["obi-ai-bot/oby"]});
    kept_projects(&folder, json!([project]));
    kept_conversation(
        &folder,
        ONE,
        json!({"project": project["id"], "projectFolders": [text(&b)]}),
    );
    let obid = start_at_home(&folder, &config);
    let path = format!("/v1/projects/{}", project["id"].as_str().unwrap());
    let before = conversation(&obid, ONE)["revision"].as_u64().unwrap();

    let renamed = patch(&obid, &path, json!({"name": "Oby app"}));
    assert_eq!(renamed.status, 200, "{}", renamed.text);
    let mut expected = project.clone();
    expected["name"] = json!("Oby app");
    assert_eq!(renamed.body, expected);
    assert_eq!(
        conversation(&obid, ONE)["revision"].as_u64().unwrap(),
        before
    );

    let moved = patch(&obid, &path, json!({"folders": [text(&c), text(&d)]}));
    assert_eq!(moved.status, 200, "{}", moved.text);
    assert_eq!(moved.body["folders"], json!([text(&c), text(&d)]));
    assert_eq!(moved.body["repos"], json!(["obi-ai-bot/oby"]));
    let now = conversation(&obid, ONE);
    assert_eq!(now["folders"], json!([text(&c), text(&d)]), "{now}");
    assert_eq!(now["revision"].as_u64().unwrap(), before + 1);

    let empty = patch(&obid, &path, json!({}));
    assert_eq!(empty.status, 422, "{}", empty.text);
    let unknown = patch(&obid, "/v1/projects/nope", json!({"name": "A"}));
    assert_eq!(unknown.status, 404, "{}", unknown.text);
    let refused = patch(&obid, &path, json!({"folders": ["/usr"]}));
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(projects(&obid)[0]["folders"], json!([text(&c), text(&d)]));
    assert_eq!(
        conversation(&obid, ONE)["revision"].as_u64().unwrap(),
        before + 1
    );
}

/// A home with conversation [`ONE`] working in its own folder `a`, and a project in `b` and `c`.
fn in_a_home(name: &str) -> (Folder, Obid, [PathBuf; 4], String) {
    let folder = Folder::new(name);
    let config = harness_config(&folder);
    let folders = <[PathBuf; 4]>::try_from(picked(&folder, &["a", "b", "c", "d"])).unwrap();
    kept_conversation(&folder, ONE, json!({"folders": [text(&folders[0])]}));
    let obid = start_at_home(&folder, &config);
    let project = made(
        &obid,
        json!({"name": "Oby", "folders": [text(&folders[1]), text(&folders[2])]}),
    );
    let id = project["id"].as_str().unwrap().to_owned();
    (folder, obid, folders, id)
}

#[test]
fn projects_4_a_conversation_put_in_a_project_answers_its_folders() {
    let (folder, obid, [a, b, c, _], project) = in_a_home("projects-4");
    let path = format!("/v1/conversations/{ONE}");
    assert_eq!(conversation(&obid, ONE)["project"], Value::Null);
    assert_eq!(conversation(&obid, ONE)["folders"], json!([text(&a)]));

    let unknown = patch(&obid, &path, json!({"project": "nope"}));
    assert_eq!(unknown.status, 422, "{}", unknown.text);
    assert_eq!(conversation(&obid, ONE)["project"], Value::Null);

    let put = patch(&obid, &path, json!({"project": project}));
    assert_eq!(put.status, 200, "{}", put.text);
    assert_eq!(put.body["project"], project.as_str());
    assert_eq!(put.body["folders"], json!([text(&b), text(&c)]));
    let listed = conversation(&obid, ONE);
    assert_eq!(listed["project"], project.as_str());
    assert_eq!(listed["folders"], json!([text(&b), text(&c)]));
    // Its own list waits underneath, on disk.
    let file = std::fs::read(folder.0.join(format!("home/conversations/{ONE}.json"))).unwrap();
    let file: Value = serde_json::from_slice(&file).unwrap();
    assert_eq!(file["folders"], json!([text(&a)]));
}

#[test]
fn projects_5_taken_out_it_answers_its_own_folders_again() {
    let (_folder, obid, [a, ..], project) = in_a_home("projects-5");
    let path = format!("/v1/conversations/{ONE}");
    assert_eq!(patch(&obid, &path, json!({"project": project})).status, 200);

    let out = patch(&obid, &path, json!({"project": null}));
    assert_eq!(out.status, 200, "{}", out.text);
    assert_eq!(out.body["project"], Value::Null);
    assert_eq!(out.body["folders"], json!([text(&a)]));
    assert_eq!(conversation(&obid, ONE)["folders"], json!([text(&a)]));
}

#[test]
fn projects_6_no_own_folders_while_in_a_project() {
    let (_folder, obid, [_, b, c, d], project) = in_a_home("projects-6");
    let path = format!("/v1/conversations/{ONE}");
    assert_eq!(patch(&obid, &path, json!({"project": project})).status, 200);
    let before = conversation(&obid, ONE);

    let own = patch(&obid, &path, json!({"folders": [text(&d)]}));
    assert_eq!(own.status, 422, "{}", own.text);
    let both = patch(
        &obid,
        &path,
        json!({"project": project, "folders": [text(&d)]}),
    );
    assert_eq!(both.status, 422, "{}", both.text);
    let words = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "tidy up", "conversation": ONE, "folders": [text(&d)]})),
    );
    assert_eq!(words.status, 422, "{}", words.text);
    assert_eq!(conversation(&obid, ONE), before);
    let turns = call(&obid, "GET", &format!("/v1/conversation?id={ONE}"), None);
    assert_eq!(turns.body, json!([]), "{}", turns.text);
    assert_eq!(before["folders"], json!([text(&b), text(&c)]));

    let out = patch(
        &obid,
        &path,
        json!({"project": null, "folders": [text(&d)]}),
    );
    assert_eq!(out.status, 200, "{}", out.text);
    assert_eq!(out.body["project"], Value::Null);
    assert_eq!(out.body["folders"], json!([text(&d)]));
}

#[test]
fn projects_7_a_conversation_starts_in_a_project() {
    let (_folder, obid, [_, b, c, d], project) = in_a_home("projects-7");
    let plan = |body: Value| call(&obid, "POST", "/v1/plans", Some(&body));

    let without_new = plan(json!({"said": "tidy up", "project": project}));
    assert_eq!(without_new.status, 422, "{}", without_new.text);
    let named = plan(json!({"said": "tidy up", "conversation": ONE, "project": project}));
    assert_eq!(named.status, 422, "{}", named.text);
    let unknown = plan(json!({"said": "tidy up", "new": true, "project": "nope"}));
    assert_eq!(unknown.status, 422, "{}", unknown.text);
    let listed = call(&obid, "GET", "/v1/conversations", None);
    assert_eq!(listed.body.as_array().unwrap().len(), 1, "{}", listed.text);

    let words = "77777777-7777-4777-8777-777777777777";
    let body = json!({"said": "tidy up", "new": true, "id": words, "project": project,
                      "folders": [text(&d)]});
    let started = plan(body.clone());
    assert_eq!(started.status, 200, "{}", started.text);
    let id = started.body["conversation"].as_str().unwrap().to_owned();
    let now = conversation(&obid, &id);
    assert_eq!(now["project"], project.as_str());
    assert_eq!(now["folders"], json!([text(&b), text(&c)]));
    // The folders sent with it are its own, underneath.
    let out = patch(
        &obid,
        &format!("/v1/conversations/{id}"),
        json!({"project": null}),
    );
    assert_eq!(out.body["folders"], json!([text(&d)]), "{}", out.text);

    let again = plan(body);
    assert_eq!(again.status, 200, "{}", again.text);
    assert_eq!(again.body["repeated"], true, "{}", again.text);
    assert_eq!(again.body["conversation"], id.as_str());
}

#[test]
fn projects_8_deleting_a_project_keeps_its_conversations_folders() {
    let (_folder, obid, [_, b, c, _], project) = in_a_home("projects-8");
    let started = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "tidy up", "new": true, "project": project})),
    );
    assert_eq!(started.status, 200, "{}", started.text);
    let other = started.body["conversation"].as_str().unwrap().to_owned();
    let path = format!("/v1/conversations/{ONE}");
    assert_eq!(patch(&obid, &path, json!({"project": project})).status, 200);
    let before = conversation(&obid, ONE)["revision"].as_u64().unwrap();
    let events = follow_events(&obid);

    let gone = call(&obid, "DELETE", &format!("/v1/projects/{project}"), None);
    assert_eq!(gone.status, 204, "{}", gone.text);
    for id in [ONE, other.as_str()] {
        let now = conversation(&obid, id);
        assert_eq!(now["project"], Value::Null, "{now}");
        assert_eq!(now["folders"], json!([text(&b), text(&c)]), "{now}");
    }
    assert_eq!(
        conversation(&obid, ONE)["revision"].as_u64().unwrap(),
        before + 1
    );
    // The other conversation's answer may raise its revision too: its event says it left.
    events_named(&events, "conversation", 10, |got| {
        got.iter()
            .any(|c| c["id"] == other.as_str() && c["project"].is_null())
    });
    assert_eq!(projects(&obid), json!([]));
    let twice = call(&obid, "DELETE", &format!("/v1/projects/{project}"), None);
    assert_eq!(twice.status, 404, "{}", twice.text);
    // They're its own now: a null project changes nothing.
    let out = patch(&obid, &path, json!({"project": null}));
    assert_eq!(out.body["folders"], json!([text(&b), text(&c)]));
}

#[test]
fn projects_9_a_restart_partway_leaves_no_conversation_in_a_gone_project() {
    let folder = Folder::new("projects-9");
    let config = harness_config(&folder);
    let [a, b, c] = <[PathBuf; 3]>::try_from(picked(&folder, &["a", "b", "c"])).unwrap();
    let kept = json!({"id": "99999999-9999-4999-8999-999999999999", "name": "Kept",
        "folders": [text(&c)], "repos": []});
    let empty = json!({"id": "88888888-8888-4888-8888-888888888888", "name": "Cut short",
        "folders": [text(&b)], "repos": []});
    kept_projects(&folder, json!([kept, empty]));
    // (a) its project was deleted, and obid stopped before the conversation was written.
    kept_conversation(
        &folder,
        ONE,
        json!({"folders": [text(&a)], "project": "00000000-0000-4000-8000-000000000000",
               "projectFolders": [text(&b)]}),
    );
    // (b) its project's folders changed, and obid stopped before the conversation was written.
    kept_conversation(
        &folder,
        TWO,
        json!({"project": kept["id"], "projectFolders": [text(&b)]}),
    );
    let obid = start_at_home(&folder, &config);

    let one = conversation(&obid, ONE);
    assert_eq!(one["project"], Value::Null, "{one}");
    assert_eq!(one["folders"], json!([text(&b)]), "{one}");
    let two = conversation(&obid, TWO);
    assert_eq!(two["project"], kept["id"], "{two}");
    assert_eq!(two["folders"], json!([text(&c)]), "{two}");
    // (c) a delete cut short left the project with no conversation: listed, and deleted again.
    assert_eq!(projects(&obid), json!([kept, empty]));
    let path = format!("/v1/projects/{}", empty["id"].as_str().unwrap());
    assert_eq!(call(&obid, "DELETE", &path, None).status, 204);
    assert_eq!(projects(&obid), json!([kept]));
}

#[test]
fn projects_11_status_lists_projects_v1() {
    let folder = Folder::new("projects-11");
    let obid = start_at_home(&folder, &harness_config(&folder));
    let status = call(&obid, "GET", "/v1/status", None).body;
    let names: Vec<&str> = status["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert!(names.contains(&"projects-v1"), "{status}");
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn projects_12_an_unreadable_file_changes_nothing() {
    let folder = Folder::new("projects-12");
    let config = harness_config(&folder);
    std::fs::create_dir_all(folder.0.join("home")).unwrap();
    std::fs::write(folder.0.join("home/projects.json"), "not json").unwrap();
    kept_conversation(
        &folder,
        ONE,
        json!({"project": "33333333-3333-4333-8333-333333333333", "projectFolders": ["/b"]}),
    );
    let file = folder.0.join(format!("home/conversations/{ONE}.json"));
    let before = std::fs::read(&file).unwrap();
    let obid = start_at_home(&folder, &config);

    assert_eq!(conversation(&obid, ONE)["folders"], json!(["/b"]));
    assert_eq!(std::fs::read(&file).unwrap(), before);
    let routes = [
        ("GET", "/v1/projects".to_owned(), None),
        (
            "POST",
            "/v1/projects".to_owned(),
            Some(json!({"name": "A"})),
        ),
        (
            "PATCH",
            format!("/v1/projects/{ONE}"),
            Some(json!({"name": "A"})),
        ),
        ("DELETE", format!("/v1/projects/{ONE}"), None),
    ];
    for (method, path, body) in routes {
        let answer = call(&obid, method, &path, body.as_ref());
        assert_eq!(answer.status, 500, "{method} {path}: {}", answer.text);
        let why = answer.body["error"].as_str().unwrap_or_default();
        assert!(why.contains("projects.json"), "{why}");
    }
    let read = std::fs::read(folder.0.join("home/projects.json")).unwrap();
    assert_eq!(read, b"not json");
}
