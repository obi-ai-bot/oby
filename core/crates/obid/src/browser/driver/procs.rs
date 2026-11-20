//! The browser's processes as the system sees them (BROWSER-25): a process's command line, whether
//! it is one of obid's browser processes (its command starts under `<home>/browser/`, so a PID
//! reused by anything else is never signalled), its children, and stopping a set of them: SIGTERM,
//! a grace, then SIGKILL for what's left. It owns no state and writes nothing.

use std::path::Path;
use std::time::Duration;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

fn ps() -> &'static str {
    if Path::new("/bin/ps").exists() {
        "/bin/ps"
    } else {
        "/usr/bin/ps"
    }
}

/// A live process's command line, its words joined by spaces (its first word is its `argv[0]`).
pub async fn command_line(pid: u32) -> Option<String> {
    if cfg!(target_os = "linux") {
        let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let line = String::from_utf8_lossy(&raw).replace('\0', " ");
        let line = line.trim();
        return (!line.is_empty()).then(|| line.to_string());
    }
    let out = tokio::process::Command::new(ps())
        .args(["-ww", "-o", "command=", "-p", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !line.is_empty()).then_some(line)
}

/// Whether `pid` is alive and one of obid's browser processes: its command starts under `root`
/// (`<home>/browser`). A zombie's or another program's never does.
pub async fn ours(pid: u32, root: &Path) -> bool {
    if pid <= 1 {
        return false;
    }
    let prefix = format!("{}/", root.display());
    command_line(pid)
        .await
        .is_some_and(|line| line.starts_with(&prefix))
}

/// The processes whose parent is `pid`.
pub async fn children(pid: u32) -> Vec<u32> {
    let Ok(out) = tokio::process::Command::new(ps())
        .args(["-axo", "pid=,ppid="])
        .stdin(std::process::Stdio::null())
        .output()
        .await
    else {
        return vec![];
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let child = it.next()?.parse::<u32>().ok()?;
            let parent = it.next()?.parse::<u32>().ok()?;
            (parent == pid).then_some(child)
        })
        .collect()
}

async fn still_ours(pids: &[u32], root: &Path) -> Vec<u32> {
    let mut alive = vec![];
    for &pid in pids {
        if ours(pid, root).await {
            alive.push(pid);
        }
    }
    alive
}

/// Stops those of `pids` that are obid's browser processes under `root`: SIGTERM (a daemon takes
/// its Chrome down with it), up to `grace` for them to go, then SIGKILL for what's left. Returns
/// the ones it signalled.
pub async fn stop(pids: &[u32], root: &Path, grace: Duration) -> Vec<u32> {
    let mut pids = pids.to_vec();
    pids.sort_unstable();
    pids.dedup();
    let mine = still_ours(&pids, root).await;
    for &pid in &mine {
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    }
    let until = tokio::time::Instant::now() + grace;
    let mut left = mine.clone();
    while !left.is_empty() && tokio::time::Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(50)).await;
        left = still_ours(&left, root).await;
    }
    for &pid in &left {
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
    }
    let until = tokio::time::Instant::now() + Duration::from_secs(2);
    while !left.is_empty() && tokio::time::Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(50)).await;
        left = still_ours(&left, root).await;
    }
    mine
}
