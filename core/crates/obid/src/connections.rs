//! Connected apps and their MCP broker (Runner/Connections.swift, Runner/Broker.swift and
//! Engine/Connected.swift). Connection files contain names and URLs only; the separate vault
//! holds the values, and the agent reaches this process with a per-job token.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::{ApiError, Shared};

/// The permission rule that lets a job's agent call the broker's tools (`mcp__oby__<connection>_<tool>`),
/// in the job's rules only when it was given the broker.
pub const BROKER_RULE: &str = "mcp__oby__*";
/// The calls a job's report keeps: the latest ones (the use log has them all).
pub const KEPT_USES: usize = 500;
/// How large the use log grows before it starts again, the one before kept as `uses.1.jsonl`.
const USES_LOG_MAX: u64 = 4 << 20;
const LONGEST: usize = 256 * 1024;
pub(crate) const MCP_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const MCP_SESSION_EXPIRED: &str = "The connected server forgot this MCP session.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub name: String,
    #[serde(default)]
    pub mcp: Option<String>,
    #[serde(default)]
    pub api: Option<String>,
    pub secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub created: String,
}

impl Connection {
    pub fn host(&self) -> String {
        self.mcp
            .as_deref()
            .or(self.api.as_deref())
            .and_then(|s| reqwest::Url::parse(s).ok())
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default()
    }

    pub fn url(&self) -> reqwest::Url {
        reqwest::Url::parse(
            self.mcp
                .as_deref()
                .or(self.api.as_deref())
                .unwrap_or_default(),
        )
        .expect("validated connection URL")
    }

    pub fn problem(&self) -> Option<String> {
        if self.name.is_empty()
            || self.name.len() > 32
            || !self.name.as_bytes()[0].is_ascii_lowercase()
                && !self.name.as_bytes()[0].is_ascii_digit()
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Some(
                "A connection's name is up to 32 lower-case letters, digits or dashes.".into(),
            );
        }
        if self.mcp.is_some() == self.api.is_some() {
            return Some("A connection has either mcp (a remote MCP server's URL) or api (an API's base URL).".into());
        }
        let Some(text) = self.mcp.as_deref().or(self.api.as_deref()) else {
            return Some("A connection needs a URL.".into());
        };
        let Ok(url) = reqwest::Url::parse(text) else {
            return Some(format!(
                "{text} isn't a URL a connection can use: https://host/path, with no login, query or fragment in it."
            ));
        };
        // The phone signs this text, and the secret goes where the parsed URL points: they must
        // be the same. WHATWG parsing reads `\` as `/`, drops tabs and newlines, lower-cases the
        // host and spells a non-ASCII one as xn--, so a URL that isn't already in that form
        // could show the owner one host and send the login to another.
        if url.as_str() != text {
            return Some(format!(
                "{text:?} isn't written the way it's read. Write it as {url}: that is where the secret would go."
            ));
        }
        let loopback = matches!(
            url.host_str().unwrap_or("").to_ascii_lowercase().as_str(),
            "127.0.0.1" | "localhost" | "::1" | "[::1]"
        );
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Some(format!(
                "{text} isn't a URL a connection can use: https://host/path, with no login, query or fragment in it."
            ));
        }
        if !(url.scheme() == "https" || url.scheme() == "http" && loopback) {
            return Some("A connection's URL is HTTPS (HTTP only on 127.0.0.1): the secret never travels in the clear.".into());
        }
        // `oby.` secrets are obid's own (Oby's Tetrix login, crate::tetrix): no connection's.
        if !valid_secret_name(&self.secret) || self.secret.starts_with("oby.") {
            return Some(
                "A secret's name is up to 64 letters, digits, dots, dashes or underscores.".into(),
            );
        }
        if let Some(name) = &self.header {
            let reserved = [
                "host",
                "content-length",
                "content-type",
                "transfer-encoding",
                "connection",
                "accept",
                "cookie",
                "mcp-session-id",
                "mcp-protocol-version",
            ];
            if name.is_empty()
                || name.len() > 64
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || reserved.contains(&name.to_ascii_lowercase().as_str())
            {
                return Some(format!(
                    "{name} can't carry the secret: name a header like Authorization or X-Api-Key."
                ));
            }
        }
        None
    }
}

fn valid_secret_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionUse {
    pub job: String,
    pub connection: String,
    pub tool: String,
    pub access: String,
    pub allowed: bool,
    pub secret: String,
    pub host: String,
    pub date: String,
}

impl ConnectionUse {
    pub fn line(&self) -> String {
        format!(
            "{} {} {} ({})",
            if self.access == "read" {
                "Read"
            } else {
                "Write"
            },
            self.connection,
            self.tool,
            if self.allowed { "allowed" } else { "refused" }
        )
    }
}

pub struct Connections {
    uses_file: PathBuf,
    /// [`USES_LOG_MAX`]; smaller in tests.
    uses_max: u64,
    /// How a vault secret is read: `crate::vault::value`, a fake in tests.
    vault: fn(&str) -> Result<Option<String>, String>,
    tokens: Mutex<HashMap<String, String>>,
    upstreams: tokio::sync::Mutex<HashMap<String, Upstream>>,
    jobs: Weak<crate::jobs::Jobs>,
    client: reqwest::Client,
    api_url: String,
    next_id: std::sync::atomic::AtomicU64,
}

#[derive(Default)]
struct Upstream {
    session: Option<String>,
    version: Option<String>,
    /// What a whole listing gave; none until one came back, so a listing that failed or was cut
    /// off by its caller's timeout is asked again, never kept as no tools for the rest of the job.
    tools: Option<HashMap<String, Value>>,
}

impl Connections {
    pub fn open(home: &Path, jobs: &Arc<crate::jobs::Jobs>, api_url: String) -> Arc<Connections> {
        let uses_file = home.join("vault/uses.jsonl");
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("reqwest client");
        Arc::new(Connections {
            uses_file,
            uses_max: USES_LOG_MAX,
            vault: crate::vault::value,
            tokens: Mutex::new(HashMap::new()),
            upstreams: tokio::sync::Mutex::new(HashMap::new()),
            jobs: Arc::downgrade(jobs),
            client,
            api_url,
            next_id: std::sync::atomic::AtomicU64::new(1),
        })
    }

    /// The vault secret `name`, through obid's vault (`crate::vault::value`): it fails whenever
    /// the vault can't be read, and its value is already in `crate::secrets::known()` as
    /// `vault <name>`, so nothing is learned here (a secret named like one of obid's own,
    /// `api-token`, would replace it).
    fn vault_value(&self, name: &str) -> Result<Option<String>, String> {
        (self.vault)(name)
    }

    /// Where agents reach obid: `http://127.0.0.1:<port>`, or slirp4netns's host on Linux.
    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    /// The broker for job `id` with no connected app needed: where a Codex or Cursor agent asks the
    /// owner (`oby_ask_owner`). Only what the job started with is listed besides, as ever.
    pub fn open_ask(&self, id: &str) -> (String, String) {
        (
            format!("{}/v1/agent/mcp/{id}", self.api_url),
            self.job_token(id),
        )
    }

    /// Oby's Tetrix server for job `id` (crate::tetrix), with the job's token: the broker's, so it
    /// dies with the job the same way.
    pub fn open_tetrix(&self, id: &str) -> (String, String) {
        (
            format!("{}/v1/agent/tetrix/{id}", self.api_url),
            self.job_token(id),
        )
    }

    /// Job `id`'s token, made the first time it's asked for. obid learns it as a secret: whatever
    /// repeats it (the agent's MCP config read back, a header it prints) is masked everywhere.
    fn job_token(&self, id: &str) -> String {
        let mut tokens = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        tokens
            .iter()
            .find_map(|(token, job)| (job == id).then(|| token.clone()))
            .unwrap_or_else(|| {
                let token = format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                );
                tokens.insert(token.clone(), id.to_owned());
                crate::secrets::known().learn(&format!("job {id} MCP token"), &token);
                token
            })
    }

    pub async fn close_job(&self, id: &str) {
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, job| job != id);
        crate::secrets::known().forget(&format!("job {id} MCP token"));
        let prefix = format!("{id}/");
        self.upstreams
            .lock()
            .await
            .retain(|key, _| !key.starts_with(&prefix));
    }

    pub fn allows(&self, id: &str, token: &str) -> bool {
        self.jobs
            .upgrade()
            .and_then(|j| j.get(id))
            .is_some_and(|j| j.status.is_active())
            && self
                .tokens
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(token)
                .is_some_and(|job| job == id)
    }

    pub(crate) fn record(&self, use_record: ConnectionUse) {
        if let Some(jobs) = self.jobs.upgrade() {
            jobs.connection_used(&use_record.job, use_record.clone());
        }
        if let Ok(mut line) = serde_json::to_vec(&use_record) {
            line.push(b'\n');
            // Full: it starts again, the one before kept (a rename keeps its 0600 and owner).
            if std::fs::symlink_metadata(&self.uses_file)
                .is_ok_and(|m| m.is_file() && m.len() + line.len() as u64 > self.uses_max)
            {
                let _ = std::fs::rename(&self.uses_file, previous_log(&self.uses_file));
            }
            let _ = append_private(&self.uses_file, &line);
        }
    }

    async fn handle(&self, id: &str, body: Value) -> Option<Value> {
        let method = body.get("method")?.as_str()?;
        let rpc_id = body.get("id").cloned().filter(|id| !id.is_null())?;
        let params = body
            .get("params")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let result = match method {
            "initialize" => {
                let asked = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                json!({"protocolVersion": if MCP_VERSIONS.contains(&asked) { asked } else { MCP_VERSIONS[0] }, "capabilities":{"tools":{"listChanged":false}}, "serverInfo":{"name":"oby","version":crate::api::API_VERSION}, "instructions":"Oby's own server: oby_ask_owner, when listed, asks your user a question."})
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools": self.tools(id).await}),
            "tools/call" => {
                self.call(
                    id,
                    params.get("name").and_then(Value::as_str).unwrap_or(""),
                    params
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                )
                .await
            }
            _ => {
                return Some(
                    json!({"jsonrpc":"2.0","id":rpc_id,"error":{"code":-32601,"message":format!("Oby's broker doesn't answer {method}.")}}),
                );
            }
        };
        Some(json!({"jsonrpc":"2.0","id":rpc_id,"result":self.clean(result)}))
    }

    async fn tools(&self, id: &str) -> Vec<Value> {
        let mut out = Vec::new();
        if self.asks_through_oby(id) {
            out.push(json!({"name": crate::permissions::ASK_OWNER,
                "description": "Ask your user one question and wait for their answer: only when you truly can't go on without them (a choice only they can make, or something only they know). It returns at once: then end your turn with a one-line summary, and Oby resumes this session with their answer (one of your options, or their own words).",
                "inputSchema": {"type": "object", "required": ["question"], "properties": {
                    "question": {"type": "string", "description": "The whole question, up to 1,000 characters."},
                    "options": {"type": "array", "description": "A few clear choices, when there are some (up to 8). The user may answer in their own words instead.",
                        "items": {"type": "object", "required": ["label"], "properties": {
                            "label": {"type": "string"}, "description": {"type": "string"}}}}}},
                "annotations": {"readOnlyHint": true}}));
        }
        out
    }

    pub(crate) async fn upstream_tools(
        &self,
        job: &str,
        c: &Connection,
        again: bool,
    ) -> Result<HashMap<String, Value>, String> {
        let key = format!("{job}/{}", c.name);
        let listed = self
            .upstreams
            .lock()
            .await
            .get(&key)
            .and_then(|u| u.tools.clone());
        match listed {
            Some(tools) if !again => return Ok(tools),
            Some(_) => {}
            // Never listed whole: a new session, in case the last one stopped halfway.
            None => self.initialize_upstream(job, c).await?,
        }
        let mut tools = HashMap::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut params = json!({});
            if let Some(value) = &cursor {
                params["cursor"] = json!(value);
            }
            let listed = self.upstream_rpc(job, c, "tools/list", params).await?;
            for tool in listed
                .get("tools")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(name) = tool.get("name").and_then(Value::as_str).filter(|n| {
                    !n.is_empty()
                        && n.len() <= 64
                        && n.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
                }) else {
                    continue;
                };
                tools.insert(name.to_owned(), tool.clone());
                if tools.len() >= 1000 {
                    break;
                }
            }
            if tools.len() >= 1000 {
                break;
            }
            cursor = listed
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        self.upstreams.lock().await.entry(key).or_default().tools = Some(tools.clone());
        Ok(tools)
    }

    async fn initialize_upstream(&self, job: &str, c: &Connection) -> Result<(), String> {
        let key = format!("{job}/{}", c.name);
        // A new session keeps what the last whole listing gave: it's the same server, so a session
        // it expired never costs the job its tools, or another session to list them again.
        let tools = self
            .upstreams
            .lock()
            .await
            .remove(&key)
            .and_then(|u| u.tools);
        let init = self
            .send_mcp(
                job,
                c,
                "initialize",
                Some(json!({"jsonrpc":"2.0","id":self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed),"method":"initialize","params":{"protocolVersion":MCP_VERSIONS[0],"capabilities":{},"clientInfo":{"name":"oby","version":crate::api::API_VERSION}}})),
            )
            .await?
            .ok_or_else(|| format!("{} didn't answer.", c.name))?;
        let version = init
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(MCP_VERSIONS[0])
            .to_owned();
        {
            let mut upstreams = self.upstreams.lock().await;
            let upstream = upstreams.entry(key).or_default();
            upstream.version = Some(version);
            upstream.tools = tools;
        }
        self.send_mcp(
            job,
            c,
            "notifications/initialized",
            Some(json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}})),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn upstream_rpc(
        &self,
        job: &str,
        c: &Connection,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let body = if method.starts_with("notifications/") {
            json!({"jsonrpc":"2.0","method":method,"params":params})
        } else {
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        };
        match self.send_mcp(job, c, method, Some(body.clone())).await {
            Err(error) if error == MCP_SESSION_EXPIRED => {
                self.initialize_upstream(job, c).await?;
                self.send_mcp(job, c, method, Some(body))
                    .await
                    .map_err(|error| {
                        if error == MCP_SESSION_EXPIRED {
                            format!("{} didn't answer after starting a new session.", c.name)
                        } else {
                            error
                        }
                    })?
                    .ok_or_else(|| format!("{} didn't answer.", c.name))
            }
            Ok(Some(answer)) => Ok(answer),
            Ok(None) => Err(format!("{} didn't answer.", c.name)),
            Err(error) => Err(error),
        }
    }

    async fn send_mcp(
        &self,
        job: &str,
        c: &Connection,
        method: &str,
        body: Option<Value>,
    ) -> Result<Option<Value>, String> {
        let expected_id = body.as_ref().and_then(|value| value.get("id")).cloned();
        let mut req = self.authorized(c, self.client.post(c.url())).await?;
        req = req
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream");
        let key = format!("{job}/{}", c.name);
        let upstreams = self.upstreams.lock().await;
        let has_session = upstreams
            .get(&key)
            .is_some_and(|upstream| upstream.session.is_some());
        if let Some(up) = upstreams.get(&key) {
            if let Some(session) = &up.session {
                req = req.header("Mcp-Session-Id", session);
            }
            if method != "initialize"
                && let Some(version) = &up.version
            {
                req = req.header("MCP-Protocol-Version", version);
            }
        }
        drop(upstreams);
        if let Some(body) = body {
            req = req.json(&body);
        }
        let response = req.send().await.map_err(|e| e.to_string())?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND && has_session && method != "initialize" {
            return Err(MCP_SESSION_EXPIRED.into());
        }
        if status.is_redirection() {
            return Err(format!(
                "{} sent Oby elsewhere (HTTP {}); the login stays with {}, so Oby doesn't follow.",
                c.host(),
                status.as_u16(),
                c.host()
            ));
        }
        if !status.is_success() {
            // A 401 stays bare: callers refresh the login on exactly that text.
            let why = if status == StatusCode::UNAUTHORIZED {
                None
            } else {
                refusal(response).await
            };
            return Err(match why {
                Some(why) => format!("{} answered HTTP {}: {why}.", c.host(), status.as_u16()),
                None => format!("{} answered HTTP {}.", c.host(), status.as_u16()),
            });
        }
        let session = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|x| x.to_str().ok())
            .map(str::to_owned);
        let is_sse = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|x| x.to_str().ok())
            .is_some_and(|x| x.contains("text/event-stream"));
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > LONGEST {
            return Err("The connected server's answer was too large.".into());
        }
        if let Some(session) = session {
            self.upstreams
                .lock()
                .await
                .entry(key.clone())
                .or_default()
                .session = Some(session);
        }
        if method.starts_with("notifications/") {
            return Ok(None);
        }
        let Some(expected_id) = expected_id else {
            return Err(format!("{} didn't answer.", c.name));
        };
        let answers: Vec<Vec<u8>> = if is_sse {
            String::from_utf8_lossy(&bytes)
                .replace("\r\n", "\n")
                .split("\n\n")
                .filter_map(|part| {
                    let data = part
                        .lines()
                        .filter_map(|l| l.strip_prefix("data:").map(str::trim_start))
                        .collect::<Vec<_>>()
                        .join("\n");
                    (!data.is_empty()).then(|| data.into_bytes())
                })
                .collect()
        } else {
            vec![bytes.to_vec()]
        };
        for answer in answers {
            let Ok(value) = serde_json::from_slice::<Value>(&answer) else {
                continue;
            };
            if value.get("id") == Some(&expected_id) {
                if let Some(error) = value.get("error") {
                    return Err(format!(
                        "{} said: {}.",
                        c.name,
                        error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("an error")
                    ));
                }
                return Ok(value.get("result").cloned().or(Some(json!({}))));
            }
        }
        Err(format!("{} didn't answer.", c.name))
    }

    async fn authorized(
        &self,
        c: &Connection,
        req: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, String> {
        let value = self
            .vault_value(&c.secret)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("The vault has no secret named {}.", c.secret))?;
        if value.contains(['\n', '\r']) {
            return Err(format!(
                "The vault secret {} spans lines, so it can't go in a header.",
                c.secret
            ));
        }
        let req = if c
            .header
            .as_deref()
            .is_some_and(|h| !h.eq_ignore_ascii_case("authorization"))
        {
            req.header(c.header.as_deref().unwrap(), value)
        } else {
            req.bearer_auth(value)
        };
        Ok(req)
    }

    /// Whether job `id`'s agent asks the owner here ([`crate::jobs::asks_through_oby`]).
    fn asks_through_oby(&self, id: &str) -> bool {
        self.jobs
            .upgrade()
            .and_then(|jobs| jobs.get(id))
            .is_some_and(|job| crate::jobs::asks_through_oby(&job))
    }

    async fn call(&self, id: &str, name: &str, args: Value) -> Value {
        // The question raised for the owner, nothing more: its words never answer or allow anything.
        if name == crate::permissions::ASK_OWNER && self.asks_through_oby(id) {
            let Some(jobs) = self.jobs.upgrade() else {
                return failure("The job is gone.");
            };
            return match jobs.ask_owner(id, &args) {
                Ok(text) => json!({"content": [{"type": "text", "text": text}]}),
                Err(why) => failure(why),
            };
        }
        failure(format!(
            "There's no tool named {name}: connected apps went with the strip to a dispatcher."
        ))
    }

    pub(crate) fn clean(&self, value: Value) -> Value {
        fn scrub(value: Value, secrets: &crate::secrets::Secrets) -> Value {
            match value {
                Value::String(s) => Value::String(secrets.redact(&s)),
                Value::Array(xs) => {
                    Value::Array(xs.into_iter().map(|x| scrub(x, secrets)).collect())
                }
                Value::Object(xs) => Value::Object(
                    xs.into_iter()
                        .map(|(k, v)| (secrets.redact(&k), scrub(v, secrets)))
                        .collect(),
                ),
                x => x,
            }
        }
        scrub(value, &crate::secrets::known())
    }
}

/// Why a server refused a call, as its JSON error body says it (`detail`, `error_description` or
/// `message`): one line of at most 200 characters, known secrets scrubbed, or none. Tetrix says
/// which organization it needs this way (#1667).
async fn refusal(mut response: reqwest::Response) -> Option<String> {
    let mut body = Vec::new();
    while body.len() < 4096
        && let Ok(Some(chunk)) = response.chunk().await
    {
        body.extend_from_slice(&chunk);
    }
    reason_in(&body)
}

/// [`refusal`]'s reason in a body already read.
fn reason_in(body: &[u8]) -> Option<String> {
    let answer: Value = serde_json::from_slice(body).ok()?;
    let why = ["detail", "error_description", "message"]
        .iter()
        .find_map(|key| answer[key].as_str())?;
    // Control characters out first, then redacted whole, then cut: a secret split by one, or cut
    // at the end, would slip past redaction.
    let why: String = why
        .chars()
        .filter(|c| c.is_whitespace() || !c.is_control())
        .collect();
    let why = crate::secrets::known().redact(&why.split_whitespace().collect::<Vec<_>>().join(" "));
    let why: String = why.chars().take(200).collect();
    let why = why.trim_end_matches('.');
    (!why.is_empty()).then(|| why.to_owned())
}

/// Where the use log before this one is kept: `uses.1.jsonl` next to `uses.jsonl`.
fn previous_log(file: &Path) -> PathBuf {
    file.with_extension("1.jsonl")
}

/// `use_record` added to a job's report, which keeps the latest [`KEPT_USES`].
pub fn keep_use(uses: &mut Vec<ConnectionUse>, use_record: ConnectionUse) {
    uses.push(use_record);
    if uses.len() > KEPT_USES {
        uses.drain(..uses.len() - KEPT_USES);
    }
}

fn failure(text: impl Into<String>) -> Value {
    json!({"content":[{"type":"text","text":text.into()}],"isError":true})
}

fn append_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let parent = path.parent().unwrap();
    crate::store::make_private_dir(parent)?;
    let uid = nix::unistd::geteuid().as_raw();
    let directory = std::fs::symlink_metadata(parent)?;
    if !directory.is_dir()
        || directory.file_type().is_symlink()
        || directory.mode() & 0o077 != 0
        || directory.uid() != uid
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "the vault folder isn't private",
        ));
    }
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    let metadata = f.metadata()?;
    if !metadata.is_file() || metadata.mode() & 0o077 != 0 || metadata.uid() != uid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "the vault use log isn't private",
        ));
    }
    use std::io::Write;
    f.write_all(data)?;
    f.sync_data()
}

pub fn router() -> Router<Arc<Shared>> {
    Router::new().route("/v1/agent/mcp/{job}", post(agent_mcp))
}

async fn agent_mcp(
    State(shared): State<Arc<Shared>>,
    UrlPath(job): UrlPath<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .unwrap_or("");
    // A conversation's session: Oby's own tools (crate::brain::tools), with its turn's token.
    if job.starts_with("conversation-") {
        let value = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
        let Some(session) = shared.harness.session(&job, token) else {
            // Its Claude Code waiting for a turn connects and lists Oby's tools, and calls none.
            if shared.harness.waiting_with(&job, token) {
                return match crate::brain::tools::handshake(&value) {
                    Some(answer) => (StatusCode::OK, Json(answer)).into_response(),
                    None => StatusCode::ACCEPTED.into_response(),
                };
            }
            return ApiError::new(StatusCode::UNAUTHORIZED, crate::api::NO_TOKEN).into_response();
        };
        return match crate::brain::tools::handle(&shared, &session, value).await {
            Some(answer) => (StatusCode::OK, Json(answer)).into_response(),
            None => StatusCode::ACCEPTED.into_response(),
        };
    }
    if !shared.connections.allows(&job, token) {
        return ApiError::new(StatusCode::UNAUTHORIZED, crate::api::NO_TOKEN).into_response();
    }
    let value = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    if value.get("method").and_then(Value::as_str) == Some("notifications/initialized") {
        return StatusCode::ACCEPTED.into_response();
    }
    match shared.connections.handle(&job, value).await {
        Some(answer) => (StatusCode::OK, Json(answer)).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A refusal's reason is redacted whole before it's cut to 200 characters: a secret that
    /// straddles the cut is never left as a prefix (#1667).
    #[test]
    fn a_refusal_s_reason_is_redacted_before_it_is_cut() {
        let hidden = "a-word-the-vault-holds-here";
        crate::secrets::known().learn("refusal test", hidden);
        let detail = format!("{} {hidden} after", "x".repeat(180));
        let why = reason_in(json!({ "detail": detail }).to_string().as_bytes()).unwrap();
        assert!(why.chars().count() <= 200, "{why}");
        assert!(!why.contains("a-word-the-vault"), "{why}");
        // Split by a control character, it's still the vault's word.
        let split = format!("before {} after", hidden.replacen('-', "-\u{7}", 1));
        let why = reason_in(json!({ "detail": split }).to_string().as_bytes()).unwrap();
        assert!(!why.contains("a-word-the-vault"), "{why}");
        assert_eq!(
            reason_in(br#"{"error_description": "  no\n  org.  "}"#).as_deref(),
            Some("no org")
        );
        assert_eq!(reason_in(b"<html>Bad Request</html>"), None);
    }

    fn folder(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("obid-connections-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    std::thread_local! {
        /// This test's vault: its secrets, or none when it can't be read.
        static VAULT: std::cell::RefCell<Option<HashMap<String, String>>> =
            std::cell::RefCell::new(Some(HashMap::new()));
    }

    /// What `crate::vault::value` answers, from this thread's [`VAULT`].
    fn fake_vault(name: &str) -> Result<Option<String>, String> {
        VAULT.with(|v| match &*v.borrow() {
            Some(secrets) => Ok(secrets.get(name).cloned()),
            None => Err("the vault can't be read: test".into()),
        })
    }

    /// This test's vault holds `secrets` (the vault itself is #570's, tested in vault.rs).
    fn vault(_home: &Path, secrets: &[(&str, &str)]) {
        let all = secrets
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        VAULT.with(|v| *v.borrow_mut() = Some(all));
    }

    fn connection(name: &str, mcp: &str, secret: &str) -> Connection {
        Connection {
            name: name.into(),
            mcp: Some(mcp.into()),
            api: None,
            secret: secret.into(),
            header: None,
            created: "2026-10-16T00:00:00Z".into(),
        }
    }

    /// A broker over `home`, with no jobs to ask.
    fn broker(home: &Path) -> Connections {
        Connections {
            uses_file: home.join("vault/uses.jsonl"),
            uses_max: 1024,
            vault: fake_vault,
            tokens: Mutex::new(HashMap::new()),
            upstreams: tokio::sync::Mutex::new(HashMap::new()),
            jobs: Weak::new(),
            client: reqwest::Client::new(),
            api_url: "http://127.0.0.1:7777".into(),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// The URLs and headers a connection refuses (422): the secret never goes in the clear, to a
    /// login in the URL, to a header that isn't the secret's own, or under a name a tool's
    /// `<connection>_<tool>` couldn't split from.
    #[test]
    fn connections_that_could_send_the_secret_elsewhere_are_refused() {
        let good = connection("mail", "https://mail.example.com/mcp", "mail-token");
        assert_eq!(good.problem(), None);
        let local = connection("local", "http://127.0.0.1:9/mcp", "mail-token");
        assert_eq!(local.problem(), None);
        let mut wrong = vec![
            connection("web", "http://example.com/mcp", "mail-token"),
            connection("web", "http://127.0.0.1.example.com/mcp", "mail-token"),
            connection("Mail_2", "https://example.com/mcp", "mail-token"),
            connection("", "https://example.com/mcp", "mail-token"),
            connection("-mail", "https://example.com/mcp", "mail-token"),
            connection("mail", "https://user:pw@example.com/mcp", "mail-token"),
            connection("mail", "https://example.com/mcp?key=1", "mail-token"),
            connection("mail", "https://example.com/mcp#x", "mail-token"),
            connection("mail", "ftp://example.com/mcp", "mail-token"),
            connection("mail", "not a url", "mail-token"),
            connection("mail", "https://example.com/mcp", "../secret"),
            connection("mail", "https://example.com/mcp", ""),
        ];
        let mut both = good.clone();
        both.api = Some("https://example.com".into());
        wrong.push(both);
        let mut neither = good.clone();
        neither.mcp = None;
        wrong.push(neither);
        for header in [
            "Host",
            "cookie",
            "Content-Length",
            "Mcp-Session-Id",
            "X Key",
            "",
        ] {
            let mut c = good.clone();
            c.header = Some(header.into());
            wrong.push(c);
        }
        for c in wrong {
            assert!(c.problem().is_some(), "{c:?}");
        }
    }

    /// The job's report keeps its latest calls and the use log starts again when full, the one
    /// before kept: neither grows without end, and the last use of each connection survives it.
    #[test]
    fn the_use_records_are_bounded() {
        let one = |n: usize| ConnectionUse {
            job: "job1".into(),
            connection: if n.is_multiple_of(2) {
                "mail"
            } else {
                "people"
            }
            .into(),
            tool: format!("tool{n}"),
            access: "read".into(),
            allowed: true,
            secret: "mail-token".into(),
            host: "mail.example.com".into(),
            date: format!("2026-10-16T00:{:02}:{:02}Z", n / 60 % 60, n % 60),
        };
        let mut uses = Vec::new();
        for n in 0..KEPT_USES + 7 {
            keep_use(&mut uses, one(n));
        }
        assert_eq!(uses.len(), KEPT_USES);
        assert_eq!(uses[0].tool, "tool7");
        assert_eq!(uses.last().unwrap().tool, format!("tool{}", KEPT_USES + 6));

        let home = folder("uses");
        vault(&home, &[]);
        let broker = broker(&home);
        for n in 0..200 {
            broker.record(one(n));
        }
        let log = home.join("vault/uses.jsonl");
        let before = home.join("vault/uses.1.jsonl");
        for file in [&log, &before] {
            let meta = std::fs::metadata(file).unwrap();
            assert!(
                meta.len() <= broker.uses_max,
                "{}: {}",
                file.display(),
                meta.len()
            );
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }
        let lines = |f: &Path| std::fs::read_to_string(f).unwrap().lines().count();
        assert!(lines(&log) > 0 && lines(&before) > 0);
        assert!(
            std::fs::read_to_string(&log)
                .unwrap()
                .contains("\"tool199\"")
        );
    }
}
