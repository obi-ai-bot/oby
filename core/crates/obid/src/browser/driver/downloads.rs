//! An agent's downloads (BROWSER-13): they stay in its job's profile (`--download-path
//! <profile>/Downloads`), listed for the API, and Save a copy copies one (never moves it) into the
//! folder it is given, under a name not taken there. It owns no state; it writes only the copy.

use std::path::{Path, PathBuf};

use super::{DownloadFile, Driver, Session};

/// A download's name is a plain file name: no folder, nothing hidden or half-done.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\0')
        && !name.ends_with(".crdownload")
}

/// `name` in `dir`, or `stem (n).ext` with the first `n` not taken.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

impl Driver {
    /// The files in a job's downloads folder, by name: finished ones only.
    pub fn downloads(&self, job: &str) -> Vec<DownloadFile> {
        let dir = Session::downloads_of(&self.options.home, job);
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
        let from = Session::downloads_of(&self.options.home, job).join(name);
        let is_file = std::fs::symlink_metadata(&from).is_ok_and(|m| m.is_file());
        if !is_file {
            return Err(format!("This job has no download named {name}."));
        }
        if !to.is_dir() {
            return Err(format!("{} isn't a folder.", to.display()));
        }
        let dest = free_name(to, name);
        std::fs::copy(&from, &dest)
            .map_err(|e| format!("can't copy {name} into {}: {e}", to.display()))?;
        Ok(dest)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::{Binaries, Options, stand_in};
    use super::*;
    use crate::browser::engine::Browsers;

    #[test]
    fn browser_13_a_download_stays_in_the_profile_and_save_a_copy_copies_it() {
        let home = stand_in::scratch("downloads");
        let d = Driver::new(
            Options::new(home.clone(), Binaries::InPlace(stand_in::install(&home))),
            Arc::new(Mutex::new(Browsers::new())),
        );
        let dir = Session::downloads_of(&home, "job-a");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("report.pdf"), b"%PDF-1.4 fixture").unwrap();
        std::fs::write(dir.join("big.zip.crdownload"), b"half").unwrap();
        std::fs::write(dir.join(".hidden"), b"x").unwrap();
        assert_eq!(
            d.downloads("job-a"),
            vec![DownloadFile {
                name: "report.pdf".into(),
                bytes: 16
            }]
        );
        let person = home.join("person-downloads");
        std::fs::create_dir_all(&person).unwrap();
        std::fs::write(person.join("report.pdf"), b"the person's own").unwrap();
        let copy = d.save_copy("job-a", "report.pdf", &person).unwrap();
        assert_eq!(copy, person.join("report (1).pdf"));
        assert_eq!(std::fs::read(&copy).unwrap(), b"%PDF-1.4 fixture");
        // The agent's file stays, and the person's own is untouched.
        assert!(dir.join("report.pdf").is_file());
        assert_eq!(
            std::fs::read(person.join("report.pdf")).unwrap(),
            b"the person's own"
        );
        for bad in [
            "../report.pdf",
            ".hidden",
            "big.zip.crdownload",
            "missing.pdf",
        ] {
            assert!(d.save_copy("job-a", bad, &person).is_err(), "{bad}");
        }
        let _ = std::fs::remove_dir_all(home);
    }
}
