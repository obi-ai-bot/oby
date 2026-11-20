//! Adding a computer with a code (docs/contracts/remote.md, COMPUTERS-13), the Mac's side: the
//! window reads the other computer's link, reaches one of its direct routes with that route's pin
//! before it sends anything, redeems the code there for a device token, and registers its own
//! device key through it, so the owner's words it signs count there. A code typed to someone
//! else's computer gives that computer nothing: the handshake fails on the pin first.

use std::time::Duration;

use serde_json::{Value, json};

use super::link::Link;
use super::pinned;
use super::route::{Route, Via};
use super::supervisor::{Tried, unreachable};

/// A computer paired with a code.
#[derive(Debug)]
pub struct Paired {
    pub name: String,
    /// Its routes, from the link, in the owner's order.
    pub routes: Vec<Route>,
    /// The route the code went over.
    pub route: String,
    /// The device token there, and its pairing's id.
    pub token: String,
    pub pairing: String,
    /// The id this Mac's key has there, when that computer let it add one.
    pub device: Option<String>,
}

/// Why nothing was paired: `status`, when the other computer answered.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    pub status: Option<u16>,
    pub said: String,
}

fn base(route: &Route) -> Option<(String, super::pin::Pin)> {
    match &route.via {
        Via::Https { host, port, pin } => Some((format!("https://{host}:{port}"), *pin)),
        Via::Ssh { .. } => None,
    }
}

/// Pairs this Mac (`name`, its device key `public_key`) with the computer `link` names, by `code`.
pub async fn redeem(
    link: &str,
    code: &str,
    name: &str,
    public_key: &str,
) -> Result<Paired, Refused> {
    let link = Link::decode(link).map_err(|said| Refused { status: None, said })?;
    let mut tried = Vec::new();
    let mut reached = None;
    for route in &link.routes {
        let Some((url, pin)) = base(route) else {
            continue;
        };
        let client = pinned::client(pin, Duration::from_secs(20))
            .map_err(|said| Refused { status: None, said })?;
        match client.get(format!("{url}/v1/status")).send().await {
            Ok(_) => {
                reached = Some((route.clone(), url, client));
                break;
            }
            Err(e) => tried.push(Tried {
                route: route.id.clone(),
                what: route.describe(),
                why: pinned::said(&e),
            }),
        }
    }
    let Some((route, url, client)) = reached else {
        return Err(Refused {
            status: None,
            said: unreachable(&link.name, &tried),
        });
    };
    let call = async |path: &str, token: Option<&str>, body: Value| -> Result<Value, Refused> {
        let mut request = client.post(format!("{url}{path}")).json(&body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let answer = request.send().await.map_err(|e| Refused {
            status: None,
            said: pinned::said(&e),
        })?;
        let status = answer.status();
        let body: Value = answer.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            Ok(body)
        } else {
            Err(Refused {
                status: Some(status.as_u16()),
                said: body["error"]
                    .as_str()
                    .unwrap_or("no reason given")
                    .to_string(),
            })
        }
    };
    let redeemed = call(
        "/v1/pairing",
        None,
        json!({"credential": code, "name": name}),
    )
    .await?;
    let token = redeemed["token"].as_str().unwrap_or_default().to_string();
    let pairing = redeemed["deviceID"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if token.is_empty() {
        return Err(Refused {
            status: None,
            said: "The other computer paired but sent no token.".into(),
        });
    }
    let device = call(
        "/v1/devices",
        Some(&token),
        json!({"name": name, "publicKey": public_key}),
    )
    .await
    .ok()
    .and_then(|d| d["id"].as_str().map(str::to_string));
    Ok(Paired {
        name: link.name,
        routes: link.routes,
        route: route.id,
        token,
        pairing,
        device,
    })
}
