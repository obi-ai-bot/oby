//! `GET /v1/events`, followed from a step on: every event obid sends is kept, so a later step
//! waits for one that may already have come. Asked as HTTP/1.0, so the stream is the body as it
//! is, with no chunk framing, until obid or the workflow ends it.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Clone, Debug)]
pub struct Event {
    pub name: String,
    pub data: Value,
    pub id: Option<String>,
}

type Log = Arc<Mutex<Vec<Event>>>;
static FOLLOWED: Mutex<Option<HashMap<String, Log>>> = Mutex::new(None);

fn log_of(base: &str) -> Option<Log> {
    FOLLOWED.lock().unwrap().as_ref()?.get(base).cloned()
}

/// Follows `base`'s events with `token` from now on; once per obid (again after its restart).
pub async fn follow(base: &str, token: &str) -> Result<Value> {
    let host = crate::http::loopback(base)?;
    let mut stream = tokio::net::TcpStream::connect(&host).await?;
    stream
        .write_all(
            format!("GET /v1/events HTTP/1.0\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nAccept: text/event-stream\r\n\r\n")
                .as_bytes(),
        )
        .await?;
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut status))
        .await
        .context("/v1/events answered nothing")??;
    let code: u16 = status
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    anyhow::ensure!(code == 200, "/v1/events answered {}", status.trim());
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        if line.trim().is_empty() {
            break;
        }
    }
    let log: Log = Arc::default();
    FOLLOWED
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(base.to_string(), log.clone());
    tokio::spawn(async move {
        let (mut name, mut data, mut id) = (String::new(), String::new(), None);
        let mut line = String::new();
        while reader.read_line(&mut line).await.is_ok_and(|n| n > 0) {
            let l = line.trim_end_matches(['\r', '\n']);
            if l.is_empty() {
                if !data.is_empty() || !name.is_empty() {
                    let value = serde_json::from_str(&data).unwrap_or(Value::String(data.clone()));
                    log.lock().unwrap().push(Event {
                        name: if name.is_empty() {
                            "message".into()
                        } else {
                            name.clone()
                        },
                        data: value,
                        id: id.take(),
                    });
                }
                name.clear();
                data.clear();
            } else if let Some(v) = l.strip_prefix("event:") {
                name = v.trim().to_string();
            } else if let Some(v) = l.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(v.strip_prefix(' ').unwrap_or(v));
            } else if let Some(v) = l.strip_prefix("id:") {
                id = Some(v.trim().to_string());
            }
            line.clear();
        }
    });
    Ok(json!({"following": base}))
}

/// The first event named `name` whose data matches `expect` (crate::expect), waiting up to
/// `seconds`; with `none`, that none comes in that time.
pub async fn wait(
    base: &str,
    name: &str,
    expect: &Value,
    seconds: u64,
    none: bool,
) -> Result<Value> {
    let log =
        log_of(base).context("this obid's events aren't followed: an events-follow step first")?;
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        let found = log.lock().unwrap().iter().find_map(|e| {
            (e.name == name && crate::expect::check(expect, &e.data, "").is_empty())
                .then(|| json!({"event": e.name, "data": e.data, "id": e.id}))
        });
        match (found, none) {
            (Some(e), false) => return Ok(e),
            (Some(e), true) => anyhow::bail!("a {name} event came: {}", e["data"]),
            (None, _) if Instant::now() > until => {
                if none {
                    return Ok(json!({"event": name, "none": true}));
                }
                let seen: Vec<String> = log
                    .lock()
                    .unwrap()
                    .iter()
                    .rev()
                    .filter(|e| e.name == name)
                    .take(3)
                    .map(|e| e.data.to_string().chars().take(300).collect())
                    .collect();
                anyhow::bail!("no {name} event matching within {seconds} s; the latest: {seen:?}");
            }
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
}
