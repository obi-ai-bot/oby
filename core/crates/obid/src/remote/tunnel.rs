//! An SSH tunnel to another computer's obid (docs/contracts/remote.md, COMPUTERS-12): the person's
//! own `ssh -N -L` from this Mac's `127.0.0.1:<local>` to that obid's gateway on its own
//! `127.0.0.1`, held open while the window uses it, and ended when it's dropped.
//!
//! It's ready only once its port takes connections: ssh binds it after it signed in, so the wait
//! is the connection's. A port another program took in between makes ssh exit
//! (`ExitOnForwardFailure`), and another port is tried, three times at most. ssh's stderr is read
//! for as long as it runs (a pipe nobody reads stalls the tunnel), its last 16 KiB kept for what
//! went wrong. ssh runs in a process group of its own and is killed when the tunnel is dropped.
//! Whether obid answers through it is the supervisor's probe (super::supervisor), not this.

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt as _;

use super::ssh::Ssh;
use super::trouble::{SshTrouble, trouble};

/// The last 16 KiB ssh printed on stderr.
#[derive(Clone, Default)]
pub struct Tail(Arc<Mutex<String>>);

impl Tail {
    pub fn text(&self) -> String {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn push(&self, more: &str) {
        let mut t = self.0.lock().unwrap_or_else(|e| e.into_inner());
        t.push_str(more);
        if t.len() > 16_384 {
            let cut = t.len() - 16_384;
            let cut = (cut..t.len()).find(|i| t.is_char_boundary(*i)).unwrap_or(0);
            t.drain(..cut);
        }
    }
}

pub struct Tunnel {
    child: tokio::process::Child,
    pub local: u16,
    pub stderr: Tail,
}

impl Tunnel {
    /// Opens it to `remote` on `destination`'s own 127.0.0.1 and waits, up to `limit`, until its
    /// port takes connections. `started` hears each ssh's process id.
    pub async fn open(
        ssh: &Ssh,
        destination: &str,
        remote: u16,
        limit: Duration,
        started: impl Fn(u32),
    ) -> Result<Tunnel, SshTrouble> {
        let mut last = SshTrouble::ForwardFailed;
        for _ in 0..3 {
            let local = free_port().map_err(|said| SshTrouble::Other { said })?;
            let args = ssh
                .tunnel_args(destination, local, remote)
                .map_err(|said| SshTrouble::Other { said })?;
            let mut command = ssh.command(&args);
            command
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .process_group(0);
            let mut child = command.spawn().map_err(|e| SshTrouble::Other {
                said: format!("ssh didn't start: {e}"),
            })?;
            if let Some(pid) = child.id() {
                started(pid);
            }
            let tail = Tail::default();
            let reading = child.stderr.take().map(|mut err| {
                let tail = tail.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    while let Ok(n) = err.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        tail.push(&String::from_utf8_lossy(&buf[..n]));
                    }
                })
            });
            let until = Instant::now() + limit;
            loop {
                if let Ok(Some(_)) = child.try_wait() {
                    // What ssh said last, read to its end.
                    if let Some(reading) = reading {
                        let _ = tokio::time::timeout(Duration::from_secs(2), reading).await;
                    }
                    last = trouble(&tail.text());
                    break;
                }
                if tokio::net::TcpStream::connect(("127.0.0.1", local))
                    .await
                    .is_ok()
                {
                    return Ok(Tunnel {
                        child,
                        local,
                        stderr: tail,
                    });
                }
                if Instant::now() >= until {
                    let _ = child.start_kill();
                    return Err(SshTrouble::Unreachable);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if last != SshTrouble::ForwardFailed {
                return Err(last);
            }
        }
        Err(last)
    }

    /// Whether ssh still runs.
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// ssh's process id while it runs.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }
}

fn free_port() -> Result<u16, String> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(l.local_addr().map_err(|e| e.to_string())?.port())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A stand-in `ssh` in a folder of the test's own: `script` is its body, after `-L`'s local
    /// port is read into `$port`.
    fn stand_in(name: &str, script: &str) -> Ssh {
        let dir = std::env::temp_dir().join(format!("obid-tunnel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("ssh");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nwhile [ $# -gt 0 ]; do [ \"$1\" = -L ] && port=$(echo \"$2\" | cut -d: -f2); shift; done\n{script}\n"
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        Ssh {
            program,
            config: None,
            env: Some(vec![("PATH".into(), "/usr/bin:/bin".into())]),
        }
    }

    #[tokio::test]
    async fn computers_12_the_tunnel_is_ready_once_its_port_takes_connections_and_ends_when_dropped()
     {
        // ssh holding the forward: its port listens until it's killed.
        let ssh = stand_in(
            "ready",
            "exec perl -MIO::Socket::INET -e '$s = IO::Socket::INET->new(LocalAddr => \"127.0.0.1\", LocalPort => $ARGV[0], Listen => 5, ReuseAddr => 1) or die; while ($c = $s->accept) { close $c }' \"$port\"",
        );
        let mut tunnel = Tunnel::open(&ssh, "box", 8810, Duration::from_secs(20), |_| {})
            .await
            .unwrap();
        assert!(tunnel.alive());
        let local = tunnel.local;
        assert!(
            tokio::net::TcpStream::connect(("127.0.0.1", local))
                .await
                .is_ok()
        );
        assert!(tunnel.pid().is_some());
        drop(tunnel);
        // ssh is killed with the tunnel: its port stops taking connections.
        let until = Instant::now() + Duration::from_secs(10);
        while tokio::net::TcpStream::connect(("127.0.0.1", local))
            .await
            .is_ok()
        {
            assert!(Instant::now() < until, "ssh outlived its tunnel");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn computers_12_what_ssh_said_is_why_it_failed_and_a_taken_port_is_tried_three_times() {
        let ssh = stand_in(
            "host-key",
            "echo 'Host key verification failed.' >&2; exit 255",
        );
        let refused = Tunnel::open(&ssh, "box", 8810, Duration::from_secs(20), |_| {}).await;
        assert_eq!(refused.err(), Some(SshTrouble::HostKeyUnknown));

        let ssh = stand_in(
            "taken",
            "echo \"bind [127.0.0.1]:$port: Address already in use\" >&2; exit 255",
        );
        let tries = AtomicU32::new(0);
        let refused = Tunnel::open(&ssh, "box", 8810, Duration::from_secs(20), |_| {
            tries.fetch_add(1, Ordering::SeqCst);
        })
        .await;
        assert_eq!(refused.err(), Some(SshTrouble::ForwardFailed));
        assert_eq!(tries.load(Ordering::SeqCst), 3);

        let ssh = stand_in("silent", "exec sleep 30");
        let started = Instant::now();
        let refused = Tunnel::open(&ssh, "box", 8810, Duration::from_millis(500), |_| {}).await;
        assert_eq!(refused.err(), Some(SshTrouble::Unreachable));
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
