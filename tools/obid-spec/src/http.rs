//! A small HTTP/1.1 client over loopback, as obid's own tests call it: no TLS, nothing left open,
//! every request ends with the connection. Enough for obid (`127.0.0.1:<port>`) and the local
//! Sphere stand-in (`http://127.0.0.1:<port>`); nothing here reaches past this Mac.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Clone, Debug, Default)]
pub struct Request {
    pub method: String,
    /// `http://127.0.0.1:<port>` and nothing after it.
    pub base: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub content_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Answer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub text: String,
    pub body: Value,
}

/// The host and port of an `http://` base on this Mac; anything else is refused.
pub fn loopback(base: &str) -> Result<String> {
    let rest = base
        .strip_prefix("http://")
        .context("only http:// on this Mac")?;
    let host = rest.trim_end_matches('/');
    if !(host.starts_with("127.0.0.1:") || host.starts_with("localhost:")) {
        bail!("{base} isn't on this Mac");
    }
    Ok(host.replace("localhost", "127.0.0.1"))
}

pub async fn send(request: &Request, timeout: Duration) -> Result<Answer> {
    // A server that answers before reading the body (obid's gate refusing a request with no
    // token) can reset the connection before the answer is read: once, then the error stands.
    match send_once(request, timeout).await {
        Err(e) if is_reset(&e) => send_once(request, timeout).await,
        other => other,
    }
}

fn is_reset(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::ConnectionReset)
    })
}

async fn send_once(request: &Request, timeout: Duration) -> Result<Answer> {
    tokio::time::timeout(timeout, send_inner(request))
        .await
        .with_context(|| {
            format!(
                "{} {} answered nothing within {}s",
                request.method,
                request.target,
                timeout.as_secs()
            )
        })?
}

async fn send_inner(request: &Request) -> Result<Answer> {
    let host = loopback(&request.base)?;
    let mut stream = TcpStream::connect(&host)
        .await
        .with_context(|| format!("can't reach {host}"))?;
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n",
        request.method, request.target
    );
    for (name, value) in &request.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    match &request.body {
        Some(body) => {
            let kind = request
                .content_type
                .as_deref()
                .unwrap_or("application/json");
            head.push_str(&format!(
                "Content-Type: {kind}\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        None if matches!(request.method.as_str(), "POST" | "PUT" | "PATCH") => {
            head.push_str("Content-Length: 0\r\n")
        }
        None => {}
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    if let Some(body) = &request.body {
        stream.write_all(body).await?;
    }
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    parse(&raw)
}

pub fn parse(raw: &[u8]) -> Result<Answer> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("an answer with no head")?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let rest = &raw[split + 4..];
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .context("an answer with no status")?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.eq_ignore_ascii_case("chunked"));
    let bytes = if chunked {
        unchunk(rest)?
    } else {
        rest.to_vec()
    };
    let text = String::from_utf8_lossy(&bytes).to_string();
    let body = serde_json::from_str(text.trim()).unwrap_or(Value::Null);
    Ok(Answer {
        status,
        headers,
        text,
        body,
    })
}

fn unchunk(mut body: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line = body
            .windows(2)
            .position(|w| w == b"\r\n")
            .context("a chunk with no size")?;
        let size = usize::from_str_radix(
            String::from_utf8_lossy(&body[..line])
                .split(';')
                .next()
                .unwrap_or("")
                .trim(),
            16,
        )?;
        body = &body[line + 2..];
        if size == 0 {
            return Ok(out);
        }
        anyhow::ensure!(body.len() >= size, "a chunk cut short");
        out.extend_from_slice(&body[..size]);
        body = body.get(size + 2..).unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_chunked_answers() {
        let a = parse(b"HTTP/1.1 422 Unprocessable\r\ncontent-type: application/json\r\n\r\n{\"error\":\"x\"}").unwrap();
        assert_eq!(a.status, 422);
        assert_eq!(a.body["error"], "x");
        let b = parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n").unwrap();
        assert_eq!(b.body["a"], 1);
    }

    #[test]
    fn only_this_macs_loopback() {
        assert!(loopback("http://127.0.0.1:9").is_ok());
        assert!(loopback("https://example.com").is_err());
        assert!(loopback("http://10.0.0.1:80").is_err());
    }
}
