//! A stand-in OpenAI on loopback for the ChatGPT tests (docs/contracts/chatgpt-sign-in.md):
//! authorize, token, JWKS, the OpenID configuration, revoke, `/v1/models` and `/v1/responses`,
//! as OpenAI's Sign in with ChatGPT pages describe them. No real sign-in, no real token: consent is
//! this file's redirect, and every token is made here. obid reaches it only through
//! `OBID_TEST_CHATGPT_ORIGIN`, which only a debug build reads.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aws_lc_rs::signature::{KeyPair as _, RsaKeyPair};
use axum::Router;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine;
use serde_json::{Value, json};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;
pub const DIRECT: &str = "chatgpt.tokens.use.direct";

/// What a `POST /v1/responses` answers next.
#[derive(Clone)]
pub enum Reply {
    Json(u16, Value),
    /// A stream that ends with these events.
    Sse(Vec<Value>),
    /// A stream the connection drops under after these events.
    Cut(Vec<Value>),
    /// A page that isn't JSON.
    Text(u16, String),
}

/// How the stand-in behaves next, and what it saw.
pub struct Script {
    pub sub: String,
    pub email: String,
    pub scope: String,
    /// The person says no.
    pub deny: bool,
    /// The id token is signed with a key OpenAI doesn't publish.
    pub forged: bool,
    /// The id token's audience is another client.
    pub wrong_aud: bool,
    /// The callback names this client, whatever was asked.
    pub callback_client: Option<String>,
    /// How long the token endpoint takes.
    pub hold: Duration,
    pub expires_in: i64,
    pub token_errors: VecDeque<(u16, Value)>,
    pub revoke_statuses: VecDeque<u16>,
    pub responses: VecDeque<Reply>,
    /// How long the next `/responses` takes before it looks at anything.
    pub response_holds: VecDeque<Duration>,
    /// Headers the next `/responses` answers with, beside its own (Codex's `x-codex-primary-*`
    /// and `x-codex-secondary-*`, which it reads its rate limits from).
    pub response_headers: VecDeque<Vec<(String, String)>>,
    pub models: Value,
    /// Saw: each authorize request's parameters.
    pub authorized: Vec<HashMap<String, String>>,
    /// Saw: each refresh grant's refresh token.
    pub refreshes: Vec<String>,
    /// Saw: every token it made.
    pub tokens: Vec<String>,
    pub revoked: Vec<String>,
    /// Saw: each `/v1` call's bearer token, method and path.
    pub bearers: Vec<(String, String, String)>,
    /// Access tokens it answers 401 for.
    pub refused: HashSet<String>,
    /// Answers 401 for every token.
    pub refuse_all: bool,
    /// Saw: each `/v1/responses` body.
    pub bodies: Vec<String>,
    issued_clients: u32,
    minted: u32,
    codes: HashMap<String, Grant>,
}

struct Grant {
    client: String,
    nonce: String,
    challenge: String,
}

pub struct OpenAi {
    pub origin: String,
    pub script: Arc<Mutex<Script>>,
    good: Arc<RsaKeyPair>,
}

#[derive(Clone)]
struct Shared {
    origin: String,
    script: Arc<Mutex<Script>>,
    good: Arc<RsaKeyPair>,
    other: Arc<RsaKeyPair>,
}

impl OpenAi {
    pub fn start() -> OpenAi {
        let good = Arc::new(RsaKeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap());
        let other = Arc::new(RsaKeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap());
        let script = Arc::new(Mutex::new(Script {
            sub: "stand-in-sub-1".into(),
            email: "person@example.com".into(),
            scope: format!("openid profile email offline_access resource.invoke {DIRECT}"),
            deny: false,
            forged: false,
            wrong_aud: false,
            callback_client: None,
            hold: Duration::ZERO,
            expires_in: 3600,
            token_errors: VecDeque::new(),
            revoke_statuses: VecDeque::new(),
            responses: VecDeque::new(),
            response_holds: VecDeque::new(),
            response_headers: VecDeque::new(),
            models: json!({"models": [
                {"slug": "gpt-stand-in", "display_name": "Stand-in", "visibility": "list"},
                {"slug": "gpt-hidden", "display_name": "Hidden", "visibility": "hide"},
            ]}),
            authorized: Vec::new(),
            refreshes: Vec::new(),
            tokens: Vec::new(),
            revoked: Vec::new(),
            bearers: Vec::new(),
            refused: HashSet::new(),
            refuse_all: false,
            bodies: Vec::new(),
            issued_clients: 0,
            minted: 0,
            codes: HashMap::new(),
        }));
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let shared = Shared {
            origin: origin.clone(),
            script: script.clone(),
            good: good.clone(),
            other,
        };
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let app = Router::new()
                    .route("/api/accounts/authorize", get(authorize))
                    .route("/api/accounts/oauth/token", post(token))
                    .route("/api/accounts/oauth/revoke", post(revoke))
                    .route("/.well-known/jwks.json", get(jwks))
                    .route("/.well-known/openid-configuration", get(openid))
                    .route("/v1/models", get(models))
                    .route("/v1/responses", post(responses))
                    .with_state(shared);
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        });
        OpenAi {
            origin,
            script,
            good,
        }
    }

    pub fn with<T>(&self, change: impl FnOnce(&mut Script) -> T) -> T {
        change(&mut self.script.lock().unwrap())
    }

    /// The person consents on the page at `url`: where the browser goes next.
    pub fn consent(&self, url: &str) -> String {
        let (status, headers, _) = http("GET", url, &[], None);
        assert_eq!(status, 302, "the stand-in's page redirects");
        headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("location"))
            .map(|(_, value)| value.clone())
            .expect("a Location")
    }

    pub fn public_key_is_published(&self) -> bool {
        !self.good.public_key().as_ref().is_empty()
    }
}

/// One HTTP/1.1 request by hand: its status, headers and body. The caller's `Host` header wins.
pub fn http(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> (u16, Vec<(String, String)>, String) {
    let rest = url.strip_prefix("http://").expect("an http URL");
    let (authority, target) = rest
        .split_once('/')
        .map_or((rest, "/".to_owned()), |(a, t)| (a, format!("/{t}")));
    // (status 0: nothing listens there, as after a sign-in's end)
    let Ok(mut stream) = TcpStream::connect(authority) else {
        return (0, Vec::new(), String::new());
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let mut text = format!("{method} {target} HTTP/1.1\r\nConnection: close\r\n");
    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("host")) {
        text.push_str(&format!("Host: {authority}\r\n"));
    }
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    let body = body.unwrap_or("");
    if !body.is_empty() || method == "POST" {
        text.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    text.push_str("\r\n");
    text.push_str(body);
    stream.write_all(text.as_bytes()).unwrap();
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let (head, rest) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(": "))
        .map(|(n, v)| (n.to_owned(), v.to_owned()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(n, v)| n.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"));
    let text = if chunked {
        unchunk(rest)
    } else {
        rest.to_owned()
    };
    (status, headers, text)
}

fn unchunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if size == 0 || rest.len() < size {
            break;
        }
        out.push_str(&rest[..size]);
        body = rest[size..].strip_prefix("\r\n").unwrap_or(&rest[size..]);
    }
    out
}

async fn authorize(
    State(shared): State<Shared>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let mut script = shared.script.lock().unwrap();
    script.authorized.push(query.clone());
    let redirect = query["redirect_uri"].clone();
    let state = query["state"].clone();
    if script.deny {
        let to = format!("{redirect}?error=access_denied&state={state}");
        return (StatusCode::FOUND, [(header::LOCATION, to)]).into_response();
    }
    let asked = query["client_id"].clone();
    let issued = if asked == "dynamic_agent_client" {
        script.issued_clients += 1;
        format!("oaiapp_{}", script.issued_clients)
    } else {
        asked
    };
    let client = script.callback_client.clone().unwrap_or(issued);
    script.minted += 1;
    let code = format!("code-{}", script.minted);
    script.codes.insert(
        code.clone(),
        Grant {
            client: client.clone(),
            nonce: query["nonce"].clone(),
            challenge: query["code_challenge"].clone(),
        },
    );
    let scope = script.scope.replace(' ', "+");
    let to = format!("{redirect}?code={code}&scope={scope}&state={state}&client_id={client}");
    (StatusCode::FOUND, [(header::LOCATION, to)]).into_response()
}

fn mint(script: &mut Script, kind: &str) -> String {
    script.minted += 1;
    let token = format!(
        "stand-in-{kind}-{}-{}",
        script.minted,
        uuid::Uuid::new_v4().simple()
    );
    script.tokens.push(token.clone());
    token
}

fn id_token(key: &RsaKeyPair, claims: &Value) -> String {
    let head = B64.encode(json!({"alg": "RS256", "kid": "stand-in-key"}).to_string());
    let body = B64.encode(claims.to_string());
    let text = format!("{head}.{body}");
    let mut signature = vec![0; key.public_modulus_len()];
    key.sign(
        &aws_lc_rs::signature::RSA_PKCS1_SHA256,
        &aws_lc_rs::rand::SystemRandom::new(),
        text.as_bytes(),
        &mut signature,
    )
    .unwrap();
    format!("{text}.{}", B64.encode(signature))
}

async fn token(
    State(shared): State<Shared>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let hold = shared.script.lock().unwrap().hold;
    tokio::time::sleep(hold).await;
    let mut script = shared.script.lock().unwrap();
    let refresh = form.get("grant_type").is_some_and(|g| g == "refresh_token");
    if refresh {
        script.refreshes.push(form["refresh_token"].clone());
    }
    if let Some((status, body)) = script.token_errors.pop_front() {
        return (StatusCode::from_u16(status).unwrap(), axum::Json(body)).into_response();
    }
    let now = obi_protocol::time::now();
    let expires_in = script.expires_in;
    let scope = script.scope.clone();
    if refresh {
        let access = mint(&mut script, "access");
        let rotated = mint(&mut script, "refresh");
        return axum::Json(json!({"access_token": access, "refresh_token": rotated,
        "token_type": "Bearer", "expires_in": expires_in, "scope": scope}))
        .into_response();
    }
    let Some(grant) = script.codes.remove(&form["code"]) else {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": "invalid_grant"})),
        )
            .into_response();
    };
    let pkce = B64.encode(obi_protocol::keys::sha256(&form["code_verifier"]));
    if pkce != grant.challenge || form["client_id"] != grant.client {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": "invalid_grant"})),
        )
            .into_response();
    }
    let aud = if script.wrong_aud {
        "oaiapp_other".to_owned()
    } else {
        grant.client
    };
    let claims = json!({"iss": shared.origin, "aud": aud, "sub": script.sub, "email": script.email,
    "nonce": grant.nonce, "iat": now, "exp": now + 3600});
    let key = if script.forged {
        &shared.other
    } else {
        &shared.good
    };
    let id = id_token(key, &claims);
    let access = mint(&mut script, "access");
    let rotated = mint(&mut script, "refresh");
    axum::Json(
        json!({"access_token": access, "refresh_token": rotated, "id_token": id,
    "token_type": "Bearer", "expires_in": expires_in, "scope": scope,
    "earliest_refresh_at": now}),
    )
    .into_response()
}

async fn revoke(
    State(shared): State<Shared>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let mut script = shared.script.lock().unwrap();
    let status = script.revoke_statuses.pop_front().unwrap_or(200);
    if status == 200 {
        script.revoked.push(form["token"].clone());
    }
    StatusCode::from_u16(status).unwrap().into_response()
}

/// RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }
fn der_ints(der: &[u8]) -> (Vec<u8>, Vec<u8>) {
    fn length(der: &[u8], at: &mut usize) -> usize {
        let first = der[*at] as usize;
        *at += 1;
        if first < 0x80 {
            return first;
        }
        let mut len = 0;
        for _ in 0..first - 0x80 {
            len = len << 8 | der[*at] as usize;
            *at += 1;
        }
        len
    }
    let mut at = 1;
    length(der, &mut at);
    let mut int = || {
        at += 1;
        let len = length(der, &mut at);
        let bytes = der[at..at + len].to_vec();
        at += len;
        bytes
            .into_iter()
            .skip_while(|b| *b == 0)
            .collect::<Vec<u8>>()
    };
    (int(), int())
}

async fn jwks(State(shared): State<Shared>) -> axum::Json<Value> {
    let (n, e) = der_ints(shared.good.public_key().as_ref());
    axum::Json(
        json!({"keys": [{"kty": "RSA", "kid": "stand-in-key", "alg": "RS256", "use": "sig",
    "n": B64.encode(n), "e": B64.encode(e)}]}),
    )
}

async fn openid(State(shared): State<Shared>) -> axum::Json<Value> {
    axum::Json(json!({"issuer": shared.origin,
    "revocation_endpoint": format!("{}/api/accounts/oauth/revoke", shared.origin)}))
}

fn bearer(headers: &HeaderMap) -> String {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_owned()
}

async fn models(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    let mut script = shared.script.lock().unwrap();
    let token = bearer(&headers);
    script
        .bearers
        .push((token.clone(), "GET".into(), "/v1/models".into()));
    if script.refused.contains(&token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    axum::Json(script.models.clone()).into_response()
}

async fn responses(State(shared): State<Shared>, headers: HeaderMap, body: String) -> Response {
    let hold = shared
        .script
        .lock()
        .unwrap()
        .response_holds
        .pop_front()
        .unwrap_or_default();
    tokio::time::sleep(hold).await;
    let mut script = shared.script.lock().unwrap();
    script.bodies.push(body);
    let token = bearer(&headers);
    script
        .bearers
        .push((token.clone(), "POST".into(), "/v1/responses".into()));
    if script.refuse_all || script.refused.contains(&token) {
        return (
            StatusCode::UNAUTHORIZED,
            [("x-request-id", "req_refused")],
            axum::Json(json!({"detail": "Unauthorized"})),
        )
            .into_response();
    }
    let headers = script.response_headers.pop_front().unwrap_or_default();
    let mut response = match script.responses.pop_front() {
        Some(Reply::Json(status, body)) => (
            StatusCode::from_u16(status).unwrap(),
            [("x-request-id", "req_scripted")],
            axum::Json(body),
        )
            .into_response(),
        Some(Reply::Sse(events)) => sse(&events),
        Some(Reply::Cut(events)) => {
            use futures_util::StreamExt;
            // (the answer's start goes out first: an error at once would be no answer at all)
            let pieces = futures_util::stream::iter([Ok(sse_text(&events))]).chain(
                futures_util::stream::once(async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Err(std::io::Error::other("the connection dropped"))
                }),
            );
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/event-stream")],
                axum::body::Body::from_stream(pieces),
            )
                .into_response()
        }
        Some(Reply::Text(status, text)) => (
            StatusCode::from_u16(status).unwrap(),
            [(header::CONTENT_TYPE, "text/html")],
            text,
        )
            .into_response(),
        None => sse(&said("From the stand-in.")),
    };
    for (name, value) in headers {
        response.headers_mut().insert(
            header::HeaderName::try_from(name).unwrap(),
            header::HeaderValue::try_from(value).unwrap(),
        );
    }
    response
}

fn sse_text(events: &[Value]) -> String {
    events
        .iter()
        .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap_or("")))
        .collect()
}

fn sse(events: &[Value]) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        sse_text(events),
    )
        .into_response()
}

/// A whole answer, as the Responses API streams one.
pub fn said(words: &str) -> Vec<Value> {
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
pub fn failed(code: &str, message: &str) -> Vec<Value> {
    vec![
        json!({"type": "response.created", "response": {"id": "resp_2"}}),
        json!({"type": "response.failed", "response": {"id": "resp_2", "status": "failed",
        "error": {"code": code, "message": message}}}),
    ]
}
