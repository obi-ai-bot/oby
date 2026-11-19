//! Reading a git remote or a host's web URL, bounded, and recognizing which repository it names.
//!
//! Where T3 Code's host packages kept fixing recognition (an SSH host that isn't the web host,
//! Enterprise hostnames, ports), each re-deriving it; here it is written once, in
//! [`HostNames::recognize`], and every definition calls it.

/// Longer input is refused unread: a remote or a link is never this long.
pub const MAX_URL_BYTES: usize = 8 * 1024;
/// A path with more segments is refused, nested groups included.
pub const MAX_SEGMENTS: usize = 32;

/// How a remote or URL was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Https,
    Http,
    /// `ssh://` or `git+ssh://`.
    Ssh,
    /// `[user@]host:path`, scp's form.
    Scp,
}

/// A remote or web URL taken apart: never a user or password, which are dropped as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location<'a> {
    pub scheme: Scheme,
    /// Lower case.
    pub host: String,
    pub port: Option<u16>,
    /// The path's segments, without the query or fragment, a trailing `/` dropped.
    pub segments: Vec<&'a str>,
}

/// A remote or URL taken apart, or none when it's longer than [`MAX_URL_BYTES`], holds anything
/// but printable ASCII (a homoglyph host, a newline), names no valid host or port, or has an
/// empty, `.`, `..` or unusual segment. Linear in its input, and never panics.
pub fn parse(input: &str) -> Option<Location<'_>> {
    let input = input.trim_matches(|c: char| c.is_ascii_whitespace());
    if input.is_empty()
        || input.len() > MAX_URL_BYTES
        || !input.bytes().all(|b| b.is_ascii_graphic())
    {
        return None;
    }
    let (scheme, authority, path) = match input.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = match scheme.to_ascii_lowercase().as_str() {
                "https" => Scheme::Https,
                "http" => Scheme::Http,
                "ssh" | "git+ssh" => Scheme::Ssh,
                _ => return None,
            };
            let (authority, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
            (scheme, authority, path)
        }
        None => {
            let (authority, path) = input.split_once(':')?;
            if authority.contains(['/', '\\']) {
                return None; // a local path, such as ./a:b
            }
            (Scheme::Scp, authority, path)
        }
    };
    // The host is after the last `@`: `https://forge.example@evil.example/` is evil.example's.
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let (host, port) = match host_port.split_once(':') {
        Some(_) if scheme == Scheme::Scp => return None,
        Some((host, port)) => (host, Some(parse_port(port)?)),
        None => (host_port, None),
    };
    if !is_hostname(host) {
        return None;
    }
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let path = path.strip_prefix('/').unwrap_or(path);
    let path = path.strip_suffix('/').unwrap_or(path);
    let mut segments = Vec::new();
    if !path.is_empty() {
        for segment in path.split('/') {
            if segments.len() == MAX_SEGMENTS || !is_segment(segment) {
                return None;
            }
            segments.push(segment);
        }
    }
    Some(Location {
        scheme,
        host: host.to_ascii_lowercase(),
        port,
        segments,
    })
}

/// A positive number written in decimal digits alone, as a change request's or issue's; none when
/// it overflows.
pub fn parse_number(text: &str) -> Option<u64> {
    if text.is_empty() || text.len() > 20 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|n| *n != 0)
}

/// A commit's id: 7 to 64 hexadecimal digits (SHA-1 abbreviated to SHA-256 whole).
pub fn is_commit_id(text: &str) -> bool {
    (7..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn parse_port(text: &str) -> Option<u16> {
    if text.is_empty() || text.len() > 5 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|p| *p != 0)
}

fn is_hostname(host: &str) -> bool {
    (1..=253).contains(&host.len())
        && host.split('.').all(|label| {
            (1..=63).contains(&label.len())
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

/// One path segment a repository, group or route may have.
pub fn is_segment(segment: &str) -> bool {
    (1..=255).contains(&segment.len())
        && segment != "."
        && segment != ".."
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
}

/// A repository on one host, the same for every spelling of its remote.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Repo {
    /// The host's web authority as configured ([`HostNames::web`]), its port kept when it has one.
    pub host: String,
    /// `owner/name`, or `group/sub/name` where the host has nested groups; no `.git`.
    pub path: String,
}

/// The names one host answers to: its web authority and the SSH hosts that aren't it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostNames {
    /// The web authority, lower case: `github.com`, or `git.corp.example:8443`.
    pub web: String,
    /// SSH-only hosts serving the same repositories: `ssh.github.com`, `ssh.codeberg.org`.
    pub ssh: Vec<String>,
}

impl HostNames {
    pub fn new(web: &str, ssh: &[&str]) -> Self {
        HostNames {
            web: web.to_ascii_lowercase(),
            ssh: ssh.iter().map(|h| h.to_ascii_lowercase()).collect(),
        }
    }

    fn web_parts(&self) -> (&str, Option<u16>) {
        match self.web.split_once(':') {
            Some((host, port)) => (host, port.parse().ok()),
            None => (self.web.as_str(), None),
        }
    }

    /// Whether a web URL is this host's: its host, and its port (the scheme's default when the
    /// web authority has none).
    pub fn serves_web(&self, location: &Location) -> bool {
        let default = match location.scheme {
            Scheme::Https => 443,
            Scheme::Http => 80,
            Scheme::Ssh | Scheme::Scp => return false,
        };
        let (host, port) = self.web_parts();
        location.host == host && location.port.unwrap_or(default) == port.unwrap_or(default)
    }

    /// Whether a remote is this host's: a web one as [`Self::serves_web`] says, an SSH one on the
    /// web host or one of the SSH hosts, whatever its port.
    pub fn serves(&self, location: &Location) -> bool {
        match location.scheme {
            Scheme::Https | Scheme::Http => self.serves_web(location),
            Scheme::Ssh | Scheme::Scp => {
                location.host == self.web_parts().0 || self.ssh.contains(&location.host)
            }
        }
    }

    /// The repository a git remote names on this host: HTTPS, `ssh://` with or without a port,
    /// scp's `git@host:path`, a trailing `.git` or `/` dropped. Exactly `owner/name`, or two or
    /// more segments with `nested` groups; none for another host's remote, a path cut short or
    /// one too deep. A user or password in it is dropped.
    pub fn recognize(&self, remote: &str, nested: bool) -> Option<Repo> {
        let location = parse(remote)?;
        if !self.serves(&location) {
            return None;
        }
        let mut segments = location.segments;
        let last = segments.pop()?;
        let name = last.strip_suffix(".git").unwrap_or(last);
        if !is_segment(name) || segments.is_empty() || (!nested && segments.len() != 1) {
            return None;
        }
        segments.push(name);
        Some(Repo {
            host: self.web.clone(),
            path: segments.join("/"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_remotes_apart() {
        let https = parse("https://user:pw@Forge.Example:8443/a/b.git?x=1#y").unwrap();
        assert_eq!(https.scheme, Scheme::Https);
        assert_eq!(https.host, "forge.example");
        assert_eq!(https.port, Some(8443));
        assert_eq!(https.segments, ["a", "b.git"]);
        let scp = parse("git@forge.example:a/b.git").unwrap();
        assert_eq!(
            (scp.scheme, scp.host.as_str(), scp.segments),
            (Scheme::Scp, "forge.example", vec!["a", "b.git"])
        );
        let ssh = parse("ssh://git@forge.example:2222/a/b").unwrap();
        assert_eq!((ssh.scheme, ssh.port), (Scheme::Ssh, Some(2222)));
    }

    #[test]
    fn refuses_what_isnt_a_remote() {
        for bad in [
            "",
            "file:///srv/a/b",
            "./a:b",
            "https://forge.example:99999/a/b",
            "https://forge.example:0/a/b",
            "https://forge.example/a//b",
            "https://forge.example/a/../b",
            "https://forge.example/a/b\n/c",
            "https://f\u{043e}rge.example/a/b",
            "https://-forge.example/a/b",
            "git@forge.example:22:a/b",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
        assert_eq!(
            parse(&format!(
                "https://forge.example/{}",
                "a".repeat(MAX_URL_BYTES)
            )),
            None
        );
        assert_eq!(
            parse(&format!(
                "https://forge.example/{}",
                "a/".repeat(MAX_SEGMENTS + 1)
            )),
            None
        );
    }

    #[test]
    fn numbers_and_commits() {
        assert_eq!(parse_number("42"), Some(42));
        for bad in ["0", "", "-1", "+1", "1e3", "99999999999999999999999"] {
            assert_eq!(parse_number(bad), None, "{bad}");
        }
        assert!(is_commit_id("abc1234"));
        assert!(!is_commit_id("abc123"));
        assert!(!is_commit_id("xyz12345"));
    }
}
