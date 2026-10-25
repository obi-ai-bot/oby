//! What this machine's memory holds and what each job's processes use (`GET /v1/status`
//! `memory`, docs/runner-api.md). Read from `sysctl`, `vm_stat` and `ps` on macOS and from `/proc`
//! on Linux: the system calls behind them would need the `unsafe` this workspace forbids, and a
//! read costs a few milliseconds, made only while jobs run or when status is asked.

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How hard the machine is pressed for memory: the kernel's own level on macOS
/// (`kern.memorystatus_vm_pressure_level`), PSI and `MemAvailable` on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Pressure {
    Normal,
    Warn,
    Critical,
}

/// The machine's memory, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Machine {
    pub total: u64,
    /// What new work can take without the system compressing or swapping what's there: free
    /// memory plus what it drops for free (cached files, purgeable memory). On macOS, the total
    /// less Activity Monitor's Memory Used (app memory, wired and compressed); on Linux,
    /// `MemAvailable`.
    pub available: u64,
    pub pressure: Pressure,
}

/// The processes obid started that lead a session of their own (an agent, a check: the job
/// sandbox's first program calls `setsid`), with the folder each runs in and whether it's a check:
/// its tree is it, what descends from it, and what's still in its process group.
static ROOTS: Mutex<Vec<(u32, PathBuf, bool)>> = Mutex::new(Vec::new());

/// A session leader obid measures while this lives ([`root`]).
pub struct Root(u32);

/// Measures `pid`'s tree as running in `cwd` until the [`Root`] is dropped. Folders are compared
/// as their real paths (`/private/var`, not `/var`).
pub fn root(pid: u32, cwd: &Path) -> Root {
    rooted(pid, cwd, false)
}

/// [`root`] for a check: measured apart from the agent ([`roots_in`]).
pub fn check_root(pid: u32, cwd: &Path) -> Root {
    rooted(pid, cwd, true)
}

fn rooted(pid: u32, cwd: &Path, check: bool) -> Root {
    if pid > 1 {
        roots().push((pid, real(cwd), check));
    }
    Root(pid)
}

fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

impl Drop for Root {
    fn drop(&mut self) {
        roots().retain(|(pid, _, _)| *pid != self.0);
    }
}

fn roots() -> std::sync::MutexGuard<'static, Vec<(u32, PathBuf, bool)>> {
    ROOTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The roots running in `folder` or below it: the checks', or the others'.
pub fn roots_in(folder: &Path, checks: bool) -> Vec<u32> {
    let folder = real(folder);
    roots()
        .iter()
        .filter(|(_, cwd, check)| *check == checks && cwd.starts_with(&folder))
        .map(|(pid, _, _)| *pid)
        .collect()
}

/// A process: its parent, its group and its resident memory in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    pub rss: u64,
}

/// The resident memory of `roots`' trees together: each root, every process descended from
/// one, and every process still in a root's group (one whose parent ended), each counted once.
pub fn tree_rss(table: &[Process], roots: &[u32]) -> u64 {
    if roots.is_empty() {
        return 0;
    }
    let mut children: HashMap<u32, Vec<&Process>> = HashMap::new();
    for process in table {
        children.entry(process.ppid).or_default().push(process);
    }
    let mut seen = HashSet::new();
    let mut stack: Vec<&Process> = table
        .iter()
        .filter(|p| roots.contains(&p.pid) || roots.contains(&p.pgid))
        .collect();
    let mut total = 0;
    while let Some(process) = stack.pop() {
        if !seen.insert(process.pid) {
            continue;
        }
        total += process.rss;
        stack.extend(children.get(&process.pid).into_iter().flatten());
    }
    total
}

/// The machine's memory now, or `None` when it can't be read. `OBID_MEMORY_AVAILABLE` (bytes) says
/// what's available instead, with no pressure, and `OBID_MEMORY_TOTAL` the total: obid's tests
/// can't fill a Mac's memory, and their jobs shouldn't wait on whatever else the Mac running them
/// holds.
pub fn machine() -> Option<Machine> {
    let machine = read_machine()?;
    let said = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|bytes| bytes.parse().ok())
    };
    let total = said("OBID_MEMORY_TOTAL").unwrap_or(machine.total);
    Some(match said("OBID_MEMORY_AVAILABLE") {
        Some(available) => Machine {
            total,
            available: available.min(total),
            pressure: Pressure::Normal,
        },
        None => machine,
    })
}

fn read_machine() -> Option<Machine> {
    #[cfg(target_os = "macos")]
    {
        let sysctl = run("/usr/sbin/sysctl", &["-n", "hw.memsize", PRESSURE_LEVEL])?;
        let vm_stat = run("/usr/bin/vm_stat", &[])?;
        mac_machine(&sysctl, &vm_stat)
    }
    #[cfg(target_os = "linux")]
    {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        let psi = std::fs::read_to_string("/proc/pressure/memory").unwrap_or_default();
        linux_machine(&meminfo, &psi)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    None
}

/// Every process now; empty when they can't be listed.
pub fn processes() -> Vec<Process> {
    #[cfg(target_os = "macos")]
    {
        run("/bin/ps", &["-axo", "pid=,ppid=,pgid=,rss="])
            .map(|out| parse_ps(&out))
            .unwrap_or_default()
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let pid = entry.file_name().to_str()?.parse().ok()?;
                parse_status(
                    pid,
                    &std::fs::read_to_string(entry.path().join("status")).ok()?,
                )
            })
            .collect()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    Vec::new()
}

#[cfg(target_os = "macos")]
const PRESSURE_LEVEL: &str = "kern.memorystatus_vm_pressure_level";

/// How long a reading's program may take (it takes milliseconds): status waits on it.
#[cfg(target_os = "macos")]
const RUN_LIMIT: std::time::Duration = std::time::Duration::from_secs(2);

/// `program`'s output, or `None` when it didn't run, failed or took longer than [`RUN_LIMIT`]
/// (then it's ended).
#[cfg(target_os = "macos")]
fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    // Read as it writes, so a full pipe never holds it up.
    let mut stdout = child.stdout.take()?;
    let reading = std::thread::spawn(move || {
        let mut out = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut out).map(|_| out)
    });
    let give_up_at = std::time::Instant::now() + RUN_LIMIT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reading.join().ok()?.ok()?;
                return status.success().then_some(out);
            }
            Ok(None) if std::time::Instant::now() < give_up_at => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// `sysctl -n hw.memsize kern.memorystatus_vm_pressure_level` and `vm_stat`, read.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mac_machine(sysctl: &str, vm_stat: &str) -> Option<Machine> {
    let mut lines = sysctl.lines();
    let total: u64 = lines.next()?.trim().parse().ok()?;
    let pressure = match lines.next().map(str::trim) {
        Some("4") => Pressure::Critical,
        Some("2") => Pressure::Warn,
        _ => Pressure::Normal,
    };
    let page: u64 = vm_stat
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |name: &str| -> Option<u64> {
        vm_stat.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim().trim_matches('"') == name)
                .then(|| value.trim().trim_end_matches('.').parse().ok())?
        })
    };
    let app = pages("Anonymous pages")?.saturating_sub(pages("Pages purgeable").unwrap_or(0));
    let used = (app + pages("Pages wired down")? + pages("Pages occupied by compressor")?) * page;
    Some(Machine {
        total,
        available: total.saturating_sub(used),
        pressure,
    })
}

/// `/proc/meminfo` and `/proc/pressure/memory` (empty when the kernel has no PSI), read. Warn when
/// tasks waited on memory 10% of the last 10 seconds or less than a tenth is available; critical
/// when all of them did, or less than a twentieth is.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_machine(meminfo: &str, psi: &str) -> Option<Machine> {
    let kb = |name: &str| -> Option<u64> {
        meminfo.lines().find_map(|line| {
            let value = line.strip_prefix(name)?.strip_prefix(':')?;
            value.split_whitespace().next()?.parse::<u64>().ok()
        })
    };
    let total = kb("MemTotal")? * 1024;
    let available = kb("MemAvailable")? * 1024;
    let avg10 = |kind: &str| -> f64 {
        psi.lines()
            .find_map(|line| {
                let rest = line.strip_prefix(kind)?;
                rest.split_whitespace()
                    .find_map(|part| part.strip_prefix("avg10="))?
                    .parse()
                    .ok()
            })
            .unwrap_or(0.0)
    };
    let pressure = if avg10("full") >= 10.0 || available < total / 20 {
        Pressure::Critical
    } else if avg10("some") >= 10.0 || available < total / 10 {
        Pressure::Warn
    } else {
        Pressure::Normal
    };
    Some(Machine {
        total,
        available,
        pressure,
    })
}

/// `ps -axo pid=,ppid=,pgid=,rss=` (resident memory in KiB), read.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_ps(out: &str) -> Vec<Process> {
    out.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace().map(|f| f.parse::<u64>().ok());
            let mut next = || fields.next().flatten();
            Some(Process {
                pid: u32::try_from(next()?).ok()?,
                ppid: u32::try_from(next()?).ok()?,
                pgid: u32::try_from(next()?).ok()?,
                rss: next()? * 1024,
            })
        })
        .collect()
}

/// `/proc/<pid>/status`, read: its `PPid`, the first `NSpgid` (this namespace's) and `VmRSS` (none
/// for a kernel thread).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_status(pid: u32, status: &str) -> Option<Process> {
    let field = |name: &str| -> Option<u64> {
        status.lines().find_map(|line| {
            let value = line.strip_prefix(name)?.strip_prefix(':')?;
            value.split_whitespace().next()?.parse().ok()
        })
    };
    Some(Process {
        pid,
        ppid: u32::try_from(field("PPid")?).ok()?,
        pgid: field("NSpgid")
            .and_then(|g| u32::try_from(g).ok())
            .unwrap_or(0),
        rss: field("VmRSS").unwrap_or(0) * 1024,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VM_STAT: &str = "Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                    14483.
Pages active:                                 226147.
Pages wired down:                             181779.
Pages purgeable:                                  11.
\"Translation faults\":                     2581496495.
File-backed pages:                            160009.
Anonymous pages:                              290878.
Pages stored in compressor:                   712295.
Pages occupied by compressor:                 347536.
";

    #[test]
    fn reads_a_mac() {
        let machine = mac_machine("17179869184\n2\n", VM_STAT).unwrap();
        // 16 GiB less (290878 - 11 + 181779 + 347536) pages of 16 KiB.
        assert_eq!(machine.total, 17_179_869_184);
        assert_eq!(machine.available, 17_179_869_184 - 820_182 * 16_384);
        assert_eq!(machine.pressure, Pressure::Warn);
        assert_eq!(
            mac_machine("17179869184\n4\n", VM_STAT).unwrap().pressure,
            Pressure::Critical
        );
        assert_eq!(
            mac_machine("17179869184\n1\n", VM_STAT).unwrap().pressure,
            Pressure::Normal
        );
        assert_eq!(mac_machine("17179869184\n1\n", "nothing"), None);
    }

    #[test]
    fn reads_linux() {
        let meminfo =
            "MemTotal:       16000000 kB\nMemFree:  100 kB\nMemAvailable:    8000000 kB\n";
        let calm = "some avg10=0.50 avg60=0.10 avg300=0.00 total=1\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        let machine = linux_machine(meminfo, calm).unwrap();
        assert_eq!(machine.total, 16_000_000 * 1024);
        assert_eq!(machine.available, 8_000_000 * 1024);
        assert_eq!(machine.pressure, Pressure::Normal);
        let stalls =
            "some avg10=12.00 avg60=1 avg300=0 total=1\nfull avg10=2.00 avg60=0 avg300=0 total=0\n";
        assert_eq!(
            linux_machine(meminfo, stalls).unwrap().pressure,
            Pressure::Warn
        );
        let low = "MemTotal: 16000000 kB\nMemAvailable: 700000 kB\n";
        assert_eq!(linux_machine(low, "").unwrap().pressure, Pressure::Critical);
    }

    #[test]
    fn sums_a_tree_once() {
        let table = parse_ps(
            "    1     0     1  10000\n  100     1   100   1000\n  101   100   100   2000\n  102   101   102   3000\n  103     1   100   4000\n  200     1   200   5000\n",
        );
        assert_eq!(table.len(), 6);
        // 100, its child 101, 101's child 102 in a group of its own, and 103 left in 100's group.
        assert_eq!(tree_rss(&table, &[100]), 10_000 * 1024);
        assert_eq!(tree_rss(&table, &[100, 101]), 10_000 * 1024);
        assert_eq!(tree_rss(&table, &[200]), 5_000 * 1024);
        assert_eq!(tree_rss(&table, &[]), 0);
        assert_eq!(tree_rss(&table, &[999]), 0);
    }

    #[test]
    fn reads_proc_status() {
        let status = "Name:\tcargo\nPPid:\t41\nNSpgid:\t40\t1\nVmRSS:\t  2048 kB\n";
        assert_eq!(
            parse_status(42, status),
            Some(Process {
                pid: 42,
                ppid: 41,
                pgid: 40,
                rss: 2048 * 1024
            })
        );
        let thread = "Name:\tkthreadd\nPPid:\t0\n";
        assert_eq!(parse_status(2, thread).map(|p| p.rss), Some(0));
    }

    #[test]
    fn roots_are_measured_while_they_live() {
        let folder = std::env::temp_dir().join(format!("meter-{}", std::process::id()));
        let kept = root(4_000_001, &folder.join("sub"));
        let check = check_root(4_000_002, &folder);
        assert_eq!(roots_in(&folder, false), vec![4_000_001]);
        assert_eq!(roots_in(&folder, true), vec![4_000_002]);
        drop(kept);
        drop(check);
        assert!(roots_in(&folder, false).is_empty() && roots_in(&folder, true).is_empty());
    }

    #[test]
    fn measures_this_machine() {
        let Some(machine) = machine() else {
            return;
        };
        assert!(machine.total > 0 && machine.available <= machine.total);
        let me = std::process::id();
        assert!(tree_rss(&processes(), &[me]) > 0);
    }
}
