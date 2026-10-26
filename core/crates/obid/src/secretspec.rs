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
//! - **Only the owner's own code gets values: an allowlist.** A check gets them only when its
//!   base is a branch the repository lets in only through a reviewed pull request, for the login
//!   Oby pushes as too, as GitHub says ([`protected_base`]; never an `oby/` branch or the job's
//!   own); its worktree is exactly its HEAD ([`worktree_is_head`]); every commit between that base
//!   and HEAD is in obid's own record of what the job made ([`on_record`]; `Job::made`, recorded
//!   after each turn only for commits that pass [`new_commits`]); and the job isn't another Oby's
//!   review, one whose agent ran on a teammate's Mac, a follow-up of a job that got none (carried
//!   by `Job::foreign`) or untrusted. Otherwise it runs with none of the owner's values, whatever
//!   `secretspec.toml` asks ([`Filled::refused`]). A received review is fetched into a clone of its
//!   own, apart from the owner's.
//!
//! **The threat model.** The values protect against other people's code reaching the check:
//! teammates', pull request authors', received work, and content that steered the agent. An agent
//! that never read foreign input (a job that isn't untrusted) counts as the owner's own, as the
//! owner running the tests on their own laptop would. What obid makes sure of is that foreign code
//! can't get in without the job turning untrusted or foreign: a commit brought in from elsewhere
//! (a cherry-pick, a reset or merge to another head, a fetch, `hash-object -w`, an alternates
//! file) fails the turn's look and makes the job foreign; a branch started from (a reused name, a
//! follow-up whose job is gone, a collaborator's push) and obid's rebase of it aren't in the
//! record; files taken from elsewhere and left uncommitted aren't HEAD. What it can't see is an
//! agent that writes foreign code out as its own new commit, having read it in the clone: that is
//! content steering the agent, and the known limit until agents' reads are tracked.
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

/// The commit `base` is at in obid's clone of a repository, and `secretspec.toml` as it has it
/// there: none when it has no such file. Read from `refs/remotes/origin/<base>` in `clone`, which
/// only obid writes (the clone's git folder is read-only in every sandbox), never from a worktree.
pub async fn at_base(clone: &Path, base: &str) -> Result<(String, Option<String>), String> {
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
        Err(_) => return Ok((commit, None)),
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
    let text = String::from_utf8(bytes).map_err(|_| format!("{FILE} on {base} isn't UTF-8."))?;
    Ok((commit, Some(text)))
}

/// Why `base` may hold what an agent wrote, so a check on it gets no values, or none: a branch
/// obid's jobs push (`oby/…`, a stacked pull request's base), or the job's own `branch`.
pub fn agent_written_base(base: &str, branch: &str) -> Option<String> {
    (base.starts_with("oby/") || base == branch)
        .then(|| format!("its base, {base}, is a branch an agent may have written"))
}

/// Whether GitHub says `base` is a branch nothing reaches but through a reviewed pull request,
/// even for the login Oby pushes as (obid's `gh`): a ruleset with a `pull_request` rule that login
/// can't bypass (`current_user_can_bypass` `never`), or classic protection that requires pull
/// request reviews, holds for admins too (or the login isn't one), and doesn't let that login past
/// it. Code there got in only through a pull request someone reviewed. Why not, otherwise; what
/// GitHub can't say is a no. Read from GitHub, never from the job or its configuration.
pub async fn protected_base(gh: &Path, repo: &str, base: &str) -> Result<(), String> {
    let ask = |path: String| async move {
        crate::github::output(gh, &["api", &path])
            .await
            .ok()
            .and_then(|said| serde_json::from_str::<serde_json::Value>(&said).ok())
    };
    let not = format!(
        "its base, {base}, isn't a branch {repo} lets in only through a reviewed pull request, for the login Oby pushes as too"
    );
    let login = crate::github_login::read_status(gh)
        .await
        .user
        .ok_or_else(|| {
            format!("obid's gh isn't signed in to GitHub to ask whether {repo} protects {base}")
        })?;
    // Rulesets: a pull_request rule from one the login can't bypass.
    if let Some(rules) = ask(format!("repos/{repo}/rules/branches/{base}")).await {
        let sets: Vec<u64> = rules
            .as_array()
            .into_iter()
            .flatten()
            .filter(|rule| rule["type"] == "pull_request")
            .filter_map(|rule| rule["ruleset_id"].as_u64())
            .collect();
        for id in sets {
            if let Some(set) = ask(format!("repos/{repo}/rulesets/{id}")).await
                && set["enforcement"] != "disabled"
                && set["enforcement"] != "evaluate"
                && set["current_user_can_bypass"] == "never"
            {
                return Ok(());
            }
        }
    }
    // Classic protection: required reviews, held for admins, with no allowance for the login.
    if let Some(protection) = ask(format!("repos/{repo}/branches/{base}/protection")).await {
        let reviews = &protection["required_pull_request_reviews"];
        let allowed = reviews["bypass_pull_request_allowances"]["users"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|user| {
                user["login"]
                    .as_str()
                    .is_some_and(|l| l.eq_ignore_ascii_case(&login))
            });
        let admin = ask(format!("repos/{repo}"))
            .await
            .map(|r| r["permissions"]["admin"] == true);
        let held = protection["enforce_admins"]["enabled"] == true || admin == Some(false);
        if reviews.is_object() && !allowed && held {
            return Ok(());
        }
    }
    Err(not)
}

/// The most commits on a job's branch obid looks at one by one.
const MOST_COMMITS: usize = 1000;
/// The most commits on other branches whose changes a turn's new commits are compared with.
const MOST_ELSEWHERE: usize = 2000;
/// How far a new commit's times may be from its turn's.
const SKEW: i64 = 300;

/// The commits from `from` (a commit) to HEAD in `worktree`, newest first: none for none, an
/// error past [`MOST_COMMITS`].
async fn commits_since(
    worktree: &Path,
    env: &[(&str, &str)],
    from: &str,
) -> Result<Vec<String>, String> {
    let range = format!("{from}..HEAD");
    let most = format!("--max-count={}", MOST_COMMITS + 1);
    let listed = crate::git::run(
        worktree,
        &[
            OsStr::new("rev-list"),
            OsStr::new(&most),
            OsStr::new(&range),
        ],
        env,
    )
    .await
    .map_err(|_| "obid couldn't list its commits".to_owned())?;
    let commits: Vec<String> = listed
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    if commits.len() > MOST_COMMITS {
        return Err(format!("it has more than {MOST_COMMITS} commits"));
    }
    Ok(commits)
}

/// The patch ids (`git patch-id --stable`) of what `revs` lists, merges left out.
async fn patch_ids(
    worktree: &Path,
    env: &[(&str, &str)],
    revs: &[&str],
) -> Result<HashSet<String>, String> {
    let mut args: Vec<&OsStr> = ["log", "-p", "--no-merges", "--format=commit %H"]
        .iter()
        .map(OsStr::new)
        .collect();
    args.extend(revs.iter().map(OsStr::new));
    let patches = crate::git::run_bytes(worktree, &args, env, None)
        .await
        .map_err(|_| "obid couldn't read the commits' changes".to_owned())?;
    if patches.is_empty() {
        return Ok(HashSet::new());
    }
    let ids = crate::git::run_bytes(
        worktree,
        &[OsStr::new("patch-id"), OsStr::new("--stable")],
        env,
        Some(patches),
    )
    .await
    .map_err(|_| "obid couldn't read the commits' changes".to_owned())?;
    Ok(String::from_utf8_lossy(&ids)
        .lines()
        .filter_map(|line| line.split(' ').next())
        .map(str::to_owned)
        .collect())
}

/// The commits one turn of a job added (from `before`, HEAD when the turn started at `started`,
/// to HEAD now), for obid's record of what the job made: each must be made then (its author and
/// committer the same, both at most [`SKEW`] seconds before the turn began), and none may carry
/// the same change as a commit on another branch the clone has (`--remotes`, not on `base_ref`).
/// What a cherry-pick, a fetch, a merge of another head, `hash-object -w` or a reset to one brings
/// fails that. Why not, otherwise: the job is then someone else's code.
pub async fn new_commits(
    worktree: &Path,
    env: &[(&str, &str)],
    before: &str,
    started: i64,
    base_ref: &str,
) -> Result<Vec<String>, String> {
    let commits = commits_since(worktree, env, before).await?;
    if commits.is_empty() {
        return Ok(commits);
    }
    let range = format!("{before}..HEAD");
    let said = crate::git::run(
        worktree,
        &[
            OsStr::new("log"),
            OsStr::new("--format=%H%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ce%x1f%ct"),
            OsStr::new(&range),
        ],
        env,
    )
    .await
    .map_err(|_| "obid couldn't read its new commits".to_owned())?;
    let now = obi_protocol::time::now();
    for line in said.lines().filter(|l| !l.is_empty()) {
        let fields: Vec<&str> = line.split('\u{1f}').collect();
        let [sha, an, ae, at, cn, ce, ct] = fields[..] else {
            return Err("obid couldn't read its new commits".into());
        };
        let at: i64 = at.parse().unwrap_or(0);
        let ct: i64 = ct.parse().unwrap_or(0);
        let fresh = |t: i64| t >= started - SKEW && t <= now + SKEW;
        if an != cn || ae != ce || !fresh(at) || !fresh(ct) {
            return Err(format!(
                "its agent brought in commit {}, made elsewhere or before its turn (a cherry-pick, a fetch or a merge of another branch)",
                &sha[..12.min(sha.len())]
            ));
        }
    }
    let ours = patch_ids(worktree, env, &[&range]).await?;
    let most = format!("--max-count={MOST_ELSEWHERE}");
    let not = format!("^{base_ref}");
    let elsewhere = patch_ids(worktree, env, &[&most, "--remotes", &not]).await?;
    if !ours.is_disjoint(&elsewhere) {
        return Err(
            "its agent committed a change another branch already has (a cherry-pick or a copy of it)"
                .into(),
        );
    }
    Ok(commits)
}

/// Whether every commit from `base` (a commit) to the job's HEAD is in `record`, obid's own record
/// of what the job (and its follow-up chain) made. Why not, otherwise.
pub async fn on_record(spec: &Spec, base: &str, record: &HashSet<String>) -> Result<(), String> {
    let env = git_env(spec);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let commits = commits_since(&spec.worktree, &env, base)
        .await
        .map_err(|why| format!("{why} on its base"))?;
    match commits.iter().find(|c| !record.contains(*c)) {
        None => Ok(()),
        Some(commit) => Err(format!(
            "commit {} on its branch isn't one obid saw this job make",
            &commit[..12.min(commit.len())]
        )),
    }
}

/// Whether the job's worktree is exactly its HEAD: every file HEAD holds there with the same
/// content (or link target), and no file outside what git ignores that HEAD doesn't hold. What its
/// check runs is then only its commits. Why not, otherwise.
pub async fn worktree_is_head(spec: &Spec) -> Result<(), String> {
    let env = git_env(spec);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let (held, same) = compared(spec, &env).await;
    let held = held.ok_or("obid couldn't compare its files with its last commit")?;
    if held.files.len() != same.len() {
        return Err("files in its worktree differ from its last commit (uncommitted, or taken from elsewhere)".into());
    }
    for (target, link) in &held.links {
        let blob = crate::git::run_bytes(
            &spec.worktree,
            &[
                OsStr::new("cat-file"),
                OsStr::new("blob"),
                OsStr::new(target),
            ],
            &env,
            None,
        )
        .await
        .ok();
        let now = std::fs::read_link(link).ok();
        if blob.as_deref() != now.as_ref().map(|n| n.as_os_str().as_encoded_bytes()) {
            return Err("a link in its worktree differs from its last commit".into());
        }
    }
    // Every file git sees there (in its index, or not ignored) is one HEAD holds.
    let listed = crate::git::run_bytes(
        &spec.worktree,
        &[
            OsStr::new("ls-files"),
            OsStr::new("--cached"),
            OsStr::new("--others"),
            OsStr::new("--exclude-standard"),
            OsStr::new("-z"),
        ],
        &env,
        None,
    )
    .await
    .map_err(|_| "obid couldn't list the files in its worktree".to_owned())?;
    let holds: HashSet<&PathBuf> = held
        .files
        .iter()
        .chain(&held.links)
        .map(|(_, p)| p)
        .collect();
    let extra = listed
        .split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| spec.worktree.join(String::from_utf8_lossy(p).as_ref()))
        .any(|path| !holds.contains(&path));
    if extra {
        return Err("its worktree has files its commits don't hold".into());
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
                    let unicode = unicode_forms(&value);
                    let mut learned: Vec<&[u8]> =
                        vec![value.as_bytes(), json.as_bytes(), json_slash.as_bytes()];
                    learned.extend(unicode.iter().map(|form| form.as_bytes()));
                    filled.secrets.learn_forms(name, &learned);
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
            files_under(&dir, &|_| false, &mut objects, &mut Vec::new());
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
                files_under(&dir, &|_| false, &mut now, &mut done.stuck);
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
        // The files whose content is exactly what HEAD holds: the repository's own, never read.
        let untouched = untouched(spec, &env).await;
        let mut files = Vec::new();
        let mut unreadable = Vec::new();
        let pointer = spec.worktree.join(".git");
        files_under(
            &spec.worktree,
            &|path| path == pointer || untouched.contains(path),
            &mut files,
            &mut unreadable,
        );
        files_under(&spec.temp, &|_| false, &mut files, &mut unreadable);
        if let Some(admin) = &spec.git_admin {
            let objects = admin.join("objects");
            files_under(admin, &|path| path == objects, &mut files, &mut unreadable);
        }
        // What obid couldn't look into may hold a value: the job stops before its agent reads it.
        done.stuck.extend(unreadable);
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

/// The worktree's regular files whose content hashes to the blob HEAD holds for them (`git
/// ls-tree` against `git hash-object --no-filters`): what the check didn't change, whatever git's
/// index says (`--assume-unchanged`, a stat it was told to trust). Anything git can't answer for
/// is left out, so it's read.
async fn untouched(spec: &Spec, env: &[(&str, &str)]) -> HashSet<PathBuf> {
    compared(spec, env).await.1
}

/// What HEAD's tree holds in the worktree: its files and its links, each `(blob, path)`.
#[derive(Default)]
struct Held {
    files: Vec<(String, PathBuf)>,
    links: Vec<(String, PathBuf)>,
}

/// HEAD's tree (none when git can't list it), and the files of it whose content in the worktree
/// hashes to the blob HEAD holds for them.
async fn compared(spec: &Spec, env: &[(&str, &str)]) -> (Option<Held>, HashSet<PathBuf>) {
    let Ok(tree) = crate::git::run_bytes(
        &spec.worktree,
        &[
            OsStr::new("ls-tree"),
            OsStr::new("-r"),
            OsStr::new("-z"),
            OsStr::new("HEAD"),
        ],
        env,
        None,
    )
    .await
    else {
        return (None, HashSet::new());
    };
    // `<mode> blob <sha>\t<path>`, each ended by a NUL.
    let mut held = Held::default();
    for entry in tree.split(|&b| b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        let mut meta = meta.split(' ');
        let (Some(mode), Some("blob"), Some(sha)) = (meta.next(), meta.next(), meta.next()) else {
            continue;
        };
        let full = spec.worktree.join(path);
        match mode {
            "100644" | "100755" => held.files.push((sha.to_owned(), full)),
            "120000" => held.links.push((sha.to_owned(), full)),
            _ => {}
        }
    }
    // Only what is there as a regular file, under a name `--stdin-paths` can take.
    let present: Vec<&(String, PathBuf)> = held
        .files
        .iter()
        .filter(|(_, path)| {
            std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
                && !path.to_string_lossy().contains('\n')
        })
        .collect();
    if present.is_empty() {
        return (Some(held), HashSet::new());
    }
    let input: String = present
        .iter()
        .map(|(_, path)| format!("{}\n", path.display()))
        .collect();
    let Ok(hashed) = crate::git::run_bytes(
        &spec.worktree,
        &[
            OsStr::new("hash-object"),
            OsStr::new("--no-filters"),
            OsStr::new("--stdin-paths"),
        ],
        env,
        Some(input.into_bytes()),
    )
    .await
    else {
        return (Some(held), HashSet::new());
    };
    let hashed = String::from_utf8_lossy(&hashed);
    let hashes: Vec<&str> = hashed.lines().collect();
    if hashes.len() != present.len() {
        return (Some(held), HashSet::new());
    }
    let same = present
        .into_iter()
        .zip(hashes)
        .filter(|((sha, _), now)| sha == now)
        .map(|((_, path), _)| path.clone())
        .collect();
    (Some(held), same)
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

/// Every file and link under `root`, never following a link, but for what `skip` names. A folder
/// it can't open is made openable (0700) and tried again; one it still can't open goes in
/// `unreadable`, as what it can't tell the kind of does: nothing in it was read.
fn files_under(
    root: &Path,
    skip: &dyn Fn(&Path) -> bool,
    out: &mut Vec<PathBuf>,
    unreadable: &mut Vec<PathBuf>,
) {
    use std::os::unix::fs::PermissionsExt as _;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
                match std::fs::read_dir(&dir) {
                    Ok(entries) => entries,
                    Err(_) => {
                        unreadable.push(dir);
                        continue;
                    }
                }
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                unreadable.push(dir.clone());
                continue;
            };
            let path = entry.path();
            if skip(&path) {
                continue;
            }
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(_) => out.push(path),
                Err(_) => unreadable.push(path),
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

/// `value` with `\uXXXX` escapes as JSON encoders write them: every character escaped, every
/// character but ASCII letters and digits, and HTML-safe JSON (`<`, `>`, `&`, `'` and anything
/// past ASCII escaped, `/` as it is and as `\/`); each with lower- and upper-case hex.
fn unicode_forms(value: &str) -> Vec<String> {
    let escape = |c: char| -> String {
        let mut units = [0u16; 2];
        c.encode_utf16(&mut units)
            .iter()
            .map(|u| format!("\\u{u:04x}"))
            .collect()
    };
    let (json, _) = json_forms(value);
    let every: String = value.chars().map(escape).collect();
    let not_alnum: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_string()
            } else {
                escape(c)
            }
        })
        .collect();
    let html: String = json
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | '&' | '\'') || !c.is_ascii() {
                escape(c)
            } else {
                c.to_string()
            }
        })
        .collect();
    let mut forms = Vec::new();
    for form in [every, not_alnum, html.clone(), html.replace('/', "\\/")] {
        // Upper-case hex: only the four digits after each `\u`.
        let upper = upper_escapes(&form);
        forms.push(form);
        forms.push(upper);
    }
    forms.sort();
    forms.dedup();
    forms.retain(|form| form.as_str() != value);
    forms
}

/// `text` with the hex of each `\uXXXX` in upper case.
fn upper_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("\\u") {
        out.push_str(&rest[..at + 2]);
        let hex: String = rest[at + 2..].chars().take(4).collect();
        out.push_str(&hex.to_ascii_uppercase());
        rest = &rest[at + 2 + hex.len()..];
    }
    out.push_str(rest);
    out
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
    forms.extend(unicode_forms(value).into_iter().map(String::into_bytes));
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
            FAKE.chars()
                .map(|c| format!("\\u{:04x}", c as u32))
                .collect(),
            FAKE.chars()
                .map(|c| format!("\\u{:04X}", c as u32))
                .collect(),
            FAKE.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_string()
                    } else {
                        format!("\\u{:04x}", c as u32)
                    }
                })
                .collect(),
        ] {
            assert!(filled.holds(form.as_bytes()), "{form}");
        }
        assert!(!filled.holds(b"nothing of it here at all"));
        // JSON's escaped forms are taken out of text too.
        let clean = filled.redact(&format!("{{\"t\": \"{}\"}}", FAKE.replace('/', "\\/")));
        assert!(!clean.contains("Z9q2"), "{clean}");
        let every: String = FAKE
            .chars()
            .map(|c| format!("\\u{:04x}", c as u32))
            .collect();
        let clean = filled.redact(&format!("{{\"t\": \"{every}\"}}"));
        assert!(clean.contains("[check secret: TOKEN]"), "{clean}");
    }

    #[test]
    fn a_base_an_agent_may_have_written_gives_nothing() {
        assert!(agent_written_base("oby/fix-12", "oby/build-3").is_some());
        assert!(agent_written_base("feature/x", "feature/x").is_some());
        assert_eq!(agent_written_base("main", "oby/build-3"), None);
        assert_eq!(agent_written_base("dev", "feature/x"), None);
    }

    /// A stand-in `gh` in `dir`, answering `api <path>` from `answers` and failing the rest.
    fn fake_gh(dir: &Path, answers: &[(&str, &str)]) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let mut script = String::from(
            "#!/bin/sh\ncase \"$1 $2\" in\n  \"auth status\") echo '{\"hosts\":{\"github.com\":[{\"login\":\"wellington\",\"active\":true,\"state\":\"success\"}]}}' ;;\n",
        );
        for (path, json) in answers {
            script.push_str(&format!("  \"api {path}\") echo '{json}' ;;\n"));
        }
        script.push_str("  *) echo 'Not Found (HTTP 404)' >&2; exit 1 ;;\nesac\n");
        let gh = dir.join("gh");
        std::fs::write(&gh, script).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        gh
    }

    /// Only a base nothing reaches but through a reviewed pull request counts, for the login Oby
    /// pushes as too: a ruleset's `pull_request` rule it can't bypass, or classic protection with
    /// required reviews that holds for it. Weaker rules, a ruleset it can bypass, a default branch
    /// alone, and what GitHub won't say all don't.
    #[tokio::test]
    async fn only_a_base_reached_through_a_reviewed_pull_request_counts() {
        let dir = std::env::temp_dir().join(format!("obid-secretspec-gh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let gh = fake_gh(
            &dir,
            &[
                (
                    "repos/o/r",
                    r#"{"default_branch":"main","permissions":{"admin":true}}"#,
                ),
                (
                    "repos/o/r/rules/branches/main",
                    r#"[{"type":"pull_request","ruleset_id":1}]"#,
                ),
                (
                    "repos/o/r/rulesets/1",
                    r#"{"id":1,"enforcement":"active","current_user_can_bypass":"never"}"#,
                ),
                (
                    "repos/o/r/rules/branches/bypassed",
                    r#"[{"type":"pull_request","ruleset_id":2}]"#,
                ),
                (
                    "repos/o/r/rulesets/2",
                    r#"{"id":2,"enforcement":"active","current_user_can_bypass":"always"}"#,
                ),
                (
                    "repos/o/r/rules/branches/evaluated",
                    r#"[{"type":"pull_request","ruleset_id":3}]"#,
                ),
                (
                    "repos/o/r/rulesets/3",
                    r#"{"id":3,"enforcement":"evaluate","current_user_can_bypass":"never"}"#,
                ),
                (
                    "repos/o/r/rules/branches/weak",
                    r#"[{"type":"non_fast_forward","ruleset_id":4},{"type":"required_signatures","ruleset_id":4}]"#,
                ),
                (
                    "repos/o/r/rulesets/4",
                    r#"{"id":4,"enforcement":"active","current_user_can_bypass":"never"}"#,
                ),
                (
                    "repos/o/r/branches/release/protection",
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":1},"enforce_admins":{"enabled":true}}"#,
                ),
                (
                    "repos/o/r/branches/admins-skip/protection",
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":1},"enforce_admins":{"enabled":false}}"#,
                ),
                (
                    "repos/o/r/branches/allowed/protection",
                    r#"{"required_pull_request_reviews":{"bypass_pull_request_allowances":{"users":[{"login":"Wellington"}]}},"enforce_admins":{"enabled":true}}"#,
                ),
                (
                    "repos/o/r/branches/no-reviews/protection",
                    r#"{"required_status_checks":{"strict":true},"enforce_admins":{"enabled":true}}"#,
                ),
            ],
        );
        assert_eq!(protected_base(&gh, "o/r", "main").await, Ok(()));
        assert_eq!(protected_base(&gh, "o/r", "release").await, Ok(()));
        for weak in [
            "bypassed",
            "evaluated",
            "weak",
            "admins-skip",
            "allowed",
            "no-reviews",
            "teammate-fix",
        ] {
            assert!(protected_base(&gh, "o/r", weak).await.is_err(), "{weak}");
        }
        assert!(protected_base(&gh, "o/other", "main").await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A clone of `origin` and a worktree of it at `start`, committing to an object folder of its
    /// own as a job's does: its sandbox spec.
    fn job_worktree(dir: &Path, name: &str, start: &str) -> Spec {
        let clone = dir.join("clone");
        let worktree = dir.join(name);
        sh(
            &clone,
            &format!(
                "git worktree add -q --detach {} {start}",
                worktree.display()
            ),
        );
        let admin = clone.join(".git/worktrees").join(name);
        std::fs::create_dir_all(admin.join("objects")).unwrap();
        Spec {
            worktree,
            git_admin: Some(admin),
            git_common: Some(clone.join(".git")),
            commits: true,
            temp: dir.join(format!("{name}-tmp")),
            ..Spec::default()
        }
    }

    /// `script` in the worktree with the job's git environment, as its agent runs git.
    fn as_job(spec: &Spec, script: &str) {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", script]).current_dir(&spec.worktree);
        for (k, v) in spec.git_env() {
            command.env(k, v);
        }
        assert!(command.status().unwrap().success(), "{script}");
    }

    const COMMIT: &str =
        "git -c user.name=T -c user.email=t@example.com -c commit.gpgsign=false commit -q";

    /// The fixture for the record's tests: an origin whose `main` is the base and whose other
    /// branches were made long ago by someone else (`oby/fix-12`, an earlier teammate-run job's;
    /// `teammate`, one commit on `main`), cloned; the clone, and the base's commit.
    async fn record_fixture(dir: &Path) -> (PathBuf, String) {
        let _ = std::fs::remove_dir_all(dir);
        let origin = dir.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        sh(
            &origin,
            &format!(
                "export GIT_AUTHOR_DATE=2001-01-01T00:00:00 GIT_COMMITTER_DATE=2001-01-01T00:00:00 && \
                 git init -q -b main && echo a > a && git add a && {COMMIT} -m base && \
                 git checkout -q -b oby/fix-12 && echo t > t && git add t && {COMMIT} -m 'a teammate-run job' && \
                 git checkout -q main && git checkout -q -b teammate && echo u > u && git add u && {COMMIT} -m 'a teammate' && \
                 git checkout -q main"
            ),
        );
        sh(dir, "git clone -q origin clone");
        let clone = dir.join("clone");
        // As obid's clones: a fetch doesn't look at the worktrees' own commits.
        sh(
            &clone,
            "git config fetch.hideRefs worktrees/ && echo ignored.log >> .git/info/exclude",
        );
        let base = crate::git::git(&clone, &["rev-parse", "refs/remotes/origin/main"])
            .await
            .unwrap();
        (clone, base)
    }

    fn env_of(spec: &Spec) -> Vec<(&'static str, String)> {
        git_env(spec)
    }

    /// What a turn adds goes in the record only when it was made then, by whoever committed it,
    /// and isn't another branch's change: every way of bringing in someone else's commit makes
    /// the job foreign instead.
    #[tokio::test]
    async fn a_turn_records_only_what_it_made() {
        let dir = std::env::temp_dir().join(format!("obid-secretspec-turn-{}", std::process::id()));
        let (_clone, base) = record_fixture(&dir).await;
        let base_ref = "refs/remotes/origin/main";
        let started = obi_protocol::time::now();
        let turn = |spec: Spec, script: &'static str| async move {
            let env = env_of(&spec);
            let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let before = crate::git::run(
                &spec.worktree,
                &[OsStr::new("rev-parse"), OsStr::new("HEAD")],
                &env,
            )
            .await
            .unwrap();
            as_job(&spec, script);
            new_commits(&spec.worktree, &env, &before, started, base_ref).await
        };
        // Its own commit: recorded.
        let own = turn(job_worktree(&dir, "own", "origin/main"), "echo b > b && git add b && git -c user.name=T -c user.email=t@example.com -c commit.gpgsign=false commit -q -m own").await;
        assert_eq!(own.map(|c| c.len()), Ok(1));
        for (name, script, says) in [
            (
                "cherry-pick",
                "git -c user.name=T -c user.email=t@example.com cherry-pick origin/teammate",
                "made elsewhere",
            ),
            (
                "reset-author",
                "git -c user.name=T -c user.email=t@example.com cherry-pick origin/teammate && git -c user.name=T -c user.email=t@example.com commit -q --amend --reset-author --no-edit",
                "another branch already has",
            ),
            (
                "checkout",
                "git checkout origin/teammate -- . && git -c user.name=T -c user.email=t@example.com -c commit.gpgsign=false commit -q -m mine",
                "another branch already has",
            ),
            (
                "merge",
                "git -c user.name=T -c user.email=t@example.com merge -q --no-ff -m merge origin/teammate",
                "made elsewhere",
            ),
            (
                "reset",
                "git reset -q --hard origin/teammate",
                "made elsewhere",
            ),
            (
                "hash-object",
                "c=$(git cat-file commit origin/teammate | git hash-object -w -t commit --stdin) && git reset -q --hard $c",
                "made elsewhere",
            ),
            (
                "alternates",
                "mkdir -p \"$GIT_OBJECT_DIRECTORY/info\" && echo \"$GIT_ALTERNATE_OBJECT_DIRECTORIES\" > \"$GIT_OBJECT_DIRECTORY/info/alternates\" && git reset -q --hard origin/oby/fix-12",
                "made elsewhere",
            ),
        ] {
            let why = turn(job_worktree(&dir, name, "origin/main"), script)
                .await
                .unwrap_err();
            assert!(why.contains(says), "{name}: {why}");
        }
        let _ = base;
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The check's code is only what obid recorded: a branch it started from (a reused name, a
    /// follow-up whose job is gone, a collaborator's push) isn't, and obid's own rebase of one
    /// doesn't make it so.
    #[tokio::test]
    async fn only_what_obid_recorded_counts() {
        let dir = std::env::temp_dir().join(format!("obid-secretspec-rec-{}", std::process::id()));
        let (clone, base) = record_fixture(&dir).await;
        let own = job_worktree(&dir, "own", "origin/main");
        as_job(&own, &format!("echo b > b && git add b && {COMMIT} -m own"));
        let env = env_of(&own);
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let made = commits_since(&own.worktree, &env, &base).await.unwrap();
        let record: HashSet<String> = made.iter().cloned().collect();
        assert_eq!(on_record(&own, &base, &record).await, Ok(()));
        assert!(on_record(&own, &base, &HashSet::new()).await.is_err());

        // A reused branch name: its old commits aren't in the record, and rebased onto the base
        // as it is now they are new commits obid never recorded either.
        let reused = job_worktree(&dir, "reused", "origin/oby/fix-12");
        assert!(
            on_record(&reused, &base, &record)
                .await
                .unwrap_err()
                .contains("isn't one obid saw")
        );
        sh(
            &dir.join("origin"),
            &format!("echo m > m && git add m && {COMMIT} -m moved"),
        );
        sh(&clone, "git fetch -q origin");
        let now = crate::git::git(&clone, &["rev-parse", "refs/remotes/origin/main"])
            .await
            .unwrap();
        as_job(
            &reused,
            &format!(
                "git -c user.name=T -c user.email=t@example.com rebase -q --onto {now} {base}"
            ),
        );
        assert!(on_record(&reused, &now, &record).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The worktree must be exactly HEAD: changed, removed or taken-from-elsewhere files, and files
    /// HEAD doesn't hold, mean no values; what git ignores is fine.
    #[tokio::test]
    async fn the_worktree_must_be_its_head() {
        let dir = std::env::temp_dir().join(format!("obid-secretspec-wt-{}", std::process::id()));
        let (_clone, _base) = record_fixture(&dir).await;
        let spec = job_worktree(&dir, "clean", "origin/main");
        assert_eq!(worktree_is_head(&spec).await, Ok(()));
        // What git ignores (a build's output) is fine.
        std::fs::write(spec.worktree.join("ignored.log"), "x").unwrap();
        assert_eq!(worktree_is_head(&spec).await, Ok(()));
        for (name, script) in [
            ("checkout", "git checkout -q origin/teammate -- ."),
            ("changed", "echo z > a"),
            (
                "hidden",
                "echo z > a && git update-index --assume-unchanged a",
            ),
            ("removed", "rm a"),
            ("untracked", "echo new > new.txt"),
            ("staged", "echo new > new.txt && git add new.txt"),
        ] {
            let spec = job_worktree(&dir, name, "origin/main");
            as_job(&spec, script);
            assert!(worktree_is_head(&spec).await.is_err(), "{name}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What the scrub can't look into isn't passed over: a folder it can't open, made so again
    /// when it can, and a tracked file git was told not to look at (`--assume-unchanged`).
    #[tokio::test]
    async fn what_the_scrub_cant_see_isnt_passed_over() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir =
            std::env::temp_dir().join(format!("obid-secretspec-hidden-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        sh(
            &work,
            &format!("git init -q && echo x > tracked.txt && git add . && {COMMIT} -m one"),
        );
        // A tracked file the check changed and told git to ignore.
        std::fs::write(work.join("tracked.txt"), format!("{FAKE}\n")).unwrap();
        sh(&work, "git update-index --assume-unchanged tracked.txt");
        // A folder it closed.
        std::fs::create_dir_all(work.join("closed")).unwrap();
        std::fs::write(work.join("closed/log"), FAKE).unwrap();
        std::fs::set_permissions(work.join("closed"), std::fs::Permissions::from_mode(0o000))
            .unwrap();
        let filled = Filled::fill(&["TOKEN".into()], |_| Ok(Some(FAKE.to_owned())));
        let spec = Spec {
            worktree: work.clone(),
            temp: dir.join("tmp"),
            ..Spec::default()
        };
        let scrubbed = filled.scrub(&spec, None).await;
        assert!(scrubbed.stuck.is_empty(), "{:?}", scrubbed.stuck);
        assert!(
            std::fs::read_to_string(work.join("tracked.txt"))
                .unwrap()
                .contains("[check secret: TOKEN]")
        );
        assert!(
            std::fs::read_to_string(work.join("closed/log"))
                .unwrap()
                .contains("[check secret: TOKEN]")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder obid can't open at all is stuck: the job stops before its agent reads it.
    #[test]
    fn a_folder_obid_cant_open_is_stuck() {
        let mut files = Vec::new();
        let mut unreadable = Vec::new();
        let missing =
            std::env::temp_dir().join(format!("obid-secretspec-none-{}", std::process::id()));
        files_under(&missing, &|_| false, &mut files, &mut unreadable);
        assert!(
            files.is_empty() && unreadable.is_empty(),
            "a folder that isn't there is nothing"
        );
        // A path that isn't a folder at all can't be opened as one, whatever its mode.
        let file =
            std::env::temp_dir().join(format!("obid-secretspec-file-{}", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        files_under(&file, &|_| false, &mut files, &mut unreadable);
        assert_eq!(unreadable, std::slice::from_ref(&file));
        let _ = std::fs::remove_file(&file);
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
