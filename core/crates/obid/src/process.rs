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
    // obid's own CLAUDE_CODE_TMPDIR (a Claude Code run's, below), TODO_TOOLS and NO_CLAUDE_MDS are
    // the only CLAUDE_CODE_* let in.
    env.retain(|(name, _)| {
        name == CLAUDE_TMPDIR
            || name == CLAUDE_TODO_TOOLS
            || name == NO_CLAUDE_MDS
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

/// What a Claude Code launch adds to [`sandbox_env`]'s `extra`: Claude Code makes its Bash folder in
/// /tmp/claude-<uid> unless told, which the sandbox doesn't let a job write (a field test,
/// 2026-10-17: every Bash command failed with EPERM), so it's the run's own temporary folder.
pub const CLAUDE_TMPDIR: &str = "CLAUDE_CODE_TMPDIR";

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

/// `base`, then the [`usual_folders`] of `home` it doesn't have yet.
fn path_with_usual(base: &[PathBuf], home: Option<&Path>) -> OsString {
    let mut dirs = base.to_vec();
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
/// nvm's default node (npm's `-g` installs) and `/opt/homebrew/bin`. Only those that are folders.
fn usual_folders(home: &Path) -> Vec<PathBuf> {
    [
        Some(home.join(".local/bin")),
        Some(home.join(".bun/bin")),
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
/// but what pushes or signs in to a code host ([`agent_withheld`]'s: Claude Code's and Anthropic's
/// are the owner's gateway, and theirs), git's and obid's own, and the shell's own bookkeeping.
pub fn with_owners(env: &mut Vec<(OsString, OsString)>, owners: &[(OsString, OsString)]) {
    for (name, value) in owners {
        let n = name.to_string_lossy();
        let gateway = n.starts_with("ANTHROPIC_") || n.starts_with("CLAUDE_CODE_");
        let withheld = (agent_withheld(&n) && !gateway)
            || ["GIT_", "OBY_", "OBID_"].iter().any(|p| n.starts_with(p))
            || ["PWD", "OLDPWD", "SHLVL", "_"].contains(&&*n);
        if !withheld && !env.iter().any(|(set, _)| set == name) {
            env.push((name.clone(), value.clone()));
        }
    }
}

/// `bin` as a path: itself when it names one, else found on [`tools_path`].
pub fn resolve_bin(bin: &str) -> Option<PathBuf> {
    if bin.contains('/') {
        let path = PathBuf::from(bin);
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
        assert_eq!(found[1], home.join(".nvm/versions/node/v22.11.0/bin"));
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
mod tests {
    use super::*;

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
