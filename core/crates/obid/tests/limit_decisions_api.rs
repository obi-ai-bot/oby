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

#[test]
fn only_one_signed_answer_wins_for_a_real_pending_group() {
    use obid::job::{Job, Status};
    let folder = Folder::new("limit-answer-race");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let config = config(&folder.0, &remote, &agent);
    let home = folder.0.join("home");
    let dir = home.join("jobs");
    std::fs::create_dir_all(&dir).unwrap();
    for id in ["aaaa0001", "aaaa0002"] {
        let mut job = Job::new(
            id.into(),
            "obi-e2e/fixture-app".into(),
            "research".into(),
            "Look around".into(),
            "claude".into(),
            "stub".into(),
        );
        job.status = Status::Interrupted;
        job.session = Some(format!("{id}-0000-4000-8000-000000000000"));
        job.account_limit_blocked = true;
        job.account_limit_nonce = Some(format!("nonce-{id}"));
        job.account_limit_reset = Some(obi_protocol::time::now() + 3600);
        job.reason = Some("Usage limit reached".into());
        std::fs::write(
            dir.join(format!("{id}.json")),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
    }
    let obid = start(&folder.0, &config, None);
    let unsigned_retry = call(
        &obid,
        "POST",
        "/v1/jobs/aaaa0001/messages",
        Some(&json!({"text": obid::job::TRY_AGAIN})),
    );
    assert_eq!(unsigned_retry.status, 403, "{}", unsigned_retry.text);
    let pending = call(&obid, "GET", "/v1/limit-decisions", None);
    assert_eq!(pending.status, 200, "{}", pending.text);
    let list = pending.body.as_array().unwrap();
    assert_eq!(list.len(), 1, "{}", pending.text);
    let decision = &list[0];
    assert_eq!(decision["jobs"].as_array().unwrap().len(), 2);
    let id = decision["id"].as_str().unwrap();
    let revision = decision["revision"].as_u64().unwrap();
    assert!(
        decision["options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["kind"] == "resume")
    );
    let signed_at = obi_protocol::time::format(obi_protocol::time::now());
    let mut approval = owner_signs(
        &obid,
        &format!("oby-limit-decision\n{id}\n{revision}\nstop\n\n{signed_at}"),
        &signed_at,
    );
    approval["action"] = json!("stop");
    let later = obi_protocol::time::format(obi_protocol::time::now() + 1);
    let mut other = owner_signs(
        &obid,
        &format!("oby-limit-decision\n{id}\n{revision}\nresume\n\n{later}"),
        &later,
    );
    other["action"] = json!("resume");
    let path = format!("/v1/limit-decisions/{id}/answer");
    let stop = json!({"revision":revision,"choice":"stop","approval":approval});
    let resume = json!({"revision":revision,"choice":"resume","approval":other});
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(|| call(&obid, "POST", &path, Some(&stop)));
        let b = s.spawn(|| call(&obid, "POST", &path, Some(&resume)));
        (a.join().unwrap(), b.join().unwrap())
    });
    let mut codes = [a.status, b.status];
    codes.sort();
    assert_eq!(codes, [200, 409], "{} / {}", a.text, b.text);
    assert!(
        call(&obid, "GET", "/v1/limit-decisions", None)
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
}
