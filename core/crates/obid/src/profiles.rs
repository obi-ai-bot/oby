//! Chrome-style profiles on this Mac (docs/contracts/profiles.md): a local id and name in
//! `home/profiles.json`, never an identity. PROF-1 keeps the registry and `GET /v1/profiles`;
//! PROF-3a creates and renames profiles, each with its own folder that nothing reads yet, and
//! apps show nothing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::{ApiError, Shared};
use crate::store::{make_private_dir, write_private};

/// The first profile's name on an install that has none yet: a label on this Mac, never who the
/// person is. An install already upgraded keeps the name it has.
const FIRST_NAME: &str = "Default";
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
    file: PathBuf,
    /// `profiles.json` didn't read at `open`: nothing writes over it until obid starts again.
    unreadable: bool,
    saved: Mutex<Saved>,
}

/// Why the registry wasn't changed.
#[derive(Debug)]
pub enum Refused {
    Unreadable,
    NoSuchProfile,
    Unwritten(std::io::Error),
}

impl Profiles {
    /// The profiles in `home`. No file, or one with no profiles, is an upgrade: one first profile
    /// is written that holds everything already there (nothing moves, renames or signs out). A
    /// file that doesn't read is left as it is, logged, and answered as that same first profile
    /// without writing over it.
    pub fn open(home: &Path) -> Profiles {
        let file = home.join("profiles.json");
        let read = match std::fs::read(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Saved::default()),
            Err(e) => Err(e.to_string()),
            Ok(bytes) => serde_json::from_slice::<Saved>(&bytes).map_err(|e| e.to_string()),
        };
        let unreadable = read.is_err();
        let mut saved = read.unwrap_or_else(|e| {
            tracing::error!(
                "can't read {}: answering with one first profile until it reads again ({e})",
                file.display()
            );
            Saved {
                profiles: vec![first_profile()],
            }
        });
        if saved.profiles.is_empty() {
            saved.profiles = vec![first_profile()];
            if let Err(e) = keep(&file, &saved) {
                tracing::error!("can't keep {}: {e}", file.display());
            }
        }
        Profiles {
            file,
            unreadable,
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

    /// A new profile, not the default, with its own empty folder under `home` (0700), kept.
    pub fn create(&self, home: &Path, name: String, color: String) -> Result<Profile, Refused> {
        let profile = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            color,
            default: false,
        };
        self.update(|saved| {
            let root = storage_root(home, &profile)
                .ok_or_else(|| Refused::Unwritten(std::io::Error::other("no storage root")))?;
            make_private_dir(&root).map_err(Refused::Unwritten)?;
            saved.profiles.push(profile.clone());
            Ok(profile)
        })
    }

    /// Profile `id` with only the given name or colour changed, kept.
    pub fn change(
        &self,
        id: &str,
        name: Option<String>,
        color: Option<String>,
    ) -> Result<Profile, Refused> {
        self.update(|saved| {
            let profile = (saved.profiles.iter_mut())
                .find(|p| p.id == id)
                .ok_or(Refused::NoSuchProfile)?;
            profile.name = name.unwrap_or_else(|| profile.name.clone());
            profile.color = color.unwrap_or_else(|| profile.color.clone());
            Ok(profile.clone())
        })
    }

    /// `edit` on a copy of the registry, kept, under the lock: refused before anything is written
    /// while `profiles.json` didn't read, and the registry is left as it was when `edit` or the
    /// write fails.
    fn update(
        &self,
        edit: impl FnOnce(&mut Saved) -> Result<Profile, Refused>,
    ) -> Result<Profile, Refused> {
        let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        if self.unreadable {
            return Err(Refused::Unreadable);
        }
        let mut next = Saved {
            profiles: saved.profiles.clone(),
        };
        let profile = edit(&mut next)?;
        keep(&self.file, &next).map_err(Refused::Unwritten)?;
        *saved = next;
        Ok(profile)
    }
}

/// Where profile `profile` keeps its own things: the default profile's is `home` itself (nothing
/// an install has moves), any other's `home/profiles/<id>` for a lower-case UUID id, and none for
/// anything else.
pub(crate) fn storage_root(home: &Path, profile: &Profile) -> Option<PathBuf> {
    if profile.default {
        return Some(home.to_path_buf());
    }
    let id: [u8; 36] = profile.id.as_bytes().try_into().ok()?;
    is_profile_id(id).then(|| home.join("profiles").join(&profile.id))
}

/// Whether `id` is a lower-case UUID: hex digits with `-` at 8, 13, 18 and 23, so never a path
/// separator, a dot, a NUL or an upper-case letter (PROFILES-17).
pub(crate) fn is_profile_id(id: [u8; 36]) -> bool {
    let mut at = 0;
    while at < 36 {
        if !id_byte_fits(at, id[at]) {
            return false;
        }
        at += 1;
    }
    true
}

fn id_byte_fits(at: usize, byte: u8) -> bool {
    match at {
        8 | 13 | 18 | 23 => byte == b'-',
        _ => matches!(byte, b'0'..=b'9' | b'a'..=b'f'),
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
pub async fn list_route(State(shared): State<Arc<Shared>>) -> Response {
    (
        StatusCode::OK,
        axum::Json(Value::Array(shared.profiles.list())),
    )
        .into_response()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct New {
    name: String,
    color: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    name: Option<String>,
    color: Option<String>,
}

/// `POST /v1/profiles`.
pub async fn create_route(
    State(shared): State<Arc<Shared>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let new: New =
        serde_json::from_slice(&body).map_err(|e| bad(format!("That isn't a new profile: {e}")))?;
    let (name, color) = (fit_name(&new.name)?, fit_color(new.color)?);
    let profile = shared.profiles.create(&shared.config.home, name, color)?;
    Ok((StatusCode::CREATED, axum::Json(profile.to_json())).into_response())
}

/// `PATCH /v1/profiles/{id}`.
pub async fn change_route(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let change: Change = serde_json::from_slice(&body)
        .map_err(|e| bad(format!("That isn't a change to a profile: {e}")))?;
    if change.name.is_none() && change.color.is_none() {
        return Err(bad("Give the profile a new name or color."));
    }
    let name = change.name.as_deref().map(fit_name).transpose()?;
    let color = change.color.map(fit_color).transpose()?;
    let profile = shared.profiles.change(&id, name, color)?;
    Ok((StatusCode::OK, axum::Json(profile.to_json())).into_response())
}

/// A name of 1 to 40 characters, trimmed.
fn fit_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if (1..=40).contains(&name.chars().count()) {
        Ok(name.into())
    } else {
        Err(bad("A profile's name is 1 to 40 characters."))
    }
}

/// A colour as `#rrggbb`, kept as sent.
fn fit_color(color: String) -> Result<String, ApiError> {
    let hex = color.strip_prefix('#').unwrap_or_default();
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(color)
    } else {
        Err(bad(
            "A profile's color is # and six hex digits, as #3a5a8c.",
        ))
    }
}

fn bad(why: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, why)
}

impl From<Refused> for ApiError {
    fn from(refused: Refused) -> ApiError {
        match refused {
            Refused::Unreadable => ApiError::new(
                StatusCode::CONFLICT,
                "profiles.json couldn't be read when obid started, so it's left as it is: nothing was changed.",
            ),
            Refused::NoSuchProfile => {
                ApiError::new(StatusCode::NOT_FOUND, "There's no such profile.")
            }
            Refused::Unwritten(e) => ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("The profile couldn't be kept: {e}"),
            ),
        }
    }
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
        assert_eq!(listed[0]["name"], "Default");
        assert_eq!(listed[0]["color"], FIRST_COLOR);
        assert_eq!(listed[0]["default"], true);
        assert_eq!(listed[0]["waiting"], false);
        assert!(listed[0]["id"].as_str().is_some_and(|id| !id.is_empty()));

        let kept = std::fs::read(home.join("profiles.json")).unwrap();
        let saved: Saved = serde_json::from_slice(&kept).unwrap();
        assert_eq!(saved.profiles.len(), 1);
        assert_eq!(saved.profiles[0].name, "Default");
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

    /// PROFILES-17: the default profile's root is `home`; any other's is `home/profiles/<id>` for
    /// a lower-case UUID id, and none for anything else.
    #[test]
    fn profiles_17_a_storage_root_is_inside_home_under_its_id() {
        let home = Path::new("/obid/home");
        let profile = |id: &str, default: bool| Profile {
            id: id.into(),
            name: "Acme".into(),
            color: FIRST_COLOR.into(),
            default,
        };
        let uuid = "3f0c2a4e-9b1d-4c8e-a7f2-5d6e8b9c0a1f";
        assert_eq!(storage_root(home, &profile("p1", true)), Some(home.into()));
        assert_eq!(storage_root(home, &profile(uuid, true)), Some(home.into()));
        let root = storage_root(home, &profile(uuid, false)).unwrap();
        assert_eq!(root, home.join("profiles").join(uuid));
        assert!(root.starts_with(home));
        let upper = uuid.to_uppercase();
        let dot = format!("{}.{}", &uuid[..5], &uuid[6..]);
        let slash = format!("{}/{}", &uuid[..5], &uuid[6..]);
        let short = &uuid[..35];
        let long = format!("{uuid}0");
        for id in [
            "", ".", "..", "../x", "a/b", &upper, &dot, &slash, short, &long,
        ] {
            assert_eq!(storage_root(home, &profile(id, false)), None, "{id:?}");
        }
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

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proof of [`is_profile_id`] (PROFILES-17): `kani::any()` stands for every 36 bytes.

    use super::*;

    /// PROFILES-17: an id is accepted exactly when it is a lower-case UUID, so an accepted one
    /// never holds a path separator, a dot, a NUL or an upper-case letter.
    #[kani::proof]
    #[kani::unwind(37)]
    fn profiles_17_only_a_lower_case_uuid_is_a_profile_id() {
        let id: [u8; 36] = kani::any();
        let mut uuid = true;
        let mut at = 0;
        while at < 36 {
            let byte = id[at];
            uuid &= if at == 8 || at == 13 || at == 18 || at == 23 {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            };
            at += 1;
        }
        let accepted = is_profile_id(id);
        assert_eq!(accepted, uuid);
        if accepted {
            for byte in id {
                assert!(!matches!(byte, b'/' | b'\\' | b'.' | 0) && !byte.is_ascii_uppercase());
            }
        }
        assert!(is_profile_id(*b"3f0c2a4e-9b1d-4c8e-a7f2-5d6e8b9c0a1f"));
    }
}
