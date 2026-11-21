//! One way into a computer through one of its routes (docs/contracts/remote.md, COMPUTERS-18, 22
//! and 25), for everything on this Mac that calls one: forwarding the window's requests, trying its
//! routes again, removing it, adding it. A direct route is HTTPS with its certificate pinned
//! (crate::remote::pinned); an SSH route is plain HTTP to the other obid's gateway on its own
//! 127.0.0.1, through this computer's tunnel (crate::remote::tunnels): who answers there is the host
//! key the person accepted.
//!
//! It owns nothing: the tunnels are crate::remote::computers', the routes the computer's.

use std::time::Duration;

use serde_json::Value;

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

/// The way into computer `id` over `route`. `whole`: the answer comes whole, within a minute; a
/// stream's runs as long as it's followed. An SSH route's tunnel is opened if it isn't running,
/// never more than once a second.
pub async fn way(shared: &Shared, id: &str, route: &Route, whole: bool) -> Result<Way, String> {
    match &route.via {
        Via::Https { host, port, pin } => Ok(Way {
            client: super::pinned::streaming_client(*pin, CONNECT, whole)?,
            base: format!("https://{host}:{port}"),
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
            Ok(Way {
                client: plain(whole)?,
                base: format!("http://127.0.0.1:{local}"),
            })
        }
    }
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
