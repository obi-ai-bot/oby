//! A registered device's approval to publish one job by hand (docs/runner-api.md, `POST
//! /v1/jobs/{id}/publish`), as Runner/Devices.swift checks it: the device's P-256 key signed
//! `"oby-publish\n<job id>\n<action>\n<signedAt>"` on the owner’s click (ECDSA with
//! SHA-256, DER, base64). obid lets it through once: from a registered device, with a signature
//! that matches its key, for exactly obid's own sentence, signed within two minutes, and never
//! used before. The API token alone can't publish by hand: agents run on the same machine.
//!
//! A Confirm on an inbox `confirm` item is the same proof over its own sentence (#776):
//! `"oby-confirm\n<item id>\n<title>\n<signedAt>"`, so no publish or connection signature passes
//! for a Confirm, and one tap confirms one item.
//!
//! Words the owner typed in Oby's apps carry the same proof, on every request that has them
//! ([`WORDS`], crate::secretspec): the device signed `"oby-words\n<method>\n<path>\n<SHA-256 of
//! the body, hex>\n<signedAt>"`, so a token alone (oby-mcp, a script, an agent) never passes for
//! the owner's own words, and one signature passes for one request.
//!
//! What was used is kept in `home/approvals.json` (0600) for 10 minutes, so a restart doesn't
//! open a replay.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine as _;
use p256::ecdsa::signature::Verifier as _;

/// How far `signedAt` may be from now, either way.
const FRESH: i64 = 120;
/// How long a used approval is remembered: well past the time it could be fresh.
const REMEMBERED: i64 = 600;

/// What a device sends to publish by hand.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub device: String,
    pub action: String,
    pub signed_at: String,
    pub signature: String,
}

/// The exact business choice covered by an account-limit approval.
pub struct LimitChoice<'a> {
    pub id: &'a str,
    pub revision: u64,
    pub choice: &'a str,
    pub account: &'a str,
}

/// The approvals used in the last 10 minutes: the hash of each device's id and signed message,
/// and when it was signed.
pub struct Approvals {
    file: PathBuf,
    used: Mutex<HashMap<String, i64>>,
    /// Until when no approval goes through, when the file was there and didn't read: what it held
    /// could have been used, so a replay can't be ruled out until it would be stale.
    unread_until: Option<i64>,
}

impl Approvals {
    /// The approvals kept in `home/approvals.json`. When the file is there and doesn't read, no
    /// approval goes through for 10 minutes.
    pub fn open(home: &Path) -> Approvals {
        let file = home.join("approvals.json");
        let (used, unread_until) = match std::fs::read(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (HashMap::new(), None),
            read => match read.ok().and_then(|b| serde_json::from_slice(&b).ok()) {
                Some(used) => (used, None),
                None => {
                    tracing::error!(
                        "can't read {}: no approval goes through for 10 minutes",
                        file.display()
                    );
                    (HashMap::new(), Some(obi_protocol::time::now() + REMEMBERED))
                }
            },
        };
        Approvals {
            file,
            used: Mutex::new(used),
            unread_until,
        }
    }

    /// Lets `approval` through once for publishing `job` by `action`, obid's own sentence, at
    /// `now` (seconds since the epoch). `key` is the registered device's public key (X9.63,
    /// base64), or none for a device obid doesn't know. `Err` is the line to show (`403`).
    pub fn check(
        &self,
        approval: &Approval,
        job: &str,
        action: &str,
        key: Option<&str>,
        now: i64,
    ) -> Result<(), String> {
        let message = format!(
            "oby-publish\n{job}\n{}\n{}",
            approval.action, approval.signed_at
        );
        self.check_signed(approval, &message, action, key, now)
    }

    /// Lets `approval` through once for confirming inbox item `item`, whose title is `title`
    /// (#776): the device signed `"oby-confirm\n<item>\n<title>\n<signedAt>"`.
    pub fn check_confirm(
        &self,
        approval: &Approval,
        item: &str,
        title: &str,
        key: Option<&str>,
        now: i64,
    ) -> Result<(), String> {
        let message = format!(
            "oby-confirm\n{item}\n{}\n{}",
            approval.action, approval.signed_at
        );
        self.check_signed(approval, &message, title, key, now)
    }

    /// Approve exactly one revision and choice of an account-limit decision. The account is
    /// included even when empty, so an approval for a move cannot authorize another destination.
    pub fn check_limit_decision(
        &self,
        approval: &Approval,
        intent: LimitChoice<'_>,
        key: Option<&str>,
        now: i64,
    ) -> Result<(), String> {
        let LimitChoice {
            id,
            revision,
            choice,
            account,
        } = intent;
        if [id, choice, account]
            .iter()
            .any(|s| s.contains(['\n', '\r']))
        {
            return Err("Invalid limit decision.".into());
        }
        let message = format!(
            "oby-limit-decision\n{id}\n{revision}\n{choice}\n{account}\n{}",
            approval.signed_at
        );
        self.check_signed(approval, &message, choice, key, now)
    }

    /// Proof that the authenticated phone holds an already registered, previously unbound key.
    pub fn check_pairing_binding(
        &self,
        approval: &Approval,
        id: &str,
        pairing: &str,
        nonce: &str,
        key: Option<&str>,
        now: i64,
    ) -> Result<(), String> {
        if approval.device != id
            || [id, pairing, nonce]
                .iter()
                .any(|s| s.contains(['\n', '\r']))
        {
            return Err("Invalid device-link proof.".into());
        }
        let message = format!(
            "oby-device-pairing\n{id}\n{pairing}\n{nonce}\n{}",
            approval.signed_at
        );
        self.check_signed(approval, &message, "bind-pairing", key, now)
    }

    /// Lets the request `method` `path` with `body` through once as words the owner typed in Oby's
    /// apps: `header` is [`WORDS`]'s value, `"<device> <signedAt> <signature>"`, and `key_of` the
    /// registered key of a device by id.
    pub fn check_words(
        &self,
        header: &str,
        method: &str,
        path: &str,
        body: &[u8],
        key_of: impl Fn(&str) -> Option<String>,
        now: i64,
    ) -> Result<(), String> {
        let mut parts = header.split(' ');
        let (Some(device), Some(signed_at), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(format!("{WORDS} is \"<device> <signedAt> <signature>\"."));
        };
        // Milliseconds keep two of the same request within a second apart: the message is signed
        // with them, its freshness read to the second.
        let whole = match signed_at.split_once('.') {
            Some((seconds, rest))
                if rest.len() > 1
                    && rest.ends_with('Z')
                    && rest[..rest.len() - 1].bytes().all(|b| b.is_ascii_digit()) =>
            {
                format!("{seconds}Z")
            }
            _ => signed_at.to_owned(),
        };
        let approval = Approval {
            device: device.into(),
            action: String::new(),
            signed_at: whole,
            signature: signature.into(),
        };
        let message = words_message(method, path, body, signed_at);
        self.check_signed(&approval, &message, "", key_of(device).as_deref(), now)
    }

    /// The checks every approval takes, on the exact `message` the device signed.
    fn check_signed(
        &self,
        approval: &Approval,
        message: &str,
        action: &str,
        key: Option<&str>,
        now: i64,
    ) -> Result<(), String> {
        if self.unread_until.is_some_and(|until| now < until) {
            return Err(
                "obid couldn't read the approvals it used. Approve it again in 10 minutes.".into(),
            );
        }
        let Some(key) = key.and_then(verifying_key) else {
            return Err("That device isn't registered with this runner.".into());
        };
        let signature = base64::engine::general_purpose::STANDARD
            .decode(approval.signature.trim())
            .ok()
            .and_then(|der| p256::ecdsa::Signature::from_der(&der).ok());
        if signature.is_none_or(|s| key.verify(message.as_bytes(), &s).is_err()) {
            return Err("The signature doesn't match the device's key.".into());
        }
        if approval.action != action {
            return Err("The device approved something else. Approve it again.".into());
        }
        let signed_at = obi_protocol::time::parse(&approval.signed_at)
            .filter(|at| (at - now).abs() <= FRESH)
            .ok_or("The approval is more than two minutes old. Approve it again.")?;
        // The signed message, not the signature: an ECDSA signature can be rewritten into another
        // valid one.
        let hash = hex::encode(obi_protocol::keys::sha256(
            [approval.device.as_bytes(), message.as_bytes()].concat(),
        ));
        let mut used = self.used.lock().unwrap_or_else(|e| e.into_inner());
        used.retain(|_, at| now - *at < REMEMBERED);
        if used.contains_key(&hash) {
            return Err("That approval was already used. Approve it again.".into());
        }
        used.insert(hash, signed_at);
        let bytes = serde_json::to_vec(&*used).map_err(|e| e.to_string())?;
        if let Err(e) = crate::store::write_private(&self.file, &bytes) {
            // Not kept, so not used: a replay after a restart must not get through either.
            tracing::error!("can't keep the used approvals: {e}");
            return Err("obid couldn't keep the approval. Approve it again.".into());
        }
        Ok(())
    }
}

/// The header an app of the owner's signs a request with when it carries words the owner typed.
pub const WORDS: &str = "x-oby-words";

/// What a device signs for the request `method` `path` (from `/v1/` on) with `body`.
pub fn words_message(method: &str, path: &str, body: &[u8], signed_at: &str) -> String {
    format!(
        "oby-words\n{method}\n{path}\n{}\n{signed_at}",
        hex::encode(obi_protocol::keys::sha256(body))
    )
}

/// The verifying key in a device's registered public key, or none.
fn verifying_key(public_key: &str) -> Option<p256::ecdsa::VerifyingKey> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(public_key)
        .ok()?;
    p256::ecdsa::VerifyingKey::from_sec1_bytes(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Signer as _;
    use p256::elliptic_curve::sec1::ToEncodedPoint;

    const NOW: i64 = 1_791_582_173;

    /// A device's key, from a fixed private scalar: no randomness, no key in the source.
    fn device(scalar: u8) -> (p256::ecdsa::SigningKey, String) {
        let secret = p256::SecretKey::from_slice(&[scalar; 32]).unwrap();
        let public = secret.public_key().to_encoded_point(false);
        (
            p256::ecdsa::SigningKey::from(secret),
            base64::engine::general_purpose::STANDARD.encode(public.as_bytes()),
        )
    }

    fn approval(key: &p256::ecdsa::SigningKey, job: &str, action: &str, at: i64) -> Approval {
        let signed_at = obi_protocol::time::format(at);
        let signature: p256::ecdsa::Signature =
            key.sign(format!("oby-publish\n{job}\n{action}\n{signed_at}").as_bytes());
        Approval {
            device: "d1".into(),
            action: action.into(),
            signed_at,
            signature: base64::engine::general_purpose::STANDARD.encode(signature.to_der()),
        }
    }

    fn home(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("obid-approval-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// Words the owner typed pass once, for exactly the request their device signed: not for
    /// another method, path or body, not from a device obid doesn't know, not when they're stale,
    /// and never as a publish or a Confirm.
    #[test]
    fn typed_words_pass_once_for_exactly_their_request() {
        let home = home("words");
        let approvals = Approvals::open(&home);
        let (key, public) = device(5);
        let (other, _) = device(6);
        let body = br#"{"said":"Fix the flaky test"}"#;
        let signed =
            |key: &p256::ecdsa::SigningKey, method: &str, path: &str, body: &[u8], at: i64| {
                let signed_at = obi_protocol::time::format(at);
                let signature: p256::ecdsa::Signature =
                    key.sign(words_message(method, path, body, &signed_at).as_bytes());
                format!(
                    "d1 {signed_at} {}",
                    base64::engine::general_purpose::STANDARD.encode(signature.to_der())
                )
            };
        let known = |device: &str| (device == "d1").then(|| public.clone());
        let good = signed(&key, "POST", "/v1/plans", body, NOW);
        for (header, method, path, body) in [
            (good.as_str(), "POST", "/v1/jobs", &body[..]),
            (good.as_str(), "PUT", "/v1/plans", &body[..]),
            (
                good.as_str(),
                "POST",
                "/v1/plans",
                &br#"{"said":"Fix it all"}"#[..],
            ),
        ] {
            assert!(
                approvals
                    .check_words(header, method, path, body, known, NOW)
                    .unwrap_err()
                    .contains("signature")
            );
        }
        let by_other = signed(&other, "POST", "/v1/plans", body, NOW);
        assert!(
            approvals
                .check_words(&by_other, "POST", "/v1/plans", body, known, NOW)
                .is_err()
        );
        assert!(
            approvals
                .check_words(&good, "POST", "/v1/plans", body, |_| None, NOW)
                .unwrap_err()
                .contains("registered")
        );
        let stale = signed(&key, "POST", "/v1/plans", body, NOW - 600);
        assert!(
            approvals
                .check_words(&stale, "POST", "/v1/plans", body, known, NOW)
                .is_err()
        );
        assert!(
            approvals
                .check_words("d1 only-two", "POST", "/v1/plans", body, known, NOW)
                .is_err()
        );
        assert_eq!(
            approvals.check_words(&good, "POST", "/v1/plans", body, known, NOW),
            Ok(())
        );
        assert!(
            approvals
                .check_words(&good, "POST", "/v1/plans", body, known, NOW + 1)
                .unwrap_err()
                .contains("already used")
        );
        // Signed words are neither a publish nor a Confirm of anything.
        let mut parts = good.split(' ');
        let (device, signed_at, signature) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        let as_approval = Approval {
            device: device.into(),
            action: String::new(),
            signed_at: signed_at.into(),
            signature: signature.into(),
        };
        assert!(
            approvals
                .check(&as_approval, "j1", "", Some(&public), NOW)
                .is_err()
        );
        assert!(
            approvals
                .check_confirm(&as_approval, "i1", "", Some(&public), NOW)
                .is_err()
        );
    }

    #[test]
    fn a_publish_signature_and_a_confirm_signature_dont_pass_for_each_other() {
        let home = home("prefixes");
        let approvals = Approvals::open(&home);
        let (key, public) = device(9);
        let title = "Delete the job \u{201c}Fix the flaky test\u{201d}?";
        // Signed for publishing job "i1" with these words: not a Confirm of item "i1".
        let publish = approval(&key, "i1", title, NOW);
        assert!(
            approvals
                .check_confirm(&publish, "i1", title, Some(&public), NOW)
                .unwrap_err()
                .contains("signature")
        );
        // Signed as a Confirm of item "i1": not a publish of job "i1", nor a Confirm of "i2".
        let signed_at = obi_protocol::time::format(NOW);
        let signature: p256::ecdsa::Signature =
            key.sign(format!("oby-confirm\ni1\n{title}\n{signed_at}").as_bytes());
        let confirm = Approval {
            device: "d1".into(),
            action: title.into(),
            signed_at,
            signature: base64::engine::general_purpose::STANDARD.encode(signature.to_der()),
        };
        assert!(
            approvals
                .check(&confirm, "i1", title, Some(&public), NOW)
                .unwrap_err()
                .contains("signature")
        );
        assert!(
            approvals
                .check_confirm(&confirm, "i2", title, Some(&public), NOW)
                .unwrap_err()
                .contains("signature")
        );
        assert_eq!(
            approvals.check_confirm(&confirm, "i1", title, Some(&public), NOW),
            Ok(())
        );
        assert!(
            approvals
                .check_confirm(&confirm, "i1", title, Some(&public), NOW + 1)
                .unwrap_err()
                .contains("already used")
        );
    }

    #[test]
    fn pairing_binding_proof_is_for_one_key_pairing_and_nonce_and_cannot_replay() {
        let home = home("binding-proof");
        let approvals = Approvals::open(&home);
        let (key, public) = device(13);
        let signed_at = obi_protocol::time::format(NOW);
        let signature: p256::ecdsa::Signature =
            key.sign(format!("oby-device-pairing\nd1\np1\nn1\n{signed_at}").as_bytes());
        let proof = Approval {
            device: "d1".into(),
            action: "bind-pairing".into(),
            signed_at,
            signature: base64::engine::general_purpose::STANDARD.encode(signature.to_der()),
        };
        for (id, pairing, nonce) in [("d2", "p1", "n1"), ("d1", "p2", "n1"), ("d1", "p1", "n2")] {
            assert!(
                approvals
                    .check_pairing_binding(&proof, id, pairing, nonce, Some(&public), NOW)
                    .is_err()
            );
        }
        assert!(
            approvals
                .check_pairing_binding(&proof, "d1", "p1", "n1", None, NOW)
                .is_err()
        );
        assert!(
            approvals
                .check_pairing_binding(&proof, "d1", "p1", "n1", Some(&public), NOW)
                .is_ok()
        );
        assert!(
            Approvals::open(&home)
                .check_pairing_binding(&proof, "d1", "p1", "n1", Some(&public), NOW)
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn limit_approval_binds_revision_choice_and_account_and_cannot_be_replayed() {
        let home = home("limit-decision");
        let approvals = Approvals::open(&home);
        let (key, public) = device(11);
        let signed_at = obi_protocol::time::format(NOW);
        let signature: p256::ecdsa::Signature =
            key.sign(format!("oby-limit-decision\ne1\n2\nmove\na2\n{signed_at}").as_bytes());
        let approval = Approval {
            device: "d1".into(),
            action: "move".into(),
            signed_at,
            signature: base64::engine::general_purpose::STANDARD.encode(signature.to_der()),
        };
        for (id, revision, choice, account) in [
            ("e2", 2, "move", "a2"),
            ("e1", 3, "move", "a2"),
            ("e1", 2, "resume", ""),
            ("e1", 2, "move", "a3"),
        ] {
            assert!(
                approvals
                    .check_limit_decision(
                        &approval,
                        LimitChoice {
                            id,
                            revision,
                            choice,
                            account
                        },
                        Some(&public),
                        NOW
                    )
                    .is_err()
            );
        }
        assert!(
            approvals
                .check_limit_decision(
                    &approval,
                    LimitChoice {
                        id: "e1",
                        revision: 2,
                        choice: "move",
                        account: "a2"
                    },
                    None,
                    NOW
                )
                .is_err()
        );
        assert_eq!(
            approvals.check_limit_decision(
                &approval,
                LimitChoice {
                    id: "e1",
                    revision: 2,
                    choice: "move",
                    account: "a2"
                },
                Some(&public),
                NOW
            ),
            Ok(())
        );
        assert!(
            Approvals::open(&home)
                .check_limit_decision(
                    &approval,
                    LimitChoice {
                        id: "e1",
                        revision: 2,
                        choice: "move",
                        account: "a2"
                    },
                    Some(&public),
                    NOW
                )
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn once_fresh_signed_and_for_exactly_the_sentence() {
        let home = home("once");
        let approvals = Approvals::open(&home);
        let (key, public) = device(7);
        let (other, other_public) = device(8);
        let action = "Push oby/fix-1 to o/r and open a pull request into dev.";
        let good = approval(&key, "j1", action, NOW);
        // Unknown device, a signature by another key, another job, another sentence, stale.
        assert!(
            approvals
                .check(&good, "j1", action, None, NOW)
                .unwrap_err()
                .contains("isn't registered")
        );
        assert!(
            approvals
                .check(&good, "j1", action, Some(&other_public), NOW)
                .unwrap_err()
                .contains("signature")
        );
        assert!(
            approvals
                .check(&good, "j2", action, Some(&public), NOW)
                .unwrap_err()
                .contains("signature")
        );
        let elsewhere = approval(&key, "j1", "Push something else.", NOW);
        assert!(
            approvals
                .check(&elsewhere, "j1", action, Some(&public), NOW)
                .unwrap_err()
                .contains("something else")
        );
        let stale = approval(&key, "j1", action, NOW - 121);
        assert!(
            approvals
                .check(&stale, "j1", action, Some(&public), NOW)
                .unwrap_err()
                .contains("two minutes")
        );
        let _ = other;
        // Once, and not again after a restart.
        assert_eq!(
            approvals.check(&good, "j1", action, Some(&public), NOW),
            Ok(())
        );
        assert!(
            approvals
                .check(&good, "j1", action, Some(&public), NOW + 1)
                .unwrap_err()
                .contains("already used")
        );
        let reopened = Approvals::open(&home);
        assert!(
            reopened
                .check(&good, "j1", action, Some(&public), NOW + 1)
                .unwrap_err()
                .contains("already used")
        );
        // A file that doesn't read: nothing goes through, not even a new approval.
        std::fs::write(home.join("approvals.json"), "{broken").unwrap();
        let unread = Approvals::open(&home);
        let now = obi_protocol::time::now();
        let fresh = approval(&key, "j1", action, now);
        assert!(
            unread
                .check(&fresh, "j1", action, Some(&public), now)
                .unwrap_err()
                .contains("couldn't read")
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}
