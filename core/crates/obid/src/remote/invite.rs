//! Adding this computer with a code (docs/contracts/remote.md, COMPUTERS-13), the other computer's
//! side: a computer's invitation carries an `oby-computer:` link (super::link) holding this
//! computer's direct addresses, each with its certificate pinned, so the Mac reaches it and knows
//! who answers before it sends the code.
//!
//! `obid invite-computer` makes one at this machine, as `obid pair-computer` pairs over SSH: being
//! at it is the owner's presence, shown to obid by a one-time file it writes in obid's home
//! (`home/invite-computer`, good for 2 minutes and once, as `home/pairing` is), so the computer the
//! code pairs may add its own key. obid's API token alone makes one that can't.

use std::io::Read as _;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use super::addresses;
use super::link::{Link, OneLink};
use super::pair::Refused;
use super::pin::Pin;
use super::route::{Route, rank};
use crate::config::Config;

const PRESENCE: &str = "invite-computer";
const PRESENCE_OPEN: Duration = Duration::from_secs(120);

/// Writes the one-time presence file and answers what it holds.
pub fn presence_write(home: &Path) -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
    let nonce = hex::encode(bytes);
    crate::store::write_private(&home.join(PRESENCE), nonce.as_bytes())?;
    Ok(nonce)
}

/// Whether `home/invite-computer` holds `nonce`, written in the last 2 minutes: then it's used up.
pub fn presence_check(home: &Path, nonce: &str, now: SystemTime) -> bool {
    let path = home.join(PRESENCE);
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return false;
    };
    let fresh = meta.is_file()
        && meta
            .modified()
            .is_ok_and(|written| match now.duration_since(written) {
                Ok(age) => age < PRESENCE_OPEN,
                Err(ahead) => ahead.duration() < PRESENCE_OPEN,
            });
    let mut held = String::new();
    let read = std::fs::File::open(&path).and_then(|f| f.take(256).read_to_string(&mut held));
    let _ = std::fs::remove_file(&path);
    fresh && read.is_ok() && !nonce.is_empty() && crate::token::same(held.trim(), nonce)
}

/// The link for this computer: its name and the direct addresses `status.gateway.direct` says it
/// listens on, each pinned. An address on every interface (`0.0.0.0`, `::`) stands for each of the
/// machine's own addresses but loopback and IPv6 link-local; at most 8, in the owner's order.
pub fn link(name: &str, direct: &Value) -> Option<Link> {
    let mut routes = Vec::new();
    for entry in direct.as_array().into_iter().flatten() {
        if entry["state"] != "listening" {
            continue;
        }
        let (Some(address), Some(pin)) = (
            entry["address"]
                .as_str()
                .and_then(|a| a.parse::<SocketAddr>().ok()),
            entry["pin"].as_str().and_then(|p| Pin::parse(p).ok()),
        ) else {
            continue;
        };
        let ips = if address.ip().is_unspecified() {
            interface_addresses(address.is_ipv6())
        } else {
            vec![address.ip()]
        };
        for ip in ips {
            let host = match ip {
                IpAddr::V4(v4) => v4.to_string(),
                IpAddr::V6(v6) => format!("[{v6}]"),
            };
            let id = format!("direct-{}", routes.len() + 1);
            if let Ok(route) = Route::direct(
                &id,
                &format!("https://{host}:{}", address.port()),
                pin,
                None,
            ) && !routes.iter().any(|r: &Route| r.via == route.via)
            {
                routes.push(route);
            }
        }
    }
    let mut routes = rank(routes);
    routes.truncate(8);
    (!routes.is_empty()).then(|| Link {
        v: 1,
        name: name.chars().filter(|c| !c.is_control()).take(100).collect(),
        routes,
    })
}

/// The one link for an invitation whose secret `make` answered (base64url of 16 bytes) and the
/// routes of its link (COMPUTERS-29); `None` for a secret of another size.
pub fn one_link(secret: &str, link: &Link) -> Option<String> {
    use base64::Engine as _;
    let bytes: [u8; 16] = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(secret)
        .ok()?
        .try_into()
        .ok()?;
    OneLink::of(bytes, &link.routes)?.encode()
}

/// This machine's own addresses another computer could reach: no loopback, no IPv6 link-local
/// (it needs a scope a link can't carry); IPv6 too when `v6` (a `::` listener takes both).
pub(crate) fn interface_addresses(v6: bool) -> Vec<IpAddr> {
    let Ok(all) = nix::ifaddrs::getifaddrs() else {
        return Vec::new();
    };
    let mut ips = Vec::new();
    for interface in all {
        let Some(address) = interface.address else {
            continue;
        };
        let ip = if let Some(v4) = address.as_sockaddr_in() {
            IpAddr::V4(v4.ip())
        } else if let Some(six) = address.as_sockaddr_in6().filter(|_| v6) {
            IpAddr::V6(six.ip())
        } else {
            continue;
        };
        let link_local = matches!(ip, IpAddr::V6(six) if (six.segments()[0] & 0xffc0) == 0xfe80);
        if !ip.is_loopback() && !ip.is_unspecified() && !link_local && !ips.contains(&ip) {
            ips.push(ip);
        }
    }
    ips
}

/// The first 16 bytes of a pin, in groups of four hex digits: what a person compares by eye
/// (docs/design/computer-picker); the link carries the whole pin.
pub fn fingerprint(pin: &Pin) -> String {
    hex::encode(&pin.0[..16])
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_uppercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `obid invite-computer`: makes this computer's invitation on the obid `config` names, as its
/// owner at this machine, and answers the line to print: `{"code", "link", "name",
/// "fingerprint", "expiresAt", "oneLink"}`: `link` and `code` as COMPUTERS-13 has them, and
/// `oneLink` the same invitation as one link with no code (COMPUTERS-29), which redeems it too.
pub async fn run(config: &Path) -> Result<String, Refused> {
    let refused = |code: u8, said: String| Refused { code, said };
    let config = Config::load(config).map_err(|e| refused(1, e.to_string()))?;
    let home = &config.mac_home;
    let api = addresses::read(home).map_or(config.listen, |a| a.api);
    let token = std::fs::read_to_string(home.join(crate::token::FILE))
        .map(|t| t.trim_end_matches(['\n', '\r']).to_string())
        .map_err(|_| {
            refused(
                1,
                "obid hasn't started on this computer yet: start it, then try again.".into(),
            )
        })?;
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| refused(1, e.to_string()))?;
    let status: Value = http
        .get(format!("http://{api}/v1/status"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|_| refused(1, format!("obid doesn't answer on {api}: is it running?")))?
        .json()
        .await
        .unwrap_or(Value::Null);
    let name = status["runner"]["host"]
        .as_str()
        .unwrap_or("Another computer")
        .to_string();
    let Some(link) = link(&name, &status["gateway"]["direct"]) else {
        return Err(refused(
            1,
            "This computer has no address another computer can reach. Set \"direct\" in obid's configuration, or add it over SSH instead.".into(),
        ));
    };
    let presence = presence_write(home)
        .map_err(|e| refused(1, format!("can't write obid's presence file: {e}")))?;
    let first = link.routes[0].describe();
    let url = first
        .split_once(' ')
        .map_or(first.as_str(), |(_, u)| u)
        .to_string();
    let answer = http
        .post(format!("http://{api}/v1/pairing/invite"))
        .bearer_auth(&token)
        .json(&json!({"url": url, "name": name, "computer": true, "presence": presence}))
        .send()
        .await
        .map_err(|_| refused(1, format!("obid doesn't answer on {api}: is it running?")))?;
    let ok = answer.status().is_success();
    let body: Value = answer.json().await.unwrap_or(Value::Null);
    if !ok {
        let _ = std::fs::remove_file(home.join(PRESENCE));
        let said = body["error"].as_str().unwrap_or("no reason given");
        return Err(refused(1, format!("obid refused the invitation: {said}")));
    }
    let pin = match &link.routes[0].via {
        super::route::Via::Https { pin, .. } => *pin,
        super::route::Via::Ssh { .. } => unreachable!("a link holds direct routes only"),
    };
    Ok(json!({
        "code": body["code"],
        "link": body["computerLink"],
        "name": name,
        "fingerprint": fingerprint(&pin),
        "expiresAt": body["expiresAt"],
        "oneLink": body["computerOneLink"],
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computers_13_presence_is_once_and_only_fresh() {
        let home = std::env::temp_dir().join(format!("obid-presence-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let now = SystemTime::now();
        assert!(!presence_check(&home, "", now));
        let nonce = presence_write(&home).unwrap();
        assert!(
            !presence_check(&home, "not-it", now),
            "a wrong one uses it up"
        );
        let nonce2 = presence_write(&home).unwrap();
        assert_ne!(nonce, nonce2);
        assert!(presence_check(&home, &nonce2, now));
        assert!(!presence_check(&home, &nonce2, now), "once");
        // Two minutes after it was written, whenever that was.
        let late = presence_write(&home).unwrap();
        let written = std::fs::metadata(home.join(PRESENCE))
            .unwrap()
            .modified()
            .unwrap();
        assert!(!presence_check(
            &home,
            &late,
            written + Duration::from_secs(121)
        ));
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn computers_13_the_link_holds_the_direct_addresses_each_pinned() {
        let pin = Pin([3; 32]);
        let direct = json!([
            {"address": "192.168.1.20:8443", "state": "listening", "pin": pin.to_string()},
            {"address": "100.64.0.9:8443", "state": "listening", "pin": pin.to_string()},
            {"address": "10.0.0.1:8443", "state": "down", "detail": "busy"},
        ]);
        let made = link("Studio\u{7}", &direct).unwrap();
        assert_eq!(made.name, "Studio");
        let routes: Vec<String> = made.routes.iter().map(Route::describe).collect();
        assert_eq!(
            routes,
            [
                "lan https://192.168.1.20:8443",
                "tailnet https://100.64.0.9:8443"
            ]
        );
        assert_eq!(Link::decode(&made.encode()).unwrap(), made, "it reads back");
        assert!(link(&"x".repeat(5), &json!([])).is_none());
        // Every interface: this machine's own addresses, never loopback.
        let any =
            json!([{"address": "0.0.0.0:8443", "state": "listening", "pin": pin.to_string()}]);
        if let Some(any) = link("Studio", &any) {
            assert!(any.routes.len() <= 8);
            assert!(
                any.routes
                    .iter()
                    .all(|r| !r.describe().contains("127.0.0.1"))
            );
        }
        assert_eq!(
            fingerprint(&Pin([0xab; 32])),
            "ABAB ABAB ABAB ABAB ABAB ABAB ABAB ABAB"
        );
    }
}
