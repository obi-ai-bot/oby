//! The Council's convener (obi-ai-bot/oby-sphere, `spec/council/review.md` section 5): one
//! `review_request` per drawn seat, each its own envelope and exchange, for the same commit, the
//! same conditions and the same words. Behind the `council` setting, with no UI.
//!
//! Who sits is the draw's (`spec/council/draw.md`), made by the clerk outside obid in this version:
//! obid sends what it is given. Each request takes the path every request takes (the spec's schema,
//! the secret scan, the seal, the signature: `Link::send_checked`), and the Sphere relays each
//! exchange between its two Obis only, so no seat sees another's request or result. obid never
//! passes one seat's result to another: the panel is blind by construction (rule I1). A panel never
//! approves on the code host in a person's place (rule I7), so a request asking it is refused.

use axum::http::StatusCode;
use serde_json::{Value, json};

use super::Link;
use crate::api::ApiError;

/// The most seats one call convenes: a panel of 15 is already larger than any profile asks for.
pub const MOST_SEATS: usize = 15;

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, message)
}

/// `POST /v1/obid/sphere/council`: `{"seats": [obi ids], "review": {a review_request body},
/// "expiresIn": seconds}`. Sends the review to every seat, in order, and answers each seat's
/// outgoing exchange, or why its request wasn't sent. `422` with nothing sent when the call itself
/// is wrong; the first seat's error when no request could be sent.
pub async fn convene(link: &Link, body: &[u8]) -> Result<Value, ApiError> {
    if !link.council_enabled() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Council panels are off: \"council\": true in obid's configuration turns them on.",
        ));
    }
    let value: Value = serde_json::from_slice(body)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("The body isn't JSON: {e}")))?;
    let Value::Object(map) = value else {
        return Err(invalid("The body is a JSON object."));
    };
    if let Some(unknown) = map
        .keys()
        .find(|k| !["seats", "review", "expiresIn"].contains(&k.as_str()))
    {
        return Err(invalid(format!(
            "{unknown} isn't a field this endpoint takes."
        )));
    }
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
    let expires_in = map
        .get("expiresIn")
        .and_then(Value::as_i64)
        .filter(|n| (60..=obi_protocol::MAX_LIFETIME_SECONDS).contains(n))
        .ok_or_else(|| invalid("expiresIn is 60 to 2,592,000 seconds."))?;

    // One request per seat, the same for every seat; each is its own exchange.
    let mut answered = Vec::with_capacity(seats.len());
    let mut first_error = None;
    for seat in seats {
        let sent = link
            .send_checked(
                &joined,
                seat.clone(),
                "review_request".into(),
                review.clone(),
                expires_in,
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
