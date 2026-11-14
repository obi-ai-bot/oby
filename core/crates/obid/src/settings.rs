//! What Settings changes and obid keeps across restarts, in `home/settings.json` (0600): how many
//! jobs run at once (docs/runner-api.md, "GET /v1/settings/jobs and PUT /v1/settings/jobs"), and
//! the owner's check-in (docs/runner-api.md, "obid only: check-ins (#1116)"), and the
//! repositories the owner said No check for (#1062).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many jobs may run at once: 1 to the owner's ceiling ([`Limits::max_running`], 20 unless
/// they raised it).
pub fn max_running_range() -> std::ops::RangeInclusive<u32> {
    1..=limits().max_running()
}

/// The owner's own ceilings on what Oby holds to by itself (the owner's decision of 2026-10-31,
/// "Oby forbids nothing"; `limits` on `PUT /v1/settings/jobs`): each unset one is its default.
/// What only protects the Mac (memory, disk, open connections) stays a fixed limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_running: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folders: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routines: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accounts: Option<u32>,
}

impl Limits {
    /// The highest each may be set to, whatever the owner says: past these the Mac itself runs out
    /// (memory and processes for running jobs, files, the inbox), not a rule of Oby's.
    pub const CEILINGS: [(&'static str, u32); 5] = [
        ("rounds", 100),
        ("maxRunning", 64),
        ("folders", 200),
        ("routines", 500),
        ("accounts", 256),
    ];

    /// The most rounds a job may be given: 10.
    pub fn rounds(&self) -> u32 {
        self.rounds.unwrap_or(10)
    }

    /// The most jobs `maxRunning` may let run at once: 20.
    pub fn max_running(&self) -> u32 {
        self.max_running.unwrap_or(20)
    }

    /// The most folders a conversation works in ([`crate::folders::MOST`]).
    pub fn folders(&self) -> usize {
        self.folders.map_or(crate::folders::MOST, |n| n as usize)
    }

    /// The most routines obid keeps ([`crate::routines::MOST`]).
    pub fn routines(&self) -> usize {
        self.routines.map_or(crate::routines::MOST, |n| n as usize)
    }

    /// The most accounts obid keeps ([`crate::accounts::MOST_ACCOUNTS`]).
    pub fn accounts(&self) -> usize {
        self.accounts
            .map_or(crate::accounts::MOST_ACCOUNTS, |n| n as usize)
    }

    /// As `GET /v1/settings/jobs` shows them: each one's value now.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "rounds": self.rounds(),
            "maxRunning": self.max_running(),
            "folders": self.folders(),
            "routines": self.routines(),
            "accounts": self.accounts(),
        })
    }
}

/// The limits in force now: what obid's settings say, once read ([`use_limits`]).
/// ponytail: one per process, set from the one home obid serves; per-home if one obid ever
/// serves several.
static LIMITS: std::sync::RwLock<Option<Limits>> = std::sync::RwLock::new(None);

/// The limits in force now.
pub fn limits() -> Limits {
    let limits = LIMITS.read().unwrap_or_else(|e| e.into_inner());
    limits.unwrap_or_default()
}

/// Puts `limits` in force: when obid starts, and when the owner changes them.
pub fn use_limits(limits: Limits) {
    *LIMITS.write().unwrap_or_else(|e| e.into_inner()) = Some(limits);
}

/// Whether obid moves work past an account's usage limit to another account with room by itself,
/// when the owner didn't say (`accountFailover`, #871): on, the owner's decision of 2026-10-24.
pub const ACCOUNT_FAILOVER: bool = true;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_agent")]
    pub default_agent: String,
    /// The account new tasks go to (Settings › Agents › New tasks go to, #1573), by id: routing
    /// tries it first while it's signed in with room ([`crate::jobs::Jobs`]'s `pick`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_account: Option<String>,
    /// OpenAI's welcome for a first ChatGPT sign-in was shown (docs/contracts/chatgpt-sign-in.md).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chatgpt_welcomed: bool,
    /// The account conversations talk through unless one chose another (`talkAccount`,
    /// docs/contracts/conversation-agent.md), by id; none (or `auto`) is Auto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub talk_account: Option<String>,
    /// Each account's model and efforts (Settings › Agents, #1574), by account id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tuning: BTreeMap<String, Tuning>,
    /// How many jobs run at once (`status.maxRunning`).
    #[serde(default = "default_max_running")]
    pub max_running: u32,
    /// The owner's check-in, when they turned it on in a conversation (crate::brain::recap).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_in: Option<CheckIn>,
    /// The due time of the last summary a check-in posted, kept while they're off: never two on
    /// one day, even off and on again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_in_sent: Option<i64>,
    /// Whether a job (and a conversation's turn) past its account's usage limit goes on on another
    /// account of the tool with room by itself (#871, #874); [`ACCOUNT_FAILOVER`] when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_failover: Option<bool>,
    /// The repositories the owner answered No check for when Oby asked for one (#1062): it never
    /// asks there again, until the owner says to (`POST /v1/repos/check`). The `checksOffered` an
    /// older obid kept listed every repository it had asked about, answered or not, so it isn't
    /// read: those are asked once more.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_check: Vec<String>,
    /// The owner's own ceilings ([`Limits`]).
    #[serde(default, skip_serializing_if = "is_default")]
    pub limits: Limits,
    /// What the owner answered about work nobody's watching using each ChatGPT login (CHATGPT-20),
    /// by account id. No entry: they haven't been asked, and it's a no.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub background_consent: BTreeMap<String, BackgroundConsent>,
    /// Keys this obid doesn't know (a newer one's), kept as they were when it saves (HOME-11).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// The owner's answer about one ChatGPT login: a yes only from a signed click in an app of theirs
/// (`POST /v1/accounts/{id}/background`), and which device it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundConsent {
    pub given: bool,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

fn is_default(limits: &Limits) -> bool {
    *limits == Limits::default()
}

/// An account's model, routine effort and complex-task effort, each only when the owner chose it:
/// left out, the job runs on the tool's own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tuning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complex_effort: Option<String>,
}

impl Tuning {
    /// A retired account choice falls back; an explicit job model is handled separately.
    /// `offered` is that account's own list ([`crate::model_lists`]), else today's fixed list; empty
    /// when no live list was read yet ([`crate::model_lists::ModelLists::to_check`]): the saved
    /// choice stands (AGENTS-6).
    pub fn offered_model<'a>(&'a self, offered: &[crate::model_lists::Offered]) -> Option<&'a str> {
        self.model
            .as_deref()
            .filter(|model| offered.is_empty() || offered.iter().any(|m| m.id == *model))
    }
    /// The effort a job runs at: a complex one's own, else the routine one.
    pub fn effort(&self, complex: bool) -> Option<&str> {
        complex
            .then_some(self.complex_effort.as_deref())
            .flatten()
            .or(self.effort.as_deref())
    }
}

/// The tool defaults exposed in status, shared by jobs and conversation turns.
pub fn default_model(tool: crate::agent::Tool) -> &'static str {
    match tool {
        crate::agent::Tool::Claude => "claude-opus-5-5",
        crate::agent::Tool::Codex => "gpt-6.1-sol",
        crate::agent::Tool::Cursor => "auto",
    }
}

/// The model a turn or job runs on for `account`: its offered choice when still on the list, else
/// the tool's default. `offered` is that account's own list (or the fixed fallback).
pub fn account_model(
    home: &Path,
    tool: crate::agent::Tool,
    account: &str,
    offered: &[crate::model_lists::Offered],
) -> String {
    load(home)
        .tuning
        .get(account)
        .and_then(|t| t.offered_model(offered))
        .unwrap_or_else(|| default_model(tool))
        .to_owned()
}

/// The efforts each tool takes, as Settings › Agents offers them (`GET /v1/settings/models/options`):
/// Claude Code's `--effort`, Codex's turn `effort`; Cursor takes none.
pub fn efforts(tool: crate::agent::Tool) -> &'static [&'static str] {
    use crate::agent::Tool;
    match tool {
        Tool::Claude => &["low", "medium", "high", "xhigh", "max"],
        Tool::Codex => &["low", "medium", "high", "xhigh"],
        Tool::Cursor => &[],
    }
}

/// The fixed models each tool falls back to when its CLI isn't there or a read failed
/// ([`crate::model_lists`]): `(id, name)`, as Sangam's screen listed them.
pub fn models(tool: crate::agent::Tool) -> &'static [(&'static str, &'static str)] {
    use crate::agent::Tool;
    match tool {
        Tool::Claude => &[
            ("claude-fable-5-1", "Fable 5.1"),
            ("claude-opus-5-5", "Opus 5.5"),
            ("claude-sonnet-5-5", "Sonnet 5.5"),
            ("claude-haiku-5-5", "Haiku 5.5"),
        ],
        Tool::Codex => &[
            ("gpt-6.1-sol", "gpt-6.1-sol"),
            ("gpt-6-astra", "GPT-6-Astra"),
        ],
        Tool::Cursor => &[
            ("auto", "Auto"),
            ("claude-opus-5-5", "Opus 5.5"),
            ("gpt-6.1-sol", "gpt-6.1-sol"),
        ],
    }
}

/// A model id obid passes on as one argument: letters, digits and `-._:@/[]`, so a provider's id is
/// one too (Bedrock's `us.anthropic.claude-…-v1:0` or its ARN, Vertex's `claude-…@2026…`, LANES-4),
/// and never a leading `-`, which a CLI would read as a flag.
pub fn valid_model(model: &str) -> bool {
    !model.trim().is_empty()
        && !model.starts_with('-')
        && model.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(b, b'-' | b'.' | b'_' | b':' | b'@' | b'/' | b'[' | b']')
        })
}

impl Settings {
    pub fn account_failover(&self) -> bool {
        self.account_failover.unwrap_or(ACCOUNT_FAILOVER)
    }
}

/// When Oby checks in by itself, and where (#1116): set, changed and turned off only by the
/// owner's words in a conversation (`oby_check_in`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckIn {
    /// Daily, weekdays or weekly at a time: never hourly.
    pub schedule: crate::routines::Schedule,
    pub zone: String,
    /// The conversation it posts in: the one the owner asked in.
    pub conversation: String,
    /// What its next time counts from: when it was set, or the latest time it was due.
    pub from: i64,
}

impl CheckIn {
    /// Its time zone; one obid doesn't know reads as UTC.
    pub fn zone(&self) -> jiff::tz::TimeZone {
        jiff::tz::TimeZone::get(&self.zone).unwrap_or(jiff::tz::TimeZone::UTC)
    }
}

fn default_max_running() -> u32 {
    crate::api::MAX_RUNNING
}

fn default_agent() -> String {
    "auto".into()
}

pub fn valid_default_agent(agent: &str) -> bool {
    agent == "auto" || crate::agent::Tool::from_id(agent).is_some()
}

impl Default for Settings {
    fn default() -> Settings {
        serde_json::from_str("{}").expect("every setting has a default")
    }
}

/// Where the settings are kept.
pub fn path(home: &Path) -> PathBuf {
    home.join("settings.json")
}

/// `bytes`, a JSON object, read one key at a time (HOME-11): the settings of the keys whose values
/// read, keys obid doesn't know in `extra`, and the keys whose values didn't read.
fn decode(bytes: &[u8]) -> serde_json::Result<(Settings, Vec<String>)> {
    let object: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(bytes)?;
    let (mut read, mut bad) = (serde_json::Map::new(), Vec::new());
    for (key, value) in object {
        match serde_json::from_value::<Settings>(serde_json::json!({ key.as_str(): &value })) {
            Ok(_) => drop(read.insert(key, value)),
            Err(_) => bad.push(key),
        }
    }
    Ok((serde_json::from_value(read.into())?, bad))
}

/// The settings kept in `home`, or the defaults when there are none. A file that doesn't read, or
/// a value that doesn't read or is out of range, gives the defaults for what it can't say, logged.
pub fn load(home: &Path) -> Settings {
    let file = path(home);
    let mut settings = match std::fs::read(&file).map(|bytes| decode(&bytes)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => {
            tracing::warn!("can't read {}: {e}", file.display());
            Settings::default()
        }
        Ok(Err(e)) => {
            tracing::warn!("{} doesn't read: {e}", file.display());
            Settings::default()
        }
        Ok(Ok((settings, bad))) => {
            for key in bad {
                tracing::warn!("{} has a {key} that doesn't read: default", file.display());
            }
            settings
        }
    };
    if !(1..=settings.limits.max_running()).contains(&settings.max_running) {
        tracing::warn!(
            "{} says maxRunning {}, which isn't 1 to {}",
            file.display(),
            settings.max_running,
            settings.limits.max_running()
        );
        settings.max_running = default_max_running();
    }
    if !valid_default_agent(&settings.default_agent) {
        tracing::warn!("{} has an unknown defaultAgent", file.display());
        settings.default_agent = default_agent();
    }
    settings
}

/// Keeps `settings` in `home`: a file there that didn't wholly read is copied aside first (HOME-12).
pub fn save(home: &Path, settings: &Settings) -> std::io::Result<()> {
    let reads = |b: &[u8]| decode(b).is_ok_and(|(_, bad)| bad.is_empty());
    crate::store::keep_aside_unless(&path(home), reads)?;
    let text = serde_json::to_vec_pretty(settings).map_err(std::io::Error::other)?;
    crate::store::write_private(&path(home), &text)
}

/// The settings in `home` changed by `change` and kept, one change at a time, so two never undo
/// each other: what `change` answers.
pub fn update<T>(home: &Path, change: impl FnOnce(&mut Settings) -> T) -> std::io::Result<T> {
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let mut settings = load(home);
    let answer = change(&mut settings);
    save(home, &settings)?;
    Ok(answer)
}

/// Remembers the owner's No check for `repo` in `home` (#1062), or with `no` false forgets it, so
/// Oby may ask there again. Letter case aside, as GitHub's names.
pub fn no_check(home: &Path, repo: &str, no: bool) -> std::io::Result<()> {
    update(home, |settings| {
        settings.no_check.retain(|r| !r.eq_ignore_ascii_case(repo));
        if no {
            settings.no_check.push(repo.to_owned());
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LANES-4 (docs/contracts/lanes.md): a provider's model id is one, as written; a flag isn't.
    #[test]
    fn lanes_4_a_providers_model_id_is_a_model() {
        for id in [
            "us.anthropic.claude-sonnet-5-5-v1:0",
            "claude-opus-5-5@20261101",
            "claude-opus-5-5[1m]",
        ] {
            assert!(valid_model(id), "{id}");
        }
        for id in ["--model", "a b", "a;b", ""] {
            assert!(!valid_model(id), "{id}");
        }
    }

    /// AGENTS-6 (docs/contracts/agents.md): a complex task runs at the account's complex-task
    /// effort, else at its effort.
    #[test]
    fn agents_6_a_complex_task_takes_its_own_effort() {
        let both = Tuning {
            effort: Some("high".into()),
            complex_effort: Some("max".into()),
            ..Tuning::default()
        };
        assert_eq!(both.effort(false), Some("high"));
        assert_eq!(both.effort(true), Some("max"));
        let routine = Tuning {
            effort: Some("low".into()),
            ..Tuning::default()
        };
        assert_eq!(routine.effort(true), Some("low"));
        assert_eq!(Tuning::default().effort(true), None);
    }

    #[test]
    fn conversation_agent_21_each_accounts_model_is_optional_and_retired_choices_fall_back() {
        let home = std::env::temp_dir().join(format!("obid-models-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        for tool in crate::agent::Tool::ALL {
            let offered = crate::model_lists::fallback(tool);
            let model = models(tool)[0].0;
            update(&home, |s| {
                s.tuning.insert(
                    "personal".into(),
                    Tuning {
                        model: Some(model.into()),
                        ..Default::default()
                    },
                );
            })
            .unwrap();
            assert_eq!(account_model(&home, tool, "personal", &offered), model);
            assert_eq!(
                account_model(&home, tool, "work", &offered),
                default_model(tool)
            );
            update(&home, |s| {
                s.tuning.get_mut("personal").unwrap().model = Some("retired".into())
            })
            .unwrap();
            assert_eq!(
                account_model(&home, tool, "personal", &offered),
                default_model(tool)
            );
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn kept_and_read_back_with_defaults_for_what_isnt_there() {
        let home = std::env::temp_dir().join(format!("obid-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(load(&home).max_running, 6);
        assert_eq!(load(&home).default_agent, "auto");
        update(&home, |s| s.default_agent = "codex".into()).unwrap();
        assert_eq!(load(&home).default_agent, "codex");
        update(&home, |s| s.default_agent = "unknown".into()).unwrap();
        assert_eq!(load(&home).default_agent, "auto");
        save(
            &home,
            &Settings {
                max_running: 3,
                ..Settings::default()
            },
        )
        .unwrap();
        assert_eq!(load(&home).max_running, 3);
        std::fs::write(path(&home), r#"{"maxRunning": 99}"#).unwrap();
        assert_eq!(load(&home).max_running, 6);
        std::fs::write(path(&home), "{").unwrap();
        assert_eq!(load(&home).max_running, 6);
        std::fs::write(path(&home), "{}").unwrap();
        assert_eq!(load(&home).max_running, 6);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Serde's own defaults are the settings' defaults.
    #[test]
    fn the_defaults_are_serdes() {
        let settings = Settings::default();
        assert_eq!(settings.default_agent, "auto");
        assert_eq!(settings.max_running, crate::api::MAX_RUNNING);
        assert!(settings.extra.is_empty());
    }

    /// A home of the test's own, with `settings.json` holding `text` when there's one.
    fn home_with(name: &str, text: Option<&str>) -> PathBuf {
        let home = std::env::temp_dir().join(format!("obid-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        if let Some(text) = text {
            std::fs::write(path(&home), text).unwrap();
        }
        home
    }

    /// The `settings.json.unreadable-*` copies in `home`, each with what it holds.
    fn kept_aside(home: &Path) -> Vec<String> {
        std::fs::read_dir(home)
            .unwrap()
            .flatten()
            .filter(|e| {
                let name = e.file_name();
                name.to_string_lossy()
                    .starts_with("settings.json.unreadable-")
            })
            .map(|e| std::fs::read_to_string(e.path()).unwrap())
            .collect()
    }

    /// HOME-11 (docs/contracts/home.md): a value that doesn't read is its default, alone; every
    /// other one, and a key obid doesn't know, is kept, and the file is copied aside before a save
    /// writes the default over it.
    #[test]
    fn home_11_a_bad_setting_resets_only_itself() {
        let text =
            r#"{"maxRunning": "lots", "defaultAccount": "work", "fromANewerObid": {"a": 1}}"#;
        let home = home_with("settings-bad", Some(text));
        let settings = load(&home);
        assert_eq!(settings.max_running, crate::api::MAX_RUNNING);
        assert_eq!(settings.default_account.as_deref(), Some("work"));
        assert_eq!(
            settings.extra.get("fromANewerObid"),
            Some(&serde_json::json!({"a": 1}))
        );
        update(&home, |s| s.talk_account = Some("personal".into())).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path(&home)).unwrap()).unwrap();
        assert_eq!(saved["fromANewerObid"], serde_json::json!({"a": 1}));
        assert_eq!(saved["defaultAccount"], "work");
        assert_eq!(saved["talkAccount"], "personal");
        assert_eq!(saved["maxRunning"], crate::api::MAX_RUNNING);
        assert_eq!(kept_aside(&home), [text]);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// HOME-12 (docs/contracts/home.md): a `settings.json` that doesn't read is copied aside before
    /// the first save writes over it, once; with no file, nothing is.
    #[test]
    fn home_12_an_unreadable_settings_file_is_kept() {
        let home = home_with("settings-unreadable", Some("{\"maxRunning\": 3,"));
        update(&home, |s| s.default_agent = "codex".into()).unwrap();
        assert_eq!(kept_aside(&home), ["{\"maxRunning\": 3,"]);
        assert_eq!(load(&home).default_agent, "codex");
        update(&home, |s| s.default_account = Some("work".into())).unwrap();
        assert_eq!(kept_aside(&home).len(), 1, "a file that reads isn't copied");
        assert_eq!(load(&home).default_account.as_deref(), Some("work"));
        let _ = std::fs::remove_dir_all(&home);

        let home = home_with("settings-none", None);
        update(&home, |s| s.default_agent = "codex".into()).unwrap();
        assert!(kept_aside(&home).is_empty());
        assert_eq!(load(&home).default_agent, "codex");
        let _ = std::fs::remove_dir_all(&home);
    }
}
