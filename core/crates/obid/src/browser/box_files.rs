//! A foreign job's files in the window's browser (BROWSER-18). Its box can write its worktree, links
//! included, so neither obid nor the window's Chromium (both the owner's user, outside the box)
//! reads or writes through a path there: an upload is copied into obid's own folder for the job,
//! read through no link, and only the copy reaches the page; its screenshots are written into that
//! folder too (`Service::call_tool` hands it to the core as the job's room). The box can replace
//! the worktree itself, so its upload is read only through the very folder obid opened and holds
//! ([`room`]) before the job's agent started: a folder put in its place later, whatever its inode,
//! is never reached through it. It owns no state and writes only into obid's folder for the job.

use std::fs::File;
use std::io::Write as _;
use std::os::fd::OwnedFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use nix::fcntl::{OFlag, open, openat};
use nix::sys::stat::Mode;
use serde_json::Value;

/// A folder: never through a link.
const FOLDER: OFlag = OFlag::O_DIRECTORY
    .union(OFlag::O_NOFOLLOW)
    .union(OFlag::O_RDONLY)
    .union(OFlag::O_CLOEXEC);

/// The folder at `room` now, opened from its parent (obid's own folder, which the box can't
/// write), the folder itself never through a link: obid holds it from the job's agent's start,
/// and an upload reads only through it, so a worktree removed or swapped after is never reached.
pub fn room(room: &Path) -> Option<OwnedFd> {
    let parent = open(room.parent()?, FOLDER & !OFlag::O_NOFOLLOW, Mode::empty()).ok()?;
    openat(&parent, room.file_name()?, FOLDER, Mode::empty()).ok()
}

/// `args` as a foreign job's `oby_browser_act` `upload` runs: each path, a file in `worktree`,
/// replaced by its copy in a new folder under `own` (obid's folder for the job). Any other call's
/// `args` as they are. `Err` names the first path that isn't a file in the worktree reached
/// through no link, read through the worktree folder obid holds (`kept`, [`room`]).
pub fn uploads(
    worktree: &Path,
    kept: Option<&OwnedFd>,
    own: &Path,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    if name != "oby_browser_act" || args["action"] != "upload" {
        return Ok(args.clone());
    }
    let into = own
        .join("uploads")
        .join(uuid::Uuid::new_v4().simple().to_string());
    let mut args = args.clone();
    for path in args["paths"].as_array_mut().into_iter().flatten() {
        let given = path.as_str().unwrap_or_default().to_owned();
        let copy = copy_in(worktree, kept, Path::new(&given), &into).ok_or(given)?;
        *path = Value::String(copy.to_string_lossy().into_owned());
    }
    Ok(args)
}

/// Copies `path` into `into` when it's a regular file of one name only (no hard link) inside
/// `room`, as written (no `..`), read through the folder obid holds for the room (`kept`), every
/// folder below it and the file opened without following a link, one inside the last (so nothing
/// swapped in after a check is read). The copy's path, or none.
fn copy_in(room: &Path, kept: Option<&OwnedFd>, path: &Path, into: &Path) -> Option<PathBuf> {
    let names = path
        .strip_prefix(room)
        .ok()?
        .components()
        .map(|c| match c {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let (file, folders) = names.split_last()?;
    let mut dir = kept?.try_clone().ok()?;
    for name in folders {
        dir = openat(&dir, *name, FOLDER, Mode::empty()).ok()?;
    }
    let flags = OFlag::O_NOFOLLOW | OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK;
    let mut source = File::from(openat(&dir, *file, flags, Mode::empty()).ok()?);
    source
        .metadata()
        .ok()
        .filter(|m| m.is_file() && m.nlink() == 1)?;
    crate::store::make_private_dir(into).ok()?;
    let copy = into.join(file);
    let mut target = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&copy)
        .ok()?;
    std::io::copy(&mut source, &mut target).ok()?;
    target.flush().ok()?;
    Some(copy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// BROWSER-18: a foreign job's upload is a copy in obid's folder, never the worktree's file;
    /// a link to a file outside the room, or a path through a linked folder, is no upload.
    #[test]
    fn browser_18_a_foreign_upload_is_a_copy_read_through_no_link() {
        let root = std::env::temp_dir().join(format!("obid-box-files-{}", uuid::Uuid::new_v4()));
        let (room, own, outside) = (root.join("wt"), root.join("own"), root.join("outside"));
        std::fs::create_dir_all(room.join("docs")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(room.join("docs/cv.pdf"), b"mine").unwrap();
        std::fs::write(outside.join("id_ed25519"), b"secret").unwrap();
        std::os::unix::fs::symlink(outside.join("id_ed25519"), room.join("leak")).unwrap();
        std::os::unix::fs::symlink(&outside, room.join("linked")).unwrap();
        let act = |paths: &[PathBuf]| json!({"action": "upload", "ref": "e1", "paths": paths});
        let kept = super::room(&room);
        let kept = kept.as_ref();
        let ran = uploads(
            &room,
            kept,
            &own,
            "oby_browser_act",
            &act(&[room.join("docs/cv.pdf")]),
        )
        .unwrap();
        let copy = PathBuf::from(ran["paths"][0].as_str().unwrap());
        assert!(copy.starts_with(own.join("uploads")), "{copy:?}");
        assert_eq!(copy.file_name().unwrap(), "cv.pdf");
        assert_eq!(std::fs::read(&copy).unwrap(), b"mine");
        for refused in [
            room.join("leak"),
            room.join("linked/id_ed25519"),
            room.join("docs/../leak"),
            outside.join("id_ed25519"),
            room.join("docs"),
        ] {
            let given = refused.to_string_lossy().into_owned();
            assert_eq!(
                uploads(&room, kept, &own, "oby_browser_act", &act(&[refused])),
                Err(given)
            );
        }
        let other = json!({"action": "click", "ref": "e1"});
        assert_eq!(
            uploads(&room, kept, &own, "oby_browser_act", &other),
            Ok(other)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// BROWSER-18: the box can replace its worktree with a link or with a folder of its own, and
    /// hard-link a file: an upload reads only from the very folder obid kept, never through a
    /// link to it, and never a file with another name elsewhere.
    #[test]
    fn browser_18_an_upload_reads_only_the_worktree_obid_kept_and_no_hard_link() {
        let root = std::env::temp_dir().join(format!("obid-box-files-{}", uuid::Uuid::new_v4()));
        let (worktrees, own, outside) = (
            root.join("worktrees"),
            root.join("own"),
            root.join("outside"),
        );
        let room = worktrees.join("j1");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("id_ed25519"), b"secret").unwrap();
        std::fs::write(room.join("cv.pdf"), b"mine").unwrap();
        std::fs::hard_link(outside.join("id_ed25519"), room.join("hard")).unwrap();
        let kept = super::room(&room);
        assert!(kept.is_some());
        let kept = kept.as_ref();
        let act = |p: PathBuf| json!({"action": "upload", "ref": "e1", "paths": [p]});
        let upload = |p: PathBuf| uploads(&room, kept, &own, "oby_browser_act", &act(p));
        assert!(upload(room.join("cv.pdf")).is_ok());
        assert!(upload(room.join("hard")).is_err(), "a hard-linked secret");
        // The worktree swapped for a link to a folder outside, then for a folder of the box's own.
        std::fs::remove_dir_all(&room).unwrap();
        std::os::unix::fs::symlink(&outside, &room).unwrap();
        assert!(
            upload(room.join("id_ed25519")).is_err(),
            "a worktree that's a link"
        );
        assert!(super::room(&room).is_none());
        std::fs::remove_file(&room).unwrap();
        std::fs::create_dir(&room).unwrap();
        std::fs::write(room.join("cv.pdf"), b"theirs").unwrap();
        assert!(
            upload(room.join("cv.pdf")).is_err(),
            "another folder in the worktree's place"
        );
        assert!(
            uploads(
                &room,
                None,
                &own,
                "oby_browser_act",
                &act(room.join("cv.pdf"))
            )
            .is_err()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
