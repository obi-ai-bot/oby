//! Each account's subscription usage and the owner's weekly reserve, kept privately across
//! restarts in `home/usage.json` (0600) (Engine/Accounts.swift is the behaviour it follows).
//!
//! It comes from the tools' own reports: Claude Code's `rate_limit_event` during a job and its
//! `/usage` (crate::jobs, `read_usages`), and Codex's `account/rateLimits/read` and
//! `account/rateLimits/updated`. Cursor's CLI reports none, so obid asks Cursor's own dashboard
//! with the account's access token, the one read that uses a login (crate::cursor_usage, the
//! owner's decision of 2026-11-03): a figure that's shown and holds nothing ([`Usage::advisory`]).
use obi_protocol::time;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// From this share of its week an account is in its weekly reserve: automatic work leaves it
/// alone until the week resets or the owner lifts the reserve (#278).
pub const RESERVE: f64 = 0.8;
/// From this share of either window an account takes no automatic work, its reserve lifted or not.
pub const NEARLY_FULL: f64 = 0.95;
const WEEK: i64 = 7 * 86_400;
/// How long a refused account is held when its tool didn't say when it resets: its next report
/// says more.
pub(crate) const UNKNOWN_RESET: i64 = 3_600;
/// The longest a five-hour window's reset can be away, with a minute for the tool's clock and the
/// Mac's to differ.
const FIVE_HOURS: i64 = 5 * 3_600 + 60;
/// The last second `time::format` writes with four digits (9999-12-31T23:59:59Z).
const LAST: i64 = 253_402_300_799;

/// One account's usage as its tool last reported it, and the owner's lift. Shares are 0...1;
/// times are seconds since the epoch.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Usage {
    pub five_hour: Option<f64>,
    /// When obid last received each window, not the time it was loaded from disk.
    pub five_hour_observed_at: Option<i64>,
    pub seven_day_observed_at: Option<i64>,
    pub seven_day: Option<f64>,
    pub five_hour_resets: Option<i64>,
    pub seven_day_resets: Option<i64>,
    /// The tool refused work, or a window reached 100%: the account takes nothing until then.
    pub full_until: Option<i64>,
    /// The hold is a conservative fallback because the provider gave no reset. A later complete
    /// room report can clear it; it must never be presented as a provider-confirmed reset.
    pub full_until_inferred: bool,
    /// The owner let it past its weekly reserve until then, its week's reset.
    pub reserve_lifted_until: Option<i64>,
    /// How the account pays, as its tool said (`api` or `plan`, crate::usage::Tokens): asked once.
    pub priced: Option<String>,
    /// `seven_day` is a plan's included usage that the plan can go on past (Cursor's, USAGE-9): it
    /// is shown, and holds nothing back whatever it reads. A refusal's `full_until` still does.
    pub advisory: bool,
    /// Claude's paid overage carries its work past a window, as its tool last said with its windows
    /// (USAGE-12): a window at 100% then doesn't read full ([`Usage::window_full`]).
    pub overage: bool,
}

/// A window's share now: one whose reset has passed starts again from nothing.
fn current(used: Option<f64>, resets: Option<i64>, now: i64) -> Option<f64> {
    used.map(|used| {
        if resets.is_some_and(|at| at <= now) {
            0.0
        } else {
            used
        }
    })
}

impl Usage {
    pub fn five_hour_at(&self, now: i64) -> Option<f64> {
        current(self.five_hour, self.five_hour_resets, now)
    }
    pub fn seven_day_at(&self, now: i64) -> Option<f64> {
        current(self.seven_day, self.seven_day_resets, now)
    }
    /// The week's reset while it is still ahead.
    fn week_resets(&self, now: i64) -> Option<i64> {
        self.seven_day_resets.filter(|at| *at > now)
    }
    pub fn full_until_at(&self, now: i64) -> Option<i64> {
        self.full_until.filter(|at| *at > now)
    }
    /// Until when the owner lifted its reserve, while that is still ahead.
    pub fn lifted_until(&self, now: i64) -> Option<i64> {
        self.reserve_lifted_until.filter(|at| *at > now)
    }
    /// At 80% of its week and not lifted by the owner. Claude Code reports usage only during a
    /// job, so the share stays until its week resets.
    pub fn reserved(&self, now: i64) -> bool {
        !self.advisory
            && self.seven_day_at(now).unwrap_or(0.0) >= RESERVE
            && self.lifted_until(now).is_none()
    }
    /// Held back by its weekly reserve alone, which the owner may lift (#872): not full, under 95%
    /// of both windows.
    pub fn only_reserved(&self, now: i64) -> bool {
        self.reserved(now)
            && self.full_until_at(now).is_none()
            && self.five_hour_at(now).unwrap_or(0.0) < NEARLY_FULL
            && self.seven_day_at(now).unwrap_or(0.0) < NEARLY_FULL
    }
    /// Whether a limit that resets at `reset` is a five-hour one (LIMITS-5): a reset its tool said,
    /// never obid's guess, no further off than that window, with the week neither nearly full nor
    /// in its reserve. Anything else is the owner's to decide (#1563).
    pub fn five_hour_limit(&self, reset: i64, now: i64) -> bool {
        let guessed = self.full_until_inferred && self.full_until == Some(reset);
        !guessed
            && reset > now
            && reset - now <= FIVE_HOURS
            && self.seven_day_at(now).unwrap_or(0.0) < NEARLY_FULL
            && !self.reserved(now)
    }

    /// When it takes automatic work again, as far as its tool said: the latest of the resets that
    /// hold it ([`Usage::held`]). `None` when it can now, or when no reset of them is known.
    pub fn free_at(&self, now: i64) -> Option<i64> {
        self.held(now)?;
        let five = (self.five_hour_at(now).unwrap_or(0.0) >= NEARLY_FULL)
            .then_some(self.five_hour_resets)
            .flatten();
        let week = (!self.advisory
            && (self.seven_day_at(now).unwrap_or(0.0) >= NEARLY_FULL || self.reserved(now)))
        .then(|| self.week_resets(now))
        .flatten();
        [self.full_until_at(now), five, week]
            .into_iter()
            .flatten()
            .max()
    }

    /// Why it can't take automatic work now ("is at 96% of its week"), or `None` when it can.
    pub fn held(&self, now: i64) -> Option<String> {
        let percent = |used: f64| (used * 100.0).round() as i64;
        let until =
            |at: Option<i64>| at.map_or(String::new(), |at| format!(" until {}", time::format(at)));
        if let Some(at) = self.full_until_at(now) {
            return Some(format!("hit its usage limit{}", until(Some(at))));
        }
        if self.advisory {
            return None;
        }
        let five = self.five_hour_at(now).unwrap_or(0.0);
        if five >= NEARLY_FULL {
            return Some(format!(
                "is at {}% of its 5-hour window{}",
                percent(five),
                until(self.five_hour_resets)
            ));
        }
        let week = self.seven_day_at(now).unwrap_or(0.0);
        if week >= NEARLY_FULL {
            return Some(format!(
                "is at {}% of its week{}",
                percent(week),
                until(self.week_resets(now))
            ));
        }
        self.reserved(now).then(|| {
            format!(
                "is in its weekly reserve ({}% of its week used){}; use it anyway from Settings › Accounts",
                percent(week),
                until(self.week_resets(now))
            )
        })
    }

    /// Whether a window reads 100%, so its tool refuses work. Never an advisory figure's, nor one
    /// past which Claude's overage carries the work (USAGE-12).
    pub fn window_full(&self, now: i64) -> bool {
        !self.advisory
            && !self.overage
            && [self.five_hour_at(now), self.seven_day_at(now)]
                .into_iter()
                .flatten()
                .any(|share| share >= 1.0)
    }
}

/// Every account's usage, by id.
pub struct Usages {
    file: PathBuf,
    state: Mutex<HashMap<String, Usage>>,
}

impl Usages {
    /// Reads what was kept. A file that doesn't read is set aside as `usage.json.unreadable` and
    /// obid starts over (as the Swift runner does): every reserve the owner lifted is back, and
    /// usage is unknown until each tool reports again.
    pub fn open(home: &Path) -> Self {
        let file = home.join("usage.json");
        let state = match std::fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                let aside = file.with_extension("json.unreadable");
                let moved = std::fs::rename(&file, &aside);
                tracing::warn!(
                    "account usage doesn't read ({error}); starting over{}",
                    match moved {
                        Ok(()) => format!(", the old file is {}", aside.display()),
                        Err(e) => format!(" (it couldn't be set aside: {e})"),
                    }
                );
                HashMap::new()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => {
                tracing::warn!("account usage doesn't read ({error}); starting over");
                HashMap::new()
            }
        };
        Self {
            file,
            state: Mutex::new(state),
        }
    }

    pub fn get(&self, id: &str) -> Usage {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// Applies `update` to one account and saves every account. With `require_saved`, a change
    /// that isn't saved isn't applied either: the owner's changes are acknowledged only when kept.
    /// A tool's report is applied anyway, so a fresh limit holds for this process.
    fn change(
        &self,
        id: &str,
        require_saved: bool,
        update: impl FnOnce(&mut Usage),
    ) -> (Usage, Usage, std::io::Result<()>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = state.clone();
        let value = next.entry(id.to_owned()).or_default();
        let was = value.clone();
        update(value);
        let value = value.clone();
        let saved = serde_json::to_vec_pretty(&next)
            .map_err(std::io::Error::other)
            .and_then(|bytes| crate::store::write_private(&self.file, &bytes));
        if saved.is_ok() || !require_saved {
            *state = next;
        }
        (was, value, saved)
    }

    /// Keeps what a tool just reported. A window it didn't report keeps its last share and reset;
    /// one reported without a reset keeps the last reset. A report never lifts a reserve. It holds
    /// for this process even when it can't be saved (that's logged). The usage before and after,
    /// read under one lock, so a report another job makes meanwhile can't fall between them.
    pub fn record(&self, id: &str, usage: &Usage) -> (Usage, Usage) {
        let (was, now, saved) = self.change(id, false, |kept| {
            // What's read now, whole: no last reset kept, and no `seven_day_observed_at`, which
            // says a window is evidence of room (crate::jobs::account_limits).
            if usage.advisory {
                kept.advisory = true;
                kept.seven_day = usage.seven_day;
                kept.seven_day_resets = usage.seven_day_resets;
                return;
            }
            if let Some(used) = usage.five_hour {
                kept.five_hour = Some(used);
                kept.five_hour_observed_at = Some(obi_protocol::time::now());
                if usage.five_hour_resets.is_some() {
                    kept.five_hour_resets = usage.five_hour_resets;
                }
            }
            if let Some(used) = usage.seven_day {
                kept.seven_day = Some(used);
                kept.seven_day_observed_at = Some(obi_protocol::time::now());
                if usage.seven_day_resets.is_some() {
                    kept.seven_day_resets = usage.seven_day_resets;
                }
            }
            // A report of windows says whether overage carries them; a refusal alone keeps it.
            if usage.five_hour.is_some() || usage.seven_day.is_some() {
                kept.overage = usage.overage;
            }
            if usage.full_until.is_some() {
                kept.full_until = usage.full_until;
                kept.full_until_inferred = usage.full_until_inferred;
            } else if kept.overage && (usage.five_hour.is_some() || usage.seven_day.is_some()) {
                // USAGE-12: overage carries its work now, so no earlier hold stands.
                kept.full_until = None;
                kept.full_until_inferred = false;
            } else if kept.full_until_inferred
                && usage.five_hour.is_some_and(|used| used < NEARLY_FULL)
                && usage.seven_day.is_some_and(|used| used < NEARLY_FULL)
            {
                kept.full_until = None;
                kept.full_until_inferred = false;
            }
            if usage.priced.is_some() {
                kept.priced.clone_from(&usage.priced);
            }
        });
        if let Err(error) = saved {
            tracing::warn!("could not keep account usage: {error}");
        }
        (was, now)
    }

    /// Forgets a removed account's usage, so an account given its id later starts unknown.
    pub fn forget(&self, id: &str) -> std::io::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.remove(id).is_none() {
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(&*state).map_err(std::io::Error::other)?;
        crate::store::write_private(&self.file, &bytes)
    }

    /// Forgets an advisory figure its read no longer gives (USAGE-8), and says whether there was
    /// one. A hold or a lift stays.
    pub fn forget_advisory(&self, id: &str) -> bool {
        if !self.get(id).advisory {
            return false;
        }
        let (_, _, saved) = self.change(id, false, |usage| {
            usage.advisory = false;
            usage.seven_day = None;
            usage.seven_day_resets = None;
        });
        if let Err(error) = saved {
            tracing::warn!("account {id}: could not keep that its usage is unknown: {error}");
        }
        true
    }

    /// A limit whose end obid only inferred is over, as a run that went through shows: whether it
    /// held one. A time its tool gave stands.
    pub fn clear_inferred(&self, id: &str) -> bool {
        if !self.get(id).full_until_inferred {
            return false;
        }
        let (_, _, saved) = self.change(id, false, |usage| {
            usage.full_until = None;
            usage.full_until_inferred = false;
        });
        if let Err(error) = saved {
            tracing::warn!("account {id}: could not keep that its limit passed: {error}");
        }
        true
    }

    /// Forgets how an account pays: a new sign-in may be another kind of login (an API key where a
    /// plan was), so its next job asks again. Its windows stay until its tool reports again.
    pub fn forget_priced(&self, id: &str) {
        if self.get(id).priced.is_none() {
            return;
        }
        if let (_, _, Err(error)) = self.change(id, false, |usage| usage.priced = None) {
            tracing::warn!("account {id}: could not keep that it signed in again: {error}");
        }
    }

    /// "Use anyway" (#278): with `lift`, the account takes automatic work past its weekly reserve
    /// until its week resets (a week from now while the reset is unknown), then it's reserved
    /// again by itself; without, it's reserved again now.
    pub fn lift(&self, id: &str, lift: bool) -> std::io::Result<Usage> {
        let now = time::now();
        let (_, kept, saved) = self.change(id, true, |usage| {
            usage.reserve_lifted_until = lift.then(|| usage.week_resets(now).unwrap_or(now + WEEK));
        });
        saved.map(|()| kept)
    }
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|v| v.is_finite())
}

/// A time a tool reported, in seconds or milliseconds since the epoch.
fn epoch(value: &Value) -> Option<i64> {
    let seconds = number(value).filter(|v| *v > 0.0)?;
    let seconds = if seconds > 1e11 {
        seconds / 1000.0
    } else {
        seconds
    };
    Some((seconds as i64).min(LAST))
}

fn share(value: &Value, divisor: f64) -> Option<f64> {
    number(value).map(|n| (n / divisor).clamp(0.0, 1.0))
}

/// Until when a refused account is full: the soonest reset of a window at 100%, else the reset
/// the refusal named, else an hour from now. `None` when nothing is full.
fn full_until(
    windows: [(Option<f64>, Option<i64>); 2],
    refused: bool,
    named: Option<i64>,
) -> Option<i64> {
    let full: Vec<Option<i64>> = windows
        .into_iter()
        .filter(|(used, _)| used.is_some_and(|n| n >= 1.0))
        .map(|(_, resets)| resets)
        .collect();
    (refused || !full.is_empty()).then(|| {
        full.into_iter()
            .flatten()
            .min()
            .or(named)
            .unwrap_or_else(|| time::now() + UNKNOWN_RESET)
    })
}

/// Whether Claude Code's paid overage carries the account's work past its windows (USAGE-12):
/// `isUsingOverage` (claude 2.1.302 says it with `status` `rejected` and `overageStatus` allowing),
/// or `overageInUse` while the overage still allows.
pub(crate) fn claude_overage(info: &Value) -> bool {
    info["isUsingOverage"] == true
        || (info["overageInUse"] == true
            && matches!(
                info["overageStatus"].as_str(),
                Some("allowed" | "allowed_warning")
            ))
}

/// Whether a `rate_limit_info`'s refusal is the whole account's (USAGE-13): `rejected` for the
/// 5-hour or weekly window, or naming none, and no overage carrying it.
pub(crate) fn claude_refused(info: &Value) -> bool {
    info["status"] == "rejected"
        && !claude_overage(info)
        && matches!(
            info["rateLimitType"].as_str(),
            None | Some("five_hour" | "seven_day")
        )
}

/// Whether a refusal is scoped (USAGE-13): a model's week (`seven_day_opus`, `seven_day_sonnet`,
/// `seven_day_overage_included`) or the overage's own (`overage`). It stops the turn's model and
/// leaves the account's other work going.
pub(crate) fn claude_scoped(info: &Value) -> bool {
    info["status"] == "rejected" && !claude_overage(info) && !claude_refused(info)
}

/// Claude Code's `rate_limit_event`: its `unifiedWindows` (`five_hour`, `seven_day`), each with a
/// `utilization` of 0...1 and a `resetsAt` in epoch seconds. Older versions name one window at the
/// top (`rateLimitType`, `utilization`, `resetsAt`). The account is full when it refused work as a
/// whole ([`claude_refused`]) or a window reads 100% with no overage carrying it.
pub fn claude(message: &Value) -> Usage {
    let info = &message["rate_limit_info"];
    let windows = &info["unifiedWindows"];
    let window = |key: &str| -> (Option<f64>, Option<i64>) {
        let w = if windows.is_object() {
            &windows[key]
        } else if info["rateLimitType"] == key {
            info
        } else {
            &Value::Null
        };
        (share(&w["utilization"], 1.0), epoch(&w["resetsAt"]))
    };
    let (five_hour, five_hour_resets) = window("five_hour");
    let (seven_day, seven_day_resets) = window("seven_day");
    let windows = [(five_hour, five_hour_resets), (seven_day, seven_day_resets)];
    let overage = claude_overage(info);
    let counted = if overage { [(None, None); 2] } else { windows };
    Usage {
        five_hour,
        seven_day,
        five_hour_resets,
        seven_day_resets,
        full_until: full_until(counted, claude_refused(info), epoch(&info["resetsAt"])),
        overage,
        ..Usage::default()
    }
}

/// A UTC time as RFC 3339 says it (`2026-10-29T22:00:00.414919+00:00`), to the second.
fn utc(value: &Value) -> Option<i64> {
    let (head, rest) = value.as_str()?.split_at_checked(19)?;
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    matches!(rest, "Z" | "+00:00")
        .then(|| time::parse(&format!("{head}Z")))
        .flatten()
}

/// What asking a tool for its account's usage came to (crate::jobs, `read_usage`).
#[derive(Debug, PartialEq)]
pub enum Asked {
    Said(Usage),
    /// It answered without any: an API key's account, or a login its tool reports none for.
    Silent,
    /// Claude Code took `/usage` for a prompt and asked a model (one without its own `/usage`).
    ModelRan,
}

/// Claude Code's own `/usage`, run by itself (`claude -p /usage --output-format stream-json
/// --verbose`: no model call, claude 2.1.302): the `usage_report` on its answer's line, whose
/// `rate_limits.limits` have `kind` `session` (the 5-hour window) and `weekly_all` (the week),
/// each a `percent` of 0...100 and a `resets_at` in RFC 3339. `None` on a line without one.
/// Its percent is whole (99.5% says 100), so it never marks the account full: a job's refusal
/// does.
pub fn claude_report(line: &Value) -> Option<Usage> {
    let limits = line["usage_report"]["rate_limits"]["limits"].as_array()?;
    let mut usage = Usage::default();
    for limit in limits {
        let window = (share(&limit["percent"], 100.0), utc(&limit["resets_at"]));
        match limit["kind"].as_str() {
            Some("session") => (usage.five_hour, usage.five_hour_resets) = window,
            Some("weekly_all") => (usage.seven_day, usage.seven_day_resets) = window,
            _ => {}
        }
    }
    (usage != Usage::default()).then_some(usage)
}

/// Whether a Claude Code `result` line says a model was asked: a turn, output, a cost or a model's
/// usage. `/usage` answered by Claude Code itself has none of them.
pub fn claude_ran_model(result: &Value) -> bool {
    result["num_turns"].as_u64().is_some_and(|n| n > 0)
        || result["usage"]["output_tokens"]
            .as_u64()
            .is_some_and(|n| n > 0)
        || result["total_cost_usd"].as_f64().is_some_and(|c| c > 0.0)
        || result["modelUsage"]
            .as_object()
            .is_some_and(|m| !m.is_empty())
}

/// One snapshot of Codex's limits (`RateLimitSnapshot`, from `account/rateLimits/read` or a sparse
/// `account/rateLimits/updated`). Only Codex's own limit counts (`limitId` `codex` or none): a
/// model's has its own id. Windows are told apart by length (300 minutes, 10080, give or take the
/// minute Codex's buckets drift by), `usedPercent` is 0...100; a `rateLimitReachedType` means
/// Codex refuses work. `None` when it says nothing.
pub fn codex(limits: &Value) -> Option<Usage> {
    if limits
        .get("limitId")
        .and_then(Value::as_str)
        .is_some_and(|id| id != "codex")
    {
        return None;
    }
    let mut usage = Usage::default();
    for key in ["primary", "secondary"] {
        let w = &limits[key];
        match w["windowDurationMins"].as_u64() {
            Some(299..=301) => {
                usage.five_hour = share(&w["usedPercent"], 100.0);
                usage.five_hour_resets = epoch(&w["resetsAt"]);
            }
            Some(10079..=10081) => {
                usage.seven_day = share(&w["usedPercent"], 100.0);
                usage.seven_day_resets = epoch(&w["resetsAt"]);
            }
            _ => {}
        }
    }
    usage.full_until = full_until(
        [
            (usage.five_hour, usage.five_hour_resets),
            (usage.seven_day, usage.seven_day_resets),
        ],
        limits["rateLimitReachedType"].is_string(),
        None,
    );
    (usage != Usage::default()).then_some(usage)
}

/// The answer to `account/rateLimits/read`: Codex's own bucket from `rateLimitsByLimitId` when the
/// server has it, else the single-bucket `rateLimits`.
pub fn codex_read(result: &Value) -> Option<Usage> {
    match &result["rateLimitsByLimitId"]["codex"] {
        own @ Value::Object(_) => codex(own),
        _ => codex(&result["rateLimits"]),
    }
}

/// Cursor's `GetCurrentPeriodUsage` answer, with its `GetPlanInfo` answer (null when that failed):
/// the highest reported share: included spend over a positive limit, total, Auto or API percent.
/// The reset is the included usage's (a weekly plan's
/// week), else the billing cycle's end. A number and a time are all that's taken from either.
pub fn cursor(current: &Value, plan: &Value) -> Option<Usage> {
    let used = &current["planUsage"];
    let included_spend = used["includedSpend"].as_f64();
    let limit = used["limit"].as_f64();
    let total = used["totalPercentUsed"].as_f64();
    let auto = used["autoPercentUsed"].as_f64();
    let api = used["apiPercentUsed"].as_f64();
    let included = included_spend
        .zip(limit.filter(|limit| *limit > 0.0))
        .map(|(spend, limit)| spend / limit);
    let winner = [
        ("includedSpend", included),
        ("totalPercentUsed", total.map(|n| n / 100.0)),
        ("autoPercentUsed", auto.map(|n| n / 100.0)),
        ("apiPercentUsed", api.map(|n| n / 100.0)),
    ]
    .into_iter()
    .filter_map(|(field, share)| share.map(|share| (field, share)))
    .max_by(|a, b| a.1.total_cmp(&b.1));
    tracing::info!(
        "{}",
        cursor_line(
            [included_spend, limit, total, auto, api],
            winner.map(|(field, _)| field)
        )
    );
    let seven_day = winner?.1.clamp(0.0, 1.0);
    Some(Usage {
        seven_day: Some(seven_day),
        seven_day_resets: cursor_reset(current, plan),
        advisory: true,
        ..Usage::default()
    })
}

pub(crate) fn cursor_reset(current: &Value, plan: &Value) -> Option<i64> {
    let at = |value: &Value| epoch(value).or_else(|| utc(value));
    at(&plan["planInfo"]["includedUsageResetsAt"]).or_else(|| at(&current["billingCycleEnd"]))
}

/// The one log line a Cursor read leaves (USAGE-7): its numbers and which field won. Numbers only, by
/// type, so nothing else of the answer can reach the log.
fn cursor_line(numbers: [Option<f64>; 5], winner: Option<&str>) -> String {
    let [included_spend, limit, total, auto, api] = numbers;
    format!(
        "Cursor usage included_spend={included_spend:?} limit={limit:?} total={total:?} auto={auto:?} api={api:?} winner={winner:?}"
    )
}

/// What a job's turns used, summed as the tools reported them (#731). A count no tool reported
/// stays `None`, never 0. `input` is the input not read from the cache.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<u64>,
    /// What the tool says it cost at API prices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// `api`: the account pays per token (an API key); `plan`: a subscription covers it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priced: Option<String>,
}

fn plus<T: std::ops::Add<Output = T> + Default>(a: Option<T>, b: Option<T>) -> Option<T> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or_default() + b.unwrap_or_default()),
    }
}

impl Tokens {
    /// `self` and `other` together; `other`'s `priced` when it says.
    pub fn plus(&self, other: &Tokens) -> Tokens {
        Tokens {
            input: plus(self.input, other.input),
            output: plus(self.output, other.output),
            cache_read: plus(self.cache_read, other.cache_read),
            cache_write: plus(self.cache_write, other.cache_write),
            cost_usd: plus(self.cost_usd, other.cost_usd),
            priced: other.priced.clone().or_else(|| self.priced.clone()),
        }
    }

    /// What `self` has beyond `other`: what a running total grew by since `other`.
    pub fn minus(&self, other: &Tokens) -> Tokens {
        let less = |a: Option<u64>, b: Option<u64>| a.map(|a| a.saturating_sub(b.unwrap_or(0)));
        Tokens {
            input: less(self.input, other.input),
            output: less(self.output, other.output),
            cache_read: less(self.cache_read, other.cache_read),
            cache_write: less(self.cache_write, other.cache_write),
            cost_usd: self
                .cost_usd
                .map(|c| (c - other.cost_usd.unwrap_or(0.0)).max(0.0)),
            priced: self.priced.clone(),
        }
    }
}

/// A Claude Code `result` line's counts: its `modelUsage` summed over the models when it's there
/// (it counts Claude Code's side calls and subagents' models too), else its `usage`, and
/// `total_cost_usd`. They're its session's so far: after `--resume`, `modelUsage` and
/// `total_cost_usd` carry the earlier turns' too (claude 2.1.302, 2026-10-19), so a job counts
/// what grew since the session's last result (crate::jobs). `priced` from the run's `init` line:
/// `apiKeySource` `none` is the account's subscription login.
// ponytail: `usage` alone is the run's own, so a Claude Code without `modelUsage` would undercount
// its resumed turns; every 2.x sends `modelUsage`.
pub fn claude_tokens(result: &Value, api_key_source: Option<&str>) -> Tokens {
    let count = |v: &Value| v.as_u64();
    let mut tokens = match result["modelUsage"].as_object().filter(|m| !m.is_empty()) {
        Some(models) => models.values().fold(Tokens::default(), |sum, m| {
            sum.plus(&Tokens {
                input: count(&m["inputTokens"]),
                output: count(&m["outputTokens"]),
                cache_read: count(&m["cacheReadInputTokens"]),
                cache_write: count(&m["cacheCreationInputTokens"]),
                ..Tokens::default()
            })
        }),
        None => {
            let usage = &result["usage"];
            Tokens {
                input: count(&usage["input_tokens"]),
                output: count(&usage["output_tokens"]),
                cache_read: count(&usage["cache_read_input_tokens"]),
                cache_write: count(&usage["cache_creation_input_tokens"]),
                ..Tokens::default()
            }
        }
    };
    tokens.cost_usd = result["total_cost_usd"]
        .as_f64()
        .filter(|c| c.is_finite() && *c >= 0.0);
    tokens.priced =
        api_key_source.map(|source| if source == "none" { "plan" } else { "api" }.into());
    tokens
}

/// One of Codex's `tokenUsage` breakdowns (`total` or `last`, from `thread/tokenUsage/updated`).
/// Its `inputTokens` include the cached ones; Codex reports no cache writes or cost.
fn codex_breakdown(b: &Value) -> Option<Tokens> {
    let input = b["inputTokens"].as_u64()?;
    let cached = b["cachedInputTokens"].as_u64();
    Some(Tokens {
        input: Some(input.saturating_sub(cached.unwrap_or(0))),
        output: b["outputTokens"].as_u64(),
        cache_read: cached,
        ..Tokens::default()
    })
}

/// A Codex turn's counts so far, from a `thread/tokenUsage/updated`'s `tokenUsage`: its thread's
/// `total` less what the thread had used before this turn (`before`, worked out from the turn's
/// first update as its `total` less its `last`). A thread resumed in a new round keeps its total,
/// and an update sent twice counts once.
// ponytail: assumes the turn's first update is for a new response; an update Codex repeats
// before the turn's first response would count its `last` twice.
pub fn codex_turn(usage: &Value, before: &mut Option<Tokens>) -> Option<Tokens> {
    let total = codex_breakdown(&usage["total"])?;
    let base = before.get_or_insert_with(|| {
        codex_breakdown(&usage["last"]).map_or_else(Tokens::default, |last| total.minus(&last))
    });
    Some(total.minus(base))
}

/// Codex's `account/read` answer: `apiKey` pays per token, `chatgpt` is a plan. Only the type is
/// read, never who is signed in.
pub fn codex_priced(result: &Value) -> Option<String> {
    match result["account"]["type"].as_str()? {
        "apiKey" => Some("api".into()),
        "chatgpt" => Some("plan".into()),
        _ => None,
    }
}

/// An ACP `session/prompt` answer's `usage`, when the agent reports one.
pub fn acp_tokens(answer: &Value) -> Option<Tokens> {
    let usage = answer.get("usage").filter(|u| u.is_object())?;
    let count = |key: &str| usage[key].as_u64();
    Some(Tokens {
        input: count("inputTokens"),
        output: count("outputTokens"),
        cache_read: count("cachedReadTokens"),
        cache_write: count("cachedWriteTokens"),
        ..Tokens::default()
    })
    .filter(|t| t != &Tokens::default())
}

impl Tokens {
    /// Every token it counted, the cache's too: what a job's share of a window is weighed by.
    // ponytail: cache reads weigh as much as fresh input; weigh them less if shares look off.
    pub fn total(&self) -> u64 {
        [self.input, self.output, self.cache_read, self.cache_write]
            .into_iter()
            .flatten()
            .sum()
    }
}

/// A job's estimated share of its account's 5-hour and weekly windows, in percentage points
/// (#732), with the window (its reset) each sum belongs to, and the job's token total at its
/// account's last report.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WindowShare {
    pub five_hour: Option<f64>,
    pub seven_day: Option<f64>,
    pub five_hour_resets: Option<i64>,
    pub seven_day_resets: Option<i64>,
    pub mark: Option<u64>,
}

impl WindowShare {
    /// What the API shows: `{fiveHour, sevenDay, estimate: true}`, or `None` before any share.
    pub fn to_json(&self) -> Option<Value> {
        let points = |p: Option<f64>| p.map(|p| (p * 100.0).round() / 100.0);
        (self.five_hour.is_some() || self.seven_day.is_some()).then(|| {
            serde_json::json!({"fiveHour": points(self.five_hour), "sevenDay": points(self.seven_day), "estimate": true})
        })
    }

    /// Adds `part` points of the window that resets at `resets` (0 for the 5-hour window, 1 for
    /// the week). A sum from an earlier window starts over.
    pub fn add(&mut self, window: usize, part: f64, resets: Option<i64>) {
        let (sum, kept) = match window {
            0 => (&mut self.five_hour, &mut self.five_hour_resets),
            _ => (&mut self.seven_day, &mut self.seven_day_resets),
        };
        if resets.is_some() && *kept != resets {
            *sum = None;
            *kept = resets;
        }
        *sum = Some(sum.unwrap_or(0.0) + part);
    }
}

/// How many points each window (5-hour, week) rose from `before` to `after`, and its reset. A
/// window whose reset passed, or moved more than a minute later, started over from 0. `None` for
/// a window either report doesn't know.
pub fn rises(before: &Usage, after: &Usage, now: i64) -> [Option<(f64, Option<i64>)>; 2] {
    let rise = |was: Option<f64>, was_resets: Option<i64>, is: Option<f64>, resets: Option<i64>| {
        let new_window = matches!((was_resets, resets), (Some(a), Some(b)) if b > a + 60);
        let was = if new_window {
            Some(0.0)
        } else {
            current(was, was_resets, now)
        };
        Some((((is? - was?) * 100.0).max(0.0), resets))
    };
    [
        rise(
            before.five_hour,
            before.five_hour_resets,
            after.five_hour,
            after.five_hour_resets,
        ),
        rise(
            before.seven_day,
            before.seven_day_resets,
            after.seven_day,
            after.seven_day_resets,
        ),
    ]
}

/// `rise` divided by each running job's tokens since the last report (`weights`); evenly when
/// none reported any.
pub fn split(rise: f64, weights: &[u64]) -> Vec<f64> {
    let total: u64 = weights.iter().sum();
    weights
        .iter()
        .map(|w| {
            if total == 0 {
                rise / weights.len() as f64
            } else {
                rise * *w as f64 / total as f64
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Home(PathBuf);
    impl Home {
        fn new(name: &str) -> Self {
            let home = std::env::temp_dir().join(format!("obid-{name}-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&home).unwrap();
            Self(home)
        }
    }
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn week(used: f64, resets: i64) -> Usage {
        Usage {
            seven_day: Some(used),
            seven_day_resets: Some(resets),
            ..Usage::default()
        }
    }

    #[test]
    fn disk_failure_keeps_fresh_limits_but_does_not_apply_an_unsaved_lift() {
        let home = Home::new("disk-usage");
        let usages = Usages::open(&home.0);
        std::fs::create_dir(home.0.join("usage.json")).unwrap();
        usages.record("a", &week(0.82, i64::MAX));
        assert!(usages.get("a").reserved(time::now()));
        assert!(usages.lift("a", true).is_err());
        assert!(usages.get("a").reserved(time::now()));
    }

    #[test]
    fn a_complete_room_report_clears_only_an_inferred_full_hold() {
        let home = Home::new("inferred-full");
        let usages = Usages::open(&home.0);
        let hold = Usage {
            full_until: Some(time::now() + UNKNOWN_RESET),
            full_until_inferred: true,
            ..Usage::default()
        };
        usages.record("a", &hold);
        usages.record(
            "a",
            &Usage {
                five_hour: Some(0.0),
                ..Usage::default()
            },
        );
        assert!(usages.get("a").full_until_at(time::now()).is_some());
        let room = Usage {
            five_hour: Some(0.0),
            seven_day: Some(0.0),
            ..Usage::default()
        };
        usages.record("a", &room);
        assert!(usages.get("a").full_until.is_none());
        assert!(!Usages::open(&home.0).get("a").full_until_inferred);
        usages.record(
            "a",
            &Usage {
                full_until_inferred: false,
                ..hold
            },
        );
        usages.record("a", &room);
        assert!(usages.get("a").full_until_at(time::now()).is_some());
    }

    #[test]
    fn an_unreadable_file_is_set_aside_and_obid_starts_over() {
        let home = Home::new("bad-usage");
        std::fs::write(home.0.join("usage.json"), "{").unwrap();
        let usages = Usages::open(&home.0);
        assert_eq!(usages.get("a"), Usage::default());
        assert_eq!(
            std::fs::read_to_string(home.0.join("usage.json.unreadable")).unwrap(),
            "{"
        );
        usages.record("a", &week(0.82, i64::MAX));
        assert!(Usages::open(&home.0).get("a").reserved(time::now()));
    }

    /// USAGE-10: Codex's buckets drift by a minute; a window 299 or 10081 minutes long still reads.
    #[test]
    fn usage_10_a_codex_window_a_minute_off_still_reads() {
        let usage = codex(&json!({"primary": {"windowDurationMins": 299, "usedPercent": 20}, "secondary": {"windowDurationMins": 10081, "usedPercent": 40}})).unwrap();
        assert_eq!((usage.five_hour, usage.seven_day), (Some(0.2), Some(0.4)));
    }

    #[test]
    fn codex_windows_and_partial_updates() {
        let limits = json!({"primary": {"windowDurationMins": 10080, "usedPercent": 82, "resetsAt": "2000000000"}, "secondary": {"windowDurationMins": 300, "usedPercent": 20, "resetsAt": 1900000000}, "planType": "plus"});
        let usage = codex(&limits).unwrap();
        assert_eq!(usage.seven_day, Some(0.82));
        assert_eq!(usage.five_hour, Some(0.2));
        assert_eq!(usage.seven_day_resets, Some(2000000000));
        assert_eq!(usage.full_until, None);
        assert!(codex(&json!({"limitId": "model", "primary": limits["primary"]})).is_none());
        assert!(
            codex(&json!({"primary": {"windowDurationMins": 300, "usedPercent": "NaN"}})).is_none()
        );
        let home = Home::new("usage");
        let usages = Usages::open(&home.0);
        usages.record("a", &usage);
        // The usage before and after a report, from one lock.
        let (was, now) = usages.record(
            "a",
            &Usage {
                five_hour: Some(0.4),
                ..Usage::default()
            },
        );
        assert_eq!((was.five_hour, now.five_hour), (Some(0.2), Some(0.4)));
        assert_eq!(now.seven_day, Some(0.82));
        let kept = Usages::open(&home.0).get("a");
        assert_eq!(kept.seven_day, Some(0.82));
        assert_eq!(kept.five_hour, Some(0.4));
        assert_eq!(kept.five_hour_resets, Some(1900000000));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(home.0.join("usage.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    /// LIMITS-5, LIMITS-10: only a reset its tool said, no further off than a five-hour window,
    /// with room in the week, is a five-hour limit. The events are what claude 2.1.302 and
    /// `codex app-server` 0.163 printed.
    #[test]
    fn limits_5_only_a_five_hour_limit_is_waited_out() {
        let now = 1_793_680_000;
        let claude = |limit: &str, reset: i64, five: f64, week: f64| {
            claude(&json!({"type": "rate_limit_event", "rate_limit_info": {
                "status": "rejected", "resetsAt": reset, "rateLimitType": limit,
                "overageStatus": "rejected", "isUsingOverage": false,
                "unifiedWindows": {"five_hour": {"utilization": five, "resetsAt": now + 3600},
                                   "seven_day": {"utilization": week, "resetsAt": now + 3 * 86_400}}}}))
        };
        let session = claude("five_hour", now + 3600, 1.0, 0.4);
        assert!(session.five_hour_limit(now + 3600, now));
        // A reset that has come, or further off than five hours, isn't.
        assert!(!session.five_hour_limit(now, now));
        assert!(!session.five_hour_limit(now + 5 * 3600 + 120, now));
        // The week: full, or in its reserve (lifted, it isn't held).
        let week = claude("seven_day", now + 3 * 86_400, 0.2, 1.0);
        assert!(!week.five_hour_limit(now + 3 * 86_400, now));
        assert!(!week.five_hour_limit(now + 3600, now));
        let mut reserved = claude("five_hour", now + 3600, 1.0, 0.85);
        assert!(!reserved.five_hour_limit(now + 3600, now));
        reserved.reserve_lifted_until = Some(now + 86_400);
        assert!(reserved.five_hour_limit(now + 3600, now));
        // obid's own guess at a reset isn't one its tool said.
        let guessed = Usage {
            full_until: Some(now + UNKNOWN_RESET),
            full_until_inferred: true,
            ..Usage::default()
        };
        assert!(!guessed.five_hour_limit(now + UNKNOWN_RESET, now));
        // A tool that reports no week (Cursor) has only its reset to go by.
        assert!(Usage::default().five_hour_limit(now + 3600, now));

        let codex = |week_used: u64| {
            codex(&json!({"limitId": "codex", "primary": {"usedPercent": 100, "windowDurationMins": 300, "resetsAt": now + 3600},
                "secondary": {"usedPercent": week_used, "windowDurationMins": 10080, "resetsAt": now + 3 * 86_400},
                "rateLimitReachedType": "rate_limit_reached"})).unwrap()
        };
        assert!(codex(40).five_hour_limit(now + 3600, now));
        assert!(!codex(100).five_hour_limit(now + 3 * 86_400, now));
    }

    /// The answer `codex app-server` 0.163 gave on 2026-10-17 for a Plus account at 100% of its
    /// 5-hour window (its `accountId`, credits and upsell taken out).
    #[test]
    fn a_real_codex_answer_marks_the_account_full() {
        let snapshot = json!({"limitId": "codex", "limitName": null, "primary": {"usedPercent": 100, "windowDurationMins": 300, "resetsAt": 1792260805}, "secondary": {"usedPercent": 70, "windowDurationMins": 10080, "resetsAt": 1792724553}, "planType": "plus", "rateLimitReachedType": "rate_limit_reached", "spendControlReached": false});
        let mut model = snapshot.clone();
        model["limitId"] = json!("gpt-5-codex-mini");
        model["primary"]["usedPercent"] = json!(3);
        let read = json!({"ordinaryUsageAllowed": false, "rateLimits": model, "rateLimitsByLimitId": {"codex": snapshot, "gpt-5-codex-mini": model}});
        let usage = codex_read(&read).unwrap();
        assert_eq!(usage.five_hour, Some(1.0));
        assert_eq!(usage.seven_day, Some(0.7));
        assert_eq!(usage.full_until, Some(1792260805));
        assert!(
            usage
                .held(1792250000)
                .unwrap()
                .starts_with("hit its usage limit until")
        );
        assert_eq!(usage.held(1792260806), None);
        // An older server has only the single bucket.
        assert_eq!(
            codex_read(&json!({"rateLimits": snapshot}))
                .unwrap()
                .full_until,
            Some(1792260805)
        );
        // Refused below 100% (credits ran out): held until its next report says more.
        let refused = codex(&json!({"rateLimitReachedType": "workspace_owner_credits_depleted"}))
            .unwrap()
            .full_until
            .unwrap();
        assert!(refused > time::now() && refused <= time::now() + UNKNOWN_RESET);
    }

    /// The `usage_report` `claude -p /usage` 2.1.302 sent on 2026-10-27; any other line has none.
    #[test]
    fn claude_codes_own_usage_report() {
        let line = json!({"type": "assistant", "usage_report": {"rate_limits": {"limits": [
            {"kind": "session", "group": "session", "percent": 8, "resets_at": "2026-10-27T21:20:00.414894+00:00", "scope": null},
            {"kind": "weekly_all", "group": "weekly", "percent": 91, "resets_at": "2026-10-29T22:00:00.414919+00:00", "scope": null},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 3, "resets_at": "2026-10-29T22:00:00Z", "scope": {"model": {"display_name": "Fable"}}}
        ]}}});
        let usage = claude_report(&line).unwrap();
        assert_eq!(usage.five_hour, Some(0.08));
        assert_eq!(usage.seven_day, Some(0.91));
        assert_eq!(usage.five_hour_resets, time::parse("2026-10-27T21:20:00Z"));
        assert_eq!(usage.seven_day_resets, time::parse("2026-10-29T22:00:00Z"));
        assert_eq!(usage.full_until, None);
        // 100% may be 99.5% rounded: never full by itself.
        let full = json!({"usage_report": {"rate_limits": {"limits": [
            {"kind": "weekly_all", "percent": 100, "resets_at": "2026-10-29T22:00:00Z"}]}}});
        let full = claude_report(&full).unwrap();
        assert_eq!((full.seven_day, full.full_until), (Some(1.0), None));
        // A time that isn't UTC isn't guessed at.
        let local = json!({"usage_report": {"rate_limits": {"limits": [
            {"kind": "weekly_all", "percent": 40, "resets_at": "2026-10-29T19:00:00-03:00"}]}}});
        assert_eq!(claude_report(&local).unwrap().seven_day_resets, None);
        assert_eq!(claude_report(&json!({"type": "result"})), None);
        assert_eq!(
            claude_report(&json!({"usage_report": {"rate_limits": null}})),
            None
        );
    }

    /// `claude -p /usage` 2.1.302's `result` (2026-10-27) asked no model; a prompt's does.
    #[test]
    fn a_result_says_whether_a_model_ran() {
        let own = json!({"type": "result", "num_turns": 0, "total_cost_usd": 0, "usage": {"input_tokens": 0, "output_tokens": 0}, "modelUsage": {}});
        assert!(!claude_ran_model(&own));
        for asked in [
            json!({"num_turns": 1}),
            json!({"usage": {"output_tokens": 3}}),
            json!({"total_cost_usd": 0.0001}),
            json!({"modelUsage": {"claude-haiku-4-5": {"inputTokens": 9}}}),
        ] {
            assert!(claude_ran_model(&asked), "{asked}");
        }
    }

    /// The event `claude -p` 2.x sent on 2026-10-17.
    #[test]
    fn a_real_claude_event_and_an_older_one() {
        let event = json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed_warning", "resetsAt": 1792764000, "rateLimitType": "seven_day", "utilization": 0.54, "overageStatus": "rejected", "isUsingOverage": false, "unifiedWindows": {"five_hour": {"utilization": 0.45, "resetsAt": 1792260000}, "seven_day": {"utilization": 0.54, "resetsAt": 1792764000}}}});
        let usage = claude(&event);
        assert_eq!(usage.five_hour, Some(0.45));
        assert_eq!(usage.seven_day, Some(0.54));
        assert_eq!(usage.five_hour_resets, Some(1792260000));
        assert_eq!(usage.seven_day_resets, Some(1792764000));
        assert_eq!(usage.full_until, None);
        let older = json!({"rate_limit_info": {"status": "rejected", "rateLimitType": "five_hour", "utilization": 1, "resetsAt": 1792260000000_i64}});
        let usage = claude(&older);
        assert_eq!(usage.five_hour, Some(1.0));
        assert_eq!(usage.seven_day, None);
        assert_eq!(usage.five_hour_resets, Some(1792260000));
        assert_eq!(usage.full_until, Some(1792260000));
        let full = claude(
            &json!({"rate_limit_info": {"unifiedWindows": {"five_hour": {"utilization": 1, "resetsAt": 400}}}}),
        );
        assert_eq!(full.full_until, Some(400));
    }

    #[test]
    fn times_are_seconds_or_milliseconds_and_never_zero() {
        assert_eq!(epoch(&json!(1792260000)), Some(1792260000));
        assert_eq!(epoch(&json!(1792260000123_i64)), Some(1792260000));
        assert_eq!(epoch(&json!("1792260000")), Some(1792260000));
        assert_eq!(epoch(&json!(0)), None);
        assert_eq!(epoch(&json!(-5)), None);
        assert_eq!(epoch(&json!(1e300)), Some(LAST));
        assert_eq!(epoch(&Value::Null), None);
    }

    #[test]
    fn reserves_expire_and_near_full_limits_hold() {
        let mut usage = week(0.82, 200);
        assert!(usage.reserved(100));
        assert!(
            usage
                .held(100)
                .unwrap()
                .contains("weekly reserve (82% of its week used)")
        );
        usage.reserve_lifted_until = Some(200);
        assert_eq!(usage.held(100), None);
        usage.five_hour = Some(0.95);
        assert!(
            usage
                .held(100)
                .unwrap()
                .contains("95% of its 5-hour window")
        );
        usage.five_hour_resets = Some(99);
        assert_eq!(usage.held(100), None);
        usage.seven_day = Some(0.95);
        assert!(usage.held(100).unwrap().contains("95% of its week"));
        // Past the week's reset it starts over.
        assert_eq!(usage.seven_day_at(200), Some(0.0));
        assert_eq!(usage.held(201), None);
        usage.full_until = Some(300);
        assert!(usage.held(201).is_some());
        assert_eq!(usage.held(300), None);
    }

    /// When an account takes automatic work again: the latest reset of what holds it (#871).
    #[test]
    fn free_at_is_the_latest_reset_that_holds_it() {
        let mut usage = week(0.5, 900);
        assert_eq!(usage.free_at(100), None, "it has room");
        usage.full_until = Some(300);
        assert_eq!(usage.free_at(100), Some(300));
        usage.five_hour = Some(0.97);
        usage.five_hour_resets = Some(400);
        assert_eq!(usage.free_at(100), Some(400));
        usage.seven_day = Some(0.85);
        assert_eq!(usage.free_at(100), Some(900), "its weekly reserve too");
        usage.seven_day_resets = None;
        assert_eq!(
            usage.free_at(100),
            Some(400),
            "a reset it didn't say isn't known"
        );
    }

    #[test]
    fn lifting_lasts_until_the_week_resets() {
        let home = Home::new("lift");
        let usages = Usages::open(&home.0);
        let resets = time::now() + 2 * 86_400;
        usages.record("a", &week(0.82, resets));
        assert_eq!(
            usages.lift("a", true).unwrap().reserve_lifted_until,
            Some(resets)
        );
        assert!(!Usages::open(&home.0).get("a").reserved(time::now()));
        assert_eq!(usages.lift("a", false).unwrap().reserve_lifted_until, None);
        assert!(usages.get("a").reserved(time::now()));
        // A removed account's usage goes, kept or not.
        usages.forget("a").unwrap();
        assert_eq!(Usages::open(&home.0).get("a"), Usage::default());
        usages.forget("nobody").unwrap();
        // Unknown reset: a week from now.
        let until = usages
            .lift("b", true)
            .unwrap()
            .reserve_lifted_until
            .unwrap();
        assert!((until - (time::now() + WEEK)).abs() <= 2);
    }

    #[test]
    fn claude_results_count_their_models_and_cost() {
        let result = json!({"type": "result", "total_cost_usd": 0.25,
            "usage": {"input_tokens": 10, "output_tokens": 2, "cache_read_input_tokens": 100, "cache_creation_input_tokens": 5},
            "modelUsage": {
                "claude-opus-5-5": {"inputTokens": 10, "outputTokens": 2, "cacheReadInputTokens": 100, "cacheCreationInputTokens": 5, "costUSD": 0.2},
                "claude-haiku-5-5": {"inputTokens": 3, "outputTokens": 1, "cacheReadInputTokens": 0, "cacheCreationInputTokens": 0, "costUSD": 0.05}}});
        let tokens = claude_tokens(&result, Some("none"));
        assert_eq!(
            tokens,
            Tokens {
                input: Some(13),
                output: Some(3),
                cache_read: Some(100),
                cache_write: Some(5),
                cost_usd: Some(0.25),
                priced: Some("plan".into()),
            }
        );
        let older = json!({"usage": {"input_tokens": 10, "output_tokens": 2}});
        let tokens = claude_tokens(&older, Some("ANTHROPIC_API_KEY"));
        assert_eq!(
            (tokens.input, tokens.cache_read, tokens.cost_usd),
            (Some(10), None, None)
        );
        assert_eq!(tokens.priced.as_deref(), Some("api"));
        assert_eq!(claude_tokens(&json!({}), None), Tokens::default());
        // Summing keeps what no tool reported absent.
        let sum = tokens.plus(&Tokens {
            cache_read: Some(4),
            ..Tokens::default()
        });
        assert_eq!(
            (sum.input, sum.cache_read, sum.cache_write),
            (Some(10), Some(4), None)
        );
        assert_eq!(serde_json::to_value(Tokens::default()).unwrap(), json!({}));
    }

    #[test]
    fn a_codex_turn_counts_what_its_thread_used_since_it_began() {
        let update = |total: u64, last: u64| {
            json!({"total": {"inputTokens": total, "cachedInputTokens": total / 2, "outputTokens": total / 10},
                   "last": {"inputTokens": last, "cachedInputTokens": last / 2, "outputTokens": last / 10}})
        };
        // A resumed thread had used 1000 before this turn; the turn's two responses use 400 and
        // 600, and the second update comes twice.
        let mut before = None;
        assert_eq!(
            codex_turn(&update(1400, 400), &mut before).unwrap().input,
            Some(200)
        );
        let turn = codex_turn(&update(2000, 600), &mut before).unwrap();
        assert_eq!(
            codex_turn(&update(2000, 600), &mut before),
            Some(turn.clone())
        );
        assert_eq!(
            (turn.input, turn.cache_read, turn.output),
            (Some(500), Some(500), Some(100))
        );
        assert_eq!(codex_turn(&json!({}), &mut None), None);
        assert_eq!(
            codex_priced(&json!({"account": {"type": "apiKey"}})).as_deref(),
            Some("api")
        );
        assert_eq!(
            codex_priced(&json!({"account": {"type": "chatgpt", "planType": "plus"}})).as_deref(),
            Some("plan")
        );
        assert_eq!(codex_priced(&json!({"account": null})), None);
        let acp = json!({"stopReason": "end_turn", "usage": {"inputTokens": 7, "outputTokens": 3, "totalTokens": 10}});
        assert_eq!(acp_tokens(&acp).unwrap().input, Some(7));
        assert_eq!(acp_tokens(&json!({"stopReason": "end_turn"})), None);
    }

    #[test]
    fn a_rise_is_split_by_tokens_and_a_new_window_starts_over() {
        // One job used 3× the other's tokens: a 20-point rise splits 15/5; alone, it's all its.
        assert_eq!(split(20.0, &[300, 100]), vec![15.0, 5.0]);
        assert_eq!(split(20.0, &[500]), vec![20.0]);
        assert_eq!(split(20.0, &[0, 0]), vec![10.0, 10.0]);

        let report = |five: f64, resets: i64| Usage {
            five_hour: Some(five),
            five_hour_resets: Some(resets),
            ..Usage::default()
        };
        let [five, week] = rises(&report(0.1, 1000), &report(0.3, 1000), 500);
        let (points, resets) = five.unwrap();
        assert!((points - 20.0).abs() < 1e-9);
        assert_eq!((resets, week), (Some(1000), None));
        // A new window: it rose from 0. Going down in the same one is no rise.
        assert!(
            (rises(&report(0.9, 1000), &report(0.05, 19000), 500)[0]
                .unwrap()
                .0
                - 5.0)
                .abs()
                < 1e-9
        );
        assert_eq!(
            rises(&report(0.3, 1000), &report(0.2, 1000), 500)[0]
                .unwrap()
                .0,
            0.0
        );
        assert_eq!(rises(&Usage::default(), &report(0.2, 1000), 500)[0], None);

        let mut share = WindowShare::default();
        assert_eq!(share.to_json(), None);
        share.add(0, 15.0, Some(1000));
        share.add(0, 0.004, Some(1000));
        assert_eq!(
            share.to_json(),
            Some(json!({"fiveHour": 15.0, "sevenDay": null, "estimate": true}))
        );
        share.add(0, 2.0, Some(19000));
        assert_eq!(share.five_hour, Some(2.0));
    }

    /// Cursor's figure at any number holds nothing, and says nothing of room to a Resume.
    #[test]
    fn usage_9_a_cursor_figure_holds_nothing_whatever_it_reads() {
        let home = Home::new("cursor-figure");
        let usages = Usages::open(&home.0);
        let now = time::now();
        for used in [0.5, 0.85, 0.96, 1.0] {
            let (_, kept) = usages.record(
                "cursor",
                &Usage {
                    seven_day: Some(used),
                    seven_day_resets: Some(now + 86_400),
                    advisory: true,
                    ..Usage::default()
                },
            );
            assert_eq!(kept.seven_day_at(now), Some(used));
            assert!(!kept.reserved(now), "{used}");
            assert!(!kept.only_reserved(now), "{used}");
            assert_eq!(kept.held(now), None, "{used}");
            assert_eq!(kept.free_at(now), None, "{used}");
            assert!(!kept.window_full(now), "{used}");
            assert_eq!(kept.full_until, None, "{used}");
            assert_eq!(kept.seven_day_observed_at, None, "{used}");
        }
        // A real refusal still holds it, until its own reset and not the figure's.
        let refused = Usage {
            full_until: Some(now + 60),
            ..usages.get("cursor")
        };
        assert!(refused.held(now).is_some());
        assert_eq!(refused.free_at(now), Some(now + 60));
        // Another tool's 100% still reads full.
        let claude = Usage {
            seven_day: Some(1.0),
            ..Usage::default()
        };
        assert!(claude.window_full(now));
    }

    /// Claude Code 2.1.302's `rate_limit_event` past its 5-hour window, paid by the account's
    /// overage (`isUsingOverage`, with `status` `rejected`, as the CLI sets it): not full, its
    /// window still shown. The same event with no overage is full.
    #[test]
    fn usage_12_overage_in_use_isnt_full() {
        let event: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/claude-rate-limit-overage.json"
        ))
        .unwrap();
        let usage = claude(&event);
        assert_eq!(usage.full_until, None);
        assert_eq!(
            (usage.five_hour, usage.five_hour_resets),
            (Some(1.0), Some(1_762_880_400))
        );
        assert_eq!(crate::limits::claude_full_until(&event), None);
        // As kept: its 100% window doesn't read full while overage carries it, so a passing error
        // is no limit (crate::jobs); a later report with no overage reads full again.
        let home = Home::new("claude-overage");
        let usages = Usages::open(&home.0);
        let before = 1_762_870_000;
        let (_, kept) = usages.record("claude", &usage);
        assert!(!kept.window_full(before));
        assert!(kept.overage);
        let (_, kept) = usages.record(
            "claude",
            &Usage {
                full_until: Some(before + 60),
                ..Usage::default()
            },
        );
        assert!(kept.overage, "a refusal alone keeps what the windows said");
        let mut spent = event.clone();
        spent["rate_limit_info"]["isUsingOverage"] = json!(false);
        spent["rate_limit_info"]["overageInUse"] = json!(false);
        let (_, kept) = usages.record("claude", &claude(&spent));
        assert!(kept.window_full(before));
        assert_eq!(kept.full_until_at(before), Some(1_762_880_400));
        // Then overage takes the work on (the owner turned it on): the hold goes with it.
        let (_, kept) = usages.record("claude", &usage);
        assert_eq!(kept.full_until, None);
        assert!(!kept.window_full(before));
        assert!(
            kept.held(before).is_some(),
            "new work still prefers room (USAGE-11)"
        );
        // `overageInUse` alone says it too, while the overage still allows.
        let mut in_use = event.clone();
        in_use["rate_limit_info"]["isUsingOverage"] = json!(false);
        assert_eq!(claude(&in_use).full_until, None);
        assert_eq!(crate::limits::claude_full_until(&in_use), None);

        let mut spent = event;
        spent["rate_limit_info"]["isUsingOverage"] = json!(false);
        spent["rate_limit_info"]["overageStatus"] = json!("rejected");
        assert_eq!(claude(&spent).full_until, Some(1_762_880_400));
        assert_eq!(
            crate::limits::claude_full_until(&spent),
            Some(1_762_880_400)
        );
    }

    /// A refusal of one model's week (claude 2.1.302's `seven_day_opus`) leaves the account's
    /// other work going; one of the 5-hour or weekly window, or naming none, fills it.
    #[test]
    fn usage_13_a_scoped_limit_doesnt_fill_the_account() {
        let event: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/claude-rate-limit-scoped.json"
        ))
        .unwrap();
        let typed = |kind: Value| {
            let mut event = event.clone();
            event["rate_limit_info"]["rateLimitType"] = kind;
            event
        };
        for kind in [
            "seven_day_opus",
            "seven_day_sonnet",
            "seven_day_overage_included",
            "overage",
        ] {
            let scoped = typed(json!(kind));
            let usage = claude(&scoped);
            assert_eq!(usage.full_until, None, "{kind}");
            assert_eq!(usage.seven_day, Some(0.58), "{kind}");
            assert_eq!(crate::limits::claude_full_until(&scoped), None, "{kind}");
        }
        for kind in [json!("five_hour"), json!("seven_day"), Value::Null] {
            let whole = typed(kind.clone());
            assert_eq!(claude(&whole).full_until, Some(1_763_290_800), "{kind}");
            assert_eq!(
                crate::limits::claude_full_until(&whole),
                Some(1_763_290_800),
                "{kind}"
            );
        }
        let mut unnamed = event;
        unnamed["rate_limit_info"]
            .as_object_mut()
            .unwrap()
            .remove("rateLimitType");
        assert_eq!(claude(&unnamed).full_until, Some(1_763_290_800));
    }

    /// A read with no figure takes the last one away, and nothing else.
    #[test]
    fn usage_8_a_cursor_read_with_no_figure_forgets_the_last() {
        let home = Home::new("cursor-forget");
        let usages = Usages::open(&home.0);
        let now = time::now();
        assert!(!usages.forget_advisory("cursor"));
        usages.record(
            "cursor",
            &Usage {
                seven_day: Some(0.4),
                seven_day_resets: Some(now + 86_400),
                advisory: true,
                ..Usage::default()
            },
        );
        usages.lift("cursor", true).unwrap();
        assert!(usages.forget_advisory("cursor"));
        let kept = Usages::open(&home.0).get("cursor");
        assert_eq!((kept.seven_day, kept.seven_day_resets), (None, None));
        assert!(!kept.advisory);
        assert!(kept.lifted_until(now).is_some());
        // Another tool's figures are no read of this kind.
        usages.record(
            "claude",
            &Usage {
                seven_day: Some(0.4),
                ..Usage::default()
            },
        );
        assert!(!usages.forget_advisory("claude"));
        assert_eq!(usages.get("claude").seven_day, Some(0.4));
    }

    /// A figure without a reset doesn't keep an old one, which would read as 0%.
    #[test]
    fn usage_7_one_log_line_has_only_numbers_and_the_winning_field() {
        let line = cursor_line(
            [Some(0.0), Some(2000.0), None, Some(72.0), None],
            Some("autoPercentUsed"),
        );
        assert!(
            line.contains("auto=Some(72.0)") && line.contains("limit=Some(2000.0)"),
            "{line}"
        );
        assert!(line.contains("winner=Some(\"autoPercentUsed\")"), "{line}");
    }

    #[test]
    fn usage_7_a_cursor_read_takes_the_place_of_the_last_whole() {
        let home = Home::new("cursor-whole");
        let usages = Usages::open(&home.0);
        let figure = |used, resets| Usage {
            seven_day: Some(used),
            seven_day_resets: resets,
            advisory: true,
            ..Usage::default()
        };
        usages.record("cursor", &figure(0.4, Some(time::now() - 60)));
        let (_, kept) = usages.record("cursor", &figure(0.2, None));
        assert_eq!(kept.seven_day_at(time::now()), Some(0.2));
    }
}
