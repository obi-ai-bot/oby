//! docs/contracts/cli.md, CLI-1 to CLI-8: the built `oby` against a stub runner on loopback.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TOKEN: &str = "6f1c0a2e9b8d4f3a7c5e1b0d2f4a6c8e9b1d3f5a7c9e0b2d";
const ROTATED: &str = "a7c9e0b2d6f1c0a2e9b8d4f3a7c5e1b0d2f4a6c8e9b1d3f5";

#[derive(Clone, Debug)]
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap()
    }
}

enum Reply {
    Json(u16, Value),
    /// Chunked, as obid's `/wait` sends: spaces, then the job.
    Chunked(Vec<String>),
    /// Spaces every second and never an answer, as `/wait` while the job runs.
    Hang,
}

struct Stub {
    url: String,
    seen: Arc<Mutex<Vec<Request>>>,
    connections: Arc<Mutex<usize>>,
}

impl Stub {
    fn requests(&self) -> Vec<Request> {
        self.seen.lock().unwrap().clone()
    }

    fn connections(&self) -> usize {
        *self.connections.lock().unwrap()
    }
}

fn stub(handler: impl Fn(&Request) -> Reply + Send + Sync + 'static) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let connections = Arc::new(Mutex::new(0));
    let handler = Arc::new(handler);
    let (s, c) = (seen.clone(), connections.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            *c.lock().unwrap() += 1;
            let (seen, handler) = (s.clone(), handler.clone());
            std::thread::spawn(move || serve(stream, &seen, &*handler));
        }
    });
    Stub {
        url,
        seen,
        connections,
    }
}

fn serve(stream: TcpStream, seen: &Mutex<Vec<Request>>, handler: &dyn Fn(&Request) -> Reply) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (
        parts.next().unwrap().to_string(),
        parts.next().unwrap().to_string(),
    );
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').unwrap();
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let length = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .map_or(0, |(_, v)| v.parse::<usize>().unwrap());
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let request = Request {
        method,
        path,
        headers,
        body: String::from_utf8(body).unwrap(),
    };
    seen.lock().unwrap().push(request.clone());
    let mut stream = stream;
    match handler(&request) {
        Reply::Json(status, value) => {
            let body = value.to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
        }
        Reply::Chunked(parts) => {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n"
            );
            for part in parts {
                let _ = write!(stream, "{:x}\r\n{part}\r\n", part.len());
            }
            let _ = write!(stream, "0\r\n\r\n");
        }
        Reply::Hang => {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n"
            );
            for _ in 0..60 {
                if write!(stream, "1\r\n \r\n").is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

/// A home holding `api-token` with `mode`.
fn home(token: &str, mode: u32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    put_token(dir.path(), token, mode);
    dir
}

/// Written beside and renamed into place, as obid rotates it.
fn put_token(home: &Path, token: &str, mode: u32) {
    let new = home.join("api-token.new");
    std::fs::write(&new, format!("{token}\n")).unwrap();
    std::fs::set_permissions(&new, std::fs::Permissions::from_mode(mode)).unwrap();
    std::fs::rename(&new, home.join("api-token")).unwrap();
}

struct Ran {
    code: i32,
    stdout: String,
    stderr: String,
}

fn oby(home: &Path, url: &str, args: &[&str], stdin: &str) -> Ran {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oby"))
        .args(args)
        .env("OBID_HOME", home)
        .env("OBID_URL", url)
        // A proxy oby read would take every call away from the stub.
        .env("http_proxy", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let ran = Ran {
        code: output.status.code().unwrap(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    };
    for token in [TOKEN, ROTATED] {
        assert!(
            !ran.stdout.contains(token) && !ran.stderr.contains(token),
            "the token showed: {}",
            ran.stderr
        );
    }
    ran
}

fn job(id: &str, status: &str) -> Value {
    json!({"id": id, "status": status, "repo": "obi-ai-bot/oby", "branch": "oby/fix", "pr": null, "step": "Working\non it", "accountLimit": null})
}

/// A runner whose job `j1` is as `status` says, whose `/wait` answers as `wait` says, and whose inbox
/// holds `inbox`.
fn runner(status: Value, wait: Option<Value>, inbox: Value) -> Stub {
    stub(move |r| match (r.method.as_str(), r.path.as_str()) {
        ("GET", "/v1/jobs/j1/wait") => match &wait {
            Some(job) => Reply::Chunked(vec![" ".into(), " ".into(), job.to_string()]),
            None => Reply::Hang,
        },
        ("GET", "/v1/jobs/j1") => Reply::Json(200, status.clone()),
        ("GET", "/v1/inbox") => Reply::Json(200, inbox.clone()),
        _ => Reply::Json(404, json!({"error": "No such route."})),
    })
}

fn every_request_is_ours(stub: &Stub) {
    let host = stub.url.trim_start_matches("http://").to_string();
    for request in stub.requests() {
        assert_eq!(
            request.header("authorization"),
            Some(format!("Bearer {TOKEN}").as_str())
        );
        assert_eq!(request.header("host"), Some(host.as_str()));
        assert_eq!(request.header("x-oby-words"), None, "oby signs nothing");
    }
}

#[test]
fn cli_1_a_done_job_exits_0() {
    let jobs: Arc<Mutex<u32>> = Arc::default();
    let made = jobs.clone();
    let stub = stub(move |r| match (r.method.as_str(), r.path.as_str()) {
        ("POST", "/v1/jobs") => {
            *made.lock().unwrap() += 1;
            Reply::Json(201, job("j1", "queued"))
        }
        ("GET", "/v1/jobs/j1/wait") => {
            Reply::Chunked(vec![" ".into(), job("j1", "done").to_string()])
        }
        ("GET", "/v1/jobs/j1") => Reply::Json(200, job("j1", "running")),
        ("GET", "/v1/inbox") => Reply::Json(200, json!([])),
        _ => Reply::Json(404, json!({"error": "No such route."})),
    });
    let home = home(TOKEN, 0o600);
    let ran = oby(
        home.path(),
        &stub.url,
        &["run", "obi-ai-bot/oby", "fix the lint", "--wait"],
        "",
    );
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(ran.stdout.lines().any(|l| l == "job=j1"), "{}", ran.stdout);
    assert!(
        ran.stdout.lines().any(|l| l == "status=done"),
        "{}",
        ran.stdout
    );
    let posted: Vec<Request> = stub
        .requests()
        .into_iter()
        .filter(|r| r.method == "POST")
        .collect();
    assert_eq!(posted.len(), 1);
    assert_eq!(
        posted[0].json(),
        json!({"job": {"repo": "obi-ai-bot/oby", "kind": "fix", "task": "fix the lint"}})
    );
    assert_eq!(posted[0].header("content-type"), Some("application/json"));
    every_request_is_ours(&stub);

    // With --json, stdout is the ended Job alone; without --wait, the id at once.
    let ran = oby(
        home.path(),
        &stub.url,
        &["run", "obi-ai-bot/oby", "-", "--wait", "--json"],
        "fix the lint\n",
    );
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert_eq!(
        serde_json::from_str::<Value>(&ran.stdout).unwrap(),
        job("j1", "done")
    );
    let ran = oby(
        home.path(),
        &stub.url,
        &[
            "run",
            "obi-ai-bot/oby",
            "fix",
            "--kind",
            "investigate",
            "--agent",
            "codex",
        ],
        "",
    );
    assert_eq!((ran.code, ran.stdout.as_str()), (0, "job=j1\n"));
    let last = stub
        .requests()
        .into_iter()
        .filter(|r| r.method == "POST")
        .last()
        .unwrap();
    assert_eq!(
        last.json()["job"],
        json!({"repo": "obi-ai-bot/oby", "kind": "investigate", "task": "fix", "agent": "codex"})
    );
    assert_eq!(*jobs.lock().unwrap(), 3);
}

#[test]
fn cli_2_a_failed_or_stopped_job_exits_1() {
    for status in ["failed", "stopped", "interrupted"] {
        let stub = runner(job("j1", "running"), Some(job("j1", status)), json!([]));
        let home = home(TOKEN, 0o600);
        let ran = oby(home.path(), &stub.url, &["wait", "j1"], "");
        assert_eq!(ran.code, 1, "{status}: {}", ran.stderr);
        assert_eq!(ran.stdout, format!("job=j1\nstatus={status}\n"));
        every_request_is_ours(&stub);
    }
}

#[test]
fn cli_3_a_usage_error_exits_2_without_a_call() {
    let stub = runner(job("j1", "done"), Some(job("j1", "done")), json!([]));
    // A token file oby would refuse: had it been read, the exit would be 5.
    let home = home(TOKEN, 0o644);
    let cases: [(&[&str], &str); 11] = [
        (&["run"], ""),
        (&["run", "repo"], ""),
        (&["run", "repo", "task", "--bogus"], ""),
        (&["run", "repo", "task", "--timeout", "5x", "--wait"], ""),
        (&["run", "repo", "task", "--timeout", "5m"], ""),
        (&["run", "repo", "task", "--title"], ""),
        (&["run", "repo", ""], ""),
        (&["run", "repo", "-"], ""),
        (&["wait"], ""),
        (&["frob"], ""),
        (&[], ""),
    ];
    for (args, stdin) in cases {
        let ran = oby(home.path(), &stub.url, args, stdin);
        assert_eq!(ran.code, 2, "{args:?}: {}", ran.stderr);
        assert!(ran.stderr.contains("usage:"), "{args:?}: {}", ran.stderr);
        assert_eq!(ran.stdout, "", "{args:?}");
    }
    assert_eq!(stub.connections(), 0);
    let ran = oby(home.path(), &stub.url, &["--help"], "");
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.contains("usage:") && ran.stderr.is_empty());
}

#[test]
fn cli_4_a_job_waiting_for_the_owner_exits_3_naming_the_item() {
    for kind in ["permission", "question"] {
        let item =
            json!({"id": format!("j1-{kind}-0"), "kind": kind, "job": "j1", "state": "open"});
        let stub = runner(
            job("j1", "running"),
            None,
            json!([{"id": "j2-q", "kind": "question", "job": "j2", "state": "open"}, item]),
        );
        let home = home(TOKEN, 0o600);
        let ran = oby(home.path(), &stub.url, &["wait", "j1"], "");
        assert_eq!(ran.code, 3, "{kind}: {}", ran.stderr);
        assert_eq!(
            ran.stdout,
            format!("job=j1\nstatus=running\ninbox=j1-{kind}-0\nkind={kind}\n")
        );
        assert!(
            !stub.requests().iter().any(|r| r.method == "POST"),
            "oby answers nothing"
        );
        every_request_is_ours(&stub);
    }
    let mut limited = job("j1", "running");
    limited["accountLimit"] = json!({"decision": "limit-7", "state": "pending", "resetAt": null});
    let stub = runner(limited.clone(), None, json!([]));
    let home = home(TOKEN, 0o600);
    let ran = oby(home.path(), &stub.url, &["wait", "j1"], "");
    assert_eq!(ran.code, 3, "{}", ran.stderr);
    assert_eq!(
        ran.stdout,
        "job=j1\nstatus=running\ninbox=limit-7\nkind=limit\n"
    );
    // With --json the item is named on stderr, and stdout stays the Job alone.
    let ran = oby(home.path(), &stub.url, &["wait", "j1", "--json"], "");
    assert_eq!(ran.code, 3, "{}", ran.stderr);
    assert_eq!(serde_json::from_str::<Value>(&ran.stdout).unwrap(), limited);
    assert!(
        ran.stderr.contains("inbox=limit-7\nkind=limit"),
        "{}",
        ran.stderr
    );
}

#[test]
fn cli_5_a_timeout_exits_4_and_the_job_goes_on() {
    let stub = runner(job("j1", "running"), None, json!([]));
    let home = home(TOKEN, 0o600);
    let started = Instant::now();
    let ran = oby(
        home.path(),
        &stub.url,
        &["wait", "j1", "--timeout", "2s"],
        "",
    );
    let took = started.elapsed();
    assert_eq!(ran.code, 4, "{}", ran.stderr);
    assert!(
        took >= Duration::from_secs(2) && took < Duration::from_secs(10),
        "{took:?}"
    );
    assert_eq!(ran.stdout, "job=j1\nstatus=running\n");
    let requests = stub.requests();
    assert!(requests.iter().any(|r| r.path == "/v1/jobs/j1/wait"));
    assert!(
        !requests
            .iter()
            .any(|r| r.method == "POST" || r.path.contains("/stop")),
        "{requests:?}"
    );
}

#[test]
fn cli_6_no_obid_or_no_usable_token_exits_5_and_never_prints_it() {
    let status = json!({"runner": {"engine": "obid", "version": "0.2.0", "host": "mac"}});
    let answered = status.clone();
    let stub = stub(move |r| match r.path.as_str() {
        "/v1/status" => Reply::Json(200, answered.clone()),
        _ => Reply::Json(404, json!({"error": "No such route."})),
    });
    let good = home(TOKEN, 0o600);
    let ran = oby(good.path(), &stub.url, &["status"], "");
    assert_eq!(
        (ran.code, ran.stdout.as_str()),
        (0, "engine=obid\nversion=0.2.0\n"),
        "{}",
        ran.stderr
    );
    let ran = oby(good.path(), &stub.url, &["status", "--json"], "");
    assert_eq!(serde_json::from_str::<Value>(&ran.stdout).unwrap(), status);
    let before = stub.connections();

    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port())
    };
    let ran = oby(good.path(), &closed, &["status"], "");
    assert_eq!(ran.code, 5, "{}", ran.stderr);
    assert!(ran.stderr.contains("isn't answering"), "{}", ran.stderr);

    let empty = tempfile::tempdir().unwrap();
    let ran = oby(empty.path(), &stub.url, &["status"], "");
    assert_eq!(ran.code, 5);
    assert!(
        ran.stderr.contains("api-token") && ran.stderr.contains("no such file"),
        "{}",
        ran.stderr
    );
    for mode in [0o640, 0o604] {
        let exposed = home(TOKEN, mode);
        let ran = oby(exposed.path(), &stub.url, &["status"], "");
        assert_eq!(ran.code, 5, "{mode:o}");
        assert!(ran.stderr.contains("chmod 600"), "{}", ran.stderr);
    }
    let linked = tempfile::tempdir().unwrap();
    let real: PathBuf = linked.path().join("real-token");
    std::fs::write(&real, TOKEN).unwrap();
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink(&real, linked.path().join("api-token")).unwrap();
    let ran = oby(linked.path(), &stub.url, &["status"], "");
    assert_eq!(ran.code, 5);
    assert!(ran.stderr.contains("is a link"), "{}", ran.stderr);
    assert_eq!(stub.connections(), before, "a refused token went nowhere");

    // Not loopback: refused before the token file is read (this one would be refused too) or any
    // connection is made.
    let exposed = home(TOKEN, 0o644);
    let port = stub.url.rsplit(':').next().unwrap().to_string();
    for url in [
        "http://example.com:7777".to_string(),
        "https://127.0.0.1:7777".to_string(),
        format!("https://127.0.0.1:{port}"),
        format!("http://user@127.0.0.1:{port}"),
        format!("http://127.0.0.1:{port}/v1"),
    ] {
        for home in [good.path(), exposed.path()] {
            let ran = oby(home, &url, &["status"], "");
            assert_eq!(ran.code, 5, "{url}");
            assert!(
                ran.stderr.contains("only to http://127.0.0.1"),
                "{url}: {}",
                ran.stderr
            );
        }
    }
    assert_eq!(stub.connections(), before);

    let starting = stub_starting();
    let ran = oby(good.path(), &starting.url, &["status"], "");
    assert_eq!(ran.code, 5, "{}", ran.stderr);
    assert!(ran.stderr.contains("starting"), "{}", ran.stderr);
}

fn stub_starting() -> Stub {
    stub(|_| {
        Reply::Json(
            503,
            json!({"error": "Oby’s engine is starting.", "starting": {"ms": 3}}),
        )
    })
}

#[test]
fn cli_7_one_key_sent_twice_makes_one_job() {
    let made: Arc<Mutex<HashMap<String, Value>>> = Arc::default();
    let jobs = made.clone();
    let stub = stub(move |r| {
        let key = r.json()["job"]["clientRequestId"]
            .as_str()
            .unwrap()
            .to_string();
        let mut jobs = jobs.lock().unwrap();
        match jobs.get(&key) {
            Some(job) => Reply::Json(200, job.clone()),
            None => {
                let job = job(&format!("j{}", jobs.len() + 1), "queued");
                jobs.insert(key, job.clone());
                Reply::Json(201, job)
            }
        }
    });
    let home = home(TOKEN, 0o600);
    for _ in 0..2 {
        let ran = oby(
            home.path(),
            &stub.url,
            &["run", "repo", "task", "--id", "k1"],
            "",
        );
        assert_eq!(
            (ran.code, ran.stdout.as_str()),
            (0, "job=j1\n"),
            "{}",
            ran.stderr
        );
    }
    let requests = stub.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.json()["job"]["clientRequestId"], "k1");
        assert_eq!(request.header("idempotency-key"), None);
    }
    assert_eq!(made.lock().unwrap().len(), 1);
}

#[test]
fn cli_8_a_rotated_token_is_read_again() {
    let home = home(TOKEN, 0o600);
    let (path, rotated) = (home.path().to_path_buf(), Mutex::new(false));
    let stub = stub(move |r| {
        if r.header("authorization") != Some(format!("Bearer {ROTATED}").as_str()) {
            // obid rotated its token: the file holds the new one before the old one is refused.
            let mut rotated = rotated.lock().unwrap();
            if !*rotated {
                put_token(&path, ROTATED, 0o600);
                *rotated = true;
            }
            return Reply::Json(401, json!({"error": "Unauthorized"}));
        }
        match r.path.as_str() {
            "/v1/jobs/j1/wait" => Reply::Chunked(vec![job("j1", "done").to_string()]),
            "/v1/jobs/j1" => Reply::Json(200, job("j1", "running")),
            _ => Reply::Json(200, json!([])),
        }
    });
    let ran = oby(home.path(), &stub.url, &["wait", "j1"], "");
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert_eq!(ran.stdout, "job=j1\nstatus=done\n");
    let waits: Vec<Request> = stub
        .requests()
        .into_iter()
        .filter(|r| r.path == "/v1/jobs/j1/wait")
        .collect();
    assert_eq!(waits.len(), 2, "{waits:?}");
    assert_eq!(
        waits[0].header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(
        waits[1].header("authorization"),
        Some(format!("Bearer {ROTATED}").as_str())
    );

    // Refused again with the token read anew: exit 5, once more and no further.
    let refusing = stub(|_| Reply::Json(401, json!({"error": "Unauthorized"})));
    put_token(home.path(), TOKEN, 0o600);
    let ran = oby(home.path(), &refusing.url, &["wait", "j1"], "");
    assert_eq!(ran.code, 5, "{}", ran.stderr);
    assert!(ran.stderr.contains("refused the token"), "{}", ran.stderr);
    let waits = refusing
        .requests()
        .into_iter()
        .filter(|r| r.path == "/v1/jobs/j1/wait")
        .count();
    assert!((1..=2).contains(&waits), "{waits}");
    let ran = oby(home.path(), &refusing.url, &["status"], "");
    assert_eq!(ran.code, 5);
    assert_eq!(
        refusing
            .requests()
            .iter()
            .filter(|r| r.path == "/v1/status")
            .count(),
        2
    );
}
