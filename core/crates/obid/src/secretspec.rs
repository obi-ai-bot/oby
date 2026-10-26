//! Secrets a repository declares for its check (oby-plan E1, "Secrets you declare";
//! docs/runner-api.md, "obid only: secrets a check declares"): the repository commits
//! `secretspec.toml` (SecretSpec) naming what its check needs, never a value, and the owner keeps
//! each value in their own vault (`crate::vault`) as `check.<NAME>`.
//!
//! - **From the base branch only.** The names are read from the job's base branch as obid's own
//!   clone has it (`refs/remotes/origin/<base>`, whose git folder no sandbox may write), never from
//!   the worktree or the job's branch: a change can't widen what its check gets ([`at_base`]).
//! - **Only into the check.** The values go into the check process's environment ([`Filled::apply`])
//!   and nowhere else: never an agent's environment, prompt, transcript, a message, the chain, a
//!   log line or a pull request. Only names travel.
//! - **Taken out at once.** Every value, plain and in every encoding `crate::secrets` knows, is taken
//!   out of what the check printed before obid keeps or passes on any of it (its output, its
//!   failing tests, its log, the agent's next prompt, the job's report), and out of the files it
//!   wrote in the worktree and the job's temporary folder ([`Filled::scrub_files`]). The values are
//!   in the vault, so `crate::secrets::known()` takes them out of everything else obid sends too.
//! - **A received job gets nothing.** Another Oby's review, or a job whose agent ran on a
//!   teammate's Mac, runs its check with none of the owner's values, whatever its
//!   `secretspec.toml` asks ([`Filled::refused`]).
//!
//! The redaction is exact-match: it catches a value a check prints by accident, in the encodings
//! `crate::secrets` knows, not one a check's code turns into something else on purpose. That is
//! why only the owner's own jobs, whose code their own agent wrote, get values at all.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::secrets::Secrets;

/// The file a repository declares its secrets in, at its root.
pub const FILE: &str = "secretspec.toml";
/// The profile whose names a check gets (SecretSpec's own default).
pub const PROFILE: &str = "default";
/// What a value's name in the vault starts with: only secrets the owner kept for checks reach one,
/// never a connection's or obid's own.
pub const VAULT_PREFIX: &str = "check.";
/// The most names a check gets.
pub const MOST_NAMES: usize = 32;
/// The largest `secretspec.toml` obid reads.
const LARGEST: usize = 64 * 1024;
/// The largest file [`Filled::scrub_files`] reads: a log or a snapshot a test wrote, not a build's
/// output.
const LARGEST_SCRUBBED: u64 = 4 * 1024 * 1024;
/// The most files [`Filled::scrub_files`] looks at.
const MOST_FILES: usize = 200_000;

/// Names the check's environment already sets, or that change how it runs: never a declared one.
const RESERVED: &[&str] = &[
    "PATH",
    "HOME",
    "TMPDIR",
    "SHELL",
    "USER",
    "LOGNAME",
    "PWD",
    "IFS",
    "ENV",
    "BASH_ENV",
    "PS4",
    "CFFIXED_USER_HOME",
    "GH_CONFIG_DIR",
    "XCRUN_DB",
];
const RESERVED_PREFIXES: &[&str] = &[
    "GIT_",
    "LD_",
    "DYLD_",
    "OBY_",
    "OBID_",
    "OBI_",
    "CLAUDE",
    "ANTHROPIC_",
    "CODEX_",
    "SSH_",
    "BASH_FUNC_",
];

/// Why `name` can't be a declared secret's name, or none: an environment variable's name (letters,
/// digits and `_`, not starting with a digit, at most 64), not one the check's environment sets.
pub fn name_problem(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let fits = !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_');
    if !fits {
        return Some(format!(
            "{name:?} isn't an environment variable's name (letters, digits and _, at most 64)."
        ));
    }
    let upper = name.to_ascii_uppercase();
    if RESERVED.contains(&upper.as_str()) || RESERVED_PREFIXES.iter().any(|p| upper.starts_with(p))
    {
        return Some(format!(
            "{name} is set by obid for every check, so it can't be declared."
        ));
    }
    None
}

/// The names the [`PROFILE`] profile of a `secretspec.toml` declares, in its order: the keys of
/// `[profiles.default]` (`NAME = { description = "…" }`) and the tables `[profiles.default.NAME]`.
///
/// SecretSpec's file is TOML; obid reads only the part it needs, and strictly: a line it can't
/// read in that part, a multi-line string anywhere, a name declared twice or more than
/// [`MOST_NAMES`] names make the whole file an error, and then the check gets nothing. Values and
/// defaults are never read: a default is the repository's to apply.
pub fn names(text: &str) -> Result<Vec<String>, String> {
    if text.len() > LARGEST {
        return Err(format!("{FILE} is over 64 KB."));
    }
    if text.contains("\"\"\"") || text.contains("'''") {
        return Err(format!(
            "{FILE} has a multi-line string, which obid doesn't read."
        ));
    }
    let profile = format!("profiles.{PROFILE}");
    let mut names: Vec<String> = Vec::new();
    let mut add = |name: &str, line: usize| -> Result<(), String> {
        if let Some(why) = name_problem(name) {
            return Err(format!("{FILE}, line {line}: {why}"));
        }
        if names.iter().any(|n| n == name) {
            return Err(format!("{FILE}, line {line}: {name} is declared twice."));
        }
        if names.len() == MOST_NAMES {
            return Err(format!("{FILE} declares more than {MOST_NAMES} secrets."));
        }
        names.push(name.to_owned());
        Ok(())
    };
    // Whether the lines now are in `[profiles.default]` itself.
    let mut inside = false;
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            let header = table_header(trimmed).ok_or_else(|| {
                format!("{FILE}, line {line}: obid can't read this table header.")
            })?;
            inside = header == profile;
            if let Some(name) = header
                .strip_prefix(&profile)
                .and_then(|rest| rest.strip_prefix('.'))
            {
                add(name, line)?;
            }
            continue;
        }
        if !inside {
            continue;
        }
        let (key, value) = trimmed
            .split_once('=')
            .ok_or_else(|| format!("{FILE}, line {line}: obid can't read this line."))?;
        let key = bare_or_quoted(key.trim())
            .ok_or_else(|| format!("{FILE}, line {line}: obid can't read this key."))?;
        let value = value.trim();
        // One inline table on its line: `{ … }`, with nothing but a comment after its last `}`.
        let closed = value.rfind('}').is_some_and(|end| {
            let rest = value[end + 1..].trim();
            rest.is_empty() || rest.starts_with('#')
        });
        if !value.starts_with('{') || !closed {
            return Err(format!(
                "{FILE}, line {line}: a secret is declared as {key} = {{ description = \"…\" }} on one line."
            ));
        }
        add(&key, line)?;
    }
    Ok(names)
}

/// `[a.b.c]`'s `a.b.c`, its parts bare or quoted, or none. An array of tables (`[[…]]`) is none.
fn table_header(line: &str) -> Option<String> {
    let inner = line.strip_prefix('[')?;
    if inner.starts_with('[') {
        return None;
    }
    let end = inner.find(']')?;
    let rest = inner[end + 1..].trim();
    if !(rest.is_empty() || rest.starts_with('#')) {
        return None;
    }
    let parts: Option<Vec<String>> = inner[..end]
        .split('.')
        .map(|part| bare_or_quoted(part.trim()))
        .collect();
    Some(parts?.join("."))
}

/// A TOML key: bare (letters, digits, `_` and `-`), or in plain double or single quotes with
/// nothing to escape.
fn bare_or_quoted(key: &str) -> Option<String> {
    let unquoted = key
        .strip_prefix('"')
        .and_then(|k| k.strip_suffix('"'))
        .or_else(|| key.strip_prefix('\'').and_then(|k| k.strip_suffix('\'')))
        .unwrap_or(key);
    let fits = !unquoted.is_empty()
        && unquoted
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    fits.then(|| unquoted.to_owned())
}

/// `secretspec.toml` as `base` has it in obid's clone of a repository: none when the base has no
/// such file. Read from `refs/remotes/origin/<base>` in `clone`, which only obid writes (the clone's
/// git folder is read-only in every sandbox), never from a worktree.
pub async fn at_base(clone: &Path, base: &str) -> Result<Option<String>, String> {
    if !crate::git::is_branch_name(base) {
        return Err(format!("{base:?} isn't a branch obid reads {FILE} from."));
    }
    let commit = crate::git::git(
        clone,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/remotes/origin/{base}^{{commit}}"),
        ],
    )
    .await
    .map_err(|_| format!("obid's clone doesn't have {base} yet."))?;
    let path = format!("{commit}:{FILE}");
    match crate::git::git(clone, &["cat-file", "-t", &path]).await {
        Ok(kind) if kind == "blob" => {}
        Ok(_) => return Err(format!("{FILE} on {base} isn't a file.")),
        Err(_) => return Ok(None),
    }
    let bytes = crate::git::run_bytes(
        clone,
        &[
            std::ffi::OsStr::new("cat-file"),
            std::ffi::OsStr::new("blob"),
            path.as_ref(),
        ],
        &[],
        None,
    )
    .await
    .map_err(|e| format!("obid couldn't read {FILE} on {base}: {e}"))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{FILE} on {base} isn't UTF-8."))
}

/// What a check gets of the secrets its repository declares: the values to put in its environment,
/// what takes them out of everything it wrote, and obid's notes on it (names only).
#[derive(Default)]
pub struct Filled {
    env: Vec<(String, String)>,
    /// The values as they are, for files that aren't text.
    raw: Vec<Vec<u8>>,
    secrets: Arc<Secrets>,
    /// What obid says about it at the top of the check's output: names and reasons, never values.
    pub notes: Vec<String>,
}

impl Filled {
    /// Nothing, and why: a received job's check, or one whose `secretspec.toml` can't be read.
    pub fn refused(note: impl Into<String>) -> Filled {
        Filled {
            notes: vec![note.into()],
            ..Filled::default()
        }
    }

    /// The `names` with their values from `lookup` (the vault's `check.<NAME>`): a name the vault
    /// doesn't have, or a vault that can't be read, is left unset and noted.
    pub fn fill(
        names: &[String],
        lookup: impl Fn(&str) -> Result<Option<String>, String>,
    ) -> Filled {
        let mut filled = Filled {
            secrets: Arc::new(Secrets::labelled("check secret")),
            ..Filled::default()
        };
        let mut missing = Vec::new();
        let mut short = Vec::new();
        for name in names {
            match lookup(&format!("{VAULT_PREFIX}{name}")) {
                Ok(Some(value)) if value.len() < crate::secrets::SHORTEST => {
                    short.push(name.as_str());
                }
                Ok(Some(value)) => {
                    filled.secrets.learn(name, value.as_bytes());
                    filled.raw.push(value.clone().into_bytes());
                    filled.env.push((name.clone(), value));
                }
                Ok(None) => missing.push(name.as_str()),
                Err(why) => {
                    return Filled::refused(format!(
                        "obid gave this check none of the secrets {FILE} declares: {why}"
                    ));
                }
            }
        }
        if !short.is_empty() {
            filled.notes.push(format!(
                "obid left {} unset: a value under {} characters can't be taken out of what the check prints.",
                short.join(", "),
                crate::secrets::SHORTEST
            ));
        }
        if !missing.is_empty() {
            filled.notes.push(format!(
                "{FILE} declares {}, which the owner's vault doesn't have as {}: obid left {} unset.",
                missing.join(", "),
                missing
                    .iter()
                    .map(|n| format!("{VAULT_PREFIX}{n}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                if missing.len() == 1 { "it" } else { "them" }
            ));
        }
        filled
    }

    /// The names it gives values to, in order.
    pub fn names(&self) -> Vec<&str> {
        self.env.iter().map(|(name, _)| name.as_str()).collect()
    }

    /// What takes its values out of text, plain and encoded: empty when it has none.
    pub fn secrets(&self) -> Arc<Secrets> {
        self.secrets.clone()
    }

    /// Puts its values in the check's `env`, after everything obid set there: a name `env` already
    /// has keeps obid's value, and is noted.
    pub fn apply(&mut self, env: &mut Vec<(OsString, OsString)>) {
        for (name, value) in &self.env {
            if env.iter().any(|(set, _)| set.as_os_str() == name.as_str()) {
                self.notes.push(format!(
                    "{name} is set by obid for every check, so obid didn't give it the declared value."
                ));
                continue;
            }
            env.push((name.into(), value.into()));
        }
    }

    /// `text` with its values taken out, plain and encoded.
    pub fn redact(&self, text: &str) -> String {
        if self.raw.is_empty() {
            text.to_owned()
        } else {
            self.secrets.redact(text)
        }
    }

    /// The check's `output` (already [`Filled::redact`]ed) with obid's notes before it.
    pub fn noted(&self, output: &str) -> String {
        let mut out: String = self.notes.iter().map(|n| format!("obid: {n}\n")).collect();
        out.push_str(output);
        out
    }

    /// Takes its values out of every file under `roots` the check wrote (changed at or after
    /// `since`), up to 4 MB each, never following a link or going into `.git`: text has each copy,
    /// plain or encoded, replaced as [`Filled::redact`] does; other bytes have each plain copy
    /// overwritten with `*`. The files it changed, for the check's output.
    pub fn scrub_files(&self, roots: &[&Path], since: SystemTime) -> Vec<PathBuf> {
        if self.raw.is_empty() {
            return Vec::new();
        }
        let mut changed = Vec::new();
        let mut seen = 0usize;
        let mut stack: Vec<PathBuf> = roots.iter().map(|r| r.to_path_buf()).collect();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                seen += 1;
                if seen > MOST_FILES {
                    return changed;
                }
                let path = entry.path();
                let Ok(meta) = std::fs::symlink_metadata(&path) else {
                    continue;
                };
                if meta.is_dir() {
                    if entry.file_name() != ".git" {
                        stack.push(path);
                    }
                    continue;
                }
                if !meta.is_file()
                    || meta.len() > LARGEST_SCRUBBED
                    || meta.modified().map_or(true, |at| at < since)
                {
                    continue;
                }
                if self.scrub_file(&path) {
                    changed.push(path);
                }
            }
        }
        changed
    }

    /// Whether `path` held a value, which is now taken out.
    fn scrub_file(&self, path: &Path) -> bool {
        let Ok(bytes) = std::fs::read(path) else {
            return false;
        };
        let clean = match std::str::from_utf8(&bytes) {
            Ok(text) => {
                let clean = self.secrets.redact(text);
                if clean == text {
                    return false;
                }
                clean.into_bytes()
            }
            Err(_) => {
                let mut clean = bytes.clone();
                for value in &self.raw {
                    overwrite(&mut clean, value);
                }
                if clean == bytes {
                    return false;
                }
                clean
            }
        };
        if std::fs::write(path, &clean).is_err() {
            // What can't be cleaned can't be read either.
            let _ = std::fs::remove_file(path);
        }
        true
    }
}

/// Every copy of `value` in `bytes` overwritten with `*`.
fn overwrite(bytes: &mut [u8], value: &[u8]) {
    if value.is_empty() || value.len() > bytes.len() {
        return;
    }
    let mut at = 0;
    while at + value.len() <= bytes.len() {
        if &bytes[at..at + value.len()] == value {
            bytes[at..at + value.len()].fill(b'*');
            at += value.len();
        } else {
            at += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Obviously fake: what the tests put where the owner's vault would be.
    const FAKE: &str = "fake-check-value/Z9q2+not=real";

    #[test]
    fn the_default_profile_names_are_read_in_order() {
        let text = r#"
[project]
name = "app"
revision = "1.0"

[profiles.default]
STRIPE_TEST_KEY = { description = "Stripe's test mode", required = true }
"DATABASE_URL" = { description = "a # in a string", default = "postgres://x" } # trailing

[profiles.development]
ONLY_LOCAL = { description = "not for checks" }

[profiles.default.SIGNING_KEY]
description = "a table of its own"
required = false
"#;
        assert_eq!(
            names(text).unwrap(),
            ["STRIPE_TEST_KEY", "DATABASE_URL", "SIGNING_KEY"]
        );
        assert_eq!(
            names("[project]\nname = \"x\"\n").unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn what_obid_cant_read_strictly_gives_nothing() {
        for (text, why) in [
            ("[profiles.default]\nA = \"\"\"\nx\n\"\"\"\n", "multi-line"),
            (
                "[profiles.default]\nA = {\n description = \"x\" }\n",
                "one line",
            ),
            ("[profiles.default]\nA = { }\nA = { }\n", "twice"),
            ("[profiles.default]\nPATH = { }\n", "set by obid"),
            ("[profiles.default]\nLD_PRELOAD = { }\n", "set by obid"),
            ("[profiles.default]\nGIT_DIR = { }\n", "set by obid"),
            ("[profiles.default]\n\"a b\" = { }\n", "key"),
            ("[profiles.default]\n1ABC = { }\n", "environment variable"),
            ("[[profiles.default]]\n", "table header"),
            ("[profiles.default]\njust words\n", "line"),
        ] {
            let error = names(text).unwrap_err();
            assert!(error.contains(why), "{text:?}: {error}");
        }
        let many: String = (0..=MOST_NAMES)
            .map(|i| format!("K{i} = {{ }}\n"))
            .collect();
        assert!(
            names(&format!("[profiles.default]\n{many}"))
                .unwrap_err()
                .contains("more than")
        );
    }

    #[test]
    fn values_come_from_the_check_names_in_the_vault() {
        let asked = std::sync::Mutex::new(Vec::new());
        let mut filled = Filled::fill(&["TOKEN".into(), "ABSENT".into()], |name| {
            asked.lock().unwrap().push(name.to_owned());
            Ok((name == "check.TOKEN").then(|| FAKE.to_owned()))
        });
        assert_eq!(*asked.lock().unwrap(), ["check.TOKEN", "check.ABSENT"]);
        assert_eq!(filled.names(), ["TOKEN"]);
        let mut env: Vec<(OsString, OsString)> = vec![("PATH".into(), "/bin".into())];
        filled.apply(&mut env);
        assert_eq!(env[1], ("TOKEN".into(), FAKE.into()));
        let note = filled.notes.join("\n");
        assert!(
            note.contains("ABSENT") && note.contains("check.ABSENT"),
            "{note}"
        );
        assert!(!note.contains(FAKE));
    }

    #[test]
    fn a_vault_that_cant_be_read_gives_nothing() {
        let filled = Filled::fill(&["TOKEN".into()], |_| {
            Err("the vault can't be read: x".into())
        });
        assert!(filled.names().is_empty());
        assert!(filled.notes[0].contains("none of the secrets"));
    }

    #[test]
    fn a_value_printed_is_taken_out_plain_and_encoded() {
        use base64::Engine as _;
        let filled = Filled::fill(&["TOKEN".into()], |_| Ok(Some(FAKE.to_owned())));
        let printed = format!(
            "plain {FAKE}\nbase64 {}\nhex {}\n",
            base64::engine::general_purpose::STANDARD.encode(FAKE),
            hex::encode(FAKE)
        );
        let clean = filled.redact(&printed);
        assert!(!clean.contains(FAKE), "{clean}");
        assert!(!clean.contains(&hex::encode(FAKE)), "{clean}");
        assert!(clean.contains("[check secret: TOKEN]"), "{clean}");
        assert!(
            clean.contains("plain ") && clean.contains("hex "),
            "{clean}"
        );
    }

    #[test]
    fn files_the_check_wrote_are_cleaned() {
        let dir = std::env::temp_dir().join(format!("obid-secretspec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let before = dir.join("old.txt");
        std::fs::write(&before, format!("kept {FAKE}")).unwrap();
        // Past a file system's coarsest clock (a second), so the file before is older.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let since = SystemTime::now();
        std::fs::write(dir.join("logs/test.log"), format!("token={FAKE}\n")).unwrap();
        let mut binary = vec![0xff, 0xfe, 0x00];
        binary.extend_from_slice(FAKE.as_bytes());
        std::fs::write(dir.join("blob.bin"), &binary).unwrap();
        std::fs::write(dir.join(".git/inside"), FAKE).unwrap();
        std::fs::write(dir.join("clean.txt"), "nothing here").unwrap();
        let filled = Filled::fill(&["TOKEN".into()], |_| Ok(Some(FAKE.to_owned())));
        let mut changed = filled.scrub_files(&[&dir], since);
        changed.sort();
        assert_eq!(changed, [dir.join("blob.bin"), dir.join("logs/test.log")]);
        let log = std::fs::read_to_string(dir.join("logs/test.log")).unwrap();
        assert_eq!(log, "token=[check secret: TOKEN]\n");
        let blob = std::fs::read(dir.join("blob.bin")).unwrap();
        assert_eq!(&blob[3..], "*".repeat(FAKE.len()).as_bytes());
        // What the check didn't write, and git's own folder, are left alone.
        assert!(std::fs::read_to_string(&before).unwrap().contains(FAKE));
        assert_eq!(
            std::fs::read_to_string(dir.join(".git/inside")).unwrap(),
            FAKE
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refused_check_says_why_and_takes_nothing() {
        let filled = Filled::refused("a review another Oby asked for");
        assert!(filled.names().is_empty());
        assert_eq!(
            filled.noted(&filled.redact("ok\n")),
            "obid: a review another Oby asked for\nok\n"
        );
    }
}
