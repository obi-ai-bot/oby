//! The browser's API on the real obid (docs/contracts/browser.md), for what its workflows can't
//! easily show: the agents' server refusing a missing, made-up or ended job's token (BROWSER-16),
//! the switches kept across a restart (BROWSER-15), a refused call reading its whole 401, never a
//! reset (BROWSER-28), the window's relay taking only obid's runner token and the routes the window
//! does now answering 410 (BROWSER-30, 31). A Codex job sleeping as its agent hands back its broker
//! token (tests/agents/fake-codex, `SLEEP TOKEN FILE`).
mod support;

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use support::*;

/// A scratch obid with a Codex account for its jobs.
fn browser_obid(folder: &Folder) -> Obid {
    start_with(&folder.0, &browser_config(folder), None, &[])
}

fn browser_config(folder: &Folder) -> Value {
    let remote = fixture(&folder.0.join("fixture"));
    let agents = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents");
    let mut config = config(&folder.0, &remote, &agents.join("fake-claude"));
    let dir = folder.0.join("home/accounts/codex");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("auth.json"),
        json!({"OPENAI_API_KEY": "fake-codex"}).to_string(),
    )
    .unwrap();
    config["agents"]["codex"] = json!({"bin": agents.join("fake-codex"),
        "accounts": [{"id": "codex", "name": "Codex", "configDir": dir}]});
    config
}

/// A Codex job that stays running, and its broker token.
fn sleeping_job(obid: &Obid) -> (String, String) {
    let made = call(
        obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "build",
            "task": "SLEEP TOKEN FILE", "agent": "codex"}}),
        ),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap().to_string();
    let file = obid.home.join("worktrees").join(&id).join(".obi/mcp-token");
    let token = within(90, "the job's stand-in writes its broker token", || {
        std::fs::read_to_string(&file)
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| t.len() == 64)
    });
    (id, token)
}

/// One request with exactly these headers (none, or another token than obid's).
fn raw(
    obid: &Obid,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(("127.0.0.1", obid.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .unwrap();
    let mut text = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in headers {
        text.push_str(&format!("{k}: {v}\r\n"));
    }
    text.push_str("\r\n");
    text.push_str(&body);
    stream.write_all(text.as_bytes()).unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).unwrap();
    let (head, rest) = out.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (
        status,
        serde_json::from_str(rest.trim()).unwrap_or(Value::Null),
    )
}

fn mcp(obid: &Obid, token: Option<&str>, method: &str, params: Value) -> (u16, Value) {
    let bearer = token.map(|t| format!("Bearer {t}"));
    let headers: Vec<(&str, &str)> = bearer
        .as_deref()
        .map(|b| vec![("Authorization", b)])
        .unwrap_or_default();
    raw(
        obid,
        "POST",
        "/v1/browser/mcp",
        &headers,
        &json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
    )
}

fn tool(obid: &Obid, token: &str, name: &str, args: Value) -> Value {
    let (status, rpc) = mcp(
        obid,
        Some(token),
        "tools/call",
        json!({"name": name, "arguments": args}),
    );
    assert_eq!(status, 200, "{rpc}");
    rpc["result"]["structuredContent"].clone()
}

/// obid stopped (SIGTERM): every browser it started stops with it, its record gone (BROWSER-25).
fn stop_with_no_browser_left(obid: &mut Obid) {
    assert!(obid.stop().success());
    let live: Vec<_> = std::fs::read_dir(obid.home.join("browser/live"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(live.is_empty(), "browsers still recorded live: {live:?}");
}

#[test]
fn browser_16_no_token_a_made_up_one_the_owners_or_an_ended_jobs_is_refused() {
    let folder = Folder::new("browser-16");
    let mut obid = browser_obid(&folder);
    let (job, token) = sleeping_job(&obid);
    let (status, body) = mcp(&obid, None, "tools/list", json!({}));
    assert_eq!(
        (status, body["reason"].as_str()),
        (401, Some("no_token")),
        "{body}"
    );
    let (status, body) = mcp(&obid, Some(&"0".repeat(64)), "tools/list", json!({}));
    assert_eq!(
        (status, body["reason"].as_str()),
        (401, Some("bad_token")),
        "{body}"
    );
    let (status, body) = mcp(&obid, Some(TOKEN), "tools/list", json!({}));
    assert_eq!(
        (status, body["reason"].as_str()),
        (401, Some("bad_token")),
        "{body}"
    );
    let (status, body) = mcp(&obid, Some(&token), "tools/list", json!({}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["tools"].as_array().map(Vec::len), Some(11));
    let stopped = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{job}/stop"),
        Some(&json!({})),
    );
    assert_eq!(stopped.status, 200, "{}", stopped.text);
    within(30, "an ended job's token is refused", || {
        let (status, body) = mcp(&obid, Some(&token), "tools/list", json!({}));
        (status == 401 && body["reason"] == "bad_token").then_some(())
    });
    stop_with_no_browser_left(&mut obid);
}

/// One request as obid-spec's client sends it: its head, then its body in a write of its own,
/// then everything obid answers until it closes. A reset, while writing or reading, is an error.
fn refused_whole(
    obid: &Obid,
    path: &str,
    token: Option<&str>,
    body: &[u8],
) -> Result<(u16, Value), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", obid.port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    // A small send buffer, so a large body is surely still being written when obid answers.
    rustix::net::sockopt::set_socket_send_buffer_size(&stream, 4096).unwrap();
    let mut head = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(token) = token {
        head.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .map_err(|e| format!("writing the head: {e}"))?;
    stream
        .write_all(body)
        .map_err(|e| format!("writing the body: {e}"))?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| format!("reading the answer: {e}"))?;
    let raw = String::from_utf8_lossy(&raw);
    let (head, rest) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("no whole answer: {raw:?}"))?;
    let status = head.split(' ').nth(1).and_then(|s| s.parse().ok());
    let status = status.ok_or_else(|| format!("no status: {head:?}"))?;
    Ok((
        status,
        serde_json::from_str(rest.trim()).unwrap_or(Value::Null),
    ))
}

#[test]
fn browser_28_a_refused_call_reads_its_whole_401_never_a_reset() {
    let folder = Folder::new("browser-26");
    let mut obid = browser_obid(&folder);
    let made_up = "0".repeat(64);
    let small = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}})
        .to_string()
        .into_bytes();
    // A body bigger than the sockets' buffers: still being written when obid answers.
    let large = {
        let mut b = br#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"pad":""#.to_vec();
        b.resize(1024 * 1024, b'x');
        b.extend_from_slice(br#""}}"#);
        b
    };
    // The browser's server (a made-up token), an agent's own MCP route (a made-up token) and the
    // app's API (no token): each refused by the API's gate before any handler reads the body.
    let routes: [(&str, Option<&str>, Option<&str>); 3] = [
        ("/v1/browser/mcp", Some(&made_up), Some("bad_token")),
        ("/v1/agent/mcp/no-such-job", Some(&made_up), None),
        ("/v1/jobs", None, None),
    ];
    let mut failures = Vec::new();
    for (path, token, reason) in routes {
        for (round, body) in std::iter::repeat_n(&small, 40).chain([&large]).enumerate() {
            match refused_whole(&obid, path, token, body) {
                Ok((401, answer)) if reason.is_none_or(|r| answer["reason"] == r) => {}
                Ok((status, answer)) => {
                    failures.push(format!("{path} #{round}: {status} {answer}"))
                }
                Err(e) => failures.push(format!("{path} #{round} ({} bytes): {e}", body.len())),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} refused calls didn't read their 401:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // obid still answers a call with its token.
    let status = call(&obid, "GET", "/v1/status", None);
    assert_eq!(status.status, 200, "{}", status.text);
    stop_with_no_browser_left(&mut obid);
}

#[test]
fn browser_15_the_switches_are_kept_across_a_restart() {
    let folder = Folder::new("browser-15");
    let mut obid = browser_obid(&folder);
    let first = call(&obid, "GET", "/v1/browser/settings", None);
    assert_eq!(
        first.body,
        json!({"agentsUseBrowser": true, "agentsUseMac": true})
    );
    let off = call_signed(
        &obid,
        "PUT",
        "/v1/browser/settings",
        Some(&json!({"agentsUseBrowser": false})),
    );
    assert_eq!(off.status, 200, "{}", off.text);
    assert!(obid.stop().success());
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.0.join("config.json")).unwrap())
            .unwrap();
    let mut again = start_with(&folder.0, &config, None, &[]);
    let after = call(&again, "GET", "/v1/browser/settings", None);
    assert_eq!(
        after.body,
        json!({"agentsUseBrowser": false, "agentsUseMac": true})
    );
    // And off, every tool refuses.
    let (_job, token) = sleeping_job(&again);
    let refused = tool(&again, &token, "oby_browser_snapshot", json!({}));
    assert_eq!(refused["error"]["code"], "browser_off", "{refused}");
    stop_with_no_browser_left(&mut again);
}

/// The head of a `GET` that streams: its status line and headers, then the connection dropped.
fn stream_head(obid: &Obid, path: &str, token: Option<&str>) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", obid.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\n{auth}\r\n"
    )
    .unwrap();
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

/// The window's relay (T3 Code's design) is the window's alone: obid's runner token follows its
/// commands and posts its events; no token, and an agent's own job token, reach neither.
#[test]
fn relay_only_the_runner_token_reaches_the_windows_relay() {
    let folder = Folder::new("relay-token");
    let mut obid = browser_obid(&folder);
    let (_job, job_token) = sleeping_job(&obid);
    let shown =
        json!({"events": [{"type": "shown", "threadId": "j1", "tabId": "tab-1", "shown": true}]});
    let auth = format!("Bearer {TOKEN}");
    let (status, body) = raw(&obid, "POST", "/v1/browser/relay", &[], &shown);
    assert_eq!(status, 401, "{body}");
    let agent = format!("Bearer {job_token}");
    let (status, body) = raw(
        &obid,
        "POST",
        "/v1/browser/relay",
        &[("Authorization", &agent)],
        &shown,
    );
    assert_eq!(
        status, 401,
        "an agent's token never posts the window's events: {body}"
    );
    let (status, body) = raw(
        &obid,
        "POST",
        "/v1/browser/relay",
        &[("Authorization", &auth)],
        &shown,
    );
    assert_eq!(status, 204, "{body}");
    let (status, body) = raw(
        &obid,
        "POST",
        "/v1/browser/relay",
        &[("Authorization", &auth)],
        &json!({"events": "not a list"}),
    );
    assert_eq!(status, 400, "{body}");
    // A type obid doesn't know yet is ignored, never refused.
    let (status, body) = raw(
        &obid,
        "POST",
        "/v1/browser/relay",
        &[("Authorization", &auth)],
        &json!({"events": [{"type": "somethingNew", "threadId": "j1"}]}),
    );
    assert_eq!(status, 204, "{body}");
    let head = stream_head(&obid, "/v1/browser/relay", Some(TOKEN));
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        head.to_ascii_lowercase().contains("text/event-stream"),
        "{head}"
    );
    for token in [None, Some(job_token.as_str())] {
        let head = stream_head(&obid, "/v1/browser/relay", token);
        assert!(head.starts_with("HTTP/1.1 401"), "{token:?}: {head}");
    }
    stop_with_no_browser_left(&mut obid);
}

/// Retired with obid's own browser (the window's browser, docs/plans/browser-t3.md): the person
/// uses the webview itself, and Fill in is the window's. Each route stays, answering 410 with
/// `retired`, so an app that still calls one is told, never left hanging (the add-only API).
#[test]
fn relay_the_routes_the_window_does_now_answer_410_retired() {
    let folder = Folder::new("relay-retired");
    let mut obid = browser_obid(&folder);
    let auth = format!("Bearer {TOKEN}");
    for (method, path, body) in [
        ("POST", "/v1/browser/tabs/tab-1/take", json!({})),
        ("POST", "/v1/browser/tabs/tab-1/giveBack", json!({})),
        (
            "POST",
            "/v1/browser/tabs/tab-1/input",
            json!({"type": "mouse"}),
        ),
        (
            "POST",
            "/v1/browser/tabs/tab-1/secret",
            json!({"field": "#pw", "value": "v"}),
        ),
        ("GET", "/v1/browser/tabs/tab-1/stream", json!(null)),
    ] {
        let (status, answer) = raw(&obid, method, path, &[("Authorization", &auth)], &body);
        assert_eq!(
            (status, answer["reason"].as_str()),
            (410, Some("retired")),
            "{method} {path}: {answer}"
        );
    }
    stop_with_no_browser_left(&mut obid);
}
