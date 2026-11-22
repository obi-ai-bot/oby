//! An agent's downloads (BROWSER-13): Oby's window saves them into the job's own folder
//! (`<home>/browser/downloads/<job>/`, the only one it takes), they are listed for the API, and
//! Save a copy copies one (never moves it) into the folder it is given, under a name not taken
//! there: the name is made new there (`O_CREAT|O_EXCL|O_NOFOLLOW`), so nothing in that folder, a
//! link included, is ever replaced or followed, and the copy carries macOS's quarantine mark. It
//! owns no state; it writes only the copy.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use super::{DownloadFile, WindowDriver};

/// A download's name is a plain file name: no folder, nothing hidden or half-done.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\0')
        && !name.ends_with(".crdownload")
}

/// How many `stem (n).ext` names are tried before giving up.
const NAMES: u32 = 10_000;

/// A new file in `dir`: `name`, or `stem (n).ext` with the first `n` free, each made only if
/// nothing is there yet, a link (dangling or not) counting as taken and never followed.
fn create_free(dir: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    let names =
        std::iter::once(name.to_string()).chain((1..NAMES).map(|n| format!("{stem} ({n}){ext}")));
    for candidate in names {
        let path = dir.join(candidate);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "every name is taken",
    ))
}

impl WindowDriver {
    /// The files in a job's downloads folder, by name: finished ones only (Chrome's half-done
    /// `.crdownload` left out).
    pub fn downloads(&self, job: &str) -> Vec<DownloadFile> {
        let dir = self.downloads_folder(job);
        let mut files: Vec<DownloadFile> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let meta = std::fs::symlink_metadata(e.path()).ok()?;
                (plain_name(&name) && meta.is_file()).then_some(DownloadFile {
                    name,
                    bytes: meta.len(),
                })
            })
            .collect();
        files.sort_by(|a, b| a.name.cmp(&b.name));
        files
    }

    /// Copies a job's download `name` into `to` (the person's Downloads, once they signed Save a
    /// copy); the agent's file stays. Returns where the copy is.
    pub fn save_copy(&self, job: &str, name: &str, to: &Path) -> Result<PathBuf, String> {
        if !plain_name(name) {
            return Err(format!("{name:?} isn't a download's name."));
        }
        let from = self.downloads_folder(job).join(name);
        let is_file = std::fs::symlink_metadata(&from).is_ok_and(|m| m.is_file());
        if !is_file {
            return Err(format!("This job has no download named {name}."));
        }
        if !to.is_dir() {
            return Err(format!("{} isn't a folder.", to.display()));
        }
        let failed = |e: io::Error| format!("can't copy {name} into {}: {e}", to.display());
        let (dest, mut file) = create_free(to, name).map_err(failed)?;
        // Copied through the file just made, never by its path again.
        let copied = File::open(&from).and_then(|mut f| io::copy(&mut f, &mut file));
        if let Err(e) = copied {
            let _ = std::fs::remove_file(&dest);
            return Err(failed(e));
        }
        // The profile file's quarantine mark, or obid's: none set, no copy (BROWSER-34).
        #[cfg(target_os = "macos")]
        if let Err(e) = crate::browser::quarantine::ensure(&dest, &from) {
            let _ = std::fs::remove_file(&dest);
            return Err(format!("no quarantine mark on {name}: {e}"));
        }
        Ok(dest)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn driver(home: &Path) -> WindowDriver {
        WindowDriver::new(
            Arc::new(crate::browser::relay::Relay::new()),
            Arc::new(Mutex::new(crate::browser::engine::Browsers::new())),
            home,
        )
    }

    #[test]
    fn browser_13_a_download_stays_in_its_folder_and_save_a_copy_copies_it() {
        let home = std::env::temp_dir().join(format!("obid-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let d = driver(&home);
        let dir = d.downloads_folder("job-a");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("report.pdf"), b"pdf").unwrap();
        std::fs::write(dir.join("half.pdf.crdownload"), b"p").unwrap();
        assert_eq!(
            d.downloads("job-a"),
            vec![DownloadFile {
                name: "report.pdf".into(),
                bytes: 3
            }]
        );
        let to = home.join("person-downloads");
        std::fs::create_dir_all(&to).unwrap();
        let first = d.save_copy("job-a", "report.pdf", &to).unwrap();
        let second = d.save_copy("job-a", "report.pdf", &to).unwrap();
        assert_eq!(first, to.join("report.pdf"));
        assert_eq!(second, to.join("report (1).pdf"));
        assert!(dir.join("report.pdf").is_file(), "the agent's file stays");
        assert!(d.save_copy("job-a", "../x", &to).is_err());
        assert!(d.save_copy("job-a", "missing.pdf", &to).is_err());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// BROWSER-13, 34: Save a copy carries the profile file's quarantine mark onto the copy, as
    /// delivery does.
    #[cfg(target_os = "macos")]
    #[test]
    fn browser_13_save_a_copy_keeps_the_quarantine_mark() {
        let home = std::env::temp_dir().join(format!("obid-dl-q-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let d = driver(&home);
        let dir = d.downloads_folder("job-a");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("setup.dmg"), b"dmg").unwrap();
        let mark = "0083;6740a1b2;Chrome;";
        let set = std::process::Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.quarantine", mark])
            .arg(dir.join("setup.dmg"))
            .status()
            .unwrap();
        assert!(set.success());
        let to = home.join("person-downloads");
        std::fs::create_dir_all(&to).unwrap();
        let copy = d.save_copy("job-a", "setup.dmg", &to).unwrap();
        assert_eq!(
            crate::browser::quarantine::read(&copy).as_deref(),
            Some(mark)
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}
