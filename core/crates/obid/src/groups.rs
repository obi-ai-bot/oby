//! The process groups obid starts for a job (each turn of its agent, its check), recorded on disk
//! while they run, so that what an obid that went down without ending them (a crash, a kill, a
//! panic) left running ends at its next start, before any job is picked up again
//! (docs/runner-api.md, "Restarts"). Each leads a session of its own (the job sandbox's first
//! program calls `setsid`), so its group holds everything it started.
//!
//! A record names the group's leader by its id and its start time (`ps`'s on macOS, `/proc`'s with
//! the boot's id on Linux): a process that took the id since started later, so it's never
//! signalled. The leader's executable is kept for the log only: the leader replaces itself on
//! purpose (`obid __session`, then the sandbox, then the agent), so its executable changes while
//! its start time doesn't.
//!
//! A run also holds a lock in its worktree's own git folder ([`LOCK`]) naming the same leader: a
//! new run there ends a live one obid recorded for the same job first, and refuses to start beside
//! any other. The agent can write that folder, and `ps` shows it every other job's leader, so the
//! lock never says what to signal: only obid's own record of this job's group, in its home, does.

use std::path::{Path, PathBuf};

use nix::sys::signal::Signal;

/// The lock's name in a worktree's own git folder.
pub const LOCK: &str = "obid-run";

/// Where obid records the groups it runs for jobs: `home/jobs/groups`.
pub fn dir(home: &Path) -> PathBuf {
    home.join("jobs").join("groups")
}

/// Where a run of a job records its group.
#[derive(Debug, Clone)]
pub struct Recording {
    /// obid's records ([`dir`]): the only ones [`reap`] and [`take`] act on.
    pub records: PathBuf,
    /// The job: the only one whose groups its runs may end ([`take`]).
    pub job: String,
    /// The worktree's own git folder, where the lock goes: the one obid made, never what the
    /// worktree's `.git` names.
    pub git_admin: Option<PathBuf>,
}

/// A group's leader as recorded.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Leader {
    pub pgid: u32,
    /// When it started: `ps`'s `lstart` in UTC on macOS, the boot's id and `/proc`'s start time
    /// (clock ticks since boot) on Linux.
    pub started: String,
    /// Its executable when it was recorded.
    pub exe: String,
    pub job: String,
}

/// A process: when it started and its executable now. None when there's no such process, or it
/// has ended and only waits to be reaped.
#[cfg(target_os = "linux")]
fn identify(pid: u32) -> Option<(String, String)> {
    let boot = match std::fs::read_to_string("/proc/sys/kernel/random/boot_id") {
        Ok(boot) => boot,
        Err(e) => {
            unidentified(&format!("/proc/sys/kernel/random/boot_id: {e}"));
            return None;
        }
    };
    // The fields after the name, which is in parentheses and can hold either: the state (field 3),
    // then the start time (field 22).
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = stat
        .get(stat.rfind(')')? + 1..)?
        .split_whitespace()
        .collect();
    if fields.first()?.starts_with('Z') {
        return None;
    }
    let exe = std::fs::read_link(format!("/proc/{pid}/exe"))
        .map(|exe| exe.display().to_string())
        .unwrap_or_default();
    Some((format!("{} {}", boot.trim(), fields.get(19)?), exe))
}

/// [`identify`] elsewhere: `ps`.
#[cfg(not(target_os = "linux"))]
fn identify(pid: u32) -> Option<(String, String)> {
    let out = match std::process::Command::new("/bin/ps")
        .args(["-o", "lstart=", "-o", "stat=", "-o", "comm=", "-p"])
        .arg(pid.to_string())
        .env_clear()
        .env("TZ", "UTC")
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(out) => out,
        Err(e) => {
            unidentified(&format!("/bin/ps: {e}"));
            return None;
        }
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut words = text.split_whitespace();
    let started: Vec<&str> = words.by_ref().take(5).collect();
    let state = words.next()?;
    (started.len() == 5 && !state.starts_with('Z'))
        .then(|| (started.join(" "), words.collect::<Vec<_>>().join(" ")))
}

/// Said once: without start times, obid records no group, so nothing is ended after a crash.
fn unidentified(why: &str) {
    static SAID: std::sync::Once = std::sync::Once::new();
    SAID.call_once(|| {
        tracing::warn!(
            "obid can't read its processes' start times ({why}), so it records no job's process group: what a crash leaves running isn't ended at the next start"
        )
    });
}

/// `leader` still runs, the very process recorded.
fn alive(leader: &Leader) -> bool {
    leader.pgid > 1 && identify(leader.pgid).is_some_and(|(started, _)| started == leader.started)
}

/// Held while a group runs ([`record`]): its record and its lock go when it's dropped.
#[derive(Default)]
pub struct Recorded {
    record: Option<PathBuf>,
    lock: Option<(PathBuf, Leader)>,
}

impl Drop for Recorded {
    fn drop(&mut self) {
        if let Some(record) = &self.record {
            let _ = std::fs::remove_file(record);
        }
        // Only while it's still this run's: never a later run's.
        if let Some((lock, leader)) = &self.lock
            && read_lock(lock).as_ref() == Some(leader)
        {
            let _ = std::fs::remove_file(lock);
        }
    }
}

/// Records the group `pgid` leads (just started) until the [`Recorded`] is dropped: in obid's
/// records (written whole, 0600), and as the lock in its worktree's git folder. A record that
/// can't be written is logged, and the run goes on. Off the async workers: it runs `ps` and syncs
/// the record to disk.
pub async fn record(recording: &Recording, pgid: u32) -> Recorded {
    let recording = recording.clone();
    tokio::task::spawn_blocking(move || record_now(&recording, pgid))
        .await
        .unwrap_or_default()
}

fn record_now(recording: &Recording, pgid: u32) -> Recorded {
    let mut recorded = Recorded::default();
    let Some((started, exe)) = (pgid > 1).then(|| identify(pgid)).flatten() else {
        return recorded;
    };
    let leader = Leader {
        pgid,
        started,
        exe,
        job: recording.job.clone(),
    };
    let bytes = serde_json::to_vec(&leader).unwrap_or_default();
    let file = recording.records.join(format!("{pgid}.json"));
    match crate::store::make_private_dir(&recording.records)
        .and_then(|()| crate::store::write_private(&file, &bytes))
    {
        Ok(()) => recorded.record = Some(file),
        Err(e) => tracing::warn!(
            "job {}: obid can't record its process group {pgid}: {e}",
            recording.job
        ),
    }
    if let Some(admin) = &recording.git_admin {
        let lock = admin.join(LOCK);
        if write_lock(&lock, &bytes).is_ok() {
            recorded.lock = Some((lock, leader));
        }
    }
    recorded
}

/// Before a run starts in the worktree whose git folder `recording` names: a live run that holds
/// its lock ends first when obid recorded it for this same job, else this one doesn't start (why,
/// as the error).
pub async fn take(recording: &Recording) -> Result<(), String> {
    let asked = recording.clone();
    let Some(held) = tokio::task::spawn_blocking(move || holder(&asked))
        .await
        .map_err(|e| e.to_string())??
    else {
        return Ok(());
    };
    tracing::warn!(
        "job {}: its earlier run ({}, group {}) was still at work in its worktree: ending it first",
        held.job,
        held.exe,
        held.pgid
    );
    crate::process::signal_group(held.pgid, Signal::SIGTERM);
    tokio::time::sleep(crate::process::STOPPING).await;
    let _ = tokio::task::spawn_blocking(move || kill_unless_replaced(&held)).await;
    Ok(())
}

/// The live run that holds the lock in `recording`'s worktree, when obid recorded it for this same
/// job: [`take`] ends it. Never one recorded for another job, whatever the lock says: the agent
/// writes the lock, and `ps` shows it every job's leader.
fn holder(recording: &Recording) -> Result<Option<Leader>, String> {
    let Some(held) = recording
        .git_admin
        .as_ref()
        .and_then(|admin| read_lock(&admin.join(LOCK)))
        .filter(alive)
    else {
        return Ok(None);
    };
    let recorded: Option<Leader> =
        std::fs::read(recording.records.join(format!("{}.json", held.pgid)))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    if held.job == recording.job && recorded.as_ref() == Some(&held) {
        return Ok(Some(held));
    }
    Err(format!(
        "an earlier run is still at work in this job's worktree (process {}), which obid didn't start for this job",
        held.pgid
    ))
}

/// SIGKILL to `leader`'s group, unless its id is another process's now. A leader that ended since
/// its SIGTERM can leave its group behind, and no new process takes the id while that group lives.
fn kill_unless_replaced(leader: &Leader) {
    if identify(leader.pgid).is_none_or(|(started, _)| started == leader.started) {
        crate::process::signal_group(leader.pgid, Signal::SIGKILL);
    }
}

/// At obid's start, before any job is picked up again: every group recorded in `records` whose
/// leader is still the process recorded ends (SIGTERM, then SIGKILL), and the records go. A leader
/// that's gone or another process now is left alone.
// ponytail: a group whose leader ended but whose members still run (a `claudeOwnSandbox` run's
// background command) isn't ended: once the id is free, any process can lead a group with it.
// Ending one needs its members recorded.
pub fn reap(records: &Path) {
    let live: Vec<Leader> = crate::store::load_dir::<Leader>(records)
        .into_iter()
        .filter(alive)
        .collect();
    for leader in &live {
        tracing::warn!(
            "job {}: ending what obid left running when it last went down ({}, group {})",
            leader.job,
            leader.exe,
            leader.pgid
        );
        crate::process::signal_group(leader.pgid, Signal::SIGTERM);
    }
    if !live.is_empty() {
        std::thread::sleep(crate::process::STOPPING);
        live.iter().for_each(kill_unless_replaced);
    }
    let _ = std::fs::remove_dir_all(records);
}

/// The lock at `path` as a run wrote it, or none. Read without following a link and without
/// waiting on a pipe: the agent can put anything there.
fn read_lock(path: &Path) -> Option<Leader> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(4096).read_to_end(&mut bytes).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Writes the lock whole: a new file of obid's (never one that's there, a link included), renamed
/// into place.
fn write_lock(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_extension(format!("{}-{n}", std::process::id()));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .and_then(|mut file| std::io::Write::write_all(&mut file, bytes))
        .and_then(|()| std::fs::rename(&temporary, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("obid-groups-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// A group in a session of its own, as the job sandbox starts one: `sleep`.
    fn session() -> std::process::Child {
        use std::os::unix::process::CommandExt;
        std::process::Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap()
    }

    /// What obid recorded and is still that process ends at the next start; a recorded id that's
    /// another process now (it started at another time: the id was reused) is never signalled.
    #[tokio::test]
    async fn a_reused_id_is_never_signalled() {
        let records = folder("reap");
        let recording = Recording {
            records: records.clone(),
            job: "j1".into(),
            git_admin: None,
        };
        let mut left = session();
        let kept = record(&recording, left.id()).await;
        assert!(records.join(format!("{}.json", left.id())).is_file());
        // obid went down: nothing dropped its record.
        std::mem::forget(kept);
        let mut other = session();
        let reused = Leader {
            pgid: other.id(),
            started: "Thu Jan  1 00:00:00 1970".into(),
            exe: "/bin/sleep".into(),
            job: "j2".into(),
        };
        crate::store::write_private(
            &records.join(format!("{}.json", other.id())),
            &serde_json::to_vec(&reused).unwrap(),
        )
        .unwrap();

        reap(&records);
        assert!(left.wait().unwrap().code().is_none(), "ended by a signal");
        assert!(
            other.try_wait().unwrap().is_none(),
            "another process: left running"
        );
        assert!(!records.exists());
        let _ = other.kill();
        let _ = other.wait();
        let _ = std::fs::remove_dir_all(&records);
    }

    /// A run's lock in its worktree's git folder: a live run obid recorded ends before the next
    /// starts there; one it didn't record makes the next refuse; a run's end takes its lock away.
    #[tokio::test]
    async fn a_worktree_takes_one_run_at_a_time() {
        let root = folder("lock");
        let admin = root.join("admin");
        std::fs::create_dir_all(&admin).unwrap();
        let recording = Recording {
            records: root.join("records"),
            job: "j1".into(),
            git_admin: Some(admin.clone()),
        };
        let mut first = session();
        let held = record(&recording, first.id()).await;
        assert_eq!(read_lock(&admin.join(LOCK)).unwrap().pgid, first.id());
        take(&recording).await.unwrap();
        assert!(first.wait().unwrap().code().is_none(), "ended by a signal");
        drop(held);
        assert!(!admin.join(LOCK).exists());

        // A lock naming a live process obid has no record of: the run refuses, and nothing is
        // signalled.
        let mut stranger = session();
        let recorded = record(&recording, stranger.id()).await;
        std::fs::remove_file(recording.records.join(format!("{}.json", stranger.id()))).unwrap();
        let refused = take(&recording).await.unwrap_err();
        assert!(refused.contains("didn't start for this job"), "{refused}");
        assert!(stranger.try_wait().unwrap().is_none());
        drop(recorded);
        let _ = stranger.kill();
        let _ = stranger.wait();

        // A link in the lock's place is never followed.
        std::os::unix::fs::symlink("/etc/hosts", admin.join(LOCK)).unwrap();
        assert!(read_lock(&admin.join(LOCK)).is_none());
        take(&recording).await.unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An agent can read another job's leader with `ps` (its id, start time and job) and write it
    /// into its own worktree's lock: the next run there refuses, and the other job's run is never
    /// signalled, whether the lock names that job or claims its own.
    #[tokio::test]
    async fn a_lock_never_ends_another_jobs_run() {
        let root = folder("other-job");
        let (mine, theirs) = (root.join("mine"), root.join("theirs"));
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::create_dir_all(&theirs).unwrap();
        let recording = |job: &str, admin: &Path| Recording {
            records: root.join("records"),
            job: job.into(),
            git_admin: Some(admin.to_path_buf()),
        };
        let mut other = session();
        let running = record(&recording("j2", &theirs), other.id()).await;
        let theirs_lock = read_lock(&theirs.join(LOCK)).unwrap();
        let claimed = Leader {
            job: "j1".into(),
            ..theirs_lock.clone()
        };
        for forged in [theirs_lock, claimed] {
            write_lock(&mine.join(LOCK), &serde_json::to_vec(&forged).unwrap()).unwrap();
            let refused = take(&recording("j1", &mine)).await.unwrap_err();
            assert!(refused.contains("didn't start for this job"), "{refused}");
            assert!(
                other.try_wait().unwrap().is_none(),
                "another job's run: left running"
            );
        }
        drop(running);
        let _ = other.kill();
        let _ = other.wait();
        let _ = std::fs::remove_dir_all(&root);
    }
}
