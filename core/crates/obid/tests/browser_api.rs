//! The browser's API on the real obid (docs/contracts/browser.md), for what its workflows can't
//! easily show: the person's input from a device that doesn't hold the tab never reaching the page
//! (BROWSER-7), the agents' server refusing a missing, made-up or ended job's token (BROWSER-16),
//! a secret in no file under obid's home (BROWSER-8), and the switches kept across a restart
//! (BROWSER-15). obid runs the tests' stand-in agent-browser (`OBID_TEST_BROWSER_STAND_IN`, a debug
//! build's only), whose live stream is a WebSocket server of the test's own; a Codex job sleeping as
//! its agent hands back its broker token (tests/agents/fake-codex, `SLEEP TOKEN FILE`).
mod support;

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use support::*;

/// A scratch obid on the stand-in agent-browser, with a Codex account for its jobs.
fn browser_obid(folder: &Folder) -> Obid {
    start_with(&folder.0, &browser_config(folder), None, &stand_in_env())
}

fn stand_in_env() -> [(&'static str, &'static str); 1] {
    [("OBID_TEST_BROWSER_STAND_IN", "1")]
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

/// A second device of the owner's, its own key: registered as the Mac app registers one.
struct Device {
    id: String,
    key: p256::ecdsa::SigningKey,
}

impl Device {
    fn register(obid: &Obid, seed: u8) -> Device {
        use base64::Engine as _;
        use p256::elliptic_curve::sec1::ToEncodedPoint as _;
        let secret = p256::SecretKey::from_slice(&[seed; 32]).unwrap();
        let public = base64::engine::general_purpose::STANDARD
            .encode(secret.public_key().to_encoded_point(false).as_bytes());
        std::fs::write(obid.home.join("pairing"), &public).unwrap();
        let added = call(
            obid,
            "POST",
            "/v1/devices",
            Some(&json!({"name": "Test owner's other device", "publicKey": public})),
        );
        assert_eq!(added.status, 200, "{}", added.text);
        Device {
            id: added.body["id"].as_str().unwrap().to_string(),
            key: p256::ecdsa::SigningKey::from(secret),
        }
    }

    /// `X-Oby-Words` for that request, as the apps sign it.
    fn words(&self, method: &str, path: &str, body: &Value) -> String {
        use base64::Engine as _;
        use p256::ecdsa::signature::Signer as _;
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_millis();
        let whole = obi_protocol::time::format(obi_protocol::time::now());
        let at = format!("{}.{millis:03}Z", &whole[..19]);
        let digest = hex::encode(obi_protocol::keys::sha256(body.to_string().as_bytes()));
        let message = format!("oby-words\n{method}\n{path}\n{digest}\n{at}");
        let signature: p256::ecdsa::Signature = self.key.sign(message.as_bytes());
        format!(
            "{} {at} {}",
            self.id,
            base64::engine::general_purpose::STANDARD.encode(signature.to_der())
        )
    }

    fn post(&self, obid: &Obid, path: &str, body: &Value) -> Answer {
        let words = self.words("POST", path, body);
        call_with_headers(
            obid,
            TOKEN,
            "POST",
            path,
            Some(body),
            &[("X-Oby-Words", &words)],
        )
    }
}

/// agent-browser's live stream, stood in: a WebSocket server on 127.0.0.1 that sends nothing and
/// hands back every message it gets. Its port goes where the stand-in agent-browser's `stream
/// status` reads it, the job's browser's own `HOME`.
fn stream_server(home: &Path, job: &str) -> mpsc::Receiver<Value> {
    let (tx, rx) = mpsc::channel();
    let (port_tx, port_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            use futures_util::StreamExt as _;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            port_tx.send(listener.local_addr().unwrap().port()).unwrap();
            while let Ok((conn, _)) = listener.accept().await {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let Ok(socket) = tokio_tungstenite::accept_async(conn).await else {
                        return;
                    };
                    let (_out, mut incoming) = socket.split();
                    while let Some(Ok(m)) = incoming.next().await {
                        if let tokio_tungstenite::tungstenite::Message::Text(t) = m
                            && let Ok(v) = serde_json::from_str::<Value>(&t)
                        {
                            let _ = tx.send(v);
                        }
                    }
                });
            }
        });
    });
    let port = port_rx.recv().unwrap();
    let dir = home.join("browser/jobs").join(job).join("home");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("stream-port"), port.to_string()).unwrap();
    rx
}

/// A job with a tab open on the stand-in, its stream stood in, held by the test owner's device.
fn held_tab(obid: &Obid) -> (String, String, String, mpsc::Receiver<Value>) {
    let (job, token) = sleeping_job(obid);
    let opened = tool(
        obid,
        &token,
        "oby_browser_open",
        json!({"url": "http://127.0.0.1:1/"}),
    );
    assert_eq!(opened["ok"], true, "{opened}");
    let tab = opened["result"]["tab"].as_str().unwrap().to_string();
    let got = stream_server(&obid.home, &job);
    let take = format!("/v1/browser/tabs/{tab}/take");
    let taken = call_signed(obid, "POST", &take, Some(&json!({})));
    assert_eq!(taken.status, 200, "{}", taken.text);
    (job, token, tab, got)
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

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        match std::fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() => files_under(&p, out),
            Ok(m) if m.is_file() => out.push(p),
            _ => {}
        }
    }
}

#[test]
fn browser_7_another_devices_input_never_reaches_the_page_and_the_holders_does() {
    let folder = Folder::new("browser-7");
    let mut obid = browser_obid(&folder);
    let (_job, _token, tab, got) = held_tab(&obid);
    let other = Device::register(&obid, 7);
    let input = format!("/v1/browser/tabs/{tab}/input");
    let click = |x: f64| json!({"type": "mouse", "eventType": "mousePressed", "x": x, "y": 5, "button": "left", "clickCount": 1});
    // Device B, registered and signing, doesn't hold the tab: refused, and nothing is sent.
    let refused = other.post(&obid, &input, &click(1.0));
    assert_eq!(refused.status, 403, "{}", refused.text);
    assert_eq!(refused.body["reason"], "not_holder");
    // Unsigned input is refused too.
    let unsigned = call(&obid, "POST", &input, Some(&click(1.0)));
    assert_eq!(unsigned.status, 401, "{}", unsigned.text);
    assert_eq!(unsigned.body["reason"], "unsigned");
    // Device A, the holder: its input reaches the page, and it is the first thing that does.
    let reached = call_signed(&obid, "POST", &input, Some(&click(2.0)));
    assert_eq!(reached.status, 200, "{}", reached.text);
    let first = within(10, "the holder's input reaches the stream", || {
        got.try_recv().ok()
    });
    assert_eq!(first["type"], "input_mouse", "{first}");
    assert_eq!(first["x"], 2.0, "B's input reached the page first: {first}");
    stop_with_no_browser_left(&mut obid);
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

#[test]
fn browser_8_a_secret_goes_to_the_page_and_is_in_no_file_under_obids_home() {
    let folder = Folder::new("browser-8");
    let mut obid = browser_obid(&folder);
    let (_job, _token, tab, got) = held_tab(&obid);
    let value = format!(
        "browser-8-{}",
        hex::encode(obi_protocol::keys::sha256(
            format!("{:?}", std::time::SystemTime::now()).as_bytes()
        ))
    );
    let path = format!("/v1/browser/tabs/{tab}/secret");
    let answer = call_signed(
        &obid,
        "POST",
        &path,
        Some(&json!({"field": "#pw", "value": value})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert_eq!(answer.body["filled"], true);
    assert!(!answer.text.contains(&value), "the answer has the secret");
    // The page got it, key by key, after a click on the field.
    let mut typed = String::new();
    within(10, "every key of the secret reaches the page", || {
        while let Ok(m) = got.try_recv() {
            if m["type"] == "input_keyboard" && m["eventType"] == "keyDown" {
                typed.push_str(m["key"].as_str().unwrap_or_default());
            }
        }
        (typed == value).then_some(())
    });
    // Nothing obid wrote has it: no file under its folder (its home, the browser's, its log).
    let mut files = vec![];
    files_under(&obid.folder, &mut files);
    assert!(!files.is_empty());
    let leaked: Vec<&PathBuf> = files
        .iter()
        .filter(|f| {
            std::fs::read(f)
                .is_ok_and(|bytes| bytes.windows(value.len()).any(|w| w == value.as_bytes()))
        })
        .collect();
    assert!(leaked.is_empty(), "the secret is in {leaked:?}");
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
    let mut again = start_with(&folder.0, &config, None, &stand_in_env());
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
