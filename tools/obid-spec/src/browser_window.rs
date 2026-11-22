//! A stand-in for Oby's window (docs/plans/browser-t3.md): what the Electron window's main process
//! does for obid's browser, so a scratch obid's agents drive real pages with no app running. It
//! follows obid's `GET /v1/browser/events` (an agent tab's `tab` event, with its partition, is a
//! webview to mount) and `GET /v1/browser/relay` (obid's CDP for each tab), and answers on
//! `POST /v1/browser/relay`. Each tab is a page in a headless Chrome that obid-spec runs, standing
//! in for the webview's debugger: test tooling, as the stand-in agents stand in for Claude Code.
//! Oby and obid never download or run a Chrome.
//!
//! It keeps the real window's guards, so a workflow fails when obid sends what the window refuses:
//! page-level CDP only (Page, Runtime, DOM, DOMSnapshot, Accessibility, Input, Network, Emulation,
//! Log; no session id), each tab in the partition obid named and only in
//! `persist:oby-agent-(claude|codex|cursor)` or `oby-box-<job>` (each partition a browser context
//! of its own), and downloads only into `<obid home>/browser/downloads/`. What it refused is kept
//! for `browser-window-seen`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::obid::Run;

/// The CDP domains the window lets through (its main's allowlist).
pub const DOMAINS: &[&str] = &[
    "Page",
    "Runtime",
    "DOM",
    "DOMSnapshot",
    "Accessibility",
    "Input",
    "Network",
    "Emulation",
    "Log",
];

/// Whether a partition is one the window takes: an agent's, kept, or a job's box.
pub fn partition_ok(p: &str) -> bool {
    if let Some(agent) = p.strip_prefix("persist:oby-agent-") {
        return matches!(agent, "claude" | "codex" | "cursor");
    }
    p.strip_prefix("oby-box-").is_some_and(|job| {
        !job.is_empty()
            && job
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

type Pending = Mutex<HashMap<u64, tokio::sync::oneshot::Sender<std::result::Result<Value, Value>>>>;

/// Chrome's DevTools connection.
struct Cdp {
    next: AtomicU64,
    pending: Pending,
    tx: tokio::sync::mpsc::UnboundedSender<String>,
}

impl Cdp {
    /// One command, at the browser (no session) or on a page's session: its result, or its error.
    async fn call(
        &self,
        session: Option<&str>,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, Value> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        self.pending.lock().unwrap().insert(id, tx);
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        if self.tx.send(msg.to_string()).is_err() {
            return Err(json!({"code": -32000, "message": "Chrome's connection is closed"}));
        }
        match tokio::time::timeout(Duration::from_secs(120), rx).await {
            Ok(Ok(answer)) => answer,
            _ => Err(json!({"code": -32000, "message": format!("{method}: no answer")})),
        }
    }
}

/// One mounted tab: its page's session in Chrome and its partition.
#[derive(Clone, Debug)]
struct Mounted {
    job: String,
    session: String,
    target: String,
    partition: String,
}

/// The stand-in window for one obid.
struct Window {
    base: String,
    token: String,
    /// `<obid home>/browser/downloads`: the only folder downloads go to.
    downloads: PathBuf,
    cdp: Arc<Cdp>,
    tabs: Mutex<HashMap<String, Mounted>>,
    /// Each partition's browser context.
    contexts: Mutex<HashMap<String, String>>,
    /// What it refused: {what, tab, detail}.
    refused: Mutex<Vec<Value>>,
    /// The events for obid, sent in order, one request in flight.
    out: tokio::sync::mpsc::UnboundedSender<Value>,
    _chrome: tokio::sync::Mutex<tokio::process::Child>,
}

static WINDOWS: Mutex<Option<HashMap<String, Arc<Window>>>> = Mutex::new(None);

fn window_of(base: &str) -> Option<Arc<Window>> {
    WINDOWS.lock().unwrap().as_ref()?.get(base).cloned()
}

impl Window {
    fn refuse(&self, what: &str, tab: &str, detail: &str) {
        self.refused
            .lock()
            .unwrap()
            .push(json!({"what": what, "tab": tab, "detail": detail}));
    }

    fn post(&self, event: Value) {
        let _ = self.out.send(event);
    }

    /// The session a page event came on: the tab it is.
    fn tab_of_session(&self, session: &str) -> Option<(String, String)> {
        self.tabs
            .lock()
            .unwrap()
            .iter()
            .find(|(_, m)| m.session == session)
            .map(|(id, m)| (id.clone(), m.job.clone()))
    }

    /// An agent tab obid showed: mounted once, in the partition it named.
    async fn mount(self: &Arc<Self>, tab: &Value) {
        let Some(id) = tab["id"].as_str() else { return };
        let Some(job) = tab["owner"]["job"].as_str() else {
            return;
        };
        let Some(partition) = tab["partition"].as_str() else {
            return;
        };
        if self.tabs.lock().unwrap().contains_key(id) {
            return;
        }
        if !partition_ok(partition) {
            self.refuse("partition", id, partition);
            self.post(json!({"type": "detached", "threadId": job, "tabId": id}));
            return;
        }
        let url = tab["url"]
            .as_str()
            .filter(|u| !u.is_empty())
            .unwrap_or("about:blank");
        match self.open(id, job, partition, url).await {
            Ok(()) => {
                self.post(json!({"type": "attached", "threadId": job, "tabId": id}));
                self.post(json!({"type": "shown", "threadId": job, "tabId": id, "shown": true}));
            }
            Err(e) => {
                self.refuse("open", id, &format!("{e:#}"));
                self.post(json!({"type": "detached", "threadId": job, "tabId": id}));
            }
        }
    }

    async fn open(&self, id: &str, job: &str, partition: &str, url: &str) -> Result<()> {
        let context = {
            let known = self.contexts.lock().unwrap().get(partition).cloned();
            match known {
                Some(c) => c,
                None => {
                    let made = self
                        .cdp
                        .call(
                            None,
                            "Target.createBrowserContext",
                            json!({"disposeOnDetach": false}),
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("createBrowserContext: {e}"))?;
                    let c = made["browserContextId"]
                        .as_str()
                        .context("its id")?
                        .to_string();
                    self.contexts
                        .lock()
                        .unwrap()
                        .insert(partition.into(), c.clone());
                    c
                }
            }
        };
        let target = self
            .cdp
            .call(
                None,
                "Target.createTarget",
                json!({"url": "about:blank", "browserContextId": context}),
            )
            .await
            .map_err(|e| anyhow::anyhow!("createTarget: {e}"))?["targetId"]
            .as_str()
            .context("the target's id")?
            .to_string();
        let session = self
            .cdp
            .call(
                None,
                "Target.attachToTarget",
                json!({"targetId": target, "flatten": true}),
            )
            .await
            .map_err(|e| anyhow::anyhow!("attachToTarget: {e}"))?["sessionId"]
            .as_str()
            .context("the session's id")?
            .to_string();
        self.tabs.lock().unwrap().insert(
            id.into(),
            Mounted {
                job: job.into(),
                session: session.clone(),
                target,
                partition: partition.into(),
            },
        );
        // As main does: the webview loads its `src`, and `attached` follows its first load.
        if url != "about:blank" {
            let _ = self
                .cdp
                .call(Some(&session), "Page.navigate", json!({"url": url}))
                .await;
            for _ in 0..50 {
                let ready = self
                    .cdp
                    .call(
                        Some(&session),
                        "Runtime.evaluate",
                        json!({"expression": "document.readyState", "returnByValue": true}),
                    )
                    .await
                    .ok()
                    .and_then(|v| v["result"]["value"].as_str().map(str::to_string));
                if ready.as_deref() == Some("complete") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        Ok(())
    }

    /// A tab obid closed: its page goes, and a box partition's context with its last tab.
    async fn unmount(&self, id: &str) {
        let Some(m) = self.tabs.lock().unwrap().remove(id) else {
            return;
        };
        let _ = self
            .cdp
            .call(None, "Target.closeTarget", json!({"targetId": m.target}))
            .await;
        let last = !self
            .tabs
            .lock()
            .unwrap()
            .values()
            .any(|t| t.partition == m.partition);
        if last && m.partition.starts_with("oby-box-") {
            let context = self.contexts.lock().unwrap().remove(&m.partition);
            if let Some(c) = context {
                let _ = self
                    .cdp
                    .call(
                        None,
                        "Target.disposeBrowserContext",
                        json!({"browserContextId": c}),
                    )
                    .await;
            }
        }
    }

    /// One relay command from obid.
    async fn command(self: &Arc<Self>, c: Value) {
        match c["type"].as_str() {
            Some("cdp") => {
                let tab = c["tabId"].as_str().unwrap_or_default().to_string();
                let job = c["threadId"].as_str().unwrap_or_default().to_string();
                let Ok(frame) =
                    serde_json::from_str::<Value>(c["message"].as_str().unwrap_or_default())
                else {
                    return;
                };
                let me = self.clone();
                // Each command runs apart, so a slow one (a navigation) never holds the next.
                tokio::spawn(async move { me.cdp_command(&job, &tab, frame).await });
            }
            Some("release" | "pointer" | "turn" | "box") => {}
            other => self.refuse("command", "", &format!("{other:?}")),
        }
    }

    async fn cdp_command(&self, job: &str, tab: &str, frame: Value) {
        let id = frame["id"].clone();
        let method = frame["method"].as_str().unwrap_or_default().to_string();
        let params = frame["params"].clone();
        let answer = |result: std::result::Result<Value, Value>| {
            let message = match result {
                Ok(r) => json!({"id": id, "result": r}),
                Err(e) => json!({"id": id, "error": e}),
            };
            json!({"type": "cdp", "threadId": job, "tabId": tab, "message": message.to_string()})
        };
        let refuse = |why: &str| Err(json!({"code": -32601, "message": why}));
        let domain = method.split('.').next().unwrap_or_default();
        let session = self
            .tabs
            .lock()
            .unwrap()
            .get(tab)
            .map(|m| m.session.clone());
        let result = if frame.get("sessionId").is_some() {
            self.refuse("session", tab, &method);
            refuse("obid's browser speaks page-level CDP only: no session ids")
        } else if !DOMAINS.contains(&domain) {
            self.refuse("domain", tab, &method);
            refuse(&format!("{domain} isn't a domain the window lets through"))
        } else if let Some(session) = session {
            if method == "Page.setDownloadBehavior" {
                self.download_behavior(tab, &session, params).await
            } else {
                self.cdp.call(Some(&session), &method, params).await
            }
        } else {
            Err(json!({"code": -32000, "message": "that tab isn't attached"}))
        };
        self.post(answer(result));
    }

    /// Downloads only into obid's own folder, as main checks: the tab's context saves there.
    async fn download_behavior(
        &self,
        tab: &str,
        session: &str,
        params: Value,
    ) -> std::result::Result<Value, Value> {
        let path = PathBuf::from(params["downloadPath"].as_str().unwrap_or_default());
        let inside = path.starts_with(&self.downloads)
            && !path
                .components()
                .any(|c| c == std::path::Component::ParentDir);
        if params["behavior"] == "allow" && !inside {
            self.refuse("downloadPath", tab, &path.to_string_lossy());
            return Err(
                json!({"code": -32000, "message": "downloads go only into obid's own folder"}),
            );
        }
        let _ = std::fs::create_dir_all(&path);
        let context = {
            let tabs = self.tabs.lock().unwrap();
            let partition = tabs
                .get(tab)
                .map(|m| m.partition.clone())
                .unwrap_or_default();
            drop(tabs);
            self.contexts.lock().unwrap().get(&partition).cloned()
        };
        let _ = session;
        self.cdp
            .call(
                None,
                "Browser.setDownloadBehavior",
                json!({"behavior": params["behavior"], "downloadPath": path,
                       "browserContextId": context, "eventsEnabled": true}),
            )
            .await
    }

    /// An event from Chrome: a page's, for its tab, without its session; a download's, for the
    /// tabs of its context, as `Page.download*` (as a webview's debugger says it).
    fn chrome_event(&self, v: &Value) {
        let method = v["method"].as_str().unwrap_or_default();
        if let Some(rest) = method.strip_prefix("Browser.download") {
            let tabs: Vec<(String, String)> = self
                .tabs
                .lock()
                .unwrap()
                .iter()
                .map(|(id, m)| (id.clone(), m.job.clone()))
                .collect();
            for (tab, job) in tabs {
                let message =
                    json!({"method": format!("Page.download{rest}"), "params": v["params"]});
                self.post(json!({"type": "cdp", "threadId": job, "tabId": tab, "message": message.to_string()}));
            }
            return;
        }
        let Some(session) = v["sessionId"].as_str() else {
            return;
        };
        let Some((tab, job)) = self.tab_of_session(session) else {
            return;
        };
        if method == "Target.detachedFromTarget" {
            self.post(json!({"type": "detached", "threadId": job, "tabId": tab}));
            return;
        }
        let message = json!({"method": method, "params": v["params"]});
        self.post(
            json!({"type": "cdp", "threadId": job, "tabId": tab, "message": message.to_string()}),
        );
    }
}

/// Opens a streaming `GET` on obid with its token; each SSE event's data goes to `on`.
async fn follow_sse(
    base: &str,
    token: &str,
    path: &str,
    on: tokio::sync::mpsc::UnboundedSender<(String, Value)>,
) -> Result<()> {
    let host = crate::http::loopback(base)?;
    let mut stream = tokio::net::TcpStream::connect(&host).await?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nAccept: text/event-stream\r\n\r\n")
                .as_bytes(),
        )
        .await?;
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut status))
        .await
        .with_context(|| format!("{path} answered nothing"))??;
    anyhow::ensure!(status.contains(" 200"), "{path} answered {}", status.trim());
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        if line.trim().is_empty() {
            break;
        }
    }
    tokio::spawn(async move {
        let (mut name, mut data) = (String::new(), String::new());
        let mut line = String::new();
        while reader.read_line(&mut line).await.is_ok_and(|n| n > 0) {
            let l = line.trim_end_matches(['\r', '\n']);
            if l.is_empty() {
                if !data.is_empty() {
                    let v = serde_json::from_str(&data).unwrap_or(Value::Null);
                    if on.send((name.clone(), v)).is_err() {
                        return;
                    }
                }
                name.clear();
                data.clear();
            } else if let Some(v) = l.strip_prefix("event:") {
                name = v.trim().to_string();
            } else if let Some(v) = l.strip_prefix("data:") {
                data.push_str(v.strip_prefix(' ').unwrap_or(v));
            }
            line.clear();
        }
    });
    Ok(())
}

/// `browser-window`: the stand-in window for `obid`, started once (its Chrome, its streams).
async fn start(run: &Run, input: &Value) -> Result<Value> {
    let obid = &input["obid"];
    let base = crate::sphere::str_of(obid, "base")?.to_string();
    let token = crate::sphere::str_of(obid, "token")?.to_string();
    let home = PathBuf::from(crate::sphere::str_of(obid, "home")?);
    if window_of(&base).is_some() {
        return Ok(json!({"window": true, "again": true}));
    }
    let exe = match crate::lab::browser::install_shell().await? {
        Ok(p) => p,
        Err(why) => bail!("the test window's Chrome isn't there: {why}"),
    };
    let dir = run
        .scratch
        .join(format!("test-window-{}", uuid::Uuid::new_v4()));
    crate::obid::private_dir(&dir)?;
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", dir.display()))
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--mute-audio",
            "--disable-gpu",
            "about:blank",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if cfg!(target_os = "linux") {
        cmd.arg("--no-sandbox");
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("{} didn't start", exe.display()))?;
    let stderr = child.stderr.take().context("Chrome's stderr")?;
    let mut lines = tokio::io::BufReader::new(stderr).lines();
    let ws = tokio::time::timeout(Duration::from_secs(30), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(at) = line.find("ws://") {
                return Some(line[at..].trim().to_string());
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
    .context("Chrome didn't say where its DevTools listen")?;
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    let (socket, _) = tokio_tungstenite::connect_async(ws.as_str())
        .await
        .context("Chrome's DevTools didn't take the connection")?;
    let (mut write, mut read) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            if write
                .send(tokio_tungstenite::tungstenite::Message::Text(text))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let cdp = Arc::new(Cdp {
        next: AtomicU64::new(1),
        pending: Mutex::new(HashMap::new()),
        tx,
    });
    let (out, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let window = Arc::new(Window {
        base: base.clone(),
        token: token.clone(),
        downloads: home.join("browser").join("downloads"),
        cdp: cdp.clone(),
        tabs: Mutex::new(HashMap::new()),
        contexts: Mutex::new(HashMap::new()),
        refused: Mutex::new(vec![]),
        out,
        _chrome: tokio::sync::Mutex::new(child),
    });
    // Chrome's answers and events.
    let reading = window.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            let tokio_tungstenite::tungstenite::Message::Text(text) = msg else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if let Some(id) = v["id"].as_u64() {
                if let Some(tx) = reading.cdp.pending.lock().unwrap().remove(&id) {
                    let _ = tx.send(match v.get("error") {
                        Some(e) => Err(e.clone()),
                        None => Ok(v["result"].clone()),
                    });
                }
                continue;
            }
            reading.chrome_event(&v);
        }
    });
    // What goes to obid: in order, one request in flight, as main sends it.
    let posting = window.clone();
    tokio::spawn(async move {
        while let Some(first) = out_rx.recv().await {
            let mut events = vec![first];
            while let Ok(more) = out_rx.try_recv() {
                events.push(more);
            }
            let request = crate::http::Request {
                method: "POST".into(),
                base: posting.base.clone(),
                target: "/v1/browser/relay".into(),
                headers: vec![("Authorization".into(), format!("Bearer {}", posting.token))],
                body: Some(json!({"events": events}).to_string().into_bytes()),
                content_type: Some("application/json".into()),
            };
            let _ = crate::http::send(&request, Duration::from_secs(30)).await;
        }
    });
    // obid's tabs: an agent tab with a partition is a webview to mount; a closed one goes.
    let (tabs_tx, mut tabs_rx) = tokio::sync::mpsc::unbounded_channel();
    follow_sse(&base, &token, "/v1/browser/events", tabs_tx).await?;
    let mounting = window.clone();
    tokio::spawn(async move {
        while let Some((name, v)) = tabs_rx.recv().await {
            match name.as_str() {
                "tab" => {
                    let me = mounting.clone();
                    tokio::spawn(async move { me.mount(&v["tab"]).await });
                }
                "tabClosed" => {
                    if let Some(id) = v["tabId"].as_str() {
                        mounting.unmount(id).await;
                    }
                }
                _ => {}
            }
        }
    });
    // obid's commands.
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    follow_sse(&base, &token, "/v1/browser/relay", cmd_tx).await?;
    let commanding = window.clone();
    tokio::spawn(async move {
        while let Some((_, v)) = cmd_rx.recv().await {
            commanding.command(v).await;
        }
    });
    WINDOWS
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(base.clone(), window);
    Ok(json!({"window": true}))
}

/// `browser-window-seen`: what the stand-in window refused, and each tab's partition.
fn seen(input: &Value) -> Result<Value> {
    let base = crate::sphere::str_of(&input["obid"], "base")?;
    let w = window_of(base).context("no test window for that obid (browser-window first)")?;
    let tabs: serde_json::Map<String, Value> = w
        .tabs
        .lock()
        .unwrap()
        .iter()
        .map(|(id, m)| (id.clone(), json!(m.partition)))
        .collect();
    let refused = w.refused.lock().unwrap().clone();
    Ok(
        json!({"refused": refused, "partitions": tabs, "contexts": w.contexts.lock().unwrap().len()}),
    )
}

/// The stand-in window's nodes.
pub async fn run_node(run: &Run, name: &str, input: &Value) -> Option<Result<Value>> {
    Some(match name {
        "browser-window" => start(run, input).await,
        "browser-window-seen" => seen(input),
        _ => return None,
    })
}

/// The workflow ended: every stand-in window and its Chrome go. How many there were.
pub fn cleanup() -> usize {
    WINDOWS.lock().unwrap().take().map(|w| w.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_window_takes_only_the_two_partition_shapes() {
        for ok in [
            "persist:oby-agent-claude",
            "persist:oby-agent-cursor",
            "oby-box-1a2b-c",
        ] {
            assert!(partition_ok(ok), "{ok}");
        }
        for no in [
            "persist:oby-agent-gemini",
            "",
            "persist:oby",
            "default",
            "oby-box-",
            "oby-box-a/b",
        ] {
            assert!(!partition_ok(no), "{no}");
        }
    }
}
