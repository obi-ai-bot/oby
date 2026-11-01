//! The routes of Obi-to-Obi work: `/v1/obid/sphere…`, `/v1/obid/decisions…`, `/v1/obid/sharing…`
//! and the inbox (`GET /v1/inbox`, `POST /v1/inbox/{id}/answer`).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::api::{ApiError, Shared};

fn json(status: StatusCode, value: Value) -> Response {
    (status, axum::Json(value)).into_response()
}

#[derive(serde::Deserialize)]
pub struct ListQuery {
    limit: Option<String>,
    state: Option<String>,
}

/// A `limit` from 1 to `max`, `default` when it's left out.
fn limit(text: Option<&str>, default: usize, max: usize) -> Result<usize, ApiError> {
    match text {
        None => Ok(default),
        Some(text) => text
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=max).contains(n))
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("limit is a number from 1 to {max}."),
                )
            }),
    }
}

/// `GET /v1/obid/sphere`.
pub async fn membership(State(shared): State<Arc<Shared>>) -> Response {
    // Until obid joins, the owner it would join as may be the GitHub login (Link::adopt_github_owner).
    if !shared.link.has_membership()
        && let Some(user) = shared.github.owner_user().await
    {
        shared.link.adopt_github_owner(&user);
    }
    json(StatusCode::OK, shared.link.membership_json())
}

/// `DELETE /v1/obid/sphere`.
pub async fn leave(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    Ok(json(StatusCode::OK, shared.link.leave().await?))
}

/// `GET /v1/obid/sphere/pins`.
pub async fn pins(State(shared): State<Arc<Shared>>) -> Response {
    json(StatusCode::OK, shared.link.pins_json())
}

/// `POST /v1/obid/sphere/pins/{owner}`.
pub async fn accept_pin(
    State(shared): State<Arc<Shared>>,
    Path(owner): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    Ok(json(
        StatusCode::OK,
        shared.link.accept_pin(&owner, &body).await?,
    ))
}

/// `POST /v1/obid/sphere/key`.
pub async fn key(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    Ok(json(StatusCode::OK, shared.link.make_key().await?))
}

/// `POST /v1/obid/sphere/card`.
pub async fn card(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    Ok(json(StatusCode::OK, shared.link.card(&body).await?))
}

/// `POST /v1/obid/sphere/join`.
pub async fn join(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    Ok(json(StatusCode::OK, shared.link.join(&body).await?))
}

/// `POST /v1/obid/sphere/requests`.
pub async fn request(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    Ok(json(
        StatusCode::ACCEPTED,
        shared.link.request(&body).await?,
    ))
}

/// `POST /v1/obid/sphere/council`. Runs apart from the caller's connection, so a client that hangs up
/// mid-call doesn't leave the remaining seats without their requests.
pub async fn council(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    let task =
        tokio::spawn(async move { crate::sphere::council::convene(&shared.link, &body).await });
    let answer = task.await.map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("The convener stopped: {e}"),
        )
    })??;
    Ok(json(StatusCode::ACCEPTED, answer))
}

/// `GET /v1/obid/sphere/exchanges?limit=50`.
pub async fn exchanges(
    State(shared): State<Arc<Shared>>,
    Query(query): Query<ListQuery>,
) -> Result<Response, ApiError> {
    let limit = limit(query.limit.as_deref(), 50, 500)?;
    Ok(json(StatusCode::OK, shared.link.exchanges_json(limit)))
}

/// `GET /v1/obid/sphere/exchanges/{correlationId}`.
pub async fn exchange(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    shared
        .link
        .exchange_json(&id)
        .map(|e| json(StatusCode::OK, e))
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "There's no such exchange."))
}

/// `GET /v1/obid/decisions`.
pub async fn decisions(State(shared): State<Arc<Shared>>) -> Response {
    json(StatusCode::OK, shared.link.decisions_json())
}

/// `POST /v1/obid/decisions`: a standing permission to approve on GitHub, waiting for the owner's
/// Confirm.
pub async fn propose_decision(
    State(shared): State<Arc<Shared>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    Ok(json(
        StatusCode::ACCEPTED,
        shared.link.propose_approval(&body)?,
    ))
}

/// `DELETE /v1/obid/decisions/{id}`.
pub async fn forget(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    if shared.link.forget(&id) {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "There's no such decision.",
        ))
    }
}

/// `GET /v1/inbox` and `GET /v1/inbox?state=done&limit=50`.
pub async fn inbox(
    State(shared): State<Arc<Shared>>,
    Query(query): Query<ListQuery>,
) -> Result<Response, ApiError> {
    let done = match query.state.as_deref() {
        None | Some("open") => false,
        Some("done") => true,
        Some(_) => {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "state is open or done.",
            ));
        }
    };
    // As the Swift runner: 50 unless asked, at most 1,000, and 0 lists nothing.
    let limit = match query.limit.as_deref() {
        None => 50,
        Some(text) => text.parse::<usize>().map_err(|_| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "limit is a number from 0 to 1,000.",
            )
        })?,
    }
    .min(1000);
    Ok(json(StatusCode::OK, shared.link.inbox_json(done, limit)))
}

/// `POST /v1/inbox/{id}/answer`.
pub async fn answer(
    State(shared): State<Arc<Shared>>,
    device: Option<axum::Extension<crate::gateway::PairedDevice>>,
    Path(id): Path<String>,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    // Whether an app of the owner's signed it: their answer to a question is then kept as their
    // own words for Oby's review (#1659).
    let typed = crate::routes::typed_by_owner(&shared, "POST", &uri, &headers, &body);
    // Words for a job's agent (its question's answer, a no's words) that no app of the owner's
    // signed: the job is no longer the owner's own words alone (crate::secretspec), before it
    // hears them.
    if let Some(item) = shared.link.item(&id).filter(|i| {
        i.done.is_none()
            && !i.job.is_empty()
            && matches!(
                i.kind.as_str(),
                "permission" | "plan" | super::inbox::QUESTION_KIND
            )
    }) && let Some(answer) = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|b| b["answer"].as_str().map(str::to_owned))
        && (item.kind == super::inbox::QUESTION_KIND
            || crate::permissions::inbox_decision(&answer).1.is_some())
        && !typed
    {
        shared.jobs.unvet(
            &item.job,
            "an inbox answer no app of the owner's signed reached it",
        );
    }
    // A phone (through the gateway) can't Approve a channel listener's review, which it doesn't
    // show yet: by the item itself, found by its decoded id, so no spelling of the path gets by.
    if device.is_some()
        && let Some(why) = crate::review::phone_refuses(shared.link.item(&id).as_ref(), &body)
    {
        return Err(ApiError::new(StatusCode::FORBIDDEN, why));
    }
    // A tap is the owner back (#594): after a while away, it gets a recap.
    crate::brain::recap::tapped(&shared);
    // Only the owner's tap confirms (#776): a Confirm on a `confirm` item carries a registered
    // device's signature over `oby-confirm\n<id>\n<title>\n<signedAt>`. Cancel, done and later
    // need none. A refusal names the title to sign, as connections do.
    let mut map: serde_json::Map<String, Value> = match serde_json::from_slice(&body) {
        Ok(Value::Object(map)) => map,
        _ => {
            return Ok(json(
                StatusCode::OK,
                shared.link.answer_typed(&id, &body, typed).await?,
            ));
        }
    };
    let approval = map.remove("approval");
    let rest = serde_json::to_vec(&Value::Object(map.clone())).unwrap_or_default();
    let confirming = map.get("answer").and_then(Value::as_str) == Some(super::inbox::CONFIRM);
    let confirm_item = shared.link.item(&id).filter(|i| i.kind == "confirm");
    if approval.is_some() && !(confirming && confirm_item.is_some()) {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "An approval goes only with Confirm on a confirm item.",
        ));
    }
    // Not a Confirm of an open confirm item: answered as before (one already done says so).
    let Some(item) = confirm_item.filter(|i| confirming && i.done.is_none()) else {
        return Ok(json(
            StatusCode::OK,
            shared.link.answer_typed(&id, &rest, typed).await?,
        ));
    };
    let refuse = |why: &str| {
        json(
            StatusCode::FORBIDDEN,
            serde_json::json!({"error": why, "action": item.title}),
        )
    };
    let Some(approval) = approval else {
        return Ok(refuse(super::link::NEEDS_YOUR_TAP));
    };
    let Ok(approval) = serde_json::from_value::<crate::approval::Approval>(approval) else {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "An approval is {\"device\", \"action\", \"signedAt\", \"signature\"}.",
        ));
    };
    let key = shared.devices.public_key(&approval.device);
    let signed_at = obi_protocol::time::parse(&approval.signed_at).unwrap_or_default();
    if let Err(why) = shared.jobs.approvals().check_confirm(
        &approval,
        &id,
        &item.title,
        key.as_deref(),
        obi_protocol::time::now(),
    ) {
        return Ok(refuse(&why));
    }
    Ok(json(
        StatusCode::OK,
        shared
            .link
            .answer_confirmed(&id, &rest, &item.title, signed_at)
            .await?,
    ))
}

/// `GET /v1/obid/sharing`.
pub async fn sharing(State(shared): State<Arc<Shared>>) -> Response {
    json(StatusCode::OK, shared.link.sharing_json())
}

/// `GET /v1/obid/sphere/members`.
pub async fn members(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    Ok(json(StatusCode::OK, shared.link.members_json().await?))
}

/// `GET /v1/obid/sphere/audit`.
pub async fn audit(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    Ok(json(StatusCode::OK, shared.link.audit_json().await?))
}

/// `PUT /v1/obid/sphere/membership`.
pub async fn renew(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    Ok(json(StatusCode::OK, shared.link.renew(&body).await?))
}

/// `POST /v1/obid/sharing/rules`.
pub async fn add_rule(
    State(shared): State<Arc<Shared>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    Ok(json(StatusCode::CREATED, shared.link.add_rule(&body)?))
}

/// `DELETE /v1/obid/sharing/rules/{id}`.
pub async fn forget_rule(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    if shared.link.forget_rule(&id) {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "There's no such sharing rule.",
        ))
    }
}

/// `PUT /v1/obid/sharing/limits`.
pub async fn limits(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    Ok(json(StatusCode::OK, shared.link.set_limits(&body)?))
}
