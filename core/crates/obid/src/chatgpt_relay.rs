//! How a run on a ChatGPT login reaches OpenAI (docs/contracts/chatgpt-sign-in.md): Codex's
//! `base_url` is this relay on loopback, and its `env_key` holds the job's broker token, which dies
//! with the job. The relay takes only `POST …/responses` and `GET …/models`, adds the current access
//! token, and forwards to OpenAI's fixed endpoints. No run ever holds an OpenAI token: spike S0 read
//! a marker out of Codex's environment from another job's box, by pid, under obid's own profile.
//!
//! It also means a token can end in the middle of a job with nothing to restart: near its end the
//! relay refreshes first, and a 401 on a token within 10 minutes of its end is refreshed and sent
//! again. Codex never hears of it.
//!
//! And it's where obid learns what went wrong: Codex's own error for a usage limit is only
//! `exceeded retry limit, last status: 429 Too Many Requests` (spike S5), so what OpenAI said is
//! read here, kept for the run ([`Refusal`]), and taken by whoever ran it. Only what OpenAI's page
//! says is final stops a run ([`refusal_of`]): that goes to Codex as a 400 with obid's own sentence,
//! since Codex retries a 401 or a 403 five times and takes a 400's body as the turn's error, word for
//! word, and what it tries anyway is answered here, without reaching OpenAI. Anything else goes to
//! Codex as it came, and tries again: Codex's own retries, then the job's bounded ones.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::api::{ApiError, Shared};
use crate::chatgpt::{ChatGpt, Refused};

/// The most a request's body may be: a long thread's history, with its images.
const LONGEST_BODY: usize = 64 << 20;
/// A 401 on a token this close to its end is taken to be that end.
const NEAR_ITS_END: i64 = 10 * 60;
/// How long a run's cached list of models stands.
const MODELS_LAST: Duration = Duration::from_secs(3600);
/// The most of OpenAI's own words kept in a sentence.
const LONGEST_SAID: usize = 200;

/// What a run can do after a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    /// The account's usage limit: #1563's question, no reset time.
    Limit,
    /// For now: credentials kept, bounded backoff.
    TryAgain,
    /// Stated, no retry, no OAuth loop, no billing switch.
    Stop,
    /// The login ended: stated too, and only a sign-in brings it back.
    SignIn,
}

/// What OpenAI, or the token behind it, last refused a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub after: After,
    /// The sentence the job or the turn says.
    pub words: String,
}

/// What the relay keeps by run: its last refusal, and whether that stopped it.
#[derive(Default)]
pub struct Runs {
    refusals: Mutex<HashMap<String, Refusal>>,
    models: Mutex<HashMap<String, (Instant, Vec<String>)>>,
}

impl Runs {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Refusal>> {
        self.refusals.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A run's process is starting: what its last one was refused is over.
    pub fn begin(&self, run: &str) {
        self.lock().remove(run);
    }

    /// What the run was refused, once.
    pub fn take(&self, run: &str) -> Option<Refusal> {
        self.lock().remove(run)
    }

    fn refused(&self, run: &str, refusal: Refusal) {
        self.lock().insert(run.to_owned(), refusal);
    }

    /// The refusal that stops the run, so what it tries anyway stays here.
    fn stopped(&self, run: &str) -> Option<Refusal> {
        self.lock()
            .get(run)
            .filter(|r| r.after != After::TryAgain)
            .cloned()
    }

    /// The account's models as OpenAI listed them, kept for an hour.
    fn known_models(&self, account: &str) -> Option<Vec<String>> {
        let models = self.models.lock().unwrap_or_else(|e| e.into_inner());
        let (at, list) = models.get(account)?;
        (at.elapsed() < MODELS_LAST).then(|| list.clone())
    }

    fn keep_models(&self, account: &str, list: Vec<String>) {
        self.models
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(account.to_owned(), (Instant::now(), list));
    }
}

/// The relay's address for `run`, as Codex's `base_url`: it asks `<base>/responses`.
pub fn base_url(api_url: &str, run: &str) -> String {
    format!("{api_url}/v1/agent/chatgpt/{run}")
}

/// The `-c` settings that send Codex to the relay at `base`, its bearer token in `OBY_MCP_TOKEN`
/// (the job's broker token), as OpenAI's Codex app-server page has them but for the address and
/// that variable. `tool_search` goes off because that page says configurations that emit it fail,
/// and the store is `ephemeral` so nothing writes a login in the run's folder.
pub fn codex_settings(base: &str) -> Vec<std::ffi::OsString> {
    const PROVIDER: &str = "openai_chatgpt_plan";
    [
        format!("model_provider=\"{PROVIDER}\""),
        format!("model_providers.{PROVIDER}.name=\"ChatGPT plan\""),
        format!("model_providers.{PROVIDER}.base_url=\"{base}\""),
        format!("model_providers.{PROVIDER}.env_key=\"OBY_MCP_TOKEN\""),
        format!("model_providers.{PROVIDER}.wire_api=\"responses\""),
        format!("model_providers.{PROVIDER}.requires_openai_auth=false"),
        format!("model_providers.{PROVIDER}.supports_websockets=false"),
        "cli_auth_credentials_store=\"ephemeral\"".to_owned(),
        "features.tool_search=false".to_owned(),
    ]
    .into_iter()
    .flat_map(|setting| ["-c".into(), setting.into()])
    .collect()
}

pub fn router() -> Router<Arc<Shared>> {
    Router::new()
        .route("/v1/settings/chatgpt", get(welcome).put(welcomed))
        .route("/v1/agent/chatgpt/{run}/responses", post(responses))
        .route("/v1/agent/chatgpt/{run}/models", get(models))
}

/// `GET /v1/settings/chatgpt`: whether OpenAI's welcome for a first sign-in was shown. obid keeps
/// it, so emptied browser storage doesn't show it again. Off the gateway, like signing in.
async fn welcome(State(shared): State<Arc<Shared>>) -> Json<Value> {
    Json(json!({"welcomed": crate::settings::load(&shared.config.home).chatgpt_welcomed}))
}

/// `PUT /v1/settings/chatgpt` with `{"welcomed": true}`.
async fn welcomed(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let said: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let Some(welcomed) = said["welcomed"].as_bool() else {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "The body is {\"welcomed\": true | false}.",
        ));
    };
    crate::settings::update(&shared.config.home, |s| s.chatgpt_welcomed = welcomed).map_err(
        |e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("obid can't keep that: {e}"),
            )
        },
    )?;
    Ok(Json(json!({"welcomed": welcomed})))
}

/// The `{run}` of one of the relay's routes.
pub(crate) fn run_of(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/agent/chatgpt/")?;
    let run = rest
        .strip_suffix("/responses")
        .or_else(|| rest.strip_suffix("/models"))?;
    (!run.is_empty() && !run.contains('/')).then_some(run)
}

/// The account `run` is on, when `headers` hold its token and that account is signed in with
/// ChatGPT: a job with its broker token, or a conversation's turn with its turn's.
fn account_of(shared: &Shared, run: &str, headers: &HeaderMap) -> Option<String> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))?;
    let account = if run.starts_with("conversation-") {
        shared.harness.session(run, token)?.account.clone()
    } else if shared.connections.allows(run, token) {
        shared.jobs.get(run)?.account
    } else {
        return None;
    };
    let (_, found) = crate::accounts::find(&shared.config.agents.get(), &account)?;
    crate::chatgpt::marker_of(&found)
        .is_some()
        .then_some(account)
}

fn no_token() -> Response {
    ApiError::new(StatusCode::UNAUTHORIZED, crate::api::NO_TOKEN).into_response()
}

async fn models(
    State(shared): State<Arc<Shared>>,
    UrlPath(run): UrlPath<String>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let Some(account) = account_of(&shared, &run, &headers) else {
        return no_token();
    };
    let query = request.uri().query().map(str::to_owned);
    let mut url = format!("{}/models", shared.chatgpt.endpoints().resource);
    if let Some(query) = &query {
        url = format!("{url}?{query}");
    }
    relay(&shared, &run, &account, Method::GET, url, &headers, None).await
}

async fn responses(
    State(shared): State<Arc<Shared>>,
    UrlPath(run): UrlPath<String>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let Some(account) = account_of(&shared, &run, &headers) else {
        return no_token();
    };
    let Ok(body) = axum::body::to_bytes(request.into_body(), LONGEST_BODY).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let url = format!("{}/responses", shared.chatgpt.endpoints().resource);
    relay(
        &shared,
        &run,
        &account,
        Method::POST,
        url,
        &headers,
        Some(body),
    )
    .await
}

/// The request's headers that go on to OpenAI: Codex's own, never its token or a cookie.
fn forwarded(from: &HeaderMap, access: &str) -> HeaderMap {
    let mut to = HeaderMap::new();
    for (name, value) in from {
        let text = name.as_str();
        if matches!(
            text,
            "content-type"
                | "content-encoding"
                | "accept"
                | "originator"
                | "user-agent"
                | "session-id"
                | "thread-id"
                | "x-client-request-id"
        ) || text.starts_with("x-codex-")
            || text.starts_with("openai-")
        {
            to.insert(name.clone(), value.clone());
        }
    }
    if let Ok(mut bearer) = HeaderValue::from_str(&format!("Bearer {access}")) {
        bearer.set_sensitive(true);
        to.insert(header::AUTHORIZATION, bearer);
    }
    to
}

/// One request through the relay.
async fn relay(
    shared: &Arc<Shared>,
    run: &str,
    account: &str,
    method: Method,
    url: String,
    headers: &HeaderMap,
    body: Option<Bytes>,
) -> Response {
    let runs = &shared.chatgpt.runs;
    if let Some(stopped) = runs.stopped(run) {
        return answer_to_codex(&stopped);
    }
    let me = &shared.chatgpt;
    let mut access = match me.access(account).await {
        Ok(access) => access,
        Err(refused) => return token_refused(shared, run, &refused).await,
    };
    let mut renewed = false;
    loop {
        let mut sending = me
            .streaming()
            .request(method.clone(), &url)
            .headers(forwarded(headers, &access));
        if let Some(body) = &body {
            sending = sending.body(body.clone());
        }
        let response = match sending.send().await {
            Ok(response) => response,
            Err(e) => {
                tracing::info!(
                    "run {run}: ChatGPT couldn't be reached: {}",
                    e.without_url()
                );
                return try_again(
                    runs,
                    run,
                    StatusCode::BAD_GATEWAY,
                    "ChatGPT couldn't be reached.".into(),
                );
            }
        };
        let status = response.status();
        if status.is_success() {
            runs.lock().remove(run);
            return streamed(shared.clone(), account.to_owned(), run.to_owned(), response);
        }
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let kind = response.headers().get(header::CONTENT_TYPE).cloned();
        let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
        let body_said = response.bytes().await.unwrap_or_default();
        let said = said_in(&body_said);
        if status == StatusCode::UNAUTHORIZED
            && !renewed
            && me.may_replace(account, &access, NEAR_ITS_END)
        {
            renewed = true;
            match me.replacing(account, &access).await {
                Ok(fresh) => {
                    access = fresh;
                    continue;
                }
                Err(refused) => return token_refused(shared, run, &refused).await,
            }
        }
        // As it came, for Codex to read: a body it can tell apart, or a status it backs off from.
        let as_it_came = || {
            let mut passed = (status, body_said.clone()).into_response();
            let headers = passed.headers_mut();
            headers.remove(header::CONTENT_TYPE);
            if let Some(kind) = kind {
                headers.insert(header::CONTENT_TYPE, kind);
            }
            if let Some(value) = retry_after {
                headers.insert(header::RETRY_AFTER, value);
            }
            passed
        };
        // The model list isn't the work: a refusal of it stops nothing.
        if method == Method::GET {
            return as_it_came();
        }
        let refusal = refusal_of(Some(status.as_u16()), &said, request_id.as_deref());
        tracing::warn!(
            "run {run}: ChatGPT answered {status} {}",
            code_of(&said).unwrap_or_default()
        );
        let after = refusal.after;
        runs.refused(run, refusal.clone());
        return match after {
            After::TryAgain => as_it_came(),
            // A limit's 429 stops Codex by itself (spike S5), and its code is why here.
            After::Limit | After::Stop | After::SignIn => answer_to_codex(&refusal),
        };
    }
}

/// An answer's body as JSON, or as the text it is.
fn said_in(body: &[u8]) -> Value {
    let body = &body[..body.len().min(1 << 20)];
    serde_json::from_slice(body).unwrap_or_else(|_| {
        Value::String(String::from_utf8_lossy(body).chars().take(4096).collect())
    })
}

fn try_again(runs: &Runs, run: &str, status: StatusCode, words: String) -> Response {
    runs.refused(
        run,
        Refusal {
            after: After::TryAgain,
            words: words.clone(),
        },
    );
    (status, words).into_response()
}

/// The token couldn't be had: for now, Codex backs off; else the run stops, saying why.
async fn token_refused(shared: &Arc<Shared>, run: &str, refused: &Refused) -> Response {
    let runs = &shared.chatgpt.runs;
    match refused {
        Refused::Later(words) => {
            try_again(runs, run, StatusCode::SERVICE_UNAVAILABLE, words.clone())
        }
        Refused::SignIn(words) | Refused::Client(words) | Refused::PlanOff(words) => {
            let refusal = Refusal {
                after: if matches!(refused, Refused::SignIn(_)) {
                    After::SignIn
                } else {
                    After::Stop
                },
                words: words.clone(),
            };
            runs.refused(run, refusal.clone());
            crate::accounts::send_accounts(shared).await;
            answer_to_codex(&refusal)
        }
    }
}

/// What Codex is told of a refusal that stops the run. A 400, which it doesn't retry and whose body
/// it says as the turn's error; a limit's 429, which it doesn't retry either, as OpenAI sends it.
fn answer_to_codex(refusal: &Refusal) -> Response {
    match refusal.after {
        After::Limit => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": {"code": LIMIT_CODE, "message": refusal.words}})),
        )
            .into_response(),
        _ => (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            refusal.words.clone(),
        )
            .into_response(),
    }
}

const LIMIT_CODE: &str = "subscription_sharing_usage_limit_exceeded";
/// OpenAI's own words for it (its UI/UX page).
pub const LIMIT_WORDS: &str =
    "Usage limit reached. Review your plan or this app's limit in ChatGPT settings.";

/// A Responses-layer error's machine-readable code. A body with `detail` has none: that's diagnostic
/// text, never matched as one, as OpenAI's page says.
fn code_of(said: &Value) -> Option<String> {
    said["error"]["code"].as_str().map(str::to_owned)
}

/// OpenAI's words, on one line and bounded, for a sentence. Data, quoted, never instructions.
fn words_of(said: &Value) -> String {
    let text = said["error"]["message"]
        .as_str()
        .or_else(|| said["detail"].as_str())
        .or_else(|| said.as_str())
        .unwrap_or("");
    let one: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(LONGEST_SAID)
        .collect();
    crate::secrets::known().redact(&one)
}

/// What an answer that isn't a success comes to, or (`status` none) a stream's failure after it
/// began. Only what OpenAI's Errors and recovery page calls final stops a run. Its list "is not
/// exhaustive", and what it doesn't name may pass: the unknown tries again, bounded by Codex's own
/// retries and the job's.
fn refusal_of(status: Option<u16>, said: &Value, request: Option<&str>) -> Refusal {
    let code = code_of(said);
    let request = request.map_or_else(String::new, |id| format!(" (request {id})"));
    let stop = |words: String| Refusal {
        after: After::Stop,
        words: format!("{words}{request}"),
    };
    match code.as_deref() {
        Some(LIMIT_CODE) => Refusal {
            after: After::Limit,
            words: LIMIT_WORDS.into(),
        },
        Some("subscription_sharing_user_not_eligible") => {
            stop("ChatGPT plan use isn't available for this account or workspace.".into())
        }
        Some("subscription_sharing_route_not_supported") => stop(
            "ChatGPT refused the way Oby asked: that's Oby's configuration, not your login.".into(),
        ),
        Some("subscription_sharing_unsupported_capability") => stop(format!(
            "ChatGPT plan use doesn't support something this run asked for{}.",
            said["error"]["param"]
                .as_str()
                .map_or_else(String::new, |param| format!(
                    " ({})",
                    crate::secrets::known().redact(param)
                ))
        )),
        Some("subscription_sharing_invalid_user") => stop(format!(
            "ChatGPT didn't accept this account's login: {}",
            words_of(said)
        )),
        Some(chatpass) if chatpass.starts_with("chatpass_v2_") => {
            stop("ChatGPT's permission for this account doesn't cover this request.".into())
        }
        _ => match status {
            // Direct admission's own, before a stream opens: the identity or permission wasn't
            // accepted. A 401 comes here only after a refresh couldn't help (`relay`). One with no
            // word of OpenAI's is an edge's page, like a 403's below.
            Some(401) if !said.is_string() => stop(format!(
                "ChatGPT didn't accept this account's login: {}",
                words_of(said)
            )),
            // A policy or a permission, in admission's own `{"detail": …}`. A 403 with no word of
            // OpenAI's is an edge's page, and says nothing of the plan.
            Some(403) if said["detail"].is_string() => {
                stop(format!("ChatGPT refused this request: {}", words_of(said)))
            }
            Some(status) if !(500..=599).contains(&status) && status != 429 => Refusal {
                after: After::TryAgain,
                words: format!(
                    "ChatGPT didn't take this request ({status}): {}{request}",
                    words_of(said)
                ),
            },
            Some(status) => Refusal {
                after: After::TryAgain,
                words: format!("ChatGPT is unavailable for now ({status}).{request}"),
            },
            None => Refusal {
                after: After::TryAgain,
                words: format!(
                    "ChatGPT's answer failed part way: {}{request}",
                    words_of(said)
                ),
            },
        },
    }
}

/// A success, as it comes: event by whole event, so the scrub of answers (crate::vault) sees each
/// one whole, and a stream that fails after it began is noticed. Its answer's `response.failed`
/// goes on to Codex; what Codex tries after it stays here.
fn streamed(
    shared: Arc<Shared>,
    account: String,
    run: String,
    response: reqwest::Response,
) -> Response {
    // What Codex reads off a direct answer (the model served, its limits' headers); never a cookie.
    let passed: Vec<_> = response
        .headers()
        .iter()
        .filter(|(name, _)| {
            let name = name.as_str();
            name == "content-type" || name.starts_with("x-") || name.starts_with("openai-")
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let request = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let events = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|kind| kind.starts_with("text/event-stream"));
    struct Reading {
        response: reqwest::Response,
        held: Vec<u8>,
        ended: bool,
        shared: Arc<Shared>,
        account: String,
        run: String,
        request: Option<String>,
        /// A stream of events, which says itself how it ended (a model list's JSON doesn't).
        events: bool,
        /// It said so.
        settled: bool,
    }
    impl Reading {
        /// The connection ended and the stream hadn't: no answer, and nothing OpenAI said of why.
        fn gone_short(&self) {
            if self.events && !self.settled {
                let said = json!("its stream stopped before it ended.");
                let refusal = refusal_of(None, &said, self.request.as_deref());
                tracing::warn!("run {}: ChatGPT's stream stopped short", self.run);
                self.shared.chatgpt.runs.refused(&self.run, refusal);
            }
        }
    }
    let frames = futures_util::stream::unfold(
        Reading {
            response,
            held: Vec::new(),
            ended: false,
            shared,
            account,
            run,
            request,
            events,
            settled: false,
        },
        |mut reading| async move {
            loop {
                if let Some(end) = frame_end(&reading.held) {
                    let frame: Vec<u8> = reading.held.drain(..end).collect();
                    reading.settled |= notice(
                        &reading.shared,
                        &reading.account,
                        &reading.run,
                        &frame,
                        reading.request.as_deref(),
                    );
                    return Some((Ok::<_, std::io::Error>(Bytes::from(frame)), reading));
                }
                if reading.ended {
                    if reading.held.is_empty() {
                        return None;
                    }
                    let rest = std::mem::take(&mut reading.held);
                    return Some((Ok(Bytes::from(rest)), reading));
                }
                match reading.response.chunk().await {
                    Ok(Some(chunk)) => reading.held.extend_from_slice(&chunk),
                    Ok(None) => {
                        reading.ended = true;
                        reading.gone_short();
                    }
                    Err(e) => {
                        reading.ended = true;
                        reading.held.clear();
                        reading.gone_short();
                        return Some((Err(std::io::Error::other(e.without_url())), reading));
                    }
                }
            }
        },
    );
    let mut answer = Body::from_stream(frames).into_response();
    answer.headers_mut().extend(passed);
    answer
}

/// Where a stream's first whole event ends, past its blank line.
fn frame_end(held: &[u8]) -> Option<usize> {
    let lf = held.windows(2).position(|w| w == b"\n\n").map(|at| at + 2);
    let crlf = held
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4);
    lf.into_iter().chain(crlf).min()
}

/// What a stream's event means, going by its own `type`: a model's words may name any of them. A
/// failure's meaning is kept for the run; a plain `error` event says the same as a `response.failed`,
/// though real Codex ignores it (spike, 2026-11-03), so a usage limit that comes that way is caught
/// only here. A response that completed lifts the hour obid inferred from a limit. Says whether the
/// event told how the stream ended.
fn notice(shared: &Shared, account: &str, run: &str, frame: &[u8], request: Option<&str>) -> bool {
    let contains = |needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);
    if !contains(b"response.failed")
        && !contains(b"response.completed")
        && !contains(b"response.incomplete")
        && !contains(b"\"error\"")
    {
        return false;
    }
    let Some(event) = String::from_utf8_lossy(frame)
        .lines()
        .find_map(|line| serde_json::from_str::<Value>(line.strip_prefix("data:")?.trim()).ok())
    else {
        return false;
    };
    let error = match event["type"].as_str() {
        Some("response.failed") => event["response"]["error"].clone(),
        Some("error") if event["error"].is_object() => event["error"].clone(),
        Some("error") => event.clone(),
        Some("response.completed") => {
            shared.jobs.limit_passed(account);
            return true;
        }
        Some("response.incomplete") => return true,
        _ => return false,
    };
    let said = json!({"error": error});
    let refusal = refusal_of(None, &said, request);
    tracing::warn!(
        "run {run}: ChatGPT's stream failed: {}",
        code_of(&said).unwrap_or_default()
    );
    shared.chatgpt.runs.refused(run, refusal);
    true
}

impl ChatGpt {
    /// The account's models, listed as OpenAI lists them for display, cached for an hour. None when
    /// they can't be had now.
    pub async fn models_of(self: &Arc<Self>, account: &str) -> Option<Vec<String>> {
        if let Some(known) = self.runs.known_models(account) {
            return Some(known);
        }
        let access = self.access(account).await.ok()?;
        let response = self
            .streaming()
            .get(format!("{}/models", self.endpoints().resource))
            .bearer_auth(access)
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let list = listed_models(&crate::tetrix::small_json(response).await);
        // An empty list is no list: it says nothing of the model the owner chose.
        if list.is_empty() {
            return None;
        }
        self.runs.keep_models(account, list.clone());
        Some(list)
    }

    /// The model a run on `account` uses: `chosen` when the account lists it, else the first it
    /// lists; `chosen` as it is when the list can't be had. The second value says it changed.
    pub async fn model_for(
        self: &Arc<Self>,
        account: &str,
        chosen: Option<&str>,
    ) -> (Option<String>, bool) {
        let chosen = chosen.filter(|m| !m.is_empty());
        let Some(list) = self.models_of(account).await else {
            return (chosen.map(str::to_owned), false);
        };
        match chosen {
            Some(model) if list.iter().any(|m| m == model) => (Some(model.to_owned()), false),
            _ => (list.first().cloned(), chosen.is_some()),
        }
    }
}

fn listed_models(said: &Value) -> Vec<String> {
    said["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["visibility"] == "list")
        .filter_map(|m| m["slug"].as_str())
        .filter(|slug| !slug.is_empty() && !slug.chars().any(char::is_whitespace))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_two_routes_of_one_run_are_the_relays() {
        assert_eq!(run_of("/v1/agent/chatgpt/job1/responses"), Some("job1"));
        assert_eq!(
            run_of("/v1/agent/chatgpt/conversation-a/models"),
            Some("conversation-a")
        );
        for path in [
            "/v1/agent/chatgpt/job1",
            "/v1/agent/chatgpt//responses",
            "/v1/agent/chatgpt/a/b/responses",
            "/v1/agent/chatgpt/job1/files",
            "/v1/agent/mcp/job1",
        ] {
            assert_eq!(run_of(path), None, "{path}");
        }
    }

    /// CHATGPT-13, CHATGPT-14: each answer OpenAI's page lists comes to what it says to do.
    #[test]
    fn chatgpt_14_each_of_openais_answers_comes_to_what_its_page_says() {
        let error =
            |code: &str| json!({"error": {"code": code, "message": "said", "param": "tools[0]"}});
        let after = |status, said: &Value| refusal_of(Some(status), said, Some("req_1")).after;
        assert_eq!(after(429, &error(LIMIT_CODE)), After::Limit);
        assert_eq!(
            refusal_of(None, &error(LIMIT_CODE), None).after,
            After::Limit,
            "after the stream began"
        );
        assert_eq!(
            refusal_of(Some(429), &error(LIMIT_CODE), None).words,
            LIMIT_WORDS
        );
        // After the stream began, only what's listed is final.
        for code in [
            "server_error",
            "rate_limit_exceeded",
            "context_length_exceeded",
            "brand_new",
        ] {
            assert_eq!(
                refusal_of(None, &error(code), None).after,
                After::TryAgain,
                "{code}"
            );
        }
        assert_eq!(
            refusal_of(None, &json!({"error": null}), None).after,
            After::TryAgain
        );
        assert_eq!(
            refusal_of(None, &error("subscription_sharing_user_not_eligible"), None).after,
            After::Stop
        );
        // Statuses and shapes the page doesn't call final.
        for status in [400, 404, 408, 409, 422] {
            assert_eq!(
                after(status, &error("brand_new")),
                After::TryAgain,
                "{status}"
            );
        }
        assert_eq!(after(403, &json!("<html>blocked</html>")), After::TryAgain);
        assert_eq!(after(403, &error("brand_new")), After::TryAgain);
        for code in [
            "subscription_sharing_usage_unavailable",
            "subscription_sharing_user_unavailable",
        ] {
            assert_eq!(after(503, &error(code)), After::TryAgain, "{code}");
        }
        assert_eq!(
            after(503, &json!({"detail": "Direct routing is unavailable"})),
            After::TryAgain
        );
        assert_eq!(after(500, &json!({})), After::TryAgain);
        assert_eq!(
            after(429, &json!({"error": {"code": "rate_limit"}})),
            After::TryAgain,
            "a rate limit is no usage limit"
        );
        for code in [
            "subscription_sharing_user_not_eligible",
            "subscription_sharing_route_not_supported",
            "subscription_sharing_unsupported_capability",
            "subscription_sharing_invalid_user",
            "chatpass_v2_scope_not_authorized",
            "chatpass_v2_invalid_authorization_context",
        ] {
            assert_eq!(after(403, &error(code)), After::Stop, "{code}");
        }
        // `detail` is diagnostic text, never a code, whatever it says.
        let looks = json!({"detail": LIMIT_CODE});
        assert_eq!(after(403, &looks), After::Stop);
        // The request id and OpenAI's words are in what the job says.
        let said = refusal_of(Some(401), &json!({"detail": "Nope\nreally"}), Some("req_9")).words;
        assert!(
            said.contains("Nope really") && said.contains("req_9"),
            "{said}"
        );
        let said = refusal_of(
            Some(400),
            &error("subscription_sharing_unsupported_capability"),
            None,
        )
        .words;
        assert!(said.contains("tools[0]"), "{said}");
    }

    /// CHATGPT-13: a stream that fails after it began stops the run as one that failed before it.
    #[test]
    fn chatgpt_13_a_stream_that_fails_with_the_limit_is_noticed() {
        let runs = Runs::default();
        assert_eq!(runs.stopped("a"), None);
        runs.refused(
            "a",
            Refusal {
                after: After::TryAgain,
                words: "later".into(),
            },
        );
        assert_eq!(runs.stopped("a"), None, "Codex may try again");
        runs.refused(
            "a",
            Refusal {
                after: After::Limit,
                words: LIMIT_WORDS.into(),
            },
        );
        assert_eq!(runs.stopped("a").unwrap().after, After::Limit);
        assert_eq!(runs.take("a").unwrap().after, After::Limit);
        assert_eq!(runs.take("a"), None);
        runs.refused(
            "b",
            Refusal {
                after: After::Stop,
                words: "no".into(),
            },
        );
        runs.begin("b");
        assert_eq!(runs.stopped("b"), None, "a new process starts clean");
    }

    #[test]
    fn a_model_is_listed_for_display_or_not_at_all() {
        let said = json!({"models": [
            {"slug": "gpt-a", "visibility": "list"},
            {"slug": "gpt-hidden", "visibility": "hide"},
            {"slug": "bad slug", "visibility": "list"},
            {"visibility": "list"},
            {"slug": "gpt-b", "visibility": "list"},
        ]});
        assert_eq!(listed_models(&said), ["gpt-a", "gpt-b"]);
        assert!(listed_models(&json!({})).is_empty());
    }

    /// A shell and a config file get nothing from these: the token's in the environment by name.
    #[test]
    fn codexs_settings_name_only_the_relay_and_its_broker_token() {
        let settings = codex_settings("http://127.0.0.1:1/v1/agent/chatgpt/job1");
        let text: Vec<String> = settings
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(text.iter().any(|s| s == "features.tool_search=false"));
        assert!(
            text.iter()
                .any(|s| s.ends_with(".env_key=\"OBY_MCP_TOKEN\""))
        );
        assert!(
            text.iter()
                .any(|s| s.ends_with(".requires_openai_auth=false"))
        );
        assert!(
            text.iter()
                .any(|s| s.ends_with(".supports_websockets=false"))
        );
    }
}
