//! A job's agent's only way off the Mac (#616; docs/runner-api.md, "obid only: the egress
//! proxy"): an HTTP `CONNECT` proxy obid runs on loopback. The agent gets it as `HTTPS_PROXY` and
//! `HTTP_PROXY` ([`env`]), with a token of its job's own, and its Seatbelt profile lets it reach
//! nothing but loopback ([`crate::sandbox::Spec::proxied`]), so whatever leaves the Mac goes
//! through here, named by its host. Its tool's own services go through ([`tools_own`]); any other
//! host is the job's to decide (`Jobs::ask_about`): the repository's `WebFetch(domain:…)` rules,
//! its mode, and else the owner's card. A tunnel carries only TLS whose hello names the host it
//! was opened for ([`hello_fits`]): a CDN's edge serves whichever of its sites the hello names.
//! A plain `http://` request is refused.

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::agent::{Answer, Tool};
use crate::jobs::Jobs;

/// obid's egress proxy: where it listens, the jobs it tunnels for, and how it reaches out.
pub struct Egress {
    port: u16,
    /// Each open job's door, by its token: a worker's own, gone with it.
    doors: Mutex<HashMap<String, Door>>,
    /// Room for [`MOST_TUNNELS`] at once, every job's together, taken once a request's token
    /// checks out; and for [`MOST_UNKNOWN`] whose head and token aren't read yet.
    room: Arc<tokio::sync::Semaphore>,
    unknown: Arc<tokio::sync::Semaphore>,
    /// The proxy obid itself is behind, as last read ([`Upstream::now`]).
    upstream: Mutex<(Option<Instant>, Option<Upstream>)>,
}

/// One job worker's way out: whose it is, its tunnels open now, and what it may reach.
struct Door {
    job: String,
    open: usize,
    /// By site, once decided, and held while it's being decided.
    sites: HashMap<String, Reached>,
}

/// A site's answer for a job: none until decided.
type Reached = Arc<tokio::sync::Mutex<Option<Known>>>;

/// A site's answer, and whether it's the owner's own (which a new mode or rule doesn't undo).
#[derive(Clone)]
struct Known {
    result: Result<(), String>,
    by_owner: bool,
}

/// How a site that isn't decided yet is: answered now, by a card the owner answers, or not now.
pub enum Asked {
    Now(Result<(), String>),
    /// Its answer, and, told before it, whether it's the owner's own.
    Card {
        answer: tokio::sync::oneshot::Receiver<Answer>,
        by_owner: tokio::sync::oneshot::Receiver<()>,
    },
    /// Refused this once (too many sites wait for the owner), and decided another time.
    Busy(String),
}

/// The most tunnels open at once, every job's together, and one job's; and the most connections
/// whose request isn't read yet, which have [`HEAD_WAIT`] to send it.
const MOST_TUNNELS: usize = 1024;
const MOST_PER_JOB: usize = 64;
const MOST_UNKNOWN: usize = 128;
const HEAD_WAIT: Duration = Duration::from_secs(5);
/// A tunnel nothing went through for this long ends.
const IDLE: Duration = Duration::from_secs(10 * 60);
/// The most a connection waits for the owner's answer about its site (the card keeps waiting).
const CARD_WAIT: Duration = Duration::from_secs(5 * 60);
/// The most a request's head, or its TLS hello, may be before it's refused, and how long it may
/// take to come.
const HEAD_CAP: usize = 16 * 1024;
const HELLO_CAP: usize = 64 * 1024;
const HELLO_WAIT: Duration = Duration::from_secs(15);

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
    /// Starts the proxy on `127.0.0.1`, on a port of its own, deciding for `jobs`, on the Tokio
    /// runtime this is called in.
    pub fn start(jobs: &Arc<Jobs>) -> std::io::Result<Arc<Egress>> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(listener)?;
        let egress = Arc::new(Egress {
            port: listener.local_addr()?.port(),
            doors: Mutex::new(HashMap::new()),
            room: Arc::new(tokio::sync::Semaphore::new(MOST_TUNNELS)),
            unknown: Arc::new(tokio::sync::Semaphore::new(MOST_UNKNOWN)),
            upstream: Mutex::new((None, None)),
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
                // Full: the connection closes at once, holding nothing.
                let Ok(unknown) = egress.unknown.clone().try_acquire_owned() else {
                    continue;
                };
                let jobs = jobs.clone();
                tokio::spawn(async move { egress.serve(client, jobs, unknown).await });
            }
        });
        Ok(egress)
    }

    fn doors(&self) -> std::sync::MutexGuard<'_, HashMap<String, Door>> {
        self.doors.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The proxy job `id`'s agent is given, with the job's token in it: made the first time it's
    /// asked for and learned as a secret, so whatever repeats it is masked everywhere; it lets
    /// nothing through once [`Egress::close`] ran, and what was decided for it goes with it.
    pub fn open(&self, id: &str) -> String {
        let mut doors = self.doors();
        let token = doors
            .iter()
            .find_map(|(token, door)| (door.job == id).then(|| token.clone()))
            .unwrap_or_else(|| {
                let token = format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                );
                doors.insert(
                    token.clone(),
                    Door {
                        job: id.to_owned(),
                        open: 0,
                        sites: HashMap::new(),
                    },
                );
                crate::secrets::known().learn(&format!("job {id} proxy token"), &token);
                token
            });
        format!("http://oby:{token}@127.0.0.1:{}", self.port)
    }

    /// Job `id`'s worker ended: its token lets nothing through any more.
    pub fn close(&self, id: &str) {
        self.doors().retain(|_, door| door.job != id);
        crate::secrets::known().forget(&format!("job {id} proxy token"));
    }

    /// The sites job `id`'s agent reaches without asking again, until its worker ends: the owner
    /// let a call naming them through. One being decided now is left to that.
    pub fn let_through(&self, id: &str, sites: &[String]) {
        for door in self.doors().values_mut().filter(|door| door.job == id) {
            for site in sites {
                if let Ok(mut known) = door.sites.entry(site.clone()).or_default().try_lock()
                    && known.is_none()
                {
                    *known = Some(Known {
                        result: Ok(()),
                        by_owner: true,
                    });
                }
            }
        }
    }

    /// The mode or the rules changed (job `id`'s, or every job's): what they decided is decided
    /// again, the owner's own answers kept.
    pub fn forget(&self, id: Option<&str>) {
        for door in self
            .doors()
            .values_mut()
            .filter(|door| id.is_none_or(|id| door.job == id))
        {
            door.sites.retain(|_, known| {
                known
                    .try_lock()
                    .map_or(true, |known| known.as_ref().is_some_and(|k| k.by_owner))
            });
        }
    }

    /// The token and job the request's `Proxy-Authorization` carries (Basic, any user name).
    fn job_of(&self, head: &str) -> Option<(String, String)> {
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
        let job = self.doors().get(token)?.job.clone();
        Some((token.to_owned(), job))
    }

    /// One more tunnel of the door `token`'s, unless it has [`MOST_PER_JOB`] open.
    fn opened(&self, token: &str) -> Option<Opened<'_>> {
        let mut doors = self.doors();
        let door = doors.get_mut(token)?;
        if door.open >= MOST_PER_JOB {
            return None;
        }
        door.open += 1;
        Some(Opened {
            egress: self,
            token: token.to_owned(),
        })
    }

    /// Whether the job of door `token` may reach `host` on `port`: decided once for its worker,
    /// connections to a site being decided waiting for that one answer, at most [`CARD_WAIT`].
    async fn decide(
        &self,
        token: &str,
        job: &str,
        host: &str,
        port: u16,
        jobs: &Jobs,
    ) -> Result<(), String> {
        // A deny rule holds whatever was decided before it was saved.
        if let Some(why) = jobs.denied_site(job, host, port) {
            return Err(why);
        }
        let site = site(host, port);
        let waiting = || {
            Err(format!(
                "obid is waiting for the owner's answer about {site}: try again once they answer."
            ))
        };
        let gate = {
            let mut doors = self.doors();
            let door = doors.get_mut(token).ok_or("The job ended.")?;
            door.sites.entry(site.clone()).or_default().clone()
        };
        let Ok(mut known) = tokio::time::timeout(CARD_WAIT, gate.lock_owned()).await else {
            return waiting();
        };
        if let Some(known) = &*known {
            return known.result.clone();
        }
        match jobs.ask_about(job, host, port) {
            Asked::Now(result) => {
                *known = Some(Known {
                    result: result.clone(),
                    by_owner: false,
                });
                result
            }
            Asked::Busy(why) => Err(why),
            Asked::Card {
                answer,
                mut by_owner,
            } => {
                // The answer is kept whenever it comes, even after this connection gave up; a
                // change of mode or rules that settled it is decided again with the next.
                let deciding = tokio::spawn(async move {
                    let result = match answer.await {
                        Ok(Answer::Allow | Answer::AllowWith(_)) => Ok(()),
                        Ok(Answer::Deny(why)) => Err(why),
                        Err(_) => Err("Nobody answered.".to_owned()),
                    };
                    let by_owner = by_owner.try_recv().is_ok();
                    *known = Some(Known {
                        result: result.clone(),
                        by_owner,
                    });
                    result
                });
                match tokio::time::timeout(CARD_WAIT, deciding).await {
                    Ok(Ok(result)) => result,
                    _ => waiting(),
                }
            }
        }
    }

    async fn serve(
        &self,
        mut client: TcpStream,
        jobs: Weak<Jobs>,
        unknown: tokio::sync::OwnedSemaphorePermit,
    ) {
        let Some((head, early)) = read_head(&mut client, HEAD_WAIT).await else {
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
        let Some((token, job)) = self.job_of(&head) else {
            let _ = client
                .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"oby\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
            return;
        };
        let Some(_opened) = self.opened(&token) else {
            refuse(
                &mut client,
                "429 Too Many Requests",
                &format!("This job has {MOST_PER_JOB} connections open through obid's proxy."),
            )
            .await;
            return;
        };
        // A job's own request now: it takes room of every job's, never before.
        let Ok(_room) = self.room.clone().try_acquire_owned() else {
            refuse(
                &mut client,
                "503 Service Unavailable",
                "obid's proxy has every connection it takes open: try again.",
            )
            .await;
            return;
        };
        drop(unknown);
        let Some(jobs) = jobs.upgrade() else { return };
        let allowed = match jobs.reach_now(&job, &host, port) {
            Some(now) => now,
            None => self.decide(&token, &job, &host, port, &jobs).await,
        };
        if let Err(why) = allowed {
            tracing::debug!("egress: job {job} refused {host}:{port}");
            refuse(&mut client, "403 Forbidden", &why).await;
            return;
        }
        // ponytail: no address filter once the name resolves; the macOS box reaches loopback
        // anyway. Linux's box (#616's next step) needs one against 10.0.2.2's host loopback.
        let Some((mut upstream, before)) = self.connect(&host, port).await else {
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
            || client.write_all(&before).await.is_err()
        {
            return;
        }
        // Nothing goes on but TLS whose hello names this very host.
        let Some(hello) = read_hello(&mut client, early, &host).await else {
            tracing::debug!("egress: job {job}'s tunnel to {host}:{port} didn't name it");
            return;
        };
        if upstream.write_all(&hello).await.is_err() {
            return;
        }
        tracing::debug!("egress: job {job} reaches {host}:{port}");
        splice(client, upstream).await;
    }

    /// A connection to `host:port`: straight, or through the proxy obid itself is behind; and
    /// what that proxy sent past its answer, for the client.
    async fn connect(&self, host: &str, port: u16) -> Option<(TcpStream, Vec<u8>)> {
        let loopback = host == "localhost" || host.starts_with("127.") || host == "[::1]";
        let upstream = match loopback {
            true => None,
            false => self.upstream().await.filter(|up| !up.skips(host)),
        };
        let name = host.trim_start_matches('[').trim_end_matches(']');
        let connect = async {
            let Some(up) = upstream else {
                return TcpStream::connect((name, port))
                    .await
                    .ok()
                    .map(|s| (s, Vec::new()));
            };
            let mut stream = TcpStream::connect((up.host.as_str(), up.port)).await.ok()?;
            let login = up
                .login
                .as_ref()
                .map(|l| format!("Proxy-Authorization: Basic {l}\r\n"))
                .unwrap_or_default();
            stream
                .write_all(
                    format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n{login}\r\n")
                        .as_bytes(),
                )
                .await
                .ok()?;
            let (answer, before) = read_head(&mut stream, Duration::from_secs(15)).await?;
            (answer.split(' ').nth(1) == Some("200")).then_some((stream, before))
        };
        tokio::time::timeout(Duration::from_secs(15), connect)
            .await
            .ok()
            .flatten()
    }

    /// The proxy obid itself is behind: its own `HTTPS_PROXY`, else, on macOS, the system's
    /// https proxy (`scutil --proxy`), read again after a minute (the Mac changes networks).
    async fn upstream(&self) -> Option<Upstream> {
        {
            let kept = self.upstream.lock().unwrap_or_else(|e| e.into_inner());
            if kept
                .0
                .is_some_and(|at| at.elapsed() < Duration::from_secs(60))
            {
                return kept.1.clone();
            }
        }
        let now = Upstream::now().await;
        *self.upstream.lock().unwrap_or_else(|e| e.into_inner()) =
            (Some(Instant::now()), now.clone());
        now
    }
}

/// A tunnel of a door's, counted while it's open.
struct Opened<'a> {
    egress: &'a Egress,
    token: String,
}

impl Drop for Opened<'_> {
    fn drop(&mut self) {
        if let Some(door) = self.egress.doors().get_mut(&self.token) {
            door.open -= 1;
        }
    }
}

/// A proxy obid is behind: where, its login (Basic, encoded), and the hosts it's skipped for.
#[derive(Clone, Debug, PartialEq)]
struct Upstream {
    host: String,
    port: u16,
    login: Option<String>,
    skip: Vec<String>,
}

impl Upstream {
    async fn now() -> Option<Upstream> {
        let var = |names: [&str; 2]| names.iter().find_map(|n| std::env::var(n).ok());
        if let Some(url) = var(["HTTPS_PROXY", "https_proxy"]).filter(|u| !u.is_empty()) {
            let skip = var(["NO_PROXY", "no_proxy"]).unwrap_or_default();
            return Upstream::from_url(&url, skip.split(','));
        }
        if !cfg!(target_os = "macos") {
            return None;
        }
        let said = tokio::process::Command::new("/usr/sbin/scutil")
            .arg("--proxy")
            .output()
            .await
            .ok()?;
        Upstream::from_scutil(&String::from_utf8_lossy(&said.stdout))
    }

    /// `[http://][user:password@]host[:port]`, the login percent-encoded. Another scheme
    /// (`https://`, `socks5://`) isn't one the proxy speaks: it's said once, and the proxy
    /// connects straight, as before.
    fn from_url<'a>(url: &str, skip: impl Iterator<Item = &'a str>) -> Option<Upstream> {
        let rest = match url.split_once("://") {
            None => url,
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => rest,
            Some((scheme, _)) => {
                static SAID: std::sync::Once = std::sync::Once::new();
                SAID.call_once(|| {
                    tracing::warn!(
                        "egress: obid's own proxy is a {scheme}:// one, which the egress proxy can't go through: it connects straight"
                    )
                });
                return None;
            }
        };
        let rest = rest.split('/').next().unwrap_or(rest);
        let (login, at) = match rest.rsplit_once('@') {
            Some((login, at)) => {
                let (user, password) = login.split_once(':').unwrap_or((login, ""));
                let login = format!("{}:{}", unescaped(user), unescaped(password));
                (Some(base64::engine::general_purpose::STANDARD.encode(login)), at)
            }
            None => (None, rest),
        };
        let (host, port) = at.rsplit_once(':').unwrap_or((at, "80"));
        Some(Upstream {
            host: host.to_owned(),
            port: port.parse().ok()?,
            login,
            skip: skip.filter_map(skipped).collect(),
        })
    }

    /// What `scutil --proxy` says: `HTTPSEnable : 1`, `HTTPSProxy : …`, `HTTPSPort : …`, and the
    /// `ExceptionsList`'s entries.
    fn from_scutil(said: &str) -> Option<Upstream> {
        let mut values = HashMap::new();
        let mut skip = Vec::new();
        let mut in_exceptions = false;
        for line in said.lines().map(str::trim) {
            if line.starts_with("ExceptionsList") {
                in_exceptions = true;
            } else if line == "}" {
                in_exceptions = false;
            } else if let Some((key, value)) = line.split_once(" : ") {
                if in_exceptions {
                    skip.extend(skipped(value));
                } else {
                    values.insert(key, value);
                }
            }
        }
        if values.get("HTTPSEnable") != Some(&"1") {
            return None;
        }
        Some(Upstream {
            host: (*values.get("HTTPSProxy")?).to_owned(),
            port: values.get("HTTPSPort")?.parse().ok()?,
            login: None,
            skip,
        })
    }

    /// Whether `host` goes straight, past this proxy.
    fn skips(&self, host: &str) -> bool {
        self.skip
            .iter()
            .any(|domain| domain == "*" || within(host, domain))
    }
}

/// A `NO_PROXY` or exceptions entry as a domain (`*.corp` and `.corp` are `corp`; `*` is every
/// host); an address range isn't kept.
fn skipped(entry: &str) -> Option<String> {
    let entry = entry.trim();
    if entry == "*" {
        return Some("*".into());
    }
    let entry = entry.trim_start_matches('*').trim_start_matches('.');
    (!entry.is_empty() && !entry.contains('/')).then(|| entry.to_ascii_lowercase())
}

/// `text` with its `%XX` escapes decoded.
fn unescaped(text: &str) -> String {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let byte = text.as_bytes()[at];
        let escaped = (byte == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(decoded) => {
                out.push(decoded);
                at += 3;
            }
            None => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Both ways at once until both ends are done, or nothing went either way for [`IDLE`].
async fn splice(mut client: TcpStream, mut upstream: TcpStream) {
    enum Went {
        Up(std::io::Result<usize>),
        Down(std::io::Result<usize>),
    }
    let (mut client_read, mut client_write) = client.split();
    let (mut up_read, mut up_write) = upstream.split();
    let (mut up_open, mut down_open) = (true, true);
    let (mut up, mut down) = (vec![0u8; 16 * 1024], vec![0u8; 16 * 1024]);
    while up_open || down_open {
        let went = tokio::time::timeout(IDLE, async {
            tokio::select! {
                n = client_read.read(&mut up), if up_open => Went::Up(n),
                n = up_read.read(&mut down), if down_open => Went::Down(n),
            }
        })
        .await;
        let written = match went {
            Err(_) => return,
            Ok(Went::Up(Ok(n))) if n > 0 => {
                tokio::time::timeout(IDLE, up_write.write_all(&up[..n])).await
            }
            Ok(Went::Down(Ok(n))) if n > 0 => {
                tokio::time::timeout(IDLE, client_write.write_all(&down[..n])).await
            }
            Ok(Went::Up(_)) => {
                up_open = false;
                tokio::time::timeout(IDLE, up_write.shutdown()).await
            }
            Ok(Went::Down(_)) => {
                down_open = false;
                tokio::time::timeout(IDLE, client_write.shutdown()).await
            }
        };
        if !matches!(written, Ok(Ok(()))) {
            return;
        }
    }
}

/// The request's head as text, and whatever the client sent after it (a TLS hello that didn't
/// wait for the answer); none when it doesn't come within `wait` or [`HEAD_CAP`].
async fn read_head(client: &mut TcpStream, wait: Duration) -> Option<(String, Vec<u8>)> {
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
    let end = tokio::time::timeout(wait, read).await.ok()??;
    let head = String::from_utf8(buffer[..end].to_vec()).ok()?;
    Some((head, buffer[end + 4..].to_vec()))
}

/// The client's TLS hello, whole, when it names `host` ([`hello_fits`]): `early` and what comes
/// after it, within [`HELLO_WAIT`] and [`HELLO_CAP`]. None for anything else.
async fn read_hello(client: &mut TcpStream, early: Vec<u8>, host: &str) -> Option<Vec<u8>> {
    let mut bytes = early;
    let read = async {
        let mut chunk = [0u8; 4096];
        loop {
            match parse_hello(&bytes) {
                Hello::Whole(named) => return Some(named),
                Hello::NotTls => return None,
                Hello::Part if bytes.len() > HELLO_CAP => return None,
                Hello::Part => {}
            }
            let n = client.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            bytes.extend_from_slice(&chunk[..n]);
        }
    };
    let named = tokio::time::timeout(HELLO_WAIT, read).await.ok()??;
    hello_fits(&named, host).then_some(bytes)
}

/// What a client's first bytes in a tunnel are: a whole TLS ClientHello (the name it's for, and
/// whether it hides another inside, Encrypted Client Hello), the start of one, or not TLS.
#[derive(Debug, PartialEq)]
enum Hello {
    Whole(Named),
    Part,
    NotTls,
}

#[derive(Debug, PartialEq)]
struct Named {
    sni: Option<String>,
    ech: bool,
}

/// Reads TLS records (type 22, handshake) until the handshake message they carry is whole, and
/// that message as a ClientHello.
fn parse_hello(bytes: &[u8]) -> Hello {
    let mut handshake = Vec::new();
    let mut at = 0;
    loop {
        if bytes.len() < at + 5 {
            return if bytes.get(at).is_some_and(|&b| b != 22) {
                Hello::NotTls
            } else {
                Hello::Part
            };
        }
        if bytes[at] != 22 {
            return Hello::NotTls;
        }
        let len = usize::from(u16::from_be_bytes([bytes[at + 3], bytes[at + 4]]));
        if bytes.len() < at + 5 + len {
            return Hello::Part;
        }
        handshake.extend_from_slice(&bytes[at + 5..at + 5 + len]);
        at += 5 + len;
        if handshake.len() >= 4 {
            if handshake[0] != 1 {
                return Hello::NotTls;
            }
            let message = usize::from(handshake[1]) << 16
                | usize::from(handshake[2]) << 8
                | usize::from(handshake[3]);
            if handshake.len() >= 4 + message {
                return named(&handshake[4..4 + message]).map_or(Hello::NotTls, Hello::Whole);
            }
        }
    }
}

/// Bytes read in order, each read checked against what's left.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (taken, rest) = (self.0.get(..n)?, self.0.get(n..)?);
        self.0 = rest;
        Some(taken)
    }
    fn u8(&mut self) -> Option<usize> {
        self.take(1).map(|b| usize::from(b[0]))
    }
    fn u16(&mut self) -> Option<usize> {
        self.take(2)
            .map(|b| usize::from(u16::from_be_bytes([b[0], b[1]])))
    }
}

/// A ClientHello's server name (`server_name`, 0) and whether it carries
/// `encrypted_client_hello` (0xfe0d).
fn named(body: &[u8]) -> Option<Named> {
    let mut hello = Reader(body);
    hello.take(2 + 32)?;
    let session = hello.u8()?;
    hello.take(session)?;
    let suites = hello.u16()?;
    hello.take(suites)?;
    let compression = hello.u8()?;
    hello.take(compression)?;
    let mut named = Named {
        sni: None,
        ech: false,
    };
    if hello.0.is_empty() {
        return Some(named);
    }
    let length = hello.u16()?;
    let mut extensions = Reader(hello.take(length)?);
    // An extension twice, or two names, would let a server read another than obid did.
    let mut seen = std::collections::HashSet::new();
    while !extensions.0.is_empty() {
        let kind = extensions.u16()?;
        let length = extensions.u16()?;
        let data = extensions.take(length)?;
        if !seen.insert(kind) {
            return None;
        }
        match kind {
            0 => {
                let mut data = Reader(data);
                let length = data.u16()?;
                let mut list = Reader(data.take(length)?);
                while !list.0.is_empty() {
                    let kind = list.u8()?;
                    let length = list.u16()?;
                    let name = list.take(length)?;
                    if kind != 0 || named.sni.is_some() {
                        return None;
                    }
                    named.sni = Some(String::from_utf8(name.to_vec()).ok()?);
                }
            }
            0xfe0d => named.ech = true,
            _ => {}
        }
    }
    Some(named)
}

/// Whether a hello may go to `host`: it names that host (case and a final dot aside), or none
/// for an address; never one that hides another name inside (Encrypted Client Hello).
fn hello_fits(named: &Named, host: &str) -> bool {
    let address = host.starts_with('[') || host.parse::<std::net::Ipv4Addr>().is_ok();
    !named.ech
        && match &named.sni {
            None => address,
            Some(name) => !address && name.trim_end_matches('.').eq_ignore_ascii_case(host),
        }
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

/// Whether `host` on `port` is one of `tool`'s own services, which its agent reaches without
/// asking, on 443 only: its model's API and its login's (Claude Code's `api.anthropic.com`;
/// Codex's `api.openai.com` with an API key and `chatgpt.com` with a ChatGPT login; Cursor's
/// `api2.cursor.sh`), and the rest of the vendor's domains, which the CLIs call for settings and
/// feature flags.
pub fn tools_own(tool: Tool, host: &str, port: u16) -> bool {
    let domains: &[&str] = match tool {
        Tool::Claude => &["anthropic.com", "claude.ai", "claude.com"],
        Tool::Codex => &["openai.com", "chatgpt.com"],
        Tool::Cursor => &["cursor.sh", "cursor.com"],
    };
    port == 443 && domains.iter().any(|domain| within(host, domain))
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

/// The sites the `http(s)://` addresses in `words` name, each once.
pub fn sites_named(words: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for word in words {
        let lower = word.to_ascii_lowercase();
        for (scheme, default) in [("https://", 443), ("http://", 80)] {
            for (at, _) in lower.match_indices(scheme) {
                let rest = &lower[at + scheme.len()..];
                let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
                let authority = authority.rsplit('@').next().unwrap_or("");
                let (host, port) = match authority.rsplit_once(':') {
                    Some((host, port)) if !port.contains(']') => (host, port.parse().ok()),
                    _ => (authority, Some(default)),
                };
                let host = host.trim_end_matches('.');
                if let Some(port) = port
                    && valid_host(host)
                {
                    let site = site(host, port);
                    if !out.contains(&site) {
                        out.push(site);
                    }
                }
            }
        }
    }
    out
}

/// A TLS ClientHello record naming `sni` (none: no `server_name`), with
/// `encrypted_client_hello` when `ech`, as a test's client sends it.
#[cfg(test)]
pub(crate) fn client_hello(sni: Option<&str>, ech: bool) -> Vec<u8> {
    let mut extensions = Vec::new();
    if let Some(sni) = sni {
        let name = sni.as_bytes();
        let list = [&[0u8][..], &(name.len() as u16).to_be_bytes(), name].concat();
        let data = [&(list.len() as u16).to_be_bytes()[..], &list].concat();
        extensions.extend([0u8, 0]);
        extensions.extend((data.len() as u16).to_be_bytes());
        extensions.extend(data);
    }
    if ech {
        extensions.extend([0xfe, 0x0d, 0, 1, 0]);
    }
    hello_with(&extensions)
}

/// A TLS ClientHello record with these extensions, as they're written.
#[cfg(test)]
fn hello_with(extensions: &[u8]) -> Vec<u8> {
    let mut body = vec![3, 3];
    body.extend([7u8; 32]);
    body.extend([0, 0, 2, 0x13, 0x01, 1, 0]);
    body.extend((extensions.len() as u16).to_be_bytes());
    body.extend(extensions);
    let length = body.len() as u32;
    let handshake = [&[1u8][..], &length.to_be_bytes()[1..], &body].concat();
    [
        &[22u8, 3, 1][..],
        &(handshake.len() as u16).to_be_bytes(),
        &handshake,
    ]
    .concat()
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
    fn a_tools_own_services_are_its_vendors_domains_on_443() {
        assert!(tools_own(Tool::Claude, "api.anthropic.com", 443));
        assert!(tools_own(Tool::Codex, "chatgpt.com", 443));
        assert!(tools_own(Tool::Cursor, "api2.cursor.sh", 443));
        assert!(!tools_own(Tool::Claude, "api.anthropic.com", 22));
        assert!(!tools_own(Tool::Claude, "chatgpt.com", 443));
        assert!(!tools_own(Tool::Claude, "evilanthropic.com", 443));
        assert!(!tools_own(Tool::Claude, "anthropic.com.evil.example", 443));
        assert!(telemetry("http-intake.logs.us5.datadoghq.com"));
    }

    #[test]
    fn addresses_name_their_sites() {
        let words: Vec<String> = [
            "-s",
            "https://u:p@Example.com:8443/x",
            "http://plain.example/",
            "--url=https://example.com/",
            "ftp://no.example",
        ]
        .map(String::from)
        .into();
        assert_eq!(
            sites_named(&words),
            ["example.com:8443", "plain.example:80", "example.com"]
        );
    }

    /// #1471's review: only TLS goes through a tunnel, and only a hello naming the very host the
    /// tunnel was opened for (a CDN serves whichever of its sites the hello names), never one
    /// that hides another inside; an address takes a hello with no name.
    #[test]
    fn a_tunnel_takes_only_a_hello_naming_its_host() {
        let whole = |hello: Vec<u8>| match parse_hello(&hello) {
            Hello::Whole(named) => named,
            other => panic!("{other:?}"),
        };
        let named = whole(client_hello(Some("chatgpt.com"), false));
        assert!(hello_fits(&named, "chatgpt.com"));
        assert!(hello_fits(
            &whole(client_hello(Some("ChatGPT.com."), false)),
            "chatgpt.com"
        ));
        assert!(!hello_fits(
            &whole(client_hello(Some("attacker.workers.dev"), false)),
            "chatgpt.com"
        ));
        assert!(!hello_fits(
            &whole(client_hello(Some("chatgpt.com"), true)),
            "chatgpt.com"
        ));
        assert!(!hello_fits(
            &whole(client_hello(None, false)),
            "chatgpt.com"
        ));
        assert!(hello_fits(&whole(client_hello(None, false)), "1.2.3.4"));
        assert!(!hello_fits(
            &whole(client_hello(Some("evil.example"), false)),
            "1.2.3.4"
        ));
        // Not TLS, and a hello cut short.
        assert_eq!(parse_hello(b"GET / HTTP/1.1\r\n\r\n"), Hello::NotTls);
        let hello = client_hello(Some("chatgpt.com"), false);
        assert_eq!(parse_hello(&hello[..hello.len() - 1]), Hello::Part);
        assert_eq!(parse_hello(&hello[..3]), Hello::Part);
        // One handshake message over two records is read whole.
        let handshake = &hello[5..];
        let (a, b) = handshake.split_at(10);
        let mut split = vec![22, 3, 1];
        split.extend((a.len() as u16).to_be_bytes());
        split.extend(a);
        split.extend([22, 3, 1]);
        split.extend((b.len() as u16).to_be_bytes());
        split.extend(b);
        assert_eq!(parse_hello(&split), Hello::Whole(named));
        // Two names, or an extension twice: a server could read another name than obid did.
        let name = |n: &str| [&[0u8][..], &(n.len() as u16).to_be_bytes(), n.as_bytes()].concat();
        let server_name = |list: Vec<u8>| {
            let data = [&(list.len() as u16).to_be_bytes()[..], &list].concat();
            [&[0u8, 0][..], &(data.len() as u16).to_be_bytes(), &data].concat()
        };
        let two = server_name([name("chatgpt.com"), name("evil.example")].concat());
        assert_eq!(parse_hello(&hello_with(&two)), Hello::NotTls);
        let twice = [
            server_name(name("chatgpt.com")),
            server_name(name("evil.example")),
        ]
        .concat();
        assert_eq!(parse_hello(&hello_with(&twice)), Hello::NotTls);
    }

    #[test]
    fn the_proxy_obid_is_behind_is_read() {
        let up = Upstream::from_url(
            "http://me:pw@proxy.corp:3128/",
            ["*.corp", "10.0.0.0/8", ""].into_iter(),
        )
        .unwrap();
        assert_eq!((up.host.as_str(), up.port), ("proxy.corp", 3128));
        assert_eq!(up.login.as_deref(), Some("bWU6cHc="));
        assert!(up.skips("git.corp") && !up.skips("example.com"));
        let said = "<dictionary> {\n  ExceptionsList : <array> {\n    0 : *.local\n    1 : 169.254/16\n  }\n  HTTPSEnable : 1\n  HTTPSPort : 8080\n  HTTPSProxy : proxy.example\n}\n";
        let up = Upstream::from_scutil(said).unwrap();
        assert_eq!(
            (up.host.as_str(), up.port, up.skip.clone()),
            ("proxy.example", 8080, vec!["local".to_owned()])
        );
        assert_eq!(
            Upstream::from_scutil("<dictionary> {\n  HTTPSEnable : 0\n}\n"),
            None
        );
        // #1471's review: another scheme isn't followed (straight, as before); a login is
        // percent-decoded; `NO_PROXY=*` skips it for every host.
        for other in ["https://proxy.corp:443", "socks5://proxy.corp:1080"] {
            assert_eq!(Upstream::from_url(other, std::iter::empty()), None, "{other}");
        }
        let up = Upstream::from_url("me%40corp:p%3Aw@proxy.corp:3128", std::iter::empty()).unwrap();
        assert_eq!(
            up.login,
            Some(base64::engine::general_purpose::STANDARD.encode("me@corp:p:w"))
        );
        let up = Upstream::from_url("http://proxy.corp:3128", ["*"].into_iter()).unwrap();
        assert!(up.skips("example.com"));
    }
}
