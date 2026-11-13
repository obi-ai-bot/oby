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
mod tests {
    use super::*;

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
}
