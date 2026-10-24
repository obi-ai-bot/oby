//! The owner's GitHub login, from the Mac: whether `gh` is signed in, and gh's own sign-in
//! (the owner's decision of 2026-10-18: Oby.app carries gh, and nobody types `gh auth login`).
//!
//! obid never reads, holds or passes the token: gh signs in through GitHub's device page and keeps
//! the login in its own Keychain item, as `gh auth login` always does. What obid takes from gh is
//! the account's name and whether it's signed in (`gh auth status --json hosts`, never
//! `--show-token`), and, while signing in, the one-time code and the page gh prints. None of this
//! is on the gateway's allowlist: a paired phone never signs the Mac in to someone's GitHub.
//!
//! The reviewer is a second GitHub account (the owner's decision of 2026-10-24: Oby gets the
//! owner's own pull requests approved, and a pull request's author can't approve it), signed in
//! through the same device page to a settings folder of obid's own, `home/github/reviewer`
//! (`GH_CONFIG_DIR`, 0700, hidden from every job's box), never the person's. Its token is obid's:
//! gh keeps it in that folder's `hosts.yml` (0600), kept away from the Keychain
//! ([`crate::github::keyless`]); obid learns it as a secret, so it's taken out of every answer,
//! log line and push, and it never reaches a job, whose gh has an empty settings folder. Only
//! Oby's review of the owner's own pull request goes as the reviewer (crate::listen).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::api::{ApiError, Shared};

/// How long a sign-in waits for the person: GitHub's device codes last 15 minutes.
const SIGN_IN_LASTS: Duration = Duration::from_secs(15 * 60);
/// How long gh has to print its code and page.
const CODE_WITHIN: Duration = Duration::from_secs(30);
/// How long a status read is kept: `gh auth status` asks GitHub.
const STATUS_KEPT: Duration = Duration::from_secs(30);
/// The start of a sign-in's folder's name in `home/tmp`.
const SCRATCH_PREFIX: &str = "github-sign-in-";
/// The only page a code is typed on.
const DEVICE_PAGE: &str = "https://github.com/login/device";

/// The owner's GitHub login and the reviewer's, each as obid last read it, with its sign-in under
/// way.
#[derive(Default)]
pub struct GitHubLogin {
    owner: Login,
    reviewer: Login,
}

/// One GitHub login: its status as last read, and its sign-in under way.
#[derive(Default)]
struct Login {
    status: Mutex<Option<(Instant, Status)>>,
    /// Never held across an await: GET and obid's stop read it while gh starts.
    sign_in: Mutex<Slot>,
    /// Why the last sign-in kept no login (the reviewer's: the owner's own account), until the next.
    refused: Mutex<Option<String>>,
}

/// Whose login: the owner's (gh's own settings) or the reviewer's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Who {
    Owner,
    Reviewer,
}

/// The reviewer's gh settings folder: `home/github/reviewer`.
pub fn reviewer_folder(home: &Path) -> PathBuf {
    home.join("github/reviewer")
}

/// The sign-in slot: none, one starting (gh hasn't printed its code yet), or one under way, until
/// it ends and clears the slot.
#[derive(Default)]
enum Slot {
    #[default]
    Idle,
    Starting,
    Running(Arc<SignIn>),
}

/// Whether gh is signed in to github.com, and as whom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub signed_in: bool,
    pub user: Option<String>,
}

/// gh's sign-in under way: its code and page, until it ends.
pub struct SignIn {
    user_code: String,
    expires: i64,
    group: Option<u32>,
    ended: tokio::sync::watch::Receiver<bool>,
    cancel: tokio::sync::Notify,
}

impl SignIn {
    fn to_json(&self) -> Value {
        json!({"url": DEVICE_PAGE, "userCode": self.user_code,
               "expiresAt": obi_protocol::time::format(self.expires)})
    }
}

impl GitHubLogin {
    /// Kills the sign-ins under way's whole process groups (obid is stopping). Only one still under
    /// way: an ended one cleared the slot, and its group id may belong to someone else by now.
    pub fn kill(&self) {
        for login in [&self.owner, &self.reviewer] {
            if let Some(running) = login.running()
                && let Some(group) = running.group
            {
                crate::process::signal_group(group, nix::sys::signal::Signal::SIGKILL);
            }
        }
    }

    fn of(&self, who: Who) -> &Login {
        match who {
            Who::Owner => &self.owner,
            Who::Reviewer => &self.reviewer,
        }
    }
}

impl Login {
    fn slot(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.sign_in.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The sign-in under way, if one is and it hasn't ended.
    fn running(&self) -> Option<Arc<SignIn>> {
        match &*self.slot() {
            Slot::Running(sign_in) if !*sign_in.ended.borrow() => Some(sign_in.clone()),
            _ => None,
        }
    }

    fn forget_status(&self) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn refused(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.refused.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The status, read again when older than [`STATUS_KEPT`]: as the reviewer from `reviewer`, its
    /// settings folder, or as the owner.
    async fn status(&self, gh: Option<&Path>, reviewer: Option<&Path>) -> Status {
        if let Some((at, status)) = &*self.status.lock().unwrap_or_else(|e| e.into_inner())
            && at.elapsed() < STATUS_KEPT
        {
            return status.clone();
        }
        let status = match gh {
            Some(gh) => read_status_of(gh, reviewer).await,
            None => Status::signed_out(),
        };
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((Instant::now(), status.clone()));
        status
    }
}

impl Status {
    fn signed_out() -> Status {
        Status {
            signed_in: false,
            user: None,
        }
    }
}

/// `gh auth status` for github.com, as `{signed_in, user}`; signed out when gh says nothing usable.
pub(crate) async fn read_status(gh: &Path) -> Status {
    read_status_of(gh, None).await
}

/// [`read_status`] as the reviewer whose settings folder is `reviewer`, or as the owner. A folder
/// with no `hosts.yml` (no login) is signed out without asking gh, and every token in it is learned
/// as a secret again (gh may have refreshed it).
async fn read_status_of(gh: &Path, reviewer: Option<&Path>) -> Status {
    if let Some(folder) = reviewer {
        if !folder.join("hosts.yml").is_file() {
            return Status::signed_out();
        }
        learn_reviewer(folder);
    }
    let said = crate::github::run_as(
        gh,
        reviewer,
        &[
            "auth",
            "status",
            "--hostname",
            "github.com",
            "--active",
            "--json",
            "hosts",
        ],
    )
    .await
    .unwrap_or_default();
    decode_status(&said)
}

/// The reviewer as obid can use it now: its settings folder and its login, signed in as an account
/// that isn't the owner's (`owner`); none when no reviewer is signed in.
pub(crate) async fn reviewer(
    gh: &Path,
    home: &Path,
    owner: &str,
) -> Result<Option<(PathBuf, String)>, String> {
    let folder = reviewer_folder(home);
    let Some(user) = read_status_of(gh, Some(&folder)).await.user else {
        return Ok(None);
    };
    if user.eq_ignore_ascii_case(owner) {
        return Err(format!(
            "the reviewer is signed in as {user}, your own account: sign it in with a second one"
        ));
    }
    Ok(Some((folder, user)))
}

/// Learns every token gh keeps in the reviewer's settings folder `folder` (`hosts.yml`'s
/// `oauth_token`, and a refreshable login's `access_token` and `refresh_token`) as the secret
/// `github reviewer`, so obid takes them out of every answer, log line and push.
pub fn learn_reviewer(folder: &Path) {
    let Ok(hosts) = std::fs::read_to_string(folder.join("hosts.yml")) else {
        return;
    };
    let tokens: Vec<&str> = hosts
        .lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once(':')?;
            ["oauth_token", "access_token", "refresh_token"]
                .contains(&key.trim())
                .then(|| value.trim().trim_matches(['"', '\'']))
        })
        .filter(|token| !token.is_empty())
        .collect();
    if !tokens.is_empty() {
        let forms: Vec<&[u8]> = tokens.iter().map(|t| t.as_bytes()).collect();
        crate::secrets::known().learn_forms("github reviewer", &forms);
    }
}

/// The active github.com account in `gh auth status --json hosts`, when its state is `success`.
pub fn decode_status(said: &str) -> Status {
    let account = serde_json::from_str::<Value>(said).ok().and_then(|v| {
        v["hosts"]["github.com"]
            .as_array()?
            .iter()
            .find(|a| a["active"] == true && a["state"] == "success")
            .cloned()
    });
    let user = account
        .as_ref()
        .and_then(|a| a["login"].as_str())
        .filter(|l| {
            !l.is_empty()
                && l.len() <= 39
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        .map(str::to_owned);
    Status {
        signed_in: user.is_some(),
        user,
    }
}

/// `GET /v1/github`: whether obid has a gh, whether it's signed in and as whom, and the sign-in
/// under way; the same for the reviewer, with why its last sign-in kept no login.
pub async fn get(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    let gh = crate::github::gh();
    let status = shared.github.owner.status(gh.as_deref(), None).await;
    // GitHub's repositories stay only the signed-in account's (crate::repos): another one's are
    // read now.
    if shared.config.repos.follow(status.user.as_deref()) && status.signed_in {
        shared.config.repos.nudge();
    }
    let sign_in = shared.github.owner.running().map(|s| s.to_json());
    let reviewer = &shared.github.reviewer;
    let folder = reviewer_folder(&shared.config.home);
    let reviewing = reviewer.status(gh.as_deref(), Some(&folder)).await;
    let reviewer = json!({"signedIn": reviewing.signed_in, "user": reviewing.user,
                          "signIn": reviewer.running().map(|s| s.to_json()),
                          "error": reviewer.refused().clone()});
    Ok(
        axum::Json(json!({"gh": gh.is_some(), "signedIn": status.signed_in,
                         "user": status.user, "signIn": sign_in, "reviewer": reviewer}))
        .into_response(),
    )
}

/// `POST /v1/github/sign-in`: gh's own sign-in, or the one under way: `{url, userCode, expiresAt}`.
/// The app opens `url`; the person types `userCode` there (gh also puts it on the clipboard).
pub async fn sign_in(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    sign_in_as(&shared, Who::Owner).await
}

/// `POST /v1/github/reviewer/sign-in`: the reviewer's sign-in, as [`sign_in`]'s, into its own
/// settings folder. Only once the owner's gh is signed in: the reviewer must be another account,
/// which obid checks when the sign-in ends, keeping no login of the owner's own account.
pub async fn reviewer_sign_in(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    // Read now, not kept: the owner may have just signed in.
    if shared.github.reviewer.running().is_none()
        && let Some(gh) = crate::github::gh()
        && !read_status(&gh).await.signed_in
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "Sign in to GitHub first: the reviewer is a second account, besides yours.",
        ));
    }
    sign_in_as(&shared, Who::Reviewer).await
}

/// `DELETE /v1/github/reviewer/sign-in`: ends the reviewer's sign-in under way, if any, and signs
/// the reviewer out: its settings folder is removed, token and all (never `gh auth logout`, which
/// clears the keyring's shared slot, the owner's). GitHub keeps the token valid until it's revoked
/// in the account's settings, as after `gh auth logout`.
pub async fn reviewer_sign_out(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    end(&shared.github.reviewer).await;
    let folder = reviewer_folder(&shared.config.home);
    if let Err(e) = std::fs::remove_dir_all(&folder)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("obid couldn't remove the reviewer's login: {e}"),
        ));
    }
    shared.github.reviewer.forget_status();
    *shared.github.reviewer.refused() = None;
    tracing::info!("GitHub reviewer signed out");
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn sign_in_as(shared: &Arc<Shared>, who: Who) -> Result<Response, ApiError> {
    let login = shared.github.of(who);
    {
        let mut slot = login.slot();
        match &*slot {
            Slot::Running(running) if !*running.ended.borrow() => {
                return Ok(axum::Json(running.to_json()).into_response());
            }
            Slot::Starting => {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    "GitHub's sign-in is starting: ask again in a moment.",
                ));
            }
            _ => *slot = Slot::Starting,
        }
    }
    // Back to idle unless the sign-in below takes the slot.
    let mut starting = Starting(login, true);
    let gh = crate::github::gh()
        .filter(|gh| gh.is_absolute())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "obid has no GitHub CLI (gh) to sign in with.",
            )
        })?;
    let started = start(shared, &gh, who)
        .await
        .map_err(|why| ApiError::new(StatusCode::BAD_GATEWAY, why))?;
    let answer = started.to_json();
    {
        let mut slot = login.slot();
        // A sign-in that ended while being registered leaves the slot idle.
        *slot = if *started.ended.borrow() {
            Slot::Idle
        } else {
            Slot::Running(started)
        };
    }
    starting.1 = false;
    Ok(axum::Json(answer).into_response())
}

/// The slot marked starting: put back to idle when dropped armed (gh didn't start, or the request
/// ended first).
struct Starting<'a>(&'a Login, bool);

impl Drop for Starting<'_> {
    fn drop(&mut self) {
        if self.1 {
            let mut slot = self.0.slot();
            if matches!(*slot, Slot::Starting) {
                *slot = Slot::Idle;
            }
        }
    }
}

/// `DELETE /v1/github/sign-in`: ends the sign-in under way, if any.
pub async fn end_sign_in(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    end(&shared.github.owner).await;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Ends `login`'s sign-in under way, if any, waiting up to 5 seconds for it.
async fn end(login: &Login) {
    if let Some(running) = login.running() {
        running.cancel.notify_one();
        let mut ended = running.ended.clone();
        let _ = tokio::time::timeout(Duration::from_secs(5), ended.wait_for(|e| *e)).await;
    }
}

/// Starts `gh auth login` and waits for its code. The reviewer's signs in to a settings folder in
/// the sign-in's own folder, kept as the reviewer's only once it ends signed in as another account
/// than the owner's ([`keep_reviewer`]).
async fn start(shared: &Arc<Shared>, gh: &Path, who: Who) -> Result<Arc<SignIn>, String> {
    let scratch = scratch_folder(&shared.config.home)?;
    let staging = (who == Who::Reviewer).then(|| scratch.join("gh"));
    if let Some(staging) = &staging
        && let Err(e) = crate::store::make_private_dir(staging)
    {
        let _ = std::fs::remove_dir_all(&scratch);
        return Err(format!("obid can't make a folder for the sign-in: {e}"));
    }
    let mut child = match command(gh, &scratch, staging.as_deref()).and_then(|mut c| {
        c.spawn()
            .map_err(|e| format!("obid couldn't start gh's sign-in: {e}"))
    }) {
        Ok(child) => child,
        Err(why) => {
            let _ = std::fs::remove_dir_all(&scratch);
            return Err(why);
        }
    };
    let group = child.id();
    let kill = move || {
        if let Some(group) = group {
            crate::process::signal_group(group, nix::sys::signal::Signal::SIGKILL);
        }
    };
    // Until the sign-in's own task takes over, a request that ends while gh starts (the app gone,
    // obid stopping) takes gh's group and its folder with it.
    let mut pending = Pending {
        group,
        scratch: scratch.clone(),
        armed: true,
    };
    let (lines, mut said) = tokio::sync::mpsc::unbounded_channel::<String>();
    for out in [
        child
            .stdout
            .take()
            .map(|o| Box::new(o) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        child
            .stderr
            .take()
            .map(|e| Box::new(e) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
    ]
    .into_iter()
    .flatten()
    {
        tokio::spawn(crate::accounts::read_lines(out, lines.clone()));
    }
    drop(lines);
    let wanted = async {
        while let Some(line) = said.recv().await {
            if let Some(code) = user_code(&crate::accounts::plain(&line)) {
                return Some(code);
            }
        }
        None
    };
    let Ok(Some(user_code)) = tokio::time::timeout(CODE_WITHIN, wanted).await else {
        kill();
        // Reaped from here on: its group id may be someone else's, so the guard leaves it be.
        pending.armed = false;
        let _ = child.wait().await;
        let _ = std::fs::remove_dir_all(&scratch);
        return Err("gh's sign-in didn't give a code to type.".into());
    };
    // The rest of what it prints is read and dropped.
    tokio::spawn(async move { while said.recv().await.is_some() {} });
    let (ended_tx, ended) = tokio::sync::watch::channel(false);
    let sign_in = Arc::new(SignIn {
        user_code,
        expires: obi_protocol::time::now() + SIGN_IN_LASTS.as_secs() as i64,
        group,
        ended,
        cancel: tokio::sync::Notify::new(),
    });
    tracing::info!("GitHub sign-in started ({who:?})");
    *shared.github.of(who).refused() = None;
    let (shared, watched, gh) = (shared.clone(), sign_in.clone(), gh.to_path_buf());
    // From here the sign-in's own task ends it.
    pending.armed = false;
    tokio::spawn(async move {
        let exited = tokio::select! {
            status = child.wait() => status.ok(),
            () = tokio::time::sleep(SIGN_IN_LASTS) => None,
            () = watched.cancel.notified() => None,
        };
        // Its group only while gh hasn't been reaped: after that its id may be someone else's.
        if exited.is_none() {
            kill();
            let _ = child.wait().await;
        }
        let mut signed = exited.is_some_and(|s| s.success());
        if signed && let Some(staging) = &staging {
            let login = &shared.github.reviewer;
            if let Err(why) = keep_reviewer(&shared, &gh, staging).await {
                signed = false;
                *login.refused() = Some(why);
            }
        }
        // Its folder goes first, then it's ended at once: obid's stop (kill) never signals the
        // group id, which may be someone else's by now, and a sign-in started after this one ended
        // never finds its folder being removed.
        let _ = std::fs::remove_dir_all(&scratch);
        let _ = ended_tx.send(true);
        let login = shared.github.of(who);
        login.forget_status();
        {
            let mut slot = login.slot();
            if matches!(&*slot, Slot::Running(s) if Arc::ptr_eq(s, &watched)) {
                *slot = Slot::Idle;
            }
        }
        if signed && who == Who::Owner {
            // What the new login can push to, now rather than within the hour.
            shared.config.repos.nudge();
        }
        tracing::info!(
            "GitHub sign-in ({who:?}) ended {}",
            if signed {
                "signed in"
            } else {
                "without signing in"
            }
        );
    });
    Ok(sign_in)
}

/// Keeps the reviewer's login gh just made in `staging` as the reviewer's (`home/github/reviewer`,
/// in place of the one before), once gh says it's signed in there as an account that isn't the
/// owner's; or why not, and nothing is kept.
async fn keep_reviewer(shared: &Shared, gh: &Path, staging: &Path) -> Result<(), String> {
    let Some(user) = read_status_of(gh, Some(staging)).await.user else {
        return Err("gh's sign-in ended without a login for the reviewer.".into());
    };
    let Some(owner) = read_status(gh).await.user else {
        return Err(
            "Sign in to GitHub first: the reviewer is a second account, besides yours.".into(),
        );
    };
    if owner.eq_ignore_ascii_case(&user) {
        return Err(format!(
            "{user} is your own GitHub account: sign the reviewer in with a second one, since nobody can approve their own pull request."
        ));
    }
    let folder = reviewer_folder(&shared.config.home);
    let _ = std::fs::remove_dir_all(&folder);
    crate::store::make_private_dir(&shared.config.home.join("github"))
        .and_then(|()| std::fs::rename(staging, &folder))
        .map_err(|e| format!("obid couldn't keep the reviewer's login: {e}"))?;
    learn_reviewer(&folder);
    tracing::info!("GitHub reviewer signed in");
    Ok(())
}

/// Removes the folders sign-ins left in `home/tmp` when obid stopped before they ended.
pub fn sweep(home: &Path) {
    let Ok(entries) = std::fs::read_dir(home.join("tmp")) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(SCRATCH_PREFIX)
            && entry.file_type().is_ok_and(|t| t.is_dir())
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// gh started and its sign-in not yet handed to its own task: dropped armed (the request ended
/// while gh started), gh's whole group is killed (not yet reaped, so the id is still its) and the
/// sign-in's folder removed.
struct Pending {
    group: Option<u32>,
    scratch: PathBuf,
    armed: bool,
}

impl Drop for Pending {
    fn drop(&mut self) {
        if self.armed {
            if let Some(group) = self.group {
                crate::process::signal_group(group, nix::sys::signal::Signal::SIGKILL);
            }
            let _ = std::fs::remove_dir_all(&self.scratch);
        }
    }
}

/// A folder of obid's own for the sign-in (its `TMPDIR`, working folder, and the git settings gh
/// may write), removed when it ends.
fn scratch_folder(home: &Path) -> Result<PathBuf, String> {
    // One of its own for each sign-in: an ended one's task removing its folder never takes the
    // next one's.
    static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = home
        .join("tmp")
        .join(format!("{SCRATCH_PREFIX}{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    crate::store::make_private_dir(&home.join("tmp"))
        .and_then(|()| crate::store::make_private_dir(&scratch))
        .map_err(|e| format!("obid can't make a folder for the sign-in: {e}"))?;
    Ok(scratch)
}

/// `gh auth login` for github.com through GitHub's device page, with https for git: obid's
/// environment without tokens or the agents' variables, the person's `HOME` (gh keeps its settings
/// and Keychain item where it always does), nothing opened on this machine (the app opens the
/// page), and git's global settings pointed at a file of the sign-in's own, so gh never changes
/// the person's own (`gh auth setup-git` isn't needed: obid gives git gh's login itself). In a
/// process group of its own. The reviewer's (`reviewer`, its settings folder) keeps its token in
/// that folder (`--insecure-storage`, gh kept away from the keyring: [`crate::github::keyless`]).
fn command(
    gh: &Path,
    scratch: &Path,
    reviewer: Option<&Path>,
) -> Result<tokio::process::Command, String> {
    let home =
        crate::jobs::login_home().ok_or("obid can't find the person's home for gh's sign-in.")?;
    let mut command = match reviewer {
        None => tokio::process::Command::new(gh),
        Some(_) => crate::github::keyless(gh)?,
    };
    command.args([
        "auth",
        "login",
        "--hostname",
        "github.com",
        "--git-protocol",
        "https",
        "--web",
        "--skip-ssh-key",
    ]);
    if reviewer.is_some() {
        command.arg("--insecure-storage");
    }
    command
        .env_clear()
        .envs(crate::process::kept(|name| std::env::var_os(name)))
        .env("HOME", home)
        .env("PATH", crate::process::tools_path())
        .env("TMPDIR", scratch)
        .env("GH_BROWSER", "true")
        .env("BROWSER", "true")
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env("GIT_CONFIG_GLOBAL", scratch.join("gitconfig"))
        .envs(reviewer.into_iter().flat_map(|folder| {
            [
                ("GH_CONFIG_DIR", folder.as_os_str()),
                (
                    "DBUS_SESSION_BUS_ADDRESS",
                    std::ffi::OsStr::new(crate::github::NO_BUS),
                ),
            ]
        }))
        .current_dir(scratch)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    Ok(command)
}

/// gh's one-time code in a line it prints (`! One-time code (BA49-7C89) copied to clipboard`, or
/// `! First copy your one-time code: BA49-7C89`): two groups of four letters or digits.
fn user_code(line: &str) -> Option<String> {
    static CODE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    if !line.to_ascii_lowercase().contains("one-time code") {
        return None;
    }
    CODE.get_or_init(|| {
        regex::Regex::new(r"\b([A-Z0-9]{4}-[A-Z0-9]{4})\b").expect("a valid pattern")
    })
    .captures(line)
    .map(|c| c[1].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sign-in whose request ended while gh started leaves no folder behind; one handed to its
    /// own task keeps it until that task ends it.
    #[test]
    fn a_dropped_start_removes_its_folder() {
        let folder = std::env::temp_dir().join(format!("obid-gh-pending-{}", std::process::id()));
        for armed in [true, false] {
            std::fs::create_dir_all(&folder).unwrap();
            drop(Pending {
                group: None,
                scratch: folder.clone(),
                armed,
            });
            assert_eq!(folder.exists(), !armed);
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// An ended sign-in is never killed nor shown, even before its task cleared the slot: its group
    /// id may belong to another process by then.
    #[test]
    fn an_ended_sign_in_is_never_the_one_under_way() {
        let login = Login::default();
        let (ended_tx, ended) = tokio::sync::watch::channel(false);
        let sign_in = Arc::new(SignIn {
            user_code: "AB12-CD34".into(),
            expires: 0,
            group: Some(999_999),
            ended,
            cancel: tokio::sync::Notify::new(),
        });
        *login.slot() = Slot::Running(sign_in);
        assert!(login.running().is_some());
        ended_tx.send(true).unwrap();
        assert!(login.running().is_none());
        *login.slot() = Slot::Starting;
        assert!(login.running().is_none());
    }

    #[test]
    fn the_code_is_taken_from_either_line_gh_prints() {
        assert_eq!(
            user_code("! One-time code (BA49-7C89) copied to clipboard").as_deref(),
            Some("BA49-7C89")
        );
        assert_eq!(
            user_code("! First copy your one-time code: 0F1A-22BC").as_deref(),
            Some("0F1A-22BC")
        );
        assert_eq!(
            user_code(
                "Open this URL to continue in your web browser: https://github.com/login/device"
            ),
            None
        );
        assert_eq!(user_code("ABCD-1234 is not said to be a code"), None);
    }

    #[test]
    fn the_status_is_the_active_github_account_signed_in() {
        let signed = r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"well-prado","tokenSource":"keyring","gitProtocol":"https"}]}}"#;
        assert_eq!(
            decode_status(signed),
            Status {
                signed_in: true,
                user: Some("well-prado".into())
            }
        );
        let signed_out = Status {
            signed_in: false,
            user: None,
        };
        assert_eq!(decode_status(r#"{"hosts":{}}"#), signed_out);
        assert_eq!(decode_status(""), signed_out);
        let failing = r#"{"hosts":{"github.com":[{"state":"error","active":true,"login":"x"}]}}"#;
        assert_eq!(decode_status(failing), signed_out);
        let odd = r#"{"hosts":{"github.com":[{"state":"success","active":true,"login":"a b"}]}}"#;
        assert_eq!(decode_status(odd), signed_out);
    }
}
