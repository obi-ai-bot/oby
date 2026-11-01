//! The Obi's key and its place in one Sphere (docs/runner-api.md, "obid only: Sphere membership"),
//! as obid keeps them in its home:
//!
//! - `keys/obi.key` (0600): the Obi key's 32-byte seed, hex. It never leaves obid: no answer, event,
//!   log or agent ever sees it, and the job sandbox hides `home` from agents.
//! - `keys/seal.key` (0600): the seal key's 32 private bytes, hex, kept the same way: the X25519 key
//!   other Obis seal this Obi's words for (spec protocol.md section 9.1), its public half in the card.
//! - `sphere/membership.json` (0600): the Sphere, the Obi's id and its owner-signed card.
//! - `sphere/delegation.jwt` (0600): the owner's delegation, never shown again once kept.
//! - `owner` (0600): the owner id obid joined a Sphere as. It outlives a leave, so obid joins again
//!   as the same owner: a Sphere binds an owner to their first key and Google account (#1214).

use std::path::{Path, PathBuf};

use obi_protocol::keys::Keypair;
use obi_protocol::seal::SealKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store;

/// A Sphere joined.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Membership {
    /// `joined` or `revoked`.
    pub state: String,
    pub url: String,
    pub sphere: String,
    pub obi_id: String,
    pub card: Value,
    pub joined_at: i64,
}

impl Membership {
    /// The request types the card accepts.
    pub fn accepts(&self) -> Vec<String> {
        obi_protocol::card::params(&self.card)
            .and_then(|p| p.get("accepts"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The card's `card_seq`.
    pub fn card_seq(&self) -> u64 {
        obi_protocol::card::params(&self.card)
            .and_then(|p| p.get("card_seq"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    }
}

/// Where the key and the membership are kept.
pub struct Files {
    key: PathBuf,
    seal: PathBuf,
    membership: PathBuf,
    delegation: PathBuf,
    owner: PathBuf,
}

impl Files {
    pub fn new(home: &Path) -> Files {
        Files {
            key: home.join("keys").join("obi.key"),
            seal: home.join("keys").join("seal.key"),
            membership: home.join("sphere").join("membership.json"),
            delegation: home.join("sphere").join("delegation.jwt"),
            owner: home.join("owner"),
        }
    }

    /// The Obi key, when it has been made.
    pub fn load_key(&self) -> std::io::Result<Option<Keypair>> {
        Ok(read_32(&self.key)?.map(|seed| Keypair::from_seed(&seed)))
    }

    /// Makes the Obi key and keeps it (0600, in a 0700 folder).
    pub fn make_key(&self) -> std::io::Result<Keypair> {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|e| std::io::Error::other(e.to_string()))?;
        write_32(&self.key, &seed)?;
        Ok(Keypair::from_seed(&seed))
    }

    /// The seal key, when it has been made.
    pub fn load_seal_key(&self) -> std::io::Result<Option<SealKey>> {
        read_32(&self.seal)?
            .map(|private| SealKey::from_private(private).map_err(std::io::Error::other))
            .transpose()
    }

    /// Makes the seal key and keeps it beside the Obi key.
    pub fn make_seal_key(&self) -> std::io::Result<SealKey> {
        let key = SealKey::generate().map_err(std::io::Error::other)?;
        write_32(&self.seal, key.private_bytes())?;
        Ok(key)
    }

    pub fn load_membership(&self) -> Option<Membership> {
        let bytes = std::fs::read(&self.membership).ok()?;
        serde_json::from_slice(&bytes)
            .map_err(|e| tracing::error!("{} doesn't read: {e}", self.membership.display()))
            .ok()
    }

    pub fn save_membership(&self, membership: &Membership) -> std::io::Result<()> {
        store::make_private_dir(self.membership.parent().expect("a folder"))?;
        let text = serde_json::to_vec_pretty(membership).map_err(std::io::Error::other)?;
        store::write_private(&self.membership, &text)
    }

    /// The owner id obid joined a Sphere as, if it ever joined one.
    pub fn load_owner(&self) -> Option<String> {
        std::fs::read_to_string(&self.owner)
            .ok()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
    }

    pub fn save_owner(&self, owner: &str) -> std::io::Result<()> {
        store::write_private(&self.owner, owner.as_bytes())
    }

    /// Removes the Obi key and the Sphere's folder with everything in it (obid left its Sphere).
    /// The owner it joined as stays.
    pub fn forget(&self) -> std::io::Result<()> {
        let gone = |r: std::io::Result<()>| match r {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
        gone(std::fs::remove_dir_all(
            self.membership.parent().expect("a folder"),
        ))?;
        gone(std::fs::remove_file(&self.seal))?;
        gone(std::fs::remove_file(&self.key))
    }

    pub fn load_delegation(&self) -> Option<String> {
        std::fs::read_to_string(&self.delegation)
            .ok()
            .map(|t| t.trim().to_owned())
    }

    pub fn save_delegation(&self, delegation: &str) -> std::io::Result<()> {
        store::make_private_dir(self.delegation.parent().expect("a folder"))?;
        store::write_private(&self.delegation, delegation.as_bytes())
    }
}

/// 32 bytes kept as hex, when the file is there.
fn read_32(path: &Path) -> std::io::Result<Option<[u8; 32]>> {
    match std::fs::read_to_string(path) {
        Ok(text) => hex::decode(text.trim())
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .map(Some)
            .ok_or_else(|| std::io::Error::other(format!("{} isn't a key", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Keeps 32 bytes as hex (0600, in a 0700 folder).
fn write_32(path: &Path, bytes: &[u8; 32]) -> std::io::Result<()> {
    store::make_private_dir(path.parent().expect("a key has a folder"))?;
    store::write_private(path, hex::encode(bytes).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_made_once_and_kept_private() {
        let home = std::env::temp_dir().join(format!("obid-key-test-{}", std::process::id()));
        let files = Files::new(&home);
        assert!(files.load_key().unwrap().is_none());
        let key = files.make_key().unwrap();
        let again = files.load_key().unwrap().unwrap();
        assert_eq!(key.key_id(), again.key_id());
        assert!(files.load_seal_key().unwrap().is_none());
        let seal = files.make_seal_key().unwrap();
        assert_eq!(
            files.load_seal_key().unwrap().unwrap().key_id(),
            seal.key_id()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for file in ["keys/obi.key", "keys/seal.key"] {
                let mode = std::fs::metadata(home.join(file))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600, "{file}");
            }
            let mode = std::fs::metadata(home.join("keys"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
