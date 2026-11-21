//! The browser as obid serves it (BROWSER-2 to 16, 25): the API core ([`Api`]) behind one lock,
//! the driver of every job's browser, and the events the screens follow, each handed to the
//! readers right after the change that made it (the lab's `Shared::flush`). It owns that state in
//! memory, and the two switches in obid's settings file (`browser`, add-only, both on by default);
//! it writes nothing else, the driver writing the browsers' own folders.
//!
//! Created at obid's start beside `Connections` ([`Service::open`], which first reaps the browsers a
//! stopped obid left), told when a job ends ([`Service::end_job`], from `crate::jobs`), and swept
//! every [`SWEEP_EVERY`] so a browser that closed itself (10 minutes idle) is `notStarted` again and
//! its tabs leave the list. Its routes are [`super::routes`], its tools [`super::tools`].

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::broadcast;

use super::api::Api;
use super::contract as c;
use super::driver::{self, Binaries, Driver};
use super::engine::{Actor, Browsers};
use super::install::{Installer, pinned_options};

/// How often a browser that closed itself is looked for.
pub const SWEEP_EVERY: Duration = Duration::from_secs(30);
/// How many events the stream's channel holds before a slow reader catches up from the log.
const CHANNEL: usize = 256;

/// The browser as obid serves it.
pub struct Service {
    api: Mutex<Api>,
    driver: Arc<Driver>,
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
    /// The window's browser: the agents' tools run on it, over the relay, on the same core.
    window: Arc<super::window::WindowDriver>,
}

impl Service {
    pub fn new(
        api: Api,
        driver: Arc<Driver>,
        home: PathBuf,
        downloads_to: PathBuf,
        commit_delay: Duration,
    ) -> Service {
        let relay = Arc::new(super::relay::Relay::new());
        Service {
            api: Mutex::new(api),
            events: broadcast::channel(CHANNEL).0,
            published: Mutex::new(0),
            downloads_to,
            commit_delay,
            window: Arc::new(super::window::WindowDriver::new(
                relay.clone(),
                driver.core().clone(),
                &home,
            )),
            relay,
            driver,
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

    /// The browser for obid's `home`: browsers a stopped obid left reaped first, the switches as the
    /// settings file keeps them, the engine downloaded on first use and checked against its pins.
    /// A debug build's test may name the tests' stand-in agent-browser instead
    /// (`OBID_TEST_BROWSER_STAND_IN`), never a release build.
    pub async fn open(home: &Path) -> Arc<Service> {
        let reaped = driver::reap_at_start(home).await;
        if !reaped.is_empty() {
            tracing::info!(
                "stopped {} browser processes a previous obid left",
                reaped.len()
            );
        }
        let mut api = Api::new();
        if let Some(settings) = crate::settings::load(home).browser {
            api.settings = settings;
        }
        let binaries = test_binaries(home)
            .unwrap_or_else(|| Binaries::Download(Arc::new(Installer::new(pinned_options(home)))));
        let driver = Driver::new(
            driver::Options::new(home.to_path_buf(), binaries),
            Arc::new(Mutex::new(Browsers::new())),
        );
        let downloads_to = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|h| h.is_absolute())
            .unwrap_or_else(|| home.to_path_buf())
            .join("Downloads");
        let service = Arc::new(Service::new(
            api,
            Arc::new(driver),
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

    pub fn driver(&self) -> &Arc<Driver> {
        &self.driver
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
            .driver
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

    /// A job ended (BROWSER-25): its browser and every process it started stop, and its tabs
    /// leave the list.
    pub async fn end_job(&self, job: &str) {
        self.window.end(job);
        self.driver.end(job).await;
        self.api(|a| a.end_job(job));
        self.flush();
    }

    /// obid is stopping: every live browser stops with it (BROWSER-25), rather than waiting for the
    /// next start to reap it, each killed after [`driver::STOP_GRACE`] so that all of it fits in
    /// obid's own stop.
    pub async fn end_all(&self) {
        let jobs = self.driver.live_jobs();
        futures_util::future::join_all(jobs.iter().map(|job| async move {
            self.driver.end_within(job, driver::STOP_GRACE).await;
            self.api(|a| a.end_job(job));
        }))
        .await;
        self.flush();
    }

    /// Browsers that closed themselves: each job's is `notStarted` again and its tabs go.
    pub async fn sweep(&self) {
        for job in self.driver.sweep().await {
            self.api(|a| a.set_browser(&job, c::BrowserState::NotStarted, None));
            self.sync(&job);
        }
        self.flush();
    }

    /// [`Service::sweep`] every [`SWEEP_EVERY`], until obid stops.
    pub fn spawn_sweeper(self: &Arc<Self>, mut stopping: tokio::sync::watch::Receiver<bool>) {
        let me = self.clone();
        tokio::spawn(async move {
            let mut every = tokio::time::interval(SWEEP_EVERY);
            loop {
                tokio::select! {
                    _ = every.tick() => me.sweep().await,
                    _ = stopping.changed() => return,
                }
            }
        });
    }
}

/// The tests' stand-in agent-browser, in a debug build whose test asks for it.
fn test_binaries(home: &Path) -> Option<Binaries> {
    #[cfg(debug_assertions)]
    if std::env::var("OBID_TEST_BROWSER_STAND_IN").is_ok_and(|v| v == "1") {
        return Some(Binaries::InPlace(driver::stand_in::install(home)));
    }
    let _ = home;
    None
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
