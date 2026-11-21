//! The foreign lane's browser (BROWSER-18): who a job's browser works for, read from obid's own
//! records, and the box's edge its open and navigate stop at. Another Oby's task opens web pages
//! (`http`, `https`), never `file:` or a browser page, and never one of the host's own services on
//! loopback: obid's API (the browser's with it) and its gateway. The lab's model is
//! `tools/obid-spec/src/lab/browsersphere.rs` (`Origin::of_job`, `in_box`). It owns no state and
//! writes nothing.

use super::api::Refused;
use super::contract::Origin;

/// How many of the Sphere's latest exchanges [`origin_of`] looks through for the one that brought
/// a job: a running job's is among them.
pub const EXCHANGES_LOOKED: usize = 500;

/// Who `job` works for, from obid's records only: the incoming exchange that brought it (its peer
/// and the peer's owner, from the Sphere's directory: `exchanges` as `Link::exchanges_json` gives
/// them), or the Obi-to-Obi request it serves (`Job::obi`), else its `untrusted` mark or Oby's own
/// review. Nothing its request says (its goal, its summary) is read here.
pub fn origin_of(job: &crate::job::Job, exchanges: &serde_json::Value) -> Origin {
    let brought = exchanges
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["direction"] == "incoming" && e["job"].as_str() == Some(job.id.as_str()));
    if let Some(e) = brought {
        return Origin::Obi {
            peer: e["peer"].as_str().unwrap_or_default().into(),
            peer_owner: e["peerOwner"].as_str().unwrap_or_default().into(),
        };
    }
    if let Some(obi) = &job.obi {
        return Origin::Obi {
            peer: obi.from.clone(),
            peer_owner: obi.from_owner.clone(),
        };
    }
    if job.untrusted || job.review_of.is_some() {
        return Origin::Untrusted;
    }
    Origin::Own
}

/// Whether a foreign job's browser may open `url`: a web page that isn't one of the host's own
/// services on loopback (`host_ports`). Refused `outside_box` otherwise. The address is read as a
/// browser reads it (WHATWG URL, `reqwest::Url`), so `127.1`, `2130706433` and `[::ffff:7f00:1]`
/// are all loopback.
pub fn in_box(url: &str, host_ports: &[u16]) -> Result<(), Refused> {
    let outside = |why: String| Refused::new(403, "outside_box", why);
    let parsed = reqwest::Url::parse(url.trim())
        .map_err(|_| outside(format!("{url} isn't a web address.")))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(outside(format!(
            "A task from another Oby opens web pages only (http or https), never {scheme}: addresses on the host's computer."
        )));
    }
    // The parser writes every IPv4 form dotted (`2130706433` is `127.0.0.1`) and IPv6 bracketed.
    let host = parsed.host_str().unwrap_or_default();
    let loopback = match host.trim_matches(['[', ']']).parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Ok(std::net::IpAddr::V6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|v4| v4.is_loopback() || v4.is_unspecified())
        }
        Err(_) => {
            let d = host.trim_end_matches('.');
            d.is_empty() || d == "localhost" || d.ends_with(".localhost")
        }
    };
    let port = parsed.port_or_known_default().unwrap_or(0);
    if loopback && host_ports.contains(&port) {
        return Err(outside(
            "That address is one of the host's own services (its Oby, its gateway or its browser), which a task from another Oby never reaches.".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORTS: &[u16] = &[7777, 7778];

    #[test]
    fn browser_18_a_foreign_browser_opens_web_pages_but_not_the_hosts_services_or_files() {
        for ok in [
            "https://example.com/",
            "http://127.0.0.1:8080/",
            "http://localhost:3000/a",
            "https://example.com:7777/",
        ] {
            assert!(in_box(ok, PORTS).is_ok(), "{ok}");
        }
        for no in [
            "http://127.0.0.1:7777/v1/jobs",
            "http://localhost:7778/",
            "http://LOCALHOST.:7777/",
            "http://[::1]:7777/",
            "http://0.0.0.0:7777/",
            "http://127.1.2.3:7777/",
            "http://a.localhost:7778/x",
            "http://user@127.0.0.1:7777/",
            "http://[::ffff:127.0.0.1]:7777/",
            "http://2130706433:7777/",
            "http://0x7f.1:7777/",
            "http://127.1:7778/",
            "HTTP://127.0.0.1:7777/",
            "file:///etc/hosts",
            "chrome://settings",
            "javascript:alert(1)",
            "about:blank",
            "http://127.0.0.1:x/",
        ] {
            let e = in_box(no, PORTS).unwrap_err();
            assert_eq!(e.reason, "outside_box", "{no}");
            assert_eq!(e.status, 403, "{no}");
        }
    }
}
