//! What a job's browser started, on disk (BROWSER-25): `<home>/browser/live/<job>.json` (0600) with
//! its session, socket folder, daemon PID and Chrome PID, written before its daemon starts and
//! again once it runs, so an obid that stopped without ending its jobs reaps them at its next
//! start. It owns those files and writes nothing else.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::{make_private_dir, write_private};

/// What a job's browser started, on disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveFile {
    pub job: String,
    pub session: String,
    pub socket_dir: PathBuf,
    pub daemon: Option<u32>,
    pub chrome: Option<u32>,
}

impl LiveFile {
    pub fn dir(home: &Path) -> PathBuf {
        home.join("browser").join("live")
    }

    pub fn path(home: &Path, job: &str) -> PathBuf {
        Self::dir(home).join(format!("{job}.json"))
    }

    pub fn write(&self, home: &Path) -> Result<(), String> {
        let dir = Self::dir(home);
        make_private_dir(&dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let path = Self::path(home, &self.job);
        write_private(&path, &bytes).map_err(|e| format!("can't write {}: {e}", path.display()))
    }

    pub fn read(path: &Path) -> Option<LiveFile> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    pub fn remove(home: &Path, job: &str) {
        let _ = std::fs::remove_file(Self::path(home, job));
    }
}
