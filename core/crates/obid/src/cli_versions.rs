//! Each agent CLI's version, as its own `--version` prints it (docs/contracts/cli-versions.md):
//! what `GET /v1/status` lists and a job's round is stamped with. Read again only when the
//! binary's resolved path, size or modified time changed; one that can't be read is none, and
//! never holds anything up.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use crate::agent::Tool;

/// How long `--version` may take (CLIVERSIONS-2).
const LIMIT: Duration = Duration::from_secs(5);
/// How long status takes a tool's last look at its binary as it is.
const FRESH: Duration = Duration::from_secs(60);

/// A tool's binary, absolute, and its version; each none when it isn't found or can't be read.
#[derive(Debug, Clone)]
pub struct Read {
    pub path: Option<PathBuf>,
    pub version: Option<String>,
}

/// A tool's last read: the binary it was of (path, size, modified time), when obid last looked at
/// that, and what it found.
struct Last {
    tool: Tool,
    binary: Option<(PathBuf, u64, SystemTime)>,
    looked: Instant,
    read: Read,
}

static LAST: Mutex<Vec<Last>> = Mutex::new(Vec::new());

/// `tool`'s binary (`bin`, as a run finds it) and its version. Status (`recheck` false) takes the
/// last read while it's under a minute old; a round start looks at the binary every time. Either
/// way `--version` runs only for a binary that changed, never under the lock.
pub fn read(tool: Tool, bin: &str, recheck: bool) -> Read {
    let now = Instant::now();
    if !recheck
        && let Some(read) = lock()
            .iter()
            .find(|l| l.tool == tool && now.duration_since(l.looked) < FRESH)
            .map(|l| l.read.clone())
    {
        return read;
    }
    let path = crate::process::resolve_bin(bin);
    let binary = path.as_deref().and_then(|path| {
        let meta = std::fs::metadata(path).ok()?;
        Some((path.to_owned(), meta.len(), meta.modified().ok()?))
    });
    if let Some(last) = lock()
        .iter_mut()
        .find(|l| l.tool == tool && l.binary.is_some() && l.binary == binary)
    {
        last.looked = now;
        return last.read.clone();
    }
    let version = path.as_deref().and_then(version_of);
    let read = Read { path, version };
    let mut store = lock();
    let before = store.iter().position(|l| l.tool == tool);
    let before = before.map(|at| store.swap_remove(at));
    // Said once per change, however many reads overlapped: decided here, under the lock.
    if before.is_none_or(|b| b.read.version != read.version) {
        let at = read.path.as_deref().unwrap_or(Path::new(bin)).display();
        match &read.version {
            Some(version) => tracing::info!("{} at {at} is version {version}", tool.name()),
            None => tracing::info!("{} at {at} has no version obid can read", tool.name()),
        }
    }
    store.push(Last {
        tool,
        binary,
        looked: now,
        read: read.clone(),
    });
    read
}

/// What a job's transcript says when `tool`'s version changed between two of its rounds
/// (CLIVERSIONS-6): none on a first round, when either can't be read, or when it's the same.
pub fn changed(tool: Tool, was: Option<&str>, now: Option<&str>) -> Option<String> {
    let (was, now) = (was?, now?);
    // Proposed words, Sangam's to change.
    let line = format!(
        "{} changed from {was} to {now} since the last round",
        tool.name()
    );
    (was != now).then_some(line)
}

/// Each tool's tested range and known-bad versions (CLIVERSIONS-7), parsed once: the shipped
/// `cli-versions.json`, or the file a debug build's `OBID_TEST_CLI_VERSIONS` names. One that can't
/// be read or parsed is empty, so every version is untested.
fn table() -> &'static serde_json::Value {
    static TABLE: OnceLock<serde_json::Value> = OnceLock::new();
    TABLE.get_or_init(|| {
        #[cfg(debug_assertions)]
        if let Some(path) = std::env::var_os("OBID_TEST_CLI_VERSIONS") {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            return serde_json::from_str(&text).unwrap_or_default();
        }
        serde_json::from_str(SHIPPED).unwrap_or_default()
    })
}

#[cfg(not(test))]
const SHIPPED: &str = include_str!("../cli-versions.json");
#[cfg(test)]
const SHIPPED: &str = tests::TABLE;

/// `tested`, `untested` or `knownBad` for `tool` at `version`, and a known-bad one's reason
/// (CLIVERSIONS-7): known bad only when listed, tested within `from..=to` by numeric dot-parts.
pub fn status(tool: Tool, version: Option<&str>) -> (&'static str, Option<String>) {
    let (entry, Some(version)) = (&table()[tool.id()], version) else {
        return ("untested", None);
    };
    let mut listed = entry["knownBad"].as_array().into_iter().flatten();
    if let Some(bad) = listed.find(|bad| bad["version"] == version) {
        let reason = bad["reason"].as_str().unwrap_or_default();
        return ("knownBad", Some(reason.into()));
    }
    let [from, to] = ["from", "to"].map(|end| entry["tested"][end].as_str().and_then(parts));
    match (from, parts(version), to) {
        (Some(from), Some(at), Some(to)) if from <= at && at <= to => ("tested", None),
        _ => ("untested", None),
    }
}

/// A version's dot-separated parts, each the number its leading digits make; none when one has none.
fn parts(version: &str) -> Option<Vec<u64>> {
    let digits = |part: &str| part.bytes().take_while(u8::is_ascii_digit).count();
    let number = |part: &str| part[..digits(part)].parse().ok();
    version.split('.').map(number).collect()
}

/// Whether `tool`'s last kept read is a known-bad version (CLIVERSIONS-8): a lookup of what's kept,
/// never a read; none kept is not.
pub fn known_bad(tool: Tool) -> bool {
    let kept = |l: &Last| (l.tool == tool).then(|| l.read.version.clone())?;
    let version = lock().iter().find_map(kept);
    #[cfg(test)]
    let version = tests::KEPT
        .with_borrow(|k| k.get(tool.id()).cloned())
        .or(version);
    status(tool, version.as_deref()).0 == "knownBad"
}

/// What a job's transcript says when its round runs on a version the table knows breaks
/// (CLIVERSIONS-9): none for any other.
pub fn breaks(tool: Tool, version: Option<&str>) -> Option<String> {
    let (name, reason, version) = (tool.name(), status(tool, version).1?, version?);
    // Proposed words, Sangam's to change.
    Some(format!("{name} {version} is known to break: {reason}"))
}

fn lock() -> MutexGuard<'static, Vec<Last>> {
    LAST.lock().unwrap_or_else(|e| e.into_inner())
}

/// What `<bin> --version` printed, as [`parse`] reads it: run with only the locale, terminal,
/// proxy and CA variables, the login `PATH`, and a folder of obid's as its home, temporary and
/// working folder, so no account, login or token reaches it.
fn version_of(bin: &Path) -> Option<String> {
    let scratch = scratch()?;
    let mut command = std::process::Command::new(bin);
    command
        .arg("--version")
        .env_clear()
        .envs(crate::process::kept(|name| std::env::var_os(name)))
        .env("PATH", crate::accounts::login_path(bin))
        .env("HOME", scratch)
        .env("TMPDIR", scratch)
        .current_dir(scratch)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let output = crate::process::output_within(&mut command, LIMIT)?;
    output
        .status
        .success()
        .then(|| parse(&String::from_utf8_lossy(&output.stdout)))?
}

/// The empty folder `--version` runs in: made new by this obid (0700, never one already there), so
/// nobody else's.
fn scratch() -> Option<&'static Path> {
    static SCRATCH: OnceLock<Option<PathBuf>> = OnceLock::new();
    SCRATCH
        .get_or_init(|| {
            use std::os::unix::fs::DirBuilderExt;
            let dir =
                std::env::temp_dir().join(format!("oby-cli-version-{}", uuid::Uuid::new_v4()));
            std::fs::DirBuilder::new().mode(0o700).create(&dir).ok()?;
            Some(dir)
        })
        .as_deref()
}

/// The first word that looks like a version: after an optional `v`, a digit first, a dot in it,
/// only letters, digits and `.+-`, at most 64 characters.
fn parse(said: &str) -> Option<String> {
    said.split_whitespace()
        .map(|word| word.strip_prefix('v').unwrap_or(word))
        .find(|word| {
            word.len() <= 64
                && word.starts_with(|c: char| c.is_ascii_digit())
                && word.contains('.')
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".+-".contains(c))
        })
        .map(str::to_owned)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The unit tests' table: Claude Code 2.1.300 known bad inside its tested range.
    pub(super) const TABLE: &str = r#"{
        "claude": {"tested": {"from": "2.1.296", "to": "2.1.302"},
                   "knownBad": [{"version": "2.1.300", "reason": "resume loses the session", "issue": 1}]},
        "cursor": {"tested": {"from": "2026.10.01", "to": "2026.10.01"}, "knownBad": []}}"#;

    thread_local! {
        /// What [`known_bad`] takes as a tool's last kept read on this test's thread, over `LAST`.
        pub(crate) static KEPT: std::cell::RefCell<std::collections::HashMap<&'static str, String>> = Default::default();
    }

    #[test]
    fn the_shipped_table_parses_with_each_tool() {
        let table: serde_json::Value =
            serde_json::from_str(include_str!("../cli-versions.json")).unwrap();
        for tool in Tool::ALL {
            let entry = &table[tool.id()];
            assert!(entry["tested"]["from"].is_string(), "{}", tool.id());
            assert!(entry["tested"]["to"].is_string(), "{}", tool.id());
            assert!(entry["knownBad"].is_array(), "{}", tool.id());
        }
    }

    #[test]
    fn cliversions_7_a_versions_status_and_its_edges() {
        let claude = |version| status(Tool::Claude, version).0;
        assert_eq!(claude(Some("2.1.298")), "tested", "in range");
        assert_eq!(claude(Some("2.1.296")), "tested", "at from");
        assert_eq!(claude(Some("2.1.302")), "tested", "at to");
        assert_eq!(claude(Some("2.1.295")), "untested", "below");
        assert_eq!(claude(Some("2.1.303")), "untested", "above");
        assert_eq!(claude(Some("2.1.1000")), "untested", "numeric, not text");
        assert_eq!(claude(None), "untested", "no version");
        assert_eq!(claude(Some("2.x.298")), "untested", "a part with no digit");
        assert_eq!(
            status(Tool::Claude, Some("2.1.300")),
            ("knownBad", Some("resume loses the session".into())),
            "listed beats the range"
        );
        assert_eq!(claude(Some("2.1.300-beta")), "tested", "known bad is exact");
        assert_eq!(status(Tool::Cursor, Some("2026.10.1")).0, "tested");
        assert_eq!(status(Tool::Cursor, Some("2026.10.01-d5b3271")).0, "tested");
        assert_eq!(
            status(Tool::Codex, Some("0.162.1")).0,
            "untested",
            "not in the table"
        );
        assert_eq!(
            breaks(Tool::Claude, Some("2.1.300")).as_deref(),
            Some("Claude Code 2.1.300 is known to break: resume loses the session")
        );
        assert_eq!(breaks(Tool::Claude, Some("2.1.301")), None);
    }

    #[test]
    fn the_version_is_the_first_word_that_looks_like_one() {
        assert_eq!(parse("2.1.317 (Claude Code)\n").as_deref(), Some("2.1.317"));
        assert_eq!(parse("codex-cli 0.163.0\n").as_deref(), Some("0.163.0"));
        assert_eq!(
            parse("2025.11.25-d5b3271\n").as_deref(),
            Some("2025.11.25-d5b3271")
        );
        assert_eq!(
            parse("tool v1.4.2+build.7").as_deref(),
            Some("1.4.2+build.7")
        );
        assert_eq!(parse("Usage: tool [options]\n  --version  1 of 2"), None);
        assert_eq!(parse(""), None);
        let long = format!("1.{}", "0".repeat(63));
        assert_eq!(parse(&format!("{long} 2.0")).as_deref(), Some("2.0"));
        assert_eq!(parse(&long), None);
    }

    #[test]
    fn cliversions_6_only_a_real_change_is_said() {
        assert_eq!(changed(Tool::Claude, None, Some("2.1.310")), None);
        assert_eq!(
            changed(Tool::Claude, Some("2.1.300"), Some("2.1.300")),
            None
        );
        assert_eq!(changed(Tool::Claude, Some("2.1.300"), None), None);
        assert_eq!(changed(Tool::Claude, None, None), None);
        let line = changed(Tool::Claude, Some("2.1.300"), Some("2.1.310")).unwrap();
        assert!(line.contains(Tool::Claude.name()), "{line}");
        assert!(
            line.contains("2.1.300") && line.contains("2.1.310"),
            "{line}"
        );
        let line = changed(Tool::Codex, Some("0.162.1"), Some("0.163.0")).unwrap();
        assert!(line.starts_with(Tool::Codex.name()), "{line}");
    }
}
