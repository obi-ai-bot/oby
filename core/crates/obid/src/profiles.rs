//! Chrome-style profiles on this Mac (docs/contracts/profiles.md): a local id and name in
//! `home/profiles.json`, never an identity. PROF-1 keeps the registry and `GET /v1/profiles`;
//! PROF-3a creates and renames profiles, each with its own folder. A request names its profile
//! with the `Oby-Profile` header ([`root_of`], A00-79): the project routes, and a conversation's
//! `project`, read that profile's own projects. Its conversations, jobs, helpers, accounts and vault
//! secrets are its own too ([`Scope`], crate::profile_scope, B00-40).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode};
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
    /// What `GET /v1/profiles` shows for this one. `waiting` is always false until a profile has
    /// work of its own that waits on the owner.
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

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Saved {
    #[serde(default)]
    profiles: Vec<Profile>,
    /// The ids of deleted profiles: everything one kept is the default profile's (PROFILES-44).
    /// Left out until a profile is deleted, so the file keeps its shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    merged: Vec<String>,
}

/// The profiles kept in `home/profiles.json` (0600).
pub struct Profiles {
    file: PathBuf,
    /// Which profile each account belongs to, by its id (crate::profile_scope, B00-40).
    pub accounts: crate::profile_scope::Tags,
    /// Which profile each vault secret belongs to, by its name.
    pub secrets: crate::profile_scope::Tags,
    /// `profiles.json` didn't read at `open`: nothing writes over it until obid starts again.
    unreadable: bool,
    saved: Mutex<Saved>,
}

/// Why the registry wasn't changed.
#[derive(Debug)]
pub enum Refused {
    Unreadable,
    NoSuchProfile,
    IsDefault,
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
                merged: Vec::new(),
            }
        });
        if saved.profiles.is_empty() {
            saved.profiles = vec![first_profile()];
            if let Err(e) = keep(&file, &saved) {
                tracing::error!("can't keep {}: {e}", file.display());
            }
        }
        Profiles {
            accounts: crate::profile_scope::Tags::open(home.join("account-profiles.json")),
            secrets: crate::profile_scope::Tags::open(home.join("vault-profiles.json")),
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

    /// `id` when a profile has it, or with none the default profile's id (crate::helpers).
    pub fn id_of(&self, id: Option<&str>) -> Option<String> {
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let mut profiles = saved.profiles.iter();
        let found = profiles.find(|p| id.map_or(p.default, |id| p.id == id));
        found.map(|p| p.id.clone())
    }

    /// Every profile's root, as crate::projects::reconcile reads them: `home`, the default's, then
    /// each other profile's own, skipping one that has none.
    pub fn roots(&self, home: &Path) -> Vec<PathBuf> {
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let others = saved.profiles.iter().filter(|p| !p.default);
        let others = others.filter_map(|p| storage_root(home, p));
        std::iter::once(home.to_path_buf()).chain(others).collect()
    }

    /// The folder whose stores a request reaches: the profile its one `Oby-Profile` header names,
    /// or with none the default profile's, `home`. A value that is no profile's id, empty, not text
    /// or sent twice is refused, `400`, never the default. The registry's lock goes on return.
    pub(crate) fn root_of(&self, home: &Path, headers: &HeaderMap) -> Result<PathBuf, ApiError> {
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let (named, twice) = named(&saved, headers);
        let default_at = saved.profiles.iter().position(|p| p.default);
        let root = match root_for(named, default_at) {
            Root::Home => Some(home.to_path_buf()),
            Root::Profile(at) => storage_root(home, &saved.profiles[at]),
            Root::Refused => None,
        };
        root.ok_or_else(|| match twice {
            true => bad("Send one Oby-Profile."),
            false => bad("There's no such profile on this Mac."),
        })
    }

    /// The profile a request is for (B00-40), as [`Self::root_of`] reads its `Oby-Profile`: none
    /// is the default profile, and one this Mac doesn't have, empty, not text or sent twice is
    /// refused, `400`, never the default.
    pub(crate) fn scope_of(&self, headers: &HeaderMap) -> Result<Scope, ApiError> {
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let (named, twice) = named(&saved, headers);
        let default_at = saved.profiles.iter().position(|p| p.default);
        let at = match (named, default_at) {
            (Named::At(at), _) => Some(at),
            (Named::Nothing, at) => at,
            (Named::Unknown, _) => None,
        };
        let refused = || match twice {
            true => bad("Send one Oby-Profile."),
            false => bad("There's no such profile on this Mac."),
        };
        match at {
            Some(at) => Ok(Scope::of(&saved.profiles[at], &saved.merged)),
            // A registry with no default answers what was there before profiles.
            None if named == Named::Nothing => Ok(Scope::before_profiles()),
            None => Err(refused()),
        }
    }

    /// The default profile's scope: what a request naming none, a phone's, a job's own work and
    /// everything kept before profiles belong to.
    pub fn default_scope(&self) -> Scope {
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        (saved.profiles.iter().find(|p| p.default))
            .map_or_else(Scope::before_profiles, |p| Scope::of(p, &saved.merged))
    }

    /// Every profile's scope, in the registry's order (PROFILES-47).
    pub fn scopes(&self) -> Vec<Scope> {
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        (saved.profiles.iter())
            .map(|p| Scope::of(p, &saved.merged))
            .collect()
    }

    /// The scope of the profile `id` (a thing's tag), or the default's for none or for a deleted
    /// profile's.
    pub fn scope_by(&self, id: Option<&str>) -> Option<Scope> {
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            return Some(self.default_scope());
        };
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        if saved.merged.iter().any(|m| m == id) {
            drop(saved);
            return Some(self.default_scope());
        }
        (saved.profiles.iter().find(|p| p.id == id)).map(|p| Scope::of(p, &saved.merged))
    }

    /// The id and name of the profile a thing kept with `tag` belongs to (none, or a deleted
    /// profile's, is the default's): what `otherProfileLogins` names.
    pub fn named_by(&self, tag: Option<&str>) -> Option<(String, String)> {
        let scope = self.scope_by(tag)?;
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let profile = saved.profiles.iter().find(|p| p.id == scope.id)?;
        Some((profile.id.clone(), profile.name.clone()))
    }

    /// Profile `id`'s storage root, when it is one that may be deleted: not the default, and the
    /// registry read at start ([`Self::delete`]).
    pub fn deletable(&self, home: &Path, id: &str) -> Result<PathBuf, Refused> {
        if self.unreadable {
            return Err(Refused::Unreadable);
        }
        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        let profile = (saved.profiles.iter().find(|p| p.id == id)).ok_or(Refused::NoSuchProfile)?;
        if profile.default {
            return Err(Refused::IsDefault);
        }
        storage_root(home, profile).ok_or(Refused::NoSuchProfile)
    }

    /// Profile `id` taken off the registry, kept, its id under `merged`: everything it kept is the
    /// default profile's from now on (PROFILES-44). Never the default.
    pub fn delete(&self, id: &str) -> Result<Profile, Refused> {
        self.update(|saved| {
            let at =
                (saved.profiles.iter().position(|p| p.id == id)).ok_or(Refused::NoSuchProfile)?;
            if saved.profiles[at].default {
                return Err(Refused::IsDefault);
            }
            saved.merged.push(id.to_owned());
            Ok(saved.profiles.remove(at))
        })
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
        let mut next = saved.clone();
        let profile = edit(&mut next)?;
        keep(&self.file, &next).map_err(Refused::Unwritten)?;
        *saved = next;
        Ok(profile)
    }
}

/// What a request's `Oby-Profile` names in `saved`, and whether it was sent twice.
fn named(saved: &Saved, headers: &HeaderMap) -> (Named, bool) {
    let mut sent = headers.get_all("oby-profile").iter();
    let (first, twice) = (sent.next(), sent.next().is_some());
    let named = match first.map(|value| value.to_str()) {
        None => Named::Nothing,
        Some(Ok(id)) if !twice && !id.is_empty() => (saved.profiles.iter())
            .position(|p| p.id == id)
            .map_or(Named::Unknown, Named::At),
        Some(_) => Named::Unknown,
    };
    (named, twice)
}

/// The profile a request is for, or a thing belongs to (B00-40): its id, and whether it's the
/// default. A thing kept with no profile (made before profiles, or in the default profile) is the
/// default's, so an upgrade loses nothing and the default profile's files keep their shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub id: String,
    pub default: bool,
    /// The default profile's only: the deleted profiles whose things are its now (PROFILES-44).
    merged: Vec<String>,
}

impl Scope {
    fn of(profile: &Profile, merged: &[String]) -> Scope {
        Scope {
            id: profile.id.clone(),
            default: profile.default,
            merged: if profile.default {
                merged.to_vec()
            } else {
                Vec::new()
            },
        }
    }

    /// The default profile's, as a test's own `Conversations` has it.
    #[cfg(test)]
    pub fn before_profiles_for_tests() -> Scope {
        Scope::before_profiles()
    }

    /// Profile `id`'s, not the default, as a test's own conversations have it.
    #[cfg(test)]
    pub fn of_profile_for_tests(id: &str) -> Scope {
        Scope {
            id: id.to_owned(),
            default: false,
            merged: Vec::new(),
        }
    }

    /// A home whose registry has no default: everything is as before profiles.
    fn before_profiles() -> Scope {
        Scope {
            id: String::new(),
            default: true,
            merged: Vec::new(),
        }
    }

    /// Whether a thing kept with the profile `tag` belongs to this profile: none, or empty, or a
    /// deleted profile's, is the default's.
    pub fn holds(&self, tag: Option<&str>) -> bool {
        match tag.filter(|tag| !tag.is_empty()) {
            None => self.default,
            Some(tag) => tag == self.id || self.merged.iter().any(|m| m == tag),
        }
    }

    /// What a thing made in this profile keeps: none in the default profile, so its record reads as
    /// one made before profiles; else the profile's id.
    pub fn tag(&self) -> Option<String> {
        (!self.default).then(|| self.id.clone())
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

/// What a request named: no profile, the profile at this place in the registry, or one this Mac doesn't have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Named {
    Nothing,
    At(usize),
    Unknown,
}

/// Whose stores a request reaches: obid's home (the default profile's), the profile at this
/// place in the registry, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Root {
    Home,
    Profile(usize),
    Refused,
}

/// The root a request reaches (PROFILES-28, 29, 32): naming none or the default profile (at
/// `default_at`) is obid's home, naming another is that profile's own, and naming one this Mac
/// doesn't have is refused, never the default.
pub(crate) fn root_for(named: Named, default_at: Option<usize>) -> Root {
    match named {
        Named::Nothing => Root::Home,
        Named::At(at) if Some(at) == default_at => Root::Home,
        Named::At(at) => Root::Profile(at),
        Named::Unknown => Root::Refused,
    }
}

/// The folder whose stores a request reaches (PROFILES-26 to 32): [`Profiles::root_of`] in obid's
/// home.
pub(crate) fn root_of(shared: &Shared, headers: &HeaderMap) -> Result<PathBuf, ApiError> {
    shared.profiles.root_of(&shared.config.home, headers)
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

/// `DELETE /v1/profiles/{id}` (PROFILES-44, 45): a profile that isn't the default goes, and
/// everything it kept is the default's: its projects are added to the default's, after its own,
/// then it leaves the registry (its id kept under `merged`, which makes its conversations, jobs,
/// helpers, accounts and secrets the default's), then its folder goes. Nothing is deleted, stopped
/// or signed out. The default's projects are put back when the registry isn't written.
pub async fn delete_route(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
) -> Result<Response, ApiError> {
    let home = &shared.config.home;
    let root = shared.profiles.deletable(home, &id)?;
    let mut held = shared.projects.lock();
    let before = held.merge_into_home(&root)?;
    if let Err(refused) = shared.profiles.delete(&id) {
        held.put_back_home(before);
        return Err(refused.into());
    }
    held.forget(&root);
    drop(held);
    if let Err(e) = std::fs::remove_dir_all(&root)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(
            "profile {id} is deleted, but {} is left: {e}",
            root.display()
        );
    }
    tracing::info!("profile {id} is deleted: what it kept is the default profile's");
    Ok(StatusCode::NO_CONTENT.into_response())
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
            Refused::IsDefault => ApiError::new(
                StatusCode::CONFLICT,
                "The default profile isn't deleted: everything kept before profiles is its.",
            ),
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
            merged: Vec::new(),
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

    /// PROFILES-32: naming none or the default profile reaches `home`, naming another its own root
    /// and never another's, and naming one this Mac doesn't have is refused.
    #[test]
    fn profiles_32_the_root_is_the_named_profiles_own() {
        assert_eq!(root_for(Named::Nothing, Some(0)), Root::Home);
        assert_eq!(root_for(Named::Nothing, None), Root::Home);
        assert_eq!(root_for(Named::At(1), Some(1)), Root::Home);
        assert_eq!(root_for(Named::At(2), Some(1)), Root::Profile(2));
        assert_eq!(root_for(Named::At(0), None), Root::Profile(0));
        assert_eq!(root_for(Named::Unknown, Some(0)), Root::Refused);

        let home = std::env::temp_dir().join(format!("obid-profiles-32-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let (a, b, c) = (
            "aaaaaaaa-1111-4111-8111-111111111111",
            "22222222-2222-4222-8222-222222222222",
            "33333333-3333-4333-8333-333333333333",
        );
        let profile = |id: &str, default: bool| Profile {
            id: id.into(),
            name: "Acme".into(),
            color: FIRST_COLOR.into(),
            default,
        };
        let saved = Saved {
            profiles: vec![profile(b, false), profile(a, true), profile(c, false)],
            merged: Vec::new(),
        };
        keep(&home.join("profiles.json"), &saved).unwrap();
        let profiles = Profiles::open(&home);
        let root = |values: &[&str]| {
            let mut headers = HeaderMap::new();
            for value in values {
                headers.append("Oby-Profile", value.parse().unwrap());
            }
            profiles.root_of(&home, &headers).map_err(|e| e.status)
        };
        assert_eq!(root(&[]), Ok(home.clone()));
        assert_eq!(root(&[a]), Ok(home.clone()));
        assert_eq!(root(&[b]), Ok(home.join("profiles").join(b)));
        assert_eq!(root(&[c]), Ok(home.join("profiles").join(c)));
        let unknown = "44444444-4444-4444-8444-444444444444";
        for values in [
            &[unknown][..],
            &[""],
            &["../x"],
            &[&a.to_uppercase()],
            &[b, b],
        ] {
            assert_eq!(root(values), Err(StatusCode::BAD_REQUEST), "{values:?}");
        }
        assert!(!home.join("profiles").exists());
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

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of [`is_profile_id`] (PROFILES-17), where `kani::any()` stands for every 36
    //! bytes, and of [`root_for`] (PROFILES-28, 29, 32), for every place in the registry.

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

    /// What the registry's default is: none, or the profile at any place.
    fn any_default() -> Option<usize> {
        let at: usize = kani::any();
        if kani::any::<bool>() { Some(at) } else { None }
    }

    /// PROOF-1, PROFILES-32: a request naming the profile at any place reaches obid's home when
    /// that profile is the default, and that profile's own root otherwise, never another's.
    #[kani::proof]
    fn profiles_32_a_named_profile_reaches_only_its_own_root() {
        let (at, default_at): (usize, Option<usize>) = (kani::any(), any_default());
        let root = root_for(Named::At(at), default_at);
        if default_at == Some(at) {
            assert_eq!(root, Root::Home);
        } else {
            assert_eq!(root, Root::Profile(at));
        }
    }

    /// PROOF-2, PROFILES-28: a request naming a profile this Mac doesn't have is refused whatever
    /// the default, never obid's home or any profile's root.
    #[kani::proof]
    fn profiles_28_an_unknown_profile_is_refused_never_the_default() {
        assert_eq!(root_for(Named::Unknown, any_default()), Root::Refused);
    }

    /// PROOF-3, PROFILES-29: a request naming no profile reaches obid's home, the default
    /// profile's root, whatever the registry holds.
    #[kani::proof]
    fn profiles_29_naming_no_profile_reaches_obids_home() {
        assert_eq!(root_for(Named::Nothing, any_default()), Root::Home);
    }
}
