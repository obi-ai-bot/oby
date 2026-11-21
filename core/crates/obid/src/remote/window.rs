//! The window's routes for its computers (docs/contracts/remote.md, COMPUTERS-19 to 24;
//! docs/api/remote.md, "This Mac's computers"), on this Mac's loopback API only: the list and which
//! one the window is on, reading a link, adding a computer with a code, renaming, trying again and
//! removing. Adding and removing are the owner's acts: their words, signed by a registered device's
//! key, go with them (crate::routes::signed_by), and adding registers that device's key there, so
//! its words count through that computer's token (COMPUTERS-15).
//!
//! It owns no state: the computers are crate::remote::computers', the calls to them its own here.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::link::Link;
use super::route::Via;
use crate::api::{ApiError, Shared};

fn refused(status: StatusCode, message: impl Into<String>, reason: &'static str) -> ApiError {
    let mut error = ApiError::new(status, message);
    error.reason = Some(reason);
    error
}

fn body_of(bytes: &Bytes, shape: &str) -> Result<Value, ApiError> {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| {
            refused(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("The body is {shape}."),
                "bad_body",
            )
        })
}

fn changed(shared: &Shared) {
    shared.events.send("computers", shared.computers.list());
}

/// `GET /v1/computers`.
pub async fn list(State(shared): State<Arc<Shared>>) -> Response {
    axum::Json(shared.computers.list()).into_response()
}

/// `POST /v1/computers/link` `{"link"}`: the name, fingerprint and routes a link holds, read by
/// obid so the window never parses one.
pub async fn read_link(
    State(shared): State<Arc<Shared>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let _ = &shared;
    let body = body_of(&body, "{\"link\": \"oby-computer:…\"}")?;
    let link = Link::decode(body["link"].as_str().unwrap_or_default().trim())
        .map_err(|why| refused(StatusCode::UNPROCESSABLE_ENTITY, why, "not_a_link"))?;
    let pin = link.routes.iter().find_map(|r| match &r.via {
        Via::Https { pin, .. } => Some(*pin),
        Via::Ssh { .. } => None,
    });
    let routes: Vec<Value> = link
        .routes
        .iter()
        .map(|r| json!({"kind": r.kind, "address": r.describe().split_once(' ').map_or("", |(_, a)| a)}))
        .collect();
    Ok(axum::Json(json!({
        "name": link.name,
        "fingerprint": pin.map(|p| super::invite::fingerprint(&p)),
        "routes": routes,
    }))
    .into_response())
}

/// `POST /v1/computers` `{"link", "code"}`: adds the computer the link names with its code, as the
/// owner's act. The device that signed it registers its key there.
pub async fn add(
    State(shared): State<Arc<Shared>>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let Some(device) = crate::routes::signed_by(&shared, "POST", &uri, &headers, &body) else {
        return Err(crate::routes::needs_the_owners_app());
    };
    let asked = body_of(
        &body,
        "{\"link\": \"oby-computer:…\", \"code\": \"123456\"} or {\"ssh\": \"<host>\"}",
    )?;
    if let Some(host) = asked["ssh"].as_str() {
        return over_ssh(shared, &device, host.trim());
    }
    let (Some(link), Some(code)) = (asked["link"].as_str(), asked["code"].as_str()) else {
        return Err(refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Adding a computer here takes its link and its code.",
            "bad_body",
        ));
    };
    let key = shared.devices.public_key(&device).ok_or_else(|| {
        refused(
            StatusCode::FORBIDDEN,
            "That device isn't registered with this Oby.",
            "unknown_device",
        )
    })?;
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let paired = super::code::redeem(link.trim(), &code, &shared.host, &key)
        .await
        .map_err(|r| match r.status {
            Some(401) => refused(StatusCode::UNPROCESSABLE_ENTITY, r.said, "wrong_code"),
            Some(410) => refused(StatusCode::UNPROCESSABLE_ENTITY, r.said, "code_ended"),
            Some(_) => refused(StatusCode::BAD_GATEWAY, r.said, "refused_there"),
            None if r.said.starts_with("Oby can") => {
                refused(StatusCode::BAD_GATEWAY, r.said, "unreachable")
            }
            None => refused(StatusCode::UNPROCESSABLE_ENTITY, r.said, "not_a_link"),
        })?;
    let os = os_of(&paired).await;
    let route = paired.route.clone();
    let computer = shared
        .computers
        .add(paired, os, Some(device))
        .map_err(|why| refused(StatusCode::INTERNAL_SERVER_ERROR, why, "not_kept"))?;
    // The route the code went over answered: the window starts on it.
    shared.computers.observe(&computer.id, &route, Ok(()));
    changed(&shared);
    Ok((
        StatusCode::CREATED,
        axum::Json(shared.computers.listed(&computer)),
    )
        .into_response())
}

/// `POST /v1/computers` `{"ssh": "<host>"}` (COMPUTERS-25): adding over the person's own SSH, its
/// steps told as `computer.adding` events. A host that could be read as an option, or isn't one,
/// is refused before ssh runs.
fn over_ssh(shared: Arc<Shared>, device: &str, host: &str) -> Result<Response, ApiError> {
    super::ssh::check_destination(host)
        .map_err(|why| refused(StatusCode::UNPROCESSABLE_ENTITY, why, "not_a_host"))?;
    let key = shared.devices.public_key(device).ok_or_else(|| {
        refused(
            StatusCode::FORBIDDEN,
            "That device isn't registered with this Oby.",
            "unknown_device",
        )
    })?;
    let asked = super::adding::Asked {
        destination: host.to_owned(),
        signer: device.to_owned(),
        key,
        name: shared.host.clone(),
    };
    let id = super::adding::start(shared, asked)
        .map_err(|why| refused(StatusCode::INTERNAL_SERVER_ERROR, why, "not_started"))?;
    Ok((StatusCode::ACCEPTED, axum::Json(json!({"adding": id}))).into_response())
}

/// `mac` or `linux`, as the other obid's status says, through the route the code went over.
async fn os_of(paired: &super::code::Paired) -> Option<String> {
    let route = paired.routes.iter().find(|r| r.id == paired.route)?;
    let Via::Https { host, port, pin } = &route.via else {
        return None;
    };
    let way = super::reach::Way {
        client: super::pinned::client(*pin, Duration::from_secs(10)).ok()?,
        base: format!("https://{host}:{port}"),
    };
    super::reach::os(&super::reach::status(&way, &paired.token).await.ok()?)
}

/// `PUT /v1/computers/current` `{"id": <id> | "this"}`: which computer the window is on, kept so
/// the window names it on its requests (crate::remote::forward).
pub async fn use_computer(
    State(shared): State<Arc<Shared>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let asked = body_of(&body, "{\"id\": \"this\" | \"<computer id>\"}")?;
    let id = asked["id"].as_str().unwrap_or_default();
    let id = (id != "this").then_some(id);
    match shared.computers.set_current(id) {
        Ok(true) => {
            changed(&shared);
            Ok(axum::Json(shared.computers.list()).into_response())
        }
        Ok(false) => Err(refused(
            StatusCode::NOT_FOUND,
            "That computer isn't one this Mac works on.",
            "no_such_computer",
        )),
        Err(why) => Err(refused(StatusCode::INTERNAL_SERVER_ERROR, why, "not_kept")),
    }
}

/// `PATCH /v1/computers/{id}` `{"name"}`: renamed on this Mac only.
pub async fn rename(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let asked = body_of(&body, "{\"name\": \"…\"}")?;
    let name = super::computers::clean_name(asked["name"].as_str().unwrap_or_default());
    if name.is_empty() {
        return Err(refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "A computer's name can't be empty.",
            "bad_body",
        ));
    }
    match shared.computers.rename(&id, &name) {
        Ok(true) => {
            changed(&shared);
            let computer = shared.computers.get(&id).expect("just renamed");
            Ok(axum::Json(shared.computers.listed(&computer)).into_response())
        }
        Ok(false) => Err(refused(
            StatusCode::NOT_FOUND,
            "That computer isn't one this Mac works on.",
            "no_such_computer",
        )),
        Err(why) => Err(refused(StatusCode::INTERNAL_SERVER_ERROR, why, "not_kept")),
    }
}

/// `POST /v1/computers/{id}/retry`: every route probed now, best first, until obid answers on one
/// (COMPUTERS-18: something on the port isn't enough).
pub async fn retry(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let Some(computer) = shared.computers.get(&id) else {
        return Err(refused(
            StatusCode::NOT_FOUND,
            "That computer isn't one this Mac works on.",
            "no_such_computer",
        ));
    };
    let token = shared
        .computers
        .token(&id)
        .ok()
        .flatten()
        .unwrap_or_default();
    for route in &computer.routes {
        let answered = match super::reach::way(&shared, &id, route, true).await {
            Ok(way) => super::reach::status(&way, &token).await.map(|_| ()),
            Err(why) => Err(why),
        };
        let ok = answered.is_ok();
        if let Some(change) = shared.computers.observe(&id, &route.id, answered) {
            let mut data = serde_json::to_value(&change).unwrap_or_default();
            data["id"] = json!(id);
            shared.events.send("computer.route", data);
        }
        if ok {
            break;
        }
    }
    changed(&shared);
    Ok(axum::Json(shared.computers.listed(&computer)).into_response())
}

/// `DELETE /v1/computers/{id}`, the owner's act: this Mac's pairing there ends with its own token
/// (`DELETE /v1/pairing`, COMPUTERS-16), which forgets this Mac's key there; then this Mac forgets
/// the computer and its token. One that doesn't answer is forgotten here all the same, and the
/// answer says so (`"there": false`).
pub async fn remove(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if crate::routes::signed_by(&shared, "DELETE", &uri, &headers, b"").is_none() {
        return Err(crate::routes::needs_the_owners_app());
    }
    let Some(computer) = shared.computers.get(&id) else {
        return Err(refused(
            StatusCode::NOT_FOUND,
            "That computer isn't one this Mac works on.",
            "no_such_computer",
        ));
    };
    let token = shared
        .computers
        .token(&id)
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut there = false;
    for route in shared.computers.to_probe(&id) {
        let Ok(way) = super::reach::way(&shared, &id, &route, true).await else {
            continue;
        };
        if let Ok(answer) = way
            .client
            .delete(format!("{}/v1/pairing", way.base))
            .bearer_auth(&token)
            .send()
            .await
        {
            // Ended now, or already: its token opens nothing there either way.
            there = matches!(answer.status().as_u16(), 204 | 401);
            break;
        }
    }
    shared
        .computers
        .forget(&computer.id)
        .map_err(|why| refused(StatusCode::INTERNAL_SERVER_ERROR, why, "not_kept"))?;
    changed(&shared);
    Ok(axum::Json(json!({"removed": computer.id, "there": there})).into_response())
}
