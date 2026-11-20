//! A route to another computer and its kind (docs/contracts/remote.md, COMPUTERS-1 and 2). A direct
//! address is always HTTPS with its certificate pinned; an SSH route is the person's own alias.
//! Routes go in the owner's order: a loopback (the same machine), the LAN, a VPN or tailnet,
//! public, then SSH; the person's own order is kept within a kind. Tailscale is one tailnet among
//! others, never required.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use super::pin::Pin;

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
    /// The owner's order: LAN, then VPN or tailnet, then public, then SSH (a loopback, the same
    /// machine, before all).
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

/// What a direct address is, from its host alone. Tailscale's names and ranges (100.64.0.0/10,
/// fd7a:115c:a1e0::/48) are a tailnet; the private ranges, link-local, `.local`, `.lan` and
/// `.home.arpa` the LAN; anything else public. A VPN Oby can't tell from its address is the
/// person's to say ([`Route::direct`]'s `kind`).
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
    /// A direct address, always HTTPS with the other computer's certificate pinned: a plain
    /// address can belong to another machine after a network change, and only the pin says who
    /// answers.
    Https { host: String, port: u16, pin: Pin },
    /// An SSH tunnel to obid's gateway on the other computer's own 127.0.0.1. Who answers is the
    /// host key, which the person accepted in their own `known_hosts`.
    Ssh {
        destination: String,
        #[serde(rename = "remotePort")]
        remote_port: u16,
    },
}

impl Route {
    /// A direct route from `https://host[:port]`. Its kind is the address's unless the person
    /// says (a VPN), and never SSH.
    pub fn direct(id: &str, url: &str, pin: Pin, kind: Option<RouteKind>) -> Result<Route, String> {
        let rest = url.strip_prefix("https://").ok_or_else(|| {
            format!("{url} isn't https://: a direct address needs the other computer's certificate")
        })?;
        let rest = rest.trim_end_matches('/');
        let (host, port) = match rest.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') || (h.starts_with('[') && h.ends_with(']')) => (
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

    /// An SSH route: the person's alias or `user@host`, and the other obid's gateway port on its
    /// own 127.0.0.1.
    pub fn ssh(id: &str, destination: &str, remote_port: u16) -> Result<Route, String> {
        super::ssh::check_destination(destination)?;
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
            Via::Https { host, port, .. } => format!("{} https://{host}:{port}", self.kind.name()),
            Via::Ssh { destination, .. } => format!("ssh {destination}"),
        }
    }
}

/// The routes in the owner's order; the person's own order kept within a kind.
pub fn rank(mut routes: Vec<Route>) -> Vec<Route> {
    routes.sort_by_key(|r| r.kind.rank());
    routes
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn direct(id: &str, url: &str) -> Route {
        Route::direct(id, url, Pin([1; 32]), None).unwrap()
    }

    #[test]
    fn computers_1_routes_rank_lan_then_tailnet_then_public_then_ssh_keeping_the_persons_order() {
        let routes = rank(vec![
            Route::ssh("s", "box", 8810).unwrap(),
            direct("p", "https://203.0.113.9:8443"),
            direct("t", "https://100.64.0.2:8443"),
            direct("l2", "https://10.0.0.9:8443"),
            direct("l1", "https://192.168.1.2:8443"),
            direct("me", "https://localhost:8443"),
        ]);
        let ids: Vec<&str> = routes.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["me", "l2", "l1", "t", "p", "s"]);
    }

    #[test]
    fn computers_1_an_address_is_classified_by_what_it_is() {
        for (host, kind) in [
            ("192.168.1.20", RouteKind::Lan),
            ("10.0.0.3", RouteKind::Lan),
            ("172.16.4.4", RouteKind::Lan),
            ("box.local", RouteKind::Lan),
            ("router.home.arpa", RouteKind::Lan),
            ("100.101.102.103", RouteKind::Tailnet),
            ("100.63.0.1", RouteKind::Public),
            ("box.tail0000.ts.net", RouteKind::Tailnet),
            ("[fd7a:115c:a1e0::1]", RouteKind::Tailnet),
            ("fe80::1", RouteKind::Lan),
            ("fd00::5", RouteKind::Lan),
            ("203.0.113.9", RouteKind::Public),
            ("example.com", RouteKind::Public),
            ("127.0.0.1", RouteKind::Loopback),
            ("[::1]", RouteKind::Loopback),
        ] {
            assert_eq!(classify(host), kind, "{host}");
        }
    }

    #[test]
    fn computers_2_a_direct_address_is_https_and_pinned_only() {
        assert!(Route::direct("x", "http://10.0.0.1:8810", Pin([1; 32]), None).is_err());
        assert!(Route::direct("x", "10.0.0.1:8810", Pin([1; 32]), None).is_err());
        assert!(Route::direct("x", "https://a b:1", Pin([1; 32]), None).is_err());
        assert!(Route::direct("x", "https://user@host:1", Pin([1; 32]), None).is_err());
        assert!(Route::direct("x", "https://host:99999", Pin([1; 32]), None).is_err());
        assert!(
            Route::direct("x", "https://h:1", Pin([1; 32]), Some(RouteKind::Ssh)).is_err(),
            "a direct address is never an SSH route"
        );
        let r = direct("x", "https://[fd7a:115c:a1e0::1]:8443");
        assert_eq!(r.kind, RouteKind::Tailnet);
        assert!(matches!(r.via, Via::Https { port: 8443, .. }));
        let default = direct("x", "https://box.local/");
        assert!(matches!(&default.via, Via::Https { host, port: 443, .. } if host == "box.local"));
        let vpn = Route::direct(
            "v",
            "https://203.0.113.9:8443",
            Pin([1; 32]),
            Some(RouteKind::Vpn),
        );
        assert_eq!(vpn.unwrap().kind, RouteKind::Vpn);
    }

    #[test]
    fn computers_2_a_route_is_kept_as_json_with_its_pin() {
        let r = direct("lan", "https://192.168.1.2:8443");
        let text = serde_json::to_string(&r).unwrap();
        assert!(
            text.contains(&format!("sha256:{}", "01".repeat(32))),
            "{text}"
        );
        assert_eq!(serde_json::from_str::<Route>(&text).unwrap(), r);
        let s = Route::ssh("s", "box", 8810).unwrap();
        let text = serde_json::to_string(&s).unwrap();
        assert!(text.contains("\"remotePort\":8810"), "{text}");
        assert_eq!(serde_json::from_str::<Route>(&text).unwrap(), s);
        assert_eq!(s.describe(), "ssh box");
        assert_eq!(r.describe(), "lan https://192.168.1.2:8443");
    }
}
