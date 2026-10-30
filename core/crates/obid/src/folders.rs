//! The folders the owner works in (docs/contracts/folders.md): the message box's Select working
//! folder… on this Mac. Every path is checked here, at the boundary, before anything of the request
//! happens: absolute, canonicalized (links resolved, `..` gone), a folder that's there, and none of
//! the places Oby never works in. The canonical path is what a conversation keeps.

use std::path::{Path, PathBuf};

use axum::http::StatusCode;

use crate::api::{ApiError, Shared};

/// The most folders a conversation keeps.
pub const MOST: usize = 20;

/// The system's own folders: refused, and everything inside them. A folder on another volume
/// (`/Volumes/X/…`) is fine; `/Volumes` itself isn't ([`refused`]).
const SYSTEM: &[&str] = &[
    "/System",
    "/Library",
    "/Applications",
    "/usr",
    "/bin",
    "/sbin",
    "/etc",
    "/var",
    "/private",
    "/opt",
    "/cores",
    "/dev",
];

/// The places a folder is checked against, besides the system's own.
pub struct Places {
    /// The person's home folders (crate::jobs::person_homes).
    pub homes: Vec<PathBuf>,
    /// obid's home.
    pub obid: PathBuf,
    /// Every account's own folder (`configDir`).
    pub accounts: Vec<PathBuf>,
}

impl Places {
    pub fn of(shared: &Shared) -> Places {
        let agents = shared.config.agents.get();
        Places {
            homes: crate::jobs::person_homes(),
            obid: shared.config.home.clone(),
            accounts: [agents.claude, agents.codex, agents.cursor]
                .into_iter()
                .flatten()
                .flat_map(|agent| agent.accounts)
                .filter_map(|account| account.config_dir)
                .collect(),
        }
    }
}

/// `paths`, each checked and canonicalized, the same folder kept once, in the order given; or `422`
/// naming the first that's refused and why.
pub fn checked(places: &Places, paths: &[String]) -> Result<Vec<String>, ApiError> {
    let mut kept: Vec<String> = Vec::new();
    for path in paths {
        let real = check(places, path)
            .map_err(|why| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why))?;
        if !kept.iter().any(|k| same(k, &real)) {
            kept.push(real);
        }
    }
    if kept.len() > MOST {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("A conversation works in at most {MOST} folders."),
        ));
    }
    Ok(kept)
}

/// `a` and `b` are the same folder: compared case-insensitively on macOS, whose disks ignore case
/// by default (`~/LIBRARY` is `~/Library`).
pub fn same(a: &str, b: &str) -> bool {
    key(Path::new(a)) == key(Path::new(b))
}

/// The folder `path`, canonicalized, or why it's refused, naming it by its name.
pub fn check(places: &Places, path: &str) -> Result<String, String> {
    let name = name(path);
    if !Path::new(path).is_absolute() {
        return Err(format!("\"{name}\" isn't a folder's full path."));
    }
    let real = std::fs::canonicalize(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => format!(
            "Oby can't read \"{name}\": macOS hasn't let it in. Allow it in System Settings › Privacy & Security › Files and Folders."
        ),
        _ => format!("\"{name}\" isn't at {path}."),
    })?;
    if !real.is_dir() {
        return Err(format!("\"{name}\" is a file, not a folder."));
    }
    if let Some(why) = refused(places, &real) {
        return Err(format!("\"{name}\" {why}"));
    }
    real.into_os_string()
        .into_string()
        .map_err(|_| format!("\"{name}\" has a name Oby can't read."))
}

/// A folder's name as its tag shows it (the app's `folderName`).
pub fn name(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    trimmed
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(path)
}

/// Why Oby never works in the canonical folder `real`, or none.
fn refused(places: &Places, real: &Path) -> Option<&'static str> {
    let real = key(real);
    let canonical = |p: &Path| key(&std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
    if real == "/" {
        return Some("is your whole disk: pick a folder inside it.");
    }
    if real == key(Path::new("/Volumes")) {
        return Some("holds your Mac's disks: pick a folder on one.");
    }
    for home in &places.homes {
        let home = std::fs::canonicalize(home).unwrap_or_else(|_| home.clone());
        if real == key(&home) {
            return Some("is your whole home folder: pick a folder inside it.");
        }
        if within(&real, &key(&home.join("Library"))) {
            return Some("is where your Mac keeps its own settings, so Oby doesn't work there.");
        }
        for login in [".claude", ".codex", ".cursor"] {
            if within(&real, &key(&home.join(login))) {
                return Some("holds an agent's login, so Oby doesn't work there.");
            }
        }
    }
    for root in SYSTEM {
        if within(&real, &key(Path::new(root))) {
            return Some("is where your Mac keeps its own files, so Oby doesn't work there.");
        }
    }
    let obid = canonical(&places.obid);
    if within(&real, &obid) || within(&obid, &real) {
        return Some("holds Oby's own files, so Oby doesn't work there.");
    }
    for account in &places.accounts {
        let account = canonical(account);
        if within(&real, &account) || within(&account, &real) {
            return Some("holds an agent's login, so Oby doesn't work there.");
        }
    }
    None
}

/// `path` is `root` or inside it.
fn within(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// How two paths are compared: lower-cased on macOS.
fn key(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    if cfg!(target_os = "macos") {
        text.to_lowercase()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_is_by_whole_components() {
        assert!(within("/usr", "/usr"));
        assert!(within("/usr/local", "/usr"));
        assert!(!within("/usrx", "/usr"));
    }

    #[test]
    fn a_name_is_the_last_component() {
        assert_eq!(name("/Users/me/Projects/api/"), "api");
        assert_eq!(name("/"), "/");
        assert_eq!(name("api"), "api");
    }
}
