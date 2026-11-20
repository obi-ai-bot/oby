//! The lab's `remote` feature (src/lab/mod.rs): other computers, as T3 Code's environments. The
//! owner's decisions of 2026-11-19: any network the person already has, routes ranked LAN, then
//! VPN or tailnet, then public, then SSH; SSH as a tunnel only (`ssh -N -L` to the other obid's
//! 127.0.0.1), keys and agent only, host keys the person's call, nothing installed over it; Macs
//! and Linux; the whole window switches to the other computer; when no route works, Oby says so.
//!
//! [`computers`] is the part obid takes as it is when this is ported: routes and their order, the
//! supervisor that picks one and moves, SSH's arguments and what its errors mean, the tunnel, the
//! pinned certificate and the pairing link. The nodes below it are thin: they hold the Mac's window
//! (its device key and the computers it added) and drive the machines (src/lab/machine.rs).
//!
//! What obid lacks today, and the lab does in its place, is listed in `workflows/lab/remote/README.md`
//! ("What obid needs when ported").

use std::collections::BTreeMap;
use std::os::unix::fs::OpenOptionsExt as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::keys::{self, DeviceKey};
use crate::obid::Run;

/// What obid takes as it is: no node, no machine, no lab state.
pub mod computers {
    use std::net::IpAddr;
    use std::path::PathBuf;
    use std::process::Stdio;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use serde::{Deserialize, Serialize};
    use sha2::Digest as _;
    use tokio::io::AsyncReadExt as _;

    // ---------------------------------------------------------------- routes

    /// How a route reaches the other computer, by what its address is.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum RouteKind {
        Loopback,
        Lan,
        Tailnet,
        Vpn,
        Public,
        Ssh,
    }

    impl RouteKind {
        /// The owner's order: LAN, then VPN or tailnet, then public, then SSH (a loopback, the
        /// same machine, before all).
        pub fn rank(self) -> u8 {
            match self {
                RouteKind::Loopback => 0,
                RouteKind::Lan => 1,
                RouteKind::Tailnet | RouteKind::Vpn => 2,
                RouteKind::Public => 3,
                RouteKind::Ssh => 4,
            }
        }
        pub fn name(self) -> &'static str {
            match self {
                RouteKind::Loopback => "loopback",
                RouteKind::Lan => "lan",
                RouteKind::Tailnet => "tailnet",
                RouteKind::Vpn => "vpn",
                RouteKind::Public => "public",
                RouteKind::Ssh => "ssh",
            }
        }
    }

    /// What a direct address is, from its host alone: a name or an IP. Tailscale's names and its
    /// range (100.64.0.0/10, fd7a:115c:a1e0::/48) are the tailnet; the private ranges, link-local
    /// and `.local` names the LAN; anything else public.
    pub fn classify(host: &str) -> RouteKind {
        let host = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if host == "localhost" {
            return RouteKind::Loopback;
        }
        if host.ends_with(".ts.net") {
            return RouteKind::Tailnet;
        }
        if host.ends_with(".local") || host.ends_with(".lan") || host.ends_with(".home.arpa") {
            return RouteKind::Lan;
        }
        match host.parse::<IpAddr>() {
            Ok(IpAddr::V4(ip)) => {
                let [a, b, ..] = ip.octets();
                if ip.is_loopback() {
                    RouteKind::Loopback
                } else if ip.is_private() || ip.is_link_local() {
                    RouteKind::Lan
                } else if a == 100 && (64..128).contains(&b) {
                    RouteKind::Tailnet
                } else {
                    RouteKind::Public
                }
            }
            Ok(IpAddr::V6(ip)) => {
                let s = ip.segments();
                if ip.is_loopback() {
                    RouteKind::Loopback
                } else if s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0 {
                    RouteKind::Tailnet
                } else if (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80 {
                    RouteKind::Lan
                } else {
                    RouteKind::Public
                }
            }
            Err(_) => RouteKind::Public,
        }
    }

    /// One way to the other computer.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Route {
        pub id: String,
        pub kind: RouteKind,
        pub via: Via,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "lowercase")]
    pub enum Via {
        /// A direct address. Always HTTPS with the other computer's certificate pinned: a plain
        /// address can belong to another machine after a network change, and only the pin says
        /// who answers.
        Https { host: String, port: u16, pin: Pin },
        /// An SSH tunnel to obid's gateway on the other computer's own 127.0.0.1. Who answers is
        /// the host key, which the person accepted in their own `known_hosts`.
        Ssh {
            destination: String,
            #[serde(rename = "remotePort")]
            remote_port: u16,
        },
    }

    impl Route {
        /// A direct route from `https://host:port`. Its kind is the address's unless the person
        /// says (a VPN Oby can't tell from its address).
        pub fn direct(
            id: &str,
            url: &str,
            pin: Pin,
            kind: Option<RouteKind>,
        ) -> Result<Route, String> {
            let rest = url.strip_prefix("https://").ok_or_else(|| {
                format!(
                    "{url} isn't https://: a direct address needs the other computer's certificate"
                )
            })?;
            let rest = rest.trim_end_matches('/');
            let (host, port) = match rest.rsplit_once(':') {
                Some((h, p)) if !h.ends_with(':') || h.starts_with('[') => (
                    h.to_string(),
                    p.parse::<u16>().map_err(|_| format!("{url}: no port"))?,
                ),
                _ => (rest.to_string(), 443),
            };
            if host.is_empty()
                || host.contains(['/', '@', '?', '#', ' '])
                || host.chars().any(char::is_control)
            {
                return Err(format!("{url} isn't an address"));
            }
            let kind = kind.unwrap_or_else(|| classify(&host));
            if kind == RouteKind::Ssh {
                return Err("a direct address isn't an SSH route".into());
            }
            Ok(Route {
                id: id.into(),
                kind,
                via: Via::Https { host, port, pin },
            })
        }

        pub fn ssh(id: &str, destination: &str, remote_port: u16) -> Result<Route, String> {
            check_destination(destination)?;
            Ok(Route {
                id: id.into(),
                kind: RouteKind::Ssh,
                via: Via::Ssh {
                    destination: destination.into(),
                    remote_port,
                },
            })
        }

        /// The route as a person reads it: `lan https://host:port`, `ssh box`.
        pub fn describe(&self) -> String {
            match &self.via {
                Via::Https { host, port, .. } => {
                    format!("{} https://{host}:{port}", self.kind.name())
                }
                Via::Ssh { destination, .. } => format!("ssh {destination}"),
            }
        }
    }

    /// The routes in the owner's order; the person's own order kept within a kind.
    pub fn rank(mut routes: Vec<Route>) -> Vec<Route> {
        routes.sort_by_key(|r| r.kind.rank());
        routes
    }

    // ---------------------------------------------------------------- the pin

    /// The other computer's certificate, pinned: SHA-256 of its DER.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Pin(pub [u8; 32]);

    impl Pin {
        pub fn of_certificate(der: &[u8]) -> Pin {
            Pin(sha2::Sha256::digest(der).into())
        }
        /// `sha256:<64 hex>`, or OpenSSL's `AB:CD:…` form.
        pub fn parse(text: &str) -> Result<Pin, String> {
            let hex_part = text.trim().strip_prefix("sha256:").unwrap_or(text.trim());
            let clean: String = hex_part.chars().filter(|c| *c != ':').collect();
            let bytes = hex::decode(clean.to_ascii_lowercase())
                .map_err(|_| format!("{text:?} isn't a SHA-256 fingerprint"))?;
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|_| format!("{text:?} isn't 32 bytes"))?;
            Ok(Pin(bytes))
        }
        pub fn matches(&self, der: &[u8]) -> bool {
            Pin::of_certificate(der) == *self
        }
    }

    impl std::fmt::Display for Pin {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "sha256:{}", hex::encode(self.0))
        }
    }

    impl Serialize for Pin {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(&self.to_string())
        }
    }

    impl<'de> Deserialize<'de> for Pin {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let text = String::deserialize(d)?;
            Pin::parse(&text).map_err(serde::de::Error::custom)
        }
    }

    /// rustls's check for a pinned direct address: the certificate the other computer shows must
    /// be exactly the pinned one, and its handshake signed by that certificate's key. No CA, no
    /// name: a self-signed certificate made on that computer is what it pins.
    #[derive(Debug)]
    pub struct PinnedVerifier {
        pin: Pin,
        algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
    }

    impl PinnedVerifier {
        pub fn new(pin: Pin) -> Self {
            PinnedVerifier {
                pin,
                algorithms: rustls::crypto::ring::default_provider()
                    .signature_verification_algorithms,
            }
        }
    }

    impl rustls::client::danger::ServerCertVerifier for PinnedVerifier {
        fn verify_server_cert(
            &self,
            end_entity: &rustls::pki_types::CertificateDer<'_>,
            _intermediates: &[rustls::pki_types::CertificateDer<'_>],
            _name: &rustls::pki_types::ServerName<'_>,
            _ocsp: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            if self.pin.matches(end_entity.as_ref()) {
                Ok(rustls::client::danger::ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General(format!(
                    "{}: {}, not the pinned {}",
                    NOT_THE_PINNED,
                    Pin::of_certificate(end_entity.as_ref()),
                    self.pin
                )))
            }
        }
        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &rustls::pki_types::CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
        }
        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &rustls::pki_types::CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
        }
        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            self.algorithms.supported_schemes()
        }
    }

    /// What a refused certificate's error starts with.
    pub const NOT_THE_PINNED: &str = "another computer answered: its certificate is";

    /// A TLS client that trusts only `pin`.
    pub fn pinned_client(pin: Pin) -> Result<Arc<rustls::ClientConfig>, String> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedVerifier::new(pin)))
            .with_no_client_auth();
        Ok(Arc::new(config))
    }

    // ---------------------------------------------------------------- the link

    /// What the other computer shows to be added without SSH: its name and its direct routes,
    /// each with its certificate pinned. The six-digit code the person types travels apart.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Link {
        pub v: u8,
        pub name: String,
        pub routes: Vec<Route>,
    }

    pub const LINK_PREFIX: &str = "oby-computer:";

    impl Link {
        pub fn encode(&self) -> String {
            use base64::Engine as _;
            format!(
                "{LINK_PREFIX}{}",
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(serde_json::to_vec(self).unwrap_or_default())
            )
        }
        /// A link as the other computer made it: version 1, 1 to 8 direct routes, each pinned.
        pub fn decode(text: &str) -> Result<Link, String> {
            use base64::Engine as _;
            let body = text
                .trim()
                .strip_prefix(LINK_PREFIX)
                .ok_or("That isn't a link to another computer.")?;
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(body)
                .map_err(|_| "The link is cut short or changed.")?;
            let link: Link = serde_json::from_slice(&bytes)
                .map_err(|e| format!("The link doesn't read: {e}"))?;
            if link.v != 1 {
                return Err(format!(
                    "The link is version {}: update Oby on this Mac.",
                    link.v
                ));
            }
            if link.routes.is_empty() || link.routes.len() > 8 {
                return Err("The link has no route, or too many.".into());
            }
            if link
                .routes
                .iter()
                .any(|r| !matches!(r.via, Via::Https { .. }) || r.kind == RouteKind::Ssh)
            {
                return Err("A link carries only direct addresses, each pinned.".into());
            }
            Ok(Link {
                name: link
                    .name
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(100)
                    .collect(),
                ..link
            })
        }
    }

    // ---------------------------------------------------------------- the supervisor

    #[derive(Clone, Debug, Default)]
    struct Seen {
        /// Answers in a row (0 after a failure).
        streak: u32,
        last: Option<Result<(), String>>,
    }

    /// A route that didn't answer, and why.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    pub struct Tried {
        pub route: String,
        pub what: String,
        pub why: String,
    }

    /// What the window does next.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    #[serde(tag = "change", rename_all = "lowercase")]
    pub enum Change {
        /// It moves to `to` (from `from`, or from none).
        Moved { from: Option<String>, to: String },
        /// No route answers: Oby says so, with each route it tried.
        Lost { tried: Vec<Tried> },
    }

    /// Picks the route the window uses: the best that answers. It moves down on the current
    /// route's first failure, and up only once a better route answered `up_after` probes in a
    /// row, so a route that flaps doesn't drag the window back and forth.
    pub struct Supervisor {
        routes: Vec<Route>,
        seen: Vec<Seen>,
        current: Option<usize>,
        up_after: u32,
        lost: bool,
    }

    impl Supervisor {
        pub fn new(routes: Vec<Route>, up_after: u32) -> Supervisor {
            let routes = rank(routes);
            Supervisor {
                seen: vec![Seen::default(); routes.len()],
                routes,
                current: None,
                up_after: up_after.max(1),
                lost: false,
            }
        }
        pub fn routes(&self) -> &[Route] {
            &self.routes
        }
        pub fn current(&self) -> Option<&Route> {
            self.current.map(|i| &self.routes[i])
        }
        /// Adds a route (kept in the owner's order); its health starts unknown.
        pub fn add(&mut self, route: Route) {
            let current = self.current().map(|r| r.id.clone());
            let mut pairs: Vec<(Route, Seen)> = self
                .routes
                .drain(..)
                .zip(self.seen.drain(..))
                .filter(|(r, _)| r.id != route.id)
                .collect();
            pairs.push((route, Seen::default()));
            pairs.sort_by_key(|(r, _)| r.kind.rank());
            (self.routes, self.seen) = pairs.into_iter().unzip();
            self.current = current.and_then(|id| self.routes.iter().position(|r| r.id == id));
        }
        /// What to probe now: the current route and every better one; all of them while none
        /// holds.
        pub fn to_probe(&self) -> Vec<Route> {
            match self.current {
                Some(i) => self.routes[..=i].to_vec(),
                None => self.routes.clone(),
            }
        }
        /// A probe's answer: `Ok` when the route reached the other computer.
        pub fn observe(&mut self, id: &str, result: Result<(), String>) {
            if let Some(i) = self.routes.iter().position(|r| r.id == id) {
                let seen = &mut self.seen[i];
                seen.streak = if result.is_ok() { seen.streak + 1 } else { 0 };
                seen.last = Some(result);
            }
        }
        /// What changed since the last decision, if anything.
        pub fn decide(&mut self) -> Option<Change> {
            let current_ok = self
                .current
                .is_some_and(|c| matches!(self.seen[c].last, Some(Ok(()))));
            let pick = (0..self.routes.len()).find(|&j| {
                let seen = &self.seen[j];
                if !matches!(seen.last, Some(Ok(()))) {
                    return false;
                }
                match self.current {
                    Some(c) if current_ok && j < c => seen.streak >= self.up_after,
                    _ => true,
                }
            });
            if pick == self.current && pick.is_some() {
                return None;
            }
            match pick {
                Some(j) => {
                    let from = self.current.map(|c| self.routes[c].id.clone());
                    self.current = Some(j);
                    self.lost = false;
                    Some(Change::Moved {
                        from,
                        to: self.routes[j].id.clone(),
                    })
                }
                None => {
                    // Lost once every route was tried, and said once until one answers again.
                    let all_tried = self.seen.iter().all(|s| s.last.is_some());
                    self.current = None;
                    if self.lost || !all_tried {
                        return None;
                    }
                    self.lost = true;
                    Some(Change::Lost {
                        tried: self.tried(),
                    })
                }
            }
        }
        /// Every route that failed its last probe, with why.
        pub fn tried(&self) -> Vec<Tried> {
            self.routes
                .iter()
                .zip(&self.seen)
                .filter_map(|(r, s)| match &s.last {
                    Some(Err(why)) => Some(Tried {
                        route: r.id.clone(),
                        what: r.describe(),
                        why: why.clone(),
                    }),
                    _ => None,
                })
                .collect()
        }
    }

    /// What the window says when no route reaches `name`: every route it tried, and why.
    pub fn unreachable(name: &str, tried: &[Tried]) -> String {
        let each: Vec<String> = tried
            .iter()
            .map(|t| format!("{} ({}): {}", t.route, t.what, t.why))
            .collect();
        format!(
            "Oby can't reach {name} right now. It tried {}.",
            if each.is_empty() {
                "no route".to_string()
            } else {
                each.join("; ")
            }
        )
    }

    // ---------------------------------------------------------------- SSH

    /// The person's own `ssh`. `config` is only for a lab or a test: in the product it's `None`
    /// and ssh reads the person's own configuration, which Oby never parses (`ssh -G` resolves).
    #[derive(Clone, Debug)]
    pub struct Ssh {
        pub program: PathBuf,
        pub config: Option<PathBuf>,
        /// The environment ssh runs with; `None` keeps Oby's own.
        pub env: Option<Vec<(String, String)>>,
    }

    /// A destination as the person typed it: an alias or `user@host`. Never an option (a leading
    /// `-` is how `ssh -oProxyCommand=…` gets in), never whitespace or a control character.
    pub fn check_destination(destination: &str) -> Result<(), String> {
        let ok = !destination.is_empty()
            && destination.len() <= 255
            && !destination.starts_with('-')
            && destination
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-@:[]%".contains(c));
        if ok {
            Ok(())
        } else {
            Err(format!("{destination:?} isn't an SSH host or alias"))
        }
    }

    impl Ssh {
        /// What every call takes: no password or code prompt (keys and the agent only), no
        /// alias's RemoteCommand or TTY taking the session over, no shared ControlMaster, and a
        /// lost link noticed in 45 seconds.
        fn base(&self) -> Vec<String> {
            let mut args = vec![];
            if let Some(config) = &self.config {
                args.extend(["-F".into(), config.display().to_string()]);
            }
            for o in [
                "BatchMode=yes",
                "ConnectTimeout=10",
                "RemoteCommand=none",
                "RequestTTY=no",
                "ControlMaster=no",
                "ControlPath=none",
                "ServerAliveInterval=15",
                "ServerAliveCountMax=3",
                "LogLevel=ERROR",
            ] {
                args.extend(["-o".into(), o.into()]);
            }
            args
        }
        /// Runs `command` on the other computer, with none of the alias's forwards.
        pub fn command_args(
            &self,
            destination: &str,
            command: &str,
        ) -> Result<Vec<String>, String> {
            check_destination(destination)?;
            let mut args = self.base();
            args.extend(
                [
                    "-o",
                    "ClearAllForwardings=yes",
                    "-T",
                    "--",
                    destination,
                    command,
                ]
                .map(String::from),
            );
            Ok(args)
        }
        /// The tunnel: this Mac's `127.0.0.1:<local>` to the other computer's own
        /// `127.0.0.1:<remote>`, and nothing else. ssh exits if it can't hold the forward.
        pub fn tunnel_args(
            &self,
            destination: &str,
            local: u16,
            remote: u16,
        ) -> Result<Vec<String>, String> {
            check_destination(destination)?;
            let mut args = self.base();
            args.extend(
                [
                    "-o",
                    "ExitOnForwardFailure=yes",
                    "-n",
                    "-N",
                    "-T",
                    "-L",
                    &format!("127.0.0.1:{local}:127.0.0.1:{remote}"),
                    "--",
                    destination,
                ]
                .map(String::from),
            );
            Ok(args)
        }
        /// `ssh -G`: what ssh itself makes of an alias.
        pub fn resolve_args(&self, destination: &str) -> Result<Vec<String>, String> {
            check_destination(destination)?;
            let mut args = self.base();
            args.extend(["-G", "--", destination].map(String::from));
            Ok(args)
        }
        pub fn command(&self, args: &[String]) -> tokio::process::Command {
            let mut c = tokio::process::Command::new(&self.program);
            c.args(args).stdin(Stdio::null()).kill_on_drop(true);
            if let Some(env) = &self.env {
                c.env_clear();
                for (k, v) in env {
                    c.env(k, v);
                }
            }
            c
        }
    }

    /// The host, user and port `ssh -G` resolved.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    pub struct Resolved {
        pub hostname: String,
        pub user: String,
        pub port: u16,
    }

    pub fn parse_resolved(out: &str) -> Option<Resolved> {
        let mut hostname = None;
        let mut user = None;
        let mut port = None;
        for line in out.lines() {
            match line.split_once(' ') {
                Some(("hostname", v)) => hostname = Some(v.trim().to_string()),
                Some(("user", v)) => user = Some(v.trim().to_string()),
                Some(("port", v)) => port = v.trim().parse().ok(),
                _ => {}
            }
        }
        Some(Resolved {
            hostname: hostname?,
            user: user?,
            port: port?,
        })
    }

    /// What went wrong with ssh, from what it printed, in words the person can act on.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    #[serde(tag = "trouble", rename_all = "camelCase")]
    pub enum SshTrouble {
        /// The host's key isn't in the person's `known_hosts`: accepting it is their call.
        HostKeyUnknown,
        /// The host's key changed: someone else may be answering.
        HostKeyChanged,
        /// No key of theirs (file or agent) is accepted; Oby never asks for a password.
        NoKey,
        Unreachable,
        ForwardFailed,
        Other {
            said: String,
        },
    }

    pub fn trouble(stderr: &str) -> SshTrouble {
        let s = stderr.to_ascii_lowercase();
        if s.contains("remote host identification has changed")
            || s.contains("host key for") && s.contains("has changed")
        {
            SshTrouble::HostKeyChanged
        } else if s.contains("host key verification failed")
            || s.contains("no matching host key")
            || s.contains("is not known")
        {
            SshTrouble::HostKeyUnknown
        } else if s.contains("permission denied") {
            SshTrouble::NoKey
        } else if s.contains("forwarding failed")
            || s.contains("cannot listen")
            || s.contains("address already in use")
        {
            SshTrouble::ForwardFailed
        } else if s.contains("connection refused")
            || s.contains("timed out")
            || s.contains("no route to host")
            || s.contains("could not resolve")
            || s.contains("network is unreachable")
            || s.contains("connection closed")
            || s.contains("broken pipe")
        {
            SshTrouble::Unreachable
        } else {
            SshTrouble::Other {
                said: stderr.trim().chars().take(300).collect(),
            }
        }
    }

    impl SshTrouble {
        pub fn say(&self, destination: &str) -> String {
            match self {
                SshTrouble::HostKeyUnknown => format!(
                    "{destination}'s host key isn't one you've accepted. Check it, then run `ssh {destination}` once in a terminal to accept it."
                ),
                SshTrouble::HostKeyChanged => format!(
                    "{destination}'s host key changed: another machine may be answering. Oby won't connect until you check it and fix your known_hosts."
                ),
                SshTrouble::NoKey => format!(
                    "{destination} accepts none of your SSH keys (Oby uses your keys and agent, never a password)."
                ),
                SshTrouble::Unreachable => format!("{destination} didn't answer over SSH."),
                SshTrouble::ForwardFailed => {
                    "ssh couldn't hold the tunnel's port on this Mac.".to_string()
                }
                SshTrouble::Other { said } => format!("ssh to {destination}: {said}"),
            }
        }
    }

    /// The last 16 KiB ssh printed, read for as long as it runs (a tunnel that stops being read
    /// blocks once its pipe fills).
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

    /// An `ssh -N -L` held open: this Mac's `127.0.0.1:<local>` reaches the other computer's
    /// obid. Dropping it ends ssh.
    pub struct Tunnel {
        pub child: tokio::process::Child,
        pub local: u16,
        pub stderr: Tail,
    }

    impl Tunnel {
        /// Opens it and waits until its port takes connections: ssh binds it only after it signed
        /// in, so the wait is the connection's, up to `limit`. A port another program took in the
        /// meantime makes ssh exit (ExitOnForwardFailure), and another port is tried, 3 times.
        pub async fn open(
            ssh: &Ssh,
            destination: &str,
            remote: u16,
            limit: Duration,
            started: impl Fn(u32),
        ) -> Result<Tunnel, SshTrouble> {
            let mut last = SshTrouble::ForwardFailed;
            for _ in 0..3 {
                let local = free_port().map_err(|e| SshTrouble::Other { said: e })?;
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
                if let Some(mut err) = child.stderr.take() {
                    let tail = tail.clone();
                    tokio::spawn(async move {
                        let mut buf = [0u8; 4096];
                        while let Ok(n) = err.read(&mut buf).await {
                            if n == 0 {
                                break;
                            }
                            tail.push(&String::from_utf8_lossy(&buf[..n]));
                        }
                    });
                }
                let until = Instant::now() + limit;
                loop {
                    if let Ok(Some(_)) = child.try_wait() {
                        // Let the reader take what ssh said last.
                        tokio::time::sleep(Duration::from_millis(100)).await;
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
    }

    fn free_port() -> Result<u16, String> {
        let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        Ok(l.local_addr().map_err(|e| e.to_string())?.port())
    }

    // ---------------------------------------------------------------- pairing over SSH

    /// What `obid pair-computer` will do, the program SSH runs on the other computer on the
    /// owner's click (placed there with obid, never installed over SSH): it reads the Mac's name
    /// and its device key from stdin, one line each, names the key in obid's `home/pairing`
    /// (which obid accepts within 2 minutes, once), registers it with obid's own API there, and
    /// makes a one-time invitation the Mac redeems through the tunnel for its device token. obid's
    /// API token never leaves that computer; the invitation's secret goes back over SSH only. Its
    /// output is one JSON line, `{"device": …, "invite": …}`. Nothing else runs: anything else on
    /// stdin is refused, so a key whose only command is this can do nothing but pair.
    pub fn pair_computer_script(
        home: &str,
        api: &str,
        gateway_url: &str,
    ) -> Result<String, String> {
        let plain = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "/._-:".contains(c))
        };
        if !plain(home) || !plain(api) || !gateway_url.starts_with("http://") || !plain(gateway_url)
        {
            return Err("the other obid's home, API and gateway are plain paths".into());
        }
        Ok(format!(
            r#"#!/bin/bash
# obid-spec's stand-in for `obid pair-computer` (tools/obid-spec/src/lab/remote.rs).
set -eu
umask 077
home='{home}'
IFS= read -r name || name=''
IFS= read -r key || key=''
case "$key" in
  ''|*[!A-Za-z0-9+/=]*) echo "That isn't a device key." >&2; exit 2 ;;
esac
[ "${{#key}}" -le 200 ] || {{ echo "That isn't a device key." >&2; exit 2; }}
name=$(printf '%s' "$name" | tr -cd 'A-Za-z0-9 ._-' | cut -c1-100)
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
printf 'Authorization: Bearer %s\n' "$(cat "$home/api-token")" > "$dir/auth"
printf '%s\n' "$key" > "$home/pairing"
printf '{{"name":"%s","publicKey":"%s"}}' "$name" "$key" > "$dir/device"
printf '{{"url":"%s","name":"%s"}}' '{gateway_url}' "$name" > "$dir/invite"
curl -sS --max-time 20 -H @"$dir/auth" -H 'Content-Type: application/json' -X POST 'http://{api}/v1/devices' --data-binary @"$dir/device" > "$dir/device.out"
curl -sS --max-time 20 -H @"$dir/auth" -H 'Content-Type: application/json' -X POST 'http://{api}/v1/pairing/invite' --data-binary @"$dir/invite" > "$dir/invite.out"
printf '{{"device":%s,"invite":%s}}\n' "$(cat "$dir/device.out")" "$(cat "$dir/invite.out")"
"#
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn pin(n: u8) -> Pin {
            Pin([n; 32])
        }
        fn direct(id: &str, url: &str) -> Route {
            Route::direct(id, url, pin(1), None).unwrap()
        }

        #[test]
        fn an_address_is_classified_by_what_it_is() {
            assert_eq!(classify("192.168.1.20"), RouteKind::Lan);
            assert_eq!(classify("10.0.0.3"), RouteKind::Lan);
            assert_eq!(classify("box.local"), RouteKind::Lan);
            assert_eq!(classify("100.101.102.103"), RouteKind::Tailnet);
            assert_eq!(classify("box.tail0000.ts.net"), RouteKind::Tailnet);
            assert_eq!(classify("[fd7a:115c:a1e0::1]"), RouteKind::Tailnet);
            assert_eq!(classify("fe80::1"), RouteKind::Lan);
            assert_eq!(classify("203.0.113.9"), RouteKind::Public);
            assert_eq!(classify("example.com"), RouteKind::Public);
            assert_eq!(classify("127.0.0.1"), RouteKind::Loopback);
        }

        #[test]
        fn routes_rank_lan_then_tailnet_then_public_then_ssh_keeping_the_persons_order() {
            let routes = rank(vec![
                Route::ssh("s", "box", 8810).unwrap(),
                direct("p", "https://203.0.113.9:8443"),
                direct("t", "https://100.64.0.2:8443"),
                direct("l2", "https://10.0.0.9:8443"),
                direct("l1", "https://192.168.1.2:8443"),
            ]);
            let ids: Vec<&str> = routes.iter().map(|r| r.id.as_str()).collect();
            assert_eq!(ids, ["l2", "l1", "t", "p", "s"]);
        }

        #[test]
        fn a_direct_address_is_https_and_pinned_only() {
            assert!(Route::direct("x", "http://10.0.0.1:8810", pin(1), None).is_err());
            assert!(Route::direct("x", "https://a b:1", pin(1), None).is_err());
            let r = direct("x", "https://[fd7a:115c:a1e0::1]:8443");
            assert_eq!(r.kind, RouteKind::Tailnet);
            assert!(matches!(r.via, Via::Https { port: 8443, .. }));
            let vpn = Route::direct(
                "v",
                "https://203.0.113.9:8443",
                pin(1),
                Some(RouteKind::Vpn),
            );
            assert_eq!(vpn.unwrap().kind, RouteKind::Vpn);
        }

        #[test]
        fn an_ssh_destination_is_never_an_option() {
            assert!(check_destination("-oProxyCommand=touch /tmp/x").is_err());
            assert!(check_destination("box; rm").is_err());
            assert!(check_destination("box\n").is_err());
            assert!(check_destination("").is_err());
            assert!(check_destination("oby@box.local").is_ok());
            let ssh = Ssh {
                program: "ssh".into(),
                config: None,
                env: None,
            };
            let args = ssh.tunnel_args("box", 50000, 8810).unwrap();
            let dash = args.iter().position(|a| a == "--").unwrap();
            assert_eq!(&args[dash + 1..], ["box"]);
            for want in [
                "BatchMode=yes",
                "ExitOnForwardFailure=yes",
                "RemoteCommand=none",
                "ControlPath=none",
                "127.0.0.1:50000:127.0.0.1:8810",
            ] {
                assert!(args.iter().any(|a| a == want), "{want} in {args:?}");
            }
            assert!(ssh.tunnel_args("-p22", 1, 2).is_err());
            let run = ssh.command_args("box", "true").unwrap();
            assert!(run.iter().any(|a| a == "ClearAllForwardings=yes"));
        }

        #[test]
        fn ssh_trouble_says_what_to_do() {
            assert_eq!(
                trouble("Host key verification failed.\r\n"),
                SshTrouble::HostKeyUnknown
            );
            assert_eq!(
                trouble("@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@"),
                SshTrouble::HostKeyChanged
            );
            assert_eq!(
                trouble("oby@box: Permission denied (publickey)."),
                SshTrouble::NoKey
            );
            assert_eq!(
                trouble("ssh: connect to host box port 22: Connection refused"),
                SshTrouble::Unreachable
            );
            assert_eq!(
                trouble("Error: remote port forwarding failed"),
                SshTrouble::ForwardFailed
            );
            assert!(SshTrouble::HostKeyUnknown.say("box").contains("ssh box"));
        }

        #[test]
        fn ssh_g_is_read_for_host_user_and_port_only() {
            let r =
                parse_resolved("user oby\nhostname 10.0.0.4\nport 2222\nidentityfile ~/.ssh/x\n");
            assert_eq!(
                r,
                Some(Resolved {
                    hostname: "10.0.0.4".into(),
                    user: "oby".into(),
                    port: 2222
                })
            );
            assert_eq!(parse_resolved("user oby\n"), None);
        }

        #[test]
        fn a_pin_reads_both_forms_and_matches_only_its_certificate() {
            let p = Pin::of_certificate(b"cert-a");
            assert_eq!(Pin::parse(&p.to_string()).unwrap(), p);
            let colons: Vec<String> = p.0.iter().map(|b| format!("{b:02X}")).collect();
            assert_eq!(Pin::parse(&colons.join(":")).unwrap(), p);
            assert!(p.matches(b"cert-a"));
            assert!(!p.matches(b"cert-b"));
            assert!(Pin::parse("sha256:abcd").is_err());
        }

        #[test]
        fn a_link_round_trips_and_refuses_anything_unpinned() {
            let link = Link {
                v: 1,
                name: "Studio".into(),
                routes: vec![direct("lan", "https://192.168.1.2:8443")],
            };
            assert_eq!(Link::decode(&link.encode()).unwrap(), link);
            let ssh = Link {
                routes: vec![Route::ssh("s", "box", 8810).unwrap()],
                ..link.clone()
            };
            assert!(Link::decode(&ssh.encode()).is_err());
            let none = Link {
                routes: vec![],
                ..link.clone()
            };
            assert!(Link::decode(&none.encode()).is_err());
            let v2 = Link { v: 2, ..link };
            assert!(Link::decode(&v2.encode())
                .unwrap_err()
                .contains("version 2"));
            assert!(Link::decode("https://evil.example").is_err());
        }

        fn routes() -> Vec<Route> {
            vec![
                Route::ssh("ssh", "box", 8810).unwrap(),
                direct("lan", "https://192.168.1.2:8443"),
            ]
        }
        fn ok() -> Result<(), String> {
            Ok(())
        }
        fn cut() -> Result<(), String> {
            Err("refused".into())
        }

        #[test]
        fn the_best_route_that_answers_is_used_and_the_next_takes_over_when_it_is_cut() {
            let mut s = Supervisor::new(routes(), 2);
            assert_eq!(
                s.to_probe().len(),
                2,
                "every route is probed while none holds"
            );
            s.observe("lan", ok());
            s.observe("ssh", ok());
            assert_eq!(
                s.decide(),
                Some(Change::Moved {
                    from: None,
                    to: "lan".into()
                })
            );
            assert_eq!(s.to_probe().len(), 1, "only the current one, the best");
            s.observe("lan", cut());
            assert_eq!(
                s.decide(),
                Some(Change::Moved {
                    from: Some("lan".into()),
                    to: "ssh".into()
                })
            );
            assert_eq!(s.decide(), None);
        }

        #[test]
        fn the_window_moves_up_only_once_the_better_route_answered_twice() {
            let mut s = Supervisor::new(routes(), 2);
            s.observe("lan", cut());
            s.observe("ssh", ok());
            s.decide();
            assert_eq!(s.current().unwrap().id, "ssh");
            s.observe("lan", ok());
            s.observe("ssh", ok());
            assert_eq!(s.decide(), None, "one answer isn't enough");
            s.observe("lan", cut());
            s.observe("lan", ok());
            assert_eq!(s.decide(), None, "a flap starts the count again");
            s.observe("lan", ok());
            assert_eq!(
                s.decide(),
                Some(Change::Moved {
                    from: Some("ssh".into()),
                    to: "lan".into()
                })
            );
        }

        #[test]
        fn when_none_answers_it_says_which_routes_it_tried_once() {
            let mut s = Supervisor::new(routes(), 2);
            s.observe("lan", cut());
            assert_eq!(s.decide(), None, "not lost before every route was tried");
            s.observe("ssh", Err("box didn't answer over SSH.".into()));
            let Some(Change::Lost { tried }) = s.decide() else {
                panic!("lost")
            };
            assert_eq!(tried.len(), 2);
            let said = unreachable("Studio", &tried);
            assert!(
                said.contains("lan (lan https://192.168.1.2:8443): refused"),
                "{said}"
            );
            assert!(said.contains("ssh (ssh box)"), "{said}");
            assert_eq!(s.decide(), None, "said once");
            s.observe("ssh", ok());
            assert!(matches!(s.decide(), Some(Change::Moved { .. })));
        }

        #[test]
        fn a_route_added_later_keeps_the_owners_order_and_the_current_route() {
            let mut s = Supervisor::new(vec![Route::ssh("ssh", "box", 8810).unwrap()], 2);
            s.observe("ssh", ok());
            s.decide();
            s.add(direct("lan", "https://192.168.1.2:8443"));
            assert_eq!(s.routes()[0].id, "lan");
            assert_eq!(s.current().unwrap().id, "ssh");
            assert_eq!(s.to_probe().len(), 2);
        }

        #[test]
        fn the_pairing_helper_takes_only_plain_paths_and_reads_the_key_from_stdin() {
            let script = pair_computer_script(
                "/home/oby/.oby/home",
                "127.0.0.1:7811",
                "http://127.0.0.1:8810",
            )
            .unwrap();
            assert!(script.contains("> \"$home/pairing\""));
            assert!(script.contains("IFS= read -r key"));
            assert!(script.contains("That isn't a device key."));
            assert!(pair_computer_script("/h'; rm -rf /", "a", "http://b").is_err());
            assert!(pair_computer_script("/h", "a", "http://b' x").is_err());
        }

        #[test]
        fn a_certificate_other_than_the_pinned_one_is_refused() {
            use rustls::client::danger::ServerCertVerifier as _;
            let a = rustls::pki_types::CertificateDer::from(b"certificate a".to_vec());
            let b = rustls::pki_types::CertificateDer::from(b"certificate b".to_vec());
            let v = PinnedVerifier::new(Pin::of_certificate(a.as_ref()));
            let name = rustls::pki_types::ServerName::try_from("192.168.1.2").unwrap();
            let now = rustls::pki_types::UnixTime::now();
            assert!(v.verify_server_cert(&a, &[], &name, &[], now).is_ok());
            let refused = v.verify_server_cert(&b, &[], &name, &[], now).unwrap_err();
            assert!(refused.to_string().contains(NOT_THE_PINNED), "{refused}");
        }
    }
}

use computers::{Change, Link, Pin, Route, RouteKind, Ssh, Supervisor, Tunnel, Via};

/// Every `lab.remote.*` node: its name, what it does.
pub const NODES: &[(&str, &str)] = &[
    ("lab.remote.window", "The Mac's window, as the node stands in for it: its device key (P-256), made once, as the Mac app's own."),
    ("lab.remote.pair-ssh", "Adds a computer over SSH on the owner's click: names the window's device key in that obid's home/pairing and registers it there, gets a one-time invitation back over SSH, and redeems it through an ssh -N -L tunnel for a device token."),
    ("lab.remote.front", "The other computer's direct address, until obid serves HTTPS itself: a TLS front on every interface of that machine (its own self-signed certificate, made once or anew) to obid's gateway on its 127.0.0.1; answers its URL and pin."),
    ("lab.remote.invite", "On the other computer, as its owner there: registers that owner's device key, makes an invitation they approve, and shows the six-digit code and the link (its direct routes, each pinned)."),
    ("lab.remote.redeem", "Adds a computer with a code: the window reads the link, reaches its route with the certificate pinned, redeems the code for a device token and registers its own key through it."),
    ("lab.remote.address", "An address on this Mac (127.0.0.1:<port>) that leads to a machine's port, and can be moved to another machine's: the same address, another computer behind it."),
    ("lab.remote.route", "Adds a route to a computer the window knows: a direct address (https, pinned) or an SSH alias."),
    ("lab.remote.watch", "Starts the window's supervisor on a computer: it probes the current route and every better one, moves down on a failure and up once a better one answered twice."),
    ("lab.remote.wait", "Waits until the window uses the route named (or none), answering the route and every move it made; with expect."),
    ("lab.remote.call", "Calls the other computer's obid as the window does, through the route in use: with its device token, and the owner's words signed by its key, a key it never registered, or none; with expect."),
    ("lab.remote.local-call", "Calls a machine's obid on its own 127.0.0.1 with its own token, as its owner there (curl, on that machine); with expect."),
    ("lab.remote.mac-computer", "Another computer that is this Mac: a second scratch obid with its own home, port and user folder, reached over SSH to localhost with a key the workflow makes, which ~/.ssh/authorized_keys lets run only that obid's pairing program and open only its gateway's port, for an hour at most; the line goes at the cleanup. Skipped while macOS Remote Login is off: the owner decides that."),
    ("lab.remote.mac-forget", "Takes the lines this workflow added to ~/.ssh/authorized_keys out now (the cleanup does too), answering how many of them are left."),
    ("lab.remote.tunnel", "Opens an SSH tunnel to a port on the other computer and asks for /v1/status through it, answering whether it got there and, if not, why."),
    ("lab.remote.find", "Finds files by name under a scratch obid's home on this Mac (a job's worktree), answering where."),
    ("lab.remote.ssh", "Runs one command on a computer over SSH as the window would (BatchMode, its ssh config), answering its status, output and what its error means; with expect."),
];

/// The window's computers.
struct Computer {
    name: String,
    supervisor: Supervisor,
    token: Option<String>,
    /// The pairing's id in the other obid's gateway (what revoking it takes).
    pairing: Option<String>,
    /// The window's key's id in the other obid's devices.json.
    device: Option<String>,
    tunnels: BTreeMap<String, Tunnel>,
    ssh: Option<Ssh>,
    /// When each SSH route last tried to open its tunnel: a cut route isn't redialed in a loop.
    dialed: BTreeMap<String, Instant>,
    moves: Vec<Value>,
}

struct Window {
    key: DeviceKey,
    name: String,
    computers: BTreeMap<String, Arc<tokio::sync::Mutex<Computer>>>,
}

static WINDOW: Mutex<Option<Window>> = Mutex::new(None);
static FRONTS: Mutex<Vec<tokio::task::JoinHandle<()>>> = Mutex::new(Vec::new());

fn window_key() -> Result<(DeviceKey, String)> {
    let w = WINDOW.lock().unwrap();
    let w = w
        .as_ref()
        .context("no window yet: a lab.remote.window step first")?;
    Ok((DeviceKey::from_hex(&w.key.secret_hex())?, w.name.clone()))
}

fn computer(name: &str) -> Result<Arc<tokio::sync::Mutex<Computer>>> {
    let w = WINDOW.lock().unwrap();
    w.as_ref()
        .and_then(|w| w.computers.get(name).cloned())
        .with_context(|| format!("the window has no computer {name:?}"))
}

fn keep(computer: Computer) -> Result<()> {
    let mut w = WINDOW.lock().unwrap();
    let w = w.as_mut().context("no window")?;
    w.computers.insert(
        computer.name.clone(),
        Arc::new(tokio::sync::Mutex::new(computer)),
    );
    Ok(())
}

pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    match name {
        "lab.remote.window" => {
            let mut w = WINDOW.lock().unwrap();
            let window = w.get_or_insert_with(|| Window {
                key: DeviceKey::generate(),
                name: input["name"].as_str().unwrap_or("obid-spec's Mac").into(),
                computers: BTreeMap::new(),
            });
            Ok(json!({"name": window.name, "publicKey": window.key.public()}))
        }
        "lab.remote.pair-ssh" => pair_ssh(run, input).await,
        "lab.remote.front" => front(run, input).await,
        "lab.remote.invite" => invite(run, input).await,
        "lab.remote.redeem" => redeem(input).await,
        "lab.remote.address" => address(input).await,
        "lab.remote.route" => add_route(input).await,
        "lab.remote.watch" => watch(input).await,
        "lab.remote.wait" => wait(input).await,
        "lab.remote.call" => call(input).await,
        "lab.remote.local-call" => local_call(run, input).await,
        "lab.remote.ssh" => ssh_run(input).await,
        "lab.remote.mac-computer" => mac_computer(run, input).await,
        "lab.remote.mac-forget" => Ok(json!({ "left": forget_authorized()? })),
        "lab.remote.tunnel" => tunnel_probe(input).await,
        "lab.remote.find" => find(input),
        other => bail!("{other} isn't a remote node"),
    }
}

// -------------------------------------------------------------------- the lab's SSH

/// The window's ssh for a lab machine: `lab.machine.ssh`'s scratch config, and a HOME and no
/// agent of the person's, so nothing of theirs (`~/.ssh`, their agent's keys) is read.
fn lab_ssh(v: &Value) -> Result<(Ssh, String)> {
    let config = crate::sphere::str_of(v, "config")?;
    let alias = crate::sphere::str_of(v, "alias")?.to_string();
    let home = crate::sphere::str_of(v, "home")?;
    Ok((
        Ssh {
            program: "/usr/bin/ssh".into(),
            config: Some(config.into()),
            env: Some(vec![
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("HOME".into(), home.into()),
            ]),
        },
        alias,
    ))
}

async fn ssh_output(
    ssh: &Ssh,
    alias: &str,
    command: &str,
    stdin: Option<&[u8]>,
) -> Result<(i32, String, String)> {
    let args = ssh
        .command_args(alias, command)
        .map_err(anyhow::Error::msg)?;
    let mut c = ssh.command(&args);
    c.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0);
    if stdin.is_some() {
        c.stdin(std::process::Stdio::piped());
    }
    let mut child = c.spawn().context("ssh didn't start")?;
    if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(bytes).await?;
        drop(pipe);
    }
    let out = tokio::time::timeout(Duration::from_secs(90), child.wait_with_output())
        .await
        .context("ssh took over 90 s")??;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

async fn ssh_run(input: &Value) -> Result<Value> {
    let (ssh, alias) = lab_ssh(&input["ssh"])?;
    let command = crate::sphere::str_of(input, "command")?;
    let (status, stdout, stderr) = ssh_output(&ssh, &alias, command, None).await?;
    let trouble = (status != 0).then(|| computers::trouble(&stderr));
    Ok(json!({"status": status, "stdout": stdout, "stderr": stderr,
        "trouble": trouble, "said": trouble.as_ref().map(|t| t.say(&alias))}))
}

// -------------------------------------------------------------------- HTTP, over any route

struct Answer {
    status: u16,
    body: Value,
    text: String,
}

async fn http_over<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    host: &str,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: Option<&str>,
) -> Result<Answer> {
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    match body {
        Some(b) => head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        )),
        None if matches!(method, "POST" | "PUT" | "PATCH" | "DELETE") => {
            head.push_str("Content-Length: 0\r\n")
        }
        None => {}
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    if let Some(b) = body {
        stream.write_all(b.as_bytes()).await?;
    }
    stream.flush().await?;
    let mut raw = Vec::new();
    // A TLS peer that closes without close_notify still gave a whole answer.
    if let Err(e) = stream.read_to_end(&mut raw).await {
        if raw.is_empty() {
            return Err(e.into());
        }
    }
    let a = crate::http::parse(&raw)?;
    Ok(Answer {
        status: a.status,
        body: a.body,
        text: a.text,
    })
}

const PROBE: Duration = Duration::from_secs(4);

/// One request to the other computer through `route`. A failure to reach it (not an HTTP
/// answer) is an `Err` with why, in the words the window shows.
async fn through(
    c: &mut Computer,
    route: &Route,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    limit: Duration,
) -> std::result::Result<Answer, String> {
    match &route.via {
        Via::Https { host, port, pin } => {
            let connect = async {
                let tcp = tokio::net::TcpStream::connect((host.as_str(), *port))
                    .await
                    .map_err(|e| format!("can't connect: {e}"))?;
                let config = computers::pinned_client(*pin)?;
                let name = rustls::pki_types::ServerName::try_from(host.clone())
                    .map_err(|e| e.to_string())?;
                let tls = tokio_rustls::TlsConnector::from(config)
                    .connect(name, tcp)
                    .await
                    .map_err(|e| e.to_string())?;
                http_over(tls, host, method, path, headers, body)
                    .await
                    .map_err(|e| format!("{e:#}"))
            };
            tokio::time::timeout(limit, connect)
                .await
                .map_err(|_| format!("no answer within {}s", limit.as_secs()))?
        }
        Via::Ssh {
            destination,
            remote_port,
        } => {
            let ssh = c.ssh.clone().ok_or("no ssh for this computer")?;
            let alive = c.tunnels.get_mut(&route.id).is_some_and(Tunnel::alive);
            if !alive {
                c.tunnels.remove(&route.id);
                // A cut route isn't redialed more than once a second.
                if c.dialed
                    .get(&route.id)
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
                {
                    return Err(format!("{destination}: the tunnel is down"));
                }
                c.dialed.insert(route.id.clone(), Instant::now());
                let tunnel = Tunnel::open(
                    &ssh,
                    destination,
                    *remote_port,
                    Duration::from_secs(20),
                    |pid| {
                        crate::procs::remember(pid, &format!("ssh tunnel to {destination}"));
                    },
                )
                .await
                .map_err(|t| t.say(destination))?;
                c.tunnels.insert(route.id.clone(), tunnel);
            }
            let local = c.tunnels[&route.id].local;
            let go = async {
                let tcp = tokio::net::TcpStream::connect(("127.0.0.1", local))
                    .await
                    .map_err(|e| format!("the tunnel: {e}"))?;
                http_over(
                    tcp,
                    &format!("127.0.0.1:{local}"),
                    method,
                    path,
                    headers,
                    body,
                )
                .await
                .map_err(|e| format!("through the tunnel: {e:#}"))
            };
            tokio::time::timeout(limit, go)
                .await
                .map_err(|_| format!("{destination}: no answer within {}s", limit.as_secs()))?
        }
    }
}

/// One supervisor round: probe what it says, decide, remember a move.
async fn round(c: &mut Computer) -> Option<Change> {
    for route in c.supervisor.to_probe() {
        let token = c.token.clone();
        let headers: Vec<(String, String)> = token
            .map(|t| vec![("Authorization".into(), format!("Bearer {t}"))])
            .unwrap_or_default();
        let got = through(c, &route, "GET", "/v1/status", &headers, None, PROBE).await;
        c.supervisor.observe(&route.id, got.map(|_| ()));
    }
    let change = c.supervisor.decide();
    if let Some(change) = &change {
        c.moves
            .push(json!({"at": keys::now_millis(), "change": change}));
    }
    change
}

// -------------------------------------------------------------------- pairing over SSH

async fn pair_ssh(_run: &Run, input: &Value) -> Result<Value> {
    let (key, mac) = window_key()?;
    let name = crate::sphere::str_of(input, "computer")?.to_string();
    let (ssh, alias) = lab_ssh(&input["ssh"])?;
    let obid = &input["obid"];
    let gateway = crate::sphere::str_of(obid, "gateway")?;
    let gateway_port: u16 = gateway
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .context("gateway: 127.0.0.1:<port>")?;
    // What runs there is obid's own pairing program (`pairCommand`), never a script of ours:
    // the Mac's name and key go on its stdin, not on a command line the other machine's `ps` shows.
    let command = crate::sphere::str_of(obid, "pairCommand")?;
    anyhow::ensure!(
        command.starts_with('/')
            && command
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c)),
        "pairCommand: a plain absolute path"
    );
    let said_on_stdin = format!("{mac}\n{}\n", key.public());
    let (status, stdout, stderr) =
        ssh_output(&ssh, &alias, command, Some(said_on_stdin.as_bytes())).await?;
    if status != 0 {
        let trouble = computers::trouble(&stderr);
        return Ok(
            json!({"paired": false, "status": status, "trouble": trouble,
            "said": trouble.say(&alias), "stderr": stderr}),
        );
    }
    let said: Value = serde_json::from_str(stdout.trim())
        .with_context(|| format!("the other computer answered {stdout:?}"))?;
    let device = said["device"]["id"].as_str().map(str::to_string);
    let secret = said["invite"]["secret"]
        .as_str()
        .with_context(|| format!("no invitation: {}", said["invite"]))?
        .to_string();
    let route = Route::ssh("ssh", &alias, gateway_port).map_err(anyhow::Error::msg)?;
    let mut c = Computer {
        name: name.clone(),
        supervisor: Supervisor::new(vec![route.clone()], 2),
        token: None,
        pairing: None,
        device,
        tunnels: BTreeMap::new(),
        ssh: Some(ssh),
        dialed: BTreeMap::new(),
        moves: vec![],
    };
    let body = json!({"credential": secret, "name": mac}).to_string();
    let redeemed = through(
        &mut c,
        &route,
        "POST",
        "/v1/pairing",
        &[],
        Some(&body),
        Duration::from_secs(30),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        redeemed.status == 200,
        "redeeming the invitation: {} {}",
        redeemed.status,
        redeemed.text
    );
    c.token = redeemed.body["token"].as_str().map(str::to_string);
    c.pairing = redeemed.body["deviceID"].as_str().map(str::to_string);
    round(&mut c).await;
    let out = describe(&c);
    keep(c)?;
    Ok(json!({"paired": true, "computer": out, "devices": said["device"]}))
}

fn describe(c: &Computer) -> Value {
    json!({"name": c.name, "device": c.device, "pairing": c.pairing, "hasToken": c.token.is_some(),
        "routes": c.supervisor.routes(), "current": c.supervisor.current().map(|r| r.id.clone()),
        "moves": c.moves})
}

// -------------------------------------------------------------------- the direct address

/// The TLS front, run by python3 on the machine (already on Ubuntu): obid has no HTTPS listener
/// of its own yet.
const FRONT_PY: &str = r#"import asyncio, ssl, sys
cert, key, port, target = sys.argv[1:5]
host, tport = target.rsplit(':', 1)
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.load_cert_chain(cert, key)
async def pipe(r, w):
    try:
        while True:
            d = await r.read(65536)
            if not d:
                break
            w.write(d)
            await w.drain()
    except Exception:
        pass
    finally:
        try:
            w.close()
        except Exception:
            pass
async def handle(cr, cw):
    try:
        ur, uw = await asyncio.open_connection(host, int(tport))
    except Exception:
        cw.close()
        return
    await asyncio.gather(pipe(cr, uw), pipe(ur, cw))
async def main():
    s = await asyncio.start_server(handle, '0.0.0.0', int(port), ssl=ctx)
    print('front listening', flush=True)
    async with s:
        await s.serve_forever()
asyncio.run(main())
"#;

async fn machine_run(
    _run: &Run,
    machine: &Value,
    script: &str,
    root: bool,
) -> Result<(i32, String, String)> {
    super::machine::script_on(machine, script, root).await
}

async fn front(run: &Run, input: &Value) -> Result<Value> {
    let machine = &input["machine"];
    let port = input["port"].as_u64().unwrap_or(8443);
    let target = input["target"].as_str().unwrap_or("127.0.0.1:8810");
    anyhow::ensure!(
        target
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ':'),
        "target: 127.0.0.1:<port>"
    );
    let fresh = input["certificate"].as_str() == Some("new");
    let script = format!(
        r#"set -eu
d=$HOME/.oby-lab-front
mkdir -p "$d" && chmod 700 "$d"
pkill -f "[.]oby-lab-front/front.py" || true
if [ {fresh} = 1 ] || [ ! -f "$d/cert.pem" ]; then
  openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 2 \
    -subj "/CN=$(hostname)" -keyout "$d/key.pem" -out "$d/cert.pem" 2>/dev/null
fi
cat > "$d/front.py" <<'OBY_PY'
{FRONT_PY}OBY_PY
setsid nohup python3 "$d/front.py" "$d/cert.pem" "$d/key.pem" {port} {target} > "$d/front.log" 2>&1 < /dev/null &
for i in $(seq 1 50); do grep -q 'front listening' "$d/front.log" 2>/dev/null && break; sleep 0.1; done
grep -q 'front listening' "$d/front.log"
printf '%s %s\n' "$(openssl x509 -in "$d/cert.pem" -outform DER | sha256sum | cut -d' ' -f1)" "$(hostname -I | cut -d' ' -f1)"
"#,
        fresh = if fresh { 1 } else { 0 },
    );
    let (status, out, err) = machine_run(run, machine, &script, false).await?;
    anyhow::ensure!(status == 0, "the front didn't start ({status}): {out}{err}");
    let mut parts = out.split_whitespace();
    let pin = Pin::parse(parts.next().context("no fingerprint")?).map_err(anyhow::Error::msg)?;
    let ip = parts.next().context("no address")?.to_string();
    Ok(
        json!({"url": format!("https://{ip}:{port}"), "pin": pin.to_string(), "host": ip, "port": port}),
    )
}

/// An address on this Mac that leads to `to`, movable: the same address, another machine behind.
static ADDRESSES: Mutex<BTreeMap<u16, Arc<Mutex<String>>>> = Mutex::new(BTreeMap::new());

async fn address(input: &Value) -> Result<Value> {
    let to = crate::sphere::str_of(input, "to")?.to_string();
    if let Some(port) = input["address"]["port"].as_u64() {
        let slot = ADDRESSES
            .lock()
            .unwrap()
            .get(&(port as u16))
            .cloned()
            .context("not an address this workflow made")?;
        *slot.lock().unwrap() = to.clone();
        return Ok(json!({"port": port, "url": format!("https://127.0.0.1:{port}"), "to": to}));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let slot = Arc::new(Mutex::new(to.clone()));
    ADDRESSES.lock().unwrap().insert(port, slot.clone());
    let task = tokio::spawn(async move {
        while let Ok((mut inbound, _)) = listener.accept().await {
            let target = slot.lock().unwrap().clone();
            tokio::spawn(async move {
                if let Ok(mut out) = tokio::net::TcpStream::connect(target).await {
                    let _ = tokio::io::copy_bidirectional(&mut inbound, &mut out).await;
                }
            });
        }
    });
    FRONTS.lock().unwrap().push(task);
    Ok(json!({"port": port, "url": format!("https://127.0.0.1:{port}"), "to": to}))
}

// -------------------------------------------------------------------- pairing with a code

async fn local_call_raw(
    run: &Run,
    machine: &Value,
    obid: &Value,
    method: &str,
    path: &str,
    body: Option<&Value>,
    words: Option<&str>,
) -> Result<(u16, Value, String)> {
    let api = crate::sphere::str_of(obid, "api")?;
    let home = crate::sphere::str_of(obid, "home")?;
    anyhow::ensure!(
        path.starts_with("/v1/")
            && path
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/-_.?=&".contains(c)),
        "a path under /v1"
    );
    anyhow::ensure!(method.chars().all(|c| c.is_ascii_uppercase()), "a method");
    let body = body.map(Value::to_string);
    let words_header = words
        .map(|w| format!("printf 'X-Oby-Words: %s\\n' '{w}' >> \"$d/h\"\n"))
        .unwrap_or_default();
    let script = format!(
        r#"set -eu
umask 077
d=$(mktemp -d); trap 'rm -rf "$d"' EXIT
printf 'Authorization: Bearer %s\n' "$(cat '{home}/api-token')" > "$d/h"
{words_header}cat > "$d/b" <<'OBY_JSON'
{body}
OBY_JSON
curl -sS --max-time 30 -o "$d/o" -w '%{{http_code}}' -H @"$d/h" -H 'Content-Type: application/json' -X {method} {data} 'http://{api}{path}'
printf '\n'
cat "$d/o"
"#,
        body = body.as_deref().unwrap_or(""),
        data = if body.is_some() {
            "--data-binary @\"$d/b\""
        } else {
            ""
        },
    );
    let (status, out, err) = machine_run(run, machine, &script, false).await?;
    anyhow::ensure!(status == 0, "curl on the machine ({status}): {out}{err}");
    let (code, text) = out.split_once('\n').unwrap_or((out.as_str(), ""));
    let code: u16 = code.trim().parse().context("no HTTP status")?;
    Ok((
        code,
        serde_json::from_str(text.trim()).unwrap_or(Value::Null),
        text.to_string(),
    ))
}

async fn local_call(run: &Run, input: &Value) -> Result<Value> {
    let method = crate::sphere::str_of(input, "method")?.to_uppercase();
    let path = crate::sphere::str_of(input, "path")?;
    // A scratch obid on this Mac (lab.remote.mac-computer): its own loopback, its own token.
    if input["machine"].is_null() {
        let a = crate::sphere::obid_call(&input["obid"], &method, path, input.get("body")).await?;
        return Ok(json!({"status": a.status, "body": a.body,
            "text": a.text.chars().take(4000).collect::<String>()}));
    }
    let (status, body, text) = local_call_raw(
        run,
        &input["machine"],
        &input["obid"],
        &method,
        path,
        input.get("body"),
        None,
    )
    .await?;
    Ok(json!({"status": status, "body": body, "text": text.chars().take(4000).collect::<String>()}))
}

/// The other computer's owner, at that computer: their own device key registered there (its
/// pairing file, as the Mac app writes it on their click), and an invitation they approve.
async fn invite(run: &Run, input: &Value) -> Result<Value> {
    let machine = &input["machine"];
    let obid = &input["obid"];
    let home = crate::sphere::str_of(obid, "home")?;
    let gateway = crate::sphere::str_of(obid, "gateway")?;
    let owner = DeviceKey::generate();
    let (status, out, err) = machine_run(
        run,
        machine,
        &format!(
            "umask 077; printf '%s\\n' '{}' > '{home}/pairing'",
            owner.public()
        ),
        false,
    )
    .await?;
    anyhow::ensure!(status == 0, "the pairing file: {out}{err}");
    let (status, device, text) = local_call_raw(
        run,
        machine,
        obid,
        "POST",
        "/v1/devices",
        Some(&json!({"name": "its owner's own app", "publicKey": owner.public()})),
        None,
    )
    .await?;
    anyhow::ensure!(status == 200, "its owner's device: {status} {text}");
    let id = device["id"].as_str().context("no device id")?;
    let at = keys::now_seconds();
    let approval = json!({"device": id, "signedAt": at,
        "signature": owner.sign(&format!("oby-pair-invite\n{at}"))});
    let (status, made, text) = local_call_raw(
        run,
        machine,
        obid,
        "POST",
        "/v1/pairing/invite",
        Some(&json!({"url": format!("http://{gateway}"), "name": input["name"].as_str().unwrap_or("Linux box"), "approval": approval})),
        None,
    )
    .await?;
    anyhow::ensure!(status == 200, "the invitation: {status} {text}");
    let front = &input["front"];
    let pin = Pin::parse(crate::sphere::str_of(front, "pin")?).map_err(anyhow::Error::msg)?;
    let url = input["url"]
        .as_str()
        .or(front["url"].as_str())
        .context("front.url")?;
    let kind = input["kind"]
        .as_str()
        .map(|k| serde_json::from_value::<RouteKind>(json!(k)))
        .transpose()?;
    let route = Route::direct("direct", url, pin, kind).map_err(anyhow::Error::msg)?;
    let link = Link {
        v: 1,
        name: made["name"].as_str().unwrap_or("Linux box").into(),
        routes: vec![route],
    };
    Ok(json!({"code": made["code"], "link": link.encode(), "expiresAt": made["expiresAt"]}))
}

async fn redeem(input: &Value) -> Result<Value> {
    let (key, mac) = window_key()?;
    let name = crate::sphere::str_of(input, "computer")?.to_string();
    let link = Link::decode(crate::sphere::str_of(input, "link")?).map_err(anyhow::Error::msg)?;
    let code = crate::sphere::str_of(input, "code")?;
    let mut c = Computer {
        name: name.clone(),
        supervisor: Supervisor::new(link.routes.clone(), 2),
        token: None,
        pairing: None,
        device: None,
        tunnels: BTreeMap::new(),
        ssh: None,
        dialed: BTreeMap::new(),
        moves: vec![],
    };
    // Reached first, with the pin: a code typed to someone else's computer gives it nothing.
    if let Some(Change::Lost { tried }) = round(&mut c).await {
        return Ok(
            json!({"paired": false, "said": computers::unreachable(&link.name, &tried), "tried": tried}),
        );
    }
    let route = c
        .supervisor
        .current()
        .cloned()
        .context("no route answered")?;
    let body = json!({"credential": code, "name": mac}).to_string();
    let redeemed = match through(
        &mut c,
        &route,
        "POST",
        "/v1/pairing",
        &[],
        Some(&body),
        Duration::from_secs(20),
    )
    .await
    {
        Ok(a) => a,
        Err(why) => return Ok(json!({"paired": false, "said": why})),
    };
    if redeemed.status != 200 {
        return Ok(json!({"paired": false, "status": redeemed.status, "body": redeemed.body}));
    }
    let token = redeemed.body["token"]
        .as_str()
        .context("no token")?
        .to_string();
    c.token = Some(token.clone());
    c.pairing = redeemed.body["deviceID"].as_str().map(str::to_string);
    // Its own key, through the invitation the other computer's owner approved.
    let auth = vec![("Authorization".to_string(), format!("Bearer {token}"))];
    let added = through(
        &mut c,
        &route,
        "POST",
        "/v1/devices",
        &auth,
        Some(&json!({"name": mac, "publicKey": key.public()}).to_string()),
        Duration::from_secs(20),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    c.device = added.body["id"].as_str().map(str::to_string);
    let out = describe(&c);
    keep(c)?;
    Ok(
        json!({"paired": true, "status": redeemed.status, "keyStatus": added.status, "computer": out}),
    )
}

// -------------------------------------------------------------------- routes and the supervisor

async fn add_route(input: &Value) -> Result<Value> {
    let c = computer(crate::sphere::str_of(input, "computer")?)?;
    let mut c = c.lock().await;
    let id = crate::sphere::str_of(input, "id")?;
    let route = if let Some(url) = input["url"].as_str() {
        let pin = Pin::parse(crate::sphere::str_of(input, "pin")?).map_err(anyhow::Error::msg)?;
        let kind = input["kind"]
            .as_str()
            .map(|k| serde_json::from_value::<RouteKind>(json!(k)))
            .transpose()?;
        Route::direct(id, url, pin, kind)
    } else {
        let (ssh, alias) = lab_ssh(&input["ssh"])?;
        c.ssh = Some(ssh);
        Route::ssh(
            id,
            &alias,
            input["remotePort"].as_u64().unwrap_or(8810) as u16,
        )
    }
    .map_err(anyhow::Error::msg)?;
    c.supervisor.add(route);
    Ok(describe(&c))
}

static WATCHERS: Mutex<Vec<tokio::task::JoinHandle<()>>> = Mutex::new(Vec::new());

async fn watch(input: &Value) -> Result<Value> {
    let c = computer(crate::sphere::str_of(input, "computer")?)?;
    let every = Duration::from_millis(input["everyMs"].as_u64().unwrap_or(500));
    let task = tokio::spawn(async move {
        loop {
            {
                let mut c = c.lock().await;
                round(&mut c).await;
            }
            tokio::time::sleep(every).await;
        }
    });
    WATCHERS.lock().unwrap().push(task);
    Ok(json!({"watching": true, "everyMs": every.as_millis() as u64}))
}

async fn wait(input: &Value) -> Result<Value> {
    let c = computer(crate::sphere::str_of(input, "computer")?)?;
    let want = input["route"].as_str().map(str::to_string);
    let within = Duration::from_secs(input["within"].as_u64().unwrap_or(60));
    let until = Instant::now() + within;
    loop {
        let (now, out) = {
            let c = c.lock().await;
            (c.supervisor.current().map(|r| r.id.clone()), describe(&c))
        };
        if now == want {
            let mut out = out;
            out["tried"] = json!(c.lock().await.supervisor.tried());
            return Ok(out);
        }
        if Instant::now() >= until {
            bail!(
                "the window uses {now:?}, not {want:?}, after {}s: {}",
                within.as_secs(),
                out["moves"]
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn call(input: &Value) -> Result<Value> {
    let c = computer(crate::sphere::str_of(input, "computer")?)?;
    let mut c = c.lock().await;
    let method = crate::sphere::str_of(input, "method")?.to_uppercase();
    let path = crate::sphere::str_of(input, "path")?;
    let body = input.get("body").map(Value::to_string);
    if c.supervisor.current().is_none() {
        round(&mut c).await;
    }
    let Some(route) = c.supervisor.current().cloned() else {
        let tried = c.supervisor.tried();
        return Ok(json!({"status": 0, "reached": false, "route": null,
            "said": computers::unreachable(&c.name, &tried), "tried": tried}));
    };
    let mut headers = vec![];
    if input["as"].as_str() != Some("none") {
        if let Some(t) = &c.token {
            headers.push(("Authorization".to_string(), format!("Bearer {t}")));
        }
    }
    match input["sign"].as_str() {
        None => {}
        Some(who @ ("owner" | "stranger")) => {
            let (key, _) = window_key()?;
            let device = c
                .device
                .clone()
                .context("the window's key isn't registered there")?;
            let key = if who == "owner" {
                key
            } else {
                DeviceKey::generate()
            };
            headers.push((
                "X-Oby-Words".into(),
                keys::words_header(&device, &key, &method, path, body.as_deref().unwrap_or("")),
            ));
        }
        Some(other) => bail!("sign: {other}? owner or stranger"),
    }
    let within = Duration::from_secs(input["within"].as_u64().unwrap_or(0));
    let until = Instant::now() + within;
    loop {
        match through(
            &mut c,
            &route,
            &method,
            path,
            &headers,
            body.as_deref(),
            Duration::from_secs(60),
        )
        .await
        {
            Ok(a) => {
                let out = json!({"status": a.status, "reached": true, "route": route.id, "body": a.body,
                    "text": a.text.chars().take(4000).collect::<String>()});
                let pass = input
                    .get("expect")
                    .map(|w| crate::expect::check(w, &out, ""))
                    .unwrap_or_default()
                    .is_empty();
                if pass || Instant::now() >= until {
                    return Ok(out);
                }
            }
            Err(why) => {
                c.supervisor.observe(&route.id, Err(why.clone()));
                c.supervisor.decide();
                return Ok(json!({"status": 0, "reached": false, "route": route.id, "said": why}));
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

// -------------------------------------------------------------------- the other computer is a Mac

/// The lines this workflow added to the person's `~/.ssh/authorized_keys`, by the marker each
/// ends with, and whether the file was made for them.
static AUTHORIZED: Mutex<(Vec<String>, bool)> = Mutex::new((Vec::new(), false));

/// This user's name and home, from the system's account (not `$HOME`, which a run may change).
fn account() -> Result<(String, std::path::PathBuf)> {
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        anyhow::ensure!(!pw.is_null(), "no account for this user");
        let name = std::ffi::CStr::from_ptr((*pw).pw_name)
            .to_string_lossy()
            .into_owned();
        let dir = std::ffi::CStr::from_ptr((*pw).pw_dir)
            .to_string_lossy()
            .into_owned();
        Ok((name, dir.into()))
    }
}

/// Whether an SSH server answers on this Mac (macOS Remote Login), read from its banner.
async fn remote_login() -> bool {
    let probe = async {
        let mut tcp = tokio::net::TcpStream::connect(("127.0.0.1", 22))
            .await
            .ok()?;
        let mut banner = [0u8; 4];
        tcp.read_exact(&mut banner).await.ok()?;
        Some(&banner == b"SSH-")
    };
    tokio::time::timeout(Duration::from_secs(3), probe)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}

async fn mac_computer(run: &Run, input: &Value) -> Result<Value> {
    if !cfg!(target_os = "macos") {
        return Ok(
            json!({"skipped": "skipped: the other computer is this Mac, and this isn't a Mac"}),
        );
    }
    if !remote_login().await {
        return Ok(
            json!({"skipped": "skipped: macOS Remote Login is off (System Settings, General, Sharing). The lab never turns it on: that's the owner's call."}),
        );
    }
    let name = input["name"].as_str().unwrap_or("other-mac");
    let alias = input["alias"].as_str().unwrap_or("lab-mac");
    let (user, person_home) = account()?;
    let ssh_dir = person_home.join(".ssh");
    if !ssh_dir.is_dir() {
        return Ok(json!({"skipped": "skipped: this user has no ~/.ssh, and the lab makes none"}));
    }
    // The other computer: a second scratch obid with its own home, port and user folder.
    let obid = crate::obid::start(
        run,
        crate::obid::StartInput {
            name: Some(name.into()),
            agent: Some("fake-harness-jobs".into()),
            config: Some(json!({"gateway": "127.0.0.1:0"})),
            ..Default::default()
        },
    )
    .await?;
    let base = crate::sphere::str_of(&obid, "base")?;
    let gateway_base = crate::sphere::str_of(&obid, "gatewayBase")?;
    let api = base.trim_start_matches("http://");
    let gateway = gateway_base.trim_start_matches("http://");
    let gateway_port = gateway.rsplit(':').next().unwrap_or_default().to_string();
    let home = crate::sphere::str_of(&obid, "home")?;
    let folder = std::path::PathBuf::from(crate::sphere::str_of(&obid, "folder")?);
    let helper = folder.join("pair-computer");
    std::fs::write(
        &helper,
        computers::pair_computer_script(home, api, gateway_base).map_err(anyhow::Error::msg)?,
    )?;
    std::fs::set_permissions(&helper, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    // This Mac's host key, as the person would check it: the system's own file.
    let host_key = std::fs::read_to_string("/etc/ssh/ssh_host_ed25519_key.pub")
        .context("this Mac's ed25519 host key")?;
    let host_key: Vec<&str> = host_key.split_whitespace().take(2).collect();
    let access = super::machine::ssh_access(run, alias, "127.0.0.1", &user, &host_key.join(" "))?;
    // The key may run only the pairing program and open only the gateway's port, for an hour.
    let marker = format!("obid-spec-lab-{}", uuid::Uuid::new_v4());
    let until = (chrono::Utc::now() + chrono::Duration::hours(1)).format("%Y%m%d%H%MZ");
    let options = format!(
        "restrict,port-forwarding,permitopen=\"127.0.0.1:{gateway_port}\",expiry-time=\"{until}\",command=\"{}\"",
        helper.display()
    );
    anyhow::ensure!(
        !helper.display().to_string().contains(['"', '\n']),
        "the helper's path can't be quoted"
    );
    let key_part: Vec<&str> = access.public.split_whitespace().take(2).collect();
    let line = format!("{options} {} {marker}\n", key_part.join(" "));
    let file = ssh_dir.join("authorized_keys");
    {
        let mut kept = AUTHORIZED.lock().unwrap();
        kept.0.push(marker.clone());
        if !file.exists() {
            kept.1 = true;
        }
    }
    let ends_clean = std::fs::read(&file)
        .map(|b| b.is_empty() || b.ends_with(b"\n"))
        .unwrap_or(true);
    let mut out = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(&file)?;
    std::io::Write::write_all(
        &mut out,
        format!("{}{line}", if ends_clean { "" } else { "\n" }).as_bytes(),
    )?;
    Ok(json!({
        "ssh": {"alias": alias, "config": access.config, "home": access.home, "host": "127.0.0.1", "user": user},
        "obid": {"base": base, "token": obid["token"], "home": home, "folder": folder, "pid": obid["pid"],
                 "api": api, "gateway": gateway, "pairCommand": helper,
                 "apiPort": api.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()),
                 "gatewayPort": gateway_port.parse::<u16>().ok()},
        "authorized": {"options": options, "marker": marker},
    }))
}

/// Takes this workflow's lines out of `~/.ssh/authorized_keys`, leaving every other line as it
/// was (and the file gone only if the workflow made it and it's empty). How many of them are left.
fn forget_authorized() -> Result<usize> {
    let (markers, made) = AUTHORIZED.lock().unwrap().clone();
    if markers.is_empty() {
        return Ok(0);
    }
    let (_, person_home) = account()?;
    let file = person_home.join(".ssh/authorized_keys");
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Ok(0);
    };
    let ours = |l: &str| markers.iter().any(|m| l.ends_with(m.as_str()));
    let kept: Vec<&str> = text.lines().filter(|l| !ours(l)).collect();
    if kept.len() != text.lines().count() {
        if made && kept.iter().all(|l| l.trim().is_empty()) {
            std::fs::remove_file(&file)?;
        } else {
            let part =
                file.with_file_name(format!(".authorized_keys.obid-spec-{}", std::process::id()));
            let mut body = kept.join("\n");
            if !body.is_empty() {
                body.push('\n');
            }
            crate::obid::private_file(&part, &body)?;
            std::fs::rename(&part, &file)?;
        }
    }
    let left = std::fs::read_to_string(&file)
        .map(|t| t.lines().filter(|l| ours(l)).count())
        .unwrap_or(0);
    if left == 0 {
        *AUTHORIZED.lock().unwrap() = (Vec::new(), false);
    }
    Ok(left)
}

async fn tunnel_probe(input: &Value) -> Result<Value> {
    let (ssh, alias) = lab_ssh(&input["ssh"])?;
    let port = input["remotePort"].as_u64().context("remotePort")? as u16;
    let mut tunnel = match Tunnel::open(&ssh, &alias, port, Duration::from_secs(20), |pid| {
        crate::procs::remember(pid, &format!("ssh tunnel to {alias}"))
    })
    .await
    {
        Ok(t) => t,
        Err(t) => return Ok(json!({"reached": false, "said": t.say(&alias)})),
    };
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", tunnel.local)).await?;
    let got = tokio::time::timeout(
        PROBE,
        http_over(tcp, "127.0.0.1", "GET", "/v1/status", &[], None),
    )
    .await;
    let _ = tunnel.child.start_kill();
    let _ = tunnel.child.wait().await;
    Ok(match got {
        Ok(Ok(a)) => json!({"reached": true, "status": a.status}),
        Ok(Err(e)) => {
            json!({"reached": false, "said": format!("{e:#}"), "stderr": tunnel.stderr.text()})
        }
        Err(_) => json!({"reached": false, "said": "no answer", "stderr": tunnel.stderr.text()}),
    })
}

fn find(input: &Value) -> Result<Value> {
    let home = std::path::PathBuf::from(crate::sphere::str_of(&input["obid"], "home")?);
    let name = crate::sphere::str_of(input, "name")?;
    let mut found = vec![];
    let mut stack = vec![home.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                stack.push(e.path());
            } else if kind.is_file() && e.file_name() == name {
                found.push(
                    e.path()
                        .strip_prefix(&home)
                        .unwrap_or(&e.path())
                        .display()
                        .to_string(),
                );
            }
        }
    }
    found.sort();
    Ok(json!({ "found": found, "count": found.len() }))
}

/// What the remote feature left running in this process: the supervisors, the addresses and
/// the tunnels (ssh is also stopped by PID at the workflow's end, `procs`).
pub async fn cleanup() -> Value {
    let mut stopped = 0;
    for t in WATCHERS
        .lock()
        .unwrap()
        .drain(..)
        .chain(FRONTS.lock().unwrap().drain(..))
    {
        t.abort();
        stopped += 1;
    }
    let computers: Vec<_> = WINDOW
        .lock()
        .unwrap()
        .take()
        .map(|w| w.computers.into_values().collect())
        .unwrap_or_default();
    let mut tunnels = 0;
    for c in computers {
        let mut c = c.lock().await;
        for (_, mut t) in std::mem::take(&mut c.tunnels) {
            let _ = t.child.start_kill();
            let _ = t.child.wait().await;
            tunnels += 1;
        }
    }
    ADDRESSES.lock().unwrap().clear();
    let authorized = match forget_authorized() {
        Ok(left) => json!({ "left": left }),
        Err(e) => json!({ "error": format!("{e:#}") }),
    };
    json!({"tasks": stopped, "tunnels": tunnels, "authorizedKeys": authorized})
}
