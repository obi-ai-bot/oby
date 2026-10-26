//! `.obi/` in a job's worktree: where obid hands the agent quoted data (`.obi/inputs/`,
//! `.obi/context/`). obid writes there itself, outside the job sandbox, and the worktree holds
//! whatever the repository commits, also a `.obi` or `.obi/<sub>` that is a link out of the
//! worktree (`.obi -> /anywhere`). So nothing here follows a link: every folder on the way must be
//! a real folder of the worktree's (checked with `symlink_metadata`), or obid refuses, and a file
//! is only ever created new (`O_EXCL`), never written through what was there, and renamed into
//! place whole.

use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};

/// The folder obid keeps in a worktree.
pub const OBI: &str = ".obi";

/// Why a path in `.obi/` can't be used.
fn not_a_folder(path: &Path) -> Error {
    Error::new(
        ErrorKind::InvalidData,
        format!(
            "{} isn't a folder (the repository has a link or a file there), so obid writes nothing through it",
            path.display()
        ),
    )
}

/// Whether `path` is a real folder (`Some(true)`), missing (`Some(false)`), or anything else, a
/// link included (`None`). The last part of `path` is never followed.
fn real_folder(path: &Path) -> std::io::Result<Option<bool>> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => Ok(Some(true)),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Some(false)),
        Err(e) => Err(e),
    }
}

/// `worktree/.obi/<sub>`, made where missing, each folder on the way checked to be a real
/// folder, not a link or a file. `Err` when one isn't: nothing is written then.
pub fn folder(worktree: &Path, sub: &str) -> std::io::Result<PathBuf> {
    let obi = worktree.join(OBI);
    let path = obi.join(sub);
    for folder in [&obi, &path] {
        match real_folder(folder)? {
            Some(true) => {}
            Some(false) => std::fs::create_dir(folder)?,
            None => return Err(not_a_folder(folder)),
        }
    }
    Ok(path)
}

/// Takes `worktree/.obi/<sub>` away with all it holds, when it is a real folder under a real
/// `.obi`. A link inside it is removed itself, never followed. Missing is fine; a link or a file
/// in place of either folder is `Err`, and nothing is removed.
pub fn clear(worktree: &Path, sub: &str) -> std::io::Result<()> {
    let obi = worktree.join(OBI);
    match real_folder(&obi)? {
        Some(false) => return Ok(()),
        None => return Err(not_a_folder(&obi)),
        Some(true) => {}
    }
    let path = obi.join(sub);
    match real_folder(&path)? {
        Some(false) => Ok(()),
        None => Err(not_a_folder(&path)),
        // Its own path is a real folder (checked above, and its parent too), and remove_dir_all
        // doesn't follow the links it finds inside.
        Some(true) => std::fs::remove_dir_all(&path),
    }
}

/// Writes `bytes` as the new file `name` in `dir` (a folder [`folder`] gave), all at once: they
/// go into a temp file created new beside it, which is then renamed over `name`. So a reader
/// (the agent, already running, or a test polling for it) sees the whole file or none, never an
/// empty or a partial one. What was there under that name, a link or a file, is replaced by the
/// rename, not written through; a folder there is `Err`. The temp file is gone on any error.
pub fn write_new(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let path = dir.join(name);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_dir() => return Err(not_a_folder(&path)),
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let (temp, mut file) = temp_beside(dir, name)?;
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| {
            drop(file);
            std::fs::rename(&temp, &path)
        });
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    Ok(path)
}

/// A new file in `dir` to write `name` into before it is renamed over it: hidden, named after
/// it, created new (`O_EXCL`, so never a link that was there), with the modes `name` was always
/// created with (0o666 less the umask), so the final file never shows other ones.
fn temp_beside(dir: &Path, name: &str) -> std::io::Result<(PathBuf, std::fs::File)> {
    use std::os::unix::fs::OpenOptionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    loop {
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temp = dir.join(format!(".{name}.{}-{n}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o666)
            .open(&temp)
        {
            Ok(file) => return Ok((temp, file)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("obid-obi-dir-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("worktree")).unwrap();
        std::fs::create_dir_all(path.join("victim/context")).unwrap();
        std::fs::write(path.join("victim/context/keep"), "keep").unwrap();
        path
    }

    #[test]
    fn real_folders_are_made_and_written() {
        let t = temp("real");
        let worktree = t.join("worktree");
        let dir = folder(&worktree, "inputs").unwrap();
        assert_eq!(dir, worktree.join(".obi/inputs"));
        write_new(&dir, "a.json", b"1").unwrap();
        write_new(&dir, "a.json", b"2").unwrap();
        assert_eq!(std::fs::read(dir.join("a.json")).unwrap(), b"2");
        clear(&worktree, "inputs").unwrap();
        assert!(!dir.exists());
        clear(&worktree, "inputs").unwrap();
        let _ = std::fs::remove_dir_all(&t);
    }

    /// What's in `dir`, by name.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_file_is_written_whole_with_its_old_modes_and_no_temp_left() {
        use std::os::unix::fs::PermissionsExt;
        let t = temp("whole");
        let dir = folder(&t.join("worktree"), "inputs").unwrap();
        // The modes a file created the way write_new always did gets (0o666 less the umask).
        let plain = t.join("plain");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&plain)
            .unwrap();
        let want = std::fs::metadata(&plain).unwrap().permissions().mode() & 0o7777;
        let bytes = vec![b'x'; 1 << 20];
        let path = write_new(&dir, "event.md", &bytes).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, want, "{mode:o} != {want:o}");
        assert_eq!(names(&dir), ["event.md"]);
        // A folder under the name is refused, and nothing is left beside it.
        std::fs::create_dir(dir.join("taken")).unwrap();
        assert!(write_new(&dir, "taken", b"x").is_err());
        assert_eq!(names(&dir), ["event.md", "taken"]);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn a_failed_write_leaves_no_temp_file() {
        use std::os::unix::fs::PermissionsExt;
        let t = temp("fails");
        let dir = folder(&t.join("worktree"), "inputs").unwrap();
        // A folder obid can't write in.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        assert!(write_new(&dir, "event.md", b"x").is_err());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(names(&dir), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn a_reader_never_sees_an_empty_or_partial_file() {
        let t = temp("race");
        let dir = folder(&t.join("worktree"), "inputs").unwrap();
        let path = dir.join("event.md");
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = {
            let (path, done) = (path.clone(), done.clone());
            std::thread::spawn(move || {
                let mut seen = 0;
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Ok(bytes) = std::fs::read(&path) {
                        assert!(
                            bytes.len() == 256 << 10 && bytes.iter().all(|b| *b == bytes[0]),
                            "read {} bytes, not one whole write",
                            bytes.len()
                        );
                        seen += 1;
                    }
                }
                seen
            })
        };
        for round in 0..200u32 {
            write_new(
                &dir,
                "event.md",
                &vec![b'a' + (round % 26) as u8; 256 << 10],
            )
            .unwrap();
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(reader.join().unwrap() > 0, "the reader read something");
        assert_eq!(names(&dir), ["event.md"]);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn a_link_on_the_way_is_refused_and_nothing_outside_changes() {
        let t = temp("links");
        let worktree = t.join("worktree");
        let victim = t.join("victim");
        // `.obi -> victim`.
        std::os::unix::fs::symlink(&victim, worktree.join(".obi")).unwrap();
        assert!(folder(&worktree, "context").is_err());
        assert!(clear(&worktree, "context").is_err());
        assert!(victim.join("context/keep").exists());
        std::fs::remove_file(worktree.join(".obi")).unwrap();
        // `.obi/context -> victim/context`.
        std::fs::create_dir(worktree.join(".obi")).unwrap();
        std::os::unix::fs::symlink(victim.join("context"), worktree.join(".obi/context")).unwrap();
        assert!(folder(&worktree, "context").is_err());
        assert!(clear(&worktree, "context").is_err());
        assert!(victim.join("context/keep").exists());
        std::fs::remove_file(worktree.join(".obi/context")).unwrap();
        // `.obi` a file.
        std::fs::remove_dir(worktree.join(".obi")).unwrap();
        std::fs::write(worktree.join(".obi"), "x").unwrap();
        assert!(folder(&worktree, "inputs").is_err());
        std::fs::remove_file(worktree.join(".obi")).unwrap();
        // A link inside a real folder: cleared as a link, never followed; a file named like a
        // link is replaced, not written through.
        let dir = folder(&worktree, "context").unwrap();
        std::os::unix::fs::symlink(victim.join("context"), dir.join("inner")).unwrap();
        std::os::unix::fs::symlink(victim.join("context/keep"), dir.join("notes.md")).unwrap();
        write_new(&dir, "notes.md", b"quoted").unwrap();
        assert_eq!(
            std::fs::read_to_string(victim.join("context/keep")).unwrap(),
            "keep"
        );
        clear(&worktree, "context").unwrap();
        assert!(victim.join("context/keep").exists());
        let _ = std::fs::remove_dir_all(&t);
    }
}
