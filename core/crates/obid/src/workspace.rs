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
    /// Where its work happens: the folder itself, or its worktree.
    pub path: PathBuf,
    /// The worktree's branch; none in place.
    pub branch: Option<String>,
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

fn read(home: &Path, id: &str) -> Kept {
    std::fs::read(file(home, id))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write(home: &Path, id: &str, kept: &Kept) -> Result<(), String> {
    crate::store::make_private_dir(&worktrees(home)).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(kept).map_err(|e| e.to_string())?;
    crate::store::write_private(&file(home, id), &bytes).map_err(|e| e.to_string())
}

/// The workspace conversation `id` chose: `Folder` when it chose none.
pub fn chosen(home: &Path, id: &str) -> Workspace {
    if is_id(id) {
        read(home, id).workspace
    } else {
        Workspace::Folder
    }
}

/// Keeps a new conversation's choice, sent with its first message (WORKSPACE-3), and makes its
/// worktrees now. A worktree that can't be made is said in obid's log; the next look tries again.
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
/// have none yet (a folder added later, or one that became a repository).
pub async fn places(shared: &Shared, id: &str) -> Vec<Place> {
    let folders = shared.conversations.working_folders(id).unwrap_or_default();
    if !is_id(id) {
        return vec![];
    }
    let home = &shared.config.home;
    let _held = LOCK.lock().await;
    let mut kept = read(home, id);
    if kept.workspace == Workspace::Worktree {
        let mut made = false;
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
                    made = true;
                }
                Ok(None) => {}
                Err(why) => tracing::warn!("conversation {id}: no worktree for a folder: {why}"),
            }
        }
        if made && let Err(why) = write(home, id, &kept) {
            tracing::warn!("conversation {id}: its worktrees aren't kept: {why}");
        }
    }
    folders
        .iter()
        .map(|folder| {
            let worktree = (kept.worktrees.iter())
                .find(|w| crate::folders::same(&w.folder, folder) && Path::new(&w.root).is_dir());
            Place {
                folder: PathBuf::from(folder),
                path: PathBuf::from(worktree.map_or(folder, |w| &w.path)),
                branch: worktree.map(|w| w.branch.clone()),
            }
        })
        .collect()
}

/// A worktree of `folder`'s repository for conversation `id`, on a new branch `oby/<id's start>`
/// at the folder's `HEAD`; none for a folder that isn't a repository with a commit (it works in
/// place, as T3's `resolveSendEnvMode` decides). The folder's files and branches don't change.
async fn make(home: &Path, id: &str, folder: &Path) -> Result<Option<Worktree>, String> {
    let state = crate::git::state(folder).await;
    if !state.repo || !state.committed {
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
    Ok(Some(Worktree {
        folder: folder.to_string_lossy().into_owned(),
        path: root.join(inside).to_string_lossy().into_owned(),
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
    for worktree in read(home, id).worktrees {
        remove(&worktree).await;
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
        let mut kept = read(home, &id);
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
        let sentence = sentence(action, &place, &state)?;
        return Ok(answer(json!({"action": sentence})));
    }
    Ok(answer(seen(&shared, &id).await))
}

/// The workspace and its places as the app shows them.
async fn seen(shared: &Shared, id: &str) -> Value {
    let mut out = Vec::new();
    for place in places(shared, id).await {
        let state = crate::git::state(&place.path).await;
        out.push(json!({
            "folder": place.folder,
            "name": crate::folders::name(&place.folder.to_string_lossy()),
            "path": place.path,
            "worktree": place.branch.is_some(),
            "branch": state.branch,
            "actions": crate::folders::finishing(&state),
        }));
    }
    json!({"workspace": chosen(&shared.config.home, id), "places": out})
}

fn answer(body: Value) -> Response {
    (StatusCode::OK, axum::Json(body)).into_response()
}

/// The conversation's place for `folder`, one of its own, and its git state now.
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
    let state = crate::git::state(&place.path).await;
    Ok((place, state))
}

/// obid's sentence for `action` in `place` now, what the owner signs; a conflict when finishing
/// can't take that action there now.
fn sentence(action: Action, place: &Place, state: &crate::git::State) -> Result<String, ApiError> {
    if !crate::folders::finishing(state).contains(&action) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "That can't be done there now. Look at it again.",
        ));
    }
    let name = crate::folders::name(&place.folder.to_string_lossy()).to_owned();
    let branch = state.branch.clone().unwrap_or_default();
    let remote = state.remote.clone().unwrap_or_default();
    Ok(match action {
        Action::Commit => format!("Commit every change in {name} on {branch}."),
        Action::Push => format!("Push {branch} from {name} to {remote}."),
        Action::PullRequest => format!("Open a pull request from {branch} in {name}."),
        Action::PublishRepository => {
            format!("Publish {name} as a new private repository on GitHub, with {branch}.")
        }
        Action::InitializeGit => format!("Make {name} a git repository."),
    })
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
    let sentence = sentence(asked.finish, &place, &state)?;
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
    act(&shared, asked.finish, &place, &state)
        .await
        .map_err(|why| ApiError::new(StatusCode::CONFLICT, why))?;
    Ok(answer(seen(&shared, &id).await))
}

/// The message a commit made from Oby's finishing gets.
const MESSAGE: &str = "Changes made with Oby";

async fn act(
    shared: &Shared,
    action: Action,
    place: &Place,
    state: &crate::git::State,
) -> Result<(), String> {
    let dir = place.path.as_path();
    let git = |args: &'static [&'static str]| async move {
        crate::git::git(dir, args)
            .await
            .map(|_| ())
            .map_err(|e| e.0)
    };
    match action {
        Action::InitializeGit => git(&["init", "-q"]).await,
        Action::Commit => {
            git(&["add", "-A"]).await?;
            crate::git::git(dir, &["commit", "-q", "-m", MESSAGE])
                .await
                .map(|_| ())
                .map_err(|e| e.0)
        }
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

/// What the scan writes its texts in: obid's own, never the Mac's shared temporary folder.
fn scratch(shared: &Shared) -> Result<PathBuf, String> {
    let scratch = worktrees(&shared.config.home).join("scan");
    crate::store::make_private_dir(&scratch).map_err(|e| e.to_string())?;
    Ok(scratch)
}

/// Pushes `dir`'s branch to its remote, its upstream set, once the secret scan of every commit the
/// remote hasn't is clean.
async fn push(shared: &Shared, dir: &Path, state: &crate::git::State) -> Result<(), String> {
    let (Some(remote), Some(branch)) = (&state.remote, &state.branch) else {
        return Err("There's no branch or remote to push to.".into());
    };
    let since = if state.upstream {
        crate::git::git(dir, &["rev-parse", "@{upstream}"])
            .await
            .ok()
    } else {
        let default = state
            .default_branch
            .as_ref()
            .map(|d| format!("{remote}/{d}"));
        match default {
            Some(default) => crate::git::git(dir, &["merge-base", &default, "HEAD"])
                .await
                .ok(),
            None => None,
        }
    };
    crate::publish::scan_folder(dir, since.as_deref(), &scratch(shared)?).await?;
    crate::git::git(
        dir,
        &[
            "push",
            "-q",
            "-u",
            remote,
            &format!("HEAD:refs/heads/{branch}"),
        ],
    )
    .await
    .map(|_| ())
    .map_err(|e| format!("Nothing was pushed: {}", e.said()))
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

/// Makes a private repository on GitHub named after the folder, with `dir` as its source and
/// `origin` its remote, and pushes it, once the secret scan of its whole history is clean.
async fn publish_repository(
    shared: &Shared,
    dir: &Path,
    place: &Place,
    state: &crate::git::State,
) -> Result<(), String> {
    if state.remote.is_some() {
        return Err("It has a remote already.".into());
    }
    let gh = gh()?;
    crate::publish::scan_folder(dir, None, &scratch(shared)?).await?;
    let name = repository_name(crate::folders::name(&place.folder.to_string_lossy()));
    let source = dir.to_string_lossy();
    let args = [
        "repo",
        "create",
        &name,
        "--private",
        "--source",
        &source,
        "--remote",
        "origin",
        "--push",
    ];
    crate::github::output(&gh, &args)
        .await
        .map(|_| ())
        .map_err(|(_, said)| format!("Not published: {said}"))
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
    fn a_conversations_id_is_a_file_name() {
        assert!(is_id("0b8e0c1a-1f2e-4c3d-9a8b-7c6d5e4f3a2b"));
        for id in ["", "../x", "a/b", "a.json", "."] {
            assert!(!is_id(id), "{id}");
        }
    }
}
