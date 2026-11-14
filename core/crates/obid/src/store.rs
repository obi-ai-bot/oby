//! Where obid keeps its jobs: one JSON file per job in `home/jobs/` (0600), written whole and
//! renamed into place, so a crash leaves the old version or the new one, never half of one.

use std::path::{Path, PathBuf};

use crate::job::Job;

pub struct Store {
    home: PathBuf,
    dir: PathBuf,
    /// The revision last written of each job: an older snapshot, saved later, never undoes it.
    // ponytail: one lock for every job's save; a lock per job if saves ever queue.
    saved: std::sync::Mutex<std::collections::HashMap<String, u64>>,
}

impl Store {
    /// The store in `home/jobs`, made if it isn't there (0700).
    pub fn open(home: &Path) -> std::io::Result<Store> {
        let dir = home.join("jobs");
        make_private_dir(&dir)?;
        let (home, saved) = (home.to_owned(), Default::default());
        Ok(Store { home, dir, saved })
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Writes the job, unless a later revision of it is written already.
    pub fn save(&self, job: &Job) -> std::io::Result<()> {
        let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        if saved.get(&job.id).is_some_and(|last| *last > job.revision) {
            return Ok(());
        }
        let text = serde_json::to_vec_pretty(job).map_err(std::io::Error::other)?;
        write_private(&self.path(&job.id), &text)?;
        saved.insert(job.id.clone(), job.revision);
        Ok(())
    }

    /// Forgets the job `id`: one that isn't kept is gone already.
    pub fn remove(&self, id: &str) -> std::io::Result<()> {
        match std::fs::remove_file(self.path(id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Every job kept, oldest first. A file that doesn't read as a job is kept aside ([`load_dir`]).
    pub fn load_all(&self) -> Vec<Job> {
        let mut jobs = load_dir::<Job>(&self.home, &self.dir);
        jobs.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.id.cmp(&b.id)));
        jobs
    }
}

/// A folder only its owner can reach (0700), with its parents.
pub fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Writes `bytes` to `path` (0600) through a temporary file renamed into place.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_extension(format!("tmp-{}-{n}", std::process::id()));
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, path)
}

/// Every `*.json` file in `dir` (in obid's `home`) that reads as a `T`. One that doesn't parse is
/// kept aside in `home/unreadable/` and one that can't be read stays: both are listed (HOME-8).
pub fn load_dir<T: serde::de::DeserializeOwned>(home: &Path, dir: &Path) -> Vec<T> {
    let mut items = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        match std::fs::read(&path).map(|bytes| serde_json::from_slice::<T>(&bytes)) {
            Ok(Ok(item)) => items.push(item),
            Ok(Err(e)) => set_aside(home, &path, reason(&e)),
            // Gone since the folder was read: a load beside this one kept it aside.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!("can't read {}: {e}", path.display());
                let why = e.kind().to_string();
                list(&mut unreadable(), &path, why, modified(&path));
            }
        }
    }
    kept_aside(&mut unreadable(), &home.join("unreadable"));
    items
}

static LISTED: std::sync::Mutex<Vec<serde_json::Value>> = std::sync::Mutex::new(Vec::new());

/// The files obid didn't load, each once (`GET /v1/status` `unreadable`): `{path, error, since}`.
pub fn unreadable() -> std::sync::MutexGuard<'static, Vec<serde_json::Value>> {
    LISTED.lock().unwrap_or_else(|e| e.into_inner())
}

fn list(listed: &mut Vec<serde_json::Value>, path: &Path, error: String, since: i64) {
    let path = path.display().to_string();
    if !listed.iter().any(|u| u["path"] == *path) {
        let since = obi_protocol::time::format(since);
        listed.push(serde_json::json!({"path": path, "error": error, "since": since}));
    }
}

/// Why a file doesn't parse, never in serde's words: they can quote what the file holds.
fn reason(e: &serde_json::Error) -> String {
    let kind = match e.classify() {
        serde_json::error::Category::Data => "not what obid expects",
        serde_json::error::Category::Io => "can't be read",
        _ => "not valid JSON",
    };
    format!("{kind} at line {} column {}", e.line(), e.column())
}

fn modified(path: &Path) -> i64 {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let since = modified.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
    since.map_or(0, |d| d.as_secs() as i64)
}

/// Renames the file at `path` to `home/unreadable/<its folder>/<stem>-<seconds>.json`, its bytes
/// as they are, never over a file kept there. One that can't move stays and is listed where it is.
fn set_aside(home: &Path, path: &Path, error: String) {
    let (now, mut listed) = (obi_protocol::time::now(), unreadable());
    let folder = path.parent().and_then(|p| p.strip_prefix(home).ok());
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let to = home.join("unreadable").join(folder.unwrap_or("".as_ref()));
    let to = to.join(format!("{stem}-{now}.json"));
    let moved = match make_private_dir(to.parent().unwrap_or(home)) {
        Ok(()) if to.symlink_metadata().is_ok() => Err(std::io::ErrorKind::AlreadyExists.into()),
        made => made.and_then(|()| std::fs::rename(path, &to)),
    };
    let (at, how) = match moved {
        // A load beside this one kept it aside first.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => (path, format!("can't be kept aside: {e}")),
        Ok(()) => (to.as_path(), format!("kept aside as {}", to.display())),
    };
    tracing::warn!("{} doesn't read ({error}): {how}", path.display());
    list(&mut listed, at, error, now);
}

/// Every file in `dir`, at any depth, kept aside at an earlier start: since the seconds in its
/// name, or its last change.
fn kept_aside(listed: &mut Vec<serde_json::Value>, dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            kept_aside(listed, &path);
            continue;
        }
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let named = stem.rsplit_once('-').and_then(|(_, s)| s.parse().ok());
        let since = named.unwrap_or_else(|| modified(&path));
        let why = "kept aside at an earlier start".into();
        list(listed, &path, why, since);
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// A home of the test's own, with an empty `jobs` folder.
    fn home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("obid-store-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(home.join("jobs")).unwrap();
        home
    }

    /// What `GET /v1/status` lists for the file at `path`.
    fn listed(path: &Path) -> Option<serde_json::Value> {
        let path = path.display().to_string();
        super::unreadable()
            .iter()
            .find(|u| u["path"] == *path)
            .cloned()
    }

    /// HOME-8: a file obid can't read (no permission) stays where it is and is listed there.
    #[test]
    fn home_8_a_file_that_cant_be_read_stays_and_is_listed() {
        use std::os::unix::fs::PermissionsExt;
        if nix::unistd::geteuid().is_root() {
            return; // root reads it anyway
        }
        let home = home("unread");
        let file = home.join("jobs/locked.json");
        std::fs::write(&file, "{}").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let loaded = super::load_dir::<serde_json::Value>(&home, &home.join("jobs"));
        assert!(loaded.is_empty());
        let entry = listed(&file).expect("listed where it is");
        assert_eq!(entry["error"], "permission denied");
        assert!(obi_protocol::time::parse(entry["since"].as_str().unwrap()).is_some());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"{}");
        assert!(!home.join("unreadable").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// HOME-8: a file that doesn't parse and can't be moved aside (`home/unreadable` is a file)
    /// stays where it is, its bytes unchanged, and is listed there.
    #[test]
    fn home_8_a_file_that_cant_move_stays_and_is_listed() {
        let home = home("unmoved");
        std::fs::write(home.join("unreadable"), "").unwrap();
        let file = home.join("jobs/broken.json");
        std::fs::write(&file, "{\"id\": \"MARKER-1\"").unwrap();
        for _ in 0..2 {
            let loaded = super::load_dir::<serde_json::Value>(&home, &home.join("jobs"));
            assert!(loaded.is_empty());
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"{\"id\": \"MARKER-1\"");
        let entry = listed(&file).expect("listed where it is");
        let error = entry["error"].as_str().unwrap();
        assert!(
            error.starts_with("not valid JSON at line 1 column"),
            "{error}"
        );
        let path = file.display().to_string();
        let times = super::unreadable()
            .iter()
            .filter(|u| u["path"] == *path)
            .count();
        assert_eq!(times, 1, "listed once, however many loads");
        let _ = std::fs::remove_dir_all(&home);
    }
}
