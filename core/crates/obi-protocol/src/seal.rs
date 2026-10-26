//! Sealed words (protocol.md section 9.1): every text value of an envelope's body, sealed for the
//! receiver's seal key with a new X25519 key of the sender's, HKDF-SHA256 and ChaCha20-Poly1305
//! (aws-lc-rs, as obid's job channel and phone relay), so the Sphere relays and checks a message,
//! and a member checks the chain, without reading what anyone wrote. A sender calls [`seal`]
//! before it signs; a receiver calls [`check`] once the signature has passed.

use std::collections::HashMap;
use std::sync::OnceLock;

use aws_lc_rs::{aead, agreement, hkdf};
use serde_json::{Map, Value, json};

use crate::keys::{b64url, from_b64url, sha256};
use crate::{Rejection, envelope, jcs, schema};

/// The one suite of v0.1.
pub const SUITE: &str = "x25519-hkdf-sha256-chacha20poly1305";
/// What the HKDF info starts with, before the canonical JSON of the envelope's header.
const INFO_PREFIX: &[u8] = b"obi/0.1 seal\n";

/// An X25519 seal key: an Obi's own (its card publishes the public half), never anyone else's.
/// Its `Debug` shows the key id only.
#[derive(Clone)]
pub struct SealKey {
    private: [u8; 32],
    public: [u8; 32],
}

impl std::fmt::Debug for SealKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SealKey({})", self.key_id())
    }
}

impl SealKey {
    /// A new key from 32 random bytes.
    pub fn generate() -> Result<SealKey, String> {
        let mut private = [0u8; 32];
        aws_lc_rs::rand::fill(&mut private).map_err(|_| "no random seal key".to_owned())?;
        SealKey::from_private(private)
    }

    pub fn from_private(private: [u8; 32]) -> Result<SealKey, String> {
        let public = public_of(&private).map_err(str::to_owned)?;
        Ok(SealKey { private, public })
    }

    /// The private half's 32 bytes: for obid to keep it, and nothing else.
    pub fn private_bytes(&self) -> &[u8; 32] {
        &self.private
    }

    pub fn public(&self) -> &[u8; 32] {
        &self.public
    }

    /// `seal_public_key`: base64url of the public half.
    pub fn public_key(&self) -> String {
        b64url(self.public)
    }

    /// `seal_key_id`.
    pub fn key_id(&self) -> String {
        key_id_of(&self.public)
    }
}

/// A seal key's id: `x25519:` and base64url of SHA-256 over its 32 raw bytes.
pub fn key_id_of(public: &[u8]) -> String {
    format!("x25519:{}", b64url(sha256(public)))
}

/// A card's seal key, when it has one whose id matches it (identity.md 2.2).
pub fn card_key(params: &Map<String, Value>) -> Option<[u8; 32]> {
    let public = params
        .get("seal_public_key")
        .and_then(Value::as_str)
        .and_then(from_b64url)
        .and_then(|b| <[u8; 32]>::try_from(b).ok())?;
    (params.get("seal_key_id").and_then(Value::as_str) == Some(key_id_of(&public).as_str()))
        .then_some(public)
}

fn public_of(private: &[u8; 32]) -> Result<[u8; 32], &'static str> {
    let key = agreement::PrivateKey::from_private_key(&agreement::X25519, private)
        .map_err(|_| "that isn't an X25519 private key")?;
    let public = key
        .compute_public_key()
        .map_err(|_| "that X25519 key has no public key")?;
    <[u8; 32]>::try_from(public.as_ref()).map_err(|_| "an X25519 public key is 32 bytes")
}

/// X25519, refusing a public key that isn't one and an all-zero secret (RFC 7748 6.1).
fn agree(private: &[u8; 32], public: &[u8]) -> Result<[u8; 32], &'static str> {
    let private = agreement::PrivateKey::from_private_key(&agreement::X25519, private)
        .map_err(|_| "that isn't an X25519 private key")?;
    agreement::agree(
        &private,
        agreement::UnparsedPublicKey::new(&agreement::X25519, public),
        "that isn't an X25519 key",
        |shared| {
            <[u8; 32]>::try_from(shared)
                .ok()
                .filter(|s| s.iter().any(|b| *b != 0))
                .ok_or("an all-zero X25519 secret")
        },
    )
}

/// The envelope's key: HKDF-SHA256 of the X25519 secret, salted with `enc` and the receiver's seal
/// key, its info the prefix and the canonical JSON of `{sphere, id, from, to, type}`.
fn derive(
    shared: &[u8; 32],
    enc: &[u8; 32],
    recipient: &[u8; 32],
    envelope: &Map<String, Value>,
) -> Result<[u8; 32], String> {
    let header: Map<String, Value> = ["sphere", "id", "from", "to", "type"]
        .into_iter()
        .filter_map(|k| Some((k.to_owned(), envelope.get(k)?.clone())))
        .collect();
    let info = [
        INFO_PREFIX,
        &jcs::canonical_bytes(&Value::Object(header)).map_err(|e| e.to_string())?,
    ]
    .concat();
    let mut key = [0u8; 32];
    hkdf::Salt::new(hkdf::HKDF_SHA256, &[enc.as_slice(), recipient].concat())
        .extract(shared)
        .expand(&[&info], hkdf::HKDF_SHA256)
        .and_then(|okm| okm.fill(&mut key))
        .map_err(|_| "HKDF failed".to_owned())?;
    Ok(key)
}

fn aead_key(key: &[u8; 32]) -> aead::LessSafeKey {
    aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::CHACHA20_POLY1305, key).expect("a 32-byte key"),
    )
}

/// A value's nonce: the first 12 bytes of SHA-256 of its pointer, unique in its envelope.
fn nonce(pointer: &str) -> aead::Nonce {
    let mut bytes = [0u8; 12];
    bytes.copy_from_slice(&sha256(pointer.as_bytes())[..12]);
    aead::Nonce::assume_unique_for_key(bytes)
}

/// Each action type's text paths (`sealed/text-paths.json`), as pointer tokens, `*` for every item
/// of an array.
fn text_paths() -> &'static HashMap<String, Vec<Vec<String>>> {
    static PATHS: OnceLock<HashMap<String, Vec<Vec<String>>>> = OnceLock::new();
    PATHS.get_or_init(|| {
        let file: Value =
            serde_json::from_str(include_str!("../spec/schemas/v0.1/sealed/text-paths.json"))
                .expect("text-paths.json is JSON");
        file["paths"]
            .as_object()
            .expect("text-paths.json has paths")
            .iter()
            .map(|(kind, paths)| {
                let paths = paths.as_array().into_iter().flatten();
                let tokens = paths
                    .filter_map(Value::as_str)
                    .map(|p| p.split('/').skip(1).map(unescape).collect())
                    .collect();
                (kind.clone(), tokens)
            })
            .collect()
    })
}

fn unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

fn pointer(at: &[String]) -> String {
    at.iter()
        .map(|t| format!("/{}", t.replace('~', "~0").replace('/', "~1")))
        .collect()
}

/// Calls `each` with every text value of a `kind` body and its pointer, in the body's own order.
fn texts(
    kind: &str,
    body: &mut Value,
    each: &mut dyn FnMut(&str, &mut String) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    fn walk(
        value: &mut Value,
        at: &mut Vec<String>,
        patterns: &[Vec<String>],
        each: &mut dyn FnMut(&str, &mut String) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        let index = |t: &str| t == "0" || (!t.starts_with('0') && t.parse::<u32>().is_ok());
        match value {
            Value::String(text) => {
                let hit = patterns.iter().any(|p| {
                    p.len() == at.len()
                        && p.iter()
                            .zip(at.iter())
                            .all(|(p, t)| p == t || (p == "*" && index(t)))
                });
                if hit {
                    each(&pointer(at), text)?;
                }
            }
            Value::Array(items) => {
                for (i, item) in items.iter_mut().enumerate() {
                    at.push(i.to_string());
                    walk(item, at, patterns, each)?;
                    at.pop();
                }
            }
            Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    at.push(key.clone());
                    walk(item, at, patterns, each)?;
                    at.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
    let patterns = text_paths().get(kind).map_or(&[][..], Vec::as_slice);
    walk(body, &mut Vec::new(), patterns, each)
}

/// Seals the words of `unsigned` (an envelope without its signature) for the seal key
/// `recipient`, and adds `seal`. Then it is signed.
pub fn seal(unsigned: &mut Map<String, Value>, recipient: &[u8; 32]) -> Result<(), String> {
    seal_with(unsigned, recipient, SealKey::generate()?.private_bytes())
}

/// [`seal`] with the sender's new X25519 key given: the known answers, and nothing else, give it.
pub fn seal_with(
    unsigned: &mut Map<String, Value>,
    recipient: &[u8; 32],
    ephemeral: &[u8; 32],
) -> Result<(), String> {
    let enc = public_of(ephemeral)?;
    let key = aead_key(&derive(
        &agree(ephemeral, recipient)?,
        &enc,
        recipient,
        unsigned,
    )?);
    let kind = unsigned
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let body = unsigned.get_mut("body").ok_or("an envelope has a body")?;
    texts(&kind, body, &mut |pointer, text| {
        let mut bytes = text.as_bytes().to_vec();
        key.seal_in_place_append_tag(
            nonce(pointer),
            aead::Aad::from(pointer.as_bytes()),
            &mut bytes,
        )
        .map_err(|_| "a value couldn't be sealed")?;
        *text = b64url(bytes);
        Ok(())
    })?;
    unsigned.insert(
        "seal".into(),
        json!({"suite": SUITE, "key_id": key_id_of(recipient), "enc": b64url(enc)}),
    );
    Ok(())
}

/// The body of a sealed envelope with its words opened, for the holder of `key`: `seal_invalid`
/// when a value doesn't open (changed, moved, copied from another envelope, for another key) or
/// isn't UTF-8.
pub fn open(envelope: &Value, key: &SealKey) -> Result<Value, Rejection> {
    let invalid = |m: &str| Rejection::new("seal_invalid", m);
    let enc = envelope["seal"]["enc"]
        .as_str()
        .and_then(from_b64url)
        .and_then(|b| <[u8; 32]>::try_from(b).ok())
        .ok_or_else(|| invalid("its seal has no enc"))?;
    let header = envelope
        .as_object()
        .ok_or_else(|| invalid("not an object"))?;
    let shared = agree(&key.private, &enc).map_err(invalid)?;
    let derived = derive(&shared, &enc, &key.public, header).map_err(|e| invalid(&e))?;
    let aead = aead_key(&derived);
    let mut body = envelope["body"].clone();
    let kind = envelope["type"].as_str().unwrap_or("");
    texts(kind, &mut body, &mut |pointer, text| {
        let mut bytes = from_b64url(text).ok_or("a sealed value isn't base64url")?;
        let plain = aead
            .open_in_place(
                nonce(pointer),
                aead::Aad::from(pointer.as_bytes()),
                &mut bytes,
            )
            .map_err(|_| "a sealed value doesn't open")?;
        *text = String::from_utf8(plain.to_vec()).map_err(|_| "an opened value isn't UTF-8")?;
        Ok(())
    })
    .map_err(invalid)?;
    Ok(body)
}

/// Check 11 (protocol.md section 9.1), once checks 1 to 10 have passed, as the receiver with the
/// seal keys `own` judges it: a sealed envelope's opened body, checked against the plain schema and
/// check 3; `None` for one in the clear, which is `not_sealed` when the receiver has a seal key
/// and the sender's trusted card has one too (`sender_seals`).
pub fn check(
    envelope: &Value,
    own: &HashMap<String, SealKey>,
    sender_seals: bool,
) -> Result<Option<Value>, Rejection> {
    if envelope.get("seal").is_none() {
        if !own.is_empty() && sender_seals {
            return Err(Rejection::new(
                "not_sealed",
                "both Obis seal: its words must be sealed",
            ));
        }
        return Ok(None);
    }
    let key_id = envelope["seal"]["key_id"].as_str().unwrap_or("");
    let key = own.get(key_id).ok_or_else(|| {
        Rejection::new(
            "seal_invalid",
            format!("sealed for {key_id}, not a seal key of the receiver's"),
        )
    })?;
    let body = open(envelope, key)?;
    let mut opened = envelope.clone();
    opened["body"] = body.clone();
    if let Some(map) = opened.as_object_mut() {
        map.remove("seal");
    }
    let errors = schema::errors("envelope", &opened, 5);
    if !errors.is_empty() {
        // The schema's words may quote the opened text: the code alone goes on.
        return Err(Rejection::new(
            "seal_invalid",
            "opened, it doesn't match the schema",
        ));
    }
    let problems = envelope::semantic_problems(&opened);
    if !problems.is_empty() {
        return Err(Rejection::new(
            "seal_invalid",
            format!("opened: {}", problems.join("; ")),
        ));
    }
    Ok(Some(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../spec/conformance/seal-vectors.json")).unwrap()
    }

    fn bytes32(text: &Value) -> [u8; 32] {
        <[u8; 32]>::try_from(from_b64url(text.as_str().unwrap()).unwrap()).unwrap()
    }

    /// seal-vectors.json, recomputed: the keys, the secret, the HKDF output, each value's nonce
    /// and ciphertext, and the sealed envelope, which opens to the plain body.
    #[test]
    fn the_specs_seal_vectors() {
        let v = vectors();
        let recipient = SealKey::from_private(bytes32(&v["recipient"]["x25519_private"])).unwrap();
        assert_eq!(recipient.public_key(), v["recipient"]["seal_public_key"]);
        assert_eq!(recipient.key_id(), v["recipient"]["seal_key_id"]);
        let ephemeral = bytes32(&v["sender_x25519_private"]);
        let enc = public_of(&ephemeral).unwrap();
        assert_eq!(b64url(enc), v["enc"]);
        let shared = agree(&ephemeral, recipient.public()).unwrap();
        assert_eq!(b64url(shared), v["shared"]);
        assert_eq!(agree(&recipient.private, &enc).unwrap(), shared);
        let plain = v["plain"].as_object().unwrap().clone();
        let derived = derive(&shared, &enc, recipient.public(), &plain).unwrap();
        assert_eq!(b64url(derived), v["hkdf_output"]);
        for value in v["values"].as_array().unwrap() {
            let pointer = value["pointer"].as_str().unwrap();
            assert_eq!(b64url(nonce_bytes(pointer)), value["nonce"]);
        }
        let mut sealed = plain.clone();
        seal_with(&mut sealed, recipient.public(), &ephemeral).unwrap();
        let mut expected = v["sealed"].clone();
        expected.as_object_mut().unwrap().remove("signature");
        assert_eq!(Value::Object(sealed), expected);
        assert_eq!(open(&v["sealed"], &recipient).unwrap(), v["plain"]["body"]);
    }

    fn nonce_bytes(pointer: &str) -> [u8; 12] {
        sha256(pointer.as_bytes())[..12].try_into().unwrap()
    }

    fn review(goal: &str) -> Map<String, Value> {
        json!({"protocol": "obi/0.1", "id": "8a6c1f4e-5d2b-4c7a-9e3f-1b2c3d4e5f60", "type": "review_request",
               "from": "obi-b", "to": "obi-a", "sphere": "s", "correlation_id": "8a6c1f4e-5d2b-4c7a-9e3f-1b2c3d4e5f60",
               "created_at": "2026-10-14T11:59:00Z", "expires_at": "2026-10-14T13:00:00Z",
               "key_id": format!("ed25519:{}", "A".repeat(43)),
               "body": {"goal": goal, "repo": "a/b", "base_branch": "main", "head_branch": "f", "head_sha": "6d20146f366b9fac8907b5066c30ee39aaec09b3",
                        "acceptance": [{"kind": "tests_pass"}, {"kind": "no_blocker_findings", "note": "Blockers first."}],
                        "summary": "What changed."}})
        .as_object()
        .unwrap()
        .clone()
    }

    #[test]
    fn only_the_fields_marked_as_text_are_sealed() {
        let key = SealKey::generate().unwrap();
        let mut sealed = review("Add average()");
        seal(&mut sealed, key.public()).unwrap();
        let body = &sealed["body"];
        for (field, plain) in [
            ("repo", "a/b"),
            ("base_branch", "main"),
            ("head_branch", "f"),
        ] {
            assert_eq!(body[field], plain);
        }
        assert_eq!(body["acceptance"][0], json!({"kind": "tests_pass"}));
        assert_eq!(body["acceptance"][1]["kind"], "no_blocker_findings");
        for sealed_text in [
            &body["goal"],
            &body["summary"],
            &body["acceptance"][1]["note"],
        ] {
            let text = sealed_text.as_str().unwrap();
            assert!(!text.contains(' ') && text.len() > 22, "{text}");
        }
        assert_eq!(sealed["seal"]["key_id"], key.key_id());
        let envelope = Value::Object(sealed);
        assert!(schema::is_valid(
            "sealed/actions/review_request",
            &envelope["body"]
        ));
        assert_eq!(
            open(&envelope, &key).unwrap(),
            review("Add average()")["body"]
        );
    }

    #[test]
    fn a_value_moved_to_another_field_doesnt_open() {
        let key = SealKey::generate().unwrap();
        let mut sealed = review("Add average()");
        seal(&mut sealed, key.public()).unwrap();
        let body = sealed["body"].as_object_mut().unwrap();
        let goal = body["goal"].clone();
        body.insert("goal".into(), body["summary"].clone());
        body.insert("summary".into(), goal);
        let rejection = open(&Value::Object(sealed), &key).unwrap_err();
        assert_eq!(rejection.code, "seal_invalid");
    }

    #[test]
    fn another_key_or_a_changed_value_doesnt_open_and_the_debug_form_hides_the_key() {
        let (key, other) = (SealKey::generate().unwrap(), SealKey::generate().unwrap());
        let mut sealed = review("Add average()");
        seal(&mut sealed, key.public()).unwrap();
        let envelope = Value::Object(sealed.clone());
        assert_eq!(open(&envelope, &other).unwrap_err().code, "seal_invalid");
        let own: HashMap<String, SealKey> = [(other.key_id(), other.clone())].into();
        assert_eq!(
            check(&envelope, &own, true).unwrap_err().code,
            "seal_invalid"
        );
        let goal = sealed["body"]["goal"].as_str().unwrap().to_owned();
        let flipped = format!(
            "{}{}",
            if goal.starts_with('A') { "B" } else { "A" },
            &goal[1..]
        );
        sealed["body"]["goal"] = json!(flipped);
        assert_eq!(
            open(&Value::Object(sealed), &key).unwrap_err().code,
            "seal_invalid"
        );
        let shown = format!("{key:?}");
        assert!(!shown.contains(&b64url(key.private_bytes())) && shown.contains("x25519:"));
    }
}
