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
//! - **Taken out at once.** Every value, plain, JSON-escaped (`\"`, `\\`, `\/`, `\uXXXX`) and in
//!   every encoding `crate::secrets` knows, is taken out of what the check printed before obid
//!   keeps or passes on any of it (its output, its failing tests, its log, the agent's next prompt,
//!   the job's report). The values are in the vault, so `crate::secrets::known()` takes them out of
//!   everything else obid sends too.
//! - **In a checkout of its own.** A check that gets values runs in a checkout obid makes from the
//!   commit the gate passed ([`fresh_checkout`]), with a temporary folder of its own, apart from
//!   everything the job's sandbox can write, and removed after it: nothing the check writes ever
//!   reaches the job, and nothing the job left outside its commits (a file left uncommitted or
//!   hidden by a `.gitignore`, a rewrite after the gate) reaches the check.
//!
//! **The threat model.** The values protect against other people's code reaching the check:
//! teammates', pull request authors', received work, and content that steered the agent. An agent
//! that never read foreign input counts as the owner's own, as the owner running the tests on their
//! own laptop would. Commit metadata can't tell where a tree came from, so obid controls the
//! inputs, and **fails closed**: a job is eligible only if every input it ever received came from
//! an allowlisted channel, and anything else, a channel no one listed included, makes it
//! someone else's code when it arrives (and marks the owner's clone, as below):
//!
//! - **The owner's own words typed in Oby's apps**, which the app signs with its device's key
//!   (`crate::approval::WORDS`, checked in `crate::routes::typed_by_owner`): a job started through
//!   the jobs API (`Job::vetted`), the owner's words sent to it, and what Oby's conversation plans
//!   from them while it has read nothing off this list (below). Anything else holding obid's token
//!   (oby-mcp, a script, an agent, an app that doesn't sign) sends words that aren't: a job they
//!   start, and one they reach, gets nothing. Not a job an event, a routine or an inbox
//!   offer started, nor words from an event, the orchestrator or a Confirm of what Oby proposed
//!   (obid's own Try again aside).
//! - **Oby's conversation, while clean** (`crate::conversation`, `foreign`): it fails closed the
//!   same way. The owner's signed words, Oby's own answers, room files outside the repositories'
//!   checkouts, its own list and a job's end that's clean throughout keep it so; anything else
//!   marks it for good: words no app signed, turns its client wrote, attachments, a supervisor's
//!   turn, a recap, a check-in, a routine's run, another job's report or words, the inbox, past
//!   conversations, the web, a command, a helper, any tool
//!   it doesn't know (`crate::brain::tools`, `reads_foreign`; `crate::brain::harness`,
//!   `known_safe`). What a marked conversation starts is someone else's from the start, in a clone
//!   apart; its words, a tell, a continue or a no's, make the job they reach someone else's
//!   (`crate::brain::written_in`, `written_after`). A memory carries where it was found
//!   (`crate::memory::Memory::foreign`): one from words that weren't the owner's alone, or saved by
//!   a Remember no app signed, takes a job's values away and marks a conversation that reads it.
//! - **The text of an issue or pull request the owner alone wrote**: its body, its title and every
//!   comment, edit, rename, review and review comment, every list whole
//!   (`crate::github_work::owners_only`; [`others_wrote`]), read as the job starts and again at its
//!   check, so what's deleted in between still counts. What obid posted under the owner's login
//!   is signed (`crate::github_work::OBY_SIGNS`), an Oby's, never the owner's: but a pull request's
//!   own description obid wrote only for jobs that are clean throughout (`Jobs::described_clean`,
//!   from their `outgoing`).
//! - **The base repository's code** (2 and 4 below).
//! - **Tool calls of kinds obid knows are safe** ([`tool_is_known_safe`]): a file read, write, edit
//!   or search, a command inside the job's sandbox, its own to-do list, obid's own question to the
//!   owner. Anything else (a vendor's own web search or fetch, any MCP tool or resource, a helper
//!   agent, an MCP server's tool descriptions given at launch, and any kind an agent adds later or
//!   obid doesn't parse: Claude Code's unparsed blocks, Codex's helper threads and unknown items,
//!   Cursor's unknown kinds and its `search`, which its web search reports as too) marks the job.
//!   So does network egress past the tool's API and the default registries (2).
//!
//! What that takes:
//!
//! 1. **The check runs only the commit**, in its own checkout (above). The turn that made it is
//!    over: on macOS its sandbox's watcher kills everything the turn started, a process that
//!    called `setsid` included (`crate::sandbox`, `WATCHER`).
//! 2. **Ref hygiene is the provenance.** obid's own clone of a repository holds only the default
//!    branch (`git clone --single-branch`, `crate::git::NARROW`) and what the owner's own jobs make
//!    there. A job that's untrusted or someone else's code when it's made keeps to a clone apart (`Job::apart`, `home/apart/clones`), as a review another Oby asks
//!    for does (`home/received/clones`). Anything else comes into the owner's clone only by obid's
//!    own fetch (a job's base, a pull request's head), and one
//!    that isn't a reviewed branch and whose commits aren't all in obid's record of the owner's
//!    jobs marks the clone (`crate::git::taint`), as a job there turning someone else's code does
//!    (`Jobs::mark_foreign`): from then on no check in that repository gets values. A clone made
//!    before clones were narrow gets none either. An agent can't fetch anything itself: it has no
//!    code host's login (an empty `GH_CONFIG_DIR`, an invalid push URL), and on macOS its only way
//!    off the Mac is obid's egress proxy, which lets its tool's own services through and decides
//!    every other site. A vendor host past its tool's API ([`crate::egress::tools_api`]) and any
//!    site but the default registries, which serve only published versions on an edge that
//!    answers only the TLS hello's host (`crate::egress::REGISTRIES`; never
//!    `crate::egress::ROUTED_BY_HOST`), mark the job ([`site_is_foreign`]): the proxy reads a
//!    connection's TLS name, not the `Host:` inside it. So do the channels that bring content past
//!    the proxy: a vendor's own web search or fetch and any MCP server's answer in the turn
//!    ([`tool_brings_content`]), a connected app's answer, and the context obid gives (Tetrix's,
//!    teammates' pull requests). A
//!    marked clone is made again for the next job once no job runs in it, the old one kept aside
//!    with the jobs that used it (`Jobs::remake_marked_clone`).
//! 3. **Linux gives no values** ([`platform_refusal`]): its agents still have a network of their
//!    own until oby#616, so they could fetch anyone's code themselves.
//! 4. **The base is reviewed**, and so is the default branch every clone holds: a branch the
//!    repository lets in only through a pull request with an approving review that a later push
//!    can't slip past (the last push approved, or stale reviews dismissed), with no one able to
//!    bypass it (no bypass actor but an empty break-glass team; classic protection held for
//!    admins) ([`protected_base`]).
//! 5. **What someone else wrote steers**: a job that works from an issue or pull request anyone but
//!    the owner wrote in (its body, a comment, a review) gets no values ([`others_wrote`]).
//! 6. **Defence in depth**: every commit between the base and the commit is in obid's own record of
//!    what the job made ([`on_record`], `Job::made`, after each turn's [`new_commits`], whose new
//!    commits may carry no change any other branch the clone has, other jobs' own included), and the job
//!    isn't another Oby's review, a follow-up of one that got none, or untrusted.
//!
//! Out of scope, and said so: a package anyone may publish to a registry the job installs from
//! (the owner's own tests would install the same); and an agent that writes out, as its own new
//! commit, foreign code from what its model already knows.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::Path;
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
    // `current_user_can_bypass` is the signed-in login's: there must be one.
    crate::github_login::read_status(gh)
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
            // Only one that needs someone's approval: a pull request with no review is a push.
            .filter(|rule| {
                let parameters = &rule["parameters"];
                // And one a push after the approval can't slip past.
                parameters["required_approving_review_count"]
                    .as_u64()
                    .is_some_and(|n| n >= 1)
                    && (parameters["require_last_push_approval"] == true
                        || parameters["dismiss_stale_reviews_on_push"] == true)
            })
            .filter_map(|rule| rule["ruleset_id"].as_u64())
            .collect();
        for id in sets {
            if let Some(set) = ask(format!("repos/{repo}/rulesets/{id}")).await
                && set["enforcement"] != "disabled"
                && set["enforcement"] != "evaluate"
                && set["current_user_can_bypass"] == "never"
                && no_one_bypasses(&ask, repo, &set).await
            {
                return Ok(());
            }
        }
    }
    // Classic protection: required reviews, held for admins (`enforce_admins`), with no bypass
    // allowance for anyone.
    if let Some(protection) = ask(format!("repos/{repo}/branches/{base}/protection")).await {
        let reviews = &protection["required_pull_request_reviews"];
        let allowances = &reviews["bypass_pull_request_allowances"];
        // Anyone allowed past it, a user, a team or an app, is someone whose push skips the review.
        let allowed = ["users", "teams", "apps"]
            .iter()
            .any(|kind| allowances[*kind].as_array().is_some_and(|a| !a.is_empty()));
        let reviewed = reviews["required_approving_review_count"]
            .as_u64()
            .is_some_and(|n| n >= 1)
            && (reviews["require_last_push_approval"] == true
                || reviews["dismiss_stale_reviews"] == true);
        // Held for admins too, whoever the login is.
        let held = protection["enforce_admins"]["enabled"] == true;
        if reviews.is_object() && reviewed && !allowed && held {
            return Ok(());
        }
    }
    Err(not)
}

/// Whether ruleset `set` lets no one past it: its `bypass_actors` (which GitHub shows obid's login
/// only when it may see them) is empty, or holds only teams with no members (a break-glass team
/// kept empty). Any other actor (a role, an app, a deploy key, a team with someone in it), or a
/// list obid can't read, is a no.
async fn no_one_bypasses<F, Fut>(ask: &F, repo: &str, set: &serde_json::Value) -> bool
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Option<serde_json::Value>>,
{
    let Some(actors) = set["bypass_actors"].as_array() else {
        return false;
    };
    if actors.is_empty() {
        return true;
    }
    let Some(org) = ask(format!("repos/{repo}"))
        .await
        .and_then(|r| r["owner"]["id"].as_u64())
    else {
        return false;
    };
    for actor in actors {
        let team = actor["actor_type"] == "Team";
        let Some(id) = actor["actor_id"].as_u64().filter(|_| team) else {
            return false;
        };
        let members = ask(format!("organizations/{org}/team/{id}/members")).await;
        if !members.is_some_and(|m| m.as_array().is_some_and(|m| m.is_empty())) {
            return false;
        }
    }
    true
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
    commits_between(worktree, env, from, "HEAD").await
}

/// The commits from `from` to `to`, newest first: none for none, an error past [`MOST_COMMITS`].
async fn commits_between(
    worktree: &Path,
    env: &[(&str, &str)],
    from: &str,
    to: &str,
) -> Result<Vec<String>, String> {
    let range = format!("{from}..{to}");
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
    own: &str,
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
    // Every other branch the clone has, other jobs' own included: not this job's own.
    let mine = format!("--exclude={own}");
    let mine_remote = format!("--exclude=origin/{own}");
    let elsewhere = patch_ids(
        worktree,
        env,
        &[&most, &mine, "--branches", &mine_remote, "--remotes", &not],
    )
    .await?;
    if !ours.is_disjoint(&elsewhere) {
        return Err(
            "its agent committed a change another branch already has (a cherry-pick or a copy of it)"
                .into(),
        );
    }
    Ok(commits)
}

/// Whether every commit from `base` to `head` (commits) is in `record`, obid's own record
/// of what the job (and its follow-up chain) made. Why not, otherwise.
pub async fn on_record(
    spec: &Spec,
    base: &str,
    head: &str,
    record: &HashSet<String>,
) -> Result<(), String> {
    let env = git_env(spec);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let commits = commits_between(&spec.worktree, &env, base, head)
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

/// The commit the job's worktree is at (its HEAD), read with the job's git environment.
pub async fn head_of(spec: &Spec) -> Result<String, String> {
    let env = git_env(spec);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let head = crate::git::run(
        &spec.worktree,
        &[
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new("HEAD^{commit}"),
        ],
        &env,
    )
    .await
    .map_err(|_| "obid couldn't read the commit its worktree is at".to_owned())?;
    if crate::git::is_sha(&head) {
        Ok(head)
    } else {
        Err("obid couldn't read the commit its worktree is at".into())
    }
}

/// Why this machine gives no check the owner's values, or none: on Linux a job's agent still has
/// a network of its own until oby#616, so it could fetch anyone's code itself (oby-plan E1: macOS
/// only until then).
pub fn platform_refusal() -> Option<String> {
    (!cfg!(target_os = "macos")).then(|| {
        "on Linux a job's agent still has a network of its own until oby#616, so it could fetch anyone's code itself".into()
    })
}

/// Whether an agent reaching `host` (not its tool's own: those never come here) makes its job
/// someone else's code: anything but the default registries ([`crate::egress::REGISTRIES`]),
/// which serve only published, immutable versions on an edge that answers only the host the TLS
/// hello named. A code host, a pull request's page, a site with code on it, and a registry whose
/// edge serves whichever of its sites the HTTP `Host` names ([`crate::egress::ROUTED_BY_HOST`]:
/// Fastly's, the Go module proxy) all could carry anyone's code: obid's proxy reads the hello's
/// name, not the `Host` inside the tunnel, and an owner's card or a rule can still let such a host
/// through.
pub fn site_is_foreign(host: &str) -> bool {
    crate::egress::ROUTED_BY_HOST.contains(&host) || !crate::egress::REGISTRIES.contains(&host)
}

/// Whether a tool call named `name` is of a kind obid knows keeps the job the owner's own: a
/// file read, write, edit or search inside the worktree, a command inside the job's sandbox (what
/// it reaches past the Mac goes through obid's proxy, which decides and marks), its own to-do
/// list, a plan's end, or obid's own question to the owner. Fail closed: anything else (a vendor's
/// web search or fetch, any MCP server's tool or resource, a helper agent whose own calls obid
/// may not see, a kind an agent adds later or obid doesn't parse) may bring it someone else's
/// words.
pub fn tool_is_known_safe(name: &str) -> bool {
    let ask_owner = format!("mcp__oby__{}", crate::permissions::ASK_OWNER);
    SAFE_TOOLS.contains(&name) || name == ask_owner
}

/// The tools [`tool_is_known_safe`] knows: Claude Code's own names, which the Codex and Cursor
/// adapters name theirs after (crate::codex, `tool_call`; crate::acp, `named`).
const SAFE_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "NotebookRead",
    "Glob",
    "Grep",
    "LS",
    "Bash",
    "BashOutput",
    "KillShell",
    "KillBash",
    "TodoWrite",
    "TodoRead",
    "ExitPlanMode",
    "StructuredOutput",
    "AskUserQuestion",
];

/// Why issue or pull request `number` of `repo` (`kind`, `issue` or `pr`) isn't only the owner's
/// own words, or none: by the threat model, what anyone else wrote there (its body, its title, an
/// edit, a comment, a review or a review's comment) is content that may steer the agent, whatever
/// they may do elsewhere (an organisation member's words count as the owner's for other
/// purposes, `crate::github_work::Owner::trusts`). Read as `crate::github_work` reads who wrote an
/// issue, every list whole; what GitHub can't say is a no. What obid posted under the owner's login
/// is signed, an Oby's, never the owner's: but a pull request's own description when
/// `signed_body`, obid's record that it wrote it only for jobs that are the owner's own throughout
/// (`Jobs::described_clean`). With `shown`, what obid quoted of it, that counts too
/// (`crate::github_work::owners_only`).
pub async fn others_wrote(
    gh: &Path,
    repo: &str,
    kind: &str,
    number: u64,
    signed_body: bool,
    shown: Option<&serde_json::Value>,
) -> Option<String> {
    let name = if kind == "pr" {
        "pull request"
    } else {
        "issue"
    };
    (!crate::github_work::owners_only(gh, repo, number, signed_body, shown).await)
        .then(|| format!("it works from {name} #{number}, which someone else wrote in or GitHub didn't say who wrote"))
}

/// What obid's git reads of the person's own settings while it handles a job's commit outside any
/// box: nothing (no `filter.lfs`, no `url.*.insteadOf`), so the commit's `.gitattributes` and
/// `.lfsconfig` reach no host of the agent's choosing.
const NO_SETTINGS: [(&str, &str); 2] = [
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
];

/// A checkout of commit `head` that obid makes for the check, apart from the job's: its worktree
/// at `at/work`, its own git folder in the clone's `worktrees/`, its own temporary folder at
/// `at/tmp`, none of them anywhere the job's sandbox can write. What the job's commits brought
/// since `base` (a commit) is copied out of the job's own object folder into the checkout's, so
/// nothing of the job's folders is read again. `agent` is the job's sandbox spec; the answer is the
/// check's. What the job's worktree holds besides its commits (files left uncommitted, hidden by a
/// `.gitignore`, rewritten after the gate) isn't in it. When an earlier check left one at `at`
/// (a job's own, kept across its checks), it's reset to `head` and everything that check left is
/// removed, ignored files too and whatever `.gitignore` it wrote, but for the folders in `keep`
/// (the repository's `checkKeep`: its builds), which can still carry what an earlier check's code
/// wrote there. obid's git here reads none of the person's own git settings.
pub async fn fresh_checkout(
    agent: &Spec,
    base: &str,
    head: &str,
    at: &Path,
    keep: &[String],
) -> Result<Spec, String> {
    let common = agent
        .git_common
        .clone()
        .ok_or("the job's worktree has no clone obid knows")?;
    let clone = common
        .parent()
        .ok_or("the job's clone has no folder")?
        .to_path_buf();
    let fail = |what: &str| format!("obid couldn't make the check's own checkout ({what})");
    let name = format!(
        "check-{}",
        at.file_name().unwrap_or_default().to_string_lossy()
    );
    // git names the checkout's git folder after its worktree's folder: one of obid's own.
    let work = at.join(&name);
    let admin = common.join("worktrees").join(&name);
    // One an earlier check of the job's left (a per-job `at`) is used again, its builds kept.
    let again = work.join(".git").is_file() && admin.join("objects/pack").is_dir();
    let temp = at.join("tmp");
    let objects = if again {
        admin.join("objects")
    } else {
        let _ = std::fs::remove_dir_all(at);
        at.join("objects")
    };
    std::fs::create_dir_all(objects.join("pack")).map_err(|_| fail("its folder"))?;
    std::fs::create_dir_all(&temp).map_err(|_| fail("its folder"))?;
    // The job's commits since the base, out of its own object folder, as one pack: git pinned to
    // the job's git folders as obid checked them, never the `.git` its agent could rewrite.
    let agent_admin = agent
        .git_admin
        .clone()
        .ok_or("the job's worktree has no git folder obid knows")?;
    let mut env = git_env(agent);
    env.extend(pinned_to(&agent_admin, &common, &agent.worktree));
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let pack = objects
        .join(format!("pack/pack-{}", head.get(..12).unwrap_or(head)))
        .to_string_lossy()
        .into_owned();
    crate::git::run_bytes(
        &agent.worktree,
        &[
            OsStr::new("pack-objects"),
            OsStr::new("--revs"),
            OsStr::new("-q"),
            OsStr::new(&pack),
        ],
        &env,
        Some(format!("{head}\n^{base}\n").into_bytes()),
    )
    .await
    .map_err(|_| fail("copying its commits"))?;
    if !again {
        let objects_text = objects.to_string_lossy().into_owned();
        crate::git::run(
            &clone,
            &[
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("--no-checkout"),
                OsStr::new("--detach"),
                work.as_os_str(),
                OsStr::new(head),
            ],
            &[
                ("GIT_ALTERNATE_OBJECT_DIRECTORIES", objects_text.as_str()),
                NO_SETTINGS[0],
                NO_SETTINGS[1],
            ],
        )
        .await
        .map_err(|_| fail("its worktree"))?;
        if !admin.is_dir() {
            return Err(fail("its git folder"));
        }
        std::fs::rename(&objects, admin.join("objects")).map_err(|_| fail("its objects"))?;
    }
    let check = Spec {
        worktree: work,
        git_admin: Some(admin.clone()),
        git_common: Some(common.clone()),
        commits: true,
        temp,
        accounts: Vec::new(),
        frozen: Vec::new(),
        fixed: Vec::new(),
        ..agent.clone()
    };
    let mut env = git_env(&check);
    env.extend(pinned_to(&admin, &common, &check.worktree));
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    crate::git::run(
        &check.worktree,
        &[
            OsStr::new("reset"),
            OsStr::new("-q"),
            OsStr::new("--hard"),
            OsStr::new(head),
        ],
        &env,
    )
    .await
    .map_err(|_| fail("its files"))?;
    // Everything the last check left goes, whatever it ignored, but for the folders the
    // repository keeps for its checks' builds (`-x` reads no `.gitignore`, only these `-e`s).
    // The agent's own files were never here; what the last check's code (the agent's) wrote can
    // be, in those folders only.
    if again {
        let kept: Vec<String> = keep
            .iter()
            .filter_map(|k| crate::merge::repo_path(k))
            .map(|k| format!("/{k}/"))
            .collect();
        let mut args: Vec<&OsStr> = ["clean", "-q", "-ffdx"].map(OsStr::new).into();
        for k in &kept {
            args.extend([OsStr::new("-e"), OsStr::new(k)]);
        }
        crate::git::run(&check.worktree, &args, &env)
            .await
            .map_err(|_| fail("its files"))?;
    }
    Ok(check)
}

/// git's environment for a worktree obid checked (its git folder `admin`, its clone's `common`),
/// with none of the person's own settings ([`NO_SETTINGS`]).
fn pinned_to(admin: &Path, common: &Path, worktree: &Path) -> Vec<(&'static str, String)> {
    let mut env: Vec<(&'static str, String)> = NO_SETTINGS.map(|(k, v)| (k, v.to_owned())).into();
    env.extend([
        ("GIT_DIR", admin.to_string_lossy().into_owned()),
        ("GIT_COMMON_DIR", common.to_string_lossy().into_owned()),
        ("GIT_WORK_TREE", worktree.to_string_lossy().into_owned()),
    ]);
    env
}

/// Removes a checkout [`fresh_checkout`] made, and its folder `at`: what the check wrote goes with
/// it. Why it couldn't, otherwise.
pub async fn remove_checkout(check: &Spec, at: &Path) -> Result<(), String> {
    if let Some(common) = &check.git_common
        && let Some(clone) = common.parent()
    {
        let _ = crate::git::run(
            clone,
            &[
                OsStr::new("worktree"),
                OsStr::new("remove"),
                OsStr::new("--force"),
                check.worktree.as_os_str(),
            ],
            &[],
        )
        .await;
        if let Some(admin) = &check.git_admin {
            let _ = std::fs::remove_dir_all(admin);
        }
    }
    let _ = std::fs::remove_dir_all(at);
    match at.exists() {
        false => Ok(()),
        true => Err(format!(
            "obid couldn't remove the check's own checkout ({})",
            at.display()
        )),
    }
}

/// Puts commit `base`'s own copy of each of `paths` (as crate::merge::check_scripts gives them) into the
/// check's checkout `work`, from obid's `clone` (PUBLISHING-6): what the job's commit has there
/// is replaced, and a path that isn't a file on the base is taken out. Answers the paths the base
/// has. A folder on the way that the commit made a link or a file is made a real one, so nothing
/// is ever written outside `work`.
pub async fn from_base(
    clone: &Path,
    base: &str,
    work: &Path,
    paths: &[String],
) -> Result<Vec<String>, String> {
    let mut had = Vec::new();
    for path in paths {
        let fail = || format!("obid couldn't read {path} as its base has it");
        let listed = crate::git::git(clone, &["ls-tree", "-z", base, "--", path])
            .await
            .map_err(|_| fail())?;
        // `<mode> blob <sha>\t<path>`, for a file; nothing, or anything else, for none.
        let file = listed
            .split_once('\t')
            .map(|(meta, _)| meta.split(' ').collect::<Vec<_>>())
            .filter(|meta| meta.len() == 3 && meta[1] == "blob" && meta[0] != "120000");
        let copy = match file {
            Some(meta) => {
                let bytes = crate::git::run_bytes(
                    clone,
                    &[
                        OsStr::new("cat-file"),
                        OsStr::new("blob"),
                        OsStr::new(meta[2]),
                    ],
                    &[],
                    None,
                )
                .await
                .map_err(|_| fail())?;
                had.push(path.clone());
                Some((bytes, meta[0] == "100755"))
            }
            None => None,
        };
        put(work, path, copy)
            .map_err(|e| format!("obid couldn't put {path} as its base has it: {e}"))?;
    }
    Ok(had)
}

/// Writes `copy` (its bytes, and whether it runs) at `path` in `work`, or takes `path` out for
/// none, through real folders only.
fn put(work: &Path, path: &str, copy: Option<(Vec<u8>, bool)>) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut at = work.to_path_buf();
    let parts: Vec<&str> = path.split('/').collect();
    let (name, folders) = parts.split_last().expect("a path has a name");
    for folder in folders {
        at.push(folder);
        match at.symlink_metadata() {
            Ok(meta) if meta.is_dir() => {}
            // Not a folder in the commit: the path isn't there to take out.
            _ if copy.is_none() => return Ok(()),
            Ok(_) => {
                std::fs::remove_file(&at)?;
                std::fs::create_dir(&at)?;
            }
            Err(_) => std::fs::create_dir(&at)?,
        }
    }
    at.push(name);
    match at.symlink_metadata() {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&at)?,
        Ok(_) => std::fs::remove_file(&at)?,
        Err(_) => {}
    }
    if let Some((bytes, runs)) = copy {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(if runs { 0o755 } else { 0o644 })
            .open(&at)?
            .write_all(&bytes)?;
    }
    Ok(())
}

/// What a check gets of the secrets its repository declares: the values to put in its environment,
/// what takes them out of everything it wrote, and obid's notes on it (names only).
#[derive(Default)]
pub struct Filled {
    env: Vec<(String, String)>,
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
                    let (json, json_slash) = json_forms(&value);
                    let unicode = unicode_forms(&value);
                    let mut learned: Vec<&[u8]> =
                        vec![value.as_bytes(), json.as_bytes(), json_slash.as_bytes()];
                    learned.extend(unicode.iter().map(|form| form.as_bytes()));
                    filled.secrets.learn_forms(name, &learned);
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
        if self.env.is_empty() {
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
}

/// The job's git environment as obid's own git calls take it.
fn git_env(spec: &Spec) -> Vec<(&'static str, String)> {
    spec.git_env()
        .into_iter()
        .map(|(k, v)| (k, v.to_string_lossy().into_owned()))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    /// JSON's escaped forms of a value are taken out of what a check prints too: `\/`, and
    /// `\uXXXX` for every character or every one but letters and digits, in either case.
    #[test]
    fn json_escaped_forms_are_taken_out() {
        let filled = Filled::fill(&["TOKEN".into()], |_| Ok(Some(FAKE.to_owned())));
        let every: String = FAKE
            .chars()
            .map(|c| format!("\\u{:04x}", c as u32))
            .collect();
        let upper: String = FAKE
            .chars()
            .map(|c| format!("\\u{:04X}", c as u32))
            .collect();
        let some: String = FAKE
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_string()
                } else {
                    format!("\\u{:04x}", c as u32)
                }
            })
            .collect();
        for form in [FAKE.replace('/', "\\/"), every, upper, some] {
            let clean = filled.redact(&format!("{{\"t\": \"{form}\"}}"));
            assert!(clean.contains("[check secret: TOKEN]"), "{form}: {clean}");
            assert!(!clean.contains("Z9q2"), "{form}: {clean}");
        }
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
                    r#"{"default_branch":"main","permissions":{"admin":true},"owner":{"id":42}}"#,
                ),
                (
                    "repos/o/r/rules/branches/glass",
                    r#"[{"type":"pull_request","ruleset_id":5,"parameters":{"required_approving_review_count":1,"require_last_push_approval":true}}]"#,
                ),
                (
                    "repos/o/r/rulesets/5",
                    r#"{"id":5,"enforcement":"active","current_user_can_bypass":"never","bypass_actors":[{"actor_type":"Team","actor_id":7,"bypass_mode":"always"}]}"#,
                ),
                ("organizations/42/team/7/members", "[]"),
                (
                    "repos/o/r/rules/branches/staffed",
                    r#"[{"type":"pull_request","ruleset_id":6,"parameters":{"required_approving_review_count":1,"require_last_push_approval":true}}]"#,
                ),
                (
                    "repos/o/r/rulesets/6",
                    r#"{"id":6,"enforcement":"active","current_user_can_bypass":"never","bypass_actors":[{"actor_type":"Team","actor_id":8,"bypass_mode":"always"}]}"#,
                ),
                (
                    "organizations/42/team/8/members",
                    r#"[{"login":"ernesto"}]"#,
                ),
                (
                    "repos/o/r/rules/branches/role",
                    r#"[{"type":"pull_request","ruleset_id":9,"parameters":{"required_approving_review_count":1,"require_last_push_approval":true}}]"#,
                ),
                (
                    "repos/o/r/rulesets/9",
                    r#"{"id":9,"enforcement":"active","current_user_can_bypass":"never","bypass_actors":[{"actor_type":"RepositoryRole","actor_id":5,"bypass_mode":"always"}]}"#,
                ),
                (
                    "repos/o/r/rules/branches/unseen",
                    r#"[{"type":"pull_request","ruleset_id":10,"parameters":{"required_approving_review_count":1,"require_last_push_approval":true}}]"#,
                ),
                (
                    "repos/o/r/rulesets/10",
                    r#"{"id":10,"enforcement":"active","current_user_can_bypass":"never"}"#,
                ),
                (
                    "repos/o/r/rules/branches/main",
                    r#"[{"type":"pull_request","ruleset_id":1,"parameters":{"required_approving_review_count":1,"require_last_push_approval":true}}]"#,
                ),
                (
                    "repos/o/r/rulesets/1",
                    r#"{"id":1,"enforcement":"active","current_user_can_bypass":"never","bypass_actors":[]}"#,
                ),
                (
                    "repos/o/r/rules/branches/bypassed",
                    r#"[{"type":"pull_request","ruleset_id":2,"parameters":{"required_approving_review_count":1}}]"#,
                ),
                (
                    "repos/o/r/rulesets/2",
                    r#"{"id":2,"enforcement":"active","current_user_can_bypass":"always","bypass_actors":[]}"#,
                ),
                (
                    "repos/o/r/rules/branches/evaluated",
                    r#"[{"type":"pull_request","ruleset_id":3,"parameters":{"required_approving_review_count":1}}]"#,
                ),
                (
                    "repos/o/r/rulesets/3",
                    r#"{"id":3,"enforcement":"evaluate","current_user_can_bypass":"never","bypass_actors":[]}"#,
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
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":1,"dismiss_stale_reviews":true},"enforce_admins":{"enabled":true}}"#,
                ),
                (
                    "repos/o/r/branches/stale-ok/protection",
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":1},"enforce_admins":{"enabled":true}}"#,
                ),
                (
                    "repos/o/r/rules/branches/push-after",
                    r#"[{"type":"pull_request","ruleset_id":1,"parameters":{"required_approving_review_count":1}}]"#,
                ),
                (
                    "repos/o/r/branches/admins-skip/protection",
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":1},"enforce_admins":{"enabled":false}}"#,
                ),
                (
                    "repos/o/r/branches/allowed/protection",
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":1,"bypass_pull_request_allowances":{"users":[{"login":"Wellington"}]}},"enforce_admins":{"enabled":true}}"#,
                ),
                (
                    "repos/o/r/branches/someone-allowed/protection",
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":1,"dismiss_stale_reviews":true,"bypass_pull_request_allowances":{"users":[{"login":"ernesto"}]}},"enforce_admins":{"enabled":true}}"#,
                ),
                (
                    "repos/o/r/branches/team-allowed/protection",
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":1,"bypass_pull_request_allowances":{"teams":[{"slug":"core"}]}},"enforce_admins":{"enabled":true}}"#,
                ),
                (
                    "repos/o/r/branches/app-allowed/protection",
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":1,"bypass_pull_request_allowances":{"apps":[{"slug":"a-bot"}]}},"enforce_admins":{"enabled":true}}"#,
                ),
                (
                    "repos/o/r/branches/no-approvals/protection",
                    r#"{"required_pull_request_reviews":{"required_approving_review_count":0},"enforce_admins":{"enabled":true}}"#,
                ),
                (
                    "repos/o/r/rules/branches/unreviewed",
                    r#"[{"type":"pull_request","ruleset_id":1,"parameters":{"required_approving_review_count":0}}]"#,
                ),
                (
                    "repos/o/r/branches/no-reviews/protection",
                    r#"{"required_status_checks":{"strict":true},"enforce_admins":{"enabled":true}}"#,
                ),
            ],
        );
        assert_eq!(protected_base(&gh, "o/r", "main").await, Ok(()));
        assert_eq!(protected_base(&gh, "o/r", "release").await, Ok(()));
        // A break-glass team kept empty doesn't let anyone past.
        assert_eq!(protected_base(&gh, "o/r", "glass").await, Ok(()));
        for weak in [
            "bypassed",
            "evaluated",
            "weak",
            "admins-skip",
            "allowed",
            "someone-allowed",
            "team-allowed",
            "app-allowed",
            "no-approvals",
            "unreviewed",
            "stale-ok",
            "push-after",
            "staffed",
            "role",
            "unseen",
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
        // Another job's own branch, kept in the clone as obid keeps a job's commits.
        sh(
            &clone,
            &format!(
                "git worktree add -q -b oby/job1 ../other origin/main && cd ../other && echo payload > payload.txt && git add payload.txt && {COMMIT} -m payload"
            ),
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
            new_commits(&spec.worktree, &env, &before, started, base_ref, "oby/me").await
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
            (
                "another-job",
                "git checkout oby/job1 -- payload.txt && git -c user.name=T -c user.email=t@example.com -c commit.gpgsign=false commit -q -m mine",
                "another branch already has",
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
        assert_eq!(on_record(&own, &base, "HEAD", &record).await, Ok(()));
        assert!(
            on_record(&own, &base, "HEAD", &HashSet::new())
                .await
                .is_err()
        );

        // A reused branch name: its old commits aren't in the record, and rebased onto the base
        // as it is now they are new commits obid never recorded either.
        let reused = job_worktree(&dir, "reused", "origin/oby/fix-12");
        assert!(
            on_record(&reused, &base, "HEAD", &record)
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
        assert!(on_record(&reused, &now, "HEAD", &record).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// obid's clones hold only their default branch, and say when obid fetched someone else's
    /// commits into them: a teammate's branch on the remote isn't in a new clone at all (its ref
    /// nor its commits), a clone made before is someone else's, and a taint stays.
    #[tokio::test]
    async fn a_clone_holds_only_what_obid_knows_is_the_owners() {
        let dir =
            std::env::temp_dir().join(format!("obid-secretspec-clone-{}", std::process::id()));
        let (wide, _base) = record_fixture(&dir).await;
        assert!(
            crate::git::foreign_in(&wide)
                .await
                .unwrap()
                .contains("made before")
        );
        let remote = format!("file://{}", dir.join("origin").display());
        let home = dir.join("home");
        let clone = crate::git::ensure_clone(&home, "o/r", &remote, &[])
            .await
            .unwrap();
        assert_eq!(crate::git::foreign_in(&clone).await, None);
        let teammate = crate::git::git(&dir.join("origin"), &["rev-parse", "refs/heads/teammate"])
            .await
            .unwrap();
        assert!(
            crate::git::git(
                &clone,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "refs/remotes/origin/teammate"
                ]
            )
            .await
            .is_err()
        );
        assert!(
            crate::git::git(&clone, &["cat-file", "-e", &teammate])
                .await
                .is_err()
        );
        crate::git::taint(&clone, "pull request #9's head").await;
        crate::git::taint(&clone, "a later one").await;
        let why = crate::git::foreign_in(&clone).await.unwrap();
        assert!(
            why.contains("pull request #9's head") && !why.contains("later"),
            "{why}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The check's own checkout is HEAD's commits and nothing else of the job's: not a file left
    /// uncommitted, not one a `.gitignore` hides, not what a process the turn left behind writes
    /// into the job's worktree after the gate; and it's gone after.
    #[tokio::test]
    async fn the_checks_own_checkout_is_only_its_commits() {
        let dir =
            std::env::temp_dir().join(format!("obid-secretspec-fresh-{}", std::process::id()));
        let (_clone, base) = record_fixture(&dir).await;
        let job = job_worktree(&dir, "job", "origin/main");
        as_job(
            &job,
            &format!(
                "echo hidden.txt > .gitignore && echo 'echo committed' > run.sh && git add .gitignore run.sh && {COMMIT} -m mine && \
                 echo SECRET-HIDDEN > hidden.txt && echo LEFT > left.txt"
            ),
        );
        let head = head_of(&job).await.unwrap();
        let at = dir.join("checks/job-1");
        let check = fresh_checkout(&job, &base, &head, &at, &[]).await.unwrap();
        // After the gate: what a survivor would write into the job's worktree.
        std::fs::write(job.worktree.join("run.sh"), "echo REWRITTEN\n").unwrap();
        assert!(!check.worktree.starts_with(&job.worktree) && !check.temp.starts_with(&job.temp));
        assert_eq!(
            std::fs::read_to_string(check.worktree.join("run.sh")).unwrap(),
            "echo committed\n"
        );
        assert!(!check.worktree.join("hidden.txt").exists());
        assert!(!check.worktree.join("left.txt").exists());
        assert_eq!(head_of(&check).await.unwrap(), head);
        // Its git reads none of the job's own objects.
        assert!(
            !std::fs::read_to_string(
                check
                    .git_admin
                    .as_ref()
                    .unwrap()
                    .join("objects/info/alternates")
            )
            .is_ok_and(|a| a.contains("job"))
        );
        remove_checkout(&check, &at).await.unwrap();
        assert!(!at.exists() && !check.git_admin.unwrap().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only macOS gives checks values, until oby#616 takes a Linux agent's own network away.
    #[test]
    fn linux_gives_no_values() {
        assert_eq!(platform_refusal().is_some(), !cfg!(target_os = "macos"));
        if let Some(why) = platform_refusal() {
            assert!(why.contains("oby#616"), "{why}");
        }
    }

    /// Reaching a code host or any site but the package registries makes the job someone else's.
    #[test]
    fn a_site_beyond_the_registries_is_foreign() {
        for host in [
            "github.com",
            "codeload.github.com",
            "gitlab.com",
            "example.com",
            "proxy.golang.org",
            "sum.golang.org",
            // Fastly's, which serve their edge's other sites for whatever `Host:` asks.
            "pypi.org",
            "files.pythonhosted.org",
            "crates.io",
            "index.crates.io",
            "static.crates.io",
            "static.rust-lang.org",
            "rubygems.org",
            "index.rubygems.org",
        ] {
            assert!(site_is_foreign(host), "{host}");
        }
        for host in [
            "registry.npmjs.org",
            "registry.yarnpkg.com",
            "repo.yarnpkg.com",
            "repo.maven.apache.org",
            "repo1.maven.org",
            "plugins.gradle.org",
            "services.gradle.org",
            "cdn.cocoapods.org",
        ] {
            assert!(!site_is_foreign(host), "{host}");
        }
    }

    /// Only the tools obid knows are safe keep a job the owner's: a vendor's search or fetch, any
    /// MCP server's tool or resource, a helper, a kind obid doesn't know don't.
    #[test]
    fn only_known_tools_are_safe() {
        for name in [
            "WebSearch",
            "WebFetch",
            "mcp__tetrix__search_entities",
            "mcp__oby__linear_search",
            "mcp__a-plugin__lookup",
        ] {
            assert!(!tool_is_known_safe(name), "{name}");
        }
        for name in [
            "Agent",
            "Task",
            "Tool",
            "ListMcpResourcesTool",
            "ReadMcpResourceTool",
            "mcp__codex__read_mcp_resource",
            "server_tool_use",
            "SomeToolAddedLater",
        ] {
            assert!(!tool_is_known_safe(name), "{name}");
        }
        for name in [
            "mcp__oby__oby_ask_owner",
            "Bash",
            "Read",
            "Edit",
            "Grep",
            "TodoWrite",
        ] {
            assert!(tool_is_known_safe(name), "{name}");
        }
    }

    /// What anyone but the owner wrote in the issue or pull request a job works from (its body, its
    /// title, an edit, a comment, a review or its comment, past the first page too) may steer it:
    /// no values. The owner's own, every list whole: values.
    #[tokio::test]
    async fn someone_elses_issue_text_gives_nothing() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = std::env::temp_dir().join(format!("obid-secretspec-text-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let whole =
            |nodes: &str| format!(r#"{{"pageInfo":{{"hasNextPage":false}},"nodes":[{nodes}]}}"#);
        let me = r#"{"login":"wellington"}"#;
        let written = |who: &str| format!(r#"{{"author":{who},"userContentEdits":{}}}"#, whole(""));
        let item = |author: &str,
                    edits: &str,
                    renames: &str,
                    comments: &str,
                    reviews: Option<&str>| {
            let reviews = reviews
                .map(|r| format!(r#","reviews":{r}"#))
                .unwrap_or_default();
            format!(
                r#"{{"data":{{"viewer":{me},"repository":{{"issueOrPullRequest":{{"author":{author},"userContentEdits":{edits},"timelineItems":{renames},"comments":{comments}{reviews}}}}}}}}}"#
            )
        };
        let other = r#"{"login":"a-member"}"#;
        let answers = [
            // 1: the owner's own, every list whole.
            item(me, &whole(""), &whole(""), &whole(&written(me)), None),
            // 2: a comment someone else wrote.
            item(me, &whole(""), &whole(""), &whole(&written(other)), None),
            // 3: the body edited by someone else.
            item(
                me,
                &whole(&format!(r#"{{"editor":{other}}}"#)),
                &whole(""),
                &whole(""),
                None,
            ),
            // 4: the title renamed by someone else.
            item(
                me,
                &whole(""),
                &whole(&format!(r#"{{"actor":{other}}}"#)),
                &whole(""),
                None,
            ),
            // 5: more comments than GitHub gave.
            item(
                me,
                &whole(""),
                &whole(""),
                r#"{"pageInfo":{"hasNextPage":true},"nodes":[]}"#,
                None,
            ),
            // 6: a pull request with a review someone else wrote.
            item(
                me,
                &whole(""),
                &whole(""),
                &whole(""),
                Some(&whole(&format!(
                    r#"{{"author":{other},"userContentEdits":{},"comments":{}}}"#,
                    whole(""),
                    whole("")
                ))),
            ),
            // 7: a pull request whose own review holds someone else's comment.
            item(
                me,
                &whole(""),
                &whole(""),
                &whole(""),
                Some(&whole(&format!(
                    r#"{{"author":{me},"userContentEdits":{},"comments":{}}}"#,
                    whole(""),
                    whole(&written(other))
                ))),
            ),
            // 8: a pull request that's the owner's own throughout.
            item(
                me,
                &whole(""),
                &whole(""),
                &whole(""),
                Some(&whole(&format!(
                    r#"{{"author":{me},"userContentEdits":{},"comments":{}}}"#,
                    whole(""),
                    whole(&written(me))
                ))),
            ),
            // 9: none (GitHub said nothing).
            String::new(),
            // 10: a pull request whose description obid signed, under the owner's login.
            format!(
                r#"{{"data":{{"viewer":{me},"repository":{{"issueOrPullRequest":{{"author":{me},"body":"Fixes it.\n\nby Oby: Oby opened this pull request.","userContentEdits":{e},"timelineItems":{e},"comments":{e},"reviews":{e}}}}}}}}}"#,
                e = whole("")
            ),
            // 11: a comment obid signed, under the owner's login.
            item(
                me,
                &whole(""),
                &whole(""),
                &whole(&format!(
                    r#"{{"author":{me},"body":"Ran it. by Oby: it passed.","userContentEdits":{}}}"#,
                    whole("")
                )),
                None,
            ),
        ];
        let mut script = String::from("#!/bin/sh\ncase \"$*\" in\n");
        for (i, answer) in answers.iter().enumerate() {
            let file = dir.join(format!("{}.json", i + 1));
            std::fs::write(&file, answer).unwrap();
            script.push_str(&format!(
                "  *number={}) cat '{}' ;;\n",
                i + 1,
                file.display()
            ));
        }
        script.push_str("  *) exit 1 ;;\nesac\n");
        let gh = dir.join("gh");
        std::fs::write(&gh, script).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            others_wrote(&gh, "o/r", "issue", 1, false, None).await,
            None
        );
        assert_eq!(others_wrote(&gh, "o/r", "pr", 8, false, None).await, None);
        // What obid signed is an Oby's: a description only when obid wrote it for clean jobs alone,
        // a comment never.
        assert!(
            others_wrote(&gh, "o/r", "pr", 10, false, None)
                .await
                .is_some()
        );
        assert_eq!(others_wrote(&gh, "o/r", "pr", 10, true, None).await, None);
        assert!(
            others_wrote(&gh, "o/r", "issue", 11, true, None)
                .await
                .is_some()
        );
        for n in [2, 3, 4, 5, 6, 7, 9] {
            assert!(
                others_wrote(&gh, "o/r", "issue", n, false, None)
                    .await
                    .is_some(),
                "#{n}"
            );
        }
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
