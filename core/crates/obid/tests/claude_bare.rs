//! Claude Code's docs say `--bare` will become the default for `claude -p`, and bare mode never
//! reads OAuth or the Keychain (#1867). Every Oby run on Claude Code is `claude -p` on a plan login,
//! so these hold what obid does about it (docs/contracts/keychain.md, KEYCHAIN-4 to KEYCHAIN-7): on
//! the real obid binary, a stand-in records each launch's real arguments and environment
//! (tests/agents/fake-claude-recording), and another ends as a bare default would
//! (tests/agents/fake-claude-bare-default).

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use support::*;

/// What a test's obid has in its own environment that a launch must never get: `--bare` sets
/// `CLAUDE_CODE_SIMPLE` to 1, and an Anthropic key would hide a bare default from a test.
const OWN_ENV: [(&str, &str); 2] = [
    ("CLAUDE_CODE_SIMPLE", "1"),
    ("ANTHROPIC_API_KEY", "set-by-the-test"),
];

/// How Claude Code ends a turn with no login it may read (tests/agents/fake-claude-bare-default).
const NOT_LOGGED_IN: &str = "Not logged in · Please run /login";

fn agent(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/agents/{name}"))
}

/// One launch of Claude Code by obid, as the stand-in recorded it.
struct Launch {
    args: Vec<String>,
    env: Vec<String>,
}

impl Launch {
    fn has(&self, flag: &str) -> bool {
        self.args.iter().any(|a| a == flag)
    }
}

/// `folder/bin/claude`: `fake-claude-recording` in `standins` in front of the stand-in `next`
/// there, node by its path (a refresh and a sign-in run with only the system's `PATH`).
fn recording_claude(folder: &Path, standins: &Path, next: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let url = planner_state(folder.join("launches"));
    let node = std::process::Command::new("sh")
        .args(["-c", "command -v node"])
        .output()
        .unwrap();
    let node = String::from_utf8(node.stdout).unwrap();
    let bin = folder.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let claude = bin.join("claude");
    std::fs::write(
        &claude,
        format!(
            "#!/bin/sh\nexec '{}' '{}' '{url}' '{}' \"$@\"\n",
            node.trim(),
            standins.join("fake-claude-recording").display(),
            standins.join(next).display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    claude
}

/// Every launch so far, oldest first. (An argument with lines in it, a prompt's, is several here;
/// none of them is a flag obid passes.)
fn launches(folder: &Path) -> Vec<Launch> {
    let calls = folder.join("launches/calls");
    let lines = |n: usize, file: &str| -> Option<Vec<String>> {
        let text = std::fs::read_to_string(calls.join(n.to_string()).join(file)).ok()?;
        Some(text.lines().map(str::to_owned).collect())
    };
    (1..)
        .map_while(|n| {
            Some(Launch {
                args: lines(n, "args")?,
                env: lines(n, "input").unwrap_or_default(),
            })
        })
        .collect()
}

/// A launch that `which` picks is there, and none of them all ran bare or was given what would make
/// it so. (The stand-ins refuse a Claude Code or Anthropic variable too, on most of these paths.)
fn never_bare(what: &str, launches: &[Launch], which: impl Fn(&Launch) -> bool) {
    assert!(
        launches.iter().any(&which),
        "{what}: no such launch of Claude Code was recorded, in {:?}",
        launches.iter().map(flags).collect::<Vec<_>>()
    );
    for launch in launches {
        assert!(
            !launch
                .args
                .iter()
                .any(|a| a == "--bare" || a.starts_with("--bare=")),
            "{what}: obid launched Claude Code with --bare, which never reads a plan login: {:?}",
            flags(launch)
        );
        for name in &launch.env {
            assert!(
                !(name == "CLAUDECODE"
                    || name.starts_with("ANTHROPIC_")
                    || (name.starts_with("CLAUDE_CODE_")
                        && name != "CLAUDE_CODE_TMPDIR"
                        && name != "CLAUDE_CODE_ENABLE_TODO_TOOLS"
                        && name != "CLAUDE_CODE_DISABLE_CLAUDE_MDS"
                        // An own conversation's auto-memory off (LANES-12, process::NO_AUTO_MEMORY).
                        && name != "CLAUDE_CODE_DISABLE_AUTO_MEMORY")),
                "{what}: {name} reached Claude Code"
            );
        }
    }
}

/// A launch's flags, without the prompts' words.
fn flags(launch: &Launch) -> Vec<&str> {
    let flags = launch.args.iter().map(String::as_str);
    flags
        .filter(|a| a.starts_with('-') || *a == "auth" || *a == "/usage")
        .collect()
}

/// A test's configuration on the stand-in `next` behind the recorder, its account signed in on a
/// login far from its end, as a plan login is.
fn signed_in(folder: &Path, next: &str) -> Value {
    let remote = fixture(&folder.join("fixture"));
    let bin = recording_claude(folder, &agent(""), next);
    let mut config = config(folder, &remote, &bin);
    config["conversation"] = json!({"agent": "claude"});
    config["claudeKeychain"] = json!(false);
    let account = folder.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    let ends = (obi_protocol::time::now() + 10 * 86_400) * 1000;
    let login =
        json!({"claudeAiOauth": {"accessToken": "tok-stub-plain-test-login", "expiresAt": ends}});
    std::fs::write(account.join(".credentials.json"), login.to_string()).unwrap();
    config
}

/// A job in the fixture repository, on `account` when one's named, once it ended.
fn job_ended(obid: &Obid, account: Option<&str>) -> Value {
    let mut job =
        json!({"repo": "obi-e2e/fixture-app", "kind": "research", "task": "Look at the average"});
    if let Some(account) = account {
        job["account"] = json!(account);
    }
    let made = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap();
    let ended = call(obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(ended.status, 200, "{}", ended.text);
    ended.body
}

/// What Oby answered the owner's words in a new conversation.
fn said(obid: &Obid, words: &str) -> Value {
    let body = json!({"said": words, "new": true});
    let answer = call(obid, "POST", "/v1/plans", Some(&body));
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

/// The launches once `which` picks one.
fn waiting_for(folder: &Path, what: &str, which: impl Fn(&Launch) -> bool) -> Vec<Launch> {
    within(60, what, || {
        let seen = launches(folder);
        seen.iter().any(&which).then_some(seen)
    })
}

// KEYCHAIN-4: each launch of Claude Code, recorded as it ran.

#[test]
fn issue_1867_a_job_never_runs_bare() {
    let folder = Folder::new("bare-job");
    let config = signed_in(&folder.0, "fake-claude");
    let obid = start_with(&folder.0, &config, None, &OWN_ENV);
    let ended = job_ended(&obid, None);
    assert_eq!(ended["status"], "done", "{ended}");
    never_bare("a job", &launches(&folder.0), |l| l.has("--max-turns"));
}

#[test]
fn issue_1867_a_conversation_turn_never_runs_bare() {
    let folder = Folder::new("bare-turn");
    let config = signed_in(&folder.0, "fake-harness");
    let obid = start_with(&folder.0, &config, None, &OWN_ENV);
    assert_eq!(said(&obid, "SAY hi")["text"], "hi");
    never_bare("a conversation's turn", &launches(&folder.0), |l| {
        l.has("--mcp-config")
    });
}

/// The next conversation's Claude Code, started ahead (`brain::harness::prestart`), adds its own
/// flags apart from a turn's.
#[test]
fn issue_1867_a_conversation_started_ahead_never_runs_bare() {
    let folder = Folder::new("bare-ahead");
    let config = signed_in(&folder.0, "fake-harness");
    let mut env = OWN_ENV.to_vec();
    env.push(("OBID_PRESTART", "on"));
    let obid = start_with(&folder.0, &config, None, &env);
    said(&obid, "SAY hi");
    let seen = within(30, "the next conversation's Claude Code", || {
        let seen = launches(&folder.0);
        (seen.iter().filter(|l| l.has("--mcp-config")).count() > 1).then_some(seen)
    });
    never_bare("a conversation started ahead", &seen, |l| {
        l.has("--mcp-config")
    });
}

/// The brain's side calls (`brain::calls`), asked for by "Find what I keep repeating". Boxed or not
/// (a recap's), a side call's flags are the one list `Quick::complete` builds.
#[test]
fn issue_1867_a_side_call_of_the_brain_never_runs_bare() {
    let folder = Folder::new("bare-brain");
    let config = signed_in(&folder.0, "fake-harness");
    let obid = start_with(&folder.0, &config, None, &OWN_ENV);
    // It reads only with the owner's words in two places.
    said(
        &obid,
        "SAY I keep asking you to end every line with a period",
    );
    said(&obid, "SAY Again, please end every line with a period");
    let found = call(&obid, "POST", "/v1/memory/find", None);
    assert_eq!(found.status, 200, "{}", found.text);
    let seen = waiting_for(&folder.0, "a side call", |l| l.has("--json-schema"));
    never_bare("a side call", &seen, |l| l.has("--json-schema"));
}

/// A review another Oby asked for has flags of its own. Oby's own review of a job's pull request is
/// a job's turn on `Jobs::launch` with a job's flags, as `issue_1867_a_job_never_runs_bare` holds.
#[tokio::test(flavor = "multi_thread")]
async fn issue_1867_a_review_never_runs_bare() {
    let folder = Folder::new("bare-review");
    let mut config = signed_in(&folder.0, "fake-claude");
    // In this test's own process, whose `PATH` doesn't show the box this checkout (under the
    // person's home or a shared temporary folder, which a box hides: #2064): the stand-ins beside
    // the agent's binary, which it reads (`sandbox::install_root`).
    let standins = folder.0.join("bin");
    std::fs::create_dir_all(&standins).unwrap();
    for name in ["fake-claude", "fake-claude-recording"] {
        std::fs::copy(agent(name), standins.join(name)).unwrap();
    }
    config["agents"]["claude"]["bin"] =
        json!(recording_claude(&folder.0, &standins, "fake-claude"));
    let config = obid::config::Config::parse(&config.to_string()).unwrap();
    obid::prepare_home(&config).unwrap();
    let jobs = obid::jobs::Jobs::open(
        config,
        obid::events::Events::new(),
        obid::sandbox::Sandbox::detect(Path::new(OBID)),
    )
    .unwrap();
    let job = jobs
        .start_obi_review(obid::jobs::ObiReview {
            correlation_id: "5f0e8c1a-3b2d-4e6f-8a9b-0c1d2e3f4a5b".into(),
            request_id: "5f0e8c1a-3b2d-4e6f-8a9b-0c1d2e3f4a5b".into(),
            from: "obi-ernesto".into(),
            from_owner: "ernesto".into(),
            body: brief(),
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(120), jobs.wait(&job.id))
        .await
        .unwrap()
        .unwrap();
    never_bare("a review", &launches(&folder.0), |l| l.has("--restricted"));
}

#[test]
fn issue_1867_the_usage_read_never_runs_bare() {
    let folder = Folder::new("bare-usage");
    let config = signed_in(&folder.0, "fake-claude");
    let mut env = OWN_ENV.to_vec();
    env.push(("OBID_USAGE_READS", "on"));
    let obid = start_with(&folder.0, &config, None, &env);
    call(&obid, "GET", "/v1/status", None);
    let seen = waiting_for(&folder.0, "the usage read", |l| l.has("/usage"));
    never_bare("the usage read", &seen, |l| l.has("/usage"));
}

/// Claude Code's sign-in and the look at who signed in (`claude auth status`,
/// `accounts::read_status`, which also runs as obid starts).
#[test]
fn issue_1867_a_sign_in_and_the_login_check_never_run_bare() {
    let folder = Folder::new("bare-sign-in");
    let mut config = signed_in(&folder.0, "fake-claude");
    if cfg!(target_os = "macos") {
        config["claudeKeychain"] = json!(true);
    }
    let obid = start_with(&folder.0, &config, None, &OWN_ENV);
    let added = call(
        &obid,
        "POST",
        "/v1/accounts",
        Some(&json!({"agent": "claude", "name": "Personal"})),
    );
    assert_eq!(added.status, 201, "{}", added.text);
    let target = format!(
        "/v1/accounts/{}/sign-in",
        added.body["id"].as_str().unwrap()
    );
    let started = call(&obid, "GET", &target, None);
    assert_eq!(started.status, 200, "{}", started.text);
    let code = json!({"code": "good-code#fake"});
    let signed = call(&obid, "POST", &target, Some(&code));
    assert_eq!(signed.status, 200, "{}", signed.text);
    let seen = waiting_for(&folder.0, "the login check", |l| l.has("status"));
    never_bare("a sign-in", &seen, |l| l.has("login"));
    never_bare("the login check", &seen, |l| l.has("status"));
}

/// The Mac's own Claude Code renewing a login near its end (`jobs::logins::refresh_command`): it
/// runs `-p /usage` unboxed, and a bare one wouldn't read the login it's there to renew. Only a
/// boxed run's copy is refreshed by obid (KEYCHAIN-10): an own run reads the account's own login,
/// which Claude Code refreshes itself (LANES-1), so the job here is an untrusted one, in Oby's box.
#[cfg(target_os = "macos")]
#[test]
fn issue_1867_the_refresh_never_runs_bare() {
    let folder = Folder::new("bare-refresh");
    let mut config = signed_in(&folder.0, "fake-claude");
    config["claudeKeychain"] = json!(true);
    let mut obid = start_with(&folder.0, &config, None, &OWN_ENV);
    let added = call(
        &obid,
        "POST",
        "/v1/accounts",
        Some(&json!({"agent": "claude", "name": "Second"})),
    );
    assert_eq!(added.status, 201, "{}", added.text);
    let id = added.body["id"].as_str().unwrap();
    // Its Keychain item, where the stand-in `security` finds it, ending in a minute.
    let dir = obid.home.join("accounts").join(id);
    let digest = obi_protocol::keys::sha256(dir.as_os_str().as_encoded_bytes());
    let item = folder.0.join(format!(
        "tmp/obid-test-keychain/Claude Code-credentials-{}",
        &hex::encode(digest)[..8]
    ));
    std::fs::create_dir_all(item.parent().unwrap()).unwrap();
    let ends = (obi_protocol::time::now() + 60) * 1000;
    let refresh = ["refresh", "plain", "test", "value"].join("-");
    let login = json!({"claudeAiOauth": {"accessToken": "tok-old-plain-test-login",
                                         "refreshToken": refresh, "expiresAt": ends}});
    std::fs::write(&item, login.to_string()).unwrap();
    // Written while obid is down, it starts as obid does.
    assert!(obid.stop().success());
    let mut job = obid::job::Job::new(
        "aaaa1867".into(),
        "obi-e2e/fixture-app".into(),
        "research".into(),
        "Look at the average".into(),
        "claude".into(),
        id.into(),
    );
    job.branch = "oby/research-aaaa1867".into();
    job.untrusted = true;
    let text = serde_json::to_vec(&job).unwrap();
    std::fs::create_dir_all(obid.home.join("jobs")).unwrap();
    std::fs::write(obid.home.join("jobs/aaaa1867.json"), text).unwrap();
    let obid = start_with(&folder.0, &config, None, &OWN_ENV);
    let ended = call(&obid, "GET", "/v1/jobs/aaaa1867/wait", None);
    assert_eq!(ended.status, 200, "{}", ended.text);
    assert!(std::fs::read_to_string(&item).unwrap().contains("renewed"));
    never_bare("the refresh", &launches(&folder.0), |l| {
        l.env
            .iter()
            .any(|name| name == "CLAUDE_SECURESTORAGE_CONFIG_DIR")
    });
}

/// The model that writes an answer to another Oby's question by itself (`sphere::answering`): a
/// launch of its own, which #1867 doesn't list.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn issue_1867_an_answer_to_another_oby_never_runs_bare() {
    let folder = Folder::new("bare-answer");
    let sphere = FakeSphere::start().await;
    let config = signed_in(&folder.0, "fake-answerer");
    let obid = start_with(&folder.0, &config, None, &OWN_ENV);
    tokio::task::block_in_place(|| {
        let accepts = ["review_request", "question"];
        join_a_accepting(&obid, &sphere, &Owner::new("wellington"), &accepts);
        job_ended(&obid, None);
        let rule =
            json!({"for": ["ernesto"], "repos": ["obi-e2e/fixture-app"], "tells": ["jobStatus"]});
        let made = call_signed(&obid, "POST", "/v1/obid/sharing/rules", Some(&rule));
        assert_eq!(made.status, 201, "{}", made.text);
    });
    let ernesto = TestObi::join_accepting(
        &sphere,
        "obi-ernesto",
        "ernesto",
        &["review_request", "question"],
    )
    .await;
    let about = json!([{"kind": "repo", "repo": "obi-e2e/fixture-app"}]);
    let question = ernesto.request(
        "obi-wellington",
        "question",
        json!({"text": "Is the average fix landing Thursday?", "about": about}),
        3600,
    );
    ernesto.send(&question).await;
    let seen = tokio::task::block_in_place(|| {
        waiting_for(&folder.0, "an answer's launch", |l| {
            l.has("--permission-prompts")
        })
    });
    never_bare("an answer", &seen, |l| l.has("--permission-prompts"));
}

/// KEYCHAIN-10 (docs/contracts/keychain.md): with no keeper, the answer to another Oby's question
/// runs boxed on a copy of the account's Keychain login, so a login near its end is refreshed by
/// the Mac's own Claude Code before that copy is made.
#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn keychain_10_an_answer_to_another_oby_refreshes_its_login_first() {
    let folder = Folder::new("keychain-10-answer");
    let sphere = FakeSphere::start().await;
    let mut config = signed_in(&folder.0, "fake-answerer");
    config["claudeKeychain"] = json!(true);
    let obid = start_with(&folder.0, &config, None, &OWN_ENV);
    tokio::task::block_in_place(|| {
        let accepts = ["review_request", "question"];
        join_a_accepting(&obid, &sphere, &Owner::new("wellington"), &accepts);
        let rule =
            json!({"for": ["ernesto"], "repos": ["obi-e2e/fixture-app"], "tells": ["jobStatus"]});
        let made = call_signed(&obid, "POST", "/v1/obid/sharing/rules", Some(&rule));
        assert_eq!(made.status, 201, "{}", made.text);
    });
    // From now on the account's login is only its Keychain item (where the stand-in `security`
    // finds it), ending in a minute.
    let dir = obid.home.join("accounts/stub");
    std::fs::remove_file(dir.join(".credentials.json")).unwrap();
    let digest = obi_protocol::keys::sha256(dir.as_os_str().as_encoded_bytes());
    let item = folder.0.join(format!(
        "tmp/obid-test-keychain/Claude Code-credentials-{}",
        &hex::encode(digest)[..8]
    ));
    std::fs::create_dir_all(item.parent().unwrap()).unwrap();
    let ends = (obi_protocol::time::now() + 60) * 1000;
    let login =
        json!({"claudeAiOauth": {"accessToken": "tok-old-plain-test-login", "expiresAt": ends}});
    std::fs::write(&item, login.to_string()).unwrap();
    let ernesto = TestObi::join_accepting(
        &sphere,
        "obi-ernesto",
        "ernesto",
        &["review_request", "question"],
    )
    .await;
    let about = json!([{"kind": "repo", "repo": "obi-e2e/fixture-app"}]);
    let question = ernesto.request(
        "obi-wellington",
        "question",
        json!({"text": "Is the average fix landing Thursday?", "about": about}),
        3600,
    );
    ernesto.send(&question).await;
    tokio::task::block_in_place(|| {
        waiting_for(&folder.0, "the refresh before the answer", |l| {
            l.has("/usage")
                && l.env
                    .iter()
                    .any(|name| name == "CLAUDE_SECURESTORAGE_CONFIG_DIR")
        })
    });
}

/// The launches that ran a model's turn, a little later: nothing launches it again by itself.
fn turns_after_a_while(folder: &Path) -> usize {
    std::thread::sleep(Duration::from_secs(3));
    launches(folder).iter().filter(|l| l.has("-p")).count()
}

/// The stand-in's account stays signed in: obid doesn't read this as a signed-out account.
fn still_signed_in(obid: &Obid) {
    let accounts = call(obid, "GET", "/v1/status", None).body;
    assert_eq!(accounts["accounts"][0]["signedIn"], true, "{accounts}");
}

/// KEYCHAIN-5, what obid does today the day `-p` runs bare by default (#1867's gap, not its fix):
/// a job on a plan login stops after one launch, with no round or retry, and says Claude Code's
/// own words, which tell the owner to run a `/login` that wouldn't help. Its next step is "Try
/// again", and nothing says Claude Code changed.
#[test]
fn issue_1867_a_bare_default_stops_a_job_in_claude_codes_words() {
    let folder = Folder::new("bare-default-job");
    let config = signed_in(&folder.0, "fake-claude-bare-default");
    let obid = start(&folder.0, &config, None);
    let ended = job_ended(&obid, None);
    assert_eq!(ended["status"], "interrupted", "{ended}");
    assert_eq!(ended["summary"], NOT_LOGGED_IN, "{ended}");
    assert_eq!(ended["reason"], NOT_LOGGED_IN, "{ended}");
    assert_eq!(ended["nextStep"], "Try again", "{ended}");
    assert_eq!(ended["resumesAt"], Value::Null, "{ended}");
    assert_eq!(turns_after_a_while(&folder.0), 1);
    still_signed_in(&obid);
}

/// KEYCHAIN-6, the same for a conversation's turn: tried twice, as any session that fails is, then
/// an answer that says it failed and not why. Claude Code's words go only to obid's log. So the
/// owner isn't told to sign in again, nor anything they could act on (#1867's gap, not its fix).
#[test]
fn issue_1867_a_bare_default_fails_a_turn_without_saying_why() {
    let folder = Folder::new("bare-default-turn");
    let config = signed_in(&folder.0, "fake-claude-bare-default");
    let obid = start(&folder.0, &config, None);
    let answer = said(&obid, "SAY hi");
    assert_eq!(
        answer["text"], "I couldn't answer: its agent session failed twice.",
        "{answer}"
    );
    assert_eq!(turns_after_a_while(&folder.0), 2);
    assert!(obid.log().contains(NOT_LOGGED_IN), "{}", obid.log());
    still_signed_in(&obid);
}

/// The early warning (KEYCHAIN-7), against the real Claude Code on this Mac's own plan login: one
/// tiny turn on haiku, spending a little of the owner's plan, with the flags of a job's turn
/// (`jobs.rs`, `run_owner_job`) and a job's environment. It never runs in the ordinary suite. Run it
/// after every Claude Code update, on a Mac signed in with a plan and no `ANTHROPIC_API_KEY`:
///
/// `cargo test -p obid --test claude_bare issue_1867_the_real_claude -- --ignored --nocapture`
#[test]
#[ignore = "spends a real turn on this Mac's plan login: run it after a Claude Code update"]
fn issue_1867_the_real_claude_code_still_takes_a_plan_login() {
    let bin = obid::process::resolve_bin("claude").expect("Claude Code isn't installed here");
    let folder = Folder::new("real-claude");
    let mut child = std::process::Command::new(&bin)
        .args(["-p", "--input-format", "stream-json"])
        .args(["--output-format", "stream-json", "--verbose"])
        .args(["--include-partial-messages"])
        .args(["--permission-prompt-tool", "stdio"])
        .args(["--permission-mode", "default"])
        .args(["--setting-sources", "", "--strict-mcp-config"])
        .args(["--max-turns", "1", "--model", "haiku", "--tools", ""])
        .current_dir(&folder.0)
        .env_clear()
        .envs(obid::process::kept(|name| std::env::var_os(name)))
        .env("HOME", std::env::var_os("HOME").expect("HOME"))
        .env("USER", std::env::var_os("USER").expect("USER"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let message = obid::agent::user_message("Reply with the word ok, nothing else.");
    let mut stdin = child.stdin.take().unwrap();
    std::io::Write::write_all(&mut stdin, format!("{message}\n").as_bytes()).unwrap();
    drop(stdin);
    let ran = child.wait_with_output().unwrap();
    let out = String::from_utf8_lossy(&ran.stdout);
    let result = out
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["type"] == "result");
    let Some(result) = result else {
        panic!(
            "Claude Code ended with no result ({}): {}",
            ran.status,
            String::from_utf8_lossy(&ran.stderr)
        );
    };
    let words = result["result"].as_str().unwrap_or_default();
    let refused = out.contains("authentication_failed") || words.contains("Not logged in");
    assert!(
        !refused,
        "Claude Code refused this Mac's plan login with obid's flags ({words}). If `claude -p` now \
         runs bare by default, every Oby run on Claude Code fails like this: see #1867."
    );
    assert_eq!(
        result["is_error"], false,
        "Claude Code took the login, but its turn failed for another reason: {words}"
    );
}
