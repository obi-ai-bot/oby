//! A foreign job's files in the window's browser (BROWSER-18). Its box can write its worktree, links
//! included, so neither obid nor the window's Chromium (both the owner's user, outside the box)
//! reads or writes through a path there: an upload is copied into obid's own folder for the job,
//! read through no link, and only the copy reaches the page; its screenshots are written into that
//! folder too (`Service::call_tool` hands it to the core as the job's room). It owns no state and
//! writes only into obid's folder for the job.

use std::fs::File;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

/// `args` as a foreign job's `oby_browser_act` `upload` runs: each path, a file in `worktree`,
/// replaced by its copy in a new folder under `own` (obid's folder for the job). Any other call's
/// `args` as they are. `Err` names the first path that isn't a file in the worktree reached
/// through no link.
pub fn uploads(worktree: &Path, own: &Path, name: &str, args: &Value) -> Result<Value, String> {
    if name != "oby_browser_act" || args["action"] != "upload" {
        return Ok(args.clone());
    }
    let into = own
        .join("uploads")
        .join(uuid::Uuid::new_v4().simple().to_string());
    let mut args = args.clone();
    for path in args["paths"].as_array_mut().into_iter().flatten() {
        let given = path.as_str().unwrap_or_default().to_owned();
        let copy = copy_in(worktree, Path::new(&given), &into).ok_or(given)?;
        *path = Value::String(copy.to_string_lossy().into_owned());
    }
    Ok(args)
}

/// Copies `path` into `into` when it's a regular file inside `room`, as written (no `..`), every
/// folder of it below the room and the file itself opened without following a link, one inside
/// the last (so nothing swapped in after a check is read): the copy's path, or none.
fn copy_in(room: &Path, path: &Path, into: &Path) -> Option<PathBuf> {
    use nix::fcntl::{OFlag, open, openat};
    use nix::sys::stat::Mode;
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
    let folder = OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_RDONLY | OFlag::O_CLOEXEC;
    // The room itself is obid's worktree, made by obid in a folder the box can't write.
    let mut dir = open(room, folder & !OFlag::O_NOFOLLOW, Mode::empty()).ok()?;
    for name in folders {
        dir = openat(&dir, *name, folder, Mode::empty()).ok()?;
    }
    let flags = OFlag::O_NOFOLLOW | OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK;
    let mut source = File::from(openat(&dir, *file, flags, Mode::empty()).ok()?);
    source.metadata().ok().filter(|m| m.is_file())?;
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
        let ran = uploads(
            &room,
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
                uploads(&room, &own, "oby_browser_act", &act(&[refused])),
                Err(given)
            );
        }
        let other = json!({"action": "click", "ref": "e1"});
        assert_eq!(uploads(&room, &own, "oby_browser_act", &other), Ok(other));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
