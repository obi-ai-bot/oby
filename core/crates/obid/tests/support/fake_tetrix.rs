//! A fake Tetrix and its OAuth server on 127.0.0.1, and obid's sign-in to it as the browser
//! would: shared by the tests that give agents Tetrix (tetrix_api.rs, helpers_api.rs).

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use serde_json::{Value, json};

use crate::support::{Obid, call, within};

pub const ACCESS_1: &str = "tetrix-access-0f9e8d7c6b5a49382716";
pub const ACCESS_2: &str = "tetrix-access-a1b2c3d4e5f60718293a";
pub const REFRESH_1: &str = "tetrix-refresh-5e4d3c2b1a0f9e8d7c6b";
pub const REFRESH_2: &str = "tetrix-refresh-9a8b7c6d5e4f30211203";
/// The organization the "orgs" Tetrix answers for.
pub const ORG: &str = "0b5e1a2c-7d3f-4e6a-9c8b-1f2e3d4c5b6a";

/// What the fake Tetrix was asked, and the state its login is in.
#[derive(Default)]
pub struct Seen {
    /// Every request: method, path, Authorization, body.
    pub requests: Vec<(String, String, String, String)>,
    pub challenge: String,
    pub access: String,
    pub refresh: String,
    /// The `save_session` calls' arguments.
    pub saved: Vec<Value>,
    /// How its earlier sessions answer: "" as they are, "leaky" listing one whose id holds a key,
    /// "slow" listing four whose context takes 9 seconds each: each call within its own 10
    /// seconds, all of them past the check's 30. "flaky": its first `tools/list` answers 503.
    /// "down": its MCP server refuses every token and its sign-in server answers 503. "orgs":
    /// its login is in several organizations, so its MCP server answers 400 to a call whose
    /// address doesn't say `?org=` [`ORG`] (Tetrix's mcp_service/app/org_request.py). "quiet":
    /// every call answers nothing, so no job gets context from it.
    pub mode: &'static str,
    /// The "flaky" 503 was sent.
    pub flaked: bool,
}

/// A key the secret scan flags, put together here so this file holds none.
pub fn made_up_key() -> String {
    ["AK", "IA", "QWERTYUIOPASDFGH"].concat()
}

pub fn reply(stream: &mut std::net::TcpStream, status: &str, kind: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

pub fn form(body: &str) -> std::collections::HashMap<String, String> {
    reqwest::Url::parse(&format!("http://x/?{body}"))
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// A fake Tetrix: RFC 9728 metadata, an authorization server whose RFC 8414 path isn't JSON (as
/// Keycloak's isn't) but whose OpenID discovery is, dynamic client registration, PKCE, refresh
/// with rotation, revocation, and an MCP server that wants the current access token and puts it
/// in its answers. The first access token lasts 30 seconds, so obid refreshes it.
pub fn fake_tetrix() -> (String, Arc<Mutex<Seen>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(Seen::default()));
    let state = seen.clone();
    let origin = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            let mut length = 0;
            let mut auth = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let line = line.trim_end().to_owned();
                if request.is_empty() {
                    request = line.clone();
                }
                if line.is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    if name.eq_ignore_ascii_case("authorization") {
                        auth = value.trim().to_owned();
                    }
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let body = String::from_utf8_lossy(&body).into_owned();
            let mut parts = request.split(' ');
            let method = parts.next().unwrap_or("").to_owned();
            let target = parts.next().unwrap_or("").to_owned();
            let path = target.split('?').next().unwrap_or("").to_owned();
            let mut seen = state.lock().unwrap();
            seen.requests
                .push((method.clone(), target.clone(), auth.clone(), body.clone()));
            let issuer = format!("{origin}/auth");
            match (method.as_str(), path.as_str()) {
                // A server whose metadata is for another one: obid must not sign in there.
                ("GET", "/.well-known/oauth-protected-resource/evil-mcp") => reply(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    &json!({"resource": "https://other.example/mcp", "authorization_servers": [issuer]})
                        .to_string(),
                ),
                // Only where its 401 says: obid must read the challenge.
                ("GET", "/meta/tetrix-resource") => reply(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    &json!({"resource": format!("{origin}/mcp"), "authorization_servers": [issuer],
                        "scopes_supported": ["graph:read", "sessions:write", "hitl:write"]})
                    .to_string(),
                ),
                ("GET", "/.well-known/oauth-authorization-server/auth") => {
                    reply(&mut stream, "200 OK", "text/html", "<html>not JSON</html>")
                }
                ("GET", "/auth/.well-known/openid-configuration") => reply(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    &json!({"issuer": issuer, "authorization_endpoint": format!("{issuer}/authorize"),
                        "token_endpoint": format!("{issuer}/token"), "registration_endpoint": format!("{issuer}/register"),
                        "revocation_endpoint": format!("{issuer}/revoke"), "code_challenge_methods_supported": ["S256"]})
                    .to_string(),
                ),
                ("POST", "/auth/register") => reply(
                    &mut stream,
                    "201 Created",
                    "application/json",
                    r#"{"client_id": "oby-dynamic-1"}"#,
                ),
                ("GET", "/auth/authorize") => {
                    let query = form(target.split_once('?').map_or("", |(_, q)| q));
                    seen.challenge = query.get("code_challenge").cloned().unwrap_or_default();
                    reply(&mut stream, "200 OK", "text/html", "<p>Sign in</p>");
                }
                ("POST", "/auth/token") if seen.mode == "down" => {
                    reply(&mut stream, "503 Service Unavailable", "application/json", "{}")
                }
                ("POST", "/auth/token") => {
                    let asked = form(&body);
                    let answer = match asked.get("grant_type").map(String::as_str) {
                        Some("authorization_code")
                            if asked.get("code").map(String::as_str) == Some("the-code")
                                && asked.get("client_id").map(String::as_str)
                                    == Some("oby-dynamic-1")
                                && base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                                    obi_protocol::keys::sha256(
                                        asked.get("code_verifier").cloned().unwrap_or_default(),
                                    ),
                                ) == seen.challenge =>
                        {
                            seen.access = ACCESS_1.into();
                            seen.refresh = REFRESH_1.into();
                            Some(
                                json!({"access_token": ACCESS_1, "refresh_token": REFRESH_1, "expires_in": 30,
                                "scope": "graph:read sessions:write hitl:write offline_access"}),
                            )
                        }
                        Some("refresh_token")
                            if asked.get("refresh_token") == Some(&seen.refresh)
                                && seen.refresh == REFRESH_1 =>
                        {
                            seen.access = ACCESS_2.into();
                            seen.refresh = REFRESH_2.into();
                            Some(
                                json!({"access_token": ACCESS_2, "refresh_token": REFRESH_2, "expires_in": 3600}),
                            )
                        }
                        _ => None,
                    };
                    match answer {
                        Some(answer) => {
                            reply(&mut stream, "200 OK", "application/json", &answer.to_string())
                        }
                        None => reply(
                            &mut stream,
                            "400 Bad Request",
                            "application/json",
                            r#"{"error": "invalid_grant"}"#,
                        ),
                    }
                }
                ("POST", "/auth/revoke") => reply(&mut stream, "200 OK", "application/json", "{}"),
                ("POST", "/mcp") => {
                    if seen.access.is_empty()
                        || auth != format!("Bearer {}", seen.access)
                        || seen.mode == "down"
                    {
                        reply(
                            &mut stream,
                            &format!("401 Unauthorized\r\nWWW-Authenticate: Bearer error=\"invalid_token\", resource_metadata=\"{origin}/meta/tetrix-resource\""),
                            "application/json",
                            "{}",
                        );
                        continue;
                    }
                    if seen.mode == "orgs" && target != format!("/mcp?org={ORG}") {
                        reply(
                            &mut stream,
                            "400 Bad Request",
                            "application/json",
                            r#"{"detail": "multiple organizations in token; org query param required"}"#,
                        );
                        continue;
                    }
                    let message: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                    if seen.mode == "flaky" && !seen.flaked && message["method"] == "tools/list" {
                        seen.flaked = true;
                        reply(&mut stream, "503 Service Unavailable", "text/plain", "busy");
                        continue;
                    }
                    let id = message["id"].clone();
                    let tool = message["params"]["name"].as_str().unwrap_or("").to_owned();
                    let leak = format!("seen with Bearer {}", seen.access);
                    let result = match message["method"].as_str().unwrap_or("") {
                        "initialize" => json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}},
                            "serverInfo": {"name": "tetrix", "version": "1"}}),
                        "tools/list" => {
                            let tools: Vec<Value> = ["search_entities", "get_entity", "repo_sessions",
                                "session_context", "session_digest", "save_session", "resume_session",
                                "admin_wipe_graph"].iter()
                                .map(|name| json!({"name": name, "description": name, "inputSchema": {"type": "object"}}))
                                .collect();
                            json!({"tools": tools})
                        }
                        "tools/call" if seen.mode == "quiet" => json!({"content": []}),
                        "tools/call" => match tool.as_str() {
                            "search_entities" => {
                                let query = message["params"]["arguments"]["query"].as_str().unwrap_or("");
                                if query == "where is the nothing" {
                                    json!({"content": [{"type": "text", "text": json!({"query": query, "count": 0, "entities": []}).to_string()}]})
                                } else {
                                    json!({"content": [{"type": "text", "text": json!({"query": query, "count": 1,
                                        "entities": [{"name": "obi-e2e/fixture-app#src/index.js", "note": leak}]}).to_string()}]})
                                }
                            }
                            // The shapes of mcp_service/app/tools/{repo_sessions,session_context,
                            // session_digest}.py.
                            "repo_sessions" => {
                                let ids = match seen.mode {
                                    "leaky" => vec![made_up_key()],
                                    "slow" => (1..=4).map(|n| format!("agent_session:g{n}")).collect(),
                                    _ => vec!["agent_session:g1".to_owned()],
                                };
                                let sessions: Vec<Value> = ids.iter().map(|id| json!({"session_id": id, "title": "Earlier work", "edge_type": "worked_on"})).collect();
                                json!({"content": [{"type": "text", "text": json!({"target": {"id": "r1", "name": "obi-e2e/fixture-app", "type": "repo"},
                                    "count": sessions.len(), "sessions": sessions,
                                    "truncated": false, "has_more": false, "next_cursor": null}).to_string()}]})
                            }
                            "session_context" => {
                                if seen.mode == "slow" {
                                    std::thread::sleep(std::time::Duration::from_secs(9));
                                }
                                json!({"content": [{"type": "text", "text": json!({"id": "agent_session:g1",
                                "natural_key": "session:acme/5b0e7c1a-9d2f-4c3e-8a1b-2f3e4d5c6b7a", "title": "Earlier work",
                                "digest_summary": {"decisions": 1}, "worked_on": [], "produced": [], "references": []}).to_string()}]})
                            }
                            "session_digest" => {
                                json!({"content": [{"type": "text", "text": json!({"session_id": "5b0e7c1a-9d2f-4c3e-8a1b-2f3e4d5c6b7a",
                                    "audience": "org", "has_digest": true, "digest": {"schema_version": 1,
                                    "projects": [{"repo": "obi-e2e/fixture-app", "paths_modified": ["config.txt"]}],
                                    "decisions": [{"statement": "Keep retries at 4: the gateway rate-limits a fifth.", "status": "decided"},
                                        {"statement": "Retries were 3 before.", "status": "reverted"}]}}).to_string()}]})
                            }
                            "get_entity" => json!({"content": [{"type": "text", "text": "Entity not found"}], "isError": true}),
                            "save_session" => {
                                let arguments = message["params"]["arguments"].clone();
                                let session = if arguments["client"] == "oby" { "sess-gaps-1" } else { "sess-agent-1" };
                                seen.saved.push(arguments);
                                json!({"content": [{"type": "text", "text": json!({"session_id": session}).to_string()}]})
                            }
                            _ => json!({"content": [{"type": "text", "text": "ran"}]}),
                        },
                        _ => json!({}),
                    };
                    if message["method"]
                        .as_str()
                        .is_some_and(|m| m.starts_with("notifications/"))
                    {
                        reply(&mut stream, "202 Accepted", "application/json", "");
                    } else {
                        reply(
                            &mut stream,
                            "200 OK",
                            "application/json",
                            &json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string(),
                        );
                    }
                }
                _ => reply(&mut stream, "404 Not Found", "text/plain", "no"),
            }
        }
    });
    (base, seen)
}

/// `text` as a form or query string writes it.
pub fn form_encode(text: &str) -> String {
    reqwest::Url::parse_with_params("http://x/", &[("v", text)])
        .unwrap()
        .query()
        .unwrap()
        .trim_start_matches("v=")
        .to_owned()
}

/// A GET to `url` (the browser following the sign-in), its status and body.
pub fn browse(url: &str) -> (u16, String) {
    let url = reqwest::Url::parse(url).unwrap();
    let mut stream =
        std::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap())).unwrap();
    let target = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    };
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        url.host_str().unwrap(),
        url.port().unwrap()
    )
    .unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    let status = answer
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, answer)
}

/// Signs `obid` in to the fake Tetrix at `base`, as the browser would.
pub fn sign_in(obid: &Obid, base: &str) {
    sign_in_at(obid, &format!("{base}/mcp"));
}

/// Signs `obid` in to the fake Tetrix whose MCP server is at `url`, as the browser would.
pub fn sign_in_at(obid: &Obid, url: &str) {
    let started = call(
        obid,
        "POST",
        "/v1/tetrix/sign-in",
        Some(&json!({"url": url})),
    );
    assert_eq!(started.status, 200, "{}", started.text);
    let page = reqwest::Url::parse(started.body["url"].as_str().unwrap()).unwrap();
    let query: std::collections::HashMap<String, String> = page
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(browse(page.as_str()).0, 200);
    let (status, said) = browse(&format!(
        "{}?code=the-code&state={}",
        query["redirect_uri"], query["state"]
    ));
    assert_eq!(status, 200, "{said}");
    within(10, "signed in", || {
        (call(obid, "GET", "/v1/tetrix", None).body["connected"] == true).then_some(())
    });
}
