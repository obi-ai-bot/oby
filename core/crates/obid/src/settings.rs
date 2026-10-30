//! What Settings changes and obid keeps across restarts, in `home/settings.json` (0600): how many
//! jobs run at once (docs/runner-api.md, "GET /v1/settings/jobs and PUT /v1/settings/jobs"), and
//! the owner's check-in (docs/runner-api.md, "obid only: check-ins (#1116)"), and the
//! repositories the owner said No check for (#1062).

use std::path::{Path, PathBuf};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// How many jobs may run at once: 1 to 20.
pub const MAX_RUNNING_RANGE: std::ops::RangeInclusive<u32> = 1..=20;

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
    /// The effort a job runs at: a complex one's own, else the routine one.
    pub fn effort(&self, complex: bool) -> Option<&str> {
        complex
            .then_some(self.complex_effort.as_deref())
            .flatten()
            .or(self.effort.as_deref())
    }
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

/// The models Settings › Agents offers for each tool, `(id, name)`, as Sangam's screen lists them.
// ponytail: a fixed list, and an account's model must be on it (no other tool's model reaches a
// CLI); each account's own (Codex's `model/list`, `cursor-agent models`) when the screen needs
// models this list doesn't have.
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

/// A model id obid passes on as one argument: letters, digits, `-`, `.` and `_`, and never a
/// leading `-`, which a CLI would read as a flag.
pub fn valid_model(model: &str) -> bool {
    !model.trim().is_empty()
        && !model.starts_with('-')
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
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
        Settings {
            default_agent: default_agent(),
            default_account: None,
            tuning: BTreeMap::new(),
            max_running: default_max_running(),
            check_in: None,
            check_in_sent: None,
            account_failover: None,
            no_check: Vec::new(),
        }
    }
}

/// Where the settings are kept.
pub fn path(home: &Path) -> PathBuf {
    home.join("settings.json")
}

/// The settings kept in `home`, or the defaults when there are none. A file that doesn't read, or
/// holds a value out of range, gives the defaults for what it can't say, and is logged.
pub fn load(home: &Path) -> Settings {
    let file = path(home);
    let mut settings = match std::fs::read(&file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => {
            tracing::warn!("can't read {}: {e}", file.display());
            Settings::default()
        }
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            tracing::warn!("{} doesn't read: {e}", file.display());
            Settings::default()
        }),
    };
    if !MAX_RUNNING_RANGE.contains(&settings.max_running) {
        tracing::warn!(
            "{} says maxRunning {}, which isn't 1 to 20",
            file.display(),
            settings.max_running
        );
        settings.max_running = default_max_running();
    }
    if !valid_default_agent(&settings.default_agent) {
        tracing::warn!("{} has an unknown defaultAgent", file.display());
        settings.default_agent = default_agent();
    }
    settings
}

/// Keeps `settings` in `home`.
pub fn save(home: &Path, settings: &Settings) -> std::io::Result<()> {
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
}
