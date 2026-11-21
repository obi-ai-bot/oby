//! An agent's downloads (BROWSER-13): Oby's window saves them into the job's own folder
//! (`<home>/browser/downloads/<job>/`, the only one it takes), they are listed for the API, and
//! Save a copy copies one (never moves it) into the folder it is given, under a name not taken
//! there. It owns no state; it writes only the copy.

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
        let dest = free_name(to, name);
        std::fs::copy(&from, &dest)
            .map_err(|e| format!("can't copy {name} into {}: {e}", to.display()))?;
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
}
