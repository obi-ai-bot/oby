//! The accounts the agents run on, as the owner adds, renames, removes and signs them in from the
//! apps (docs/runner-api.md, "POST /v1/accounts" to "GET /v1/accounts/{id}/sign-in").
//!
//! The configuration's `agents.<tool>.accounts` are the seed. What the API changes is kept in
//! `home/accounts.json` (0600) and laid over the seed at every start ([`Live::open`]), so a restart
//! keeps it and the configuration file is never rewritten. Every copy of the configuration shares
//! one [`Live`], so a change reaches routing, the brain and the gateway's scrubbing at once. An
//! account the API adds always has a folder obid makes, `home/accounts/<id>`.
//!
//! Signing in runs the tool's own login, never obid's (AGENTS.md), in the account's own folder: no
//! box ever gets that folder (#655), so nothing a job wrote can steer the login, and obid never
//! reads or copies what it writes. obid passes on only the link to open and the code to type,
//! both read from what the login prints and checked, and the code the person pastes back goes to
//! the login's stdin. Nothing else the login prints is kept, shown or logged. The one exception is
//! Continue with ChatGPT (`?with=chatgpt`), where obid is the OAuth client itself (crate::chatgpt).
//!
//! Who an account is (its [`Identity`]) is what the tool's own status says of it, run the way its
//! sign-in is: only the email, plan and org are taken from the answer (docs/contracts/accounts.md).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::agent::Tool;
use crate::api::{AccountJson, ApiError, Shared};
use crate::config::{Account, Agent, Agents};

/// Where obid keeps what the API changed, under its home.
const FILE: &str = "accounts.json";

/// The longest name or payer, in characters.
const LONGEST: usize = 60;

/// The most accounts obid keeps, the configuration's and the API's together, unless the owner
/// raised it ([`most_accounts`]).
pub const MOST_ACCOUNTS: usize = 32;

/// The most accounts obid keeps now (`limits.accounts`).
fn most_accounts() -> usize {
    crate::settings::limits().accounts()
}

/// How long a look for the tools' own logins stands ([`refind_own_logins`]).
pub const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// How long a sign-in may wait for the person: Codex's one-time code lasts 15 minutes.
const SIGN_IN_LASTS: Duration = Duration::from_secs(15 * 60);

/// How long a login may take to print its link.
const LINK_WITHIN: Duration = Duration::from_secs(30);

/// How long a pasted code may take to sign the login in.
const CODE_WITHIN: Duration = Duration::from_secs(60);

/// How long a tool's own status may take to say who the account is.
const STATUS_WITHIN: Duration = Duration::from_secs(15);

/// The longest line of a login's output obid reads; the rest of a longer line is dropped.
const LONGEST_LINE: usize = 4096;

/// The `PATH` a login runs with: the system's folders, then the folder of the tool's own binary.
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// The folders a sign-in uses for its temporary files, under obid's home: `sign-in-<…>`, swept at
/// every start ([`sweep`]).
pub(crate) const SCRATCH_PREFIX: &str = "sign-in-";

// ---------------------------------------------------------------------------------------------
// The accounts, live

/// The agents' accounts as they are now: the configuration's, with what the API changed laid over
/// them. Cloning shares it.
#[derive(Clone)]
pub struct Live(Arc<RwLock<Inner>>);

struct Inner {
    seed: Agents,
    saved: Saved,
    /// `home/accounts.json`, once [`Live::open`] read it.
    file: Option<PathBuf>,
    /// Why `home/accounts.json` can't be used: nothing is changed while it can't.
    broken: Option<String>,
    agents: Agents,
    /// The tools whose own login obid found on this Mac at start ([`adopt_own_logins`]).
    found: Vec<Tool>,
    /// When [`refind_own_logins`] last looked.
    looked: Option<std::time::Instant>,
}

/// What the API changed: the accounts it added, and the configuration's accounts it renamed or
/// removed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    #[serde(default)]
    added: Vec<Added>,
    #[serde(default)]
    changed: BTreeMap<String, Change>,
    #[serde(default)]
    removed: Vec<String>,
    /// The folders of the accounts removed: still hidden from every box ([`Live::hidden_folders`]),
    /// since what was copied there or left there stays.
    #[serde(default)]
    retired: Vec<PathBuf>,
    /// The tools whose own login obid found on this Mac and listed ([`Live::adopt`]): kept once
    /// removed too (`removed` says so), so the tool's own login can come back.
    #[serde(default)]
    own: Vec<String>,
    /// Who each account is, as its tool last said, by id ([`Live::learn`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    identities: BTreeMap<String, Identity>,
}

/// What a tool says of the account it's signed in to, and all obid keeps of what it says: each
/// value only when the tool gave one. Never a token.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// The tool's own word for the plan (`max`, `plus`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
}

/// An account the API added: its folder is obid's own, `home/accounts/<id>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Added {
    id: String,
    agent: String,
    name: String,
    #[serde(default)]
    payer: Option<String>,
    config_dir: PathBuf,
    /// Added and never signed in yet: its first sign-in may turn out to be another account's login
    /// (ACCOUNTS-7). An account from before this key isn't.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    fresh: bool,
    /// obid picked its name: its first sign-in fills it from the org or plan (ACCOUNTS-2).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    auto_name: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Change {
    name: String,
    #[serde(default)]
    payer: Option<String>,
}

impl Added {
    fn account(&self) -> Account {
        Account {
            id: self.id.clone(),
            name: self.name.clone(),
            payer: self.payer.clone(),
            config_dir: Some(self.config_dir.clone()),
        }
    }
}

impl Live {
    pub fn new(seed: Agents) -> Live {
        Live(Arc::new(RwLock::new(Inner {
            agents: seed.clone(),
            seed,
            saved: Saved::default(),
            file: None,
            broken: None,
            found: Vec::new(),
            looked: None,
        })))
    }

    /// Whether `home/accounts.json` doesn't read: the accounts now are the configuration's alone,
    /// and one missing from them says nothing.
    pub fn broken(&self) -> bool {
        self.read().broken.is_some()
    }

    /// Whether the owner removed the account `id` (kept in `home/accounts.json`).
    pub fn was_removed(&self, id: &str) -> bool {
        self.read().saved.removed.iter().any(|r| r == id)
    }

    /// The agents and their accounts now.
    pub fn get(&self) -> Agents {
        self.read().agents.clone()
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        self.0.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        self.0.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Every account's folder no box may read (#655): the accounts' now
    /// ([`crate::jobs::account_folders`]), and those of the accounts removed, which may still hold
    /// a login.
    pub fn hidden_folders(&self, home: &Path) -> Vec<PathBuf> {
        let mut all = crate::jobs::account_folders(&self.get(), home);
        let retired = self.read().saved.retired.clone();
        for dir in retired {
            if dir.is_dir() && !home.starts_with(&dir) && !all.contains(&dir) {
                all.push(dir);
            }
        }
        all
    }

    /// Lays `home/accounts.json` over the configuration's accounts (crate::serve, before anything
    /// uses them), each entry checked as the API checks it ([`checked`]): one that isn't right is
    /// left out and logged. A file that doesn't read is left as it is, logged, and changes are
    /// refused until the owner fixes or removes it.
    pub fn open(&self, home: &Path) {
        let file = home.join(FILE);
        let mut inner = self.write();
        match std::fs::read(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => inner.broken = Some(format!("obid can't read {}: {e}", file.display())),
            Ok(bytes) => match serde_json::from_slice::<Saved>(&bytes) {
                Ok(saved) => inner.saved = checked(&inner.seed, saved, home),
                Err(e) => inner.broken = Some(format!("{} doesn't read: {e}", file.display())),
            },
        }
        if let Some(why) = &inner.broken {
            tracing::warn!("{why}: accounts can't be changed until it's fixed or removed");
        }
        inner.agents = merged(&inner.seed, &inner.saved);
        inner.file = Some(file);
    }

    /// On a fresh obid (crate::serve, [`adopt_own_logins`]): lists the own login of each of
    /// `found` that has no account at all (none configured, added or implied) and wasn't listed by
    /// obid before, within [`MOST_ACCOUNTS`]. The tools it listed.
    fn adopt(&self, found: &[Tool]) -> Vec<Tool> {
        let fresh = |agents: &Agents, saved: &Saved, tool: Tool| {
            let none = agent_of(agents, tool)
                .as_ref()
                .is_none_or(|a| a.accounts.is_empty() && implicit(tool).is_empty());
            none && !saved.own.iter().any(|t| t == tool.id())
                && !saved.removed.contains(&own_login(tool).id)
        };
        {
            let inner = self.read();
            if !found
                .iter()
                .any(|tool| fresh(&inner.agents, &inner.saved, *tool))
            {
                return Vec::new();
            }
        }
        let listed = self.change(|agents, saved| {
            let mut listed: Vec<Tool> = Vec::new();
            for tool in found {
                // Those listed in this pass count too.
                if fresh(agents, saved, *tool) && all(agents).len() + listed.len() < most_accounts()
                {
                    listed.push(*tool);
                }
            }
            saved
                .own
                .extend(listed.iter().map(|tool| tool.id().to_owned()));
            Ok(listed)
        });
        listed.unwrap_or_else(|e| {
            tracing::warn!("the tools' own logins weren't listed: {}", e.message);
            Vec::new()
        })
    }

    /// Who the account `id` is, as its tool last said.
    pub fn identity(&self, id: &str) -> Identity {
        self.read()
            .saved
            .identities
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// Whether the account `id` was added and never signed in.
    fn fresh(&self, id: &str) -> bool {
        self.read()
            .saved
            .added
            .iter()
            .any(|a| a.id == id && a.fresh)
    }

    /// What the tool said of a signed-in account: kept, and a name obid picked filled from the
    /// plan, unique per tool: never the org, a name someone else chose that would reach agents
    /// unquoted with the account's. `signed_in_now`: a new sign-in, after which saying nothing forgets
    /// who it was before; a look at a login that didn't change keeps it. Nothing for an account
    /// gone meanwhile.
    fn learn(&self, tool: Tool, id: &str, identity: Identity, signed_in_now: bool) {
        let kept = self.change(|agents, saved| {
            if find(agents, id).is_none() {
                return Ok(());
            }
            if let Some(added) = saved.added.iter_mut().find(|a| a.id == id) {
                added.fresh = false;
                let base = identity.plan.as_deref().and_then(|plan| {
                    let mut letters = plan.chars();
                    Some(
                        letters
                            .next()?
                            .to_uppercase()
                            .chain(letters)
                            .collect::<String>(),
                    )
                });
                if added.auto_name
                    && let Some(name) = base.and_then(|b| free_name(agents, tool, &b, Some(id)))
                {
                    added.name = name;
                    added.auto_name = false;
                }
            }
            if identity != Identity::default() {
                saved.identities.insert(id.to_owned(), identity);
            } else if signed_in_now {
                saved.identities.remove(id);
            }
            Ok(())
        });
        if let Err(e) = kept {
            tracing::warn!("account {id}: who it is wasn't kept: {}", e.message);
        }
    }

    /// The tools' own logins obid found on this Mac that aren't listed: `POST /v1/accounts`
    /// `{"agent", "default": true}` lists one on the owner's word ([`restore_default`]).
    pub fn available(&self) -> Vec<(Tool, Account)> {
        let inner = self.read();
        inner
            .found
            .iter()
            .filter(|tool| addable(&inner.seed, &inner.agents, **tool))
            .map(|tool| (*tool, own_login(*tool)))
            .collect()
    }

    /// Changes what's saved with `change`, keeps it, and only then uses it. `change` answers what
    /// the caller needs, or why it refused.
    fn change<T>(
        &self,
        change: impl FnOnce(&Agents, &mut Saved) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let mut inner = self.write();
        if let Some(why) = &inner.broken {
            return Err(ApiError::new(StatusCode::SERVICE_UNAVAILABLE, why.clone()));
        }
        let mut saved = inner.saved.clone();
        let answer = change(&inner.agents, &mut saved)?;
        if let Some(file) = &inner.file {
            let text = serde_json::to_vec_pretty(&saved).map_err(|e| unkept(file, e))?;
            crate::store::write_private(file, &text).map_err(|e| unkept(file, e))?;
        }
        inner.agents = merged(&inner.seed, &saved);
        inner.saved = saved;
        Ok(answer)
    }
}

fn unkept(file: &Path, e: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("obid couldn't keep the change in {}: {e}", file.display()),
    )
}

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Live").field(&self.get()).finish()
    }
}

impl PartialEq for Live {
    fn eq(&self, other: &Live) -> bool {
        self.get() == other.get()
    }
}

impl Eq for Live {}

/// `saved` as read from `home/accounts.json`, with what the API would refuse left out (and
/// logged): an added account must have an id obid makes, a tool, its folder `home/accounts/<id>`,
/// a name and payer the API takes and no other account's name, and keep the total within
/// [`MOST_ACCOUNTS`]; a change's name and payer the same.
fn checked(seed: &Agents, saved: Saved, home: &Path) -> Saved {
    let drop = |what: &str, id: &str| {
        tracing::warn!("{}: {what} {id:?} left out", home.join(FILE).display());
    };
    let mut own: Vec<String> = Vec::new();
    for tool in saved.own {
        match Tool::from_id(&tool) {
            Some(_) if !own.contains(&tool) => own.push(tool),
            _ => drop("own login", &tool),
        }
    }
    let mut kept = Saved {
        own,
        removed: saved.removed,
        retired: saved
            .retired
            .into_iter()
            .filter(|dir| dir.is_absolute())
            .collect(),
        ..Saved::default()
    };
    for (id, change) in saved.changed {
        // A configured account's, with a name no other account of its tool has.
        let agents = merged(seed, &kept);
        let fine = find(&agents, &id).and_then(|(tool, _)| {
            let name = name(&agents, tool, &change.name, Some(&id)).ok()?;
            let payer = payer_text(change.payer.as_deref()).ok()?;
            Some(Change { name, payer })
        });
        match fine {
            Some(change) => {
                kept.changed.insert(id, change);
            }
            None => drop("change", &id),
        }
    }
    for added in saved.added {
        let agents = merged(seed, &kept);
        let fine = Tool::from_id(&added.agent).filter(|_| {
            own_folder(home, &added.id).is_some_and(|own| own == added.config_dir)
                && find(&agents, &added.id).is_none()
                && all(&agents).len() < most_accounts()
                && payer_text(added.payer.as_deref()).is_ok()
        });
        match fine.map(|tool| name(&agents, tool, &added.name, None)) {
            Some(Ok(name)) if name == added.name => kept.added.push(added),
            _ => drop("account", &added.id),
        }
    }
    let agents = merged(seed, &kept);
    kept.identities = saved
        .identities
        .into_iter()
        .filter(|(id, _)| find(&agents, id).is_some())
        .map(|(id, who)| (id, who.cleaned()))
        .collect();
    kept
}

/// The tool's own login as an account: no folder, signed in with the tool itself.
fn own_login(tool: Tool) -> Account {
    let (id, name) = match tool {
        Tool::Claude => ("default", "Default"),
        Tool::Codex => ("codex", "Codex"),
        Tool::Cursor => ("cursor", "Cursor"),
    };
    Account {
        id: id.into(),
        name: name.into(),
        payer: None,
        config_dir: None,
    }
}

/// The tool's own default login, which an empty `accounts` stands for (Cursor has none: an empty
/// list is no account; its own login is listed only once obid found it, [`Live::adopt`]).
fn implicit(tool: Tool) -> Vec<Account> {
    match tool {
        Tool::Cursor => Vec::new(),
        _ => vec![own_login(tool)],
    }
}

/// The binary a tool the configuration doesn't name runs as, found on the login shell's `PATH`.
fn default_bin(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
        Tool::Cursor => "cursor-agent",
    }
}

fn agent_of(agents: &Agents, tool: Tool) -> &Option<Agent> {
    match tool {
        Tool::Claude => &agents.claude,
        Tool::Codex => &agents.codex,
        Tool::Cursor => &agents.cursor,
    }
}

fn agent_of_mut(agents: &mut Agents, tool: Tool) -> &mut Option<Agent> {
    match tool {
        Tool::Claude => &mut agents.claude,
        Tool::Codex => &mut agents.codex,
        Tool::Cursor => &mut agents.cursor,
    }
}

/// The configuration's accounts with `saved` laid over them: renamed, removed, then the added
/// ones after. A tool the configuration doesn't name is there while it has an added account. A
/// tool whose accounts were all removed is gone, rather than back on its own default login (an
/// empty list's meaning): its default login is listed (and can be removed) once anything about
/// the tool's accounts changed.
fn merged(seed: &Agents, saved: &Saved) -> Agents {
    let seeded: Vec<String> = Tool::ALL
        .iter()
        .filter_map(|t| agent_of(seed, *t).as_ref())
        .flat_map(|a| a.accounts.iter().map(|a| a.id.clone()))
        .collect();
    let mut agents = seed.clone();
    for tool in Tool::ALL {
        // The tool's own login obid found on this Mac, unless removed, then what the API added.
        let own = (saved.own.iter().any(|t| t == tool.id()))
            .then(|| own_login(tool))
            .filter(|a| !saved.removed.contains(&a.id));
        let added: Vec<Account> = own
            .into_iter()
            .chain(
                saved
                    .added
                    .iter()
                    .filter(|a| a.agent == tool.id())
                    .map(Added::account),
            )
            .filter(|a| !seeded.contains(&a.id))
            .collect();
        let slot = agent_of_mut(&mut agents, tool);
        let agent = match slot {
            Some(agent) => {
                let own_removed = implicit(tool).iter().any(|a| saved.removed.contains(&a.id));
                if agent.accounts.is_empty() && (!added.is_empty() || own_removed) {
                    agent.accounts = implicit(tool);
                }
                agent
            }
            None if added.is_empty() => continue,
            None => slot.insert(Agent {
                bin: default_bin(tool).into(),
                accounts: Vec::new(),
            }),
        };
        let had = agent.accounts.len() + added.len();
        agent.accounts.retain(|a| !saved.removed.contains(&a.id));
        for account in added {
            if !agent.accounts.iter().any(|a| a.id == account.id) {
                agent.accounts.push(account);
            }
        }
        for account in &mut agent.accounts {
            if let Some(change) = saved.changed.get(&account.id) {
                account.name = change.name.clone();
                account.payer = change.payer.clone();
            }
        }
        if had > 0 && agent.accounts.is_empty() {
            *slot = None;
        }
    }
    agents
}

/// At every start (crate::serve): the tools whose CLI is on this Mac with their own login there
/// (Claude Code's, Codex's `~/.codex/auth.json`, Cursor's login file). On a fresh obid (no
/// `home/accounts.json` and no `home/jobs` yet: its first start), each such tool without an account
/// gets its own login listed ([`Live::adopt`]), so a new Mac needs nothing more. An obid that ran
/// before keeps its accounts as they are: what it finds is only offered ([`Live::available`]).
pub fn adopt_own_logins(agents: &Live, obid_home: &Path) {
    let first = [FILE, "jobs"]
        .iter()
        .all(|name| std::fs::symlink_metadata(obid_home.join(name)).is_err());
    let found = own_logins(agents);
    {
        // A look, as refind_own_logins's: the first status after a start doesn't ask again.
        let mut inner = agents.write();
        inner.found = found.clone();
        inner.looked = Some(std::time::Instant::now());
    }
    if !first {
        return;
    }
    for tool in agents.adopt(&found) {
        tracing::info!(
            "{}'s own login on this Mac is listed as an account",
            tool.name()
        );
    }
}

/// The tools whose CLI is on this Mac with their own login there.
fn own_logins(agents: &Live) -> Vec<Tool> {
    let home = crate::jobs::login_home();
    let now = agents.get();
    Tool::ALL
        .into_iter()
        .filter(|tool| {
            let bin = agent_of(&now, *tool)
                .as_ref()
                .map_or(default_bin(*tool), |a| a.bin.as_str());
            crate::process::resolve_bin(bin).is_some()
                && crate::jobs::has_login(*tool, &own_login(*tool), home.as_deref())
        })
        .collect()
}

/// Looks again for the tools' own logins (`GET /v1/status`): one installed or signed in after obid
/// started, as on a fresh Mac's first launch, is offered ([`Live::available`]) without a restart.
/// With `claudeKeychain` or `cursorKeychain` left out and off, a login now only in the Keychain
/// turns its copy on, as it would have at start.
/// It looks at most every [`LOOK_EVERY`]: a look asks `security`, about 50 ms, and `GET /v1/status`
/// calls it every time.
pub fn refind_own_logins(agents: &Live) {
    {
        let mut inner = agents.write();
        if inner.looked.is_some_and(|at| at.elapsed() < LOOK_EVERY) {
            return;
        }
        inner.looked = Some(std::time::Instant::now());
    }
    if crate::jobs::keychain_auto() && !crate::jobs::keychain_on() && crate::jobs::keychain_wanted()
    {
        crate::jobs::use_keychain(true, true);
        tracing::info!("claudeKeychain on by itself: Claude Code's login is only in the Keychain");
    }
    if crate::jobs::cursor_keychain_auto()
        && !crate::jobs::cursor_keychain_on()
        && crate::jobs::cursor_keychain_wanted()
    {
        crate::jobs::use_cursor_keychain(true, true);
        tracing::info!("cursorKeychain on by itself: Cursor's login is only in the Keychain");
    }
    let found = own_logins(agents);
    agents.write().found = found;
}

/// Whether `tool`'s own login can be listed: it isn't (by its id, as an empty list stands for it,
/// or as any account without a folder), the configuration gives no account its id with a folder of
/// its own, its name is free, and there's room.
fn addable(seed: &Agents, agents: &Agents, tool: Tool) -> bool {
    let own = own_login(tool);
    let listed = agent_of(agents, tool).as_ref().is_some_and(|a| {
        (a.accounts.is_empty() && !implicit(tool).is_empty())
            || a.accounts.iter().any(|a| a.config_dir.is_none())
    });
    !listed
        && find(agents, &own.id).is_none()
        && find(seed, &own.id).is_none_or(|(_, a)| a.config_dir.is_none())
        && name(agents, tool, &own.name, None).is_ok()
        && all(agents).len() < most_accounts()
}

/// Every account, with its tool.
fn all(agents: &Agents) -> Vec<(Tool, Account)> {
    Tool::ALL
        .iter()
        .filter_map(|t| agent_of(agents, *t).as_ref().map(|a| (*t, a)))
        .flat_map(|(t, a)| a.accounts.iter().map(move |a| (t, a.clone())))
        .collect()
}

/// The account `id`, with its tool.
pub fn find(agents: &Agents, id: &str) -> Option<(Tool, Account)> {
    all(agents).into_iter().find(|(_, a)| a.id == id)
}

// ---------------------------------------------------------------------------------------------
// What the owner may give

fn unprocessable(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, message)
}

/// A name or payer: one line, trimmed, 1 to 60 characters.
fn one_line(what: &str, text: &str) -> Result<String, ApiError> {
    let text = text.trim();
    let count = text.chars().count();
    if count == 0 || count > LONGEST || text.chars().any(char::is_control) {
        return Err(unprocessable(format!(
            "{what} is one line of 1 to {LONGEST} characters."
        )));
    }
    Ok(text.to_owned())
}

/// A name no other account of the tool has, as its name or its id (jobs name an account by
/// either), letter case aside.
fn name(agents: &Agents, tool: Tool, text: &str, except: Option<&str>) -> Result<String, ApiError> {
    let name = one_line("name", text)?;
    let lower = name.to_lowercase();
    let taken = agent_of(agents, tool)
        .iter()
        .flat_map(|a| &a.accounts)
        .any(|a| {
            Some(a.id.as_str()) != except
                && (a.name.to_lowercase() == lower || a.id.to_lowercase() == lower)
        });
    if taken {
        return Err(unprocessable(format!(
            "name {name:?} is another {} account's already.",
            tool.name()
        )));
    }
    Ok(name)
}

/// `base` as a name no other account of the tool has: as it is, else `<base> 2`, `<base> 3`… None
/// when nothing of `base` can be a name.
fn free_name(agents: &Agents, tool: Tool, base: &str, except: Option<&str>) -> Option<String> {
    // Room for the number, and what isn't one line goes.
    let base: String = base
        .chars()
        .filter(|c| !c.is_control())
        .take(LONGEST - 4)
        .collect();
    let base = base.trim();
    std::iter::once(base.to_owned())
        .chain((2..1000).map(|n| format!("{base} {n}")))
        // (its own login's stays free for it: taken, that login could no longer be listed)
        .filter(|text| !text.eq_ignore_ascii_case(&own_login(tool).name))
        .find_map(|text| name(agents, tool, &text, except).ok())
        .filter(|_| !base.is_empty())
}

/// A payer, or none for empty or null.
fn payer(value: Option<&Value>) -> Result<Option<String>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => payer_text(Some(text)),
        Some(_) => Err(unprocessable("payer is text or null.")),
    }
}

fn payer_text(text: Option<&str>) -> Result<Option<String>, ApiError> {
    match text {
        None => Ok(None),
        Some(text) if text.trim().is_empty() => Ok(None),
        Some(text) => one_line("payer", text).map(Some),
    }
}

/// An id no account has, nor had (a removed one), as job ids are made.
fn new_id(agents: &Agents, saved: &Saved) -> String {
    loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).expect("the system's random source works");
        let id = hex::encode(bytes);
        if find(agents, &id).is_none() && !saved.removed.contains(&id) {
            return id;
        }
    }
}

/// The folder obid makes for an account it adds: `home/accounts/<id>`, `id` being one obid made
/// (eight lowercase hex digits: macOS ignores letter case, so `A1B2C3D4` would be `a1b2c3d4`'s
/// folder), never one anybody wrote.
fn own_folder(home: &Path, id: &str) -> Option<PathBuf> {
    (id.len() == 8 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .then(|| home.join("accounts").join(id))
}

/// The body as a JSON object with only `keys`.
fn object(body: &Bytes, keys: &[&str], shape: &str) -> Result<Map<String, Value>, ApiError> {
    let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(body) else {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("The body is {shape}."),
        ));
    };
    if let Some(key) = map.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(unprocessable(format!(
            "{key} isn't something an account has here: the body is {shape}."
        )));
    }
    Ok(map)
}

fn text_field<'a>(map: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, ApiError> {
    match map.get(key) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(unprocessable(format!("{key} is text."))),
    }
}

fn no_account(id: &str) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, format!("There's no account {id:?}."))
}

fn answer(status: StatusCode, value: impl Serialize) -> Response {
    (status, axum::Json(value)).into_response()
}

/// Every account as the apps see it, worked out off obid's async runtime: whether an account is
/// signed in can ask the macOS Keychain (`security`), which blocks.
pub async fn list_now(shared: &Arc<Shared>) -> Vec<AccountJson> {
    let shared = shared.clone();
    tokio::task::spawn_blocking(move || crate::api::account_list(&shared))
        .await
        .unwrap_or_default()
}

/// One account as the apps see it, off obid's async runtime ([`list_now`]).
async fn json_now(shared: &Arc<Shared>, tool: Tool, account: Account) -> Option<AccountJson> {
    let shared = shared.clone();
    tokio::task::spawn_blocking(move || crate::api::account_json(&shared, tool, &account))
        .await
        .ok()
}

/// Tells every app the accounts as they are now (`accounts`, docs/runner-api.md, "GET /v1/events").
pub async fn send_accounts(shared: &Arc<Shared>) {
    let list = list_now(shared).await;
    shared
        .events
        .send("accounts", serde_json::to_value(list).unwrap_or_default());
}

// ---------------------------------------------------------------------------------------------
// POST /v1/accounts, PATCH and DELETE /v1/accounts/{id}

const NEW_SHAPE: &str = "{\"agent\": \"claude\" | \"codex\" | \"cursor\", \"name\": \"…\" (optional), \"payer\": \"…\"}";
const DEFAULT_SHAPE: &str = "{\"agent\": \"claude\" | \"codex\" | \"cursor\", \"default\": true}";

/// `POST /v1/accounts`: a new account for Claude Code, Codex or Cursor, in a folder obid makes,
/// `home/accounts/<id>` (0700), under the name given, else the tool's own until its first sign-in
/// fills it (ACCOUNTS-1, ACCOUNTS-2). `201` with the Account.
pub async fn add(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    let map = object(&body, &["agent", "name", "payer", "default"], NEW_SHAPE)?;
    let agent = text_field(&map, "agent")?
        .ok_or_else(|| unprocessable(format!("agent is missing: the body is {NEW_SHAPE}.")))?;
    let tool = Tool::from_id(agent).ok_or_else(|| {
        unprocessable(format!("agent is claude, codex or cursor, not {agent:?}."))
    })?;
    if map.contains_key("default") {
        if map.len() != 2 || map.get("default") != Some(&Value::Bool(true)) {
            return Err(unprocessable(format!(
                "The tool's own login comes back with {DEFAULT_SHAPE}."
            )));
        }
        return restore_default(shared, tool).await;
    }
    let text = text_field(&map, "name")?;
    let payer = payer(map.get("payer"))?;
    let home = shared.config.home.clone();
    let added = shared.config.agents.change(|agents, saved| {
        if all(agents).len() >= most_accounts() {
            return Err(unprocessable(format!(
                "obid keeps at most {} accounts: remove one first.",
                most_accounts()
            )));
        }
        let name = match text {
            Some(text) => name(agents, tool, text, None)?,
            None => free_name(agents, tool, tool.name(), None)
                .ok_or_else(|| unprocessable("name is missing and the tool's own is taken."))?,
        };
        let id = new_id(agents, saved);
        let config_dir = own_folder(&home, &id).expect("obid's own id");
        make_folder(&config_dir).map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("obid couldn't make the account's folder: {e}"),
            )
        })?;
        let added = Added {
            id,
            agent: tool.id().into(),
            name,
            payer,
            config_dir,
            fresh: true,
            auto_name: text.is_none(),
        };
        saved.added.push(added.clone());
        Ok(added)
    })?;
    tracing::info!("account {}: added ({})", added.id, tool.id());
    send_accounts(&shared).await;
    Ok(answer(
        StatusCode::CREATED,
        json_now(&shared, tool, added.account()).await,
    ))
}

/// `POST /v1/accounts` `{"agent", "default": true}`: the tool's own login (`default`, `codex`,
/// `cursor`), removed earlier or found on this Mac ([`Live::available`]), listed: no folder,
/// signed in with the tool itself. `201` with the Account; `409` while it's listed, or when it was
/// neither removed nor found.
async fn restore_default(shared: Arc<Shared>, tool: Tool) -> Result<Response, ApiError> {
    let own = own_login(tool);
    // The configuration may name an account with that id and a folder of its own: that one isn't
    // the tool's own login, and doesn't come back here.
    let seeded = find(&shared.config.agents.read().seed, &own.id);
    if seeded.is_some_and(|(_, a)| a.config_dir.is_some()) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!(
                "obid's configuration gives {} a folder of its own: it isn't {}'s own login, and can't be added back here.",
                own.id,
                tool.name()
            ),
        ));
    }
    let (seed, found) = {
        let inner = shared.config.agents.read();
        (inner.seed.clone(), inner.found.contains(&tool))
    };
    shared.config.agents.change(|agents, saved| {
        if find(agents, &own.id).is_some() {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                format!("{}'s own login is listed already.", tool.name()),
            ));
        }
        if all(agents).len() >= most_accounts() {
            return Err(unprocessable(format!(
                "obid keeps at most {} accounts: remove one first.",
                most_accounts()
            )));
        }
        // Removed here, or found on this Mac at start and not listed (Live::available).
        let removed = saved.removed.contains(&own.id);
        if !(removed || (found && addable(&seed, agents, tool))) {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                format!(
                    "{}'s own login wasn't removed here, and obid didn't find it on this Mac.",
                    tool.name()
                ),
            ));
        }
        if !removed && !saved.own.iter().any(|t| t == tool.id()) {
            saved.own.push(tool.id().into());
        }
        saved.removed.retain(|id| id != &own.id);
        saved.changed.remove(&own.id);
        Ok(())
    })?;
    // Alone, an empty list is the tool's own login again, with nothing listed for it.
    let (tool, account) = find(&shared.config.agents.get(), &own.id).unwrap_or((tool, own.clone()));
    tracing::info!("account {}: the tool's own login is back", own.id);
    tokio::spawn(read_identities(shared.clone(), Some(own.id.clone())));
    send_accounts(&shared).await;
    Ok(answer(
        StatusCode::CREATED,
        json_now(&shared, tool, account).await,
    ))
}

/// A new folder of obid's own (0700), in a private `home/accounts` (or `home/tmp`).
fn make_folder(dir: &Path) -> std::io::Result<()> {
    if let Some(parent) = dir.parent() {
        crate::store::make_private_dir(parent)?;
    }
    let mut builder = std::fs::DirBuilder::new();
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

const CHANGE_SHAPE: &str = "{\"name\": \"…\", \"payer\": \"…\" | null, \"model\": \"…\" | null, \"effort\": \"…\" | null, \"complexEffort\": \"…\" | null}";

/// `PATCH /v1/accounts/{id}`: a new name, a new payer (null for none), and its model, effort and
/// complex-task effort (#1574; null for the tool's own), any of them. The Account.
pub async fn change(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let map = object(
        &body,
        &["name", "payer", "model", "effort", "complexEffort"],
        CHANGE_SHAPE,
    )?;
    let text = text_field(&map, "name")?;
    let new_payer = map.contains_key("payer").then(|| payer(map.get("payer")));
    let (tool, _) = find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?;
    let offered = shared.jobs.model_lists.for_account(tool, &id);
    let tuned = tuning(&map, tool, &offered)?;
    // The name and payer first: a refused one keeps the model and efforts as they were too.
    let (tool, account) = if text.is_none() && new_payer.is_none() {
        find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?
    } else {
        shared.config.agents.change(|agents, saved| {
            let (tool, mut account) = find(agents, &id).ok_or_else(|| no_account(&id))?;
            if let Some(text) = text {
                account.name = name(agents, tool, text, Some(&id))?;
            }
            if let Some(new_payer) = new_payer {
                account.payer = new_payer?;
            }
            match saved.added.iter_mut().find(|a| a.id == id) {
                Some(added) => {
                    added.auto_name &= text.is_none();
                    added.name = account.name.clone();
                    added.payer = account.payer.clone();
                }
                None => {
                    saved.changed.insert(
                        id.clone(),
                        Change {
                            name: account.name.clone(),
                            payer: account.payer.clone(),
                        },
                    );
                }
            }
            Ok((tool, account))
        })?
    };
    if !tuned.is_empty() {
        crate::settings::update(&shared.config.home, |s| {
            let kept = s.tuning.entry(id.clone()).or_default();
            for (key, value) in tuned {
                *match key {
                    "model" => &mut kept.model,
                    "effort" => &mut kept.effort,
                    _ => &mut kept.complex_effort,
                } = value;
            }
            if *kept == crate::settings::Tuning::default() {
                s.tuning.remove(&id);
            }
        })
        .map_err(|e| unkept(Path::new("settings.json"), e))?;
    }
    // A conversation's Claude Code waiting on the account starts anew on it as it is now.
    shared.harness.end_warm_on(&id);
    send_accounts(&shared).await;
    Ok(answer(
        StatusCode::OK,
        json_now(&shared, tool, account).await,
    ))
}

/// The model and efforts a `PATCH` sets, each a value or null (the tool's own): a model `tool`
/// offers for this account ([`crate::model_lists`]), and an effort it takes ([`crate::settings::efforts`]).
fn tuning(
    map: &Map<String, Value>,
    tool: Tool,
    offered: &[crate::model_lists::Offered],
) -> Result<Vec<(&'static str, Option<String>)>, ApiError> {
    let mut tuned = Vec::new();
    for key in ["model", "effort", "complexEffort"] {
        let value = match map.get(key) {
            None => continue,
            Some(Value::Null) => None,
            Some(Value::String(v)) if key == "model" && offered.iter().any(|m| m.id == *v) => {
                Some(v.clone())
            }
            Some(Value::String(v))
                if key != "model" && crate::settings::efforts(tool).contains(&v.as_str()) =>
            {
                Some(v.clone())
            }
            Some(_) if key == "model" => {
                let ids: Vec<_> = offered.iter().map(|m| m.id.as_str()).collect();
                return Err(unprocessable(format!(
                    "model is one of {}'s: {}, or null.",
                    tool.name(),
                    ids.join(", ")
                )));
            }
            Some(_) => {
                let efforts = crate::settings::efforts(tool);
                return Err(unprocessable(if efforts.is_empty() {
                    format!("{} has no effort: {key} is null for it.", tool.name())
                } else {
                    format!("{key} is one of {}, or null.", efforts.join(", "))
                }));
            }
        };
        tuned.push((key, value));
    }
    Ok(tuned)
}

/// `POST /v1/accounts/{id}/reserve` `{"lift": true | false}`: lifts the account's weekly reserve
/// until its week resets, or puts it back now (crate::usage). The Account.
pub async fn reserve(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let shape = "{\"lift\": true | false}";
    let map = object(&body, &["lift"], shape)?;
    let Some(lift) = map.get("lift").and_then(Value::as_bool) else {
        return Err(unprocessable(format!(
            "lift is true or false: the body is {shape}."
        )));
    };
    let (tool, account) = find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?;
    shared
        .jobs
        .usage
        .lift(&id, lift)
        .map_err(|e| unkept(Path::new("usage.json"), e))?;
    shared.jobs.reserve_changed();
    send_accounts(&shared).await;
    Ok(answer(
        StatusCode::OK,
        json_now(&shared, tool, account).await,
    ))
}

/// `POST /v1/accounts/{id}/background` `{"given": true | false}`: whether work nobody's watching may
/// use this ChatGPT login (CHATGPT-20). A yes only from a click in an app of the owner's, signed by
/// its device's key: never the token alone, and so never a job or an agent. A no takes nothing it
/// shouldn't, so it's kept unsigned too, and a key that can't sign never keeps anyone from
/// withdrawing it. Off the gateway, like signing in. The Account.
pub async fn background(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let shape = "{\"given\": true | false}";
    let map = object(&body, &["given"], shape)?;
    let Some(given) = map.get("given").and_then(Value::as_bool) else {
        return Err(unprocessable(format!(
            "given is true or false: the body is {shape}."
        )));
    };
    let (tool, account) = find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?;
    if tool != Tool::Codex || crate::chatgpt::marker_of(&account).is_none() {
        return Err(unprocessable(format!(
            "{} isn't signed in with ChatGPT.",
            account.name
        )));
    }
    let device = crate::routes::signed_by(&shared, "POST", &uri, &headers, &body);
    if given && device.is_none() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Only a click in Oby on your own device can allow this.",
        ));
    }
    crate::settings::update(&shared.config.home, |s| {
        s.background_consent.insert(
            id.clone(),
            crate::settings::BackgroundConsent {
                given,
                at: obi_protocol::time::now(),
                device,
            },
        );
    })
    .map_err(|e| unkept(Path::new("settings.json"), e))?;
    shared.jobs.background_answered(&id, given);
    send_accounts(&shared).await;
    Ok(answer(
        StatusCode::OK,
        json_now(&shared, tool, account).await,
    ))
}

/// `DELETE /v1/accounts/{id}`: `204`. A sign-in under way ends first; then `409` while a job of
/// the account is queued or running. Its folder goes too when obid made it. A ChatGPT login is
/// revoked first and its entry deleted; `200 {"revoked": false}` when OpenAI didn't confirm it.
pub async fn remove(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
) -> Result<Response, ApiError> {
    find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?;
    // One change of the account at a time: a sign-in can't start while it's removed. Looked up
    // again once held: it may have gone meanwhile.
    let lock = shared.sign_ins.lock_for(&id);
    let _held = lock.lock().await;
    let (_, account) = find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?;
    if !shared.sign_ins.end(&id).await {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!(
                "{}'s sign-in didn't end within 5 seconds: try again in a moment.",
                account.name
            ),
        ));
    }
    let _changing = shared
        .jobs
        .begin_change(&id)
        .ok_or_else(|| busy(&account))?;
    let revoked = shared.chatgpt.remove(&id).await;
    forget_consent(&shared, &id);
    forget_account(&shared, &account)?;
    send_accounts(&shared).await;
    if !revoked {
        return Ok(answer(StatusCode::OK, json!({"revoked": false})));
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The answer about a ChatGPT login goes with the login.
fn forget_consent(shared: &Shared, id: &str) {
    if let Err(e) = crate::settings::update(&shared.config.home, |s| {
        s.background_consent.remove(id);
    }) {
        tracing::error!("can't forget what the owner said about {id}: {e}");
    }
}

/// Takes `account` off, its [`crate::jobs::Changing`] held: its login copy, its folder when obid
/// made it, its usage and its settings. What the tool itself keeps stays.
fn forget_account(shared: &Shared, account: &Account) -> Result<(), ApiError> {
    let home = shared.config.home.clone();
    let id = account.id.clone();
    // No conversation's Claude Code waits on its login copy any more (none keeps one meanwhile:
    // [`Jobs::being_changed`]).
    shared.harness.end_warm_on(&id);
    let made = shared.config.agents.change(|agents, saved| {
        find(agents, &id).ok_or_else(|| no_account(&id))?;
        saved.identities.remove(&id);
        let made = match saved.added.iter().position(|a| a.id == id) {
            Some(at) => {
                let added = saved.added.remove(at);
                own_folder(&home, &id).filter(|own| added.config_dir == *own)
            }
            None => {
                saved.changed.remove(&id);
                None
            }
        };
        retire(saved, account);
        Ok(made)
    })?;
    // Never through a link in the folder's place: only a real folder is removed, and what's in it
    // without following links.
    if let Some(dir) = made
        && std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir())
        && let Err(e) = std::fs::remove_dir_all(&dir)
    {
        tracing::warn!("account {id}: its folder {} stayed: {e}", dir.display());
    }
    // The tool's own login stays where the tool kept it (on macOS, Claude Code's Keychain item)
    // and with its provider.
    shared.sign_ins.forget(&id);
    if let Err(e) = shared.jobs.usage.forget(&id) {
        tracing::warn!("account {id}: its usage stayed in usage.json: {e}");
    }
    shared.jobs.model_lists.forget(&id);
    tracing::info!("account {id}: removed");
    // Its model and efforts go with it, and new tasks go by routing's own order again.
    if let Err(e) = crate::settings::update(&shared.config.home, |s| {
        s.tuning.remove(&id);
        s.default_account.take_if(|d| *d == id);
        s.talk_account.take_if(|d| *d == id);
    }) {
        tracing::warn!("account {id}: its settings stay: {e}");
    }
    shared.conversations.forget_account(&id);
    Ok(())
}

/// `DELETE /v1/accounts/{id}/sign-in`: ends the account's sign-in under way, if any, so an
/// abandoned one doesn't keep jobs off the account until it runs out. `204`.
pub async fn end_sign_in(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
) -> Result<Response, ApiError> {
    find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?;
    // A sign-in starting now finishes registering first, so it's the one ended.
    let lock = shared.sign_ins.lock_for(&id);
    let _held = lock.lock().await;
    let (_, account) = find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?;
    if !shared.sign_ins.end(&id).await {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!(
                "{}'s sign-in didn't end within 5 seconds: try again in a moment.",
                account.name
            ),
        ));
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// What's kept of a removed account: its id, never made again ([`new_id`]) nor laid over the
/// configuration again, and its folder, still hidden from every box.
fn retire(saved: &mut Saved, account: &Account) {
    if !saved.removed.contains(&account.id) {
        saved.removed.push(account.id.clone());
    }
    if let Some(dir) = &account.config_dir
        && !saved.retired.contains(dir)
    {
        saved.retired.push(dir.clone());
    }
}

fn busy(account: &Account) -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        format!(
            "{} has a job queued or running: stop it first. A stopped job can move to another account.",
            account.name
        ),
    )
    .because("busy")
}

// ---------------------------------------------------------------------------------------------
// Signing in

/// The sign-ins under way, one per account at most.
#[derive(Default)]
pub struct SignIns {
    running: Mutex<HashMap<String, Arc<SignIn>>>,
    /// One change of an account at a time (a sign-in starting, the account removed).
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// The accounts a sign-in took off as another's login again, with that other's id, so the app
    /// that was waiting on one can ask (ACCOUNTS-7). ponytail: kept until obid restarts; at most
    /// one short entry per account added.
    same: Mutex<HashMap<String, String>>,
}

/// One sign-in under way: the tool's own login, waiting for the person.
pub struct SignIn {
    account: String,
    url: String,
    /// Codex: the one-time code the person types on the page.
    user_code: Option<String>,
    /// Claude Code: the page shows a code the person pastes back (`POST`).
    paste: bool,
    expires: i64,
    /// The login's process group.
    group: Option<u32>,
    stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    /// Whether it signed the account in, once it ended.
    ended: tokio::sync::watch::Receiver<Option<bool>>,
    cancel: tokio::sync::Notify,
    /// Continue with ChatGPT, not the tool's own login.
    chatgpt: bool,
}

impl SignIns {
    fn get(&self, id: &str) -> Option<Arc<SignIn>> {
        self.running().get(id).cloned()
    }

    /// Whether the account's sign-in is under way.
    pub(crate) fn under_way(&self, id: &str) -> bool {
        self.running().contains_key(id)
    }

    fn running(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<SignIn>>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The lock for changing the account `id` (one that exists: at most one per account obid had).
    fn lock_for(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(id.to_owned())
            .or_default()
            .clone()
    }

    /// Ends the account's sign-in, if one is under way, and waits until it has: false when it
    /// hadn't within 5 seconds.
    async fn end(&self, id: &str) -> bool {
        let Some(sign_in) = self.get(id) else {
            return true;
        };
        sign_in.cancel.notify_one();
        let mut ended = sign_in.ended.clone();
        tokio::time::timeout(Duration::from_secs(5), ended.wait_for(Option::is_some))
            .await
            .is_ok()
    }

    /// The account `id` was another's login again: said here until obid restarts.
    fn same_as(&self, id: &str) -> Option<String> {
        self.same
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }

    /// Forgets the account's lock (the account is gone).
    fn forget(&self, id: &str) {
        self.locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
    }

    /// Kills every sign-in's whole process group (obid is stopping).
    pub fn kill_all(&self) {
        for sign_in in self.running().values() {
            if let Some(group) = sign_in.group {
                crate::process::signal_group(group, nix::sys::signal::Signal::SIGKILL);
            }
        }
    }
}

impl SignIn {
    fn to_json(&self) -> Value {
        json!({"account": self.account, "url": self.url, "userCode": self.user_code,
               "pasteCode": self.paste, "expiresAt": obi_protocol::time::format(self.expires)})
    }
}

/// Removes the folders sign-ins left in `home/tmp` when obid stopped before they ended: those
/// whose names start with `prefix`, accounts' ([`SCRATCH_PREFIX`]) or GitHub's.
pub fn sweep(home: &Path, prefix: &str) {
    let Ok(entries) = std::fs::read_dir(home.join("tmp")) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(prefix)
            && entry.file_type().is_ok_and(|t| t.is_dir())
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// `GET /v1/accounts/{id}/sign-in`: the account's sign-in under way, or a new one: the tool's own
/// login in the account's folder, and what the person needs from it (`url`, `userCode`,
/// `pasteCode`).
pub async fn sign_in(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let with_chatgpt = query.get("with").is_some_and(|with| with == "chatgpt");
    if let Some(other) = shared.sign_ins.same_as(&id) {
        return Ok(same_account(&shared, &other));
    }
    find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?;
    // One change of the account at a time; looked up again once held (it may have been removed).
    let lock = shared.sign_ins.lock_for(&id);
    let _held = lock.lock().await;
    let agents = shared.config.agents.get();
    let (tool, account) = find(&agents, &id).ok_or_else(|| no_account(&id))?;
    if let Some(sign_in) = shared.sign_ins.get(&id) {
        // A sign-in of the other kind under way: one at a time, and the one under way isn't touched.
        if sign_in.chatgpt != with_chatgpt {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                format!(
                    "{} is signing in another way: finish or cancel that first.",
                    account.name
                ),
            )
            .because("busy"));
        }
        return Ok(answer(StatusCode::OK, sign_in.to_json()));
    }
    // Each refusal carries its stable reason (docs/contracts/sign-in.md, SIGN-IN-1 to SIGN-IN-7).
    let conflict = |reason: &'static str, message: String| {
        Err(ApiError::new(StatusCode::CONFLICT, message).because(reason))
    };
    let Some(dir) = account.config_dir.clone() else {
        return conflict(
            "ownLogin",
            format!(
                "{} is {}'s own login, the person's: obid signs in only accounts with a folder of their own. Sign in with {} itself on this machine.",
                account.name,
                tool.name(),
                tool.name()
            ),
        );
    };
    if let Err(why) = login_paths_safe(tool, &dir) {
        return conflict(
            "folderUnsafe",
            format!(
                "{}'s folder has {why}: obid never runs a sign-in through it.",
                account.name
            ),
        );
    }
    if with_chatgpt && tool != Tool::Codex {
        return Err(unprocessable(
            "Continue with ChatGPT is for Codex accounts.",
        ));
    }
    if tool == Tool::Claude && cfg!(target_os = "macos") && !crate::jobs::keychain_on() {
        return conflict("keychainOff", "On macOS Claude Code keeps its login in the Keychain, which obid uses only with claudeKeychain on: obid turns it on by itself when it's left out and Claude Code's login is only in the Keychain, or set it to true in its configuration.".into());
    }
    let bin = agent_of(&agents, tool)
        .as_ref()
        .and_then(|agent| crate::process::resolve_bin(&agent.bin))
        .filter(|bin| bin.is_absolute())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "obid can't find {}'s binary by an absolute path, which is how it runs a sign-in.",
                    tool.name()
                ),
            )
            .because("notInstalled")
        })?;
    let changing = shared
        .jobs
        .begin_change(&id)
        .ok_or_else(|| busy(&account))?;
    // A conversation's Claude Code waiting on the old login ends: the next turn copies the new one.
    shared.harness.end_warm_on(&id);
    // The account's own folder, checked as it's made: a link or a file in its place is the
    // folder's problem, not the login's.
    crate::jobs::not_a_link(&dir)
        .and_then(|()| {
            crate::store::make_private_dir(&dir)
                .map_err(|e| format!("obid can't make the account's folder: {e}"))
        })
        .map_err(|why| ApiError::new(StatusCode::BAD_GATEWAY, why).because("folderUnsafe"))?;
    let sign_in = if with_chatgpt {
        start_chatgpt(&shared, tool, &account, changing).await
    } else {
        start(&shared, tool, &account, &bin, &dir, changing).await
    }
    .map_err(|why| ApiError::new(StatusCode::BAD_GATEWAY, why).because("didNotStart"))?;
    Ok(answer(StatusCode::OK, sign_in.to_json()))
}

/// Continue with ChatGPT: the listener runs and the page's address is answered, then the account
/// stays off jobs until the person's browser comes back, they cancel, or it runs out
/// (crate::chatgpt). How it ended shows as the account's `signedIn`, or its `label`.
async fn start_chatgpt(
    shared: &Arc<Shared>,
    tool: Tool,
    account: &Account,
    changing: crate::jobs::Changing,
) -> Result<Arc<SignIn>, String> {
    let mut flow = shared.chatgpt.begin(&account.id).await?;
    let (ended_tx, ended) = tokio::sync::watch::channel(None);
    let sign_in = Arc::new(SignIn {
        account: account.id.clone(),
        url: flow.url.clone(),
        user_code: None,
        paste: false,
        expires: obi_protocol::time::now() + SIGN_IN_LASTS.as_secs() as i64,
        group: None,
        stdin: tokio::sync::Mutex::new(None),
        ended,
        cancel: tokio::sync::Notify::new(),
        chatgpt: true,
    });
    shared
        .sign_ins
        .running()
        .insert(account.id.clone(), sign_in.clone());
    tracing::info!("account {}: ChatGPT sign-in started", account.id);
    let (shared, watched, account) = (shared.clone(), sign_in.clone(), account.clone());
    tokio::spawn(async move {
        let signed = tokio::select! {
            signed = flow.ended() => signed,
            () = tokio::time::sleep(SIGN_IN_LASTS) => false,
            () = watched.cancel.notified() => false,
        };
        flow.stop();
        if signed {
            shared.jobs.usage.forget_priced(&account.id);
            let who = crate::chatgpt::marker_of(&account).and_then(|m| m.email);
            shared.config.agents.learn(
                tool,
                &account.id,
                Identity {
                    email: who,
                    plan: None,
                    org: None,
                },
                true,
            );
            tracing::info!("account {}: signed in with ChatGPT", account.id);
        } else {
            tracing::info!(
                "account {}: ChatGPT sign-in ended without signing in",
                account.id
            );
        }
        drop(changing);
        // (it stops saying `signingIn` only once jobs may use it)
        {
            let mut running = shared.sign_ins.running();
            if running
                .get(&account.id)
                .is_some_and(|s| Arc::ptr_eq(s, &watched))
            {
                running.remove(&account.id);
            }
        }
        let _ = ended_tx.send(Some(signed));
        send_accounts(&shared).await;
    });
    Ok(sign_in)
}

/// `409` for an account a sign-in took off as `other`'s login again (ACCOUNTS-7): `sameAs` is the
/// account it was.
fn same_account(shared: &Shared, other: &str) -> Response {
    let name = find(&shared.config.agents.get(), other)
        .map_or_else(|| "another account".to_owned(), |(_, account)| account.name);
    answer(
        StatusCode::CONFLICT,
        json!({"error": format!("That's {name} again."), "reason": "sameAccount", "sameAs": other}),
    )
}

/// Whether what a tool's login writes in the account's folder `dir` is the person's own: the
/// folder, and the login file, its locks and the folders on the way to it, where they're there,
/// each a real file or folder (not a link) owned by the person. Else why not.
pub(crate) fn login_paths_safe(tool: Tool, dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let uid = nix::unistd::getuid().as_raw();
    let check = |path: &Path, shown: &str| match std::fs::symlink_metadata(path) {
        Err(_) => Ok(()),
        Ok(meta) if meta.file_type().is_symlink() => Err(format!("{shown} as a link")),
        Ok(meta) if meta.uid() != uid => Err(format!("{shown} owned by someone else")),
        Ok(_) => Ok(()),
    };
    check(dir, "its folder itself")?;
    let (prefixes, paths): (&[&str], &[&str]) = match tool {
        Tool::Claude => (&[".credentials"], &[]),
        Tool::Codex => (&["auth.json"], &[]),
        Tool::Cursor => (
            &[],
            &[
                ".cursor",
                ".cursor/auth.json",
                ".config",
                ".config/cursor",
                ".config/cursor/auth.json",
            ],
        ),
    };
    for name in paths {
        check(&dir.join(name), name)?;
    }
    if !prefixes.is_empty()
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if prefixes.iter().any(|p| name.starts_with(p)) {
                check(&entry.path(), &name)?;
            }
        }
    }
    Ok(())
}

/// `POST /v1/accounts/{id}/sign-in` with `{"code": "…"}`: the code the page showed, to the login
/// waiting for it (Claude Code's). The Account once it's signed in.
pub async fn sign_in_code(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    if let Some(other) = shared.sign_ins.same_as(&id) {
        return Ok(same_account(&shared, &other));
    }
    let (tool, account) = find(&shared.config.agents.get(), &id).ok_or_else(|| no_account(&id))?;
    let map = object(&body, &["code"], "{\"code\": \"…\"}")?;
    let code = text_field(&map, "code")?.unwrap_or_default().trim();
    if code.is_empty() || code.len() > 4096 || code.chars().any(char::is_control) {
        return Err(unprocessable(
            "code is the one line the sign-in page showed.",
        ));
    }
    let conflict = |message: String| ApiError::new(StatusCode::CONFLICT, message);
    let sign_in = shared.sign_ins.get(&id).ok_or_else(|| {
        conflict(format!(
            "There's no sign-in under way for {}: start one first.",
            account.name
        ))
    })?;
    if !sign_in.paste {
        return Err(conflict(format!(
            "{}'s sign-in takes no code: it ends by itself once you've signed in in the browser.",
            tool.name()
        )));
    }
    let mut ended = sign_in.ended.clone();
    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = sign_in.stdin.lock().await;
        let mut pipe = stdin
            .take()
            .ok_or_else(|| conflict("A code went to this sign-in already.".into()))?;
        let line = format!("{code}\n");
        if pipe.write_all(line.as_bytes()).await.is_err() || pipe.flush().await.is_err() {
            return Err(conflict(format!(
                "{}'s sign-in has ended: start again.",
                tool.name()
            )));
        }
    }
    let signed = tokio::time::timeout(CODE_WITHIN, ended.wait_for(Option::is_some))
        .await
        .ok()
        .and_then(|r| r.ok().and_then(|e| *e));
    match signed {
        Some(true) => {
            if let Some(other) = shared.sign_ins.same_as(&id) {
                return Ok(same_account(&shared, &other));
            }
            let account = find(&shared.config.agents.get(), &id)
                .map(|(_, a)| a)
                .unwrap_or(account);
            Ok(answer(
                StatusCode::OK,
                json_now(&shared, tool, account).await,
            ))
        }
        Some(false) => Err(unprocessable(format!(
            "{} didn't sign in with that code. Start again for a new link.",
            tool.name()
        ))
        .because("codeRefused")),
        None => Err(ApiError::new(
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                "{} didn't finish signing in within {} seconds.",
                tool.name(),
                CODE_WITHIN.as_secs()
            ),
        )),
    }
}

/// A new folder for one run of a tool's login or status, in `home/tmp`, swept at every start
/// ([`sweep`]).
fn scratch_folder(shared: &Shared) -> Result<PathBuf, String> {
    static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = shared
        .config
        .home
        .join("tmp")
        .join(format!("{SCRATCH_PREFIX}{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    make_folder(&scratch).map_err(|e| format!("obid can't make a folder for the sign-in: {e}"))?;
    Ok(scratch)
}

/// Starts the tool's own login in the account's folder `dir` and waits for its link. No box ever
/// gets that folder (#655), so nothing a job wrote there steers the login, and the login writes
/// its sign-in where the tool keeps it, read by nobody but the tool itself and the copies obid
/// makes for its boxed runs:
///
/// - Claude Code: `claude auth login` with `CLAUDE_CONFIG_DIR` the account's folder (on macOS its
///   login then goes to that folder's Keychain item). With no terminal it prints a link whose page
///   shows a code to paste back on its stdin.
/// - Codex: `codex login --device-auth`, its login kept in a file, with `CODEX_HOME` and `HOME` the
///   account's folder: a link and a one-time code to type there; it ends by itself.
/// - Cursor: `cursor-agent login` with the account's folders and its login kept in a file, its
///   `cli-config.json` rewritten first ([`crate::acp::cursor::rewrite_config`]); it ends by itself.
///
/// `changing` keeps jobs off the account until the login ends.
async fn start(
    shared: &Arc<Shared>,
    tool: Tool,
    account: &Account,
    bin: &Path,
    dir: &Path,
    changing: crate::jobs::Changing,
) -> Result<Arc<SignIn>, String> {
    let scratch = scratch_folder(shared)?;
    let child = command(tool, bin, dir, &scratch).and_then(|mut c| {
        c.spawn()
            .map_err(|e| format!("obid couldn't start {}'s sign-in: {e}", tool.name()))
    });
    let mut child = match child {
        Ok(child) => child,
        Err(why) => {
            let _ = std::fs::remove_dir_all(&scratch);
            return Err(why);
        }
    };
    let group = child.id();
    // Until the sign-in is registered, a request that ends (the app gone, obid stopping) takes
    // the login and its folder with it.
    let mut pending = Pending {
        group,
        scratch: scratch.clone(),
        armed: true,
    };
    let kill = move || {
        if let Some(group) = group {
            crate::process::signal_group(group, nix::sys::signal::Signal::SIGKILL);
        }
    };
    // What it prints, a line at a time (at most [`LONGEST_LINE`] of each), read until it ends:
    // only the link and the code are taken.
    let (lines, mut said) = tokio::sync::mpsc::unbounded_channel::<String>();
    for out in [
        child
            .stdout
            .take()
            .map(|o| Box::new(o) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        child
            .stderr
            .take()
            .map(|e| Box::new(e) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
    ]
    .into_iter()
    .flatten()
    {
        tokio::spawn(read_lines(out, lines.clone()));
    }
    drop(lines);
    let wanted = async {
        let (mut url, mut code) = (None, None);
        while let Some(line) = said.recv().await {
            let line = plain(&line);
            url = url.or_else(|| link(tool, &line));
            if tool == Tool::Codex && url.is_some() {
                code = code.or_else(|| user_code(&line));
            }
            if url.is_some() && (tool != Tool::Codex || code.is_some()) {
                return Some((url?, code));
            }
        }
        None
    };
    let found = tokio::time::timeout(LINK_WITHIN, wanted).await;
    let (url, user_code) = match found {
        Ok(Some(found)) => found,
        _ => {
            kill();
            let _ = child.wait().await;
            let _ = std::fs::remove_dir_all(&scratch);
            // Reaped: its process group may be another's by now.
            pending.armed = false;
            return Err(format!(
                "{}'s sign-in didn't give a link to open.",
                tool.name()
            ));
        }
    };
    // The rest of what it prints is read and dropped.
    tokio::spawn(async move { while said.recv().await.is_some() {} });
    let (ended_tx, ended) = tokio::sync::watch::channel(None);
    let sign_in = Arc::new(SignIn {
        account: account.id.clone(),
        url,
        user_code,
        paste: tool == Tool::Claude,
        expires: obi_protocol::time::now() + SIGN_IN_LASTS.as_secs() as i64,
        group,
        stdin: tokio::sync::Mutex::new(child.stdin.take()),
        ended,
        cancel: tokio::sync::Notify::new(),
        chatgpt: false,
    });
    shared
        .sign_ins
        .running()
        .insert(account.id.clone(), sign_in.clone());
    // From here the sign-in's own task ends it.
    pending.armed = false;
    tracing::info!("account {}: {} sign-in started", account.id, tool.id());
    let (shared, watched, account) = (shared.clone(), sign_in.clone(), account.clone());
    tokio::spawn(async move {
        let exited = tokio::select! {
            status = child.wait() => status.ok(),
            () = tokio::time::sleep(SIGN_IN_LASTS) => None,
            () = watched.cancel.notified() => None,
        };
        // Whatever it started goes with it.
        kill();
        if exited.is_none() {
            let _ = child.wait().await;
        }
        let _ = std::fs::remove_dir_all(&scratch);
        let signed = match exited {
            Some(status) if status.success() => {
                // A ChatGPT login goes only now that this one took (CHATGPT-16).
                if tool == Tool::Codex
                    && let Some(login) = account.config_dir.as_ref().map(|d| d.join("auth.json"))
                    && login.is_file()
                    && crate::jobs::codex_copy_ready(&login, &account.name).is_ok()
                {
                    // Whoever signs in with ChatGPT here next is asked anew: the yes goes before
                    // the account shows its new login, so no job takes the new one with the old yes.
                    forget_consent(&shared, &account.id);
                    shared.chatgpt.replaced_by_device_login(&account.id).await;
                }
                let account = account.clone();
                tokio::task::spawn_blocking(move || {
                    crate::jobs::signed_in(tool, &account, crate::jobs::login_home().as_deref())
                })
                .await
                .unwrap_or(false)
            }
            _ => false,
        };
        {
            let mut running = shared.sign_ins.running();
            if running
                .get(&account.id)
                .is_some_and(|s| Arc::ptr_eq(s, &watched))
            {
                running.remove(&account.id);
            }
        }
        if signed {
            shared.jobs.usage.forget_priced(&account.id);
            // Before jobs may use it: it may be another account's login again.
            settle_sign_in(&shared, tool, &account).await;
        }
        drop(changing);
        let _ = ended_tx.send(Some(signed));
        if signed {
            tracing::info!("account {}: signed in", account.id);
        } else {
            tracing::info!("account {}: sign-in ended without signing in", account.id);
        }
        send_accounts(&shared).await;
    });
    Ok(sign_in)
}

/// A sign-in took: who the account is, from its tool. One just added with the email, org and plan
/// of another signed-in account of the tool is that account again: taken off, and said at its sign-in routes
/// (ACCOUNTS-7). A tool that doesn't say keeps the account; so does signing an account in again.
async fn settle_sign_in(shared: &Arc<Shared>, tool: Tool, account: &Account) {
    let agents = &shared.config.agents;
    let who = read_identity(shared, tool, account)
        .await
        .unwrap_or_default();
    if agents.fresh(&account.id)
        && who.email.is_some()
        && let Some(other) = same_login(shared, tool, &account.id, &who).await
    {
        // Said before it's gone: an app that sees it gone asks at once.
        let same = || {
            shared
                .sign_ins
                .same
                .lock()
                .unwrap_or_else(|e| e.into_inner())
        };
        same().insert(account.id.clone(), other);
        match forget_account(shared, account) {
            Ok(()) => {
                return tracing::info!("account {}: another's login again, removed", account.id);
            }
            Err(e) => tracing::warn!("account {}: stayed: {}", account.id, e.message),
        }
        same().remove(&account.id);
    }
    agents.learn(tool, &account.id, who, true);
}

/// Another signed-in account of `tool` that is `who`, asking the tool of any that hasn't said yet.
/// One email can hold a personal plan and a team's seat, so its org and plan must match too: a plan
/// that's changed since gives an extra account, never a lost one.
async fn same_login(shared: &Arc<Shared>, tool: Tool, id: &str, who: &Identity) -> Option<String> {
    let agents = &shared.config.agents;
    let others = agent_of(&agents.get(), tool).as_ref()?.accounts.clone();
    for other in others.into_iter().filter(|a| a.id != id) {
        if !signed_in_now(tool, &other).await {
            continue;
        }
        if agents.identity(&other.id).email.is_none()
            && let Some(who) = read_identity(shared, tool, &other).await
        {
            agents.learn(tool, &other.id, who, false);
        }
        let known = agents.identity(&other.id);
        let same_email = known
            .email
            .as_deref()
            .zip(who.email.as_deref())
            .is_some_and(|(known, email)| known.eq_ignore_ascii_case(email));
        if same_email && known.org == who.org && known.plan == who.plan {
            return Some(other.id);
        }
    }
    None
}

/// Whether the account's login can run, off obid's async runtime: it can ask the macOS Keychain.
async fn signed_in_now(tool: Tool, account: &Account) -> bool {
    let account = account.clone();
    tokio::task::spawn_blocking(move || {
        crate::jobs::signed_in(tool, &account, crate::jobs::login_home().as_deref())
    })
    .await
    .unwrap_or(false)
}

/// Asks the tools who each signed-in account is (`only`: that one), as obid starts and when a
/// tool's own login is listed. A login the person changed outside Oby shows at the next start.
pub async fn read_identities(shared: Arc<Shared>, only: Option<String>) {
    let agents = &shared.config.agents;
    let mut said = false;
    for (tool, account) in all(&agents.get()) {
        // Whether a login is there asks for no value of the Keychain; the tool says if it's over.
        let (looked, home) = (account.clone(), crate::jobs::login_home());
        let there = tokio::task::spawn_blocking(move || {
            crate::jobs::has_login(tool, &looked, home.as_deref())
        });
        if only.as_ref().is_some_and(|id| *id != account.id)
            || shared.jobs.being_changed(&account.id)
            || !there.await.unwrap_or(false)
        {
            continue;
        }
        if let Some(who) = read_identity(&shared, tool, &account).await {
            agents.learn(tool, &account.id, who, false);
            said = true;
        }
    }
    if said {
        send_accounts(&shared).await;
    }
}

/// What the tool says of the account, from its own status run the way its sign-in is, or None when
/// it didn't answer. Nothing but [`identity_from`]'s values is kept or logged of the answer.
async fn read_identity(shared: &Shared, tool: Tool, account: &Account) -> Option<Identity> {
    // A ChatGPT login has no status of the tool's to ask: who it is came with its id token.
    if tool == Tool::Codex
        && let Some(marker) = crate::chatgpt::marker_of(account)
    {
        return Some(Identity {
            email: marker.email,
            plan: None,
            org: None,
        });
    }
    let bin = agent_of(&shared.config.agents.get(), tool)
        .as_ref()
        .and_then(|agent| crate::process::resolve_bin(&agent.bin))
        .filter(|bin| bin.is_absolute())?;
    let scratch = scratch_folder(shared).ok()?;
    let dir = account.config_dir.as_deref();
    let said = match tool {
        Tool::Codex => read_codex_account(&bin, dir, &scratch).await,
        _ => read_status(tool, &bin, dir, &scratch).await,
    };
    let _ = std::fs::remove_dir_all(&scratch);
    said.map(|said| identity_from(tool, &said))
}

/// `claude auth status --json` or `cursor-agent status --format json`, parsed.
async fn read_status(tool: Tool, bin: &Path, dir: Option<&Path>, scratch: &Path) -> Option<Value> {
    let mut command = tool_command(bin, scratch);
    command
        .args(match tool {
            Tool::Claude => ["auth", "status", "--json"].as_slice(),
            _ => ["status", "--format", "json"].as_slice(),
        })
        .envs(account_env(tool, dir, scratch).ok()?)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let child = command.spawn().ok()?;
    let group = child.id();
    match tokio::time::timeout(STATUS_WITHIN, child.wait_with_output()).await {
        // ponytail: its whole answer in memory: it's the tool's own, a few lines.
        Ok(output) => serde_json::from_slice(&output.ok()?.stdout).ok(),
        Err(_) => {
            if let Some(group) = group {
                crate::process::signal_group(group, nix::sys::signal::Signal::SIGKILL);
            }
            None
        }
    }
}

/// Codex's `account/read`, from a server started for that alone, never refreshing a token.
async fn read_codex_account(bin: &Path, dir: Option<&Path>, scratch: &Path) -> Option<Value> {
    let mut argv: Vec<std::ffi::OsString> = vec![bin.into()];
    if dir.is_some() {
        argv.extend(["-c".into(), CODEX_FILE_STORE.into()]);
    }
    argv.extend([
        "-c".into(),
        "features.plugins=false".into(),
        "app-server".into(),
    ]);
    let mut env = crate::process::kept(|name| std::env::var_os(name));
    env.push(("PATH".into(), login_path(bin).into()));
    env.push(("TMPDIR".into(), scratch.into()));
    env.extend(
        account_env(Tool::Codex, dir, scratch)
            .ok()?
            .into_iter()
            .map(|(k, v)| (k.into(), v)),
    );
    let launch = crate::agent::Launch {
        argv,
        env,
        cwd: scratch.to_path_buf(),
        stderr: scratch.join("stderr.log"),
        own_sandbox: false,
        group: None,
    };
    let params = json!({"refreshToken": false});
    crate::codex::ask_once(&launch, STATUS_WITHIN, "account/read", params)
        .await
        .ok()
}

/// A character that breaks a line or turns text around without showing: never in a tool's word.
fn hides(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{2028}' | '\u{2029}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// One value of a tool's answer: one line of some length, or none.
fn said(value: &Value, longest: usize) -> Option<String> {
    let text = value.as_str()?.trim();
    let fine = !text.is_empty() && text.chars().count() <= longest && !text.chars().any(hides);
    fine.then(|| text.to_owned())
}

/// The email, plan and org in a tool's answer, and nothing else of it:
///
/// - Claude Code: `email`, `subscriptionType`, `orgName`, while `loggedIn`. A personal login's
///   `<someone>'s Organization` is no org anybody named.
/// - Codex: `account.email`, `account.planType`, its `unknown` being none.
/// - Cursor: `userInfo.email`, `userInfo.teamName`: it says no plan, and no `userInfo` at all when
///   it can't reach Cursor.
fn identity_from(tool: Tool, answer: &Value) -> Identity {
    let (email, plan, org) = match tool {
        Tool::Claude if answer["loggedIn"] != true => (&Value::Null, &Value::Null, &Value::Null),
        Tool::Claude => (
            &answer["email"],
            &answer["subscriptionType"],
            &answer["orgName"],
        ),
        Tool::Codex => (
            &answer["account"]["email"],
            &answer["account"]["planType"],
            &Value::Null,
        ),
        Tool::Cursor => (
            &answer["userInfo"]["email"],
            &Value::Null,
            &answer["userInfo"]["teamName"],
        ),
    };
    Identity {
        email: said(email, 254),
        plan: said(plan, LONGEST).filter(|plan| plan != "unknown"),
        org: said(org, 120)
            .filter(|org| !(org.ends_with("'s Organization") || org.ends_with("’s Organization"))),
    }
    .cleaned()
}

impl Identity {
    /// Only an email that looks like one: a tool's words or a hand-edited file aren't.
    fn cleaned(mut self) -> Identity {
        self.email.take_if(|email| !email.contains('@'));
        self
    }
}

/// A login started and not yet registered as a sign-in: dropped armed (its request ended), its
/// whole process group is killed and its folder removed.
struct Pending {
    group: Option<u32>,
    scratch: PathBuf,
    armed: bool,
}

impl Drop for Pending {
    fn drop(&mut self) {
        if self.armed {
            if let Some(group) = self.group {
                crate::process::signal_group(group, nix::sys::signal::Signal::SIGKILL);
            }
            let _ = std::fs::remove_dir_all(&self.scratch);
        }
    }
}

/// Reads `out` a line at a time, each cut to [`LONGEST_LINE`] bytes, into `lines`, until it ends.
pub(crate) async fn read_lines(
    mut out: Box<dyn tokio::io::AsyncRead + Unpin + Send>,
    lines: tokio::sync::mpsc::UnboundedSender<String>,
) {
    use tokio::io::AsyncReadExt;
    let mut buffer = [0u8; 4096];
    let mut line = Vec::new();
    while let Ok(n) = out.read(&mut buffer).await {
        if n == 0 {
            break;
        }
        for byte in &buffer[..n] {
            if *byte == b'\n' {
                let _ = lines.send(String::from_utf8_lossy(&line).into_owned());
                line.clear();
            } else if line.len() < LONGEST_LINE {
                line.push(*byte);
            }
        }
    }
    if !line.is_empty() {
        let _ = lines.send(String::from_utf8_lossy(&line).into_owned());
    }
}

/// The tool's login command for the account's folder `dir`: its environment made from scratch (no
/// token, no Claude Code or Anthropic variable), a `PATH` of the system's folders and the tool's
/// own, run by its absolute path from `scratch` (its temporary folder, and Cursor's caches), no
/// browser opened on this machine, nothing printed to a terminal, in a process group of its own.
fn command(
    tool: Tool,
    bin: &Path,
    dir: &Path,
    scratch: &Path,
) -> Result<tokio::process::Command, String> {
    let mut command = tool_command(bin, scratch);
    command
        .stdin(std::process::Stdio::piped())
        .envs(account_env(tool, Some(dir), scratch)?);
    match tool {
        Tool::Claude => {
            command.args(["auth", "login"]);
        }
        Tool::Codex => {
            command.args(["login", "--device-auth", "-c", CODEX_FILE_STORE]);
        }
        Tool::Cursor => {
            crate::acp::cursor::rewrite_config(dir)
                .map_err(|e| format!("obid can't write Cursor's settings for the sign-in: {e}"))?;
            command.arg("login").env("NO_OPEN_BROWSER", "1");
        }
    }
    Ok(command)
}

/// Keeps Codex's login in a file in its folder, for an account with one.
const CODEX_FILE_STORE: &str = r#"cli_auth_credentials_store="file""#;

/// A tool's command as [`command`] says, whatever it's asked.
fn tool_command(bin: &Path, scratch: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(bin);
    command
        .env_clear()
        .envs(crate::process::kept(|name| std::env::var_os(name)))
        .env("PATH", login_path(bin))
        .env("TMPDIR", scratch)
        .env("BROWSER", "true")
        .current_dir(scratch)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    command
}

/// The variables that point a tool at the account: its folder `dir`, or for the tool's own login
/// (no folder) the person's home, as the tool itself would run.
pub(crate) fn account_env(
    tool: Tool,
    dir: Option<&Path>,
    scratch: &Path,
) -> Result<Vec<(&'static str, std::ffi::OsString)>, String> {
    let home = || {
        crate::jobs::login_home()
            .map(|home| ("HOME", home.into_os_string()))
            .ok_or_else(|| format!("obid can't find the person's home for {}.", tool.name()))
    };
    Ok(match (tool, dir) {
        (Tool::Claude, Some(dir)) => vec![home()?, ("CLAUDE_CONFIG_DIR", dir.into())],
        (Tool::Codex, Some(dir)) => vec![("HOME", dir.into()), ("CODEX_HOME", dir.into())],
        (Tool::Cursor, dir) => {
            let mut env = match dir {
                Some(dir) => crate::acp::cursor::environment(dir),
                None => vec![home()?],
            };
            env.push(("NODE_COMPILE_CACHE", scratch.into()));
            env.push(("XDG_CACHE_HOME", scratch.into()));
            env
        }
        (_, None) => vec![home()?],
    })
}

/// [`SYSTEM_PATH`], then the folder the tool's binary is in (links followed: Cursor's runs the
/// `node` beside it), then, for a script that starts `#!/usr/bin/env <program>` (a CLI installed
/// with npm: `#!/usr/bin/env node`), the folder of that program as obid finds it.
pub(crate) fn login_path(bin: &Path) -> String {
    let real = std::fs::canonicalize(bin).ok();
    let mut path = SYSTEM_PATH.to_owned();
    let mut add = |dir: Option<&Path>| {
        if let Some(dir) = dir.and_then(|d| d.to_str())
            && !path.split(':').any(|p| p == dir)
        {
            path.push(':');
            path.push_str(dir);
        }
    };
    add(real.as_deref().and_then(Path::parent));
    let program = real.as_deref().and_then(interpreter);
    let found = program.as_deref().and_then(crate::process::resolve_bin);
    add(found
        .as_deref()
        .filter(|p| p.is_absolute())
        .and_then(Path::parent));
    path
}

/// The program a script names on its first line as `#!/usr/bin/env [-S] <program> …`.
fn interpreter(script: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = [0u8; 256];
    let n = std::fs::File::open(script).ok()?.read(&mut head).ok()?;
    let line = std::str::from_utf8(&head[..n]).ok()?.lines().next()?;
    let mut words = line.strip_prefix("#!")?.split_whitespace();
    if words.next()? != "/usr/bin/env" {
        return None;
    }
    let program = words.find(|w| !w.starts_with('-'))?;
    (!program.contains('/') && !program.contains('=')).then(|| program.to_owned())
}

/// `line` without terminal colours and around spaces.
pub(crate) fn plain(line: &str) -> String {
    static ESCAPES: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    ESCAPES
        .get_or_init(|| regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").expect("a valid pattern"))
        .replace_all(line, "")
        .trim()
        .to_owned()
}

/// The sites each tool's sign-in page is on.
fn sites(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude => &["claude.com", "claude.ai", "anthropic.com"],
        Tool::Codex => &["openai.com", "chatgpt.com"],
        Tool::Cursor => &["cursor.com", "cursor.sh"],
    }
}

/// The first `https://` link in `line` on one of the tool's own sites (the site or below it), as
/// it is: no login in it, no space, at most 4,096 characters.
fn link(tool: Tool, line: &str) -> Option<String> {
    let at = line.find("https://")?;
    let url: &str = line[at..].split_whitespace().next()?;
    let rest = &url["https://".len()..];
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let host = authority.to_ascii_lowercase();
    let fine = url.len() <= 4096
        && !host.contains(['@', ':', '\\'])
        && sites(tool)
            .iter()
            .any(|site| host == *site || host.ends_with(&format!(".{site}")))
        && url.chars().all(|c| c.is_ascii_graphic());
    fine.then(|| url.to_owned())
}

/// Codex's one-time code, a line of its own like `3U0G-DCVSU`.
fn user_code(line: &str) -> Option<String> {
    static CODE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    CODE.get_or_init(|| {
        regex::Regex::new(r"^[A-Z0-9]{4,8}(-[A-Z0-9]{4,8})+$").expect("a valid pattern")
    })
    .is_match(line)
    .then(|| line.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn accounts_3_only_the_email_plan_and_org_are_taken_from_what_a_tool_says() {
        let who = identity_from(
            Tool::Claude,
            &json!({"loggedIn": true, "email": "me@deskree.com", "orgName": "Deskree",
                    "subscriptionType": "team", "accessToken": "never", "orgId": "never"}),
        );
        assert_eq!(
            serde_json::to_value(&who).unwrap(),
            json!({"email": "me@deskree.com", "plan": "team", "org": "Deskree"})
        );
        let who = identity_from(
            Tool::Codex,
            &json!({"account": {"type": "chatgpt", "email": "me@x.dev", "planType": "plus"},
                    "requiresOpenaiAuth": true}),
        );
        assert_eq!(who.email.as_deref(), Some("me@x.dev"));
        assert_eq!(who.plan.as_deref(), Some("plus"));
        assert_eq!(who.org, None);
        // Signed out, or a key's account: nothing to say.
        assert_eq!(
            identity_from(
                Tool::Claude,
                &json!({"loggedIn": false, "email": "me@x.dev"})
            ),
            Identity::default()
        );
        assert_eq!(
            identity_from(Tool::Codex, &json!({"account": {"type": "apiKey"}})),
            Identity::default()
        );
        // What isn't one line, or isn't an email, isn't taken.
        assert_eq!(
            identity_from(
                Tool::Codex,
                &json!({"account": {"email": "a@b\nc", "planType": "unknown"}})
            ),
            Identity::default()
        );
        assert_eq!(
            identity_from(Tool::Codex, &json!({"account": {"email": "nobody"}})),
            Identity::default()
        );
    }

    #[test]
    fn accounts_3_a_word_that_breaks_a_line_or_turns_text_around_is_no_word() {
        for word in [
            "a\u{2028}b",
            "a\u{2029}b",
            "a\u{200B}b",
            "a\u{202E}b",
            "a\u{2066}b",
            "a\nb",
        ] {
            assert_eq!(said(&json!(word), 60), None, "{word:?}");
        }
        assert_eq!(said(&json!(" Déskree "), 60).as_deref(), Some("Déskree"));
    }

    #[test]
    fn accounts_4_a_personal_logins_own_organization_is_no_org() {
        for org in ["me@x.dev's Organization", "Wellington’s Organization"] {
            let who = identity_from(
                Tool::Claude,
                &json!({"loggedIn": true, "email": "me@x.dev", "orgName": org, "subscriptionType": "max"}),
            );
            assert_eq!(who.org, None, "{org}");
            assert_eq!(who.plan.as_deref(), Some("max"));
        }
    }

    #[test]
    fn accounts_5_cursor_says_its_email_and_team_under_user_info_or_nothing() {
        let who = identity_from(
            Tool::Cursor,
            &json!({"status": "authenticated", "isAuthenticated": true,
                    "userInfo": {"email": "me@x.dev", "userId": 7, "firstName": "Me", "teamName": "Deskree"}}),
        );
        assert_eq!(who.email.as_deref(), Some("me@x.dev"));
        assert_eq!(who.org.as_deref(), Some("Deskree"));
        assert_eq!(who.plan, None);
        // What cursor-agent 2026.10.01 says when it can't reach Cursor.
        assert_eq!(
            identity_from(
                Tool::Cursor,
                &json!({"status": "authenticated", "isAuthenticated": true,
                        "message": "Logged in (unable to fetch user details)"}),
            ),
            Identity::default()
        );
    }

    /// The environment variables a sign-in's command names.
    fn env_of(command: &tokio::process::Command) -> HashMap<String, Option<OsString>> {
        command
            .as_std()
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_owned())))
            .collect()
    }

    fn account(id: &str, name: &str, dir: Option<&str>) -> Account {
        Account {
            id: id.into(),
            name: name.into(),
            payer: None,
            config_dir: dir.map(PathBuf::from),
        }
    }

    fn agents(claude: Option<Vec<Account>>) -> Agents {
        Agents {
            claude: claude.map(|accounts| Agent {
                bin: "claude".into(),
                accounts,
            }),
            codex: None,
            cursor: None,
        }
    }

    fn added(id: &str, agent: &str) -> Added {
        Added {
            fresh: false,
            auto_name: false,
            id: id.into(),
            agent: agent.into(),
            name: format!("Name {id}"),
            payer: Some("Deskree".into()),
            config_dir: PathBuf::from(format!("/srv/obid/accounts/{id}")),
        }
    }

    fn ids(agents: &Agents) -> Vec<String> {
        agents
            .claude
            .iter()
            .flat_map(|a| a.accounts.iter().map(|a| a.id.clone()))
            .collect()
    }

    #[test]
    fn whats_saved_is_laid_over_the_configuration() {
        let seed = agents(Some(vec![
            account("default", "Default", None),
            account("work", "Work", Some("/srv/work")),
        ]));
        let saved = Saved {
            retired: Vec::new(),
            own: Vec::new(),
            added: vec![added("a1b2c3d4", "codex"), added("work", "claude")],
            changed: BTreeMap::from([(
                "default".into(),
                Change {
                    name: "Personal".into(),
                    payer: Some("Wellington".into()),
                },
            )]),
            removed: vec!["work".into()],
            identities: BTreeMap::new(),
        };
        let agents = merged(&seed, &saved);
        let claude = agents.claude.unwrap().accounts;
        // Renamed, the removed one gone, and an added id the configuration has is left out.
        assert_eq!(claude.len(), 1);
        assert_eq!(claude[0].name, "Personal");
        assert_eq!(claude[0].payer.as_deref(), Some("Wellington"));
        // A tool the configuration doesn't name is there with its default binary.
        let codex = agents.codex.unwrap();
        assert_eq!(codex.bin, "codex");
        assert_eq!(codex.accounts[0].id, "a1b2c3d4");
        assert!(agents.cursor.is_none());
    }

    /// A fresh obid lists the own login only of a tool with no account, and never past the most
    /// accounts obid keeps, those it lists in the same pass counted.
    #[test]
    fn own_logins_are_listed_only_for_tools_without_an_account_within_the_most() {
        let many: Vec<Account> = (0..MOST_ACCOUNTS - 1)
            .map(|n| account(&format!("w{n}"), &format!("W{n}"), Some("/srv/w")))
            .collect();
        let live = Live::new(agents(Some(many)));
        assert_eq!(
            live.adopt(&[Tool::Claude, Tool::Codex, Tool::Cursor]),
            [Tool::Codex]
        );
        assert_eq!(all(&live.get()).len(), MOST_ACCOUNTS);
        assert!(live.adopt(&Tool::ALL).is_empty());
        // Claude Code has accounts: its own login isn't listed by itself.
        let live = Live::new(agents(Some(vec![account(
            "work",
            "Work",
            Some("/srv/work"),
        )])));
        assert!(live.adopt(&[Tool::Claude]).is_empty());
        assert_eq!(ids(&live.get()), ["work"]);
    }

    #[test]
    fn an_empty_list_keeps_the_tools_own_login_beside_what_is_added() {
        let seed = agents(Some(Vec::new()));
        let saved = Saved {
            added: vec![added("a1b2c3d4", "claude")],
            ..Saved::default()
        };
        assert_eq!(ids(&merged(&seed, &saved)), ["default", "a1b2c3d4"]);
        // Nothing added: nothing changes.
        assert_eq!(merged(&seed, &Saved::default()), seed);
    }

    /// Removing a tool's last account takes the tool away: it never comes back on its own default
    /// login, the person's, which an empty list would otherwise mean.
    #[test]
    fn the_last_account_removed_never_brings_the_default_login_back() {
        let empty = agents(Some(Vec::new()));
        let saved = Saved {
            removed: vec!["default".into()],
            ..Saved::default()
        };
        assert!(merged(&empty, &saved).claude.is_none());
        let saved = Saved {
            added: vec![added("a1b2c3d4", "claude")],
            removed: vec!["default".into()],
            ..Saved::default()
        };
        assert_eq!(ids(&merged(&empty, &saved)), ["a1b2c3d4"]);
        let listed = agents(Some(vec![account("work", "Work", Some("/srv/w"))]));
        let saved = Saved {
            removed: vec!["work".into()],
            ..Saved::default()
        };
        assert!(merged(&listed, &saved).claude.is_none());
    }

    /// A removed account's id is never made again, and its folder stays hidden from every box
    /// (what was left there stays), across restarts.
    #[test]
    fn a_removed_accounts_id_and_folder_are_kept() {
        let home = std::env::temp_dir().join(format!("obid-retired-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let folder = home.join("elsewhere/work");
        std::fs::create_dir_all(&folder).unwrap();
        let seed = || agents(Some(vec![account("work", "Work", folder.to_str())]));
        let live = Live::new(seed());
        live.open(&home);
        assert_eq!(live.hidden_folders(&home), std::slice::from_ref(&folder));
        live.change(|agents, saved| {
            let (_, account) = find(agents, "work").unwrap();
            retire(saved, &account);
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{}", e.message));
        assert!(live.get().claude.is_none());
        let next = Live::new(seed());
        next.open(&home);
        assert_eq!(next.hidden_folders(&home), std::slice::from_ref(&folder));
        assert!(next.read().saved.removed.contains(&"work".to_owned()));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_change_is_kept_before_it_is_used() {
        let home = std::env::temp_dir().join(format!("obid-accounts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let seed = || {
            agents(Some(vec![
                account("work", "Work", Some("/srv/w")),
                account("more", "More", Some("/srv/m")),
            ]))
        };
        let live = Live::new(seed());
        live.open(&home);
        let copy = live.clone();
        live.change(|_, saved| {
            saved.removed.push("work".into());
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{}", e.message));
        // Every copy sees it, and so does the next start.
        assert_eq!(ids(&copy.get()), ["more"]);
        let next = Live::new(seed());
        next.open(&home);
        assert_eq!(ids(&next.get()), ["more"]);
        // A file that doesn't read changes nothing, and nothing is changed over it.
        std::fs::write(home.join(FILE), "{").unwrap();
        let broken = Live::new(seed());
        broken.open(&home);
        assert_eq!(ids(&broken.get()), ["work", "more"]);
        let refused = broken.change(|_, _| Ok(())).err().unwrap();
        assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(std::fs::read(home.join(FILE)).unwrap(), b"{");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// What `home/accounts.json` holds is checked as the API checks it: an entry the API would
    /// refuse is left out, the rest kept.
    #[test]
    fn a_wrong_entry_in_the_file_is_left_out() {
        let home = std::env::temp_dir().join(format!("obid-accounts-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let own = |id: &str| home.join("accounts").join(id);
        let entry = |id: &str, agent: &str, name: &str, dir: PathBuf| json!({"id": id, "agent": agent, "name": name, "configDir": dir});
        let file = json!({
            "added": [
                entry("a1b2c3d4", "claude", "Good", own("a1b2c3d4")),
                entry("b1b2c3d4", "claude", "Elsewhere", PathBuf::from("/Users/someone/.claude")),
                entry("../../x", "claude", "Traversal", own("../../x")),
                entry("c1b2c3d4", "gemini", "Tool", own("c1b2c3d4")),
                entry("d1b2c3d4", "claude", "two\nlines", own("d1b2c3d4")),
                entry("e1b2c3d4", "claude", "good", own("e1b2c3d4")),
                entry("work", "claude", "Seeded", own("work")),
                entry("A1B2C3D4", "claude", "Upper", own("A1B2C3D4")),
            ],
            "changed": {"work": {"name": ""}, "more": {"name": "Renamed", "payer": "x\u{7}"},
                        "other": {"name": "WORK"}},
            "removed": []
        });
        std::fs::write(home.join(FILE), file.to_string()).unwrap();
        let live = Live::new(agents(Some(vec![
            account("work", "Work", Some("/srv/w")),
            account("other", "Other", Some("/srv/o")),
        ])));
        live.open(&home);
        assert_eq!(ids(&live.get()), ["work", "other", "a1b2c3d4"]);
        let accounts = live.get().claude.unwrap().accounts;
        assert_eq!(accounts[0].name, "Work");
        // A change to a name another account has ("Work", letter case aside) is left out.
        assert_eq!(accounts[1].name, "Other");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn only_the_tools_own_sign_in_link_is_taken() {
        let claude = "If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&state=x";
        assert_eq!(
            link(Tool::Claude, claude).as_deref(),
            Some("https://claude.com/cai/oauth/authorize?code=true&state=x")
        );
        assert_eq!(
            link(
                Tool::Codex,
                &plain("   \x1b[94mhttps://auth.openai.com/codex/device\x1b[0m")
            )
            .as_deref(),
            Some("https://auth.openai.com/codex/device")
        );
        for (tool, line) in [
            (Tool::Claude, "visit: https://evil.example/claude.com"),
            (Tool::Claude, "visit: https://claude.com@evil.example/x"),
            (Tool::Claude, "visit: https://claude.com.evil.example/x"),
            (Tool::Claude, "visit: https://notclaude.com/x"),
            (Tool::Claude, "visit: https://claude.com:8443/x"),
            (Tool::Claude, "visit: http://claude.com/x"),
            (Tool::Codex, "https://claude.com/cai/oauth/authorize"),
            (Tool::Cursor, "nothing here"),
        ] {
            assert_eq!(link(tool, line), None, "{line}");
        }
        assert_eq!(
            link(
                Tool::Cursor,
                "navigate to this link: https://cursor.com/loginDeepControl?challenge=a"
            ),
            Some("https://cursor.com/loginDeepControl?challenge=a".into())
        );
        assert_eq!(user_code("3U0G-DCVSU").as_deref(), Some("3U0G-DCVSU"));
        assert_eq!(
            user_code("2. Enter this one-time code (expires in 15 minutes)"),
            None
        );
        assert_eq!(user_code("ABCD"), None);
    }

    /// A login runs in the account's own folder, which no box gets (#655), with a `PATH` of the
    /// system's folders and its own, no secret of obid's, and Cursor's caches in obid's folder.
    #[test]
    fn a_login_runs_in_the_accounts_folder_with_a_pinned_path() {
        let (dir, scratch) = (Path::new("/srv/acct"), Path::new("/srv/obid/tmp/s"));
        let claude = command(Tool::Claude, Path::new("/bin/claude"), dir, scratch).unwrap();
        let env = env_of(&claude);
        assert_eq!(env["CLAUDE_CONFIG_DIR"], Some(dir.into()));
        assert!(!env.contains_key("CLAUDE_SECURESTORAGE_CONFIG_DIR"));
        let codex = command(Tool::Codex, Path::new("/bin/codex"), dir, scratch).unwrap();
        let env = env_of(&codex);
        assert_eq!(env["CODEX_HOME"], Some(dir.into()));
        assert_eq!(env["HOME"], Some(dir.into()));
        for command in [&claude, &codex] {
            let env = env_of(command);
            assert!(
                env["PATH"]
                    .as_ref()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(SYSTEM_PATH)
            );
            assert!(
                command
                    .as_std()
                    .get_envs()
                    .all(|(k, _)| !crate::process::is_forbidden(&k.to_string_lossy()))
            );
            assert_eq!(command.as_std().get_current_dir(), Some(scratch));
        }
        assert_eq!(login_path(Path::new("/nowhere/claude")), SYSTEM_PATH);
    }

    /// A login whose request ended before its link (the app gone, obid stopping) is killed with
    /// everything it started, and its folder removed.
    #[test]
    fn a_login_its_request_dropped_ends_with_everything_it_started() {
        use std::os::unix::process::CommandExt;
        let root = std::env::temp_dir().join(format!("obid-pending-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let pids = root.join("pids");
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "sleep 300 & echo $$ $! > '{}'; wait",
                pids.display()
            ))
            .process_group(0)
            .spawn()
            .unwrap();
        let said = loop {
            if let Ok(text) = std::fs::read_to_string(&pids)
                && text.ends_with('\n')
            {
                break text;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        drop(Pending {
            group: Some(child.id()),
            scratch: root.clone(),
            armed: true,
        });
        let _ = child.wait();
        assert!(!root.exists());
        for pid in said.split_whitespace() {
            let pid = nix::unistd::Pid::from_raw(pid.parse().unwrap());
            let gone = (0..100).any(|_| {
                std::thread::sleep(Duration::from_millis(20));
                nix::sys::signal::kill(pid, None).is_err()
            });
            assert!(gone, "{pid} outlived its login");
        }
    }

    /// A CLI installed with npm is a `#!/usr/bin/env node` script: node's folder joins its PATH.
    #[test]
    fn an_npm_scripts_interpreter_is_on_its_path() {
        let root = std::env::temp_dir().join(format!("obid-npm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("codex.js");
        std::fs::write(&script, "#!/usr/bin/env node\nconsole.log(1)\n").unwrap();
        assert_eq!(interpreter(&script).as_deref(), Some("node"));
        std::fs::write(&script, "#!/usr/bin/env -S node --no-warnings\n").unwrap();
        assert_eq!(interpreter(&script).as_deref(), Some("node"));
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        assert_eq!(interpreter(&script), None);
        std::fs::write(&script, "#!/usr/bin/env node\n").unwrap();
        if let Some(node) = crate::process::resolve_bin("node") {
            let path = login_path(&script);
            assert!(path.starts_with(SYSTEM_PATH), "{path}");
            assert!(
                path.split(':')
                    .any(|p| Path::new(p) == node.parent().unwrap()),
                "{path}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn names_and_payers_are_checked() {
        let agents = agents(Some(vec![account("work", "Work", Some("/srv/acct/work"))]));
        assert_eq!(
            name(&agents, Tool::Claude, " Personal ", None)
                .ok()
                .as_deref(),
            Some("Personal")
        );
        for bad in ["", "  ", "a\u{0}b", "Work", "WORK", "work"] {
            assert!(name(&agents, Tool::Claude, bad, None).is_err(), "{bad:?}");
        }
        // Its own name is its own; another tool's isn't taken.
        assert!(name(&agents, Tool::Claude, "Work", Some("work")).is_ok());
        assert!(name(&agents, Tool::Codex, "Work", None).is_ok());
        assert_eq!(payer(Some(&json!(" "))).ok(), Some(None));
        assert!(payer(Some(&json!(3))).is_err());
        let home = Path::new("/srv/obid");
        assert!(own_folder(home, "a1b2c3d4").is_some());
        assert!(own_folder(home, "../../x").is_none());
        assert!(own_folder(home, "A1B2C3D4").is_none());
    }
}
