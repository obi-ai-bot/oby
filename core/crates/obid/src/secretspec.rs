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
//!   wrote in the worktree, its git folder and the job's temporary folder ([`Filled::scrub`]),
//!   failing closed: what obid can't take a value out of is removed, and what it can't remove
//!   stops the job. The values are in the vault, so `crate::secrets::known()` takes them out of
//!   everything else obid sends too.
//! - **Someone else's code gets nothing.** Whose code runs decides, not only who asked: another
//!   Oby's review, a job whose agent ran on a teammate's Mac, a follow-up of either (carried by
//!   `Job::foreign`), a job whose conversation read something Oby didn't write, one about a pull
//!   request that isn't the owner's own from a branch of the repository itself, and one whose
//!   base an agent may have written (an `oby/` branch, or its own branch) all run their check
//!   with none of the owner's values, whatever `secretspec.toml` asks ([`Filled::refused`]).
//!
//! The redaction is exact-match: it catches a value a check prints by accident, in the encodings
//! `crate::secrets` knows, not one a check's code turns into something else on purpose. That is
//! why only the owner's own jobs, on the owner's own code, get values at all.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::sandbox::Spec;
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
/// The largest file [`Filled::scrub`] reads whole, as text it can take a value out of; a larger
/// one is read through, and removed when it holds one.
const LARGEST_SCRUBBED: u64 = 4 * 1024 * 1024;

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

/// Why `base` may hold what an agent wrote, so a check on it gets no values, or none: a branch
/// obid's jobs push (`oby/…`, a stacked pull request's base), or the job's own `branch`.
pub fn agent_written_base(base: &str, branch: &str) -> Option<String> {
    (base.starts_with("oby/") || base == branch)
        .then(|| format!("its base, {base}, is a branch an agent may have written"))
}

/// Whether pull request `pr` of `repo` is its owner's own: opened by the login obid's `gh` is
/// signed in as, from a branch of `repo` itself, not a fork. Why not, otherwise; what GitHub can't
/// say is a no.
pub async fn owners_own_pr(gh: &Path, repo: &str, pr: u64) -> Result<(), String> {
    let owner = crate::github_login::read_status(gh)
        .await
        .user
        .ok_or("and obid's gh isn't signed in to GitHub to say whose it is")?;
    let number = pr.to_string();
    let said = crate::github::output(
        gh,
        &[
            "pr",
            "view",
            &number,
            "-R",
            repo,
            "--json",
            "author,isCrossRepository",
        ],
    )
    .await
    .map_err(|_| "and GitHub didn't say whose it is".to_owned())?;
    let answer: serde_json::Value =
        serde_json::from_str(&said).map_err(|_| "and GitHub didn't say whose it is".to_owned())?;
    if answer["isCrossRepository"] != false {
        return Err("which comes from another repository (a fork)".into());
    }
    let author = answer["author"]["login"].as_str().unwrap_or_default();
    if answer["author"]["is_bot"] == true || !author.eq_ignore_ascii_case(&owner) {
        return Err("which someone else opened".into());
    }
    Ok(())
}

/// What a check gets of the secrets its repository declares: the values to put in its environment,
/// what takes them out of everything it wrote, and obid's notes on it (names only).
#[derive(Default)]
pub struct Filled {
    env: Vec<(String, String)>,
    /// Every form a value can take in a file obid reads as bytes ([`forms`]).
    forms: Vec<Vec<u8>>,
    /// The forms, any of them, as one search.
    found: Option<regex::bytes::Regex>,
    secrets: Arc<Secrets>,
    /// What obid says about it at the top of the check's output: names and reasons, never values.
    pub notes: Vec<String>,
}

/// What the job's git folder held before its check ran: what HEAD was, and its own objects. obid
/// can't read inside a compressed object, so a check that wrote to git has that undone.
pub struct Before {
    head: Option<String>,
    objects: HashSet<PathBuf>,
}

/// What [`Filled::scrub`] did, for the check's output: and what it couldn't take a value out of
/// or remove, which stops the job before its agent reads it.
#[derive(Default)]
pub struct Scrubbed {
    pub notes: Vec<String>,
    pub stuck: Vec<PathBuf>,
}

/// What became of one file.
#[derive(Debug, PartialEq, Eq)]
enum Found {
    Clean,
    Cleaned,
    Removed,
    Stuck,
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
                    let (json, json_slash) = json_forms(&value);
                    filled.secrets.learn_forms(
                        name,
                        &[value.as_bytes(), json.as_bytes(), json_slash.as_bytes()],
                    );
                    filled.forms.extend(forms(&value));
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
        filled.forms.sort();
        filled.forms.dedup();
        if !filled.forms.is_empty() {
            let pattern = filled
                .forms
                .iter()
                .map(|form| {
                    // Each byte as itself: ASCII escaped, anything else as `\xHH`.
                    form.iter()
                        .map(|&b| {
                            if b.is_ascii() {
                                regex::escape(&(b as char).to_string())
                            } else {
                                format!("\\x{b:02x}")
                            }
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("|");
            match regex::bytes::RegexBuilder::new(&pattern)
                .unicode(false)
                .size_limit(64 << 20)
                .build()
            {
                Ok(found) => filled.found = Some(found),
                // What obid couldn't look for in files would go unseen: nothing, then.
                Err(_) => {
                    return Filled::refused(format!(
                        "obid gave this check none of the secrets {FILE} declares: it couldn't make the search that takes them out of files."
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
        if self.forms.is_empty() {
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

    /// What the job's git folder holds now, before the check runs: none when it gets no values.
    pub async fn before(&self, spec: &Spec) -> Option<Before> {
        if self.forms.is_empty() {
            return None;
        }
        let env = git_env(spec);
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let head = crate::git::run(
            &spec.worktree,
            &[OsStr::new("rev-parse"), OsStr::new("HEAD")],
            &env,
        )
        .await
        .ok();
        let mut objects = Vec::new();
        if let Some(dir) = objects_dir(spec) {
            files_under(&dir, &|_| false, &mut objects);
        }
        Some(Before {
            head,
            objects: objects.into_iter().collect(),
        })
    }

    /// Takes its values out of everything the check could have written, before the agent or
    /// anything else reads it, failing closed:
    ///
    /// - git: a check that moved HEAD has it put back, and one that wrote objects (a commit, a
    ///   staged file) has its index reset to HEAD and those objects removed, since obid can't read
    ///   inside them; every other file of the job's git folder is read as below.
    /// - the worktree, but for the files git tracks that the check didn't change, and the job's
    ///   temporary folder, whole: never following a link, whatever a file's time says. A text file
    ///   up to 4 MB has each copy, plain or encoded, replaced as [`Filled::redact`] does; any other
    ///   file, of any size, is read through for every form a value takes ([`forms`]), and removed
    ///   when it holds one, as is a file obid can't read or rewrite, or a link that names one.
    ///
    /// What can't be removed is `stuck`: the job stops before its agent reads it.
    pub async fn scrub(&self, spec: &Spec, before: Option<Before>) -> Scrubbed {
        let mut done = Scrubbed::default();
        if self.forms.is_empty() {
            return done;
        }
        let env = git_env(spec);
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let git = |args: &'static [&'static str]| {
            let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
            let env = &env;
            async move { crate::git::run(&spec.worktree, &args, env).await }
        };
        if let Some(before) = before {
            let head = git(&["rev-parse", "HEAD"]).await.ok();
            if let Some(was) = before
                .head
                .as_deref()
                .filter(|was| head.as_deref() != Some(*was))
            {
                let args = [
                    OsStr::new("reset"),
                    OsStr::new("-q"),
                    OsStr::new("--soft"),
                    OsStr::new(was),
                ];
                let _ = crate::git::run(&spec.worktree, &args, &env).await;
                done.notes
                    .push("the check moved HEAD (a commit), so obid put it back.".into());
            }
            let mut now = Vec::new();
            if let Some(dir) = objects_dir(spec) {
                files_under(&dir, &|_| false, &mut now);
            }
            let new: Vec<PathBuf> = now
                .into_iter()
                .filter(|path| !before.objects.contains(path))
                .collect();
            if !new.is_empty() {
                let _ = git(&["reset", "-q"]).await;
                for path in &new {
                    if remove(path) == Found::Stuck {
                        done.stuck.push(path.clone());
                    }
                }
                done.notes.push(format!(
                    "the check wrote to git ({} new object(s): a commit or a staged file), which obid can't read inside, so obid removed them and unstaged what it staged.",
                    new.len()
                ));
            }
        }
        // The files git tracks that the check didn't change: the repository's own, never read.
        let tracked = lines(git(&["ls-files", "-z"]).await.ok());
        let changed = lines(git(&["ls-files", "-m", "-z"]).await.ok());
        let untouched: HashSet<PathBuf> = tracked
            .difference(&changed)
            .map(|path| spec.worktree.join(path))
            .collect();
        let mut files = Vec::new();
        let pointer = spec.worktree.join(".git");
        files_under(
            &spec.worktree,
            &|path| path == pointer || untouched.contains(path),
            &mut files,
        );
        files_under(&spec.temp, &|_| false, &mut files);
        if let Some(admin) = &spec.git_admin {
            let objects = admin.join("objects");
            files_under(admin, &|path| path == objects, &mut files);
        }
        let shown = |path: &Path| {
            path.strip_prefix(&spec.worktree)
                .or_else(|_| path.strip_prefix(&spec.temp))
                .unwrap_or(path)
                .display()
                .to_string()
        };
        let mut cleaned = Vec::new();
        let mut removed = Vec::new();
        for path in files {
            match self.check_file(&path) {
                Found::Clean => {}
                Found::Cleaned => cleaned.push(shown(&path)),
                Found::Removed => removed.push(shown(&path)),
                Found::Stuck => done.stuck.push(path),
            }
        }
        if !cleaned.is_empty() {
            done.notes.push(format!(
                "the check wrote a declared secret in {}, so obid took it out.",
                listed(&cleaned)
            ));
        }
        if !removed.is_empty() {
            done.notes.push(format!(
                "obid removed {}: it held a declared secret obid couldn't take out, or obid couldn't read it.",
                listed(&removed)
            ));
        }
        done
    }

    /// Takes the values out of one file, or removes it.
    fn check_file(&self, path: &Path) -> Found {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return Found::Clean;
        };
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(path).unwrap_or_default();
            return if self.holds(target.as_os_str().as_encoded_bytes()) {
                remove(path)
            } else {
                Found::Clean
            };
        }
        if !meta.is_file() {
            return Found::Clean;
        }
        if meta.len() > LARGEST_SCRUBBED {
            return match self.holds_streamed(path) {
                Some(false) => Found::Clean,
                _ => remove(path),
            };
        }
        let Ok(bytes) = std::fs::read(path) else {
            return remove(path);
        };
        match std::str::from_utf8(&bytes) {
            Ok(text) => {
                let clean = self.secrets.redact(text);
                if self.holds(clean.as_bytes()) {
                    return remove(path);
                }
                if clean == text {
                    return Found::Clean;
                }
                if std::fs::write(path, clean.as_bytes()).is_err() {
                    return remove(path);
                }
                Found::Cleaned
            }
            Err(_) if self.holds(&bytes) => remove(path),
            Err(_) => Found::Clean,
        }
    }

    /// Whether `bytes` hold any form of a value.
    fn holds(&self, bytes: &[u8]) -> bool {
        self.found
            .as_ref()
            .is_some_and(|found| found.is_match(bytes))
    }

    /// Whether the file at `path` holds any form of a value, read through in pieces that overlap
    /// by the longest form; none when it can't be read.
    fn holds_streamed(&self, path: &Path) -> Option<bool> {
        use std::io::Read as _;
        let mut file = std::fs::File::open(path).ok()?;
        let overlap = self.forms.iter().map(Vec::len).max().unwrap_or(0);
        let mut buffer = Vec::with_capacity(PIECE + overlap);
        let mut piece = vec![0u8; PIECE];
        loop {
            let n = file.read(&mut piece).ok()?;
            if n == 0 {
                return Some(false);
            }
            buffer.extend_from_slice(&piece[..n]);
            if self.holds(&buffer) {
                return Some(true);
            }
            let keep = buffer.len().min(overlap);
            buffer.drain(..buffer.len() - keep);
        }
    }
}

/// How much of a large file is read at a time.
const PIECE: usize = 8 * 1024 * 1024;

/// The job's own object folder, when it commits.
fn objects_dir(spec: &Spec) -> Option<PathBuf> {
    spec.git_admin
        .as_ref()
        .filter(|_| spec.commits)
        .map(|admin| admin.join("objects"))
}

/// The job's git environment as obid's own git calls take it.
fn git_env(spec: &Spec) -> Vec<(&'static str, String)> {
    spec.git_env()
        .into_iter()
        .map(|(k, v)| (k, v.to_string_lossy().into_owned()))
        .collect()
}

/// The paths in `git ls-files -z`'s answer.
fn lines(answer: Option<String>) -> HashSet<PathBuf> {
    answer
        .unwrap_or_default()
        .split('\0')
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Up to 10 of `paths`, and how many more.
fn listed(paths: &[String]) -> String {
    let mut text = paths
        .iter()
        .take(10)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if paths.len() > 10 {
        text.push_str(&format!(" and {} more", paths.len() - 10));
    }
    text
}

/// Every file and link under `root`, never following a link, but for what `skip` names.
fn files_under(root: &Path, skip: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if skip(&path) {
                continue;
            }
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(_) => out.push(path),
                Err(_) => {}
            }
        }
    }
}

/// Removes `path`, making it writable first when it isn't.
fn remove(path: &Path) -> Found {
    use std::os::unix::fs::PermissionsExt as _;
    if std::fs::remove_file(path).is_ok() {
        return Found::Removed;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    if std::fs::remove_file(path).is_ok() {
        Found::Removed
    } else {
        Found::Stuck
    }
}

/// `value` as JSON writes it inside a string, and with `/` written `\/` as some encoders do.
fn json_forms(value: &str) -> (String, String) {
    let quoted = serde_json::to_string(value).unwrap_or_default();
    let json = quoted[1..quoted.len().saturating_sub(1).max(1)].to_owned();
    let slash = json.replace('/', "\\/");
    (json, slash)
}

/// Every form `value` takes in a file, as bytes: itself, JSON-escaped, hex, URL-encoded, and the
/// middle of its base64 (standard and URL-safe) at each of the three offsets a longer run can put
/// it at. Forms shorter than [`crate::secrets::SHORTEST`] are left out.
fn forms(value: &str) -> Vec<Vec<u8>> {
    use base64::Engine as _;
    let bytes = value.as_bytes();
    let (json, json_slash) = json_forms(value);
    let mut forms: Vec<Vec<u8>> = vec![
        bytes.to_vec(),
        json.into_bytes(),
        json_slash.into_bytes(),
        hex::encode(bytes).into_bytes(),
        hex::encode_upper(bytes).into_bytes(),
    ];
    let percent: String = bytes.iter().map(|b| format!("%{b:02X}")).collect();
    let url: String = bytes
        .iter()
        .map(|&b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    for text in [percent, url] {
        forms.push(text.to_ascii_lowercase().into_bytes());
        forms.push(text.into_bytes());
    }
    for engine in [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::URL_SAFE,
    ] {
        for offset in 0..3usize {
            let mut padded = vec![0u8; offset];
            padded.extend_from_slice(bytes);
            let encoded = engine.encode(&padded);
            // The characters the zeros before it, or the end, also make are left off.
            let from = [0, 2, 3][offset];
            let to = encoded.len().saturating_sub(4);
            if to > from {
                forms.push(encoded.as_bytes()[from..to].to_vec());
            }
        }
    }
    forms.retain(|form| form.len() >= crate::secrets::SHORTEST);
    forms
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

    fn sh(dir: &Path, script: &str) {
        let ok = std::process::Command::new("/bin/sh")
            .args(["-c", script])
            .current_dir(dir)
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .status()
            .unwrap()
            .success();
        assert!(ok, "{script}");
    }

    /// Whatever its time says, every file the check could have written is read: a copy that kept
    /// an old time (`cp -p`), a large file, one that isn't text, a link, JSON's escaped form. What
    /// can't be taken out goes; what git tracks and the check didn't change is left alone.
    #[tokio::test]
    async fn the_scrub_reads_everything_the_check_could_write_and_fails_closed() {
        use base64::Engine as _;
        let dir = std::env::temp_dir().join(format!("obid-secretspec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let work = dir.join("work");
        let temp = dir.join("tmp");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(temp.join("cache")).unwrap();
        sh(
            &work,
            &format!(
                "git init -q && printf 'repo {FAKE}' > tracked.txt && printf 'x' > edited.txt && git add . && \
                 git -c user.name=T -c user.email=t@example.com -c commit.gpgsign=false commit -q -m one"
            ),
        );
        // What the check writes.
        std::fs::write(work.join("edited.txt"), format!("now {FAKE}\n")).unwrap();
        std::fs::create_dir_all(work.join("logs")).unwrap();
        std::fs::write(work.join("logs/old.log"), format!("token={FAKE}\n")).unwrap();
        sh(&work, "touch -t 200101010000 logs/old.log");
        let escaped = serde_json::to_string(FAKE).unwrap().replace('/', "\\/");
        std::fs::write(work.join("snapshot.json"), format!("{{\"t\": {escaped}}}")).unwrap();
        let mut binary = vec![0xff, 0xfe, 0x00];
        binary.extend_from_slice(FAKE.as_bytes());
        std::fs::write(work.join("blob.bin"), &binary).unwrap();
        let mut big = vec![b'a'; 5 * 1024 * 1024];
        big.extend_from_slice(
            base64::engine::general_purpose::STANDARD
                .encode(format!("x{FAKE}"))
                .as_bytes(),
        );
        big.push(0xff);
        std::fs::write(work.join("bundle.js"), &big).unwrap();
        std::fs::write(work.join("big-clean.js"), vec![b'a'; 5 * 1024 * 1024]).unwrap();
        std::os::unix::fs::symlink(format!("/tmp/{FAKE}"), work.join("link")).unwrap();
        std::fs::write(temp.join("cache/t"), FAKE).unwrap();

        let filled = Filled::fill(&["TOKEN".into()], |_| Ok(Some(FAKE.to_owned())));
        let spec = Spec {
            worktree: work.clone(),
            temp: temp.clone(),
            ..Spec::default()
        };
        let scrubbed = filled.scrub(&spec, filled.before(&spec).await).await;
        assert!(scrubbed.stuck.is_empty());
        let notes = scrubbed.notes.join("\n");
        for cleaned in ["edited.txt", "logs/old.log", "snapshot.json", "cache/t"] {
            let text = std::fs::read_to_string(if cleaned == "cache/t" {
                temp.join(cleaned)
            } else {
                work.join(cleaned)
            })
            .unwrap();
            assert!(text.contains("[check secret: TOKEN]"), "{cleaned}: {text}");
            assert!(notes.contains(cleaned), "{notes}");
        }
        for removed in ["blob.bin", "bundle.js", "link"] {
            assert!(
                std::fs::symlink_metadata(work.join(removed)).is_err(),
                "{removed}"
            );
            assert!(notes.contains(removed), "{notes}");
        }
        // The repository's own file, and a large one with nothing in it, are as they were.
        assert_eq!(
            std::fs::read_to_string(work.join("tracked.txt")).unwrap(),
            format!("repo {FAKE}")
        );
        assert!(work.join("big-clean.js").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_form_is_looked_for_in_bytes() {
        use base64::Engine as _;
        let filled = Filled::fill(&["TOKEN".into()], |_| Ok(Some(FAKE.to_owned())));
        for form in [
            FAKE.to_owned(),
            FAKE.replace('/', "\\/"),
            hex::encode(FAKE),
            hex::encode_upper(FAKE),
            base64::engine::general_purpose::STANDARD.encode(FAKE),
            base64::engine::general_purpose::STANDARD.encode(format!("ab{FAKE}")),
            base64::engine::general_purpose::URL_SAFE.encode(format!("a{FAKE}")),
            FAKE.bytes().map(|b| format!("%{b:02x}")).collect(),
        ] {
            assert!(filled.holds(form.as_bytes()), "{form}");
        }
        assert!(!filled.holds(b"nothing of it here at all"));
        // JSON's escaped form is taken out of text too.
        let clean = filled.redact(&format!("{{\"t\": \"{}\"}}", FAKE.replace('/', "\\/")));
        assert!(!clean.contains("Z9q2"), "{clean}");
    }

    #[test]
    fn a_base_an_agent_may_have_written_gives_nothing() {
        assert!(agent_written_base("oby/fix-12", "oby/build-3").is_some());
        assert!(agent_written_base("feature/x", "feature/x").is_some());
        assert_eq!(agent_written_base("main", "oby/build-3"), None);
        assert_eq!(agent_written_base("dev", "feature/x"), None);
    }

    /// Only the owner's own pull request from a branch of the repository itself counts as the
    /// owner's code: a fork's, a teammate's, or one GitHub won't say about doesn't.
    #[tokio::test]
    async fn only_the_owners_own_pull_request_is_the_owners_code() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = std::env::temp_dir().join(format!("obid-secretspec-gh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let gh = dir.join("gh");
        std::fs::write(
            &gh,
            r#"#!/bin/sh
case "$1 $2" in
  "auth status") echo '{"hosts":{"github.com":[{"login":"wellington","active":true,"state":"success"}]}}' ;;
  "pr view") case "$3" in
    1) echo '{"author":{"login":"Wellington"},"isCrossRepository":false}' ;;
    2) echo '{"author":{"login":"wellington"},"isCrossRepository":true}' ;;
    3) echo '{"author":{"login":"ernesto"},"isCrossRepository":false}' ;;
    4) echo '{"author":{"login":"wellington"}}' ;;
    *) echo 'no such pull request' >&2; exit 1 ;;
  esac ;;
  *) exit 1 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(owners_own_pr(&gh, "o/r", 1).await, Ok(()));
        assert!(
            owners_own_pr(&gh, "o/r", 2)
                .await
                .unwrap_err()
                .contains("fork")
        );
        assert!(
            owners_own_pr(&gh, "o/r", 3)
                .await
                .unwrap_err()
                .contains("someone else")
        );
        assert!(owners_own_pr(&gh, "o/r", 4).await.is_err());
        assert!(
            owners_own_pr(&gh, "o/r", 5)
                .await
                .unwrap_err()
                .contains("didn't say")
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
