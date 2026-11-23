//! The window's requests to another computer (docs/contracts/remote.md, COMPUTERS-21 and 22): a
//! request on this Mac's loopback API, with obid's own token, that names a computer this Mac keeps
//! in `Oby-Computer` goes to that computer's gateway instead, over the best of its routes that
//! answers (crate::remote::reach: a direct one pinned, an SSH one through its tunnel), with that
//! computer's device token. Only what the
//! window sends is carried: its body, its content type, its signed words, its request key and the
//! event stream's resume point. Nothing else of this Mac's goes with it, its profile neither: a
//! profile is this Mac's, so the other computer answers for its own default.
//!
//! It is per request, as `Oby-Profile` is: a request that names no computer is this Mac's, so the
//! Mac's own agents, oby-mcp and scripts keep working on this Mac whichever computer the window
//! shows. `/v1/computers` and under it are always this Mac's.
//!
//! It owns nothing: it reads the computers and their routes (crate::remote::computers) and tells
//! their supervisors what each try came to.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::computers::HEADER;
use crate::api::Shared;

/// The longest body the window sends another computer: the gateway's own limit.
const BODY: usize = 16 * 1024 * 1024;

/// The headers the window's request carries to the other computer, besides its token.
const CARRIED: &[&str] = &[
    "content-type",
    crate::approval::WORDS,
    "idempotency-key",
    "last-event-id",
    "accept",
];

/// The answer's headers brought back.
const BROUGHT: &[&str] = &["content-type", "cache-control", "content-disposition"];

fn refusal(status: StatusCode, message: &str, reason: &'static str) -> Response {
    (
        status,
        axum::Json(json!({"error": message, "reason": reason})),
    )
        .into_response()
}

/// What a request that names a computer is for: `None` when it's this Mac's (no header, `this`,
/// `/v1/computers` and under it); otherwise the computer and the route as it'll be sent, or why
/// it isn't sent at all, decided on this Mac before anything leaves it.
fn named(shared: &Shared, request: &Request) -> Option<Result<(String, String), Response>> {
    let value = request.headers().get(HEADER)?;
    let id = value.to_str().ok().map(str::trim).unwrap_or_default();
    let path = request.uri().path();
    if id == "this" || path == "/v1/computers" || path.starts_with("/v1/computers/") {
        return None;
    }
    // Only the window itself: obid's own token, on loopback, not through the gateway or the
    // relay. An agent's broker token, a paired device's or none naming a computer is refused,
    // never sent (COMPUTERS-22).
    let ext = request.extensions();
    if ext.get::<crate::gateway::PairedDevice>().is_some()
        || ext.get::<crate::gateway::Relayed>().is_some()
        || crate::api::admit(request.uri(), request.headers(), &shared.token).is_err()
        || id.is_empty()
    {
        return Some(Err(refusal(
            StatusCode::BAD_REQUEST,
            "Only Oby's window on this Mac works on another computer.",
            "not_the_window",
        )));
    }
    // Only what a computer's token reaches there: a route it can't is refused here, before it
    // leaves this Mac (COMPUTERS-14), the path read one way as the gateway reads it.
    let Some(plain) = crate::gateway::plain_path(path)
        .filter(|plain| crate::gateway::allows_computer(request.method(), plain))
    else {
        return Some(Err(refusal(
            StatusCode::NOT_FOUND,
            "That isn't available on another computer: it's this Mac's only.",
            "not_for_a_computer",
        )));
    };
    if shared.computers.get(id).is_none() {
        return Some(Err(refusal(
            StatusCode::NOT_FOUND,
            "That computer isn't one this Mac works on any more.",
            "no_such_computer",
        )));
    }
    let target = match request.uri().query() {
        Some(query) => format!("{}?{query}", crate::gateway::canonical(&plain)),
        None => crate::gateway::canonical(&plain),
    };
    Some(Ok((id.to_owned(), target)))
}

/// The API's outermost layer: a request for another computer goes there, or is refused here; any
/// other runs here, through obid's own gate.
pub async fn layer(State(shared): State<Arc<Shared>>, request: Request, next: Next) -> Response {
    match named(&shared, &request) {
        None => next.run(request).await,
        Some(Err(refused)) => refused,
        Some(Ok((id, target))) => forward(&shared, &id, &target, request).await,
    }
}

async fn forward(shared: &Arc<Shared>, id: &str, target: &str, request: Request) -> Response {
    let Some(computer) = shared.computers.get(id) else {
        return refusal(
            StatusCode::NOT_FOUND,
            "That computer isn't one this Mac works on any more.",
            "no_such_computer",
        );
    };
    let token = match shared.computers.token(id) {
        Ok(Some(token)) => token,
        _ => {
            return refusal(
                StatusCode::CONFLICT,
                &format!(
                    "Oby has no way into {} any more: add it again.",
                    computer.name
                ),
                "no_token",
            );
        }
    };
    let (parts, body) = request.into_parts();
    let Ok(body) = axum::body::to_bytes(body, BODY).await else {
        return refusal(
            StatusCode::PAYLOAD_TOO_LARGE,
            "That request is too large.",
            "too_large",
        );
    };
    // The window's streams (COMPUTERS-29): no whole-answer limit, and they end with the computer.
    let stream = parts.method == Method::GET
        && matches!(
            parts.uri.path(),
            "/v1/events" | "/v1/browser/events" | "/v1/browser/relay"
        );
    // The route in use and those above it first, then the rest, so a cut route's fallback answers
    // this same request.
    let mut routes = shared.computers.to_probe(id);
    for route in &computer.routes {
        if !routes.iter().any(|r| r.id == route.id) {
            routes.push(route.clone());
        }
    }
    for route in routes {
        let way = match super::reach::way(shared, id, &route, !stream).await {
            Ok(way) => way,
            Err(why) => {
                observe(shared, id, &route.id, Err(why));
                continue;
            }
        };
        let mut upstream = way
            .client
            .request(parts.method.clone(), format!("{}{target}", way.base))
            .bearer_auth(&token)
            .body(body.clone());
        for name in CARRIED {
            if let Some(value) = parts.headers.get(*name) {
                upstream = match there(&computer, name, value) {
                    Some(words) => upstream.header(*name, words),
                    None => upstream.header(*name, value),
                };
            }
        }
        match upstream.send().await {
            Ok(answer) => {
                observe(shared, id, &route.id, Ok(()));
                return brought_back(answer, stream.then(|| (shared.clone(), id.to_owned())));
            }
            Err(e) => observe(shared, id, &route.id, Err(super::pinned::said(&e))),
        }
    }
    let (_, tried) = shared.computers.route(id);
    refusal(
        StatusCode::BAD_GATEWAY,
        &super::supervisor::unreachable(&computer.name, &tried),
        "unreachable",
    )
}

/// The owner's words as the other computer reads them: signed by the key this Mac registered there,
/// they name its id there instead of this Mac's (the signature covers the request and its time,
/// not the id). Words any other device signed go as they are, and count there only if it knows
/// that key (COMPUTERS-15). None for any other header.
fn there(
    computer: &super::computers::Computer,
    name: &str,
    value: &axum::http::HeaderValue,
) -> Option<String> {
    if name != crate::approval::WORDS {
        return None;
    }
    let (device, rest) = value.to_str().ok()?.split_once(' ')?;
    if computer.signer.as_deref() != Some(device) {
        return None;
    }
    Some(format!("{} {rest}", computer.device.as_deref()?))
}

/// What one try came to, told to the computer's supervisor; the window hears of a change.
fn observe(shared: &Shared, id: &str, route: &str, result: Result<(), String>) {
    if let Some(change) = shared.computers.observe(id, route, result) {
        let mut data = serde_json::to_value(&change).unwrap_or_default();
        data["id"] = json!(id);
        shared.events.send("computer.route", data);
        shared.events.send("computers", shared.computers.list());
    }
}

/// The other computer's answer, streamed back as it comes, with its status and only the headers a
/// window reads. A stream (`following`) ends once the window is no longer on that computer, or the
/// Mac forgot it: it never outlives its computer, and carries nothing but that computer's events.
fn brought_back(answer: reqwest::Response, following: Option<(Arc<Shared>, String)>) -> Response {
    let status = StatusCode::from_u16(answer.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = HeaderMap::new();
    for name in BROUGHT {
        if let Some(value) = answer.headers().get(*name)
            && let Ok(value) = header::HeaderValue::from_bytes(value.as_bytes())
            && let Ok(name) = header::HeaderName::from_bytes(name.as_bytes())
        {
            headers.insert(name, value);
        }
    }
    let changes = following
        .as_ref()
        .map(|(shared, _)| shared.computers.changes());
    let chunks =
        futures_util::stream::unfold(Some((answer, changes, following)), |state| async move {
            let (mut answer, mut changes, following) = state?;
            loop {
                let next = match changes.as_mut() {
                    None => answer.chunk().await,
                    Some(changes) => tokio::select! {
                        chunk = answer.chunk() => chunk,
                        changed = changes.changed() => {
                            let (shared, id) = following.as_ref().expect("a stream follows");
                            let still = changed.is_ok()
                                && shared.computers.current().as_deref() == Some(id.as_str());
                            if still {
                                continue;
                            }
                            return None;
                        }
                    },
                };
                return match next {
                    Ok(Some(bytes)) => Some((
                        Ok::<_, std::io::Error>(bytes),
                        Some((answer, changes, following)),
                    )),
                    Ok(None) => None,
                    Err(e) => Some((Err(std::io::Error::other(e)), None)),
                };
            }
        });
    let body = Body::from_stream(chunks);
    (status, headers, body).into_response()
}
