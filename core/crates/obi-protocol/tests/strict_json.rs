//! A card's signed header is read strictly (protocol.md section 2; identity.md section 2.2): a
//! header the owner really signed is still refused when it names a member twice, or isn't UTF-8.

use obi_protocol::card::{self, Context};
use obi_protocol::keys::{Keypair, b64url};
use serde_json::{Value, json};

const SPHERE: &str = "conf-sphere";

fn key(label: &str) -> Keypair {
    Keypair::from_seed_string(&format!("obi-conformance-test-key-{label}"))
}

/// A valid card of obi-a for owner-a, unsigned.
fn card(obi: &Keypair, owner: &Keypair) -> Value {
    let skill = |id: &str| json!({"id": id, "name": "Take it", "description": "Takes it for its owner.", "tags": ["obi-action"]});
    json!({
        "protocolVersion": "0.3.0",
        "name": "owner-a's Obi",
        "description": "A personal delegate for an engineer.",
        "url": format!("https://sphere.example.invalid/v0.1/spheres/{SPHERE}/members/obi-a"),
        "preferredTransport": "HTTP+JSON",
        "version": "0.1.0",
        "capabilities": {
            "streaming": false, "pushNotifications": false,
            "extensions": [{
                "uri": "urn:obi-sphere:ext:obi:0.1", "required": true,
                "params": {"protocol": "obi/0.1", "sphere": SPHERE, "obi_id": "obi-a", "owner": "owner-a",
                           "owner_key": owner.public_key(), "key_id": obi.key_id(),
                           "public_key": obi.public_key(), "accepts": ["review_request", "question"],
                           "card_seq": 1, "issued_at": "2026-10-14T11:59:00Z"}
            }]
        },
        "defaultInputModes": ["application/json"],
        "defaultOutputModes": ["application/json"],
        "skills": [skill("review_request"), skill("question")],
    })
}

/// `unsigned` signed by `owner` under the protected header whose bytes are `header`.
fn signed_with_header(unsigned: &Value, owner: &Keypair, header: &[u8]) -> Value {
    let protected = b64url(header);
    let signature = owner.sign(&card::signing_input(unsigned, &protected).unwrap());
    let mut card = unsigned.clone();
    card["signatures"] = json!([{"protected": protected, "signature": signature}]);
    card
}

#[test]
fn a_signed_header_with_a_member_twice_or_not_utf8_is_refused() {
    let (a, oa) = (key("obi-a"), key("owner-a"));
    let context = Context::from_value(
        &json!({"now": "2026-10-14T12:00:00Z", "bound_owner_key": oa.public_key()}),
    );
    let unsigned = card(&a, &oa);
    let kid = oa.key_id();
    // The header the spec writes verifies.
    let plain = format!(r#"{{"alg":"EdDSA","kid":"{kid}","typ":"JOSE"}}"#);
    assert_eq!(
        card::verify(
            &signed_with_header(&unsigned, &oa, plain.as_bytes()),
            &context
        ),
        Ok(())
    );
    // The same, with kid twice: the last is the owner's, so a reader that keeps the last would
    // take it; one that keeps the first wouldn't.
    let twice = format!(
        r#"{{"alg":"EdDSA","kid":"{}","kid":"{kid}","typ":"JOSE"}}"#,
        a.key_id()
    );
    let refused = card::verify(
        &signed_with_header(&unsigned, &oa, twice.as_bytes()),
        &context,
    )
    .unwrap_err();
    assert_eq!(refused.code, "card_invalid");
    assert_eq!(refused.message, "the protected header is not JSON");
    // A byte that isn't UTF-8, in place of one of typ's letters, which a lossy read would replace.
    let mut not_utf8 = plain.clone().into_bytes();
    let at = plain.find("JOSE").unwrap();
    not_utf8[at] = 0xFF;
    let refused =
        card::verify(&signed_with_header(&unsigned, &oa, &not_utf8), &context).unwrap_err();
    assert_eq!(refused.code, "card_invalid");
    assert_eq!(refused.message, "the protected header is not JSON");
}
