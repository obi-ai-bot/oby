//! The account-limit decision is an owner-device action, not an API-token action.
mod support;

use base64::Engine as _;
use p256::ecdsa::signature::Signer as _;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use serde_json::{Value, json};
use std::path::Path;
use support::*;

fn signature(
    key: &p256::ecdsa::SigningKey,
    device: &str,
    id: &str,
    revision: u64,
    choice: &str,
    account: &str,
) -> Value {
    let signed_at = obi_protocol::time::format(obi_protocol::time::now());
    let signed: p256::ecdsa::Signature = key.sign(
        format!("oby-limit-decision\n{id}\n{revision}\n{choice}\n{account}\n{signed_at}")
            .as_bytes(),
    );
    json!({"device":device,"action":choice,"signedAt":signed_at,"signature":base64::engine::general_purpose::STANDARD.encode(signed.to_der())})
}

#[test]
fn limit_answers_require_the_exact_owner_device_signature() {
    let folder = Folder::new("limit-answer-auth");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let config = config(&folder.0, &remote, &agent);
    let obid = start(&folder.0, &config, None);
    let secret = p256::SecretKey::from_slice(&[31; 32]).unwrap();
    let public = base64::engine::general_purpose::STANDARD
        .encode(secret.public_key().to_encoded_point(false).as_bytes());
    let key = p256::ecdsa::SigningKey::from(secret);
    std::fs::write(obid.home.join("pairing"), &public).unwrap();
    let registered = call(
        &obid,
        "POST",
        "/v1/devices",
        Some(&json!({"name":"Test Mac","publicKey":public})),
    );
    assert_eq!(registered.status, 200, "{}", registered.text);
    let device = registered.body["id"].as_str().unwrap();
    let target = "/v1/limit-decisions/e1/answer";
    let send = |value: Value| call(&obid, "POST", target, Some(&value));
    assert_eq!(send(json!({"revision":1,"choice":"stop"})).status, 422);
    let signed = signature(&key, device, "e1", 1, "move", "a1");
    for (revision, choice, account) in [(2, "move", "a1"), (1, "move", "a2"), (1, "resume", "")] {
        let body = if account.is_empty() {
            json!({"revision":revision,"choice":choice,"approval":signed})
        } else {
            json!({"revision":revision,"choice":choice,"account":account,"approval":signed})
        };
        assert_eq!(send(body).status, 403);
    }
    let wrong_id = call(
        &obid,
        "POST",
        "/v1/limit-decisions/e2/answer",
        Some(&json!({"revision":1,"choice":"move","account":"a1","approval":signed})),
    );
    assert_eq!(wrong_id.status, 403);
    let exact = json!({"revision":1,"choice":"move","account":"a1","approval":signed});
    // Signature succeeds; the nonexistent decision is still refused by domain validation.
    assert_eq!(send(exact.clone()).status, 409);
    assert_eq!(
        send(exact).status,
        403,
        "a consumed signature cannot be replayed"
    );
}

/// LIMITS-25: `GET /v1/limit-decisions` is always `[]`, on a fresh home and on one an older
/// release left with a pending and a kept-Stop decision; a signed answer to one is `409`, and its
/// spent signature can't be sent again.
#[test]
fn limits_25_decisions_list_is_empty() {
    use obid::job::{Job, Status};
    let folder = Folder::new("limits-25");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let config = config(&folder.0, &remote, &agent);
    let mut obid = start(&folder.0, &config, None);
    let fresh = call(&obid, "GET", "/v1/limit-decisions", None);
    assert_eq!(
        (fresh.status, &fresh.body),
        (200, &json!([])),
        "{}",
        fresh.text
    );
    obid.stop();
    let dir = folder.0.join("home/jobs");
    for (id, episode) in [("aaaa0001", "d-pending"), ("aaaa0002", "d-stop")] {
        let mut job = Job::new(
            id.into(),
            "obi-e2e/fixture-app".into(),
            "research".into(),
            "Look around".into(),
            "claude".into(),
            "stub".into(),
        );
        job.status = Status::Interrupted;
        job.account_limit_blocked = true;
        job.account_limit_episode = Some(episode.into());
        job.account_limit_nonce = Some(format!("nonce-{id}"));
        job.account_limit_reset = Some(obi_protocol::time::now() + 3600);
        std::fs::write(
            dir.join(format!("{id}.json")),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
    }
    older_limit_decisions(
        &folder.0,
        &[
            ("d-pending", &["aaaa0001"], "pending"),
            ("d-stop", &["aaaa0002"], "stop"),
        ],
    );
    let obid = start(&folder.0, &config, None);
    let listed = call(&obid, "GET", "/v1/limit-decisions", None);
    assert_eq!(
        (listed.status, &listed.body),
        (200, &json!([])),
        "{}",
        listed.text
    );
    let signed_at = obi_protocol::time::format(obi_protocol::time::now());
    let mut approval = owner_signs(
        &obid,
        &format!("oby-limit-decision\nd-pending\n3\nresume\n\n{signed_at}"),
        &signed_at,
    );
    approval["action"] = json!("resume");
    let body = json!({"revision": 3, "choice": "resume", "approval": approval});
    let target = "/v1/limit-decisions/d-pending/answer";
    let answered = call(&obid, "POST", target, Some(&body));
    assert_eq!(answered.status, 409, "{}", answered.text);
    let again = call(&obid, "POST", target, Some(&body));
    assert_eq!(
        again.status, 403,
        "a spent signature can't be sent again: {}",
        again.text
    );
}
