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

/// `GET /v1/obid/sphere/peers/{owner}?limit=50`: that member's thread
/// (docs/contracts/peer-thread.md).
pub async fn peer_thread(
    State(shared): State<Arc<Shared>>,
    Path(owner): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Response, ApiError> {
    let limit = limit(query.limit.as_deref(), 50, 500)?;
    Ok(json(
        StatusCode::OK,
        shared.link.peer_thread_json(&owner, limit)?,
    ))
}

/// `POST /v1/obid/sphere/peers/{owner}/ask`: the owner's own words to that member's Oby, signed by an
/// app of theirs (docs/contracts/peer-thread.md).
pub async fn peer_ask(
    State(shared): State<Arc<Shared>>,
    Path(owner): Path<String>,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if crate::routes::signed_by(&shared, "POST", &uri, &headers, &body).is_none() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Only your own words go from their thread as yours: an app of yours signs them.",
        ));
    }
    Ok(json(
        StatusCode::ACCEPTED,
        Box::pin(shared.link.ask_peer(&owner, &body)).await?,
    ))
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
    let signer = crate::routes::signed_by(&shared, "POST", &uri, &headers, &body);
    let typed = signer.is_some();
    // Another Oby's question is answered or declined only by the owner's hand (PEER-33): a signature
    // of one of their devices, or a paired phone through the gateway. Putting it off says nothing, so
    // it needs neither; a body that's neither is refused below, as any.
    if !typed
        && device.is_none()
        && shared
            .link
            .item(&id)
            .is_some_and(|i| i.done.is_none() && i.is_question())
        && serde_json::from_slice::<Value>(&body)
            .is_ok_and(|b| b.get("answer").is_some() || b.get("done").is_some())
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            super::link::NEEDS_YOUR_HAND,
        ));
    }
    // A job's permission, or its plan, is allowed only by the owner's signed words (PERMISSIONS-15):
    // neither the token alone nor a phone paired without a key. An approved plan leaves Plan for
    // the owner's default mode, a widening (SIGN-2). A no and a question are as before.
    if !typed
        && shared.link.item(&id).is_some_and(|i| {
            i.done.is_none()
                && !i.job.is_empty()
                && matches!(i.kind.as_str(), "permission" | "plan")
        })
        && serde_json::from_slice::<Value>(&body).is_ok_and(|b| {
            b["answer"].as_str().is_some_and(|answer| {
                crate::permissions::inbox_decision(answer).0 != crate::permissions::Decision::Deny
            })
        })
    {
        return Err(crate::routes::needs_the_owners_app());
    }
    // A request key (docs/contracts/receipts.md, RECEIPTS-10 and 15): the same answer again, as
    // signed as the first, gets the first one's answer and does nothing. A Confirm's approval is its
    // signature (#776): it's checked, and used up, here, as signed words are, and whether it held
    // is compared, never the approval itself; so a retry with a used one is `409` (q067).
    let sent = serde_json::from_slice::<Value>(&body).unwrap_or_default();
    let key = crate::routes::request_key(&sent, &headers)
        .map_err(|why| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why))?;
    let confirming = sent["answer"].as_str() == Some(super::inbox::CONFIRM);
    // On an item done since too: a retry re-signed after the first Confirm settled it.
    let item = shared.link.item(&id).filter(|i| i.kind == "confirm");
    let approval = sent.get("approval").cloned();
    let approval =
        approval.and_then(|a| serde_json::from_value::<crate::approval::Approval>(a).ok());
    let checked = item
        .filter(|_| confirming)
        .zip(approval)
        .map(|(item, approval)| {
            let key = shared.devices.public_key(&approval.device);
            let now = obi_protocol::time::now();
            let confirms = shared.jobs.approvals();
            confirms.check_confirm(&approval, &id, &item.title, key.as_deref(), now)
        });
    let mut judged = sent;
    if let Some(map) = judged.as_object_mut() {
        map.remove("clientRequestId");
        map.remove("approval");
    }
    let approved = checked.as_ref().map(Result::is_ok);
    let judged = serde_json::json!({"body": judged, "signed": typed, "approved": approved});
    let route = format!("POST /v1/inbox/{id}/answer");
    let work = async {
        // Who answered stays on the item, and in the log: a merge can be traced afterwards (#1825).
        let by = signer.or_else(|| device.as_ref().map(|d| d.0.0.clone()));
        tracing::info!(
            "inbox {id}: answered by {}",
            by.as_deref().unwrap_or("nothing signed")
        );
        shared.link.note_answered_by(&id, by);
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
                let answered = shared.link.answer_typed(&id, &body, typed).await?;
                return Ok((StatusCode::OK, answered, None));
            }
        };
        let approval = map.remove("approval");
        map.remove("clientRequestId");
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
            let answered = shared.link.answer_typed(&id, &rest, typed).await?;
            return Ok((StatusCode::OK, answered, None));
        };
        // Refused: a 403, so no receipt is kept.
        let refuse = |why: &str| {
            let refused = serde_json::json!({"error": why, "action": item.title});
            Ok((StatusCode::FORBIDDEN, refused, None))
        };
        let Some(approval) = approval else {
            return refuse(super::link::NEEDS_YOUR_TAP);
        };
        let Ok(approval) = serde_json::from_value::<crate::approval::Approval>(approval) else {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "An approval is {\"device\", \"action\", \"signedAt\", \"signature\"}.",
            ));
        };
        let signed_at = obi_protocol::time::parse(&approval.signed_at).unwrap_or_default();
        // Checked before the lookup; none when the item wasn't a confirm item then.
        let checked = checked.unwrap_or_else(|| Err(super::link::NEEDS_YOUR_TAP.to_owned()));
        if let Err(why) = checked {
            return refuse(&why);
        }
        let answered = shared
            .link
            .answer_confirmed(&id, &rest, &item.title, signed_at)
            .await;
        if let Err(why) = &answered {
            crate::brain::ask::refused_after_the_click(&shared, &item, why);
        } else {
            crate::conversation::confirmed(&shared, &item).await;
        }
        Ok((StatusCode::OK, answered?, None))
    };
    let stored = crate::routes::stored;
    crate::routes::keyed(&shared, key.as_deref(), &route, &judged, stored, work).await
}

#[derive(serde::Deserialize)]
pub struct RoomQuery {
    after: Option<String>,
}

/// `GET /v1/spheres/{sphere}/room?after=`: the room (docs/contracts/rooms.md).
pub async fn room(
    State(shared): State<Arc<Shared>>,
    Path(sphere): Path<String>,
    Query(query): Query<RoomQuery>,
) -> Result<Response, ApiError> {
    let after = match query.after.as_deref() {
        None => 0,
        Some(text) => text.parse::<u64>().map_err(|_| {
            ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "after is a post's seq.")
        })?,
    };
    Ok(json(
        StatusCode::OK,
        Box::pin(shared.link.room_json(&sphere, after)).await?,
    ))
}

/// `POST /v1/spheres/{sphere}/room`: the owner's own words in the room, signed by an app of theirs
/// (docs/contracts/signed-words.md).
pub async fn room_say(
    State(shared): State<Arc<Shared>>,
    Path(sphere): Path<String>,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if crate::routes::signed_by(&shared, "POST", &uri, &headers, &body).is_none() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Only your own words go in the room as yours: an app of yours signs them.",
        ));
    }
    Ok(json(
        StatusCode::ACCEPTED,
        Box::pin(shared.link.room_say(&sphere, &body)).await?,
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
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    // What obid tells people by itself is only the owner's to widen (PEER-17): obid's token, which
    // the page reaches through main, isn't enough. Taking one out shares less, and needs no more.
    if crate::routes::signed_by(&shared, "POST", &uri, &headers, &body).is_none() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "A sharing rule is only yours to make: an app of yours signs it.",
        ));
    }
    Ok(json(StatusCode::CREATED, shared.link.add_rule(&body)?))
}

/// `POST /v1/obid/sharing/offer`.
pub async fn offer_to_share(
    State(shared): State<Arc<Shared>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let body = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    let chosen = crate::sphere::sharing::chosen_from(&body)
        .map_err(|why| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why))?;
    let room = body["room"] == true;
    Ok(json(
        StatusCode::CREATED,
        shared.link.offer_again(room, chosen)?,
    ))
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
pub async fn limits(
    State(shared): State<Arc<Shared>>,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    // How much of the owner's account answers can spend is theirs to raise, as a rule is (PEER-41).
    if crate::routes::signed_by(&shared, "PUT", &uri, &headers, &body).is_none() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "How much your Oby may answer is only yours to change: an app of yours signs it.",
        ));
    }
    Ok(json(StatusCode::OK, shared.link.set_limits(&body)?))
}
