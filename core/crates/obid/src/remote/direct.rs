//! obid's direct addresses (docs/contracts/remote.md, COMPUTERS-10): the gateway's own router
//! (crate::gateway) served over HTTPS on the addresses the owner picks (`direct` in the
//! configuration), with this computer's own certificate (super::certificate), so another
//! computer's window reaches it over any network they already have, its certificate pinned.
//! Nothing but the gateway is served there: the same pairing, device tokens and allowlist.
//!
//! A handshake runs in a task of its own, at most 10 seconds, so a slow or silent client holds
//! up no other; one that fails is dropped and the listener goes on.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_rustls::server::TlsStream;

use super::certificate::{self, Certificate};

/// How long a client has to finish its handshake.
const HANDSHAKE: Duration = Duration::from_secs(10);

/// The TLS server configuration for `certificate`.
pub fn tls(certificate: &Certificate) -> Result<Arc<rustls::ServerConfig>, String> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(
                certificate.der.clone(),
            )],
            rustls::pki_types::PrivateKeyDer::Pkcs8(certificate.key.clone().into()),
        )
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(config))
}

/// A bound direct address, as axum serves it: each connection already past its handshake.
pub struct Direct {
    local: SocketAddr,
    pin: super::pin::Pin,
    ready: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
}

impl Direct {
    /// Takes TCP connections on `listener` and finishes their handshakes with `tls`, until the
    /// `Direct` is dropped.
    pub fn new(
        listener: TcpListener,
        tls: Arc<rustls::ServerConfig>,
        pin: super::pin::Pin,
    ) -> std::io::Result<Direct> {
        let local = listener.local_addr()?;
        let acceptor = tokio_rustls::TlsAcceptor::from(tls);
        let (send, ready) = mpsc::channel(64);
        tokio::spawn(async move {
            loop {
                let (tcp, peer) = tokio::select! {
                    accepted = listener.accept() => match accepted {
                        Ok(accepted) => accepted,
                        // Out of descriptors, or the like: the next try may work.
                        Err(_) => {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            continue;
                        }
                    },
                    () = send.closed() => return,
                };
                let acceptor = acceptor.clone();
                let send = send.clone();
                tokio::spawn(async move {
                    if let Ok(Ok(tls)) = tokio::time::timeout(HANDSHAKE, acceptor.accept(tcp)).await
                    {
                        let _ = send.send((tls, peer)).await;
                    }
                });
            }
        });
        Ok(Direct { local, pin, ready })
    }
}

impl axum::serve::Listener for Direct {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(ready) => ready,
            // The accepting task ended only once this was dropped: nothing more comes.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.local)
    }
}

/// Binds each of `addresses` with this computer's certificate (made now the first time, in
/// `home/keys`): the listeners bound, and what `GET /v1/status` says of each under
/// `gateway.direct`, `{"address", "state": "listening", "pin"}` or `{"address", "state": "down",
/// "detail"}`. A busy address is a warning, never a stop, as the gateway's is.
pub async fn bind(home: &Path, addresses: &[SocketAddr]) -> (Vec<Direct>, Vec<Value>) {
    if addresses.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let down = |address: &SocketAddr, detail: &str| {
        tracing::warn!("no direct address {address}: {detail}");
        json!({"address": address.to_string(), "state": "down", "detail": detail})
    };
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let made = certificate::load_or_make(home, &host)
        .and_then(|certificate| Ok((tls(&certificate)?, certificate.pin)));
    let (tls, pin) = match made {
        Ok(made) => made,
        Err(detail) => {
            let states = addresses.iter().map(|a| down(a, &detail)).collect();
            return (Vec::new(), states);
        }
    };
    let mut bound = Vec::new();
    let mut states = Vec::new();
    for address in addresses {
        let listener = match TcpListener::bind(address).await {
            Ok(listener) => listener,
            Err(e) => {
                states.push(down(address, &format!("can't listen on {address}: {e}")));
                continue;
            }
        };
        match Direct::new(listener, tls.clone(), pin) {
            Ok(direct) => {
                states.push(
                    json!({"address": direct.local.to_string(), "state": "listening",
                                   "pin": pin.to_string()}),
                );
                bound.push(direct);
            }
            Err(e) => states.push(down(address, &e.to_string())),
        }
    }
    (bound, states)
}

/// Serves `app`, the gateway's router, on each of `directs` until obid stops. Where each is goes
/// to the log and `GET /v1/status`, never stdout: a launcher reads its first lines and may close it.
pub fn serve(directs: Vec<Direct>, app: &axum::Router, stopping: &watch::Receiver<bool>) {
    for direct in directs {
        tracing::info!(address = %direct.local, pin = %direct.pin, "obid direct listening");
        let app = app.clone();
        let mut stopping = stopping.clone();
        tokio::spawn(async move {
            let stopped = async move {
                while !*stopping.borrow() {
                    if stopping.changed().await.is_err() {
                        return;
                    }
                }
            };
            if let Err(e) = axum::serve(direct, app)
                .with_graceful_shutdown(stopped)
                .await
            {
                tracing::error!("a direct address stopped: {e}");
            }
        });
    }
}

/// The first port local-network reach tries (COMPUTERS-31), and how many after it it may take
/// when one is busy: the link carries the port it got.
pub const AUTO_PORT: u16 = 8443;
const AUTO_PORTS: u16 = 10;

/// Whether local-network reach serves on `ip`: a private IPv4 address (RFC 1918), a tailnet's
/// (100.64.0.0/10, Tailscale's among them), or a unique-local IPv6 one (fc00::/7, Tailscale's
/// fd7a:115c:a1e0::/48 among them). Never loopback, link-local (a link can't carry its scope),
/// unspecified (`0.0.0.0`, `::`) or public.
pub fn private(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_private() || (a == 100 && (64..128).contains(&b))
        }
        std::net::IpAddr::V6(v6) => (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

/// This machine's own addresses local-network reach serves on now, as its interfaces have them.
pub fn private_addresses() -> Vec<std::net::IpAddr> {
    super::invite::interface_addresses(true)
        .into_iter()
        .filter(|ip| private(*ip))
        .collect()
}

/// Local-network reach, on by default (the owner's word of 2026-11-23, COMPUTERS-31): with no
/// `direct` in the configuration, the gateway is served over HTTPS on each of this machine's own
/// private and tailnet addresses ([`private`]), never a public or wildcard one, on
/// [`AUTO_PORT`] or the next free one. Addresses that came since (another Wi-Fi) are bound when a
/// link is made ([`Auto::refresh`]); one that went is left out of what's offered.
pub struct Auto {
    home: std::path::PathBuf,
    held: std::sync::Mutex<Held>,
    serving: std::sync::OnceLock<(axum::Router, watch::Receiver<bool>)>,
}

#[derive(Default)]
struct Held {
    port: Option<u16>,
    tls: Option<(Arc<rustls::ServerConfig>, super::pin::Pin)>,
    /// Each address bound so far, and what status says of it.
    bound: Vec<(std::net::IpAddr, Value)>,
    /// What the latest look at the interfaces offered.
    offered: Vec<Value>,
}

impl Auto {
    pub fn new(home: &Path) -> Auto {
        Auto {
            home: home.to_path_buf(),
            held: std::sync::Mutex::new(Held::default()),
            serving: std::sync::OnceLock::new(),
        }
    }

    /// Binds this machine's private addresses now, before the gateway is served: the listeners,
    /// and what status says of each.
    pub fn start(&self) -> (Vec<Direct>, Vec<Value>) {
        self.bind_new(&private_addresses())
    }

    /// Serves what's bound from now on with the gateway's `app`, until obid stops.
    pub fn arm(&self, app: axum::Router, stopping: watch::Receiver<bool>) {
        let _ = self.serving.set((app, stopping));
    }

    /// The port it serves on, or would: [`AUTO_PORT`] before it has one.
    pub fn port(&self) -> u16 {
        self.held
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .port
            .unwrap_or(AUTO_PORT)
    }

    /// What it offered at its last look, as `GET /v1/status` says it under `gateway.direct`.
    pub fn last(&self) -> Vec<Value> {
        self.held
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .offered
            .clone()
    }

    /// Looks at the interfaces again: an address that came is bound and served now, one that went
    /// is no longer offered. What it offers, in [`bind`]'s shape.
    pub fn refresh(&self) -> Vec<Value> {
        let (new, offered) = self.bind_new(&private_addresses());
        if let Some((app, stopping)) = self.serving.get() {
            serve(new, app, stopping);
        }
        offered
    }

    fn bind_new(&self, ips: &[std::net::IpAddr]) -> (Vec<Direct>, Vec<Value>) {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        let fresh: Vec<std::net::IpAddr> = ips
            .iter()
            .copied()
            .filter(|ip| !held.bound.iter().any(|(b, _)| b == ip))
            .collect();
        let mut directs = Vec::new();
        if !fresh.is_empty() && held.tls.is_none() {
            let host = gethostname::gethostname().to_string_lossy().into_owned();
            match certificate::load_or_make(&self.home, &host).and_then(|c| Ok((tls(&c)?, c.pin))) {
                Ok(made) => held.tls = Some(made),
                Err(detail) => tracing::warn!("no local-network reach: {detail}"),
            }
        }
        if let (false, Some((tls, pin))) = (fresh.is_empty(), held.tls.clone()) {
            // The port: the one it has, else the first of its range every address takes, else
            // the first any takes.
            let ports: Vec<u16> = match held.port {
                Some(port) => vec![port],
                None => (AUTO_PORT..AUTO_PORT + AUTO_PORTS).collect(),
            };
            let mut best: Option<(u16, Tried)> = None;
            for port in ports {
                let tried: Vec<_> = fresh
                    .iter()
                    .map(|ip| (*ip, listen(SocketAddr::new(*ip, port))))
                    .collect();
                let took = tried.iter().filter(|(_, l)| l.is_ok()).count();
                let better = best
                    .as_ref()
                    .is_none_or(|(_, b)| took > b.iter().filter(|(_, l)| l.is_ok()).count());
                let all = took == fresh.len();
                if better {
                    best = Some((port, tried));
                }
                if all {
                    break;
                }
            }
            if let Some((port, tried)) = best {
                if tried.iter().any(|(_, l)| l.is_ok()) {
                    held.port = Some(port);
                }
                for (ip, listener) in tried {
                    let address = SocketAddr::new(ip, port);
                    let state = match listener.and_then(|l| Direct::new(l, tls.clone(), pin)) {
                        Ok(direct) => {
                            directs.push(direct);
                            json!({"address": address.to_string(), "state": "listening",
                                   "pin": pin.to_string()})
                        }
                        Err(e) => {
                            tracing::warn!("no direct address {address}: {e}");
                            json!({"address": address.to_string(), "state": "down",
                                   "detail": format!("can't listen on {address}: {e}")})
                        }
                    };
                    // A busy address is tried again at the next look.
                    if state["state"] == "listening" {
                        held.bound.push((ip, state));
                    }
                }
            }
        }
        let offered: Vec<Value> = ips
            .iter()
            .filter_map(|ip| {
                held.bound
                    .iter()
                    .find(|(b, _)| b == ip)
                    .map(|(_, s)| s.clone())
            })
            .collect();
        held.offered = offered.clone();
        (directs, offered)
    }
}

/// Each address tried on one port, and its listener or why not.
type Tried = Vec<(std::net::IpAddr, std::io::Result<TcpListener>)>;

/// A listener on `address` for tokio, bound now.
fn listen(address: SocketAddr) -> std::io::Result<TcpListener> {
    let listener = std::net::TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    TcpListener::from_std(listener)
}

/// What a computer's invitation offers now (COMPUTERS-13, 30, 31): with local-network reach on by
/// default, this machine's private addresses as its interfaces have them at this moment;
/// otherwise the `direct` addresses obid bound when it started.
pub fn offered(shared: &crate::api::Shared) -> Value {
    match &shared.direct_auto {
        Some(auto) => Value::Array(auto.refresh()),
        None => shared
            .gateway_state
            .as_ref()
            .map_or(Value::Null, |state| state["direct"].clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::{pin::Pin, pinned};

    async fn served(certificate: &Certificate) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let direct = Direct::new(listener, tls(certificate).unwrap(), certificate.pin).unwrap();
        let address = axum::serve::Listener::local_addr(&direct).unwrap();
        let app = axum::Router::new().route("/v1/status", axum::routing::get(|| async { "here" }));
        tokio::spawn(async move { axum::serve(direct, app).await });
        address
    }

    #[tokio::test]
    async fn computers_11_the_pinned_client_reaches_its_computer_and_no_other() {
        let mine = certificate::make("Studio", 1_800_000_000).unwrap();
        let address = served(&mine).await;
        // A client that sends nothing holds up nobody.
        let _silent = TcpStream::connect(address).await.unwrap();
        let url = format!("https://{address}/v1/status");
        let pinned = pinned::client(mine.pin, Duration::from_secs(20)).unwrap();
        let answer = pinned.get(&url).send().await.unwrap();
        assert_eq!(answer.status(), 200);
        assert_eq!(answer.text().await.unwrap(), "here");

        let other = pinned::client(Pin([7; 32]), Duration::from_secs(20)).unwrap();
        let refused = other.get(&url).send().await.unwrap_err();
        let said = pinned::said(&refused);
        assert!(said.starts_with(pinned::NOT_THE_PINNED), "{said}");
        assert!(said.contains(&mine.pin.to_string()), "{said}");
    }

    #[test]
    fn computers_31_reach_serves_only_private_and_tailnet_addresses() {
        for ip in [
            "192.168.1.20",
            "10.0.0.2",
            "172.16.4.1",
            "100.64.0.9",
            "100.127.255.1",
            "fd00::2",
            "fd7a:115c:a1e0::9",
        ] {
            assert!(private(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "0.0.0.0",
            "::",
            "127.0.0.1",
            "::1",
            "169.254.3.4",
            "fe80::1",
            "203.0.113.9",
            "100.128.0.1",
            "8.8.8.8",
            "2001:db8::1",
        ] {
            assert!(!private(ip.parse().unwrap()), "{ip}");
        }
        assert!(private_addresses().into_iter().all(private));
    }

    #[tokio::test]
    async fn computers_31_an_address_that_came_is_bound_and_one_that_went_isnt_offered() {
        let home = std::env::temp_dir().join(format!("obid-reach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        // Port 8443 held by something else: the next free one, which the offer carries.
        let held = std::net::TcpListener::bind(("127.0.0.1", AUTO_PORT)).ok();
        let auto = Auto::new(&home);
        let v4: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let v6: std::net::IpAddr = "::1".parse().unwrap();
        let (bound, offered) = auto.bind_new(&[v4]);
        assert_eq!(bound.len(), 1, "{offered:?}");
        let port = auto.port();
        if held.is_some() {
            assert_ne!(port, AUTO_PORT);
        }
        assert!((AUTO_PORT..AUTO_PORT + AUTO_PORTS).contains(&port));
        assert_eq!(offered[0]["address"], format!("127.0.0.1:{port}"));
        assert_eq!(offered[0]["state"], "listening");
        // Another address came: bound on the same port, both offered.
        let (bound, offered) = auto.bind_new(&[v4, v6]);
        assert_eq!(bound.len(), 1, "only the new one");
        assert_eq!(offered.len(), 2, "{offered:?}");
        assert_eq!(offered[1]["address"], format!("[::1]:{port}"));
        // The first went: it isn't offered any more.
        let (bound, offered) = auto.bind_new(&[v6]);
        assert!(bound.is_empty());
        assert_eq!(offered.len(), 1);
        assert_eq!(auto.last(), offered);
        std::fs::remove_dir_all(&home).unwrap();
    }
}
