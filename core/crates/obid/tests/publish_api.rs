//! Publishing the owner's work, on the real obid binary asked over HTTP (docs/runner-api.md, "Job",
//! "Job pipeline and pull request following", `GET` and `POST /v1/jobs/{id}/publish`; #451): a job
//! that ends done with commits publishes by itself (the gate, the push, the pull request); a
//! possible secret, or a scanner that can't run, sends nothing; one bounded round rewrites a
//! history that holds a secret; someone else's push is never overwritten; publishing by hand needs
//! a registered device's fresh signature of obid's own sentence; and the published pull request
//! is followed. obid never merges: the remote's base never moves.
//!
//! The remote is the e2e fixture's bare repository (a `file://` remote), and GitHub is a stand-in
//! `gh` first on obid's `PATH` that keeps what it was asked.

mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

/// The stand-in agent of these tests (`COMMIT:`, `LEAK:`, and the fix the publish gate asks for).
fn agent() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/publishing-claude")
}

/// The pull request the stand-in `gh` opens.
const PULL: &str = "https://github.com/obi-e2e/fixture-app/pull/7";

/// A stand-in `gh` in `folder/bin`, keeping its state in `folder/gh`: every call's arguments
/// (`calls`), its environment (`env`), the title and description it was given, the open pull
/// requests `pr list` answers (`open`: `__HEAD__` stands for the commit obid's clone has the
/// branch at, which is what obid pushed), whether `pr list` fails (`list-fails`, or once for the
/// branch's own lookup, as GitHub's 502: `list-fails-once`), and what `pr view` answers (`view.json`).
fn fake_gh(folder: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = folder.join("bin");
    let state = folder.join("gh");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let script = format!(
        r#"#!/bin/sh
dir='{state}'
printf '%s\n' "$*" >> "$dir/calls"
env >> "$dir/env"
prev=
case "$1 $2" in
  "pr list")
    [ -f "$dir/list-fails" ] && {{ echo 'GraphQL: Could not resolve to a Repository' >&2; exit 1; }}
    case "$*" in *--head*)
      [ -f "$dir/list-fails-once" ] && {{ rm "$dir/list-fails-once"; echo 'HTTP 502: Bad Gateway (https://api.github.com/graphql)' >&2; exit 1; }} ;;
    esac
    [ -f "$dir/open" ] || exit 0
    for a in "$@"; do [ "$prev" = --head ] && branch="$a"; prev="$a"; done
    oid=$(git -C '{clone}' rev-parse "refs/heads/$branch")
    sed "s/__HEAD__/$oid/" "$dir/open"
    exit 0 ;;
  "pr create")
    for a in "$@"; do
      [ "$prev" = --body-file ] && cp "$a" "$dir/body"
      case "$a" in --title=*) printf '%s' "${{a#--title=}}" > "$dir/title" ;; esac
      prev="$a"
    done
    echo '[{{"url":"{PULL}","isCrossRepository":false,"headRefOid":"__HEAD__","baseRefName":"main"}}]' > "$dir/open"
    echo 'Creating pull request…'
    echo '{PULL}'
    exit 0 ;;
  "pr edit")
    for a in "$@"; do [ "$prev" = --body-file ] && cp "$a" "$dir/body"; prev="$a"; done
    exit 0 ;;
  "pr view") [ -f "$dir/view.json" ] && cat "$dir/view.json" && exit 0; exit 1 ;;
  *) echo "gh: not in this test" >&2; exit 1 ;;
esac
"#,
        state = state.display(),
        clone = folder.join("home/clones/obi-e2e/fixture-app").display()
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// What the stand-in `gh` keeps under `name`, or nothing.
fn kept(folder: &Path, name: &str) -> String {
    std::fs::read_to_string(folder.join("gh").join(name)).unwrap_or_default()
}

/// obid with the fixture repository, the stand-in agent (its account signed in, so Oby's review
/// can run there), and `path_first` first on its `PATH`.
fn obid_with(folder: &Folder, path_first: &Path) -> (Obid, PathBuf, Value) {
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &agent());
    sign_in(folder, "stub");
    (start(&folder.0, &config, Some(path_first)), remote, config)
}

/// The test's account `id` is signed in: its folder holds a login file.
fn sign_in(folder: &Folder, id: &str) -> PathBuf {
    let dir = folder.0.join(format!("home/accounts/{id}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".credentials.json"), "{}").unwrap();
    dir
}

/// [`obid_with`], its quiet window (`stallAfter`) cut to `seconds`, which caps the waits before
/// each try again of a publish GitHub or the remote didn't take.
fn obid_retrying_within(folder: &Folder, path_first: &Path, seconds: u64) -> (Obid, PathBuf) {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agent());
    config["stallAfter"] = json!(seconds);
    (start(&folder.0, &config, Some(path_first)), remote)
}

/// The owner's own words, signed by their app as Oby's apps send them: words no app signed need
/// two reviews to merge (MERGING-14), [`dispatch_unsigned`].
fn dispatch(obid: &Obid, job: Value) -> String {
    let answer = call_typed(obid, "POST", "/v1/jobs", &json!({"job": job}));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

/// [`dispatch`] with words no app of the owner's signed.
fn dispatch_unsigned(obid: &Obid, job: Value) -> String {
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({"job": job})));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

fn ended(obid: &Obid, id: &str) -> Value {
    let answer = call(obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        // Not a committing job's objects, when this runs in its sandbox (sandbox::Spec::git_env).
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

fn stage(job: &Value, id: &str) -> Value {
    job["pipeline"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .cloned()
        .unwrap_or(Value::Null)
}

/// The made-up key id the stand-in agent leaks, put together here so this file holds none.
fn leaked_key() -> String {
    ["AK", "IA", "QWERTYUIOPASDFGH"].concat()
}

/// A job that ends done with a commit publishes by itself: the scan, one push of its branch to the
/// remote, its pull request opened into the base with a scanned title and description, every
/// outgoing action listed, and the pull request followed. Telling it more, its new commit goes to
/// the same branch and pull request. Nothing is merged, and the base on the remote never moves.
#[test]
fn a_done_job_publishes_by_itself_and_follows_its_pull_request() {
    let folder = Folder::new("publish-itself");
    let bin = fake_gh(&folder.0);
    std::fs::write(
        folder.0.join("gh/view.json"),
        r#"{"state":"OPEN","reviewDecision":"REVIEW_REQUIRED","mergedAt":null,"baseRefName":"main","headRefOid":""}"#,
    )
    .unwrap();
    let (obid, remote, _) = obid_with(&folder, &bin);
    let main = remote_branch(&remote, "main").unwrap();
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "test -f hello.txt", "title": "Hello file"}),
    );
    let job = ended(&obid, &id);
    let branch = format!("oby/build-{id}");
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["publishError"], Value::Null, "{job}");
    assert_eq!(job["published"], PULL);
    assert_eq!(job["commits"], 1);
    assert_eq!(job["pushed"], 1);
    assert_eq!(job["summary"], "Added hello.txt.");
    // The scan ran on what went out, and was clean (#735).
    let scan = &job["scan"];
    assert_eq!(scan["ran"], true, "{job}");
    assert_eq!(scan["clean"], true, "{job}");
    assert_eq!(scan["commits"], 1, "{job}");
    assert_eq!(scan["files"], 1, "{job}");
    assert!(scan["seconds"].is_u64(), "{job}");
    assert!(
        scan.get("findings").is_none() && scan.get("reason").is_none(),
        "{job}"
    );
    // The branch on the remote is the job's commit; the base is where it was.
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let head = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(
        remote_branch(&remote, &branch).as_deref(),
        Some(head.as_str())
    );
    assert_eq!(
        remote_branch(&remote, "main").as_deref(),
        Some(main.as_str())
    );
    // What went out, ids only, oldest first.
    let outgoing = job["outgoing"].as_array().unwrap();
    assert_eq!(outgoing.len(), 2, "{job}");
    assert_eq!(outgoing[0]["action"], "push");
    assert_eq!(outgoing[0]["id"], head.as_str());
    assert_eq!(outgoing[0]["branch"], branch.as_str());
    assert_eq!(outgoing[0]["repo"], "obi-e2e/fixture-app");
    assert_eq!(outgoing[1]["action"], "pullRequest");
    assert_eq!(outgoing[1]["id"], PULL);
    assert!(outgoing[1].get("branch").is_none());
    // The pull request: into the base, from the job's branch, ready for review, never merged.
    let calls = kept(&folder.0, "calls");
    assert!(
        calls.contains(&format!("pr create -R obi-e2e/fixture-app --base main --head {branch} --title=feat: added hello.txt")),
        "{calls}"
    );
    assert!(
        !calls.contains("--draft") && !calls.contains("pr merge"),
        "{calls}"
    );
    let body = kept(&folder.0, "body");
    assert!(
        body.starts_with("The agent's summary:\n\n> Added hello.txt.\n\n"),
        "{body}"
    );
    assert!(
        body.contains("- Check: ` test -f hello.txt ` passed.\n- Secret scan: clean."),
        "{body}"
    );
    assert!(
        body.contains("Oby merges it only when its owner says so."),
        "{body}"
    );
    // gh got no secret of obid's: not the API token, in its arguments or its environment.
    assert!(!calls.contains(TOKEN) && !kept(&folder.0, "env").contains(TOKEN));
    // Published: the Publish stage is done, and Oby's review is its own (below).
    assert_eq!(stage(&job, "publish")["state"], "done");
    assert_eq!(job["reviewError"], Value::Null, "{job}");
    assert_eq!(stage(&job, "merged")["state"], "upcoming");
    assert_eq!(stage(&job, "merged")["title"], "Merged into main");
    // Followed: obid reads the pull request right after it opened it.
    let followed = within(30, "obid reads the pull request", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        job["pullRequest"].is_object().then_some(job)
    });
    assert_eq!(followed["pullRequest"]["state"], "OPEN");
    assert_eq!(followed["pullRequest"]["reviewDecision"], "REVIEW_REQUIRED");
    assert_eq!(followed["pullRequest"]["base"], "main");
    assert_eq!(followed["pullRequestError"], Value::Null);
    assert!(calls_now(&folder.0).contains("pr view 7 --repo obi-e2e/fixture-app --json state,reviewDecision,mergedAt,baseRefName,headRefOid"));
    // Everything is published: nothing to publish by hand.
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None);
    assert_eq!(answer.status, 409, "{}", answer.text);
    assert_eq!(
        answer.body["error"],
        "It's already published, with every commit."
    );

    // Tried again, its new commit goes to the same branch and pull request.
    again_commits(&obid, &id, "second.txt");
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(told.status, 200, "{}", told.text);
    let again = ended(&obid, &id);
    assert_eq!(again["status"], "done", "{again}\n{}", obid.log());
    assert_eq!(again["commits"], 2);
    assert_eq!(again["pushed"], 2);
    assert_eq!(again["published"], PULL);
    let second = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(
        remote_branch(&remote, &branch).as_deref(),
        Some(second.as_str())
    );
    let actions: Vec<&str> = again["outgoing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["push", "pullRequest", "push", "pullRequestEdit"]);
    assert_eq!(calls_now(&folder.0).matches("pr create").count(), 1);
    assert!(kept(&folder.0, "body").contains("Round 2, the agent's summary:"));
    assert_eq!(
        remote_branch(&remote, "main").as_deref(),
        Some(main.as_str())
    );
}

/// A follow-up of a job whose pull request is still open goes on its branch: it starts at the
/// branch head and publishes onto it, updating that pull request rather than opening a second.
/// Its own commits are all it counts. Meanwhile the first job takes no Try again: its follow-up
/// carries its branch on.
#[test]
fn a_follow_up_of_published_work_updates_its_pull_request() {
    let folder = Folder::new("publish-follow-up");
    let bin = fake_gh(&folder.0);
    std::fs::write(
        folder.0.join("gh/view.json"),
        r#"{"state":"OPEN","reviewDecision":"REVIEW_REQUIRED","mergedAt":null,"baseRefName":"main","headRefOid":""}"#,
    )
    .unwrap();
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "test -f hello.txt", "title": "Hello file"}),
    );
    let first = ended(&obid, &id);
    assert_eq!(first["published"], PULL, "{first}");
    let branch = format!("oby/build-{id}");
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let head = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);

    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": "COMMIT: second.txt"})),
    );
    assert_eq!(told.status, 201, "{}", told.text);
    let next = told.body["id"].as_str().unwrap().to_owned();
    assert_eq!(told.body["branch"], branch.as_str());
    // The first job takes no Try again while its follow-up has its branch.
    let again = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(again.status, 422, "{}", again.text);
    let follow = ended(&obid, &next);
    assert_eq!(follow["status"], "done", "{follow}\n{}", obid.log());
    assert_eq!(follow["startCommit"], head.as_str());
    assert_eq!(follow["commits"], 1);
    assert_eq!(follow["published"], PULL);
    let actions: Vec<&str> = follow["outgoing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["push", "pullRequestEdit"]);
    assert_eq!(kept(&folder.0, "calls").matches("pr create").count(), 1);
    let pushed = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(
        remote_branch(&remote, &branch).as_deref(),
        Some(pushed.as_str())
    );
    assert_eq!(
        git(&clone, &["rev-parse", &format!("{pushed}~1")]),
        head,
        "on top of the first job's commit"
    );
    let again = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(again.status, 422, "{}", again.text);
    assert!(
        again.body["error"].as_str().unwrap().contains("follow-up"),
        "{}",
        again.text
    );
}

/// What a job hears when the owner tries it again (`Inbox.tryAgainMessage`): its session again,
/// where words for a job that ended would start a follow-up.
const TRY_AGAIN: &str =
    "Try again, picking up where you left off. Finish with a short summary as asked.";

/// The stand-in agent commits `file` in job `id`'s next Try again.
fn again_commits(obid: &Obid, id: &str, file: &str) {
    let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    let worktree = Path::new(job["worktree"].as_str().unwrap()).join(".obi");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(worktree.join("again"), format!("COMMIT: {file}")).unwrap();
}

fn calls_now(folder: &Path) -> String {
    kept(folder, "calls")
}

/// A possible secret in a commit that the fix round leaves there: nothing goes out (no push, no
/// pull request), the job fails naming the file, the line and the rule, and never the secret.
#[test]
fn a_possible_secret_sends_nothing() {
    let folder = Folder::new("publish-secret");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "LEAK: keys.txt STUBBORN"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    // It had its one fix round, and a second block of the same is the end.
    assert_eq!(job["round"], 2, "{job}");
    let summary = job["reason"].as_str().unwrap();
    assert!(
        summary.starts_with("Not published: possible secret in keys.txt:1 (AWS access key"),
        "{summary}"
    );
    assert!(!summary.contains(&leaked_key()), "{summary}");
    // It needs you, nothing lost: one more try at rewriting it, and the agent's own summary.
    assert_eq!(job["nextStep"], "Try again", "{job}");
    assert!(
        !job["summary"]
            .as_str()
            .unwrap()
            .starts_with("Not published:"),
        "{job}"
    );
    assert!(!obid.log().contains(&leaked_key()));
    // The scan's record names the file, line and rule, never the token (#735).
    let scan = &job["scan"];
    assert_eq!(
        (&scan["ran"], &scan["clean"]),
        (&json!(true), &json!(false)),
        "{job}"
    );
    let finding = &scan["findings"][0];
    assert_eq!(
        (&finding["file"], &finding["line"]),
        (&json!("keys.txt"), &json!(1)),
        "{job}"
    );
    assert!(finding["rule"].as_str().unwrap().contains("AWS"), "{job}");
    assert!(!job.to_string().contains(&leaked_key()), "{job}");
    let kept = std::fs::read_to_string(obid.home.join(format!("jobs/{id}.json"))).unwrap();
    assert!(!kept.contains(&leaked_key()));
    assert_eq!(job["published"], Value::Null);
    assert_eq!(job["outgoing"], json!([]));
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
    assert!(!calls_now(&folder.0).contains("pr create"));
}

/// A job with one round still gets one to rewrite what the scan found (seen live 2026-10-18: a
/// chat job's test token failed it outright, and Claude Code would just fix and commit again).
#[test]
fn a_one_round_job_still_gets_its_fix_round() {
    let folder = Folder::new("publish-fix-one");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "LEAK: keys.txt"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["round"], 2);
    assert_eq!(job["published"], PULL);
    let pushed = remote_branch(&remote, &format!("oby/build-{id}")).unwrap();
    let history = git(&remote, &["log", "-p", &format!("main..{pushed}")]);
    assert!(!history.contains(&leaked_key()), "{history}");
}

/// With a round left, the agent gets one to rewrite the commit that holds the secret (a fixup,
/// autosquashed onto the base); obid scans the rewritten history again and publishes it, and no
/// commit on the remote holds the secret.
#[test]
fn a_fix_round_rewrites_the_history_before_anything_goes_out() {
    let folder = Folder::new("publish-fix");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "LEAK: keys.txt", "rounds": 2}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["round"], 2);
    assert_eq!(job["summary"], "Took the key out of keys.txt.");
    assert_eq!(job["published"], PULL);
    let branch = format!("oby/build-{id}");
    let pushed = remote_branch(&remote, &branch).unwrap();
    let history = git(&remote, &["log", "-p", &format!("main..{pushed}")]);
    assert!(history.contains("keys.txt"), "{history}");
    assert!(!history.contains(&leaked_key()), "{history}");
}

/// When the scanner can't run (a gitleaks that is installed and broken), nothing goes out: the job
/// stays done with `publishError` saying so, and Publish by hand is offered.
#[test]
fn a_scanner_that_cant_run_sends_nothing() {
    let folder = Folder::new("publish-broken");
    let bin = fake_gh(&folder.0);
    let broken = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bin-broken-gitleaks");
    std::fs::copy(broken.join("gitleaks"), bin.join("gitleaks")).unwrap();
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let error = job["publishError"].as_str().unwrap();
    assert!(
        error.starts_with("Not published: the secret scan couldn't run (gitleaks couldn't run"),
        "{error}"
    );
    assert_eq!(job["published"], Value::Null);
    assert_eq!(job["outgoing"], json!([]));
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
    assert_eq!(stage(&job, "publish")["state"], "failed");
    assert_eq!(stage(&job, "publish")["reason"], error);
    // It didn't run, and the record says why (#735).
    assert_eq!(job["scan"], json!({"ran": false, "reason": error}));
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
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

/// What a device sends to publish `job` by `action`, signed at `at` (seconds since the epoch).
fn approval(
    key: &p256::ecdsa::SigningKey,
    device: &str,
    job: &str,
    action: &str,
    at: i64,
) -> Value {
    use base64::Engine as _;
    use p256::ecdsa::signature::Signer as _;
    let signed_at = obi_protocol::time::format(at);
    let signature: p256::ecdsa::Signature =
        key.sign(format!("oby-publish\n{job}\n{action}\n{signed_at}").as_bytes());
    json!({"device": device, "action": action, "signedAt": signed_at,
           "signature": base64::engine::general_purpose::STANDARD.encode(signature.to_der())})
}

/// Publishing by hand, for a job whose own publish failed: `GET` says obid's one sentence; `POST`
/// goes through only with a registered device's fresh signature of exactly that sentence, runs the
/// same gate (a scanner that can't run still sends nothing), and answers the pull request.
#[test]
fn publishing_by_hand_needs_a_devices_fresh_signature() {
    let folder = Folder::new("publish-by-hand");
    let bin = fake_gh(&folder.0);
    let broken = folder.0.join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bin-broken-gitleaks/gitleaks"),
        broken.join("gitleaks"),
    )
    .unwrap();
    std::fs::copy(bin.join("gh"), broken.join("gh")).unwrap();
    let (mut obid, remote, config) = obid_with(&folder, &broken);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "fix", "task": "COMMIT: hello.txt"}),
    );
    let job = ended(&obid, &id);
    assert!(job["publishError"].as_str().is_some(), "{job}");
    let branch = format!("oby/fix-{id}");
    let sentence =
        format!("Push {branch} to obi-e2e/fixture-app and open a pull request into main.");
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert_eq!(answer.body, json!({"action": sentence, "base": "main"}));

    let (key, public) = device(5);
    std::fs::write(obid.home.join("pairing"), &public).unwrap();
    let (stranger, _) = device(6);
    let registered = call(
        &obid,
        "POST",
        "/v1/devices",
        Some(&json!({"name": "Test iPhone", "publicKey": public})),
    );
    assert_eq!(registered.status, 200, "{}", registered.text);
    let device_id = registered.body["id"].as_str().unwrap().to_owned();
    let now = obi_protocol::time::now();
    let publish = |body: &Value| call(&obid, "POST", &format!("/v1/jobs/{id}/publish"), Some(body));
    for (body, why) in [
        (
            approval(&key, "nobody", &id, &sentence, now),
            "isn't registered",
        ),
        (
            approval(&stranger, &device_id, &id, &sentence, now),
            "signature",
        ),
        (
            approval(&key, &device_id, &id, "Push it anywhere.", now),
            "something else",
        ),
        (
            approval(&key, &device_id, &id, &sentence, now - 600),
            "two minutes",
        ),
    ] {
        let refused = publish(&body);
        assert_eq!(refused.status, 403, "{why}: {}", refused.text);
        assert!(
            refused.body["error"].as_str().unwrap().contains(why),
            "{}",
            refused.text
        );
    }
    assert_eq!(publish(&json!({"device": device_id})).status, 422);
    // The scanner still can't run: nothing goes out, by hand either.
    let blocked = publish(&approval(&key, &device_id, &id, &sentence, now));
    assert_eq!(blocked.status, 409, "{}", blocked.text);
    assert!(
        blocked.body["error"]
            .as_str()
            .unwrap()
            .contains("couldn't run"),
        "{}",
        blocked.text
    );
    assert_eq!(remote_branch(&remote, &branch), None);

    // With a scanner that runs, the same job publishes by hand.
    obid.stop();
    let obid = start(&folder.0, &config, Some(&bin));
    // A new approval: the one above was used up, though nothing went out.
    let now = now + 2;
    let good = approval(&key, &device_id, &id, &sentence, now);
    let published = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/publish"),
        Some(&good),
    );
    assert_eq!(published.status, 200, "{}\n{}", published.text, obid.log());
    assert_eq!(published.body, json!({"url": PULL}));
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["published"], PULL);
    assert_eq!(job["publishError"], Value::Null);
    assert_eq!(job["pushed"], 1);
    assert!(remote_branch(&remote, &branch).is_some());
    // Published by hand, Oby doesn't review it: the owner does, on GitHub.
    assert_eq!(job["reviewing"], Value::Null, "{job}");
    assert_eq!(
        stage(&job, "review")["detail"],
        "Pull request open · waiting for your review on GitHub"
    );
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    assert!(
        jobs.as_array()
            .unwrap()
            .iter()
            .all(|j| j["reviewOf"].is_null()),
        "{jobs}"
    );
    // The same approval again: there's nothing left to publish.
    let again = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/publish"),
        Some(&good),
    );
    assert_eq!(again.status, 409, "{}", again.text);
}

/// Someone else pushed to the job's branch: obid never overwrites it. The job stays done with
/// `publishError` saying so, and the remote branch keeps their commit.
#[test]
fn someone_elses_push_is_never_overwritten() {
    let folder = Folder::new("publish-lease");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt"}),
    );
    assert_eq!(ended(&obid, &id)["published"], PULL);
    let branch = format!("oby/build-{id}");
    // Someone pushes a commit of their own over the branch.
    let theirs = folder.0.join("theirs");
    git(
        &folder.0,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            theirs.to_str().unwrap(),
        ],
    );
    git(&theirs, &["checkout", "-q", "main"]);
    std::fs::write(theirs.join("theirs.txt"), "theirs\n").unwrap();
    git(&theirs, &["add", "theirs.txt"]);
    git(
        &theirs,
        &[
            "-c",
            "user.name=Someone",
            "-c",
            "user.email=someone@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "Theirs",
        ],
    );
    git(
        &theirs,
        &[
            "push",
            "-q",
            "-f",
            "origin",
            &format!("HEAD:refs/heads/{branch}"),
        ],
    );
    let their_commit = remote_branch(&remote, &branch).unwrap();
    again_commits(&obid, &id, "second.txt");
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(told.status, 200, "{}", told.text);
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let error = job["publishError"].as_str().unwrap();
    assert!(
        error.starts_with(&format!(
            "Not published: {branch} on GitHub is at {}",
            &their_commit[..12]
        )),
        "{error}"
    );
    assert_eq!(remote_branch(&remote, &branch), Some(their_commit));
}

/// The push went out and then GitHub's pull request lookup failed (#627), in a way a later try
/// wouldn't change: the job counts its pushed commits, and `publishError` is the pull request's
/// own failure, not "not published". Publishing by hand is still offered, for the pull request.
#[test]
fn a_pull_request_that_fails_after_the_push_still_counts_the_push() {
    let folder = Folder::new("publish-no-pull");
    let bin = fake_gh(&folder.0);
    std::fs::write(folder.0.join("gh/list-fails"), "").unwrap();
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt"}),
    );
    let job = ended(&obid, &id);
    let branch = format!("oby/build-{id}");
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["published"], Value::Null, "{job}");
    assert_eq!(job["commits"], 1);
    assert_eq!(job["pushed"], 1, "{job}");
    assert_eq!(
        job["publishError"],
        "Pushed, but couldn't look for the open pull request: GraphQL: Could not resolve to a Repository"
    );
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let head = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(remote_branch(&remote, &branch), Some(head.clone()));
    assert_eq!(job["outgoing"][0]["action"], "push");
    assert_eq!(job["outgoing"][0]["id"], head.as_str());
    assert_eq!(stage(&job, "publish")["state"], "failed");
    // Not tried again: GitHub doesn't know the repository.
    let lookups = calls_now(&folder.0)
        .lines()
        .filter(|call| call.starts_with("pr list") && call.contains("--head"))
        .count();
    assert_eq!(lookups, 1, "{}", calls_now(&folder.0));
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
}

/// GitHub answers 502 once: obid publishes again after a wait (`Publishing again in …`), and the
/// job ends published, with nothing for the owner to do.
#[test]
fn a_publish_github_turned_down_is_tried_again() {
    let folder = Folder::new("publish-retry");
    let bin = fake_gh(&folder.0);
    std::fs::write(folder.0.join("gh/list-fails-once"), "").unwrap();
    // Room for a loaded machine: the window caps the stand-in agent's quiet time too.
    let (obid, _) = obid_retrying_within(&folder, &bin, 10);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["published"], PULL, "{job}");
    assert_eq!(job["publishError"], Value::Null, "{job}");
    let log = obid.log();
    assert!(
        log.contains(&format!(
            "job {id}: Publishing again in 10 seconds (1 of 3): Pushed, but "
        )) && log.contains("HTTP 502: Bad Gateway"),
        "{log}"
    );
}

/// A try whose pull request failed pushed (#627): that push stays counted whatever comes after
/// it, a stop while obid waits to try again, or a later try that fails before it pushes.
#[test]
fn a_push_that_went_out_stays_counted_across_tries() {
    let folder = Folder::new("publish-kept-push");
    let bin = fake_gh(&folder.0);
    let (obid, remote) = obid_retrying_within(&folder, &bin, 10);
    let waiting = |id: &str| {
        within(60, "obid to wait before publishing again", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
            job["step"]
                .as_str()?
                .starts_with("Publishing again")
                .then_some(job)
        })
    };

    // Stopped while it waits.
    std::fs::write(folder.0.join("gh/list-fails-once"), "").unwrap();
    let stopped = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: a.txt"}),
    );
    assert_eq!(waiting(&stopped)["pushed"], 1);
    let stop = call(&obid, "POST", &format!("/v1/jobs/{stopped}/stop"), None);
    assert_eq!(stop.status, 200, "{}", stop.text);
    let job = ended(&obid, &stopped);
    assert_eq!(job["status"], "stopped", "{job}");
    assert_eq!(job["pushed"], 1, "{job}");
    assert_eq!(job["published"], Value::Null, "{job}");

    // The next try can't fetch the base: the push it made before still counts.
    std::fs::write(folder.0.join("gh/list-fails-once"), "").unwrap();
    let cut = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: b.txt"}),
    );
    waiting(&cut);
    let gone = folder.0.join("gone");
    std::fs::rename(&remote, &gone).unwrap();
    let job = ended(&obid, &cut);
    std::fs::rename(&gone, &remote).unwrap();
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["pushed"], 1, "{job}");
    let error = job["publishError"].as_str().unwrap();
    assert!(
        error.starts_with("Not published: couldn't fetch main: "),
        "{error}"
    );
}

/// A remote that isn't GitHub (here the fixture's `file://` one, with no `pullRequests` in the
/// configuration) gets the push and nothing else (#627): no `gh` pull request call, the job's
/// commits counted as pushed, and nothing left to publish by hand.
#[test]
fn a_remote_that_isnt_github_gets_the_push_and_no_pull_request() {
    let folder = Folder::new("publish-file-remote");
    let bin = fake_gh(&folder.0);
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agent());
    config["repos"][0]
        .as_object_mut()
        .unwrap()
        .remove("pullRequests");
    let obid = start(&folder.0, &config, Some(&bin));
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt"}),
    );
    let job = ended(&obid, &id);
    let branch = format!("oby/build-{id}");
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["publishError"], Value::Null, "{job}");
    assert_eq!(job["published"], Value::Null);
    assert_eq!(job["pushed"], 1, "{job}");
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let head = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(remote_branch(&remote, &branch), Some(head));
    // No pull request is looked for or opened (the GitHub feed's own `api graphql` calls aside).
    assert!(
        !calls_now(&folder.0)
            .lines()
            .any(|call| call.starts_with("pr ")),
        "{}",
        calls_now(&folder.0)
    );
    assert_eq!(stage(&job, "publish")["state"], "done");
    assert_eq!(
        stage(&job, "publish")["detail"],
        "Secret scan clean · pushed · no pull request for this remote"
    );
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None);
    assert_eq!(answer.status, 409, "{}", answer.text);
    assert_eq!(
        answer.body["error"],
        "Nothing to publish: the remote is already at its head."
    );
}

/// Stopped while it publishes: nothing more goes out. A gitleaks that takes its time holds the
/// scan open; the job is stopped then, and no push or pull request follows.
#[test]
fn a_job_stopped_while_it_publishes_sends_nothing_more() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new("publish-stop");
    let bin = fake_gh(&folder.0);
    std::fs::write(bin.join("gitleaks"), SLOW_GITLEAKS).unwrap();
    std::fs::set_permissions(bin.join("gitleaks"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt"}),
    );
    let publishing = within(60, "the job publishes", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["step"] == "Publishing").then_some(job)
    });
    assert_eq!(publishing["status"], "running");
    assert_eq!(publishing["publishStep"], "scan");
    assert_eq!(stage(&publishing, "publish")["state"], "current");
    assert_eq!(stage(&publishing, "publish")["detail"], "Secret scan");
    let stopped = call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    assert_eq!(stopped.status, 200, "{}", stopped.text);
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "stopped", "{job}");
    // Long enough for a publish that didn't stop to have pushed and opened its pull request.
    std::thread::sleep(std::time::Duration::from_secs(8));
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["status"], "stopped", "{job}");
    assert_eq!(job["outgoing"], json!([]));
    assert_eq!(job["published"], Value::Null);
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
    assert!(!calls_now(&folder.0).contains("pr create"));
}

/// What leaves in a commit besides its files and message is scanned too: a token as the author's
/// name is a possible secret, and a commit carrying a header obid doesn't publish (here one holding
/// a token) is refused. Nothing goes out either way.
#[test]
fn a_commits_author_and_headers_are_scanned_too() {
    let folder = Folder::new("publish-headers");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let author = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "AUTHOR: a.txt", "rounds": 2}),
    );
    let job = ended(&obid, &author);
    assert_eq!(job["status"], "interrupted", "{job}\n{}", obid.log());
    let summary = job["reason"].as_str().unwrap();
    assert!(
        summary.starts_with("Not published: possible secret in commit author:1 (GitHub token"),
        "{summary}"
    );
    // Who made a commit isn't the agent's to rewrite with a fixup: no fix round, and no next
    // step but a person's.
    assert_eq!(job["round"], 1);
    assert_eq!(job["nextStep"], Value::Null, "{job}");
    assert_eq!(remote_branch(&remote, &format!("oby/build-{author}")), None);
    let header = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "HEADER: b.txt"}),
    );
    let job = ended(&obid, &header);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    let error = job["publishError"].as_str().unwrap();
    assert!(
        error.contains("carries headers other than its tree, parents, author and committer"),
        "{error}"
    );
    assert_eq!(remote_branch(&remote, &format!("oby/build-{header}")), None);
    assert_eq!(job["outgoing"], json!([]));
    assert!(!calls_now(&folder.0).contains("pr create"));
}

/// An open pull request from a fork's branch of the same name isn't obid's: obid opens its own
/// rather than edit (and vouch for) someone else's.
#[test]
fn a_forks_pull_request_of_the_same_branch_is_never_taken_for_obids() {
    let folder = Folder::new("publish-fork");
    let bin = fake_gh(&folder.0);
    std::fs::write(
        folder.0.join("gh/open"),
        r#"[{"url":"https://github.com/obi-e2e/fixture-app/pull/9","isCrossRepository":true,"headRefOid":"__HEAD__","baseRefName":"main"}]"#,
    )
    .unwrap();
    let (obid, _, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["published"], PULL, "{job}");
    let calls = calls_now(&folder.0);
    assert!(calls.contains("pr create"), "{calls}");
    assert!(!calls.contains("pr edit"), "{calls}");
}

/// The base can't be fetched when it publishes: nothing goes out, and `publishError` says so
/// without git's command line or the remote's address.
#[test]
fn a_base_that_cant_be_fetched_sends_nothing() {
    let folder = Folder::new("publish-nofetch");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "SLOW: hello.txt"}),
    );
    within(60, "the agent starts", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["status"] == "running").then_some(())
    });
    let gone = folder.0.join("gone");
    std::fs::rename(&remote, &gone).unwrap();
    let job = ended(&obid, &id);
    std::fs::rename(&gone, &remote).unwrap();
    assert_eq!(job["status"], "done", "{job}");
    let error = job["publishError"].as_str().unwrap();
    assert!(
        error.starts_with("Not published: couldn't fetch main: "),
        "{error}"
    );
    assert!(!error.contains(remote.to_str().unwrap()), "{error}");
    assert!(
        !error.contains("credential.helper") && !error.contains("--no-tags"),
        "{error}"
    );
    assert_eq!(job["outgoing"], json!([]));
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
}

/// Jobs on one repository that end together all publish (#628): one keeping its commits in the
/// clone never breaks another's fetch of the base, and every branch reaches the remote.
#[test]
fn jobs_ending_together_on_one_clone_all_publish() {
    let folder = Folder::new("publish-together");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let ids: Vec<String> = (0..4)
        .map(|n| {
            dispatch(
                &obid,
                json!({"repo": "obi-e2e/fixture-app", "kind": "build",
                       "task": format!("SLOW: together-{n}.txt")}),
            )
        })
        .collect();
    for id in &ids {
        let job = ended(&obid, id);
        assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
        assert_eq!(job["publishError"], Value::Null, "{job}\n{}", obid.log());
        assert_eq!(job["published"], PULL, "{job}");
        let clone = obid.home.join("clones/obi-e2e/fixture-app");
        let head = git(
            &clone,
            &["rev-parse", &format!("refs/heads/oby/build-{id}")],
        );
        assert_eq!(
            remote_branch(&remote, &format!("oby/build-{id}")).as_deref(),
            Some(head.as_str())
        );
    }
}

/// The made-up GitHub token the stand-in agent uses (`AUTHOR:`, `HEADER:`, `FOLDER`), put together
/// here so this file holds none.
fn made_up_token() -> String {
    ["gh", "p_", "Zq8LmN2pR4sT6uV8wX0yA1bC3dE5fG7hJ9kM"].concat()
}

/// A token in a folder's name is found, however git writes the name in a diff's header (here one
/// holding " b/", which the header can't be split back into), and it is never repeated: not in
/// the job, its summary, the API, or obid's log.
#[test]
fn a_token_in_a_folders_name_is_found_and_never_repeated() {
    let folder = Folder::new("publish-folder");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "FOLDER"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}\n{}", obid.log());
    let summary = job["reason"].as_str().unwrap();
    assert!(
        summary.starts_with("Not published: possible secret in a file whose name obid won't show"),
        "{summary}"
    );
    assert!(summary.contains("GitHub token in a name"), "{summary}");
    // What the owner can do: where to look.
    assert!(
        summary.contains("it is among the names of the files and folders the branch adds"),
        "{summary}"
    );
    assert_eq!(job["publishStep"], Value::Null);
    let token = made_up_token();
    let api = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).text;
    assert!(!api.contains(&token), "{api}");
    assert!(!obid.log().contains(&token), "{}", obid.log());
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
}

/// The owner's own git configuration can't narrow the scan: with `log.showRoot = false` in the
/// global configuration of obid's `HOME`, a root commit merged in from an unrelated history still
/// has its files read.
#[test]
fn the_owners_git_configuration_cant_hide_a_root_commit() {
    let folder = Folder::new("publish-showroot");
    let bin = fake_gh(&folder.0);
    std::fs::create_dir_all(folder.0.join("env-home")).unwrap();
    std::fs::write(
        folder.0.join("env-home/.gitconfig"),
        "[log]\n\tshowRoot = false\n",
    )
    .unwrap();
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "ORPHAN STUBBORN"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}\n{}", obid.log());
    let summary = job["reason"].as_str().unwrap();
    assert!(
        summary.starts_with("Not published: possible secret in root.txt:1 (AWS access key"),
        "{summary}"
    );
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
}

/// Publishing by hand publishes the run that was approved: the sentence names the branch, not its
/// commits, so when obid described an earlier run in the two minutes before the device signed, the
/// approval could be of either and is refused, after a restart too. While a hand publish runs, the
/// job can't be dismissed.
#[test]
fn a_hand_publish_is_for_the_run_that_was_approved() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new("publish-run");
    let bin = fake_gh(&folder.0);
    let broken = folder.0.join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bin-broken-gitleaks/gitleaks"),
        broken.join("gitleaks"),
    )
    .unwrap();
    std::fs::copy(bin.join("gh"), broken.join("gh")).unwrap();
    let (mut obid, remote, config) = obid_with(&folder, &broken);
    // Two jobs whose own publish failed (the scanner can't run).
    let ran = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "fix", "task": "COMMIT: hello.txt"}),
    );
    let held = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "fix", "task": "COMMIT: other.txt"}),
    );
    assert!(ended(&obid, &ran)["publishError"].is_string());
    assert!(ended(&obid, &held)["publishError"].is_string());
    let (key, public) = device(9);
    std::fs::write(obid.home.join("pairing"), &public).unwrap();
    let device_id = call(
        &obid,
        "POST",
        "/v1/devices",
        Some(&json!({"name": "Test iPhone", "publicKey": public})),
    )
    .body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let sentence_of = |obid: &Obid, id: &str| {
        call(obid, "GET", &format!("/v1/jobs/{id}/publish"), None).body["action"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    // obid describes the first job's run; then it runs again, and its own publish fails again:
    // the same sentence, other commits. Another device asks again, and this one signs.
    let sentence = sentence_of(&obid, &ran);
    again_commits(&obid, &ran, "second.txt");
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{ran}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(told.status, 200, "{}", told.text);
    assert_eq!(ended(&obid, &ran)["commits"], 2);
    assert_eq!(sentence_of(&obid, &ran), sentence);
    let refused = |obid: &Obid, at: i64| {
        let answer = call(
            obid,
            "POST",
            &format!("/v1/jobs/{ran}/publish"),
            Some(&approval(&key, &device_id, &ran, &sentence, at)),
        );
        assert_eq!(answer.status, 409, "{}", answer.text);
        assert_eq!(
            answer.body["error"],
            "The job ran again since obid said what publishing it would do. Approve it again in two minutes."
        );
    };
    refused(&obid, obi_protocol::time::now());
    // What obid said is kept with the job: a restart doesn't forget it.
    obid.stop();
    std::fs::write(bin.join("gitleaks"), SLOW_GITLEAKS).unwrap();
    std::fs::set_permissions(bin.join("gitleaks"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let obid = start(&folder.0, &config, Some(&bin));
    refused(&obid, obi_protocol::time::now() + 1);
    assert_eq!(remote_branch(&remote, &format!("oby/fix-{ran}")), None);

    // A gitleaks that takes its time holds the other job's hand publish open: no dismissing it
    // meanwhile.
    let sentence = sentence_of(&obid, &held);
    let good = approval(
        &key,
        &device_id,
        &held,
        &sentence,
        obi_protocol::time::now() + 1,
    );
    std::thread::scope(|scope| {
        let publishing = scope.spawn(|| {
            call(
                &obid,
                "POST",
                &format!("/v1/jobs/{held}/publish"),
                Some(&good),
            )
        });
        within(30, "the hand publish is under way", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{held}"), None).body;
            (job["publishStep"] == "scan").then_some(())
        });
        let dismissed = call(&obid, "POST", &format!("/v1/jobs/{held}/dismiss"), None);
        assert_eq!(dismissed.status, 409, "{}", dismissed.text);
        assert!(
            dismissed.body["error"]
                .as_str()
                .unwrap()
                .contains("publishing")
        );
        let published = publishing.join().unwrap();
        assert_eq!(published.status, 200, "{}", published.text);
    });
    let job = call(&obid, "GET", &format!("/v1/jobs/{held}"), None).body;
    assert_eq!(job["dismissed"], Value::Null);
    assert_eq!(job["pushed"], 1);
}

/// A folder first on obid's `PATH` with the stand-in `gh` of `bin` and a gitleaks that can't run:
/// every publish fails before anything goes out.
fn broken_scanner(folder: &Folder, bin: &Path) -> PathBuf {
    let broken = folder.0.join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bin-broken-gitleaks/gitleaks"),
        broken.join("gitleaks"),
    )
    .unwrap();
    std::fs::copy(bin.join("gh"), broken.join("gh")).unwrap();
    broken
}

/// A device registered with obid, its key and id.
fn registered(obid: &Obid, scalar: u8) -> (p256::ecdsa::SigningKey, String) {
    let (key, public) = device(scalar);
    std::fs::write(obid.home.join("pairing"), &public).unwrap();
    let answer = call(
        obid,
        "POST",
        "/v1/devices",
        Some(&json!({"name": "Test iPhone", "publicKey": public})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    (key, answer.body["id"].as_str().unwrap().to_owned())
}

/// PUBLISHING-1 and PUBLISHING-2 (docs/contracts/publishing.md): a job's publish pushes the commit
/// its check passed on. Its branch moved in obid's clone since (another job's commit on it),
/// Publish by hand sends nothing and says why; back at that commit, it goes out, and nothing else.
#[test]
fn publishing_1_2_only_the_commit_its_check_passed_on_goes_out() {
    let folder = Folder::new("publish-checked-head");
    let bin = fake_gh(&folder.0);
    let broken = broken_scanner(&folder, &bin);
    let (mut obid, remote, config) = obid_with(&folder, &broken);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "fix", "task": "COMMIT: hello.txt",
               "check": "test -f hello.txt"}),
    );
    let job = ended(&obid, &id);
    assert!(job["publishError"].is_string(), "{job}");
    let branch = format!("oby/fix-{id}");
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let checked = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);
    // Another commit on its branch, one its check never ran on.
    let tree = git(&clone, &["rev-parse", &format!("{checked}^{{tree}}")]);
    let other = git(
        &clone,
        &[
            "-c",
            "user.name=x",
            "-c",
            "user.email=x@x",
            "commit-tree",
            &tree,
            "-p",
            &checked,
            "-m",
            "unchecked",
        ],
    );
    git(
        &clone,
        &["update-ref", &format!("refs/heads/{branch}"), &other],
    );
    obid.stop();
    let obid = start(&folder.0, &config, Some(&bin));
    let (key, device_id) = registered(&obid, 11);
    let publish = |at: i64| {
        let sentence = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None).body["action"]
            .as_str()
            .unwrap()
            .to_owned();
        call(
            &obid,
            "POST",
            &format!("/v1/jobs/{id}/publish"),
            Some(&approval(&key, &device_id, &id, &sentence, at)),
        )
    };
    let refused = publish(obi_protocol::time::now());
    assert_eq!(refused.status, 409, "{}", refused.text);
    assert_eq!(
        refused.body["error"],
        format!(
            "Not published: {branch} moved since this job's check passed on it, so its commits now aren't the ones checked."
        )
    );
    assert_eq!(remote_branch(&remote, &branch), None);
    // Back at the commit its check passed on: that one goes out.
    git(
        &clone,
        &["update-ref", &format!("refs/heads/{branch}"), &checked],
    );
    let published = publish(obi_protocol::time::now() + 1);
    assert_eq!(published.status, 200, "{}\n{}", published.text, obid.log());
    assert_eq!(
        remote_branch(&remote, &branch).as_deref(),
        Some(checked.as_str())
    );
}

/// PUBLISHING-3, the core audit's first trigger (#1635): a published job's next run passes its
/// check and its publish fails; a follow-up on its branch then fails its check. Publish on the
/// first job is refused while the follow-up carries its branch on, and once the follow-up's
/// commits moved the branch, it's refused for that, and offered no more: its failing commits never
/// go out.
#[test]
fn publishing_3_a_follow_ups_failing_commits_never_go_out_on_the_first_job() {
    let folder = Folder::new("publish-follow-up-fails");
    let bin = fake_gh(&folder.0);
    std::fs::write(
        folder.0.join("gh/view.json"),
        r#"{"state":"OPEN","reviewDecision":"REVIEW_REQUIRED","mergedAt":null,"baseRefName":"main","headRefOid":""}"#,
    )
    .unwrap();
    let broken = broken_scanner(&folder, &bin);
    let (mut obid, remote, config) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "test ! -f bad.txt", "title": "Hello file"}),
    );
    assert_eq!(ended(&obid, &id)["published"], PULL);
    let branch = format!("oby/build-{id}");
    let first = remote_branch(&remote, &branch).unwrap();
    // Its next run passes its check, and its publish fails: it waits for Publish by hand.
    obid.stop();
    let obid = start(&folder.0, &config, Some(&broken));
    again_commits(&obid, &id, "second.txt");
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(told.status, 200, "{}", told.text);
    let job = ended(&obid, &id);
    assert!(job["publishError"].is_string(), "{job}");
    assert_eq!(
        call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None).status,
        200
    );
    // A follow-up on its branch, whose commit (after 3 seconds) fails its check.
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": "SLOW: bad.txt"})),
    );
    assert_eq!(told.status, 201, "{}", told.text);
    assert_eq!(told.body["branch"], branch.as_str());
    let next = told.body["id"].as_str().unwrap().to_owned();
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None);
    assert_eq!(answer.status, 409, "{}", answer.text);
    assert!(
        answer.body["error"].as_str().unwrap().contains("follow-up"),
        "{}",
        answer.text
    );
    let follow = ended(&obid, &next);
    assert_ne!(follow["status"], "done", "{follow}");
    assert_eq!(follow["checks"][0]["steps"][0]["passed"], false, "{follow}");
    // Its commits moved the branch: the first job is offered no Publish, and one by hand refused.
    let moved = format!(
        "Not published: {branch} moved since this job's check passed on it, so its commits now aren't the ones checked."
    );
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None);
    assert_eq!(answer.status, 409, "{}", answer.text);
    assert_eq!(answer.body["error"], moved.as_str());
    let (key, device_id) = registered(&obid, 12);
    let action = format!(
        "Push the new commits on {branch} to obi-e2e/fixture-app, for its pull request into main."
    );
    let refused = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/publish"),
        Some(&approval(
            &key,
            &device_id,
            &id,
            &action,
            obi_protocol::time::now(),
        )),
    );
    assert_eq!(refused.status, 409, "{}", refused.text);
    assert_eq!(refused.body["error"], moved.as_str());
    assert_eq!(
        remote_branch(&remote, &branch).as_deref(),
        Some(first.as_str())
    );
}

/// PUBLISHING-5: a check passes on the commits alone or publishes nothing. One that ran with a
/// file left uncommitted (after the one round Oby tells it to commit) and one that committed while
/// it ran both end done with why, nothing pushed, and no Publish offered.
#[test]
fn publishing_5_a_check_that_didnt_check_the_commits_publishes_nothing() {
    let folder = Folder::new("publish-unchecked");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let no_publish = |id: &str, why: &str| {
        let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None);
        assert_eq!(answer.status, 409, "{}", answer.text);
        assert_eq!(answer.body["error"], why);
    };
    // Published once; then a file it never commits is there when its next check runs.
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "test -f hello.txt"}),
    );
    assert_eq!(ended(&obid, &id)["published"], PULL);
    let branch = format!("oby/build-{id}");
    let first = remote_branch(&remote, &branch).unwrap();
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    std::fs::write(
        Path::new(job["worktree"].as_str().unwrap()).join("junk.txt"),
        "not committed\n",
    )
    .unwrap();
    again_commits(&obid, &id, "second.txt");
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(told.status, 200, "{}", told.text);
    let job = ended(&obid, &id);
    let dirty = "Not published: its check ran with changes in the worktree that weren't committed, so it didn't check the commits alone. Tell Oby to commit them or drop them, and it checks again.";
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["reason"], dirty, "{job}");
    no_publish(&id, dirty);
    assert_eq!(
        remote_branch(&remote, &branch).as_deref(),
        Some(first.as_str())
    );

    // A check that commits while it runs commits in its own checkout (PUBLISHING-6): the job's
    // branch never gets that commit, and what goes out is the commit the check passed on.
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: other.txt",
               "check": "git -c user.name=x -c user.email=x@x -c commit.gpgsign=false commit -q --allow-empty -m sneaky"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["reason"], Value::Null, "{job}");
    let pushed = remote_branch(&remote, &format!("oby/build-{id}")).unwrap();
    assert_eq!(
        git(&remote, &["log", "-1", "--format=%s", &pushed]),
        "Add other.txt"
    );
}

/// PUBLISHING-5 and 6: what a check writes (a report, untracked) stays in its own checkout, so it's
/// never the job's uncommitted work. Its first run's check writes `report.xml`; Try again's job
/// publishes its new commit without being told to commit the report, which never reaches the
/// job's worktree or goes out. (The stand-in agent can't do a second round of one
/// run: the failed check's prompt quotes its task, and it commits the same file again.)
#[test]
fn publishing_5_what_a_check_leaves_isnt_uncommitted_work() {
    let folder = Folder::new("publish-check-left");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "echo '<testsuite/>' > report.xml"}),
    );
    assert_eq!(ended(&obid, &id)["published"], PULL);
    again_commits(&obid, &id, "second.txt");
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(told.status, 200, "{}", told.text);
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["reason"], Value::Null, "{job}");
    assert_eq!(job["pushed"], 2, "{job}");
    let branch = format!("oby/build-{id}");
    let pushed = remote_branch(&remote, &branch).unwrap();
    let files = git(&remote, &["ls-tree", "-r", "--name-only", &pushed]);
    assert!(files.lines().any(|f| f == "second.txt"), "{files}");
    assert!(!files.lines().any(|f| f == "report.xml"), "{files}");
    // The report stayed in the check's own checkout, gone with it.
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert!(
        !Path::new(job["worktree"].as_str().unwrap())
            .join("report.xml")
            .exists()
    );
}

/// The fixture's remote with `files` (a path, its content, whether it runs) committed on `main`
/// by someone, as a repository's own check script is there before any job.
fn base_with(folder: &Folder, files: &[(&str, &str, bool)]) -> PathBuf {
    let remote = fixture(&folder.0.join("fixture"));
    git(
        &folder.0,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            folder.0.join("theirs").to_str().unwrap(),
        ],
    );
    commit_on(folder, "main", files);
    remote
}

/// Someone's commit of `files` on `branch` of the remote [`base_with`] made, from `main`.
fn commit_on(folder: &Folder, branch: &str, files: &[(&str, &str, bool)]) {
    use std::os::unix::fs::PermissionsExt;
    let theirs = folder.0.join("theirs");
    git(&theirs, &["checkout", "-q", "-B", branch, "origin/main"]);
    for (path, text, runs) in files {
        let at = theirs.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(&at, text).unwrap();
        let mode = if *runs { 0o755 } else { 0o644 };
        std::fs::set_permissions(&at, std::fs::Permissions::from_mode(mode)).unwrap();
    }
    git(&theirs, &["add", "-A"]);
    git(
        &theirs,
        &[
            "-c",
            "user.name=Someone",
            "-c",
            "user.email=someone@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "The check",
        ],
    );
    git(&theirs, &["push", "-q", "origin", branch]);
}

/// obid on the remote [`base_with`] made, `bin` first on its `PATH`.
fn obid_on(folder: &Folder, remote: &Path, bin: &Path) -> Obid {
    let config = config(&folder.0, remote, &agent());
    sign_in(folder, "stub");
    start(&folder.0, &config, Some(bin))
}

/// PUBLISHING-6: the check runs in a clean checkout of the commit, with the base's own
/// `.gitleaks.toml`: a `.env` the agent left (ignored, so not uncommitted work) isn't there, and
/// the job's own `.gitleaks.toml` isn't what the check reads, though it's what goes out.
#[test]
fn publishing_6_the_check_runs_on_the_commit_alone_with_the_bases_files() {
    let folder = Folder::new("publish-base-files");
    let bin = fake_gh(&folder.0);
    let remote = base_with(
        &folder,
        &[
            (".gitignore", ".env\n", false),
            (".gitleaks.toml", "# BASE\n", false),
        ],
    );
    let obid = obid_on(&folder, &remote, &bin);
    // Its own `.gitleaks.toml` says "hello": the check reads the base's.
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: .gitleaks.toml",
               "check": "grep -q BASE .gitleaks.toml"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["published"], PULL, "{job}\n{}", obid.log());
    let pushed = remote_branch(&remote, &format!("oby/build-{id}")).unwrap();
    assert_eq!(
        git(&remote, &["show", &format!("{pushed}:.gitleaks.toml")]),
        "hello"
    );
    // A `.env` it left makes no check pass: the commit, as it goes out, has none.
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt LEAVE: .env",
               "check": "test -f .env", "rounds": 1}),
    );
    let job = ended(&obid, &id);
    let worktree = Path::new(job["worktree"].as_str().unwrap());
    assert!(worktree.join(".env").is_file(), "{job}");
    assert_eq!(job["checks"][0]["steps"][0]["passed"], false, "{job}");
    assert_ne!(job["status"], "done", "{job}");
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
}

/// PUBLISHING-7: a task that rewrites its check script to `exit 0` still runs the base's, which
/// fails: nothing goes out.
#[test]
fn publishing_7_a_check_rewritten_to_pass_still_runs_the_bases() {
    let folder = Folder::new("publish-base-script");
    let bin = fake_gh(&folder.0);
    let remote = base_with(
        &folder,
        &[(
            "check.sh",
            "#!/bin/sh\necho the base check ran\nexit 1\n",
            true,
        )],
    );
    let obid = obid_on(&folder, &remote, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "PASS: check.sh",
               "check": "./check.sh", "rounds": 1}),
    );
    let job = ended(&obid, &id);
    assert_eq!(
        job["checks"][0]["steps"][0]["passed"],
        false,
        "{job}\n{}",
        obid.log()
    );
    assert_ne!(job["status"], "done", "{job}");
    let log = std::fs::read_to_string(obid.home.join(format!("logs/{id}/check.log"))).unwrap();
    assert!(log.contains("the base check ran"), "{log}");
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
}

/// PUBLISHING-7: what the check runs can't rewrite the base's script while it runs (a shell reads
/// it as it goes: emptied, it ends with the last command's 0), nor move it out from under that
/// with a folder it's in, nor rewrite the base's `.gitleaks.toml`.
#[test]
fn publishing_7_a_running_check_cant_rewrite_the_bases_script() {
    let folder = Folder::new("publish-script-frozen");
    let bin = fake_gh(&folder.0);
    let script = "#!/bin/sh\ntrue > tools/ci/check.sh\nmv tools moved && true > moved/ci/check.sh\nmv tools/ci tools/moved && true > tools/moved/check.sh\ntrue > .gitleaks.toml && echo the gitleaks config was rewritten\ntest -s .gitleaks.toml && test -s tools/ci/check.sh && test ! -e moved && test ! -e tools/moved && echo the base check stayed\nexit 1\n";
    let remote = base_with(
        &folder,
        &[
            ("tools/ci/check.sh", script, true),
            (".gitleaks.toml", "# BASE\n", false),
        ],
    );
    let obid = obid_on(&folder, &remote, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "tools/ci/check.sh", "rounds": 1}),
    );
    let job = ended(&obid, &id);
    assert_eq!(
        job["checks"][0]["steps"][0]["passed"],
        false,
        "{job}\n{}",
        obid.log()
    );
    let log = std::fs::read_to_string(obid.home.join(format!("logs/{id}/check.log"))).unwrap();
    assert!(log.contains("the base check stayed"), "{log}");
    assert!(!log.contains("the gitleaks config was rewritten"), "{log}");
    assert!(
        log.contains("runs tools/ci/check.sh as main has it"),
        "{log}"
    );
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
}

/// PUBLISHING-6: a job based on a branch obid pushed (`oby/…`, another task's, whose check script
/// passes) is checked with the default branch's script, never that branch's.
#[test]
fn publishing_6_a_check_never_comes_from_a_branch_obid_pushed() {
    let folder = Folder::new("publish-oby-base");
    let bin = fake_gh(&folder.0);
    let remote = base_with(
        &folder,
        &[(
            "check.sh",
            "#!/bin/sh\necho the check main has\nexit 1\n",
            true,
        )],
    );
    commit_on(
        &folder,
        "oby/a",
        &[("check.sh", "#!/bin/sh\nexit 0\n", true)],
    );
    let obid = obid_on(&folder, &remote, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "base": "oby/a", "check": "./check.sh", "rounds": 1}),
    );
    let job = ended(&obid, &id);
    assert_eq!(
        job["checks"][0]["steps"][0]["passed"],
        false,
        "{job}\n{}",
        obid.log()
    );
    let log = std::fs::read_to_string(obid.home.join(format!("logs/{id}/check.log"))).unwrap();
    assert!(log.contains("the check main has"), "{log}");
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
}

/// PUBLISHING-5: a commit that lands on the job's worktree while its check runs (not the check's:
/// it can't reach that worktree) leaves nothing checked: done, why, nothing pushed.
#[test]
fn publishing_5_a_commit_made_while_the_check_runs_isnt_checked() {
    let folder = Folder::new("publish-moved-head");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "sleep 4"}),
    );
    let started = std::time::Instant::now();
    let job = loop {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        // Its check's step is running: it's checking the commit it read before it.
        if job["checks"][0]["steps"][0]["running"] == true || started.elapsed().as_secs() > 60 {
            break job;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    assert_eq!(job["checks"][0]["steps"][0]["running"], true, "{job}");
    // With the job's own object folder, where its commits are, as its agent's git has it.
    let worktree = Path::new(job["worktree"].as_str().unwrap());
    let pointer = std::fs::read_to_string(worktree.join(".git")).unwrap();
    let admin = PathBuf::from(pointer.trim().strip_prefix("gitdir: ").unwrap());
    let common = admin.parent().unwrap().parent().unwrap();
    let made = std::process::Command::new("git")
        .env("GIT_OBJECT_DIRECTORY", admin.join("objects"))
        .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", common.join("objects"))
        .args([
            "-c",
            "user.name=x",
            "-c",
            "user.email=x@x",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(["commit", "-q", "--allow-empty", "-m", "outside"])
        .current_dir(worktree)
        .output()
        .unwrap();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    let job = ended(&obid, &id);
    let moved = "Not published: its commits changed while its check ran, so the check didn't pass on the ones there now. Tell Oby to carry on, and it checks them again.";
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["reason"], moved, "{job}");
    assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
}

/// What a check cut short by obid's stop left (its checkout, its git folder in the clone) is gone
/// once obid starts again; the clone's other worktrees stay.
#[test]
fn a_check_cut_short_leaves_nothing_after_a_restart() {
    let folder = Folder::new("publish-check-sweep");
    let bin = fake_gh(&folder.0);
    let remote = fixture(&folder.0.join("fixture"));
    let home = folder.0.join("home");
    let admins = home.join("clones/obi-e2e/fixture-app/.git/worktrees");
    for left in [
        home.join("checks/abc-123/tmp"),
        admins.join("check-abc-123/objects"),
        admins.join("abc"),
    ] {
        std::fs::create_dir_all(left).unwrap();
    }
    let obid = start(&folder.0, &config(&folder.0, &remote, &agent()), Some(&bin));
    assert_eq!(call(&obid, "GET", "/v1/jobs", None).status, 200);
    assert!(!home.join("checks").exists());
    assert!(!admins.join("check-abc-123").exists());
    assert!(admins.join("abc").exists());
}

/// PUBLISHING-8: a check whose script the base doesn't have (added in this very task) counts for
/// nothing: the job ends done with why, nothing published by itself, and Publish by hand, a
/// click, pushes it.
#[test]
fn publishing_8_a_check_the_base_doesnt_have_counts_for_nothing() {
    let folder = Folder::new("publish-no-base-script");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "PASS: check.sh",
               "check": "sh check.sh"}),
    );
    let job = ended(&obid, &id);
    let why = "Not published: its check runs check.sh as main has it, and main has no check.sh, so nothing counts as checked. Read its change, then Publish it by hand.";
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["reason"], why, "{job}");
    assert!(job["checks"].as_array().is_none_or(Vec::is_empty), "{job}");
    let branch = format!("oby/build-{id}");
    assert_eq!(remote_branch(&remote, &branch), None);
    let (key, device_id) = registered(&obid, 13);
    let sentence = call(&obid, "GET", &format!("/v1/jobs/{id}/publish"), None);
    assert_eq!(sentence.status, 200, "{}", sentence.text);
    let published = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/publish"),
        Some(&approval(
            &key,
            &device_id,
            &id,
            sentence.body["action"].as_str().unwrap(),
            obi_protocol::time::now(),
        )),
    );
    assert_eq!(published.status, 200, "{}\n{}", published.text, obid.log());
    assert!(remote_branch(&remote, &branch).is_some());
}

/// PUBLISHING-4, Oby's review: changes it asks for start no round while another job works on the
/// job's branch; the job says why instead.
#[test]
fn publishing_4_a_reviews_changes_wait_off_a_busy_branch() {
    let folder = Folder::new("publish-review-busy");
    let bin = fake_gh(&folder.0);
    let (obid, _remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build",
               "task": "COMMIT: hello.txt WANTS: fix.txt HOLD REVIEW", "check": "test -f hello.txt"}),
    );
    let job_of = |id: &str| call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    let review = review_of(&obid, &id, 0);
    let review = within(60, "the review runs", || {
        let review = job_of(review["id"].as_str().unwrap());
        (review["status"] == "running").then_some(review)
    });
    // Another job on its branch, at work while the review answers.
    let branch = format!("oby/build-{id}");
    let other = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "Look", "branch": branch,
               "check": "sleep 60"}),
    );
    within(60, "the other job's check runs", || {
        (job_of(&other)["step"] == "Running sleep 60").then_some(())
    });
    std::fs::write(
        Path::new(review["worktree"].as_str().unwrap()).join(".obi/release"),
        "",
    )
    .unwrap();
    let job = within(120, "the review's verdict", || {
        let job = job_of(&id);
        job["reviewError"].is_string().then_some(job)
    });
    assert_eq!(job["status"], "done", "{job}");
    assert!(
        job["reviewError"]
            .as_str()
            .unwrap()
            .contains(&format!("(job {other}) is working on {branch} now")),
        "{job}"
    );
    call(&obid, "POST", "/v1/stop-all", None);
}

/// Only the names a job adds are its own: a base that already holds `.env.development` and a
/// fixture `id_rsa` in a folder still publishes a job that adds a file next to them, and a job
/// that adds a `.env` there is still stopped.
#[test]
fn names_already_on_the_base_are_the_bases() {
    let folder = Folder::new("publish-base-names");
    let bin = fake_gh(&folder.0);
    let remote = fixture(&folder.0.join("fixture"));
    // The base gets the files a person committed there before.
    let theirs = folder.0.join("theirs");
    git(
        &folder.0,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            theirs.to_str().unwrap(),
        ],
    );
    git(&theirs, &["checkout", "-q", "main"]);
    std::fs::create_dir_all(theirs.join("cfg/.env.d")).unwrap();
    std::fs::write(theirs.join("cfg/.env.development"), "MODE=dev\n").unwrap();
    std::fs::write(theirs.join("cfg/id_rsa"), "not a key, a fixture\n").unwrap();
    std::fs::write(theirs.join("cfg/.env.d/a.txt"), "a\n").unwrap();
    git(&theirs, &["add", "-A"]);
    git(
        &theirs,
        &[
            "-c",
            "user.name=Someone",
            "-c",
            "user.email=someone@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "Fixtures",
        ],
    );
    git(&theirs, &["push", "-q", "origin", "main"]);
    let config = config(&folder.0, &remote, &agent());
    let obid = start(&folder.0, &config, Some(&bin));
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: cfg/b.txt"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["publishError"], Value::Null, "{job}");
    assert_eq!(job["published"], PULL);
    let env = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: cfg/.env"}),
    );
    let job = ended(&obid, &env);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert!(
        job["reason"]
            .as_str()
            .unwrap()
            .starts_with("Not published: possible secret in .env (.env file)"),
        "{job}"
    );
    assert_eq!(remote_branch(&remote, &format!("oby/build-{env}")), None);
}

/// A stand-in gitleaks that takes its time, then reads its input to the end and finds only the
/// canary obid puts last (crate::scan, `run_gitleaks`).
const SLOW_GITLEAKS: &str = "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in --report-path=*) r=\"${a#--report-path=}\";; esac; done\n\
sleep 2\nn=$(awk 'END {print NR}' \"$2\")\necho \"[{\\\"StartLine\\\": $n, \\\"RuleID\\\": \\\"github-pat\\\"}]\" > \"$r\"\nexit 99\n";

/// A gitleaks that reports a finding in the names without saying which (a line it wasn't given)
/// still stops the publish: nothing is dropped as clean. One with no report it could read is a
/// gitleaks that didn't read its input, a scan that couldn't run (crate::scan's tests).
#[test]
fn a_name_finding_gitleaks_cant_place_still_counts() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new("publish-unplaced");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    // It reads to the end (its report has obid's canary, the last line) and, in the names only
    // (a line that reads exactly the file's name), finds something it can't place: line 999.
    let file = "trigger-two";
    {
        std::fs::write(
            bin.join("gitleaks"),
            format!(
                "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in --report-path=*) r=\"${{a#--report-path=}}\";; esac; done\n\
                 n=$(awk 'END {{print NR}}' \"$2\")\n\
                 if grep -qx '{file}' \"$2\"; then extra='{{\"StartLine\": 999, \"RuleID\": \"github-pat\"}}, '; else extra=''; fi\n\
                 echo \"[$extra{{\\\"StartLine\\\": $n, \\\"RuleID\\\": \\\"github-pat\\\"}}]\" > \"$r\"\nexit 99\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(bin.join("gitleaks"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let id = dispatch(
            &obid,
            json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": format!("EMPTY: {file}")}),
        );
        let job = ended(&obid, &id);
        assert_eq!(job["status"], "interrupted", "{file}: {job}");
        let summary = job["reason"].as_str().unwrap();
        assert!(summary.contains("(gitleaks"), "{file}: {summary}");
        assert!(summary.contains("in a name)"), "{file}: {summary}");
        assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
    }
}

/// The clone's own settings can't hide a name: with `log.diffMerges = off` a `.env.local` added
/// in a merge commit itself (in neither parent) is still found, and with
/// `diff.ignoreSubmodules = all` a submodule named with a token is too.
#[test]
fn the_clones_settings_cant_hide_a_merge_or_a_submodule() {
    let folder = Folder::new("publish-clone-settings");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    // A first job makes the clone; once Oby's review of it is over too, nothing of obid's runs
    // there (it writes the clone's settings getting it ready for a job), and they are changed.
    let warm = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: warm.txt"}),
    );
    assert_eq!(reviewed(&obid, &warm, 1)["published"], PULL);
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    git(&clone, &["config", "log.diffMerges", "off"]);
    git(&clone, &["config", "diff.ignoreSubmodules", "all"]);
    let evil = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "EVIL: .env.local"}),
    );
    let job = ended(&obid, &evil);
    assert_eq!(job["status"], "interrupted", "{job}\n{}", obid.log());
    assert!(
        job["reason"]
            .as_str()
            .unwrap()
            .starts_with("Not published: possible secret in .env.local (.env file)"),
        "{job}"
    );
    assert_eq!(remote_branch(&remote, &format!("oby/build-{evil}")), None);
    let link = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "GITLINK"}),
    );
    let job = ended(&obid, &link);
    assert_eq!(job["status"], "interrupted", "{job}\n{}", obid.log());
    let summary = job["reason"].as_str().unwrap();
    assert!(summary.contains("GitHub token in a name"), "{summary}");
    assert!(!summary.contains(&made_up_token()), "{summary}");
    assert_eq!(remote_branch(&remote, &format!("oby/build-{link}")), None);
}

/// A name that holds secrets by itself is found however it arrives: a `.env` moved into a new
/// folder, and an `id_ed25519` brought in through a merged side branch.
#[test]
fn a_moved_env_and_a_key_from_a_side_branch_are_found() {
    let folder = Folder::new("publish-moves");
    let bin = fake_gh(&folder.0);
    let remote = fixture(&folder.0.join("fixture"));
    let theirs = folder.0.join("theirs");
    git(
        &folder.0,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            theirs.to_str().unwrap(),
        ],
    );
    git(&theirs, &["checkout", "-q", "main"]);
    std::fs::write(theirs.join(".env"), "MODE=dev\n").unwrap();
    git(&theirs, &["add", "-A"]);
    git(
        &theirs,
        &[
            "-c",
            "user.name=Someone",
            "-c",
            "user.email=someone@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "An env",
        ],
    );
    git(&theirs, &["push", "-q", "origin", "main"]);
    let obid = start(&folder.0, &config(&folder.0, &remote, &agent()), Some(&bin));
    for (task, place) in [
        ("MOVE: .env newdir/.env", ".env (.env file)"),
        ("SIDE: id_ed25519", "id_ed25519 (private key file)"),
    ] {
        let id = dispatch(
            &obid,
            json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": task}),
        );
        let job = ended(&obid, &id);
        assert_eq!(
            job["status"],
            "interrupted",
            "{task}: {job}\n{}",
            obid.log()
        );
        assert!(
            job["reason"]
                .as_str()
                .unwrap()
                .starts_with(&format!("Not published: possible secret in {place}")),
            "{task}: {job}"
        );
        assert_eq!(remote_branch(&remote, &format!("oby/build-{id}")), None);
    }
}

/// The jobs reviewing job `id`'s published work, oldest first by `created`, which is in whole
/// seconds: reviews created in the same second come in any order (by their random ids), so tell
/// those apart by their agent.
fn reviews_of(obid: &Obid, id: &str) -> Vec<Value> {
    let jobs = call(obid, "GET", "/v1/jobs", None).body;
    let mut reviews: Vec<Value> = jobs
        .as_array()
        .unwrap()
        .iter()
        .filter(|j| j["reviewOf"] == id)
        .cloned()
        .collect();
    reviews.sort_by_key(|j| j["created"].as_str().unwrap_or_default().to_owned());
    reviews
}

/// Job `id` once Oby's review `round` of it has its verdict and nothing more runs by itself.
fn reviewed(obid: &Obid, id: &str, round: u64) -> Value {
    within(120, &format!("Oby's review {round} of {id}"), || {
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["review"]["round"] == round && job["status"] == "done" && job["reviewing"].is_null())
            .then_some(job)
    })
}

/// A job that publishes by itself gets one review of Oby's (#525): a `review` job of its own on
/// another account with room (`reviewOf` the author), reading the head obid pushed, in Plan mode,
/// committing and publishing nothing. Its approval marks the job reviewed (`Approved by Oby`,
/// then the owner's merge on GitHub), and nothing of it goes to GitHub.
#[test]
fn a_published_job_gets_one_review_on_another_account() {
    let folder = Folder::new("publish-review");
    let bin = fake_gh(&folder.0);
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agent());
    let accounts: Vec<Value> = [("stub", "Stub"), ("second", "Second")]
        .into_iter()
        .map(|(id, name)| json!({"id": id, "name": name, "configDir": sign_in(&folder, id)}))
        .collect();
    config["agents"]["claude"]["accounts"] = json!(accounts);
    let obid = start(&folder.0, &config, Some(&bin));
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "test -f hello.txt", "account": "stub"}),
    );
    let job = reviewed(&obid, &id, 1);
    assert_eq!(job["published"], PULL, "{job}\n{}", obid.log());
    let review = &job["review"];
    let pushed = job["outgoing"][0]["id"].as_str().unwrap();
    assert_eq!(review["verdict"], "approved", "{job}");
    assert_eq!(review["agent"], "claude");
    assert_eq!(review["account"], "Second");
    assert!(review.get("sameAccount").is_none(), "{job}");
    assert_eq!(review["commit"], pushed);
    assert_eq!(job["reviewError"], Value::Null);
    assert_eq!(job["reviewNeedsYou"], false);
    assert_eq!(job["round"], 1);
    assert_eq!(stage(&job, "review")["state"], "done");
    assert_eq!(stage(&job, "review")["detail"], "Approved by Oby");
    assert_eq!(stage(&job, "merged")["detail"], "Merge it on GitHub");
    // The review job: its own, on the other account, from the pushed head, in Plan mode.
    let reviews = reviews_of(&obid, &id);
    assert_eq!(reviews.len(), 1, "{reviews:?}");
    let reviewer = &reviews[0];
    assert_eq!(reviewer["id"], review["job"]);
    assert_eq!(reviewer["kind"], "review");
    assert_eq!(reviewer["status"], "done", "{reviewer}");
    assert_eq!(reviewer["account"], "second");
    assert_eq!(reviewer["mode"], "plan");
    assert_eq!(reviewer["startCommit"], pushed);
    assert_eq!(reviewer["commits"], 0);
    assert_eq!(reviewer["published"], Value::Null);
    // It loaded no project docs from the change, and read the base's rules (none here).
    let said = reviewer["summary"].as_str().unwrap();
    assert!(
        said.starts_with("Project docs off; base rules: none."),
        "{said}"
    );
    assert!(said.ends_with("VERDICT: APPROVE"), "{said}");
    // Nothing of the review went to GitHub: no review, comment or push of its own.
    let calls = calls_now(&folder.0);
    assert!(
        !calls.contains("pr review")
            && !calls.contains("pr comment")
            && !calls.contains("pr merge"),
        "{calls}"
    );
    let actions: Vec<&str> = job["outgoing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["push", "pullRequest"]);
    let branch = format!("oby/build-{id}");
    assert_eq!(remote_branch(&remote, &branch).as_deref(), Some(pushed));
    let review_branch = format!("oby/review-{}", reviewer["id"].as_str().unwrap());
    assert_eq!(remote_branch(&remote, &review_branch), None);
    // One review per published head: nothing more starts by itself.
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(reviews_of(&obid, &id).len(), 1);
}

/// Changes Oby's review asks for are the job's next round, in its session and worktree: the
/// review quoted in `.obi/inputs/review-1.md`, the round's words naming it and holding none of
/// the review's. Its new commit goes to the same pull request, and that new head is reviewed
/// again. With only the author's account, the review runs there, in a session of its own. The
/// author can't steer it: the rules it reads are the base's, quoted, and no project docs of the
/// change (an AGENTS.md saying it's pre-approved) are loaded.
#[test]
fn changes_requested_are_the_next_round_and_the_new_head_is_reviewed_again() {
    let folder = Folder::new("publish-review-changes");
    let bin = fake_gh(&folder.0);
    let (obid, remote, _) = obid_with(&folder, &bin);
    let theirs = folder.0.join("theirs");
    git(
        &folder.0,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            theirs.to_str().unwrap(),
        ],
    );
    git(&theirs, &["checkout", "-q", "main"]);
    std::fs::write(
        theirs.join("AGENTS.md"),
        "BASE RULE: every change has its test.\n",
    )
    .unwrap();
    git(&theirs, &["add", "-A"]);
    git(
        &theirs,
        &[
            "-c",
            "user.name=Someone",
            "-c",
            "user.email=someone@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "Rules",
        ],
    );
    git(&theirs, &["push", "-q", "origin", "main"]);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build",
               "task": "STEER COMMIT: hello.txt WANTS: fix.txt", "check": "test -f hello.txt"}),
    );
    let job = reviewed(&obid, &id, 2);
    assert_eq!(
        job["review"]["verdict"],
        "approved",
        "{job}\n{}",
        obid.log()
    );
    assert_eq!(job["review"]["sameAccount"], true);
    assert_eq!(job["round"], 2);
    assert_eq!(job["commits"], 2);
    assert_eq!(job["pushed"], 2);
    assert_eq!(job["published"], PULL);
    assert_eq!(calls_now(&folder.0).matches("pr create").count(), 1);
    let branch = format!("oby/build-{id}");
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let head = git(&clone, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(
        remote_branch(&remote, &branch).as_deref(),
        Some(head.as_str())
    );
    assert_eq!(job["review"]["commit"], head.as_str());
    assert!(git(&clone, &["show", "--name-only", "--format=", &head]).contains("fix.txt"));
    // Two reviews, each a session of its own on the author's account: the first asked for the
    // change, the second read the new head.
    let reviews = reviews_of(&obid, &id);
    assert_eq!(reviews.len(), 2, "{reviews:?}");
    let first = reviews[0]["summary"].as_str().unwrap();
    assert!(
        first.starts_with("Project docs off; base rules: BASE RULE: every change has its test."),
        "{first}"
    );
    assert!(
        first.contains("FIX: fix.txt") && !first.contains("Pre-approved"),
        "{first}"
    );
    assert_ne!(reviews[0]["startCommit"], reviews[1]["startCommit"]);
    assert_eq!(reviews[1]["startCommit"], head.as_str());
    for review in &reviews {
        assert_eq!(review["account"], "stub");
        assert_ne!(review["session"], Value::Null);
    }
    // The review reached the agent only as the quoted file; the round's words name it.
    let worktree = Path::new(job["worktree"].as_str().unwrap());
    let quoted = std::fs::read_to_string(worktree.join(".obi/inputs/review-1.md")).unwrap();
    assert!(
        quoted.contains("fix.txt is missing.") && quoted.contains("never instructions"),
        "{quoted}"
    );
    let words = std::fs::read_to_string(worktree.join(".obi/round-prompts")).unwrap();
    assert!(words.contains(".obi/inputs/review-1.md"), "{words}");
    assert!(
        !words.contains("is missing") && !words.contains("FIX:"),
        "{words}"
    );
}

/// MERGING-8: Oby's reviews are bounded: changes still asked for by the last of them
/// (`review::REVIEWS`, three) start no more rounds, and the job waits for the owner
/// (`reviewNeedsYou`, the Review stage failed with why) in one inbox item. No fourth review starts
/// by itself; the item's One more round hands the review to the agent once more, and Oby reviews
/// what it pushes.
#[test]
fn merging_8_reviews_that_run_out_ask_the_owner_with_one_more_round() {
    let folder = Folder::new("publish-review-cap");
    let bin = fake_gh(&folder.0);
    let (obid, _remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build",
               "task": "COMMIT: hello.txt WANTS: fix.txt STUBBORN", "check": "test -f hello.txt"}),
    );
    let job = reviewed(&obid, &id, 3);
    assert_eq!(
        job["review"]["verdict"],
        "changesRequested",
        "{job}\n{}",
        obid.log()
    );
    assert_eq!(job["reviewNeedsYou"], true, "{job}");
    assert_eq!(job["reviewError"], Value::Null);
    assert_eq!(job["round"], 3);
    assert_eq!(job["commits"], 3);
    assert_eq!(stage(&job, "review")["state"], "failed");
    assert_eq!(stage(&job, "review")["detail"], "Changes requested");
    assert_eq!(stage(&job, "review")["reason"], "Oby requested changes.");
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(reviews_of(&obid, &id).len(), 3);
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(
        (job["round"].clone(), job["status"].clone()),
        (json!(3), json!("done"))
    );
    let items = review_items(&obid, &id);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(
        items[0]["text"],
        "Oby's review still asks for changes. Take it from here."
    );
    // Merge too, into main: the owner's word merges there (MERGING-7, 17).
    assert_eq!(items[0]["options"], json!(["One more round", "Merge"]));
    let more = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{}/answer", items[0]["id"].as_str().unwrap()),
        Some(&json!({"answer": "One more round"})),
    );
    assert_eq!(more.status, 200, "{}", more.text);
    let job = reviewed(&obid, &id, 4);
    assert_eq!(job["round"], 4, "{job}");
    assert_eq!(job["review"]["round"], 4);
    // Still asking for changes: one new item, the old one done.
    let again = within(60, "a new item", || {
        let open = review_items(&obid, &id);
        (open.len() == 1 && open[0]["id"] != items[0]["id"]).then_some(open)
    });
    assert_eq!(again[0]["options"], json!(["One more round", "Merge"]));
}

/// MERGING-8, the path #1658 found: a follow-up works on the job's branch when Oby's first review
/// asks for changes, so no round starts. Once it ended, the owner gets obid's own item after that
/// one round, with One more round and Merge, and no question of the orchestrator's.
#[test]
fn merging_8_a_follow_up_on_the_branch_at_round_one_leaves_it_to_the_owner() {
    let folder = Folder::new("publish-review-follow-up");
    let bin = fake_gh(&folder.0);
    let (obid, _remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build",
               "task": "COMMIT: hello.txt WANTS: fix.txt HOLD REVIEW", "check": "test -f hello.txt"}),
    );
    let job_of = |id: &str| call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    let review = review_of(&obid, &id, 0);
    let review = within(60, "the review runs", || {
        let review = job_of(review["id"].as_str().unwrap());
        (review["status"] == "running").then_some(review)
    });
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": "Look"})),
    );
    assert_eq!(told.status, 201, "{}", told.text);
    let next = told.body["id"].as_str().unwrap().to_owned();
    assert_eq!(told.body["branch"], format!("oby/build-{id}"));
    let follow = ended(&obid, &next);
    assert!(!follow["status"].as_str().unwrap().is_empty());
    std::fs::write(
        Path::new(review["worktree"].as_str().unwrap()).join(".obi/release"),
        "",
    )
    .unwrap();
    let job = reviewed(&obid, &id, 1);
    assert_eq!(job["review"]["verdict"], "changesRequested", "{job}");
    assert_eq!(job["round"], 1, "no round started: {job}");
    assert_eq!(job["reviewNeedsYou"], true);
    let items = within(60, "the owner's item", || {
        let open = review_items(&obid, &id);
        (!open.is_empty()).then_some(open)
    });
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(
        items[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Oby's review still asks for changes. Take it from here."),
        "{}",
        items[0]
    );
    assert_eq!(items[0]["options"], json!(["One more round", "Merge"]));
    let questions = call(&obid, "GET", "/v1/inbox", None).body;
    assert!(
        !questions.as_array().unwrap().iter().any(|i| i["id"]
            .as_str()
            .unwrap_or_default()
            .starts_with("orchestrator-")),
        "{questions}"
    );
}

/// The review job of `id`'s published work at `nth` (oldest first), once there is one.
fn review_of(obid: &Obid, id: &str, nth: usize) -> Value {
    within(120, &format!("review {nth} of {id}"), || {
        reviews_of(obid, id).get(nth).cloned()
    })
}

/// A review that gives no verdict says so on the job (`reviewError`), and so does one whose verdict
/// a restart kept from landing: once obid starts again, a `reviewing` whose review ended takes its
/// end, and one whose review is gone gets a new review.
#[test]
fn a_review_with_no_verdict_or_cut_off_by_a_restart_settles() {
    let folder = Folder::new("publish-review-settle");
    let bin = fake_gh(&folder.0);
    let (mut obid, _remote, config) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt MUTE REVIEW",
               "check": "test -f hello.txt"}),
    );
    let settled = |obid: &Obid| {
        within(120, "Oby's review settles", || {
            let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
            (job["reviewing"].is_null() && job["reviewError"].is_string()).then_some(job)
        })
    };
    let job = settled(&obid);
    let none = "Oby's review didn't finish: it gave no verdict";
    assert_eq!(job["reviewError"], none, "{job}\n{}", obid.log());
    assert_eq!(job["review"], Value::Null);
    assert_eq!(job["reviewNeedsYou"], false);
    assert_eq!(
        stage(&job, "review")["detail"],
        format!("Waiting for review · {none}")
    );
    let review = reviews_of(&obid, &id)[0]["id"].as_str().unwrap().to_owned();
    // Stopped before its verdict landed, then started again: it lands.
    let stored = obid.home.join(format!("jobs/{id}.json"));
    let cut_off = |reviewing: &str| {
        let mut kept: Value = serde_json::from_slice(&std::fs::read(&stored).unwrap()).unwrap();
        kept["reviewing"] = json!(reviewing);
        kept["reviewError"] = Value::Null;
        std::fs::write(&stored, kept.to_string()).unwrap();
    };
    obid.stop();
    cut_off(&review);
    let mut obid = start(&folder.0, &config, Some(&bin));
    assert_eq!(settled(&obid)["reviewError"], none);
    assert_eq!(reviews_of(&obid, &id).len(), 1);
    // Its review gone: a new one.
    obid.stop();
    cut_off("deadbeef");
    let obid = start(&folder.0, &config, Some(&bin));
    let again = review_of(&obid, &id, 1);
    assert_ne!(again["id"], review.as_str());
    assert_eq!(settled(&obid)["reviewError"], none);
}

/// obid with the fixture repository, the publishing stand-in on Claude Code's account `stub`, and
/// a Codex account `codex` (an API key's login) on the stand-in Codex: Oby's review of Claude
/// Code's work goes to Codex first.
fn obid_with_codex(folder: &Folder, path_first: &Path) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agent());
    sign_in(folder, "stub");
    let codex = folder.0.join("home/accounts/codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(
        codex.join("auth.json"),
        json!({"OPENAI_API_KEY": "sk-stub"}).to_string(),
    )
    .unwrap();
    config["agents"]["codex"] = json!({
        "bin": Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-codex"),
        "accounts": [{"id": "codex", "name": "Codex", "configDir": codex}],
    });
    start(&folder.0, &config, Some(path_first))
}

/// The tools of Oby's approving reviews of job `job`'s head, oldest first.
fn approvals(job: &Value) -> Vec<&str> {
    job["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["commit"] == job["review"]["commit"])
        .filter_map(|r| r["agent"].as_str())
        .collect()
}

/// MERGING-15, with MERGING-14's second review: the owner's own change to AGENTS.md merges only
/// after two approving reviews of its head by different tools. Codex's approves first; Oby then
/// asks Claude Code, and nothing more starts once both are in.
#[test]
fn merging_15_a_change_to_agents_md_gets_a_second_review_by_another_tool() {
    let folder = Folder::new("publish-review-two");
    let bin = fake_gh(&folder.0);
    let obid = obid_with_codex(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: AGENTS.md",
               "check": "test -f AGENTS.md", "account": "stub"}),
    );
    let job = reviewed(&obid, &id, 2);
    assert_eq!(
        job["review"]["verdict"],
        "approved",
        "{job}\n{}",
        obid.log()
    );
    assert_eq!(job["review"]["twoReviews"], "it changes AGENTS.md");
    assert_eq!(approvals(&job), ["codex", "claude"], "{job}");
    assert_eq!(job["noReviewer"], Value::Null);
    assert_eq!(stage(&job, "review")["detail"], "Approved by Oby");
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(reviews_of(&obid, &id).len(), 2);
}

/// MERGING-14: with no other tool signed in with room, the second review waits, saying why
/// (`noReviewer`, under the Review stage), and only the one approval is on the job: its merge is
/// the owner's "Ready to merge?" (crate::merge's `merging_14_…`).
#[test]
fn merging_14_with_no_other_tool_the_second_review_waits_saying_why() {
    let folder = Folder::new("publish-review-alone");
    let bin = fake_gh(&folder.0);
    let (mut obid, _remote, mut config) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: AGENTS.md",
               "check": "test -f AGENTS.md"}),
    );
    let job = within(120, "the second review waits", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        job["noReviewer"].is_string().then_some(job)
    });
    let why = "Oby's second review waits for a tool other than Claude Code signed in with room.";
    assert_eq!(job["noReviewer"], why, "{job}\n{}", obid.log());
    assert_eq!(job["review"]["verdict"], "approved");
    assert_eq!(approvals(&job), ["claude"]);
    assert_eq!(
        stage(&job, "review")["detail"],
        format!("Waiting for review · {why}")
    );
    assert_eq!(reviews_of(&obid, &id).len(), 1);
    // Cut off between the first verdict and the second review's start (no `noReviewer` kept),
    // and started again with Codex signed in: the second review starts by itself.
    obid.stop();
    let stored = obid.home.join(format!("jobs/{id}.json"));
    let mut kept: Value = serde_json::from_slice(&std::fs::read(&stored).unwrap()).unwrap();
    kept.as_object_mut().unwrap().remove("noReviewer");
    kept.as_object_mut().unwrap().remove("no_reviewer");
    std::fs::write(&stored, kept.to_string()).unwrap();
    let codex = folder.0.join("home/accounts/codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(
        codex.join("auth.json"),
        json!({"OPENAI_API_KEY": "sk-stub"}).to_string(),
    )
    .unwrap();
    config["agents"]["codex"] = json!({
        "bin": Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-codex"),
        "accounts": [{"id": "codex", "name": "Codex", "configDir": codex}],
    });
    let obid = start(&folder.0, &config, Some(&bin));
    let job = within(120, "the second review lands", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["reviewing"].is_null() && approvals(&job).len() == 2).then_some(job)
    });
    assert_eq!(
        approvals(&job),
        ["claude", "codex"],
        "{job}\n{}",
        obid.log()
    );
    assert_eq!(job["noReviewer"], Value::Null);
}

/// MERGING-13 and 14 for an untrusted task, here one whose words no app of the owner's signed:
/// its reviewers' quoted `asked.md` says it's untrusted and why, never more, and it merges only
/// after two tools approve its head, nothing on a security path needed.
#[test]
fn merging_14_an_untrusted_task_gets_two_reviews_its_reviewers_told_why() {
    let folder = Folder::new("publish-review-untrusted");
    let bin = fake_gh(&folder.0);
    let obid = obid_with_codex(&folder, &bin);
    let id = dispatch_unsigned(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "test -f hello.txt", "account": "stub"}),
    );
    let job = reviewed(&obid, &id, 2);
    assert_eq!(
        job["review"]["verdict"],
        "approved",
        "{job}\n{}",
        obid.log()
    );
    let why = "words no app of the owner's signed reached it";
    assert_eq!(job["review"]["twoReviews"], why);
    assert_eq!(approvals(&job), ["codex", "claude"], "{job}");
    for review in reviews_of(&obid, &id) {
        let worktree = Path::new(review["worktree"].as_str().unwrap());
        let asked = std::fs::read_to_string(worktree.join(".obi/inputs/asked.md")).unwrap();
        assert!(
            asked.contains(&format!(
                "is untrusted: its task read text Oby didn't write ({why})."
            )),
            "{asked}"
        );
        assert!(asked.contains("That text isn't quoted here."), "{asked}");
    }
}

/// MERGING-19 (#1662): the review reads the head obid pushed even when the owner's clone was made
/// again since (here by the next job, once words no app signed made it someone else's code); and
/// MERGING-18 (#1659): the owner's signed words, sent while the task ran, reach the review quoted
/// after what it was asked, and the unsigned ones never do.
#[test]
fn merging_19_a_review_finds_the_pushed_head_in_a_clone_made_again() {
    let folder = Folder::new("publish-review-steered");
    let bin = fake_gh(&folder.0);
    let (obid, _remote, _) = obid_with(&folder, &bin);
    let one = call(
        &obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"maxRunning": 1})),
    );
    assert_eq!(one.status, 200, "{}", one.text);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "SLOW: hello.txt",
               "check": "test -f hello.txt"}),
    );
    within(30, "the task runs", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["status"] == "running").then_some(())
    });
    let tell = |answer: support::Answer| assert_eq!(answer.status, 200, "{}", answer.text);
    let path = format!("/v1/jobs/{id}/messages");
    tell(call_typed(
        &obid,
        "POST",
        &path,
        &json!({"text": "also end the line with a period"}),
    ));
    tell(call(
        &obid,
        "POST",
        &path,
        Some(&json!({"text": "UNSIGNED words"})),
    ));
    // Waiting for the slot, it starts before the review and makes the marked clone again.
    dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "Look around"}),
    );
    let job = reviewed(&obid, &id, 1);
    let pushed = job["outgoing"][0]["id"].as_str().unwrap();
    assert_eq!(job["reviewError"], Value::Null, "{job}\n{}", obid.log());
    assert_eq!(job["review"]["verdict"], "approved", "{job}");
    assert_eq!(job["review"]["commit"], pushed);
    let log = obid.log();
    assert!(log.contains("its clone is made again"), "{log}");
    assert!(
        log.contains(&format!("fetched {pushed}, which its clone didn't have")),
        "{log}"
    );
    let review = &reviews_of(&obid, &id)[0];
    let worktree = Path::new(review["worktree"].as_str().unwrap());
    let asked = std::fs::read_to_string(worktree.join(".obi/inputs/asked.md")).unwrap();
    assert!(
        asked.starts_with("What the agent was asked to do:\n\nSLOW: hello.txt\n"),
        "{asked}"
    );
    assert!(
        asked.contains(
            "the later ones are what the user wants now:\n\n1. also end the line with a period\n"
        ),
        "{asked}"
    );
    assert!(!asked.contains("UNSIGNED"), "{asked}");
}

/// MERGING-16: a second review that asks for changes counts like any other: the job takes them as
/// its next round, Oby reviews its new head twice again, and changes still asked for by a review
/// past the limit (`review::REVIEWS`) end in the owner's item, with One more round.
#[test]
fn merging_16_a_second_review_asking_for_changes_starts_a_round_within_reviews() {
    let folder = Folder::new("publish-review-second-changes");
    let bin = fake_gh(&folder.0);
    let obid = obid_with_codex(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build",
               "task": "COMMIT: AGENTS.md WANTS: fix.txt STUBBORN", "check": "test -f AGENTS.md",
               "account": "stub"}),
    );
    let job = reviewed(&obid, &id, 4);
    assert_eq!(
        job["review"]["verdict"],
        "changesRequested",
        "{job}\n{}",
        obid.log()
    );
    assert_eq!(job["review"]["agent"], "claude");
    // Review 2 (Claude Code's, after Codex's approval) started round 2; review 4 ran out.
    assert_eq!(job["round"], 2, "{job}");
    assert_eq!(job["commits"], 2);
    assert_eq!(approvals(&job), ["codex"]);
    assert_eq!(job["reviewNeedsYou"], true);
    let reviews = reviews_of(&obid, &id);
    let by = |agent: &str| reviews.iter().filter(|r| r["agent"] == agent).count();
    assert_eq!((by("codex"), by("claude")), (2, 2), "{reviews:?}");
    let items = within(60, "the owner's item", || {
        let open = review_items(&obid, &id);
        (!open.is_empty()).then_some(open)
    });
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(
        items[0]["text"],
        "Oby's review still asks for changes. Take it from here."
    );
    assert_eq!(items[0]["options"], json!(["One more round", "Merge"]));
}

/// Job `id`'s open inbox items about Oby's review that couldn't run.
fn review_items(obid: &Obid, id: &str) -> Vec<Value> {
    call(obid, "GET", "/v1/inbox", None)
        .body
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["job"] == id && i["kind"] == "failed" && i["done"].is_null())
        .cloned()
        .collect()
}

/// A review that ends before its verdict (here Codex's login refused, as codex-cli 0.162.1 says it:
/// `workspace routing discovery unauthorized (401)`) goes to the next reviewer that may take it, the
/// other tool, by itself: its verdict lands, nothing waits for the owner, and the failed review job
/// asks no one either.
#[test]
fn a_failed_reviewer_falls_back_to_the_other_tool() {
    let folder = Folder::new("publish-review-fallback");
    let bin = fake_gh(&folder.0);
    let obid = obid_with_codex(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt CODEX REFUSED",
               "check": "test -f hello.txt", "account": "stub"}),
    );
    let job = reviewed(&obid, &id, 1);
    assert_eq!(
        job["review"]["verdict"],
        "approved",
        "{job}\n{}",
        obid.log()
    );
    assert_eq!(job["review"]["agent"], "claude");
    assert_eq!(job["reviewError"], Value::Null);
    assert_eq!(job["reviewNeedsYou"], false);
    // Codex's first: Claude Code's verdict landed, so Codex's review, which ended without one, came
    // before it. Told apart by agent: Codex fails fast, and both can be created in one second.
    let reviews = reviews_of(&obid, &id);
    assert_eq!(reviews.len(), 2, "{reviews:?}");
    let by = |agent: &str| {
        reviews
            .iter()
            .find(|r| r["agent"] == agent)
            .unwrap_or_else(|| panic!("no {agent} review: {reviews:?}"))
    };
    let (codex, claude) = (by("codex"), by("claude"));
    assert_eq!(codex["account"], "codex");
    assert!(
        matches!(codex["status"].as_str(), Some("failed" | "interrupted")),
        "{codex}"
    );
    assert!(
        codex["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("workspace routing discovery unauthorized (401)"),
        "{codex}"
    );
    assert!(
        codex["dismissed"].is_string(),
        "its end is its author's: {codex}"
    );
    assert_eq!(claude["id"], job["review"]["job"]);
    assert!(review_items(&obid, &id).is_empty());
}

/// When no reviewer can give a verdict (Codex's login refused, then Claude Code's turn erring), the
/// job waits for the owner in one item, `Oby couldn't review #7: …`, with Try again (no workflow
/// merges without a review): `reviewNeedsYou`, and nothing more starts by itself. Try again runs
/// the review again, with one more reviewer to fall back on, and a second give-up is one item again.
#[test]
fn every_reviewer_failing_waits_for_the_owner_in_one_item() {
    let folder = Folder::new("publish-review-none");
    let bin = fake_gh(&folder.0);
    let obid = obid_with_codex(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build",
               "task": "COMMIT: hello.txt CODEX REFUSED BROKEN REVIEW",
               "check": "test -f hello.txt", "account": "stub"}),
    );
    // Settled once its item is in the inbox too: the job says it needs the owner a moment (two
    // writes to disk) before its item is added (`Jobs::reviewed`).
    let waits = |reviews: usize| {
        within(120, "the review waits for the owner", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
            let items = review_items(&obid, &id);
            (job["reviewNeedsYou"] == true
                && reviews_of(&obid, &id).len() == reviews
                && !items.is_empty())
            .then_some((job, items))
        })
    };
    let (job, items) = waits(2);
    let error = job["reviewError"].as_str().unwrap_or_default();
    assert!(
        error.starts_with("Oby couldn't review #7: ") && error.contains("Invalid API key"),
        "{job}\n{}",
        obid.log()
    );
    assert_eq!(job["status"], "done");
    assert_eq!(job["review"], Value::Null);
    assert_eq!(
        stage(&job, "review")["detail"],
        format!("Waiting for review · {error}")
    );
    let reviews = reviews_of(&obid, &id);
    let mut agents: Vec<&str> = reviews.iter().filter_map(|r| r["agent"].as_str()).collect();
    agents.sort_unstable();
    assert_eq!(agents, ["claude", "codex"], "{reviews:?}");
    assert_eq!(items.len(), 1, "{items:?}");
    // Codex's first, then Claude Code's: the item names the review that gave up last, and its
    // error is Claude Code's (above). Not by `created`: both can be created in one second.
    let claude = reviews.iter().find(|r| r["agent"] == "claude").unwrap();
    assert_eq!(
        items[0]["id"],
        format!("{id}-review-{}", claude["id"].as_str().unwrap())
    );
    assert_eq!(items[0]["text"], error);
    assert_eq!(items[0]["options"], json!(["Try again"]));
    // Never a loop: nothing more starts by itself.
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(reviews_of(&obid, &id).len(), 2);
    assert_eq!(review_items(&obid, &id).len(), 1);
    // Try again: the review again, then the owner's again, in one new item. Codex's 401 ended its
    // login (#1423), so it isn't tried again until the owner signs in: Claude Code alone takes it.
    let tried = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{}/answer", items[0]["id"].as_str().unwrap()),
        Some(&json!({"answer": "Try again"})),
    );
    assert_eq!(tried.status, 200, "{}", tried.text);
    let (job, again) = waits(3);
    assert_eq!(job["reviewNeedsYou"], true);
    // Created at least two seconds after the others: last by `created`.
    assert_eq!(reviews_of(&obid, &id)[2]["agent"], "claude");
    assert_eq!(again.len(), 1, "{again:?}");
    assert_ne!(again[0]["id"], items[0]["id"]);
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(reviews_of(&obid, &id).len(), 3);
}

/// A round Oby's review asked for that pushes nothing new leaves the changes on the pull request's
/// head: the job waits for the owner (`reviewNeedsYou`), and no second review starts.
#[test]
fn a_round_that_pushes_nothing_new_waits_for_the_owner() {
    let folder = Folder::new("publish-review-idle");
    let bin = fake_gh(&folder.0);
    let (obid, _remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build",
               "task": "COMMIT: hello.txt WANTS: fix.txt IDLE ROUND", "check": "test -f hello.txt"}),
    );
    let job = within(120, "the job waits for the owner", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["reviewNeedsYou"] == true).then_some(job)
    });
    assert_eq!(
        (job["round"].clone(), job["status"].clone()),
        (json!(2), json!("done")),
        "{job}"
    );
    assert_eq!(job["review"]["round"], 1);
    assert_eq!(job["review"]["verdict"], "changesRequested");
    assert_eq!(
        (job["commits"].clone(), job["pushed"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(stage(&job, "review")["state"], "failed");
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(reviews_of(&obid, &id).len(), 1);
}

/// A review reads the very commit obid pushed. A head pushed while it ran makes it count for
/// nothing, however it ended (here with no verdict): not one of the job's reviews, no error, and
/// the new head gets a review of its own, one, at that commit.
#[test]
fn a_head_pushed_while_its_review_ran_gets_its_own() {
    let folder = Folder::new("publish-review-moved");
    let bin = fake_gh(&folder.0);
    let (obid, _remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt HOLD REVIEW",
               "check": "test -f hello.txt"}),
    );
    let head = |job: &Value| {
        job["outgoing"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|o| o["action"] == "push")
            .map(|o| o["id"].as_str().unwrap().to_owned())
            .unwrap()
    };
    // Held, once its worktree is there.
    let job_of = |id: &str| call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    let first = review_of(&obid, &id, 0);
    let first = within(60, "the first review runs", || {
        let review = job_of(first["id"].as_str().unwrap());
        (review["status"] == "running").then_some(review)
    });
    let one = head(&job_of(&id));
    assert_eq!(first["startCommit"], one.as_str());
    // Tried again while its review is held: a second commit goes out.
    again_commits(&obid, &id, "second.txt");
    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(told.status, 200, "{}", told.text);
    let pushed = within(120, "the second head goes out", || {
        let job = job_of(&id);
        (job["status"] == "done" && job["pushed"] == 2).then_some(job)
    });
    let two = head(&pushed);
    assert_ne!(one, two);
    let release = |review: &Value| {
        let worktree = Path::new(review["worktree"].as_str().unwrap()).join(".obi");
        std::fs::write(worktree.join("release"), "").unwrap();
    };
    std::fs::write(
        Path::new(first["worktree"].as_str().unwrap()).join(".obi/mute"),
        "",
    )
    .unwrap();
    release(&first);
    let second = review_of(&obid, &id, 1);
    assert_eq!(second["startCommit"], two.as_str());
    let second = within(60, "the second review runs", || {
        let review = job_of(second["id"].as_str().unwrap());
        (review["status"] == "running").then_some(review)
    });
    release(&second);
    let job = reviewed(&obid, &id, 1);
    assert_eq!(job["review"]["job"], second["id"], "{job}");
    assert_eq!(job["review"]["commit"], two.as_str());
    assert_eq!(job["reviewError"], Value::Null);
    assert_eq!(reviews_of(&obid, &id).len(), 2);
}

/// A review stopped at its account's usage limit waits for the owner's one decision (#1563), never
/// a reset retry of its own; if the owner lets it go (Dismiss), the job it reviewed says so, never
/// `Oby is reviewing` for good.
#[test]
fn a_dismissed_review_says_so_on_its_job() {
    let folder = Folder::new("publish-review-dismissed");
    let bin = fake_gh(&folder.0);
    let (obid, _remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt LIMITED REVIEW",
               "check": "test -f hello.txt"}),
    );
    let rid = review_of(&obid, &id, 0)["id"].as_str().unwrap().to_owned();
    within(120, "the review waits for the owner's decision", || {
        let decisions = call(&obid, "GET", "/v1/limit-decisions", None).body;
        decisions
            .as_array()?
            .iter()
            .any(|d| {
                d["jobs"]
                    .as_array()
                    .is_some_and(|jobs| jobs.iter().any(|j| j["id"] == rid.as_str()))
            })
            .then_some(())
    });
    let waits = call(&obid, "GET", &format!("/v1/jobs/{rid}"), None).body;
    assert_eq!(waits["status"], "interrupted", "{waits}");
    assert_eq!(waits["resumesAt"], Value::Null, "{waits}");
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["reviewing"], rid.as_str(), "{job}");
    assert_eq!(stage(&job, "review")["detail"], "Oby is reviewing");
    let dismissed = call(&obid, "POST", &format!("/v1/jobs/{rid}/dismiss"), None);
    assert_eq!(dismissed.status, 200, "{}", dismissed.text);
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["reviewing"], Value::Null, "{job}");
    assert!(
        job["reviewError"]
            .as_str()
            .unwrap()
            .starts_with("Oby's review didn't finish: "),
        "{job}"
    );
}

/// With no account that fits (the author's own nearly full, no other), Oby's review never runs over
/// the limit: the job says why it waits (`noReviewer`, the Review stage), with no review job and no
/// error.
#[test]
fn with_no_account_with_room_the_review_waits() {
    let folder = Folder::new("publish-review-no-room");
    let bin = fake_gh(&folder.0);
    let (obid, _remote, _) = obid_with(&folder, &bin);
    let id = dispatch(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "check": "test -f hello.txt",
               "task": "COMMIT: hello.txt USAGE: {\"status\":\"allowed\",\"unifiedWindows\":{\"five_hour\":{\"utilization\":0.97,\"resetsAt\":4102444800}}}"}),
    );
    let job = within(120, "the review waits for an account", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        job["noReviewer"].is_string().then_some(job)
    });
    let why = job["noReviewer"].as_str().unwrap();
    assert!(
        why.starts_with("Oby's review waits for an account with room: Stub is at 97%"),
        "{why}"
    );
    assert_eq!(
        stage(&job, "review")["detail"],
        format!("Waiting for review · {why}")
    );
    assert_eq!(job["reviewing"], Value::Null);
    assert_eq!(job["reviewError"], Value::Null);
    assert!(reviews_of(&obid, &id).is_empty());
}

/// MERGING-1, MERGING-2: a passing check keeps the clean head it ran on, and why it can't stand for
/// the Mac's checks when it can't (a Swift file only the Mac tests, words no app of the owner's
/// signed); a failing one clears it.
#[test]
fn merging_1_a_passing_check_keeps_its_head_and_a_failing_one_clears_it() {
    let folder = Folder::new("publish-checked");
    let bin = fake_gh(&folder.0);
    let (obid, _remote, config) = obid_with(&folder, &bin);
    let home = Path::new(config["home"].as_str().unwrap()).to_path_buf();
    let checked = |id: &str| {
        let file = std::fs::read(home.join(format!("jobs/{id}.json"))).unwrap();
        serde_json::from_slice::<Value>(&file).unwrap()["checked"].clone()
    };
    // The owner's own words, signed by their app, as Oby's apps send them.
    let owners = |job: Value| {
        let answer = call_typed(&obid, "POST", "/v1/jobs", &json!({"job": job}));
        assert_eq!(answer.status, 201, "{}", answer.text);
        answer.body["id"].as_str().unwrap().to_owned()
    };
    let pushed = |job: &Value| {
        job["outgoing"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|o| o["action"] == "push")
            .map(|o| o["id"].clone())
            .unwrap()
    };
    let plain = owners(json!({"repo": "obi-e2e/fixture-app", "kind": "build",
                              "task": "COMMIT: hello.txt", "check": "test -f hello.txt"}));
    let job = ended(&obid, &plain);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(
        checked(&plain),
        json!({"commit": pushed(&job)}),
        "{}",
        obid.log()
    );
    let swift = owners(json!({"repo": "obi-e2e/fixture-app", "kind": "build",
                              "task": "COMMIT: Thing.swift", "check": "test -f Thing.swift"}));
    let job = ended(&obid, &swift);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(
        checked(&swift),
        json!({"commit": pushed(&job), "apart": obid::merge::MAC_LEFT})
    );
    // Words no app of the owner's signed: its check can't vouch for the merge.
    let unsigned = dispatch_unsigned(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build",
               "task": "COMMIT: other.txt", "check": "test -f other.txt"}),
    );
    let job = ended(&obid, &unsigned);
    assert_eq!(
        checked(&unsigned),
        json!({"commit": pushed(&job), "apart": "Its check passed, but it didn't start from the owner's own words in Oby's apps, so it's yours to merge."})
    );
    // It passes, publishes, and Oby's review asks for a change its round commits (again-1.txt),
    // which the check then fails on.
    let cleared = owners(json!({"repo": "obi-e2e/fixture-app", "kind": "build",
                                "task": "COMMIT: second.txt WANTS: fix.txt STUBBORN", "check": "test ! -f again-1.txt"}));
    within(60, "its first check to pass", || {
        checked(&cleared).is_object().then_some(())
    });
    within(180, "its check to fail", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{cleared}"), None).body;
        (job["round"].as_u64() >= Some(2) && checked(&cleared).is_null()).then_some(())
    });
}
