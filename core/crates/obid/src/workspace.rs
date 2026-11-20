//! A conversation's workspace (docs/contracts/folders.md, WORKSPACE-1 to 6): where its work
//! happens, chosen with its first message and never changed after, as T3 Code's thread `envMode`
//! (`local`, `worktree`; `packages/contracts/src/environment.ts`). **This folder** (the default)
//! edits the project's folder in place; **A new worktree** works in a worktree of the folder's
//! repository on a branch of its own, made under `home/workspaces/<conversation>/`. A folder that
//! isn't a git repository always works in place. Factory jobs keep their own worktree in obid's
//! clone, whatever the choice (WORKSPACE-6).
//!
//! It also finishes the work, as T3's git actions do (crate::folders::finishing): `GET` and `POST
//! /v1/conversations/{id}/workspace`, each action signed by the owner's device, and what leaves
//! the Mac scanned for secrets first.
//!
//! State it owns: `home/workspaces/<conversation>.json` (the choice, the worktrees made for it) and
//! the worktrees themselves. It never writes the conversation's own file or a job; in a folder's
//! repository it only adds a worktree and its branch, and takes the worktree off again with its
//! conversation, or once its branch is merged.

pub mod writes;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::{ApiError, Shared};
use crate::folders::Action;

/// Where a conversation works: the words on screen are "This folder" and "A new worktree".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Workspace {
    /// The project's folder itself, edited in place (T3's `local`).
    #[default]
    Folder,
    /// A worktree of the folder's repository on a branch of its own (T3's `worktree`).
    Worktree,
}

/// What `home/workspaces/<conversation>.json` keeps.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
    workspace: Workspace,
    #[serde(default)]
    worktrees: Vec<Worktree>,
}

/// A worktree made for one of the conversation's folders.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Worktree {
    /// The conversation's folder it was made from (canonical).
    folder: String,
    /// The worktree's own root, under `home/workspaces/<conversation>/`.
    root: String,
    /// Where the conversation works in it: the root, or the folder's place inside its repository.
    path: String,
    branch: String,
    /// The commit it started from: a branch still there has nothing merged.
    base: String,
}

/// Where a conversation works in one of its folders, for the lanes (WORKSPACE-1) and the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The conversation's folder.
    pub folder: PathBuf,
    /// Where its work happens: the folder itself, or its worktree. None when obid can't tell or
    /// can't make it: then the conversation works only in its own room there, never in the
    /// folder instead (fail closed, WORKSPACE-3).
    pub path: Option<PathBuf>,
    /// The worktree's branch; none in place.
    pub branch: Option<String>,
    /// Why it has no path.
    pub why: Option<String>,
}

impl Place {
    fn at(folder: &str, path: &str, branch: Option<String>) -> Place {
        Place {
            folder: PathBuf::from(folder),
            path: Some(PathBuf::from(path)),
            branch,
            why: None,
        }
    }

    /// Its room only, and why.
    fn room(folder: &str, why: String) -> Place {
        Place {
            folder: PathBuf::from(folder),
            path: None,
            branch: None,
            why: Some(why),
        }
    }
}

/// One change to the workspaces at a time: two requests never make two worktrees for one folder.
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The folder that holds every conversation's worktrees and its file.
pub fn worktrees(home: &Path) -> PathBuf {
    home.join("workspaces")
}

/// A conversation's id as a file name: letters, digits and `-` only.
fn is_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn file(home: &Path, id: &str) -> PathBuf {
    worktrees(home).join(format!("{id}.json"))
}

/// What conversation `id` keeps: none when it chose nothing (no file), why not when its file is
/// there but can't be read or doesn't parse. Never a default in its place.
fn read(home: &Path, id: &str) -> Result<Option<Kept>, String> {
    match std::fs::read(file(home, id)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("its file doesn't read: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn write(home: &Path, id: &str, kept: &Kept) -> Result<(), String> {
    crate::store::make_private_dir(&worktrees(home)).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(kept).map_err(|e| e.to_string())?;
    crate::store::write_private(&file(home, id), &bytes).map_err(|e| e.to_string())
}

/// The workspace conversation `id` chose: `Folder` when it chose none; why not when its choice
/// can't be read.
pub fn chosen(home: &Path, id: &str) -> Result<Workspace, String> {
    if !is_id(id) {
        return Err("That isn't a conversation's id.".into());
    }
    Ok(read(home, id)?.unwrap_or_default().workspace)
}

/// Keeps a new conversation's choice, sent with its first message (WORKSPACE-3), and makes its
/// worktrees now. `Err` when the choice can't be kept: the caller refuses the conversation, as
/// none kept would read as "This folder".
pub async fn chose(shared: &Shared, id: &str, workspace: Workspace) -> Result<(), String> {
    if !is_id(id) {
        return Err("That isn't a conversation's id.".into());
    }
    {
        let _held = LOCK.lock().await;
        write(
            &shared.config.home,
            id,
            &Kept {
                workspace,
                worktrees: vec![],
            },
        )?;
    }
    places(shared, id).await;
    Ok(())
}

/// Where conversation `id` works in each of its folders, its worktrees made first for those that
/// have none yet (a folder added later, one that became a repository, or one whose worktree is
/// gone). It fails closed: a choice it can't read, or a worktree it can't make, gives that folder
/// no path (the room only), never the folder itself.
pub async fn places(shared: &Shared, id: &str) -> Vec<Place> {
    if !is_id(id) {
        return vec![];
    }
    let folders = shared.conversations.working_folders(id).unwrap_or_default();
    let home = &shared.config.home;
    let _held = LOCK.lock().await;
    let mut kept = match read(home, id) {
        Ok(kept) => kept.unwrap_or_default(),
        Err(why) => {
            tracing::warn!("conversation {id}: where it works can't be read: {why}");
            let why = format!(
                "Oby couldn't read where this conversation works ({why}), so it works only in its own room."
            );
            return (folders.iter())
                .map(|f| Place::room(f, why.clone()))
                .collect();
        }
    };
    if kept.workspace == Workspace::Folder {
        return (folders.iter()).map(|f| Place::at(f, f, None)).collect();
    }
    // A worktree that's gone is made again, on a branch of its own.
    let before = kept.worktrees.len();
    kept.worktrees.retain(|w| Path::new(&w.root).is_dir());
    let mut changed = kept.worktrees.len() != before;
    let mut in_place: Vec<&str> = Vec::new();
    let mut failed: Vec<(&str, String)> = Vec::new();
    for folder in &folders {
        if kept
            .worktrees
            .iter()
            .any(|w| crate::folders::same(&w.folder, folder))
        {
            continue;
        }
        match make(home, id, Path::new(folder)).await {
            Ok(Some(worktree)) => {
                kept.worktrees.push(worktree);
                changed = true;
            }
            Ok(None) => in_place.push(folder.as_str()),
            Err(why) => {
                tracing::warn!("conversation {id}: no worktree for a folder: {why}");
                failed.push((folder.as_str(), why));
            }
        }
    }
    if changed && let Err(why) = write(home, id, &kept) {
        tracing::warn!("conversation {id}: its worktrees aren't kept: {why}");
    }
    folders
        .iter()
        .map(|folder| {
            let worktree = (kept.worktrees.iter()).find(|w| crate::folders::same(&w.folder, folder));
            if let Some(w) = worktree {
                Place::at(folder, &w.path, Some(w.branch.clone()))
            } else if in_place.contains(&folder.as_str()) {
                Place::at(folder, folder, None)
            } else {
                let why = (failed.iter())
                    .find(|(f, _)| *f == folder.as_str())
                    .map_or("no worktree was made", |(_, why)| why.as_str());
                Place::room(
                    folder,
                    format!(
                        "Oby couldn't make a worktree of {folder} ({why}), so this conversation works only in its own room there."
                    ),
                )
            }
        })
        .collect()
}

/// A worktree of `folder`'s repository for conversation `id`, on a new branch `oby/<id's start>`
/// at the folder's `HEAD`; none for a folder that isn't a repository with a commit (it works in
/// place, as T3's `resolveSendEnvMode` decides). The folder's files and branches don't change.
async fn make(home: &Path, id: &str, folder: &Path) -> Result<Option<Worktree>, String> {
    let state = crate::git::state(folder).await;
    if !state.repo {
        // In place only when git itself says it isn't a repository: git that can't say is no
        // place at all (fail closed), never the folder.
        let args = [OsStr::new("rev-parse"), OsStr::new("--is-inside-work-tree")];
        return match crate::git::run(folder, &args, &[("LC_ALL", "C")]).await {
            Err(e) if e.0.contains("not a git repository") => Ok(None),
            Err(e) => Err(e.0),
            Ok(said) => Err(format!("git says {said} of it as a repository")),
        };
    }
    if !state.committed {
        return Ok(None);
    }
    let git = |args: Vec<String>| async move {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        crate::git::git(folder, &args).await.map_err(|e| e.0)
    };
    let top = git(vec!["rev-parse".into(), "--show-toplevel".into()]).await?;
    let top = std::fs::canonicalize(&top).map_err(|e| e.to_string())?;
    let inside = folder
        .strip_prefix(&top)
        .unwrap_or(Path::new(""))
        .to_path_buf();
    let base = git(vec!["rev-parse".into(), "--verify".into(), "HEAD".into()]).await?;
    let short: String = id.chars().take(8).collect();
    let mut branch = format!("oby/{short}");
    for n in 2.. {
        let taken = git(vec![
            "rev-parse".into(),
            "--verify".into(),
            "-q".into(),
            format!("refs/heads/{branch}"),
        ])
        .await
        .is_ok();
        if !taken {
            break;
        }
        branch = format!("oby/{short}-{n}");
    }
    let name = crate::folders::name(&top.to_string_lossy()).to_owned();
    let under = worktrees(home).join(id);
    crate::store::make_private_dir(&under).map_err(|e| e.to_string())?;
    let mut root = under.join(&name);
    for n in 2.. {
        if !root.exists() {
            break;
        }
        root = under.join(format!("{name}-{n}"));
    }
    git(vec![
        "worktree".into(),
        "add".into(),
        "-q".into(),
        "-b".into(),
        branch.clone(),
        root.to_string_lossy().into_owned(),
        base.clone(),
    ])
    .await?;
    let root = std::fs::canonicalize(&root).map_err(|e| e.to_string())?;
    // The root itself for a folder at its repository's top: `join("")` would end it in a `/`.
    let path = if inside.as_os_str().is_empty() {
        root.clone()
    } else {
        root.join(inside)
    };
    Ok(Some(Worktree {
        folder: folder.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        root: root.to_string_lossy().into_owned(),
        branch,
        base,
    }))
}

/// Takes a worktree off its folder's repository; its branch, and every commit on it, stay.
async fn remove(worktree: &Worktree) {
    let folder = Path::new(&worktree.folder);
    let removed = crate::git::git(
        folder,
        &["worktree", "remove", "--force", "--force", &worktree.root],
    )
    .await;
    if removed.is_err() {
        let _ = std::fs::remove_dir_all(&worktree.root);
        let _ = crate::git::git(folder, &["worktree", "prune"]).await;
    }
}

/// Conversation `id` went: its worktrees go with it, their branches stay (T3 removes a thread's
/// worktree with the thread).
pub async fn forget(shared: &Shared, id: &str) {
    if !is_id(id) {
        return;
    }
    let home = &shared.config.home;
    let _held = LOCK.lock().await;
    match read(home, id) {
        Ok(kept) => {
            for worktree in kept.unwrap_or_default().worktrees {
                remove(&worktree).await;
            }
        }
        // Its folder below goes anyway; the repositories prune the worktree on their next look.
        Err(why) => tracing::warn!("conversation {id}: its worktrees can't be read: {why}"),
    }
    let _ = std::fs::remove_file(file(home, id));
    let _ = std::fs::remove_dir_all(worktrees(home).join(id));
}

/// Whether a worktree's branch is merged: it has commits of its own since `base`, nothing
/// uncommitted, and its remote's default branch holds its `HEAD`.
pub(crate) fn merged(state: &crate::git::State, head: &str, base: &str, on_default: bool) -> bool {
    head != base && !state.changes && on_default
}

/// The cleanup pass's half for conversations: every worktree whose branch is merged is taken off,
/// as a merged job's is (A00-69). Its branch stays.
pub async fn tidy(shared: &Shared) {
    let home = &shared.config.home;
    let Ok(entries) = std::fs::read_dir(worktrees(home)) else {
        return;
    };
    let ids: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".json")
                .map(str::to_owned)
        })
        .filter(|id| is_id(id))
        .collect();
    for id in ids {
        let _held = LOCK.lock().await;
        let mut kept = match read(home, &id) {
            Ok(Some(kept)) => kept,
            Ok(None) => continue,
            Err(why) => {
                tracing::warn!("conversation {id}: its worktrees can't be read: {why}");
                continue;
            }
        };
        let before = kept.worktrees.len();
        let mut left = Vec::new();
        for worktree in std::mem::take(&mut kept.worktrees) {
            let root = Path::new(&worktree.root);
            if !root.is_dir() {
                continue;
            }
            if is_merged(&worktree).await {
                remove(&worktree).await;
            } else {
                left.push(worktree);
            }
        }
        kept.worktrees = left;
        if kept.worktrees.len() != before
            && let Err(why) = write(home, &id, &kept)
        {
            tracing::warn!("conversation {id}: its worktrees aren't kept: {why}");
        }
    }
}

async fn is_merged(worktree: &Worktree) -> bool {
    let root = Path::new(&worktree.root);
    let state = crate::git::state(root).await;
    let (Some(remote), Some(default)) = (&state.remote, &state.default_branch) else {
        return false;
    };
    let Ok(head) = crate::git::git(root, &["rev-parse", "HEAD"]).await else {
        return false;
    };
    let target = format!("refs/remotes/{remote}/{default}");
    let on_default = (crate::git::git(root, &["merge-base", "--is-ancestor", &head, &target]))
        .await
        .is_ok();
    merged(&state, &head, &worktree.base, on_default)
}

/// The router: a conversation's workspace and its finishing actions, loopback only (the gateway
/// lets neither through).
pub fn router() -> Router<Arc<Shared>> {
    Router::new().route("/v1/conversations/{id}/workspace", get(look).post(finish))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asked {
    /// With `finish`: the sentence for that action in this folder, which the owner signs.
    folder: Option<String>,
    finish: Option<Action>,
}

fn no_conversation() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "There's no such conversation.")
}

/// `404` unless obid has conversation `id` in the profile the request names: another profile's
/// conversation is none of this one's (B00-40).
fn held(
    shared: &Shared,
    id: &str,
    scope: Option<axum::Extension<crate::profiles::Scope>>,
) -> Result<(), ApiError> {
    let scope = crate::profile_scope::of(shared, scope);
    let profile = (shared.conversations.profile_of(id)).ok_or_else(no_conversation)?;
    if scope.holds(profile.as_deref()) {
        Ok(())
    } else {
        Err(no_conversation())
    }
}

/// `GET /v1/conversations/{id}/workspace`: its workspace and, for each folder, where it works and
/// what finishing can do there now. With `folder` and `finish`, obid's sentence for that action,
/// which the owner's device signs.
async fn look(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    scope: Option<axum::Extension<crate::profiles::Scope>>,
    Query(asked): Query<Asked>,
) -> Result<Response, ApiError> {
    held(&shared, &id, scope)?;
    if let (Some(folder), Some(action)) = (asked.folder, asked.finish) {
        let (place, state) = find(&shared, &id, &folder).await?;
        let (sentence, _) = what(action, &place, &state).await?;
        return Ok(answer(json!({"action": sentence})));
    }
    Ok(answer(seen(&shared, &id).await))
}

/// The workspace and its places as the app shows them. A place obid can't give a path is the
/// room only: no path, no action, and why.
async fn seen(shared: &Shared, id: &str) -> Value {
    let mut out = Vec::new();
    for place in places(shared, id).await {
        let name = crate::folders::name(&place.folder.to_string_lossy()).to_owned();
        let Some(path) = &place.path else {
            out.push(json!({
                "folder": place.folder,
                "name": name,
                "path": null,
                "worktree": false,
                "branch": null,
                "actions": [],
                "why": place.why,
            }));
            continue;
        };
        let state = crate::git::state(path).await;
        out.push(json!({
            "folder": place.folder,
            "name": name,
            "path": path,
            "worktree": place.branch.is_some(),
            "branch": state.branch,
            "actions": crate::folders::finishing(&state),
        }));
    }
    json!({"workspace": chosen(&shared.config.home, id).ok(), "places": out})
}

fn answer(body: Value) -> Response {
    (StatusCode::OK, axum::Json(body)).into_response()
}

/// The conversation's place for `folder`, one of its own with a path, and its git state now.
async fn find(
    shared: &Shared,
    id: &str,
    folder: &str,
) -> Result<(Place, crate::git::State), ApiError> {
    let place = places(shared, id)
        .await
        .into_iter()
        .find(|p| crate::folders::same(&p.folder.to_string_lossy(), folder))
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "That folder isn't one this conversation works in.",
            )
        })?;
    let Some(path) = &place.path else {
        let why = place.why.clone().unwrap_or_default();
        return Err(ApiError::new(StatusCode::CONFLICT, why));
    };
    let state = crate::git::state(path).await;
    Ok((place, state))
}

/// A file a commit from finishing takes: its path from the repository's top, and whether git
/// doesn't track it yet.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Change {
    path: String,
    new: bool,
}

/// What git shows changed in `dir` (the place, never the rest of its repository): each untracked
/// file by itself, none git ignores, renames as their two halves.
async fn changes(dir: &Path) -> Result<Vec<Change>, String> {
    let args = [
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--no-renames",
        "--",
        ".",
    ];
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    let out = crate::git::run_bytes(dir, &args, &[], None)
        .await
        .map_err(|e| e.0)?;
    Ok(parse_status(&out))
}

/// `git status --porcelain=v1 -z --no-renames`: `XY path\0`, one path each.
fn parse_status(out: &[u8]) -> Vec<Change> {
    out.split(|b| *b == 0)
        .filter(|entry| entry.len() > 3)
        .map(|entry| Change {
            path: String::from_utf8_lossy(&entry[3..]).into_owned(),
            new: entry.starts_with(b"??"),
        })
        .collect()
}

/// The most files a commit's sentence names one by one; the rest are counted.
const NAMED: usize = 50;

/// The files as the commit's sentence names them: new ones marked, past [`NAMED`] counted.
fn listed(files: &[Change]) -> String {
    let mut names: Vec<String> = (files.iter().take(NAMED))
        .map(|c| {
            if c.new {
                format!("{} (new)", c.path)
            } else {
                c.path.clone()
            }
        })
        .collect();
    if files.len() > NAMED {
        names.push(format!("and {} more", files.len() - NAMED));
    }
    names.join(", ")
}

/// obid's sentence for `action` in `place` now, what the owner signs, and for a commit the files
/// it takes; a conflict when finishing can't take that action there now. The folder is named by
/// its full path, so two folders of one name can't be told apart wrongly.
async fn what(
    action: Action,
    place: &Place,
    state: &crate::git::State,
) -> Result<(String, Vec<Change>), ApiError> {
    if !crate::folders::finishing(state).contains(&action) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "That can't be done there now. Look at it again.",
        ));
    }
    let path = place.folder.to_string_lossy().into_owned();
    let branch = state.branch.clone().unwrap_or_default();
    let remote = state.remote.clone().unwrap_or_default();
    let mut files = Vec::new();
    let sentence = match action {
        Action::Commit => {
            let dir = place.path.as_deref().unwrap_or(place.folder.as_path());
            files = changes(dir)
                .await
                .map_err(|why| ApiError::new(StatusCode::CONFLICT, why))?;
            if files.is_empty() {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    format!("There's nothing to commit in {path}."),
                ));
            }
            let n = files.len();
            let changes = if n == 1 { "change" } else { "changes" };
            format!(
                "Commit {n} {changes} in {path} on {branch}: {}.",
                listed(&files)
            )
        }
        Action::Push => format!("Push {branch} from {path} to {remote}."),
        Action::PullRequest => format!("Open a pull request from {branch} in {path}."),
        Action::PublishRepository => {
            format!("Publish {path} as a new private repository on GitHub, with {branch}.")
        }
        Action::InitializeGit => format!("Make {path} a git repository."),
    };
    Ok((sentence, files))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Finish {
    folder: String,
    finish: Action,
    approval: crate::approval::Approval,
}

/// `POST /v1/conversations/{id}/workspace`: does one finishing action in one of the conversation's
/// folders, on the owner's signed click for obid's own sentence. What leaves the Mac (a push, a pull
/// request, a published repository) passes the secret scan first; a finding, or a scan that can't
/// run, sends nothing. Answers the workspace as `GET` does.
async fn finish(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    scope: Option<axum::Extension<crate::profiles::Scope>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let asked: Finish = serde_json::from_slice(&body).map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "The body is {\"folder\": \"/path\", \"finish\": an action, \"approval\": {\"device\", \"action\", \"signedAt\", \"signature\"}}.",
        )
    })?;
    held(&shared, &id, scope)?;
    let (place, state) = find(&shared, &id, &asked.folder).await?;
    let (sentence, files) = what(asked.finish, &place, &state).await?;
    let key = shared.devices.public_key(&asked.approval.device);
    shared
        .jobs
        .approvals()
        .check_finish(
            &asked.approval,
            &id,
            &sentence,
            key.as_deref(),
            obi_protocol::time::now(),
        )
        .map_err(|why| ApiError::new(StatusCode::FORBIDDEN, why))?;
    let Some(dir) = place.path.as_deref() else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "There's no place to do it.",
        ));
    };
    act(&shared, asked.finish, dir, &place, &state, &files)
        .await
        .map_err(|why| ApiError::new(StatusCode::CONFLICT, why))?;
    Ok(answer(seen(&shared, &id).await))
}

/// The message a commit made from Oby's finishing gets.
const MESSAGE: &str = "Changes made with Oby";

async fn act(
    shared: &Shared,
    action: Action,
    dir: &Path,
    place: &Place,
    state: &crate::git::State,
    files: &[Change],
) -> Result<(), String> {
    let git = |args: &'static [&'static str]| async move {
        crate::git::git(dir, args)
            .await
            .map(|_| ())
            .map_err(|e| e.0)
    };
    match action {
        Action::InitializeGit => git(&["init", "-q"]).await,
        Action::Commit => commit(dir, files).await,
        Action::Push => push(shared, dir, state).await,
        Action::PullRequest => {
            if state.ahead > 0 || !state.upstream {
                push(shared, dir, state).await?;
            }
            pull_request(dir, state).await
        }
        Action::PublishRepository => publish_repository(shared, dir, place, state).await,
    }
}

/// Commits exactly `files`, the ones the owner's sentence named, as T3 Code commits the files its
/// dialog shows (`git add -A` of those paths, `GitVcsDriverCore.ts`): never anything else of the
/// folder, nor of its repository outside it. The owner's hooks don't run (obid's git never runs a
/// hook, and `--no-verify` says it again); their name, email and signing settings stay theirs.
async fn commit(dir: &Path, files: &[Change]) -> Result<(), String> {
    if files.is_empty() {
        return Err("There's nothing to commit.".into());
    }
    let mut paths = Vec::new();
    for change in files {
        paths.extend_from_slice(format!(":(top,literal){}", change.path).as_bytes());
        paths.push(0);
    }
    let add = ["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"];
    let add: Vec<&OsStr> = add.iter().map(OsStr::new).collect();
    crate::git::run_bytes(dir, &add, &[], Some(paths))
        .await
        .map_err(|e| e.0)?;
    crate::git::git(dir, &["commit", "-q", "--no-verify", "-m", MESSAGE])
        .await
        .map(|_| ())
        .map_err(|e| e.0)
}

/// What the scan writes its texts in: obid's own, never the Mac's shared temporary folder.
fn scratch(shared: &Shared) -> Result<PathBuf, String> {
    let scratch = worktrees(&shared.config.home).join("scan");
    crate::store::make_private_dir(&scratch).map_err(|e| e.to_string())?;
    Ok(scratch)
}

/// A bare repository of obid's own that borrows a folder's objects (`objects/info/alternates`):
/// what finishing scans and sends is read and pushed from it, so none of the folder's settings,
/// hooks or attributes takes part. Removed when dropped.
struct Borrowed(PathBuf);

impl Drop for Borrowed {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn borrow(shared: &Shared, dir: &Path) -> Result<Borrowed, String> {
    let common = crate::git::git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await
    .map_err(|e| e.0)?;
    let objects = Path::new(&common).join("objects");
    if !objects.is_dir() {
        return Err("its repository's objects aren't there".into());
    }
    let format = crate::git::git(dir, &["rev-parse", "--show-object-format"])
        .await
        .unwrap_or_else(|_| "sha1".into());
    let scratch = scratch(shared)?;
    let path = scratch.join(format!("borrowed-{}", uuid::Uuid::new_v4()));
    let borrowed = Borrowed(path.clone());
    crate::git::git(
        &scratch,
        &[
            "init",
            "-q",
            "--bare",
            "--template=",
            &format!("--object-format={format}"),
            &path.to_string_lossy(),
        ],
    )
    .await
    .map_err(|e| e.0)?;
    let info = path.join("objects/info");
    std::fs::create_dir_all(&info).map_err(|e| e.to_string())?;
    std::fs::write(info.join("alternates"), format!("{}\n", objects.display()))
        .map_err(|e| e.to_string())?;
    Ok(borrowed)
}

/// The commit `dir`'s `HEAD` is now, by its id: what is scanned, and exactly what is sent.
async fn head(dir: &Path) -> Result<String, String> {
    let head = crate::git::git(dir, &["rev-parse", "--verify", "-q", "HEAD^{commit}"])
        .await
        .map_err(|_| "There's no commit to send.".to_owned())?;
    if crate::git::is_sha(&head) {
        Ok(head)
    } else {
        Err("There's no commit to send.".into())
    }
}

/// Scans what `head` adds since `since` (none: its whole history), then sends exactly `head` to
/// `branch` on `url`, both from a [`Borrowed`] repository: the commit pushed is the commit scanned.
async fn send(
    shared: &Shared,
    dir: &Path,
    url: &str,
    since: Option<&str>,
    head: &str,
    branch: &str,
) -> Result<(), String> {
    let borrowed = borrow(shared, dir).await?;
    crate::publish::scan_folder(&borrowed.0, since, head, &scratch(shared)?).await?;
    crate::publish::push_out(&borrowed.0, url, head, branch).await
}

/// After a push of `head` to `branch` on `remote`: the folder's own record of it, as a push by
/// name would have written it (the remote's branch, and its upstream when it had none).
async fn pushed(dir: &Path, remote: &str, branch: &str, head: &str, upstream: bool) {
    let tracking = format!("refs/remotes/{remote}/{branch}");
    let _ = crate::git::git(dir, &["update-ref", &tracking, head]).await;
    if !upstream {
        let key = |k: &str| format!("branch.{branch}.{k}");
        let _ = crate::git::git(dir, &["config", &key("remote"), remote]).await;
        let merge = format!("refs/heads/{branch}");
        let _ = crate::git::git(dir, &["config", &key("merge"), &merge]).await;
    }
}

/// Pushes `dir`'s branch to its remote, its upstream set, once the secret scan of every commit the
/// remote hasn't is clean: exactly the commit scanned, `<sha>:refs/heads/<branch>`.
async fn push(shared: &Shared, dir: &Path, state: &crate::git::State) -> Result<(), String> {
    let (Some(remote), Some(branch)) = (&state.remote, &state.branch) else {
        return Err("There's no branch or remote to push to.".into());
    };
    let head = head(dir).await?;
    let since = (if state.upstream {
        crate::git::git(dir, &["rev-parse", "--verify", "-q", "@{upstream}"])
            .await
            .ok()
    } else {
        match &state.default_branch {
            Some(default) => {
                let default = format!("refs/remotes/{remote}/{default}");
                crate::git::git(dir, &["merge-base", &default, &head])
                    .await
                    .ok()
            }
            None => None,
        }
    })
    .filter(|since| crate::git::is_sha(since));
    let url = crate::git::git(dir, &["remote", "get-url", "--push", remote])
        .await
        .map_err(|e| e.0)?;
    send(shared, dir, &url, since.as_deref(), &head, branch).await?;
    pushed(dir, remote, branch, &head, state.upstream).await;
    Ok(())
}

/// The GitHub CLI, or why finishing can't reach GitHub.
fn gh() -> Result<PathBuf, String> {
    crate::github::gh().ok_or_else(|| "This needs GitHub: sign in to GitHub in Settings.".into())
}

/// Opens a pull request from `dir`'s branch into its remote's default branch, titled after its
/// last commit.
async fn pull_request(dir: &Path, state: &crate::git::State) -> Result<(), String> {
    let (Some(remote), Some(branch)) = (&state.remote, &state.branch) else {
        return Err("There's no branch to open a pull request from.".into());
    };
    let url = crate::git::git(dir, &["remote", "get-url", remote])
        .await
        .map_err(|e| e.0)?;
    let repo = github_repo(&url).ok_or("Pull requests are opened on GitHub only for now.")?;
    let title = crate::git::git(dir, &["log", "-1", "--format=%s"])
        .await
        .map_err(|e| e.0)?;
    let mut args = vec![
        "pr", "create", "--repo", &repo, "--head", branch, "--title", &title,
    ];
    args.extend(["--body", "Opened from Oby."]);
    if let Some(default) = &state.default_branch {
        args.extend(["--base", default]);
    }
    crate::github::output(&gh()?, &args)
        .await
        .map(|_| ())
        .map_err(|(_, said)| format!("The branch is pushed, but no pull request opened: {said}"))
}

/// Makes a private repository on GitHub named after the folder, once the secret scan of its whole
/// history is clean; adds it as `origin`, then sends exactly the commit scanned with obid's own git
/// (`gh repo create` without `--push`, so gh runs no git of the folder's).
async fn publish_repository(
    shared: &Shared,
    dir: &Path,
    place: &Place,
    state: &crate::git::State,
) -> Result<(), String> {
    if state.remote.is_some() {
        return Err("It has a remote already.".into());
    }
    let Some(branch) = &state.branch else {
        return Err("There's no branch to publish.".into());
    };
    let gh = gh()?;
    let head = head(dir).await?;
    // The scan first, from obid's own repository: nothing is made on GitHub when it isn't clean.
    {
        let borrowed = borrow(shared, dir).await?;
        crate::publish::scan_folder(&borrowed.0, None, &head, &scratch(shared)?).await?;
    }
    let name = repository_name(crate::folders::name(&place.folder.to_string_lossy()));
    let made = crate::github::output(&gh, &["repo", "create", &name, "--private"])
        .await
        .map_err(|(_, said)| format!("Not published: {said}"))?;
    let url = (made.lines().rev())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_owned();
    let file = shared.config.allow_file_remotes && url.starts_with("file://");
    if !file && (!url.starts_with("https://github.com/") || github_repo(&url).is_none()) {
        return Err(format!(
            "Not published: GitHub didn't say where the repository is ({url})."
        ));
    }
    crate::git::git(dir, &["remote", "add", "origin", &url])
        .await
        .map_err(|e| format!("Not published: {}", e.said()))?;
    send(shared, dir, &url, None, &head, branch).await?;
    pushed(dir, "origin", branch, &head, false).await;
    // GitHub's default branch is the first one pushed: the folder's record says so too.
    let default = format!("refs/remotes/origin/{branch}");
    let _ = crate::git::git(dir, &["symbolic-ref", "refs/remotes/origin/HEAD", &default]).await;
    Ok(())
}

/// A folder's name as a GitHub repository's: letters, digits, `.`, `-` and `_`, anything else `-`.
fn repository_name(folder: &str) -> String {
    let name: String = folder
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches(|c| c == '-' || c == '.');
    if name.is_empty() {
        "project".into()
    } else {
        name.to_owned()
    }
}

/// `owner/name` of a GitHub remote's URL (`https://github.com/o/n(.git)`, `git@github.com:o/n`,
/// `ssh://git@github.com/o/n`), or none for another host.
fn github_repo(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("git@github.com:"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let (owner, name) = rest.split_once('/')?;
    let fine = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    };
    (fine(owner) && fine(name)).then(|| format!("{owner}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_github_remote_names_its_repository() {
        for url in [
            "https://github.com/me/api.git",
            "https://github.com/me/api",
            "git@github.com:me/api.git",
            "ssh://git@github.com/me/api",
        ] {
            assert_eq!(github_repo(url).as_deref(), Some("me/api"), "{url}");
        }
        assert_eq!(github_repo("https://gitlab.com/me/api"), None);
        assert_eq!(github_repo("https://github.com/me/a/b"), None);
    }

    #[test]
    fn a_repository_is_named_after_its_folder() {
        assert_eq!(repository_name("My Notes"), "My-Notes");
        assert_eq!(repository_name("api"), "api");
        assert_eq!(repository_name("…"), "project");
    }

    #[test]
    fn a_worktree_is_merged_only_with_its_own_commits_on_the_default_branch() {
        let clean = crate::git::State::default();
        assert!(merged(&clean, "b", "a", true));
        assert!(!merged(&clean, "a", "a", true), "nothing of its own yet");
        assert!(
            !merged(&clean, "b", "a", false),
            "not on the default branch"
        );
        let changed = crate::git::State {
            changes: true,
            ..clean
        };
        assert!(!merged(&changed, "b", "a", true), "uncommitted work stays");
    }

    #[test]
    fn workspace_4_a_commit_names_each_file_git_shows() {
        let out = b" M api/main.rs\0?? api/todo.md\0D  api/old.md\0";
        let files = parse_status(out);
        assert_eq!(
            files,
            vec![
                Change {
                    path: "api/main.rs".into(),
                    new: false
                },
                Change {
                    path: "api/todo.md".into(),
                    new: true
                },
                Change {
                    path: "api/old.md".into(),
                    new: false
                },
            ]
        );
        assert_eq!(listed(&files), "api/main.rs, api/todo.md (new), api/old.md");
        let many: Vec<Change> = (0..NAMED + 2)
            .map(|n| Change {
                path: format!("f{n}"),
                new: false,
            })
            .collect();
        assert!(listed(&many).ends_with(&format!("f{}, and 2 more", NAMED - 1)));
    }

    #[test]
    fn a_conversations_id_is_a_file_name() {
        assert!(is_id("0b8e0c1a-1f2e-4c3d-9a8b-7c6d5e4f3a2b"));
        for id in ["", "../x", "a/b", "a.json", "."] {
            assert!(!is_id(id), "{id}");
        }
    }
}
