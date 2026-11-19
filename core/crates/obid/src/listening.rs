//! Which TCP ports a given set of processes listens on, of those this machine reaches as
//! `localhost` (docs/contracts/task-servers.md, TASKSERVERS-1, 5 and 6): a listen on a loopback
//! address or on every address, never one on another address alone. Read with `lsof`, scoped to
//! those processes, on macOS, and from `/proc` on Linux, where a process in a network namespace of
//! its own (Oby's box: crate::sandbox) listens where the host's table doesn't show it, so it's left
//! out: the host can't reach it. It reads only the processes it's given and writes nothing.

use std::net::IpAddr;
use std::time::Duration;

/// A process listening: its id, its name and its port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listening {
    pub pid: u32,
    pub process: String,
    pub port: u16,
}

/// How long a read may take (it takes some 20 ms): the meter's tick waits on it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const READ_LIMIT: Duration = Duration::from_secs(2);

/// What `pids` listen on that `localhost` reaches, or `None` when it couldn't be read (the caller
/// keeps what it had). No processes, nothing read.
pub fn listening(pids: &[u32]) -> Option<Vec<Listening>> {
    if pids.is_empty() {
        return Some(Vec::new());
    }
    #[cfg(target_os = "macos")]
    {
        let list = pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        // -a: every condition at once; -n, -P: numbers, no lookups; -F pcn: a field a line.
        let out = crate::process::output_within(
            std::process::Command::new("/usr/sbin/lsof")
                .args(["-nP", "-a", "-iTCP", "-sTCP:LISTEN", "-Fpcn", "-p", &list])
                .env_clear()
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null()),
            READ_LIMIT,
        )?;
        // 1 with nothing written: none of them listens (or a process ended meanwhile).
        (out.status.success() || out.status.code() == Some(1))
            .then(|| parse_lsof(&String::from_utf8_lossy(&out.stdout)))
    }
    #[cfg(target_os = "linux")]
    {
        let mut table = String::new();
        for file in ["/proc/net/tcp", "/proc/net/tcp6"] {
            table.push_str(&std::fs::read_to_string(file).unwrap_or_default());
        }
        let sockets = parse_proc_net(&table);
        let mut found = Vec::new();
        for &pid in pids {
            let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
                continue;
            };
            let process = process_name(
                std::fs::read_link(format!("/proc/{pid}/exe"))
                    .ok()
                    .as_deref(),
                &std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default(),
                &std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default(),
            );
            for fd in fds.flatten() {
                let Ok(link) = std::fs::read_link(fd.path()) else {
                    continue;
                };
                let inode = link
                    .to_str()
                    .and_then(|l| l.strip_prefix("socket:["))
                    .and_then(|l| l.strip_suffix(']'))
                    .and_then(|l| l.parse::<u64>().ok());
                if let Some(&(_, port)) = inode.and_then(|i| sockets.iter().find(|(s, _)| *s == i))
                {
                    found.push(Listening {
                        pid,
                        process: process.clone(),
                        port,
                    });
                }
            }
        }
        Some(found)
    }
}

/// A Linux process's name as the owner knows it (TASKSERVERS-1): its executable's file name
/// (`/proc/<pid>/exe`), else its command line's first word's, else `comm`. Not `comm` first: that's
/// the main thread's name, which a program may change (Node names it `MainThread`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn process_name(exe: Option<&std::path::Path>, cmdline: &[u8], comm: &str) -> String {
    let file_name = |path: &std::path::Path| {
        path.file_name()
            .map(|n| {
                n.to_string_lossy()
                    .trim_end_matches(" (deleted)")
                    .to_owned()
            })
            .filter(|n| !n.is_empty())
    };
    let first = cmdline.split(|b| *b == 0).next().unwrap_or_default();
    exe.and_then(file_name)
        .or_else(|| file_name(std::path::Path::new(&*String::from_utf8_lossy(first))))
        .unwrap_or_else(|| comm.trim().to_owned())
}

/// Whether `localhost` reaches a listen on `host`: a loopback address, or every address (`*`,
/// `0.0.0.0`, `::`), IPv4 in IPv6 too. `[::1]`'s brackets are lsof's.
fn reachable(host: &str) -> bool {
    if host == "*" {
        return true;
    }
    let host = host.trim_start_matches('[').trim_end_matches(']');
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(v6)) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.is_loopback() || v4.is_unspecified(),
            None => v6.is_loopback() || v6.is_unspecified(),
        },
        Ok(ip) => ip.is_loopback() || ip.is_unspecified(),
        Err(_) => false,
    }
}

/// `lsof -F pcn`'s answer: a `p` line names a process, a `c` line its name, an `n` line one of
/// its listens (`127.0.0.1:3000`, `*:3000`, `[::1]:3000`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_lsof(out: &str) -> Vec<Listening> {
    let (mut pid, mut process) = (None, String::new());
    let mut found = Vec::new();
    for line in out.lines() {
        let (field, value) = line.split_at(line.len().min(1));
        match field {
            "p" => (pid, process) = (value.parse().ok(), String::new()),
            "c" => process = value.to_owned(),
            "n" => {
                let Some((host, port)) = value.rsplit_once(':') else {
                    continue;
                };
                if let (Some(pid), Ok(port), true) = (pid, port.parse(), reachable(host)) {
                    found.push(Listening {
                        pid,
                        process: process.clone(),
                        port,
                    });
                }
            }
            _ => {}
        }
    }
    found
}

/// `/proc/net/tcp` and `tcp6`'s listening sockets `localhost` reaches: each one's inode and port.
/// An address is the kernel's words in this machine's byte order, a port big-endian hex.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_proc_net(table: &str) -> Vec<(u64, u16)> {
    let address = |hex: &str| -> Option<IpAddr> {
        let words = (0..hex.len() / 8)
            .map(|i| u32::from_str_radix(&hex[i * 8..i * 8 + 8], 16).map(u32::to_ne_bytes))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        match words.as_slice() {
            [v4] => Some(IpAddr::from(*v4)),
            [a, b, c, d] => {
                let mut bytes = [0u8; 16];
                for (i, word) in [a, b, c, d].iter().enumerate() {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(*word);
                }
                Some(IpAddr::from(bytes))
            }
            _ => None,
        }
    };
    table
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // sl local rem st tx:rx tr:when retrnsmt uid timeout inode
            if fields.len() < 10 || fields[3] != "0A" {
                return None;
            }
            let (host, port) = fields[1].split_once(':')?;
            let port = u16::from_str_radix(port, 16).ok()?;
            let host = address(host)?;
            reachable(&host.to_string()).then_some(())?;
            Some((fields[9].parse().ok()?, port))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TASKSERVERS-5: only a listen `localhost` reaches is a server to open.
    #[test]
    fn taskservers_5_only_listens_localhost_reaches() {
        let out = "p501\ncnode\nf12\nn*:3000\nf13\nn127.0.0.1:3001\nf14\nn[::1]:3002\nf15\nn192.168.1.20:3003\nf16\nn[::]:3004\np502\ncvite\nf3\nn0.0.0.0:5173\nf4\nn10.0.0.5:5174\n";
        let found = parse_lsof(out);
        let ports: Vec<(u32, &str, u16)> = found
            .iter()
            .map(|l| (l.pid, l.process.as_str(), l.port))
            .collect();
        assert_eq!(
            ports,
            [
                (501, "node", 3000),
                (501, "node", 3001),
                (501, "node", 3002),
                (501, "node", 3004),
                (502, "vite", 5173)
            ]
        );
        assert!(reachable("[::ffff:127.0.0.1]"));
        assert!(!reachable("[::ffff:192.168.1.2]"));
        assert!(!reachable("example.test"));
    }

    /// TASKSERVERS-1: on Linux a listener is named after its executable, not its main thread.
    #[test]
    fn taskservers_1_a_linux_listener_is_named_after_its_executable() {
        use std::path::Path;
        let exe = Path::new("/home/runner/.nvm/versions/node/v24.11.0/bin/node");
        assert_eq!(
            process_name(Some(exe), b"node\0-e\0x", "MainThread\n"),
            "node"
        );
        let deleted = Path::new("/usr/bin/node (deleted)");
        assert_eq!(process_name(Some(deleted), b"", "MainThread\n"), "node");
        // No exe to read (another user's process, one that ended): the command line, then comm.
        assert_eq!(
            process_name(
                None,
                b"/usr/local/bin/vite\0--port\x005173\0",
                "MainThread\n"
            ),
            "vite"
        );
        assert_eq!(process_name(None, b"", "python3\n"), "python3");
    }

    /// TASKSERVERS-5: Linux's table, in this machine's byte order, listens only.
    #[test]
    fn taskservers_5_reads_the_linux_table() {
        let word = |ip: [u8; 4]| format!("{:08X}", u32::from_ne_bytes(ip));
        let v6 = |ip: std::net::Ipv6Addr| {
            ip.octets()
                .chunks(4)
                .map(|c| format!("{:08X}", u32::from_ne_bytes([c[0], c[1], c[2], c[3]])))
                .collect::<String>()
        };
        let line = |local: String, port: u16, st: &str, inode: u64| {
            format!(
                "   0: {local}:{port:04X} 00000000:0000 {st} 00000000:00000000 00:00000000 00000000  1000        0 {inode} 1 0000000000000000 100 0 0 10 0\n"
            )
        };
        let mut table = String::from(
            "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n",
        );
        table += &line(word([127, 0, 0, 1]), 3000, "0A", 11);
        table += &line(word([0, 0, 0, 0]), 3001, "0A", 12);
        table += &line(word([192, 168, 1, 20]), 3002, "0A", 13);
        table += &line(word([127, 0, 0, 1]), 3003, "01", 14);
        table += &line(v6(std::net::Ipv6Addr::LOCALHOST), 3004, "0A", 15);
        table += &line(v6(std::net::Ipv6Addr::UNSPECIFIED), 3005, "0A", 16);
        assert_eq!(
            parse_proc_net(&table),
            [(11, 3000), (12, 3001), (15, 3004), (16, 3005)]
        );
    }
}
