//! The devices that may approve publishing (docs/runner-api.md, `POST /v1/devices`), as
//! Runner/Devices.swift keeps them: each by its P-256 key, in `home/devices.json`. A key obid knows
//! gets its id back at once, so an app registers every time it connects. A new key needs the
//! owner's presence, in one of three ways: a registered key's signature vouching for it (a device
//! already paired, signing on its owner's tap), a paired phone's own first key in the minutes
//! after it redeemed an invitation the owner made on their own device (signed by a registered key
//! on the owner's click, crate::gateway), or `home/pairing` naming exactly that key, written in
//! the last 2 minutes and used up at once (the Mac app writes it on the owner's click; the owner,
//! on another runner's machine). The API token can't be the gate on its own: agents run on the
//! same machine, and a token can leak; an invitation it makes alone pairs a phone that can't add
//! a key. Agents can't write `pairing`: obid's home is hidden in their sandbox.

use std::collections::HashMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// How long `pairing` names a key after it was written.
const PAIRING_OPEN: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Device {
    id: String,
    name: String,
    /// P-256, X9.63 (65 bytes, uncompressed), base64.
    public_key: String,
    registered: String,
    /// The paired phone (crate::gateway) that registered this key with its own pairing: one key
    /// per pairing, and it goes when the pairing is revoked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paired: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    devices: Vec<Device>,
    /// SHA-256 of each signed approval message and its signed timestamp, shared with the Mac
    /// runner's `devices.json` so a phone approval can't be replayed after a restart.
    #[serde(default)]
    used: HashMap<String, String>,
}

/// Where a registration came from: obid's own API (the owner's apps on this machine), or a
/// paired phone through the gateway (crate::gateway), which may add its own first key only while
/// its pairing allows (`Gateway::may_add_key`).
#[derive(Debug, Clone, Copy)]
pub enum Origin<'a> {
    Api,
    Phone { pairing: &'a str, may_add_key: bool },
}

/// Why a device didn't register, or wasn't revoked.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// `422`: the key isn't a P-256 public key.
    Malformed(String),
    /// `403`: a new key without the owner's presence, or a revocation no device signed; the line
    /// to show.
    Unapproved(String),
    /// `404`: no device of that id.
    Unknown,
    /// `500`: obid couldn't keep it.
    Unkept(String),
}

pub struct Devices {
    file: PathBuf,
    pairing: PathBuf,
    saved: Mutex<Saved>,
    binding_challenges: Mutex<HashMap<String, (String, String, i64)>>,
}

impl Devices {
    /// The devices kept in `home/devices.json`. A file that doesn't read is left as it is and
    /// counts as none: no device can approve until one registers again.
    pub fn open(home: &Path) -> Devices {
        let file = home.join("devices.json");
        let saved = match std::fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                tracing::error!(
                    "can't read {}: no device can approve until one registers again",
                    file.display()
                );
                Saved::default()
            }),
            Err(_) => Saved::default(),
        };
        for device in &saved.devices {
            tracing::info!("device {}: {}", device.id, device.name);
        }
        Devices {
            file,
            pairing: home.join("pairing"),
            saved: Mutex::new(saved),
            binding_challenges: Mutex::new(HashMap::new()),
        }
    }

    /// The publishing key (X9.63, base64) of the registered device `id`, or none for a device obid
    /// doesn't know: what checks its approval to publish by hand (crate::approval).
    pub fn public_key(&self, id: &str) -> Option<String> {
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        saved
            .devices
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.public_key.clone())
    }

    /// Whether the registered signing key belongs to this gateway pairing. The signing-device
    /// identifier and the gateway pairing identifier are distinct, independent UUIDs.
    pub fn belongs_to_pairing(&self, id: &str, pairing: &str) -> bool {
        self.saved
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .devices
            .iter()
            .any(|d| d.id == id && d.paired.as_deref() == Some(pairing))
    }

    /// An older endorsed phone key has no pairing recorded. A fresh challenge lets its holder
    /// prove possession of that already trusted private key without changing registration.
    pub fn pairing_challenge(&self, id: &str, pairing: &str, now: i64) -> Result<String, String> {
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let device = saved
            .devices
            .iter()
            .find(|d| d.id == id)
            .ok_or("Unknown signing key.")?;
        if device.paired.is_some()
            || saved
                .devices
                .iter()
                .any(|d| d.paired.as_deref() == Some(pairing))
        {
            return Err("This key or pairing is already linked to a device.".into());
        }
        let nonce = uuid::Uuid::new_v4().to_string();
        let mut challenges = self
            .binding_challenges
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        challenges.retain(|_, (_, _, at)| now.saturating_sub(*at) <= 120);
        challenges.insert(id.to_owned(), (pairing.to_owned(), nonce.clone(), now));
        Ok(nonce)
    }

    /// Called only after the registered key signed the exact challenge for the authenticated
    /// pairing. No public-key-only request can bind or replace a pairing's key.
    pub fn bind_pairing(
        &self,
        id: &str,
        pairing: &str,
        nonce: &str,
        now: i64,
    ) -> Result<(), String> {
        let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let mut challenges = self
            .binding_challenges
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let valid = challenges.get(id).is_some_and(|(p, n, at)| {
            p == pairing && n == nonce && now >= *at && now.saturating_sub(*at) <= 120
        });
        if !valid {
            return Err("The device-link challenge changed or expired. Try again.".into());
        }
        if saved
            .devices
            .iter()
            .any(|d| d.paired.as_deref() == Some(pairing))
        {
            return Err("This pairing already has an approval key.".into());
        }
        let device = saved
            .devices
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or("Unknown signing key.")?;
        if device.paired.is_some() {
            return Err("This approval key is already linked to another pairing.".into());
        }
        device.paired = Some(pairing.to_owned());
        if let Err(error) = self.keep(&saved) {
            if let Some(device) = saved.devices.iter_mut().find(|d| d.id == id) {
                device.paired = None;
            }
            return Err(format!("Could not keep this device link: {error}"));
        }
        challenges.remove(id);
        Ok(())
    }

    /// Registers a device's publishing key and answers its id: the one it has for a key it knows;
    /// a new one only with the owner's presence. `endorsement` is a registered device's signature
    /// over `"oby-device\n<publicKey>\n<signedAt>"`; a paired phone (`origin`) may register one key
    /// of its own while its pairing allows; otherwise, on obid's own API, `home/pairing` must name
    /// this key, and is used up.
    pub fn register(
        &self,
        name: &str,
        public_key: &str,
        endorsement: Option<&serde_json::Value>,
        origin: Origin<'_>,
        now: SystemTime,
    ) -> Result<String, Refused> {
        if !is_p256_public_key(public_key) {
            return Err(Refused::Malformed(
                "publicKey isn't a P-256 public key (X9.63, base64).".into(),
            ));
        }
        if let Some(id) = self.known(public_key) {
            return Ok(id);
        }
        let bound = match (endorsement, origin) {
            (Some(endorsement), _) => {
                if self
                    .signed(endorsement, |at| format!("oby-device\n{public_key}\n{at}"))
                    .is_none()
                {
                    return Err(Refused::Unapproved(
                        "That endorsement isn't a registered device's, signed for this key in the last two minutes. Approve it again.".into(),
                    ));
                }
                None
            }
            (
                None,
                Origin::Phone {
                    pairing,
                    may_add_key: true,
                },
            ) => Some(pairing),
            (None, Origin::Phone { .. }) => {
                return Err(Refused::Unapproved(
                    "To add this iPhone's publishing key, pair it again from Oby on your Mac, then open Oby on the iPhone within 10 minutes.".into(),
                ));
            }
            (None, Origin::Api) => {
                if !self.names(public_key, now) {
                    return Err(Refused::Unapproved(format!(
                        "To add this device, run `echo '{public_key}' > {}` on obid's machine, then try again within 2 minutes.",
                        self.pairing.display()
                    )));
                }
                None
            }
        };
        let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(known) = saved.devices.iter().find(|d| d.public_key == public_key) {
            return Ok(known.id.clone());
        }
        if let Some(paired) = bound
            && saved
                .devices
                .iter()
                .any(|d| d.paired.as_deref() == Some(paired))
        {
            return Err(Refused::Unapproved(
                "This iPhone registered its publishing key already. To register another, pair it again from your Mac.".into(),
            ));
        }
        let seconds = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let device = Device {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.chars().filter(|c| !c.is_control()).take(100).collect(),
            public_key: public_key.to_owned(),
            registered: obi_protocol::time::format(seconds),
            paired: bound.map(str::to_owned),
        };
        saved.devices.push(device.clone());
        if let Err(e) = self.keep(&saved) {
            saved.devices.pop();
            return Err(Refused::Unkept(e.to_string()));
        }
        tracing::info!("registered device {}: {}", device.id, device.name);
        Ok(device.id)
    }

    /// Whether `approval` is a registered device's signature over `"oby-pair-invite\n<signedAt>"`,
    /// in the last two minutes, once: the owner made a pairing invitation on their own device.
    pub fn approves_invite(&self, approval: &serde_json::Value) -> bool {
        self.signed(approval, |at| format!("oby-pair-invite\n{at}"))
            .is_some()
    }

    /// Forgets the device `id`, with a registered device's signature (its own, or another's) over
    /// `"oby-revoke-device\n<id>\n<signedAt>"`, signed in the last two minutes, once.
    pub fn revoke(&self, id: &str, approval: &serde_json::Value) -> Result<(), Refused> {
        if self.public_key(id).is_none() {
            return Err(Refused::Unknown);
        }
        if self
            .signed(approval, |at| format!("oby-revoke-device\n{id}\n{at}"))
            .is_none()
        {
            return Err(Refused::Unapproved(
                "That revocation isn't a registered device's, signed for this device in the last two minutes. Approve it again.".into(),
            ));
        }
        self.forget(|d| d.id == id)
            .map_err(|e| Refused::Unkept(e.to_string()))
    }

    /// Forgets the key the paired phone `paired` registered with its pairing: the pairing was
    /// revoked (crate::gateway).
    pub fn forget_paired(&self, paired: &str) -> Result<(), std::io::Error> {
        self.forget(|d| d.paired.as_deref() == Some(paired))
    }

    fn forget(&self, which: impl Fn(&Device) -> bool) -> Result<(), std::io::Error> {
        let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let (gone, stay): (Vec<Device>, Vec<Device>) =
            saved.devices.iter().cloned().partition(which);
        if gone.is_empty() {
            return Ok(());
        }
        let kept = std::mem::replace(&mut saved.devices, stay);
        if let Err(e) = self.keep(&saved) {
            saved.devices = kept;
            return Err(e);
        }
        for device in gone {
            tracing::info!("revoked device {}: {}", device.id, device.name);
        }
        Ok(())
    }

    fn known(&self, public_key: &str) -> Option<String> {
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        saved
            .devices
            .iter()
            .find(|d| d.public_key == public_key)
            .map(|d| d.id.clone())
    }

    /// Whether `home/pairing` names `public_key`, written in the last two minutes: then it's used
    /// up. A file that names another key stays for it; one older than two minutes goes.
    fn names(&self, public_key: &str, now: SystemTime) -> bool {
        let Ok(meta) = std::fs::symlink_metadata(&self.pairing) else {
            return false;
        };
        let fresh = meta.is_file()
            && meta
                .modified()
                .is_ok_and(|written| match now.duration_since(written) {
                    Ok(age) => age < PAIRING_OPEN,
                    Err(ahead) => ahead.duration() < PAIRING_OPEN,
                });
        if !fresh {
            let _ = std::fs::remove_file(&self.pairing);
            return false;
        }
        let mut named = String::new();
        let read = std::fs::File::open(&self.pairing)
            .and_then(|f| f.take(4096).read_to_string(&mut named));
        if read.is_err() || named.trim() != public_key {
            return false;
        }
        let _ = std::fs::remove_file(&self.pairing);
        true
    }

    fn keep(&self, saved: &Saved) -> Result<(), std::io::Error> {
        let bytes = serde_json::to_vec_pretty(saved).map_err(std::io::Error::other)?;
        crate::store::write_private(&self.file, &bytes)
    }

    /// Checks the phone's signed approval sentence for a sensitive connection change. A
    /// signature is tied to the exact action and timestamp, expires after two minutes, and is
    /// accepted once and kept in `devices.json`, across restarts.
    pub fn approves_connection(&self, value: &serde_json::Value, action: &str) -> bool {
        if value.get("action").and_then(serde_json::Value::as_str) != Some(action) {
            return false;
        }
        self.signed(value, |at| format!("oby-connection\n{action}\n{at}"))
            .is_some()
    }

    /// The registered device whose key signed `message(signedAt)` in `value` (`device`,
    /// `signedAt`, `signature`: ECDSA P-256 with SHA-256, DER, base64), within two minutes of
    /// now, never before: it's used up, and kept used in `devices.json`, across restarts.
    fn signed(
        &self,
        value: &serde_json::Value,
        message: impl FnOnce(&str) -> String,
    ) -> Option<String> {
        use p256::ecdsa::signature::Verifier as _;
        let device = value.get("device")?.as_str()?;
        let signed_at = value.get("signedAt")?.as_str()?;
        let signature = value.get("signature")?.as_str()?;
        let seconds = obi_protocol::time::parse(signed_at)?;
        if (obi_protocol::time::now() - seconds).abs() > 120 {
            return None;
        }
        let signature_bytes = base64::engine::general_purpose::STANDARD
            .decode(signature)
            .ok()?;
        let signature = p256::ecdsa::Signature::from_der(&signature_bytes).ok()?;
        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(self.public_key(device)?)
            .ok()?;
        let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(&key_bytes).ok()?;
        let signed_message = message(signed_at);
        key.verify(signed_message.as_bytes(), &signature).ok()?;
        let mut used = device.as_bytes().to_vec();
        used.extend_from_slice(signed_message.as_bytes());
        let hash = hex::encode(obi_protocol::keys::sha256(used));
        let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let now = obi_protocol::time::now();
        saved
            .used
            .retain(|_, at| obi_protocol::time::parse(at).is_some_and(|at| now - at < 600));
        if saved.used.contains_key(&hash) {
            return None;
        }
        saved.used.insert(hash.clone(), signed_at.to_owned());
        if self.keep(&saved).is_err() {
            saved.used.remove(&hash);
            return None;
        }
        Some(device.to_owned())
    }
}

/// A P-256 public key as the apps send it: X9.63 uncompressed (`04`, then X and Y, 65 bytes),
/// base64, and a point on the curve.
pub fn is_p256_public_key(text: &str) -> bool {
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(text) else {
        return false;
    };
    bytes.len() == 65 && bytes[0] == 4 && p256::PublicKey::from_sec1_bytes(&bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::elliptic_curve::sec1::ToEncodedPoint;

    /// A device's key, from a fixed private scalar: no randomness, no key in the source.
    pub(crate) fn key(scalar: u8) -> String {
        let secret = p256::SecretKey::from_slice(&[scalar; 32]).unwrap();
        let point = secret.public_key().to_encoded_point(false);
        base64::engine::general_purpose::STANDARD.encode(point.as_bytes())
    }

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("obid-devices-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn older_unbound_key_requires_exact_fresh_pairing_challenge_and_survives_restart() {
        let home = folder("bind-old-key");
        std::fs::write(home.join("pairing"), key(12)).unwrap();
        let devices = Devices::open(&home);
        let id = devices
            .register("Phone", &key(12), None, Origin::Api, SystemTime::now())
            .unwrap();
        let nonce = devices.pairing_challenge(&id, "phone-pair", 1000).unwrap();
        assert!(
            devices
                .bind_pairing(&id, "another-pair", &nonce, 1001)
                .is_err()
        );
        assert!(
            devices
                .bind_pairing(&id, "phone-pair", "wrong-nonce", 1001)
                .is_err()
        );
        assert!(
            devices
                .bind_pairing(&id, "phone-pair", &nonce, 1121)
                .is_err()
        );
        assert!(
            devices
                .bind_pairing(&id, "phone-pair", &nonce, 1001)
                .is_ok()
        );
        assert!(devices.belongs_to_pairing(&id, "phone-pair"));
        assert!(
            devices
                .bind_pairing(&id, "phone-pair", &nonce, 1002)
                .is_err()
        );
        assert!(
            devices
                .pairing_challenge(&id, "another-pair", 1002)
                .is_err()
        );
        let again = Devices::open(&home);
        assert!(again.belongs_to_pairing(&id, "phone-pair"));
        again.forget_paired("phone-pair").unwrap();
        assert!(again.public_key(&id).is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn keys() {
        assert!(is_p256_public_key(&key(1)));
        let raw = base64::engine::general_purpose::STANDARD
            .decode(key(1))
            .unwrap();
        for bad in [
            "BJ3x".to_owned(),
            "not base64!".to_owned(),
            base64::engine::general_purpose::STANDARD.encode([4u8; 65]),
            // The raw representation (X and Y, no prefix), as RunnerTests sends it.
            base64::engine::general_purpose::STANDARD.encode(&raw[1..]),
        ] {
            assert!(!is_p256_public_key(&bad), "{bad}");
        }
    }

    #[test]
    fn a_registered_key_survives_a_restart() {
        let home = folder("restart");
        std::fs::write(home.join("pairing"), key(3)).unwrap();
        let id = Devices::open(&home)
            .register("Mac", &key(3), None, Origin::Api, SystemTime::now())
            .unwrap();
        let again = Devices::open(&home);
        assert_eq!(
            again.register("Mac", &key(3), None, Origin::Api, SystemTime::now()),
            Ok(id)
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A new key needs the owner's presence: `pairing` naming it, fresh, used up at once; a
    /// registered key vouching for it, once; or a paired phone's own first key. Nothing else, an
    /// open pairing for another key included, lets one in.
    #[test]
    fn a_new_key_needs_the_owners_presence() {
        let home = folder("presence");
        let devices = Devices::open(&home);
        let now = SystemTime::now();
        let pairing = home.join("pairing");
        let refused = |key: &str, endorsement: Option<&serde_json::Value>| {
            matches!(
                devices.register("Mac", key, endorsement, Origin::Api, now),
                Err(Refused::Unapproved(_))
            )
        };

        assert!(refused(&key(3), None), "nothing");
        std::fs::write(&pairing, "").unwrap();
        assert!(
            refused(&key(3), None),
            "pairing touched, as it opened before"
        );
        std::fs::write(&pairing, key(4)).unwrap();
        assert!(refused(&key(3), None), "pairing naming another key");
        assert!(pairing.exists(), "which it keeps for that key");
        std::fs::write(&pairing, format!("{}\n", key(3))).unwrap();
        let file = std::fs::File::options().write(true).open(&pairing).unwrap();
        file.set_modified(now - Duration::from_secs(121)).unwrap();
        drop(file);
        assert!(refused(&key(3), None), "pairing written 121 seconds ago");
        assert!(!pairing.exists(), "closed: it never stays open idle");
        std::fs::write(&pairing, format!("{}\n", key(3))).unwrap();
        let mac = devices
            .register("Mac", &key(3), None, Origin::Api, now)
            .unwrap();
        assert!(!pairing.exists(), "used up");
        assert!(refused(&key(5), None), "pairing closed again");

        // The Mac's key vouches for its next one, once, for that key only.
        let at = obi_protocol::time::format(obi_protocol::time::now());
        let vouch = |scalar: u8, device: &str, new: &str| {
            signed(scalar, device, "", &at, &format!("oby-device\n{new}\n{at}"))
        };
        assert!(
            refused(&key(5), Some(&vouch(5, &mac, &key(5)))),
            "a key vouching for itself"
        );
        assert!(
            refused(&key(5), Some(&vouch(3, "nobody", &key(5)))),
            "an unknown device"
        );
        assert!(
            refused(&key(5), Some(&vouch(3, &mac, &key(6)))),
            "vouching for another key"
        );
        let endorsement = vouch(3, &mac, &key(5));
        let next = devices
            .register("Mac", &key(5), Some(&endorsement), Origin::Api, now)
            .unwrap();
        assert_ne!(next, mac);
        assert!(refused(&key(6), Some(&endorsement)), "used once");

        // A pairing invitation the owner made on their own device: a registered key's signature,
        // once; a stranger's, or another sentence's, isn't one.
        let invite = |scalar: u8, device: &str, sentence: &str| {
            signed(scalar, device, "", &at, &format!("{sentence}\n{at}"))
        };
        assert!(
            !devices.approves_invite(&invite(9, &mac, "oby-pair-invite")),
            "not the Mac's key"
        );
        assert!(
            !devices.approves_invite(&invite(3, &mac, "oby-device")),
            "another sentence"
        );
        let made = invite(3, &mac, "oby-pair-invite");
        assert!(devices.approves_invite(&made));
        assert!(!devices.approves_invite(&made), "used once");

        // A paired phone registers one key of its own while its pairing may add one (an
        // invitation the owner made, redeemed in the last 10 minutes); revoking its pairing
        // forgets it. A pairing the API token alone made, or an older one, adds none.
        let phone_may = Origin::Phone {
            pairing: "phone-1",
            may_add_key: true,
        };
        let token_only = Origin::Phone {
            pairing: "phone-2",
            may_add_key: false,
        };
        assert!(matches!(
            devices.register("iPhone", &key(9), None, token_only, now),
            Err(Refused::Unapproved(_))
        ));
        std::fs::write(&pairing, key(9)).unwrap();
        assert!(
            matches!(
                devices.register("iPhone", &key(9), None, token_only, now),
                Err(Refused::Unapproved(_))
            ),
            "a phone's request never uses the pairing file"
        );
        std::fs::remove_file(&pairing).unwrap();
        let phone = devices
            .register("iPhone", &key(7), None, phone_may, now)
            .unwrap();
        assert!(matches!(
            devices.register("iPhone", &key(8), None, phone_may, now),
            Err(Refused::Unapproved(_))
        ));
        assert_eq!(
            devices.register("iPhone", &key(7), None, token_only, now),
            Ok(phone.clone()),
            "its own key, again, once it's registered"
        );
        devices.forget_paired("phone-1").unwrap();
        assert_eq!(devices.public_key(&phone), None);
        assert!(Devices::open(&home).public_key(&phone).is_none());

        // Revoking takes a registered device's signature for that device.
        let revoke = |scalar: u8, device: &str, id: &str| {
            signed(
                scalar,
                device,
                "",
                &at,
                &format!("oby-revoke-device\n{id}\n{at}"),
            )
        };
        assert_eq!(
            devices.revoke("nobody", &revoke(3, &mac, "nobody")),
            Err(Refused::Unknown)
        );
        assert!(matches!(
            devices.revoke(&mac, &serde_json::json!({})),
            Err(Refused::Unapproved(_))
        ));
        assert!(
            matches!(
                devices.revoke(&mac, &revoke(9, &mac, &mac)),
                Err(Refused::Unapproved(_))
            ),
            "another key"
        );
        assert!(
            matches!(
                devices.revoke(&mac, &revoke(3, &mac, &next)),
                Err(Refused::Unapproved(_))
            ),
            "signed for another device"
        );
        assert_eq!(devices.revoke(&mac, &revoke(5, &next, &mac)), Ok(()));
        assert_eq!(Devices::open(&home).public_key(&mac), None);
        assert!(Devices::open(&home).public_key(&next).is_some());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// What a phone sends to approve a connection change: `message` signed with `scalar`'s key.
    fn signed(
        scalar: u8,
        device: &str,
        action: &str,
        signed_at: &str,
        message: &str,
    ) -> serde_json::Value {
        use p256::ecdsa::signature::Signer as _;
        let key = p256::ecdsa::SigningKey::from_slice(&[scalar; 32]).unwrap();
        let signature: p256::ecdsa::Signature = key.sign(message.as_bytes());
        serde_json::json!({"device": device, "action": action, "signedAt": signed_at,
            "signature": base64::engine::general_purpose::STANDARD.encode(signature.to_der().as_bytes())})
    }

    /// A connection change goes through only with a registered device's signature over obid's
    /// own sentence, `"oby-connection\n<action>\n<signedAt>"`, signed within two minutes, once,
    /// even across a restart (ConnectionTests.agentsUseConnectedAppsWithoutEverSeeingTheLogin's
    /// approvals, and what it can't reach: the clock and the other kinds of signed sentence).
    #[test]
    fn a_connection_change_needs_one_fresh_signature_from_a_registered_device() {
        let home = folder("connection-approval");
        std::fs::write(
            home.join("devices.json"),
            serde_json::json!({"devices": [{"id": "phone", "name": "Phone", "publicKey": key(9),
                "registered": "2026-10-16T00:00:00Z"}]})
            .to_string(),
        )
        .unwrap();
        let devices = Devices::open(&home);
        let action = "Connect mail: agents' read-only calls to the MCP server https://mail.example.com/mcp carry the vault secret mail-token as Authorization: Bearer.";
        let at = |offset: i64| obi_protocol::time::format(obi_protocol::time::now() + offset);
        let now = at(0);
        let good = |signed_at: &str| {
            signed(
                9,
                "phone",
                action,
                signed_at,
                &format!("oby-connection\n{action}\n{signed_at}"),
            )
        };

        // What is refused, none of it using an approval up.
        let refused = [
            ("no device", {
                let mut a = good(&now);
                a.as_object_mut().unwrap().remove("device");
                a
            }),
            (
                "an unknown device",
                signed(
                    9,
                    "laptop",
                    action,
                    &now,
                    &format!("oby-connection\n{action}\n{now}"),
                ),
            ),
            (
                "another key",
                signed(
                    8,
                    "phone",
                    action,
                    &now,
                    &format!("oby-connection\n{action}\n{now}"),
                ),
            ),
            (
                "another action, signed",
                signed(
                    9,
                    "phone",
                    "Disconnect mail.",
                    &now,
                    &format!("oby-connection\nDisconnect mail.\n{now}"),
                ),
            ),
            (
                "another action, claimed",
                signed(
                    9,
                    "phone",
                    "Disconnect mail.",
                    &now,
                    &format!("oby-connection\n{action}\n{now}"),
                ),
            ),
            (
                "another time than signed",
                signed(
                    9,
                    "phone",
                    action,
                    &at(1),
                    &format!("oby-connection\n{action}\n{now}"),
                ),
            ),
            (
                "a publish approval",
                signed(
                    9,
                    "phone",
                    action,
                    &now,
                    &format!("oby-publish\njob1\n{action}\n{now}"),
                ),
            ),
            ("the bare action", signed(9, "phone", action, &now, action)),
            ("three minutes ago", good(&at(-180))),
            ("three minutes ahead", good(&at(180))),
            ("no time", good("yesterday")),
            ("no signature", {
                let mut a = good(&now);
                a["signature"] = serde_json::json!("not base64!");
                a
            }),
            ("a signature that isn't DER", {
                let mut a = good(&now);
                a["signature"] = serde_json::json!("AAAA");
                a
            }),
        ];
        for (why, approval) in &refused {
            assert!(!devices.approves_connection(approval, action), "{why}");
        }

        let approval = good(&now);
        assert!(devices.approves_connection(&approval, action));
        assert!(!devices.approves_connection(&approval, action), "used once");
        assert!(
            !Devices::open(&home).approves_connection(&approval, action),
            "used once, after a restart"
        );
        // Another one, signed a second later, goes through: each is its own.
        assert!(Devices::open(&home).approves_connection(&good(&at(1)), action));
        let _ = std::fs::remove_dir_all(&home);
    }
}
