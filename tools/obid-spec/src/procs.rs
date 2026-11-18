//! Every process the suite starts, by PID, so every one is stopped: the Mac is shared, and a
//! scratch obid or Sphere left running is the next session's problem. Each starts in its own
//! process group, and stopping it signals the whole group (obid's agents, wrangler's workerd).

use std::sync::Mutex;
use std::time::{Duration, Instant};

static STARTED: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());

pub fn remember(pid: u32, what: &str) {
    STARTED.lock().unwrap().push((pid, what.to_string()));
}

pub fn ours(pid: u32) -> bool {
    STARTED.lock().unwrap().iter().any(|(p, _)| *p == pid)
}

pub fn forget(pid: u32) {
    STARTED.lock().unwrap().retain(|(p, _)| *p != pid);
}

pub fn alive(pid: u32) -> bool {
    // Signal 0 checks only that the process is there (and ours to signal).
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn group_alive(pid: u32) -> bool {
    unsafe { libc::kill(-(pid as i32), 0) == 0 }
}

/// SIGTERM to the process's group, then SIGKILL to what's left after `grace`. True when the
/// leader is gone. Reaps the leader when it's this process's child.
pub async fn stop(pid: u32, grace: Duration) -> bool {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let until = Instant::now() + grace;
    while Instant::now() < until && (alive(pid) || group_alive(pid)) {
        reap(pid);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if group_alive(pid) || alive(pid) {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
            libc::kill(pid as i32, libc::SIGKILL);
        }
        for _ in 0..50 {
            reap(pid);
            if !alive(pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    reap(pid);
    forget(pid);
    !alive(pid)
}

fn reap(pid: u32) {
    let mut status = 0;
    unsafe {
        libc::waitpid(pid as i32, &mut status, libc::WNOHANG);
    }
}

/// Stops everything still remembered: what a workflow left, at its end, whatever it ended in.
pub async fn stop_all() -> Vec<String> {
    let all: Vec<(u32, String)> = STARTED.lock().unwrap().drain(..).collect();
    let mut stopped = vec![];
    for (pid, what) in all.into_iter().rev() {
        stop(pid, Duration::from_secs(10)).await;
        stopped.push(format!("{what} ({pid})"));
    }
    stopped
}
