//! The Council's convener (obi-ai-bot/oby-sphere, `spec/council/review.md` section 5): one
//! `review_request` per drawn seat, each its own envelope and exchange, for the same commit, the
//! same conditions and the same words. Behind the `council` setting, with no UI.
//!
//! Who sits is the draw's (`spec/council/draw.md`), made by the clerk outside obid in this version:
//! obid sends what it is given. The review's words are scanned for secrets once and every seat is
//! checked before anything goes (a member, active, accepting reviews, not this Obi's owner's, one per
//! owner), so nothing obid can check leaves a seat out; each request is then sealed, signed and sent
//! as every request is (`Link::send_scanned`), all expiring at the same instant, the motion's
//! `deadlines.reviews`. A seat obid already sent this motion's request is answered with that exchange,
//! never sent a second one (the tally refuses two requests to one seat), and one call runs at a time. The Obysphere relays each exchange between its two Obis only, so no seat sees
//! another's request or result. obid never
//! passes one seat's result to another: the panel is blind by construction (rule I1). A panel never
//! approves on the code host in a person's place (rule I7), so a request asking it is refused.

use axum::http::StatusCode;
use serde_json::{Value, json};

use super::Link;
use super::link::{Expiry, object};
use crate::api::ApiError;

/// The most seats one call convenes: a panel of 15 is already larger than any profile asks for.
pub const MOST_SEATS: usize = 15;

/// One convening at a time, so two calls for one motion can't both send a seat its request.
static CONVENING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, message)
}

/// `POST /v1/obid/sphere/council`: `{"seats": [obi ids], "review": {a review_request body},
/// "expiresAt": the motion's `deadlines.reviews`}`. Checks everything first (the review's words for
/// secrets, once, and every seat); then sends the review to each seat in order, every request
/// expiring at the same instant, and answers each seat's outgoing exchange: the one already sent,
/// when the call is repeated. A seat whose send fails on the way (the Obysphere refusing it) says
/// why, and the others still go.
pub async fn convene(link: &Link, body: &[u8]) -> Result<Value, ApiError> {
    if !link.council_enabled() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Council panels are off: \"council\": true in obid's configuration turns them on.",
        ));
    }
    let map = object(body, &["seats", "review", "expiresAt"])?;
    let joined = link.joined()?;

    let seats: Vec<String> = map
        .get("seats")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().map(|s| s.as_str().map(str::to_owned)).collect())
        .ok_or_else(|| invalid("seats is a list of the seats' obi ids, and it's required."))?;
    if seats.is_empty() || seats.len() > MOST_SEATS {
        return Err(invalid(format!("seats has 1 to {MOST_SEATS} obi ids.")));
    }
    let mut seen = std::collections::HashSet::new();
    if let Some(twice) = seats.iter().find(|s| !seen.insert(s.as_str())) {
        return Err(invalid(format!(
            "{twice} is in seats twice: a panel has one seat per member."
        )));
    }
    if seats.contains(&joined.obi_id) {
        return Err(invalid(
            "The convener never sits on its own panel (rule I2): seats can't name this Obi.",
        ));
    }

    let review = map
        .get("review")
        .filter(|r| r.is_object())
        .cloned()
        .ok_or_else(|| invalid("review is a review_request's fields, and it's required."))?;
    let errors = obi_protocol::schema::errors("actions/review_request", &review, 5);
    if !errors.is_empty() {
        return Err(invalid(format!(
            "review doesn't match the spec's review_request: {}",
            errors.join("; ")
        )));
    }
    if review.get("approve_on_host").is_some() {
        return Err(invalid(
            "A panel never approves on the code host in a person's place (rule I7): review can't ask approve_on_host.",
        ));
    }
    let now = obi_protocol::time::now();
    let expires_at = map
        .get("expiresAt")
        .and_then(Value::as_str)
        .and_then(obi_protocol::time::parse)
        .filter(|t| *t >= now + 60 && *t - now <= obi_protocol::MAX_LIFETIME_SECONDS)
        .ok_or_else(|| {
            invalid(
                "expiresAt is a time 60 to 2,592,000 seconds from now, like 2026-10-14T12:00:00Z.",
            )
        })?;

    // Everything that would stop a seat's request, before any is sent: a panel that reaches only
    // some of its seats leaves the others absent, and nobody takes their places.
    let _one_at_a_time = CONVENING.lock().await;
    link.scan_outgoing(&review).await?;
    link.check_seats(&seats).await?;
    let repo = review["repo"].as_str().unwrap_or_default().to_owned();
    let head = review["head_sha"].as_str().unwrap_or_default().to_owned();

    // One request per seat, the same for every seat, each its own exchange.
    let mut answered = Vec::with_capacity(seats.len());
    let mut first_error = None;
    for seat in seats {
        if let Some(exchange) = link.sent_review(&seat, &repo, &head, expires_at) {
            answered.push(json!({"seat": seat, "exchange": exchange}));
            continue;
        }
        let sent = link
            .send_scanned(
                &joined,
                seat.clone(),
                "review_request".into(),
                review.clone(),
                Expiry::At(expires_at),
            )
            .await;
        match sent {
            Ok(exchange) => answered.push(json!({"seat": seat, "exchange": exchange})),
            Err(e) => {
                answered
                    .push(json!({"seat": seat, "status": e.status.as_u16(), "error": e.message}));
                first_error.get_or_insert(e);
            }
        }
    }
    if answered.iter().all(|a| a.get("exchange").is_none())
        && let Some(e) = first_error
    {
        return Err(e);
    }
    Ok(json!({"seats": answered}))
}
