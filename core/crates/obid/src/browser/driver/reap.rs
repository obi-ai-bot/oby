//! Browsers left from before obid restarted (BROWSER-25): every `<home>/browser/live/<job>.json`
//! names a daemon, its Chrome and its socket folder; each of those still alive and still one of
//! obid's browser processes (its command under `<home>/browser/`) is stopped, never a PID reused
//! by anything else, and the files and sockets go. It writes nothing; it removes only those files
//! and the jobs' socket folders.

use std::path::Path;

use super::session::pid_in;
use super::{END_GRACE, LiveFile, Session, procs};

/// Stops every browser a previous obid left under `home`, and removes what recorded it. Returns
/// the processes it signalled.
pub async fn reap_at_start(home: &Path) -> Vec<u32> {
    let root = home.join("browser");
    let mut stopped = vec![];
    let entries: Vec<_> = std::fs::read_dir(LiveFile::dir(home))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    for path in entries {
        let Some(file) = LiveFile::read(&path) else {
            let _ = std::fs::remove_file(&path);
            continue;
        };
        // The socket folder as obid names it for this job, not as the file says.
        let socket_dir = Session::socket_dir_of(home, &file.job);
        let mut daemons: Vec<u32> = [file.daemon, pid_in(&socket_dir, &file.session)]
            .into_iter()
            .flatten()
            .collect();
        daemons.sort_unstable();
        daemons.dedup();
        let mut pids = daemons.clone();
        pids.extend(file.chrome);
        for &d in &daemons {
            pids.extend(procs::children(d).await);
        }
        stopped.extend(procs::stop(&pids, &root, END_GRACE).await);
        let _ = std::fs::remove_dir_all(&socket_dir);
        let _ = std::fs::remove_file(&path);
    }
    stopped
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::super::{Binaries, Driver, Options, stand_in};
    use super::*;
    use crate::browser::engine::Browsers;

    /// A process started double-forked (not this test's child) whose command line begins with
    /// `argv0`: a stand-in for a daemon an earlier obid left, or for anything else.
    async fn orphan(argv0: &str, pidfile: &Path) -> u32 {
        let status = std::process::Command::new("/bin/bash")
            .arg("-c")
            .arg(format!(
                "( bash -c 'echo $$ > \"{}\"; exec -a \"{}\" /bin/sleep 100000' </dev/null >/dev/null 2>&1 & )",
                pidfile.display(),
                argv0
            ))
            .status()
            .unwrap();
        assert!(status.success());
        let read = || {
            std::fs::read_to_string(pidfile)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
        };
        assert!(stand_in::within(Duration::from_secs(10), || async { read().is_some() }).await);
        read().unwrap()
    }

    #[tokio::test]
    async fn browser_25_a_daemon_left_from_before_obid_restarted_is_stopped_at_its_start() {
        let home = stand_in::scratch("reap");
        let root = home.join("browser");
        // An obid that stopped without ending its job: its browser is still up.
        {
            let installed = stand_in::install(&home);
            let d = Driver::new(
                Options::new(home.clone(), Binaries::InPlace(installed)),
                Arc::new(Mutex::new(Browsers::new())),
            );
            d.start("job-a").await.unwrap();
        }
        let file = LiveFile::read(&LiveFile::path(&home, "job-a")).unwrap();
        let (daemon, chrome) = (file.daemon.unwrap(), file.chrome.unwrap());
        assert!(procs::ours(daemon, &root).await && procs::ours(chrome, &root).await);
        // A PID the file names that is now something else's: never signalled.
        let other = orphan("/bin/sleep", &home.join("other.pid")).await;
        LiveFile {
            job: "job-b".into(),
            session: Session::name_of(&home, "job-b"),
            socket_dir: home.join("none"),
            daemon: Some(other),
            chrome: None,
        }
        .write(&home)
        .unwrap();
        let stopped = reap_at_start(&home).await;
        assert!(stopped.contains(&daemon), "{stopped:?}");
        assert!(!stopped.contains(&other), "{stopped:?}");
        assert!(!procs::ours(daemon, &root).await, "the daemon is gone");
        assert!(!procs::ours(chrome, &root).await, "its Chrome is gone");
        assert!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(other as i32), None).is_ok(),
            "the other process is still alive"
        );
        assert_eq!(std::fs::read_dir(LiveFile::dir(&home)).unwrap().count(), 0);
        assert!(!Session::socket_dir_of(&home, "job-a").exists());
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(other as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
        let _ = std::fs::remove_dir_all(home);
    }
}
