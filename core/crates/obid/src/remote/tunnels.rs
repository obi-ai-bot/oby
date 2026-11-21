//! The SSH tunnels to this Mac's computers (docs/contracts/remote.md, COMPUTERS-18 and 25): one per
//! computer's SSH route, opened the first time a request needs it, kept while its ssh runs, and
//! dialed again after it's cut, never more than once a second, so a cut route isn't redialed on
//! every request. A computer forgotten loses its tunnels: dropping one kills its ssh.
//!
//! It owns only the running tunnels and when each was last dialed. Which route a request takes is
//! the computer's supervisor's (crate::remote::computers), and whether obid answers through it is
//! the caller's to find out (a port that takes connections isn't yet obid).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::ssh::Ssh;
use super::tunnel::Tunnel;

/// The shortest time between two dials of one route.
pub const REDIAL: Duration = Duration::from_secs(1);

/// How long ssh has to sign in and hold the forward.
const OPEN: Duration = Duration::from_secs(15);

#[derive(Default)]
struct Dial {
    tunnel: Option<Tunnel>,
    last: Option<Instant>,
    /// Why the last dial failed, said again until the next may be tried.
    why: Option<String>,
}

/// Each computer's route, by `(computer, route)`, and its dial: one at a time per route.
type Dials = HashMap<(String, String), Arc<tokio::sync::Mutex<Dial>>>;

#[derive(Default)]
pub struct Tunnels {
    dials: Mutex<Dials>,
}

impl Tunnels {
    fn dial(&self, computer: &str, route: &str) -> Arc<tokio::sync::Mutex<Dial>> {
        self.dials
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry((computer.to_owned(), route.to_owned()))
            .or_default()
            .clone()
    }

    /// The port on this Mac's 127.0.0.1 that leads to `remote` on `destination`'s own 127.0.0.1,
    /// for `computer`'s route `route`: the running tunnel's, or a new one's. A route dialed less than
    /// a second ago that failed says why again instead of dialing.
    pub async fn port(
        &self,
        ssh: &Ssh,
        computer: &str,
        route: &str,
        destination: &str,
        remote: u16,
    ) -> Result<u16, String> {
        let dial = self.dial(computer, route);
        let mut dial = dial.lock().await;
        if let Some(tunnel) = dial.tunnel.as_mut() {
            if tunnel.alive() {
                return Ok(tunnel.local);
            }
            dial.tunnel = None;
            dial.why = Some(format!("the SSH tunnel to {destination} ended"));
        }
        if let (Some(last), Some(why)) = (dial.last, &dial.why)
            && last.elapsed() < REDIAL
        {
            return Err(why.clone());
        }
        dial.last = Some(Instant::now());
        match Tunnel::open(ssh, destination, remote, OPEN, |_| {}).await {
            Ok(tunnel) => {
                let local = tunnel.local;
                dial.tunnel = Some(tunnel);
                dial.why = None;
                Ok(local)
            }
            Err(trouble) => {
                let why = trouble.say(destination);
                dial.why = Some(why.clone());
                Err(why)
            }
        }
    }

    /// Ends `computer`'s tunnels: each ssh is killed as its tunnel drops.
    pub fn forget(&self, computer: &str) {
        self.dials
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(c, _), _| c != computer);
    }

    /// How many tunnels run for `computer`.
    pub fn running(&self, computer: &str) -> usize {
        let dials: Vec<_> = self
            .dials
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|((c, _), _)| c == computer)
            .map(|(_, d)| d.clone())
            .collect();
        dials
            .iter()
            .filter(|d| {
                d.try_lock()
                    .is_ok_and(|mut d| d.tunnel.as_mut().is_some_and(Tunnel::alive))
            })
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in `ssh` that counts its dials in `count` and runs `script` with `-L`'s local port in
    /// `$port`.
    fn stand_in(name: &str, script: &str) -> (Ssh, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("obid-tunnels-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let count = dir.join("dials");
        let program = dir.join("ssh");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\necho x >> '{}'\nwhile [ $# -gt 0 ]; do [ \"$1\" = -L ] && port=$(echo \"$2\" | cut -d: -f2); shift; done\n{script}\n",
                count.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        (
            Ssh {
                program,
                config: None,
                env: Some(vec![("PATH".into(), "/usr/bin:/bin".into())]),
            },
            count,
        )
    }

    fn dials(count: &std::path::Path) -> usize {
        std::fs::read_to_string(count).map_or(0, |s| s.lines().count())
    }

    #[tokio::test]
    async fn computers_18_a_cut_route_is_dialed_at_most_once_a_second() {
        let (ssh, count) = stand_in(
            "refused",
            "echo 'ssh: connect to host box port 22: Connection refused' >&2; exit 255",
        );
        let tunnels = Tunnels::default();
        let first = tunnels.port(&ssh, "c1", "ssh", "box", 8810).await;
        assert!(first.is_err());
        // Asked again and again within the second: the same words, no new ssh.
        for _ in 0..20 {
            let again = tunnels.port(&ssh, "c1", "ssh", "box", 8810).await;
            assert_eq!(again, first);
        }
        assert_eq!(dials(&count), 1, "redialed within a second");
        // A second later, it's dialed again.
        let until = Instant::now() + Duration::from_secs(5);
        while dials(&count) < 2 {
            assert!(Instant::now() < until, "never dialed again");
            let _ = tunnels.port(&ssh, "c1", "ssh", "box", 8810).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    #[tokio::test]
    async fn computers_25_a_running_tunnel_is_reused_and_forgetting_its_computer_ends_it() {
        let (ssh, count) = stand_in(
            "holds",
            "exec perl -MIO::Socket::INET -e '$s = IO::Socket::INET->new(LocalAddr => \"127.0.0.1\", LocalPort => $ARGV[0], Listen => 5, ReuseAddr => 1) or die; while ($c = $s->accept) { close $c }' \"$port\"",
        );
        let tunnels = Tunnels::default();
        let port = tunnels.port(&ssh, "c1", "ssh", "box", 8810).await.unwrap();
        assert_eq!(tunnels.port(&ssh, "c1", "ssh", "box", 8810).await, Ok(port));
        assert_eq!(dials(&count), 1, "a running tunnel is used again");
        assert_eq!(tunnels.running("c1"), 1);
        tunnels.forget("c1");
        assert_eq!(tunnels.running("c1"), 0);
        let until = Instant::now() + Duration::from_secs(10);
        while tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            assert!(
                Instant::now() < until,
                "ssh outlived its forgotten computer"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
