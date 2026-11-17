//! `GET /v1/conversations/{id}/files?q=`: the files of a conversation's working folders whose path
//! holds what the owner typed after an `@` in its message box (docs/contracts/composer.md,
//! COMPOSER-14 and 15). It reads only names, only in the folders obid keeps for that conversation
//! (its own, or its project's), never a path the request names; it keeps nothing and writes
//! nothing. The owner's Mac app asks it; the gateway and the phone relay don't serve it.

use std::path::Path;
use std::sync::Arc;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::api::{ApiError, Shared};

/// The most names read in one folder: past it, the rest isn't searched.
pub const PER_FOLDER: usize = 5000;
/// The most matches answered.
pub const MOST: usize = 20;
/// Folders a walk (a folder with no git) never goes into: dependencies and builds.
const SKIPPED: [&str; 5] = ["node_modules", "target", "dist", "build", "__pycache__"];

#[derive(Deserialize)]
pub struct Search {
    #[serde(default)]
    q: String,
}

pub async fn search(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    scope: Option<axum::Extension<crate::profiles::Scope>>,
    Query(search): Query<Search>,
) -> Result<Response, ApiError> {
    let scope = crate::profile_scope::of(&shared, scope);
    let gone = || ApiError::new(StatusCode::NOT_FOUND, "There's no such conversation.");
    // Another profile's conversation is none of this one's (B00-40).
    let profile = shared.conversations.profile_of(&id).ok_or_else(gone)?;
    if !scope.holds(profile.as_deref()) {
        return Err(gone());
    }
    let folders = shared.conversations.working_folders(&id).ok_or_else(gone)?;
    let q = search.q.trim().to_lowercase();
    let files = tokio::task::spawn_blocking(move || found(&folders, &q))
        .await
        .unwrap_or_default();
    Ok((StatusCode::OK, axum::Json(json!({ "files": files }))).into_response())
}

/// The best [`MOST`] matches of `q` in `folders`, each `{folder, path}` (its path from the
/// folder, with `/`): a name starting with `q` first, then a name holding it, then a path
/// holding it; shorter paths first within each. Nothing typed: the shortest paths.
pub fn found(folders: &[String], q: &str) -> Vec<serde_json::Value> {
    let mut ranked: Vec<(u8, usize, String, String)> = vec![];
    for folder in folders {
        for path in names(Path::new(folder)) {
            let lower = path.to_lowercase();
            let name = lower.rsplit('/').next().unwrap_or(&lower);
            let rank = if q.is_empty() || name.starts_with(q) {
                0
            } else if name.contains(q) {
                1
            } else if lower.contains(q) {
                2
            } else {
                continue;
            };
            ranked.push((rank, path.len(), folder.clone(), path));
        }
    }
    ranked.sort();
    ranked
        .into_iter()
        .take(MOST)
        .map(|(_, _, folder, path)| json!({"folder": folder, "path": path}))
        .collect()
}

/// The files of one folder, at most [`PER_FOLDER`]: git's own list in a repository (tracked, and
/// untracked but not ignored), else a walk that skips hidden folders and [`SKIPPED`].
fn names(folder: &Path) -> Vec<String> {
    let listed = std::process::Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    if let Ok(out) = listed
        && out.status.success()
    {
        return (out.stdout.split(|b| *b == 0))
            .filter(|p| !p.is_empty())
            .take(PER_FOLDER)
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect();
    }
    let mut out = vec![];
    walk(folder, "", &mut out);
    out
}

fn walk(dir: &Path, under: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= PER_FOLDER {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if under.is_empty() { name.clone() } else { format!("{under}/{name}") };
        // A link is never followed: it could lead anywhere on the Mac.
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if !name.starts_with('.') && !SKIPPED.contains(&name.as_str()) {
                walk(&entry.path(), &path, out);
            }
        } else if kind.is_file() {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("obid-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// COMPOSER-15: a folder with no git is walked, hidden folders, dependencies and links left
    /// out; names that start with what's typed come first.
    #[test]
    fn composer_15_files_of_a_folder_match_what_is_typed() {
        let dir = scratch("walk");
        let root = dir.as_path();
        for (path, body) in [
            ("src/app.ts", "x"),
            ("src/lib/apply.ts", "x"),
            ("docs/map.md", "x"),
            ("node_modules/app/index.js", "x"),
            (".git-ish/app.ts", "x"),
        ] {
            let at = root.join(path);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(at, body).unwrap();
        }
        std::os::unix::fs::symlink("/etc", root.join("etc-link")).unwrap();
        let folder = root.to_string_lossy().into_owned();
        let paths = |q: &str| -> Vec<String> {
            found(std::slice::from_ref(&folder), q).iter().map(|f| f["path"].as_str().unwrap().to_owned()).collect()
        };
        assert_eq!(paths("app"), ["src/app.ts", "src/lib/apply.ts"]);
        assert_eq!(paths("map"), ["docs/map.md"]);
        assert_eq!(paths("lib"), ["src/lib/apply.ts"]);
        assert!(paths("index").is_empty(), "node_modules is skipped");
        assert!(paths("passwd").is_empty(), "a link isn't followed");
        assert_eq!(paths("").len(), 3);
        assert_eq!(found(std::slice::from_ref(&folder), "app")[0]["folder"], folder.as_str());
    }

    /// COMPOSER-15: in a repository, git's own list: ignored files aren't offered.
    #[test]
    fn composer_15_a_repository_offers_what_git_does() {
        let dir = scratch("git");
        let root = dir.as_path();
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(root).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        std::fs::write(root.join(".gitignore"), "secret.env\n").unwrap();
        std::fs::write(root.join("secret.env"), "x").unwrap();
        std::fs::write(root.join("server.rs"), "x").unwrap();
        let folder = root.to_string_lossy().into_owned();
        let paths: Vec<_> = found(&[folder], "se").iter().map(|f| f["path"].as_str().unwrap().to_owned()).collect();
        assert_eq!(paths, ["server.rs"]);
    }
}
