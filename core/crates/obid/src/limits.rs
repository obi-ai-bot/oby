//! When a tool's usage limit resets (Engine/Jobs.swift, `Limits`): from its own events (Claude
//! Code's `rate_limit_event`, Codex's `account/rateLimits/updated`), or from its words ("…usage
//! limit reached|1760000000", "try again in 2 hours", Cursor's "Your usage limits will reset when
//! your monthly cycle ends on 4/12/2026").

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

/// A hard limit: Claude Code's and Codex's words, and Cursor's (`You've hit your usage limit`,
/// `You've hit your usage limit for Opus`, `You've hit your free requests limit`, `You're out of
/// usage`; docs/cursor.md, "Usage and limits"). Not Cursor's soft gates (`Increase limits for
/// faster responses…`, `not available in the slow pool`): those are a model's pool, not the
/// account.
static LIMIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:usage|rate)[ -]?limit|hit your (?:[\w-]+ ){0,3}limit|limit (?:reached|exceeded)\b|out of usage\b",
    )
    .expect("a regex")
});
/// Claude Code's older line: `Claude AI usage limit reached|1760000000`.
static EPOCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"limit reached\|(\d{10})\b").expect("a regex"));
static WAIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:in|after)\s+\d").expect("a regex"));
static PART: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\d+)\s*(hours?|hrs?|h|minutes?|mins?|m)\b").expect("a regex")
});
/// Cursor's monthly cycle: `…will reset when your monthly cycle ends on 4/12/2026` (month, day,
/// year, as Cursor writes them).
static CYCLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bcycle ends on (\d{1,2})/(\d{1,2})/(\d{4})\b").expect("a regex")
});

/// The text says a usage limit stopped it.
pub fn is_limit(text: &str) -> bool {
    LIMIT.is_match(text)
}

/// When the limit `text` names resets, in seconds since the epoch: an epoch after `|`, the day
/// Cursor's monthly cycle ends (its start, in UTC: a run that day finds out whether the cycle has
/// turned, and an hour's hold follows if not), or a wait ("in 2 hours 15 minutes") from `now`.
/// None when it names none of them.
// ponytail: a clock time ("at 10:45 PM") needs the Mac's zone, which obid can't read without
// unsafe code; the tools' own events give the time instead, and Try again is there.
pub fn reset_in(text: &str, now: i64) -> Option<i64> {
    if let Some(at) = EPOCH.captures(text).and_then(|c| c[1].parse().ok()) {
        return Some(at);
    }
    if let Some(at) = cycle_end(text) {
        return Some(at);
    }
    if !WAIT.is_match(text) {
        return None;
    }
    // Saturating: a made-up number of hours never overflows (the caller bounds the wait).
    let seconds = PART
        .captures_iter(text)
        .map(|part| {
            let n: i64 = part[1].parse().unwrap_or(0);
            n.saturating_mul(if part[2].to_lowercase().starts_with('h') {
                3600
            } else {
                60
            })
        })
        .fold(0_i64, i64::saturating_add);
    (seconds > 0).then_some(now.saturating_add(seconds))
}

/// The day Cursor's monthly cycle ends, as its limit says it (`cycle ends on 4/12/2026`), at
/// 00:00 UTC of that day; none without one, or with a day that isn't one.
pub fn cycle_end(text: &str) -> Option<i64> {
    let found = CYCLE.captures(text)?;
    let (month, day, year): (u32, u32, u32) = (
        found[1].parse().ok()?,
        found[2].parse().ok()?,
        found[3].parse().ok()?,
    );
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    obi_protocol::time::parse(&format!("{year:04}-{month:02}-{day:02}T00:00:00Z"))
}

/// The latest epoch obid takes from a tool: 2100-01-01. Anything past it is a bad value.
const LATEST: f64 = 4_102_444_800.0;

/// An epoch in seconds, from seconds or milliseconds, or none when it isn't a plausible one.
fn epoch(value: &Value) -> Option<i64> {
    let n = value.as_f64()?;
    let seconds = if n > 1e12 { n / 1000.0 } else { n };
    (seconds > 0.0 && seconds <= LATEST).then_some(seconds as i64)
}

/// Claude Code's `rate_limit_event`: when the account resets, if it's full (refused, or a window
/// at 100%): the latest full window's reset (it has room only once every full one resets), else
/// the event's own.
pub fn claude_full_until(message: &Value) -> Option<i64> {
    let info = &message["rate_limit_info"];
    let windows = &info["unifiedWindows"];
    let full: Vec<i64> = ["five_hour", "seven_day"]
        .iter()
        .filter(|w| windows[**w]["utilization"].as_f64().unwrap_or(0.0) >= 1.0)
        .filter_map(|w| epoch(&windows[*w]["resetsAt"]))
        .collect();
    if let Some(at) = full.iter().max() {
        return Some(*at);
    }
    (info["status"] == "rejected")
        .then(|| epoch(&info["resetsAt"]))
        .flatten()
}

/// Codex's `account/rateLimits/updated` (its `rateLimits`): when its own limit resets, if a window
/// is at 100%: the latest full window's reset.
pub fn codex_full_until(limits: &Value) -> Option<i64> {
    // Other limits (a model's reserve) come with their own id and say nothing about Codex's.
    if limits["limitId"].as_str().unwrap_or("codex") != "codex" {
        return None;
    }
    ["primary", "secondary"]
        .iter()
        .filter(|w| limits[**w]["usedPercent"].as_f64().unwrap_or(0.0) >= 100.0)
        .filter_map(|w| epoch(&limits[*w]["resetsAt"]))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_limit_says_when_it_resets() {
        let now = 1_760_000_000;
        assert!(is_limit("Codex hit an error: You've hit your usage limit."));
        assert!(is_limit("Claude AI usage limit reached|1760003600"));
        assert!(!is_limit("The check failed."));
        assert_eq!(
            reset_in("Claude AI usage limit reached|1760003600", now),
            Some(1_760_003_600)
        );
        assert_eq!(
            reset_in("Rate limit exceeded: try again in 2 hours 15 minutes.", now),
            Some(now + 2 * 3600 + 15 * 60)
        );
        assert_eq!(reset_in("You've hit your usage limit.", now), None);
        // A made-up wait saturates instead of overflowing.
        assert_eq!(reset_in("in 3000000000000000 hours", now), Some(i64::MAX));
        assert_eq!(reset_in("Try again at 10:45 PM.", now), None);
    }

    /// Cursor's own words for a spent account (forum and GitHub reports, 2025–2026; docs/cursor.md)
    /// are a limit, with the day its monthly cycle ends as the reset; its soft gates (a model's
    /// slow pool) aren't.
    #[test]
    fn cursors_limits_and_the_day_its_cycle_ends() {
        let now = 1_760_000_000;
        let opus = "You've hit your usage limit for Opus You've saved $12 with Pro. Switch to Auto for more usage or set a Spend Limit to continue with Opus. Your usage limits will reset when your monthly cycle ends on 4/12/2026";
        assert!(is_limit(opus));
        assert_eq!(
            reset_in(opus, now),
            obi_protocol::time::parse("2026-04-12T00:00:00Z")
        );
        assert!(is_limit(
            "You're out of usage. Switch to Auto, or ask your admin to increase your limit to continue."
        ));
        let free = "You've hit your free requests limit. Your usage limits will reset when your monthly cycle ends on 11/17/2025.";
        assert!(is_limit(free));
        assert_eq!(
            reset_in(free, now),
            obi_protocol::time::parse("2025-11-17T00:00:00Z")
        );
        assert!(!is_limit(
            "Increase limits for faster responses: Opus is not available in the slow pool. Please switch to Auto."
        ));
        // A day that isn't one, or a limit with no date, names no reset.
        assert_eq!(cycle_end("cycle ends on 13/40/2026"), None);
        assert_eq!(
            reset_in("You've hit your usage limit. Try again later.", now),
            None
        );
    }

    #[test]
    fn the_tools_events_say_when_a_full_account_resets() {
        let claude = |info: Value| json!({"type": "rate_limit_event", "rate_limit_info": info});
        let full = claude(json!({"status": "allowed_warning", "unifiedWindows": {
            "five_hour": {"utilization": 1.0, "resetsAt": 1_760_003_600},
            "seven_day": {"utilization": 0.4, "resetsAt": 1_760_500_000}}}));
        assert_eq!(claude_full_until(&full), Some(1_760_003_600));
        // Both windows full: it has room only once the later one resets.
        let both = claude(json!({"status": "rejected", "unifiedWindows": {
            "five_hour": {"utilization": 1.0, "resetsAt": 1_760_003_600},
            "seven_day": {"utilization": 1.0, "resetsAt": 1_760_500_000}}}));
        assert_eq!(claude_full_until(&both), Some(1_760_500_000));
        // A reset past 2100 is a bad value.
        let far = claude(json!({"status": "rejected", "resetsAt": 9e15}));
        assert_eq!(claude_full_until(&far), None);
        let refused = claude(json!({"status": "rejected", "resetsAt": 1_760_007_200_000_i64}));
        assert_eq!(claude_full_until(&refused), Some(1_760_007_200));
        let room = claude(json!({"status": "allowed", "unifiedWindows": {
            "five_hour": {"utilization": 0.5, "resetsAt": 1_760_003_600}}}));
        assert_eq!(claude_full_until(&room), None);

        let codex = json!({"limitId": "codex",
            "primary": {"usedPercent": 100.0, "windowDurationMins": 300, "resetsAt": 1_760_003_600},
            "secondary": {"usedPercent": 30.0, "windowDurationMins": 10080, "resetsAt": 1_760_500_000}});
        assert_eq!(codex_full_until(&codex), Some(1_760_003_600));
        let other =
            json!({"limitId": "gpt-reserve", "primary": {"usedPercent": 100.0, "resetsAt": 1}});
        assert_eq!(codex_full_until(&other), None);
    }
}
