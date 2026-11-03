//! Continue with ChatGPT (docs/contracts/chatgpt-sign-in.md): a Codex account signed in to the
//! person's ChatGPT plan with OpenAI's Sign in with ChatGPT for open-source and local apps. obid is
//! the OAuth client (a public client, PKCE, a loopback redirect on 127.0.0.1), the one provider it
//! signs in to itself (the owner's decision of 2026-11-02). It owns the listener, the code's
//! exchange, the id token's check, storage, refresh and revocation; the apps open the page and show
//! how it stands.
//!
//! - **Where the tokens are:** one vault entry per account, `oby.chatgpt.<id>`, a JSON [`Record`].
//!   The vault learns its tokens as forms of that one name, so every answer, log line and push has
//!   them taken out (`crate::vault`). The id token is checked and dropped.
//! - **What the rest of obid sees:** the [`Marker`] in the account's folder, no secret in it, which
//!   `crate::jobs::{has_login, signed_in, login_problem, login_note}` branch on. No box ever gets
//!   that folder (#655).
//! - **Where the tokens go:** only to OpenAI. No run gets one: Codex reaches OpenAI through obid's
//!   relay (crate::chatgpt_relay) with the job's broker token.
//! - **Refresh can't lose a rotated token:** each account's refresh is one spawned task callers wait
//!   on, so stopping a job can't cancel it, and its record is written before any caller gets the
//!   token. A failed write stays in memory and is tried again, with no other refresh meanwhile.
//!
//! Every call uses `redirect::Policy::none()` and OpenAI's fixed endpoints ([`Endpoints`]): a 307
//! would otherwise replay a refresh token. Only a debug build can be pointed elsewhere, at a
//! stand-in on loopback, and never from `config.json`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use axum::routing::get;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{oneshot, watch};

use crate::accounts::Live;
use crate::vault::Vault;

/// What every entry's name starts with. No connection can name it (`crate::connections`: `oby.`).
pub const VAULT_PREFIX: &str = "oby.chatgpt.";
/// The marker's name in the account's folder.
pub const MARKER: &str = "chatgpt.json";
/// The scope that lets a token use the person's plan. A valid id token alone doesn't.
pub const DIRECT_SCOPE: &str = "chatgpt.tokens.use.direct";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
/// First-time registration's entry point, never a client id to keep.
const REGISTER: &str = "dynamic_agent_client";
/// The app's name, on registration and as Codex's `clientInfo.name`, which OpenAI asks be the same.
pub const AGENT_NAME: &str = "Oby";
/// The redirect's path: OpenAI lets only the port vary.
const CALLBACK: &str = "/auth/callback";
/// A refresh token lasts 30 days from the last refresh.
pub const REFRESH_LASTS: i64 = 30 * 24 * 3600;
/// A token this close to its end is refreshed before it's used.
const REFRESH_BEFORE: i64 = 5 * 60;
/// How long a sign-in waits for the person (the app stops waiting then too).
const SIGN_IN_LASTS: Duration = Duration::from_secs(15 * 60);
/// Tries at a refresh after a network failure or a 5xx, the same refresh token each time.
const REFRESH_WAITS: [u64; 3] = [0, 1, 3];
/// Refresh answers OpenAI lists as an unusable refresh token.
const TERMINAL: &[&str] = &[
    "invalid_grant",
    "invalid_refresh_token",
    "token_expired",
    "refresh_token_expired",
    "refresh_token_invalidated",
    "refresh_token_reused",
];

/// Where obid talks to OpenAI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub issuer: String,
    pub resource: String,
}

impl Endpoints {
    /// OpenAI's. Nothing but this exists in a release build.
    pub fn openai() -> Endpoints {
        Endpoints {
            issuer: "https://auth.openai.com".into(),
            resource: "https://api.openai.com/v1".into(),
        }
    }

    /// OpenAI's; a debug build's tests may name a stand-in on 127.0.0.1 in the environment. Compiled
    /// out of a release build, and never read from the configuration.
    pub fn here() -> Endpoints {
        #[cfg(debug_assertions)]
        if let Some(origin) = std::env::var("OBID_TEST_CHATGPT_ORIGIN")
            .ok()
            .filter(|o| test_origin(o))
        {
            return Endpoints {
                issuer: origin.clone(),
                resource: format!("{origin}/v1"),
            };
        }
        Endpoints::openai()
    }
}

/// `http://127.0.0.1:<port>`, nothing else: the only stand-in a test may name.
#[cfg(debug_assertions)]
fn test_origin(origin: &str) -> bool {
    origin
        .strip_prefix("http://127.0.0.1:")
        .is_some_and(|port| port.parse::<u16>().is_ok())
}

/// The vault entry: one account's registration, and its tokens while it has them. Replaced whole,
/// so tokens, expiry and scopes change together, as OpenAI asks. Tokens cleared keep the record, so
/// the issued client id survives a sign-in again.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub client_id: String,
    pub sub: String,
    pub host_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Tokens>,
    /// Access tokens replaced but not yet ended: still scrubbed, also after a restart.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous: Vec<OldToken>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub scopes: Vec<String>,
    pub expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earliest_refresh_at: Option<i64>,
    pub refreshed_at: i64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OldToken {
    pub access: String,
    pub expires_at: i64,
}

impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never a token, whatever prints this.
        f.write_str("chatgpt::Record")
    }
}

impl Tokens {
    pub fn plan_use(&self) -> bool {
        self.scopes.iter().any(|s| s == DIRECT_SCOPE)
    }
}

/// The tokens a record's JSON holds, each taken out wherever it's written (crate::vault).
pub fn secret_forms(value: &str) -> Vec<String> {
    let Ok(record) = serde_json::from_str::<Record>(value) else {
        return Vec::new();
    };
    let mut forms: Vec<String> = record.previous.into_iter().map(|old| old.access).collect();
    if let Some(tokens) = record.tokens {
        forms.push(tokens.access);
        forms.push(tokens.refresh);
    }
    forms
}

/// What the account's folder says of its ChatGPT login: no secret, nothing but obid reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Tokens are held. A terminal refresh answer turns it off, keeping the marker.
    pub signed: bool,
    /// The grant has [`DIRECT_SCOPE`].
    pub plan_use: bool,
    pub expires_at: i64,
    pub refreshed_at: i64,
}

impl Marker {
    fn of(record: &Record) -> Marker {
        let tokens = record.tokens.as_ref();
        Marker {
            email: record.email.clone(),
            signed: tokens.is_some(),
            plan_use: tokens.is_some_and(Tokens::plan_use),
            expires_at: tokens.map_or(0, |t| t.expires_at),
            refreshed_at: tokens.map_or(0, |t| t.refreshed_at),
        }
    }

    /// Past 30 days from the last refresh: only a new sign-in brings it back.
    pub fn too_old(&self, now: i64) -> bool {
        now >= self.refreshed_at.saturating_add(REFRESH_LASTS)
    }

    /// A run can use it. Depends on the refresh token, never on the access token's expiry.
    pub fn usable(&self, now: i64) -> bool {
        self.signed && self.plan_use && !self.too_old(now)
    }
}

/// The folder's marker: a regular file only, never through a link.
pub fn marker_in(dir: &Path) -> Option<Marker> {
    let file = dir.join(MARKER);
    if !std::fs::symlink_metadata(&file).is_ok_and(|m| m.is_file()) {
        return None;
    }
    serde_json::from_slice(&std::fs::read(&file).ok()?).ok()
}

/// The account's marker: an account signed in with ChatGPT has one.
pub fn marker_of(account: &crate::config::Account) -> Option<Marker> {
    marker_in(account.config_dir.as_deref()?)
}

/// Why no token came.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The login ended: only a new sign-in brings it back.
    SignIn(String),
    /// For now: nothing was lost, and a later try may work.
    Later(String),
    /// OpenAI doesn't accept the client: Oby's configuration, no sign-in cures it.
    Client(String),
    /// Signed in without the plan's scope.
    PlanOff(String),
}

impl Refused {
    pub fn words(&self) -> &str {
        match self {
            Refused::SignIn(w) | Refused::Later(w) | Refused::Client(w) | Refused::PlanOff(w) => w,
        }
    }
}

pub const SIGN_IN_AGAIN: &str = "This ChatGPT login has ended: use Continue with ChatGPT again.";
const PLAN_OFF: &str =
    "ChatGPT plan use isn't on for this account: use Continue with ChatGPT and allow it.";
const NOT_NOW: &str = "ChatGPT can't renew access right now. Your login is kept: Oby tries again.";
const BAD_CLIENT: &str = "ChatGPT doesn't accept Oby's registration for this account. Signing in again won't change that.";

/// What's kept in memory, behind one lock: removal and a refresh's end both take it, so nothing is
/// written for an account on its way out.
#[derive(Default)]
struct Memory {
    /// A record a failed vault write left: the only copy of a rotated refresh token.
    unwritten: HashMap<String, Record>,
    refreshing: HashMap<String, watch::Receiver<Option<Result<String, Refused>>>>,
    /// Accounts being removed or switched away: an exchange or refresh in flight writes nothing.
    gone: HashSet<String>,
    /// Why each account's last sign-in didn't take, till its next try.
    problems: HashMap<String, String>,
    /// An issued client whose code failed: the fresh authorization reuses it.
    retry_client: HashMap<String, String>,
}

/// obid's ChatGPT sign-ins and tokens.
pub struct ChatGpt {
    ends: Endpoints,
    vault: Arc<Vault>,
    agents: Live,
    home: PathBuf,
    http: reqwest::Client,
    state: Mutex<Memory>,
}

impl ChatGpt {
    pub fn open(home: &Path, vault: Arc<Vault>, agents: Live, ends: Endpoints) -> Arc<ChatGpt> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client");
        Arc::new(ChatGpt {
            ends,
            vault,
            agents,
            home: home.to_path_buf(),
            http,
            state: Mutex::default(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Memory> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn name(account: &str) -> String {
        format!("{VAULT_PREFIX}{account}")
    }

    /// The account's record as it is now: the one a failed write left, else the vault's.
    fn record(&self, account: &str) -> Result<Option<Record>, String> {
        if let Some(record) = self.lock().unwritten.get(account) {
            return Ok(Some(record.clone()));
        }
        Ok(self
            .vault
            .value(&Self::name(account))?
            .and_then(|text| serde_json::from_str(&text).ok()))
    }

    /// Writes `record`, then the marker, unless the account is on its way out. A failed write stays
    /// in memory to be tried again; the error says so.
    fn keep(&self, account: &str, record: &Record, dir: &Path) -> Result<Kept, String> {
        let mut state = self.lock();
        if state.gone.contains(account) {
            return Ok(Kept::Gone);
        }
        let text = serde_json::to_string(record).map_err(|e| e.to_string())?;
        if let Err(why) = self.vault.add(&text, &Self::name(account)) {
            state.unwritten.insert(account.to_owned(), record.clone());
            return Err(why);
        }
        state.unwritten.remove(account);
        write_marker(dir, &Marker::of(record));
        Ok(Kept::Written)
    }

    /// Why the account's last sign-in didn't take, till its next try.
    pub fn problem(&self, account: &str) -> Option<String> {
        self.lock().problems.get(account).cloned()
    }

    fn say_problem(&self, account: &str, why: &str) {
        self.lock()
            .problems
            .insert(account.to_owned(), why.to_owned());
    }

    fn folder(&self, account: &str) -> Option<PathBuf> {
        crate::accounts::find(&self.agents.get(), account)
            .and_then(|(tool, account)| (tool == crate::agent::Tool::Codex).then_some(account))
            .and_then(|account| account.config_dir)
    }

    /// This Mac's `ext_agent_host_id`: chosen once, before its first sign-in, and kept. An opaque
    /// id, no credential.
    fn host_id(&self) -> Result<String, String> {
        let file = self.home.join("chatgpt-host");
        if let Ok(text) = std::fs::read_to_string(&file)
            && text.trim().starts_with("urn:uuid:")
        {
            return Ok(text.trim().to_owned());
        }
        let id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
        crate::store::write_private(&file, id.as_bytes())
            .map_err(|e| format!("obid can't keep this Mac's id: {e}"))?;
        Ok(id)
    }

    // -----------------------------------------------------------------------------------------
    // Signing in

    /// Starts a sign-in of `account`: its listener runs before the page opens, and keeps nothing
    /// until the id token checks out. The page to open, and what ends it.
    pub async fn begin(self: &Arc<Self>, account: &str) -> Result<Flow, String> {
        let prior = self.record(account)?;
        let host_id = match &prior {
            // OpenAI asks for the same host id on the same host.
            Some(record) if !record.host_id.is_empty() => record.host_id.clone(),
            _ => self.host_id()?,
        };
        let client = prior
            .as_ref()
            .map(|r| r.client_id.clone())
            .or_else(|| self.lock().retry_client.get(account).cloned());
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|e| format!("obid couldn't listen for ChatGPT's answer: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("obid couldn't listen for ChatGPT's answer: {e}"))?
            .port();
        let redirect = format!("http://127.0.0.1:{port}{CALLBACK}");
        let (state, nonce, verifier) = (random(32), random(32), random(32));
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(obi_protocol::keys::sha256(&verifier));
        let mut page = reqwest::Url::parse(&format!("{}/api/accounts/authorize", self.ends.issuer))
            .map_err(|e| e.to_string())?;
        {
            let mut query = page.query_pairs_mut();
            query.append_pair("client_id", client.as_deref().unwrap_or(REGISTER));
            // Only on a first registration, OpenAI says.
            if client.is_none() {
                query.append_pair("agent_name_hint", AGENT_NAME);
            }
            query.append_pair("ext_agent_host_id", &host_id);
            if let Some(email) = prior.as_ref().and_then(|r| r.email.as_deref()) {
                query.append_pair("login_hint", email);
            }
            // Turning plan use on after an earlier no: the one case OpenAI asks to force consent.
            if prior
                .as_ref()
                .and_then(|r| r.tokens.as_ref())
                .is_some_and(|t| !t.plan_use())
            {
                query.append_pair("prompt", "consent");
            }
            query
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", &redirect)
                .append_pair("scope", SCOPES)
                .append_pair("resource", &self.ends.resource)
                .append_pair("state", &state)
                .append_pair("nonce", &nonce)
                .append_pair("code_challenge_method", "S256")
                .append_pair("code_challenge", &challenge);
        }
        let (told, outcome) = oneshot::channel();
        let pending = Arc::new(Pending {
            account: account.to_owned(),
            port,
            state,
            nonce,
            verifier,
            redirect,
            client,
            prior,
            host_id,
            taken: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            told: Mutex::new(Some(told)),
            end: tokio::sync::Notify::new(),
        });
        {
            let mut state = self.lock();
            state.problems.remove(account);
            // One change of an account at a time (`SignIns::lock_for`): a removal that failed
            // half way doesn't hold this one back.
            state.gone.remove(account);
        }
        let (me, running) = (self.clone(), pending.clone());
        tokio::spawn(async move {
            let app = Router::new()
                .route(CALLBACK, get(callback))
                .with_state((me, running.clone()));
            let ended = async move {
                tokio::select! {
                    () = running.end.notified() => {}
                    () = tokio::time::sleep(SIGN_IN_LASTS) => {}
                }
            };
            if let Err(e) = axum::serve(listener, app)
                .with_graceful_shutdown(ended)
                .await
            {
                tracing::warn!("ChatGPT's sign-in page stopped: {e}");
            }
        });
        Ok(Flow {
            url: page.to_string(),
            pending,
            outcome,
        })
    }

    /// One request to the token endpoint: its status and its JSON.
    async fn token_request(&self, form: &[(&str, &str)]) -> Result<(StatusCode, Value), String> {
        let response = self
            .http
            .post(format!("{}/api/accounts/oauth/token", self.ends.issuer))
            .form(form)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        let status = response.status();
        Ok((status, crate::tetrix::small_json(response).await))
    }

    async fn jwks(&self) -> Result<Value, String> {
        let response = self
            .http
            .get(format!("{}/.well-known/jwks.json", self.ends.issuer))
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if !response.status().is_success() {
            return Err(format!("its keys answered {}", response.status().as_u16()));
        }
        Ok(crate::tetrix::small_json(response).await)
    }

    /// What the callback's answer comes to. `Err` is what the row says.
    async fn finish(
        &self,
        pending: &Pending,
        params: &HashMap<String, String>,
    ) -> Result<(), String> {
        let account = &pending.account;
        if params.contains_key("error") {
            return Err("ChatGPT sign-in wasn't allowed. Try again when you're ready.".into());
        }
        // A returning sign-in may leave it out; one it does give must be the same.
        let given = params.get("client_id").map(String::as_str);
        let client = match (&pending.client, given) {
            (Some(saved), Some(given)) if saved != given => {
                return Err(
                    "ChatGPT answered for another registration, so nothing was replaced.".into(),
                );
            }
            (Some(saved), _) => saved.clone(),
            (None, Some(given)) if !given.is_empty() && given != REGISTER => given.to_owned(),
            (None, _) => {
                return Err("ChatGPT didn't finish registering Oby. Try again.".into());
            }
        };
        if let Some(other) = self.account_with_client(&client, account) {
            return Err(format!(
                "That ChatGPT registration is already on {other}, so nothing was replaced."
            ));
        }
        let Some(code) = params.get("code").filter(|c| !c.is_empty()) else {
            return Err("ChatGPT sent no code. Try again.".into());
        };
        // The keys first: nothing that can fail for a dull reason after the code is spent.
        let keys = self
            .jwks()
            .await
            .map_err(|_| "ChatGPT's keys couldn't be read. Try again.".to_owned())?;
        let (status, answer) = self
            .token_request(&[
                ("grant_type", "authorization_code"),
                ("client_id", &client),
                ("code", code),
                ("code_verifier", &pending.verifier),
                ("redirect_uri", &pending.redirect),
                ("resource", &self.ends.resource),
            ])
            .await
            .map_err(|_| "ChatGPT couldn't be reached. Try again.".to_owned())?;
        if !status.is_success() {
            // A spent code: discarded, and the fresh try keeps the client it issued.
            if error_code(&answer).as_deref() == Some("invalid_grant") {
                self.lock()
                    .retry_client
                    .insert(account.clone(), client.clone());
            }
            return Err("ChatGPT couldn't finish this sign-in. Try again.".into());
        }
        let refresh = answer["refresh_token"].as_str().unwrap_or("").to_owned();
        let drop_it = async |why: String| {
            self.revoke_best_effort(&client, &refresh).await;
            why
        };
        let Some(id_token) = answer["id_token"].as_str() else {
            return Err(drop_it("ChatGPT sent no identity. Try again.".into()).await);
        };
        let who = match verify_id_token(
            id_token,
            &keys,
            &self.ends.issuer,
            &client,
            Some(&pending.nonce),
            obi_protocol::time::now(),
        ) {
            Ok(who) => who,
            Err(why) => {
                tracing::warn!("account {account}: ChatGPT's id token refused: {why}");
                return Err(drop_it(
                    "ChatGPT's identity couldn't be verified. Nothing was kept.".into(),
                )
                .await);
            }
        };
        if let Some(prior) = &pending.prior
            && prior.sub != who.sub
        {
            return Err(drop_it(
                "That's a different ChatGPT login: add it as another account. Nothing was replaced.".into(),
            )
            .await);
        }
        let now = obi_protocol::time::now();
        let Some(tokens) = tokens_from(&answer, now, None) else {
            return Err(drop_it("ChatGPT's answer was incomplete. Try again.".into()).await);
        };
        let mut record = Record {
            client_id: client.clone(),
            sub: who.sub,
            host_id: pending.host_id.clone(),
            email: who.email,
            previous: pending
                .prior
                .as_ref()
                .map(|p| carried_over(p, now))
                .unwrap_or_default(),
            tokens: Some(tokens),
        };
        // The access token it replaces stays scrubbed until it ends. The refresh token isn't
        // revoked: whether that would end this new session is spike S8's, not known yet, and
        // Tetrix's server does exactly that.
        // ponytail: same `sub`, so left to expire; revoke it here once S8 says it's safe.
        if let Some(old) = pending.prior.as_ref().and_then(|p| p.tokens.as_ref()) {
            record.previous.push(OldToken {
                access: old.access.clone(),
                expires_at: old.expires_at,
            });
        }
        let Some(dir) = self.folder(account) else {
            return Err(drop_it("That account has gone.".into()).await);
        };
        if pending.cancelled.load(Ordering::SeqCst) {
            return Err(drop_it("That sign-in was stopped.".into()).await);
        }
        match self.keep(account, &record, &dir) {
            Ok(Kept::Written) => {}
            Ok(Kept::Gone) => return Err(drop_it("That account has gone.".into()).await),
            // Nothing was running on it yet: tried again by signing in, not from memory.
            Err(why) => {
                tracing::error!("account {account}: its ChatGPT login wasn't written: {why}");
                self.lock().unwritten.remove(account);
                return Err(drop_it("Oby couldn't keep this login. Try again.".into()).await);
            }
        }
        self.lock().retry_client.remove(account);
        // A device login it replaces goes only now that this one took (CHATGPT-16).
        let _ = std::fs::remove_file(dir.join("auth.json"));
        Ok(())
    }

    /// Another account holding the same issued client: never two accounts on one registration.
    fn account_with_client(&self, client: &str, except: &str) -> Option<String> {
        let names = self.vault.names().ok()?;
        names.into_iter().find_map(|name| {
            let id = name.strip_prefix(VAULT_PREFIX)?;
            if id == except {
                return None;
            }
            let record: Record = serde_json::from_str(&self.vault.value(&name).ok()??).ok()?;
            (record.client_id == client).then(|| {
                crate::accounts::find(&self.agents.get(), id)
                    .map_or_else(|| id.to_owned(), |(_, a)| a.name)
            })
        })
    }

    // -----------------------------------------------------------------------------------------
    // Tokens

    /// The access token to use now, refreshed first when it's near its end.
    pub async fn access(self: &Arc<Self>, account: &str) -> Result<String, Refused> {
        let now = obi_protocol::time::now();
        let record = self.current(account)?;
        let tokens = record.tokens.as_ref().ok_or_else(sign_in_again)?;
        if !tokens.plan_use() {
            return Err(Refused::PlanOff(PLAN_OFF.into()));
        }
        if !self.lock().unwritten.contains_key(account) && tokens.expires_at > now + REFRESH_BEFORE
        {
            return Ok(tokens.access.clone());
        }
        // Not before OpenAI says it may be refreshed, while what it has still works.
        if tokens.earliest_refresh_at.is_some_and(|at| at > now) && tokens.expires_at > now {
            return Ok(tokens.access.clone());
        }
        self.refreshed(account).await
    }

    /// A token OpenAI refused as ended: a new one, unless another caller has replaced it already.
    pub async fn replacing(
        self: &Arc<Self>,
        account: &str,
        stale: &str,
    ) -> Result<String, Refused> {
        let record = self.current(account)?;
        let tokens = record.tokens.as_ref().ok_or_else(sign_in_again)?;
        if tokens.access != stale {
            return Ok(tokens.access.clone());
        }
        self.refreshed(account).await
    }

    fn current(&self, account: &str) -> Result<Record, Refused> {
        self.record(account)
            .map_err(|why| Refused::Later(format!("Oby's vault can't be read: {why}")))?
            .ok_or_else(sign_in_again)
    }

    /// Waits for the account's one refresh, starting it if none is under way. Spawned, so a caller
    /// going away (a job stopped, a request dropped) can't cancel it between OpenAI's answer and
    /// the write.
    async fn refreshed(self: &Arc<Self>, account: &str) -> Result<String, Refused> {
        let mut waiting = {
            let mut state = self.lock();
            match state.refreshing.get(account) {
                Some(running) => running.clone(),
                None => {
                    let (tell, waiting) = watch::channel(None);
                    state.refreshing.insert(account.to_owned(), waiting.clone());
                    let (me, id) = (self.clone(), account.to_owned());
                    tokio::spawn(async move {
                        let result = me.refresh(&id).await;
                        me.lock().refreshing.remove(&id);
                        let _ = tell.send(Some(result));
                    });
                    waiting
                }
            }
        };
        let done = waiting
            .wait_for(Option::is_some)
            .await
            .map_err(|_| Refused::Later(NOT_NOW.into()))?;
        done.clone()
            .unwrap_or_else(|| Err(Refused::Later(NOT_NOW.into())))
    }

    /// One refresh, start to end. Never runs twice at once for an account ([`Self::refreshed`]).
    async fn refresh(&self, account: &str) -> Result<String, Refused> {
        let dir = self.folder(account).ok_or_else(sign_in_again)?;
        // What a failed write left is the only copy of a rotated token: written, never refreshed
        // over.
        let left = self.lock().unwritten.get(account).cloned();
        if let Some(left) = left {
            return match self.keep(account, &left, &dir) {
                Ok(Kept::Written) => left.tokens.map(|t| t.access).ok_or_else(sign_in_again),
                Ok(Kept::Gone) => {
                    self.revoke_kept(&left).await;
                    Err(sign_in_again())
                }
                Err(_) => Err(Refused::Later(NOT_NOW.into())),
            };
        }
        let record = self.current(account)?;
        let tokens = record.tokens.clone().ok_or_else(sign_in_again)?;
        for wait in REFRESH_WAITS {
            tokio::time::sleep(Duration::from_secs(wait)).await;
            let answered = self
                .token_request(&[
                    ("grant_type", "refresh_token"),
                    ("client_id", &record.client_id),
                    ("refresh_token", &tokens.refresh),
                    ("resource", &self.ends.resource),
                ])
                .await;
            let (status, answer) = match answered {
                Ok(answered) => answered,
                // A timeout may have rotated it at OpenAI; the same token again is the only try.
                Err(why) => {
                    tracing::info!("account {account}: ChatGPT's refresh didn't arrive: {why}");
                    continue;
                }
            };
            if status.is_success() {
                return self
                    .refreshed_with(account, &dir, record, &tokens, &answer)
                    .await;
            }
            if status.is_server_error() {
                continue;
            }
            let code = error_code(&answer).unwrap_or_default();
            if code == "invalid_client" {
                return Err(Refused::Client(BAD_CLIENT.into()));
            }
            if TERMINAL.contains(&code.as_str()) {
                return self.ended(account, &dir, &tokens, &code);
            }
            tracing::warn!("account {account}: ChatGPT's refresh answered {status} {code}");
            return Err(Refused::Later(NOT_NOW.into()));
        }
        Err(Refused::Later(NOT_NOW.into()))
    }

    /// OpenAI's new tokens: kept before anyone gets them.
    async fn refreshed_with(
        &self,
        account: &str,
        dir: &Path,
        mut record: Record,
        old: &Tokens,
        answer: &Value,
    ) -> Result<String, Refused> {
        let now = obi_protocol::time::now();
        let Some(new) = tokens_from(answer, now, Some(old)) else {
            tracing::warn!("account {account}: ChatGPT's refresh answer was incomplete");
            return Err(Refused::Later(NOT_NOW.into()));
        };
        record.previous = carried_over(&record, now);
        record.previous.push(OldToken {
            access: old.access.clone(),
            expires_at: old.expires_at,
        });
        let access = new.access.clone();
        record.tokens = Some(new);
        match self.keep(account, &record, dir) {
            Ok(Kept::Written) => Ok(access),
            // Removed meanwhile: nothing written, and what it was given goes back.
            Ok(Kept::Gone) => {
                self.revoke_kept(&record).await;
                Err(sign_in_again())
            }
            Err(why) => {
                tracing::error!(
                    "account {account}: its renewed ChatGPT login isn't written yet: {why}"
                );
                Err(Refused::Later(NOT_NOW.into()))
            }
        }
    }

    /// A terminal refresh answer: tokens cleared, the record kept so its client id survives. For a
    /// token "reused", only if the vault still holds the one that was sent: another obid may have
    /// rotated it.
    fn ended(
        &self,
        account: &str,
        dir: &Path,
        sent: &Tokens,
        code: &str,
    ) -> Result<String, Refused> {
        let mut record = self.current(account)?;
        if code == "refresh_token_reused"
            && let Some(now) = &record.tokens
            && now.refresh != sent.refresh
        {
            return Ok(now.access.clone());
        }
        tracing::warn!("account {account}: ChatGPT's login ended ({code})");
        let at = obi_protocol::time::now();
        record.previous = carried_over(&record, at);
        if let Some(old) = record.tokens.take() {
            record.previous.push(OldToken {
                access: old.access,
                expires_at: old.expires_at,
            });
        }
        if let Err(why) = self.keep(account, &record, dir) {
            tracing::error!("account {account}: its ended ChatGPT login stays in the vault: {why}");
        }
        Err(sign_in_again())
    }

    // -----------------------------------------------------------------------------------------
    // Ending

    /// Asks OpenAI to end the session: whether it confirmed (only a 200 counts). A network failure
    /// or a 5xx gets at most three tries within 10 seconds.
    async fn revoke(&self, client: &str, refresh: &str) -> bool {
        let tried = async {
            for wait in [0, 1, 3] {
                tokio::time::sleep(Duration::from_secs(wait)).await;
                let Some(at) = self.revocation_endpoint().await else {
                    continue;
                };
                let form = [
                    ("token", refresh),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", client),
                ];
                match self.http.post(at).form(&form).send().await {
                    Ok(answer) if answer.status() == StatusCode::OK => return true,
                    Ok(answer) if answer.status().is_server_error() => {}
                    Ok(answer) => {
                        tracing::warn!("ChatGPT's revoke answered {}", answer.status());
                        return false;
                    }
                    Err(e) => tracing::info!("ChatGPT's revoke failed: {}", e.without_url()),
                }
            }
            false
        };
        tokio::time::timeout(Duration::from_secs(10), tried)
            .await
            .unwrap_or(false)
    }

    /// `revocation_endpoint` from OpenAI's configuration, only when it's on the issuer's own origin.
    async fn revocation_endpoint(&self) -> Option<String> {
        let response = self
            .http
            .get(format!(
                "{}/.well-known/openid-configuration",
                self.ends.issuer
            ))
            .send()
            .await
            .ok()?;
        let config = crate::tetrix::small_json(response).await;
        let at = config["revocation_endpoint"].as_str()?;
        let origin = format!("{}/", self.ends.issuer);
        at.starts_with(&origin).then(|| at.to_owned())
    }

    async fn revoke_best_effort(&self, client: &str, refresh: &str) {
        if !refresh.is_empty() && !self.revoke(client, refresh).await {
            tracing::warn!("ChatGPT didn't confirm taking back tokens obid won't keep");
        }
    }

    async fn revoke_kept(&self, record: &Record) {
        if let Some(tokens) = &record.tokens {
            self.revoke_best_effort(&record.client_id, &tokens.refresh)
                .await;
        }
    }

    /// Takes the account's login off: revoke, delete the entry. Anything in flight writes nothing
    /// afterwards, and gives back what it was given. Whether OpenAI confirmed the revoke (`true`
    /// when there was nothing to revoke).
    pub async fn remove(&self, account: &str) -> bool {
        self.lock().gone.insert(account.to_owned());
        let record = self.record(account).ok().flatten();
        let confirmed = match record
            .as_ref()
            .and_then(|r| r.tokens.as_ref().map(|t| (r, t)))
        {
            Some((record, tokens)) => self.revoke(&record.client_id, &tokens.refresh).await,
            None => true,
        };
        let mut state = self.lock();
        state.unwritten.remove(account);
        state.problems.remove(account);
        state.retry_client.remove(account);
        if record.is_some()
            && let Err(why) = self.vault.remove(&Self::name(account))
        {
            tracing::warn!("account {account}: its ChatGPT entry stayed in the vault: {why}");
        }
        confirmed
    }

    /// A device login took on this account (CHATGPT-16): the ChatGPT one goes, and the account can
    /// sign in with ChatGPT again.
    pub async fn replaced_by_device_login(&self, account: &str) {
        let had = self
            .folder(account)
            .and_then(|dir| marker_in(&dir).map(|_| dir));
        if let Some(dir) = had {
            if !self.remove(account).await {
                tracing::warn!("account {account}: ChatGPT didn't confirm ending the old session");
            }
            let _ = std::fs::remove_file(dir.join(MARKER));
            self.lock().gone.remove(account);
        }
    }

    /// As obid starts: an entry whose account has gone (a crash during removal) is revoked and
    /// deleted, and a marker whose entry has gone stops saying it's signed in.
    pub async fn sweep(&self) {
        let Ok(names) = self.vault.names() else {
            return;
        };
        for name in names {
            let Some(id) = name.strip_prefix(VAULT_PREFIX) else {
                continue;
            };
            if self.folder(id).is_none() {
                tracing::warn!("a ChatGPT login's account has gone: taking it back");
                let _ = self.remove(id).await;
            }
        }
        let Some(codex) = self.agents.get().codex else {
            return;
        };
        for account in codex.accounts {
            if let Some(dir) = &account.config_dir
                && let Some(mut marker) = marker_in(dir)
                && marker.signed
                && matches!(self.vault.value(&Self::name(&account.id)), Ok(None))
            {
                marker.signed = false;
                write_marker(dir, &marker);
            }
        }
    }
}

/// Whether a write took.
enum Kept {
    Written,
    Gone,
}

fn sign_in_again() -> Refused {
    Refused::SignIn(SIGN_IN_AGAIN.into())
}

fn write_marker(dir: &Path, marker: &Marker) {
    match serde_json::to_vec(marker) {
        Ok(bytes) => {
            if let Err(e) = crate::store::write_private(&dir.join(MARKER), &bytes) {
                tracing::error!("obid can't write {}: {e}", dir.join(MARKER).display());
            }
        }
        Err(e) => tracing::error!("obid can't write ChatGPT's marker: {e}"),
    }
}

/// Replaced access tokens still alive.
fn carried_over(record: &Record, now: i64) -> Vec<OldToken> {
    let mut left: Vec<OldToken> = record
        .previous
        .iter()
        .filter(|old| old.expires_at > now)
        .cloned()
        .collect();
    // ponytail: one a refresh an hour at most, so this stays tiny; the cap only guards the vault's
    // 64 KB against a server refreshing wildly.
    let extra = left.len().saturating_sub(4);
    left.drain(..extra);
    left
}

/// The tokens in a token answer, or none when it's short of what it must have. On a refresh, what
/// it leaves out stays (`old`); a grant with plan use must come with a refresh token.
fn tokens_from(answer: &Value, now: i64, old: Option<&Tokens>) -> Option<Tokens> {
    let access = answer["access_token"].as_str().filter(|t| !t.is_empty())?;
    let expires_in = answer["expires_in"]
        .as_i64()
        .filter(|s| (1..=86_400).contains(s))?;
    let scopes: Vec<String> = match (answer["scope"].as_str(), old) {
        (Some(scope), _) => scope.split_whitespace().map(str::to_owned).collect(),
        (None, Some(old)) => old.scopes.clone(),
        (None, None) => return None,
    };
    let refresh = answer["refresh_token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .or_else(|| old.map(|o| o.refresh.clone()))
        .unwrap_or_default();
    if refresh.is_empty() && scopes.iter().any(|s| s == DIRECT_SCOPE) {
        return None;
    }
    Some(Tokens {
        access: access.to_owned(),
        refresh,
        scopes,
        expires_at: now + expires_in,
        earliest_refresh_at: answer["earliest_refresh_at"].as_i64(),
        refreshed_at: now,
    })
}

/// A token endpoint's error code: `{"error": "…"}`, or `{"error": {"code": "…"}}`.
fn error_code(answer: &Value) -> Option<String> {
    let error = &answer["error"];
    error
        .as_str()
        .or_else(|| error["code"].as_str())
        .map(str::to_owned)
}

fn random(bytes: usize) -> String {
    let mut buffer = vec![0; bytes];
    getrandom::fill(&mut buffer).expect("the system's random source works");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
}

// ---------------------------------------------------------------------------------------------
// The id token

/// Who a checked id token says the person is.
#[derive(Debug, PartialEq, Eq)]
pub struct Who {
    pub sub: String,
    pub email: Option<String>,
}

/// Checks an id token as OpenAI asks: its RS256 signature against OpenAI's published keys (all they
/// publish: `id_token_signing_alg_values_supported`), issuer, audience (the issued client id),
/// expiry and this attempt's nonce. `Err` names what failed, never a token.
pub fn verify_id_token(
    token: &str,
    jwks: &Value,
    issuer: &str,
    client: &str,
    nonce: Option<&str>,
    now: i64,
) -> Result<Who, &'static str> {
    use aws_lc_rs::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut parts = token.split('.');
    let (Some(head), Some(body), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("not a JWT");
    };
    let decode = |part: &str| -> Result<Value, &'static str> {
        let bytes = b64.decode(part).map_err(|_| "not base64url")?;
        serde_json::from_slice(&bytes).map_err(|_| "not JSON")
    };
    let header = decode(head)?;
    if header["alg"] != "RS256" {
        return Err("not RS256");
    }
    let kid = header["kid"].as_str().ok_or("no kid")?;
    let key = jwks["keys"]
        .as_array()
        .and_then(|keys| keys.iter().find(|k| k["kid"] == kid && k["kty"] == "RSA"))
        .ok_or("no such key")?;
    let n = b64
        .decode(key["n"].as_str().ok_or("no modulus")?)
        .map_err(|_| "bad modulus")?;
    let e = b64
        .decode(key["e"].as_str().ok_or("no exponent")?)
        .map_err(|_| "bad exponent")?;
    let signature = b64.decode(signature).map_err(|_| "bad signature")?;
    RsaPublicKeyComponents { n: &n, e: &e }
        .verify(
            &RSA_PKCS1_2048_8192_SHA256,
            format!("{head}.{body}").as_bytes(),
            &signature,
        )
        .map_err(|_| "bad signature")?;
    let claims = decode(body)?;
    if claims["iss"] != issuer {
        return Err("wrong issuer");
    }
    let audience = match &claims["aud"] {
        Value::String(one) => one == client,
        Value::Array(many) => {
            many.iter().any(|a| a == client) && (many.len() == 1 || claims["azp"] == client)
        }
        _ => false,
    };
    if !audience {
        return Err("wrong audience");
    }
    // 30 seconds either way for a clock that's a little off.
    if claims["exp"].as_i64().is_none_or(|exp| exp <= now - 30) {
        return Err("expired");
    }
    if claims["iat"].as_i64().is_some_and(|iat| iat > now + 30) {
        return Err("issued in the future");
    }
    if let Some(nonce) = nonce
        && claims["nonce"] != nonce
    {
        return Err("wrong nonce");
    }
    let sub = claims["sub"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 512)
        .ok_or("no subject")?;
    Ok(Who {
        sub: sub.to_owned(),
        email: claims["email"]
            .as_str()
            .filter(|e| e.len() <= 320)
            .map(str::to_owned),
    })
}

// ---------------------------------------------------------------------------------------------
// The callback

/// One attempt, until its callback.
struct Pending {
    account: String,
    port: u16,
    state: String,
    nonce: String,
    verifier: String,
    redirect: String,
    /// The issued client this attempt uses: none on a first registration.
    client: Option<String>,
    prior: Option<Record>,
    host_id: String,
    /// The first callback with the attempt's `state` has come: there's no second.
    taken: AtomicBool,
    cancelled: AtomicBool,
    told: Mutex<Option<oneshot::Sender<bool>>>,
    end: tokio::sync::Notify,
}

/// A sign-in under way, to the caller that keeps it.
pub struct Flow {
    pub url: String,
    pending: Arc<Pending>,
    outcome: oneshot::Receiver<bool>,
}

impl Flow {
    /// Waits for how it ended: whether it took. None: its channel went without an answer.
    pub async fn ended(&mut self) -> bool {
        (&mut self.outcome).await.unwrap_or(false)
    }

    /// Ends the listener. An exchange in flight writes nothing afterwards and gives its tokens back.
    pub fn stop(&self) {
        self.pending.cancelled.store(true, Ordering::SeqCst);
        self.pending.end.notify_one();
    }
}

/// `GET /auth/callback` on the attempt's own port. A request naming another host or `state` isn't
/// from this sign-in: refused, and the sign-in goes on, so a page can't end it. The first with its
/// `state` decides it.
async fn callback(
    State((me, pending)): State<(Arc<ChatGpt>, Arc<Pending>)>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    use crate::tetrix::page;
    let host = format!("127.0.0.1:{}", pending.port);
    if headers.get(header::HOST).and_then(|h| h.to_str().ok()) != Some(host.as_str()) {
        return page(StatusCode::BAD_REQUEST, "This isn't Oby's sign-in page.");
    }
    let Some(params) = query.as_deref().and_then(params_of) else {
        return page(StatusCode::BAD_REQUEST, "This isn't Oby's sign-in page.");
    };
    if !params
        .get("state")
        .is_some_and(|given| crate::token::same(given, &pending.state))
    {
        return page(StatusCode::BAD_REQUEST, "This isn't Oby's sign-in page.");
    }
    if pending.taken.swap(true, Ordering::SeqCst) {
        return page(
            StatusCode::BAD_REQUEST,
            "This sign-in has already finished.",
        );
    }
    // Its own task: a browser that goes away can't stop it between a token and its record.
    let finishing = {
        let (me, pending) = (me.clone(), pending.clone());
        tokio::spawn(async move {
            let done = me.finish(&pending, &params).await;
            if let Err(why) = &done {
                me.say_problem(&pending.account, why);
            }
            let told = pending
                .told
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(told) = told {
                let _ = told.send(done.is_ok());
            }
            pending.end.notify_one();
            done
        })
    };
    match finishing.await {
        Ok(Ok(())) => page(
            StatusCode::OK,
            "You're signed in to ChatGPT. You can close this tab and go back to Oby.",
        ),
        Ok(Err(why)) => page(StatusCode::BAD_REQUEST, &why),
        Err(_) => page(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Oby couldn't finish signing in.",
        ),
    }
}

/// The callback's parameters; none when one is given twice.
fn params_of(query: &str) -> Option<HashMap<String, String>> {
    let url = reqwest::Url::parse(&format!("http://oby.invalid/?{query}")).ok()?;
    let mut params = HashMap::new();
    for (key, value) in url.query_pairs() {
        if params
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return None;
        }
    }
    Some(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::signature::{KeyPair as _, RsaKeyPair};
    use serde_json::json;

    const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

    fn key() -> (RsaKeyPair, Value) {
        let key = RsaKeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap();
        let public = jwk(&key, "k1");
        (key, json!({"keys": [public]}))
    }

    /// The JWK of a key: the modulus and exponent out of its DER, as OpenAI publishes them.
    fn jwk(key: &RsaKeyPair, kid: &str) -> Value {
        let der = key.public_key().as_ref().to_vec();
        let (n, e) = der_ints(&der);
        json!({"kty": "RSA", "kid": kid, "alg": "RS256", "use": "sig",
               "n": B64.encode(n), "e": B64.encode(e)})
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

    fn signed(key: &RsaKeyPair, header: Value, claims: Value) -> String {
        let head = B64.encode(header.to_string());
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

    fn claims() -> Value {
        json!({"iss": "https://auth.example", "aud": "oaiapp_1", "sub": "user-1", "email": "a@b.c",
               "nonce": "n1", "exp": 2000, "iat": 1000})
    }

    fn check(token: &str, keys: &Value) -> Result<Who, &'static str> {
        verify_id_token(
            token,
            keys,
            "https://auth.example",
            "oaiapp_1",
            Some("n1"),
            1500,
        )
    }

    /// CHATGPT-4's id token half: signature, `iss`, `aud`, `exp` and `nonce` each refused.
    #[test]
    fn chatgpt_4_an_id_token_must_check_out_in_every_part() {
        let (key, keys) = key();
        let head = json!({"alg": "RS256", "kid": "k1"});
        let good = signed(&key, head.clone(), claims());
        assert_eq!(
            check(&good, &keys).unwrap(),
            Who {
                sub: "user-1".into(),
                email: Some("a@b.c".into())
            }
        );
        let with = |field: &str, value: Value| {
            let mut c = claims();
            c[field] = value;
            signed(&key, head.clone(), c)
        };
        assert_eq!(
            check(&with("iss", json!("https://evil.example")), &keys),
            Err("wrong issuer")
        );
        assert_eq!(
            check(&with("aud", json!("oaiapp_2")), &keys),
            Err("wrong audience")
        );
        assert_eq!(check(&with("exp", json!(100)), &keys), Err("expired"));
        assert_eq!(
            check(&with("nonce", json!("n2")), &keys),
            Err("wrong nonce")
        );
        assert_eq!(check(&with("sub", json!("")), &keys), Err("no subject"));
        // Another key's signature, a token with its body changed, none at all.
        let (other, _) = self::key();
        assert_eq!(
            check(&signed(&other, head.clone(), claims()), &keys),
            Err("bad signature")
        );
        let parts: Vec<&str> = good.split('.').collect();
        let forged = format!(
            "{}.{}.{}",
            parts[0],
            B64.encode(json!({"sub": "x"}).to_string()),
            parts[2]
        );
        assert_eq!(check(&forged, &keys), Err("bad signature"));
        let none = format!(
            "{}.{}.",
            B64.encode(json!({"alg": "none", "kid": "k1"}).to_string()),
            parts[1]
        );
        assert_eq!(check(&none, &keys), Err("not RS256"));
        assert_eq!(
            check(
                &signed(&key, json!({"alg": "RS256", "kid": "k9"}), claims()),
                &keys
            ),
            Err("no such key")
        );
        // Many audiences only with itself as the party it was issued to.
        let mut many = claims();
        many["aud"] = json!(["oaiapp_1", "other"]);
        assert_eq!(
            check(&signed(&key, head.clone(), many.clone()), &keys),
            Err("wrong audience")
        );
        many["azp"] = json!("oaiapp_1");
        assert!(check(&signed(&key, head, many), &keys).is_ok());
    }

    /// CHATGPT-4's callback half: a parameter given twice isn't read at all.
    #[test]
    fn chatgpt_4_a_parameter_given_twice_is_refused() {
        assert!(params_of("code=a&state=b").is_some());
        assert!(params_of("code=a&code=b&state=c").is_none());
    }

    /// CHATGPT-7: `signedIn` follows the refresh token and the marker, never the access token.
    #[test]
    fn chatgpt_7_an_expired_access_token_is_still_signed_in() {
        let marker = Marker {
            email: None,
            signed: true,
            plan_use: true,
            expires_at: 1000,
            refreshed_at: 1000,
        };
        assert!(
            marker.usable(1000 + 3600 * 24),
            "a day past its access token"
        );
        assert!(
            !marker.usable(1000 + REFRESH_LASTS),
            "30 days from the last refresh"
        );
        assert!(
            !Marker {
                plan_use: false,
                ..marker.clone()
            }
            .usable(1000),
            "CHATGPT-6"
        );
        assert!(
            !Marker {
                signed: false,
                ..marker
            }
            .usable(1000),
            "CHATGPT-9"
        );
    }

    #[test]
    fn a_record_shows_no_token_when_printed_and_every_token_is_a_secret_form() {
        let record = Record {
            client_id: "oaiapp_1".into(),
            sub: "s".into(),
            host_id: "urn:uuid:x".into(),
            email: None,
            tokens: Some(Tokens {
                access: "access-now".into(),
                refresh: "refresh-now".into(),
                scopes: vec![],
                expires_at: 1,
                earliest_refresh_at: None,
                refreshed_at: 1,
            }),
            previous: vec![OldToken {
                access: "access-old".into(),
                expires_at: 2,
            }],
        };
        assert!(!format!("{record:?}").contains("access"));
        let forms = secret_forms(&serde_json::to_string(&record).unwrap());
        for token in ["access-now", "refresh-now", "access-old"] {
            assert!(forms.iter().any(|f| f == token), "{token}");
        }
    }

    #[test]
    fn a_token_answer_without_a_refresh_token_keeps_no_plan_grant() {
        let plan = format!("openid {DIRECT_SCOPE}");
        let good =
            json!({"access_token": "a", "refresh_token": "r", "expires_in": 3600, "scope": plan});
        assert!(tokens_from(&good, 0, None).is_some_and(|t| t.plan_use()));
        let short = json!({"access_token": "a", "expires_in": 3600, "scope": plan});
        assert!(tokens_from(&short, 0, None).is_none());
        // No scope on a first answer tells nothing; a refresh keeps the grant it had.
        let no_scope = json!({"access_token": "a", "refresh_token": "r", "expires_in": 3600});
        assert!(tokens_from(&no_scope, 0, None).is_none());
        let old = tokens_from(&good, 0, None).unwrap();
        assert!(tokens_from(&no_scope, 0, Some(&old)).is_some_and(|t| t.plan_use()));
        let no_rotation = json!({"access_token": "b", "expires_in": 3600});
        assert_eq!(
            tokens_from(&no_rotation, 0, Some(&old)).unwrap().refresh,
            "r"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn a_test_stand_in_is_only_ever_on_loopback() {
        assert!(test_origin("http://127.0.0.1:4000"));
        for origin in [
            "http://evil.example:4000",
            "https://127.0.0.1:4000",
            "http://127.0.0.1",
            "http://127.0.0.1:4000/x",
        ] {
            assert!(!test_origin(origin), "{origin}");
        }
    }

    // -----------------------------------------------------------------------------------------
    // Refreshing, against a stand-in token endpoint in this process.

    /// What the stand-in's token endpoint answers next: status and body, after a wait.
    #[derive(Clone)]
    enum Answer {
        Says(u16, Value),
    }

    #[derive(Default)]
    struct Script {
        answers: std::collections::VecDeque<Answer>,
        hold: Duration,
        refreshes: Vec<String>,
        revoked: Vec<String>,
        minted: u32,
    }

    type Shared = Arc<Mutex<Script>>;

    async fn token_endpoint(
        State(script): State<Shared>,
        axum::Form(form): axum::Form<HashMap<String, String>>,
    ) -> Response {
        use axum::response::IntoResponse;
        let hold = {
            let mut script = script.lock().unwrap();
            script.refreshes.push(form["refresh_token"].clone());
            script.hold
        };
        tokio::time::sleep(hold).await;
        let mut script = script.lock().unwrap();
        match script.answers.pop_front() {
            Some(Answer::Says(status, body)) => {
                (StatusCode::from_u16(status).unwrap(), axum::Json(body)).into_response()
            }
            None => {
                script.minted += 1;
                let n = script.minted;
                axum::Json(json!({"access_token": format!("access-new-{n}-abcdefgh"),
                    "refresh_token": format!("refresh-new-{n}-abcdefgh"),
                    "expires_in": 3600, "scope": DIRECT_SCOPE}))
                .into_response()
            }
        }
    }

    async fn openid_config(State(origin): State<String>) -> axum::Json<Value> {
        axum::Json(json!({"revocation_endpoint": format!("{origin}/revoke")}))
    }

    async fn revoke_endpoint(
        State(script): State<Shared>,
        axum::Form(form): axum::Form<HashMap<String, String>>,
    ) -> StatusCode {
        script.lock().unwrap().revoked.push(form["token"].clone());
        StatusCode::OK
    }

    /// A vault file whose writes fail while `failing` is on.
    struct Flaky {
        inner: crate::vault::FileStore,
        failing: Arc<AtomicBool>,
    }

    impl crate::vault::Store for Flaky {
        fn stamp(&self) -> Result<String, String> {
            self.inner.stamp()
        }
        fn read(&self) -> Result<std::collections::BTreeMap<String, String>, String> {
            self.inner.read()
        }
        fn save(&self, value: &str, name: &str) -> Result<(), String> {
            if self.failing.load(Ordering::SeqCst) {
                return Err("the disk is full".into());
            }
            self.inner.save(value, name)
        }
        fn remove(&self, name: &str) -> Result<bool, String> {
            self.inner.remove(name)
        }
    }

    struct World {
        me: Arc<ChatGpt>,
        script: Shared,
        failing: Arc<AtomicBool>,
        dir: PathBuf,
        vault: Arc<Vault>,
        _home: Scratch,
    }

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const ID: &str = "work";

    /// An account signed in with a token ending `ends_in` seconds from now.
    async fn world(ends_in: i64) -> World {
        let home = std::env::temp_dir().join(format!("chatgpt-unit-{}", uuid::Uuid::new_v4()));
        let dir = home.join("accounts").join(ID);
        crate::store::make_private_dir(&dir).unwrap();
        let script: Shared = Arc::default();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let app = Router::new()
            .route(
                "/api/accounts/oauth/token",
                axum::routing::post(token_endpoint),
            )
            .route("/revoke", axum::routing::post(revoke_endpoint))
            .with_state(script.clone())
            .merge(
                Router::new()
                    .route("/.well-known/openid-configuration", get(openid_config))
                    .with_state(origin.clone()),
            );
        tokio::spawn(async move { axum::serve(listener, app).await });
        let failing = Arc::new(AtomicBool::new(false));
        let vault = Arc::new(Vault::new(
            Box::new(Flaky {
                inner: crate::vault::FileStore::new(home.join("vault")),
                failing: failing.clone(),
            }),
            Arc::new(crate::secrets::Secrets::default()),
        ));
        let mut agents = crate::config::Agents {
            claude: None,
            codex: None,
            cursor: None,
        };
        agents.codex = Some(crate::config::Agent {
            bin: "codex".into(),
            accounts: vec![crate::config::Account {
                id: ID.into(),
                name: "Work".into(),
                config_dir: Some(dir.clone()),
                payer: None,
            }],
        });
        let me = ChatGpt::open(
            &home,
            vault.clone(),
            Live::new(agents),
            Endpoints {
                issuer: origin.clone(),
                resource: format!("{origin}/v1"),
            },
        );
        let now = obi_protocol::time::now();
        let record = Record {
            client_id: "oaiapp_1".into(),
            sub: "user-1".into(),
            host_id: "urn:uuid:x".into(),
            email: Some("a@b.c".into()),
            tokens: Some(Tokens {
                access: "access-old-abcdefgh".into(),
                refresh: "refresh-old-abcdefgh".into(),
                scopes: vec![DIRECT_SCOPE.into()],
                expires_at: now + ends_in,
                earliest_refresh_at: None,
                refreshed_at: now,
            }),
            previous: Vec::new(),
        };
        assert!(matches!(me.keep(ID, &record, &dir), Ok(Kept::Written)));
        World {
            me,
            script,
            failing,
            dir,
            vault,
            _home: Scratch(home),
        }
    }

    impl World {
        fn stored(&self) -> Record {
            let text = self.vault.value(&ChatGpt::name(ID)).unwrap().unwrap();
            serde_json::from_str(&text).unwrap()
        }
        fn refreshes(&self) -> usize {
            self.script.lock().unwrap().refreshes.len()
        }
    }

    /// A token far from its end goes out as it is, with no call.
    #[tokio::test]
    async fn a_token_far_from_its_end_needs_no_refresh() {
        let world = world(3600).await;
        assert_eq!(world.me.access(ID).await.unwrap(), "access-old-abcdefgh");
        assert_eq!(world.refreshes(), 0);
    }

    /// CHATGPT-8: two runs at once with an expiring token cause one refresh, and get its token.
    #[tokio::test]
    async fn chatgpt_8_two_at_once_refresh_once() {
        let world = world(60).await;
        world.script.lock().unwrap().hold = Duration::from_millis(200);
        let (a, b) = tokio::join!(world.me.access(ID), world.me.access(ID));
        assert_eq!(a.unwrap(), "access-new-1-abcdefgh");
        assert_eq!(b.unwrap(), "access-new-1-abcdefgh");
        assert_eq!(world.refreshes(), 1);
        let kept = world.stored();
        assert_eq!(
            kept.tokens.as_ref().unwrap().refresh,
            "refresh-new-1-abcdefgh"
        );
        // The token it replaced stays scrubbed until it ends.
        assert_eq!(kept.previous[0].access, "access-old-abcdefgh");
        assert!(marker_in(&world.dir).unwrap().signed);
    }

    /// CHATGPT-9: each terminal refresh answer clears the tokens, asks for a sign-in and keeps the
    /// client id. One `invalid_client` is Oby's configuration, with no sign-in asked and nothing lost.
    #[tokio::test]
    async fn chatgpt_9_a_terminal_refresh_error_clears_tokens_and_keeps_the_client() {
        for code in TERMINAL {
            let world = world(60).await;
            world
                .script
                .lock()
                .unwrap()
                .answers
                .push_back(Answer::Says(400, json!({"error": code})));
            let refused = world.me.access(ID).await.unwrap_err();
            assert_eq!(refused, sign_in_again(), "{code}");
            let kept = world.stored();
            assert!(kept.tokens.is_none(), "{code}");
            assert_eq!(kept.client_id, "oaiapp_1", "{code}");
            let marker = marker_in(&world.dir).unwrap();
            assert!(!marker.signed, "{code}");
            assert!(!marker.usable(0), "{code}");
        }
        // The nested shape too.
        let world_ = world(60).await;
        world_
            .script
            .lock()
            .unwrap()
            .answers
            .push_back(Answer::Says(
                401,
                json!({"error": {"code": "refresh_token_expired", "message": "no"}}),
            ));
        assert_eq!(world_.me.access(ID).await.unwrap_err(), sign_in_again());

        let world = world(60).await;
        world
            .script
            .lock()
            .unwrap()
            .answers
            .push_back(Answer::Says(401, json!({"error": "invalid_client"})));
        assert!(matches!(world.me.access(ID).await, Err(Refused::Client(_))));
        assert!(world.stored().tokens.is_some());
    }

    /// `refresh_token_reused` clears only if the vault still holds the token that was sent.
    #[tokio::test]
    async fn chatgpt_9_a_reused_token_clears_only_if_it_is_still_the_vaults() {
        let world = world(60).await;
        world.script.lock().unwrap().hold = Duration::from_millis(300);
        world
            .script
            .lock()
            .unwrap()
            .answers
            .push_back(Answer::Says(400, json!({"error": "refresh_token_reused"})));
        let (me, id) = (world.me.clone(), ID);
        let waiting = tokio::spawn(async move { me.access(id).await });
        // Another obid rotated it meanwhile, and wrote its record first.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut newer = world.stored();
        let tokens = newer.tokens.as_mut().unwrap();
        tokens.refresh = "refresh-theirs-abcdefgh".into();
        tokens.access = "access-theirs-abcdefgh".into();
        assert!(matches!(
            world.me.keep(ID, &newer, &world.dir),
            Ok(Kept::Written)
        ));
        assert_eq!(waiting.await.unwrap().unwrap(), "access-theirs-abcdefgh");
        assert!(world.stored().tokens.is_some());
    }

    /// CHATGPT-19: stopping a job mid-refresh doesn't cancel it, and its new tokens are kept.
    #[tokio::test]
    async fn chatgpt_19_stopping_a_job_mid_refresh_keeps_the_new_tokens() {
        let world = world(60).await;
        world.script.lock().unwrap().hold = Duration::from_millis(300);
        let asking = {
            let me = world.me.clone();
            tokio::spawn(async move { me.access(ID).await })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        asking.abort();
        let _ = asking.await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let kept = world.stored();
        assert_eq!(kept.tokens.unwrap().refresh, "refresh-new-1-abcdefgh");
        assert!(marker_in(&world.dir).unwrap().signed);
    }

    /// CHATGPT-19: a lost answer is tried again with the same refresh token, and stays signed in.
    /// (A 5xx here: the endpoint can't drop a connection on purpose without taking the task down.)
    #[tokio::test]
    async fn chatgpt_19_a_lost_answer_is_tried_again_with_the_same_token() {
        let world = world(60).await;
        world
            .script
            .lock()
            .unwrap()
            .answers
            .push_back(Answer::Says(503, json!({"error": "unavailable"})));
        assert_eq!(world.me.access(ID).await.unwrap(), "access-new-1-abcdefgh");
        let sent = world.script.lock().unwrap().refreshes.clone();
        assert_eq!(sent, ["refresh-old-abcdefgh", "refresh-old-abcdefgh"]);
        // All three lost: credentials kept, and nothing asks for a sign-in.
        let world = self::world(60).await;
        for _ in 0..3 {
            world
                .script
                .lock()
                .unwrap()
                .answers
                .push_back(Answer::Says(503, json!({})));
        }
        assert!(matches!(world.me.access(ID).await, Err(Refused::Later(_))));
        assert_eq!(
            world.stored().tokens.unwrap().refresh,
            "refresh-old-abcdefgh"
        );
        assert!(marker_in(&world.dir).unwrap().signed);
    }

    /// CHATGPT-19: a failed vault write keeps the rotated record in memory, sends no other refresh,
    /// and gives nobody the token until it's written.
    #[tokio::test]
    async fn chatgpt_19_a_failed_vault_write_refreshes_no_more_and_is_written_later() {
        let world = world(60).await;
        world.failing.store(true, Ordering::SeqCst);
        assert!(matches!(world.me.access(ID).await, Err(Refused::Later(_))));
        assert_eq!(world.refreshes(), 1);
        assert_eq!(
            world.stored().tokens.unwrap().refresh,
            "refresh-old-abcdefgh"
        );
        assert!(marker_in(&world.dir).unwrap().signed);
        // Still failing: still no second refresh of a token OpenAI has already rotated.
        assert!(matches!(world.me.access(ID).await, Err(Refused::Later(_))));
        assert_eq!(world.refreshes(), 1);
        world.failing.store(false, Ordering::SeqCst);
        assert_eq!(world.me.access(ID).await.unwrap(), "access-new-1-abcdefgh");
        assert_eq!(world.refreshes(), 1);
        assert_eq!(
            world.stored().tokens.unwrap().refresh,
            "refresh-new-1-abcdefgh"
        );
    }

    /// CHATGPT-15: a refresh in flight when the account goes writes nothing and gives back what it
    /// was given.
    #[tokio::test]
    async fn chatgpt_15_a_refresh_in_flight_writes_nothing_after_removal_and_revokes() {
        let world = world(60).await;
        world.script.lock().unwrap().hold = Duration::from_millis(400);
        let asking = {
            let me = world.me.clone();
            tokio::spawn(async move { me.access(ID).await })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(world.me.remove(ID).await);
        assert_eq!(asking.await.unwrap().unwrap_err(), sign_in_again());
        assert!(world.vault.value(&ChatGpt::name(ID)).unwrap().is_none());
        let revoked = world.script.lock().unwrap().revoked.clone();
        assert_eq!(revoked, ["refresh-old-abcdefgh", "refresh-new-1-abcdefgh"]);
    }

    /// CHATGPT-11: the vault learns a record's tokens, and its replaced access token, as forms of one
    /// name: taken out of what `for_agent` and the answers use, also read from disk after a restart.
    #[tokio::test]
    async fn chatgpt_11_every_generation_is_a_form_of_the_one_name() {
        let world = world(60).await;
        assert_eq!(world.me.access(ID).await.unwrap(), "access-new-1-abcdefgh");
        let text =
            "old access-old-abcdefgh new access-new-1-abcdefgh refresh refresh-new-1-abcdefgh";
        let clean = world.vault.redact(text).unwrap();
        for token in [
            "access-old-abcdefgh",
            "access-new-1-abcdefgh",
            "refresh-new-1-abcdefgh",
        ] {
            assert!(!clean.contains(token), "{clean}");
        }
        assert!(
            clean.contains(&format!("[vault: {VAULT_PREFIX}{ID}]")),
            "{clean}"
        );
        // A new obid reading the same file: the same, with nothing learned before.
        let home = world.dir.parent().unwrap().parent().unwrap().join("vault");
        let again = Vault::file(home, Arc::new(crate::secrets::Secrets::default()));
        let clean = again.redact(text).unwrap();
        for token in [
            "access-old-abcdefgh",
            "access-new-1-abcdefgh",
            "refresh-new-1-abcdefgh",
        ] {
            assert!(!clean.contains(token), "{clean}");
        }
    }

    /// As obid starts: an entry whose account went is taken back and deleted, and a marker whose
    /// entry went stops saying it's signed in.
    #[tokio::test]
    async fn the_sweep_takes_back_what_a_crash_left() {
        let world = world(3600).await;
        let record = world.stored();
        let orphan = format!("{VAULT_PREFIX}gone");
        world
            .vault
            .add(&serde_json::to_string(&record).unwrap(), &orphan)
            .unwrap();
        world.me.sweep().await;
        assert!(world.vault.value(&orphan).unwrap().is_none());
        assert_eq!(
            world.script.lock().unwrap().revoked,
            ["refresh-old-abcdefgh"]
        );
        assert!(
            world.stored().tokens.is_some(),
            "the account it does have stays"
        );
        world.vault.remove(&ChatGpt::name(ID)).unwrap();
        world.me.sweep().await;
        assert!(!marker_in(&world.dir).unwrap().signed);
    }
}
