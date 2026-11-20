//! What the other computer shows to be added without SSH (docs/contracts/remote.md, COMPUTERS-6):
//! its name and its direct routes, each with its certificate pinned, as `oby-computer:` and
//! base64url JSON. The six-digit code the person types travels apart from it, never instead of
//! it: a code alone can't carry a pin.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::pin::Pin;

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
}
