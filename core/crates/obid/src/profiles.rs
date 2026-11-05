//! Chrome-style profiles on this Mac (docs/contracts/profiles.md): a local id and name in
//! `home/profiles.json`, never an identity. PROF-1 keeps the registry and `GET /v1/profiles`;
//! nothing else reads it yet, and apps show nothing.

use std::path::Path;
use std::sync::Mutex;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::Shared;
use crate::store::write_private;

/// The first profile's name on an install that has none yet: a label on this Mac, never who the
/// person is.
const FIRST_NAME: &str = "Personal";
/// The first profile's colour (Sangam's first avatar colour).
const FIRST_COLOR: &str = "#3a5a8c";

/// One profile as `profiles.json` keeps it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub color: String,
    pub default: bool,
}

impl Profile {
    /// What `GET /v1/profiles` shows for this one. `waiting` is always false until something else
    /// reads the registry and has work that waits on the owner in a profile.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "color": self.color,
            "default": self.default,
            "waiting": false,
        })
    }
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Saved {
    #[serde(default)]
    profiles: Vec<Profile>,
}

/// The profiles kept in `home/profiles.json` (0600).
pub struct Profiles {
    saved: Mutex<Saved>,
}

impl Profiles {
    /// The profiles in `home`. No file, or one with no profiles, is an upgrade: one first profile
    /// is written that holds everything already there (nothing moves, renames or signs out). A
    /// file that doesn't read is left as it is, logged, and answered as that same first profile
    /// without writing over it.
    pub fn open(home: &Path) -> Profiles {
        let file = home.join("profiles.json");
        let (saved, write) = match std::fs::read(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Saved::default(), true),
            Err(e) => {
                tracing::error!(
                    "can't read {}: answering with one first profile until it reads again ({e})",
                    file.display()
                );
                (
                    Saved {
                        profiles: vec![first_profile()],
                    },
                    false,
                )
            }
            Ok(bytes) => match serde_json::from_slice::<Saved>(&bytes) {
                Ok(saved) if saved.profiles.is_empty() => (Saved::default(), true),
                Ok(saved) => (saved, false),
                Err(e) => {
                    tracing::error!(
                        "can't read {}: answering with one first profile until it reads again ({e})",
                        file.display()
                    );
                    (
                        Saved {
                            profiles: vec![first_profile()],
                        },
                        false,
                    )
                }
            },
        };
        let mut saved = saved;
        if write {
            saved.profiles = vec![first_profile()];
            if let Err(e) = keep(&file, &saved) {
                tracing::error!("can't keep {}: {e}", file.display());
            }
        }
        Profiles {
            saved: Mutex::new(saved),
        }
    }

    /// Every profile, as `GET /v1/profiles` shows them.
    pub fn list(&self) -> Vec<Value> {
        self.saved
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .profiles
            .iter()
            .map(Profile::to_json)
            .collect()
    }
}

/// One first profile: local id and name, the default, Sangam's first colour.
fn first_profile() -> Profile {
    Profile {
        id: uuid::Uuid::new_v4().to_string(),
        name: FIRST_NAME.into(),
        color: FIRST_COLOR.into(),
        default: true,
    }
}

fn keep(file: &Path, saved: &Saved) -> std::io::Result<()> {
    let text = serde_json::to_vec_pretty(saved).map_err(std::io::Error::other)?;
    write_private(file, &text)
}

/// `GET /v1/profiles`.
pub async fn list_route(State(shared): State<std::sync::Arc<Shared>>) -> Response {
    (
        StatusCode::OK,
        axum::Json(Value::Array(shared.profiles.list())),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// PROFILES-1: an install with no `profiles.json` gets one first profile; nothing else in the
    /// home moves, renames or signs out.
    #[test]
    fn profiles_1_upgrade_keeps_everything_in_the_first_profile() {
        let home = std::env::temp_dir().join(format!(
            "obid-profiles-1-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("jobs")).unwrap();
        std::fs::create_dir_all(home.join("accounts")).unwrap();
        let accounts = br#"{"identities":{"stub":{"email":"a@b.dev"}}}"#;
        let job = br#"{"id":"job1","title":"Fix login"}"#;
        let conversation = br#"{"id":"c1","title":"Vault"}"#;
        let usage = br#"{"stub":{"fiveHour":0.1}}"#;
        let permissions = br#"{"mode":"bypass"}"#;
        std::fs::write(home.join("accounts.json"), accounts).unwrap();
        std::fs::write(home.join("jobs/job1.json"), job).unwrap();
        std::fs::write(home.join("conversations.json"), conversation).unwrap();
        std::fs::write(home.join("usage.json"), usage).unwrap();
        std::fs::write(home.join("permissions.json"), permissions).unwrap();
        let before = snapshot(&home);

        let profiles = Profiles::open(&home);
        let listed = profiles.list();
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0]["name"], FIRST_NAME);
        assert_eq!(listed[0]["color"], FIRST_COLOR);
        assert_eq!(listed[0]["default"], true);
        assert_eq!(listed[0]["waiting"], false);
        assert!(listed[0]["id"].as_str().is_some_and(|id| !id.is_empty()));

        let kept = std::fs::read(home.join("profiles.json")).unwrap();
        let saved: Saved = serde_json::from_slice(&kept).unwrap();
        assert_eq!(saved.profiles.len(), 1);
        assert_eq!(saved.profiles[0].name, FIRST_NAME);
        assert!(saved.profiles[0].default);
        assert_eq!(
            std::fs::metadata(home.join("profiles.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        let after = snapshot(&home);
        for (path, bytes) in &before {
            assert_eq!(
                after.get(path).map(Vec::as_slice),
                Some(bytes.as_slice()),
                "{} changed on upgrade",
                path.display()
            );
        }
        assert!(
            home.join("profiles.json").is_file(),
            "profiles.json was written"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A home that already has profiles keeps them; nothing is rewritten as a first profile.
    #[test]
    fn an_existing_registry_is_left_as_it_is() {
        let home = std::env::temp_dir().join(format!(
            "obid-profiles-kept-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let saved = Saved {
            profiles: vec![Profile {
                id: "p1".into(),
                name: "Work".into(),
                color: "#7a4a8c".into(),
                default: true,
            }],
        };
        keep(&home.join("profiles.json"), &saved).unwrap();
        let before = std::fs::read(home.join("profiles.json")).unwrap();
        let listed = Profiles::open(&home).list();
        assert_eq!(listed, vec![saved.profiles[0].to_json()]);
        assert_eq!(std::fs::read(home.join("profiles.json")).unwrap(), before);
        let _ = std::fs::remove_dir_all(&home);
    }

    fn snapshot(home: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        for name in [
            "accounts.json",
            "jobs/job1.json",
            "conversations.json",
            "usage.json",
            "permissions.json",
        ] {
            let path = home.join(name);
            out.insert(path.clone(), std::fs::read(&path).unwrap());
        }
        out
    }
}
