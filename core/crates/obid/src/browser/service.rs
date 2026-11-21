//! The browser as obid serves it (BROWSER-2 to 4, 9 to 13, 15, 16, 25, 30 to 32): the API core
//! ([`Api`]) behind one lock, the window's relay and the window's browser driven over it (every
//! agent tab a `<webview>` in Oby's window, docs/plans/browser-t3.md), and the events the screens
//! follow, each handed to the readers right after the change that made it (the lab's
//! `Shared::flush`). It owns that state in memory, and the two switches in obid's settings file
//! (`browser`, add-only, both on by default); it writes nothing else. obid runs no browser of its
//! own.
//!
//! Created at obid's start beside `Connections` ([`Service::open`]) and told when a job ends
//! ([`Service::end_job`], from `crate::jobs`). Its routes are [`super::routes`], its tools
//! [`super::tools`].

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::broadcast;

use super::api::Api;
use super::contract as c;
use super::engine::{Actor, Browsers};

/// How many events the stream's channel holds before a slow reader catches up from the log.
const CHANNEL: usize = 256;

/// The browser as obid serves it.
pub struct Service {
    api: Mutex<Api>,
    events: broadcast::Sender<c::Event>,
    /// The last event handed to the stream's readers.
    published: Mutex<u64>,
    home: PathBuf,
    /// The person's Downloads: where Save a copy and their own downloads go.
    downloads_to: PathBuf,
    /// How long an agent's act waits between being let through and being run: none, but in a
    /// debug build's test (`OBID_TEST_BROWSER_COMMIT_DELAY_MS`, the lab's `commitDelayMs`), the
    /// race's window for BROWSER-6.
    pub(super) commit_delay: Duration,
    /// The window's relay (T3 Code's design): each agent tab a `<webview>` in the window.
    relay: Arc<super::relay::Relay>,
    /// The window's browser: the agents' tools run on it, over the relay, on the engine's core.
    window: Arc<super::window::WindowDriver>,
}

impl Service {
    pub fn new(api: Api, home: PathBuf, downloads_to: PathBuf, commit_delay: Duration) -> Service {
        let relay = Arc::new(super::relay::Relay::new());
        Service {
            api: Mutex::new(api),
            events: broadcast::channel(CHANNEL).0,
            published: Mutex::new(0),
            downloads_to,
            commit_delay,
            window: Arc::new(super::window::WindowDriver::new(
                relay.clone(),
                Arc::new(Mutex::new(Browsers::new())),
                &home,
            )),
            relay,
            home,
        }
    }

    /// The window's browser.
    pub fn window(&self) -> &Arc<super::window::WindowDriver> {
        &self.window
    }

    /// A new tab of the window's is shown (its `tab` event, with its partition) before the
    /// window is waited for, so the window mounts its webview.
    fn wire(self: &Arc<Self>) {
        let me = Arc::downgrade(self);
        self.window.on_tabs(move |job| {
            if let Some(s) = me.upgrade() {
                s.sync(job);
            }
        });
    }

    /// The window's relay.
    pub fn relay(&self) -> &Arc<super::relay::Relay> {
        &self.relay
    }

    /// The browser for obid's `home`: the switches as the settings file keeps them. obid starts no
    /// browser: every agent tab is the window's.
    pub async fn open(home: &Path) -> Arc<Service> {
        let mut api = Api::new();
        if let Some(settings) = crate::settings::load(home).browser {
            api.settings = settings;
        }
        let downloads_to = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|h| h.is_absolute())
            .unwrap_or_else(|| home.to_path_buf())
            .join("Downloads");
        let service = Arc::new(Service::new(
            api,
            home.to_path_buf(),
            downloads_to,
            test_commit_delay(),
        ));
        service.wire();
        service
    }

    /// The API core, under its lock.
    pub fn api<T>(&self, f: impl FnOnce(&mut Api) -> T) -> T {
        f(&mut self.api.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn downloads_to(&self) -> &Path {
        &self.downloads_to
    }

    /// Hands every event the API made since the last call to the stream's readers.
    pub fn flush(&self) {
        let mut published = self.published.lock().unwrap_or_else(|e| e.into_inner());
        let new = self.api(|a| a.events_after(*published));
        for e in new {
            *published = e.seq;
            let _ = self.events.send(e);
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<c::Event> {
        self.events.subscribe()
    }

    /// A job the API doesn't know yet: its browser `notStarted`.
    pub fn ensure_job(&self, job: &str, agent: &str, partition: &str) {
        let added = self.api(|a| {
            let new = a.job(job).is_none();
            if new {
                a.add_job_in(job, agent, partition);
            }
            new
        });
        if added {
            self.flush();
        }
    }

    /// A job's tabs as the engine knows them, into the API (`tab` and `tabClosed` events).
    pub fn sync(&self, job: &str) {
        let status = self
            .window
            .core()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status(&Actor::Agent(job.into()));
        let result = &status["result"];
        let tabs = result["tabs"].as_array().cloned().unwrap_or_default();
        let titles = result["outside"]["data"].clone();
        self.api(|a| a.sync_job(job, &tabs, &titles));
        self.flush();
    }

    /// The owner's signed change to the switches, kept in obid's settings file first: one that
    /// can't be kept changes nothing.
    pub fn put_settings(&self, patch: c::SettingsPatch) -> std::io::Result<c::Settings> {
        let mut next = self.api(|a| a.settings);
        if let Some(b) = patch.agents_use_browser {
            next.agents_use_browser = b;
        }
        if let Some(m) = patch.agents_use_mac {
            next.agents_use_mac = m;
        }
        crate::settings::update(&self.home, |s| s.browser = Some(next))?;
        let settings = self.api(|a| a.put_settings(patch));
        self.flush();
        Ok(settings)
    }

    /// A job ended (BROWSER-25): its tabs are released on the relay and leave the list.
    pub async fn end_job(&self, job: &str) {
        self.window.end(job);
        self.api(|a| a.end_job(job));
        self.flush();
    }

    /// obid is stopping: every job's tabs are released and leave the list.
    pub async fn end_all(&self) {
        for job in self.api(|a| a.job_ids()) {
            self.window.end(&job);
            self.api(|a| a.end_job(&job));
        }
        self.flush();
    }
}

/// The race's window a debug build's test may ask for, at most 10 s.
fn test_commit_delay() -> Duration {
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var("OBID_TEST_BROWSER_COMMIT_DELAY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        return Duration::from_millis(ms.min(10_000));
    }
    Duration::ZERO
}
