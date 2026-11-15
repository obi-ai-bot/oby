//! Where obid is: its address, only ever loopback, and its home, where `api-token` is.

use std::ffi::OsString;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;

pub const DEFAULT_URL: &str = "http://127.0.0.1:7777";
const HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

/// obid's loopback address, with the host as given, for the `Host` header obid checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: &'static str,
    pub port: u16,
}

impl Endpoint {
    /// Connected to directly, never looked up: `localhost` is 127.0.0.1 whatever a resolver says.
    pub fn addr(&self) -> SocketAddr {
        match self.host {
            "[::1]" => SocketAddr::from((Ipv6Addr::LOCALHOST, self.port)),
            _ => SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)),
        }
    }

    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// `http://<host>[:<port>]`, then at most a `/`, where the host is exactly 127.0.0.1, localhost or
/// [::1]: no https, user info, path, query or other host, so the token never leaves this machine.
pub fn loopback(url: &str) -> Option<Endpoint> {
    let rest = url.strip_prefix("http://")?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let (host, port) = match rest.strip_prefix("[::1]") {
        Some(after) => ("[::1]", after),
        None => rest.split_at(rest.find(':').unwrap_or(rest.len())),
    };
    let host = HOSTS.into_iter().find(|h| *h == host)?;
    let port = match port {
        "" => 80,
        port => {
            let digits = port.strip_prefix(':')?;
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            digits.parse::<u16>().ok().filter(|p| *p != 0)?
        }
    };
    Some(Endpoint { host, port })
}

/// `OBID_URL`, else obid's default address.
pub fn from_env() -> Result<Endpoint, String> {
    let url = set("OBID_URL").map_or(Some(DEFAULT_URL.to_string()), |u| u.into_string().ok());
    url.as_deref().and_then(loopback).ok_or_else(|| {
        "OBID_URL isn't obid's loopback address: oby sends the token only to \
         http://127.0.0.1, http://localhost or http://[::1], with a port and nothing after it"
            .to_string()
    })
}

/// `OBID_HOME`, else obid's default home on this system.
pub fn home() -> Result<PathBuf, String> {
    if let Some(home) = set("OBID_HOME") {
        return Ok(home.into());
    }
    let user = set("HOME").ok_or("HOME isn't set, so obid's home isn't known: set OBID_HOME")?;
    let user = PathBuf::from(user);
    Ok(if cfg!(target_os = "macos") {
        user.join("Library/Application Support/Oby/Runner")
    } else {
        user.join(".local/share/obid")
    })
}

fn set(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_loopback_host_is_taken_with_and_without_a_port() {
        for (url, host, port) in [
            ("http://127.0.0.1", "127.0.0.1", 80),
            ("http://127.0.0.1:7777", "127.0.0.1", 7777),
            ("http://127.0.0.1:7777/", "127.0.0.1", 7777),
            ("http://localhost", "localhost", 80),
            ("http://localhost:7777", "localhost", 7777),
            ("http://localhost/", "localhost", 80),
            ("http://[::1]", "[::1]", 80),
            ("http://[::1]:7777", "[::1]", 7777),
            ("http://[::1]:7777/", "[::1]", 7777),
        ] {
            assert_eq!(loopback(url), Some(Endpoint { host, port }), "{url}");
        }
        assert_eq!(
            loopback("http://[::1]:7777").unwrap().authority(),
            "[::1]:7777"
        );
        assert!(loopback("http://[::1]:7777")
            .unwrap()
            .addr()
            .ip()
            .is_loopback());
        assert!(loopback("http://localhost:1")
            .unwrap()
            .addr()
            .ip()
            .is_loopback());
    }

    #[test]
    fn anything_else_is_refused() {
        for url in [
            "http://127.0.0.2",
            "http://127.0.0.2:7777",
            "http://example.com",
            "http://example.com:7777",
            "http://localhost.example.com",
            "http://localhost.example.com:7777",
            "https://127.0.0.1",
            "https://127.0.0.1:7777",
            "http://user@127.0.0.1",
            "http://user:pass@127.0.0.1:7777",
            "http://127.0.0.1/v1",
            "http://127.0.0.1:7777/v1",
            "http://127.0.0.1:7777?x=1",
            "http://127.0.0.1:7777#x",
            "http://127.0.0.1:",
            "http://127.0.0.1:0",
            "http://127.0.0.1:65536",
            "http://127.0.0.1:+80",
            "http://[::1]x",
            "http://[::2]:7777",
            "http://LOCALHOST",
            "HTTP://127.0.0.1",
            "127.0.0.1:7777",
            "",
        ] {
            assert_eq!(loopback(url), None, "{url}");
        }
    }

    #[test]
    fn the_default_is_obids_own_address() {
        assert_eq!(
            loopback(DEFAULT_URL),
            Some(Endpoint {
                host: "127.0.0.1",
                port: 7777
            })
        );
    }
}
