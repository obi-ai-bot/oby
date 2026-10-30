//! Codex's app-server adapter and the Agent Client Protocol's (Cursor), each driving a scripted
//! stand-in for the tool (tests/agents/fake-codex, tests/agents/fake-cursor) through the real obid
//! binary, asked over HTTP, in the job sandbox: a job that commits, obid's answers to what the
//! agent asks (and the fixed denials, in every mode), the session picked up again in the next
//! round, the model and the mode, a tool's own requests obid refuses, a signed-out account, a turn
//! that fails, and Stop. No credentials: the stand-ins sign in from a file the test writes.
//!
//! What the Swift runner's ACPTests and CursorTests checked of its own types is covered here
//! through the API, and in src/acp.rs and src/codex.rs for the wire format (tests/PORTING.md).

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use support::*;

fn agents() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents")
}

/// obid with the fixture repository, Codex and Cursor (the stand-ins), and no Claude Code: each
/// account in a folder of its own. Cursor's account is signed in unless `signed_out`.
fn obid(folder: &Folder, signed_out: bool) -> Obid {
    obid_and_remote(folder, signed_out).0
}

/// [`obid`], and the fixture's bare remote.
fn obid_and_remote(folder: &Folder, signed_out: bool) -> (Obid, PathBuf) {
    obid_with(folder, signed_out, json!({}))
}

/// [`obid_and_remote`], with `more` in its configuration.
fn obid_with(folder: &Folder, signed_out: bool, more: Value) -> (Obid, PathBuf) {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-claude"));
    for (key, value) in more.as_object().unwrap() {
        config[key] = value.clone();
    }
    let accounts = folder.0.join("home/accounts");
    config["agents"] = json!({
        "codex": {"bin": agents().join("fake-codex"),
                  "accounts": [{"id": "work", "name": "Work", "configDir": accounts.join("codex-work")}]},
        "cursor": {"bin": agents().join("fake-cursor"),
                   "accounts": [{"id": "cursor", "name": "Personal", "configDir": accounts.join("cursor")}]}
    });
    let cursor = accounts.join("cursor");
    // Where Cursor's file store keeps its login.
    let login = cursor.join(if cfg!(target_os = "macos") {
        ".cursor/auth.json"
    } else {
        ".config/cursor/auth.json"
    });
    std::fs::create_dir_all(login.parent().unwrap()).unwrap();
    std::fs::create_dir_all(cursor.join(".config")).unwrap();
    if !signed_out {
        std::fs::write(&login, "{}").unwrap();
    }
    // Cursor's own settings: obid keeps its keys and empties what it would let through.
    std::fs::write(
        cursor.join("cli-config.json"),
        r#"{"authInfo": {"email": "owner@example.com"}, "version": 1, "permissions": {"allow": ["Shell(*)"], "deny": []}}"#,
    )
    .unwrap();
    (start(&folder.0, &config, None), remote)
}

fn dispatch(obid: &Obid, job: Value) -> String {
    let mut whole = json!({"repo": "obi-e2e/fixture-app", "kind": "research", "check": "true"});
    for (key, value) in job.as_object().unwrap() {
        whole[key] = value.clone();
    }
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": whole })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

fn ended(obid: &Obid, id: &str) -> Value {
    let answer = call(obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

fn summary(job: &Value) -> &str {
    job["summary"].as_str().unwrap_or("")
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
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Whether the agent of job `id`, which writes `.obi/beat` every 50 ms, has stopped.
fn stopped_beating(obid: &Obid, id: &str) -> bool {
    let path = obid.home.join("worktrees").join(id).join(".obi/beat");
    let first = std::fs::read_to_string(&path).ok();
    std::thread::sleep(Duration::from_millis(400));
    std::fs::read_to_string(&path).ok() == first
}

fn beats(obid: &Obid, id: &str) {
    let path = obid.home.join("worktrees").join(id).join(".obi/beat");
    within(30, "the agent beats", || path.exists().then_some(()));
}

/// A Codex job: the thread starts in obid's sandbox with every approval coming to obid, its file
/// change and its commit are approved, its commit counts, and its words are the summary. Its
/// CODEX_HOME is a folder of the job's own (#655), never the account's: a box never writes it.
#[test]
fn codex_commits_with_obids_approval() {
    let folder = Folder::new("codex-commits");
    let obid = obid(&folder, false);
    let id = dispatch(
        &obid,
        json!({"agent": "codex", "kind": "build", "task": "COMMIT: hello.txt", "check": "test -f hello.txt", "model": "gpt-6-sol"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["agent"], "codex");
    assert_eq!(job["account"], "work");
    assert_eq!(job["commits"], 1);
    assert_eq!(summary(&job), "Added hello.txt.");
    let thread = job["session"].as_str().unwrap();
    let kept: Value = serde_json::from_str(
        &std::fs::read_to_string(
            obid.home
                .join("logins")
                .join(&id)
                .join(format!("threads/{thread}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(kept["sandbox"], "danger-full-access");
    assert_eq!(kept["approvalPolicy"], "untrusted");
    assert_eq!(kept["model"], "gpt-6-sol");
    let worktree = obid.home.join("worktrees").join(&id);
    assert_eq!(
        git(&worktree, &["log", "-1", "--format=%s"]),
        "Add hello.txt"
    );

    let id = dispatch(&obid, json!({"agent": "codex", "task": "ENV"}));
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let home = obid.home.join("logins").join(&id);
    assert!(
        summary(&job).contains(&format!("CODEX_HOME={}", home.display())),
        "{job}"
    );
    // The account's folder got nothing of the jobs'.
    let account = folder.0.join("home/accounts/codex-work");
    assert!(
        !account.join("threads").exists(),
        "a job wrote the account's folder"
    );

    let status = call(&obid, "GET", "/v1/status", None).body;
    let accounts: Vec<(&str, &str)> = status["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["id"].as_str().unwrap(), a["agent"].as_str().unwrap()))
        .collect();
    assert_eq!(accounts, [("work", "codex"), ("cursor", "cursor")]);
}

/// A Codex job's transcript (#381) has the same entries a Claude Code job's has: each item's start
/// and end as one entry, its reads as reads, its file changes with their lines, a failed command
/// with its exit code, and its plan as a to-do list. No command's output is in it.
#[test]
fn a_codex_jobs_transcript_matches_claudes() {
    let folder = Folder::new("codex-transcript");
    let obid = obid(&folder, false);
    let id = dispatch(
        &obid,
        json!({"agent": "codex", "task": "TOOLS", "check": "node --test"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    let answer = call(&obid, "GET", &format!("/v1/jobs/{id}/transcript"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert_eq!(answer.body["next"], Value::Null, "{}", answer.text);
    let entries = answer.body["entries"].as_array().unwrap().clone();
    let shape: Vec<(&str, &str, &str)> = entries
        .iter()
        .map(|e| {
            (
                e["kind"].as_str().unwrap(),
                e["tool"].as_str().unwrap_or(""),
                e["state"].as_str().unwrap_or(""),
            )
        })
        .collect();
    let read = ("tool", "Read", "done");
    assert_eq!(
        shape,
        [
            ("start", "", ""),
            // The job's request, as the first words from you.
            ("you", "", ""),
            ("said", "", ""),
            read,
            read,
            read,
            read,
            read,
            ("plan", "", ""),
            ("said", "", ""),
            ("tool", "Edit", "done"),
            ("tool", "Edit", "done"),
            ("tool", "Bash", "failed"),
        ],
        "{}",
        answer.text
    );
    assert_eq!(entries[0]["summary"], "Started on Codex");
    assert_eq!(entries[3]["summary"], "Reading scan.rs");
    assert_eq!(entries[8]["summary"], "1 of 2 done");
    assert_eq!(
        entries[8]["todos"][1],
        json!({"text": "Fix it", "state": "running"})
    );
    assert_eq!(
        (&entries[10]["added"], &entries[10]["removed"]),
        (&json!(3), &json!(2))
    );
    assert_eq!(entries[12]["summary"], "Running node --test");
    assert_eq!(entries[12]["exit"], 101);
    assert!(!answer.text.contains("raw output"), "{}", answer.text);
    // The same kinds in its activity (#730).
    assert_eq!(
        job["activity"]["counts"],
        json!({"read": 5, "search": 0, "edit": 2, "command": 0, "test": 1, "tetrix": 0, "web": 0, "mcp": 0}),
        "{job}"
    );
    assert_eq!(job["activity"]["groups"][1]["title"], "Found it");
    // Opened (#784): a command's output and exit code, a file change's diff.
    let bash = call(&obid, "GET", &format!("/v1/jobs/{id}/transcript/13"), None).body;
    assert_eq!(bash["detail"]["input"]["command"], "node --test", "{bash}");
    assert_eq!(
        bash["detail"]["output"],
        json!({"text": "raw output that never leaves obid", "lines": 1, "truncated": false, "exit": 101})
    );
    let edit = call(&obid, "GET", &format!("/v1/jobs/{id}/transcript/11"), None).body;
    assert_eq!(
        edit["detail"]["diff"],
        json!([{"path": "src/scan.rs",
                "patch": "--- a/src/scan.rs\n+++ b/src/scan.rs\n@@ -1,2 +1,3 @@\n-a\n-b\n+c\n+d\n+e\n"}]),
        "{edit}"
    );
    assert_eq!(job["context"], "Found it. Fixing it.");
}

/// obid's fixed denials hold for Codex: a push is declined, and since the decision carries no
/// words, the turn hears why; a change outside the worktree is declined and nothing is written.
/// A request obid doesn't answer is refused, so Codex never waits on it; a command that's fine is
/// accepted.
#[test]
fn codex_hears_what_obid_declined_and_why() {
    let folder = Folder::new("codex-declines");
    let obid = obid(&folder, false);
    let push = dispatch(
        &obid,
        json!({"agent": "codex", "task": "ASK: /bin/zsh -lc 'git push origin HEAD'"}),
    );
    let outside = dispatch(&obid, json!({"agent": "codex", "task": "OUTSIDE"}));
    let unknown = dispatch(&obid, json!({"agent": "codex", "task": "UNKNOWN"}));
    let fine = dispatch(&obid, json!({"agent": "codex", "task": "ASK: node --test"}));

    let job = ended(&obid, &push);
    assert_eq!(job["status"], "done", "{job}");
    assert!(
        summary(&job).starts_with(
            "answer: decline\nheard: obid declined that: obid publishes the job's work itself"
        ),
        "{job}"
    );
    let job = ended(&obid, &outside);
    assert!(
        summary(&job).starts_with(
            "answer: decline\nheard: obid declined that: Stay inside this job's worktree"
        ),
        "{job}"
    );
    assert!(!obid.home.join("worktrees/outside.txt").exists());
    let job = ended(&obid, &unknown);
    assert!(summary(&job).contains("-32601"), "{job}");
    let job = ended(&obid, &fine);
    assert_eq!(summary(&job), "answer: accept", "{job}");
}

/// Plan mode: Codex reads and plans, and obid declines its file changes.
#[test]
fn codex_in_plan_mode_changes_no_file() {
    let folder = Folder::new("codex-plan");
    let obid = obid(&folder, false);
    let id = dispatch(
        &obid,
        json!({"agent": "codex", "kind": "build", "task": "COMMIT: plan.txt", "mode": "plan"}),
    );
    let job = ended(&obid, &id);
    // Nothing committed: done, and the agent's summary says why.
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["commits"], 0);
    assert_eq!(
        job["reason"], "Nothing committed, so nothing was published.",
        "{job}"
    );
    assert!(summary(&job).contains("edit: decline"), "{job}");
    assert!(
        !obid
            .home
            .join("worktrees")
            .join(&id)
            .join("plan.txt")
            .exists()
    );
}

/// Review jobs only read (#630): Plan mode whatever they ask for or the default says, so Codex's
/// file change is declined, and a mode change keeps them in Plan. Investigations run as other
/// work does (the owner's decision of 2026-10-18).
#[test]
fn codex_reviews_and_investigates_in_plan_mode() {
    let folder = Folder::new("codex-read-only");
    let obid = obid(&folder, false);
    {
        let kind = "review";
        let id = dispatch(
            &obid,
            json!({"agent": "codex", "kind": kind, "task": "COMMIT: plan.txt", "mode": "bypass"}),
        );
        let changed = call(
            &obid,
            "POST",
            &format!("/v1/jobs/{id}/mode"),
            Some(&json!({"mode": "bypass"})),
        );
        assert_eq!(changed.status, 200, "{}", changed.text);
        assert_eq!(changed.body["mode"], "plan", "{kind}");
        let job = ended(&obid, &id);
        assert_eq!(job["mode"], "plan", "{job}");
        assert_eq!(job["status"], "done", "{job}");
        assert_eq!(job["commits"], 0, "{job}");
        assert!(summary(&job).starts_with("edit: decline"), "{job}");
        assert!(
            !obid
                .home
                .join("worktrees")
                .join(&id)
                .join("plan.txt")
                .exists()
        );
    }
}

/// A review on Codex that commits anyway (#630) is done, its review standing: it publishes
/// nothing, says so, and shows the commits it made.
#[test]
fn a_review_that_commits_publishes_nothing() {
    let folder = Folder::new("codex-review-commits");
    let obid = obid(&folder, false);
    let id = dispatch(
        &obid,
        json!({"agent": "codex", "kind": "review", "task": "SNEAK: notes.txt"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(
        job["reason"], "A review made commits; nothing was published.",
        "{job}"
    );
    assert_eq!(job["commits"], 1, "{job}");
    assert_eq!(job["published"], Value::Null, "{job}");
}

/// The next round (the check failed) picks the same thread up again (`thread/resume`) in a new
/// app-server, which remembers the task; a turn that fails says what the API said; Stop ends the server and
/// everything it started within Stop's 2 seconds.
#[test]
fn codex_resumes_fails_and_stops() {
    let folder = Folder::new("codex-rounds");
    let obid = obid(&folder, false);
    let twice = dispatch(
        &obid,
        json!({"agent": "codex", "task": "TWICE", "rounds": 2, "check": "test -f .obi/twice"}),
    );
    let fail = dispatch(&obid, json!({"agent": "codex", "task": "FAIL"}));
    let sleep = dispatch(&obid, json!({"agent": "codex", "task": "SLEEP"}));

    let job = ended(&obid, &twice);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["round"], 2);
    let thread = job["session"].as_str().unwrap();
    let summaries = job["summaries"].as_array().unwrap();
    assert_eq!(summaries[0], "First round.");
    let second = summaries[1].as_str().unwrap();
    assert!(
        second.starts_with(&format!("Round 2 on thread {thread}, which remembers: "))
            && second.contains("What the user asked for: TWICE")
            && second.ends_with("GOAL MET"),
        "{job}"
    );
    // A turn that fails part way needs you: Try again picks its thread up.
    let job = ended(&obid, &fail);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(summary(&job), "Codex hit an error: it broke");
    assert_eq!(job["reason"], "Codex hit an error: it broke");
    assert_eq!(job["nextStep"], "Try again");

    beats(&obid, &sleep);
    let started = std::time::Instant::now();
    let stopped = call(&obid, "POST", &format!("/v1/jobs/{sleep}/stop"), None);
    assert_eq!(stopped.status, 200);
    assert_eq!(stopped.body["status"], "stopped");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(stopped_beating(&obid, &sleep), "Codex outlived its stop");
}

/// A Cursor job: Cursor writes without asking (as the probe saw), asks before its commit, and
/// obid picks `allow_once`, never `allow_always`. Its three folders are the job's own (#655), with
/// a copy of the account's login and settings, and obid's settings for it allow nothing ahead of
/// obid, keeping Cursor's own keys. The account's own folder is left as it was.
#[test]
fn cursor_commits_with_obids_approval() {
    let folder = Folder::new("cursor-commits");
    let obid = obid(&folder, false);
    let id = dispatch(
        &obid,
        json!({"agent": "cursor", "kind": "build", "task": "COMMIT: hello.txt", "check": "test -f hello.txt"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["agent"], "cursor");
    assert_eq!(job["account"], "cursor");
    assert_eq!(job["commits"], 1);
    // What it said after its last tool call is the summary.
    assert_eq!(summary(&job), "Added hello.txt.");
    assert!(job["session"].as_str().is_some_and(|s| !s.is_empty()));

    let id = dispatch(
        &obid,
        json!({"agent": "cursor", "task": "ENV", "model": "gpt-5.6-luna"}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let home = obid.home.join("logins").join(&id);
    let account = folder.0.join("home/accounts/cursor");
    assert_eq!(
        std::fs::read_to_string(account.join("cli-config.json")).unwrap(),
        r#"{"authInfo": {"email": "owner@example.com"}, "version": 1, "permissions": {"allow": ["Shell(*)"], "deny": []}}"#,
        "a job's settings reached the account's folder"
    );
    assert_eq!(
        summary(&job),
        [
            format!("HOME={}", home.display()),
            format!("XDG_CONFIG_HOME={}", home.join(".config").display()),
            format!("CURSOR_CONFIG_DIR={}", home.display()),
            "model=gpt-5.6-luna[context=272k,reasoning=medium]".into(),
            "mode=agent".into(),
            "approvalMode=allowlist".into(),
            "allow=[]".into(),
            "kept=owner@example.com".into(),
        ]
        .join("\n")
    );
}

/// Cursor's own login on the Mac (`~/.cursor`, no account folder) is listed at start and runs as
/// an account's does: from a copy of its login and settings in a folder of the job's own, the
/// person's own files left as they were.
#[test]
fn cursors_own_login_runs_from_a_copy() {
    let folder = Folder::new("cursor-own");
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-claude"));
    config["agents"] = json!({"cursor": {"bin": agents().join("fake-cursor"), "accounts": []}});
    let person = folder.0.join("env-home");
    let login = person.join(if cfg!(target_os = "macos") {
        ".cursor/auth.json"
    } else {
        ".config/cursor/auth.json"
    });
    std::fs::create_dir_all(login.parent().unwrap()).unwrap();
    std::fs::create_dir_all(person.join(".cursor")).unwrap();
    // Built at runtime, so a secret scan doesn't take the test's value for a credential.
    std::fs::write(
        &login,
        json!({"accessToken": (["own", "-access"].concat())}).to_string(),
    )
    .unwrap();
    let settings = r#"{"authInfo": {"email": "owner@example.com"}, "permissions": {"allow": ["Shell(*)"], "deny": []}}"#;
    std::fs::write(person.join(".cursor/cli-config.json"), settings).unwrap();
    let obid = start(&folder.0, &config, None);
    let id = dispatch(&obid, json!({"agent": "cursor", "task": "ENV"}));
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["account"], "cursor");
    let home = obid.home.join("logins").join(&id);
    let said = summary(&job);
    assert!(said.contains(&format!("HOME={}", home.display())), "{said}");
    assert!(
        said.contains("allow=[]") && said.contains("kept=owner@example.com"),
        "{said}"
    );
    assert_eq!(
        std::fs::read_to_string(person.join(".cursor/cli-config.json")).unwrap(),
        settings
    );
}

/// Cursor's own login only in the macOS Keychain (`cursorKeychain`, a stand-in `security` holding
/// its access and refresh tokens): a job runs from a private copy holding only the access token
/// (and an API key, when there's one), the refresh token never asked for; the copy is gone once
/// the run ends, and the token never shows in what the job says.
#[cfg(target_os = "macos")]
#[test]
fn cursors_keychain_login_runs_from_a_copy_of_only_what_calls_the_api() {
    let folder = Folder::new("cursor-keychain");
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-claude"));
    config["agents"] = json!({"cursor": {"bin": agents().join("fake-cursor"), "accounts": []}});
    config["cursorKeychain"] = json!(true);
    let security = agents().join("fake-security-cursor");
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_SECURITY_BIN", security.to_str().unwrap())],
    );
    let id = dispatch(&obid, json!({"agent": "cursor", "task": "LOGIN"}));
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    assert_eq!(job["account"], "cursor");
    let said = summary(&job);
    assert!(said.contains("keys=accessToken\n"), "{said}");
    assert!(
        !said.contains("keychain-held-access") && said.contains("[redacted"),
        "{said}"
    );
    assert!(!said.contains("keychain-held-refresh"), "{said}");
    let login = obid.home.join("logins").join(&id);
    assert!(login.is_dir());
    assert!(!login.join(".cursor/auth.json").exists());
    let asked = std::fs::read_to_string(folder.0.join("tmp/obid-test-security-calls")).unwrap();
    assert!(
        asked.contains("-a cursor-user -s cursor-access-token"),
        "{asked}"
    );
    assert!(!asked.contains("cursor-refresh-token"), "{asked}");
    assert!(!obid.log().contains("keychain-held-access"));
}

/// obid's fixed denials hold for Cursor: a push is `reject_once`, and the next round's prompt
/// starts with what obid declined and why, since ACP's answer carries no words. A request for
/// obid's files is refused, and a fine command is allowed once.
#[test]
fn cursor_hears_what_obid_declined_at_its_next_prompt() {
    let folder = Folder::new("cursor-declines");
    let obid = obid(&folder, false);
    let twice = dispatch(
        &obid,
        json!({"agent": "cursor", "task": "TWICE", "rounds": 2, "check": "test -f .obi/twice"}),
    );
    let fs = dispatch(&obid, json!({"agent": "cursor", "task": "FS"}));
    let fine = dispatch(
        &obid,
        json!({"agent": "cursor", "task": "ASK: node --test"}),
    );

    let job = ended(&obid, &twice);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["round"], 2);
    let summaries = job["summaries"].as_array().unwrap();
    assert_eq!(summaries[0], "push: reject-once");
    assert!(
        summaries[1]
            .as_str()
            .unwrap()
            .starts_with("heard: obid declined git push: obid publishes the job's work itself."),
        "{job}"
    );
    assert_eq!(ended(&obid, &fs)["summary"], "fs: error -32601");
    assert_eq!(ended(&obid, &fine)["summary"], "answer: allow-once");
}

/// Job `id` once its agent's question waits for the owner (#858): the job's `permission`, with its
/// `question`, and an `agentQuestion` inbox item.
fn question_waits(obid: &Obid, id: &str) -> Value {
    let job = within(60, "the question waits for the owner", || {
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        job["permission"]["question"].is_object().then_some(job)
    });
    let item = format!(
        "{id}-permission-{}",
        job["permission"]["id"].as_str().unwrap()
    );
    let items = call(obid, "GET", "/v1/inbox", None).body;
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == item && i["kind"] == "agentQuestion"),
        "{items}"
    );
    job
}

/// The owner answers job `id`'s waiting question with `answer`: an option's label or their words.
fn answer_question(obid: &Obid, id: &str, job: &Value, answer: &str) {
    let answered = call(
        obid,
        "POST",
        &format!("/v1/jobs/{id}/permission"),
        Some(&json!({"id": job["permission"]["id"], "answer": answer})),
    );
    assert_eq!(answered.status, 200, "{}", answered.text);
}

/// Cursor's own question (`cursor/ask_question`) waits for the owner as Claude Code's
/// AskUserQuestion does (#858): an option goes back as its id; the owner's own words, which
/// Cursor's answer can't carry, go back as `skipped` with the words and come as the next prompt.
#[test]
fn cursors_question_waits_for_the_owners_answer() {
    let folder = Folder::new("cursor-question");
    let obid = obid(&folder, false);
    let picked = dispatch(&obid, json!({"agent": "cursor", "task": "QUESTION"}));
    let words = dispatch(&obid, json!({"agent": "cursor", "task": "QUESTION"}));
    let job = question_waits(&obid, &picked);
    assert_eq!(job["permission"]["question"]["question"], "Which?", "{job}");
    assert_eq!(
        job["permission"]["question"]["header"], "Need input",
        "{job}"
    );
    answer_question(&obid, &picked, &job, "b");
    let job = question_waits(&obid, &words);
    answer_question(&obid, &words, &job, "Neither: use C");

    let job = ended(&obid, &picked);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(
        summary(&job),
        r#"question: answered [{"questionId":"q","selectedOptionIds":["b"]}]"#
    );
    let job = ended(&obid, &words);
    assert_eq!(
        summary(&job),
        r#"heard: The owner answered your question in their own words: "Neither: use C". Carry on with that, then finish with a short summary. after question: skipped "Neither: use C""#
    );
}

/// A Cursor job asks the owner with Oby's `oby_ask_owner`, on the `oby` server obid gives it
/// with that tool alone: the tool answers at once, the turn ends, and the job waits `running`
/// with no agent, its question asked as Claude Code's AskUserQuestion is (#858). The owner's own
/// words, through the inbox item, pick the same session up, quoted.
#[test]
fn a_cursor_job_asks_the_owner_and_resumes_with_the_answer() {
    let folder = Folder::new("cursor-asks");
    let obid = obid(&folder, false);
    let id = dispatch(&obid, json!({"agent": "cursor", "task": "ASK OWNER"}));
    let job = question_waits(&obid, &id);
    assert_eq!(job["status"], "running", "{job}");
    assert_eq!(job["step"], "Waiting for your answer", "{job}");
    assert_eq!(job["permission"]["tool"], "AskUserQuestion", "{job}");
    assert_eq!(job["permission"]["question"]["question"], "Which store?");
    assert!(
        summary(&job)
            .starts_with("listed: oby_ask_owner\nAsked: Asked the owner. End your turn now"),
        "{job}"
    );
    let item = format!(
        "{id}-permission-{}",
        job["permission"]["id"].as_str().unwrap()
    );
    let answered = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{item}/answer"),
        Some(&json!({"answer": "Use a JSON file"})),
    );
    assert_eq!(answered.status, 200, "{}", answered.text);

    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert!(
        summary(&job).starts_with(
            "heard: You asked the owner with oby_ask_owner and ended your turn. You asked:\n- \"Which store?\": the owner answered \"Use a JSON file\"\nCarry on with these answers"
        ),
        "{job}"
    );
}

/// A Codex job asks the owner with Oby's `oby_ask_owner` (approved as Oby's own tool, with no
/// connected app), waits with no agent, and the owner's option picks its thread up.
#[test]
fn a_codex_job_asks_the_owner_and_resumes_with_the_answer() {
    let folder = Folder::new("codex-asks");
    let obid = obid(&folder, false);
    let id = dispatch(&obid, json!({"agent": "codex", "task": "ASK OWNER"}));
    let job = question_waits(&obid, &id);
    assert_eq!(job["status"], "running", "{job}");
    assert!(
        summary(&job).starts_with("Asked: Asked the owner. End your turn now"),
        "{job}"
    );
    let options = &job["permission"]["question"]["options"];
    assert_eq!(
        options[1],
        json!({"label": "Postgres", "description": "A server"})
    );
    answer_question(&obid, &id, &job, "postgres");

    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert!(
        summary(&job).contains("- \"Which store?\": the owner answered \"Postgres\""),
        "{job}"
    );
}

/// Codex's own question (`item/tool/requestUserInput`) waits for the owner as Claude Code's
/// AskUserQuestion does (#858), the answer going back by the question's id; a secret is never
/// asked.
#[test]
fn codexs_own_question_waits_for_the_owners_answer() {
    let folder = Folder::new("codex-question");
    let obid = obid(&folder, false);
    let id = dispatch(&obid, json!({"agent": "codex", "task": "QUESTION"}));
    let secret = dispatch(&obid, json!({"agent": "codex", "task": "QUESTION SECRET"}));
    let job = question_waits(&obid, &id);
    assert_eq!(job["permission"]["question"]["header"], "Store", "{job}");
    answer_question(&obid, &id, &job, "sqlite");
    assert_eq!(
        summary(&ended(&obid, &id)),
        r#"question: {"answers":{"store":{"answers":["SQLite"]}}}"#
    );
    let job = ended(&obid, &secret);
    assert!(
        summary(&job).contains("obid never collects a secret for an agent"),
        "{job}"
    );
}

/// Plan mode (#1008): Cursor's plan (`cursor/create_plan`) waits for the owner as Claude Code's
/// `ExitPlanMode` does, a `plan` item in the inbox. Keep planning and the owner's words reach
/// Cursor as `rejected`, still in its plan mode; Approve as `accepted`, and the same round goes on
/// in its agent mode, the job in Ask.
#[test]
fn a_cursor_plan_waits_for_the_owners_approval() {
    let folder = Folder::new("cursor-plan");
    let obid = obid(&folder, false);
    let id = dispatch(
        &obid,
        json!({"agent": "cursor", "task": "PLAN", "mode": "plan"}),
    );
    let answer = |plan: &str, words: &str| {
        let item = within(60, "the plan waits for the owner", || {
            call(&obid, "GET", "/v1/inbox", None)
                .body
                .as_array()?
                .iter()
                .find(|i| i["job"] == id && i["done"].is_null() && i["details"][0] == plan)
                .cloned()
        });
        assert_eq!(item["kind"], "plan", "{item}");
        assert_eq!(item["options"], json!(["Approve", "Keep planning"]));
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        assert_eq!(job["step"], "Waiting for your OK on the plan", "{job}");
        assert_eq!(job["permission"]["title"], "Approve the plan?");
        let answered = call(
            &obid,
            "POST",
            &format!("/v1/inbox/{}/answer", item["id"].as_str().unwrap()),
            Some(&json!({ "answer": words })),
        );
        assert_eq!(answered.status, 200, "{}", answered.text);
    };
    answer("1. Do it.", "Keep planning");
    answer("2. Do it.", "Also test it");
    answer("3. Do it.", "Approve");
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["mode"], "sandbox", "the owner's default mode");
    assert_eq!(
        summary(&job),
        "mode agent after rejected in mode plan (The owner didn't approve the plan yet: keep \
         planning, then present it again.); rejected in mode plan (Also test it); accepted in \
         mode plan: Your plan is approved: carry it out now, then finish with a short summary."
    );
}

/// A signed-out Cursor account fails the job saying so; a prompt answered with an error, or a
/// refusal, leaves it needing you with the tool's words; Stop cancels the prompt and ends Cursor.
#[test]
fn cursor_signed_out_failures_and_stop() {
    let folder = Folder::new("cursor-out");
    let obid = obid(&folder, true);
    let id = dispatch(&obid, json!({"agent": "cursor", "task": "anything"}));
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "failed", "{job}");
    assert!(
        summary(&job).starts_with("Cursor · Personal is signed out"),
        "{job}"
    );
    drop(obid);

    let folder = Folder::new("cursor-fails");
    let obid = self::obid(&folder, false);
    let fail = dispatch(&obid, json!({"agent": "cursor", "task": "FAIL"}));
    let refuse = dispatch(&obid, json!({"agent": "cursor", "task": "REFUSE"}));
    let sleep = dispatch(&obid, json!({"agent": "cursor", "task": "SLEEP"}));
    assert_eq!(
        ended(&obid, &fail)["summary"],
        "Cursor hit an error: it broke"
    );
    let job = ended(&obid, &refuse);
    assert_eq!(job["status"], "interrupted");
    assert_eq!(summary(&job), "The model refused to continue.");
    beats(&obid, &sleep);
    let started = std::time::Instant::now();
    let stopped = call(&obid, "POST", &format!("/v1/jobs/{sleep}/stop"), None);
    assert_eq!(stopped.body["status"], "stopped");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(stopped_beating(&obid, &sleep), "Cursor outlived its stop");
}

/// Codex runs `danger-full-access` and Cursor edits without asking: obid's job sandbox is what
/// keeps either from writing outside the worktree.
#[test]
fn the_sandbox_holds_what_the_tools_dont_ask_about() {
    let folder = Folder::new("adapters-escape");
    let obid = obid(&folder, false);
    let codex = dispatch(&obid, json!({"agent": "codex", "task": "ESCAPE"}));
    let cursor = dispatch(&obid, json!({"agent": "cursor", "task": "ESCAPE"}));
    for id in [codex, cursor] {
        let job = ended(&obid, &id);
        assert_eq!(job["status"], "done", "{job}");
        assert!(summary(&job).starts_with("escape: E"), "{job}");
    }
    assert!(!obid.home.join("worktrees/escaped.txt").exists());
}

/// A repository's hooks, which Cursor runs itself outside obid's prompts (its `.cursor/hooks.json`,
/// and Claude Code's settings, which it loads too): Cursor doesn't start until the owner says yes.
/// A no starts nothing and names Claude Code and Codex; Always is remembered for those very hooks
/// in that repository, and a changed hook asks again. In Bypass they run without asking.
#[test]
fn cursor_asks_you_before_it_runs_a_repositorys_hooks() {
    let folder = Folder::new("cursor-hooks");
    let (obid, remote) = obid_and_remote(&folder, false);
    let work = folder.0.join("work");
    git(
        &folder.0,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            work.to_str().unwrap(),
        ],
    );
    let push = |file: &str, text: &str| {
        std::fs::create_dir_all(work.join(file).parent().unwrap()).unwrap();
        std::fs::write(work.join(file), text).unwrap();
        git(&work, &["add", file]);
        git(
            &work,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                "hooks",
            ],
        );
        git(&work, &["push", "-q", "origin", "HEAD:main"]);
    };
    let card = |id: &str| {
        within(60, "the hooks wait for the owner", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
            (job["permission"]["tool"] == "RepositoryHooks").then_some(job)
        })
    };
    let answer = |id: &str, job: &Value, decision: &str| {
        let answered = call(
            &obid,
            "POST",
            &format!("/v1/jobs/{id}/permission"),
            Some(&json!({"id": job["permission"]["id"], "decision": decision})),
        );
        assert_eq!(answered.status, 200, "{}", answered.text);
    };
    push(
        ".cursor/hooks.json",
        r#"{"version": 1, "hooks": {"afterFileEdit": [{"command": "./format.sh"}]}}"#,
    );

    // A no: Cursor never started.
    let id = dispatch(&obid, json!({"agent": "cursor", "task": "anything"}));
    let job = card(&id);
    assert_eq!(
        job["permission"]["title"],
        "Let Cursor run this repository's hooks: ./format.sh"
    );
    assert_eq!(
        job["permission"]["detail"],
        ".cursor/hooks.json (afterFileEdit): ./format.sh"
    );
    assert!(job["session"].is_null(), "{job}");
    answer(&id, &job, "deny");
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "failed", "{job}");
    assert!(
        summary(&job).starts_with(
            "Cursor didn't start: you said no to this repository's hooks, which Cursor runs itself"
        ) && summary(&job).contains("Claude Code and Codex jobs don't run them"),
        "{job}"
    );

    // Always: it runs, and the next job on the same hooks doesn't ask.
    let id = dispatch(&obid, json!({"agent": "cursor", "task": "anything"}));
    answer(&id, &card(&id), "always");
    assert_eq!(ended(&obid, &id)["status"], "done");
    let rules = call(&obid, "GET", "/v1/permissions/rules", None).body;
    assert!(
        rules
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["repo"] == "obi-e2e/fixture-app"
                && r["rule"].as_str().unwrap().starts_with("RepositoryHooks(")),
        "{rules}"
    );
    let id = dispatch(&obid, json!({"agent": "cursor", "task": "anything"}));
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(summary(&job), "Nothing to do.");

    // A hook added in Claude Code's settings changes the set: it asks again.
    push(
        ".claude/settings.json",
        r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "./notify.sh"}]}]}}"#,
    );
    let id = dispatch(&obid, json!({"agent": "cursor", "task": "anything"}));
    let job = card(&id);
    assert_eq!(
        job["permission"]["detail"],
        ".cursor/hooks.json (afterFileEdit): ./format.sh\n.claude/settings.json (Stop): ./notify.sh"
    );
    answer(&id, &job, "allow");
    assert_eq!(ended(&obid, &id)["status"], "done");

    // In Bypass the owner chose to be asked nothing: the same hooks, never allowed always, run
    // without a card (#1570, BYPASS-3).
    let id = dispatch(
        &obid,
        json!({"agent": "cursor", "task": "anything", "mode": "bypass"}),
    );
    let job = ended(&obid, &id);
    assert_eq!((job["status"].as_str(), job["mode"].as_str()), (Some("done"), Some("bypass")), "{job}");
}

/// A repository whose `.cursor/cli.json` lets commands through without asking: Cursor doesn't
/// start, and the job says why. And jobs name a tool obid has: Claude Code isn't set up here.
#[test]
fn a_repository_allowlist_keeps_cursor_from_starting() {
    let folder = Folder::new("cursor-allowlist");
    let (obid, remote) = obid_and_remote(&folder, false);
    // The fixture's remote, with a commit that adds the allowlist.
    let work = folder.0.join("work");
    git(
        &folder.0,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            work.to_str().unwrap(),
        ],
    );
    std::fs::create_dir_all(work.join(".cursor")).unwrap();
    std::fs::write(
        work.join(".cursor/cli.json"),
        r#"{"permissions": {"allow": ["Shell(npm test)"]}}"#,
    )
    .unwrap();
    git(&work, &["add", ".cursor/cli.json"]);
    git(
        &work,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "allowlist",
        ],
    );
    git(&work, &["push", "-q", "origin", "HEAD:main"]);
    let id = dispatch(&obid, json!({"agent": "cursor", "task": "anything"}));
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "failed", "{job}");
    assert!(
        summary(&job).starts_with("Cursor didn't start: this repository's .cursor/cli.json lets it run Shell(npm test) without asking obid"),
        "{job}"
    );

    let answer = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "fix", "task": "x", "agent": "claude"}}),
        ),
    );
    assert_eq!(answer.status, 422);
    assert!(
        answer.text.contains("names no agents.claude"),
        "{}",
        answer.text
    );
    let answer = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "fix", "task": "x", "account": "Personal"}}),
        ),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    assert_eq!(answer.body["agent"], "cursor");
}

/// The real `cursor-agent`, signed out on a fresh account folder: obid starts it in the job sandbox,
/// speaks ACP with it, and the job fails saying the account is signed out. No credentials. Runs only
/// when asked (`cargo test -- --ignored`) on a machine with `cursor-agent` on PATH.
#[test]
#[ignore]
fn the_real_cursor_agent_signed_out_says_so() {
    let Some(bin) = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("cursor-agent"))
        .find(|p| p.exists())
    else {
        panic!("cursor-agent isn't on PATH");
    };
    let folder = Folder::new("real-cursor");
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-claude"));
    // A short folder: Cursor keeps its sockets under its config folder only while that path is
    // short enough for a socket's name (else in /tmp/.cursor, which the sandbox doesn't let it write).
    let fresh = Folder(
        std::path::Path::new("/tmp")
            .canonicalize()
            .unwrap()
            .join(format!("obid-cursor-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&fresh.0);
    config["agents"] = json!({"cursor": {"bin": bin, "accounts": [
        {"id": "fresh", "name": "Fresh", "configDir": fresh.0}]}});
    let obid = start(&folder.0, &config, None);
    let id = dispatch(&obid, json!({"task": "Say hello."}));
    let job = ended(&obid, &id);
    eprintln!("{job}");
    assert_eq!(job["status"], "failed", "{job}");
    assert!(
        summary(&job).starts_with("Cursor · Fresh is signed out"),
        "{job}\n{}",
        std::fs::read_to_string(obid.home.join("jobs").join(&id).join("agent.log"))
            .unwrap_or_default()
    );
}

/// In `ask`, Codex's and Cursor's commands wait for the owner like Claude Code's
/// (docs/runner-api.md, "Permission modes"): the job shows the permission, and the owner's answer
/// reaches the tool as its own decision (Codex's `accept` or `decline`, Cursor's `allow-once` or
/// `reject-once`).
#[test]
fn codex_and_cursor_ask_the_owner_in_ask_mode() {
    let folder = Folder::new("ask-tools");
    let obid = obid(&folder, false);
    let codex = dispatch(
        &obid,
        json!({"agent": "codex", "task": "ASK: node --test", "mode": "ask"}),
    );
    let cursor = dispatch(
        &obid,
        json!({"agent": "cursor", "task": "ASK: node --test", "mode": "ask"}),
    );
    let answer = |id: &str, decision: &str| {
        let job = within(60, "the tool waits for the owner", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
            (job["permission"]["title"] == "Run node --test").then_some(job)
        });
        let answered = call(
            &obid,
            "POST",
            &format!("/v1/jobs/{id}/permission"),
            Some(&json!({"id": job["permission"]["id"], "decision": decision})),
        );
        assert_eq!(answered.status, 200, "{}", answered.text);
    };
    answer(&codex, "allow");
    answer(&cursor, "deny");
    assert_eq!(summary(&ended(&obid, &codex)), "answer: accept");
    assert_eq!(summary(&ended(&obid, &cursor)), "answer: reject-once");
}

/// What a Codex job asks the owner, by mode (2026-10-27: the owner, on Bypass, was asked for each
/// read of Codex's reviews). Settings' Bypass (`PUT /v1/settings/permissions`) reaches new jobs:
/// the thread keeps obid's full access with every approval coming to obid, which answers the file
/// change, the commit and the network without the owner. A review, in Plan, runs Codex's
/// read-only commands without the owner. In Sandbox, git's local reads and tests go through, and
/// the network waits for the owner.
#[test]
fn codex_asks_the_owner_only_what_its_mode_keeps() {
    let folder = Folder::new("codex-modes");
    let obid = obid(&folder, false);
    let set = call(
        &obid,
        "PUT",
        "/v1/settings/permissions",
        Some(&json!({"mode": "bypass"})),
    );
    assert_eq!(set.body, json!({"mode": "bypass"}));
    let bypass = dispatch(
        &obid,
        json!({"agent": "codex", "task": "COMMIT: hello.txt"}),
    );
    let fetch = dispatch(
        &obid,
        json!({"agent": "codex", "task": "ASK: curl -s https://example.com"}),
    );
    let review = dispatch(
        &obid,
        json!({"agent": "codex", "kind": "review",
               "task": "ASK: git status --short && git diff --stat HEAD && grep -n e README.md | head -5; sed -n '1,20p' README.md"}),
    );
    let sandbox = |task: &str| {
        dispatch(
            &obid,
            json!({"agent": "codex", "task": task, "mode": "sandbox"}),
        )
    };
    let local = sandbox("ASK: git status --short; git check-ignore .obi/x; node --test");
    let network = sandbox("ASK: curl -s https://example.com");

    let job = ended(&obid, &bypass);
    assert_eq!(job["mode"], "bypass", "{job}");
    assert_eq!(job["commits"], 1, "{job}");
    let answered: Vec<(&str, &str)> = job["answered"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["tool"].as_str().unwrap(), a["decision"].as_str().unwrap()))
        .collect();
    assert_eq!(
        answered,
        [("Edit", "allowed"), ("Bash", "allowed")],
        "{job}"
    );
    let thread = job["session"].as_str().unwrap();
    let kept: Value = serde_json::from_str(
        &std::fs::read_to_string(
            obid.home
                .join("logins")
                .join(&bypass)
                .join(format!("threads/{thread}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(kept["sandbox"], "danger-full-access");
    assert_eq!(kept["approvalPolicy"], "untrusted");
    for id in [&fetch, &review, &local] {
        let job = ended(&obid, id);
        assert_eq!(summary(&job), "answer: accept", "{job}");
    }
    assert_eq!(ended(&obid, &review)["mode"], "plan");

    let job = within(60, "Sandbox asks about the network", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{network}"), None).body;
        job["permission"].is_object().then_some(job)
    });
    assert_eq!(job["permission"]["title"], "Run curl");
    let answered = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{network}/permission"),
        Some(&json!({"id": job["permission"]["id"], "decision": "deny"})),
    );
    assert_eq!(answered.status, 200, "{}", answered.text);
    assert!(
        summary(&ended(&obid, &network)).starts_with("answer: decline"),
        "the owner's no"
    );
}

/// Codex's and Cursor's approvals and questions park their jobs as Claude Code's do
/// (docs/runner-api.md, "Parked"): past `parkAfter` the agent ends, its thread or session kept,
/// having heard no answer, and the job keeps its card. An allow picks it up and its same call goes
/// through (Codex's `accept`, Cursor's `allow-once`); a no or a question's answer picks it up told
/// them. A Cursor that can't load its session fails the job rather than start a new one.
#[test]
fn codex_and_cursor_park_and_resume_with_the_answer() {
    let folder = Folder::new("park-tools");
    let (obid, _) = obid_with(&folder, false, json!({"parkAfter": 1}));
    let jobs: Vec<(String, Value)> = [
        ("codex", "ASK: node --test", json!({"decision": "allow"})),
        (
            "codex",
            "ASK: node --test",
            json!({"decision": "deny", "message": "Not here."}),
        ),
        ("codex", "QUESTION", json!({"answer": "SQLite"})),
        ("cursor", "ASK: node --test", json!({"decision": "allow"})),
        (
            "cursor",
            "ASK: node --test",
            json!({"decision": "deny", "message": "Not here."}),
        ),
        ("cursor", "QUESTION", json!({"answer": "A"})),
        (
            "cursor",
            "ASK: node --test NO LOAD",
            json!({"decision": "allow"}),
        ),
    ]
    .into_iter()
    .map(|(agent, task, answer)| {
        let id = dispatch(&obid, json!({"agent": agent, "task": task, "mode": "ask"}));
        (id, answer)
    })
    .collect();
    let worktree = |id: &str| obid.home.join("worktrees").join(id);
    for (id, answer) in &jobs {
        let job = within(60, "the job parks", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
            let parked = job["changes"].as_array().is_some_and(|c| {
                c.iter().any(|c| {
                    c["what"]
                        .as_str()
                        .is_some_and(|w| w.starts_with("Parked by obid"))
                })
            });
            parked.then_some(job)
        });
        assert_eq!(job["status"], "running", "{job}");
        if job["task"].as_str().unwrap().contains("NO LOAD") {
            std::fs::create_dir_all(worktree(id).join(".obi")).unwrap();
            std::fs::write(worktree(id).join(".obi/no-load"), "").unwrap();
        }
        let mut answer = answer.clone();
        answer["id"] = job["permission"]["id"].clone();
        let answered = call(
            &obid,
            "POST",
            &format!("/v1/jobs/{id}/permission"),
            Some(&answer),
        );
        assert_eq!(answered.status, 200, "{}", answered.text);
    }
    let said: Vec<(Value, String)> = jobs
        .iter()
        .map(|(id, _)| {
            let answers = std::fs::read_to_string(worktree(id).join(".obi/answers.log"));
            (ended(&obid, id), answers.unwrap_or_default())
        })
        .collect();
    let [
        codex_yes,
        codex_no,
        codex_asked,
        cursor_yes,
        cursor_no,
        cursor_asked,
        unloaded,
    ] = &said[..]
    else {
        panic!("{said:?}");
    };
    // The parked runs heard nothing: only the resumed runs' answers are there.
    assert_eq!(summary(&codex_yes.0), "answer: accept", "{}", codex_yes.0);
    assert_eq!(codex_yes.1, "accept\n");
    assert_eq!(
        summary(&cursor_yes.0),
        "answer: allow-once",
        "{}",
        cursor_yes.0
    );
    assert_eq!(cursor_yes.1, "allow-once\n");
    for (job, answers) in [codex_no, cursor_no] {
        assert!(
            summary(job).starts_with("heard: obid paused you")
                && summary(job).ends_with("The owner said no: Not here."),
            "{job}"
        );
        assert_eq!(answers, "", "{job}");
    }
    for (job, _) in [codex_asked, cursor_asked] {
        assert!(
            summary(job).starts_with(
                "heard: obid paused you while your AskUserQuestion call waited for the owner"
            ),
            "{job}"
        );
    }
    assert_eq!(unloaded.0["status"], "interrupted", "{}", unloaded.0);
    assert!(
        unloaded.0["reason"]
            .as_str()
            .unwrap()
            .starts_with("Cursor can't load its session"),
        "{}",
        unloaded.0
    );
    assert_eq!(unloaded.1, "", "{}", unloaded.0);
}

/// Codex wraps each command in `/bin/zsh -lc "…"` (#634): the card asks about the command out of
/// that shell, all of it, and offers Always with a rule when it's a single simple command.
#[test]
fn codex_cards_show_the_command_out_of_its_shell() {
    let folder = Folder::new("codex-cards");
    let obid = obid(&folder, false);
    let simple = dispatch(
        &obid,
        json!({"agent": "codex", "task": r#"ASK: /bin/zsh -lc "git add \"src/my notes.txt\"""#, "mode": "ask"}),
    );
    let chained = dispatch(
        &obid,
        json!({"agent": "codex", "task": r#"ASK: /bin/zsh -lc "git add src/permissions.rs src/codex.rs && git commit -m \"fix(obid): show it all\"""#, "mode": "ask"}),
    );
    let card = |id: &str| {
        within(60, "the tool waits for the owner", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
            job["permission"]
                .is_object()
                .then(|| job["permission"].clone())
        })
    };
    let answer = |id: &str, card: &Value| {
        let answered = call(
            &obid,
            "POST",
            &format!("/v1/jobs/{id}/permission"),
            Some(&json!({"id": card["id"], "decision": "allow"})),
        );
        assert_eq!(answered.status, 200, "{}", answered.text);
    };
    let first = card(&simple);
    assert_eq!(first["title"], "Run git add", "{first}");
    assert_eq!(first["detail"], r#"git add "src/my notes.txt""#, "{first}");
    assert_eq!(first["rule"], "Bash(git add:*)", "{first}");
    assert_eq!(
        first["always"],
        "Yes, and don't ask again for git add in obi-e2e/fixture-app"
    );
    let second = card(&chained);
    let command =
        r#"git add src/permissions.rs src/codex.rs && git commit -m "fix(obid): show it all""#;
    assert_eq!(second["title"], format!("Run {command}"), "{second}");
    assert_eq!(second["detail"], command, "{second}");
    assert_eq!(second["rule"], Value::Null, "{second}");
    answer(&simple, &first);
    answer(&chained, &second);
    assert_eq!(summary(&ended(&obid, &simple)), "answer: accept");
    assert_eq!(summary(&ended(&obid, &chained)), "answer: accept");
}

/// Codex's usage reaches its Account: the `account/rateLimits/read` obid sends as the run starts,
/// then an `account/rateLimits/updated` that changes one window and keeps the other.
#[test]
fn codex_reports_its_accounts_usage() {
    let folder = Folder::new("codex-usage");
    let obid = obid(&folder, false);
    let reset = obi_protocol::time::now() + 86_400;
    let read = json!({"limitId": "codex", "primary": {"windowDurationMins": 300, "usedPercent": 20, "resetsAt": reset - 3_600},
        "secondary": {"windowDurationMins": 10080, "usedPercent": 40, "resetsAt": reset}});
    let id = dispatch(
        &obid,
        json!({"agent": "codex", "task": format!("READ: {read}")}),
    );
    assert_eq!(ended(&obid, &id)["status"], "done");
    let work = |obid: &Obid| call(obid, "GET", "/v1/status", None).body["accounts"][0].clone();
    let account = work(&obid);
    assert_eq!(account["id"], "work");
    assert_eq!(account["fiveHour"], 0.2);
    assert_eq!(account["sevenDay"], 0.4);
    assert_eq!(
        account["fiveHourResets"],
        obi_protocol::time::format(reset - 3_600)
    );
    assert_eq!(account["sevenDayResets"], obi_protocol::time::format(reset));
    assert_eq!(account["reserved"], false);

    let update = json!({"primary": {"windowDurationMins": 300, "usedPercent": 100, "resetsAt": reset - 1_800}});
    let id = dispatch(
        &obid,
        json!({"agent": "codex", "task": format!("LIMITS: {update}")}),
    );
    ended(&obid, &id);
    let account = work(&obid);
    assert_eq!(account["fiveHour"], 1.0);
    assert_eq!(account["sevenDay"], 0.4);
    assert_eq!(
        account["fullUntil"],
        obi_protocol::time::format(reset - 1_800)
    );
}

/// A Codex job's tokens: each round's own, though its thread is resumed with the earlier round's
/// total, and an update sent twice counts once; `account/read` says a plan pays (#731).
#[test]
fn codex_reports_a_jobs_tokens() {
    let folder = Folder::new("codex-tokens");
    let obid = obid(&folder, false);
    let id = dispatch(
        &obid,
        json!({"agent": "codex", "kind": "build", "task": "TOKENS", "check": "false", "rounds": 2}),
    );
    let job = ended(&obid, &id);
    assert_eq!(job["round"], 2, "{job}");
    assert_eq!(
        job["usage"],
        json!({"input": 800, "output": 200, "cacheRead": 1200, "priced": "plan"}),
        "{job}"
    );
    // obid asks how the account pays once: a later job's answer, an API key's, isn't asked for.
    let id = dispatch(&obid, json!({"agent": "codex", "task": "TOKENS APIKEY"}));
    assert_eq!(ended(&obid, &id)["usage"]["priced"], "plan");
}

/// A Codex that ends its turn to wait for something still running (it wakes for nothing) is told
/// once, in its thread, to wait in the foreground, and goes on; one that ends waiting again needs
/// you, never "nothing committed".
#[test]
fn a_codex_turn_that_ends_waiting_goes_on_or_needs_you() {
    let folder = Folder::new("adapters-waits");
    let obid = obid(&folder, false);
    let id = dispatch(&obid, json!({"agent": "codex", "task": "WAITS"}));
    let job = ended(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(summary(&job), "Built in the foreground.", "{job}");

    let again = dispatch(
        &obid,
        json!({"agent": "codex", "kind": "build", "task": "WAITS ALWAYS"}),
    );
    let job = ended(&obid, &again);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert!(
        job["reason"]
            .as_str()
            .unwrap()
            .starts_with("Stopped waiting: Still building."),
        "{job}"
    );
}
