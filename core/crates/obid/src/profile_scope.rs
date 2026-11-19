//! What belongs to which profile (docs/contracts/profiles.md, B00-40, PROFILES-33 to 40): each
//! request is for one profile ([`crate::profiles::Scope`], from its `Oby-Profile`), and a
//! conversation, a job, a helper, an account or a vault secret made in one is that profile's alone.
//! Conversations, jobs and helpers keep their profile in their own record; accounts and secrets,
//! whose files an older obid reads strictly, in a side file of their own ([`Tags`]). A thing kept
//! with no profile is the default's, so nothing made before profiles is lost.
//!
//! It owns the per-request check ([`scoped`]), the side files, and which events a follower is sent
//! ([`shown`]). It never moves a thing from one profile to another.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::api::{ApiError, Shared};
use crate::profiles::Scope;

/// Which profile each of a kind of thing belongs to, by its key (an account's id, a secret's
/// name), in a file of its own (0600): `{"<key>": "<profile id>"}`. A key it doesn't hold is the
/// default profile's. A file that doesn't read is left as it is, and nothing is kept in it until
/// obid starts again with one that reads: until then every key reads as the default profile's,
/// and adding one to another profile is refused.
pub struct Tags {
    file: PathBuf,
    unreadable: bool,
    tags: Mutex<BTreeMap<String, String>>,
}

impl Tags {
    pub fn open(file: PathBuf) -> Tags {
        let read = match std::fs::read(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e.to_string()),
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        };
        let (tags, unreadable) = match read {
            Ok(tags) => (tags, false),
            Err(why) => {
                tracing::warn!("{} doesn't read ({why}): left as it is", file.display());
                (BTreeMap::new(), true)
            }
        };
        Tags {
            file,
            unreadable,
            tags: Mutex::new(tags),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        self.tags.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The profile `key` belongs to: none is the default's.
    pub fn of(&self, key: &str) -> Option<String> {
        self.lock().get(key).cloned()
    }

    /// Whether `key` is `scope`'s.
    pub fn holds(&self, scope: &Scope, key: &str) -> bool {
        scope.holds(self.of(key).as_deref())
    }

    /// `key` is made in `scope`, kept: the default profile's keeps no line.
    pub fn made_in(&self, scope: &Scope, key: &str) -> Result<(), String> {
        let mut tags = self.lock();
        let mut next = tags.clone();
        match scope.tag() {
            Some(profile) => next.insert(key.to_owned(), profile),
            None => next.remove(key),
        };
        if next == *tags {
            return Ok(());
        }
        if self.unreadable {
            return Err(format!(
                "{} couldn't be read when obid started, so it's left as it is.",
                self.file.display()
            ));
        }
        let text = serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?;
        crate::store::write_private(&self.file, &text).map_err(|e| e.to_string())?;
        *tags = next;
        Ok(())
    }

    /// `key` is gone: its line goes too, so a new one of the same key starts where it's made.
    pub fn forget(&self, key: &str) {
        let mut tags = self.lock();
        if self.unreadable || tags.remove(key).is_none() {
            return;
        }
        let text = serde_json::to_vec_pretty(&*tags).unwrap_or_default();
        if let Err(e) = crate::store::write_private(&self.file, &text) {
            tracing::warn!("can't keep {}: {e}", self.file.display());
        }
    }
}

/// Every `/v1` request: the profile it names is found ([`crate::profiles::Profiles::scope_of`]),
/// refused `400` when this Mac has none such, and given to its route as an extension. A request for
/// one job, conversation, account or helper of another profile is answered `404`, as one this Mac
/// doesn't have: nothing of another profile is read, changed or used from this one.
pub async fn scoped(
    State(shared): State<Arc<Shared>>,
    mut request: Request,
    next: Next,
) -> Response {
    if !request.uri().path().starts_with("/v1/") {
        return next.run(request).await;
    }
    let scope = match shared.profiles.scope_of(request.headers()) {
        Ok(scope) => scope,
        Err(refused) => return refused.into_response(),
    };
    if let Some(refused) = another_profiles(&shared, &scope, request.uri().path()) {
        return refused.into_response();
    }
    request.extensions_mut().insert(scope);
    next.run(request).await
}

/// The `404` a path naming another profile's thing gets, or none.
fn another_profiles(shared: &Shared, scope: &Scope, path: &str) -> Option<ApiError> {
    let mut parts = path.trim_start_matches("/v1/").split('/');
    let (kind, id) = (parts.next()?, parts.next().filter(|id| !id.is_empty())?);
    let (theirs, what) = match kind {
        "jobs" => (
            job_profile(shared, id).map(|tag| !scope.holds(tag.as_deref())),
            "job",
        ),
        "conversations" => (
            (shared.conversations.profile_of(id)).map(|tag| !scope.holds(tag.as_deref())),
            "conversation",
        ),
        "accounts" => (Some(!shared.profiles.accounts.holds(scope, id)), "account"),
        "helpers" => (
            (shared.helpers.lock().iter().find(|h| h.id == id))
                .map(|h| !scope.holds(Some(&h.profile))),
            "helper",
        ),
        _ => return None,
    };
    theirs.filter(|theirs| *theirs).map(|_| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            format!("There's no {what} with that id."),
        )
    })
}

/// The profile job `id` was made in, or none when obid has no such job.
fn job_profile(shared: &Shared, id: &str) -> Option<Option<String>> {
    shared.jobs.profile_of(id)
}

/// Every obid's registry, by its home: what a job's own work reads, where no request names a
/// profile (one obid per home; a test may run several).
static REGISTRIES: Mutex<Vec<(PathBuf, Arc<crate::profiles::Profiles>)>> = Mutex::new(Vec::new());

/// `profiles` is the registry of the obid in `home`.
pub fn register(home: &std::path::Path, profiles: Arc<crate::profiles::Profiles>) {
    let mut registries = REGISTRIES.lock().unwrap_or_else(|e| e.into_inner());
    registries.retain(|(at, _)| at != home);
    registries.push((home.to_path_buf(), profiles));
}

/// The registry of the obid in `home`, once it started.
pub fn registry(home: &std::path::Path) -> Option<Arc<crate::profiles::Profiles>> {
    let registries = REGISTRIES.lock().unwrap_or_else(|e| e.into_inner());
    (registries.iter().find(|(at, _)| at == home)).map(|(_, profiles)| profiles.clone())
}

/// Whether the account `id` may take work of the profile `tag` (none: the default's), in the obid
/// in `home`: an account of that profile only. With no registry (a test's own `Jobs`), any.
pub fn account_for(home: &std::path::Path, tag: Option<&str>, id: &str) -> bool {
    let Some(profiles) = registry(home) else {
        return true;
    };
    match profiles.scope_by(tag) {
        Some(scope) => profiles.accounts.holds(&scope, id),
        None => false,
    }
}

/// The scope a route was given by [`scoped`], else the default profile's (a route reached without
/// it: a test's own router).
pub fn of(shared: &Shared, scope: Option<axum::Extension<Scope>>) -> Scope {
    scope.map_or_else(|| shared.profiles.default_scope(), |s| s.0)
}

/// The scope of the profile conversation `id` was made in: what its Oby may read and start
/// (the brain's tools). A conversation obid doesn't have is the default profile's.
pub fn of_conversation(shared: &Shared, id: &str) -> Scope {
    let tag = shared.conversations.profile_of(id).flatten();
    (shared.profiles.scope_by(tag.as_deref())).unwrap_or_else(|| shared.profiles.default_scope())
}

/// Every job of the profile conversation `id` was made in: what its Oby is told of.
pub fn jobs_of(shared: &Shared, id: &str) -> Vec<crate::job::Job> {
    let scope = of_conversation(shared, id);
    let jobs = shared.jobs.list(usize::MAX).into_iter();
    jobs.filter(|job| scope.holds(job.profile.as_deref()))
        .collect()
}

/// What an event follower of `scope` is sent of `name`'s `data`: the event itself, or for
/// `accounts` only this profile's, or nothing when it's another profile's job's or
/// conversation's. What belongs to no profile (the inbox, GitHub, the Obysphere) goes to all.
pub fn shown(shared: &Shared, scope: &Scope, name: &str, data: &Value) -> Option<Value> {
    let tagged = |data: &Value| scope.holds(data.get("profile").and_then(Value::as_str));
    match name {
        "accounts" => Some(Value::Array(
            (data.as_array().into_iter().flatten())
                .filter(|account| {
                    let id = account
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    shared.profiles.accounts.holds(scope, id)
                })
                .cloned()
                .collect(),
        )),
        "job" | "conversation" if data.get("id").is_some() => tagged(data).then(|| data.clone()),
        _ => {
            let conversation = data.get("conversation").and_then(Value::as_str);
            let job = data.get("job").and_then(Value::as_str);
            let theirs = match (conversation, job) {
                (Some(id), _) => shared.conversations.profile_of(id),
                (None, Some(id)) => job_profile(shared, id),
                (None, None) => None,
            };
            theirs
                .is_none_or(|tag| scope.holds(tag.as_deref()))
                .then(|| data.clone())
        }
    }
}
