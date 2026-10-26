//! A job's agent's only way off the Mac (#616; docs/runner-api.md, "The job sandbox"): an HTTP
//! `CONNECT` proxy obid runs on loopback. The agent gets it as `HTTPS_PROXY` and `HTTP_PROXY`
//! ([`env`]), with a token of its job's own, and its Seatbelt profile lets it reach nothing but
//! loopback ([`crate::sandbox::Spec::proxied`]), so whatever leaves the Mac goes through here, named
//! by its host. Its tool's own services go through ([`tools_own`]); any other host is the job's
//! to decide (`Jobs::may_reach`): the repository's `WebFetch(domain:…)` rules, its mode, and else
//! the owner's card. Only tunnels (https and anything else over `CONNECT`): a plain `http://`
//! request is refused.

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::agent::Tool;
use crate::jobs::Jobs;

/// obid's egress proxy: where it listens, and the jobs it tunnels for.
pub struct Egress {
    port: u16,
    /// Each open job's token, and the job.
    tokens: Mutex<HashMap<String, String>>,
}

/// The most a request's head may be before it's refused.
const HEAD_CAP: usize = 16 * 1024;

/// What an agent's host names may be: a name or an IPv4 address, or an IPv6 one in brackets.
fn valid_host(host: &str) -> bool {
    let bracketed = host.starts_with('[') && host.ends_with(']');
    !host.is_empty()
        && host.len() <= 253
        && host.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '.' || c == '-' || (bracketed && "[]:".contains(c))
        })
}

impl Egress {
    /// Starts the proxy on `127.0.0.1`, on a port of its own, deciding for `jobs`.
    pub async fn start(jobs: &Arc<Jobs>) -> std::io::Result<Arc<Egress>> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let egress = Arc::new(Egress {
            port: listener.local_addr()?.port(),
            tokens: Mutex::new(HashMap::new()),
        });
        let (weak, jobs) = (Arc::downgrade(&egress), Arc::downgrade(jobs));
        tokio::spawn(async move {
            loop {
                let client = match listener.accept().await {
                    Ok((client, _)) => client,
                    // Out of descriptors, say: the next accept may work.
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                };
                let Some(egress) = weak.upgrade() else { break };
                let jobs = jobs.clone();
                tokio::spawn(async move { egress.serve(client, jobs).await });
            }
        });
        Ok(egress)
    }

    /// The proxy job `id`'s agent is given, with the job's token in it: made the first time it's
    /// asked for and learned as a secret, so whatever repeats it is masked everywhere; it lets
    /// nothing through once [`Egress::close`] ran.
    pub fn open(&self, id: &str) -> String {
        let mut tokens = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        let token = tokens
            .iter()
            .find_map(|(token, job)| (job == id).then(|| token.clone()))
            .unwrap_or_else(|| {
                let token = format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                );
                tokens.insert(token.clone(), id.to_owned());
                crate::secrets::known().learn(&format!("job {id} proxy token"), &token);
                token
            });
        format!("http://oby:{token}@127.0.0.1:{}", self.port)
    }

    /// Job `id` ended: its token lets nothing through any more.
    pub fn close(&self, id: &str) {
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, job| job != id);
        crate::secrets::known().forget(&format!("job {id} proxy token"));
    }

    /// The job whose token the request's `Proxy-Authorization` carries (Basic, any user name).
    fn job_of(&self, head: &str) -> Option<String> {
        let value = head.lines().skip(1).find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("proxy-authorization")
                .then(|| value.trim())
        })?;
        let (scheme, encoded) = value.split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("basic") {
            return None;
        }
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .ok()?;
        let (_, token) = std::str::from_utf8(&decoded).ok()?.split_once(':')?;
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(token)
            .cloned()
    }

    async fn serve(&self, mut client: TcpStream, jobs: Weak<Jobs>) {
        let Some((head, early)) = read_head(&mut client).await else {
            return;
        };
        let Some((host, port)) = target(&head) else {
            refuse(
                &mut client,
                "405 Method Not Allowed",
                "obid's proxy only tunnels (CONNECT): use https.",
            )
            .await;
            return;
        };
        // No token, or a job's that ended: nobody is asked (a client that sends the proxy's login
        // only once challenged, like git, comes back with it).
        let Some(job) = self.job_of(&head) else {
            let _ = client
                .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"oby\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
            return;
        };
        let Some(jobs) = jobs.upgrade() else { return };
        if let Err(why) = jobs.may_reach(&job, &host, port).await {
            tracing::info!("egress: job {job} refused {host}:{port}");
            refuse(&mut client, "403 Forbidden", &why).await;
            return;
        }
        // ponytail: no address filter once the name resolves; the macOS box reaches loopback
        // anyway. Linux's box (#616's next step) needs one against 10.0.2.2's host loopback.
        tracing::debug!("egress: job {job} reaches {host}:{port}");
        let name = host.trim_start_matches('[').trim_end_matches(']');
        let connected =
            tokio::time::timeout(Duration::from_secs(15), TcpStream::connect((name, port))).await;
        let Ok(Ok(mut upstream)) = connected else {
            refuse(
                &mut client,
                "502 Bad Gateway",
                &format!("obid's proxy couldn't reach {host}:{port}."),
            )
            .await;
            return;
        };
        if client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .is_err()
            || upstream.write_all(&early).await.is_err()
        {
            return;
        }
        let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    }
}

/// The request's head as text, and whatever the client sent after it (a TLS hello that didn't
/// wait for the answer); none when it doesn't come within 30 seconds or [`HEAD_CAP`].
async fn read_head(client: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    let mut buffer = Vec::new();
    let read = async {
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                return Some(end);
            }
            if buffer.len() > HEAD_CAP {
                return None;
            }
            let n = client.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buffer.extend_from_slice(&chunk[..n]);
        }
    };
    let end = tokio::time::timeout(Duration::from_secs(30), read)
        .await
        .ok()??;
    let head = String::from_utf8(buffer[..end].to_vec()).ok()?;
    Some((head, buffer[end + 4..].to_vec()))
}

/// `CONNECT <host>:<port>`'s host (lowercase, without a final dot) and port.
fn target(head: &str) -> Option<(String, u16)> {
    let mut words = head.lines().next()?.split(' ');
    let (method, authority) = (words.next()?, words.next()?);
    if method != "CONNECT" || !words.next()?.starts_with("HTTP/1.") {
        return None;
    }
    let (host, port) = authority.rsplit_once(':')?;
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let port: u16 = port.parse().ok().filter(|p| *p != 0)?;
    valid_host(&host).then_some((host, port))
}

async fn refuse(client: &mut TcpStream, status: &str, why: &str) {
    let _ = client
        .write_all(
            format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{why}",
                why.len()
            )
            .as_bytes(),
        )
        .await;
}

/// What a proxied agent's environment adds: the proxy for https and http, in both spellings
/// (curl reads only the lowercase `http_proxy`), loopback left out of it (obid's MCP servers, a
/// test's own servers), and Node's built-in `fetch` and `https` told to use it too.
pub fn env(proxy: &str) -> Vec<(&'static str, OsString)> {
    let mut env: Vec<(&'static str, OsString)> =
        ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"]
            .into_iter()
            .map(|name| (name, proxy.into()))
            .collect();
    for name in ["NO_PROXY", "no_proxy"] {
        env.push((name, "localhost,127.0.0.1,::1".into()));
    }
    env.push(("NODE_USE_ENV_PROXY", "1".into()));
    env
}

/// What a host is shown as and kept as for a job: its name, and `:<port>` but for 443.
pub fn site(host: &str, port: u16) -> String {
    if port == 443 {
        host.to_owned()
    } else {
        format!("{host}:{port}")
    }
}

/// Whether `host` is `domain` or one of its subdomains.
fn within(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|rest| rest.ends_with('.'))
}

/// Whether `host` is one of `tool`'s own services, which its agent reaches without asking: its
/// model's API and its login's (Claude Code's `api.anthropic.com`; Codex's `api.openai.com` with
/// an API key and `chatgpt.com` with a ChatGPT login; Cursor's `api2.cursor.sh`), and the rest of
/// the vendor's domains, which the CLIs call for settings and feature flags.
pub fn tools_own(tool: Tool, host: &str) -> bool {
    let domains: &[&str] = match tool {
        Tool::Claude => &["anthropic.com", "claude.ai", "claude.com"],
        Tool::Codex => &["openai.com", "chatgpt.com"],
        Tool::Cursor => &["cursor.sh", "cursor.com"],
    };
    domains.iter().any(|domain| within(host, domain))
}

/// Telemetry a CLI sends elsewhere than its vendor (Claude Code's to Datadog, seen 2026-10-26):
/// refused without asking the owner about it.
pub fn telemetry(host: &str) -> bool {
    ["datadoghq.com", "sentry.io"]
        .iter()
        .any(|domain| within(host, domain))
}

/// The package registries a job in Sandbox mode reaches without asking, on 443 (the PR's
/// proposal, #616): what installs fetch from, exactly these hosts. Each also takes a publish (a
/// login the agent brought with it would work), which is what Sandbox mode accepts for them.
pub const REGISTRIES: &[&str] = &[
    "registry.npmjs.org",
    "registry.yarnpkg.com",
    "repo.yarnpkg.com",
    "pypi.org",
    "files.pythonhosted.org",
    "crates.io",
    "index.crates.io",
    "static.crates.io",
    "static.rust-lang.org",
    "proxy.golang.org",
    "sum.golang.org",
    "rubygems.org",
    "index.rubygems.org",
    "repo.maven.apache.org",
    "repo1.maven.org",
    "plugins.gradle.org",
    "services.gradle.org",
    "cdn.cocoapods.org",
];

/// The sites a call the owner (or a rule) allowed names, so the proxy doesn't ask again when the
/// agent goes there: a fetch's URL, and every `http(s)://` address in a command.
pub fn sites_named(call: &crate::permissions::Call) -> Vec<String> {
    let words = call.url.iter().map(String::as_str).chain(
        call.command
            .iter()
            .flat_map(|c| c.split(|ch: char| ch.is_whitespace() || "'\"`()<>".contains(ch))),
    );
    let mut out = Vec::new();
    for word in words {
        let Some((scheme, rest)) = word.split_once("://") else {
            continue;
        };
        let default = match scheme.to_ascii_lowercase().as_str() {
            "https" => 443,
            "http" => 80,
            _ => continue,
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        let authority = authority.rsplit('@').next().unwrap_or("");
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) if !port.contains(']') => (host, port.parse().ok()),
            _ => (authority, Some(default)),
        };
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if let Some(port) = port
            && valid_host(&host)
        {
            let site = site(&host, port);
            if !out.contains(&site) {
                out.push(site);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connect_names_its_host_and_port() {
        let head = "CONNECT API.Example.com.:443 HTTP/1.1\r\nHost: x";
        assert_eq!(target(head), Some(("api.example.com".into(), 443)));
        assert_eq!(
            target("CONNECT [::1]:8443 HTTP/1.1"),
            Some(("[::1]".into(), 8443))
        );
        for refused in [
            "GET http://example.com/ HTTP/1.1",
            "CONNECT example.com HTTP/1.1",
            "CONNECT example.com:0 HTTP/1.1",
            "CONNECT ex ample.com:443 HTTP/1.1",
            "CONNECT a/b:443 HTTP/1.1",
            "CONNECT example.com:443",
        ] {
            assert_eq!(target(refused), None, "{refused}");
        }
    }

    #[test]
    fn a_tools_own_services_are_its_vendors_domains() {
        assert!(tools_own(Tool::Claude, "api.anthropic.com"));
        assert!(tools_own(Tool::Codex, "chatgpt.com"));
        assert!(tools_own(Tool::Cursor, "api2.cursor.sh"));
        assert!(!tools_own(Tool::Claude, "chatgpt.com"));
        assert!(!tools_own(Tool::Claude, "evilanthropic.com"));
        assert!(!tools_own(Tool::Claude, "anthropic.com.evil.example"));
        assert!(telemetry("http-intake.logs.us5.datadoghq.com"));
    }

    #[test]
    fn an_allowed_call_names_its_sites() {
        let fetch = crate::permissions::Call {
            tool: "WebFetch".into(),
            url: Some("https://Docs.rs/serde".into()),
            ..Default::default()
        };
        assert_eq!(sites_named(&fetch), ["docs.rs"]);
        let curl = crate::permissions::Call {
            tool: "Bash".into(),
            command: Some(
                "curl -s 'https://u:p@example.com:8443/x' http://plain.example/ \"https://example.com/\" ftp://no.example".into(),
            ),
            ..Default::default()
        };
        assert_eq!(
            sites_named(&curl),
            ["example.com:8443", "plain.example:80", "example.com"]
        );
    }
}
