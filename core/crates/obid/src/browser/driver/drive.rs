//! The driver of every job's browser (BROWSER-2, 23 to 25): starting a job's agent-browser daemon
//! and Chrome once the engine is in place (the core's [`Launch`]), at most [`super::MAX_LIVE`] at
//! once (a fifth waits for one to end), noticing one that closed itself after 10 minutes idle, and
//! stopping a job's at its end (SIGTERM, then SIGKILL, only processes under `<home>/browser/`).
//!
//! It owns the live browsers (in memory and in `<home>/browser/live/`), their sockets and the
//! jobs' folders; the core's state is the core's ([`Browsers`]), told through its own methods. It
//! never writes the person's own files.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::live::Live;
use super::waiting::Waiting;
use super::{Call, LiveFile, Options, Session, Stream, procs, session::pid_in};
use crate::browser::api::Launch;
use crate::browser::engine::Browsers;

/// How often a launch waiting for a place looks for a browser that closed itself.
const SWEEP_EVERY: Duration = Duration::from_secs(1);

/// The driver of every job's browser.
pub struct Driver {
    pub(super) options: Options,
    pub(super) core: Arc<Mutex<Browsers>>,
    pub(super) live: Arc<Mutex<BTreeMap<String, Live>>>,
    /// Timed-out commands' tabs settling in the background, per job: stopped at its end.
    pub(super) settling: Mutex<BTreeMap<String, Vec<tokio::task::AbortHandle>>>,
    pub(super) streams: tokio::sync::Mutex<BTreeMap<String, Arc<Stream>>>,
    places: Arc<Semaphore>,
    waiting: AtomicUsize,
    epoch: tokio::time::Instant,
}

impl Driver {
    pub fn new(options: Options, core: Arc<Mutex<Browsers>>) -> Driver {
        Driver {
            places: Arc::new(Semaphore::new(options.max_live)),
            options,
            core,
            live: Arc::new(Mutex::new(BTreeMap::new())),
            settling: Mutex::new(BTreeMap::new()),
            streams: tokio::sync::Mutex::new(BTreeMap::new()),
            waiting: AtomicUsize::new(0),
            epoch: tokio::time::Instant::now(),
        }
    }

    /// The clock the core's deadlines are on: pass it to [`Browsers::begin`].
    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// obid's home, under which every browser's folders are.
    pub fn home(&self) -> &std::path::Path {
        &self.options.home
    }

    pub fn core(&self) -> &Arc<Mutex<Browsers>> {
        &self.core
    }

    /// `<home>/browser`: every process obid may signal starts under it.
    pub(super) fn root(&self) -> PathBuf {
        self.options.home.join("browser")
    }

    /// How many browsers are live now.
    pub fn live_count(&self) -> usize {
        self.live.lock().unwrap().len()
    }

    /// The jobs whose browsers are live now, or still recorded on disk: a job whose end is under
    /// way has already left the live list but keeps its record until its processes are gone, so
    /// stopping obid ends it again rather than leaving the record behind (`end` runs twice safely).
    pub fn live_jobs(&self) -> Vec<String> {
        let mut jobs: Vec<String> = self.live.lock().unwrap().keys().cloned().collect();
        let recorded = std::fs::read_dir(LiveFile::dir(&self.options.home))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| LiveFile::read(&e.path()).map(|f| f.job));
        jobs.extend(recorded);
        jobs.sort_unstable();
        jobs.dedup();
        jobs
    }

    /// How many launches wait for a live browser to end.
    pub fn waiting(&self) -> usize {
        self.waiting.load(Ordering::SeqCst)
    }

    /// The daemon's and Chrome's PIDs of a job's live browser.
    pub fn pids(&self, job: &str) -> Option<(Option<u32>, Option<u32>)> {
        self.live
            .lock()
            .unwrap()
            .get(job)
            .map(|l| (l.daemon, l.chrome))
    }

    pub(super) fn session_of(&self, job: &str) -> Option<Session> {
        self.live
            .lock()
            .unwrap()
            .get(job)
            .map(|l| l.session.clone())
    }

    /// Whether a job's browser is live; one that closed itself (10 minutes idle) is let go first.
    /// When it isn't, the job's next browser call starts it again (`browser_up`).
    pub async fn is_live(&self, job: &str) -> bool {
        self.sweep().await;
        self.live.lock().unwrap().contains_key(job)
    }

    /// Lets go of every live browser whose daemon is gone (closed itself when idle, or died): its
    /// place frees, its file goes, and the core forgets its tabs, which went with its Chrome.
    /// Returns those jobs, for whoever shows their tabs closed.
    pub async fn sweep(&self) -> Vec<String> {
        let daemons: Vec<(String, u32)> = self
            .live
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(job, l)| Some((job.clone(), l.daemon?)))
            .collect();
        let root = self.root();
        let mut gone = vec![];
        for (job, pid) in daemons {
            if !procs::ours(pid, &root).await {
                gone.push(job);
            }
        }
        for job in &gone {
            let live = self.live.lock().unwrap().remove(job);
            if let Some(chrome) = live.as_ref().and_then(|l| l.chrome) {
                procs::stop(&[chrome], &root, self.options.end_grace).await;
            }
            LiveFile::remove(&self.options.home, job);
            if let Some(s) = self.streams.lock().await.remove(job) {
                s.close();
            }
            self.core.lock().unwrap().ended(job);
        }
        gone
    }

    /// A place among the live browsers, waiting for one to end when all are taken.
    async fn place(&self) -> OwnedSemaphorePermit {
        if let Ok(p) = self.places.clone().try_acquire_owned() {
            return p;
        }
        let _waiting = Waiting::new(&self.waiting);
        loop {
            tokio::select! {
                p = self.places.clone().acquire_owned() => {
                    if let Ok(p) = p {
                        return p;
                    }
                }
                _ = tokio::time::sleep(SWEEP_EVERY) => {}
            }
            self.sweep().await;
        }
    }

    /// Starts a job's browser: the engine put in place first if it isn't, a place among the live
    /// ones (waiting for one to end), the job's folders, then its daemon and Chrome, by a first
    /// `get url`. A job already live is left as it is.
    pub async fn start(&self, job: &str) -> Result<(), String> {
        if self.is_live(job).await {
            return Ok(());
        }
        let installed = self.options.binaries.get().await?;
        let session = Session::for_job(
            &self.options.home,
            job,
            &installed,
            &self.options.idle_timeout,
        )?;
        let place = self.place().await;
        session.make_folders()?;
        let mut file = LiveFile {
            job: job.into(),
            session: session.name.clone(),
            socket_dir: session.socket_dir.clone(),
            daemon: None,
            chrome: None,
        };
        // Written before the daemon starts: an obid stopped right after still finds its sockets.
        file.write(&self.options.home)?;
        match session
            .call(&["get".into(), "url".into()], self.options.start_limit)
            .await
        {
            Call::Ok(_) => {}
            Call::Failed(e) => return Err(e),
            Call::TimedOut(ms) => {
                return Err(format!(
                    "agent-browser didn't start the browser within {ms} ms"
                ));
            }
        }
        let daemon = match session.daemon_pid() {
            Some(pid) => Some(pid),
            None => match session
                .call(
                    &["session".into(), "info".into()],
                    self.options.system_limit,
                )
                .await
            {
                Call::Ok(v) => v["pid"].as_u64().map(|p| p as u32),
                _ => None,
            },
        };
        let chrome = match daemon {
            Some(pid) => procs::children(pid).await.into_iter().next(),
            None => None,
        };
        file.daemon = daemon;
        file.chrome = chrome;
        file.write(&self.options.home)?;
        self.live.lock().unwrap().insert(
            job.into(),
            Live {
                session,
                daemon,
                chrome,
                _place: place,
            },
        );
        Ok(())
    }

    /// A job ended: its settling reads stop, its stream closes, its daemon gets SIGTERM (its Chrome
    /// follows), then SIGKILL for whatever of it is left, its file and sockets go, its place frees,
    /// and the core forgets it. Its profile stays (its downloads), until the job is cleaned up.
    pub async fn end(&self, job: &str) {
        for task in self
            .settling
            .lock()
            .unwrap()
            .remove(job)
            .into_iter()
            .flatten()
        {
            task.abort();
        }
        if let Some(s) = self.streams.lock().await.remove(job) {
            s.close();
        }
        let live = self.live.lock().unwrap().remove(job);
        let home = &self.options.home;
        let file = LiveFile::read(&LiveFile::path(home, job));
        let socket_dir = Session::socket_dir_of(home, job);
        let mut daemons: Vec<u32> = [
            live.as_ref().and_then(|l| l.daemon),
            file.as_ref().and_then(|f| f.daemon),
            pid_in(&socket_dir, &Session::name_of(home, job)),
        ]
        .into_iter()
        .flatten()
        .collect();
        daemons.sort_unstable();
        daemons.dedup();
        let mut pids = daemons.clone();
        pids.extend(live.as_ref().and_then(|l| l.chrome));
        pids.extend(file.as_ref().and_then(|f| f.chrome));
        for &d in &daemons {
            pids.extend(procs::children(d).await);
        }
        procs::stop(&pids, &self.root(), self.options.end_grace).await;
        LiveFile::remove(home, job);
        let _ = std::fs::remove_dir_all(&socket_dir);
        // Its place frees only now, once its processes are gone.
        drop(live);
        self.core.lock().unwrap().ended(job);
    }

    /// The JSON the core's refusals take, with how long the call took and its command.
    pub(super) fn answer_refused(
        refusal: crate::browser::engine::Refusal,
        started: tokio::time::Instant,
        cmd: u64,
        tab: Option<&str>,
    ) -> serde_json::Value {
        let mut v = refusal.to_json();
        v["elapsedMs"] = json!(started.elapsed().as_millis() as u64);
        v["cmd"] = json!(cmd);
        if let Some(t) = tab {
            v["tab"] = json!(t);
        }
        v
    }
}

impl Launch for Driver {
    async fn launch(&self, job: &str) -> Result<(), String> {
        self.start(job).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Binaries, stand_in};
    use super::*;
    use crate::browser::api::{Api, browser_up};
    use crate::browser::contract as c;

    fn driver(home: &std::path::Path) -> Arc<Driver> {
        let installed = stand_in::install(home);
        let mut options = Options::new(home.to_path_buf(), Binaries::InPlace(installed));
        options.end_grace = Duration::from_secs(2);
        Arc::new(Driver::new(options, Arc::new(Mutex::new(Browsers::new()))))
    }

    async fn alive(home: &std::path::Path, pid: Option<u32>) -> bool {
        match pid {
            Some(p) => procs::ours(p, &home.join("browser")).await,
            None => false,
        }
    }

    const WAIT: Duration = Duration::from_secs(10);

    #[tokio::test]
    async fn browser_24_one_browser_per_job_its_own_profile_and_ten_minutes_idle() {
        let home = stand_in::scratch("one");
        let d = driver(&home);
        d.start("job-a").await.unwrap();
        d.start("job-b").await.unwrap();
        // A job already live isn't started again.
        d.start("job-a").await.unwrap();
        assert_eq!(d.live_count(), 2);
        let (a, b) = (d.pids("job-a").unwrap(), d.pids("job-b").unwrap());
        assert!(a.0.is_some() && a.1.is_some(), "{a:?}");
        assert_ne!(a.0, b.0);
        assert!(alive(&home, a.0).await && alive(&home, a.1).await);
        let calls = stand_in::calls(&home, "job-a");
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert!(calls[0].contains("--idle-timeout 10m"), "{}", calls[0]);
        assert!(
            calls[0].contains(&format!(
                "--profile {}",
                Session::job_dir(&home, "job-a").join("profile").display()
            )),
            "{}",
            calls[0]
        );
        let file = LiveFile::read(&LiveFile::path(&home, "job-a")).unwrap();
        assert_eq!((file.daemon, file.chrome), a);
        d.end("job-a").await;
        d.end("job-b").await;
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_24_four_live_at_once_and_a_fifth_gets_in_when_one_ends() {
        let home = stand_in::scratch("four");
        let d = driver(&home);
        for j in ["j1", "j2", "j3", "j4"] {
            d.start(j).await.unwrap();
        }
        let fifth = tokio::spawn({
            let d = d.clone();
            async move { d.start("j5").await }
        });
        // The fifth waits for a place, with four live.
        assert!(stand_in::within(WAIT, || async { d.waiting() == 1 }).await);
        assert_eq!(d.live_count(), 4);
        assert!(!fifth.is_finished());
        assert!(stand_in::calls(&home, "j5").is_empty());
        // Within browser_up's allowance it is `browser_starting`, never a bare timeout.
        let api = Mutex::new(Api::new());
        api.lock().unwrap().add_job("j5", "claude");
        let e = browser_up(&api, &*d, "j5", Duration::from_millis(300), &|| ())
            .await
            .unwrap_err();
        assert_eq!(e.reason, "browser_starting");
        // One ends: the fifth gets in.
        d.end("j2").await;
        fifth.await.unwrap().unwrap();
        assert_eq!(d.live_count(), 4);
        assert!(d.pids("j5").is_some() && d.pids("j2").is_none());
        browser_up(&api, &*d, "j5", Duration::from_secs(5), &|| ())
            .await
            .unwrap();
        assert_eq!(
            api.lock().unwrap().job("j5").unwrap().browser,
            c::BrowserState::Ready
        );
        for j in ["j1", "j3", "j4", "j5"] {
            d.end(j).await;
        }
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_24_a_browser_that_closed_itself_frees_its_place_and_starts_again() {
        let home = stand_in::scratch("idle");
        let d = driver(&home);
        d.start("job-a").await.unwrap();
        let (daemon, chrome) = d.pids("job-a").unwrap();
        // Ten minutes idle: agent-browser closes its daemon, and Chrome with it.
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(daemon.unwrap() as i32),
            nix::sys::signal::Signal::SIGTERM,
        );
        assert!(stand_in::within(WAIT, || async { !alive(&home, chrome).await }).await);
        assert!(!d.is_live("job-a").await);
        assert_eq!(d.live_count(), 0);
        // The job's next call starts it again.
        d.start("job-a").await.unwrap();
        let (again, _) = d.pids("job-a").unwrap();
        assert_ne!(again, daemon);
        assert!(alive(&home, again).await);
        d.end("job-a").await;
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_25_a_jobs_end_leaves_no_daemon_or_chrome_alive_and_the_core_forgets_it() {
        let home = stand_in::scratch("end");
        let d = driver(&home);
        d.start("job-a").await.unwrap();
        d.core()
            .lock()
            .unwrap()
            .opened("job-a", "T1", "http://127.0.0.1:1/", "Fixture");
        let (daemon, chrome) = d.pids("job-a").unwrap();
        let socket_dir = Session::socket_dir_of(&home, "job-a");
        assert!(socket_dir.is_dir());
        d.end("job-a").await;
        assert!(!alive(&home, daemon).await, "the daemon is gone");
        assert!(!alive(&home, chrome).await, "its Chrome is gone");
        assert!(!LiveFile::path(&home, "job-a").exists());
        assert!(!socket_dir.exists());
        assert!(d.core().lock().unwrap().tabs_of("job-a").is_empty());
        assert_eq!(d.live_count(), 0);
        // Its downloads stay in its profile.
        assert!(Session::downloads_of(&home, "job-a").is_dir());
        let _ = std::fs::remove_dir_all(home);
    }

    /// A job whose end is under way has left the live list but keeps its record: stopping obid
    /// still finds it, so its record and processes don't outlive obid (BROWSER-25).
    #[tokio::test]
    async fn browser_25_a_job_still_recorded_is_among_the_live_ones() {
        let home = stand_in::scratch("recorded");
        let d = driver(&home);
        assert!(d.live_jobs().is_empty());
        LiveFile {
            job: "job-ending".into(),
            session: "s".into(),
            socket_dir: home.join("none"),
            daemon: None,
            chrome: None,
        }
        .write(&home)
        .unwrap();
        assert_eq!(d.live_jobs(), vec!["job-ending".to_string()]);
        d.end("job-ending").await;
        assert!(d.live_jobs().is_empty(), "ending it removed its record");
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_25_a_daemon_that_ignores_sigterm_is_killed() {
        let home = stand_in::scratch("stubborn");
        let d = driver(&home);
        let root = home.join("browser");
        // A daemon of obid's that won't stop on SIGTERM, double-forked as agent-browser's is.
        let pidfile = home.join("stubborn.pid");
        let name = root.join("agent-browser-0.43.0/package/bin/agent-browser");
        let status = std::process::Command::new("/bin/bash")
            .arg("-c")
            .arg(format!(
                "( exec -a '{}' /bin/bash -c 'trap \"\" TERM; echo $$ > {}; while :; do /bin/sleep 1; done' </dev/null >/dev/null 2>&1 & )",
                name.display(),
                pidfile.display()
            ))
            .status()
            .unwrap();
        assert!(status.success());
        // The shell creates the file before it writes the pid: wait for the pid itself, or a read
        // between the two finds the file empty.
        let read_pid = || {
            std::fs::read_to_string(&pidfile)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
        };
        assert!(stand_in::within(WAIT, || async { read_pid().is_some() }).await);
        let pid = read_pid().unwrap();
        LiveFile {
            job: "job-a".into(),
            session: "s".into(),
            socket_dir: home.join("none"),
            daemon: Some(pid),
            chrome: None,
        }
        .write(&home)
        .unwrap();
        assert!(procs::ours(pid, &root).await);
        d.end("job-a").await;
        assert!(!procs::ours(pid, &root).await, "SIGKILL after the grace");
        let _ = std::fs::remove_dir_all(home);
    }
}
