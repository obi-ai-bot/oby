//! The window's browser (T3 Code's design, docs/plans/browser-t3.md): every agent tab is a
//! `<webview>` in Oby's window, driven over the window's relay ([`super::relay`]) in page-level
//! CDP. No browser of obid's own runs, and there is no fallback: with no window, a call is
//! refused `no_window`. Its [`WindowDriver::run`] runs what the engine core's
//! [`crate::browser::engine::Ticket`] says, under its deadline, as the old driver did; [`page`]
//! says each of the engine's words in CDP and [`snapshot`] reads a page's accessibility tree.
//!
//! It owns, in memory only, each job's current tab and each tab's snapshot refs (their DOM
//! nodes); the core's state is the core's ([`Browsers`]), the relay's the relay's. It writes only
//! a screenshot into the agent's room when asked; the window saves downloads under
//! `<home>/browser/downloads/<job>/`, which obid names.

mod call;
mod download_file;
mod downloads;
pub mod page;
pub mod run;
pub mod snapshot;

pub use call::Call;
pub use download_file::DownloadFile;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use super::engine::Browsers;
use super::relay::Relay;

/// How long a new tab's webview has to attach (T3 waits as long).
pub const ATTACH_WITHIN: Duration = Duration::from_secs(10);

/// What the driver tells whoever shows the tabs that a job's tabs changed (the API's sync).
type OnTabs = Box<dyn Fn(&str) + Send + Sync>;

/// The window's browser: see the module's header.
pub struct WindowDriver {
    relay: Arc<Relay>,
    core: Arc<Mutex<Browsers>>,
    home: PathBuf,
    epoch: tokio::time::Instant,
    /// Each job's current tab.
    active: Mutex<HashMap<String, String>>,
    /// Each tab's last snapshot's refs: their DOM nodes (`backendDOMNodeId`).
    refs: Mutex<HashMap<String, HashMap<String, i64>>>,
    /// Tabs whose page domain obid enabled (it hears their loads and dialogs).
    enabled: Mutex<HashSet<String>>,
    on_tabs: OnceLock<OnTabs>,
}

impl WindowDriver {
    pub fn new(relay: Arc<Relay>, core: Arc<Mutex<Browsers>>, home: &Path) -> WindowDriver {
        WindowDriver {
            relay,
            core,
            home: home.to_path_buf(),
            epoch: tokio::time::Instant::now(),
            active: Mutex::new(HashMap::new()),
            refs: Mutex::new(HashMap::new()),
            enabled: Mutex::new(HashSet::new()),
            on_tabs: OnceLock::new(),
        }
    }

    /// Who hears that a job's tabs changed, so its new tab's `tab` event (with its partition) goes
    /// out before the driver waits for the window to attach it.
    pub fn on_tabs(&self, f: impl Fn(&str) + Send + Sync + 'static) {
        let _ = self.on_tabs.set(Box::new(f));
    }

    fn tabs_changed(&self, job: &str) {
        if let Some(f) = self.on_tabs.get() {
            f(job);
        }
    }

    /// The clock the core's deadlines are on: pass it to [`Browsers::begin`].
    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn core(&self) -> &Arc<Mutex<Browsers>> {
        &self.core
    }

    pub fn relay(&self) -> &Arc<Relay> {
        &self.relay
    }

    /// Where the window saves a job's downloads: `<home>/browser/downloads/<job>/`, the only
    /// folder it takes.
    pub fn downloads_folder(&self, job: &str) -> PathBuf {
        self.home.join("browser").join("downloads").join(job)
    }

    /// The agent's own folder when its job has no worktree: `<home>/browser/jobs/<job>/room`.
    pub fn room(&self, job: &str) -> PathBuf {
        self.home
            .join("browser")
            .join("jobs")
            .join(job)
            .join("room")
    }

    /// A job ended: its tabs are released (the window drops their webviews) and forgotten.
    pub fn end(&self, job: &str) {
        let tabs: Vec<String> = self
            .core
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .tabs_of(job)
            .iter()
            .map(|t| t.id.clone())
            .collect();
        for tab in &tabs {
            self.relay.release(tab);
            self.forget(tab);
        }
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(job);
        self.core
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ended(job);
        self.tabs_changed(job);
    }

    fn forget(&self, tab: &str) {
        self.refs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(tab);
        self.enabled
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(tab);
    }
}
