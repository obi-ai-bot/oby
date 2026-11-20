//! A task's changes, round by round, and Undo this round (docs/contracts/task-changes.md,
//! CHANGES-1 to 13), on the real obid binary asked over HTTP: a task of two rounds (a job, then the
//! owner's words to it, its follow-up on the same branch) lists both, answers each round's diff
//! apart and the whole task's with whitespace left out; Undo this round takes the last one back
//! only on a registered device's fresh signature of obid's own sentence, force-pushing the task's
//! own `oby/` branch back when the round went out, and never a branch Oby didn't make.
//!
//! The remote is the e2e fixture's bare repository (a `file://` remote); GitHub is a stand-in `gh`
//! first on obid's `PATH` that shows the task's pull request open, so the follow-up carries its
//! branch on.

mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

fn agent() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/publishing-claude")
}

/// A stand-in `gh` in `folder/bin` that opens one pull request and shows it open, as
/// tests/publish_api.rs's does for PUBLISHING-12.
fn fake_gh(folder: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = folder.join("bin");
    let state = folder.join("gh");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        state.join("view.json"),
        r#"{"state":"OPEN","reviewDecision":"REVIEW_REQUIRED","mergedAt":null,"baseRefName":"main","headRefOid":""}"#,
    )
    .unwrap();
    let script = format!(
        r#"#!/bin/sh
dir='{state}'
case "$1 $2" in
  "pr list") [ -f "$dir/open" ] && cat "$dir/open"; exit 0 ;;
  "pr create")
    echo '[{{"url":"https://github.com/obi-e2e/fixture-app/pull/7","isCrossRepository":false,"headRefOid":"","baseRefName":"main"}}]' > "$dir/open"
    echo 'https://github.com/obi-e2e/fixture-app/pull/7'; exit 0 ;;
  "pr edit") exit 0 ;;
  "pr view") cat "$dir/view.json"; exit 0 ;;
  *) echo "gh: not in this test" >&2; exit 1 ;;
esac
"#,
        state = state.display(),
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// The commit `branch` is at on the remote, or none.
fn remote_branch(remote: &Path, branch: &str) -> Option<String> {
    let listed = git(
        remote,
        &[
            "for-each-ref",
            "--format=%(objectname)",
            &format!("refs/heads/{branch}"),
        ],
    );
    (!listed.is_empty()).then_some(listed)
}

fn dispatch(obid: &Obid, job: Value) -> String {
    let answer = call_typed(obid, "POST", "/v1/jobs", &json!({"job": job}));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

fn ended(obid: &Obid, id: &str) -> Value {
    let answer = call(obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

fn changes(obid: &Obid, id: &str, query: &str) -> Value {
    let answer = call(obid, "GET", &format!("/v1/jobs/{id}/changes{query}"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

fn paths(answer: &Value) -> Vec<String> {
    let mut paths: Vec<String> = answer["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap().to_owned())
        .collect();
    paths.sort();
    paths
}

/// A task of two rounds: a build job that committed hello.txt and published it, then the owner's
/// words to it, its follow-up on the same branch, that committed second.txt.
struct Task {
    folder: Folder,
    obid: Obid,
    remote: PathBuf,
    first: String,
    next: String,
    branch: String,
    first_head: String,
    next_head: String,
}

fn two_rounds(name: &str) -> Task {
    let folder = Folder::new(name);
    let bin = fake_gh(&folder.0);
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &agent());
    let obid = start(&folder.0, &config, Some(&bin));
    let first = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "test -f hello.txt", "title": "Hello file"}),
    );
    let done = ended(&obid, &first);
    assert_eq!(done["status"], "done", "{done}\n{}", obid.log());
    let branch = format!("oby/build-{first}");
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let first_head = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{first}/messages"),
        Some(&json!({"text": "COMMIT: second.txt"})),
    );
    assert_eq!(told.status, 201, "{}", told.text);
    assert_eq!(told.body["branch"], branch.as_str());
    let next = told.body["id"].as_str().unwrap().to_owned();
    let follow = ended(&obid, &next);
    assert_eq!(follow["status"], "done", "{follow}\n{}", obid.log());
    let next_head = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(
        remote_branch(&remote, &branch).as_deref(),
        Some(next_head.as_str())
    );
    Task {
        folder,
        obid,
        remote,
        first,
        next,
        branch,
        first_head,
        next_head,
    }
}

/// CHANGES-1, CHANGES-2: the follow-up lists the task's two rounds, its first job's then its own,
/// each from the commit it started at to the one it ended at; the whole task's diff has both
/// files, each round's only its own, and a round the task doesn't have none. The first job lists
/// only its own.
#[test]
fn changes_1_2_a_two_round_task_lists_its_rounds_and_each_rounds_diff() {
    let task = two_rounds("changes-1");
    let whole = changes(&task.obid, &task.next, "");
    assert_eq!(paths(&whole), ["hello.txt", "second.txt"], "{whole}");
    assert_eq!(whole["round"], Value::Null);
    let rounds = whole["rounds"].as_array().unwrap();
    assert_eq!(rounds.len(), 2, "{whole}");
    assert_eq!(rounds[0]["round"], 1);
    assert_eq!(rounds[0]["job"], task.first.as_str());
    assert_eq!(rounds[0]["to"], task.first_head.as_str());
    assert_eq!(rounds[1]["round"], 2);
    assert_eq!(rounds[1]["job"], task.next.as_str());
    assert_eq!(rounds[1]["from"], task.first_head.as_str());
    assert_eq!(rounds[1]["to"], task.next_head.as_str());
    assert!(rounds[0]["at"].as_str().unwrap().ends_with('Z'), "{whole}");

    let one = changes(&task.obid, &task.next, "?round=1");
    assert_eq!(paths(&one), ["hello.txt"], "{one}");
    assert_eq!(one["round"], 1);
    assert!(!one["diff"].as_str().unwrap().contains("second.txt"));
    let two = changes(&task.obid, &task.next, "?round=2");
    assert_eq!(paths(&two), ["second.txt"], "{two}");
    assert!(!two["diff"].as_str().unwrap().contains("hello.txt"));
    let none = changes(&task.obid, &task.next, "?round=3");
    assert_eq!(none["files"], json!([]));
    assert_eq!(none["diff"], "");
    assert_eq!(none["head"], task.next_head.as_str());

    let first = changes(&task.obid, &task.first, "");
    assert_eq!(paths(&first), ["hello.txt"]);
    assert_eq!(first["rounds"].as_array().unwrap().len(), 1, "{first}");
}

/// CHANGES-3: `whitespace=ignore` leaves out a change in whitespace alone: a re-indented file isn't
/// listed and has no hunk, the task's other files are, a file whose mode alone changed and an
/// empty new file among them, on any git (2.43 lists the re-indented one under `-w`, 2.54 and later
/// don't); anything else is `422`.
#[test]
fn changes_3_ignoring_whitespace_leaves_out_a_reindented_file() {
    let task = two_rounds("changes-3");
    let worktree = task.obid.home.join("worktrees").join(&task.next);
    let greet = std::fs::read_to_string(worktree.join("src/greet.js")).unwrap();
    let indented: String = greet.lines().map(|l| format!("    {l}\n")).collect();
    std::fs::write(worktree.join("src/greet.js"), indented).unwrap();
    // Its mode alone changes (counted 0 0, as a re-indented file is on an older git), and a new
    // file with nothing in it (0 0 too): both are changes whitespace doesn't make.
    {
        use std::os::unix::fs::PermissionsExt;
        let readme = worktree.join("README.md");
        std::fs::set_permissions(&readme, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(worktree.join("empty.txt"), "").unwrap();
    git(
        &worktree,
        &["add", "src/greet.js", "README.md", "empty.txt"],
    );
    git(
        &worktree,
        &[
            "-c",
            "user.name=Author",
            "-c",
            "user.email=author@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "Indent",
        ],
    );
    let count = |answer: &Value| {
        let file = answer["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == "src/greet.js")
            .cloned()
            .unwrap_or_else(|| panic!("{answer}"));
        (
            file["additions"].as_u64().unwrap(),
            file["deletions"].as_u64().unwrap(),
        )
    };
    let hunks = |answer: &Value| answer["diff"].as_str().unwrap().matches("\n@@ ").count();
    let head = git(&worktree, &["rev-parse", "HEAD"]);
    let all = within(60, "obid reads the re-indented commit", || {
        let all = changes(&task.obid, &task.next, "");
        (all["head"] == head.as_str()
            && all["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["path"] == "src/greet.js"))
        .then_some(all)
    });
    assert_ne!(count(&all), (0, 0), "{all}");
    let ignored = changes(&task.obid, &task.next, "?whitespace=ignore");
    assert_eq!(
        paths(&ignored),
        ["README.md", "empty.txt", "hello.txt", "second.txt"],
        "{ignored}"
    );
    assert_eq!(hunks(&ignored), hunks(&all) - 1, "{ignored}");
    assert!(
        !ignored["diff"].as_str().unwrap().contains("greet.js"),
        "{ignored}"
    );
    let wrong = call(
        &task.obid,
        "GET",
        &format!("/v1/jobs/{}/changes?whitespace=all", task.next),
        None,
    );
    assert_eq!(wrong.status, 422, "{}", wrong.text);
}

/// A device's key from a fixed private scalar (no key in the source), and its public key as the
/// apps register it (X9.63, base64).
fn device(scalar: u8) -> (p256::ecdsa::SigningKey, String) {
    use base64::Engine as _;
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    let secret = p256::SecretKey::from_slice(&[scalar; 32]).unwrap();
    let public = base64::engine::general_purpose::STANDARD
        .encode(secret.public_key().to_encoded_point(false).as_bytes());
    (p256::ecdsa::SigningKey::from(secret), public)
}

/// What a device sends to undo `job`'s last round by `action`, its message opening with `kind`.
fn approval(
    key: &p256::ecdsa::SigningKey,
    device: &str,
    kind: &str,
    job: &str,
    action: &str,
    at: i64,
) -> Value {
    use base64::Engine as _;
    use p256::ecdsa::signature::Signer as _;
    let signed_at = obi_protocol::time::format(at);
    let signature: p256::ecdsa::Signature =
        key.sign(format!("{kind}\n{job}\n{action}\n{signed_at}").as_bytes());
    json!({"device": device, "action": action, "signedAt": signed_at,
           "signature": base64::engine::general_purpose::STANDARD.encode(signature.to_der())})
}

/// A registered device of this obid: its key and id.
fn registered(obid: &Obid) -> (p256::ecdsa::SigningKey, String) {
    let (key, public) = device(7);
    std::fs::write(obid.home.join("pairing"), &public).unwrap();
    let answer = call(
        obid,
        "POST",
        "/v1/devices",
        Some(&json!({"name": "Test Mac", "publicKey": public})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    (key, answer.body["id"].as_str().unwrap().to_owned())
}

/// CHANGES-8, CHANGES-10, CHANGES-11: obid's sentence for the follow-up's round names its branch,
/// the commit it goes back to and the force-push; no signature but a registered device's fresh one
/// of exactly that sentence, as `oby-undo`, undoes anything; that one puts the remote branch, the
/// clone's branch and the worktree back at the first round's end, the task lists one round, the
/// job counts no commit and lists the push, and there's nothing more to undo on it.
#[test]
fn changes_8_10_11_undo_this_round_force_pushes_the_tasks_own_branch_back() {
    let task = two_rounds("changes-8");
    let (obid, id) = (&task.obid, task.next.as_str());
    let short = &task.first_head[..7];
    let sentence = format!(
        "{branch} goes back to {short}, as it was before this round: its 1 commit leaves the branch. Oby force-pushes {branch} to {short}, so its pull request loses it too.",
        branch = task.branch
    );
    let asked = call(obid, "GET", &format!("/v1/jobs/{id}/undo"), None);
    assert_eq!(asked.status, 200, "{}", asked.text);
    assert_eq!(
        asked.body,
        json!({"action": sentence, "from": task.first_head, "to": task.next_head,
               "branch": task.branch, "push": true})
    );

    let (key, device_id) = registered(obid);
    let (stranger, _) = device(8);
    let now = obi_protocol::time::now();
    let undo = |body: &Value| call(obid, "POST", &format!("/v1/jobs/{id}/undo"), Some(body));
    for (body, why) in [
        (
            approval(&key, "nobody", "oby-undo", id, &sentence, now),
            "isn't registered",
        ),
        (
            approval(&stranger, &device_id, "oby-undo", id, &sentence, now),
            "signature",
        ),
        (
            approval(&key, &device_id, "oby-publish", id, &sentence, now),
            "signature",
        ),
        (
            approval(&key, &device_id, "oby-undo", id, "Undo it all.", now),
            "something else",
        ),
        (
            approval(&key, &device_id, "oby-undo", id, &sentence, now - 600),
            "two minutes",
        ),
    ] {
        let refused = undo(&body);
        assert_eq!(refused.status, 403, "{why}: {}", refused.text);
        assert!(
            refused.body["error"].as_str().unwrap().contains(why),
            "{}",
            refused.text
        );
    }
    assert_eq!(undo(&json!({"device": device_id})).status, 422);
    assert_eq!(
        remote_branch(&task.remote, &task.branch).as_deref(),
        Some(task.next_head.as_str()),
        "nothing moved on a refusal"
    );

    let done = undo(&approval(
        &key,
        &device_id,
        "oby-undo",
        id,
        &sentence,
        now + 1,
    ));
    assert_eq!(done.status, 200, "{}\n{}", done.text, obid.log());
    assert_eq!(done.body["id"], id);
    assert_eq!(done.body["commits"], 0, "{}", done.text);
    assert_eq!(
        remote_branch(&task.remote, &task.branch).as_deref(),
        Some(task.first_head.as_str())
    );
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    assert_eq!(
        git(
            &clone,
            &["rev-parse", &format!("refs/heads/{}", task.branch)]
        ),
        task.first_head
    );
    let worktree = obid.home.join("worktrees").join(id);
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), task.first_head);
    assert!(!worktree.join("second.txt").exists());
    let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    let last = job["outgoing"].as_array().unwrap().last().cloned().unwrap();
    assert_eq!(
        (
            last["action"].clone(),
            last["id"].clone(),
            last["branch"].clone()
        ),
        (json!("push"), json!(task.first_head), json!(task.branch)),
        "{job}"
    );
    let after = changes(obid, id, "");
    assert_eq!(paths(&after), ["hello.txt"], "{after}");
    assert_eq!(after["rounds"].as_array().unwrap().len(), 1, "{after}");
    let again = call(obid, "GET", &format!("/v1/jobs/{id}/undo"), None);
    assert_eq!(again.status, 409, "{}", again.text);
    assert_eq!(
        again.body["error"],
        "Nothing this task did is left to undo."
    );
    let used = undo(&approval(
        &key,
        &device_id,
        "oby-undo",
        id,
        &sentence,
        now + 1,
    ));
    assert_eq!(used.status, 409, "{}", used.text);
    drop(task.folder);
}

/// CHANGES-9: a job no round of which moved its branch has nothing to undo (`409`), and no such
/// job is `404`.
#[test]
fn changes_9_a_job_with_no_round_has_nothing_to_undo() {
    let folder = Folder::new("changes-9");
    let bin = fake_gh(&folder.0);
    let remote = fixture(&folder.0.join("fixture"));
    let obid = start(&folder.0, &config(&folder.0, &remote, &agent()), Some(&bin));
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "investigate", "task": "Look around"}),
    );
    ended(&obid, &id);
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/undo"), None);
    assert_eq!(answer.status, 409, "{}", answer.text);
    assert_eq!(
        answer.body["error"],
        "Nothing this task did is left to undo."
    );
    assert_eq!(changes(&obid, &id, "")["rounds"], json!([]));
    let missing = call(&obid, "GET", "/v1/jobs/not-a-job/undo", None);
    assert_eq!(missing.status, 404, "{}", missing.text);
}

/// CHANGES-13: a round that went out to a branch the owner named, not one of Oby's `oby/` ones,
/// isn't force-pushed back: obid says why, and nothing moves.
#[test]
fn changes_13_a_round_pushed_to_a_branch_oby_didnt_make_isnt_force_pushed() {
    let folder = Folder::new("changes-13");
    let bin = fake_gh(&folder.0);
    let remote = fixture(&folder.0.join("fixture"));
    let obid = start(&folder.0, &config(&folder.0, &remote, &agent()), Some(&bin));
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "branch": "feature/hello"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    let pushed = remote_branch(&remote, "feature/hello");
    assert!(pushed.is_some(), "{job}");
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/undo"), None);
    assert_eq!(answer.status, 409, "{}", answer.text);
    assert!(
        answer.body["error"]
            .as_str()
            .unwrap()
            .contains("Oby force-pushes only its own oby/ branches"),
        "{}",
        answer.text
    );
    assert_eq!(remote_branch(&remote, "feature/hello"), pushed);
}

/// TASKS-23 (#2529): reads of a task's changes at the same time each answer its whole diff, never
/// none taken for one cut: one git call's end in the job's sandbox isn't read as another's.
#[test]
fn tasks_23_reads_of_a_tasks_changes_at_once_each_answer_its_diff() {
    let folder = Folder::new("tasks-23-at-once");
    let bin = fake_gh(&folder.0);
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &agent());
    let obid = start(&folder.0, &config, Some(&bin));
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "test -f hello.txt", "title": "Hello file"}),
    );
    let done = ended(&obid, &id);
    assert_eq!(done["status"], "done", "{done}\n{}", obid.log());
    let answers: Vec<Value> = std::thread::scope(|scope| {
        let reads: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| (0..4).map(|_| changes(&obid, &id, "")).collect::<Vec<_>>()))
            .collect();
        reads.into_iter().flat_map(|r| r.join().unwrap()).collect()
    });
    for answer in &answers {
        assert_eq!(paths(answer), ["hello.txt"], "{answer}");
        assert_eq!(answer["truncated"], false, "{answer}\n{}", obid.log());
        assert_eq!(answer.get("error"), None, "{answer}");
        assert!(
            answer["diff"].as_str().unwrap().contains("+++ b/hello.txt"),
            "{answer}"
        );
    }
}
