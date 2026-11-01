//! Agent login discovery, macOS Keychain probing & token refresh, per-run private login copy
//! preparation, and `claudeOwnSandbox` settings generation.

use super::*;

/// Where the tool's own login is: the `HOME` obid was started with, as the tool itself would look
/// when the person runs it, else the password database's.
pub(crate) fn login_home() -> Option<PathBuf> {
    let homes = person_homes();
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| homes.contains(home))
        .or_else(|| homes.into_iter().next())
}

/// The file `account`'s login is in, in its `configDir` or else the tool's own in `home`: Claude
/// Code's `.credentials.json`, Codex's `auth.json`, Cursor's [`cursor_login_file`] (a Cursor
/// account's folder is its `HOME`). The file a run's copy is made from.
pub(super) fn login_file(
    tool: Tool,
    account: &crate::config::Account,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let (own, file) = match tool {
        Tool::Claude => (".claude/.credentials.json", ".credentials.json"),
        Tool::Codex => (".codex/auth.json", "auth.json"),
        Tool::Cursor => (cursor_login_file(), cursor_login_file()),
    };
    match (&account.config_dir, home) {
        (Some(dir), _) => Some(dir.join(file)),
        (None, Some(home)) => Some(home.join(own)),
        (None, None) => None,
    }
}

/// Whether `account`'s login is on this Mac: its [`login_file`] (a regular file only, as the
/// copies take it), or Claude Code's item in the macOS Keychain, or Cursor's own login there, which
/// obid copies in its place ([`keychain_login`], [`cursor_keychain_login`]). Whether a run can use it
/// is [`signed_in`].
pub(crate) fn has_login(tool: Tool, account: &crate::config::Account, home: Option<&Path>) -> bool {
    login_file(tool, account, home).is_some_and(|path| is_regular_file(&path))
        || (tool == Tool::Claude && keychain_has(account.config_dir.as_deref()))
        || (tool == Tool::Cursor && account.config_dir.is_none() && cursor_keychain_has())
}

/// Whether `account` has a login a boxed run can use (#626): [`has_login`], and for Codex an access
/// token that doesn't end within 5 minutes ([`codex_copy_ready`]: a copy can't refresh it), for
/// Cursor an access token or API key in its file, or its own login in the Keychain (all a run's copy
/// keeps). Claude Code's is only looked for.
pub(crate) fn signed_in(tool: Tool, account: &crate::config::Account, home: Option<&Path>) -> bool {
    has_login(tool, account, home)
        && match (tool, login_file(tool, account, home)) {
            (Tool::Codex, Some(file)) => codex_copy_ready(&file, &account.name).is_ok(),
            (Tool::Cursor, Some(file)) if is_regular_file(&file) => cursor_has_token(&file),
            // With no file, the login is Cursor's own Keychain items'.
            (Tool::Cursor, Some(_)) => cursor_keychain_live(),
            (Tool::Claude, _) => !claude_refused(account, home),
            _ => true,
        }
}

/// Why a login that's there can't run, as a run would say it: Codex's access token ends within 5
/// minutes, and only Codex itself refreshes it ([`codex_copy_ready`]); Cursor's own Keychain login
/// has ended, and only the Mac's own cursor-agent refreshes it ([`cursor_keychain_live`]).
pub(super) fn login_problem(
    tool: Tool,
    account: &crate::config::Account,
    home: Option<&Path>,
) -> Option<String> {
    let file = login_file(tool, account, home)?;
    if !has_login(tool, account, home) {
        return None;
    }
    match tool {
        Tool::Codex => codex_copy_ready(&file, &account.name).err(),
        Tool::Claude if claude_refused(account, home) => Some(keychain_refused(tool)),
        // Why it isn't live once it was read: macOS refused it, or its token ended.
        Tool::Cursor if !is_regular_file(&file) && !cursor_keychain_live() => Some(
            if CURSOR_ITEMS.iter().any(|(_, service)| refused(service)) {
                keychain_refused(tool)
            } else {
                CURSOR_ENDED.into()
            },
        ),
        _ => None,
    }
}

/// Why a login macOS didn't give obid can't run (#1679): the tool keeps it in the Keychain, macOS
/// asked whether obid may read it, and the owner denied it or didn't answer in time. Asked again
/// once the tool rewrites its login ([`refused`]): a sign-in does.
pub(crate) fn keychain_refused(tool: Tool) -> String {
    let sign_in = match tool {
        Tool::Cursor => "run cursor-agent login",
        _ => "run claude, then /login",
    };
    format!(
        "{} keeps its login in your Keychain, and macOS asked whether Oby may read it: that was denied or not answered, so Oby won't ask again until that login changes. To be asked again, {sign_in}, and choose Always Allow when macOS asks.",
        tool.name()
    )
}

/// Why no Keychain login could be read while the login Keychain was locked, or no one was signed in
/// at the screen (`security`'s exit 36): never kept, so the next read asks again.
pub(crate) const KEYCHAIN_LOCKED: &str = "Your login Keychain is locked, so Oby couldn't read the agent's login there: unlock your Mac (sign in at its screen), then try again.";

/// Whether macOS refused Claude Code's Keychain login for `account` and the item hasn't changed
/// since. An account with a login file (`.credentials.json`) is never refused: that file is its
/// login.
fn claude_refused(account: &crate::config::Account, home: Option<&Path>) -> bool {
    !login_file(Tool::Claude, account, home).is_some_and(|f| is_regular_file(&f))
        && refused(&keychain_service(account.config_dir.as_deref()))
}

/// Why a run on `service`'s Keychain login can't start, once its value was asked for: refused
/// ([`keychain_refused`]), or the Keychain was locked ([`KEYCHAIN_LOCKED`]).
pub(super) fn keychain_unread(tool: Tool, service: &str) -> Option<String> {
    if refused(service) {
        Some(keychain_refused(tool))
    } else if LOCKED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(service)
    {
        Some(KEYCHAIN_LOCKED.into())
    } else {
        None
    }
}

/// What the apps show beside an account whose login is there but can't run ([`login_problem`]).
pub(crate) fn login_note(
    tool: Tool,
    account: &crate::config::Account,
    home: Option<&Path>,
) -> Option<&'static str> {
    login_problem(tool, account, home).map(|why| match tool {
        _ if why.contains("Always Allow") => "Sign in again, then Always Allow",
        Tool::Cursor => "Run cursor-agent once",
        _ if why.contains(CODEX_SIGN_IN) => "Run codex login",
        _ => "Run codex once",
    })
}

/// Why a run on Cursor's own login doesn't start when its token has ended.
pub(super) const CURSOR_ENDED: &str = "The Mac's Cursor login has ended: run cursor-agent once.";

/// Whether Cursor's login file holds what a run's copy keeps ([`cursor_login_live`]).
pub(super) fn cursor_has_token(file: &Path) -> bool {
    std::fs::read(file).is_ok_and(|bytes| cursor_login_live(&bytes))
}

/// Whether a Cursor login (its file, or its Keychain items as [`cursor_login_json`] makes them)
/// holds what a run's copy keeps ([`cursor_without_refresh`]): an API key, or an access token that
/// hasn't ended, when its JWT says when (`exp`). An empty value is none.
pub(super) fn cursor_login_live(login: &[u8]) -> bool {
    let login: Value = serde_json::from_slice(login).unwrap_or_default();
    let value = |key: &str| login[key].as_str().filter(|t| !t.is_empty());
    value("apiKey").is_some()
        || value("accessToken")
            .is_some_and(|token| jwt_ends(token).is_none_or(|ends| ends > now_ms() / 1000))
}

/// What a Claude Code run in its own sandbox (`claudeOwnSandbox`) gets over a boxed run's
/// environment: the account's own folder and Keychain item (the default ones for an account with
/// no `configDir`), and, as its own `HOME`, the job's `login` folder (`home/logins/<job id>`,
/// which its commands can neither read nor write), holding only `Library/Keychains`, a link to the
/// person's, where `security` finds that item. Its commands get the job's temporary folder as
/// `HOME` instead, through `CLAUDE_ENV_FILE` (`env.sh` there, written fresh every turn), so what
/// they build and cache stays the job's. git reads only the clone's settings, through the job's
/// own git folders whatever the worktree's `.git` or `commondir` says: Claude Code runs git itself,
/// outside its sandbox, and a setting its commands could write would run what they put there.
/// With no global settings and an empty `credential.helper` there's no credential helper either.
/// Refused when the account's `.claude.json` sets, in its `env` (which Claude Code applies whatever
/// `--setting-sources` says), a variable no agent gets or one this sets.
pub(super) fn own_sandbox_env(
    account: &crate::config::Account,
    spec: &Spec,
    login: &Path,
) -> Result<Vec<(&'static str, OsString)>, String> {
    let home = login_home().ok_or("obid can't find the home Claude Code's login is in.")?;
    let (Some(admin), Some(common)) = (&spec.git_admin, &spec.git_common) else {
        return Err("obid has no git folder for this job's Claude Code.".into());
    };
    let folder = account
        .config_dir
        .clone()
        .unwrap_or_else(|| home.join(".claude"));
    let storage = account.config_dir.clone().unwrap_or_default();
    let env: Vec<(&'static str, OsString)> = vec![
        ("HOME", login.as_os_str().to_owned()),
        // Where CoreFoundation and `security` look too: never the job's temporary folder, which
        // its commands write (`env.sh` gives them that one).
        ("CFFIXED_USER_HOME", login.as_os_str().to_owned()),
        ("CLAUDE_ENV_FILE", login.join("env.sh").into_os_string()),
        ("CLAUDE_CONFIG_DIR", folder.clone().into_os_string()),
        ("CLAUDE_SECURESTORAGE_CONFIG_DIR", storage.into_os_string()),
        ("GIT_CONFIG_NOSYSTEM", "1".into()),
        ("GIT_CONFIG_GLOBAL", "/dev/null".into()),
        // And an empty credential helper on top of whatever the clone's settings say: in the
        // environment, which nothing the agent writes changes for Claude Code's own git.
        ("GIT_CONFIG_COUNT", "1".into()),
        ("GIT_CONFIG_KEY_0", "credential.helper".into()),
        ("GIT_CONFIG_VALUE_0", "".into()),
        ("GIT_DIR", admin.clone().into_os_string()),
        ("GIT_COMMON_DIR", common.clone().into_os_string()),
        ("GIT_WORK_TREE", spec.worktree.clone().into_os_string()),
    ];
    let state = std::fs::read(folder.join(".claude.json")).unwrap_or_default();
    let set = serde_json::from_slice::<Value>(&state).unwrap_or_default();
    for (name, _) in set["env"].as_object().into_iter().flatten() {
        if process::is_forbidden(name)
            || name.starts_with("GIT_")
            || ["HOME", "CFFIXED_USER_HOME", "PATH"].contains(&name.as_str())
            || env.iter().any(|(set, _)| set == name)
        {
            return Err(format!(
                "{}'s .claude.json sets {name} in its env, which a job's agent must never get or which obid sets itself: take it out, or take {} out of claudeOwnSandbox.",
                account.name, account.id
            ));
        }
    }
    let mut made = std::fs::DirBuilder::new();
    std::os::unix::fs::DirBuilderExt::mode(made.recursive(true), 0o700);
    made.create(login)
        .and_then(|()| link_keychains(login, &home))
        .map_err(|e| format!("obid can't show Claude Code its Keychain: {e}"))?;
    let temp = shell_quoted(&spec.temp.to_string_lossy());
    write_fresh(
        &login.join("env.sh"),
        format!("export HOME={temp}\nexport CFFIXED_USER_HOME={temp}\n").as_bytes(),
    )
    .map_err(|e| format!("obid can't give Claude Code's commands their HOME: {e}"))?;
    Ok(env)
}

/// `bytes` in a new file at `path` (0600), whatever was there gone first and never followed: Claude
/// Code lets its commands write `CLAUDE_ENV_FILE`, so one could have left a link to a file of the
/// person's in its place. `create_new` (`O_EXCL`) fails on a link rather than write through it.
pub(super) fn write_fresh(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path)?,
        Ok(_) => std::fs::remove_file(path)?,
        Err(_) => {}
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?
        .write_all(bytes)
}

/// `text` in single quotes for `sh`, a quote in it closed, escaped and opened again.
pub(super) fn shell_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// `at/Library/Keychains` as a link to `home`'s, where `security` finds the login Keychain through
/// `HOME`. Never through a link found there: a `Library` that isn't a folder of its own, or
/// anything at `Keychains`, goes first.
pub(super) fn link_keychains(at: &Path, home: &Path) -> std::io::Result<()> {
    let library = at.join("Library");
    if std::fs::symlink_metadata(&library).is_ok_and(|meta| !meta.is_dir()) {
        std::fs::remove_file(&library)?;
    }
    if std::fs::symlink_metadata(&library).is_err() {
        std::fs::create_dir(&library)?;
    }
    let keychains = library.join("Keychains");
    match std::fs::symlink_metadata(&keychains) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&keychains)?,
        Ok(_) => std::fs::remove_file(&keychains)?,
        Err(_) => {}
    }
    std::os::unix::fs::symlink(home.join("Library/Keychains"), &keychains)
}

/// `--settings` for a Claude Code run in its own sandbox. Its shell is always sandboxed (no
/// backend, no command: `failIfUnavailable`; never retried outside it); it reads nothing in obid's
/// home, the person's or the accounts' folders ([`Spec::hidden`], denied whole, so a folder made
/// there mid-turn is denied too) but what obid's box reads ([`Spec::readable`], re-opened), writes
/// only where the box would ([`Spec::writable`]), and never the git pointers ([`Spec::pointers`]) or
/// what Claude Code keeps in `login` (its `Library` with the Keychains link, and `env.sh`, which
/// Claude Code lets its commands write whatever the settings say: obid writes it again every turn,
/// never through a link, [`write_fresh`], and what it sets reaches only them). Its Read and Edit tools ask before going outside the
/// worktree, which obid refuses in this mode (`policy::names_outside`), and are denied the pointers
/// and those outright. obid still answers each of its commands (`autoAllowBashIfSandboxed` off). A
/// path the settings can't name stops the run.
pub(super) fn own_sandbox_settings(spec: &Spec, login: &Path) -> Result<String, String> {
    let spec = spec.canonical();
    let name = |path: &PathBuf| {
        path.to_str().map(str::to_owned).ok_or_else(|| {
            format!(
                "obid can't name {} to Claude Code's sandbox.",
                path.display()
            )
        })
    };
    let names = |paths: &[PathBuf]| paths.iter().map(name).collect::<Result<Vec<_>, _>>();
    let login = login.canonicalize().unwrap_or_else(|_| login.to_path_buf());
    let keychains = login.join("Library/Keychains");
    let mut kept = spec.pointers();
    kept.extend([login.join("Library"), login.join("env.sh")]);
    kept.push(keychains.clone());
    let mut hidden = spec.hidden.clone();
    hidden.push(keychains.clone());
    let mut rules = Vec::new();
    for path in names(&kept)? {
        rules.push(format!("Edit(/{path})"));
    }
    let keychains = name(&keychains)?;
    rules.extend([
        format!("Edit(/{keychains}/**)"),
        format!("Read(/{keychains}/**)"),
    ]);
    Ok(json!({
        "permissions": {"deny": rules},
        "sandbox": {
            "enabled": true,
            "failIfUnavailable": true,
            "allowUnsandboxedCommands": false,
            "autoAllowBashIfSandboxed": false,
            "filesystem": {
                "denyRead": names(&hidden)?,
                "allowRead": names(&spec.readable())?,
                "allowWrite": names(&spec.writable())?,
                "denyWrite": names(&kept)?,
            },
            "network": {"deniedDomains": CODE_HOSTS},
        },
    })
    .to_string())
}

/// The code hosts a Claude Code run in its own sandbox never reaches from its commands, whatever
/// it's allowed: only Publish pushes and opens pull requests (AGENTS.md, "Agents can't publish").
pub(super) const CODE_HOSTS: [&str; 6] = [
    "github.com",
    "*.github.com",
    "gitlab.com",
    "*.gitlab.com",
    "bitbucket.org",
    "*.bitbucket.org",
];

/// The name Claude Code gives its login in the macOS Keychain for a `CLAUDE_CONFIG_DIR` (none: its
/// default `~/.claude`): `Claude Code-credentials`, then `-` and the first 8 hex digits of the
/// folder's SHA-256 for any other folder.
pub(crate) fn keychain_service(config_dir: Option<&Path>) -> String {
    match config_dir {
        None => "Claude Code-credentials".into(),
        Some(dir) => {
            let digest = obi_protocol::keys::sha256(dir.as_os_str().as_encoded_bytes());
            format!("Claude Code-credentials-{}", &hex::encode(digest)[..8])
        }
    }
}

/// Whether obid copies Claude Code's Keychain login (`claudeKeychain` in the configuration, or
/// obid's own choice when it's left out, set when obid starts).
pub(super) static KEYCHAIN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// Whether obid chose it itself (`claudeKeychain` left out) rather than the owner.
pub(super) static KEYCHAIN_AUTO: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether the Keychain copy is on.
pub fn keychain_on() -> bool {
    KEYCHAIN.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether obid chose the Keychain copy's setting itself (`GET /v1/status`, `claudeKeychain`).
pub fn keychain_auto() -> bool {
    KEYCHAIN_AUTO.load(std::sync::atomic::Ordering::Relaxed)
}

/// Turns the Keychain copy on or off (crate::serve), and says whether obid chose it.
pub fn use_keychain(on: bool, auto: bool) {
    KEYCHAIN.store(on, std::sync::atomic::Ordering::Relaxed);
    KEYCHAIN_AUTO.store(auto, std::sync::atomic::Ordering::Relaxed);
}

/// Whether obid turns the Keychain copy on by itself, with `claudeKeychain` left out: on macOS,
/// when Claude Code's default login is in the Keychain and no `~/.claude/.credentials.json` is
/// there (Claude Code writes none when it can use the Keychain), so a fresh Mac's Claude accounts
/// don't all read as signed out. Only whether the item exists is looked at, never what it holds.
pub fn keychain_wanted() -> bool {
    let file = person_homes()
        .iter()
        .any(|home| is_regular_file(&home.join(".claude/.credentials.json")));
    wanted(cfg!(target_os = "macos"), file, || {
        keychain_item(None, false).is_some()
    })
}

/// [`keychain_wanted`]'s rule: on macOS, with no login file, when the Keychain holds the item.
pub(super) fn wanted(macos: bool, file: bool, item: impl FnOnce() -> bool) -> bool {
    macos && !file && item()
}

pub(super) fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

/// `security`'s answer for Claude Code's Keychain item for `config_dir`, as the person (`-a`):
/// the item itself with `-w`, else only whether it's there; nothing while the Keychain copy is off.
pub(super) fn keychain(config_dir: Option<&Path>, value: bool) -> Option<Vec<u8>> {
    if !keychain_on() {
        return None;
    }
    keychain_item(config_dir, value)
}

/// [`keychain`], whether or not the copy is on: what [`keychain_wanted`] asks once at start.
#[cfg(target_os = "macos")]
pub(super) fn keychain_item(config_dir: Option<&Path>, value: bool) -> Option<Vec<u8>> {
    let user = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()??
        .name;
    let service = keychain_service(config_dir);
    let mut command = std::process::Command::new(security_bin());
    command
        .args(["find-generic-password", "-s", &service])
        .args(["-a", &user]);
    ask_security(&service, command, value)
}

/// What `security` last answered for an item's value, by its service, while the item's
/// modification date (`stamp`, read without `-w`: no secret, so no prompt) stays the same (#1679).
/// A value is kept for the run and read again only once the tool rewrote the item (a refresh, a
/// sign-in): with "Allow" rather than "Always Allow" macOS asks at each read, so obid asks at most
/// once per change of the login, never at each of the keeper's minutes. A refusal (Deny, or no
/// answer within [`security_limit`]) stands until the item changes too, so prompts never stack.
/// Claude Code and cursor-agent write their items with `/usr/bin/security` itself, which macOS lets
/// read them back without asking: a prompt comes only for an item another program made.
pub(super) struct Answer {
    /// The `security` and its arguments that find the item, without `-w`.
    args: (std::ffi::OsString, Vec<std::ffi::OsString>),
    stamp: String,
    /// None: refused.
    value: Option<Vec<u8>>,
    /// When the item's stamp was last looked at for a refusal ([`refused`]).
    pub(super) looked: std::time::Instant,
}

pub(super) static ANSWERS: Mutex<BTreeMap<String, Answer>> = Mutex::new(BTreeMap::new());

/// The items whose last value read found the login Keychain locked (`security`'s exit 36), which
/// is never kept as an answer: the next read asks again.
pub(super) static LOCKED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn locked(service: &str, now: bool) {
    let mut all = LOCKED.lock().unwrap_or_else(|e| e.into_inner());
    if now {
        all.insert(service.to_owned());
    } else {
        all.remove(service);
    }
}

/// How often, at most, a refused item's stamp is looked at again, from `GET /v1/status`.
const LOOK_AGAIN: Duration = Duration::from_secs(30);

/// Whether macOS refused `service`'s value and the item hasn't changed since: its stamp is looked
/// at again at most every [`LOOK_AGAIN`], so a sign-in (which rewrites it) clears the refusal.
pub(super) fn refused(service: &str) -> bool {
    let (args, stamp) = {
        let mut answers = ANSWERS.lock().unwrap_or_else(|e| e.into_inner());
        let Some(answer) = answers.get_mut(service).filter(|a| a.value.is_none()) else {
            return false;
        };
        if answer.looked.elapsed() < LOOK_AGAIN {
            return true;
        }
        answer.looked = std::time::Instant::now();
        (answer.args.clone(), answer.stamp.clone())
    };
    if item_stamp(&args).as_deref() == Some(stamp.as_str()) {
        return true;
    }
    ANSWERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(service);
    false
}

/// The item's modification date as `security` prints it without `-w` (its `"mdat"` line; the whole
/// answer when there's none), or none when it isn't there.
#[cfg(target_os = "macos")]
fn item_stamp((program, args): &(std::ffi::OsString, Vec<std::ffi::OsString>)) -> Option<String> {
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let output = process::output_within(&mut command, security_limit(false))?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(
        text.lines()
            .find(|line| line.contains("\"mdat\""))
            .unwrap_or(&text)
            .trim()
            .to_owned(),
    )
}

#[cfg(not(target_os = "macos"))]
fn item_stamp(_args: &(std::ffi::OsString, Vec<std::ffi::OsString>)) -> Option<String> {
    None
}

/// `security`'s answer for `service` (`command` finds it): with `value`, the item's value, from
/// [`ANSWERS`] while the item is unchanged; else only whether it's there, which reads no secret and
/// so never prompts for an item.
#[cfg(target_os = "macos")]
pub(super) fn ask_security(
    service: &str,
    mut command: std::process::Command,
    value: bool,
) -> Option<Vec<u8>> {
    command
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if !value {
        let output = process::output_within(&mut command, security_limit(false))?;
        return output.status.success().then_some(output.stdout);
    }
    let args = (
        command.get_program().to_owned(),
        command
            .get_args()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>(),
    );
    let forget = || {
        ANSWERS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(service);
    };
    let Some(stamp) = item_stamp(&args) else {
        forget();
        locked(service, false);
        return None;
    };
    if let Some(answer) = ANSWERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(service)
        .filter(|a| a.stamp == stamp)
    {
        return answer.value.clone();
    }
    // ponytail: a date has whole seconds, so a rewrite in the second of the last read is missed
    // until the next one; the keeper's refresh comes seconds after its read.
    command.arg("-w");
    let output = process::output_within(&mut command, security_limit(true));
    let keep = |value: Option<Vec<u8>>| {
        ANSWERS.lock().unwrap_or_else(|e| e.into_inner()).insert(
            service.to_owned(),
            Answer {
                args: args.clone(),
                stamp: stamp.clone(),
                value,
                looked: std::time::Instant::now(),
            },
        );
    };
    // 36: the Keychain is locked, or no one is at the screen; 51: not allowed; 128: Deny.
    match output
        .as_ref()
        .map(|o| (o.status.success(), o.status.code()))
    {
        Some((true, _)) => {
            locked(service, false);
            let bytes = output.map(|o| o.stdout);
            keep(bytes.clone());
            bytes
        }
        Some((_, Some(36))) => {
            tracing::warn!("the login Keychain is locked, so obid couldn't read {service}");
            locked(service, true);
            forget();
            None
        }
        None | Some((_, Some(51 | 128))) => {
            tracing::warn!(
                "macOS didn't give obid {service} from the Keychain: it won't ask again until the item changes"
            );
            keep(None);
            None
        }
        Some(_) => {
            forget();
            None
        }
    }
}

/// The names of the Swift runner's vault items (service `com.deskree.oby.runner.vault`) in the
/// Keychain, for [`crate::vault::left_in_keychain`]: their attributes only (`security dump-keychain`
/// without `-d`), never a value, so macOS asks nothing (#1679). None elsewhere, or when `security`
/// can't say within 10 seconds.
pub fn swift_vault_names() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new(security_bin());
        command
            .arg("dump-keychain")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        process::output_within(&mut command, Duration::from_secs(10))
            .filter(|o| o.status.success())
            .map(|o| vault_names_in(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default()
    }
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

/// The `acct` of every item under the Swift runner's vault service in `security dump-keychain`'s
/// answer, one item per `keychain:` block.
pub(super) fn vault_names_in(dump: &str) -> Vec<String> {
    let quoted = |line: &str, key: &str| {
        line.trim()
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix("<blob>=\""))
            .and_then(|rest| rest.strip_suffix('"'))
            .map(str::to_owned)
    };
    let mut names: Vec<String> = dump
        .split("\nkeychain: ")
        .filter(|item| {
            item.lines()
                .any(|l| quoted(l, "\"svce\"").as_deref() == Some("com.deskree.oby.runner.vault"))
        })
        .filter_map(|item| item.lines().find_map(|l| quoted(l, "\"acct\"")))
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(not(target_os = "macos"))]
pub(super) fn keychain_item(_config_dir: Option<&Path>, _value: bool) -> Option<Vec<u8>> {
    None
}

/// The `security` obid reads the Keychain with: `/usr/bin/security`, or the absolute path in
/// `OBID_SECURITY_BIN`, which obid's tests set to a stand-in so a test's obid never reads the
/// person's real Keychain (tests/agents/fake-security).
#[cfg(target_os = "macos")]
pub(super) fn security_bin() -> PathBuf {
    std::env::var_os("OBID_SECURITY_BIN")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/usr/bin/security"))
}

/// How long `security` gets: 5 seconds to say whether an item is there (obid asks while it
/// starts), 60 to give its value, which can wait on the person's OK to the Keychain's prompt.
#[cfg(target_os = "macos")]
pub(super) fn security_limit(value: bool) -> Duration {
    Duration::from_secs(if value { 60 } else { 5 })
}

/// Whether Claude Code keeps a login for `config_dir` in the Keychain (macOS only). An item
/// found is taken as there for [`crate::accounts::LOOK_EVERY`]: `GET /v1/status` asks twice per
/// account (its label and `signedIn`), and each `security` takes 50 ms to most of a second. An
/// item not found is asked for again every time, so a sign-in shows at once.
pub(super) fn keychain_has(config_dir: Option<&Path>) -> bool {
    static SEEN: Mutex<Vec<(Option<PathBuf>, std::time::Instant)>> = Mutex::new(Vec::new());
    let fresh = |seen: &[(Option<PathBuf>, std::time::Instant)]| {
        seen.iter().any(|(dir, at)| {
            dir.as_deref() == config_dir && at.elapsed() < crate::accounts::LOOK_EVERY
        })
    };
    if fresh(&SEEN.lock().unwrap_or_else(|e| e.into_inner())) {
        return true;
    }
    // ponytail: two callers at once may both ask security; the cache only saves the next ones.
    // Timed from the ask, so a slow answer is kept no longer than one that came at once.
    let asked = std::time::Instant::now();
    let has = keychain(config_dir, false).is_some();
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    seen.retain(|(dir, _)| dir.as_deref() != config_dir);
    if !has {
        return false;
    }
    seen.push((config_dir.map(Path::to_path_buf), asked));
    true
}

/// Claude Code's own login for `config_dir` from the macOS Keychain, the owner's decision of
/// 2026-10-17: the sandbox denies the Keychain, and Claude Code no longer writes a login file when
/// it can't use the Keychain, so obid copies the Keychain's item into the place a boxed run reads
/// its `.credentials.json` from, as it copies a login file. Never logged, shown or kept anywhere
/// else; none on other systems. The copy never carries the refresh token ([`without_refresh`]);
/// the Mac's own Claude Code keeps the login fresh ([`keychain_ready`], [`keep_keychain_logins`]).
pub(crate) fn keychain_login(config_dir: Option<&Path>) -> Option<Vec<u8>> {
    let bytes = keychain(config_dir, true)?;
    learn_claude_tokens(&bytes, &keychain_service(config_dir));
    without_refresh(&bytes)
}

/// Claude Code's login as obid copies it from the Keychain: exactly what calls the API (the access
/// token, when it ends, its scopes, the plan and its rate tier) and nothing else the item holds.
/// Never a refresh token: Anthropic rotates them, so a boxed run that refreshed with a copied one
/// revoked the Keychain's own and signed the Mac's own `claude` out (2026-10-17). Nor the owner's
/// MCP logins, plug-in secrets or device tokens (#651's review). None when it has no access token.
pub(super) fn without_refresh(bytes: &[u8]) -> Option<Vec<u8>> {
    let login: Value = serde_json::from_slice(bytes).ok()?;
    let oauth = login.get("claudeAiOauth")?;
    oauth.get("accessToken")?.as_str()?;
    let mut kept = serde_json::Map::new();
    for key in [
        "accessToken",
        "expiresAt",
        "scopes",
        "subscriptionType",
        "rateLimitTier",
    ] {
        if let Some(value) = oauth.get(key) {
            kept.insert(key.into(), value.clone());
        }
    }
    serde_json::to_vec(&json!({ "claudeAiOauth": kept })).ok()
}

/// Whether obid copies Cursor's own login from the macOS Keychain (`cursorKeychain` in the
/// configuration, or obid's own choice when it's left out, set when obid starts), and whether obid
/// chose it itself.
pub(super) static CURSOR_KEYCHAIN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
pub(super) static CURSOR_KEYCHAIN_AUTO: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether the Cursor Keychain copy is on.
pub fn cursor_keychain_on() -> bool {
    CURSOR_KEYCHAIN.load(Ordering::Relaxed)
}

/// Whether obid chose the Cursor Keychain copy's setting itself (`GET /v1/status`, `cursorKeychain`).
pub fn cursor_keychain_auto() -> bool {
    CURSOR_KEYCHAIN_AUTO.load(Ordering::Relaxed)
}

/// Turns the Cursor Keychain copy on or off (crate::serve), and says whether obid chose it.
pub fn use_cursor_keychain(on: bool, auto: bool) {
    CURSOR_KEYCHAIN.store(on, Ordering::Relaxed);
    CURSOR_KEYCHAIN_AUTO.store(auto, Ordering::Relaxed);
}

/// Whether obid turns the Cursor Keychain copy on by itself, with `cursorKeychain` left out, as for
/// Claude Code ([`keychain_wanted`]): on macOS, when Cursor's own login is in the Keychain (where
/// cursor-agent keeps it there by default) and no login file ([`cursor_login_file`]) is there. Only
/// whether an item exists is looked at, never what it holds.
pub fn cursor_keychain_wanted() -> bool {
    let file = person_homes()
        .iter()
        .any(|home| is_regular_file(&home.join(cursor_login_file())));
    wanted(cfg!(target_os = "macos"), file, || {
        CURSOR_ITEMS
            .iter()
            .any(|(_, service)| cursor_keychain_item(service, false).is_some())
    })
}

/// Cursor's own login in the macOS Keychain, as cursor-agent 2026.10.01 keeps it there with its
/// credential store left as it is: account `cursor-user`, the access token in `cursor-access-token`
/// and the API key in `cursor-api-key`, by the login file's keys. Its refresh token
/// (`cursor-refresh-token`) is never asked for: a refresh inside a box would rotate it away from the
/// Mac's own cursor-agent (#651).
pub(super) const CURSOR_ITEMS: [(&str, &str); 2] = [
    ("accessToken", "cursor-access-token"),
    ("apiKey", "cursor-api-key"),
];

/// `security`'s answer for Cursor's Keychain item `service`: its value with `-w`, else only whether
/// it's there.
#[cfg(target_os = "macos")]
pub(super) fn cursor_keychain_item(service: &str, value: bool) -> Option<Vec<u8>> {
    let mut command = std::process::Command::new(security_bin());
    command.args(["find-generic-password", "-a", "cursor-user", "-s", service]);
    ask_security(service, command, value)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn cursor_keychain_item(_service: &str, _value: bool) -> Option<Vec<u8>> {
    None
}

/// Whether Cursor's own Keychain login can run ([`cursor_login_live`]), for `GET /v1/status` and
/// routing: its values are read at most once a minute, not at every status (a run's copy reads them
/// afresh, [`cursor_keychain_login`]).
pub(super) fn cursor_keychain_live() -> bool {
    static LAST: Mutex<Option<(std::time::Instant, bool)>> = Mutex::new(None);
    if let Some((at, live)) = *LAST.lock().unwrap_or_else(|e| e.into_inner())
        && at.elapsed() < Duration::from_secs(60)
    {
        return live;
    }
    // Not under the lock: a read can wait on macOS's prompt, and other statuses mustn't.
    let live = cursor_keychain_login().is_some_and(|login| cursor_login_live(&login));
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((std::time::Instant::now(), live));
    live
}

/// Whether Cursor's own login is in the Keychain, while the copy is on.
/// An item found is taken as there for [`crate::accounts::LOOK_EVERY`], as [`keychain_has`]'s.
pub(super) fn cursor_keychain_has() -> bool {
    static FOUND: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    if !cursor_keychain_on() {
        return false;
    }
    if FOUND
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some_and(|at| at.elapsed() < crate::accounts::LOOK_EVERY)
    {
        return true;
    }
    let asked = std::time::Instant::now();
    let has = CURSOR_ITEMS
        .iter()
        .any(|(_, service)| cursor_keychain_item(service, false).is_some());
    *FOUND.lock().unwrap_or_else(|e| e.into_inner()) = has.then_some(asked);
    has
}

/// Cursor's own login from the macOS Keychain, while the copy is on: as the login file a run's copy
/// is made from ([`copy_cursor`]), with only its access token and API key ([`cursor_login_json`]).
/// None when neither is there.
pub(super) fn cursor_keychain_login() -> Option<Vec<u8>> {
    if !cursor_keychain_on() {
        return None;
    }
    cursor_login_json(|service| cursor_keychain_item(service, true))
}

/// The login file `{accessToken, apiKey}` from Cursor's Keychain items, as `read` answers each
/// (`security -w` ends it with a newline); an empty or missing one is left out, and none at all is
/// no login.
pub(super) fn cursor_login_json(read: impl Fn(&str) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    let mut login = serde_json::Map::new();
    for (key, service) in CURSOR_ITEMS {
        let Some(bytes) = read(service) else { continue };
        let value = String::from_utf8_lossy(&bytes).trim_end().to_owned();
        if !value.is_empty() {
            login.insert(key.into(), value.into());
        }
    }
    (!login.is_empty())
        .then(|| serde_json::to_vec(&login).ok())
        .flatten()
}

/// When the access token in Claude Code's `login` ends (milliseconds since 1970), if it says.
pub(super) fn expires_at(login: &[u8]) -> Option<u64> {
    serde_json::from_slice::<Value>(login).ok()?["claudeAiOauth"]["expiresAt"].as_u64()
}

/// When the Mac's own Claude Code last refreshed an account's login, and whether that worked.
type Refreshed = Option<(std::time::Instant, bool)>;

/// What [`keep_fresh`] found of a Keychain login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fresh {
    /// Good to copy as it is (or there's none: nothing is copied).
    Good,
    /// The Mac's own Claude Code just refreshed it: every copy is written again.
    Renewed,
    /// It can't be refreshed: the Mac's Claude login needs signing in again.
    SignIn,
}

/// Whether a token ending at `ends` is far enough from its end at `now` that Claude Code wouldn't
/// refresh it: it refreshes only in a token's last 5 minutes (2.1.296).
pub(super) fn far_from_end(ends: Option<u64>, now: u64) -> bool {
    ends.is_some_and(|t| t > now + 5 * 60_000)
}

/// The Keychain login `read` answers, kept fresh: when its token ends within 5 minutes of `now`
/// or doesn't say when, the Mac's own Claude Code refreshes it (`refresh`), renewed if that worked
/// and the token hasn't ended. At most once per 4 minutes (`last`: when it ran and whether it
/// worked), so a failed refresh isn't hammered; meanwhile a token that hasn't ended is still good.
pub(super) fn keep_fresh(
    last: &mut Refreshed,
    now: u64,
    read: impl Fn() -> Option<Vec<u8>>,
    refresh: impl FnOnce() -> bool,
) -> Fresh {
    let good = |yes: bool| if yes { Fresh::Good } else { Fresh::SignIn };
    let Some(login) = read() else {
        return Fresh::Good;
    };
    let ends = expires_at(&login);
    if far_from_end(ends, now) {
        return Fresh::Good;
    }
    if let Some((_, worked)) = last.filter(|(at, _)| at.elapsed() < Duration::from_secs(4 * 60)) {
        return good(ends.map_or(worked, |t| t > now));
    }
    let worked = refresh() && read().is_some_and(|l| expires_at(&l).is_none_or(|t| t > now));
    *last = Some((std::time::Instant::now(), worked));
    if worked {
        Fresh::Renewed
    } else {
        Fresh::SignIn
    }
}

/// Each Keychain item's lock and last refresh ([`Refreshed`]), by its name: one refresh at a time
/// for an item, so many runs starting at once refresh it once; other items go on.
pub(super) static ITEMS: Mutex<Vec<(String, Arc<Mutex<Refreshed>>)>> = Mutex::new(Vec::new());

/// [`keep_fresh`] for the Keychain item `service`: a login far from its end takes no lock; else
/// one refresh at a time for that item, and the one that comes after it finds the login fresh (or
/// within the 4 minutes after a failed one). After a refresh, the access token running jobs still
/// hold stays a secret obid knows, next to the new one. Blocks: never on obid's async runtime.
pub(super) fn refresh_if_due(
    service: &str,
    now: u64,
    read: impl Fn() -> Option<Vec<u8>>,
    refresh: impl FnOnce() -> bool,
) -> Fresh {
    let Some(before) = read() else {
        return Fresh::Good;
    };
    if far_from_end(expires_at(&before), now) {
        return Fresh::Good;
    }
    let item = {
        let mut items = ITEMS.lock().unwrap_or_else(|e| e.into_inner());
        match items.iter().find(|(s, _)| s == service) {
            Some((_, item)) => item.clone(),
            None => {
                let item = Arc::new(Mutex::new(None));
                items.push((service.to_owned(), item.clone()));
                item
            }
        }
    };
    let mut last = item.lock().unwrap_or_else(|e| e.into_inner());
    let fresh = keep_fresh(&mut last, now, &read, refresh);
    if fresh == Fresh::Renewed
        && let Ok(login) = serde_json::from_slice::<Value>(&before)
        && let Some(token) = login["claudeAiOauth"]["accessToken"].as_str()
    {
        crate::secrets::known().learn(&format!("{service} accessToken before its refresh"), token);
    }
    fresh
}

/// A Keychain login the keeper keeps fresh: the default one (no `configDir`), or that of an account
/// obid added itself (#877, [`keychain_ready`]).
pub(super) struct Kept {
    /// Its Keychain item ([`keychain_service`]).
    service: String,
    /// The account's folder, which names its item; none for the default login.
    config_dir: Option<PathBuf>,
    /// The Mac's own Claude Code, which refreshes it.
    bin: PathBuf,
    /// When a boxed run last started on it.
    used: std::time::Instant,
    /// The jobs' private login copies of it (`.credentials.json`) while their runs go.
    copies: Vec<PathBuf>,
}

/// The logins [`keep_keychain_logins`] keeps fresh.
pub(super) static KEPT: Mutex<Vec<Kept>> = Mutex::new(Vec::new());

pub(super) fn kept() -> std::sync::MutexGuard<'static, Vec<Kept>> {
    KEPT.lock().unwrap_or_else(|e| e.into_inner())
}

/// Milliseconds since 1970, as `expiresAt` counts.
pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Writes `bytes` again to each of `copies` that is still there ([`write_login`], never through a
/// link); one that's gone (its run ended) is forgotten, never made again.
pub(super) fn rewrite(copies: &mut Vec<PathBuf>, bytes: &[u8]) {
    copies.retain(|path| std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file()));
    for path in copies.iter() {
        let _ = write_login(path, bytes);
    }
}

/// The Keychain login for `config_dir` refreshed when it's due ([`refresh_if_due`]); once renewed,
/// written again into every running job's private copy. Blocks.
pub(super) fn keep_one(service: &str, config_dir: Option<&Path>, bin: &Path) -> Fresh {
    let fresh = refresh_if_due(
        service,
        now_ms(),
        || keychain(config_dir, true),
        || refresh_claude_login(bin, config_dir),
    );
    if fresh == Fresh::Renewed
        && let Some(bytes) = keychain_login(config_dir)
        // Under the keeper's lock, so a job that ends ([`forget_keychain_copy`]) never has its
        // copy made again.
        && let Some(account) = kept().iter_mut().find(|k| k.service == service)
    {
        rewrite(&mut account.copies, &bytes);
    }
    fresh
}

/// Before a boxed Claude Code run on `account` that gets the Keychain's login
/// ([`keychain_login`]): if its token ends within 5 minutes, the Mac's own Claude Code refreshes
/// it first (or the run waits for the refresh that's going), off obid's async runtime, and the
/// login is kept fresh from now on ([`keep_keychain_logins`]): the default one, and that of an
/// account obid added itself (`made`: its folder is obid's `home/accounts/<id>`, #877), each its
/// own Keychain item. Any other account with a folder of its own isn't refreshed: its login is good
/// while its token is far from its end, and refused after ([`folder_login_ok`]). Throws one line
/// when the login can't be used. obid never writes the Keychain.
pub(crate) async fn keychain_ready(
    bin: PathBuf,
    account: &crate::config::Account,
    made: bool,
) -> Result<(), String> {
    if !keychain_on() {
        return Ok(());
    }
    let dir = account.config_dir.clone();
    // With no folder of its own, the Keychain's login is copied only when there's no login file.
    if dir.is_none()
        && login_home()
            .is_some_and(|h| std::fs::symlink_metadata(h.join(".claude/.credentials.json")).is_ok())
    {
        return Ok(());
    }
    // A login file is the account's login: a Keychain read macOS refused doesn't stop it.
    let file = login_file(Tool::Claude, account, login_home().as_deref())
        .is_some_and(|f| is_regular_file(&f));
    let unread = |service: &str| {
        if file {
            None
        } else {
            keychain_unread(Tool::Claude, service)
        }
    };
    if let Some(folder) = dir.clone().filter(|_| !made) {
        // Never refreshed by obid while the Mac app may run unboxed on the folder (#655's open
        // item, [`refresh_claude_login`]): good far from its end, else refused.
        let service = keychain_service(Some(&folder));
        let read = tokio::task::spawn_blocking(move || keychain(Some(&folder), true))
            .await
            .unwrap_or(None);
        if read.is_none()
            && let Some(why) = unread(&service)
        {
            return Err(why);
        }
        return if folder_login_ok(read.as_deref(), now_ms()) {
            Ok(())
        } else {
            Err(format!(
                "The Mac's Claude login for {} ends soon or has ended, and obid doesn't refresh an account's own folder while the Mac app may run on it unboxed ({SHARED_FOLDER_ISSUE}): run claude on that account to refresh it.",
                account.name
            ))
        };
    }
    let service = keychain_service(dir.as_deref());
    {
        let mut all = kept();
        match all.iter_mut().find(|k| k.service == service) {
            Some(k) => {
                k.used = std::time::Instant::now();
                k.bin = bin.clone();
            }
            None => all.push(Kept {
                service: service.clone(),
                config_dir: dir.clone(),
                bin: bin.clone(),
                used: std::time::Instant::now(),
                copies: Vec::new(),
            }),
        }
    }
    let fresh = {
        let service = service.clone();
        tokio::task::spawn_blocking(move || keep_one(&service, dir.as_deref(), &bin))
            .await
            .unwrap_or(Fresh::SignIn)
    };
    // A login macOS didn't give: the run would start with none and fail "not logged in".
    if let Some(why) = unread(&service) {
        return Err(why);
    }
    match fresh {
        Fresh::SignIn => Err(format!(
            "The Mac's Claude login for {} needs signing in again (run claude, then /login).",
            account.name
        )),
        _ => Ok(()),
    }
}

/// Whether the Keychain login of an account with a folder of its own, which obid doesn't refresh,
/// is good to copy: there is none (nothing is copied from it), or its token is far from its end.
pub(super) fn folder_login_ok(login: Option<&[u8]>, now: u64) -> bool {
    login.is_none_or(|login| far_from_end(expires_at(login), now))
}

/// Whether the login copy a run was given in `login` (its `.credentials.json`, or its
/// `.claude/.credentials.json`) is good for one more turn: there is none (none was copied), or its
/// token doesn't end within 5 minutes ([`file_login_ok`]). A conversation's Claude Code kept
/// running between turns reads it.
pub(crate) fn login_copy_lasts(login: &Path) -> bool {
    login_copy_lasts_past(login, now_ms())
}

/// [`login_copy_lasts`] at `now` (milliseconds): later than now asks for a wider margin.
pub(super) fn login_copy_lasts_past(login: &Path, now: u64) -> bool {
    match [".credentials.json", ".claude/.credentials.json"]
        .iter()
        .map(|name| login.join(name))
        .find(|file| std::fs::symlink_metadata(file).is_ok())
    {
        None => true,
        // A link in its place is never followed: the turn starts a Claude Code with a new copy.
        Some(file) => {
            std::fs::symlink_metadata(&file).is_ok_and(|m| m.is_file())
                && std::fs::read(file)
                    .ok()
                    .is_some_and(|login| file_login_ok(Some(&login), now))
        }
    }
}

/// The issue for the Mac app's Swift engine still running agents unboxed on an account's folder,
/// after which obid can refresh an account's own login too.
pub(super) const SHARED_FOLDER_ISSUE: &str = "#655";

/// Whether an account's login file is good to copy for a run (the Keychain isn't used): there is
/// none (nothing is copied), it doesn't say when it ends (the tool decides), or its token is far
/// from its end ([`far_from_end`]). A copy carries
/// no refresh token, so it can't refresh itself in the box; the person refreshes the file by
/// running the tool on the account, which no box can steer now (#655).
pub(super) fn file_login_ok(login: Option<&[u8]>, now: u64) -> bool {
    login.is_none_or(|login| expires_at(login).is_none_or(|ends| far_from_end(Some(ends), now)))
}

/// Before a boxed Claude Code run on an account with a folder of its own when obid doesn't use
/// the Keychain: its login file's token is far from its end ([`file_login_ok`]).
pub(super) fn file_login_ready(account: &crate::config::Account) -> Result<(), String> {
    let Some(dir) = &account.config_dir else {
        return Ok(());
    };
    let file = dir.join(".credentials.json");
    let login = std::fs::symlink_metadata(&file)
        .is_ok_and(|m| m.is_file())
        .then(|| std::fs::read(&file).ok())
        .flatten();
    if file_login_ok(login.as_deref(), now_ms()) {
        Ok(())
    } else {
        Err(format!(
            "The Claude login for {} ends soon or has ended, and a job's copy of it can't refresh itself: run claude on that account to refresh it.",
            account.name
        ))
    }
}

/// An account's login as obid copies it for a run that must not read the account's folder (the
/// unboxed brain, `crate::brain::calls`): its `.credentials.json` when that's a regular file, else
/// the Keychain's ([`keychain_login`]), only what calls the API ([`without_refresh`]).
pub(crate) fn account_login(config_dir: &Path) -> Option<Vec<u8>> {
    folder_login(config_dir).or_else(|| keychain_login(Some(config_dir)))
}

/// An account's `.credentials.json` when it's a regular file, only what calls the API
/// ([`without_refresh`]), its tokens learned as secrets obid knows; never the Keychain's.
pub(crate) fn folder_login(config_dir: &Path) -> Option<Vec<u8>> {
    let file = config_dir.join(".credentials.json");
    let bytes = std::fs::symlink_metadata(&file)
        .is_ok_and(|m| m.is_file())
        .then(|| std::fs::read(&file).ok())
        .flatten()?;
    learn_claude_tokens(&bytes, &keychain_service(Some(config_dir)));
    without_refresh(&bytes)
}

/// At obid's start, each configured account's folder looked at once: what a box of an older obid
/// could have planted there for the next unboxed run (#655), said in obid's log
/// ([`planted`]); and, where the Keychain holds the account's login, a `.credentials.json` an older
/// obid copied there (it has no refresh token) taken out, `claudeKeychain` on or off, since it
/// would make the account look signed in after its Keychain item is gone (#646).
pub fn check_account_folders(agents: &crate::config::Agents) {
    for (tool, agent) in [
        (Tool::Claude, &agents.claude),
        (Tool::Codex, &agents.codex),
        (Tool::Cursor, &agents.cursor),
    ] {
        for account in agent.iter().flat_map(|a| &a.accounts) {
            let Some(dir) = &account.config_dir else {
                continue;
            };
            for found in planted(tool, dir) {
                tracing::warn!(
                    "account {}: {found} in {}: a job of an older obid could have put it there (#655); look before running {} on that folder.",
                    account.id,
                    dir.display(),
                    tool.name()
                );
            }
            // Only where the Keychain holds that login: the file is then a copy, never the login
            // itself. Whether or not `claudeKeychain` is on now (#646): a copy left by an obid
            // that had it on would make the account look signed in, and be copied for runs.
            if tool == Tool::Claude
                && cfg!(target_os = "macos")
                && remove_older_copy(dir, keychain_item(Some(dir), false).is_some())
            {
                tracing::info!(
                    "account {}: took out an older obid's copy of its login from its folder",
                    account.id
                );
            }
        }
    }
}

/// What in an account's folder would run something, or change where a tool sends its requests,
/// the next time the tool runs unboxed on it: Claude Code's `hooks`, `env`, `apiKeyHelper`,
/// `statusLine` and `mcpServers` (in `settings.json`, `settings.local.json` or `.claude.json`);
/// Codex's `notify`, `mcp_servers`, `model_providers` and `profiles` (in `config.toml`); Cursor's
/// `mcp.json`, and a `permissions.allow` that lets something through in `cli-config.json`.
pub(super) fn planted(tool: Tool, dir: &Path) -> Vec<String> {
    let read = |name: &str| {
        let file = dir.join(name);
        std::fs::symlink_metadata(&file)
            .is_ok_and(|m| m.is_file())
            .then(|| std::fs::read_to_string(&file).ok())
            .flatten()
    };
    let mut found = Vec::new();
    match tool {
        Tool::Claude => {
            for name in ["settings.json", "settings.local.json", ".claude.json"] {
                let Some(json) = read(name).and_then(|t| serde_json::from_str::<Value>(&t).ok())
                else {
                    continue;
                };
                for key in ["hooks", "env", "apiKeyHelper", "statusLine", "mcpServers"] {
                    if json.get(key).is_some_and(|v| !v.is_null()) {
                        found.push(format!("`{key}` in {name}"));
                    }
                }
            }
        }
        Tool::Codex => {
            if let Some(text) = read("config.toml") {
                for key in ["notify", "mcp_servers", "model_providers", "profiles"] {
                    let set = text.lines().map(str::trim).any(|line| {
                        line.starts_with(&format!("[{key}"))
                            || line
                                .strip_prefix(key)
                                .is_some_and(|rest| rest.trim_start().starts_with(['=', '.']))
                    });
                    if set {
                        found.push(format!("`{key}` in config.toml"));
                    }
                }
            }
        }
        Tool::Cursor => {
            if std::fs::symlink_metadata(dir.join("mcp.json")).is_ok() {
                found.push("an `mcp.json`".into());
            }
            let allows = read("cli-config.json")
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|c| c["permissions"]["allow"].as_array().map(|a| !a.is_empty()));
            if allows == Some(true) {
                found.push("`permissions.allow` in cli-config.json".into());
            }
        }
    }
    found
}

/// Takes out a `.credentials.json` in an account's folder that has no refresh token, when the
/// Keychain holds the account's login (`held`): then it is a copy an older obid wrote there from
/// the Keychain before every run (Claude Code's own has one), never the login itself. Never
/// through a link. Whether one was taken out.
pub(super) fn remove_older_copy(dir: &Path, held: bool) -> bool {
    let file = dir.join(".credentials.json");
    let older = held
        && std::fs::symlink_metadata(&file).is_ok_and(|m| m.is_file())
        && std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|login| {
                login["claudeAiOauth"].is_object()
                    && login["claudeAiOauth"]
                        .get("refreshToken")
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
            });
    older && std::fs::remove_file(&file).is_ok()
}

/// A job's private copy (`path`) of the Keychain login `service`, kept fresh while its run goes.
pub(super) fn keep_keychain_copy(service: &str, path: PathBuf) {
    if let Some(account) = kept().iter_mut().find(|k| k.service == service)
        && !account.copies.contains(&path)
    {
        account.copies.push(path);
    }
}

/// The copy at `path` isn't kept fresh any more: its run ended.
pub(super) fn forget_keychain_copy(path: &Path) {
    for account in kept().iter_mut() {
        account.copies.retain(|p| p != path);
    }
}

/// For tests: the keeper keeps the login `service` (of `config_dir`), and `copy` of it.
#[cfg(test)]
pub(crate) fn keeping(service: &str, config_dir: Option<&Path>, copy: PathBuf) {
    kept().push(Kept {
        service: service.to_owned(),
        config_dir: config_dir.map(Path::to_path_buf),
        bin: PathBuf::from("/usr/bin/false"),
        used: std::time::Instant::now(),
        copies: Vec::new(),
    });
    keep_keychain_copy(service, copy);
}

/// For tests: the copies the keeper writes again of the login `service`.
#[cfg(test)]
pub(crate) fn kept_copies(service: &str) -> Vec<PathBuf> {
    kept()
        .iter()
        .filter(|k| k.service == service)
        .flat_map(|k| k.copies.clone())
        .collect()
}

/// With `claudeKeychain` on, obid's keeper: every 60 seconds, on a thread of its own, each login it
/// keeps (the default one, and the accounts obid added) is refreshed when it's due and the running
/// jobs' copies written again ([`keep_one`]),
/// while a boxed run used it in the last hour or a job of it still runs. A running Claude Code
/// reads its `.credentials.json` again when the file changes, before it calls the model (2.1.296),
/// so a long job goes on with the renewed token. Started once, by obid's start.
pub fn keep_keychain_logins() {
    let _ = std::thread::Builder::new()
        .name("keychain-keeper".into())
        .spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(60));
                let due: Vec<(String, Option<PathBuf>, PathBuf)> = {
                    let mut all = kept();
                    for account in all.iter_mut() {
                        account
                            .copies
                            .retain(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file()));
                    }
                    all.retain(|k| {
                        !k.copies.is_empty() || k.used.elapsed() < Duration::from_secs(60 * 60)
                    });
                    all.iter()
                        .map(|k| (k.service.clone(), k.config_dir.clone(), k.bin.clone()))
                        .collect()
                };
                for (service, config_dir, bin) in due {
                    keep_one(&service, config_dir.as_deref(), &bin);
                }
            }
        });
}

/// The command that has the Mac's own Claude Code (`bin`, by its absolute path) refresh the login
/// whose Keychain item `storage` names (none: the default one), as it does before it calls its API:
/// its own `/usage` (`claude -p /usage`), which refreshes an ended login and asks no model (Claude
/// Code 2.1.302: `num_turns` 0, `total_cost_usd` 0), with no tools and none of the person's
/// settings; haiku and one turn bound a Claude Code without its own `/usage`, which takes it for a
/// prompt and refreshes before it asks, as `-p ok` did before (USAGE-2). It never reads a
/// folder a job can write: its config folder is `scratch`, a new empty one of obid's (Claude Code
/// applies a `.claude.json`'s `env` from its config folder whatever `--setting-sources` says), and
/// `storage` reaches it only as `CLAUDE_SECURESTORAGE_CONFIG_DIR`, which names the Keychain item.
/// `PATH` is the system's folders first (it runs `security`), then only what the binary needs to
/// start, as its sign-in's ([`crate::accounts::login_path`]: a Claude Code installed with npm is a
/// `node` script), and obid's Claude Code and Anthropic variables are left out.
pub(super) fn refresh_command(
    bin: &Path,
    storage: Option<&Path>,
    scratch: &Path,
    home: &Path,
) -> Command {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(bin);
    command
        .args([
            "-p",
            "/usage",
            "--model",
            "haiku",
            "--max-turns",
            "1",
            "--tools",
            "",
        ])
        .args(["--strict-mcp-config", "--setting-sources", ""])
        .arg("--no-session-persistence")
        .current_dir(scratch)
        .env_clear()
        .envs(process::kept(|name| std::env::var_os(name)))
        .env("PATH", crate::accounts::login_path(bin))
        .env("HOME", home)
        .env("TMPDIR", scratch)
        .env("CLAUDE_CONFIG_DIR", scratch)
        .env(
            "CLAUDE_SECURESTORAGE_CONFIG_DIR",
            storage.map_or(std::ffi::OsStr::new(""), Path::as_os_str),
        )
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    command
}

/// Has the Mac's own Claude Code (`bin`, not boxed, so it reaches the Keychain) refresh its
/// default login ([`refresh_command`]) within 60 seconds. Claude Code 2.1.302 has no command that
/// only refreshes (`claude auth status` reads the login without refreshing it); its `/usage` is the
/// nearest, refreshing without asking a model. What it says is
/// never kept. For an account with a folder of its own (`config_dir`), only one obid added itself
/// ([`keychain_ready`]'s `made`, #877): on a Mac the Swift engine still runs agents unboxed with
/// the folders it knows ([`SHARED_FOLDER_ISSUE`]'s open item), never with obid's own. Claude Code
/// keeps the item's refresh lock, write lock and plaintext fallback in the folder, so nothing runs
/// unless those are the person's own and no link ([`crate::accounts::login_paths_safe`]).
pub(super) fn refresh_claude_login(bin: &Path, config_dir: Option<&Path>) -> bool {
    if !bin.is_absolute()
        || config_dir.is_some_and(|dir| {
            !dir.is_dir() || crate::accounts::login_paths_safe(Tool::Claude, dir).is_err()
        })
    {
        return false;
    }
    let Some(home) = login_home() else {
        return false;
    };
    static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = std::env::temp_dir().join(format!("obid-refresh-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    // A new folder: made here, by obid, or nothing runs.
    let mut builder = std::fs::DirBuilder::new();
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    if builder.create(&scratch).is_err() {
        return false;
    }
    let ended = refresh_command(bin, config_dir, &scratch, &home)
        .spawn()
        .map(|mut child| {
            let give_up_at = std::time::Instant::now() + Duration::from_secs(60);
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) if std::time::Instant::now() < give_up_at => {
                        std::thread::sleep(Duration::from_millis(50))
                    }
                    _ => {
                        process::signal_group(child.id(), nix::sys::signal::Signal::SIGKILL);
                        let _ = child.wait();
                        break None;
                    }
                }
            }
        });
    let worked = matches!(ended, Ok(Some(status)) if status.success());
    // Only how it ended, never what it said: enough to tell a binary that can't start (127) from
    // a login that's really gone.
    if !worked {
        let how = match &ended {
            Ok(Some(status)) => status.to_string(),
            Ok(None) => "it didn't end within 60 seconds".into(),
            Err(e) => e.to_string(),
        };
        tracing::warn!(
            "Claude Code ({}) didn't refresh its login: {how}",
            bin.display()
        );
    }
    let _ = std::fs::remove_dir_all(&scratch);
    worked
}

/// Writes `bytes` to `path` as only the person may read it (0600), replacing what's there. Never
/// through a link: not one in place of the file, nor one a job left in place of its folder (the
/// folder is opened without following a link, and the file made in that very folder; #643's
/// security review).
pub(crate) fn write_login(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use nix::fcntl::{OFlag, open, openat};
    use nix::sys::stat::Mode;
    let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(std::io::Error::other("no folder for the login"));
    };
    let dir = open(
        folder,
        OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_RDONLY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    let _ = nix::unistd::unlinkat(&dir, name, nix::unistd::UnlinkatFlags::NoRemoveDir);
    let file = openat(
        &dir,
        name,
        OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_WRONLY | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(std::io::Error::from)?;
    std::io::Write::write_all(&mut std::fs::File::from(file), bytes)
}

/// The tokens in Claude Code's login (`claudeAiOauth`'s access and refresh tokens), learned as
/// secrets obid knows, so whatever repeats them (an agent, the brain, a log) is redacted.
pub(super) fn learn_claude_tokens(bytes: &[u8], service: &str) {
    let Ok(login) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return;
    };
    for key in ["accessToken", "refreshToken"] {
        if let Some(token) = login["claudeAiOauth"][key]
            .as_str()
            .filter(|t| !t.is_empty())
        {
            // Named by its Keychain item, so a second account's never replaces the first's.
            crate::secrets::known().learn(&format!("{service} {key}"), token);
        }
    }
}

/// How a review's account is named in its author's `review_failed`: its tool and account id.
pub(super) fn reviewed_on(tool: &str, account: &str) -> String {
    format!("{tool}:{account}")
}

/// Codex's `auth.json` as obid copies it, jobs' and conversations' alike: all of it but the
/// refresh token, which the provider rotates, so a refresh inside a copy would sign the owner's own
/// Codex out (as #651 for Claude). `tokens.refresh_token` is kept as an empty string: codex-cli
/// 0.162.1 sends no credentials at all for ChatGPT tokens without that key (401, "Missing bearer").
/// `None` when it isn't JSON. obid never writes a token back.
pub(super) fn codex_without_refresh(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut login: Value = serde_json::from_slice(bytes).ok()?;
    if let Some(tokens) = login.get_mut("tokens").and_then(Value::as_object_mut) {
        tokens.insert("refresh_token".into(), json!(""));
    }
    serde_json::to_vec(&login).ok()
}

/// Where Cursor keeps its login when it keeps it in a file (`AGENT_CLI_CREDENTIAL_STORE=file`), from
/// its `HOME`: `~/.cursor/auth.json` on macOS, `$XDG_CONFIG_HOME/cursor/auth.json` elsewhere
/// (cursor-agent 2026.10.01's own credentials store); obid sets `XDG_CONFIG_HOME` to `~/.config`.
pub(crate) fn cursor_login_file() -> &'static str {
    if cfg!(target_os = "macos") {
        ".cursor/auth.json"
    } else {
        ".config/cursor/auth.json"
    }
}

/// Cursor's `auth.json` as obid copies it for a job: only what calls Cursor's API, its
/// `accessToken` and `apiKey`, and nothing else the file holds (never the refresh token, which a
/// refresh inside a box would rotate away from the account, #651, nor Bedrock's keys). Its tokens
/// are learned as secrets obid knows. `None` when it isn't a JSON object.
pub(super) fn cursor_without_refresh(bytes: &[u8], account: &Path) -> Option<Vec<u8>> {
    let login: Value = serde_json::from_slice(bytes).ok()?;
    let fields = login.as_object()?;
    for key in ["accessToken", "refreshToken", "apiKey"] {
        if let Some(token) = fields
            .get(key)
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
        {
            crate::secrets::known().learn(&format!("Cursor {} {key}", account.display()), token);
        }
    }
    let kept: serde_json::Map<String, Value> = ["accessToken", "apiKey"]
        .into_iter()
        .filter_map(|key| Some((key.to_owned(), fields.get(key)?.clone())))
        .collect();
    serde_json::to_vec(&kept).ok()
}

/// Whether a copied Codex login (`copy`, its `auth.json`) can run: none was copied, an API-key
/// login (no access token), or an access token whose JWT says it ends more than 5 minutes from now.
/// A copy can't refresh itself, so else the run stops with the one line that says so.
pub(crate) fn codex_copy_ready(copy: &Path, account: &str) -> Result<(), String> {
    let Ok(bytes) = std::fs::read(copy) else {
        return Ok(());
    };
    let login: Value = serde_json::from_slice(&bytes).unwrap_or_default();
    if codex_credential(&login).is_some_and(|c| ended_codex().lock().unwrap().contains(&c)) {
        return Err(codex_ended(account));
    }
    let ready = match login["tokens"]["access_token"].as_str() {
        None => login.is_object(),
        Some(token) => jwt_ends(token).is_some_and(|ends| ends > now_ms() / 1000 + 5 * 60),
    };
    ready.then_some(()).ok_or_else(|| {
        format!("The Mac's Codex login for {account} needs refreshing: run codex once.")
    })
}

/// What a Codex login OpenAI refused needs: a new sign-in, which only the owner can do.
pub(super) const CODEX_SIGN_IN: &str = "run codex login";

/// Why a run on a Codex login OpenAI refused doesn't start ([`end_codex_login`]).
pub(super) fn codex_ended(account: &str) -> String {
    format!("The Mac's Codex login for {account} has ended: {CODEX_SIGN_IN}.")
}

/// The Codex logins OpenAI refused, by their credential ([`codex_credential`]): never signed in
/// again until the login changes (a new `codex login` holds another token). A copy can't refresh,
/// and its token's `exp` still says it's good: OpenAI ends a session early (its refresh token
/// invalidated, `token_revoked`), as it did the owner's on 2026-10-21.
/// ponytail: in memory, so after a restart a dead login is tried once more; keep it on disk if
/// that run costs too much.
pub(super) fn ended_codex() -> &'static Mutex<std::collections::HashSet<u64>> {
    static ENDED: std::sync::OnceLock<Mutex<std::collections::HashSet<u64>>> =
        std::sync::OnceLock::new();
    ENDED.get_or_init(Default::default)
}

/// A Codex login's credential (its ChatGPT access token, else its API key), hashed: what obid
/// keeps of one OpenAI refused, never the token.
pub(super) fn codex_credential(login: &Value) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let token = login["tokens"]["access_token"]
        .as_str()
        .or(login["OPENAI_API_KEY"].as_str())
        .filter(|t| !t.is_empty())?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    Some(hasher.finish())
}

/// Codex's own words in a failed Codex run's `reason` (crate::codex): its turn's error (`Codex hit
/// an error: …`, when the agent said nothing that turn) or a request it refused (`Codex couldn't
/// …: …`). `None` when the reason is the agent's own last message, which may say anything.
pub(super) fn codex_error(reason: &str) -> Option<&str> {
    reason
        .strip_prefix("Codex hit an error: ")
        .or_else(|| reason.starts_with("Codex couldn't ").then_some(reason))
}

/// Whether a Codex run's error says OpenAI refused its login: codex-cli 0.162.1's
/// `workspace routing discovery unauthorized (401)` (`account/read`, `thread/start`), a `401
/// Unauthorized` from the API, or the session ended (`token_revoked`, `refresh_token_invalidated`).
/// A run on a copy can't refresh, so trying again changes nothing until the owner signs in.
pub(super) fn codex_login_refused(error: &str) -> bool {
    let error = error.to_lowercase();
    [
        "unauthorized (401)",
        "401 unauthorized",
        "token_revoked",
        "refresh_token_invalidated",
        "invalidated oauth token",
        "your session has ended",
    ]
    .iter()
    .any(|w| error.contains(w))
}

/// Remembers that OpenAI refused the Codex login a run used (`copy`, the run's `auth.json`), the
/// one of `account`: it isn't [`signed_in`] again (no review, failover or routing picks it) until
/// the account's login holds another token, as when the owner signed in again meanwhile.
pub(super) fn end_codex_login(copy: &Path, account: &str) {
    let credential = std::fs::read(copy)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|login| codex_credential(&login));
    if let Some(credential) = credential {
        tracing::warn!("OpenAI refused Codex account {account}'s login");
        ended_codex().lock().unwrap().insert(credential);
    }
}

/// When a JWT ends (its `exp`, seconds since 1970), if it says.
pub(super) fn jwt_ends(token: &str) -> Option<u64> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice::<Value>(&claims).ok()?["exp"].as_u64()
}

/// The tokens in Codex's login (`auth.json`: its ChatGPT access, refresh and id tokens, and an API
/// key), learned as secrets obid knows, so whatever repeats them (an agent, a log) is redacted.
pub(super) fn learn_codex_tokens(bytes: &[u8], account: &str) {
    let Ok(login) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return;
    };
    let tokens = &login["tokens"];
    for (key, token) in [
        ("access_token", &tokens["access_token"]),
        ("refresh_token", &tokens["refresh_token"]),
        ("id_token", &tokens["id_token"]),
        ("OPENAI_API_KEY", &login["OPENAI_API_KEY"]),
    ] {
        if let Some(token) = token.as_str().filter(|t| !t.is_empty()) {
            crate::secrets::known().learn(&format!("Codex {account} {key}"), token);
        }
    }
}

/// Every configured account's own folder that is a folder now (but one holding obid's home): what
/// no box reads, wherever it is (#655: macOS lets a box read what isn't hidden, and Linux binds `/`
/// read-only). A run reads its own copy of the login instead.
pub(crate) fn account_folders(agents: &crate::config::Agents, home: &Path) -> Vec<PathBuf> {
    [&agents.claude, &agents.codex, &agents.cursor]
        .into_iter()
        .flatten()
        .flat_map(|agent| &agent.accounts)
        .filter_map(|account| account.config_dir.clone())
        .filter(|dir| dir.is_dir() && !home.starts_with(dir))
        .collect()
}

/// Whether `folder` is a real folder, or not there yet: never a link a job left in its place,
/// which obid would otherwise follow (#643's security review).
pub(crate) fn not_a_link(folder: &Path) -> Result<(), String> {
    // Without a trailing `/` or `/.`, which would make the look follow a link.
    let folder: PathBuf = folder.components().collect();
    match std::fs::symlink_metadata(&folder) {
        Ok(meta) if !meta.is_dir() => Err(format!(
            "{} isn't a folder any more (a link or a file is in its place), so nothing started there.",
            folder.display()
        )),
        _ => Ok(()),
    }
}

/// A conversation's own copy of an account's login (crate::brain::harness), written into `to`
/// without following a link, nothing else of the account's folder. Claude Code's: the macOS
/// Keychain's item for the account's folder when obid copies from there, else that folder's
/// `.credentials.json` (a regular file only), only what calls the API ([`account_login`]). Codex's:
/// its `auth.json` (a regular file only), its tokens learned as secrets.
pub(crate) fn copy_account_login(tool: Tool, account: &Path, to: &Path) -> std::io::Result<()> {
    not_a_link(to).map_err(std::io::Error::other)?;
    store::make_private_dir(to)?;
    if tool == Tool::Codex {
        let file = account.join("auth.json");
        if !std::fs::symlink_metadata(&file).is_ok_and(|m| m.is_file()) {
            return Ok(());
        }
        let bytes = std::fs::read(&file)?;
        learn_codex_tokens(&bytes, &account.display().to_string());
        let Some(copy) = codex_without_refresh(&bytes) else {
            return Ok(());
        };
        return write_login(&to.join("auth.json"), &copy);
    }
    // Only what calls the API, never a refresh token, from the Keychain or the folder's file
    // ([`account_login`], #651). A job's or a conversation's copy is kept fresh by the keeper
    // while the account's login is kept ([`keep_folder_copy`]).
    let Some(bytes) = account_login(account) else {
        return Ok(());
    };
    learn_claude_tokens(&bytes, &keychain_service(Some(account)));
    write_login(&to.join(".credentials.json"), &bytes)
}

/// A job's or a conversation's copy of an account folder's Claude Code login (`login`'s
/// `.credentials.json`), written again by the keeper while it keeps that account's login fresh
/// (#877: only an account obid added; for any other, nothing is kept). Let go when the run or
/// turn ends ([`forget_login_secrets`]).
pub(super) fn keep_folder_copy(account: &Path, login: &Path) {
    keep_keychain_copy(
        &keychain_service(Some(account)),
        login.join(".credentials.json"),
    );
}

/// A job's own folder (`login`) for an account with a folder of its own (`account`), never the
/// account's folder itself (#655), each copy written without following a link:
/// - Claude Code (`CLAUDE_CONFIG_DIR`): the login as [`copy_account_login`] copies it, without its
///   refresh token, again before every start; once, the account's `.claude.json` trimmed to what
///   keeps it signed in ([`trimmed_claude_json`]).
/// - Codex (`CODEX_HOME`): `auth.json` without its refresh token, again before every start; never
///   its `config.toml` (MCP servers and their tokens).
/// - Cursor (`HOME`, `XDG_CONFIG_HOME`, `CURSOR_CONFIG_DIR`): its login file ([`cursor_login_file`])
///   as only its access token and API key ([`cursor_without_refresh`]), again before every start;
///   once, its `cli-config.json`, then obid's settings over it ([`acp::cursor::rewrite_config`]);
///   never its `mcp.json`.
///
/// The rest is the job's own: what it writes there reaches no other run and no unboxed process.
pub(crate) fn copy_account_folder(tool: Tool, account: &Path, login: &Path) -> std::io::Result<()> {
    not_a_link(login).map_err(std::io::Error::other)?;
    store::make_private_dir(login)?;
    // A regular file of the account's, as it is.
    let read = |name: &str| -> Option<Vec<u8>> {
        let from = account.join(name);
        std::fs::symlink_metadata(&from)
            .is_ok_and(|m| m.is_file())
            .then(|| std::fs::read(&from).ok())
            .flatten()
    };
    let once = |name: &str, bytes: Option<Vec<u8>>| -> std::io::Result<()> {
        match bytes {
            Some(bytes) if std::fs::symlink_metadata(login.join(name)).is_err() => {
                write_login(&login.join(name), &bytes)
            }
            _ => Ok(()),
        }
    };
    match tool {
        Tool::Claude => {
            copy_account_login(tool, account, login)?;
            let state =
                read(".claude.json").map_or_else(|| b"{}".to_vec(), |b| trimmed_claude_json(&b));
            once(".claude.json", Some(state))
        }
        // Only its login: never the account's `config.toml`, whose MCP servers would start inside
        // the box with whatever tokens they carry (agents have no code-host login).
        Tool::Codex => copy_account_login(tool, account, login),
        Tool::Cursor => copy_cursor(
            read(cursor_login_file()),
            &account.join("cli-config.json"),
            account,
            login,
        ),
    }
}

/// Cursor's own login in `home`: its login file (a regular file, as it is), else its Keychain
/// item's ([`cursor_keychain_login`]), where cursor-agent keeps it by default on macOS.
pub(super) fn own_cursor_login(home: &Path) -> Option<Vec<u8>> {
    let file = home.join(cursor_login_file());
    if is_regular_file(&file) {
        return std::fs::read(file).ok();
    }
    cursor_keychain_login()
}

/// A Cursor job's own folder (`login`, its `HOME`) from the login file's bytes `from` and the
/// settings `settings` (`cli-config.json`), an account's or Cursor's own (`~/.cursor`, its login
/// file or Keychain item, [`own_cursor_login`]): the login as only its access token and API key
/// ([`cursor_without_refresh`]), again before every start; once, the settings, then obid's over them
/// ([`acp::cursor::rewrite_config`]); never its `mcp.json` (MCP servers starting inside the box,
/// with their tokens). `account` names the login's secrets.
pub(super) fn copy_cursor(
    from: Option<Vec<u8>>,
    settings: &Path,
    account: &Path,
    login: &Path,
) -> std::io::Result<()> {
    not_a_link(login).map_err(std::io::Error::other)?;
    store::make_private_dir(login)?;
    // A regular file, as it is.
    let read = |from: &Path| -> Option<Vec<u8>> {
        std::fs::symlink_metadata(from)
            .is_ok_and(|m| m.is_file())
            .then(|| std::fs::read(from).ok())
            .flatten()
    };
    let file = cursor_login_file();
    if let Some(copy) = from.and_then(|bytes| cursor_without_refresh(&bytes, account)) {
        let mut folder = login.to_path_buf();
        for part in Path::new(file)
            .parent()
            .into_iter()
            .flat_map(Path::components)
        {
            folder.push(part);
            not_a_link(&folder).map_err(std::io::Error::other)?;
            store::make_private_dir(&folder)?;
        }
        write_login(&login.join(file), &copy)?;
    }
    let config = login.join("cli-config.json");
    if std::fs::symlink_metadata(&config).is_err()
        && let Some(bytes) = read(settings)
    {
        write_login(&config, &bytes)?;
    }
    acp::cursor::rewrite_config(login)
}

/// The tool's own login, copied from `home` into the job's `login` folder, for an account with no
/// `configDir`: Claude Code's `~/.claude/.credentials.json` and `~/.claude.json`, Codex's
/// `~/.codex/auth.json`. Nothing else of those folders (no settings, hooks, commands or sessions of
/// the person's; never Codex's `config.toml`, whose MCP servers and `notify` start programs). The credentials are copied again before every start (the
/// person may have signed in again) and removed when the job's run ends
/// ([`forget_login_secrets`]); the rest is copied once, then the job's own. Answers the names to
/// show in the job's `HOME`. On macOS a login that lives only in the Keychain is copied from there
/// as `.credentials.json` ([`keychain_login`]).
pub(crate) fn copy_login(
    home: Option<&Path>,
    login: &Path,
    tool: Tool,
    keychain: bool,
) -> std::io::Result<Copied> {
    let (shown, secret, once): (&[&'static str], &[&str], &[&str]) = match tool {
        Tool::Codex => (&[".codex"], &[".codex/auth.json"], &[]),
        _ => (
            &[".claude", ".claude.json"],
            &[".claude/.credentials.json"],
            &[".claude.json"],
        ),
    };
    not_a_link(login).map_err(std::io::Error::other)?;
    store::make_private_dir(login)?;
    // The job may have changed its login folder: obid writes only into a real folder there,
    // never through a link the job left in its place.
    let folder = login.join(shown[0]);
    if std::fs::symlink_metadata(&folder).is_ok_and(|m| !m.is_dir()) {
        std::fs::remove_file(&folder)?;
    }
    store::make_private_dir(&folder)?;
    let Some(home) = home else {
        return Ok(Copied {
            names: shown.to_vec(),
            from_keychain: false,
        });
    };
    let copy = |name: &str| -> std::io::Result<()> {
        let from = home.join(name);
        let to = login.join(name);
        // A regular file only: a link or a folder in its place is not a login.
        match std::fs::symlink_metadata(&from) {
            Ok(meta) if meta.is_file() => {}
            _ => return Ok(()),
        }
        let mut bytes = std::fs::read(&from)?;
        if tool == Tool::Codex {
            learn_codex_tokens(&bytes, "~/.codex");
            let Some(copy) = codex_without_refresh(&bytes) else {
                return Ok(());
            };
            bytes = copy;
        }
        let _ = std::fs::remove_file(&to);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        std::io::Write::write_all(&mut options.open(&to)?, &bytes)
    };
    for name in secret {
        copy(name)?;
    }
    if tool == Tool::Claude
        && let Ok(bytes) = std::fs::read(login.join(secret[0]))
    {
        learn_claude_tokens(&bytes, &keychain_service(None));
    }
    // On macOS Claude Code's login may be only in the Keychain (keychain_login).
    let mut from_keychain = false;
    if keychain
        && tool == Tool::Claude
        && std::fs::symlink_metadata(home.join(secret[0])).is_err()
        && let Some(bytes) = keychain_login(None)
    {
        write_login(&login.join(secret[0]), &bytes)?;
        from_keychain = true;
    }
    for name in once {
        if std::fs::symlink_metadata(login.join(name)).is_err() {
            if *name == ".claude.json" {
                let trimmed = std::fs::read(home.join(name))
                    .map(|bytes| trimmed_claude_json(&bytes))
                    .unwrap_or_else(|_| b"{}".to_vec());
                write_login(&login.join(name), &trimmed)?;
            } else {
                copy(name)?;
            }
        }
    }
    Ok(Copied {
        names: shown.to_vec(),
        from_keychain,
    })
}

/// What [`copy_login`] copied: the names to show in the run's `HOME`, and whether the login came
/// from the Keychain (only such a copy is kept fresh, [`keep_keychain_logins`]).
pub(crate) struct Copied {
    pub names: Vec<&'static str>,
    pub from_keychain: bool,
}

/// `~/.claude.json` with only the keys a boxed Claude Code needs to stay signed in: the account,
/// the user id and the onboarding flags. Never the person's projects, history, MCP servers or
/// anything else that could carry their data or instructions into a job or a conversation (N5).
/// `{}` when it doesn't parse as an object.
pub(super) fn trimmed_claude_json(bytes: &[u8]) -> Vec<u8> {
    let Ok(serde_json::Value::Object(all)) = serde_json::from_slice::<serde_json::Value>(bytes)
    else {
        return b"{}".to_vec();
    };
    let mut kept = serde_json::Map::new();
    for (key, value) in all {
        if key == "oauthAccount" || key == "userID" || key.to_ascii_lowercase().contains("onboard")
        {
            kept.insert(key, value);
        }
    }
    serde_json::to_vec(&serde_json::Value::Object(kept)).unwrap_or_else(|_| b"{}".to_vec())
}

/// Removes the copied credentials from a job's login folder once its run ended: they are copied
/// again before its next start.
pub(crate) fn forget_login_secrets(login: &Path) {
    forget_keychain_copy(&login.join(".claude/.credentials.json"));
    forget_keychain_copy(&login.join(".credentials.json"));
    if !std::fs::symlink_metadata(login).is_ok_and(|m| m.is_dir()) {
        return;
    }
    for name in [
        // A copy of an account's folder's login ([`copy_account_folder`]).
        ".credentials.json",
        "auth.json",
        ".config/cursor/auth.json",
        ".cursor/auth.json",
        // A copy of the tool's own login ([`copy_login`]).
        ".claude/.credentials.json",
        ".codex/auth.json",
    ] {
        remove_inside(login, name);
    }
}

/// What a new session hears first when it takes over a job from `was`, which hit its usage limit
/// (#871): where the work is, and what the earlier rounds reported, quoted, with every secret obid
/// knows taken out.
pub(super) fn handoff(job: &Job, was: &str) -> String {
    let note = format!(
        "{was} was working on this job and hit its account's usage limit. You're taking it over in the same worktree and branch: its commits are on the branch, and it may have left changes uncommitted. Check `git status` and `git log` before you go on."
    );
    match job.summaries.as_ref().filter(|s| !s.is_empty()) {
        Some(summaries) => {
            let nonce = crate::brain::briefing::nonce();
            let said = crate::secrets::known().redact(&summaries.join("\n\n"));
            format!(
                "{note}\n\nWhat its earlier rounds reported, quoted:\n\n{}\n{}",
                crate::brain::briefing::fence_rule(&nonce, "the agent before you"),
                crate::brain::briefing::fence(&nonce, &said)
            )
        }
        None => note,
    }
}

/// Removes `login/<name>`, each folder on the way looked at without following a link: a link (or
/// a file) a job left in place of one is removed itself, and nothing past it is touched.
pub(super) fn remove_inside(login: &Path, name: &str) {
    let mut path = login.to_path_buf();
    let mut parts = name.split('/').peekable();
    while let Some(part) = parts.next() {
        path.push(part);
        if parts.peek().is_none() {
            let _ = std::fs::remove_file(&path);
            return;
        }
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                let _ = std::fs::remove_file(&path);
                return;
            }
            Err(_) => return,
        }
    }
}

/// The person's home folders: obid's `HOME`, and the one the password database names (they
/// differ when obid was given a `HOME` of its own).
pub fn person_homes() -> Vec<PathBuf> {
    let mut homes = Vec::new();
    if let Ok(Some(user)) = nix::unistd::User::from_uid(nix::unistd::getuid())
        && user.dir.is_absolute()
        && user.dir != Path::new("/")
    {
        homes.push(user.dir);
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && home.is_absolute()
        && home != Path::new("/")
        && !homes.contains(&home)
    {
        homes.push(home);
    }
    homes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// KEYCHAIN-3: a Keychain login's value is asked for once per change of the item: a value
    /// macOS gave is kept while the item stays the same (an "Allow" isn't asked again every
    /// minute), a refusal stands until the item changes (a sign-in rewrites it), and a locked
    /// Keychain is never kept, so the next read asks again.
    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_3_a_login_is_asked_for_once_per_change() {
        let dir = std::env::temp_dir().join(format!("obid-refused-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (mdat, answer, asked) = (dir.join("mdat"), dir.join("answer"), dir.join("asked"));
        // Without -w the item's date; with -w what `answer` says: Deny (128), locked (36), a value.
        let script = format!(
            "#!/bin/sh\ncase \" $* \" in *\" -w \"*) echo x >> {asked:?}; a=$(cat {answer:?}); \
             [ \"$a\" = refuse ] && exit 128; [ \"$a\" = locked ] && exit 36; echo \"$a\";; \
             *) printf '    \"mdat\"<timedate>=\"%s\"\\n' \"$(cat {mdat:?})\";; esac\n"
        );
        let bin = dir.join("security");
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let service = format!("Claude Code-credentials-test-{}", std::process::id());
        let ask = || {
            let mut command = std::process::Command::new(&bin);
            command.args(["find-generic-password", "-s", &service]);
            ask_security(&service, command, true)
        };
        let times = || {
            std::fs::read_to_string(&asked)
                .unwrap_or_default()
                .lines()
                .count()
        };
        let set = |path: &Path, text: &str| std::fs::write(path, text).unwrap();

        // Deny: asked once, refused while the item stays the same, said on the account.
        set(&mdat, "1");
        set(&answer, "refuse");
        assert_eq!(ask(), None);
        assert_eq!(ask(), None);
        assert_eq!(times(), 1, "asked once");
        assert!(refused(&service));
        assert!(
            keychain_unread(Tool::Claude, &service)
                .unwrap()
                .contains("Always Allow")
        );
        // A sign-in rewrites the item: the refusal goes, once obid looks again.
        set(&mdat, "2");
        set(&answer, "token-one");
        ANSWERS.lock().unwrap().get_mut(&service).unwrap().looked -= LOOK_AGAIN;
        assert!(!refused(&service));
        assert_eq!(ask(), Some(b"token-one\n".to_vec()));
        // Allowed: kept while the item stays the same, so an "Allow" isn't asked again.
        assert_eq!(ask(), Some(b"token-one\n".to_vec()));
        assert_eq!(times(), 2);
        // A refresh rewrites it: read again, once.
        set(&mdat, "3");
        set(&answer, "token-two");
        assert_eq!(ask(), Some(b"token-two\n".to_vec()));
        assert_eq!(times(), 3);
        // Locked: never kept, said as such, and asked again at the next read.
        set(&mdat, "4");
        set(&answer, "locked");
        assert_eq!(ask(), None);
        assert!(!refused(&service));
        assert_eq!(
            keychain_unread(Tool::Claude, &service).as_deref(),
            Some(KEYCHAIN_LOCKED)
        );
        set(&answer, "token-three");
        assert_eq!(ask(), Some(b"token-three\n".to_vec()));
        assert_eq!(keychain_unread(Tool::Claude, &service), None);
        assert_eq!(times(), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Swift runner's vault items are found by name in `security dump-keychain`'s attributes,
    /// and no other service's (#1679).
    #[test]
    fn the_swift_runners_vault_items_are_named_from_their_attributes() {
        let dump = r#"keychain: "/Users/o/Library/Keychains/login.keychain-db"
class: "genp"
attributes:
    "acct"<blob>="oby.tetrix.access"
    "svce"<blob>="com.deskree.oby.runner.vault"
keychain: "/Users/o/Library/Keychains/login.keychain-db"
class: "genp"
attributes:
    "acct"<blob>="someone"
    "svce"<blob>="Claude Code-credentials"
keychain: "/Users/o/Library/Keychains/login.keychain-db"
class: "genp"
attributes:
    "acct"<blob>="oby.fcm.serviceAccount"
    "mdat"<timedate>=0x3230 "20261031120000Z\000"
    "svce"<blob>="com.deskree.oby.runner.vault"
"#;
        assert_eq!(
            vault_names_in(dump),
            ["oby.fcm.serviceAccount", "oby.tetrix.access"]
        );
        assert!(vault_names_in("").is_empty());
    }

    /// N5: a copied `~/.claude.json` keeps only the keys a boxed run needs to stay signed in, never
    /// the person's projects, history or MCP state.
    #[test]
    fn claude_json_is_trimmed_to_the_auth_keys() {
        let full = serde_json::json!({
            "oauthAccount": {"emailAddress": "a@b.c"},
            "userID": "u1",
            "hasCompletedOnboarding": true,
            "projects": {"/Users/me/secret": {"history": ["did x"]}},
            "mcpServers": {"evil": {}},
            "tipsHistory": {"x": 1}
        });
        let trimmed: serde_json::Value =
            serde_json::from_slice(&trimmed_claude_json(full.to_string().as_bytes())).unwrap();
        assert_eq!(trimmed["oauthAccount"]["emailAddress"], "a@b.c");
        assert_eq!(trimmed["userID"], "u1");
        assert_eq!(trimmed["hasCompletedOnboarding"], true);
        assert!(trimmed.get("projects").is_none());
        assert!(trimmed.get("mcpServers").is_none());
        assert!(trimmed.get("tipsHistory").is_none());
        assert_eq!(trimmed_claude_json(b"not json"), b"{}");
        assert_eq!(trimmed_claude_json(b"[1,2]"), b"{}");
    }

    /// claudeKeychain left out: on only on macOS, only with no login file, only when the Keychain
    /// holds Claude Code's item; the Keychain isn't asked when the file is there (the owner's
    /// decision of 2026-10-18).
    #[test]
    fn the_keychain_turns_on_by_itself_only_for_a_keychain_only_login() {
        assert!(wanted(true, false, || true));
        assert!(!wanted(true, false, || false));
        assert!(!wanted(true, true, || panic!(
            "the Keychain isn't asked when the file is there"
        )));
        assert!(!wanted(false, false, || true));
    }

    /// #643's security review: a job that put a link in place of its account folder can't make obid
    /// write the login wherever the link points; a real folder still gets it, as only the person
    /// may read it.
    #[test]
    fn a_login_is_never_written_through_a_link() {
        let root = std::env::temp_dir().join(format!("obid-login-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (real, elsewhere, planted) = (
            root.join("real"),
            root.join("elsewhere"),
            root.join("planted"),
        );
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &planted).unwrap();
        assert!(write_login(&planted.join(".credentials.json"), b"{}").is_err());
        assert!(!elsewhere.join(".credentials.json").exists());
        write_login(&real.join(".credentials.json"), b"{}").unwrap();
        let mode = std::fs::metadata(real.join(".credentials.json"))
            .unwrap()
            .permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The tokens in a copied Keychain login are secrets obid knows: what repeats them is redacted.
    #[test]
    fn a_keychain_logins_tokens_are_redacted() {
        let access = ["sk-ant-oat01-", &"x".repeat(40)].concat();
        let refresh = ["sk-ant-ort01-", &"y".repeat(40)].concat();
        let login =
            serde_json::json!({"claudeAiOauth": {"accessToken": access, "refreshToken": refresh}});
        learn_claude_tokens(login.to_string().as_bytes(), "Claude Code-credentials");
        // A second account's login doesn't push the first's out (#643's re-review).
        let other = ["sk-ant-oat01-", &"z".repeat(40)].concat();
        let second = serde_json::json!({"claudeAiOauth": {"accessToken": other}});
        learn_claude_tokens(
            second.to_string().as_bytes(),
            "Claude Code-credentials-2e25c0fb",
        );
        let said = crate::secrets::known().redact(&format!("{access} and {refresh} and {other}"));
        assert!(
            !said.contains(&access) && !said.contains(&refresh) && !said.contains(&other),
            "{said}"
        );
    }

    /// A copy of the Keychain's login carries only what calls the API: never a refresh token
    /// (a boxed run that refreshed with it would rotate the Mac's own login away, 2026-10-17), nor
    /// anything else the item holds (MCP logins, plug-in secrets, device tokens; #651's review).
    #[test]
    fn a_keychain_copy_carries_no_refresh_token() {
        let login = json!({
            "claudeAiOauth": {
                "accessToken": "access",
                "refreshToken": "refresh",
                "refreshTokenExpiresAt": 1,
                "expiresAt": 1_760_000_000_000u64,
                "scopes": ["user:inference"],
                "subscriptionType": "max",
                "rateLimitTier": "default_claude_max_20x",
                "clientId": "client",
            },
            "mcpOAuth": {"server|1": {"accessToken": "m", "refreshToken": "mr"}},
            "mcpOAuthClientConfig": {"server|1": {"clientSecret": "s"}},
            "mcpXaaIdp": {"refreshToken": "x"},
            "mcpXaaIdpConfig": {"clientSecret": "x"},
            "pluginSecrets": {"plugin": "p"},
            "trustedDeviceToken": "t",
        });
        let copy: Value =
            serde_json::from_slice(&without_refresh(login.to_string().as_bytes()).unwrap())
                .unwrap();
        assert_eq!(
            copy,
            json!({"claudeAiOauth": {
                "accessToken": "access",
                "expiresAt": 1_760_000_000_000u64,
                "scopes": ["user:inference"],
                "subscriptionType": "max",
                "rateLimitTier": "default_claude_max_20x",
            }})
        );
        // Not a login, or one with no access token: nothing to copy.
        assert!(without_refresh(b"").is_none());
        assert!(without_refresh(br#"{"mcpOAuth": {}}"#).is_none());
    }

    /// A Claude Code login as the Keychain has it, its token ending at `ends` (none: not said).
    fn a_login(ends: Option<u64>) -> Option<Vec<u8>> {
        let mut oauth = json!({"accessToken": "a", "scopes": ["user:inference"]});
        if let Some(ends) = ends {
            oauth["expiresAt"] = ends.into();
        }
        Some(json!({"claudeAiOauth": oauth}).to_string().into_bytes())
    }

    /// The Mac's own Claude Code refreshes a login obid copies when its token ends within 5
    /// minutes (when Claude Code refreshes at all) or doesn't say when, and not before; a refresh
    /// isn't tried again for 4 minutes, so a failed one isn't hammered.
    #[test]
    fn the_macs_claude_code_refreshes_a_login_near_its_end() {
        let now = 1_760_000_000_000u64;
        let minute = 60_000;
        let ran = std::cell::Cell::new(0);
        let refresh = |works: bool| {
            ran.set(ran.get() + 1);
            works
        };
        // Six minutes off: nothing runs (Claude Code wouldn't refresh yet).
        let mut last = None;
        let far = || a_login(Some(now + 6 * minute));
        assert_eq!(
            keep_fresh(&mut last, now, far, || refresh(true)),
            Fresh::Good
        );
        assert_eq!(ran.get(), 0);
        // No Keychain login: nothing to copy, nothing runs.
        assert_eq!(
            keep_fresh(&mut last, now, || None, || refresh(true)),
            Fresh::Good
        );
        assert_eq!(ran.get(), 0);
        // Within 5 minutes: it runs.
        let near = || a_login(Some(now + 4 * minute));
        assert_eq!(
            keep_fresh(&mut None, now, near, || refresh(true)),
            Fresh::Renewed
        );
        assert_eq!(ran.get(), 1);
        // No end said: it runs; one that fails means signing in again...
        let mut last = None;
        let unsaid = || a_login(None);
        assert_eq!(
            keep_fresh(&mut last, now, unsaid, || refresh(false)),
            Fresh::SignIn
        );
        assert_eq!(ran.get(), 2);
        // ...and isn't tried again within 4 minutes.
        assert_eq!(
            keep_fresh(&mut last, now, unsaid, || refresh(true)),
            Fresh::SignIn
        );
        assert_eq!(ran.get(), 2);
        // In those 4 minutes a token that hasn't ended still works.
        assert_eq!(
            keep_fresh(&mut last, now, near, || refresh(true)),
            Fresh::Good
        );
        assert_eq!(ran.get(), 2);
        // A refresh that worked but left a token already ended fails too.
        let ended = || a_login(Some(now - 1));
        assert_eq!(
            keep_fresh(&mut None, now, ended, || refresh(true)),
            Fresh::SignIn
        );
        assert_eq!(ran.get(), 3);
    }

    /// Many runs starting at once for the same account refresh it once: the rest wait for that
    /// one and find the login fresh, or within the 4 minutes after a failed one.
    #[test]
    fn runs_starting_at_once_refresh_once() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let now = 1_760_000_000_000u64;
        for works in [true, false] {
            let service = format!("obid-test-{}-{works}", std::process::id());
            let ran = AtomicUsize::new(0);
            let refreshed = AtomicBool::new(false);
            let read = || {
                let fresh = refreshed.load(Ordering::SeqCst);
                a_login(Some(if fresh { now + 3_600_000 } else { now + 60_000 }))
            };
            let refresh = || {
                ran.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(50));
                refreshed.store(works, Ordering::SeqCst);
                works
            };
            std::thread::scope(|scope| {
                for _ in 0..8 {
                    scope.spawn(|| refresh_if_due(&service, now, read, refresh));
                }
            });
            assert_eq!(ran.load(Ordering::SeqCst), 1, "works: {works}");
        }
    }

    /// After a refresh, every live copy is written again from the Keychain: one whose run ended
    /// (its file removed) is forgotten, never made again.
    /// A job queued while its account is signed in or removed waits (its account's change is
    /// under way), and starts once the change ends, instead of failing.
    /// Git in a job's sandbox is held to its time and its output's size (#736).
    #[tokio::test]
    async fn sandboxed_output_is_held_to_its_limits() {
        let sh =
            |script: &str| -> Vec<OsString> { vec!["/bin/sh".into(), "-c".into(), script.into()] };
        let dir = std::env::temp_dir();
        let run = |script: &'static str, limit: u64, max: usize| {
            let argv = sh(script);
            let dir = dir.clone();
            async move {
                sandboxed_output(&argv, &[], &dir, None, Duration::from_secs(limit), max).await
            }
        };
        assert_eq!(run("printf hello", 10, 5).await.unwrap(), b"hello");
        let too_much = run("yes | head -c 100000", 10, 100).await.unwrap_err();
        assert!(
            too_much.contains("printed more than 100 bytes"),
            "{too_much}"
        );
        // A failure's last line is said, however much came before it on stderr.
        let said = run(
            "yes noise | head -n 200000 >&2; echo the last line >&2; false",
            10,
            100,
        )
        .await
        .unwrap_err();
        assert!(said.ends_with("failed: the last line"), "{said}");
        let too_long = run("sleep 30", 1, 100).await.unwrap_err();
        assert!(too_long.contains("took more than 1 seconds"), "{too_long}");
    }

    #[tokio::test]
    async fn a_job_queued_during_an_accounts_change_waits_for_it() {
        let root = std::env::temp_dir().join(format!("obid-changing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config = crate::config::Config::parse(
            &serde_json::json!({"listen": "127.0.0.1:0", "home": root.join("home"), "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://example.com/o/r.git"}]})
            .to_string(),
        )
        .unwrap();
        let jobs = Jobs::open(
            config,
            crate::events::Events::new(),
            Err("no sandbox".into()),
        )
        .unwrap();
        let changing = jobs.begin_change("a").unwrap();
        let job = Job::new(
            "j".into(),
            "o/r".into(),
            "fix".into(),
            "Fix it".into(),
            "claude".into(),
            "a".into(),
        );
        jobs.insert(job);
        jobs.schedule();
        assert!(!jobs.lock().workers.contains_key("j"));
        assert_eq!(jobs.get("j").unwrap().status, Status::Queued);
        drop(changing);
        let started = (0..100).any(|_| {
            jobs.lock().workers.contains_key("j")
                || jobs.get("j").is_some_and(|j| j.status != Status::Queued)
        });
        assert!(started, "the job never started once the change ended");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_waiting_claude_code_runs_a_turn_only_on_a_copy_that_lasts() {
        let root = std::env::temp_dir().join(format!("obid-lasts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        assert!(login_copy_lasts(&root), "no copy: nothing ends");
        let ends = |ms: u64| format!(r#"{{"claudeAiOauth": {{"expiresAt": {ms}}}}}"#);
        let file = root.join(".claude/.credentials.json");
        std::fs::write(&file, ends(now_ms() + 60 * 60_000)).unwrap();
        assert!(login_copy_lasts(&root));
        std::fs::write(&file, ends(now_ms() + 60_000)).unwrap();
        assert!(!login_copy_lasts(&root), "it ends within 5 minutes");
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(root.join("elsewhere"), &file).unwrap();
        assert!(!login_copy_lasts(&root), "a link isn't followed");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_refresh_rewrites_the_live_copies_only() {
        let root = std::env::temp_dir().join(format!("obid-copies-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (live, ended) = (root.join("live"), root.join("ended"));
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&ended).unwrap();
        let (live, ended) = (
            live.join(".credentials.json"),
            ended.join(".credentials.json"),
        );
        std::fs::write(&live, b"old").unwrap();
        let mut copies = vec![live.clone(), ended.clone()];
        rewrite(&mut copies, b"new");
        assert_eq!(std::fs::read(&live).unwrap(), b"new");
        assert!(!ended.exists());
        assert_eq!(copies, vec![live]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The refresh never lets a box-writable folder steer the unboxed Claude Code (#651's
    /// review): a job could plant `.claude.json` with an `env` (Claude Code applies it whatever
    /// `--setting-sources` says) and a `security` of its own on that `PATH`. The command runs
    /// `claude` by its absolute path, in a fresh empty config folder of obid's, with a `PATH` of
    /// the system's folders only; the account's folder only names its Keychain item.
    #[test]
    fn a_refresh_is_never_steered_by_the_accounts_folder() {
        let root = std::env::temp_dir().join(format!("obid-steer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let account = root.join("account");
        std::fs::create_dir_all(account.join("bin")).unwrap();
        std::fs::write(account.join("bin/security"), "#!/bin/sh\n").unwrap();
        let planted = json!({"env": {"PATH": account.join("bin"), "HTTPS_PROXY": "http://x"}});
        std::fs::write(account.join(".claude.json"), planted.to_string()).unwrap();
        let scratch = root.join("scratch");
        let bin = Path::new("/opt/claude/bin/claude");
        let command = refresh_command(
            bin,
            Some(account.as_path()),
            &scratch,
            Path::new("/Users/me"),
        );
        assert_eq!(command.get_program(), bin.as_os_str());
        let env: HashMap<String, Option<String>> = command
            .get_envs()
            .map(|(k, v)| {
                let v = v.map(|v| v.to_string_lossy().into_owned());
                (k.to_string_lossy().into_owned(), v)
            })
            .collect();
        let get = |name: &str| env.get(name).cloned().flatten();
        assert_eq!(
            get("CLAUDE_CONFIG_DIR"),
            Some(scratch.display().to_string())
        );
        assert_eq!(
            get("CLAUDE_SECURESTORAGE_CONFIG_DIR"),
            Some(account.display().to_string())
        );
        assert_eq!(
            get("PATH").as_deref(),
            Some("/usr/bin:/bin:/usr/sbin:/sbin")
        );
        assert_eq!(get("HTTPS_PROXY"), std::env::var("HTTPS_PROXY").ok());
        // Nothing else names the account's folder, and the config folder is obid's own, empty.
        for (name, value) in &env {
            let value = value.clone().unwrap_or_default();
            assert!(
                name == "CLAUDE_SECURESTORAGE_CONFIG_DIR" || !value.contains("account"),
                "{name}={value}"
            );
        }
        assert_eq!(command.get_current_dir(), Some(scratch.as_path()));
        // The default login's item: an empty storage folder name.
        let command = refresh_command(bin, None, &scratch, Path::new("/Users/me"));
        let storage = command
            .get_envs()
            .find(|(k, _)| *k == "CLAUDE_SECURESTORAGE_CONFIG_DIR")
            .and_then(|(_, v)| v);
        assert_eq!(storage, Some(std::ffi::OsStr::new("")));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An account obid added itself is refreshed by the Mac's own Claude Code, its Keychain item
    /// named by its folder (#877); never one whose login files in the folder aren't the person's
    /// own: Claude Code keeps the item's locks and plaintext fallback there. Nothing runs then.
    #[test]
    fn a_folder_is_refreshed_only_while_its_login_files_are_its_own() {
        let root = std::env::temp_dir().join(format!("obid-folder-refresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let folder = root.join("account");
        std::fs::create_dir_all(&folder).unwrap();
        let ran = root.join("ran");
        let bin = root.join("claude");
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\nprintf '%s' \"$CLAUDE_SECURESTORAGE_CONFIG_DIR\" > '{}'\n",
                ran.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        assert!(refresh_claude_login(&bin, Some(&folder)));
        assert_eq!(
            std::fs::read_to_string(&ran).unwrap(),
            folder.display().to_string()
        );
        std::fs::remove_file(&ran).unwrap();
        std::os::unix::fs::symlink(root.join("elsewhere"), folder.join(".credentials.json"))
            .unwrap();
        assert!(!refresh_claude_login(&bin, Some(&folder)));
        assert!(!ran.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Taking a run's login copy out never follows a link the job left in its login folder: a
    /// `.config` or `.cursor` leading out of the box is removed, and what it led to stays.
    #[test]
    fn forgetting_a_login_never_follows_a_link_out() {
        let root = std::env::temp_dir().join(format!("obid-forget-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (login, outside) = (root.join("login"), root.join("outside"));
        std::fs::create_dir_all(&login).unwrap();
        std::fs::create_dir_all(outside.join("cursor")).unwrap();
        std::fs::write(outside.join("cursor/auth.json"), "person's").unwrap();
        std::fs::write(outside.join("auth.json"), "person's").unwrap();
        std::os::unix::fs::symlink(&outside, login.join(".config")).unwrap();
        std::os::unix::fs::symlink(&outside, login.join(".cursor")).unwrap();
        std::os::unix::fs::symlink(&outside, login.join(".claude")).unwrap();
        forget_login_secrets(&login);
        assert_eq!(
            std::fs::read_to_string(outside.join("cursor/auth.json")).unwrap(),
            "person's"
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("auth.json")).unwrap(),
            "person's"
        );
        for name in [".config", ".cursor", ".claude"] {
            assert!(
                std::fs::symlink_metadata(login.join(name)).is_err(),
                "{name} stayed"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A Codex job's copy of its account's folder is its login only, never its `config.toml`, whose
    /// MCP servers would start inside the box with whatever tokens they carry.
    #[test]
    fn a_codex_job_gets_its_login_and_not_its_config() {
        let root = std::env::temp_dir().join(format!("obid-codex-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (account, login) = (root.join("account"), root.join("login"));
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(
            account.join("auth.json"),
            json!({"tokens": {"access_token": "a", "refresh_token": "r"}}).to_string(),
        )
        .unwrap();
        std::fs::write(
            account.join("config.toml"),
            "[mcp_servers.github]\ncommand = \"x\"\nenv = { GITHUB_PAT = \"made-up\" }\n",
        )
        .unwrap();
        copy_account_folder(Tool::Codex, &account, &login).unwrap();
        assert!(login.join("auth.json").is_file());
        assert!(
            !login.join("config.toml").exists(),
            "the account's config reached the box"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A Cursor job's copy of its account's login is only its access token and API key: a box that
    /// refreshed would rotate the account's own login away, as #651 found for Claude Code. They're
    /// learned as secrets obid knows, and the account's `mcp.json` isn't copied.
    #[test]
    fn a_cursor_login_copy_has_no_refresh_token() {
        let root = std::env::temp_dir().join(format!("obid-cursor-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (account, login) = (root.join("account"), root.join("login"));
        let file = if cfg!(target_os = "macos") {
            ".cursor/auth.json"
        } else {
            ".config/cursor/auth.json"
        };
        std::fs::create_dir_all(account.join(file).parent().unwrap()).unwrap();
        let access = ["cursor-access-", &"a".repeat(30)].concat();
        let refresh = ["cursor-refresh-", &"r".repeat(30)].concat();
        std::fs::write(
            account.join(file),
            json!({"accessToken": access, "refreshToken": refresh, "apiKey": null,
                   "bedrockCredentials": {"secretKey": "s"}})
            .to_string(),
        )
        .unwrap();
        std::fs::write(account.join("mcp.json"), r#"{"mcpServers": {}}"#).unwrap();
        copy_account_folder(Tool::Cursor, &account, &login).unwrap();
        let copied: Value =
            serde_json::from_slice(&std::fs::read(login.join(file)).unwrap()).unwrap();
        // Only what calls Cursor's API: no refresh token, no Bedrock keys.
        assert_eq!(copied, json!({"accessToken": access, "apiKey": null}));
        // Never the account's MCP servers.
        assert!(!login.join("mcp.json").exists());
        assert!(
            !crate::secrets::known().redact(&access).contains(&access),
            "not learned"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Cursor's own login only in the macOS Keychain (cursor-agent's default there): a run's copy
    /// is its access token and API key, as from a login file, and its refresh token is never asked
    /// for. Nothing in the Keychain is no login.
    #[test]
    fn a_cursor_login_from_the_keychain_is_only_its_access_token_and_api_key() {
        let access = ["keychain-access-", &"a".repeat(30)].concat();
        let read = |service: &str| -> Option<Vec<u8>> {
            assert_ne!(
                service, "cursor-refresh-token",
                "the refresh token is never asked for"
            );
            (service == "cursor-access-token").then(|| format!("{access}\n").into_bytes())
        };
        let bytes = cursor_login_json(read).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"accessToken": access})
        );
        assert_eq!(cursor_login_json(|_| None), None);
        assert_eq!(cursor_login_json(|_| Some(b"\n".to_vec())), None);

        let root =
            std::env::temp_dir().join(format!("obid-cursor-keychain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let login = root.join("login");
        copy_cursor(Some(bytes), &root.join("cli-config.json"), &root, &login).unwrap();
        let copied: Value =
            serde_json::from_slice(&std::fs::read(login.join(cursor_login_file())).unwrap())
                .unwrap();
        assert_eq!(copied, json!({"accessToken": access}));
        assert!(
            !crate::secrets::known().redact(&access).contains(&access),
            "not learned"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A Cursor login is signed in only with something a run can use: an API key, or an access
    /// token that hasn't ended (when its JWT says). Empty values, or nothing, are signed out.
    #[test]
    fn a_cursor_login_is_signed_in_only_with_a_live_token_or_key() {
        let live = |login: Value| cursor_login_live(login.to_string().as_bytes());
        assert!(live(json!({"accessToken": codex_access(3600)})));
        assert!(live(json!({"accessToken": "opaque-token-with-no-expiry"})));
        assert!(!live(json!({"accessToken": codex_access(-60)})));
        assert!(live(
            json!({"accessToken": codex_access(-60), "apiKey": "key-of-the-account"})
        ));
        assert!(!live(json!({"accessToken": "", "apiKey": ""})));
        assert!(!live(json!({})));
        assert!(!cursor_login_live(b"not json"));
    }

    /// What a box of an older obid could have planted in an account's folder for the next unboxed
    /// run is found: Claude Code's hooks and env, Codex's notify, MCP servers and model providers.
    #[test]
    fn whats_planted_in_an_accounts_folder_is_found() {
        let root = std::env::temp_dir().join(format!("obid-planted-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert!(planted(Tool::Claude, &root).is_empty());
        std::fs::write(
            root.join("settings.json"),
            r#"{"hooks": {"SessionStart": []}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join(".claude.json"),
            r#"{"env": {"PATH": "/x"}, "userID": "u"}"#,
        )
        .unwrap();
        assert_eq!(
            planted(Tool::Claude, &root),
            ["`hooks` in settings.json", "`env` in .claude.json"]
        );
        std::fs::write(
            root.join("config.toml"),
            "model = \"o3\"\nnotify = [\"sh\"]\n[mcp_servers.x]\ncommand = \"x\"\n",
        )
        .unwrap();
        assert_eq!(
            planted(Tool::Codex, &root),
            ["`notify` in config.toml", "`mcp_servers` in config.toml"]
        );
        std::fs::write(
            root.join("settings.local.json"),
            r#"{"apiKeyHelper": "x", "statusLine": {"command": "y"}}"#,
        )
        .unwrap();
        std::fs::write(root.join(".claude.json"), r#"{"mcpServers": {"x": {}}}"#).unwrap();
        assert_eq!(
            planted(Tool::Claude, &root),
            [
                "`hooks` in settings.json",
                "`apiKeyHelper` in settings.local.json",
                "`statusLine` in settings.local.json",
                "`mcpServers` in .claude.json"
            ]
        );
        std::fs::write(root.join("config.toml"), "[profiles.fast]\nmodel = \"x\"\n").unwrap();
        assert_eq!(planted(Tool::Codex, &root), ["`profiles` in config.toml"]);
        assert!(planted(Tool::Cursor, &root).is_empty());
        std::fs::write(root.join("mcp.json"), "{}").unwrap();
        std::fs::write(
            root.join("cli-config.json"),
            r#"{"permissions": {"allow": ["Shell(*)"]}}"#,
        )
        .unwrap();
        assert_eq!(
            planted(Tool::Cursor, &root),
            ["an `mcp.json`", "`permissions.allow` in cli-config.json"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A `.credentials.json` an older obid copied into an account's folder (no refresh token) is
    /// taken out when the Keychain holds the login; Claude Code's own (with one), one the Keychain
    /// doesn't hold, and a link stay.
    #[test]
    fn an_older_copy_of_a_login_is_taken_out() {
        let root = std::env::temp_dir().join(format!("obid-older-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join(".credentials.json");
        std::fs::write(
            &file,
            r#"{"claudeAiOauth": {"accessToken": "a", "expiresAt": 1}}"#,
        )
        .unwrap();
        // Not while the Keychain doesn't hold the login: it may be the only one.
        assert!(!remove_older_copy(&root, false));
        assert!(file.exists());
        assert!(remove_older_copy(&root, true));
        assert!(!file.exists());
        std::fs::write(
            &file,
            r#"{"claudeAiOauth": {"accessToken": "a", "refreshToken": "r"}}"#,
        )
        .unwrap();
        assert!(!remove_older_copy(&root, true));
        assert!(file.exists());
        std::fs::remove_file(&file).unwrap();
        std::fs::write(root.join("elsewhere"), r#"{"claudeAiOauth": {}}"#).unwrap();
        std::os::unix::fs::symlink(root.join("elsewhere"), &file).unwrap();
        assert!(!remove_older_copy(&root, true));
        assert!(root.join("elsewhere").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A conversation's copy of an account's login is #651's allowlisted one: what calls the API,
    /// never the refresh token (a copy that refreshed would revoke the account's own login).
    #[test]
    fn a_conversations_login_copy_has_no_refresh_token() {
        let root = std::env::temp_dir().join(format!("obid-conv-login-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let account = root.join("account");
        std::fs::create_dir_all(&account).unwrap();
        let access = ["sk-ant-oat01-", &"a".repeat(40)].concat();
        let refresh = ["sk-ant-ort01-", &"r".repeat(40)].concat();
        std::fs::write(
            account.join(".credentials.json"),
            json!({"claudeAiOauth": {"accessToken": access, "refreshToken": refresh,
                   "expiresAt": 1, "scopes": ["user:inference"]},
                   "mcpOAuth": {"x": "y"}})
            .to_string(),
        )
        .unwrap();
        copy_account_login(Tool::Claude, &account, &root.join("config")).unwrap();
        let copied: Value =
            serde_json::from_slice(&std::fs::read(root.join("config/.credentials.json")).unwrap())
                .unwrap();
        assert_eq!(copied["claudeAiOauth"]["accessToken"], access.as_str());
        assert_eq!(copied["claudeAiOauth"]["expiresAt"], 1);
        assert!(
            copied["claudeAiOauth"].get("refreshToken").is_none(),
            "{copied}"
        );
        assert!(copied.get("mcpOAuth").is_none(), "{copied}");
        // No usable login (no access token): nothing is copied.
        std::fs::write(account.join(".credentials.json"), b"{}").unwrap();
        let _ = std::fs::remove_file(root.join("config/.credentials.json"));
        copy_account_login(Tool::Claude, &account, &root.join("config")).unwrap();
        assert!(!root.join("config/.credentials.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A ChatGPT access token (a JWT) that ends `seconds` from now.
    fn codex_access(seconds: i64) -> String {
        use base64::Engine;
        let encode =
            |v: Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v.to_string());
        let ends = (now_ms() / 1000) as i64 + seconds;
        format!(
            "{}.{}.sig",
            encode(json!({"alg": "RS256"})),
            encode(json!({ "exp": ends }))
        )
    }

    /// No copy obid makes of a Codex login, a job's (`copy_login`) or a conversation's
    /// (`copy_account_login`), carries the refresh token, which the provider rotates; the tokens
    /// are learned all the same. A copy whose access token ends within 5 minutes, or doesn't say
    /// when, doesn't run (G2).
    #[test]
    fn no_codex_login_copy_carries_a_refresh_token() {
        let root = std::env::temp_dir().join(format!("obid-codex-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let access = codex_access(3600);
        let refresh = format!("rt_{}", "q".repeat(40));
        let login = json!({"auth_mode": "chatgpt", "OPENAI_API_KEY": null, "last_refresh": "2026-10-12T23:13:02Z",
                           "tokens": {"access_token": access, "refresh_token": refresh, "id_token": "i", "account_id": "a"}});
        std::fs::write(home.join(".codex/auth.json"), login.to_string()).unwrap();
        // Its config.toml starts programs (`notify`, MCP servers): never a run's.
        std::fs::write(
            home.join(".codex/config.toml"),
            "notify = [\"touch\", \"x\"]\n",
        )
        .unwrap();
        copy_login(Some(&home), &root.join("job"), Tool::Codex, true).unwrap();
        assert!(!root.join("job/.codex/config.toml").exists());
        copy_account_login(
            Tool::Codex,
            &home.join(".codex"),
            &root.join("conversation"),
        )
        .unwrap();
        for copy in [
            root.join("job/.codex/auth.json"),
            root.join("conversation/auth.json"),
        ] {
            let copied: Value = serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
            assert_eq!(copied["tokens"]["refresh_token"], "", "{copied}");
            assert_eq!(copied["tokens"]["access_token"], access.as_str());
            assert_eq!(copied["last_refresh"], login["last_refresh"]);
            assert_eq!(codex_copy_ready(&copy, "Codex"), Ok(()));
        }
        let said = crate::secrets::known().redact(&format!("{access} {refresh}"));
        assert!(
            !said.contains(&access) && !said.contains(&refresh),
            "{said}"
        );

        let refused =
            Err("The Mac's Codex login for Codex needs refreshing: run codex once.".into());
        let copy = root.join("conversation/auth.json");
        for token in [codex_access(60), codex_access(-60), "not-a-jwt".into()] {
            std::fs::write(
                &copy,
                json!({"tokens": {"access_token": token}}).to_string(),
            )
            .unwrap();
            assert_eq!(codex_copy_ready(&copy, "Codex"), refused, "{token}");
        }
        // An API-key login has no access token to end; no copy at all is the tool's to report.
        std::fs::write(&copy, json!({"OPENAI_API_KEY": "sk-x"}).to_string()).unwrap();
        assert_eq!(codex_copy_ready(&copy, "Codex"), Ok(()));
        assert_eq!(codex_copy_ready(&root.join("nowhere"), "Codex"), Ok(()));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A Codex login OpenAI refused (codex-cli 0.162.1's `workspace routing discovery unauthorized
    /// (401)`, though its token's `exp` is days away) isn't signed in any more, and a copy of it
    /// doesn't run, until the owner signs in again: then the new token is signed in.
    #[test]
    fn a_codex_login_openai_refused_is_signed_out_until_a_new_sign_in() {
        let dir = std::env::temp_dir().join(format!("obid-codex-ended-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let account = crate::config::Account {
            id: "codex".into(),
            name: "Codex".into(),
            config_dir: Some(dir.clone()),
            payer: None,
        };
        let write = |token: &str| {
            let login =
                json!({"tokens": {"access_token": token, "refresh_token": "", "account_id": "a"}});
            std::fs::write(dir.join("auth.json"), login.to_string()).unwrap();
        };
        let refused = codex_access(3 * 86400);
        write(&refused);
        assert!(signed_in(Tool::Codex, &account, None));
        // Only Codex's own words about the run (its turn's error, a request it refused), never
        // the agent's last message: one fixing an auth bug may well say "401 Unauthorized".
        let refusal = |reason: &str| codex_error(reason).is_some_and(codex_login_refused);
        for reason in [
            "Codex hit an error: workspace routing discovery unauthorized (401)",
            "Codex couldn't start a thread: workspace routing discovery unauthorized (401)",
            "Codex hit an error: unexpected status 401 Unauthorized: Encountered invalidated oauth token for user",
            r#"Codex hit an error: Failed to refresh token: {"error_code": "refresh_token_invalidated"}"#,
        ] {
            assert!(refusal(reason), "{reason}");
        }
        for reason in [
            "Codex hit an error: You've hit your usage limit.",
            "Codex hit an error: stream disconnected before completion",
            "The endpoint returns 401 Unauthorized when the token is stale; I fixed the header.",
            "workspace routing discovery unauthorized (401)",
        ] {
            assert!(!refusal(reason), "{reason}");
        }
        end_codex_login(&dir.join("auth.json"), "Codex");
        assert!(!signed_in(Tool::Codex, &account, None));
        let ended = Err("The Mac's Codex login for Codex has ended: run codex login.".to_owned());
        assert_eq!(codex_copy_ready(&dir.join("auth.json"), "Codex"), ended);
        assert_eq!(
            login_note(Tool::Codex, &account, None),
            Some("Run codex login")
        );
        // Signed in again: another token, which runs.
        write(&format!("{refused}x"));
        assert!(signed_in(Tool::Codex, &account, None));
        assert_eq!(login_note(Tool::Codex, &account, None), None);
        // A run's copy refused after the owner signed in again meanwhile: what's ended is that
        // copy's login, never the new one.
        let copy = dir.join("copy.json");
        std::fs::write(
            &copy,
            json!({"tokens": {"access_token": codex_access(86400)}}).to_string(),
        )
        .unwrap();
        end_codex_login(&copy, "Codex");
        assert!(signed_in(Tool::Codex, &account, None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A login copied from a file isn't one the keeper writes again: only a copy made from the
    /// Keychain is (#651's review).
    #[test]
    fn copy_login_says_whether_it_copied_the_keychains_login() {
        let root = std::env::temp_dir().join(format!("obid-copy-from-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::write(home.join(".claude/.credentials.json"), b"{}").unwrap();
        let copied = copy_login(Some(&home), &root.join("login"), Tool::Claude, true).unwrap();
        assert!(!copied.from_keychain);
        assert!(root.join("login/.claude/.credentials.json").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// After a refresh the access token running jobs still hold stays redacted, next to the new.
    #[test]
    fn the_token_before_a_refresh_stays_redacted() {
        let now = 1_760_000_000_000u64;
        let old = ["sk-ant-oat01-", &"o".repeat(40)].concat();
        let service = format!("obid-test-previous-{}", std::process::id());
        let refreshed = std::sync::atomic::AtomicBool::new(false);
        let read = || {
            let fresh = refreshed.load(std::sync::atomic::Ordering::SeqCst);
            let (token, ends) = if fresh {
                ("sk-ant-oat01-new", now + 3_600_000)
            } else {
                (old.as_str(), now + 60_000)
            };
            Some(
                json!({"claudeAiOauth": {"accessToken": token, "expiresAt": ends, "scopes": ["user:inference"]}})
                    .to_string()
                    .into_bytes(),
            )
        };
        let refresh = || {
            refreshed.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        };
        assert_eq!(refresh_if_due(&service, now, read, refresh), Fresh::Renewed);
        let said = crate::secrets::known().redact(&format!("a job said {old}"));
        assert!(!said.contains(&old), "{said}");
    }

    /// A login far from its end takes no lock, and one account's refresh doesn't hold up
    /// another's (#651's review).
    #[test]
    fn one_accounts_refresh_holds_up_no_other() {
        use std::sync::mpsc;
        let now = 1_760_000_000_000u64;
        let pid = std::process::id();
        let (a, b) = (format!("obid-test-a-{pid}"), format!("obid-test-b-{pid}"));
        let near = || a_login(Some(now + 60_000));
        let (started, on_start) = mpsc::channel();
        let (release, on_release) = mpsc::channel::<()>();
        let done = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|scope| {
            let (a, done) = (&a, &done);
            scope.spawn(move || {
                refresh_if_due(a, now, near, || {
                    started.send(()).unwrap();
                    let _ = on_release.recv_timeout(Duration::from_secs(30));
                    true
                });
                done.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            on_start.recv().unwrap();
            // While A's refresh goes: B refreshes, and A far from its end needs no lock.
            assert_eq!(refresh_if_due(&b, now, near, || true), Fresh::Renewed);
            let far = || a_login(Some(now + 3_600_000));
            assert_eq!(refresh_if_due(a, now, far, || true), Fresh::Good);
            assert!(!done.load(std::sync::atomic::Ordering::SeqCst));
            release.send(()).unwrap();
        });
    }

    /// A login file copied without its refresh token (no Keychain) is good while its token is
    /// far from its end, and refused otherwise. No login file: nothing is copied from it.
    #[test]
    fn a_login_files_copy_is_good_only_far_from_its_end() {
        let now = 1_760_000_000_000u64;
        assert!(file_login_ok(
            a_login(Some(now + 3_600_000)).as_deref(),
            now
        ));
        assert!(!file_login_ok(a_login(Some(now + 60_000)).as_deref(), now));
        assert!(!file_login_ok(a_login(Some(now - 1)).as_deref(), now));
        assert!(file_login_ok(a_login(None).as_deref(), now));
        assert!(file_login_ok(None, now));
    }

    /// The brain's copy of an account's login, for its own empty config folder: from the
    /// account's login file when the Keychain isn't used, only what calls the API, never through
    /// a link.
    #[test]
    fn an_accounts_login_copy_carries_only_what_calls_the_api() {
        let root = std::env::temp_dir().join(format!("obid-login-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("account")).unwrap();
        let login = json!({"claudeAiOauth": {"accessToken": "a", "refreshToken": "r", "scopes": ["user:inference"]}});
        std::fs::write(root.join("account/.credentials.json"), login.to_string()).unwrap();
        let copy: Value =
            serde_json::from_slice(&account_login(&root.join("account")).unwrap()).unwrap();
        assert_eq!(
            copy,
            json!({"claudeAiOauth": {"accessToken": "a", "scopes": ["user:inference"]}})
        );
        // A link in the file's place is not a login.
        std::fs::create_dir_all(root.join("linked")).unwrap();
        std::os::unix::fs::symlink(
            root.join("account/.credentials.json"),
            root.join("linked/.credentials.json"),
        )
        .unwrap();
        assert!(account_login(&root.join("linked")).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A folder named with a trailing `/` or `/.` is still looked at as itself, not where a link
    /// in its place leads (#643's re-review).
    #[test]
    fn a_trailing_slash_doesnt_hide_a_link() {
        let root = std::env::temp_dir().join(format!("obid-trailing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::os::unix::fs::symlink(root.join("target"), root.join("link")).unwrap();
        for name in ["link", "link/", "link/."] {
            assert!(not_a_link(&root.join(name)).is_err(), "{name}");
        }
        assert!(not_a_link(&root.join("target/")).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Claude Code's Keychain names, as it makes them: the default folder's, and any other's with
    /// the first 8 hex digits of its SHA-256 (seen on 2026-10-17 for an account's folder).
    #[test]
    fn claude_codes_keychain_items_are_named_as_it_names_them() {
        assert_eq!(keychain_service(None), "Claude Code-credentials");
        let folder = Path::new(
            "/Users/example/Library/Application Support/Oby/accounts/32203c5b-b24c-4353-bdcb-f34797790d0a",
        );
        assert_eq!(
            keychain_service(Some(folder)),
            "Claude Code-credentials-d5b15a4f"
        );
    }
}
