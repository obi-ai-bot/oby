//! Adding a computer with a code (docs/contracts/remote.md, COMPUTERS-13), the Mac's side: the
//! window reads the other computer's link, reaches one of its direct routes with that route's pin
//! before it sends anything, redeems the code there for a device token, and registers its own
//! device key through it, so the owner's words it signs count there. A code typed to someone
//! else's computer gives that computer nothing: the handshake fails on the pin first. A one link
//! (COMPUTERS-29) is redeemed the same way, with the secret it holds instead of a code.

use std::time::Duration;

use serde_json::{Value, json};

use super::link::Read;
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

/// The first of `routes` that answers with its pin, best first, or why none did.
async fn reach(routes: &[Route], name: &str) -> Result<(Route, String, reqwest::Client), Refused> {
    let mut tried = Vec::new();
    for route in routes {
        let Some((url, pin)) = base(route) else {
            continue;
        };
        let client = pinned::client(pin, Duration::from_secs(20))
            .map_err(|said| Refused { status: None, said })?;
        match client.get(format!("{url}/v1/status")).send().await {
            Ok(_) => return Ok((route.clone(), url, client)),
            Err(e) => tried.push(Tried {
                route: route.id.clone(),
                what: route.describe(),
                why: pinned::said(&e),
            }),
        }
    }
    Err(Refused {
        status: None,
        said: unreachable(name, &tried),
    })
}

/// `body` posted to `path` there, with `token` when given: its answer, or its refusal in its own
/// words.
async fn post(
    client: &reqwest::Client,
    url: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> Result<Value, Refused> {
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
}

/// What a link stands for there: version 1's own name; a one link's name as the other computer
/// says it to the secret's holder, over the first of its routes that answers with its pin
/// (COMPUTERS-29). Nothing is used up.
pub async fn name_of(link: &Read) -> Result<String, Refused> {
    let Read::One(one) = link else {
        return Ok(link.name().unwrap_or_default().to_owned());
    };
    let (_, url, client) = reach(&one.routes, "the other computer").await?;
    let said = post(
        &client,
        &url,
        "/v1/pairing/peek",
        None,
        json!({"credential": one.credential()}),
    )
    .await
    .map_err(link_words)?;
    Ok(clean(said["name"].as_str().unwrap_or_default()))
}

/// A name as obid keeps one: no control characters, at most 100, never empty.
fn clean(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(100).collect();
    if name.trim().is_empty() {
        "Another computer".into()
    } else {
        name
    }
}

/// A one link's refusal there, in words about a link rather than a code.
fn link_words(refused: Refused) -> Refused {
    let said = match refused.status {
        Some(410) => {
            "That link has ended or was already used. Make a new one on the other computer."
        }
        Some(401) => {
            "That link isn't the other computer's invitation anymore. Make a new one there."
        }
        Some(404) => {
            "The other computer's Oby doesn't take a link alone yet. Update Oby there, or add it with a code."
        }
        _ => return refused,
    };
    Refused {
        status: refused.status,
        said: said.into(),
    }
}

/// Pairs this Mac (`name`, its device key `public_key`) with the computer `link` names: by `code`
/// for a link of version 1, by the secret it holds for a one link (`code` unused).
pub async fn redeem(
    link: &str,
    code: &str,
    name: &str,
    public_key: &str,
) -> Result<Paired, Refused> {
    let link = Read::decode(link).map_err(|said| Refused { status: None, said })?;
    let credential = match &link {
        Read::WithCode(_) if code.is_empty() => {
            return Err(Refused {
                status: None,
                said: "Adding a computer with this link takes its code.".into(),
            });
        }
        Read::WithCode(_) => code.to_owned(),
        Read::One(one) => one.credential(),
    };
    let (route, url, client) =
        reach(link.routes(), link.name().unwrap_or("the other computer")).await?;
    let call = async |path: &str, token: Option<&str>, body: Value| -> Result<Value, Refused> {
        post(&client, &url, path, token, body).await
    };
    let one = matches!(link, Read::One(_));
    let redeemed = call(
        "/v1/pairing",
        None,
        json!({"credential": credential, "name": name}),
    )
    .await
    .map_err(|r| if one { link_words(r) } else { r })?;
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
    // A one link holds no name: the other computer said its own on redeeming.
    let there = match &link {
        Read::WithCode(l) => l.name.clone(),
        Read::One(_) => clean(redeemed["link"]["name"].as_str().unwrap_or_default()),
    };
    Ok(Paired {
        name: there,
        routes: link.routes().to_vec(),
        route: route.id,
        token,
        pairing,
        device,
    })
}
