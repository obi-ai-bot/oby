//! The browser as obid serves it (BROWSER-2 to 4, 9 to 13, 15, 16, 25, 30 to 32, 34, 35): the API core
//! ([`Api`]) behind one lock, the window's relay and the window's browser driven over it (every
//! agent tab a `<webview>` in Oby's window, docs/plans/browser-t3.md), and the events the screens
//! follow, each handed to the readers right after the change that made it (the lab's
//! `Shared::flush`). It owns that state in memory, and the two switches in obid's settings file
//! (`browser`, add-only, both on by default); it writes nothing else but a download the person
//! started in an own job's tab, through `watch_downloads`. obid runs no browser of its own.
//!
//! Created at obid's start beside `Connections` ([`Service::open`]) and told when a job ends
//! ([`Service::end_job`], from `crate::jobs`). Its routes are [`super::routes`], its tools
//! [`super::tools`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::broadcast;

use super::api::Api;
use super::contract as c;
use super::engine::{Actor, Browsers};

/// How many events the stream's channel holds before a slow reader catches up from the log.
const CHANNEL: usize = 256;
/// Who started a download in a tab nobody holds: the person at the window.
const THE_WINDOWS_PERSON: &str = "window";

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
    /// The host's own services' ports (obid's, its gateway's, the `direct` listeners'): the box's
    /// edge a foreign job's browser stops at (BROWSER-18), on loopback for its navigate and on any
    /// address for its browser door. obid's start sets them ([`Service::set_host_ports`]).
    host_ports: Mutex<Vec<u16>>,
    /// Each foreign job's worktree folder, held open from its agent's start (`box_files::room`):
    /// its uploads are read through it, whatever its box puts in its place after.
    rooms: Mutex<std::collections::HashMap<String, Arc<std::os::fd::OwnedFd>>>,
}

impl Service {
    pub fn new(
        api: Api,
        home: PathBuf,
        downloads_to: PathBuf,
        commit_delay: Duration,
        agent_input_window: Duration,
    ) -> Service {
        let relay = Arc::new(super::relay::Relay::with_agent_input_window(
            agent_input_window,
        ));
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
            host_ports: Mutex::new(Vec::new()),
            rooms: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// A foreign job's worktree folder, opened and held the first time its agent is given the
    /// browser (before it starts): later its box could have swapped it. After obid restarts it's
    /// opened again at the job's next launch, as the folder is then.
    pub(crate) fn keep_room(&self, job: &str, worktree: &Path) {
        let mut rooms = self.rooms.lock().unwrap_or_else(|e| e.into_inner());
        if !rooms.contains_key(job)
            && let Some(folder) = super::box_files::room(worktree)
        {
            rooms.insert(job.to_owned(), Arc::new(folder));
        }
    }

    /// The worktree folder held for a foreign job (`keep_room`), if any.
    pub(super) fn room(&self, job: &str) -> Option<Arc<std::os::fd::OwnedFd>> {
        self.rooms
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(job)
            .cloned()
    }

    /// The host's own services' ports: obid's, its gateway's and the `direct` listeners'
    /// (BROWSER-18).
    pub fn set_host_ports(&self, ports: Vec<u16>) {
        *self.host_ports.lock().unwrap_or_else(|e| e.into_inner()) = ports;
    }

    pub(crate) fn host_ports(&self) -> Vec<u16> {
        self.host_ports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The window's browser.
    pub fn window(&self) -> &Arc<super::window::WindowDriver> {
        &self.window
    }

    /// A new tab of the window's is shown (its `tab` event, with its partition) before the
    /// window is waited for, so the window mounts its webview; and a download the person starts
    /// in an own job's tab is delivered once it completes (BROWSER-34, 35).
    fn wire(self: &Arc<Self>) {
        let me = Arc::downgrade(self);
        self.window.on_tabs(move |job| {
            if let Some(s) = me.upgrade() {
                s.sync(job);
            }
        });
        let mut events = self.relay.page_events();
        let me = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut persons = HashMap::new();
            loop {
                let event = match events.recv().await {
                    Ok(e) => e,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                let Some(s) = me.upgrade() else { return };
                s.download_event(&mut persons, event);
            }
        });
    }

    /// One page event for the person's downloads: `persons` holds, by guid, each download decided
    /// the person's (its job, tab and file name) until it completes or is cancelled.
    fn download_event(
        self: &Arc<Self>,
        persons: &mut HashMap<String, (String, String, String)>,
        event: super::relay::PageEvent,
    ) {
        let p = &event.params;
        let Some(guid) = p["guid"].as_str() else {
            return;
        };
        // ponytail: a page script on a timer passes for the person (relay's `person_started`), so
        // only the owner's own job's tab delivers, its one named file, quarantined, never opened.
        let own = |job: &str| self.api(|a| a.job(job).is_some_and(|j| j.origin == c::Origin::Own));
        match (event.method.as_str(), p["state"].as_str()) {
            ("Page.downloadWillBegin", _) => {
                let Some(job) = self.relay.job_of(&event.tab) else {
                    return;
                };
                if event.person_started && own(&job) {
                    // Main saves it under the suggested name's last segment (`basename`).
                    let name = p["suggestedFilename"]
                        .as_str()
                        .and_then(|n| n.rsplit('/').next())
                        .filter(|n| !n.is_empty())
                        .unwrap_or("download");
                    persons.insert(guid.into(), (job, event.tab.clone(), name.into()));
                }
            }
            ("Page.downloadProgress", Some("canceled")) => {
                persons.remove(guid);
            }
            ("Page.downloadProgress", Some("completed")) => {
                let Some((job, tab, name)) = persons.remove(guid) else {
                    return;
                };
                if own(&job) {
                    let holder = self
                        .api(|a| a.tab(&tab).and_then(|t| t.held_by.clone()))
                        .unwrap_or_else(|| THE_WINDOWS_PERSON.into());
                    self.watch_downloads(&job, Some(tab), Actor::Person(holder), Some(name));
                }
            }
            _ => {}
        }
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
            test_agent_input_window(),
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

    /// A job the API doesn't know yet: its browser `notStarted`. One it knows takes `origin` when
    /// obid's records now find it foreign (BROWSER-18): read on every call, never only the first,
    /// and never back to the owner's.
    pub fn ensure_job(&self, job: &str, agent: &str, origin: c::Origin) {
        let changed = self.api(|a| {
            if a.job(job).is_none() {
                a.add_job_for(job, agent, origin);
                true
            } else {
                a.found_foreign(job, origin)
            }
        });
        if changed {
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
        self.rooms
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(job);
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

/// How long after the last agent input a tab's download is still the agent's: T3's 5 s, but in a
/// debug build's test (`OBID_TEST_BROWSER_AGENT_INPUT_MS`), at most 10 s.
fn test_agent_input_window() -> Duration {
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var("OBID_TEST_BROWSER_AGENT_INPUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        return Duration::from_millis(ms.min(10_000));
    }
    super::relay::AGENT_INPUT_WINDOW
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A service on a scratch `home`, its person's Downloads `home/Downloads`, with an own job
    /// `j1` whose download folder holds `files`.
    fn scratch(name: &str, files: &[(&str, &[u8])]) -> (Arc<Service>, PathBuf) {
        let home = std::env::temp_dir().join(format!("oby-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let mut api = Api::new();
        api.add_job_for("j1", "codex", c::Origin::Own);
        let service = Arc::new(Service::new(
            api,
            home.clone(),
            home.join("Downloads"),
            Duration::ZERO,
            super::super::relay::AGENT_INPUT_WINDOW,
        ));
        let folder = service.window().downloads_folder("j1");
        std::fs::create_dir_all(&folder).unwrap();
        for (file, bytes) in files {
            std::fs::write(folder.join(file), bytes).unwrap();
        }
        (service, home)
    }

    /// The person's named watch, run to its end on the paused clock.
    async fn person_watch(service: &Arc<Service>, name: &str) {
        service.watch_downloads(
            "j1",
            Some("tab-1".into()),
            Actor::Person(THE_WINDOWS_PERSON.into()),
            Some(name.into()),
        );
        for _ in 0..30 {
            tokio::time::advance(Duration::from_millis(250)).await;
            tokio::task::yield_now().await;
        }
    }

    /// BROWSER-34: the person's watch takes only the file its download named, beside an agent's
    /// that stays unlisted in the folder, and a file already in Downloads keeps its bytes: the
    /// delivered one takes `stem (n).ext`, the name it is listed under.
    #[tokio::test(start_paused = true)]
    async fn browser_34_only_the_named_file_is_delivered_and_nothing_in_downloads_is_replaced() {
        let (service, home) = scratch(
            "browser-34-named",
            &[
                ("statement-oct.pdf", b"profile"),
                ("invoices.csv", b"agent"),
            ],
        );
        let downloads = home.join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::write(downloads.join("statement-oct.pdf"), b"the person's own").unwrap();
        person_watch(&service, "statement-oct.pdf").await;
        assert_eq!(
            std::fs::read(downloads.join("statement-oct (1).pdf")).unwrap(),
            b"profile"
        );
        assert_eq!(
            std::fs::read(downloads.join("statement-oct.pdf")).unwrap(),
            b"the person's own"
        );
        let folder = service.window().downloads_folder("j1");
        assert!(
            !folder.join("statement-oct.pdf").exists(),
            "moved, not copied"
        );
        assert!(folder.join("invoices.csv").is_file(), "the agent's stays");
        let listed = serde_json::to_value(service.api(|a| a.downloads())).unwrap();
        let listed = listed["downloads"].as_array().unwrap();
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0]["name"], "statement-oct (1).pdf");
        assert_eq!(
            (listed[0]["startedBy"].as_str(), listed[0]["place"].as_str()),
            (Some("person"), Some("delivered"))
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// BROWSER-34: a delivered download carries macOS's quarantine mark: the one its profile file
    /// had, or obid's when it had none.
    #[cfg(target_os = "macos")]
    #[tokio::test(start_paused = true)]
    async fn browser_34_a_delivered_download_is_quarantined() {
        use super::super::quarantine;
        let (service, home) = scratch(
            "browser-34-quarantine",
            &[("marked.dmg", b"m"), ("bare.dmg", b"b")],
        );
        let folder = service.window().downloads_folder("j1");
        let mark = "0083;6740a1b2;Chrome;";
        let set = std::process::Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.quarantine", mark])
            .arg(folder.join("marked.dmg"))
            .status()
            .unwrap();
        assert!(set.success());
        assert_eq!(quarantine::read(&folder.join("bare.dmg")), None);
        person_watch(&service, "marked.dmg").await;
        person_watch(&service, "bare.dmg").await;
        let downloads = home.join("Downloads");
        assert_eq!(
            quarantine::read(&downloads.join("marked.dmg")).as_deref(),
            Some(mark)
        );
        let obids = quarantine::read(&downloads.join("bare.dmg")).unwrap_or_default();
        assert!(obids.ends_with(";Oby;"), "{obids:?}");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// An own job the browser already knows, later found foreign in obid's records, is foreign
    /// from its next call: its origin is never only the first one heard (BROWSER-18).
    #[tokio::test]
    async fn browser_18_a_job_that_becomes_foreign_loses_its_browser() {
        let home = std::env::temp_dir().join(format!("oby-browser-18-{}", std::process::id()));
        let service = Service::open(&home).await;
        service.ensure_job("j1", "claude", c::Origin::Own);
        service.ensure_job("j1", "claude", c::Origin::Untrusted);
        assert_eq!(
            service.api(|a| a.job("j1").map(|j| j.origin.clone())),
            Some(c::Origin::Untrusted)
        );
        let answer = service
            .call_tool("j1", &home, "oby_browser_snapshot", &json!({}))
            .await;
        assert_eq!(
            answer["structuredContent"]["error"]["code"], "browser_off",
            "{answer}"
        );
        // A job found foreign never goes back to its agent's partition.
        service.ensure_job("j1", "claude", c::Origin::Own);
        assert_eq!(
            service.api(|a| a.job("j1").map(|j| j.partition.clone())),
            Some("oby-box-j1".into())
        );
    }
}
