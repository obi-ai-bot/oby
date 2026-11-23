//! What the other computer shows to be added without SSH (docs/contracts/remote.md, COMPUTERS-6
//! and 29). Version 1: its name and its direct routes, each with its certificate pinned, as
//! `oby-computer:` and base64url JSON, with a six-digit code the person types apart from it (a code
//! alone can't carry a pin). Version 2, one link and no code (the owner's word of 2026-11-23): a
//! few bytes, base64url, holding the invitation's own random secret, the pin and the routes'
//! addresses, so it reads as nothing to a person and is all another Mac needs.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::pin::Pin;
use super::route::{Route, RouteKind, Via};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub v: u8,
    pub name: String,
    pub routes: Vec<Route>,
}

pub const LINK_PREFIX: &str = "oby-computer:";

impl Link {
    pub fn encode(&self) -> String {
        format!(
            "{LINK_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(self).unwrap_or_default())
        )
    }

    /// A link as the other computer made it: version 1, 1 to 8 direct routes, each pinned; its
    /// name without control characters, at most 100 of them.
    pub fn decode(text: &str) -> Result<Link, String> {
        let body = text
            .trim()
            .strip_prefix(LINK_PREFIX)
            .ok_or("That isn't a link to another computer.")?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|_| "The link is cut short or changed.")?;
        let link: Link =
            serde_json::from_slice(&bytes).map_err(|e| format!("The link doesn't read: {e}"))?;
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

/// A one-link invitation (COMPUTERS-29): the invitation's 128-bit secret, which redeems it once,
/// and the direct routes, each pinned. No name: the other computer says it, over a pinned route,
/// to whoever holds the secret (`POST /v1/pairing/peek`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OneLink {
    pub secret: [u8; 16],
    pub routes: Vec<Route>,
}

/// A link of either version, as the Mac reads one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    /// Version 1, redeemed with the code typed apart from it.
    WithCode(Link),
    /// Version 2, redeemed with the secret it holds.
    One(OneLink),
}

impl Read {
    pub fn routes(&self) -> &[Route] {
        match self {
            Read::WithCode(link) => &link.routes,
            Read::One(one) => &one.routes,
        }
    }

    /// The other computer's name, when the link holds it (version 1).
    pub fn name(&self) -> Option<&str> {
        match self {
            Read::WithCode(link) => Some(&link.name),
            Read::One(_) => None,
        }
    }

    /// A link of either version, refused as [`Link::decode`] and [`OneLink::decode`] say.
    pub fn decode(text: &str) -> Result<Read, String> {
        let body = text
            .trim()
            .strip_prefix(LINK_PREFIX)
            .ok_or("That isn't a link to another computer.")?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|_| "The link is cut short or changed.")?;
        if bytes.first() == Some(&ONE_LINK) {
            OneLink::from_bytes(&bytes).map(Read::One)
        } else {
            Link::decode(text).map(Read::WithCode)
        }
    }
}

/// Version 2's first byte: JSON, version 1's, always starts with `{`.
const ONE_LINK: u8 = 2;
/// A route's first byte: its address family, and whether a pin of its own follows (one that isn't
/// the link's first).
const V4: u8 = 4;
const V6: u8 = 6;
const OWN_PIN: u8 = 0x80;

impl OneLink {
    /// At most this many routes go in a one-link, to keep it short: an IPv6 address of a kind an
    /// IPv4 one already covers stays out.
    pub const MOST: usize = 4;

    /// `oby-computer:` and base64url of: 2, the secret, the first route's pin, then each route as
    /// its family (`4` or `6`, `| 0x80` when its own pin follows), its address, its port (big
    /// endian) and that pin. Only addresses: a route by name has no place in it (`None`).
    pub fn encode(&self) -> Option<String> {
        let first = self.routes.first().and_then(pin_of)?;
        let mut bytes = vec![ONE_LINK];
        bytes.extend_from_slice(&self.secret);
        bytes.extend_from_slice(&first.0);
        for route in &self.routes {
            let Via::Https { host, port, pin } = &route.via else {
                return None;
            };
            let ip: std::net::IpAddr = host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse()
                .ok()?;
            let own = if *pin == first { 0 } else { OWN_PIN };
            match ip {
                std::net::IpAddr::V4(v4) => {
                    bytes.push(V4 | own);
                    bytes.extend_from_slice(&v4.octets());
                }
                std::net::IpAddr::V6(v6) => {
                    bytes.push(V6 | own);
                    bytes.extend_from_slice(&v6.octets());
                }
            }
            bytes.extend_from_slice(&port.to_be_bytes());
            if own != 0 {
                bytes.extend_from_slice(&pin.0);
            }
        }
        Some(format!(
            "{LINK_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        ))
    }

    /// A one-link for `secret` from version 1's routes, in their order: IPv4 first within a kind,
    /// at most [`OneLink::MOST`]. `None` without a route to carry.
    pub fn of(secret: [u8; 16], routes: &[Route]) -> Option<OneLink> {
        let is_v6 = |r: &Route| matches!(&r.via, Via::Https { host, .. } if host.contains(':'));
        let mut kept: Vec<Route> = Vec::new();
        for route in routes {
            if is_v6(route)
                && routes
                    .iter()
                    .any(|other| other.kind == route.kind && !is_v6(other))
            {
                continue;
            }
            kept.push(route.clone());
        }
        kept.truncate(Self::MOST);
        (!kept.is_empty()).then_some(OneLink {
            secret,
            routes: kept,
        })
    }

    /// The secret as the other computer's gateway takes it: base64url, as `make` answers it.
    pub fn credential(&self) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(self.secret)
    }

    fn from_bytes(bytes: &[u8]) -> Result<OneLink, String> {
        const CHANGED: &str = "The link is cut short or changed.";
        let take = |at: &mut usize, n: usize| -> Result<&[u8], String> {
            let part = bytes.get(*at..*at + n).ok_or(CHANGED)?;
            *at += n;
            Ok(part)
        };
        let mut at = 1;
        let secret: [u8; 16] = take(&mut at, 16)?.try_into().map_err(|_| CHANGED)?;
        let first = Pin(take(&mut at, 32)?.try_into().map_err(|_| CHANGED)?);
        let mut routes = Vec::new();
        while at < bytes.len() {
            let tag = take(&mut at, 1)?[0];
            let host = match tag & !OWN_PIN {
                V4 => {
                    let o: [u8; 4] = take(&mut at, 4)?.try_into().map_err(|_| CHANGED)?;
                    std::net::Ipv4Addr::from(o).to_string()
                }
                V6 => {
                    let o: [u8; 16] = take(&mut at, 16)?.try_into().map_err(|_| CHANGED)?;
                    format!("[{}]", std::net::Ipv6Addr::from(o))
                }
                _ => return Err(CHANGED.into()),
            };
            let port = u16::from_be_bytes(take(&mut at, 2)?.try_into().map_err(|_| CHANGED)?);
            let pin = if tag & OWN_PIN != 0 {
                Pin(take(&mut at, 32)?.try_into().map_err(|_| CHANGED)?)
            } else {
                first
            };
            if port == 0 {
                return Err(CHANGED.into());
            }
            let id = format!("direct-{}", routes.len() + 1);
            routes.push(Route::direct(
                &id,
                &format!("https://{host}:{port}"),
                pin,
                None,
            )?);
            if routes.len() > 8 {
                return Err("The link has no route, or too many.".into());
            }
        }
        if routes.is_empty() {
            return Err("The link has no route, or too many.".into());
        }
        Ok(OneLink { secret, routes })
    }
}

fn pin_of(route: &Route) -> Option<Pin> {
    match &route.via {
        Via::Https { pin, .. } => Some(*pin),
        Via::Ssh { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link() -> Link {
        Link {
            v: 1,
            name: "Studio".into(),
            routes: vec![
                Route::direct("lan", "https://192.168.1.2:8443", Pin([1; 32]), None).unwrap(),
            ],
        }
    }

    #[test]
    fn computers_6_a_link_round_trips_and_refuses_anything_unpinned() {
        let link = link();
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
        let many = Link {
            routes: vec![link.routes[0].clone(); 9],
            ..link.clone()
        };
        assert!(Link::decode(&many.encode()).is_err());
        let v2 = Link { v: 2, ..link };
        assert!(
            Link::decode(&v2.encode())
                .unwrap_err()
                .contains("version 2")
        );
        assert!(Link::decode("https://evil.example").is_err());
        assert!(Link::decode("oby-computer:!!").is_err());
    }

    #[test]
    fn computers_6_a_links_name_loses_control_characters() {
        let mut odd = link();
        odd.name = format!("Stu\u{1b}[2Jdio{}", "x".repeat(200));
        let read = Link::decode(&odd.encode()).unwrap();
        assert!(read.name.starts_with("Stu[2Jdio"));
        assert_eq!(read.name.chars().count(), 100);
    }

    fn one() -> OneLink {
        let pin = Pin([0x5a; 32]);
        OneLink {
            secret: [0x11; 16],
            routes: vec![
                Route::direct("direct-1", "https://192.168.1.20:8443", pin, None).unwrap(),
                Route::direct("direct-2", "https://100.64.0.9:8443", pin, None).unwrap(),
            ],
        }
    }

    #[test]
    fn computers_29_a_one_link_round_trips_short_with_no_readable_host_or_pin() {
        let one = one();
        let text = one.encode().unwrap();
        assert!(text.starts_with("oby-computer:"), "{text}");
        assert!(text.len() <= 100, "{} chars: {text}", text.len());
        for readable in ["192.168", "100.64", "8443", "5a5a", "sha256", "Studio"] {
            assert!(!text.contains(readable), "{readable} in {text}");
        }
        assert_eq!(Read::decode(&text).unwrap(), Read::One(one.clone()));
        assert_eq!(Read::decode(&text).unwrap().name(), None);
        // One route alone: under 90 characters.
        let alone = OneLink {
            routes: one.routes[..1].to_vec(),
            ..one.clone()
        };
        assert!(alone.encode().unwrap().len() < 90);
        // A route pinned to another certificate carries its own pin, and reads back.
        let mut two = one.clone();
        two.routes[1] = Route::direct(
            "direct-2",
            "https://[fd7a:115c:a1e0::9]:8443",
            Pin([7; 32]),
            None,
        )
        .unwrap();
        assert_eq!(
            Read::decode(&two.encode().unwrap()).unwrap(),
            Read::One(two)
        );
        assert_eq!(one.credential().len(), 22);
    }

    #[test]
    fn computers_29_a_tampered_one_link_is_refused() {
        let text = one().encode().unwrap();
        let body = text.strip_prefix(LINK_PREFIX).unwrap();
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(body)
            .unwrap();
        let again = |b: &[u8]| {
            format!(
                "{LINK_PREFIX}{}",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
            )
        };
        // Cut short anywhere, a byte too many, an unknown family, a zero port, no route.
        for cut in (1..bytes.len()).filter(|cut| *cut != 56) {
            assert!(Read::decode(&again(&bytes[..cut])).is_err(), "cut at {cut}");
        }
        let mut more = bytes.clone();
        more.push(4);
        assert!(Read::decode(&again(&more)).is_err());
        let mut family = bytes.clone();
        family[49] = 5;
        assert!(Read::decode(&again(&family)).is_err());
        let mut port = bytes.clone();
        port[54] = 0;
        port[55] = 0;
        assert!(Read::decode(&again(&port)).is_err());
        assert!(Read::decode(&again(&bytes[..49])).is_err());
        assert!(Read::decode(&format!("{text}!")).is_err());
        // Version 1 reads as it always did.
        assert_eq!(
            Read::decode(&link().encode()).unwrap(),
            Read::WithCode(link())
        );
    }

    #[test]
    fn computers_29_a_one_link_keeps_ipv4_first_and_at_most_four_routes() {
        let pin = Pin([1; 32]);
        let routes: Vec<Route> = [
            "https://192.168.1.2:8443",
            "https://[fd00::2]:8443",
            "https://10.0.0.2:8443",
            "https://100.64.0.2:8443",
            "https://[fd7a:115c:a1e0::2]:8443",
            "https://203.0.113.2:8443",
            "https://198.51.100.2:8443",
        ]
        .iter()
        .enumerate()
        .map(|(i, url)| Route::direct(&format!("d{i}"), url, pin, None).unwrap())
        .collect();
        let one = OneLink::of([9; 16], &routes).unwrap();
        let kept: Vec<String> = one.routes.iter().map(Route::describe).collect();
        assert_eq!(
            kept,
            [
                "lan https://192.168.1.2:8443",
                "lan https://10.0.0.2:8443",
                "tailnet https://100.64.0.2:8443",
                "public https://203.0.113.2:8443"
            ]
        );
        assert!(OneLink::of([9; 16], &[]).is_none());
        // An IPv6 address alone of its kind stays.
        let v6 = OneLink::of([9; 16], &routes[1..2]).unwrap();
        assert_eq!(v6.routes.len(), 1);
    }
}
