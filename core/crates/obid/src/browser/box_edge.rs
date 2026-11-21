//! The foreign lane's browser (BROWSER-18): who a job's browser works for, read from obid's own
//! records, and the box's edge its open and navigate stop at. Another Oby's task opens web pages
//! (`http`, `https`), never `file:` or a browser page, and never one of the host's own services on
//! loopback: obid's API (the browser's with it) and its gateway. The lab's model is
//! `tools/obid-spec/src/lab/browsersphere.rs` (`Origin::of_job`, `in_box`). It owns no state and
//! writes nothing.

use super::api::Refused;
use super::contract::Origin;

/// Whether a foreign job (another Oby's task, one marked `untrusted`, Oby's own review) gets a
/// browser. Off until the window's side of the box's edge lands, the browser lane's task after
/// W01-03: the window runs pages on the owner's Mac and network, outside the box's egress proxy,
/// and [`in_box`] reads only the address open and navigate are given. That task turns this on once
/// the window holds a foreign page to the job's egress decisions, refuses private, link-local and
/// every loopback host after resolving names, and holds script, links, redirects and navigations a
/// page starts to the same rules (`evaluate`, `act`). Until then `jobs::reach` gives such a job no
/// browser server and every tool refuses it ([`not_yet`]); the tests that fail if this breaks are
/// `browser_api.rs` `browser_18_a_foreign_jobs_browser_tools_are_refused_until_the_windows_edge`
/// and `jobs_api.rs` `lanes_23_*` (an untrusted run loads no server).
pub const FOREIGN_BROWSER: bool = false;

/// A foreign job's browser tool while [`FOREIGN_BROWSER`] is off: it finishes without one.
pub fn not_yet() -> Refused {
    Refused::new(
        403,
        "browser_off",
        "A task from another Oby, or one marked untrusted, gets no browser on this Oby yet. Finish without a browser, and say so in your summary.",
    )
}

/// How many of the Sphere's latest exchanges [`origin_of`] looks through for the one that brought
/// a job: a running job's is among them.
pub const EXCHANGES_LOOKED: usize = 500;

/// Who `job` works for, from obid's records only: the incoming exchange that brought it (its peer
/// and the peer's owner, from the Sphere's directory: `exchanges` as `Link::exchanges_json` gives
/// them), or the Obi-to-Obi request it serves (`Job::obi`), else its `untrusted` mark or Oby's own
/// review. A follow-up is read the same way back along the jobs it follows up (`earlier` finds
/// each), so one of another Oby's job or review, or of Oby's own review, is never the owner's; a
/// line it can't read to its start (an earlier job deleted, or more of them than the walk reads)
/// is foreign too, never the owner's. Its
/// `foreign` mark isn't read: a page an own job's agent read sets it too. Nothing its request says
/// (its goal, its summary) is read here.
pub fn origin_of(
    job: &crate::job::Job,
    exchanges: &serde_json::Value,
    earlier: impl Fn(&str) -> Option<crate::job::Job>,
) -> Origin {
    let mut next = Some(job.clone());
    let mut untrusted = false;
    // A line of follow-ups is short; this only stops a loop, and a line past it is foreign.
    for _ in 0..64 {
        // An earlier job gone: where the line came from can't be shown.
        let Some(j) = next else {
            return Origin::Untrusted;
        };
        let brought = exchanges
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["direction"] == "incoming" && e["job"].as_str() == Some(j.id.as_str()));
        if let Some(e) = brought {
            return Origin::Obi {
                peer: e["peer"].as_str().unwrap_or_default().into(),
                peer_owner: e["peerOwner"].as_str().unwrap_or_default().into(),
            };
        }
        if let Some(obi) = &j.obi {
            return Origin::Obi {
                peer: obi.from.clone(),
                peer_owner: obi.from_owner.clone(),
            };
        }
        untrusted |= j.untrusted || j.review_of.is_some();
        let Some(of) = &j.follow_up_of else {
            return if untrusted {
                Origin::Untrusted
            } else {
                Origin::Own
            };
        };
        next = earlier(of);
    }
    Origin::Untrusted
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

    fn job(id: &str, kind: &str) -> crate::job::Job {
        crate::job::Job::new(
            id.into(),
            "o/r".into(),
            kind.into(),
            "t".into(),
            "claude".into(),
            "a".into(),
        )
    }

    /// A follow-up keeps where its line of jobs came from: one of another Oby's review (its `obi`
    /// on the review, not on the follow-up) works for that Oby, one of Oby's own review is
    /// untrusted, and one of the owner's own job is the owner's (BROWSER-18).
    #[test]
    fn browser_18_a_follow_up_of_another_obys_review_is_foreign() {
        let mut theirs = job("r1", "review");
        theirs.review_of = Some("j0".into());
        theirs.obi = Some(crate::job::ObiRef {
            correlation_id: "c".into(),
            from: "obi-b".into(),
            from_owner: "ben".into(),
            request_id: "q".into(),
        });
        let mut ours = job("r2", "review");
        ours.review_of = Some("j0".into());
        let own = job("j0", "build");
        let earlier = [theirs, ours, own];
        let get = |id: &str| earlier.iter().find(|j| j.id == id).cloned();
        let follow = |of: &str| {
            let mut f = job(&format!("f-{of}"), "build");
            f.follow_up_of = Some(of.into());
            // A follow-up of a foreign job carries `foreign`, which a page read also sets: never
            // what decides.
            f.foreign = Some("it follows up a job whose check got none of them".into());
            f
        };
        let obi = Origin::Obi {
            peer: "obi-b".into(),
            peer_owner: "ben".into(),
        };
        assert_eq!(origin_of(&follow("r1"), &serde_json::Value::Null, get), obi);
        let mut twice = follow("f-r1");
        twice.follow_up_of = Some("f-r1".into());
        let chain = [follow("r1")];
        let get2 = |id: &str| get(id).or_else(|| chain.iter().find(|j| j.id == id).cloned());
        assert_eq!(origin_of(&twice, &serde_json::Value::Null, get2), obi);
        assert_eq!(
            origin_of(&follow("r2"), &serde_json::Value::Null, get),
            Origin::Untrusted
        );
        assert_eq!(
            origin_of(&follow("j0"), &serde_json::Value::Null, get),
            Origin::Own
        );
    }

    /// A line whose earlier job is gone, or longer than the walk reads, can't show it's the
    /// owner's: it is foreign, never `Own` (BROWSER-18).
    #[test]
    fn browser_18_a_line_of_follow_ups_that_cant_be_read_to_its_start_is_foreign() {
        let mut gone = job("f1", "build");
        gone.follow_up_of = Some("deleted".into());
        let none = |_: &str| None;
        assert_eq!(
            origin_of(&gone, &serde_json::Value::Null, none),
            Origin::Untrusted
        );
        // 70 own follow-ups, the first of them following up another Oby's review.
        let mut line: Vec<crate::job::Job> = (0..70)
            .map(|n| {
                let mut f = job(&format!("f{n}"), "build");
                f.follow_up_of = Some(if n == 0 {
                    "r".into()
                } else {
                    format!("f{}", n - 1)
                });
                f
            })
            .collect();
        let mut review = job("r", "review");
        review.obi = Some(crate::job::ObiRef {
            correlation_id: "c".into(),
            from: "obi-b".into(),
            from_owner: "ben".into(),
            request_id: "q".into(),
        });
        line.push(review);
        let get = |id: &str| line.iter().find(|j| j.id == id).cloned();
        let last = get("f69").unwrap();
        assert!(origin_of(&last, &serde_json::Value::Null, get).foreign());
        // A line of the owner's own that ends where the walk can read it stays the owner's.
        let mut own = job("o0", "build");
        own.follow_up_of = None;
        let mut next = job("o1", "build");
        next.follow_up_of = Some("o0".into());
        let owned = [own];
        let get = |id: &str| owned.iter().find(|j| j.id == id).cloned();
        assert_eq!(origin_of(&next, &serde_json::Value::Null, get), Origin::Own);
    }

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
