//! Oby's MCP server's Obysphere tools (oby-plan E1, "Bindings"; docs/mcp.md) end to end: the real
//! obid binary joined to a test Obysphere (tests/support's fake Sphere, which checks what it's
//! sent with the spec's checks), a teammate's Oby (obi-ernesto) played through the Sphere's API,
//! and `GET /v1/obid/sphere/members`, which `sphere_members` reads.
//!
//! The tests marked ignored drive the `oby-mcp` binary itself, as a local Claude Code session does:
//!
//! ```sh
//! swift build --product oby-mcp
//! OBY_MCP_BIN=$PWD/.build/debug/oby-mcp cargo test -p obid --test mcp_sphere -- --ignored
//! ```
//!
//! With `OBY_MCP_CLAUDE=1` too, the last one has a real `claude -p` send the review request through
//! oby-mcp (it uses the Claude Code login on this Mac).

mod support;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};
use support::*;

const HEAD_SHA: &str = "6d20146f366b9fac8907b5066c30ee39aaec09b3";

/// obid (wellington) joined to a new test Obysphere, and obi-ernesto there too.
async fn joined(folder: &Folder) -> (FakeSphere, Obid, TestObi) {
    let sphere = FakeSphere::start().await;
    let remote = fixture(&folder.0.join("fixture"));
    let obid = start(&folder.0, &config(&folder.0, &remote, &stub_agent()), None);
    let before =
        tokio::task::block_in_place(|| call(&obid, "GET", "/v1/obid/sphere/members", None));
    assert_eq!(before.status, 409, "{}", before.text);
    let wellington = Owner::new("wellington");
    let joined = tokio::task::block_in_place(|| join_a(&obid, &sphere, &wellington));
    assert_eq!(joined["state"], "joined");
    let ernesto = TestObi::join_accepting(
        &sphere,
        "obi-ernesto",
        "ernesto",
        &["review_request", "question"],
    )
    .await;
    (sphere, obid, ernesto)
}

/// `GET /v1/obid/sphere/members`: the other members whose cards obid trusts, typed fields only,
/// read fresh, so a teammate who joined a moment ago shows.
#[tokio::test(flavor = "multi_thread")]
async fn members_are_the_teammates_obys() {
    let folder = Folder::new("mcp-members");
    let (_sphere, obid, _ernesto) = joined(&folder).await;
    let members =
        tokio::task::block_in_place(|| call(&obid, "GET", "/v1/obid/sphere/members", None));
    assert_eq!(members.status, 200, "{}", members.text);
    assert_eq!(
        members.body,
        json!([{"obi": "obi-ernesto", "owner": "ernesto", "accepts": ["question", "review_request"],
                "sealed": false, "verified": false}])
    );
}

// ---------------------------------------------------------------------------------------------
// oby-mcp itself

/// oby-mcp, as a client starts it, finding obid's token where it looks on this machine.
struct Mcp {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A home where oby-mcp finds `token` as obid's: the Mac app's runner home, and Linux's.
fn user_home(folder: &Path, token: &str) -> PathBuf {
    let home = folder.join("user-home");
    for place in [
        "Library/Application Support/Oby/Runner",
        ".local/share/obid",
    ] {
        let dir = home.join(place);
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .unwrap();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join("api-token"))
            .unwrap();
        file.write_all(format!("{token}\n").as_bytes()).unwrap();
    }
    home
}

fn mcp_bin() -> PathBuf {
    PathBuf::from(
        std::env::var_os("OBY_MCP_BIN")
            .expect("OBY_MCP_BIN names the oby-mcp binary (swift build --product oby-mcp)"),
    )
}

/// What oby-mcp's environment holds: obid on this machine, and the home its token is in.
fn mcp_env(obid: &Obid, home: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("OBY_RUNNER_URL", format!("http://127.0.0.1:{}", obid.port)),
        ("HOME", home.display().to_string()),
        // Foundation's home on a Mac, whatever HOME says.
        ("CFFIXED_USER_HOME", home.display().to_string()),
    ]
}

impl Mcp {
    fn start(obid: &Obid, home: &Path) -> Mcp {
        let mut child = Command::new(mcp_bin())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .envs(mcp_env(obid, home))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("oby-mcp starts");
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut mcp = Mcp {
            child,
            input,
            output,
            next: 0,
        };
        let initialized = mcp.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "obid-test", "version": "1"}}),
        );
        assert_eq!(initialized["result"]["serverInfo"]["name"], "oby");
        writeln!(
            mcp.input,
            "{}",
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .unwrap();
        mcp
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        )
        .unwrap();
        loop {
            let mut line = String::new();
            assert!(
                self.output.read_line(&mut line).unwrap() > 0,
                "oby-mcp hung up"
            );
            let answer: Value = serde_json::from_str(&line).unwrap();
            if answer["id"] == id {
                return answer;
            }
        }
    }

    /// A tool call: its text, and whether it's an error.
    fn call(&mut self, tool: &str, arguments: Value) -> (String, bool) {
        let answer = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        let result = &answer["result"];
        (
            result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            result["isError"] == true,
        )
    }
}

/// The done-when of E1's bindings, scripted as a client: a local session lists the teammates'
/// Obys, sends one a review request through oby-mcp, and follows the exchange to its result.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the oby-mcp binary: OBY_MCP_BIN=… cargo test -p obid --test mcp_sphere -- --ignored"]
async fn a_review_request_goes_through_oby_mcp() {
    let folder = Folder::new("mcp-review");
    let (_sphere, obid, ernesto) = joined(&folder).await;
    let home = user_home(&folder.0, TOKEN);
    let mut mcp = tokio::task::block_in_place(|| Mcp::start(&obid, &home));

    let listed = tokio::task::block_in_place(|| mcp.request("tools/list", json!({})));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for tool in ["sphere_members", "sphere_exchange", "sphere_review_request"] {
        assert!(names.contains(&tool), "{names:?}");
    }

    let (members, error) = tokio::task::block_in_place(|| mcp.call("sphere_members", json!({})));
    assert!(!error, "{members}");
    assert!(
        members.contains("obi-ernesto · owner ernesto · takes question, review_request"),
        "{members}"
    );

    let (sent, error) = tokio::task::block_in_place(|| {
        mcp.call(
            "sphere_review_request",
            json!({"to": "obi-ernesto", "repo": "obi-e2e/fixture-app", "head_branch": "feature/average",
                   "head_sha": HEAD_SHA, "base_branch": "main", "goal": "Add an average helper",
                   "summary": "Adds src/average.js with a test.", "expires_in": 3600}),
        )
    });
    assert!(!error, "{sent}");
    assert!(sent.starts_with("Sent to obi-ernesto: exchange "), "{sent}");
    let id = sent["Sent to obi-ernesto: exchange ".len()..]
        .split('.')
        .next()
        .unwrap()
        .to_owned();

    // The teammate's Oby gets a review_request the Sphere checked, as the spec writes it.
    let requests = ernesto.receive(1, 20).await;
    let request = &requests[0];
    assert_eq!(request["type"], "review_request");
    assert_eq!(request["correlation_id"], id.as_str());
    assert_eq!(
        request["body"],
        json!({"goal": "Add an average helper", "repo": "obi-e2e/fixture-app", "head_branch": "feature/average",
               "head_sha": HEAD_SHA, "base_branch": "main", "summary": "Adds src/average.js with a test.",
               "acceptance": [{"kind": "tests_pass"}, {"kind": "no_blocker_findings"}]})
    );

    // It accepts and answers; the session follows the exchange to its result.
    ernesto
        .send(&ernesto.reply(request, "accept", json!({})))
        .await;
    ernesto
        .send(&ernesto.reply(request, "result", expected_result()))
        .await;
    let done = within(20, "the exchange done", || {
        let (text, _) =
            tokio::task::block_in_place(|| mcp.call("sphere_exchange", json!({"id": id})));
        text.contains("state: done").then_some(text)
    });
    assert!(done.contains("outcome: approved"), "{done}");
    assert!(
        done.contains("major · src/average.js:4 · average() divides by zero for an empty list"),
        "{done}"
    );
    let (list, _) = tokio::task::block_in_place(|| mcp.call("sphere_exchange", json!({})));
    assert!(
        list.contains(&format!(
            "{id} · outgoing · review_request · obi-ernesto (ernesto) · done"
        )),
        "{list}"
    );

    // A refusal says what to do, nothing of obid's insides: a recipient that isn't a member.
    let (refused, error) = tokio::task::block_in_place(|| {
        mcp.call(
            "sphere_review_request",
            json!({"to": "obi-nobody", "repo": "obi-e2e/fixture-app", "head_branch": "feature/average",
                   "head_sha": HEAD_SHA, "base_branch": "main", "goal": "Add an average helper"}),
        )
    });
    assert!(error, "{refused}");
    assert!(
        refused.starts_with("Nothing was sent: the runner refused it."),
        "{refused}"
    );
    assert!(!refused.contains(TOKEN), "{refused}");
}

/// The same through a real Claude Code session: `claude -p` with oby-mcp as its only MCP server
/// sends the review request, and the teammate's Oby gets it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the oby-mcp binary and, with OBY_MCP_CLAUDE=1, Claude Code signed in on this Mac"]
async fn a_claude_code_session_sends_a_review_request() {
    if std::env::var_os("OBY_MCP_CLAUDE").is_none() {
        eprintln!("OBY_MCP_CLAUDE isn't set: no real Claude Code session run");
        return;
    }
    let folder = Folder::new("mcp-claude");
    let (_sphere, obid, ernesto) = joined(&folder).await;
    let home = user_home(&folder.0, TOKEN);
    let env: serde_json::Map<String, Value> = mcp_env(&obid, &home)
        .into_iter()
        .map(|(k, v)| (k.to_owned(), json!(v)))
        .collect();
    let mcp_config = folder.0.join("mcp.json");
    std::fs::write(
        &mcp_config,
        json!({"mcpServers": {"oby": {"command": mcp_bin(), "env": env}}}).to_string(),
    )
    .unwrap();
    let prompt = format!(
        "Use the oby MCP tools. Call sphere_members to find the teammate's Oby, then call \
         sphere_review_request to ask it to review repo obi-e2e/fixture-app, head_branch \
         feature/average, head_sha {HEAD_SHA}, base_branch main, goal \"Add an average helper\". \
         Then reply with the exchange id only."
    );
    // Spawning claude: no CLAUDECODE, CLAUDE_CODE_* or ANTHROPIC_* (AGENTS.md).
    let mut command = Command::new("claude");
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy().into_owned();
        if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") || key.starts_with("ANTHROPIC_") {
            command.env_remove(&key);
        }
    }
    let output = tokio::task::block_in_place(|| {
        command
            .args([
                "-p",
                &prompt,
                "--output-format",
                "json",
                "--strict-mcp-config",
                "--mcp-config",
            ])
            .arg(&mcp_config)
            .args([
                "--allowedTools",
                "mcp__oby__sphere_members,mcp__oby__sphere_review_request",
                "--max-turns",
                "6",
            ])
            .current_dir(&folder.0)
            .stdin(Stdio::null())
            .output()
            .expect("claude runs")
    });
    let said = String::from_utf8_lossy(&output.stdout);
    eprintln!("claude -p answered: {said}");
    assert!(
        output.status.success() && !said.contains("\"is_error\":true"),
        "claude -p didn't do the task (is Claude Code signed in on this Mac?): {said}"
    );
    let requests = ernesto.receive(1, 30).await;
    assert_eq!(requests[0]["type"], "review_request", "{said}");
    assert_eq!(requests[0]["body"]["head_sha"], HEAD_SHA);
    assert_eq!(requests[0]["body"]["repo"], "obi-e2e/fixture-app");
    let id = requests[0]["correlation_id"].as_str().unwrap();
    assert!(said.contains(id), "{said}");
}
