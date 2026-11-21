//! A stand-in OpenAI on loopback for the accounts area's ChatGPT workflows
//! (docs/contracts/chatgpt-sign-in.md, CHATGPT-n), as obid's own tests have one
//! (core/crates/obid/tests/support/stand_in_openai.rs): authorize, token, JWKS, the OpenID
//! configuration, revoke, `/v1/models` and `/v1/responses`, as OpenAI's Sign in with ChatGPT pages
//! describe them. No real sign-in and no real token: consent is this server's redirect, every token
//! is made here, and its two RSA keys are made by `openssl` at start, never kept. A scratch obid
//! reaches it only through `OBID_TEST_CHATGPT_ORIGIN`, which only a debug build reads.
//!
//! It lives in the workflow's nodes process, so it lasts the workflow and nothing else. Like the
//! area's other nodes it only acts or reads: what obid answers is asserted with `call`, `check` and
//! these nodes' own `expect`.

use crate::expect;
use anyhow::{bail, Context, Result};
use base64::Engine;
use ring::signature::RsaKeyPair;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;
const DIRECT: &str = "chatgpt.tokens.use.direct";

/// This file's nodes, listed in the area's own `NODES` (src/coverage_accounts.rs).
pub const NAMES: &[&str] = &[
    "accounts.openai",
    "accounts.openai-set",
    "accounts.openai-saw",
    "accounts.visit",
    "accounts.tokens",
];

pub async fn run_node(name: &str, input: &Value) -> Result<Value> {
    match name {
        "accounts.openai" => start().await,
        "accounts.openai-set" => set(input),
        "accounts.openai-saw" => {
            judged_within(input, || {
                let shared = of(input)?;
                let out = saw(&shared.script.lock().unwrap());
                Ok(out)
            })
            .await
        }
        "accounts.visit" => visit(input).await,
        "accounts.tokens" => judged_within(input, || tokens(input)).await,
        other => bail!("{other} isn't a stand-in OpenAI node"),
    }
}

/// What a `POST /v1/responses` answers next.
#[derive(Clone)]
enum Reply {
    Json(u16, Value),
    Sse(Vec<Value>),
    /// A stream the connection drops under after these events.
    Cut(Vec<Value>),
    Text(u16, String),
}

struct Grant {
    client: String,
    nonce: String,
    challenge: String,
}

#[derive(Default)]
struct Script {
    sub: String,
    email: String,
    scope: String,
    deny: bool,
    forged: bool,
    wrong_aud: bool,
    callback_client: Option<String>,
    hold: Duration,
    expires_in: i64,
    token_errors: VecDeque<(u16, Value)>,
    revoke_statuses: VecDeque<u16>,
    responses: VecDeque<Reply>,
    response_holds: VecDeque<Duration>,
    models: Value,
    authorized: Vec<HashMap<String, String>>,
    refreshes: Vec<String>,
    tokens: Vec<String>,
    revoked: Vec<String>,
    bearers: Vec<(String, String, String)>,
    refused: HashSet<String>,
    refuse_all: bool,
    issued_clients: u32,
    minted: u32,
    codes: HashMap<String, Grant>,
}

struct Shared {
    origin: String,
    script: Mutex<Script>,
    good: RsaKeyPair,
    other: RsaKeyPair,
}

static STAND_INS: Mutex<Option<HashMap<String, Arc<Shared>>>> = Mutex::new(None);

fn of(input: &Value) -> Result<Arc<Shared>> {
    let origin = input["openai"]["origin"]
        .as_str()
        .context("openai: an accounts.openai step's output")?;
    STAND_INS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(origin).cloned())
        .with_context(|| format!("no stand-in OpenAI at {origin} in this workflow"))
}

/// An RSA key of 2048 bits, made by `openssl` into a private folder and read back.
fn rsa_key() -> Result<RsaKeyPair> {
    let dir = std::env::temp_dir().join(format!("obid-spec-openai-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir)?;
    let file = dir.join("key.der");
    let made = std::process::Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .args(["-outform", "DER", "-out"])
        .arg(&file)
        .output()
        .context("openssl, to make the stand-in's key")?;
    let der = std::fs::read(&file);
    let _ = std::fs::remove_dir_all(&dir);
    anyhow::ensure!(
        made.status.success(),
        "openssl genpkey: {}",
        String::from_utf8_lossy(&made.stderr)
    );
    // PKCS#8 from some versions, PKCS#1 (OpenSSL 3.0's DER) from others.
    let der = der?;
    RsaKeyPair::from_pkcs8(&der)
        .or_else(|_| RsaKeyPair::from_der(&der))
        .map_err(|e| anyhow::anyhow!("the stand-in's key: {e}"))
}

async fn start() -> Result<Value> {
    let (good, other) =
        tokio::task::spawn_blocking(|| Ok::<_, anyhow::Error>((rsa_key()?, rsa_key()?))).await??;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let script = Script {
        sub: "stand-in-sub-1".into(),
        email: "person@example.com".into(),
        scope: format!("openid profile email offline_access resource.invoke {DIRECT}"),
        expires_in: 3600,
        models: json!({"models": [
            {"slug": "gpt-stand-in", "display_name": "Stand-in", "visibility": "list"},
            {"slug": "gpt-hidden", "display_name": "Hidden", "visibility": "hide"},
        ]}),
        ..Default::default()
    };
    let shared = Arc::new(Shared {
        origin: origin.clone(),
        script: Mutex::new(script),
        good,
        other,
    });
    STAND_INS
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(origin.clone(), shared.clone());
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let shared = shared.clone();
            tokio::spawn(async move {
                let _ = serve(stream, &shared).await;
            });
        }
    });
    Ok(json!({"origin": origin}))
}

fn reply_of(v: &Value) -> Result<Reply> {
    if let Some(words) = v["said"].as_str() {
        return Ok(Reply::Sse(said(words)));
    }
    if let Some(f) = v.get("failed") {
        return Ok(Reply::Sse(failed(
            f["code"].as_str().context("failed.code")?,
            f["message"].as_str().unwrap_or(""),
        )));
    }
    if let Some(events) = v["cut"].as_array() {
        return Ok(Reply::Cut(events.clone()));
    }
    let status = v["status"]
        .as_u64()
        .context("a reply: said, failed, cut or status")? as u16;
    if let Some(text) = v["text"].as_str() {
        return Ok(Reply::Text(status, text.to_string()));
    }
    Ok(Reply::Json(status, v["json"].clone()))
}

fn set(input: &Value) -> Result<Value> {
    let shared = of(input)?;
    let mut s = shared.script.lock().unwrap();
    let mut changed = vec![];
    let obj = input.as_object().context("inputs")?;
    for (key, v) in obj {
        match key.as_str() {
            "openai" => continue,
            "sub" => s.sub = v.as_str().context("sub")?.into(),
            "email" => s.email = v.as_str().context("email")?.into(),
            "scope" => s.scope = v.as_str().context("scope")?.replace("{direct}", DIRECT),
            "deny" => s.deny = v.as_bool().context("deny")?,
            "forged" => s.forged = v.as_bool().context("forged")?,
            "wrongAud" => s.wrong_aud = v.as_bool().context("wrongAud")?,
            "callbackClient" => s.callback_client = v.as_str().map(str::to_string),
            "holdMs" => s.hold = Duration::from_millis(v.as_u64().context("holdMs")?),
            "expiresIn" => s.expires_in = v.as_i64().context("expiresIn")?,
            "tokenErrors" => {
                for e in v.as_array().context("tokenErrors")? {
                    let status = e["status"].as_u64().context("tokenErrors[].status")? as u16;
                    s.token_errors.push_back((status, e["body"].clone()));
                }
            }
            "revokeStatuses" => {
                for st in v.as_array().context("revokeStatuses")? {
                    s.revoke_statuses
                        .push_back(st.as_u64().context("a status")? as u16);
                }
            }
            "responses" => {
                for r in v.as_array().context("responses")? {
                    s.responses.push_back(reply_of(r)?);
                }
            }
            "responseHoldsMs" => {
                for h in v.as_array().context("responseHoldsMs")? {
                    s.response_holds
                        .push_back(Duration::from_millis(h.as_u64().context("ms")?));
                }
            }
            "refuse" => {
                let access: Vec<String> = s
                    .tokens
                    .iter()
                    .filter(|t| t.contains("-access-"))
                    .cloned()
                    .collect();
                match v.as_str().context("refuse")? {
                    "first-access" => s
                        .refused
                        .insert(access.first().context("no access token yet")?.clone()),
                    "last-access" => s
                        .refused
                        .insert(access.last().context("no access token yet")?.clone()),
                    "all-access" => {
                        s.refused.extend(access);
                        true
                    }
                    "none" => {
                        s.refused.clear();
                        true
                    }
                    other => {
                        bail!("refuse: {other}? first-access, last-access, all-access or none")
                    }
                };
            }
            "refuseAll" => s.refuse_all = v.as_bool().context("refuseAll")?,
            "models" => s.models = v.clone(),
            other => bail!("accounts.openai-set doesn't know {other}"),
        }
        changed.push(key.clone());
    }
    Ok(json!({"set": changed}))
}

fn saw(s: &Script) -> Value {
    let access: Vec<&String> = s.tokens.iter().filter(|t| t.contains("-access-")).collect();
    let refresh: Vec<&String> = s
        .tokens
        .iter()
        .filter(|t| t.contains("-refresh-"))
        .collect();
    json!({
        "authorized": s.authorized,
        "refreshes": s.refreshes,
        "tokens": s.tokens,
        "access": access,
        "refresh": refresh,
        "firstAccess": access.first(),
        "lastAccess": access.last(),
        "lastRefresh": refresh.last(),
        "revoked": s.revoked,
        "bearers": s.bearers.iter().map(|(t, m, p)| json!({"token": t, "method": m, "path": p})).collect::<Vec<_>>(),
        "posts": s.bearers.iter().filter(|(_, m, _)| m == "POST").count(),
    })
}

/// Every way `out` falls short of `want` (`status` against `out.status`, `body`, `paths`, `text`).
fn failures(want: &Value, out: &Value) -> Vec<String> {
    let mut f: Vec<String> = want
        .get("status")
        .and_then(|s| expect::check_status(s, out["status"].as_u64().unwrap_or(0) as u16))
        .into_iter()
        .collect();
    f.extend(expect::check(want, out, out["text"].as_str().unwrap_or("")));
    f
}

/// The proof the step's assertion can fail (OBIDSPEC-6): each place it looks made wrong must fail it.
fn red(want: &Value, out: &Value) -> Value {
    let status = out["status"].as_u64().unwrap_or(0) as u16;
    let all = expect::mutants(want, status, out, out["text"].as_str().unwrap_or(""));
    let vacuous: Vec<String> = all
        .iter()
        .filter(|m| {
            let mut body = m.body.clone();
            if want.get("status").is_some() {
                body["status"] = json!(m.status);
            }
            if want.get("text").is_some() {
                body["text"] = json!(m.text);
            }
            failures(want, &body).is_empty()
        })
        .map(|m| m.what.clone())
        .collect();
    json!({"mutants": all.len(), "vacuous": vacuous})
}

/// `read` once, or again every 250 ms until it holds the step's `expect` or `within` seconds pass.
async fn judged_within(input: &Value, read: impl Fn() -> Result<Value>) -> Result<Value> {
    let until = Instant::now() + Duration::from_secs(input["within"].as_u64().unwrap_or(0));
    loop {
        let out = read()?;
        let Some(want) = input.get("expect") else {
            return Ok(out);
        };
        let f = failures(want, &out);
        if f.is_empty() {
            let mut out = out;
            out["red"] = red(want, &out);
            return Ok(out);
        }
        if Instant::now() >= until {
            bail!("{}", f.join("; "));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

type Visits = HashMap<String, tokio::task::JoinHandle<std::result::Result<Value, String>>>;
static VISITS: Mutex<Option<Visits>> = Mutex::new(None);

async fn visit(input: &Value) -> Result<Value> {
    let out = if let Some(handle) = input["wait"]["handle"].as_str() {
        let task = VISITS
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|v| v.remove(handle))
            .context("no background visit with that handle")?;
        let seconds = input["seconds"].as_u64().unwrap_or(60);
        tokio::time::timeout(Duration::from_secs(seconds), task)
            .await
            .with_context(|| format!("the visit answered nothing within {seconds} s"))??
            .map_err(|e| anyhow::anyhow!(e))?
    } else {
        let mut url = input["url"].as_str().context("url")?.to_string();
        if let Some([from, to]) = input["replace"].as_array().map(Vec::as_slice) {
            url = url.replace(
                from.as_str().context("replace")?,
                to.as_str().context("replace")?,
            );
        }
        if input["dropQuery"] == true {
            url = url.split('?').next().unwrap_or_default().to_string();
        }
        let host = input["host"].as_str().map(str::to_string);
        if input["background"] == true {
            let handle = uuid::Uuid::new_v4().to_string();
            let task = tokio::spawn(async move {
                get(&url, host.as_deref())
                    .await
                    .map_err(|e| format!("{e:#}"))
            });
            VISITS
                .lock()
                .unwrap()
                .get_or_insert_with(HashMap::new)
                .insert(handle.clone(), task);
            return Ok(json!({"handle": handle}));
        }
        get(&url, host.as_deref()).await?
    };
    if let Some(want) = input.get("expect") {
        let f = failures(want, &out);
        if !f.is_empty() {
            bail!(
                "{}: {}",
                f.join("; "),
                out["text"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(400)
                    .collect::<String>()
            );
        }
        let mut out = out;
        out["red"] = red(want, &out);
        return Ok(out);
    }
    Ok(out)
}

/// One GET over loopback, its `Host` header `host` when given, no redirect followed.
async fn get(url: &str, host: Option<&str>) -> Result<Value> {
    let rest = url
        .strip_prefix("http://127.0.0.1:")
        .context("only http://127.0.0.1 is visited")?;
    let (port, target) = rest
        .split_once('/')
        .map_or((rest, "/".to_string()), |(p, t)| (p, format!("/{t}")));
    let port: u16 = port.parse().context("a port")?;
    let mut stream = match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
        Ok(s) => s,
        // (nothing listens there: a sign-in's page that ended)
        Err(_) => return Ok(json!({"status": 0, "location": null, "text": ""})),
    };
    let authority = format!("127.0.0.1:{port}");
    let head = format!(
        "GET {target} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        host.unwrap_or(&authority)
    );
    stream.write_all(head.as_bytes()).await?;
    let mut raw = vec![];
    let _ = tokio::time::timeout(Duration::from_secs(60), stream.read_to_end(&mut raw)).await;
    if raw.is_empty() {
        return Ok(json!({"status": 0, "location": null, "text": ""}));
    }
    let answer = crate::http::parse(&raw)?;
    let location = answer
        .headers
        .iter()
        .find(|(k, _)| k == "location")
        .map(|(_, v)| v.clone());
    Ok(
        json!({"status": answer.status, "location": location, "text": answer.text.chars().take(4000).collect::<String>()}),
    )
}

/// Every file under `dir`.
fn files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            files(&e.path(), out);
        } else if kind.is_file() {
            out.push(e.path());
        }
    }
}

fn tokens(input: &Value) -> Result<Value> {
    let shared = of(input)?;
    let tokens = shared.script.lock().unwrap().tokens.clone();
    let folder = std::path::PathBuf::from(
        input["obid"]["folder"]
            .as_str()
            .context("obid.folder: an obid-start step's output")?,
    );
    let mut all = vec![];
    files(&folder, &mut all);
    let mut holding: Vec<String> = all
        .iter()
        .filter(|p| {
            std::fs::read(p).is_ok_and(|bytes| {
                tokens
                    .iter()
                    .any(|t| memchr::memmem::find(&bytes, t.as_bytes()).is_some())
            })
        })
        .filter_map(|p| {
            p.strip_prefix(&folder)
                .ok()
                .map(|r| r.to_string_lossy().into_owned())
        })
        .collect();
    holding.sort();
    // Every process's arguments and environment, as anything of this user can read them.
    let flags = if cfg!(target_os = "macos") {
        "-axEww"
    } else {
        "axeww"
    };
    let ps = std::process::Command::new("/bin/ps")
        .args([flags, "-o", "command="])
        .output()
        .context("ps")?;
    let running = String::from_utf8_lossy(&ps.stdout);
    let in_processes = tokens
        .iter()
        .filter(|t| running.contains(t.as_str()))
        .count();
    // `look`: other texts, each said back in `found` when a process's arguments or environment
    // hold it.
    let found: Vec<&str> = (input["look"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .filter(|t| running.contains(t))
        .collect();
    Ok(
        json!({"files": holding, "inProcesses": in_processes, "tokens": tokens.len(), "found": found}),
    )
}

// The server.

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                match std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                {
                    Some(b) => {
                        out.push(b);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn form(s: &str) -> HashMap<String, String> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect()
}

struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    bearer: String,
    body: String,
}

async fn read_request(stream: &mut BufReader<tokio::net::TcpStream>) -> Result<Request> {
    let mut first = String::new();
    stream.read_line(&mut first).await?;
    let mut parts = first.split_whitespace();
    let method = parts.next().context("no method")?.to_string();
    let target = parts.next().context("no target")?.to_string();
    let mut length = 0usize;
    let mut bearer = String::new();
    loop {
        let mut line = String::new();
        stream.read_line(&mut line).await?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = v.trim().parse().unwrap_or(0),
                "authorization" => {
                    bearer = v.trim().strip_prefix("Bearer ").unwrap_or("").to_string()
                }
                _ => {}
            }
        }
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body).await?;
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    Ok(Request {
        method,
        path: path.to_string(),
        query: form(query),
        bearer,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    /// The connection drops after the body, with no end to its stream.
    cut: bool,
}

fn json_answer(status: u16, body: &Value) -> Answer {
    Answer {
        status,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: body.to_string(),
        cut: false,
    }
}

async fn serve(stream: tokio::net::TcpStream, shared: &Shared) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let request = read_request(&mut reader).await?;
    let answer = route(shared, request).await;
    let mut stream = reader.into_inner();
    let mut head = format!("HTTP/1.1 {} X\r\nConnection: close\r\n", answer.status);
    for (k, v) in &answer.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    if answer.cut {
        head.push_str("Transfer-Encoding: chunked\r\n\r\n");
        stream.write_all(head.as_bytes()).await?;
        let chunk = format!("{:x}\r\n{}\r\n", answer.body.len(), answer.body);
        stream.write_all(chunk.as_bytes()).await?;
        stream.flush().await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        // Dropped with no last chunk: the stream never ends.
        return Ok(());
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", answer.body.len()));
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(answer.body.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn route(shared: &Shared, r: Request) -> Answer {
    match (r.method.as_str(), r.path.as_str()) {
        ("GET", "/api/accounts/authorize") => authorize(shared, &r.query),
        ("POST", "/api/accounts/oauth/token") => token(shared, &form(&r.body)).await,
        ("POST", "/api/accounts/oauth/revoke") => revoke(shared, &form(&r.body)),
        ("GET", "/.well-known/jwks.json") => json_answer(200, &jwks(&shared.good)),
        ("GET", "/.well-known/openid-configuration") => json_answer(
            200,
            &json!({"issuer": shared.origin,
                "revocation_endpoint": format!("{}/api/accounts/oauth/revoke", shared.origin)}),
        ),
        ("GET", "/v1/models") => {
            let mut s = shared.script.lock().unwrap();
            s.bearers
                .push((r.bearer.clone(), "GET".into(), "/v1/models".into()));
            if s.refused.contains(&r.bearer) {
                return json_answer(401, &json!({"detail": "Unauthorized"}));
            }
            json_answer(200, &s.models.clone())
        }
        ("POST", "/v1/responses") => responses(shared, &r.bearer).await,
        _ => json_answer(404, &json!({"detail": "Not Found"})),
    }
}

fn authorize(shared: &Shared, query: &HashMap<String, String>) -> Answer {
    let mut s = shared.script.lock().unwrap();
    s.authorized.push(query.clone());
    let redirect = query.get("redirect_uri").cloned().unwrap_or_default();
    let state = query.get("state").cloned().unwrap_or_default();
    let found = |to: String| Answer {
        status: 302,
        headers: vec![("Location".into(), to)],
        body: String::new(),
        cut: false,
    };
    if s.deny {
        return found(format!("{redirect}?error=access_denied&state={state}"));
    }
    let asked = query.get("client_id").cloned().unwrap_or_default();
    let issued = if asked == "dynamic_agent_client" {
        s.issued_clients += 1;
        format!("oaiapp_{}", s.issued_clients)
    } else {
        asked
    };
    let client = s.callback_client.clone().unwrap_or(issued);
    s.minted += 1;
    let code = format!("code-{}", s.minted);
    s.codes.insert(
        code.clone(),
        Grant {
            client: client.clone(),
            nonce: query.get("nonce").cloned().unwrap_or_default(),
            challenge: query.get("code_challenge").cloned().unwrap_or_default(),
        },
    );
    let scope = s.scope.replace(' ', "+");
    found(format!(
        "{redirect}?code={code}&scope={scope}&state={state}&client_id={client}"
    ))
}

fn mint(s: &mut Script, kind: &str) -> String {
    s.minted += 1;
    let token = format!(
        "stand-in-{kind}-{}-{}",
        s.minted,
        uuid::Uuid::new_v4().simple()
    );
    s.tokens.push(token.clone());
    token
}

fn id_token(key: &RsaKeyPair, claims: &Value) -> String {
    let head = B64.encode(json!({"alg": "RS256", "kid": "stand-in-key"}).to_string());
    let body = B64.encode(claims.to_string());
    let text = format!("{head}.{body}");
    let mut signature = vec![0; key.public().modulus_len()];
    let _ = key.sign(
        &ring::signature::RSA_PKCS1_SHA256,
        &ring::rand::SystemRandom::new(),
        text.as_bytes(),
        &mut signature,
    );
    format!("{text}.{}", B64.encode(signature))
}

fn jwks(key: &RsaKeyPair) -> Value {
    let c: ring::rsa::PublicKeyComponents<Vec<u8>> = key.public().into();
    let strip = |v: &[u8]| {
        v.iter()
            .skip_while(|b| **b == 0)
            .copied()
            .collect::<Vec<u8>>()
    };
    let (n, e) = (strip(&c.n), strip(&c.e));
    json!({"keys": [{"kty": "RSA", "kid": "stand-in-key", "alg": "RS256", "use": "sig",
        "n": B64.encode(n), "e": B64.encode(e)}]})
}

async fn token(shared: &Shared, form: &HashMap<String, String>) -> Answer {
    let refresh = form.get("grant_type").is_some_and(|g| g == "refresh_token");
    // Seen as it comes, before its hold: a workflow can act while a refresh is in flight.
    let hold = {
        let mut s = shared.script.lock().unwrap();
        if refresh {
            let t = form.get("refresh_token").cloned().unwrap_or_default();
            s.refreshes.push(t);
        }
        s.hold
    };
    tokio::time::sleep(hold).await;
    let mut s = shared.script.lock().unwrap();
    if let Some((status, body)) = s.token_errors.pop_front() {
        return json_answer(status, &body);
    }
    let now = now();
    let (expires_in, scope) = (s.expires_in, s.scope.clone());
    if refresh {
        let access = mint(&mut s, "access");
        let rotated = mint(&mut s, "refresh");
        return json_answer(
            200,
            &json!({"access_token": access, "refresh_token": rotated, "token_type": "Bearer",
                "expires_in": expires_in, "scope": scope}),
        );
    }
    let code = form.get("code").cloned().unwrap_or_default();
    let Some(grant) = s.codes.remove(&code) else {
        return json_answer(400, &json!({"error": "invalid_grant"}));
    };
    let verifier = form.get("code_verifier").cloned().unwrap_or_default();
    let pkce = B64.encode(<sha2::Sha256 as sha2::Digest>::digest(verifier.as_bytes()));
    if pkce != grant.challenge || form.get("client_id") != Some(&grant.client) {
        return json_answer(400, &json!({"error": "invalid_grant"}));
    }
    let aud = if s.wrong_aud {
        "oaiapp_other".to_string()
    } else {
        grant.client.clone()
    };
    let claims = json!({"iss": shared.origin, "aud": aud, "sub": s.sub, "email": s.email,
        "nonce": grant.nonce, "iat": now, "exp": now + 3600});
    let id = id_token(
        if s.forged {
            &shared.other
        } else {
            &shared.good
        },
        &claims,
    );
    let access = mint(&mut s, "access");
    let rotated = mint(&mut s, "refresh");
    json_answer(
        200,
        &json!({"access_token": access, "refresh_token": rotated, "id_token": id,
            "token_type": "Bearer", "expires_in": expires_in, "scope": scope,
            "earliest_refresh_at": now}),
    )
}

fn revoke(shared: &Shared, form: &HashMap<String, String>) -> Answer {
    let mut s = shared.script.lock().unwrap();
    let status = s.revoke_statuses.pop_front().unwrap_or(200);
    if status == 200 {
        let t = form.get("token").cloned().unwrap_or_default();
        s.revoked.push(t);
    }
    Answer {
        status,
        headers: vec![],
        body: String::new(),
        cut: false,
    }
}

fn sse_text(events: &[Value]) -> String {
    events
        .iter()
        .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap_or("")))
        .collect()
}

fn sse(events: &[Value]) -> Answer {
    Answer {
        status: 200,
        headers: vec![("Content-Type".into(), "text/event-stream".into())],
        body: sse_text(events),
        cut: false,
    }
}

async fn responses(shared: &Shared, bearer: &str) -> Answer {
    let hold = shared
        .script
        .lock()
        .unwrap()
        .response_holds
        .pop_front()
        .unwrap_or_default();
    tokio::time::sleep(hold).await;
    let mut s = shared.script.lock().unwrap();
    s.bearers
        .push((bearer.to_string(), "POST".into(), "/v1/responses".into()));
    if s.refuse_all || s.refused.contains(bearer) {
        let mut a = json_answer(401, &json!({"detail": "Unauthorized"}));
        a.headers
            .push(("x-request-id".into(), "req_refused".into()));
        return a;
    }
    match s.responses.pop_front() {
        Some(Reply::Json(status, body)) => {
            let mut a = json_answer(status, &body);
            a.headers
                .push(("x-request-id".into(), "req_scripted".into()));
            a
        }
        Some(Reply::Sse(events)) => sse(&events),
        Some(Reply::Cut(events)) => {
            let mut a = sse(&events);
            a.cut = true;
            a
        }
        Some(Reply::Text(status, text)) => Answer {
            status,
            headers: vec![("Content-Type".into(), "text/html".into())],
            body: text,
            cut: false,
        },
        None => sse(&said("From the stand-in.")),
    }
}

/// A whole answer, as the Responses API streams one.
fn said(words: &str) -> Vec<Value> {
    vec![
        json!({"type": "response.created", "response": {"id": "resp_1"}}),
        json!({"type": "response.output_item.done", "item": {"type": "message", "role": "assistant",
            "content": [{"type": "output_text", "text": words}]}}),
        json!({"type": "response.completed", "response": {"id": "resp_1", "usage": {
            "input_tokens": 1, "output_tokens": 1, "total_tokens": 2,
            "input_tokens_details": {"cached_tokens": 0},
            "output_tokens_details": {"reasoning_tokens": 0}}}}),
    ]
}

/// A stream that fails with `code` after it began.
fn failed(code: &str, message: &str) -> Vec<Value> {
    vec![
        json!({"type": "response.created", "response": {"id": "resp_2"}}),
        json!({"type": "response.failed", "response": {"id": "resp_2", "status": "failed",
            "error": {"code": code, "message": message}}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_decodes_as_a_browser_sends_it() {
        let f = form("a=b+c&redirect_uri=http%3A%2F%2F127.0.0.1%3A9%2Fx&e=");
        assert_eq!(f["a"], "b c");
        assert_eq!(f["redirect_uri"], "http://127.0.0.1:9/x");
        assert_eq!(f["e"], "");
        assert_eq!(decode("100%"), "100%");
    }

    #[test]
    fn only_loopback_is_visited() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(get("https://example.com/", None)).is_err());
        assert!(rt.block_on(get("http://10.0.0.1:80/", None)).is_err());
    }
}
