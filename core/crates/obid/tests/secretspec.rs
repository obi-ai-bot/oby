//! Secrets a repository declares for its check (crate::secretspec; oby-plan E1, "Secrets you
//! declare"), on the real obid binary and as obid runs a review another Obi asked for:
//!
//! - an owner's job's check gets the values its base branch's `secretspec.toml` declares, from
//!   the owner's vault as `check.<NAME>`, and nothing its own branch adds to the file;
//! - every value the check prints, or writes in a file, is taken out before obid keeps it or its
//!   agent reads it, and the agent never has one in its environment or its prompt;
//! - a received review's check gets nothing, whatever the file asks, and neither does a follow-up
//!   of it.
//!
//! Every value here is an obviously fake test value.

mod support;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};
use support::*;

/// The value the base branch's file asks for, as the owner's vault holds it.
fn fake() -> String {
    ["fake-check-", "value/", "Z9q2+not=real"].concat()
}

/// The value only the job's own branch asks for: it must never reach the check.
fn wider() -> String {
    ["fake-wider-", "value/", "K3p8+not=real"].concat()
}

/// What the check prints of [`fake`] to show it got it, without printing it: the start of its
/// SHA-256.
fn fingerprint(value: &str) -> String {
    let output = Command::new("node")
        .args([
            "-e",
            "process.stdout.write(require('crypto').createHash('sha256').update(process.argv[1]).digest('hex').slice(0,16))",
            value,
        ])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}

/// Asserts `text` holds no copy of `value`, plain, base64 or hex.
fn holds_none(text: &str, value: &str, what: &str) {
    for copy in [
        value.to_owned(),
        base64::engine::general_purpose::STANDARD.encode(value),
        hex::encode(value),
    ] {
        assert!(!text.contains(&copy), "{what} holds the value: {text}");
    }
}

/// The check script the base branch carries.
const CHECK: &str = r#"#!/bin/sh
echo "sha: $(node -e "process.stdout.write(require('crypto').createHash('sha256').update(process.env.FAKE_CHECK_TOKEN||'').digest('hex').slice(0,16))")"
echo "wider: ${WIDER_TOKEN:-unset}"
echo "plain: $FAKE_CHECK_TOKEN"
echo "base64: $(printf '%s' "$FAKE_CHECK_TOKEN" | base64)"
mkdir -p .obi
printf 'token=%s\n' "$FAKE_CHECK_TOKEN" > .obi/check-wrote.txt
printf 'staged=%s\n' "$FAKE_CHECK_TOKEN" > staged.txt
if git add staged.txt 2>/dev/null; then echo "git add: done"; else echo "git add: refused"; fi
exit 1
"#;

/// The base branch's `secretspec.toml`: one name.
const SPEC: &str = r#"[project]
name = "fixture-app"
revision = "1.0"

[profiles.default]
FAKE_CHECK_TOKEN = { description = "a fake token the tests use" }
"#;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
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

/// The fixture's `main` with `secretspec.toml` and `check.sh` committed on top, pushed.
fn declare_on_main(folder: &Path, remote: &Path) {
    let work = folder.join("work-main");
    git(
        folder,
        &[
            "clone",
            "-q",
            "--branch",
            "main",
            remote.to_str().unwrap(),
            work.to_str().unwrap(),
        ],
    );
    std::fs::write(work.join("secretspec.toml"), SPEC).unwrap();
    std::fs::write(work.join("check.sh"), CHECK).unwrap();
    git(&work, &["add", "secretspec.toml", "check.sh"]);
    git(
        &work,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "Declare the check's secrets",
        ],
    );
    git(&work, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
}

/// `obid vault --config <config> add <name>`, the value on stdin.
fn add(config: &Path, name: &str, value: &str) {
    let mut child = Command::new(OBID)
        .args(["vault", "--config"])
        .arg(config)
        .args(["add", name])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(value.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn agent() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/secretspec-claude")
}

/// An owner's job whose agent widens `secretspec.toml` on its branch: its check gets the base's
/// value and not the branch's (b), every copy the check prints or writes is taken out (c), and
/// the agent sees no value in its environment, its prompts or the file the check wrote (d).
#[test]
fn an_owners_check_gets_the_bases_values_and_nothing_leaves_it() {
    let folder = Folder::new("secretspec-owner");
    let remote = fixture(&folder.0.join("fixture"));
    declare_on_main(&folder.0, &remote);
    let config = config(&folder.0, &remote, &agent());
    put_token(&config, TOKEN);
    let path = folder.0.join("config.json");
    std::fs::write(&path, config.to_string()).unwrap();
    add(&path, "check.FAKE_CHECK_TOKEN", &fake());
    add(&path, "check.WIDER_TOKEN", &wider());
    let obid = start(&folder.0, &config, None);

    let answer = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "build",
                              "task": "Make the check pass", "check": "sh check.sh", "rounds": 2}}),
        ),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}/wait"), None).body;
    // Its check failed twice, so its agent had a second round, told what the check printed.
    assert_eq!(job["checks"][0]["round"], 2, "{job}");

    // (b) The check got the base's value, and nothing for the name only the branch added.
    let log = std::fs::read_to_string(obid.home.join(format!("logs/{id}/check.log"))).unwrap();
    assert!(
        log.contains(&format!("sha: {}", fingerprint(&fake()))),
        "{log}"
    );
    assert!(log.contains("wider: unset"), "{log}");
    // (c) Every copy it printed is taken out of what obid kept.
    assert!(
        log.contains("plain: [check secret: FAKE_CHECK_TOKEN]"),
        "{log}"
    );
    assert!(
        log.lines().any(
            |l| l.starts_with("obid: the check wrote a declared secret in ")
                && l.contains(".obi/check-wrote.txt")
                && l.contains("staged.txt")
        ),
        "{log}"
    );
    holds_none(&log, &fake(), "check.log");
    holds_none(&job.to_string(), &fake(), "the job");
    let kept = std::fs::read_to_string(obid.home.join(format!("jobs/{id}.json"))).unwrap();
    holds_none(&kept, &fake(), "the job as kept");
    let worktree = obid.home.join(format!("worktrees/{id}"));
    let wrote = std::fs::read_to_string(worktree.join(".obi/check-wrote.txt")).unwrap();
    assert_eq!(wrote, "token=[check secret: FAKE_CHECK_TOKEN]\n");
    // What it staged in git, which obid can't read inside, is undone: the objects gone, the file
    // unstaged and taken out like any other.
    assert!(log.contains("git add: done"), "{log}");
    assert!(log.contains("obid: the check wrote to git"), "{log}");
    assert_eq!(
        std::fs::read_to_string(worktree.join("staged.txt")).unwrap(),
        "staged=[check secret: FAKE_CHECK_TOKEN]\n"
    );
    // Nothing the job's branch holds has a value: not its commits (obid's own "work left
    // uncommitted" one took the cleaned file), nor any object of the job's own.
    let admin = PathBuf::from(
        std::fs::read_to_string(worktree.join(".git")).unwrap()["gitdir: ".len()..].trim(),
    );
    let history = Command::new("git")
        .args(["log", "-p", "--all"])
        .env("GIT_OBJECT_DIRECTORY", admin.join("objects"))
        .env(
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            obid.home.join("clones/obi-e2e/fixture-app/.git/objects"),
        )
        .current_dir(&worktree)
        .output()
        .unwrap();
    let history = String::from_utf8_lossy(&history.stdout);
    assert!(history.contains("round 2"), "{history}");
    holds_none(&history, &fake(), "the job's git history");
    // (d) The agent: its second prompt has the check's output, with no value in it; its
    // environment never had one; what it read of the check's file had it taken out.
    let prompts = std::fs::read_to_string(worktree.join(".obi/prompts.log")).unwrap();
    assert!(prompts.contains("wider: unset"), "{prompts}");
    let env = std::fs::read_to_string(worktree.join(".obi/env.log")).unwrap();
    assert_eq!(env.lines().count(), 2, "{env}");
    for line in env.lines() {
        let env: Value = serde_json::from_str(line).unwrap();
        assert!(env.get("FAKE_CHECK_TOKEN").is_none() && env.get("WIDER_TOKEN").is_none());
    }
    let read = std::fs::read_to_string(worktree.join(".obi/read.log")).unwrap_or_default();
    for (what, text) in [("prompts", &prompts), ("env", &env), ("read", &read)] {
        holds_none(text, &fake(), what);
        holds_none(text, &wider(), what);
    }
    // obid's own log names the secrets it gave, never a value.
    let obid_log = std::fs::read_to_string(folder.0.join("obid.stderr")).unwrap();
    assert!(
        obid_log.contains("its check gets 1 of the 1 secret(s) secretspec.toml on main declares"),
        "{obid_log}"
    );
    holds_none(&obid_log, &fake(), "obid's log");
}

/// (a) A review another Obi asked for: its check gets none of the owner's values, though the
/// base's `secretspec.toml` asks and the owner's vault has them, and obid says why.
#[tokio::test(flavor = "multi_thread")]
async fn a_received_reviews_check_gets_nothing() {
    use obid::events::Events;
    use obid::jobs::{Jobs, ObiReview};
    use obid::sandbox::Sandbox;

    let folder = Folder::new("secretspec-review");
    let remote = fixture(&folder.0.join("fixture"));
    declare_on_main(&folder.0, &remote);
    let home = folder.0.join("home");
    let config = obid::config::Config::parse(
        &json!({
            "listen": "127.0.0.1:0",
            "home": home,
            "owner": "wellington",
            "agents": {"claude": {"bin": stub_agent(),
                                  "accounts": [{"id": "stub", "name": "Stub", "configDir": home.join("accounts/stub")}]}},
            "repos": [{"repo": "obi-e2e/fixture-app", "remote": format!("file://{}", remote.display()),
                       "defaultBranch": "main",
                       "check": "node -e \"console.log('token=' + (process.env.FAKE_CHECK_TOKEN ? 'SET' : 'unset'))\""}],
            "allowFileRemotes": true,
            "telemetry": false
        })
        .to_string(),
    )
    .unwrap();
    obid::prepare_home(&config).unwrap();
    let vault = obid::vault::install(&config.home, &config.vault);
    vault.add(&fake(), "check.FAKE_CHECK_TOKEN").unwrap();
    let jobs = Jobs::open(
        config,
        Events::new(),
        Sandbox::detect(Path::new(env!("CARGO_BIN_EXE_obid"))),
    )
    .unwrap();
    let job = jobs
        .start_obi_review(ObiReview {
            correlation_id: "5f0e8c1a-3b2d-4e6f-8a9b-0c1d2e3f4a5b".into(),
            request_id: "5f0e8c1a-3b2d-4e6f-8a9b-0c1d2e3f4a5b".into(),
            from: "obi-ernesto".into(),
            from_owner: "ernesto".into(),
            body: brief(),
        })
        .unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(120), jobs.wait(&job.id))
        .await
        .unwrap()
        .unwrap();
    let log = std::fs::read_to_string(home.join(format!("logs/{}/check.log", job.id))).unwrap();
    assert!(log.contains("token=unset"), "{}: {log}", ended.summary);
    assert!(
        log.contains("secretspec.toml on main declares FAKE_CHECK_TOKEN; obid gave this check none of its owner's secrets: another Oby asked for this job."),
        "{log}"
    );
    holds_none(&log, &fake(), "the review's check.log");

    // (a) Telling the ended review something ("fix the lint") starts a follow-up that runs the
    // same code: it gets nothing either, and neither does a follow-up of that.
    let follow = jobs
        .create(obid::jobs::follow_up(&ended, "fix the lint"))
        .unwrap();
    let why = follow.check_refuses_secrets().unwrap();
    assert!(why.contains("another Oby asked for this job"), "{why}");
    jobs.stop(&follow.id);
    let again = jobs
        .create(obid::jobs::follow_up(&follow, "and the tests"))
        .unwrap();
    assert!(again.check_refuses_secrets().is_some());
    jobs.stop(&again.id);
}
