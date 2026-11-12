//! The processes obid starts for a job (an agent, the repository's check): each in a session (and
//! so a process group) of its own, its environment made from scratch, and ended with everything it
//! started.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use nix::sys::signal::killpg;
use nix::unistd::Pid;

/// What a job's worker is told while its agent or check runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Carry on.
    Run,
    /// The owner stopped the job: end everything now.
    Stop,
    /// obid is stopping: end everything now and leave the job as it is, for the next start.
    Shutdown,
}

/// From SIGTERM to SIGKILL when a job is stopped: Stop all promises every agent ends within 2 seconds.
pub const STOPPING: Duration = Duration::from_millis(400);

/// The variables a sandboxed process may keep from obid's own environment: the locale, the
/// terminal, who it runs as, and how to reach the network through a proxy. Nothing else: no token,
/// no login, no Claude Code or Anthropic variable (AGENTS.md, "Spawning claude").
const KEPT: &[&str] = &[
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "TERM",
    "USER",
    "LOGNAME",
    "SHELL",
    "TZ",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "NO_PROXY",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
];

/// Names that never reach an agent or a check, however they got into a list.
pub fn is_forbidden(name: &str) -> bool {
    name == "CLAUDECODE"
        || name.starts_with("CLAUDE_CODE_")
        || name.starts_with("ANTHROPIC_")
        || name == "GH_TOKEN"
        || name == "GITHUB_TOKEN"
        || name == crate::token::ENV
        || name.starts_with("OBY_GITHUB_APP")
}

/// Names an agent never gets, in either lane: [`is_forbidden`]'s, and every one that pushes or
/// signs in to a code host (an SSH agent, an askpass, an ssh git runs, gh's and GitHub's). Not
/// `is_forbidden` itself: obid's own gh and Publish drop only its names, and Publish over SSH
/// needs `SSH_AUTH_SOCK` (docs/contracts/lanes.md, LANES-2).
pub fn agent_withheld(name: &str) -> bool {
    is_forbidden(name)
        || [
            "SSH_AUTH_SOCK",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
            "GIT_SSH",
            "GIT_SSH_COMMAND",
        ]
        .contains(&name)
        || name.starts_with("GH_")
        || name.starts_with("GITHUB_")
}

/// What every agent's environment sets, in either lane, so nothing it runs publishes (LANES-2):
/// an empty GitHub CLI configuration in `temp`, git that never prompts, reads no settings but the
/// repository's and has [`no_publish_git`] on top of whatever those say.
pub fn no_publish_env(temp: &Path) -> Vec<(&'static str, OsString)> {
    vec![
        (
            "GH_CONFIG_DIR",
            temp.join(".config/gh-none").into_os_string(),
        ),
        ("GIT_TERMINAL_PROMPT", "0".into()),
        ("GIT_CONFIG_NOSYSTEM", "1".into()),
        ("GIT_CONFIG_GLOBAL", "/dev/null".into()),
        ("GIT_CONFIG_PARAMETERS", no_publish_git()),
    ]
}

/// The `GIT_CONFIG_PARAMETERS` of [`no_publish_env`] and of the egress proxy's environment, one
/// value since a launch's last one wins: an empty credential helper, and `origin`'s push URLs
/// emptied (an empty value resets git's list) and set to obid's invalid one
/// (`crate::git::NO_PUSH_URL`), whatever the clone's own settings say. Never `GIT_CONFIG_KEY_0`:
/// a Codex that drops `*KEY*` variables would leave git its count alone, and git refuses to run then.
pub fn no_publish_git() -> OsString {
    format!(
        "'credential.helper=' 'remote.origin.pushurl=' 'remote.origin.pushurl={}'",
        crate::git::NO_PUSH_URL
    )
    .into()
}

/// The [`KEPT`] variables `get` has (obid's own environment): what every process obid starts
/// keeps of it, a job's and the brain's alike.
pub fn kept(get: impl Fn(&str) -> Option<OsString>) -> Vec<(OsString, OsString)> {
    KEPT.iter()
        .filter_map(|name| Some(((*name).into(), get(name)?)))
        .collect()
}

/// `path` with a folder holding a `ps` a box can run first. macOS runs no setuid program in a
/// sandbox, whatever its profile says, and `/bin/ps` is one, so a repository's tests that look at
/// processes could never pass in a job (#1789). A copy without setuid, signed ad hoc (the platform's
/// signature no longer fits it, and macOS would kill it), reads the same processes of the same
/// person: nothing an agent couldn't make for itself. Anywhere else, or when it can't be made,
/// `path` as it is.
fn with_own_ps(path: &OsStr) -> OsString {
    let Some(dir) = cfg!(target_os = "macos")
        .then(|| own_ps_folder(&std::env::temp_dir()))
        .flatten()
    else {
        return path.to_os_string();
    };
    let mut dirs = vec![dir];
    dirs.extend(std::env::split_paths(path));
    std::env::join_paths(dirs).unwrap_or_else(|_| path.to_os_string())
}

/// Process inspection uses the same non-setuid copy as a job's PATH on macOS. Absolute
/// `/bin/ps` calls bypass that copy and are refused when obid itself runs inside a job's box.
#[cfg(not(target_os = "linux"))]
pub(crate) fn ps_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(dir) = own_ps_folder(&std::env::temp_dir()) {
        return dir.join("ps");
    }
    PathBuf::from("/bin/ps")
}

/// Where [`own_ps_folder`] keeps the copy in `temp`, made or not: a box that hides `temp` still
/// reads it (`crate::sandbox`).
pub(crate) fn own_ps_dir(temp: &Path) -> PathBuf {
    temp.join(format!("oby-tools-{}", nix::unistd::getuid()))
}

/// The folder of [`with_own_ps`]'s copy, in the person's own temporary folder: no box writes
/// there and every box reads it, so no job can swap it, and nothing is written in a job's folders.
/// Only a private folder of theirs on an absolute path is first on anyone's `PATH` (a `TMPDIR` of
/// `/tmp`, where another person could have made it first, or an empty one, gets none). macOS clears
/// unused files there after some days: it's made again when it's gone.
fn own_ps_folder(temp: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    // One at a time: two threads' copies under one name would rename a half-signed one into place.
    static MAKING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let uid = nix::unistd::getuid();
    let dir = own_ps_dir(temp);
    if !dir.is_absolute() {
        return None;
    }
    let _making = MAKING.lock().unwrap_or_else(|e| e.into_inner());
    let _ = std::fs::DirBuilder::new().mode(0o700).create(&dir);
    if !crate::sandbox::private_folder(&dir, uid.as_raw()) {
        return None;
    }
    let ps = dir.join("ps");
    // (a power cut can leave an empty one, which would run and print nothing)
    if std::fs::metadata(&ps).is_ok_and(|m| m.is_file() && m.len() > 0) {
        return Some(dir);
    }
    let making = dir.join(format!("ps.{}", std::process::id()));
    std::fs::copy("/bin/ps", &making).ok()?;
    std::fs::set_permissions(&making, std::fs::Permissions::from_mode(0o755)).ok()?;
    let signed = std::process::Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", "-"])
        .arg(&making)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    let kept = std::fs::File::open(&making).is_ok_and(|f| f.sync_all().is_ok());
    if !signed || !kept || std::fs::rename(&making, &ps).is_err() {
        let _ = std::fs::remove_file(&making);
        return None;
    }
    Some(dir)
}

/// The environment of a process in the job sandbox: `PATH`, `HOME` and `TMPDIR` (the job's
/// temporary folder), [`no_publish_env`] but its credential helper, xcrun's cache in
/// the temporary folder, the kept variables above, then `extra` (an account's `CLAUDE_CONFIG_DIR`).
pub fn sandbox_env(
    path: &OsStr,
    temp: &Path,
    extra: &[(&str, OsString)],
) -> Vec<(OsString, OsString)> {
    let mut env = kept(|name| std::env::var_os(name));
    env.push(("PATH".into(), with_own_ps(path)));
    env.push(("HOME".into(), temp.as_os_str().to_os_string()));
    env.push(("TMPDIR".into(), temp.as_os_str().to_os_string()));
    // No git settings but the repository's: never a `.gitconfig` the box's own commands wrote in
    // its `HOME` (a pager, a `diff.external`, a textconv that runs or hides what git prints), nor
    // the system's (PERMISSIONS-14). An agent's launch adds the empty credential helper too.
    env.extend(
        no_publish_env(temp)
            .into_iter()
            .filter(|(name, _)| *name != "GIT_CONFIG_PARAMETERS")
            .map(|(name, value)| (name.into(), value)),
    );
    // macOS's /usr/bin/git is xcrun's shim: its cache goes here, not in /var/folders, which the
    // sandbox doesn't let a job write.
    env.push(("xcrun_db".into(), temp.join("xcrun_db").into_os_string()));
    // Foundation's home is the password database's, whatever `HOME` says, so SwiftPM's caches and
    // Xcode's DerivedData would go in the person's hidden `~/Library`: this makes it the job's.
    env.push(("CFFIXED_USER_HOME".into(), temp.as_os_str().to_os_string()));
    // What `extra` sets wins over what obid's own environment had (a job's own proxy), and its
    // last word for a name over its first (the proxy's credential helper and LANES-2's).
    for (name, value) in extra {
        env.retain(|(set, _)| set.as_os_str() != OsStr::new(name));
        env.push(((*name).into(), value.clone()));
    }
    // obid's own CLAUDE_CODE_TMPDIR (a Claude Code run's, below), TODO_TOOLS, NO_CLAUDE_MDS and
    // NO_AUTO_MEMORY are the only CLAUDE_CODE_* let in.
    env.retain(|(name, _)| {
        name == CLAUDE_TMPDIR
            || name == CLAUDE_TODO_TOOLS
            || name == NO_CLAUDE_MDS
            || name == NO_AUTO_MEMORY
            || !is_forbidden(&name.to_string_lossy())
    });
    env
}

/// About what one rustc of a big crate holds: a build gets one job for each this much memory.
pub const BUILD_JOB_MEMORY: u64 = 1536 * 1024 * 1024;

/// How many jobs a build runs at once, and its tests' threads, with `room` bytes of memory left it
/// (crate::jobs, `build_env`): one for each [`BUILD_JOB_MEMORY`], at most half the cores (the Mac
/// has other work), at least 1.
pub fn build_jobs(room: u64) -> usize {
    let cores = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
    let fits = room / BUILD_JOB_MEMORY;
    usize::try_from(fits)
        .unwrap_or(usize::MAX)
        .clamp(1, (cores / 2).max(1))
}

/// What a build is told of its parallelism ([`build_jobs`]): Cargo's jobs and Rust's test threads,
/// and `OBY_BUILD_JOBS` for the repository's own scripts (xcodebuild's `-jobs`, swift's `-j`).
pub fn build_env(jobs: usize) -> Vec<(&'static str, OsString)> {
    let jobs = OsString::from(jobs.to_string());
    ["CARGO_BUILD_JOBS", "RUST_TEST_THREADS", "OBY_BUILD_JOBS"]
        .into_iter()
        .map(|name| (name, jobs.clone()))
        .collect()
}

/// What Oby's review of published work adds to a Claude Code run's `extra`: no CLAUDE.md of the
/// change it reads is loaded as instructions (crate::review quotes the base's rules instead).
pub const NO_CLAUDE_MDS: &str = "CLAUDE_CODE_DISABLE_CLAUDE_MDS";

/// What an own conversation's Claude Code adds (LANES-12): its auto-memory off, as `--restricted`
/// keeps it in a boxed one, so what a turn read never lands in the account's memory.
pub const NO_AUTO_MEMORY: &str = "CLAUDE_CODE_DISABLE_AUTO_MEMORY";

/// What a Claude Code launch adds to [`sandbox_env`]'s `extra`: Claude Code makes its Bash folder in
/// /tmp/claude-<uid> unless told, which the sandbox doesn't let a job write (a field test,
/// 2026-10-17: every Bash command failed with EPERM), so it's the run's own temporary folder.
pub const CLAUDE_TMPDIR: &str = "CLAUDE_CODE_TMPDIR";

/// The launch's variable naming an own run's tag ([`end_sandboxed`]); never logged.
pub const RUN_TAG: &str = "OBY_RUN_TAG";

/// What a job's Claude Code launch adds too: headless, Claude Code offers no to-do tool unless told
/// (claude 2.1.302's system `init`, 2026-11-02: no `TodoWrite` or `TaskCreate`), so a job could never
/// make the list its card shows (docs/contracts/live-task.md).
pub const CLAUDE_TODO_TOOLS: &str = "CLAUDE_CODE_ENABLE_TODO_TOOLS";

/// The person's Rust toolchain as their shell finds it: rustup's home (`RUSTUP_HOME`, else
/// `~/.rustup`) and cargo's own `bin` (`CARGO_HOME/bin`, else `~/.cargo/bin`), each when it's a
/// folder. A box's `HOME` is the job's, so without them a rustup proxy finds no toolchain there,
/// and cargo, which looks for `cargo-*` in its own `bin` before `PATH` when that `bin` isn't on
/// `PATH` (cargo#11020), skipped the person's rustup clippy for Homebrew's older one on `PATH`,
/// whose lints differ (2026-10-24).
fn rust_homes() -> [Option<PathBuf>; 2] {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let named = |var: &str, default: &str| {
        std::env::var_os(var)
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(default)))
            .filter(|p| p.is_absolute())
    };
    [
        named("RUSTUP_HOME", ".rustup"),
        named("CARGO_HOME", ".cargo").map(|c| c.join("bin")),
    ]
    .map(|dir| dir.filter(|d| d.is_dir()))
}

/// What a job's box reads of the person's Rust toolchain ([`rust_homes`]), read-only: rustup's
/// toolchains and settings, and cargo's `bin`, never the rest of the cargo home (its
/// `credentials.toml`, its registry), and neither when it would show what the box hides (a
/// `RUSTUP_HOME` that is the person's home, say: [`crate::sandbox::shows_nothing_hidden`]).
pub fn rust_reads(hidden: &[PathBuf], never: &[PathBuf]) -> Vec<PathBuf> {
    rust_homes()
        .into_iter()
        .flatten()
        .filter(|dir| crate::sandbox::shows_nothing_hidden(dir, hidden, never))
        .collect()
}

/// Shows the person's Rust toolchain to a box whose `HOME` is `temp`: `RUSTUP_HOME` for its
/// environment and, when the person's cargo `bin` isn't on `PATH` (so their cargo looks there
/// first), its `cargo-*` subcommands in the job's own cargo home, where the box's cargo looks first
/// ([`link_cargo_subcommands`]). When it is on `PATH`, the box's `PATH`, the person's, decides as
/// theirs does; and so it does when another cargo comes first on `PATH` (Homebrew's), whose own
/// subcommands go with it: a rustup clippy beside it builds with another rustc (E0514).
pub fn show_rust(temp: &Path) -> Vec<(&'static str, OsString)> {
    let [rustup, cargo_bin] = rust_homes();
    if let Some(from) = cargo_bin {
        let path = tools_path();
        let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        link_cargo_subcommands(&from, temp, links_cargo_subcommands(&dirs, &from));
    }
    rustup
        .map(|dir| ("RUSTUP_HOME", dir.into_os_string()))
        .into_iter()
        .collect()
}

/// Whether the box's cargo needs the person's subcommands linked: only when `PATH` (`dirs`) holds
/// neither their cargo `bin` (`from`) nor another folder with a cargo of its own.
fn links_cargo_subcommands(dirs: &[PathBuf], from: &Path) -> bool {
    !dirs
        .iter()
        .any(|dir| dir == from || dir.join("cargo").is_file())
}

/// A link to each `cargo-*` in `from` in `temp/.cargo/bin`, or, without `link`, none (one an
/// earlier run made is taken out). `temp` is the job's, so nothing is written through a link the
/// job left: `.cargo` and its `bin` must be real folders, and a link of the same name is replaced,
/// not followed.
fn link_cargo_subcommands(from: &Path, temp: &Path, link: bool) {
    use std::os::unix::fs::DirBuilderExt;
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    let bin = temp.join(".cargo/bin");
    let real_dir =
        |p: &Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !m.is_symlink());
    for dir in [temp.join(".cargo"), bin.clone()] {
        let _ = std::fs::DirBuilder::new().mode(0o700).create(&dir);
        if !real_dir(&dir) {
            return;
        }
    }
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("cargo-") {
            continue;
        }
        let at = bin.join(&name);
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.is_symlink()) {
            let _ = std::fs::remove_file(&at);
        }
        if link {
            let _ = std::os::unix::fs::symlink(entry.path(), &at);
        }
    }
}

/// The `PATH` processes get: obid's own, then what the login shell adds (AGENTS.md, "PATH"), then
/// where CLIs are installed that only an interactive shell (`.zshrc`) adds ([`usual_folders`]), so
/// a tool installed for the person is found even when obid was started with a short `PATH`. The
/// first two are read once (the shell takes a moment); the usual folders are looked at every time,
/// so a tool installed after obid started (Claude Code's installer makes `~/.local/bin` on a fresh
/// Mac) is found without restarting it.
pub fn tools_path() -> OsString {
    static BASE: OnceLock<Vec<PathBuf>> = OnceLock::new();
    let base = BASE.get_or_init(|| {
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        for dir in login_shell_path() {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        dirs
    });
    let home = std::env::var_os("HOME").map(PathBuf::from);
    path_with_usual(base, home.as_deref())
}

/// `base`, then the [`usual_folders`] of `home` it doesn't have yet. Absolute folders only: a
/// relative one would be looked up in whatever folder a run starts in, a job's worktree.
fn path_with_usual(base: &[PathBuf], home: Option<&Path>) -> OsString {
    let mut dirs = base.to_vec();
    dirs.retain(|dir| dir.is_absolute());
    for dir in home.map(usual_folders).unwrap_or_default() {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    if dirs.is_empty() {
        dirs = ["/usr/local/bin", "/usr/bin", "/bin"]
            .map(PathBuf::from)
            .to_vec();
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into())
}

/// Where the person's CLIs usually are when their `PATH` is set in `.zshrc`, which a login shell
/// that isn't interactive never reads: `~/.local/bin` (Claude Code's installer), `~/.bun/bin`,
/// `~/.cargo/bin` (`cargo install`'s: the check's `oby-check-blok`), nvm's default node (npm's `-g`
/// installs) and `/opt/homebrew/bin`. Only those that are folders.
fn usual_folders(home: &Path) -> Vec<PathBuf> {
    [
        Some(home.join(".local/bin")),
        Some(home.join(".bun/bin")),
        Some(home.join(".cargo/bin")),
        nvm_default_bin(home),
        Some(PathBuf::from("/opt/homebrew/bin")),
    ]
    .into_iter()
    .flatten()
    .filter(|dir| dir.is_dir())
    .collect()
}

/// nvm's default node's `bin`: the newest `~/.nvm/versions/node/v*` that `~/.nvm/alias/default`
/// names (`22`, `22.11`, `v22.11.0`). ponytail: any other alias (`node`, `lts/*`, `lts/jod`) takes
/// the newest installed; follow nvm's alias files if someone's default is an older LTS.
fn nvm_default_bin(home: &Path) -> Option<PathBuf> {
    let nvm = home.join(".nvm");
    let alias = std::fs::read_to_string(nvm.join("alias/default")).unwrap_or_default();
    let wanted = alias.trim().trim_start_matches('v');
    let numbered = wanted.starts_with(|c: char| c.is_ascii_digit());
    let versions = nvm.join("versions/node");
    std::fs::read_dir(&versions)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let number = name.strip_prefix('v')?;
            let parts: Vec<u64> = number
                .split('.')
                .map(|p| p.parse().ok())
                .collect::<Option<_>>()?;
            let matches =
                !numbered || number == wanted || number.starts_with(&format!("{wanted}."));
            matches.then_some((parts, name))
        })
        .max()
        .map(|(_, name)| versions.join(name).join("bin"))
}

/// The login shell's `PATH` ([`login_env`]'s): empty when it can't be read.
fn login_shell_path() -> Vec<PathBuf> {
    login_env()
        .unwrap_or_default()
        .into_iter()
        .find(|(name, _)| name == "PATH")
        .map(|(_, path)| {
            std::env::split_paths(&path)
                .filter(|p| p.is_absolute())
                .collect()
        })
        .unwrap_or_default()
}

/// The files a login shell reads, as `$HOME` joins them (an absolute one stays itself).
const SHELL_FILES: &[&str] = &[
    ".zshenv",
    ".zprofile",
    ".zlogin",
    ".profile",
    ".bash_profile",
    ".bash_login",
    ".config/fish/config.fish",
    "/etc/zshenv",
    "/etc/zprofile",
    "/etc/profile",
];

/// The owner's login shell's environment, as a terminal starts with it (LANES-4): the shell from
/// the password database (it can differ from `$SHELL`; `OBID_LOGIN_SHELL` in obid's tests), run as
/// `<shell> -l -c '/usr/bin/env -0'` with only what login(1) gives it, none of obid's own. Read
/// again once one of [`SHELL_FILES`] changes; `None` when the shell didn't answer within 10 seconds.
pub fn login_env() -> Option<Vec<(OsString, OsString)>> {
    type Read = (
        Vec<Option<std::time::SystemTime>>,
        Option<Vec<(OsString, OsString)>>,
    );
    static READ: std::sync::Mutex<Option<Read>> = std::sync::Mutex::new(None);
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let stamps: Vec<_> = SHELL_FILES
        .iter()
        .map(|file| {
            std::fs::metadata(home.join(file))
                .and_then(|m| m.modified())
                .ok()
        })
        .collect();
    let mut read = READ.lock().unwrap_or_else(|e| e.into_inner());
    match &*read {
        Some((at, env)) if *at == stamps => env.clone(),
        _ => read.insert((stamps, read_login_env())).1.clone(),
    }
}

fn read_login_env() -> Option<Vec<(OsString, OsString)>> {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::process::CommandExt;
    let shell = std::env::var_os("OBID_LOGIN_SHELL")
        .map(PathBuf::from)
        .or_else(|| {
            Some(
                nix::unistd::User::from_uid(nix::unistd::getuid())
                    .ok()??
                    .shell,
            )
        })
        .filter(|shell| shell.is_absolute())?;
    let mut child = std::process::Command::new(&shell)
        .args(["-l", "-c", "/usr/bin/env -0"])
        .env_clear()
        .envs(
            ["HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "TERM"]
                .into_iter()
                .filter_map(|n| Some((n, std::env::var_os(n)?))),
        )
        .env("SHELL", &shell)
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    // Read as it writes (an environment past the pipe's buffer would never let the shell end), until
    // nothing holds the pipe; then whatever its group left goes, the shell not yet reaped.
    let mut stdout = child.stdout.take()?;
    let (sent, read) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut out);
        let _ = sent.send(out);
    });
    let out = read.recv_timeout(Duration::from_secs(10));
    let _ = killpg(
        Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGKILL,
    );
    let _ = child.wait();
    let out = out.ok()?;
    Some(
        out.split(|b| *b == 0)
            .filter_map(|entry| {
                let at = entry.iter().position(|b| *b == b'=')?;
                let value = OsString::from_vec(entry[at + 1..].to_vec());
                Some((OsString::from_vec(entry[..at].to_vec()), value))
            })
            .collect(),
    )
}

/// `owners` ([`login_env`]'s) under `env`, an own run's (LANES-4): every variable `env` doesn't set,
/// but what pushes or signs in to a code host ([`agent_withheld`]'s), git's, obid's and Codex's own
/// (`CODEX_*`), the shell's own bookkeeping, and Claude Code's own but the owner's gateway
/// ([`claude_gateway`]).
pub fn with_owners(env: &mut Vec<(OsString, OsString)>, owners: &[(OsString, OsString)]) {
    for (name, value) in owners {
        let n = name.to_string_lossy();
        let withheld = if n.starts_with("CLAUDE") || n.starts_with("ANTHROPIC_") {
            !claude_gateway(&n)
        } else {
            agent_withheld(&n)
                || ["GIT_", "OBY_", "OBID_", "CODEX_"]
                    .iter()
                    .any(|p| n.starts_with(p))
                || ["PWD", "OLDPWD", "SHLVL", "_"].contains(&&*n)
        };
        if !withheld && !env.iter().any(|(set, _)| set == name) {
            env.push((name.clone(), value.clone()));
        }
    }
}

/// Whether an own run takes `name` from the owner (their login shell, their user settings' `env`)
/// as Claude Code's (the owner's decision LANES-4a, "Allow-list"): any `ANTHROPIC_*` (its gateway,
/// its key, its models), or one of [`CLAUDE_GATEWAY`]. No other `CLAUDE*` name: they name its
/// folders, settings and policy sources, permissions, extra directories, its sandbox and its
/// session, any of which could outrank obid's `--settings` or widen the run (LANES-5).
pub fn claude_gateway(name: &str) -> bool {
    name.starts_with("ANTHROPIC_") || CLAUDE_GATEWAY.contains(&name)
}

/// The `CLAUDE_CODE_*` switches an own run takes from the owner (LANES-4a): which provider Claude
/// Code calls (2.1.302's Bedrock, Vertex, Foundry, Anthropic on AWS or Google Cloud, Mantle), how
/// it signs in there, and its models and output size. Nothing that names a path, a settings
/// source, permissions, directories, the sandbox, hooks, MCP or a session.
pub const CLAUDE_GATEWAY: [&str; 15] = [
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    "CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD",
    "CLAUDE_CODE_USE_MANTLE",
    "CLAUDE_CODE_SKIP_BEDROCK_AUTH",
    "CLAUDE_CODE_SKIP_VERTEX_AUTH",
    "CLAUDE_CODE_SKIP_FOUNDRY_AUTH",
    "CLAUDE_CODE_SKIP_ANTHROPIC_AWS_AUTH",
    "CLAUDE_CODE_SKIP_ANTHROPIC_GOOGLE_CLOUD_AUTH",
    "CLAUDE_CODE_SKIP_MANTLE_AUTH",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
    "CLAUDE_CODE_SUBAGENT_MODEL",
    "CLAUDE_CODE_EFFORT_LEVEL",
];

/// `bin` as an absolute path: itself when it names one (a relative one from obid's own folder,
/// never the worktree a run starts in), else found on [`tools_path`].
pub fn resolve_bin(bin: &str) -> Option<PathBuf> {
    if bin.contains('/') {
        let path = std::path::absolute(bin).ok()?;
        return crate::sandbox::is_executable(&path).then_some(path);
    }
    std::env::split_paths(&tools_path())
        .map(|dir| dir.join(bin))
        .find(|candidate| crate::sandbox::is_executable(candidate))
}

/// Runs `argv` in a session of its own, in place of this process: `setsid`, then `exec`. What
/// `obid __session -- <argv>` does ([`crate::sandbox::SESSION`]), so that a sandboxed process has
/// no controlling terminal, and its process group (its id) holds everything it starts. Returns only
/// when it couldn't: then nothing ran. With [`NICE`] in its environment, it runs at that nice, and
/// the program doesn't see it.
pub fn exec_in_own_session(argv: &[OsString]) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    let Some((program, args)) = argv.split_first() else {
        return std::io::Error::other("nothing to run");
    };
    if let Err(errno) = nix::unistd::setsid() {
        return std::io::Error::other(format!("can't start a session of its own: {errno}"));
    }
    if let Some(nice) = std::env::var(NICE).ok().and_then(|n| n.parse().ok()) {
        // Raising one's own nice needs no privilege; one obid can't set leaves it as it is.
        let _ = rustix::process::setpriority_process(None, nice);
    }
    std::process::Command::new(program)
        .args(args)
        .env_remove(NICE)
        .exec()
}

/// What tells `obid __session` the nice its program runs at: a job's agent's (PUBLISHING-30).
pub const NICE: &str = "OBY_SESSION_NICE";

/// Sends `signal` to the process group `pgid`; a group that's gone is fine.
pub fn signal_group(pgid: u32, signal: nix::sys::signal::Signal) {
    if pgid > 1 {
        let _ = killpg(Pid::from_raw(pgid as i32), signal);
    }
}

/// Ends what an own run left running, `setsid` too (LANES-16): each process of this user's, not a
/// zombie, whose sandbox may write the run's `tag`, `writes` and `room`, but not fresh files of
/// this call's in `denied` and beside `writes`, proven before each stop. Err when it can't prove.
// ponytail: an id freed between check and stop could be reused; needs macOS's ids to wrap.
#[cfg(target_os = "macos")]
pub fn end_sandboxed(
    writes: &Path,
    room: &Path,
    denied: &Path,
    tag: &Path,
) -> Result<usize, String> {
    use libproc::bsd_info::BSDInfo;
    use libproc::processes::{ProcFilter, pids_by_type};
    use nix::sys::signal::Signal;
    let name = writes.file_name().ok_or("no folder")?.to_string_lossy();
    let call = uuid::Uuid::new_v4().simple();
    let files = [
        denied.join(format!(".oby-reap-{call}")),
        writes.with_file_name(format!("{name}.oby-reap-{call}")),
    ];
    let made = files.iter().try_for_each(|file| {
        fresh(file).map_err(|e| format!("obid can't make {}: {e}", file.display()))
    });
    let me = std::process::id();
    let folder = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
    let one = |m: std::fs::Metadata| m.is_file() && std::os::unix::fs::MetadataExt::nlink(&m) == 1;
    let proven = || {
        [writes, room]
            .iter()
            .all(|f| folder(f) && may_write(me, f) == Some(true))
            && std::fs::symlink_metadata(tag).is_ok_and(one)
            && may_write(me, tag) == Some(true)
            && files.iter().all(|file| {
                std::fs::symlink_metadata(file).is_ok_and(|m| m.is_file())
                    && may_write(me, file) == Some(true)
            })
    };
    let zombie = |pid: u32| {
        libproc::proc_pid::pidinfo::<BSDInfo>(pid as i32, 0).is_ok_and(|info| info.pbi_status == 5)
    };
    let theirs = |pid: u32| {
        !zombie(pid)
            && [tag, writes, room]
                .iter()
                .all(|f| may_write(pid, f) == Some(true))
            && files.iter().all(|file| may_write(pid, file) == Some(false))
    };
    let uid = nix::unistd::getuid().as_raw();
    let list = || pids_by_type(ProcFilter::ByUID { uid }).map_err(|e| e.to_string());
    let signal = |pid: u32, last: bool| {
        let sig = [Signal::SIGSTOP, Signal::SIGKILL][usize::from(last)];
        nix::sys::signal::kill(Pid::from_raw(pid as i32), sig).is_ok()
    };
    let swept = made.and_then(|()| sweep(&list, &theirs, &proven, &signal));
    for file in &files {
        let _ = std::fs::remove_file(file);
    }
    swept
}

/// `path` as a new empty file (0600), never through what was there.
pub(crate) fn fresh(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create_new(true).mode(0o600);
    open.custom_flags(libc::O_NOFOLLOW).open(path).map(drop)
}

/// [`end_sandboxed`]'s rule: `theirs` stopped while `proven`, killed, until a sweep finds none.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn sweep(
    list: &dyn Fn() -> Result<Vec<u32>, String>,
    theirs: &dyn Fn(u32) -> bool,
    proven: &dyn Fn() -> bool,
    signal: &dyn Fn(u32, bool) -> bool,
) -> Result<usize, String> {
    let me = std::process::id();
    let unproven = || Err("obid can't prove which processes are the run's".to_string());
    if !proven() {
        return unproven();
    }
    let mut ended = 0;
    for _ in 0..SWEEPS {
        let found = list()?
            .into_iter()
            .filter(|&pid| pid > 1 && pid != me && theirs(pid));
        let found: Vec<u32> = found.collect();
        if found.is_empty() {
            return if proven() { Ok(ended) } else { unproven() };
        }
        let mut stopped = Vec::new();
        // What it stopped was proven: killed even when a later proof fails.
        let mut lost = false;
        for pid in found {
            lost = !proven();
            if lost {
                break;
            }
            if theirs(pid) && signal(pid, false) {
                stopped.push(pid);
            }
        }
        ended += stopped.into_iter().filter(|&pid| signal(pid, true)).count();
        if lost {
            return unproven();
        }
    }
    Err(format!(
        "the run's processes still ran after {SWEEPS} sweeps"
    ))
}

const SWEEPS: usize = 10;

/// Whether `pid`'s sandbox may write `path` (`sandbox_check`): obid's one `unsafe` (UNSAFE-1).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn may_write(pid: u32, path: &Path) -> Option<bool> {
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn sandbox_check(
            pid: libc::pid_t,
            op: *const libc::c_char,
            kind: libc::c_int,
            ...
        ) -> libc::c_int;
    }
    // FILTER_PATH | CHECK_NO_REPORT | CHECK_NOFOLLOW (the path as named, never a link's target).
    const PATH_NO_REPORT: libc::c_int = 1 | 0x4000_0000 | 0x1000_0000;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let pid = libc::pid_t::try_from(pid).ok()?;
    // SAFETY: NUL-terminated strings that outlive the call; the path is the one variadic arg.
    let answer = unsafe {
        sandbox_check(
            pid,
            c"file-write-data".as_ptr(),
            PATH_NO_REPORT,
            path.as_ptr(),
        )
    };
    match answer {
        0 => Some(true),
        1 => Some(false),
        _ => None,
    }
}

/// [`end_sandboxed`]: macOS only (Linux and an obid that crashed are later work: lanes.md).
#[cfg(not(target_os = "macos"))]
pub fn end_sandboxed(
    _writes: &Path,
    _room: &Path,
    _denied: &Path,
    _tag: &Path,
) -> Result<usize, String> {
    Ok(0)
}

/// SIGTERM to the group now, SIGKILL `grace` later: everything it started ends.
pub async fn end_group(pgid: u32, grace: Duration) {
    signal_group(pgid, nix::sys::signal::Signal::SIGTERM);
    tokio::time::sleep(grace).await;
    signal_group(pgid, nix::sys::signal::Signal::SIGKILL);
}

/// `command`'s exit and stdout once it ends, or none when it didn't start or end within `limit`
/// (it's killed then): for the short commands obid asks while it starts (`security`), so none can
/// hang it. Its stdout is read meanwhile, so a long answer can't stall it.
pub fn output_within(
    command: &mut std::process::Command,
    limit: Duration,
) -> Option<std::process::Output> {
    let mut child = command.stdout(std::process::Stdio::piped()).spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let read = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut out);
        out
    });
    let give_up_at = std::time::Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < give_up_at => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                tracing::warn!(
                    "{} didn't answer within {} s",
                    command.get_program().to_string_lossy(),
                    limit.as_secs()
                );
                return None;
            }
        }
    };
    Some(std::process::Output {
        status,
        stdout: read.join().ok()?,
        stderr: Vec::new(),
    })
}

/// "exit status 1", or "killed by signal 9".
pub fn describe(status: std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("killed by signal {signal}");
        }
    }
    match status.code() {
        Some(code) => format!("exit status {code}"),
        None => "ended".into(),
    }
}

#[cfg(test)]
mod usual_folders_tests {
    use super::*;

    /// A CLI the login shell's `PATH` misses is looked for where installers put it, nvm's default
    /// node among them: the newest version its alias names.
    #[test]
    fn clis_set_up_in_zshrc_are_found_where_installers_put_them() {
        let home = std::env::temp_dir().join(format!("obid-usual-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for dir in [
            ".local/bin",
            ".cargo/bin",
            ".nvm/versions/node/v20.1.0/bin",
            ".nvm/versions/node/v22.3.0/bin",
            ".nvm/versions/node/v22.11.0/bin",
            ".nvm/versions/node/v23.0.0/bin",
            ".nvm/alias",
        ] {
            std::fs::create_dir_all(home.join(dir)).unwrap();
        }
        std::fs::write(home.join(".nvm/alias/default"), "22\n").unwrap();
        let found = usual_folders(&home);
        assert_eq!(found[0], home.join(".local/bin"));
        // `cargo install`'s, where the check's oby-check-blok is (BLOK-1).
        assert_eq!(found[1], home.join(".cargo/bin"));
        assert_eq!(found[2], home.join(".nvm/versions/node/v22.11.0/bin"));
        // No ~/.bun/bin here: only folders that are there.
        assert!(!found.contains(&home.join(".bun/bin")));
        for (alias, version) in [("v22.3.0", "v22.3.0"), ("22.1", ""), ("lts/*", "v23.0.0")] {
            std::fs::write(home.join(".nvm/alias/default"), alias).unwrap();
            let bin = nvm_default_bin(&home);
            let wanted = (!version.is_empty())
                .then(|| home.join(".nvm/versions/node").join(version).join("bin"));
            assert_eq!(bin, wanted, "{alias}");
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A folder an installer makes after obid started is on the next `PATH`: a tool installed
    /// during first launch is seen without a restart.
    #[test]
    fn a_folder_made_after_start_is_on_the_next_path() {
        let home = std::env::temp_dir().join(format!("obid-later-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let base = [PathBuf::from("/usr/bin")];
        let local = home.join(".local/bin");
        let has = |path: OsString| std::env::split_paths(&path).any(|dir| dir == local);
        assert!(!has(path_with_usual(&base, Some(&home))));
        std::fs::create_dir_all(&local).unwrap();
        assert!(has(path_with_usual(&base, Some(&home))));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// LANES-18: a tool named by a relative path, or found through a relative `PATH` folder, is
    /// the one in obid's own folder, never one of the same name in the worktree a run starts in
    /// (an unboxed run would be the repository's own program).
    #[test]
    fn a_relative_tool_is_never_the_worktrees() {
        use std::os::unix::fs::PermissionsExt;
        let named = "tests/bin-broken-gitleaks/gitleaks";
        let worktree = std::env::temp_dir().join(format!("obid-relbin-{}", std::process::id()));
        let twin = worktree.join(named);
        std::fs::create_dir_all(twin.parent().unwrap()).unwrap();
        std::fs::write(&twin, "#!/bin/sh\necho the repository's\n").unwrap();
        std::fs::set_permissions(&twin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bin = resolve_bin(named).unwrap();
        assert_eq!(bin, std::env::current_dir().unwrap().join(named));
        let ran = std::process::Command::new(&bin)
            .current_dir(&worktree)
            .output()
            .unwrap();
        assert!(!String::from_utf8_lossy(&ran.stdout).contains("the repository's"));
        let path = path_with_usual(&["bin".into(), ".".into(), "/usr/bin".into()], None);
        assert_eq!(path, "/usr/bin");
        let _ = std::fs::remove_dir_all(&worktree);
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;

    /// A build gets one job per 1.5 GiB of the room left it, at most half the cores, at least one;
    /// every name says the same number.
    #[test]
    fn builds_are_as_wide_as_memory_holds() {
        let gib = 1024 * 1024 * 1024;
        let most =
            (std::thread::available_parallelism().map_or(2, std::num::NonZero::get) / 2).max(1);
        assert_eq!(build_jobs(0), 1);
        assert_eq!(build_jobs(gib), 1);
        assert_eq!(build_jobs(3 * gib), 2.min(most));
        assert_eq!(build_jobs(6 * gib), 4.min(most));
        assert_eq!(build_jobs(1024 * gib), most);
        assert_eq!(
            build_env(3),
            vec![
                ("CARGO_BUILD_JOBS", OsString::from("3")),
                ("RUST_TEST_THREADS", OsString::from("3")),
                ("OBY_BUILD_JOBS", OsString::from("3")),
            ]
        );
    }
}

#[cfg(test)]
mod claude_tmpdir_tests {
    use super::*;

    /// #1789: no `TMPDIR` puts someone else's folder, or the worktree, first on a box's `PATH`; a
    /// private one gets its `ps`, again when it was left empty.
    #[cfg(target_os = "macos")]
    #[test]
    fn own_ps_is_only_in_a_private_folder_on_an_absolute_path() {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(own_ps_folder(Path::new("")), None);
        let temp = std::env::temp_dir().join(format!("obid-own-ps-{}", std::process::id()));
        let uid = nix::unistd::getuid();
        let dir = temp.join(format!("oby-tools-{uid}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(own_ps_folder(&temp), None, "open to others");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(dir.join("ps"), "").unwrap();
        assert_eq!(own_ps_folder(&temp).as_deref(), Some(dir.as_path()));
        assert!(
            std::fs::metadata(dir.join("ps")).unwrap().len() > 0,
            "an empty one is made again"
        );
        std::fs::remove_dir_all(&temp).unwrap();
    }

    /// #1789: a box runs `ps`, which macOS refuses as `/bin/ps` (setuid), and sees its own process.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_box_runs_ps() {
        // Inside a sandbox already (a job's own check of this repository), none can be applied.
        let boxed = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
            .status();
        if !boxed.is_ok_and(|s| s.success()) {
            return;
        }
        let env = sandbox_env(OsStr::new("/usr/bin:/bin"), Path::new("/tmp/job-temp"), &[]);
        let ran = |path: Option<&OsString>| {
            let mut command = std::process::Command::new("/usr/bin/sandbox-exec");
            command
                .args([
                    "-p",
                    "(version 1)(allow default)",
                    "/bin/sh",
                    "-c",
                    "ps -o pid= -p $$",
                ])
                .env_clear();
            if let Some(path) = path {
                command.env("PATH", path);
            }
            command.output().unwrap()
        };
        let path = env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v);
        let out = ran(path);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!String::from_utf8_lossy(&out.stdout).trim().is_empty());
        // what it's for: without it, the system's own is refused
        assert!(!ran(Some(&"/usr/bin:/bin".into())).status.success());
    }

    /// PERMISSIONS-14: nothing in the box reads a git setting but the repository's: not a
    /// `.gitconfig` its own commands wrote in its `HOME`, nor the system's.
    #[test]
    fn permissions_14_a_box_reads_no_git_settings_but_the_repositorys() {
        let env = sandbox_env(OsStr::new("/usr/bin"), Path::new("/tmp/job-temp"), &[]);
        let get = |name: &str| {
            let values: Vec<_> = env
                .iter()
                .filter(|(k, _)| k == name)
                .map(|(_, v)| v)
                .collect();
            assert_eq!(values.len(), 1, "{name}: {values:?}");
            values[0].to_string_lossy().into_owned()
        };
        assert_eq!(get("GIT_CONFIG_GLOBAL"), "/dev/null");
        assert_eq!(get("GIT_CONFIG_NOSYSTEM"), "1");
    }

    /// Claude Code's Bash folder is the run's own temporary folder when its launch says so; no
    /// other CLAUDE_CODE_* gets in.
    #[test]
    fn claude_code_makes_its_bash_folder_in_the_jobs_temp() {
        let temp = Path::new("/tmp/job-temp");
        let env = sandbox_env(
            OsStr::new("/usr/bin"),
            temp,
            &[
                (CLAUDE_TMPDIR, temp.into()),
                (CLAUDE_TODO_TOOLS, "1".into()),
                ("CLAUDE_CODE_OTHER", "x".into()),
            ],
        );
        let names: Vec<_> = env
            .iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&CLAUDE_TMPDIR.to_owned()), "{names:?}");
        assert!(names.contains(&CLAUDE_TODO_TOOLS.to_owned()), "{names:?}");
        assert!(
            !names.contains(&"CLAUDE_CODE_OTHER".to_owned()),
            "{names:?}"
        );
        assert!(
            sandbox_env(OsStr::new("/usr/bin"), temp, &[])
                .iter()
                .all(|(k, _)| k != CLAUDE_TMPDIR)
        );
    }
    /// A job's cargo home gets links to the person's `cargo-*` (only those), a link the job left
    /// at one's name is replaced, they go when they no longer apply, and nothing is written
    /// through a link the job put in place of `.cargo` or its `bin`.
    #[test]
    fn one_toolchain_homebrews_cargo_on_path_gets_no_rustup_subcommands() {
        let root = std::env::temp_dir().join(format!("obid-one-rust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (brew, from, empty) = (root.join("brew"), root.join("from"), root.join("empty"));
        for dir in [&brew, &from, &empty] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(brew.join("cargo"), "").unwrap();
        assert!(!links_cargo_subcommands(
            &[brew.clone(), empty.clone()],
            &from
        ));
        assert!(!links_cargo_subcommands(
            &[empty.clone(), from.clone()],
            &from
        ));
        assert!(links_cargo_subcommands(std::slice::from_ref(&empty), &from));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_persons_cargo_subcommands_are_linked_never_through_a_link() {
        let root = std::env::temp_dir().join(format!("obid-cargo-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (from, temp, outside) = (root.join("from"), root.join("temp"), root.join("outside"));
        for dir in [&from, &temp, &outside] {
            std::fs::create_dir_all(dir).unwrap();
        }
        for name in ["cargo-clippy", "cargo-fmt", "cargo", "rustc"] {
            std::fs::write(from.join(name), "").unwrap();
        }
        std::fs::create_dir_all(temp.join(".cargo/bin")).unwrap();
        std::os::unix::fs::symlink(&outside, temp.join(".cargo/bin/cargo-fmt")).unwrap();
        link_cargo_subcommands(&from, &temp, true);
        let bin = temp.join(".cargo/bin");
        let mut names: Vec<String> = std::fs::read_dir(&bin)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["cargo-clippy", "cargo-fmt"]);
        for name in &names {
            assert_eq!(std::fs::read_link(bin.join(name)).unwrap(), from.join(name));
        }
        // Once the person's cargo `bin` is on `PATH`, the links go.
        link_cargo_subcommands(&from, &temp, false);
        assert_eq!(std::fs::read_dir(&bin).unwrap().count(), 0);

        // The job's `.cargo`, then its `bin`, a link out: nothing lands where they lead.
        for planted in [temp.join(".cargo"), temp.join(".cargo/bin")] {
            std::fs::remove_dir_all(temp.join(".cargo")).unwrap();
            if planted.ends_with("bin") {
                std::fs::create_dir(temp.join(".cargo")).unwrap();
            }
            std::os::unix::fs::symlink(&outside, &planted).unwrap();
            link_cargo_subcommands(&from, &temp, true);
            assert_eq!(
                std::fs::read_dir(&outside).unwrap().count(),
                0,
                "{planted:?}"
            );
            let _ = std::fs::remove_file(&planted);
            std::fs::create_dir_all(temp.join(".cargo")).unwrap();
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// macOS's /usr/bin/git is xcrun's shim, whose cache goes in the user's /var/folders, which the
    /// sandbox doesn't let a job write: every git call printed "couldn't create cache file". Its
    /// cache is in the job's temporary folder instead.
    #[test]
    fn xcrun_keeps_its_cache_in_the_jobs_temp() {
        let temp = Path::new("/tmp/job-temp");
        let env = sandbox_env(OsStr::new("/usr/bin"), temp, &[]);
        assert!(
            env.iter()
                .any(|(k, v)| k == "xcrun_db" && Path::new(v) == temp.join("xcrun_db")),
            "{env:?}"
        );
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// What a test started, killed when it's dropped, a failing assertion's unwinding included,
    /// so a test that fails never leaves processes behind: `children`, and every process id
    /// listed in `file` (one a line: what they started, detached).
    pub(crate) struct Started {
        pub(crate) children: Vec<std::process::Child>,
        pub(crate) file: Option<PathBuf>,
    }

    impl Drop for Started {
        fn drop(&mut self) {
            // Its own children first, a spawner among them, so none adds to the list meanwhile.
            for child in &mut self.children {
                let _ = child.kill();
                let _ = child.wait();
            }
            let listed = self
                .file
                .as_ref()
                .and_then(|f| std::fs::read_to_string(f).ok());
            let listed = listed.unwrap_or_default();
            for pid in listed.lines().filter_map(|l| l.trim().parse::<i32>().ok()) {
                let pid = Pid::from_raw(pid);
                let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
            }
        }
    }

    /// LANES-5 with LANES-4: the owner's shell can't move an own run's Claude Code to another
    /// folder or Keychain item than obid names (none for the default account), nor change what obid
    /// sets (`HOME`); its gateway still reaches the run.
    #[test]
    fn lanes_5_the_owners_shell_keeps_the_accounts_folder() {
        let os = |text: &str| OsString::from(text);
        let mut env = vec![(os("HOME"), os("/Users/p"))];
        let owners = [
            (os("CLAUDE_CONFIG_DIR"), os("/Users/p/.claude-other")),
            (
                os("CLAUDE_SECURESTORAGE_CONFIG_DIR"),
                os("/Users/p/.claude-other"),
            ),
            (os("HOME"), os("/elsewhere")),
            (os("ANTHROPIC_BASE_URL"), os("https://gateway.example")),
        ];
        with_owners(&mut env, &owners);
        assert_eq!(
            env,
            vec![
                (os("HOME"), os("/Users/p")),
                (os("ANTHROPIC_BASE_URL"), os("https://gateway.example")),
            ]
        );
    }

    /// LANES-5 with LANES-4a ("Allow-list"): of Claude Code's own variables, the owner's shell gives
    /// an own run only `ANTHROPIC_*` and the gateway switches ([`CLAUDE_GATEWAY`]); a settings or
    /// policy source that outranks obid's `--settings`, extra session folders and allow rules, and
    /// any other `CLAUDE*` name never reach it.
    #[test]
    fn lanes_5_the_owners_shell_gives_no_settings_source() {
        let os = |text: &str| OsString::from(text);
        let mut env = Vec::new();
        let mut owners: Vec<(OsString, OsString)> = [
            "CLAUDE_CODE_MANAGED_SETTINGS_PATH",
            "CLAUDE_CODE_REMOTE_SETTINGS_PATH",
            "CLAUDE_CODE_SANDBOXED",
            "CLAUDE_RELAUNCH_SESSION_ADD_DIRS",
            "CLAUDE_BG_SESSION_PERMISSION_RULES",
            "CLAUDE_CODE_SESSION_KIND",
            "CLAUDE_FOO",
            "CLAUDECODE",
        ]
        .iter()
        .map(|name| (os(name), os("/tmp/policy")))
        .collect();
        owners.push((os("CLAUDE_CODE_USE_BEDROCK"), os("1")));
        owners.push((os("ANTHROPIC_MODEL"), os("us.anthropic.claude")));
        with_owners(&mut env, &owners);
        assert_eq!(
            env,
            vec![
                (os("CLAUDE_CODE_USE_BEDROCK"), os("1")),
                (os("ANTHROPIC_MODEL"), os("us.anthropic.claude")),
            ]
        );
    }

    /// LANES-16 (docs/contracts/lanes.md): what an own run's commands left running ends with the
    /// run: a process in a Seatbelt sandbox that may write only the run's temporary folder, even
    /// one that left its group with `setsid`, is killed; the person's own unsandboxed process and
    /// another run's sandboxed one aren't.
    #[cfg(target_os = "macos")]
    #[test]
    fn lanes_16_an_own_runs_leftovers_end_with_it() {
        let root = std::env::temp_dir().join(format!("obid-lanes-16-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        // Each run's temporary folder, its room, its `login` folder, which its commands may not
        // write, and its tag, a file beside its temp folder only its own sandbox may write.
        let run = |name: &str| {
            let [temp, room, login] = ["temp", "room", "login"].map(|d| root.join(name).join(d));
            for dir in [&temp, &room, &login] {
                std::fs::create_dir_all(dir).unwrap();
            }
            let tag = root
                .join(name)
                .join(format!("temp.run-{}", uuid::Uuid::new_v4().simple()));
            std::fs::write(&tag, "").unwrap();
            (temp, room, login, tag)
        };
        let ((temp, room, login, tag), (other, other_room, _, other_tag)) =
            (run("this"), run("other"));
        // Short-lived, and every one killed when the test ends however it ends.
        let sleeper = "use POSIX; setsid(); exec q(sleep), 60";
        let pids = root.join("pids");
        let mut started = Started {
            children: Vec::new(),
            file: Some(pids.clone()),
        };
        let boxed = |writes: &[&Path], tags: &[&Path], program: &[&str]| {
            let mut profile = format!(
                "(version 1)(allow default)(deny file-write* (subpath \"{}\"))",
                root.display()
            );
            for path in writes {
                profile += &format!("(allow file-write* (subpath \"{}\"))", path.display());
            }
            for path in tags.iter().chain([&pids.as_path()]) {
                profile += &format!("(allow file-write* (literal \"{}\"))", path.display());
            }
            // A broad one may name the run's `login` folder last: it stays denied.
            profile += &format!("(deny file-write* (subpath \"{}\"))", login.display());
            std::process::Command::new("/usr/bin/sandbox-exec")
                .args(["-p", &profile])
                .args(program)
                .spawn()
                .unwrap()
        };
        let perl = ["/usr/bin/perl", "-e", sleeper];
        // 0: the run's, tagged.
        started
            .children
            .push(boxed(&[&temp, &room], &[&tag], &perl));
        // 1: another run's, with its own tag.
        started
            .children
            .push(boxed(&[&other, &other_room], &[&other_tag], &perl));
        // 2: the person's own, unsandboxed.
        let persons = std::process::Command::new(perl[0]).args(&perl[1..]).spawn();
        started.children.push(persons.unwrap());
        // 3: a sandbox of someone else's that opens a parent of the run's folders.
        let parent = temp.parent().unwrap();
        started.children.push(boxed(&[parent], &[], &perl));
        // 4: the run's, tagged, forking detached children, 400 at most, while it's being ended.
        let forks = format!(
            "i=0; while [ $i -lt 400 ]; do /usr/bin/perl -e '{sleeper}' & echo $! >> '{}'; i=$((i+1)); /bin/sleep 0.001; done",
            pids.display()
        );
        started
            .children
            .push(boxed(&[&temp, &room], &[&tag], &["/bin/sh", "-c", &forks]));
        // 5: a sandbox that matches the run's in everything but its tag (RUNTAG-1a).
        started.children.push(boxed(&[&temp, &room], &[], &perl));
        // 6: one that matches it too, but carries another run's tag.
        started
            .children
            .push(boxed(&[&temp, &room], &[&other_tag], &perl));
        // Mid-way through its forking.
        std::thread::sleep(std::time::Duration::from_millis(150));
        // What obid can't prove is the run's ends nothing: a folder or a tag that isn't there
        // (macOS says no one may write it, the person's own processes included).
        let gone = root.join("gone");
        assert!(end_sandboxed(&temp, &room, &gone, &tag).is_err());
        assert!(end_sandboxed(&gone, &room, &login, &tag).is_err());
        assert!(end_sandboxed(&temp, &gone, &login, &tag).is_err());
        assert!(end_sandboxed(&temp, &room, &login, &gone).is_err());
        // Nor does a tag with a second name (a hard link somewhere else).
        std::fs::hard_link(&tag, root.join("also")).unwrap();
        assert!(end_sandboxed(&temp, &room, &login, &tag).is_err());
        std::fs::remove_file(root.join("also")).unwrap();
        assert!(started.children[0].try_wait().unwrap().is_none());
        assert!(end_sandboxed(&temp, &room, &login, &tag).unwrap() >= 3);
        // Nothing of the run's is left, a child forked during the sweep included.
        assert_eq!(
            end_sandboxed(&temp, &room, &login, &tag),
            Ok(0),
            "a leftover escaped the sweep"
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
        for at in [0, 4] {
            let ended = started.children[at].try_wait().unwrap();
            assert!(ended.is_some(), "the run's leftover still runs ({at})");
        }
        for at in [1, 2, 3, 5, 6] {
            let ended = started.children[at].try_wait().unwrap();
            assert!(ended.is_none(), "a process not the run's ended ({at})");
        }
        drop(started);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-16: a run that swaps its tag for a link to a file of its temp folder, again and
    /// again while obid sweeps, never gets someone else's process ended: one whose sandbox opens
    /// that temp folder and room, but not the tag, is judged by the tag's own path, never where a
    /// link there leads.
    #[cfg(target_os = "macos")]
    #[test]
    fn lanes_16_a_swapped_tag_never_matches_another_sandbox() {
        let root = std::env::temp_dir().join(format!("obid-lanes-16s-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let [temp, room, login] = ["temp", "room", "login"].map(|d| root.join(d));
        for dir in [&temp, &room, &login] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let tag = root.join("temp.run-test");
        std::fs::write(&tag, "").unwrap();
        std::fs::write(temp.join("x"), "").unwrap();
        let profile = format!(
            "(version 1)(allow default)(deny file-write* (subpath \"{}\"))(allow file-write* (subpath \"{}\"))(allow file-write* (subpath \"{}\"))",
            root.display(),
            temp.display(),
            room.display()
        );
        let mut started = Started {
            children: Vec::new(),
            file: None,
        };
        let other = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &profile, "/usr/bin/perl", "-e"])
            .arg("use POSIX; setsid(); exec q(sleep), 60")
            .spawn()
            .unwrap();
        started.children.push(other);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let swapping = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let swapper = {
            let (swapping, tag, target) = (swapping.clone(), tag.clone(), temp.join("x"));
            std::thread::spawn(move || {
                // Renamed over it, so a tag is always there: a link, then a plain file again.
                let (link, file) = (tag.with_extension("link"), tag.with_extension("file"));
                while swapping.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = std::os::unix::fs::symlink(&target, &link);
                    let _ = std::fs::rename(&link, &tag);
                    let _ = std::fs::write(&file, "");
                    let _ = std::fs::rename(&file, &tag);
                }
            })
        };
        for _ in 0..300 {
            let _ = end_sandboxed(&temp, &room, &login, &tag);
        }
        swapping.store(false, std::sync::atomic::Ordering::Relaxed);
        swapper.join().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let ended = started.children[0].try_wait().unwrap();
        assert!(
            ended.is_none(),
            "a swapped tag got another sandbox's process ended"
        );
        drop(started);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LANES-16: the sweep ends a process only while obid can prove it's the run's, kills what it
    /// stopped, and says when it couldn't finish: a listing that failed, a proof lost part way
    /// (the run's folders deleted meanwhile), processes still there after every sweep. It never
    /// reads any of those as a clean sweep.
    #[test]
    fn lanes_16_a_sweep_proves_each_kill_and_says_when_it_cant() {
        use std::cell::{Cell, RefCell};
        let signalled = RefCell::new(Vec::new());
        let signal = |pid: u32, last: bool| {
            signalled.borrow_mut().push((pid, last));
            true
        };
        let theirs = |_: u32| true;
        let failed = || Err("no listing".to_string());
        assert!(sweep(&failed, &theirs, &|| true, &signal).is_err());
        assert!(signalled.borrow().is_empty());
        // A proof that doesn't hold to begin with signals nothing, even when nothing would match;
        // nor does one lost while it listed (the run's folders deleted meanwhile).
        let none = || Ok(Vec::new());
        assert!(sweep(&none, &theirs, &|| false, &signal).is_err());
        let checks = Cell::new(0);
        let then_lost = || {
            checks.set(checks.get() + 1);
            checks.get() == 1
        };
        assert!(sweep(&none, &theirs, &then_lost, &signal).is_err());
        assert!(signalled.borrow().is_empty());
        // The proof holds at the start and for the first process only: that one's stopped and
        // killed, the next isn't.
        let proofs = Cell::new(0);
        let proven = || {
            proofs.set(proofs.get() + 1);
            proofs.get() <= 2
        };
        let two = || Ok(vec![100, 101]);
        assert!(sweep(&two, &theirs, &proven, &signal).is_err());
        assert_eq!(*signalled.borrow(), [(100, false), (100, true)]);
        signalled.borrow_mut().clear();
        // Still there after every sweep.
        assert!(sweep(&two, &theirs, &|| true, &signal).is_err());
        assert_eq!(signalled.borrow().len(), 2 * 2 * SWEEPS);
        signalled.borrow_mut().clear();
        // Gone after the first: a clean sweep, and never obid itself, launchd or 0.
        let sweeps = Cell::new(0);
        let once = || {
            sweeps.set(sweeps.get() + 1);
            Ok(match sweeps.get() {
                1 => vec![0, 1, std::process::id(), 100],
                _ => vec![0, 1, std::process::id()],
            })
        };
        assert_eq!(sweep(&once, &theirs, &|| true, &signal), Ok(1));
        assert_eq!(*signalled.borrow(), [(100, false), (100, true)]);
        signalled.borrow_mut().clear();
        // What no longer carries the run's tag, or its sandbox, right before it's stopped isn't
        // stopped (its tag and sandbox are checked again then, with the proof).
        let looks = Cell::new(0);
        let then_not = |_: u32| {
            looks.set(looks.get() + 1);
            looks.get() == 1
        };
        let one = || Ok(vec![100]);
        assert!(sweep(&one, &then_not, &|| true, &signal).is_ok());
        assert!(signalled.borrow().is_empty(), "{:?}", signalled.borrow());
    }

    /// UNSAFE-1 (the owner's decision of 2026-11-11): `process::may_write` is obid's one function
    /// allowed `unsafe`. Every attribute in the core's sources is read whole, however it's laid out
    /// over lines, and only one allows, expects or warns of the lint, on `fn may_write` in this
    /// file; the workspace denies it, and no crate's manifest names it.
    #[test]
    fn unsafe_1_only_may_write_is_unsafe() {
        let lint = ["unsafe", "_code"].concat();
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let core = crates.parent().unwrap();
        let mut found = Vec::new();
        let mut folders = vec![crates.to_path_buf()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                } else if path.file_name().is_some_and(|n| n == "Cargo.toml") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    assert!(!text.contains(&lint), "{} names {lint}", path.display());
                    assert!(
                        text.contains("[lints]\nworkspace = true"),
                        "{}",
                        path.display()
                    );
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    let lines: Vec<&str> = text.lines().collect();
                    for (at, line) in lines.iter().enumerate().filter(|(_, l)| l.contains(&lint)) {
                        let next = lines.get(at + 1).copied().unwrap_or_default();
                        found.push((path.clone(), line.trim().to_owned(), next.to_owned()));
                    }
                }
            }
        }
        // The lint's name once, whatever surrounds it (a comment, a `cfg_attr`, lines apart):
        // `#[allow(..)]` on `fn may_write` in this file.
        assert_eq!(found.len(), 1, "{found:#?}");
        let (path, line, next) = &found[0];
        assert!(path.ends_with("obid/src/process.rs"), "{}", path.display());
        assert_eq!(line, &format!("#[allow({lint})]"));
        assert!(next.starts_with("fn may_write("), "{next}");
        // Nor do cargo's own settings lift it, and the workspace denies it.
        for config in [
            core.join(".cargo/config.toml"),
            core.join("../.cargo/config.toml"),
        ] {
            let text = std::fs::read_to_string(&config).unwrap_or_default();
            assert!(
                !text.contains(&lint) && !text.contains("cap-lints"),
                "{}",
                config.display()
            );
        }
        // ponytail: a lint named through a macro (`allow($l)` with `$l` = the lint) isn't seen.
        let workspace = std::fs::read_to_string(core.join("Cargo.toml")).unwrap();
        assert!(
            workspace.contains(&format!("{lint} = \"deny\"")),
            "the workspace's lint"
        );
    }

    /// LANES-2: a boxed agent's environment and an own one's alike give git no settings but the
    /// repository's and an empty credential helper, and nothing that pushes or signs in reaches an
    /// agent, while obid's own gh and Publish keep `SSH_AUTH_SOCK`.
    #[test]
    fn lanes_2_neither_lane_can_publish() {
        let root = std::env::temp_dir().join(format!("obid-lanes-2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (worktree, temp, login) = (root.join("worktree"), root.join("tmp"), root.join("login"));
        for dir in [&worktree, &temp, &root.join("account")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let spec = crate::sandbox::Spec {
            worktree: worktree.clone(),
            git_admin: Some(root.join("clone/.git/worktrees/job")),
            git_common: Some(root.join("clone/.git")),
            commits: true,
            temp: temp.clone(),
            accounts: Vec::new(),
            read_only: Vec::new(),
            hidden: Vec::new(),
            network: false,
            loopback: false,
            proxied: false,
            frozen: Vec::new(),
            fixed: Vec::new(),
        };
        let account = crate::config::Account {
            id: "work".into(),
            name: "Work".into(),
            config_dir: Some(root.join("account")),
            payer: None,
        };
        let own: Vec<(OsString, OsString)> = crate::jobs::own_sandbox_env(&account, &spec, &login)
            .unwrap()
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect();
        let boxed = sandbox_env(OsStr::new("/usr/bin"), &temp, &no_publish_env(&temp));
        for env in [&boxed, &own] {
            let get = |name: &str| {
                let values: Vec<_> = env.iter().filter(|(k, _)| k == name).collect();
                assert_eq!(values.len(), 1, "{name}: {values:?}");
                values[0].1.to_string_lossy().into_owned()
            };
            assert_eq!(get("GIT_CONFIG_GLOBAL"), "/dev/null");
            assert_eq!(get("GIT_CONFIG_NOSYSTEM"), "1");
            assert_eq!(
                get("GIT_CONFIG_PARAMETERS"),
                no_publish_git().to_string_lossy()
            );
            assert_eq!(get("GIT_TERMINAL_PROMPT"), "0");
            assert_eq!(
                get("GH_CONFIG_DIR"),
                temp.join(".config/gh-none").to_string_lossy()
            );
        }

        // And git itself: a clone whose settings push somewhere real, with a credential helper
        // that would answer, pushes only to obid's invalid URL and asks nobody.
        let repo = root.join("repo");
        let asked = root.join("asked");
        let git = |args: &[&str], env: &[(OsString, OsString)]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .env_clear()
                .env("PATH", "/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin")
                // (not the job's git folders, which this test never made, nor its PATH)
                .envs(
                    env.iter()
                        .filter(|(k, _)| {
                            !["PATH", "GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE"]
                                .contains(&&*k.to_string_lossy())
                        })
                        .map(|(k, v)| (k, v)),
                )
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap()
        };
        let plain = [
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ];
        std::fs::create_dir_all(&repo).unwrap();
        let helper = format!("!touch '{}'; echo password=t", asked.display());
        for args in [
            &["init", "-q"][..],
            &[
                "config",
                "remote.origin.url",
                "https://example.com/repo.git",
            ],
            &[
                "config",
                "remote.origin.pushurl",
                "https://example.com/repo.git",
            ],
            &["config", "credential.helper", &helper],
            &[
                "-c",
                "user.name=a",
                "-c",
                "user.email=a@b",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ],
        ] {
            assert!(git(args, &plain).status.success(), "{args:?}");
        }
        let said =
            |out: std::process::Output| String::from_utf8_lossy(&out.stdout).trim().to_owned();
        assert_eq!(
            said(git(&["config", "remote.origin.pushurl"], &plain)),
            "https://example.com/repo.git"
        );
        for env in [&boxed, &own] {
            assert_eq!(
                said(git(&["config", "remote.origin.pushurl"], env)),
                crate::git::NO_PUSH_URL
            );
            assert_eq!(
                said(git(
                    &["remote", "get-url", "--push", "--all", "origin"],
                    env
                )),
                crate::git::NO_PUSH_URL
            );
            let pushed = git(&["push", "origin", "HEAD:refs/heads/main"], env);
            let stderr = String::from_utf8_lossy(&pushed.stderr);
            assert!(!pushed.status.success(), "{stderr}");
            assert!(stderr.contains("obid-agents-cannot-push"), "{stderr}");
            assert!(!stderr.contains("example.com"), "{stderr}");
            assert!(!asked.exists(), "{stderr}");
        }
        for name in [
            "SSH_AUTH_SOCK",
            "GH_TOKEN",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
            "GIT_SSH",
            "GIT_SSH_COMMAND",
            "GH_HOST",
            "GITHUB_TOKEN",
            "GITHUB_APP_KEY",
            "ANTHROPIC_API_KEY",
        ] {
            assert!(agent_withheld(name), "{name}");
        }
        for name in ["PATH", "LANG", "GIT_AUTHOR_NAME"] {
            assert!(!agent_withheld(name), "{name}");
        }
        assert!(!is_forbidden("SSH_AUTH_SOCK"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
