//! What the owner's own app signs, written apart from obid's crates so the suite checks obid from
//! outside: the owner key (Ed25519) signs an Oby's card and delegation as the Electron app's
//! `shared/sphere.ts` does (spec identity.md 2.1 and 4.2), and the device key (P-256) signs the
//! owner's words and Confirm taps as its `main/words.ts` and `main/device.ts` do.

use base64::Engine as _;
use ed25519_dalek::Signer as _;
use serde_json::{json, Map, Value};
use sha2::Digest as _;

pub fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// RFC 8785 for what the protocol uses (objects, arrays, strings, integers, booleans, null): keys
/// sorted by UTF-16 code units, as `canonical` in shared/sphere.ts.
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", Value::String(k.clone()), canonical(&map[k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        other => other.to_string(),
    }
}

/// An owner key, from its 32-byte seed in hex (what a node hands the next step).
pub struct OwnerKey(ed25519_dalek::SigningKey);

impl OwnerKey {
    pub fn generate() -> Self {
        Self(ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng))
    }
    pub fn from_hex(seed: &str) -> anyhow::Result<Self> {
        let bytes: [u8; 32] = hex::decode(seed)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("an owner key's seed is 32 bytes"))?;
        Ok(Self(ed25519_dalek::SigningKey::from_bytes(&bytes)))
    }
    pub fn seed_hex(&self) -> String {
        hex::encode(self.0.to_bytes())
    }
    /// Its public half, raw, as base64url (identity.md section 1).
    pub fn public(&self) -> String {
        b64url(self.0.verifying_key().as_bytes())
    }
    pub fn key_id(&self) -> String {
        format!(
            "ed25519:{}",
            b64url(&sha2::Sha256::digest(self.0.verifying_key().as_bytes()))
        )
    }
    fn signed(&self, input: &str) -> String {
        b64url(&self.0.sign(input.as_bytes()).to_bytes())
    }
    pub fn jwt(&self, claims: &Value) -> String {
        let header = json!({"alg": "EdDSA", "kid": self.key_id(), "typ": "JWT"});
        let input = format!(
            "{}.{}",
            b64url(canonical(&header).as_bytes()),
            b64url(canonical(claims).as_bytes())
        );
        format!("{input}.{}", self.signed(&input))
    }
    /// The owner's own assertion for `sphere` (`life` seconds), or with `may_act` a delegation to
    /// that Oby, as `assertion` in shared/sphere.ts.
    pub fn assertion(
        &self,
        owner: &str,
        sphere: &str,
        scope: &str,
        may_act: Option<&str>,
        life: i64,
    ) -> String {
        let iat = chrono::Utc::now().timestamp();
        let mut claims = json!({"iss": owner, "sub": owner, "aud": format!("obi-sphere:{sphere}"),
            "iat": iat, "exp": iat + life, "jti": uuid::Uuid::new_v4().to_string(), "scope": scope});
        if let Some(obi) = may_act {
            claims["may_act"] = json!({"sub": obi});
        }
        self.jwt(&claims)
    }
    /// The card with the owner's one signature: a detached JWS over its canonical JSON.
    pub fn sign_card(&self, card: &Value) -> anyhow::Result<Value> {
        let mut rest: Map<String, Value> = card
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("a card is an object"))?
            .clone();
        rest.remove("signatures");
        let protected = b64url(
            canonical(&json!({"alg": "EdDSA", "kid": self.key_id(), "typ": "JOSE"})).as_bytes(),
        );
        let input = format!(
            "{protected}.{}",
            b64url(canonical(&Value::Object(rest.clone())).as_bytes())
        );
        let signature = self.signed(&input);
        rest.insert(
            "signatures".into(),
            json!([{"protected": protected, "signature": signature}]),
        );
        Ok(Value::Object(rest))
    }
}

/// A device of the owner's (the Mac app's own, as main/device.ts registers it): P-256.
pub struct DeviceKey(p256::ecdsa::SigningKey);

impl DeviceKey {
    pub fn generate() -> Self {
        Self(p256::ecdsa::SigningKey::random(&mut rand::rngs::OsRng))
    }
    pub fn from_hex(secret: &str) -> anyhow::Result<Self> {
        Ok(Self(p256::ecdsa::SigningKey::from_slice(&hex::decode(
            secret,
        )?)?))
    }
    pub fn secret_hex(&self) -> String {
        hex::encode(self.0.to_bytes())
    }
    /// The uncompressed point, base64, as the apps send it to `POST /v1/devices`.
    pub fn public(&self) -> String {
        b64(self.0.verifying_key().to_encoded_point(false).as_bytes())
    }
    /// A DER ECDSA-SHA256 signature, base64.
    pub fn sign(&self, message: &str) -> String {
        let signature: p256::ecdsa::Signature = self.0.sign(message.as_bytes());
        b64(signature.to_der().as_bytes())
    }
}

/// Now, to the millisecond, as the apps sign words (`2026-11-18T12:00:00.123Z`).
pub fn now_millis() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Now, to the second, as a Confirm tap is signed.
pub fn now_seconds() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// `X-Oby-Words` for one request's exact body, as the app's main signs the owner's words.
pub fn words_header(
    device_id: &str,
    key: &DeviceKey,
    method: &str,
    path: &str,
    body: &str,
) -> String {
    let at = now_millis();
    let path = path.split('?').next().unwrap_or(path);
    let digest = hex::encode(sha2::Sha256::digest(body.as_bytes()));
    let signature = key.sign(&format!("oby-words\n{method}\n{path}\n{digest}\n{at}"));
    format!("{device_id} {at} {signature}")
}

/// The owner's tap on Confirm on an item, as `approval` in the answer's body.
pub fn confirm_approval(device_id: &str, key: &DeviceKey, item: &str, title: &str) -> Value {
    let at = now_seconds();
    let signature = key.sign(&format!("oby-confirm\n{item}\n{title}\n{at}"));
    json!({"device": device_id, "action": title, "signedAt": at, "signature": signature})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_sorts_keys_and_keeps_arrays() {
        assert_eq!(
            canonical(&json!({"b": 1, "a": [true, null, "x"], "c": {"z": 1, "y": 2}})),
            r#"{"a":[true,null,"x"],"b":1,"c":{"y":2,"z":1}}"#
        );
    }

    #[test]
    fn an_owner_key_round_trips_and_signs_a_card_once() {
        let key = OwnerKey::generate();
        let again = OwnerKey::from_hex(&key.seed_hex()).unwrap();
        assert_eq!(key.public(), again.public());
        let card = key
            .sign_card(&json!({"name": "x", "signatures": [{"old": 1}]}))
            .unwrap();
        assert_eq!(card["signatures"].as_array().unwrap().len(), 1);
        assert_eq!(card["name"], "x");
    }

    #[test]
    fn a_device_key_round_trips() {
        let key = DeviceKey::generate();
        let again = DeviceKey::from_hex(&key.secret_hex()).unwrap();
        assert_eq!(key.public(), again.public());
        assert!(words_header("d1", &key, "POST", "/v1/x?y=1", "{}").starts_with("d1 "));
    }
}
