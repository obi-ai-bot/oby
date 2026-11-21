//! An artifact's version laid out as files to change (docs/contracts/artifacts.md, ARTIFACTS-19
//! to 23): in a job's worktree at `.obi/artifacts/<entry>` and in a conversation's room at
//! `artifacts/<entry>`, so what an agent changes there publishes as the artifact's next version.
//! Only the files the version lists, read through the store's own checks, each folder and file
//! made new without following a link, and never a file an agent loads as its own instructions or
//! settings ([`steers`]). It writes nothing but under the folder it is given.

use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

use nix::errno::Errno;
use nix::fcntl::{OFlag, open, openat};
use nix::sys::stat::{Mode, mkdirat};

use super::{Artifact, Artifacts, Kind, extension};

/// The entry `version` of `artifact` is laid out under, from obid's own data only: a folder named
/// by the first 8 characters of its id, or, for a one-file kind, that name with its file's
/// extension. None for a version it hasn't.
pub fn entry_for(artifact: &Artifact, version: u32) -> Option<String> {
    let kept = artifact.versions.iter().find(|v| v.version == version)?;
    let name: String = artifact.id.chars().take(8).collect();
    if is_folder(artifact.kind) {
        return Some(name);
    }
    Some(format!("{name}.{}", extension(kept.files.first()?)))
}

fn is_folder(kind: Kind) -> bool {
    matches!(kind, Kind::Html | Kind::Canvas | Kind::Design)
}

/// Whether `path` (`/` between its parts) is what an agent's CLI loads as instructions or
/// settings, so it is never laid out (ARTIFACTS-22): a hidden part anywhere (`.claude/`,
/// `.codex/`, `.cursor/`, `.mcp.json`), or a file named as Claude Code's or Codex's own, in any
/// letter case.
pub fn steers(path: &str) -> bool {
    const NAMES: [&str; 4] = [
        "CLAUDE.md",
        "CLAUDE.local.md",
        "AGENTS.md",
        "AGENTS.override.md",
    ];
    let name = path.rsplit('/').next().unwrap_or(path);
    path.split('/').any(|part| part.starts_with('.'))
        || NAMES.iter().any(|n| name.eq_ignore_ascii_case(n))
}

impl Artifacts {
    /// Writes version `version` of `id` into `into` as `entry`: `into/<entry>/…` for a folder
    /// kind, `into/<entry>` for a one-file one. `into/<entry>` must not be there yet; `entry` is
    /// one plain name. An `entry` that [`steers`] lays out nothing.
    pub fn lay_out(&self, id: &str, version: u32, into: &Path, entry: &str) -> Result<(), String> {
        if entry.is_empty() || entry.starts_with('.') || entry.contains(['/', '\\', ':', '\0']) {
            return Err("an artifact's entry is one plain name.".into());
        }
        if steers(entry) {
            return Ok(());
        }
        let artifact = self.get(id).ok_or("there's no such artifact.")?;
        let kept = (artifact.versions.iter())
            .find(|v| v.version == version)
            .ok_or_else(|| format!("artifact {id} has no version {version}."))?;
        let shown = version.to_string();
        let read = |file: &str| {
            self.file(id, &shown, file)
                .map(|(_, bytes)| bytes)
                .ok_or_else(|| format!("{file} of version {version} can't be read."))
        };
        let into = open(
            into,
            OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_RDONLY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| format!("{} can't be opened ({e}).", into.display()))?;
        if !is_folder(artifact.kind) {
            let file = kept.files.first().ok_or("the version has no file.")?;
            return write_new(&into, entry, &read(file)?);
        }
        mkdirat(&into, entry, Mode::from_bits_truncate(0o700))
            .map_err(|e| format!("{entry} can't be made ({e})."))?;
        let root = open_folder(&into, entry)?;
        for file in kept.files.iter().filter(|f| !steers(f)) {
            let (folders, name) = file.rsplit_once('/').unwrap_or(("", file));
            let mut at: Option<OwnedFd> = None;
            for part in folders.split('/').filter(|p| !p.is_empty()) {
                let parent = at.as_ref().unwrap_or(&root);
                match mkdirat(parent, part, Mode::from_bits_truncate(0o700)) {
                    Ok(()) | Err(Errno::EEXIST) => {}
                    Err(e) => return Err(format!("{folders} can't be made ({e}).")),
                }
                at = Some(open_folder(parent, part)?);
            }
            write_new(at.as_ref().unwrap_or(&root), name, &read(file)?)?;
        }
        Ok(())
    }
}

/// The folder `name` in `at`, never through a link.
fn open_folder(at: &impl AsFd, name: &str) -> Result<OwnedFd, String> {
    openat(
        at,
        name,
        OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_RDONLY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| format!("{name} can't be opened ({e})."))
}

/// `bytes` as the new file `name` in `at` (0600): never written through what was there.
fn write_new(at: &impl AsFd, name: &str, bytes: &[u8]) -> Result<(), String> {
    let file = openat(
        at,
        name,
        OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_WRONLY | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(|e| format!("{name} can't be made ({e})."))?;
    std::io::Write::write_all(&mut std::fs::File::from(file), bytes)
        .map_err(|e| format!("{name} can't be written ({e})."))
}

#[cfg(test)]
mod tests {
    use super::super::tests::Temp;
    use super::*;

    /// ARTIFACTS-22: of a version holding files agents load as instructions or settings, only
    /// the page is laid out, and an entry named as one of them lays out nothing.
    #[test]
    fn artifacts_22_steering_files_reach_no_room_or_worktree() {
        let t = Temp::new("seed-22");
        let (store, _events) = t.store();
        let id = "0b9a7c1e-3d2f-4a5b-8c6d-7e8f9a0b1c2d";
        let files = [
            "index.html",
            "CLAUDE.md",
            "sub/agents.md",
            "AGENTS.override.md",
            "docs/claude.local.md",
            ".claude/settings.json",
        ];
        let folder = t.0.join("home/artifacts").join(id);
        for file in files {
            let path = folder.join("1").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, file).unwrap();
        }
        let artifact = serde_json::json!({
            "id": id, "title": "Page", "kind": "html", "conversation": "c1", "job": null,
            "created": "2026-11-21T00:00:00Z",
            "versions": [{"version": 1, "created": "2026-11-21T00:00:00Z", "files": files, "bytes": 10}]
        });
        std::fs::write(folder.join("artifact.json"), artifact.to_string()).unwrap();
        let artifact = store.get(id).unwrap();
        assert_eq!(entry_for(&artifact, 1).as_deref(), Some("0b9a7c1e"));
        assert_eq!(entry_for(&artifact, 2), None);

        let into = t.room();
        store.lay_out(id, 1, &into, "0b9a7c1e").unwrap();
        let mut laid: Vec<String> = walk(&into.join("0b9a7c1e"));
        laid.sort();
        assert_eq!(laid, ["index.html"]);
        // Already there: never laid over.
        assert!(store.lay_out(id, 1, &into, "0b9a7c1e").is_err());

        store.lay_out(id, 1, &into, "CLAUDE.md").unwrap();
        assert!(!into.join("CLAUDE.md").exists());
        for refused in ["", ".claude", "a/b", "a:b"] {
            assert!(store.lay_out(id, 1, &into, refused).is_err(), "{refused}");
        }
    }

    /// ARTIFACTS-25: a stored version's file is read without following a link: one that is a
    /// hard link to a file outside its folder, or a symbolic link, is refused, and nothing of
    /// the outside file is laid out.
    #[test]
    fn artifacts_25_a_linked_file_in_a_stored_version_is_refused() {
        let t = Temp::new("seed-25");
        let (store, _events) = t.store();
        let outside = t.0.join("outside.html");
        std::fs::write(&outside, "OUTSIDE-TEXT").unwrap();
        for (id, link) in [
            ("1b9a7c1e-3d2f-4a5b-8c6d-7e8f9a0b1c2d", "hard"),
            ("2b9a7c1e-3d2f-4a5b-8c6d-7e8f9a0b1c2d", "soft"),
        ] {
            let folder = t.0.join("home/artifacts").join(id);
            std::fs::create_dir_all(folder.join("1")).unwrap();
            let page = folder.join("1/index.html");
            if link == "hard" {
                std::fs::hard_link(&outside, &page).unwrap();
            } else {
                std::os::unix::fs::symlink(&outside, &page).unwrap();
            }
            let artifact = serde_json::json!({
                "id": id, "title": "Page", "kind": "html", "conversation": "c1", "job": null,
                "created": "2026-11-21T00:00:00Z",
                "versions": [{"version": 1, "created": "2026-11-21T00:00:00Z", "files": ["index.html"], "bytes": 12}]
            });
            std::fs::write(folder.join("artifact.json"), artifact.to_string()).unwrap();
            let into = t.room().join(link);
            std::fs::create_dir_all(&into).unwrap();
            assert!(store.lay_out(id, 1, &into, "page").is_err(), "{link}");
            let page = into.join("page/index.html");
            assert!(
                !std::fs::read_to_string(&page).is_ok_and(|t| t.contains("OUTSIDE")),
                "{link}"
            );
        }
    }

    /// Every file under `dir`, relative to it.
    fn walk(dir: &Path) -> Vec<String> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().unwrap().is_dir() {
                found.extend(
                    walk(&entry.path())
                        .into_iter()
                        .map(|f| format!("{name}/{f}")),
                );
            } else {
                found.push(name);
            }
        }
        found
    }
}
