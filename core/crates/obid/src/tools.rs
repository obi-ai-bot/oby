//! The tools obid sets up itself: Cursor's agent (`cursor-agent`), when Connect finds none on this
//! Mac (docs/runner-api.md, "obid only: tools"). Claude Code and Codex stay the person's own
//! installs.
//!
//! obid never runs Cursor's install script (`curl https://cursor.com/install | bash`): it reads
//! that script for the package it names (`https://downloads.cursor.com/lab/<version>/<os>/<arch>/
//! agent-cli-package.tar.gz`), downloads that very package, and puts it where the script puts it
//! (`~/.local/share/cursor-agent/versions/<version>`, with `~/.local/bin/cursor-agent` and
//! `~/.local/bin/agent` leading to it), so the person's own `cursor-agent`, Cursor's own `update`
//! and the job sandbox (which lets that folder through, [`crate::sandbox::install_root`]) all see
//! one Cursor. Cursor publishes no checksum for the package: the download is over HTTPS from
//! `downloads.cursor.com` only, and nothing in it runs until a sign-in or a job starts it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::agent::Tool;
use crate::api::{ApiError, Shared};

/// Where Cursor's install script is, which names the package to download. obid's tests point
/// `OBID_CURSOR_INSTALLER` at a stand-in on 127.0.0.1.
const INSTALLER: &str = "https://cursor.com/install";
/// The only site a package is taken from.
const PACKAGE_HOST: &str = "downloads.cursor.com";
/// The most the install script may be.
const SCRIPT_MOST: usize = 1 << 20;
/// The most a package may be: Cursor's is about 180 MB (2026.10.22).
const PACKAGE_MOST: u64 = 1 << 30;
/// How long reading the script may take.
const SCRIPT_WITHIN: Duration = Duration::from_secs(30);
/// How long the download may take.
const DOWNLOAD_WITHIN: Duration = Duration::from_secs(15 * 60);
/// How long unpacking may take.
const UNPACK_WITHIN: Duration = Duration::from_secs(5 * 60);

/// What obid is setting up, by tool.
#[derive(Default)]
pub struct Tools {
    cursor: Mutex<Progress>,
}

/// Where a set-up stands: none under way, one running, or why the last one failed (until the next).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Progress {
    #[default]
    Idle,
    Installing,
    Failed(String),
}

impl Tools {
    fn cursor(&self) -> Progress {
        self.cursor
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set_cursor(&self, progress: Progress) {
        *self.cursor.lock().unwrap_or_else(|e| e.into_inner()) = progress;
    }
}

/// The name Cursor's binary is looked for by: the configuration's `agents.cursor.bin`, else
/// `cursor-agent` (crate::accounts, `default_bin`).
fn cursor_bin(shared: &Shared) -> String {
    shared
        .config
        .agents
        .get()
        .cursor
        .map_or_else(|| "cursor-agent".to_owned(), |agent| agent.bin)
}

/// The version an installed Cursor is, when its binary leads into
/// `…/cursor-agent/versions/<version>/` (Cursor's installer's layout, and obid's).
fn installed_version(bin: &Path) -> Option<String> {
    let real = bin.canonicalize().ok()?;
    let folder = real.parent()?;
    (folder.parent()?.file_name()? == "versions")
        .then(|| folder.file_name()?.to_str().map(str::to_owned))
        .flatten()
}

/// `status.tools`: for Cursor, whether its agent is on this Mac (`installed`, with `path` and its
/// `version` when the install's layout says it), `missing`, `installing` (obid's set-up runs), or
/// `failed` (`error` says why, until the next set-up).
pub fn status_json(shared: &Shared) -> Value {
    let found = crate::process::resolve_bin(&cursor_bin(shared));
    let (state, error) = match (&found, shared.tools.cursor()) {
        (_, Progress::Installing) => ("installing", None),
        (Some(_), _) => ("installed", None),
        (None, Progress::Failed(why)) => ("failed", Some(why)),
        (None, Progress::Idle) => ("missing", None),
    };
    json!({"cursor": {
        "state": state,
        "path": found.as_deref().map(Path::display).map(|p| p.to_string()),
        "version": found.as_deref().and_then(installed_version),
        "error": error,
    }})
}

/// `POST /v1/tools/{tool}/install`: sets Cursor's agent up on this Mac, in the background.
/// `202` with `status.tools` as it is then; `200` when the agent is there already; `409` while a
/// set-up is under way, or when the configuration names Cursor's binary by a path that isn't
/// there (obid installs only where the default name is found); `404` for any other tool. When it
/// ends, every app gets `tools` (docs/runner-api.md, "GET /v1/events").
pub async fn install(
    State(shared): State<Arc<Shared>>,
    UrlPath(tool): UrlPath<String>,
) -> Result<Response, ApiError> {
    if Tool::from_id(&tool) != Some(Tool::Cursor) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("obid sets up Cursor's agent only, not {tool:?}."),
        ));
    }
    let bin = cursor_bin(&shared);
    if crate::process::resolve_bin(&bin).is_some() {
        return Ok((StatusCode::OK, axum::Json(status_json(&shared))).into_response());
    }
    if bin.contains('/') {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!(
                "obid's configuration names Cursor's binary as {bin}, which isn't there: fix that path, or take agents.cursor.bin out so obid can set Cursor's agent up."
            ),
        ));
    }
    {
        let mut progress = shared
            .tools
            .cursor
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *progress == Progress::Installing {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "Cursor's agent is being set up: a moment.".to_owned(),
            ));
        }
        *progress = Progress::Installing;
    }
    tracing::info!("Cursor's agent: set-up started");
    let task = shared.clone();
    tokio::spawn(async move {
        let ended = set_up().await;
        match &ended {
            Ok(version) => tracing::info!("Cursor's agent: {version} set up"),
            Err(why) => tracing::warn!("Cursor's agent: set-up failed: {why}"),
        }
        task.tools.set_cursor(match ended {
            Ok(_) => Progress::Idle,
            Err(why) => Progress::Failed(why),
        });
        task.events.send("tools", status_json(&task));
    });
    Ok((StatusCode::ACCEPTED, axum::Json(status_json(&shared))).into_response())
}

/// Why a Cursor sign-in or job can't start without the agent, with what to do.
pub const CURSOR_MISSING: &str = "Cursor's agent isn't on this Mac yet: Connect sets it up (POST /v1/tools/cursor/install), then signs in.";

/// Downloads Cursor's package and puts it in place (the module's notes). The version set up.
async fn set_up() -> Result<String, String> {
    let home = crate::jobs::login_home()
        .ok_or("obid can't find the person's home to put Cursor's agent in.")?;
    let installer = std::env::var("OBID_CURSOR_INSTALLER").ok();
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_WITHIN)
        .build()
        .map_err(|e| format!("obid can't make an HTTP client: {e}"))?;
    let (version, url) = package(&client, installer.as_deref()).await?;
    let versions = home.join(".local/share/cursor-agent/versions");
    std::fs::create_dir_all(&versions)
        .map_err(|e| format!("obid can't make {}: {e}", versions.display()))?;
    let staging = versions.join(format!(".tmp-{version}-obid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir(&staging)
        .map_err(|e| format!("obid can't make {}: {e}", staging.display()))?;
    let unpacked = fetch_and_unpack(&client, &url, &staging).await;
    let agent = staging.join("cursor-agent");
    let fine = unpacked.and_then(|()| {
        crate::sandbox::is_executable(&agent)
            .then_some(())
            .ok_or_else(|| "the package has no cursor-agent in it.".to_owned())
    });
    if let Err(why) = fine {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(why);
    }
    let kept = versions.join(&version);
    let _ = std::fs::remove_dir_all(&kept);
    std::fs::rename(&staging, &kept).map_err(|e| {
        let _ = std::fs::remove_dir_all(&staging);
        format!("obid can't move the package to {}: {e}", kept.display())
    })?;
    let bin = home.join(".local/bin");
    std::fs::create_dir_all(&bin).map_err(|e| format!("obid can't make {}: {e}", bin.display()))?;
    for name in ["cursor-agent", "agent"] {
        let link = bin.join(name);
        match std::fs::symlink_metadata(&link) {
            Ok(meta) if meta.is_dir() => {
                return Err(format!("{} is a folder: move it aside.", link.display()));
            }
            Ok(_) => std::fs::remove_file(&link)
                .map_err(|e| format!("obid can't replace {}: {e}", link.display()))?,
            Err(_) => {}
        }
        std::os::unix::fs::symlink(kept.join("cursor-agent"), &link)
            .map_err(|e| format!("obid can't make {}: {e}", link.display()))?;
    }
    Ok(version)
}

/// The package Cursor's install script names for this machine: its version and its address.
async fn package(
    client: &reqwest::Client,
    installer: Option<&str>,
) -> Result<(String, String), String> {
    let script_url = installer.unwrap_or(INSTALLER);
    let answer = client
        .get(script_url)
        .timeout(SCRIPT_WITHIN)
        .send()
        .await
        .map_err(|e| format!("obid can't read Cursor's install script: {e}"))?;
    if !answer.status().is_success() {
        return Err(format!(
            "Cursor's install script answered {}.",
            answer.status()
        ));
    }
    let script = read_at_most(answer, SCRIPT_MOST as u64).await?;
    let script = String::from_utf8_lossy(&script);
    let (version, url) = package_in_script(&script, machine_os(), machine_arch())
        .ok_or("Cursor's install script names no package for this machine.")?;
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("a bad package address: {e}"))?;
    let host = parsed.host_str().unwrap_or_default();
    let allowed = match installer {
        // The stand-in's own site, for obid's tests.
        Some(stand_in) => reqwest::Url::parse(stand_in)
            .ok()
            .is_some_and(|s| s.host_str() == Some(host) && s.scheme() == parsed.scheme()),
        None => parsed.scheme() == "https" && host == PACKAGE_HOST,
    };
    if !allowed {
        return Err(format!(
            "Cursor's install script names a package on {host}, not {PACKAGE_HOST}: not taken."
        ));
    }
    Ok((version, url))
}

/// Cursor's install script's `DOWNLOAD_URL="https://downloads.cursor.com/lab/<version>/${OS}/
/// ${ARCH}/agent-cli-package.tar.gz"`, for `os` and `arch`: the version and the address.
pub fn package_in_script(script: &str, os: &str, arch: &str) -> Option<(String, String)> {
    let line = script
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("DOWNLOAD_URL="))?;
    let url = line
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .replace("${OS}", os)
        .replace("${ARCH}", arch);
    if url.contains("${") || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let version = url
        .split("/lab/")
        .nth(1)?
        .split('/')
        .next()
        .filter(|v| {
            !v.is_empty()
                && v.len() <= 64
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        })?
        .to_owned();
    Some((version, url))
}

/// This machine, as Cursor's packages name it.
fn machine_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

fn machine_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    }
}

/// An answer's body, up to `most` bytes; past that, an error.
async fn read_at_most(mut answer: reqwest::Response, most: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = answer
        .chunk()
        .await
        .map_err(|e| format!("the download broke off: {e}"))?
    {
        if bytes.len() as u64 + chunk.len() as u64 > most {
            return Err(format!("the download is past {most} bytes: not taken."));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Downloads the package at `url` into `staging` and unpacks it there (its one top folder
/// dropped, as Cursor's script does), with the system's `tar`.
async fn fetch_and_unpack(
    client: &reqwest::Client,
    url: &str,
    staging: &Path,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let file = staging.join("agent-cli-package.tar.gz");
    let mut answer = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("obid can't download Cursor's package: {e}"))?;
    if !answer.status().is_success() {
        return Err(format!("Cursor's package answered {}.", answer.status()));
    }
    let mut out = tokio::fs::File::create(&file)
        .await
        .map_err(|e| format!("obid can't write {}: {e}", file.display()))?;
    let mut got = 0_u64;
    while let Some(chunk) = answer
        .chunk()
        .await
        .map_err(|e| format!("the download broke off: {e}"))?
    {
        got += chunk.len() as u64;
        if got > PACKAGE_MOST {
            return Err(format!(
                "the package is past {PACKAGE_MOST} bytes: not taken."
            ));
        }
        out.write_all(&chunk)
            .await
            .map_err(|e| format!("obid can't write {}: {e}", file.display()))?;
    }
    out.flush().await.map_err(|e| e.to_string())?;
    drop(out);
    let tar = ["/usr/bin/tar", "/bin/tar"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| crate::sandbox::is_executable(p))
        .ok_or("obid can't find tar to unpack Cursor's package.")?;
    let unpack = tokio::process::Command::new(tar)
        .args(["--strip-components=1", "-xzf"])
        .arg(&file)
        .arg("-C")
        .arg(staging)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(UNPACK_WITHIN, unpack)
        .await
        .map_err(|_| "unpacking Cursor's package took too long.".to_owned())?
        .map_err(|e| format!("obid couldn't run tar: {e}"))?;
    let _ = std::fs::remove_file(&file);
    if !out.status.success() {
        return Err(format!(
            "tar couldn't unpack Cursor's package: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The script's `DOWNLOAD_URL` line (Cursor's installer of 2026-10-22), for this machine.
    #[test]
    fn the_package_is_read_off_cursors_install_script() {
        let script = "#!/usr/bin/env bash\nTEMP_EXTRACT_DIR=\"$HOME/.local/share/cursor-agent/versions/.tmp-2026.10.22-27dcf5f-$(date +%s)\"\nDOWNLOAD_URL=\"https://downloads.cursor.com/lab/2026.10.22-27dcf5f/${OS}/${ARCH}/agent-cli-package.tar.gz\"\necho \"${DOWNLOAD_URL}\"\n";
        assert_eq!(
            package_in_script(script, "darwin", "arm64"),
            Some((
                "2026.10.22-27dcf5f".into(),
                "https://downloads.cursor.com/lab/2026.10.22-27dcf5f/darwin/arm64/agent-cli-package.tar.gz".into()
            ))
        );
        assert_eq!(package_in_script("echo hi\n", "darwin", "arm64"), None);
        // A variable obid doesn't know, or a version that isn't one, is no package.
        assert_eq!(
            package_in_script(
                "DOWNLOAD_URL=\"https://x/lab/1/${OS}/${OTHER}/a.tgz\"",
                "linux",
                "x64"
            ),
            None
        );
        assert_eq!(
            package_in_script(
                "DOWNLOAD_URL=\"https://x/lab//linux/x64/a.tgz\"",
                "linux",
                "x64"
            ),
            None
        );
    }

    #[test]
    fn the_version_is_read_off_the_installs_layout() {
        let root = std::env::temp_dir().join(format!("obid-cursor-version-{}", std::process::id()));
        let kept = root.join("share/cursor-agent/versions/2026.10.22-27dcf5f");
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(kept.join("cursor-agent"), "#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(kept.join("cursor-agent"), root.join("bin/cursor-agent"))
            .unwrap();
        assert_eq!(
            installed_version(&root.join("bin/cursor-agent")).as_deref(),
            Some("2026.10.22-27dcf5f")
        );
        std::fs::write(root.join("bin/other"), "").unwrap();
        assert_eq!(installed_version(&root.join("bin/other")), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
