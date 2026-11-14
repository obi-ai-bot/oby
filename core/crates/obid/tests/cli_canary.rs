//! The CLI canary (docs/contracts/ci.md, CI-14 to 16; docs/ci.md, "The CLI canary"): the newest
//! Claude Code and Codex, which .github/workflows/cli-canary.yml installs every 6 hours, run through
//! the real obid binary's adapters against fakes on 127.0.0.1, with no login and no secret. Each
//! case drives the job through obid's API only.
//!
//! - Claude Code (`CLI_CANARY_CLAUDE`) runs in the own lane, its environment the owner's login
//!   shell's: a stand-in shell here gives it `ANTHROPIC_BASE_URL`, the fake model server's
//!   (tests/agents/fake-model-server), and the key `dummy`.
//! - Codex (`CLI_CANARY_CODEX`) runs boxed on a ChatGPT login, through obid's relay, to the stand-in
//!   OpenAI the ChatGPT tests use (tests/support/stand_in_openai.rs).
//!
//! The adapter cases are ignored: `cargo test -p obid --test cli_canary -- --include-ignored`
//! runs them, each `skipped` when its CLI isn't named. The report and the two-in-a-row rule
//! (scripts/cli-canary.mjs) are tested here too, on a stand-in `gh`, and those tests always run.

#[allow(dead_code)]
#[path = "support/stand_in_openai.rs"]
mod stand_in_openai;
mod support;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use stand_in_openai::*;
use support::*;

/// Every CLI the canary reports and its cases, by the names its result files and its report use:
/// the adapter cases below, and cursor-agent's signed-out check (adapters.rs). Written once:
/// scripts/cli-canary.mjs reads this list from here, and an issue names nothing else.
const CASES: &[(&str, &[&str])] = &[
    (
        "claude-code",
        &["start", "permission_ask", "stop", "resume"],
    ),
    (
        "codex",
        &[
            "start",
            "permission_ask",
            "stop",
            "resume",
            "rate_limits_updated",
        ],
    ),
    ("cursor-agent", &["signed_out"]),
];

const CLAUDE: &str = "CLI_CANARY_CLAUDE";
const CODEX: &str = "CLI_CANARY_CODEX";
/// How long a real CLI may take to end a job or reach the model: a cold start on a busy runner
/// took nearly two minutes in R01-10's runs.
const WAIT: u64 = 300;
/// What the fake model server says.
const WORDS: &str = "The canary's fake model answered.";
const TRY_AGAIN: &str =
    "Try again, picking up where you left off. Finish with a short summary as asked.";

fn agents() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents")
}

/// An environment variable set to something.
fn set(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Runs case `name` of `cli` on the binary `var` names, the one wrapper every case runs in: its
/// result goes to `CLI_CANARY_RESULTS/<cli>/<name>.json` when that's set. `skipped` (and passes)
/// with `CLI_CANARY_SKIP` set, its text the why, or with `var` unset (`not installed`); `failed`
/// on a panic, which goes on, so the test still fails; `CLI_CANARY_FORCE_FAIL=<cli>/<name>` fails
/// it on purpose (`forced`).
fn case(cli: &str, name: &str, var: &str, body: impl FnOnce(&Path)) {
    assert!(
        CASES.iter().any(|(c, n)| *c == cli && n.contains(&name)),
        "{cli}/{name} isn't in CASES"
    );
    let record = |result: &str, why: &str| {
        let Some(results) = set("CLI_CANARY_RESULTS") else {
            return;
        };
        let dir = Path::new(&results).join(cli);
        std::fs::create_dir_all(&dir).unwrap();
        let why: String = why.lines().next().unwrap_or("").chars().take(200).collect();
        let said = json!({"result": result, "why": why});
        std::fs::write(dir.join(format!("{name}.json")), said.to_string()).unwrap();
    };
    if let Some(why) = set("CLI_CANARY_SKIP") {
        return record("skipped", &why);
    }
    let Some(bin) = set(var) else {
        return record("skipped", "not installed");
    };
    let forced = set("CLI_CANARY_FORCE_FAIL").is_some_and(|f| f == format!("{cli}/{name}"));
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(!forced, "forced");
        let bin = Path::new(&bin);
        assert!(bin.is_absolute(), "{var} isn't an absolute path");
        body(bin);
    }));
    match ran {
        Ok(()) => record("passed", ""),
        Err(panic) => {
            let why = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("it panicked");
            record("failed", why);
            std::panic::resume_unwind(panic);
        }
    }
}

/// A job of the fixture repository's, with `job` over the defaults, its id. Signed as the
/// owner's own app sends it, so `mode` takes.
fn dispatch(obid: &Obid, job: Value) -> String {
    let mut whole = json!({"repo": "obi-e2e/fixture-app", "kind": "research", "check": "true"});
    for (key, value) in job.as_object().unwrap() {
        whole[key] = value.clone();
    }
    // (an account just signed in is held for a moment)
    let posted = within(30, "the job was taken", || {
        let posted = call_typed(obid, "POST", "/v1/jobs", &json!({ "job": whole }));
        (posted.status != 422).then_some(posted)
    });
    assert_eq!(posted.status, 201, "{}", posted.text);
    posted.body["id"].as_str().unwrap().to_owned()
}

fn job_now(obid: &Obid, id: &str) -> Value {
    call(obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

fn summary(job: &Value) -> &str {
    job["summary"].as_str().unwrap_or("")
}

/// What went on in job `id`, for a failure's message: its agent's log and obid's, their ends.
fn story(obid: &Obid, id: &str) -> String {
    let tail = |text: String| {
        let start = text.len().saturating_sub(3000);
        let start = (start..text.len())
            .find(|i| text.is_char_boundary(*i))
            .unwrap_or(text.len());
        text[start..].to_owned()
    };
    let agent = std::fs::read_to_string(obid.home.join("jobs").join(id).join("agent.log"));
    format!(
        "\n--- agent.log\n{}\n--- obid\n{}",
        tail(agent.unwrap_or_default()),
        tail(obid.log())
    )
}

/// The job once it asks the owner, failing when it ends without asking.
fn asked(obid: &Obid, id: &str) -> Value {
    let job = within(WAIT, "the job asks, or ends", || {
        let job = job_now(obid, id);
        let ended = !["queued", "running"].contains(&job["status"].as_str().unwrap_or_default());
        (job["permission"]["id"].is_string() || ended).then_some(job)
    });
    assert!(
        job["permission"]["id"].is_string(),
        "it ended without asking: {job}{}",
        story(obid, id)
    );
    job
}

/// The owner's Allow on the job's permission, signed.
fn allow(obid: &Obid, id: &str, job: &Value) {
    let allowed = call_signed(
        obid,
        "POST",
        &format!("/v1/jobs/{id}/permission"),
        Some(&json!({"id": job["permission"]["id"], "decision": "allow"})),
    );
    assert_eq!(allowed.status, 200, "{}", allowed.text);
}

/// Stops job `id` through the API: it's `stopped` within a bounded wait.
fn stop(obid: &Obid, id: &str) {
    let stopped = call(obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    assert_eq!(stopped.status, 200, "{}", stopped.text);
    within(30, "the job is stopped", || {
        (job_now(obid, id)["status"] == "stopped").then_some(())
    });
}

/// The Try-again words to a stopped job: its session picked up again in a new process.
fn try_again(obid: &Obid, id: &str) {
    let again = call(
        obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(again.status, 200, "{}", again.text);
}

// ---------------------------------------------------------------------------------------------
// Claude Code, against the fake model server

/// tests/agents/fake-model-server on a free port, logging every request to a file of the test's.
struct Model {
    child: Child,
    port: u16,
    log: PathBuf,
}

impl Drop for Model {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Model {
    fn start(folder: &Path) -> Model {
        let log = folder.join("model.log");
        let mut child = Command::new(agents().join("fake-model-server"))
            .arg(&log)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port = line.trim().parse().expect("the fake model server's port");
        Model { child, port, log }
    }

    /// Every request it got, as it logged them.
    fn requests(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

/// obid with the real Claude Code on the fixture repository; dropped in this order.
struct Claude {
    obid: Obid,
    model: Model,
    _folder: Folder,
}

fn claude(name: &str, bin: &Path) -> Claude {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new(&format!("canary-claude-{name}"));
    let model = Model::start(&folder.0);
    // The owner's login shell, as obid runs it (`<shell> -l -c '/usr/bin/env -0'`): Claude Code's
    // gateway is the fake, with a key that is no key.
    let shell = folder.0.join("login-shell");
    std::fs::write(
        &shell,
        format!(
            "#!/bin/sh\nexport ANTHROPIC_BASE_URL=http://127.0.0.1:{} ANTHROPIC_API_KEY=dummy\n\
             [ \"$1 $2\" = '-l -c' ] && exec /bin/sh -c \"$3\"\n",
            model.port
        ),
    )
    .unwrap();
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, bin);
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_LOGIN_SHELL", shell.to_str().unwrap())],
    );
    Claude {
        obid,
        model,
        _folder: folder,
    }
}

/// A Claude Code job told `task`, with `more` over it.
fn claude_job(world: &Claude, task: &str, more: Value) -> String {
    let mut job = json!({"agent": "claude", "task": task});
    for (key, value) in more.as_object().unwrap() {
        job[key] = value.clone();
    }
    dispatch(&world.obid, job)
}

/// A Claude Code job whose model holds its answer, stopped while it does: its id.
fn claude_stopped(world: &Claude) -> String {
    let id = claude_job(world, "canary:hold Wait for the canary model.", json!({}));
    within(WAIT, "the model holds its answer", || {
        let held = world.model.requests().iter().any(|r| r["answer"] == "hold");
        let ended = !["queued", "running"].contains(
            &job_now(&world.obid, &id)["status"]
                .as_str()
                .unwrap_or_default(),
        );
        assert!(!ended, "it ended first{}", story(&world.obid, &id));
        held.then_some(())
    });
    stop(&world.obid, &id);
    id
}

#[test]
#[ignore = "needs the real Claude Code: CLI_CANARY_CLAUDE=<binary>"]
fn canary_claude_code_start() {
    case("claude-code", "start", CLAUDE, |bin| {
        let world = claude("start", bin);
        let id = claude_job(&world, "Say hello to the canary.", json!({}));
        let job = ended_within(&world.obid, &id, WAIT);
        let said = story(&world.obid, &id);
        assert_eq!(job["status"], "done", "{job}{said}");
        assert!(summary(&job).contains(WORDS), "{job}{said}");
        let requests = world.model.requests();
        assert!(
            requests
                .iter()
                .any(|r| r["method"] == "POST" && r["path"] == "/v1/messages"),
            "{requests:?}"
        );
    });
}

#[test]
#[ignore = "needs the real Claude Code: CLI_CANARY_CLAUDE=<binary>"]
fn canary_claude_code_permission_ask() {
    case("claude-code", "permission_ask", CLAUDE, |bin| {
        let world = claude("permission-ask", bin);
        let id = claude_job(
            &world,
            "canary:tool Make the canary's file.",
            json!({"mode": "ask"}),
        );
        let job = asked(&world.obid, &id);
        assert!(
            job["permission"].to_string().contains("canary-made-this"),
            "{job}"
        );
        allow(&world.obid, &id, &job);
        let job = ended_within(&world.obid, &id, WAIT);
        let made = world
            .obid
            .home
            .join("worktrees")
            .join(&id)
            .join("canary-made-this")
            .exists();
        let answered = world
            .model
            .requests()
            .iter()
            .any(|r| r["toolResult"] == true);
        assert!(
            made || answered,
            "no file and no tool_result: {job}{}",
            story(&world.obid, &id)
        );
    });
}

#[test]
#[ignore = "needs the real Claude Code: CLI_CANARY_CLAUDE=<binary>"]
fn canary_claude_code_stop() {
    case("claude-code", "stop", CLAUDE, |bin| {
        let world = claude("stop", bin);
        claude_stopped(&world);
    });
}

#[test]
#[ignore = "needs the real Claude Code: CLI_CANARY_CLAUDE=<binary>"]
fn canary_claude_code_resume() {
    case("claude-code", "resume", CLAUDE, |bin| {
        let world = claude("resume", bin);
        let id = claude_stopped(&world);
        let before = world.model.requests().len();
        try_again(&world.obid, &id);
        within(
            WAIT,
            "the new process's request carries the first task",
            || {
                world.model.requests()[before..]
                    .iter()
                    .any(|r| {
                        let text = r["text"].as_str().unwrap_or_default();
                        text.contains("canary:hold") && text.contains("Try again")
                    })
                    .then_some(())
            },
        );
        // (its model may hold this answer too)
        let _ = call(&world.obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    });
}

// ---------------------------------------------------------------------------------------------
// Codex, through obid's ChatGPT relay to the stand-in OpenAI

/// obid with the real Codex on a ChatGPT account signed in to the stand-in; dropped in this order.
struct Codex {
    obid: Obid,
    openai: OpenAi,
    account: String,
    _folder: Folder,
}

fn codex(name: &str, bin: &Path) -> Codex {
    let folder = Folder::new(&format!("canary-codex-{name}"));
    let openai = OpenAi::start();
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-claude"));
    config["agents"]["codex"] = json!({"bin": bin, "accounts": []});
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_TEST_CHATGPT_ORIGIN", &openai.origin)],
    );
    let account = signed_in(&obid, &openai);
    Codex {
        obid,
        openai,
        account,
        _folder: folder,
    }
}

fn account(obid: &Obid, id: &str) -> Option<Value> {
    call(obid, "GET", "/v1/status", None).body["accounts"]
        .as_array()?
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
}

/// A new Codex account, signed in with ChatGPT at the stand-in, as chatgpt_api.rs signs one in:
/// its id.
fn signed_in(obid: &Obid, openai: &OpenAi) -> String {
    let added = call(
        obid,
        "POST",
        "/v1/accounts",
        Some(&json!({"agent": "codex"})),
    );
    assert_eq!(added.status, 201, "{}", added.text);
    let id = added.body["id"].as_str().unwrap().to_owned();
    let page = within(10, "the sign-in started", || {
        let page = call(
            obid,
            "GET",
            &format!("/v1/accounts/{id}/sign-in?with=chatgpt"),
            None,
        );
        (page.status != 409).then_some(page)
    });
    assert_eq!(page.status, 200, "{}", page.text);
    let back = openai.consent(page.body["url"].as_str().unwrap());
    let (status, _, _) = http("GET", &back, &[], None);
    assert_eq!(status, 200);
    within(15, "the account is signed in", || {
        (account(obid, &id)?["signedIn"] == true).then_some(())
    });
    id
}

fn codex_job(world: &Codex, task: &str, more: Value) -> String {
    let mut job = json!({"agent": "codex", "account": world.account, "task": task});
    for (key, value) in more.as_object().unwrap() {
        job[key] = value.clone();
    }
    dispatch(&world.obid, job)
}

/// A Codex job whose model holds its answer, stopped while it does: its id.
fn codex_stopped(world: &Codex) -> String {
    world
        .openai
        .with(|s| s.response_holds.push_back(Duration::from_secs(90)));
    let id = codex_job(world, "Wait for the canary model.", json!({}));
    within(WAIT, "the model holds its answer", || {
        let ended = !["queued", "running"].contains(
            &job_now(&world.obid, &id)["status"]
                .as_str()
                .unwrap_or_default(),
        );
        assert!(!ended, "it ended first{}", story(&world.obid, &id));
        world
            .openai
            .with(|s| s.response_holds.is_empty())
            .then_some(())
    });
    stop(&world.obid, &id);
    id
}

#[test]
#[ignore = "needs the real Codex: CLI_CANARY_CODEX=<binary>"]
fn canary_codex_start() {
    case("codex", "start", CODEX, |bin| {
        let world = codex("start", bin);
        let id = codex_job(&world, "Say hello to the canary.", json!({}));
        let job = ended_within(&world.obid, &id, WAIT);
        let said = story(&world.obid, &id);
        assert_eq!(job["status"], "done", "{job}{said}");
        assert!(summary(&job).contains("From the stand-in."), "{job}{said}");
        let bodies = world.openai.with(|s| s.bodies.clone());
        assert!(
            bodies
                .iter()
                .any(|b| b.contains("Say hello to the canary.")),
            "{bodies:?}"
        );
    });
}

#[test]
#[ignore = "needs the real Codex: CLI_CANARY_CODEX=<binary>"]
fn canary_codex_permission_ask() {
    case("codex", "permission_ask", CODEX, |bin| {
        let world = codex("permission-ask", bin);
        let touch = json!({"cmd": "touch canary-made-this"}).to_string();
        world.openai.with(|s| {
            s.responses.push_back(Reply::Sse(vec![
                json!({"type": "response.created", "response": {"id": "r1"}}),
                json!({"type": "response.output_item.done", "item": {"type": "function_call",
                    "id": "fc", "call_id": "c1", "name": "exec_command", "arguments": touch}}),
                json!({"type": "response.completed", "response": {"id": "r1", "usage": {
                    "input_tokens": 1, "output_tokens": 1, "total_tokens": 2,
                    "input_tokens_details": {"cached_tokens": 0},
                    "output_tokens_details": {"reasoning_tokens": 0}}}}),
            ]));
            s.responses.push_back(Reply::Sse(said("Made it.")));
        });
        let id = codex_job(&world, "Make the canary's file.", json!({"mode": "ask"}));
        let job = asked(&world.obid, &id);
        assert!(
            job["permission"].to_string().contains("canary-made-this"),
            "{job}"
        );
        allow(&world.obid, &id, &job);
        let job = ended_within(&world.obid, &id, WAIT);
        let made = world
            .obid
            .home
            .join("worktrees")
            .join(&id)
            .join("canary-made-this")
            .exists();
        let answered = world
            .openai
            .with(|s| s.bodies.iter().any(|b| b.contains("function_call_output")));
        assert!(
            made || answered,
            "no file and no function_call_output: {job}{}",
            story(&world.obid, &id)
        );
    });
}

#[test]
#[ignore = "needs the real Codex: CLI_CANARY_CODEX=<binary>"]
fn canary_codex_stop() {
    case("codex", "stop", CODEX, |bin| {
        let world = codex("stop", bin);
        codex_stopped(&world);
    });
}

#[test]
#[ignore = "needs the real Codex: CLI_CANARY_CODEX=<binary>"]
fn canary_codex_resume() {
    case("codex", "resume", CODEX, |bin| {
        let world = codex("resume", bin);
        let id = codex_stopped(&world);
        try_again(&world.obid, &id);
        within(
            WAIT,
            "the new process's request carries the first task",
            || {
                world
                    .openai
                    .with(|s| {
                        s.bodies.iter().any(|b| {
                            b.contains("Wait for the canary model.") && b.contains("Try again")
                        })
                    })
                    .then_some(())
            },
        );
        let _ = call(&world.obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    });
}

#[test]
#[ignore = "needs the real Codex: CLI_CANARY_CODEX=<binary>"]
fn canary_codex_rate_limits_updated() {
    case("codex", "rate_limits_updated", CODEX, |bin| {
        let world = codex("rate-limits", bin);
        let now = obi_protocol::time::now();
        let headers = [
            ("x-codex-primary-used-percent", "42".to_owned()),
            ("x-codex-primary-window-minutes", "300".to_owned()),
            ("x-codex-primary-reset-at", (now + 3600).to_string()),
            ("x-codex-primary-reset-after-seconds", "3600".to_owned()),
            ("x-codex-secondary-used-percent", "10".to_owned()),
            ("x-codex-secondary-window-minutes", "10080".to_owned()),
            ("x-codex-secondary-reset-at", (now + 86400).to_string()),
            ("x-codex-secondary-reset-after-seconds", "86400".to_owned()),
        ];
        world.openai.with(|s| {
            s.response_headers.push_back(
                headers
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), value.clone()))
                    .collect(),
            )
        });
        let id = codex_job(&world, "Say hello to the canary.", json!({}));
        let job = ended_within(&world.obid, &id, WAIT);
        assert_eq!(job["status"], "done", "{job}{}", story(&world.obid, &id));
        let five_hour = within(30, "the account's five-hour window is 42%", || {
            let share = account(&world.obid, &world.account)?["fiveHour"].as_f64()?;
            ((share - 0.42).abs() < 1e-6).then_some(share)
        });
        assert!((five_hour - 0.42).abs() < 1e-6);
    });
}

// ---------------------------------------------------------------------------------------------
// The report and the two-in-a-row rule: scripts/cli-canary.mjs, on a stand-in `gh`

/// A stand-in `gh` in `dir`: it logs each call's arguments, and answers `issue list` from a file.
struct Gh {
    bin: PathBuf,
    log: PathBuf,
    issues: PathBuf,
}

impl Gh {
    fn new(dir: &Path) -> Gh {
        use std::os::unix::fs::PermissionsExt;
        let (bin, log, issues) = (dir.join("gh"), dir.join("gh.log"), dir.join("issues.json"));
        std::fs::write(
            &bin,
            format!(
                "#!/usr/bin/env node\nconst fs = require(\"node:fs\");\nconst args = process.argv.slice(2);\n\
                 fs.appendFileSync({log:?}, JSON.stringify(args) + \"\\n\");\n\
                 if (args[0] === \"issue\" && args[1] === \"list\") process.stdout.write(fs.readFileSync({issues:?}, \"utf8\"));\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(&issues, "[]").unwrap();
        Gh { bin, log, issues }
    }

    fn open(&self, issues: Value) {
        std::fs::write(&self.issues, issues.to_string()).unwrap();
    }

    /// Every call so far, its arguments.
    fn calls(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// The calls that write: every one but `issue list`.
    fn writes(&self) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|c| c.first().map(String::as_str) != Some("issue") || c[1] != "list")
            .collect()
    }

    fn clear(&self) {
        let _ = std::fs::remove_file(&self.log);
    }
}

/// `node scripts/cli-canary.mjs <args>`, with `gh` the stand-in: what it printed, once it passed.
fn canary_script(gh: &Gh, args: &[&str]) -> String {
    let ran = Command::new("node")
        .arg(obi().join("scripts/cli-canary.mjs"))
        .args(args)
        .env("GH", &gh.bin)
        .env("GITHUB_SERVER_URL", "https://github.com")
        .env_remove("CLI_CANARY_SKIP")
        .output()
        .unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    assert!(ran.status.success(), "{said}");
    said
}

/// A report of run `run` where Codex on `version` failed `failed` and passed its other cases,
/// skipped `skipped`, and Claude Code passed everything.
fn report(run: u64, version: &str, failed: &[&str], skipped: &[&str]) -> Value {
    let cases = |cli: &str| -> Value {
        let names = CASES.iter().find(|(c, _)| *c == cli).unwrap().1;
        let mut cases = json!({});
        for name in names {
            let result = if cli != "codex" {
                "passed"
            } else if failed.contains(name) {
                "failed"
            } else if skipped.contains(name) {
                "skipped"
            } else {
                "passed"
            };
            cases[*name] = json!({"result": result, "why": ""});
        }
        cases
    };
    json!({"run": run, "clis": {
        "claude-code": {"version": "2.1.325", "cases": cases("claude-code")},
        "codex": {"version": version, "cases": cases("codex")},
        "cursor-agent": {"version": "not installed", "cases": cases("cursor-agent")},
    }})
}

/// `drift` on two reports (no previous one when `previous` is null).
fn drift(folder: &Folder, gh: &Gh, previous: Value, current: Value) -> String {
    let (before, now) = (
        folder.0.join("previous.json"),
        folder.0.join("current.json"),
    );
    let _ = std::fs::remove_file(&before);
    if !previous.is_null() {
        std::fs::write(&before, previous.to_string()).unwrap();
    }
    std::fs::write(&now, current.to_string()).unwrap();
    canary_script(
        gh,
        &[
            "drift",
            "--current",
            now.to_str().unwrap(),
            "--previous",
            before.to_str().unwrap(),
            "--repo",
            "example/oby",
        ],
    )
}

/// The value after `flag` in a call.
fn flag<'a>(call: &'a [String], flag: &str) -> &'a str {
    let at = call.iter().position(|a| a == flag).unwrap();
    &call[at + 1]
}

/// CI-14: the report names each CLI's version (`unknown` for a version line that isn't one, `not
/// installed` with none) and each case's result, from the results the cases wrote and the ones the
/// workflow hands it.
#[test]
fn ci_14_the_report_names_each_clis_version_and_each_cases_result() {
    let folder = Folder::new("canary-ci-14");
    let gh = Gh::new(&folder.0);
    let results = folder.0.join("results");
    let write = |path: &str, text: &str| {
        let path = results.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("claude-code.version", "2.1.325 (Claude Code)\n");
    write("codex.version", "codex-cli 0.169.0;$(touch x)\n");
    write(
        "claude-code/start.json",
        r#"{"result": "passed", "why": ""}"#,
    );
    write(
        "claude-code/stop.json",
        r#"{"result": "failed", "why": "within 30 seconds: the job is stopped"}"#,
    );
    write(
        "codex/start.json",
        r#"{"result": "skipped", "why": "bubblewrap can't run on this runner"}"#,
    );
    write("codex/resume.json", "not json");
    let out = folder.0.join("report.json");
    let args = |more: &[&str]| -> Vec<String> {
        let mut args: Vec<String> = ["report", "--results", results.to_str().unwrap()]
            .iter()
            .map(|a| a.to_string())
            .collect();
        args.extend(["--run", "4242", "--out", out.to_str().unwrap()].map(String::from));
        args.extend(more.iter().map(|a| a.to_string()));
        args
    };
    let run = |more: &[&str]| -> Value {
        let args = args(more);
        canary_script(&gh, &args.iter().map(String::as_str).collect::<Vec<_>>());
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap()
    };
    let report = run(&[]);
    assert_eq!(report["run"], 4242);
    let names = |cli: &str| -> Vec<String> {
        report["clis"][cli]["cases"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    };
    for (cli, cases) in CASES {
        let mut want: Vec<String> = cases.iter().map(|c| c.to_string()).collect();
        want.sort();
        let mut got = names(cli);
        got.sort();
        assert_eq!(got, want, "{report}");
    }
    let clis = &report["clis"];
    assert_eq!(clis["claude-code"]["version"], "2.1.325");
    assert_eq!(clis["codex"]["version"], "unknown", "a junk version line");
    assert_eq!(clis["cursor-agent"]["version"], "not installed");
    let result = |cli: &str, case: &str| clis[cli]["cases"][case].clone();
    assert_eq!(
        result("claude-code", "start"),
        json!({"result": "passed", "why": ""})
    );
    assert_eq!(
        result("claude-code", "stop"),
        json!({"result": "failed", "why": "within 30 seconds: the job is stopped"})
    );
    assert_eq!(
        result("claude-code", "resume"),
        json!({"result": "skipped", "why": "no result"})
    );
    assert_eq!(
        result("codex", "start"),
        json!({"result": "skipped", "why": "bubblewrap can't run on this runner"})
    );
    assert_eq!(result("codex", "resume")["result"], "failed");
    assert_eq!(
        result("cursor-agent", "signed_out"),
        json!({"result": "skipped", "why": "not installed"})
    );

    // cursor-agent installed, its check's result handed over by the workflow.
    write("cursor-agent.version", "2026.11.13-eacade1\n");
    let report = run(&["--record", "cursor-agent/signed_out=failed"]);
    assert_eq!(
        report["clis"]["cursor-agent"]["version"],
        "2026.11.13-eacade1"
    );
    assert_eq!(
        report["clis"]["cursor-agent"]["cases"]["signed_out"]["result"],
        "failed"
    );
    assert!(gh.calls().is_empty(), "the report calls no gh");
}

/// CI-15: the same case failed on the same version twice in a row: one issue, labelled
/// `cli-drift`, naming the CLI, the version and the case, and the two runs.
#[test]
fn ci_15_a_case_failing_twice_on_the_same_version_opens_one_issue() {
    let folder = Folder::new("canary-ci-15-open");
    let gh = Gh::new(&folder.0);
    drift(
        &folder,
        &gh,
        report(1, "0.169.0", &["start"], &[]),
        report(2, "0.169.0", &["start"], &[]),
    );
    let writes = gh.writes();
    let created: Vec<_> = writes
        .iter()
        .filter(|c| c[..2] == ["issue", "create"])
        .collect();
    assert_eq!(created.len(), 1, "{writes:?}");
    assert_eq!(flag(created[0], "--label"), "cli-drift");
    assert_eq!(
        flag(created[0], "--title"),
        "CLI drift: codex 0.169.0 start"
    );
    let body = flag(created[0], "--body");
    assert!(body.contains("- codex 0.169.0 start"), "{body}");
    assert!(
        body.contains("https://github.com/example/oby/actions/runs/1")
            && body.contains("https://github.com/example/oby/actions/runs/2"),
        "{body}"
    );
    assert!(
        writes.iter().all(|c| c[..2] != ["issue", "comment"]),
        "{writes:?}"
    );
    assert!(
        writes
            .iter()
            .any(|c| c[..3] == ["label", "create", "cli-drift"]),
        "{writes:?}"
    );
}

/// CI-15: with one `cli-drift` issue open, a second pair comments on it, naming only that pair;
/// a third run with the same failures adds nothing.
#[test]
fn ci_15_a_second_pair_comments_on_the_open_issue_and_a_third_run_adds_nothing() {
    let folder = Folder::new("canary-ci-15-comment");
    let gh = Gh::new(&folder.0);
    let opened = "The CLI canary failed these cases twice in a row on the same version:\n\n\
                  - codex 0.169.0 start\n";
    gh.open(json!([{"number": 7, "body": opened, "comments": []}]));
    drift(
        &folder,
        &gh,
        report(2, "0.169.0", &["start", "resume"], &[]),
        report(3, "0.169.0", &["start", "resume"], &[]),
    );
    let writes = gh.writes();
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert_eq!(writes[0][..3], ["issue", "comment", "7"]);
    let body = flag(&writes[0], "--body");
    assert!(body.contains("- codex 0.169.0 resume"), "{body}");
    assert!(!body.contains("start"), "{body}");

    gh.clear();
    gh.open(json!([{"number": 7, "body": opened, "comments": [{"body": body}]}]));
    drift(
        &folder,
        &gh,
        report(3, "0.169.0", &["start", "resume"], &[]),
        report(4, "0.169.0", &["start", "resume"], &[]),
    );
    assert_eq!(gh.writes(), Vec::<Vec<String>>::new());
}

/// CI-15: a case that failed on one version and fails again on the next isn't twice in a row.
#[test]
fn ci_15_a_failure_on_a_new_version_opens_nothing() {
    let folder = Folder::new("canary-ci-15-version");
    let gh = Gh::new(&folder.0);
    drift(
        &folder,
        &gh,
        report(1, "0.169.0", &["start"], &[]),
        report(2, "0.170.0", &["start"], &[]),
    );
    assert_eq!(gh.writes(), Vec::<Vec<String>>::new());
}

/// CI-16: a case that failed and then passed opens nothing; nor does a failure with no previous
/// report to compare with.
#[test]
fn ci_16_a_failure_then_a_pass_opens_nothing() {
    let folder = Folder::new("canary-ci-16-pass");
    let gh = Gh::new(&folder.0);
    drift(
        &folder,
        &gh,
        report(1, "0.169.0", &["start"], &[]),
        report(2, "0.169.0", &[], &[]),
    );
    drift(
        &folder,
        &gh,
        Value::Null,
        report(2, "0.169.0", &["start"], &[]),
    );
    assert_eq!(gh.writes(), Vec::<Vec<String>>::new());
}

/// CI-16: a case skipped in either run (its CLI not installed, bubblewrap unable to run) opens
/// nothing.
#[test]
fn ci_16_a_skipped_case_opens_nothing() {
    let folder = Folder::new("canary-ci-16-skipped");
    let gh = Gh::new(&folder.0);
    drift(
        &folder,
        &gh,
        report(1, "0.169.0", &["start"], &[]),
        report(2, "0.169.0", &[], &["start"]),
    );
    drift(
        &folder,
        &gh,
        report(2, "0.169.0", &[], &["start"]),
        report(3, "0.169.0", &["start"], &[]),
    );
    assert_eq!(gh.writes(), Vec::<Vec<String>>::new());
}
