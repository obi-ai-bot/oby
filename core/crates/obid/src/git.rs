//! obid's own git: its clones (`home/clones/<owner>/<name>`, whose push URL is invalid, so nothing
//! an agent runs can push) and the jobs' worktrees (`home/worktrees/<job id>`). This git runs
//! outside any sandbox, so it never runs a hook or an fsmonitor, whatever a worktree or another
//! Obi's branch holds, and it never asks for a password.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

/// The push URL every clone gets: git refuses it, so no push leaves from a clone (AGENTS.md,
/// "Agents can't publish").
pub const NO_PUSH_URL: &str = "obid-agents-cannot-push:///";

/// How long one git command may take before obid gives up on it.
const GIT_TIMEOUT: Duration = Duration::from_secs(600);

/// How a git call is cut.
#[derive(Clone, Copy)]
enum Limit {
    /// After [`GIT_TIMEOUT`], whatever it does.
    Fixed,
    /// After `quiet` with nothing new on stderr, or after `ceiling` in all: a clone or fetch that
    /// keeps printing `--progress` is never cut for its size (RECOVERY-13).
    Stall { quiet: Duration, ceiling: Duration },
}

/// A clone's or fetch's [`Limit`]: 5 minutes with no progress, 6 hours in all.
const TRANSFER: Limit = Limit::Stall {
    quiet: Duration::from_secs(300),
    ceiling: Duration::from_secs(6 * 3600),
};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct GitError(pub String);

impl GitError {
    /// What git itself said, without what it quotes ('a URL', 'a path') or the command it ran: the
    /// repository's name and address can hold any word, `timeout` too (RECOVERY-14).
    pub fn said(&self) -> String {
        let said = self.0.split_once(" failed: ").map_or("", |(_, said)| said);
        said.split('\'').step_by(2).collect()
    }
}

/// The settings every git call of obid's carries.
const SAFE: &[&str] = &[
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "credential.interactive=never",
    "-c",
    "protocol.ext.allow=never",
];

/// Runs git with `args` in `dir`; its stdout, trimmed, or why it failed.
pub async fn run(dir: &Path, args: &[&OsStr], extra: &[(&str, &str)]) -> Result<String, GitError> {
    let output = run_bytes(dir, args, extra, None).await?;
    Ok(String::from_utf8_lossy(&output).trim().to_owned())
}

/// [`run`] with `input` on git's stdin (none: `/dev/null`), answering its stdout as it is.
pub async fn run_bytes(
    dir: &Path,
    args: &[&OsStr],
    extra: &[(&str, &str)],
    input: Option<Vec<u8>>,
) -> Result<Vec<u8>, GitError> {
    // Boxed, as is every big future here: a debug obid's poll frames hold their children's whole
    // futures, near tokio's 2 MiB worker stack (jobs.rs).
    Box::pin(run_limited(dir, args, extra, input, Limit::Fixed)).await
}

/// A clone or fetch: [`run`] under [`TRANSFER`], with git's `--progress` on stderr.
async fn transfer(dir: &Path, args: &[&OsStr]) -> Result<String, GitError> {
    let output = Box::pin(run_limited(dir, args, &[], None, TRANSFER)).await?;
    Ok(String::from_utf8_lossy(&output).trim().to_owned())
}

async fn run_limited(
    dir: &Path,
    args: &[&OsStr],
    extra: &[(&str, &str)],
    input: Option<Vec<u8>>,
    limit: Limit,
) -> Result<Vec<u8>, GitError> {
    use tokio::io::AsyncWriteExt;
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(SAFE)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .stdin(if input.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    for (key, value) in extra {
        command.env(key, value);
    }
    let shown = args
        .iter()
        .map(|a| a.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let mut child = command
        .spawn()
        .map_err(|e| GitError(format!("git can't run: {e}")))?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        // Written alongside the read, so a large input and a large output never wait on each other.
        tokio::spawn(async move {
            let _ = stdin.write_all(&input).await;
        });
    }
    let output = match limit {
        Limit::Fixed => tokio::time::timeout(GIT_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| {
                GitError(format!(
                    "git {shown} took more than {} seconds",
                    GIT_TIMEOUT.as_secs()
                ))
            })?,
        Limit::Stall { quiet, ceiling } => {
            Box::pin(watch(child, &format!("git {shown}"), quiet, ceiling)).await?
        }
    }
    .map_err(|e| GitError(format!("git can't run: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // `--progress` redraws its line with `\r`: git's last words are after the last of either.
        let line = stderr
            .split(['\n', '\r'])
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_owned();
        return Err(GitError(format!("git {shown} failed: {line}")));
    }
    Ok(output.stdout)
}

/// Waits for `child`, whose stderr is piped, and cuts it (dropping it kills it) once its stderr
/// has said nothing new for `quiet`, or once it has run `ceiling` in all; the error names the rule
/// that cut it. Its stderr is kept (the last 64 KiB) for the caller to read its last line.
async fn watch(
    mut child: tokio::process::Child,
    shown: &str,
    quiet: Duration,
    ceiling: Duration,
) -> Result<std::io::Result<std::process::Output>, GitError> {
    use tokio::io::AsyncReadExt;
    use tokio::time::{Instant, sleep, sleep_until};
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let output = child.wait_with_output();
    tokio::pin!(output);
    let over = sleep(ceiling);
    tokio::pin!(over);
    let mut said = Vec::new();
    let mut chunk = vec![0u8; 8192];
    let mut open = true;
    let mut last = Instant::now();
    let done = loop {
        tokio::select! {
            done = &mut output => break done,
            read = stderr.read(&mut chunk), if open => match read {
                Ok(0) | Err(_) => open = false,
                Ok(n) => {
                    said.extend_from_slice(&chunk[..n]);
                    // ponytail: only the last line is read, so 64 KiB is kept.
                    if said.len() > 64 * 1024 {
                        said.drain(..said.len() - 64 * 1024);
                    }
                    last = Instant::now();
                }
            },
            () = sleep_until(last + quiet) => {
                return Err(GitError(format!(
                    "{shown} made no progress for {} seconds, so obid stopped it",
                    quiet.as_secs()
                )));
            }
            () = &mut over => {
                return Err(GitError(format!(
                    "{shown} was still running after {} seconds, obid's ceiling for a clone or fetch",
                    ceiling.as_secs()
                )));
            }
        }
    };
    if open {
        let _ = stderr.read_to_end(&mut said).await;
    }
    Ok(done.map(|mut output| {
        output.stderr = said;
        output
    }))
}

/// Indexes `pack` (what a job committed, packed in its sandbox) into the clone. `index-pack`
/// computes every object's id from its content, and `--strict` refuses a broken object or one
/// that names an object neither the pack nor the clone has, so nothing the job wrote lands under a
/// false name or half there.
pub async fn take_pack(clone: &Path, pack: Vec<u8>) -> Result<(), GitError> {
    let args: Vec<&OsStr> = ["index-pack", "--stdin", "--strict"]
        .iter()
        .map(OsStr::new)
        .collect();
    run_bytes(clone, &args, &[], Some(pack)).await.map(|_| ())
}

/// Moves the clone's `refs/heads/<branch>` to `commit`, which the clone must have.
pub async fn move_branch(clone: &Path, branch: &str, commit: &str) -> Result<(), GitError> {
    if !is_branch_name(branch) || !is_sha(commit) {
        return Err(GitError(format!(
            "{branch} at {commit} isn't a branch and commit obid moves"
        )));
    }
    if !has_commit(clone, commit).await {
        return Err(GitError(format!(
            "the clone doesn't have the commit {commit}"
        )));
    }
    git(
        clone,
        &["update-ref", &format!("refs/heads/{branch}"), commit],
    )
    .await
    .map(|_| ())
}

/// [`run`] with plain string arguments.
pub async fn git(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    run(dir, &args, &[]).await
}

/// [`git`] in a job's worktree with its git folders given (`GIT_DIR`, `GIT_COMMON_DIR`,
/// `GIT_WORK_TREE`, crate::jobs, `git_folders`), so git never reads the worktree's `.git` or the
/// git folder's `commondir`, which the job's agent could have rewritten since obid checked them.
pub async fn pinned(
    dir: &Path,
    folders: &[(&str, String)],
    args: &[&str],
) -> Result<String, GitError> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    let extra: Vec<(&str, &str)> = folders.iter().map(|(k, v)| (*k, v.as_str())).collect();
    run(dir, &args, &extra).await
}

/// A git command of obid's in `dir` that doesn't wait on the runtime (`std::process`), with what
/// [`run_bytes`] gives every call (the [`SAFE`] settings, no prompt, none of git's variables from
/// obid's own environment) and `folders` pinning it to a job's git folders ([`pinned`]); with none,
/// git finds them itself, as [`run`] does.
pub fn command(dir: &Path, folders: &[(&str, String)]) -> std::process::Command {
    let mut command = std::process::Command::new("git");
    command
        .current_dir(dir)
        .args(SAFE)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .stdin(std::process::Stdio::null());
    for (key, value) in folders {
        command.env(key, value);
    }
    command
}

/// The clone of `repo` (`owner/name`) under `home`.
pub fn clone_path(home: &Path, repo: &str) -> PathBuf {
    let mut path = home.join("clones");
    for part in repo.split('/') {
        path.push(part);
    }
    path
}

/// Makes the clone of `repo` from `remote` if there is none, with its push URL invalid, then
/// fetches `refspecs` from origin. Answers the clone's path.
pub async fn ensure_clone(
    home: &Path,
    repo: &str,
    remote: &str,
    refspecs: &[String],
) -> Result<PathBuf, GitError> {
    let clone = clone_path(home, repo);
    if !clone.join(".git").is_dir() {
        let parent = clone.parent().expect("a clone has a parent");
        std::fs::create_dir_all(parent)
            .map_err(|e| GitError(format!("can't make {}: {e}", parent.display())))?;
        let partial = parent.join(format!(
            ".{}.cloning",
            clone.file_name().unwrap_or_default().to_string_lossy()
        ));
        let _ = std::fs::remove_dir_all(&partial);
        let signed = crate::publish::fetch_credentials(remote, crate::github::gh().as_deref());
        transfer(parent, &clone_args(&signed, remote, &partial)).await?;
        git(&partial, &["config", "remote.origin.pushurl", NO_PUSH_URL]).await?;
        // Only the default branch: what else the clone holds comes in by obid's own fetches,
        // which say when it's someone else's ([`taint`]; crate::secretspec).
        git(&partial, &["config", NARROW, "true"]).await?;
        std::fs::rename(&partial, &clone)
            .map_err(|e| GitError(format!("can't move the clone into place: {e}")))?;
    } else {
        // A clone made before is held to the same rules.
        git(&clone, &["config", "remote.origin.url", remote]).await?;
        git(&clone, &["config", "remote.origin.pushurl", NO_PUSH_URL]).await?;
    }
    // No automatic maintenance: after an agent's commit it packs refs into packed-refs, which the
    // job sandbox doesn't let it write, and the agent spends a turn on the error.
    git(&clone, &["config", "maintenance.auto", "false"]).await?;
    git(&clone, &["config", "gc.auto", "0"]).await?;
    // A job's worktree HEAD is its commits, whose objects stay in the job's own object folder
    // until obid keeps them: a fetch here checks what it got against every worktree's HEAD too,
    // and fails ("did not send all necessary objects") while another job holds commits (#628).
    // Fetches leave the worktrees' HEADs out of that check, so it checks more, never less.
    git(&clone, &["config", "fetch.hideRefs", "worktrees/"]).await?;
    exclude_obi_folders(&clone)?;
    if !refspecs.is_empty() {
        fetch(&clone, remote, refspecs).await?;
    }
    Ok(clone)
}

/// The clone's setting that says obid made it with only the default branch (`git clone
/// --single-branch`): a clone made before holds every branch of its remote, teammates' too.
pub const NARROW: &str = "obid.narrow";
/// The clone's setting that says obid fetched someone else's commits into it, and why.
pub const FOREIGN: &str = "obid.foreign";

/// Notes in `clone`'s own settings, which no job's sandbox can write, that obid fetched someone
/// else's commits into it (`why`): from then on no check whose worktree reads it gets the owner's
/// secrets (crate::secretspec). The first reason is kept.
pub async fn taint(clone: &Path, why: &str) {
    if git(clone, &["config", "--get", FOREIGN]).await.is_ok() {
        return;
    }
    if let Err(e) = git(clone, &["config", FOREIGN, why]).await {
        tracing::warn!(
            "couldn't note that {} holds someone else's commits: {e}",
            clone.display()
        );
    }
}

/// [`taint`], at once, from a caller that can't wait: `git config` run as it is.
pub fn taint_now(clone: &Path, why: &str) {
    let has = std::process::Command::new("git")
        .args(["config", "--get", FOREIGN])
        .current_dir(clone)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if has {
        return;
    }
    let set = std::process::Command::new("git")
        .args(["config", FOREIGN, why])
        .current_dir(clone)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if !set.is_ok_and(|s| s.success()) {
        tracing::warn!(
            "couldn't note that {} holds someone else's commits",
            clone.display()
        );
    }
}

/// Why `clone` may hold someone else's commits, or none: it was made before clones held only
/// their default branch, or obid fetched someone else's into it ([`taint`]).
pub async fn foreign_in(clone: &Path) -> Option<String> {
    if git(clone, &["config", "--get", NARROW])
        .await
        .ok()
        .as_deref()
        != Some("true")
    {
        return Some(
            "obid's clone of the repository was made before clones held only their default branch, so teammates' branches are in it (move it aside, and obid makes it again)".into(),
        );
    }
    git(clone, &["config", "--get", FOREIGN])
        .await
        .ok()
        .map(|why| format!("obid's clone of the repository holds someone else's commits ({why})"))
}

/// `git clone` of `remote` into `partial`, after the credential settings `signed`
/// ([`crate::publish::fetch_credentials`]).
fn clone_args<'a>(signed: &'a [String], remote: &'a str, partial: &'a Path) -> Vec<&'a OsStr> {
    let mut args: Vec<&OsStr> = signed.iter().map(OsStr::new).collect();
    args.extend::<[&OsStr; 7]>([
        "clone".as_ref(),
        "--no-checkout".as_ref(),
        "--single-branch".as_ref(),
        // On stderr even with no terminal: what tells [`TRANSFER`] the clone goes on. Never with
        // `--quiet`, which keeps git's own `Receiving objects` silent and lets only the server's
        // early `remote:` lines through.
        "--progress".as_ref(),
        "--".as_ref(),
        remote.as_ref(),
        partial.as_os_str(),
    ]);
    args
}

/// `git fetch --progress --no-tags --no-write-fetch-head origin <refspecs>` in `clone`,
/// under [`TRANSFER`], an `https`
/// `remote` signed in with gh's login alone, as a clone is ([`crate::publish::fetch_credentials`]):
/// a private repository fetches with nothing set up but gh's sign-in.
pub async fn fetch(clone: &Path, remote: &str, refspecs: &[String]) -> Result<String, GitError> {
    let signed = crate::publish::fetch_credentials(remote, crate::github::gh().as_deref());
    let args: Vec<&OsStr> = fetch_args(&signed, refspecs)
        .into_iter()
        .map(OsStr::new)
        .collect();
    transfer(clone, &args).await
}

/// `git fetch` of `refspecs` from origin, after the credential settings `signed`.
pub(crate) fn fetch_args<'a>(signed: &'a [String], refspecs: &'a [String]) -> Vec<&'a str> {
    let mut args: Vec<&str> = signed.iter().map(String::as_str).collect();
    args.extend([
        // `--progress` and never `--quiet`, as a clone ([`clone_args`]); with
        // `--no-write-fetch-head` nothing goes to stdout.
        "fetch",
        "--progress",
        "--no-tags",
        "--no-write-fetch-head",
        "origin",
    ]);
    args.extend(refspecs.iter().map(String::as_str));
    args
}

/// `.obi/` (what obid hands an agent: quoted inputs and context) and `.oby/` (the Swift runner's)
/// are never part of the work: excluded in the clone's `info/exclude`, which every worktree shares.
pub fn exclude_obi_folders(clone: &Path) -> Result<(), GitError> {
    let info = clone.join(".git").join("info");
    std::fs::create_dir_all(&info)
        .map_err(|e| GitError(format!("can't make {}: {e}", info.display())))?;
    let exclude = info.join("exclude");
    let text = std::fs::read_to_string(&exclude).unwrap_or_default();
    let mut lines: Vec<&str> = text.lines().collect();
    let mut changed = false;
    for folder in ["/.obi/", "/.oby/"] {
        if !lines.contains(&folder) {
            lines.push(folder);
            changed = true;
        }
    }
    if changed {
        let mut joined = lines.join("\n");
        joined.push('\n');
        std::fs::write(&exclude, joined)
            .map_err(|e| GitError(format!("can't write {}: {e}", exclude.display())))?;
    }
    Ok(())
}

/// Whether `commit` is a commit the clone has.
pub async fn has_commit(clone: &Path, commit: &str) -> bool {
    git(clone, &["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .await
        .is_ok()
}

/// The symbolic links `commit` holds (tree mode `120000`), as paths relative to its root.
pub async fn committed_links(dir: &Path, commit: &str) -> Result<Vec<PathBuf>, GitError> {
    use std::os::unix::ffi::OsStrExt;
    let words = ["ls-tree", "-r", "-z", "--full-tree", commit];
    let args: Vec<&OsStr> = words.iter().map(OsStr::new).collect();
    let listed = run_bytes(dir, &args, &[], None).await?;
    let mut links = Vec::new();
    for entry in listed.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        // `<mode> <type> <object>\t<path>`
        let Some(tab) = entry.iter().position(|b| *b == b'\t') else {
            return Err(GitError(
                "git ls-tree answered a line obid can't read".into(),
            ));
        };
        if entry.starts_with(b"120000 ") {
            links.push(PathBuf::from(OsStr::from_bytes(&entry[tab + 1..])));
        }
    }
    Ok(links)
}

/// What `head` changes against `base` from their merge base, the files it touches listed first,
/// as git shows it with nothing a repository or a branch could set up taking part: no replacement
/// objects, no external diff or text conversion, fixed `a/` and `b/` prefixes, no color.
pub async fn change(dir: &Path, base: &str, head: &str) -> Result<Vec<u8>, GitError> {
    let range = format!("{base}...{head}");
    let words = [
        "--no-replace-objects",
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--find-renames",
        "--patch-with-stat",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        &range,
        "--",
    ];
    let args: Vec<&OsStr> = words.iter().map(OsStr::new).collect();
    run_bytes(dir, &args, &[], None).await
}

/// A worktree's git directories: its own (`<common>/worktrees/<name>`) and the common one.
pub async fn worktree_dirs(worktree: &Path) -> Result<(PathBuf, PathBuf), GitError> {
    let admin = git(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
    )
    .await?;
    let common = git(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await?;
    Ok((PathBuf::from(admin), PathBuf::from(common)))
}

/// Whether a branch this name would read as another ref in a job's or a review's `git diff
/// origin/dev...HEAD`: a local `origin/dev` hides the remote's, and blinds the review.
pub fn is_ref_shaped(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "head"
        || ["refs/", "heads/", "remotes/", "tags/", "origin/"]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
}

/// A valid branch name, as git and the spec allow it (no option-like or odd names).
pub fn is_branch_name(name: &str) -> bool {
    let ok_chars = name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'));
    !name.is_empty()
        && name.len() <= 200
        && ok_chars
        && !name.starts_with(['-', '/', '.'])
        && !name.contains("..")
        && !name.contains("//")
        && !name.ends_with('/')
        && !name.ends_with('.')
        && !name.ends_with(".lock")
        && !name.contains("/.")
}

/// What a checkout of obid's sets so Git LFS files stay pointers, as the check sees them
/// (PUBLISHING-6, 20): the person's own `filter.lfs` would run git-lfs with no login and fail it.
pub const LFS_POINTERS: [&str; 6] = [
    "-c",
    "filter.lfs.smudge=",
    "-c",
    "filter.lfs.process=",
    "-c",
    "filter.lfs.required=false",
];

/// Whether `commit` sends files to Git LFS: a `filter=lfs` that isn't a comment in any of its
/// `.gitattributes`, nested ones too (PUBLISHING-20). A tree git can't read says no.
pub async fn uses_lfs(clone: &Path, commit: &str) -> bool {
    git(
        clone,
        &[
            "grep",
            "-q",
            "-e",
            "^[^#]*filter=lfs",
            commit,
            "--",
            ":(glob)**/.gitattributes",
        ],
    )
    .await
    .is_ok()
}

/// Whether `commit` has submodules: a `.gitmodules` at its root (PUBLISHING-20).
pub async fn has_submodules(clone: &Path, commit: &str) -> bool {
    git(clone, &["cat-file", "-e", &format!("{commit}:.gitmodules")])
        .await
        .is_ok()
}

/// The most submodules [`fill_submodules`] tries in one worktree, nested ones too: one that holds
/// itself would go on forever.
const MOST_SUBMODULES: usize = 100;

/// How long [`fill_submodules`] has for all of a worktree's submodules: it holds the repository's
/// preparing lock (`submodule init` and `sync` write the clone's shared config), so another job on
/// it waits at most this long.
const SUBMODULES_FOR: Duration = Duration::from_secs(20 * 60);

/// Fills the submodules of `worktree`'s checkout, by obid itself before the agent starts (SETUP-1):
/// `git submodule update --init` for each one the commit has, then each one's own, run as obid's
/// fetches are, with [`SAFE`], [`LFS_POINTERS`], no recursion of git's own,
/// `protocol.file.allow=never` (`always` with `allowFileRemotes`, the tests' fixture) and
/// [`submodule_credentials`]. Each one's URL is read as git will fetch it and checked
/// ([`fetchable`]) first. It tries at most [`MOST_SUBMODULES`] in [`SUBMODULES_FOR`], and ends
/// at once on a Stop. One that fails is named (`` its submodule `<path>` wasn't filled (<why>) ``)
/// and the others still fill; one from anywhere but the repository's owner marks `clone`
/// ([`taint`], `<job>: a submodule from <url>`).
pub async fn fill_submodules(
    clone: &Path,
    worktree: &Path,
    remote: &str,
    allow_file: bool,
    job: &str,
    stop: &tokio::sync::watch::Receiver<crate::process::Signal>,
) -> Vec<String> {
    let mut failed = Vec::new();
    let file = if allow_file { "always" } else { "never" };
    let mut settings = submodule_credentials(remote, crate::github::gh().as_deref());
    settings.extend(
        [
            "-c",
            format!("protocol.file.allow={file}").as_str(),
            "-c",
            "submodule.recurse=false",
            "-c",
            "fetch.recurseSubmodules=false",
        ]
        .map(str::to_owned),
    );
    settings.extend(LFS_POINTERS.map(str::to_owned));
    let deadline = tokio::time::Instant::now() + SUBMODULES_FOR;
    let mut stop = stop.clone();
    let mut tried = 0;
    let mut tainted = false;
    // Each checkout to fill and where it is in the worktree; a submodule's own once it's filled.
    let mut todo = vec![(worktree.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = todo.pop() {
        let listed = match git(&dir, &["ls-files", "--stage", "-z"]).await {
            Ok(listed) => listed,
            Err(e) => {
                let within = match prefix.trim_end_matches('/') {
                    "" => String::new(),
                    sub => format!(" in `{sub}`"),
                };
                failed.push(format!("its submodules' list{within} wasn't read ({e})"));
                continue;
            }
        };
        // A gitlink is mode 160000: `160000 <sha> 0\t<path>`.
        for path in listed
            .split('\0')
            .filter_map(|entry| entry.strip_prefix("160000 ")?.split_once('\t'))
            .map(|(_, path)| path)
        {
            let shown = format!("{prefix}{path}");
            let one = fill_one(&dir, path, &settings, remote, allow_file);
            let done = if tried >= MOST_SUBMODULES {
                Err(format!("obid tries at most {MOST_SUBMODULES}"))
            } else if tokio::time::Instant::now() >= deadline {
                Err(format!(
                    "obid gives submodules {} minutes in all",
                    SUBMODULES_FOR.as_secs() / 60
                ))
            } else {
                tried += 1;
                tokio::select! {
                    done = Box::pin(one) => done,
                    () = tokio::time::sleep_until(deadline) => Err(format!(
                        "obid gives submodules {} minutes in all", SUBMODULES_FOR.as_secs() / 60
                    )),
                    // Dropping the fill kills its git; the job ends stopped.
                    _ = stop.wait_for(|s| *s != crate::process::Signal::Run) => return failed,
                }
            };
            match done {
                Ok(url) => {
                    if !tainted && !same_owner(remote, &url) {
                        taint(clone, &format!("{job}: a submodule from {url}")).await;
                        tainted = true;
                    }
                    todo.push((dir.join(path), format!("{shown}/")));
                }
                Err(why) => {
                    let why: String = why.chars().take(200).collect();
                    failed.push(format!("its submodule `{shown}` wasn't filled ({why})"));
                }
            }
        }
    }
    failed
}

/// Fills the submodule at `path` in the checkout `dir`, not its own: its URL, or why not.
async fn fill_one(
    dir: &Path,
    path: &str,
    settings: &[String],
    remote: &str,
    allow_file: bool,
) -> Result<String, String> {
    let names = git(
        dir,
        &[
            "config",
            "-f",
            ".gitmodules",
            "-z",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ],
    )
    .await
    .unwrap_or_default();
    let mut named = names
        .split('\0')
        .filter_map(|entry| entry.split_once('\n'))
        .filter(|(_, named)| *named == path)
        .filter_map(|(key, _)| key.strip_prefix("submodule.")?.strip_suffix(".path"));
    let name = named.next().ok_or("`.gitmodules` doesn't name it")?;
    // git would fetch one of them, and not surely the one checked.
    if named.next().is_some() {
        return Err("`.gitmodules` names it more than once".into());
    }
    // `init` keeps a URL the clone's config already has (another commit's); `sync` writes this
    // commit's, resolved as `update` fetches it. Both run under the preparing lock, so the URL
    // read is the one fetched.
    for step in ["init", "sync"] {
        let mut args: Vec<&str> = settings.iter().map(String::as_str).collect();
        args.extend(["submodule", step, "--quiet", "--", path]);
        git(dir, &args).await.map_err(|e| e.0)?;
    }
    let url = git(dir, &["config", "--get", &format!("submodule.{name}.url")])
        .await
        .map_err(|_| "it has no URL".to_owned())?;
    fetchable(remote, &url, allow_file)?;
    let mut command = tokio::process::Command::from(self::command(dir, &[]));
    command
        .args(settings)
        .args(["submodule", "update", "--init", "--progress", "--", path])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let Limit::Stall { quiet, ceiling } = TRANSFER else {
        unreachable!("a transfer's limit is a stall's")
    };
    let child = command.spawn().map_err(|e| format!("git can't run: {e}"))?;
    let out = Box::pin(watch(child, "git submodule update", quiet, ceiling))
        .await
        .map_err(|e| e.0)?
        .map_err(|e| format!("git can't run: {e}"))?;
    if out.status.success() {
        return Ok(url);
    }
    // git's first `fatal:` says why; its last says only that it gave up.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = stderr
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let fatal = lines.iter().find(|l| l.starts_with("fatal: "));
    Err(fatal.or(lines.last()).copied().unwrap_or("").to_owned())
}

/// The `-c` settings a submodule's fetch signs in with: git's own credential helpers reset for
/// every host, then the owner's gh login ([`crate::publish::fetch_credentials`]) only for
/// `remote`'s own host, so a submodule anywhere else gets no credentials (SETUP-1).
pub(crate) fn submodule_credentials(remote: &str, gh: Option<&Path>) -> Vec<String> {
    let mut args = crate::publish::fetch_credentials(remote, gh);
    let host = remote
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .and_then(|auth| auth.rsplit('@').next())
        .filter(|host| !host.is_empty());
    match (host, args.get_mut(3)) {
        (Some(host), Some(helper)) => {
            *helper = helper.replacen(
                "credential.helper=",
                &format!("credential.https://{host}.helper="),
                1,
            );
        }
        _ => args = vec!["-c".into(), "credential.helper=".into()],
    }
    args
}

/// Where a network git URL points, read strictly (SETUP-1).
struct Place {
    ssh: bool,
    host: String,
    owner: String,
}

/// A network git URL's place (`https`, `http`, `git`, `ssh`, `git+ssh`, `ssh+git`, or git's
/// `[user@]host:path`), lower-case. None for a local path, another scheme, or anything git could
/// read another way than this does, so a caller fails closed: a `%` (git decodes it), `[`, `]`,
/// `\`, whitespace or a control character anywhere; a user or host of anything but letters,
/// digits, `.`, `-` (and `_` in a user); a port of anything but digits; a `.` or `..` in its path.
fn place(url: &str) -> Option<Place> {
    let odd = |c: char| matches!(c, '%' | '[' | ']' | '\\') || c.is_whitespace() || c.is_control();
    if url.chars().any(odd) {
        return None;
    }
    let url = url.to_ascii_lowercase();
    let (ssh, auth, path, port) = match url.split_once("://") {
        Some((scheme, rest)) => {
            let ssh = match scheme {
                "https" | "http" | "git" => false,
                "ssh" | "git+ssh" | "ssh+git" => true,
                _ => return None,
            };
            let (auth, path) = rest.split_once('/')?;
            (ssh, auth, path, true)
        }
        // git's scp form: a colon with no slash before it, and no port.
        None => {
            let (auth, path) = url.split_once(':')?;
            (true, auth, path, false)
        }
    };
    let (user, host) = match auth.rsplit_once('@') {
        Some((user, host)) => (Some(user), host),
        None => (None, auth),
    };
    let host = match host.split_once(':') {
        Some((host, digits))
            if port && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            host
        }
        Some(_) => return None,
        None => host,
    };
    let word = |text: &str, also: &[u8]| {
        !text.is_empty()
            && !text.starts_with(['-', '.'])
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-') || also.contains(&b))
    };
    if !word(host, b"") || user.is_some_and(|user| !word(user, b"_")) {
        return None;
    }
    let mut segments = path.trim_start_matches('/').split('/');
    if path.split('/').any(|s| s == "." || s == "..") {
        return None;
    }
    let owner = segments.next().filter(|owner| !owner.is_empty())?;
    Some(Place {
        ssh,
        host: host.to_owned(),
        owner: owner.to_owned(),
    })
}

/// Whether `url` is on `remote`'s code host under its owner: a submodule from anywhere else, or
/// one [`place`] can't read, is someone else's ([`taint`]).
pub fn same_owner(remote: &str, url: &str) -> bool {
    let at = |url: &str| place(url).map(|p| (p.host, p.owner));
    at(url).is_some() && at(url) == at(remote)
}

/// Whether obid fetches a submodule from `url`, as git will read it, for a repository at `remote`
/// (SETUP-1), fail-closed: an ssh one on another host than `remote`'s isn't, since the person's
/// ssh agent and keys would sign in there; nor one on this Mac or its network (loopback, a private
/// or link-local address, `localhost`) unless `remote` is there too; nor one [`place`] can't read
/// (a local path is the tests' fixture's, with `allowFileRemotes`).
fn fetchable(remote: &str, url: &str, allow_file: bool) -> Result<(), &'static str> {
    let home = place(remote).map(|p| p.host);
    let path = url.starts_with("file://")
        || !url.contains("://")
            && url
                .split_once(':')
                .is_none_or(|(before, _)| before.contains('/'));
    match place(url) {
        Some(p) if p.ssh && home.as_ref() != Some(&p.host) => {
            Err("another host's ssh submodule gets no keys")
        }
        Some(p) if near(&p.host) && home.as_ref() != Some(&p.host) => {
            Err("its URL is on this Mac or its network")
        }
        Some(_) => Ok(()),
        None if allow_file && path => Ok(()),
        None => Err("its URL isn't one obid can check"),
    }
}

/// Whether `host` is this Mac or its network: `localhost`, a loopback, private, link-local,
/// shared or unspecified IPv4 address, or a numeric name git's resolver reads as some address
/// (`127.1`, `2130706433`, `0x7f.0.0.1`). ponytail: a name whose DNS answers with such an
/// address isn't caught; resolve and pin it if a submodule's DNS is ever the attacker's.
fn near(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    let last = host.rsplit('.').next().unwrap_or(host);
    if !last.bytes().all(|b| b.is_ascii_digit()) && !last.starts_with("0x") {
        return false;
    }
    match host.parse::<std::net::Ipv4Addr>() {
        Ok(ip) => {
            let [a, b, ..] = ip.octets();
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || (a == 100 && (64..128).contains(&b))
        }
        // A numeric form only a resolver reads: fail closed.
        Err(_) => true,
    }
}

/// A full commit id, lower-case hex.
pub fn is_sha(text: &str) -> bool {
    (text.len() == 40 || text.len() == 64)
        && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RECOVERY-14: a repository whose name says `timeout` isn't a network failure.
    #[test]
    fn recovery_14_only_gits_own_words_say_it_passes() {
        let gone = GitError("git clone https://h/acme/timeout-service failed: fatal: repository 'https://h/acme/timeout-service/' not found".into());
        assert!(
            !crate::publish::passes_later(&gone.said()),
            "{}",
            gone.said()
        );
        let dns = GitError("git clone https://h/acme/x failed: fatal: unable to access 'https://h/acme/x/': Could not resolve host: h".into());
        assert!(crate::publish::passes_later(&dns.said()));
    }

    /// SETUP-1: a submodule's fetch gets the owner's gh login for the repository's own host only.
    /// Another host's, one whose name only starts like it, gets nothing: neither gh nor the
    /// person's own credential helper, which is reset.
    #[test]
    fn setup_1_another_hosts_submodule_gets_no_credentials() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("obid-submodule-login-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let gh = root.join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\ncat >> \"$0.log\"\necho username=x\necho password=from-gh\n",
        )
        .unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            root.join(".gitconfig"),
            "[credential]\n\thelper = \"!f() { echo username=u; echo password=the-persons; }; f\"\n",
        )
        .unwrap();
        let settings = submodule_credentials("https://github.com/o/r.git", Some(&gh));
        for (host, signs_in) in [
            ("github.com", true),
            ("gitlab.com", false),
            ("github.com.example.net", false),
        ] {
            let mut fill = command(&root, &[]);
            let mut child = fill
                .args(&settings)
                .args(["credential", "fill"])
                .env("HOME", &root)
                .env("XDG_CONFIG_HOME", &root)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            use std::io::Write;
            let asked = format!("protocol=https\nhost={host}\npath=o/lib.git\n\n");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(asked.as_bytes())
                .unwrap();
            let out = String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap();
            assert_eq!(out.contains("password=from-gh"), signs_in, "{host}: {out}");
            assert!(!out.contains("the-persons"), "{host}: {out}");
        }
        let log = std::fs::read_to_string(root.join("gh.log")).unwrap();
        assert!(log.contains("host=github.com\n"), "{log}");
        assert!(
            !log.contains("gitlab.com") && !log.contains("example.net"),
            "{log}"
        );
        // With no gh, every host's helpers are reset all the same.
        assert_eq!(
            submodule_credentials("https://github.com/o/r.git", None),
            ["-c", "credential.helper="]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// SETUP-1: an ssh submodule on another host than the repository's isn't fetched, so the
    /// person's ssh agent and keys never sign in there; one on its own host, or over https (which
    /// gets no credentials elsewhere), is. One git could read as another host than obid does
    /// (git decodes `%` and strips `[ ]` in an ssh URL) isn't fetched at all.
    #[test]
    fn setup_1_another_hosts_ssh_submodule_gets_no_keys() {
        let remote = "https://github.com/obi-ai-bot/oby.git";
        for keyless in [
            "git@gitlab.com:obi-ai-bot/lib.git",
            "ssh://git@gitlab.com/obi-ai-bot/lib",
            "git+ssh://git@bitbucket.org/o/lib",
            "git@github.com.example.net:o/lib.git",
            "ssh://git@gitlab.com:2222/o/lib",
        ] {
            let keys = Err("another host's ssh submodule gets no keys");
            assert_eq!(fetchable(remote, keyless, false), keys, "{keyless}");
        }
        for unread in [
            "ssh://evil.com%2F@github.com/obi-ai-bot/lib",
            "ssh://git@[evil.com]/obi-ai-bot/lib",
            "[evil.com]:obi-ai-bot/lib",
            "ssh://git@github.com:22x/obi-ai-bot/lib",
            "ssh://-oProxyCommand=x@github.com/o/lib",
            "ssh://git@-oProxyCommand=x/o/lib",
            "ssh://git@github.com /o/lib",
            "ssh://a@b@github.com/o/lib",
            "/tmp/lib",
            "file:///tmp/lib",
            "ext::sh -c x",
        ] {
            let unread_ = Err("its URL isn't one obid can check");
            assert_eq!(fetchable(remote, unread, false), unread_, "{unread}");
        }
        for fetched in [
            "git@github.com:someone/lib.git",
            "ssh://git@GitHub.com/obi-ai-bot/lib",
            "https://gitlab.com/obi-ai-bot/lib.git",
        ] {
            assert_eq!(fetchable(remote, fetched, false), Ok(()), "{fetched}");
        }
        // The tests' fixture's paths, with `allowFileRemotes` only.
        for path in ["/tmp/lib", "./lib:with-colon", "file:///tmp/lib"] {
            assert_eq!(fetchable(remote, path, true), Ok(()), "{path}");
        }
        // A remote that isn't on a host (the tests' `file://` fixture): every ssh one is elsewhere.
        assert!(fetchable("file:///tmp/r", "git@github.com:o/lib.git", true).is_err());
    }

    /// SETUP-1: a submodule on this Mac or its network isn't fetched, unless the repository is
    /// there too.
    #[test]
    fn setup_1_a_submodule_on_this_macs_network_isnt_fetched() {
        let remote = "https://github.com/o/r.git";
        for near in [
            "http://127.0.0.1/o/lib",
            "https://localhost/o/lib",
            "https://git.localhost/o/lib",
            "http://10.0.0.5/o/lib",
            "https://172.16.3.4:8443/o/lib",
            "http://192.168.1.2/o/lib",
            "http://169.254.169.254/latest",
            "git://100.64.0.1/o/lib",
            "http://0.0.0.0/o/lib",
            "http://2130706433/o/lib",
            "http://127.1/o/lib",
            "http://0x7f.0.0.1/o/lib",
        ] {
            let mac = Err("its URL is on this Mac or its network");
            assert_eq!(fetchable(remote, near, false), mac, "{near}");
        }
        assert!(fetchable(remote, "https://[::1]/o/lib", false).is_err());
        assert_eq!(
            fetchable(remote, "https://140.82.112.3/o/lib", false),
            Ok(())
        );
        // A repository on the network's own host fetches its submodules there.
        let own = "https://10.0.0.5/o/lib";
        assert_eq!(fetchable("https://10.0.0.5/o/r", own, false), Ok(()));
    }

    /// SETUP-1: a submodule is the owner's when it's on the repository's host under its owner;
    /// a path that climbs out of the owner, or that git decodes, isn't.
    #[test]
    fn setup_1_a_submodule_elsewhere_is_someone_elses() {
        let remote = "https://github.com/Obi-AI-Bot/oby.git";
        for ours in [
            "https://github.com/obi-ai-bot/lib.git",
            "git@github.com:obi-ai-bot/lib.git",
            "ssh://git@github.com/obi-ai-bot/lib",
        ] {
            assert!(same_owner(remote, ours), "{ours}");
        }
        for theirs in [
            "https://github.com/someone/lib.git",
            "https://gitlab.com/obi-ai-bot/lib.git",
            "https://github.com/obi-ai-bot/../someone/lib.git",
            "https://github.com/obi-ai-bot/./../someone/lib.git",
            "https://github.com/obi-ai-bot%2F..%2Fsomeone/lib.git",
            "https://github.com/obi-ai-bot/%2e%2e/someone/lib.git",
            "ssh://evil.com%2F@github.com/obi-ai-bot/lib",
            "file:///tmp/lib",
            "/tmp/lib",
        ] {
            assert!(!same_owner(remote, theirs), "{theirs}");
        }
    }

    /// A plain repository whose index has a gitlink at each of `links`, with `gitmodules`, for
    /// [`fill_submodules`] to read; and the stop signal's sender, at Run.
    fn gitlinked(
        name: &str,
        links: &[String],
        gitmodules: &str,
    ) -> (PathBuf, tokio::sync::watch::Sender<crate::process::Signal>) {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("obid-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        };
        assert!(git(&["init", "-q"]).wait().unwrap().success());
        std::fs::write(root.join(".gitmodules"), gitmodules).unwrap();
        let mut index = git(&["update-index", "--index-info"]);
        let sha = "1".repeat(40);
        for link in links {
            writeln!(index.stdin.as_mut().unwrap(), "160000 {sha}\t{link}").unwrap();
        }
        drop(index.stdin.take());
        assert!(index.wait().unwrap().success());
        (
            root,
            tokio::sync::watch::channel(crate::process::Signal::Run).0,
        )
    }

    /// SETUP-1: the URL obid checks is the one git fetches: this commit's `.gitmodules`, not one
    /// another commit left in the clone's config; and a path `.gitmodules` names twice isn't
    /// filled, since git could fetch either.
    #[tokio::test]
    async fn setup_1_the_url_checked_is_the_one_fetched() {
        let lib = std::env::temp_dir().join(format!("obid-sub-lib-{}", std::process::id()));
        let lib = lib.to_str().unwrap();
        let gitmodules = format!(
            "[submodule \"sub\"]\n\tpath = sub\n\turl = git@evil.example:x/y.git\n\
             [submodule \"a\"]\n\tpath = dup\n\turl = {lib}\n\
             [submodule \"b\"]\n\tpath = dup\n\turl = {lib}\n"
        );
        let links = ["sub".to_owned(), "dup".to_owned()];
        let (root, stop) = gitlinked("sub-url", &links, &gitmodules);
        // Another commit's URL for `sub`, which a bare `init` keeps.
        git(&root, &["config", "submodule.sub.url", lib])
            .await
            .unwrap();
        let failed =
            fill_submodules(&root, &root, "file:///r", true, "job j", &stop.subscribe()).await;
        assert_eq!(
            failed,
            [
                "its submodule `dup` wasn't filled (`.gitmodules` names it more than once)",
                "its submodule `sub` wasn't filled (another host's ssh submodule gets no keys)",
            ],
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// SETUP-1: every submodule obid tries counts toward its 100, the failed ones too; and a
    /// Stop ends the fill at once, naming nothing.
    #[tokio::test]
    async fn setup_1_every_try_counts_and_a_stop_ends_it() {
        let links: Vec<String> = (0..=MOST_SUBMODULES).map(|i| format!("p{i}")).collect();
        let (root, stop) = gitlinked("sub-count", &links, "");
        let failed =
            fill_submodules(&root, &root, "file:///r", true, "job j", &stop.subscribe()).await;
        assert_eq!(failed.len(), MOST_SUBMODULES + 1);
        let not_named = failed
            .iter()
            .filter(|f| f.ends_with("(`.gitmodules` doesn't name it)"));
        assert_eq!(not_named.count(), MOST_SUBMODULES);
        assert!(
            failed
                .iter()
                .any(|f| f.ends_with("(obid tries at most 100)")),
            "{failed:?}"
        );
        stop.send(crate::process::Signal::Stop).unwrap();
        let failed =
            fill_submodules(&root, &root, "file:///r", true, "job j", &stop.subscribe()).await;
        assert_eq!(failed, Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A clone and a fetch of an https remote with a gh reset git's own credential helpers before
    /// gh's, so no other helper (Apple git's system-wide osxkeychain) stores gh's token.
    #[test]
    fn https_clones_and_fetches_reset_gits_own_helpers_first() {
        let signed = crate::publish::fetch_credentials(
            "https://github.com/o/r.git",
            Some(Path::new("/opt/gh")),
        );
        let clone = clone_args(
            &signed,
            "https://github.com/o/r.git",
            Path::new("/c/.r.cloning"),
        );
        assert_eq!(
            clone[..2],
            [OsStr::new("-c"), OsStr::new("credential.helper=")]
        );
        assert_eq!(clone[2], OsStr::new("-c"));
        assert!(
            clone[3]
                .to_string_lossy()
                .ends_with("'/opt/gh' auth git-credential")
        );
        assert_eq!(clone[4], OsStr::new("clone"));
        let refspecs = ["main".to_owned()];
        let fetch = fetch_args(&signed, &refspecs);
        assert_eq!(fetch[..2], ["-c", "credential.helper="]);
        assert_eq!(
            fetch[4..],
            [
                "fetch",
                "--progress",
                "--no-tags",
                "--no-write-fetch-head",
                "origin",
                "main"
            ]
        );
    }

    /// `sh -c script` with its stderr piped, as [`run_limited`] spawns git.
    fn sh(script: &str) -> tokio::process::Child {
        Command::new("sh")
            .args(["-c", script])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap()
    }

    /// A clone that runs past its quiet window in all, printing all along, isn't cut.
    #[tokio::test]
    async fn recovery_13_a_clone_that_keeps_printing_progress_isnt_cut() {
        let child = sh(
            "for i in $(seq 15); do printf 'Receiving objects: %s\\r' $i >&2; sleep 0.2; done; echo done",
        );
        let output = watch(
            child,
            "git clone",
            Duration::from_secs(2),
            Duration::from_secs(30),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "done");
        assert!(String::from_utf8_lossy(&output.stderr).contains("Receiving objects: 15"));
    }

    /// A clone that goes silent is cut after the quiet window, long before the ceiling, and says
    /// which rule cut it; one that keeps printing still meets the ceiling.
    #[tokio::test]
    async fn recovery_13_a_silent_clone_is_cut_after_the_stall() {
        let started = std::time::Instant::now();
        let err = watch(
            sh("printf 'Receiving objects: 1%%\\r' >&2; sleep 30"),
            "git clone",
            Duration::from_millis(300),
            Duration::from_secs(30),
        )
        .await
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(
            err.0.starts_with("git clone made no progress for "),
            "{}",
            err.0
        );
        let err = watch(
            sh("while true; do echo x >&2; sleep 0.05; done"),
            "git clone",
            Duration::from_secs(5),
            Duration::from_millis(400),
        )
        .await
        .unwrap_err();
        assert!(
            err.0.contains("obid's ceiling for a clone or fetch"),
            "{}",
            err.0
        );
    }

    /// A clone's and a fetch's real arguments ask for `--progress` and never `--quiet`, which
    /// silences git's own progress so a long download looks stalled.
    #[test]
    fn recovery_13_clones_and_fetches_print_progress_without_quiet() {
        let signed = crate::publish::fetch_credentials(
            "https://github.com/o/r.git",
            Some(Path::new("/opt/gh")),
        );
        let clone = clone_args(&signed, "https://github.com/o/r.git", Path::new("/c/.r"));
        let refspecs = ["main".to_owned()];
        let fetch = fetch_args(&signed, &refspecs);
        assert!(clone.contains(&OsStr::new("--progress")));
        assert!(fetch.contains(&"--progress"));
        for quiet in ["--quiet", "-q"] {
            assert!(!clone.contains(&OsStr::new(quiet)));
            assert!(!fetch.contains(&quiet));
        }
    }

    #[test]
    fn branch_names() {
        for good in ["main", "feature/average", "oby/fix-12", "v1.2"] {
            assert!(is_branch_name(good), "{good}");
        }
        for bad in [
            "",
            "-x",
            "--upload-pack=sh",
            "a..b",
            "a//b",
            "a/",
            "a.",
            "a.lock",
            "a/.b",
            "a b",
            ".x",
            "/x",
        ] {
            assert!(!is_branch_name(bad), "{bad}");
        }
    }

    /// git's automatic maintenance never runs in obid's clones: after an agent's commit it packs
    /// refs into the clone's packed-refs, which the job sandbox doesn't let it write, and the
    /// agent spent a turn checking a commit that had landed. A clone made before is held to it too.
    #[tokio::test]
    async fn clones_run_no_automatic_maintenance() {
        let root = std::env::temp_dir().join(format!("obid-maintenance-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let remote = root.join("remote");
        std::fs::create_dir_all(&remote).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &[
                "-c",
                "user.name=a",
                "-c",
                "user.email=a@b",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "first",
            ],
        ] {
            git(&remote, args).await.unwrap();
        }
        let home = root.join("home");
        let url = format!("file://{}", remote.display());
        let setting = |clone: PathBuf, key: &'static str| async move {
            git(&clone, &["config", "--get", key])
                .await
                .unwrap_or_default()
        };
        let clone = ensure_clone(&home, "o/r", &url, &[]).await.unwrap();
        assert_eq!(setting(clone.clone(), "maintenance.auto").await, "false");
        assert_eq!(setting(clone.clone(), "gc.auto").await, "0");
        assert_eq!(setting(clone.clone(), "fetch.hideRefs").await, "worktrees/");
        git(&clone, &["config", "maintenance.auto", "true"])
            .await
            .unwrap();
        let clone = ensure_clone(&home, "o/r", &url, &[]).await.unwrap();
        assert_eq!(setting(clone, "maintenance.auto").await, "false");
        let _ = std::fs::remove_dir_all(&root);
    }
}
