//! One way into a computer through one of its routes (docs/contracts/remote.md, COMPUTERS-18, 22,
//! 25 and 27), for everything on this Mac that calls one: forwarding the window's requests, trying
//! its routes again, removing it, adding it, a page's tunnel to its loopback. A direct route is
//! HTTPS with its certificate pinned (crate::remote::pinned); an SSH route is plain HTTP to the other obid's gateway on its own
//! 127.0.0.1, through this computer's tunnel (crate::remote::tunnels): who answers there is the host
//! key the person accepted.
//!
//! It owns nothing: the tunnels are crate::remote::computers', the routes the computer's.

use std::time::Duration;

use serde_json::Value;

use super::pin::Pin;
use super::route::{Route, Via};
use super::ssh::Ssh;
use crate::api::Shared;

/// How long a route has to take a connection before the next is tried.
pub const CONNECT: Duration = Duration::from_secs(5);

/// The longest a whole answer takes (a stream runs as long as it's followed).
const WHOLE: Duration = Duration::from_secs(60);

/// The person's own ssh: their configuration, keys and agent, found on obid's `PATH`.
pub fn ssh() -> Ssh {
    Ssh {
        program: "ssh".into(),
        config: None,
        env: None,
    }
}

/// A client and the base URL a request to the computer goes to over one route.
pub struct Way {
    pub client: reqwest::Client,
    pub base: String,
}

/// Where a route reaches the computer: its base URL, and the certificate's pin a direct one's
/// must have; an SSH one's is plain, ssh carrying it.
pub struct Base {
    pub url: String,
    pub pin: Option<Pin>,
}

/// Where computer `id` is over `route`: `https://host:port` pinned, or this Mac's end of its SSH
/// tunnel, opened if it isn't running, never more than once a second.
pub async fn base(shared: &Shared, id: &str, route: &Route) -> Result<Base, String> {
    match &route.via {
        Via::Https { host, port, pin } => Ok(Base {
            url: format!("https://{host}:{port}"),
            pin: Some(*pin),
        }),
        Via::Ssh {
            destination,
            remote_port,
        } => {
            let local = shared
                .computers
                .tunnels
                .port(&ssh(), id, &route.id, destination, *remote_port)
                .await?;
            Ok(Base {
                url: format!("http://127.0.0.1:{local}"),
                pin: None,
            })
        }
    }
}

/// The way into computer `id` over `route` ([`base`]). `whole`: the answer comes whole, within a
/// minute; a stream's runs as long as it's followed.
pub async fn way(shared: &Shared, id: &str, route: &Route, whole: bool) -> Result<Way, String> {
    let base = base(shared, id, route).await?;
    let client = match base.pin {
        Some(pin) => super::pinned::streaming_client(pin, CONNECT, whole)?,
        None => plain(whole)?,
    };
    Ok(Way {
        client,
        base: base.url,
    })
}

/// A client for this Mac's own loopback end of a tunnel: no proxy, no TLS (ssh carries it).
pub fn plain(whole: bool) -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(CONNECT);
    let builder = if whole {
        builder.timeout(WHOLE)
    } else {
        builder
    };
    builder.build().map_err(|e| e.to_string())
}

/// Its status, when it's obid that answers, not merely something on the port (COMPUTERS-18).
pub async fn status(way: &Way, token: &str) -> Result<Value, String> {
    let answer = way
        .client
        .get(format!("{}/v1/status", way.base))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| super::pinned::said(&e))?;
    let code = answer.status();
    let body: Value = answer.json().await.unwrap_or(Value::Null);
    if body["runner"]["engine"] == "obid" {
        return Ok(body);
    }
    if code == reqwest::StatusCode::UNAUTHORIZED {
        return Err("it no longer accepts this Mac's token: add it again".into());
    }
    Err(format!(
        "something answered there, but not Oby ({})",
        code.as_u16()
    ))
}

/// `mac` or `linux`, from a status.
pub fn os(status: &Value) -> Option<String> {
    status["runner"]["os"]
        .as_str()
        .filter(|os| matches!(*os, "mac" | "linux"))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    /// A server on this machine that answers every request with `status` and `body`, as long as
    /// the test runs: the far end of a tunnel.
    async fn answers(status: &'static str, body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = socket.read(&mut buf).await;
                    let answer = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(answer.as_bytes()).await;
                });
            }
        });
        base
    }

    #[tokio::test]
    async fn computers_18_only_obid_answering_counts_not_something_on_the_port() {
        let way = |base: String| Way {
            client: plain(true).unwrap(),
            base,
        };
        let obid = way(answers("200 OK", r#"{"runner":{"engine":"obid","os":"linux"}}"#).await);
        let answered = status(&obid, "t").await.unwrap();
        assert_eq!(os(&answered).as_deref(), Some("linux"));
        let other = way(answers("200 OK", r#"{"hello":"world"}"#).await);
        let said = status(&other, "t").await.unwrap_err();
        assert!(said.contains("not Oby"), "{said}");
        let refused = way(answers("401 Unauthorized", r#"{"error":"no"}"#).await);
        let said = status(&refused, "t").await.unwrap_err();
        assert!(said.contains("token"), "{said}");
    }
}
