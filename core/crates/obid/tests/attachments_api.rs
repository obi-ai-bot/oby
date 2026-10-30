//! Files and photos reach Oby (docs/contracts/attachments.md, ATTACH-1 to ATTACH-9 and ATTACH-11
//! to ATTACH-14): the Mac app uploads what the owner picked or pasted (`POST /v1/attachments`), then sends the words with the
//! paths obid kept, the tasks attached and an id of their own (`POST /v1/plans`). The agent here is
//! the conversation harness's stand-in (tests/agents/fake-harness).
mod support;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use support::*;

fn obid(name: &str) -> (Folder, Obid) {
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let mut config = config(&folder.0, &remote, &harness);
    config["conversation"] = json!({"agent": "claude"});
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    let obid = start_with(&folder.0, &config, None, &[]);
    (folder, obid)
}

/// `POST /v1/attachments?name=<name>` with `data` as the body, as main sends a file's bytes.
fn upload(obid: &Obid, name: &str, data: &[u8]) -> Answer {
    let mut stream = TcpStream::connect(("127.0.0.1", obid.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let name: String = name
        .bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'_' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    let head = format!(
        "POST /v1/attachments?name={name} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {TOKEN}\r\nConnection: close\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
        data.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(data).unwrap();
    let mut raw = vec![];
    stream.read_to_end(&mut raw).unwrap();
    let raw = String::from_utf8_lossy(&raw);
    let (head, rest) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    let body = serde_json::from_str(rest.trim()).unwrap_or(Value::Null);
    Answer {
        status,
        body,
        text: rest.to_owned(),
    }
}

fn kept(obid: &Obid, name: &str, data: &[u8]) -> String {
    let answer = upload(obid, name, data);
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["path"].as_str().unwrap().to_owned()
}

fn plan(obid: &Obid, body: Value) -> Answer {
    call(obid, "POST", "/v1/plans", Some(&body))
}

fn thread(obid: &Obid, conversation: &str) -> Vec<Value> {
    call(
        obid,
        "GET",
        &format!("/v1/conversation?id={conversation}"),
        None,
    )
    .body
    .as_array()
    .unwrap()
    .clone()
}

#[test]
fn attach_1_a_picked_file_is_kept_in_obids_attachments_folder() {
    use std::os::unix::fs::PermissionsExt;
    let (_folder, obid) = obid("attach-1");
    let answer = upload(&obid, "pic one.png", b"\x89PNG not really");
    assert_eq!(answer.status, 201, "{}", answer.text);
    assert_eq!(answer.body["name"], "pic_one.png");
    let path = Path::new(answer.body["path"].as_str().unwrap()).to_owned();
    let app = obid.home.join("attachments/app").canonicalize().unwrap();
    assert!(path.canonicalize().unwrap().starts_with(&app), "{path:?}");
    assert_eq!(std::fs::read(&path).unwrap(), b"\x89PNG not really");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn attach_2_files_past_the_limits_are_refused() {
    let (_folder, obid) = obid("attach-2");
    let empty = upload(&obid, "empty.txt", b"");
    assert_eq!(empty.status, 422, "{}", empty.text);
    assert_eq!(empty.body["error"], "empty.txt is empty.");
    let big = upload(&obid, "big.bin", &vec![0u8; 25_000_001]);
    assert_eq!(big.status, 413, "{}", big.text);
    assert_eq!(
        big.body["error"],
        "big.bin is larger than 25 MB, so Oby can't take it."
    );
    let path = kept(&obid, "a.txt", b"a");
    let many = plan(
        &obid,
        json!({"said": "SAY many", "new": true, "attachments": vec![path; 21]}),
    );
    assert_eq!(many.status, 422, "{}", many.text);
    assert_eq!(many.body["error"], "A message attaches at most 20 files.");
}

#[test]
fn attach_3_a_path_outside_what_the_owner_picked_is_refused() {
    let (folder, obid) = obid("attach-3");
    let outside = plan(
        &obid,
        json!({"said": "SAY look", "new": true, "attachments": ["/etc/hosts"]}),
    );
    assert_eq!(outside.status, 422, "{}", outside.text);
    // A link in the attachments folder that leads out of it is outside too.
    let secret = folder.0.join("secret.txt");
    std::fs::write(&secret, "not the owner's pick").unwrap();
    let app = obid.home.join("attachments/app");
    std::fs::create_dir_all(&app).unwrap();
    std::os::unix::fs::symlink(&secret, app.join("link.txt")).unwrap();
    let linked = plan(
        &obid,
        json!({"said": "SAY look", "new": true, "attachments": [app.join("link.txt")]}),
    );
    assert_eq!(linked.status, 422, "{}", linked.text);
    assert_eq!(
        call(&obid, "GET", "/v1/conversations", None).body,
        json!([])
    );
}

#[test]
fn attach_4_uploaded_files_reach_the_conversation_and_tag_the_words() {
    let (_folder, obid) = obid("attach-4");
    let path = kept(&obid, "notes.txt", format!("token {TOKEN}\n").as_bytes());
    let answer = plan(
        &obid,
        json!({"said": "FILES", "new": true, "attachments": [path]}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let conversation = answer.body["conversation"].as_str().unwrap();
    let turns = thread(&obid, conversation);
    assert_eq!(turns[0]["attachments"], json!(["notes.txt"]));
    let turn = turns[0]["id"].as_str().unwrap();
    assert_eq!(
        answer.body["text"],
        format!("files: attachments/{turn}/notes.txt named=true")
    );
    // In the room with every secret obid knows taken out; Oby's answer has no tags.
    let copy = obid.home.join(format!(
        "conversations/{conversation}/room/attachments/{turn}/notes.txt"
    ));
    let text = std::fs::read_to_string(copy).unwrap();
    assert!(!text.contains(TOKEN), "{text}");
    assert!(turns[1].get("attachments").is_none(), "{}", turns[1]);
}

#[test]
fn attach_5_files_or_tasks_alone_are_a_message() {
    let (_folder, obid) = obid("attach-5");
    let nothing = plan(&obid, json!({"said": " ", "new": true}));
    assert_eq!(nothing.status, 422, "{}", nothing.text);
    let path = kept(&obid, "report.pdf", b"%PDF-1.7");
    let answer = plan(
        &obid,
        json!({"said": "", "new": true, "attachments": [path]}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let conversation = answer.body["conversation"].as_str().unwrap();
    let listed = call(&obid, "GET", "/v1/conversations", None).body;
    assert_eq!(listed[0]["title"], "Report.pdf");
    let turns = thread(&obid, conversation);
    assert_eq!(
        (turns[0]["text"].clone(), turns[0]["attachments"].clone()),
        (json!(""), json!(["report.pdf"]))
    );
}

#[test]
fn attach_6_a_task_started_from_words_with_files_gets_them() {
    let (_folder, obid) = obid("attach-6");
    let path = kept(&obid, "shot.png", b"\x89PNG pixels");
    let dispatch = r#"OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#;
    let answer = plan(
        &obid,
        json!({"said": dispatch, "new": true, "attachments": [path]}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let id = answer.body["jobs"][0]["id"].as_str().unwrap().to_owned();
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    // An attachment is untrusted: the task it reaches is marked so.
    assert_eq!(job["untrusted"], true, "{job}");
    let input = obid
        .home
        .join(format!("worktrees/{id}/.obi/inputs/shot.png"));
    within(120, "the task has the file", || {
        input.is_file().then_some(())
    });
}

#[test]
fn attach_7_attached_tasks_go_on_the_owners_words() {
    let (_folder, obid) = obid("attach-7");
    let dispatch = r#"OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#;
    let started = plan(&obid, json!({"said": dispatch, "new": true}));
    let job = started.body["jobs"][0]["id"].as_str().unwrap().to_owned();
    let answer = plan(&obid, json!({"said": "", "new": true, "jobs": [job]}));
    assert_eq!(answer.status, 200, "{}", answer.text);
    let turns = thread(&obid, answer.body["conversation"].as_str().unwrap());
    assert_eq!(turns[0]["jobs"], json!([job]));
}

#[test]
fn attach_8_words_sent_again_with_their_id_add_nothing_twice() {
    let (_folder, obid) = obid("attach-8");
    let path = kept(&obid, "notes.txt", b"once");
    let id = "5b0e6f4c-8a51-4d0e-9c3e-2f7a1b6d9e10";
    let body = json!({"said": "FILES", "new": true, "attachments": [path], "id": id});
    let first = plan(&obid, body.clone());
    assert_eq!(first.status, 200, "{}", first.text);
    let conversation = first.body["conversation"].as_str().unwrap().to_owned();
    let again = plan(&obid, body);
    assert_eq!(again.status, 200, "{}", again.text);
    assert_eq!(again.body["repeated"], true, "{}", again.text);
    assert_eq!(again.body["conversation"], json!(conversation));
    let turns = thread(&obid, &conversation);
    assert_eq!(turns.len(), 2, "{turns:?}");
    assert_eq!(turns[0]["id"], id);
    let room = obid
        .home
        .join(format!("conversations/{conversation}/room/attachments"));
    assert_eq!(std::fs::read_dir(room).unwrap().count(), 1);
    assert_eq!(
        call(&obid, "GET", "/v1/conversations", None)
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn attach_9_files_are_uploaded_only_on_the_mac() {
    let post = axum::http::Method::POST;
    assert!(!obid::gateway::allows(&post, "/v1/attachments"));
    assert!(obid::gateway::allows(&post, "/v1/plans"));
}

fn dispatch(obid: &Obid, task: &str, attachments: &[&str]) -> (String, String) {
    let words = format!(
        r#"OBY oby_dispatch {{"repo": "obi-e2e/fixture-app", "task": "{task}", "kind": "investigate"}}"#
    );
    let answer = plan(
        obid,
        json!({"said": words, "new": true, "attachments": attachments}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    (
        answer.body["conversation"].as_str().unwrap().to_owned(),
        answer.body["jobs"][0]["id"].as_str().unwrap().to_owned(),
    )
}

fn status(obid: &Obid, job: &str) -> Value {
    call(obid, "GET", &format!("/v1/jobs/{job}"), None).body["status"].clone()
}

#[test]
fn attach_11_a_deleted_conversation_takes_its_files_with_it() {
    let (_folder, obid) = obid("attach-11");
    let path = kept(&obid, "notes.txt", b"the owner's own");
    let answer = plan(
        &obid,
        json!({"said": "SAY ok", "new": true, "attachments": [path]}),
    );
    let conversation = answer.body["conversation"].as_str().unwrap();
    let folder = Path::new(&path).parent().unwrap().to_owned();
    assert!(folder.is_dir());
    let gone = call(
        &obid,
        "DELETE",
        &format!("/v1/conversations/{conversation}"),
        None,
    );
    assert_eq!(gone.status, 204, "{}", gone.text);
    assert!(!folder.exists(), "{folder:?}");

    // One a task that hasn't ended still holds stays, for that task.
    let held = kept(&obid, "shot.png", b"\x89PNG");
    let (conversation, job) = dispatch(&obid, "STALLS-ALWAYS look", &[&held]);
    within(60, "the task is at work", || {
        (status(&obid, &job) == "running").then_some(())
    });
    call(
        &obid,
        "DELETE",
        &format!("/v1/conversations/{conversation}"),
        None,
    );
    assert!(Path::new(&held).is_file());
}

#[test]
fn attach_12_an_upload_whose_words_never_went_can_go() {
    let (_folder, obid) = obid("attach-12");
    let unload = |path: &str| {
        let query: String = path
            .bytes()
            .map(|b| match b {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'_' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect();
        call(
            &obid,
            "DELETE",
            &format!("/v1/attachments?path={query}"),
            None,
        )
    };
    let unused = kept(&obid, "a.txt", b"a");
    assert_eq!(unload(&unused).status, 204);
    assert!(!Path::new(&unused).parent().unwrap().exists());
    assert_eq!(
        unload(&unused).status,
        422,
        "gone, so not an upload any more"
    );
    assert_eq!(unload("/etc/hosts").status, 422);
    let used = kept(&obid, "b.txt", b"b");
    plan(
        &obid,
        json!({"said": "SAY ok", "new": true, "attachments": [used]}),
    );
    assert_eq!(unload(&used).status, 409);
    assert!(Path::new(&used).is_file());
}

#[test]
fn attach_13_files_alone_follow_up_a_finished_task_but_not_a_running_one() {
    let (_folder, obid) = obid("attach-13");
    let (_, done) = dispatch(&obid, "Look around", &[]);
    within(120, "the task is done", || {
        (status(&obid, &done) == "done").then_some(())
    });
    let path = kept(&obid, "shot.png", b"\x89PNG");
    let followed = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{done}/messages"),
        Some(&json!({"text": "", "attachments": [path]})),
    );
    assert_eq!(followed.status, 201, "{}", followed.text);
    assert_eq!(followed.body["request"], obid::jobs::FILES_ALONE);
    let next = followed.body["id"].as_str().unwrap();
    let input = obid
        .home
        .join(format!("worktrees/{next}/.obi/inputs/shot.png"));
    within(120, "the follow-up has the file", || {
        input.is_file().then_some(())
    });

    let (_, running) = dispatch(&obid, "STALLS-ALWAYS look", &[]);
    within(60, "the task is at work", || {
        (status(&obid, &running) == "running").then_some(())
    });
    let other = kept(&obid, "more.png", b"\x89PNG");
    let refused = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{running}/messages"),
        Some(&json!({"text": "", "attachments": [other]})),
    );
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(
        refused.body["error"],
        "Files go with a follow-up of a finished task or a new task: tell this one in words."
    );
}

#[test]
fn attach_14_a_file_gone_before_its_turn_is_named_as_left_out() {
    let (_folder, obid) = obid("attach-14");
    let path = kept(&obid, "notes.txt", b"soon gone");
    // A turn under way; words sent meanwhile wait for it, and their file goes while they wait.
    let started = plan(&obid, json!({"said": "SAY first", "new": true}));
    let conversation = started.body["conversation"].as_str().unwrap().to_owned();
    std::thread::scope(|scope| {
        let busy = scope.spawn(|| {
            plan(
                &obid,
                json!({"said": "LINGER 2000", "conversation": conversation}),
            )
        });
        std::thread::sleep(Duration::from_millis(500));
        let waiting = scope.spawn(|| {
            plan(
                &obid,
                json!({"said": "FILES", "conversation": conversation, "attachments": [path]}),
            )
        });
        std::thread::sleep(Duration::from_millis(500));
        std::fs::remove_dir_all(Path::new(&path).parent().unwrap()).unwrap();
        busy.join().unwrap();
        waiting.join().unwrap();
    });
    let recorded = std::fs::read_to_string(
        obid.home
            .join(format!("conversations/{conversation}/config/turns.jsonl")),
    )
    .unwrap();
    let last: Value = serde_json::from_str(recorded.lines().last().unwrap()).unwrap();
    let prompt = last["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("(left out: notes.txt, it is no longer on this Mac)"),
        "{prompt}"
    );
}
