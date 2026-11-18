//! A Sphere that lies: a proxy in front of the run's local Sphere stand-in that passes every call
//! through as it is, except the directory, where it can serve a member's card re-signed with
//! another owner key, as a compromised Sphere could. The real Sphere never changes a bound owner
//! key, so this is how obid's pinning (SPHERE-8 to 10: a changed owner key is left out and asked
//! about) is proven black-box, against a real server otherwise.

use crate::http::{self, Request};
use crate::keys::OwnerKey;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

type Swaps = Arc<Mutex<HashMap<String, String>>>;
static LIARS: Mutex<Option<HashMap<String, Swaps>>> = Mutex::new(None);

/// Starts a proxy to `upstream` on a free port: its `http://127.0.0.1:<port>`.
pub async fn start(upstream: &str) -> Result<String> {
    crate::http::loopback(upstream)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let swaps: Swaps = Arc::default();
    LIARS
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(url.clone(), swaps.clone());
    let upstream = upstream.to_string();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (upstream, swaps) = (upstream.clone(), swaps.clone());
            tokio::spawn(async move {
                let _ = serve(stream, &upstream, &swaps).await;
            });
        }
    });
    Ok(url)
}

/// From now on the proxy at `url` serves `owner`'s cards signed by a new owner key (its seed).
pub fn swap(url: &str, owner: &str, seed: &str) -> Result<()> {
    let liars = LIARS.lock().unwrap();
    let swaps = liars
        .as_ref()
        .and_then(|l| l.get(url))
        .context("no lying Sphere at that URL in this workflow")?;
    swaps
        .lock()
        .unwrap()
        .insert(owner.to_string(), seed.to_string());
    Ok(())
}

async fn serve(stream: tokio::net::TcpStream, upstream: &str, swaps: &Swaps) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader.read_line(&mut first).await?;
    let mut parts = first.split_whitespace();
    let (method, target) = (
        parts.next().context("no method")?.to_string(),
        parts.next().context("no target")?.to_string(),
    );
    let mut headers = vec![];
    let mut length = 0usize;
    let mut content_type = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
            match k.as_str() {
                "content-length" => length = v.parse().unwrap_or(0),
                "content-type" => content_type = Some(v),
                "authorization" | "dpop" | "idempotency-key" => headers.push((k, v)),
                _ => {}
            }
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    let request = Request {
        method: method.clone(),
        base: upstream.to_string(),
        target: target.clone(),
        headers,
        body: (length > 0).then_some(body),
        content_type,
    };
    let answer = http::send(&request, Duration::from_secs(120)).await?;
    let mut text = answer.text.clone();
    let path = target.split('?').next().unwrap_or(&target);
    let directory = method == "GET"
        && path.starts_with("/v0.1/spheres/")
        && path.contains("/members")
        && answer.status == 200;
    if directory {
        let seeds = swaps.lock().unwrap().clone();
        if !seeds.is_empty() {
            let mut value = answer.body.clone();
            if let Some(list) = value.get_mut("members").and_then(Value::as_array_mut) {
                for m in list.iter_mut() {
                    lie(m, &seeds);
                }
            } else {
                lie(&mut value, &seeds);
            }
            text = value.to_string();
        }
    }
    let kind = answer
        .headers
        .iter()
        .find(|(k, _)| k == "content-type")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "application/json".into());
    let out = format!(
        "HTTP/1.1 {} X\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        answer.status,
        text.len()
    );
    let mut stream = reader.into_inner();
    stream.write_all(out.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

/// A member re-bound to the owner key the swap names: its owner key, its id, and its card
/// re-signed with it.
fn lie(member: &mut Value, seeds: &HashMap<String, String>) {
    let Some(seed) = member["owner"].as_str().and_then(|o| seeds.get(o)) else {
        return;
    };
    let Ok(key) = OwnerKey::from_hex(seed) else {
        return;
    };
    member["owner_key"] = json!(key.public());
    member["owner_key_id"] = json!(key.key_id());
    if let Some(params) = member["card"]["capabilities"]["extensions"][0]
        .get_mut("params")
        .and_then(Value::as_object_mut)
    {
        params.insert("owner_key".into(), json!(key.public()));
    }
    if let Ok(card) = key.sign_card(&member["card"]) {
        member["card"] = card;
    }
}
