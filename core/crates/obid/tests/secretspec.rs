//! Secrets a repository declares for its check (crate::secretspec; oby-plan E1, "Secrets you
//! declare"), on the real obid binary and as obid runs a review another Oby asked for:
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
echo "hidden: $(cat hidden.txt 2>/dev/null || echo none)"
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
    // As GitHub's: the remote's HEAD is its default branch, which a clone takes alone.
    git(remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
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

/// A stand-in `gh` in `folder/bin`: signed in as wellington, and GitHub says the fixture's `main`
/// is under a ruleset that requires a pull request, which wellington can't bypass.
fn github(folder: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = folder.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        bin.join("gh"),
        r#"#!/bin/sh
case "$1 $2" in
  "auth status") echo '{"hosts":{"github.com":[{"login":"wellington","active":true,"state":"success"}]}}' ;;
  "api repos/obi-e2e/fixture-app/rules/branches/main") echo '[{"type":"pull_request","ruleset_id":7,"parameters":{"required_approving_review_count":1,"require_last_push_approval":true}}]' ;;
  "api repos/obi-e2e/fixture-app/rulesets/7") echo '{"id":7,"enforcement":"active","current_user_can_bypass":"never","bypass_actors":[]}' ;;
  *) exit 1 ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
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
    let obid = start(&folder.0, &config, Some(&github(&folder.0)));

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
    holds_none(&log, &fake(), "check.log");
    holds_none(&job.to_string(), &fake(), "the job");
    let kept = std::fs::read_to_string(obid.home.join(format!("jobs/{id}.json"))).unwrap();
    holds_none(&kept, &fake(), "the job as kept");
    // It ran in a checkout of its own, made from the commit: not the file a .gitignore hides,
    // not what a process the turn left behind wrote, and nothing it wrote reached the job.
    assert!(log.contains("hidden: none"), "{log}");
    assert!(!log.contains("SURVIVOR"), "{log}");
    assert!(log.contains("git add: done"), "{log}");
    let worktree = obid.home.join(format!("worktrees/{id}"));
    assert!(!worktree.join(".obi/check-wrote.txt").exists());
    assert!(!worktree.join("staged.txt").exists());
    // The process the first turn left behind was ended with it, by its sandbox's watcher.
    let check = std::fs::read_to_string(worktree.join("check.sh")).unwrap();
    assert!(!check.contains("SURVIVOR"), "{check}");
    assert_eq!(
        std::fs::read_dir(obid.home.join("checks"))
            .map(|d| d.count())
            .unwrap_or(0),
        0
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

/// Starts obid with the fixture, `main` declaring FAKE_CHECK_TOKEN, the fake value in the vault,
/// and the stand-in gh; runs one job `job` to its end: obid, and the job's check log.
fn run_one(name: &str, job: Value) -> (Folder, Obid, String) {
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    declare_on_main(&folder.0, &remote);
    let config = config(&folder.0, &remote, &agent());
    put_token(&config, TOKEN);
    let path = folder.0.join("config.json");
    std::fs::write(&path, config.to_string()).unwrap();
    add(&path, "check.FAKE_CHECK_TOKEN", &fake());
    let obid = start(&folder.0, &config, Some(&github(&folder.0)));
    let log = dispatch_and_log(&obid, job);
    (folder, obid, log)
}

fn dispatch_and_log(obid: &Obid, job: Value) -> String {
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    call(obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    std::fs::read_to_string(obid.home.join(format!("logs/{id}/check.log"))).unwrap_or_default()
}

/// A teammate's branch isn't in obid's clone to take: the clone holds only the default branch,
/// so an agent's cherry-pick of one finds nothing.
#[test]
fn a_teammates_branch_isnt_in_the_clone_to_take() {
    let (_folder, obid, log) = run_one(
        "secretspec-cherry",
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "CHERRY", "check": "sh check.sh"}),
    );
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let refs = git(&clone, &["for-each-ref", "--format=%(refname)"]);
    assert!(!refs.contains("feature/average"), "{refs}");
    // Nothing came in, so the owner's own job's check still gets its value.
    assert!(
        log.contains(&format!("sha: {}", fingerprint(&fake()))),
        "{log}"
    );
    holds_none(&log, &fake(), "check.log");
}

/// An agent that reaches a code host through obid's proxy (here allowed, in Bypass) could have
/// read anyone's code there: its check gets nothing from then on.
#[test]
fn an_agent_that_reaches_a_code_host_gets_nothing() {
    let (_folder, _obid, log) = run_one(
        "secretspec-fetch",
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "FETCH", "check": "sh check.sh", "mode": "bypass"}),
    );
    assert!(
        log.contains(
            "obid gave this check none of its owner's secrets: its agent reached code.example.test"
        ),
        "{log}"
    );
    assert!(
        log.contains("sha: e3b0c44298fc1c14"),
        "an empty value's hash: {log}"
    );
}

/// The reviewer's repro: a job that turns someone else's code (its agent reached a code host)
/// commits a payload, which obid keeps on its branch in the owner's clone, and marks the clone. A
/// plain job after it gets a clone made again, with no other job's branch in it: the payload
/// isn't there to take, and its check gets the owner's value. The old clone, marked, is kept
/// aside with the first job.
#[test]
fn a_payload_from_another_jobs_branch_isnt_there_to_take() {
    let (_folder, obid, _) = run_one(
        "secretspec-steal",
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "FETCH", "check": "true", "mode": "bypass"}),
    );
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let marked = git(&clone, &["config", "--get", "obid.foreign"]);
    assert!(marked.contains("reached code.example.test"), "{marked}");
    let log = dispatch_and_log(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "STEAL", "check": "sh check.sh"}),
    );
    let branches = git(
        &clone,
        &["for-each-ref", "--format=%(refname)", "refs/heads/oby/"],
    );
    assert_eq!(branches.lines().count(), 1, "only its own: {branches}");
    assert!(
        log.contains(&format!("sha: {}", fingerprint(&fake()))),
        "{log}"
    );
    holds_none(&log, &fake(), "check.log");
    let aside: Vec<_> = std::fs::read_dir(obid.home.join("marked"))
        .unwrap()
        .collect();
    assert_eq!(aside.len(), 1);
}

/// Each channel that brings an agent content obid's proxy doesn't see, used once in a turn: the
/// vendor's own web search, Tetrix's MCP server, a plug-in's, obid's connected apps. Its check gets
/// nothing.
#[test]
fn content_from_outside_the_proxy_gives_nothing() {
    let (_folder, obid, _) = run_one(
        "secretspec-channels",
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "TOOL: WebSearch", "check": "sh check.sh"}),
    );
    for name in [
        "WebSearch",
        "mcp__tetrix__search_entities",
        "mcp__a-plugin__lookup",
        "mcp__oby__linear_search",
    ] {
        let log = dispatch_and_log(
            &obid,
            json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": format!("TOOL: {name}"), "check": "sh check.sh"}),
        );
        assert!(
            log.contains(&format!(
                "none of its owner's secrets: its agent used {name}"
            )),
            "{name}: {log}"
        );
        holds_none(&log, &fake(), "check.log");
    }
}

/// A job that works from an issue someone else wrote, or one GitHub won't say about: no values.
#[test]
fn an_issue_someone_else_wrote_gives_nothing() {
    let (_folder, _obid, log) = run_one(
        "secretspec-issue",
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "Fix issue 3", "issue": 3, "check": "sh check.sh"}),
    );
    assert!(log.contains("who wrote issue #3"), "{log}");
    holds_none(&log, &fake(), "check.log");
}

/// A job based on a branch obid didn't record anyone of the owner's making (a teammate's) brings
/// that branch's commits into the clone and marks it. The next job gets the clone made again, so
/// its check gets the owner's values.
#[test]
fn a_marked_clone_is_made_again_and_values_come_back() {
    let (_folder, obid, _) = run_one(
        "secretspec-taint",
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "Nothing to do here",
               "base": "feature/average", "check": "true"}),
    );
    let clone = obid.home.join("clones/obi-e2e/fixture-app");
    let marked = git(&clone, &["config", "--get", "obid.foreign"]);
    assert!(marked.contains("the branch feature/average"), "{marked}");
    let log = dispatch_and_log(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "Nothing to do here", "check": "sh check.sh"}),
    );
    assert!(
        log.contains(&format!("sha: {}", fingerprint(&fake()))),
        "{log}"
    );
    assert!(git(&clone, &["config", "--get-all", "obid.narrow"]).contains("true"));
}

/// (a) A review another Oby asked for: its check gets none of the owner's values, though the
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
    // What it fetched is in a clone of its own, which no owner's job's worktree reads.
    assert!(
        home.join("received/clones/obi-e2e/fixture-app/.git")
            .is_dir()
    );
    assert!(!home.join("clones/obi-e2e/fixture-app").exists());

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

    // A job made untrusted keeps to a clone apart: nothing it fetches, commits or keeps lands in
    // the owner's own clone, which other jobs' checks read.
    let untrusted = jobs
        .create(obid::jobs::NewJob {
            repo: "obi-e2e/fixture-app".into(),
            kind: "build".into(),
            task: "Read what someone sent".into(),
            untrusted: true,
            ..Default::default()
        })
        .unwrap();
    assert!(untrusted.apart);
    let prepared = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let Some(job) = jobs.get(&untrusted.id)
                && job.worktree.is_some()
            {
                break job;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let pointer =
        std::fs::read_to_string(PathBuf::from(prepared.worktree.unwrap()).join(".git")).unwrap();
    assert!(
        pointer.contains("/apart/clones/obi-e2e/fixture-app/"),
        "{pointer}"
    );
    assert!(!home.join("clones/obi-e2e/fixture-app").exists());
    jobs.stop(&untrusted.id);
}
