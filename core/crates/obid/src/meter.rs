//! What this machine's memory holds and what each job's processes use (`GET /v1/status`
//! `memory`, docs/runner-api.md). Read from `sysctl`, `vm_stat`, `ps` and libproc's safe rusage
//! wrapper on macOS and from `/proc` on Linux, with no unsafe code in obid. A read costs a few
//! milliseconds, made only while jobs run or when status is asked.

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

/// A process: its parent, its group, its resident memory in bytes and the CPU time it and the
/// children it reaped have used, user and system, in nanoseconds (0 when it can't be read).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    pub rss: u64,
    pub cpu: u64,
    /// Whether `cpu` was read: a 0 that wasn't is unknown, one that was is idle (PUBLISHING-29).
    pub cpu_known: bool,
}

/// The resident memory of `roots`' trees together: each root, every process descended from
/// one, and every process still in a root's group (one whose parent ended), each counted once.
pub fn tree_rss(table: &[Process], roots: &[u32]) -> u64 {
    tree_sum(table, roots, |p| p.rss)
}

/// The CPU time `roots`' trees have used together, in nanoseconds, each process counted once as
/// [`tree_rss`] counts it. A process that ended takes its time with it.
pub fn tree_cpu(table: &[Process], roots: &[u32]) -> u64 {
    tree_sum(table, roots, |p| p.cpu)
}

/// The CPU time `pgid`'s tree has used now ([`tree_cpu`]), or `None` when it can't be read.
pub async fn group_cpu(pgid: u32) -> Option<u64> {
    tokio::task::spawn_blocking(move || known_tree_cpu(&processes(), &[pgid]))
        .await
        .ok()
        .flatten()
}

/// [`tree_cpu`], or `None` when it can't be told: an empty table, or a process in the tree whose
/// CPU time couldn't be read (a read that failed isn't an idle 0, which Linux's ticks can give):
/// a check whose CPU is unknown isn't stopped as idle (PUBLISHING-29).
pub fn known_tree_cpu(table: &[Process], roots: &[u32]) -> Option<u64> {
    let unread = std::cell::Cell::new(table.is_empty());
    let total = tree_sum(table, roots, |p| {
        unread.set(unread.get() || !p.cpu_known);
        p.cpu
    });
    (!unread.get()).then_some(total)
}

fn tree_sum(table: &[Process], roots: &[u32], value: impl Fn(&Process) -> u64) -> u64 {
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
        total += value(process);
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
        run(
            crate::process::ps_path(),
            &["-axo", "pid=,ppid=,pgid=,rss="],
        )
        .map(|out| {
            let mut table = parse_ps(&out);
            let ns = mach_tick_ns();
            for process in &mut table {
                // An ad-hoc ps can read identities, but loses Apple's task-port
                // entitlement for other processes' RSS. rusage needs no task port.
                if let Ok(pid) = i32::try_from(process.pid)
                    && let Ok(usage) =
                        libproc::pid_rusage::pidrusage::<libproc::pid_rusage::RUsageInfoV2>(pid)
                {
                    if process.rss == 0 {
                        process.rss = usage.ri_resident_size;
                    }
                    // In Mach ticks, not nanoseconds, on Apple silicon.
                    let ticks = [
                        usage.ri_user_time,
                        usage.ri_system_time,
                        usage.ri_child_user_time,
                        usage.ri_child_system_time,
                    ]
                    .into_iter()
                    .fold(0u64, u64::saturating_add);
                    process.cpu = (u128::from(ticks) * ns.0 / ns.1) as u64;
                    process.cpu_known = true;
                }
            }
            table
        })
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
                let mut process = parse_status(
                    pid,
                    &std::fs::read_to_string(entry.path().join("status")).ok()?,
                )?;
                let cpu = std::fs::read_to_string(entry.path().join("stat"))
                    .ok()
                    .and_then(|stat| stat_cpu(&stat, CLOCK_TICKS));
                process.cpu = cpu.unwrap_or(0);
                process.cpu_known = cpu.is_some();
                Some(process)
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
fn run(program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Option<String> {
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

/// A Mach tick in nanoseconds, as a fraction (numerator, denominator): 1/1 on Intel, 125/3 on
/// Apple silicon. Read once from obid's own CPU time, which `getrusage` gives in microseconds and
/// rusage in ticks: `mach_timebase_info` would need `unsafe`, which this workspace forbids.
#[cfg(target_os = "macos")]
fn mach_tick_ns() -> (u128, u128) {
    static TICK: std::sync::OnceLock<(u128, u128)> = std::sync::OnceLock::new();
    *TICK.get_or_init(|| {
        use nix::sys::resource::{UsageWho, getrusage};
        let ticks = i32::try_from(std::process::id()).ok().and_then(|pid| {
            libproc::pid_rusage::pidrusage::<libproc::pid_rusage::RUsageInfoV2>(pid).ok()
        });
        let Ok(usage) = getrusage(UsageWho::RUSAGE_SELF) else {
            return (1, 1);
        };
        let micros = |t: nix::sys::time::TimeVal| {
            u128::try_from(t.tv_sec()).unwrap_or(0) * 1_000_000
                + u128::try_from(t.tv_usec()).unwrap_or(0)
        };
        let ns = (micros(usage.user_time()) + micros(usage.system_time())) * 1000;
        match ticks.map(|t| u128::from(t.ri_user_time) + u128::from(t.ri_system_time)) {
            Some(ticks) if ticks > 0 && ns > 0 => (ns, ticks),
            _ => (1, 1),
        }
    })
}

/// The kernel's clock ticks a second (`USER_HZ`), the unit of `/proc/<pid>/stat`'s times.
// ponytail: 100 on x86_64 and arm64, the ABI's; sysconf(_SC_CLK_TCK) if obid runs where it isn't.
#[cfg(target_os = "linux")]
const CLOCK_TICKS: u64 = 100;

/// `/proc/<pid>/stat`'s `utime`, `stime`, `cutime` and `cstime` (its 14th to 17th fields: its own
/// and its reaped children's), in nanoseconds. The command's name, in parentheses, may hold spaces
/// and parentheses itself.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn stat_cpu(stat: &str, ticks_per_second: u64) -> Option<u64> {
    let mut fields = stat.get(stat.rfind(')')? + 1..)?.split_whitespace();
    // After the name comes the state (the 3rd field): utime is 11 fields on.
    let mut ticks: u64 = fields.nth(11)?.parse().ok()?;
    for _ in 0..3 {
        ticks = ticks.saturating_add(fields.next()?.parse().ok()?);
    }
    Some(ticks.saturating_mul(1_000_000_000) / ticks_per_second.max(1))
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
                cpu: 0,
                cpu_known: false,
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
        cpu: 0,
        cpu_known: false,
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
    fn sums_a_trees_cpu_once() {
        let mut table = parse_ps(
            "  100     1   100   1000\n  101   100   100   2000\n  102   101   102   3000\n  103     1   100   4000\n  200     1   200   5000\n",
        );
        for (process, cpu) in table.iter_mut().zip([1, 2, 4, 8, 16]) {
            process.cpu = cpu;
        }
        // 100, its child 101, 101's child 102 in a group of its own, and 103 left in 100's group.
        assert_eq!(tree_cpu(&table, &[100]), 15);
        assert_eq!(tree_cpu(&table, &[100, 101, 102]), 15);
        assert_eq!(tree_cpu(&table, &[200]), 16);
        assert_eq!(tree_cpu(&table, &[]), 0);
        // utime 300, stime 200, cutime 400 and cstime 100 ticks of 100 a second: 10 seconds, a
        // name with ") (" in it read past.
        let stat = "42 (a) (b c) S 41 40 40 0 -1 4194304 10 0 0 0 300 200 400 100 20 0 1 0 5 0 0\n";
        assert_eq!(stat_cpu(stat, 100), Some(10_000_000_000));
        assert_eq!(stat_cpu("42 (cut", 100), None);
        // This process, busy for a moment, has used some CPU of its own.
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(50) {
            std::hint::black_box(0u64);
        }
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            assert!(tree_cpu(&processes(), &[std::process::id()]) > 0);
        }
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
                rss: 2048 * 1024,
                cpu: 0,
                cpu_known: false,
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

    #[cfg(target_os = "macos")]
    #[test]
    fn measures_another_process_without_ps_task_port_privileges() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let rss = tree_rss(&processes(), &[child.id()]);
        let _ = child.kill();
        let _ = child.wait();
        assert!(rss > 0, "the child's resident memory wasn't measured");
    }
}

#[cfg(test)]
mod known_cpu_tests {
    use super::*;

    /// `cpu` None: a read that failed.
    fn p(pid: u32, ppid: u32, cpu: Option<u64>) -> Process {
        Process {
            pid,
            ppid,
            pgid: 1,
            rss: 0,
            cpu: cpu.unwrap_or(0),
            cpu_known: cpu.is_some(),
        }
    }

    #[test]
    fn publishing_29_a_cpu_time_that_cant_be_read_is_unknown_not_idle() {
        let read = [p(1, 0, Some(5)), p(2, 1, Some(7)), p(3, 2, Some(11))];
        assert_eq!(known_tree_cpu(&read, &[1]), Some(23));
        let unread = [p(1, 0, Some(5)), p(2, 1, None), p(3, 2, Some(11))];
        assert_eq!(known_tree_cpu(&unread, &[1]), None);
        // A 0 that was read is idle, as a sleeping process reads on Linux, not unknown.
        let idle = [p(1, 0, Some(0)), p(2, 1, Some(0))];
        assert_eq!(known_tree_cpu(&idle, &[1]), Some(0));
        // Outside the tree, an unreadable process changes nothing.
        let other = [p(1, 0, Some(5)), p(9, 0, None)];
        assert_eq!(
            known_tree_cpu(
                &[
                    other[0],
                    Process {
                        pgid: 9,
                        ..other[1]
                    }
                ],
                &[1]
            ),
            Some(5)
        );
        assert_eq!(known_tree_cpu(&[], &[1]), None);
    }
}
