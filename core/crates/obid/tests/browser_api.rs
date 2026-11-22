//! The browser's API on the real obid (docs/contracts/browser.md), for what its workflows can't
//! easily show: the agents' server refusing a missing, made-up or ended job's token (BROWSER-16),
//! the switches kept across a restart (BROWSER-15), a refused call reading its whole 401, never a
//! reset (BROWSER-28), the window's relay taking only obid's runner token and the routes the window
//! does now answering 410 (BROWSER-30, 31), and the owner's signed take and give back of an agent's
//! tab, driven through a stand-in window on the relay (BROWSER-5, 6). A Codex job sleeping as its
//! agent hands back its broker token (tests/agents/fake-codex, `SLEEP TOKEN FILE`).
mod support;

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
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
    raw_at(obid.port, method, path, headers, body)
}

/// [`raw`] to the obid on `port`, for a thread that has no `Obid` of its own.
fn raw_at(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
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

/// A Codex job that stays running, its file written before obid starts as `mark` leaves it, and
/// its broker token.
fn seeded_job(folder: &Folder, id: &str, mark: impl FnOnce(&mut obid::job::Job)) -> (Obid, String) {
    let config = browser_config(folder);
    let mut job = obid::job::Job::new(
        id.into(),
        "obi-e2e/fixture-app".into(),
        "build".into(),
        "SLEEP TOKEN FILE".into(),
        "codex".into(),
        "codex".into(),
    );
    job.branch = format!("oby/build-{id}");
    mark(&mut job);
    let jobs = folder.0.join("home/jobs");
    std::fs::create_dir_all(&jobs).unwrap();
    std::fs::write(
        jobs.join(format!("{id}.json")),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    let obid = start_with(&folder.0, &config, None, &[]);
    let file = obid.home.join(format!("worktrees/{id}/.obi/mcp-token"));
    let token = within(90, "the job hands back its token", || {
        std::fs::read_to_string(&file)
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| t.len() == 64)
    });
    (obid, token)
}

/// A foreign job's tab, opened by its agent in the stand-in window, is in its box partition
/// (`oby-box-<job>`), and its navigate to obid's own API on loopback is outside the box
/// (BROWSER-18).
fn browses_in_its_box(obid: &Obid, token: &str, job: &str) {
    let window = Window::follow(obid);
    let opening = {
        let (port, token) = (obid.port, token.to_string());
        std::thread::spawn(move || {
            let bearer = format!("Bearer {token}");
            raw_at(
                port,
                "POST",
                "/v1/browser/mcp",
                &[("Authorization", &bearer)],
                &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": {"name": "oby_browser_open", "arguments": {"url": PAGE}}}),
            )
        })
    };
    let tab = window.attach(obid, job);
    let (status, opened) = opening.join().unwrap();
    assert_eq!(status, 200, "{opened}");
    assert_eq!(
        opened["result"]["structuredContent"]["ok"], true,
        "{opened}"
    );
    let tabs = call(obid, "GET", "/v1/browser", None).body;
    let shown = tabs["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == tab.as_str())
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(shown["partition"], format!("oby-box-{job}"), "{tabs}");
    let url = format!("http://127.0.0.1:{}/v1/jobs", obid.port);
    let refused = tool(obid, token, "oby_browser_navigate", json!({"url": url}));
    assert_eq!(refused["error"]["code"], "outside_box", "{refused}");
}

/// A job marked `untrusted` gets the browser in its box (BROWSER-18): its agent's tab opens in
/// `oby-box-<job>`, and its navigate to obid's own API is refused. Written before obid starts, as
/// a foreign job is.
#[test]
fn browser_18_a_foreign_jobs_tabs_open_in_its_box_and_its_navigate_to_obid_is_outside_box() {
    let folder = Folder::new("browser-18");
    let (mut obid, token) = seeded_job(&folder, "bbbb1818", |job| job.untrusted = true);
    browses_in_its_box(&obid, &token, "bbbb1818");
    stop_with_no_browser_left(&mut obid);
}

/// A follow-up whose earlier job is gone (a received review the owner deleted, then obid
/// restarted) can't show where its line came from: it browses in its box, never the owner's
/// partition (BROWSER-18). It carries the `foreign` mark obid gives a follow-up of someone else's
/// job.
#[test]
fn browser_18_a_follow_up_whose_earlier_job_is_gone_browses_in_its_box() {
    let folder = Folder::new("browser-18-gone");
    let (mut obid, token) = seeded_job(&folder, "cccc1818", |job| {
        job.follow_up_of = Some("dddd1818".into());
        job.foreign = Some(
            "it follows up a job whose check got none of them (another Oby asked for this job)"
                .into(),
        );
    });
    browses_in_its_box(&obid, &token, "cccc1818");
    stop_with_no_browser_left(&mut obid);
}

/// The owner's review saved by an obid from before its start mark (`someone_elses_code`), marked
/// `foreign` alone, is someone else's code once read again: it browses in its box, whether or not
/// its worktree is there to find who wrote it (BROWSER-18).
#[test]
fn browser_18_a_review_saved_before_its_start_mark_browses_in_its_box() {
    let folder = Folder::new("browser-18-saved-review");
    let (mut obid, token) = seeded_job(&folder, "eeee1818", |job| {
        job.kind = "review".into();
        job.foreign = Some("as it started, someone else wrote in pull request #90".into());
    });
    browses_in_its_box(&obid, &token, "eeee1818");
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
fn browser_30_input_secret_and_stream_still_answer_410_retired() {
    let folder = Folder::new("relay-retired");
    let mut obid = browser_obid(&folder);
    let auth = format!("Bearer {TOKEN}");
    for (method, path, body) in [
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

/// The address every page of the stand-in window is at.
const PAGE: &str = "https://example.com/";

/// An answer the stand-in window keeps back: for the first frame of this method, until the test's
/// condition holds.
type Hold = (&'static str, Box<dyn Fn() -> bool + Send>);

/// A stand-in for Oby's window on the relay (`browser/relay.rs`): it follows `GET
/// /v1/browser/relay`, answers each CDP frame by its id as a page at [`PAGE`] with one button would,
/// and records every method it was sent. One answer may be held back ([`Window::hold`]).
struct Window {
    port: u16,
    methods: Arc<Mutex<Vec<String>>>,
    hold: Arc<Mutex<Option<Hold>>>,
}

impl Window {
    /// The window, following the relay before this returns.
    fn follow(obid: &Obid) -> Window {
        let window = Window {
            port: obid.port,
            methods: Arc::default(),
            hold: Arc::default(),
        };
        let mut stream = TcpStream::connect(("127.0.0.1", obid.port)).unwrap();
        write!(
            stream,
            "GET /v1/browser/relay HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\nAuthorization: Bearer {TOKEN}\r\n\r\n"
        )
        .unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.starts_with("HTTP/1.1 200"), "{line}");
        let (port, methods, hold) = (window.port, window.methods.clone(), window.hold.clone());
        std::thread::spawn(move || {
            while {
                line.clear();
                reader.read_line(&mut line).unwrap_or(0) > 0
            } {
                let Some(command) = line
                    .trim()
                    .strip_prefix("data: ")
                    .and_then(|d| serde_json::from_str::<Value>(d).ok())
                    .filter(|c| c["type"] == "cdp")
                else {
                    continue;
                };
                let frame: Value =
                    serde_json::from_str(command["message"].as_str().unwrap()).unwrap();
                let method = frame["method"].as_str().unwrap_or_default().to_string();
                methods.lock().unwrap().push(method.clone());
                let events = page_answers(&command, &frame);
                let held = {
                    let mut hold = hold.lock().unwrap();
                    match hold.take() {
                        Some((m, until)) if m == method => Some(until),
                        other => {
                            *hold = other;
                            None
                        }
                    }
                };
                match held {
                    Some(until) => {
                        std::thread::spawn(move || {
                            within(60, "the test lets the held answer go", || {
                                until().then_some(())
                            });
                            relay_post(port, events);
                        });
                    }
                    None => relay_post(port, events),
                }
            }
        });
        window
    }

    /// Posts `attached` for `job`'s new tab, once `GET /v1/browser` shows it: its id.
    fn attach(&self, obid: &Obid, job: &str) -> String {
        let tab = within(30, "the job's new tab shows", || {
            let tabs = call(obid, "GET", "/v1/browser", None).body;
            tabs["tabs"]
                .as_array()?
                .iter()
                .find(|t| t["owner"]["job"] == job)
                .and_then(|t| t["id"].as_str().map(str::to_string))
        });
        relay_post(
            self.port,
            vec![json!({"type": "attached", "threadId": job, "tabId": tab})],
        );
        tab
    }

    /// How many frames of `method` the window was sent.
    fn sent(&self, method: &str) -> usize {
        self.methods
            .lock()
            .unwrap()
            .iter()
            .filter(|m| *m == method)
            .count()
    }

    /// Holds back the answer to the next frame of `method` until `until` holds.
    fn hold(&self, method: &'static str, until: impl Fn() -> bool + Send + 'static) {
        *self.hold.lock().unwrap() = Some((method, Box::new(until)));
    }
}

/// What a page at [`PAGE`] with one button answers a CDP `frame`: its answer by id, and for a
/// navigate, its load after.
fn page_answers(command: &Value, frame: &Value) -> Vec<Value> {
    let result = match frame["method"].as_str().unwrap_or_default() {
        "Runtime.evaluate" => json!({"result": {"value": {"url": PAGE, "title": "A page"}}}),
        "Page.navigate" => json!({"frameId": "f1", "loaderId": "l1"}),
        "Accessibility.getFullAXTree" => {
            json!({"nodes": [{"nodeId": "1", "role": {"value": "button"},
            "name": {"value": "Go"}, "backendDOMNodeId": 7, "childIds": []}]})
        }
        "DOM.getContentQuads" => json!({"quads": [[10, 10, 30, 10, 30, 20, 10, 20]]}),
        _ => json!({}),
    };
    let cdp = |message: Value| {
        json!({"type": "cdp", "threadId": command["threadId"], "tabId": command["tabId"],
               "message": message.to_string()})
    };
    let mut events = vec![cdp(json!({"id": frame["id"], "result": result}))];
    if frame["method"] == "Page.navigate" {
        events.push(cdp(
            json!({"method": "Page.loadEventFired", "params": {"timestamp": 1}}),
        ));
    }
    events
}

/// The window's events, posted to obid's relay in order.
fn relay_post(port: u16, events: Vec<Value>) {
    let auth = format!("Bearer {TOKEN}");
    let (status, body) = raw_at(
        port,
        "POST",
        "/v1/browser/relay",
        &[("Authorization", &auth)],
        &json!({ "events": events }),
    );
    assert_eq!(status, 204, "{body}");
}

/// A sleeping job's agent with a tab of its own in the stand-in window (BROWSER-5, 6).
struct AgentTab {
    obid: Obid,
    window: Window,
    token: String,
    tab: String,
    _folder: Folder,
}

impl AgentTab {
    fn open(name: &str) -> AgentTab {
        let folder = Folder::new(name);
        let obid = browser_obid(&folder);
        let (job, token) = sleeping_job(&obid);
        let window = Window::follow(&obid);
        let opening = {
            let (port, token) = (obid.port, token.clone());
            std::thread::spawn(move || {
                let bearer = format!("Bearer {token}");
                raw_at(
                    port,
                    "POST",
                    "/v1/browser/mcp",
                    &[("Authorization", &bearer)],
                    &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                        "params": {"name": "oby_browser_open", "arguments": {"url": PAGE}}}),
                )
            })
        };
        let tab = window.attach(&obid, &job);
        let (status, opened) = opening.join().unwrap();
        assert_eq!(status, 200, "{opened}");
        let opened = &opened["result"]["structuredContent"];
        assert_eq!(opened["ok"], true, "{opened}");
        AgentTab {
            obid,
            window,
            token,
            tab,
            _folder: folder,
        }
    }

    fn tool(&self, name: &str, args: Value) -> Value {
        tool(&self.obid, &self.token, name, args)
    }

    fn navigate(&self) -> Value {
        self.tool("oby_browser_navigate", json!({"url": PAGE}))
    }

    fn path(&self, verb: &str) -> String {
        format!("/v1/browser/tabs/{}/{verb}", self.tab)
    }

    /// The owner's signed take or give back.
    fn signed(&self, verb: &str) -> Answer {
        call_signed(&self.obid, "POST", &self.path(verb), Some(&json!({})))
    }

    /// The tab as `GET /v1/browser` shows it.
    fn shown(&self) -> Value {
        let tabs = call(&self.obid, "GET", "/v1/browser", None).body;
        tabs["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == self.tab.as_str())
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// The agent's navigate, on a thread of its own: it says `act` on `done` once answered.
    fn navigate_on_thread(&self, done: mpsc::Sender<&'static str>) -> JoinHandle<(u16, Value)> {
        let (port, token) = (self.obid.port, self.token.clone());
        std::thread::spawn(move || {
            let bearer = format!("Bearer {token}");
            let answer = raw_at(
                port,
                "POST",
                "/v1/browser/mcp",
                &[("Authorization", &bearer)],
                &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": {"name": "oby_browser_navigate", "arguments": {"url": PAGE}}}),
            );
            done.send("act").unwrap();
            answer
        })
    }

    /// The owner's signed take, on a thread of its own: it says `take` on `done` once answered.
    /// Its words are signed here: the thread has no obid of its own to sign with.
    fn take_on_thread(&self, done: mpsc::Sender<&'static str>) -> JoinHandle<(u16, Value)> {
        let (port, path) = (self.obid.port, self.path("take"));
        let header = words_header(&self.obid, "POST", &path, Some(&json!({})));
        std::thread::spawn(move || {
            let auth = format!("Bearer {TOKEN}");
            let answer = raw_at(
                port,
                "POST",
                &path,
                &[("Authorization", &auth), ("X-Oby-Words", &header)],
                &json!({}),
            );
            done.send("take").unwrap();
            answer
        })
    }

    /// Holds back the page's answer to the next navigate until `go` is set; `page` hears when it
    /// goes.
    fn hold_navigate(&self, go: &Arc<AtomicBool>, page: mpsc::Sender<&'static str>) {
        let (go, page) = (go.clone(), Mutex::new(page));
        self.window.hold("Page.navigate", move || {
            let going = go.load(Ordering::SeqCst);
            if going {
                page.lock().unwrap().send("page").unwrap();
            }
            going
        });
    }

    fn stop(mut self) {
        stop_with_no_browser_left(&mut self.obid);
    }
}

/// The test owner's device, which signs the owner's words.
fn device(obid: &Obid) -> String {
    let header = words_header(obid, "GET", "/", None);
    header.split(' ').next().unwrap().to_string()
}

/// The first event `GET /v1/browser/events` holds, from its start, that `matches`.
fn browser_event(obid: &Obid, matches: impl Fn(&Value) -> bool) -> Value {
    let mut stream = TcpStream::connect(("127.0.0.1", obid.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    write!(
        stream,
        "GET /v1/browser/events HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\nAuthorization: Bearer {TOKEN}\r\n\r\n"
    )
    .unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).unwrap_or(0);
        assert!(read > 0, "no such browser event");
        if let Some(event) = line
            .trim()
            .strip_prefix("data: ")
            .and_then(|d| serde_json::from_str::<Value>(d).ok())
            .filter(|e| matches(e))
        {
            return event;
        }
    }
}

#[test]
fn browser_5_a_signed_take_refuses_the_agents_act_human_control() {
    let agent = AgentTab::open("browser-5-take");
    let before = agent.shown()["holdGeneration"].as_u64().unwrap();
    let taken = agent.signed("take");
    assert_eq!(taken.status, 200, "{}", taken.text);
    assert_eq!(taken.body["tabId"], agent.tab.as_str());
    assert_eq!(taken.body["heldBy"], device(&agent.obid));
    assert!(
        taken.body["holdGeneration"].as_u64().unwrap() > before,
        "{}",
        taken.text
    );
    let navigates = agent.window.sent("Page.navigate");
    let refused = agent.navigate();
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(refused["error"]["code"], "human_control", "{refused}");
    assert_eq!(
        agent.window.sent("Page.navigate"),
        navigates,
        "no navigate reached the page"
    );
    browser_event(&agent.obid, |e| {
        e["type"] == "action"
            && e["tool"] == "oby_browser_navigate"
            && e["outcome"] == "refused"
            && e["reason"] == "human_control"
    });
    agent.stop();
}

#[test]
fn browser_5_while_the_owner_holds_a_tab_the_agents_reads_still_answer() {
    let agent = AgentTab::open("browser-5-reads");
    let taken = agent.signed("take");
    assert_eq!(taken.status, 200, "{}", taken.text);
    let read = agent.tool("oby_browser_snapshot", json!({}));
    assert_eq!(read["ok"], true, "{read}");
    agent.stop();
}

#[test]
fn browser_5_give_back_lets_the_agents_next_act_run() {
    let agent = AgentTab::open("browser-5-give-back");
    let taken = agent.signed("take");
    assert_eq!(taken.status, 200, "{}", taken.text);
    let given = agent.signed("giveBack");
    assert_eq!(given.status, 200, "{}", given.text);
    assert_eq!(given.body["heldBy"], Value::Null, "{}", given.text);
    let navigates = agent.window.sent("Page.navigate");
    let act = agent.navigate();
    assert_eq!(act["ok"], true, "{act}");
    assert_eq!(agent.window.sent("Page.navigate"), navigates + 1);
    agent.stop();
}

#[test]
fn browser_6_a_take_during_an_act_answers_once_the_act_ends() {
    let agent = AgentTab::open("browser-6-wait");
    let go = Arc::new(AtomicBool::new(false));
    let (done, order) = mpsc::channel();
    // The act ends when the page answers its navigate: the stand-in says so, then answers.
    agent.hold_navigate(&go, done.clone());
    let navigates = agent.window.sent("Page.navigate");
    let acting = agent.navigate_on_thread(done.clone());
    within(30, "the act reaches the page", || {
        (agent.window.sent("Page.navigate") > navigates).then_some(())
    });
    let taking = agent.take_on_thread(done);
    let device = device(&agent.obid);
    within(30, "the tab shows held while the act runs", || {
        (agent.shown()["heldBy"] == device.as_str()).then_some(())
    });
    assert!(order.try_recv().is_err(), "nothing answered yet");
    go.store(true, Ordering::SeqCst);
    let (status, act) = acting.join().unwrap();
    assert_eq!(status, 200, "{act}");
    assert_eq!(act["result"]["structuredContent"]["ok"], true, "{act}");
    let (status, taken) = taking.join().unwrap();
    assert_eq!(status, 200, "{taken}");
    assert_eq!(taken["heldBy"], device.as_str(), "{taken}");
    // The act counts as ended once the page answered it (`Api::done`), before its own answer is
    // shaped and sent: the take answers after the page's answer, the act's answer either side.
    let order: Vec<&str> = order.iter().take(3).collect();
    let at = |what| order.iter().position(|o| *o == what).unwrap();
    assert!(
        at("page") < at("take"),
        "the take answers after the act: {order:?}"
    );
    assert!(at("page") < at("act"), "{order:?}");
    agent.stop();
}

/// T3's epoch check (`SessionControl.take`): a give back while the device's own take still waits
/// for the agent's act ends that take, so the engine is never left holding a tab the API shows free.
#[test]
fn browser_6_a_give_back_while_its_take_waits_leaves_the_agent_free() {
    let agent = AgentTab::open("browser-6-give-back-waiting");
    let go = Arc::new(AtomicBool::new(false));
    let (done, order) = mpsc::channel();
    agent.hold_navigate(&go, done.clone());
    let navigates = agent.window.sent("Page.navigate");
    let acting = agent.navigate_on_thread(done.clone());
    within(30, "the act reaches the page", || {
        (agent.window.sent("Page.navigate") > navigates).then_some(())
    });
    let taking = agent.take_on_thread(done);
    let device = device(&agent.obid);
    within(30, "the tab shows held while the act runs", || {
        (agent.shown()["heldBy"] == device.as_str()).then_some(())
    });
    let given = agent.signed("giveBack");
    assert_eq!(given.status, 200, "{}", given.text);
    assert_eq!(given.body["heldBy"], Value::Null, "{}", given.text);
    assert!(order.try_recv().is_err(), "nothing answered yet");
    go.store(true, Ordering::SeqCst);
    let (status, act) = acting.join().unwrap();
    assert_eq!(status, 200, "{act}");
    assert_eq!(act["result"]["structuredContent"]["ok"], true, "{act}");
    let (status, taken) = taking.join().unwrap();
    assert_eq!(
        (status, taken["reason"].as_str()),
        (409, Some("control_changed")),
        "{taken}"
    );
    assert_eq!(agent.shown()["heldBy"], Value::Null);
    let navigates = agent.window.sent("Page.navigate");
    let next = agent.navigate();
    assert_eq!(next["ok"], true, "{next}");
    assert_eq!(agent.window.sent("Page.navigate"), navigates + 1);
    agent.stop();
}

#[test]
fn browser_5_an_unsigned_wrongly_signed_or_job_token_take_is_refused_and_nothing_is_held() {
    let agent = AgentTab::open("browser-5-refused");
    let free = |why: &str| {
        assert_eq!(agent.shown()["heldBy"], Value::Null, "{why}: nothing held");
        let act = agent.navigate();
        assert_eq!(act["ok"], true, "{why}: {act}");
    };
    for verb in ["take", "giveBack"] {
        let path = agent.path(verb);
        let unsigned = call(&agent.obid, "POST", &path, Some(&json!({})));
        assert_eq!(
            (unsigned.status, unsigned.body["reason"].as_str()),
            (401, Some("unsigned")),
            "{verb}: {}",
            unsigned.text
        );
        free("unsigned");
        let other = words_header(&agent.obid, "POST", &path, Some(&json!({"other": true})));
        let wrong = call_with_headers(
            &agent.obid,
            TOKEN,
            "POST",
            &path,
            Some(&json!({})),
            &[("X-Oby-Words", &other)],
        );
        assert_eq!(
            (wrong.status, wrong.body["reason"].as_str()),
            (401, Some("bad_signature")),
            "{verb}: {}",
            wrong.text
        );
        free("signed over another body");
        let words = words_header(&agent.obid, "POST", &path, Some(&json!({})));
        let job = call_with_headers(
            &agent.obid,
            &agent.token,
            "POST",
            &path,
            Some(&json!({})),
            &[("X-Oby-Words", &words)],
        );
        assert_eq!(
            (job.status, job.body["reason"].as_str()),
            (401, Some("bad_token")),
            "{verb}: {}",
            job.text
        );
        free("the job's token");
    }
    agent.stop();
}

#[test]
fn browser_5_a_ref_from_before_a_take_is_stale_after_give_back() {
    let agent = AgentTab::open("browser-5-stale-ref");
    let snapshot = agent.tool("oby_browser_snapshot", json!({}));
    assert_eq!(snapshot["ok"], true, "{snapshot}");
    let click = json!({"action": "click", "ref": "e1"});
    for verb in ["take", "giveBack"] {
        let answer = agent.signed(verb);
        assert_eq!(answer.status, 200, "{verb}: {}", answer.text);
    }
    let clicks = agent.window.sent("Input.dispatchMouseEvent");
    let stale = agent.tool("oby_browser_act", click.clone());
    assert_eq!(stale["ok"], false, "{stale}");
    assert_eq!(stale["error"]["code"], "stale_ref", "{stale}");
    assert_eq!(agent.window.sent("Input.dispatchMouseEvent"), clicks);
    let snapshot = agent.tool("oby_browser_snapshot", json!({}));
    assert_eq!(snapshot["ok"], true, "{snapshot}");
    let act = agent.tool("oby_browser_act", click);
    assert_eq!(act["ok"], true, "{act}");
    assert!(agent.window.sent("Input.dispatchMouseEvent") > clicks);
    agent.stop();
}
