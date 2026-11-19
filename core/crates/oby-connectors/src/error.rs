//! What a connector's call answers when it doesn't succeed: typed, as T3 Code's
//! `PullRequestProviderError` reasons are, plus `Unsupported` for a capability the host lacks.

use std::fmt;

use crate::definition::Capability;

/// The longest detail kept: a CLI's whole output is never carried.
pub const MAX_DETAIL_BYTES: usize = 2000;

/// What a host said about a failure, made safe to show and log: a URL's user and password taken
/// out, control characters made spaces, cut at [`MAX_DETAIL_BYTES`]. A token the adapter holds is
/// taken out with [`Detail::redact`]; one the host echoes bare can't be found any other way.
#[derive(Clone, PartialEq, Eq)]
pub struct Detail(String);

impl Detail {
    pub fn new(text: &str) -> Self {
        let mut text = strip_userinfo(text);
        text = text
            .chars()
            .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
            .collect();
        if text.len() > MAX_DETAIL_BYTES {
            let mut end = MAX_DETAIL_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push('…');
        }
        Detail(text)
    }

    /// The same, with every copy of `secret` replaced; a secret under 4 bytes is left alone (it
    /// would blank ordinary words).
    #[must_use]
    pub fn redact(self, secret: &str) -> Self {
        if secret.len() < 4 {
            return self;
        }
        Detail(self.0.replace(secret, "[redacted]"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Every `scheme://user:password@` in `text` without its `user:password@`.
fn strip_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| {
                c == '/' || c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | ']')
            })
            .unwrap_or(tail.len());
        let authority = &tail[..end];
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        out.push_str(host);
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Why a connector's call didn't succeed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorError {
    /// The host's definition doesn't declare `capability`, so the call was never made.
    Unsupported {
        host: &'static str,
        capability: Capability,
    },
    /// No tool to reach the host with (its CLI isn't installed).
    MissingTool(Detail),
    Unauthenticated(Detail),
    Forbidden(Detail),
    NotFound(Detail),
    RateLimited {
        retry_after_secs: Option<u64>,
        detail: Detail,
    },
    Failed(Detail),
}

impl ConnectorError {
    pub fn failed(text: &str) -> Self {
        ConnectorError::Failed(Detail::new(text))
    }
}

impl fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectorError::Unsupported { host, capability } => {
                write!(
                    f,
                    "{host} doesn't offer {}, so Oby didn't ask it.",
                    capability.what()
                )
            }
            ConnectorError::MissingTool(d) => {
                write!(f, "The tool to reach it isn't installed: {d}")
            }
            ConnectorError::Unauthenticated(d) => write!(f, "Not signed in: {d}"),
            ConnectorError::Forbidden(d) => write!(f, "Not allowed: {d}"),
            ConnectorError::NotFound(d) => write!(f, "Not found: {d}"),
            ConnectorError::RateLimited {
                retry_after_secs: Some(secs),
                detail,
            } => {
                write!(f, "Rate limited, for {secs} seconds: {detail}")
            }
            ConnectorError::RateLimited {
                retry_after_secs: None,
                detail,
            } => write!(f, "Rate limited: {detail}"),
            ConnectorError::Failed(d) => write!(f, "{d}"),
        }
    }
}

impl std::error::Error for ConnectorError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_detail_drops_credentials_in_urls() {
        let d =
            Detail::new("fatal: https://x-access-token:abcd1234@forge.example/a/b.git not found");
        assert_eq!(d.as_str(), "fatal: https://forge.example/a/b.git not found");
        let d = Detail::new("see ssh://git:pw@h/x and 'https://u@h'");
        assert_eq!(d.as_str(), "see ssh://h/x and 'https://h'");
    }

    #[test]
    fn a_detail_redacts_bounds_and_cleans() {
        let d = Detail::new("token abcd1234 refused\u{1b}[31m").redact("abcd1234");
        assert_eq!(d.as_str(), "token [redacted] refused [31m");
        assert_eq!(Detail::new("short").redact("ab").as_str(), "short");
        let long = Detail::new(&"é".repeat(MAX_DETAIL_BYTES));
        assert!(long.as_str().len() <= MAX_DETAIL_BYTES + '…'.len_utf8());
    }
}
