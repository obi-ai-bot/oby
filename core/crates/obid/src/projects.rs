//! Projects (docs/contracts/projects.md): a named set of folders and GitHub repositories that
//! conversations belong to, kept in `home/projects.json`. A conversation in a project answers the
//! project's folders, from a copy obid keeps in it (crate::conversation). Only the owner's Mac app
//! reaches the routes: they aren't on the gateway's list (crate::gateway::ROUTES).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::{ApiError, Shared};
use crate::conversation::Conversations;

/// Whose folders a conversation answers: its own list, or its project's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Whose {
    Own,
    Project,
}

/// What happens to a conversation's project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Assign,
    TakeOff,
    ProjectGone,
}

/// Whose folders a conversation answers after `event`, given whose it answered before, and
/// whether its own list becomes the project's folders it was working in.
pub fn next(now: Whose, event: Event) -> (Whose, bool) {
    match (now, event) {
        (_, Event::Assign) => (Whose::Project, false),
        (Whose::Project, Event::ProjectGone) => (Whose::Own, true),
        (_, Event::TakeOff | Event::ProjectGone) => (Whose::Own, false),
    }
}

/// The folders a conversation answers, of its `own` and its `project`'s: no decision of its own.
pub fn answered<T>(own: T, project: T, whose: Whose) -> T {
    match whose {
        Whose::Own => own,
        Whose::Project => project,
    }
}

/// One project as `projects.json` keeps it and the routes answer it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub folders: Vec<String>,
    pub repos: Vec<String>,
}

#[derive(Deserialize)]
struct Saved {
    projects: Vec<Project>,
}

/// The projects kept, in the order made, or why `projects.json` doesn't read.
pub type Kept = Result<Vec<Project>, String>;

/// The projects kept in `home/projects.json` (0600). Its lock is taken before the conversations'.
pub struct Projects {
    file: PathBuf,
    kept: Mutex<Kept>,
}

impl Projects {
    /// The projects in `home`; no file is none. A file that doesn't read is logged and left as it
    /// is: nothing writes over it, and every project route answers why.
    pub fn open(home: &Path) -> Projects {
        let file = home.join("projects.json");
        let kept = match std::fs::read(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            read => read.map_err(|e| e.to_string()).and_then(|bytes| {
                let saved = serde_json::from_slice::<Saved>(&bytes);
                saved.map(|s| s.projects).map_err(|e| e.to_string())
            }),
        };
        let kept = kept.map_err(|e| {
            tracing::error!("can't read {}: left as it is ({e})", file.display());
            format!("Oby can't read {}: it's left as it is.", file.display())
        });
        Projects {
            file,
            kept: Mutex::new(kept),
        }
    }

    /// The projects, held until the guard goes: every change to a conversation's project is made
    /// under it.
    pub fn lock(&self) -> MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn keep(&self, projects: &[Project]) -> Result<(), ApiError> {
        let text = serde_json::to_vec_pretty(&json!({ "projects": projects }));
        let written = text.map_err(std::io::Error::other);
        written
            .and_then(|text| crate::store::write_private(&self.file, &text))
            .map_err(|e| {
                tracing::error!("can't keep {}: {e}", self.file.display());
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "The projects couldn't be kept.",
                )
            })
    }
}

/// The project `id` as a conversation joins it: its id and its folders, or why not.
pub fn joined(kept: &Kept, id: &str) -> Result<(String, Vec<String>), String> {
    let project = kept.as_ref()?.iter().find(|p| p.id == id);
    let project = project.ok_or("There's no project with that id.")?;
    Ok((project.id.clone(), project.folders.clone()))
}

/// What a paired device or the phone relay hears when it puts a conversation in a project.
pub fn chosen_on_the_mac() -> ApiError {
    refused(
        "Projects are chosen only on the Mac: from here you can take a conversation out of one.",
    )
}

/// At start, before obid serves: each conversation in a kept project gets its folders as they are
/// now, and one whose project is gone leaves it ([`Event::ProjectGone`]). Skipped when
/// `projects.json` doesn't read.
pub fn reconcile(projects: &Projects, conversations: &Conversations) {
    let kept = projects.lock();
    let Ok(kept) = &*kept else { return };
    let done = conversations.regroup(|id, copy| match kept.iter().find(|p| p.id == id) {
        Some(project) if project.folders == copy => None,
        Some(project) => Some(Some(project.folders.clone())),
        None => Some(None),
    });
    if let Err(e) = done {
        tracing::error!("conversations not moved to their projects as they are: {e:?}");
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    name: Option<String>,
    folders: Option<Vec<String>>,
    repos: Option<Vec<String>>,
}

const NAME: &str = "name is one line of 1 to 100 characters.";

fn refused(why: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why)
}

fn unreadable(why: &str) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, why)
}

/// Where the project `id` is in `projects`.
fn find(projects: &[Project], id: &str) -> Result<usize, ApiError> {
    let at = projects.iter().position(|p| p.id == id);
    at.ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "There's no project with that id."))
}

/// The body, each value it has checked: the name trimmed, the folders as crate::folders keeps
/// them, each repository once (any case).
fn checked(shared: &Shared, body: &Bytes) -> Result<Body, ApiError> {
    let body: Body = serde_json::from_slice(body).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "The body is {\"name\": \"…\", \"folders\": [\"/path\"], \"repos\": [\"owner/name\"]}.",
        )
    })?;
    let name = body.name.map(|n| n.trim().to_owned());
    if !name.as_deref().is_none_or(crate::conversation::one_line) {
        return Err(refused(NAME));
    }
    let places = crate::folders::Places::of(shared);
    let folders = body.folders.map(|f| crate::folders::checked(&places, &f));
    let folders = folders.transpose().map_err(refused)?;
    let mut repos = body.repos;
    if let Some(repo) = repos.iter().flatten().find(|r| !crate::github::is_repo(r)) {
        return Err(refused(format!(
            "\"{repo}\" isn't a repository's owner/name."
        )));
    }
    let mut seen = std::collections::HashSet::new();
    repos
        .iter_mut()
        .for_each(|r| r.retain(|repo| seen.insert(repo.to_ascii_lowercase())));
    Ok(Body {
        name,
        folders,
        repos,
    })
}

/// `GET /v1/projects`: every project, in the order made.
pub async fn list(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    let kept = shared.projects.lock();
    let projects = kept.as_ref().map_err(|why| unreadable(why))?;
    Ok((StatusCode::OK, axum::Json(projects)).into_response())
}

/// `POST /v1/projects`: a new project, `folders` and `repos` none when left out.
pub async fn create(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    let body = checked(&shared, &body)?;
    let project = Project {
        id: uuid::Uuid::new_v4().to_string().to_lowercase(),
        name: body.name.ok_or_else(|| refused(NAME))?,
        folders: body.folders.unwrap_or_default(),
        repos: body.repos.unwrap_or_default(),
    };
    let mut kept = shared.projects.lock();
    let projects = kept.as_mut().map_err(|why| unreadable(why))?;
    projects.push(project.clone());
    if let Err(e) = shared.projects.keep(projects) {
        projects.pop();
        return Err(e);
    }
    Ok((StatusCode::CREATED, axum::Json(project)).into_response())
}

/// `PATCH /v1/projects/{id}`: its name, folders or repositories. New folders are kept first, then
/// reach every conversation in it.
pub async fn change(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let body = checked(&shared, &body)?;
    if body.name.is_none() && body.folders.is_none() && body.repos.is_none() {
        return Err(refused("Send a name, folders, repos, or more than one."));
    }
    let mut kept = shared.projects.lock();
    let projects = kept.as_mut().map_err(|why| unreadable(why))?;
    let at = find(projects, &id)?;
    let before = projects[at].clone();
    let project = &mut projects[at];
    project.name = body.name.unwrap_or(before.name.clone());
    project.folders = body.folders.clone().unwrap_or(before.folders.clone());
    project.repos = body.repos.unwrap_or(before.repos.clone());
    let project = project.clone();
    if let Err(e) = shared.projects.keep(projects) {
        projects[at] = before;
        return Err(e);
    }
    if let Some(folders) = body.folders {
        let moved = |p: &str, _: &[String]| (p == id).then(|| Some(folders.clone()));
        shared.conversations.regroup(moved)?;
    }
    Ok((StatusCode::OK, axum::Json(project)).into_response())
}

/// `DELETE /v1/projects/{id}`: every conversation in it leaves it first, keeping the folders it
/// works in; then the project goes. One not written answers 500, and the project stays.
pub async fn delete(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
) -> Result<Response, ApiError> {
    let mut kept = shared.projects.lock();
    let projects = kept.as_mut().map_err(|why| unreadable(why))?;
    let at = find(projects, &id)?;
    let gone = |p: &str, _: &[String]| (p == id).then_some(None);
    shared.conversations.regroup(gone)?;
    let gone = projects.remove(at);
    if let Err(e) = shared.projects.keep(projects) {
        projects.insert(at, gone);
        return Err(e);
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PROJECTS-4: a conversation put in a project answers its folders, its own list untouched.
    #[test]
    fn projects_4_assign_answers_the_project() {
        for now in [Whose::Own, Whose::Project] {
            assert_eq!(next(now, Event::Assign), (Whose::Project, false));
        }
        let (own, theirs) = (
            vec!["/a".to_owned()],
            vec!["/b".to_owned(), "/c".to_owned()],
        );
        assert_eq!(answered(&own, &theirs, Whose::Project), &theirs);
        assert_eq!(answered(&own, &theirs, Whose::Own), &own);
    }

    /// PROJECTS-5: taken out, it answers its own list again, never replaced.
    #[test]
    fn projects_5_take_off_answers_the_own_list() {
        for now in [Whose::Own, Whose::Project] {
            assert_eq!(next(now, Event::TakeOff), (Whose::Own, false));
        }
    }

    /// PROJECTS-8: a gone project's conversation keeps the folders it answered as its own; one
    /// not in it is left as it was.
    #[test]
    fn projects_8_project_gone_keeps_the_folders() {
        assert_eq!(next(Whose::Project, Event::ProjectGone), (Whose::Own, true));
        assert_eq!(next(Whose::Own, Event::ProjectGone), (Whose::Own, false));
    }

    /// PROJECTS-12: a `projects.json` that doesn't read is answered as why, and left as it is.
    #[test]
    fn projects_12_an_unreadable_file_is_left_as_it_is() {
        let home = std::env::temp_dir().join(format!("obid-projects-12-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("projects.json"), "not json").unwrap();
        let projects = Projects::open(&home);
        let why = joined(&projects.lock(), "p").unwrap_err();
        assert!(why.contains("projects.json"), "{why}");
        assert_eq!(
            std::fs::read(home.join("projects.json")).unwrap(),
            b"not json"
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of [`next`]: `kani::any()` stands for whose folders a conversation answers,
    //! and for its two lists, each a number.

    use super::*;

    fn any_whose() -> Whose {
        if kani::any::<bool>() {
            Whose::Project
        } else {
            Whose::Own
        }
    }

    /// PROOF-1, PROJECTS-8: when its project goes, a conversation answers the same folders as
    /// before, now its own; one not in it is left as it was.
    #[kani::proof]
    fn projects_8_a_gone_project_leaves_its_folders() {
        let (now, own, theirs): (Whose, u8, u8) = (any_whose(), kani::any(), kani::any());
        let before = answered(own, theirs, now);
        let (whose, keeps) = next(now, Event::ProjectGone);
        let own_after = if keeps { theirs } else { own };
        assert_eq!(whose, Whose::Own);
        assert_eq!(answered(own_after, theirs, whose), before);
        if now == Whose::Own {
            assert_eq!(own_after, own);
        }
    }

    /// PROOF-2, PROJECTS-5: taken out of its project, a conversation answers its own list, never
    /// replaced, whichever it answered.
    #[kani::proof]
    fn projects_5_taking_a_project_off_keeps_the_own_list() {
        let (now, own, theirs): (Whose, u8, u8) = (any_whose(), kani::any(), kani::any());
        let (whose, keeps) = next(now, Event::TakeOff);
        let own_after = if keeps { theirs } else { own };
        assert_eq!(whose, Whose::Own);
        assert_eq!(own_after, own);
        assert_eq!(answered(own_after, theirs, whose), own);
    }
}
