//! obid's own calls to GitHub, through the owner's GitHub CLI (`gh`, found on the login shell's
//! `PATH`): opening the pull request of a published job or bringing its description up to date,
//! reading an issue's shape, and following a published pull request (docs/runner-api.md, "Job
//! pipeline and pull request following"). obid has no GitHub app, so every call goes as the owner
//! (`GET /v1/status` says `"github": "owner"`), but one: Oby's review of the owner's own pull
//! request, which GitHub lets nobody approve, goes as the reviewer, a second account signed in to a
//! settings folder of obid's own (crate::github_login, [`output_as`]).
//!
//! `gh` signs in with its own login: obid never passes a token, and reads none but the reviewer's,
//! only to take it out of what it writes (crate::github_login::learn_reviewer). Its environment is
//! obid's without `GH_TOKEN`, `GITHUB_TOKEN` or the agents' variables, with prompts, colour and
//! update checks off; nothing it is given is a secret (titles and
//! descriptions went through the secret scan first). What `gh` prints is never logged whole and
//! never reaches an agent: an error keeps its last line, with the secrets obid holds taken out.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

/// How long one `gh` call may take.
const GH_TIMEOUT: Duration = Duration::from_secs(120);

/// The GitHub CLI obid uses: the one Oby.app carries when obid runs from inside it (the owner's
/// decision of 2026-10-18: nobody installs gh to use Oby), else the login shell's, or none.
pub fn gh() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| bundled_gh(&exe))
        .or_else(|| crate::process::resolve_bin("gh"))
}

/// `Contents/MacOS/gh` of the app obid runs from (`Contents/Helpers/obid` or `Contents/MacOS/obid`),
/// signed with it (apps/mac/app/Tools/bundle-gh.sh); none for an obid anywhere else.
fn bundled_gh(exe: &Path) -> Option<PathBuf> {
    let contents = exe.parent()?.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    let gh = contents.join("MacOS/gh");
    crate::sandbox::is_executable(&gh).then_some(gh)
}

/// The pull request as obid last read it from GitHub (`pullRequest` on a job): only typed status,
/// never anything a person wrote.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    /// `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    /// `REVIEW_REQUIRED`, `CHANGES_REQUESTED`, `APPROVED`, or none (no decision yet).
    pub review_decision: Option<String>,
    pub base: String,
    /// Seconds since the epoch; only once merged.
    pub merged_at: Option<i64>,
    pub checked_at: i64,
    /// The pull request's head commit as GitHub has it.
    pub head: Option<String>,
}

impl PullRequest {
    /// As the runner API shows it: `mergedAt` and `head` only when known.
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "state": self.state,
            "reviewDecision": self.review_decision,
            "base": self.base,
            "checkedAt": obi_protocol::time::format(self.checked_at),
        });
        if let Some(merged) = self.merged_at {
            value["mergedAt"] = json!(obi_protocol::time::format(merged));
        }
        if let Some(head) = &self.head {
            value["head"] = json!(head);
        }
        value
    }

    pub fn is_open(&self) -> bool {
        self.state == "OPEN"
    }
}

/// `owner/name`, as GitHub names a repository: nothing a URL or a path could be made of.
pub fn is_repo(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !name.is_empty()
        && !repo.contains("..")
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// A remote on GitHub (`https://github.com/…`, `ssh://git@github.com/…` or `git@github.com:…`),
/// where a push can have a pull request. Any other (a `file://` one) gets the push alone.
pub fn is_remote(remote: &str) -> bool {
    let remote = remote.to_ascii_lowercase();
    [
        "https://github.com/",
        "ssh://git@github.com/",
        "git@github.com:",
    ]
    .iter()
    .any(|prefix| remote.starts_with(prefix))
}

/// The number of a pull request URL in `repo`: exactly `https://github.com/<repo>/pull/<n>`, or
/// none. Only a pull request of the job's own repository is ever read or followed.
pub fn pull_request_number(url: &str, repo: &str) -> Option<u64> {
    if !is_repo(repo) {
        return None;
    }
    let rest = url.strip_prefix("https://github.com/")?;
    let number = rest.strip_prefix(repo)?.strip_prefix("/pull/")?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) || number.len() > 10 {
        return None;
    }
    number.parse::<u64>().ok().filter(|n| *n > 0)
}

/// Runs `gh` with `args`: its stdout, trimmed, or the last line it said on stderr.
pub(crate) async fn run(gh: &Path, args: &[&str]) -> Result<String, String> {
    output(gh, args).await.map_err(|(_, said)| said)
}

/// [`run`] as the reviewer whose settings folder is `reviewer` ([`output_as`]), or as the owner.
pub(crate) async fn run_as(
    gh: &Path,
    reviewer: Option<&Path>,
    args: &[&str],
) -> Result<String, String> {
    output_as(gh, reviewer, args)
        .await
        .map_err(|(_, said)| said)
}

/// [`run`], keeping what `gh` printed on stdout when it fails too (trimmed; empty when it never
/// ran): `gh api graphql` exits non-zero on an answer with `errors` even when it has `data`.
pub(crate) async fn output(gh: &Path, args: &[&str]) -> Result<String, (String, String)> {
    output_as(gh, None, args).await
}

/// [`output`] as the owner (`None`: gh's own settings), or as the reviewer whose settings folder is
/// `reviewer` (`GH_CONFIG_DIR`), gh kept away from the Keychain ([`keyless`]).
pub(crate) async fn output_as(
    gh: &Path,
    reviewer: Option<&Path>,
    args: &[&str],
) -> Result<String, (String, String)> {
    let child = command(gh, reviewer, args)
        .map_err(|why| (String::new(), why))?
        .spawn()
        .map_err(|e| (String::new(), format!("gh couldn't start: {e}")))?;
    let output = tokio::time::timeout(GH_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| {
            (
                String::new(),
                format!("gh took more than {} seconds", GH_TIMEOUT.as_secs()),
            )
        })?
        .map_err(|e| (String::new(), format!("gh: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    Err((stdout, said(&String::from_utf8_lossy(&output.stderr))))
}

/// [`run`] for an answer that could be large: at most `most` bytes of stdout and `within` in all,
/// else an error and `gh` is stopped. What it says on stderr isn't read.
pub(crate) async fn run_bounded(
    gh: &Path,
    args: &[&str],
    most: usize,
    within: Duration,
) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let mut command = command(gh, None, args)?;
    command.stderr(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("gh couldn't start: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("gh has no stdout")?;
    let read = async {
        let mut bytes = Vec::new();
        (&mut stdout)
            .take(most as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| format!("gh: {e}"))?;
        if bytes.len() > most {
            return Err(format!("gh printed more than {most} bytes"));
        }
        let status = child.wait().await.map_err(|e| format!("gh: {e}"))?;
        if !status.success() {
            return Err(format!("gh ended with {status}"));
        }
        Ok(String::from_utf8_lossy(&bytes).trim().to_owned())
    };
    // Dropped on either error, `gh` is killed (`kill_on_drop`).
    tokio::time::timeout(within, read)
        .await
        .map_err(|_| format!("gh took more than {} seconds", within.as_secs()))?
}

/// `gh` with `args`, its environment cleaned, stdout and stderr piped: as the owner, or as the
/// reviewer whose settings folder is `reviewer`.
fn command(
    gh: &Path,
    reviewer: Option<&Path>,
    args: &[&str],
) -> Result<tokio::process::Command, String> {
    let mut command = match reviewer {
        None => tokio::process::Command::new(gh),
        Some(folder) => {
            let mut command = keyless(gh)?;
            command.env("GH_CONFIG_DIR", folder);
            command
        }
    };
    // gh signs in with its own login only: a token that found its way into obid's environment
    // (`GH_TOKEN`, `GITHUB_TOKEN`, the agents' variables) never reaches it.
    for (name, _) in std::env::vars_os() {
        if crate::process::is_forbidden(&name.to_string_lossy()) {
            command.env_remove(name);
        }
    }
    command
        .args(args)
        .env("PATH", crate::process::tools_path())
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GH_SPINNER_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    Ok(command)
}

/// The Seatbelt profile the reviewer's gh runs under on macOS: everything as usual but the
/// Keychain's tool, which is how gh reaches the Keychain (zalando/go-keyring runs
/// `/usr/bin/security`).
const NO_KEYCHAIN: &str =
    "(version 1)(allow default)(deny process-exec (literal \"/usr/bin/security\"))";

/// `gh`, kept away from the person's keyring, for the reviewer's login. gh keeps one login's token
/// per host in the keyring's shared slot (`gh:github.com`, no account), whatever its settings
/// folder: every sign-in clears that slot (the owner's), and a reviewer's call whose own file had
/// no token would fall back to it and go as the owner. With the keyring out of reach gh keeps the
/// reviewer's token in its settings folder only, and never touches the owner's. On macOS under
/// [`NO_KEYCHAIN`] (inside a job sandbox, which denies the Keychain already and can't nest
/// another, as it is); elsewhere with no D-Bus session to reach a keyring through. Refused when
/// neither holds: the reviewer's gh never runs with the keyring in reach.
pub(crate) fn keyless(gh: &Path) -> Result<tokio::process::Command, String> {
    static BOXED: std::sync::OnceLock<Result<bool, String>> = std::sync::OnceLock::new();
    let boxed = BOXED.get_or_init(|| {
        if !cfg!(target_os = "macos") {
            return Ok(false);
        }
        let tried = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", NO_KEYCHAIN, "/usr/bin/true"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| format!("sandbox-exec can't run: {e}"))?;
        if tried.status.success() {
            Ok(true)
        } else if String::from_utf8_lossy(&tried.stderr).contains("sandbox_apply")
            && crate::sandbox::inside_a_job_sandbox()
        {
            Ok(false)
        } else {
            Err("sandbox-exec can't keep the reviewer's gh away from the Keychain".into())
        }
    });
    let mut command = match boxed.clone()? {
        true => {
            let mut command = tokio::process::Command::new("/usr/bin/sandbox-exec");
            command.args(["-p", NO_KEYCHAIN]).arg(gh);
            command
        }
        false => tokio::process::Command::new(gh),
    };
    command.env("DBUS_SESSION_BUS_ADDRESS", NO_BUS);
    Ok(command)
}

/// The reviewer's gh's D-Bus session: one that isn't there, since go-keyring finds the Secret
/// Service only through it.
pub(crate) const NO_BUS: &str = "unix:path=/dev/null/no-keyring";

/// Whether the account signed in to the settings folder `reviewer` can push to `repo`
/// (`permissions.push`): a review from an account without write access doesn't count toward
/// GitHub's required approvals.
pub async fn can_push(gh: &Path, who: Option<&Path>, repo: &str) -> Result<bool, String> {
    if !is_repo(repo) {
        return Err(format!("{repo} isn't a repository name"));
    }
    let path = format!("repos/{repo}");
    let said = run_as(gh, who, &["api", &path, "--jq", ".permissions.push"]).await?;
    Ok(said == "true")
}

/// The last thing a tool said on stderr, skipping git's hints, with the secrets obid holds taken
/// out and at most 300 characters.
pub fn said(stderr: &str) -> String {
    let line = stderr
        .lines()
        .rfind(|l| !l.trim().is_empty() && !l.starts_with("hint:"))
        .unwrap_or("no reason given")
        .trim();
    let line = crate::secrets::known().redact(line);
    if line.chars().count() > 300 {
        format!("{}…", line.chars().take(299).collect::<String>())
    } else {
        line
    }
}

/// What publishing sent for the pull request: opened, or its description brought up to date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    Opened,
    Edited,
    /// The pull request was open already and its description didn't update.
    Nothing,
}

/// The open pull request obid published `branch` to: from this repository itself (never a fork's
/// branch of the same name, which `gh pr list --head` would match too), into `base`, and with one of `heads` as
/// its head: the commit obid just pushed, or the one it pushed before (GitHub may not have moved
/// it yet). `gh pr list --json` answers a list of them.
pub fn own_pull_request(listed: &str, repo: &str, base: &str, heads: &[&str]) -> Option<String> {
    if listed.trim().is_empty() {
        return None;
    }
    let list: Vec<Value> = serde_json::from_str(listed).ok()?;
    list.iter()
        .filter(|pr| {
            pr["isCrossRepository"] == Value::Bool(false)
                && pr["baseRefName"] == base
                && pr["headRefOid"]
                    .as_str()
                    .is_some_and(|h| git_sha(h) && heads.contains(&h))
        })
        .filter_map(|pr| pr["url"].as_str())
        .find(|url| pull_request_number(url, repo).is_some())
        .map(str::to_owned)
}

/// Opens the pull request for `branch` into `base`, ready for review (never a draft), or finds the
/// one obid already opened for it (`own_pull_request`: same repository, one of `heads` its head) and
/// brings its description up to date from `body_file`. Answers its URL, which must be a pull
/// request of `repo`, and what went out; or what failed, worded to follow "Pushed, but ".
pub async fn open_pull_request(
    gh: &Path,
    repo: &str,
    base: &str,
    branch: &str,
    heads: &[&str],
    title: &str,
    body_file: &Path,
) -> Result<(String, Sent), String> {
    let body = body_file.to_string_lossy();
    let listed = run(
        gh,
        &[
            "pr",
            "list",
            "-R",
            repo,
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "url,isCrossRepository,headRefOid,baseRefName",
            "--limit",
            "100",
        ],
    )
    .await
    .map_err(|e| format!("couldn't look for the open pull request: {e}"))?;
    if let Some(open) = own_pull_request(&listed, repo, base, heads) {
        // The commits are on it already; a description that didn't update isn't worth failing for.
        let edited = run(gh, &["pr", "edit", &open, "-R", repo, "--body-file", &body])
            .await
            .is_ok();
        return Ok((open, if edited { Sent::Edited } else { Sent::Nothing }));
    }
    let title = format!("--title={title}");
    let created = run(
        gh,
        &[
            "pr",
            "create",
            "-R",
            repo,
            "--base",
            base,
            "--head",
            branch,
            &title,
            "--body-file",
            &body,
        ],
    )
    .await
    .map_err(|e| format!("couldn't open the pull request: {e}"))?;
    let url = created.lines().last().unwrap_or("").trim().to_owned();
    if pull_request_number(&url, repo).is_none() {
        return Err("gh opened no pull request of this repository.".into());
    }
    Ok((url, Sent::Opened))
}

/// The body of issue `number` in `repo`, or none when `gh` can't read it. Only its shape is used
/// (whether it has a checklist), never sent anywhere.
pub async fn issue_body(gh: &Path, repo: &str, number: u64) -> Option<String> {
    let number = number.to_string();
    run(
        gh,
        &[
            "issue", "view", &number, "-R", repo, "--json", "body", "-q", ".body",
        ],
    )
    .await
    .ok()
}

/// What the follower asks of a pull request: its status, and what a merge needs (crate::merge).
/// Its reviews aren't among them: `latestReviews` has no commit for an approval (cli/cli#14805),
/// so [`reviews`] reads them from the REST API.
const FIELDS: &str = "state,reviewDecision,mergedAt,baseRefName,headRefOid,statusCheckRollup,mergeable,mergeStateStatus,author";

/// Why a read of the pull request failed: a fixed line, since what `gh` printed could hold a
/// credential or someone's words.
const FAILED: &str = "Couldn't refresh the pull request. Will try again shortly.";

/// What GitHub says now about the pull request `number` of `repo`, read at `now`.
pub async fn view(gh: &Path, repo: &str, number: u64, now: i64) -> Result<PullRequest, String> {
    let output = pr_view(gh, repo, number).await?;
    decode(&output, now).ok_or_else(|| FAILED.to_owned())
}

/// [`view`], with what a merge needs to know besides ([`Merging`]): the same read, then its
/// reviews ([`reviews`]). Merging's own error when only that part couldn't be read: the pull
/// request's status still is.
pub async fn view_merging(
    gh: &Path,
    repo: &str,
    number: u64,
    now: i64,
) -> Result<(PullRequest, Result<Merging, String>), String> {
    let output = pr_view(gh, repo, number).await?;
    let pr = decode(&output, now).ok_or_else(|| FAILED.to_owned())?;
    let merging = match decode_merging(&output) {
        Some(mut merging) => match reviews(gh, repo, number).await {
            Ok(reviews) => {
                merging.reviews = reviews;
                match required_approvals(gh, repo, &pr.base).await {
                    Ok(required) => merging.required_approvals = Some(required),
                    Err(refused) => merging.rules_refused = refused,
                }
                Ok(merging)
            }
            Err(_) => Err(FAILED.to_owned()),
        },
        None => Err(FAILED.to_owned()),
    };
    Ok((pr, merging))
}

async fn pr_view(gh: &Path, repo: &str, number: u64) -> Result<String, String> {
    let number = number.to_string();
    run(
        gh,
        &["pr", "view", &number, "--repo", repo, "--json", FIELDS],
    )
    .await
    .map_err(|_| FAILED.to_owned())
}

/// How many approving reviews GitHub requires before a pull request into `base` of `repo` merges:
/// the most its rulesets (`GET repos/{repo}/rules/branches/{base}`, a `pull_request` rule's
/// `required_approving_review_count`) and its classic protection (`…/branches/{base}/protection/
/// required_pull_request_reviews`) ask for. An error when either can't be read
/// ([`classic_reviews`]), and then nothing merges by Oby's own review: GitHub's words when it
/// refused in a way that won't pass by itself ([`refused`]), none when it may (a rate limit, a 5xx,
/// a timeout).
pub async fn required_approvals(gh: &Path, repo: &str, base: &str) -> Result<u32, Option<String>> {
    if !is_repo(repo) || !crate::git::is_branch_name(base) {
        return Err(None);
    }
    let rules = match output(gh, &["api", &format!("repos/{repo}/rules/branches/{base}")]).await {
        Ok(text) => decode_rules(&text).ok_or(None)?,
        Err((printed, said)) => return Err(refused(&printed, &said)),
    };
    let path = format!("repos/{repo}/branches/{base}/protection/required_pull_request_reviews");
    let classic = classic_reviews(output(gh, &["api", &path]).await)?;
    Ok(rules.max(classic))
}

/// GitHub's own words for a refusal that won't pass by itself: a 401, or a 403 that isn't a rate
/// limit (an organization's SSO not authorized for this login, a token without access). None for
/// anything that may (a rate limit, a 5xx, a timeout, an answer it can't read).
pub fn refused(printed: &str, said: &str) -> Option<String> {
    let value: Value = serde_json::from_str(printed).unwrap_or(Value::Null);
    let message = value["message"].as_str().unwrap_or(said);
    let status = value["status"].as_str().map(str::to_owned).or_else(|| {
        ["401", "403"]
            .into_iter()
            .find(|code| said.contains(&format!("(HTTP {code})")))
            .map(str::to_owned)
    })?;
    let limited = message.to_lowercase().contains("rate limit");
    (matches!(status.as_str(), "401" | "403") && !limited && !message.is_empty())
        .then(|| said_line(message))
}

/// At most 200 characters of what GitHub said, on one line.
fn said_line(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > 200 {
        format!("{}…", line.chars().take(199).collect::<String>())
    } else {
        line
    }
}

/// The approving reviews classic protection asks for, from `gh api …/protection/
/// required_pull_request_reviews` (its answer, or on failure what it printed and said): its
/// count; 0 for GitHub's "Branch not protected" and "Required pull request reviews not enabled";
/// 0 as well for "Not Found", what an owner without admin rights gets (GitHub's own
/// `reviewDecision`, `REVIEW_REQUIRED`, still says when that protection asks for a review). Any
/// other refusal (a rate limit, a 5xx, a timeout, an answer it can't read) is none: unknown.
pub fn classic_reviews(answer: Result<String, (String, String)>) -> Result<u32, Option<String>> {
    match answer {
        Ok(text) => decode_protection(&text).ok_or(None),
        Err((printed, said)) => {
            let value: Value = serde_json::from_str(&printed).unwrap_or(Value::Null);
            match value["message"].as_str() {
                Some("Branch not protected" | "Required pull request reviews not enabled")
                    if value["status"] == "404" =>
                {
                    Ok(0)
                }
                Some("Not Found") if value["status"] == "404" => Ok(0),
                _ => Err(refused(&printed, &said)),
            }
        }
    }
}

/// The most approving reviews a branch's rules ask for (`GET …/rules/branches/{branch}`); none
/// when it isn't that list.
pub fn decode_rules(text: &str) -> Option<u32> {
    let rules: Vec<Value> = serde_json::from_str(text).ok()?;
    Some(
        rules
            .iter()
            .filter(|r| r["type"] == "pull_request")
            .filter_map(|r| r["parameters"]["required_approving_review_count"].as_u64())
            .max()
            .unwrap_or(0)
            .min(u64::from(u32::MAX)) as u32,
    )
}

/// The approving reviews a branch's classic protection asks for (`…/protection/
/// required_pull_request_reviews`).
pub fn decode_protection(text: &str) -> Option<u32> {
    let value: Value = serde_json::from_str(text).ok()?;
    value["required_approving_review_count"]
        .as_u64()
        .map(|n| n.min(u64::from(u32::MAX)) as u32)
}

/// Each reviewer's latest review of the pull request `number` that counts ([`decode_reviews`]),
/// every page of `GET repos/{repo}/pulls/{n}/reviews`.
pub async fn reviews(gh: &Path, repo: &str, number: u64) -> Result<Vec<Review>, String> {
    let path = format!("repos/{repo}/pulls/{number}/reviews?per_page=100");
    let text = run_bounded(
        gh,
        &["api", "--paginate", "--slurp", &path],
        8 * 1024 * 1024,
        Duration::from_secs(60),
    )
    .await?;
    decode_reviews(&text).ok_or_else(|| "gh said something else about its reviews.".into())
}

/// One reviewer's latest review that counts, as GitHub's REST API has it: typed status only,
/// never what they wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// Their login; empty for an account GitHub deleted.
    pub login: String,
    /// `APPROVED`, `CHANGES_REQUESTED` or `DISMISSED`.
    pub state: String,
    /// The commit it reviewed (`commit_id`).
    pub commit: Option<String>,
    /// Their `author_association` with the repository: `OWNER`, `MEMBER`, `COLLABORATOR`,
    /// `CONTRIBUTOR`, `NONE`…
    pub association: String,
    /// Not a person's: `user.type` isn't `User` (a bot's, or an account GitHub deleted).
    pub bot: bool,
}

/// Each reviewer's latest review that counts, from `gh api --paginate --slurp …/reviews` (pages
/// of reviews): an approval, a change request or a dismissal. A comment or a pending review never
/// replaces the one before it, so a later comment can't hide a change request. None when it isn't
/// that.
pub fn decode_reviews(text: &str) -> Option<Vec<Review>> {
    let pages: Vec<Vec<Value>> = serde_json::from_str(text).ok()?;
    let mut all: Vec<&Value> = pages.iter().flatten().collect();
    all.sort_by_key(|r| r["id"].as_u64().unwrap_or(0));
    let mut latest: Vec<Review> = vec![];
    for review in all {
        let state = review["state"].as_str().unwrap_or_default();
        if !matches!(state, "APPROVED" | "CHANGES_REQUESTED" | "DISMISSED") {
            continue;
        }
        let login = review["user"]["login"].as_str().unwrap_or_default();
        latest.retain(|r| !r.login.eq_ignore_ascii_case(login));
        latest.push(Review {
            login: login.to_owned(),
            state: state.to_owned(),
            commit: review["commit_id"]
                .as_str()
                .filter(|c| crate::git::is_sha(c))
                .map(str::to_owned),
            association: review["author_association"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            bot: review["user"]["type"] != "User",
        });
    }
    Some(latest)
}

/// What a merge needs to know about a pull request besides [`PullRequest`], read with it
/// (crate::merge): typed status and logins only, never what anyone wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Merging {
    /// Its head's checks on GitHub, summed up: `SUCCESS` (every one passed, was neutral or was
    /// skipped), `PENDING`, `FAILURE`, or `NONE` (it has none); none when GitHub didn't say.
    pub checks: Option<&'static str>,
    /// `MERGEABLE`, `CONFLICTING` or `UNKNOWN`; none when GitHub didn't say.
    pub mergeable: Option<String>,
    /// GitHub's `mergeStateStatus`: `CLEAN`, `HAS_HOOKS`, `BEHIND`, `BLOCKED`, `DIRTY`, `DRAFT`,
    /// `UNSTABLE` or `UNKNOWN`; none when GitHub didn't say.
    pub merge_state: Option<String>,
    /// Who opened it (`author.login`); none when GitHub didn't say.
    pub author: Option<String>,
    /// Each reviewer's latest review that counts ([`reviews`]); empty until read.
    pub reviews: Vec<Review>,
    /// The approving reviews GitHub requires on its base ([`required_approvals`]); none when that
    /// couldn't be read.
    pub required_approvals: Option<u32>,
    /// Why GitHub refused to show those rules, in its words, when that won't pass by itself
    /// (an organization's SSO not authorized, a token without access): the owner can act on it.
    pub rules_refused: Option<String>,
}

/// [`Merging`] from `gh pr view --json` with [`FIELDS`], its reviews not yet read; none when it
/// isn't JSON.
pub fn decode_merging(text: &str) -> Option<Merging> {
    let value: Value = serde_json::from_str(text).ok()?;
    let word = |key: &str, known: &[&str]| {
        value[key]
            .as_str()
            .filter(|v| known.contains(v))
            .map(str::to_owned)
    };
    Some(Merging {
        checks: value["statusCheckRollup"]
            .as_array()
            .map(Vec::as_slice)
            .map(rollup),
        mergeable: word("mergeable", &["MERGEABLE", "CONFLICTING", "UNKNOWN"]),
        merge_state: word(
            "mergeStateStatus",
            &[
                "CLEAN",
                "HAS_HOOKS",
                "BEHIND",
                "BLOCKED",
                "DIRTY",
                "DRAFT",
                "UNSTABLE",
                "UNKNOWN",
            ],
        ),
        author: value["author"]["login"]
            .as_str()
            .filter(|a| !a.is_empty())
            .map(str::to_owned),
        reviews: vec![],
        required_approvals: None,
        rules_refused: None,
    })
}

/// A head's check runs and statuses (`statusCheckRollup`), summed up: any failed is `FAILURE`,
/// then any not finished (or that reads as nothing known) is `PENDING`, then `SUCCESS`; an empty
/// list is `NONE`.
fn rollup(list: &[Value]) -> &'static str {
    const FAILED: [&str; 7] = [
        "FAILURE",
        "ERROR",
        "TIMED_OUT",
        "CANCELLED",
        "ACTION_REQUIRED",
        "STARTUP_FAILURE",
        "STALE",
    ];
    const PASSED: [&str; 3] = ["SUCCESS", "NEUTRAL", "SKIPPED"];
    if list.is_empty() {
        return "NONE";
    }
    // A check run says `conclusion` once `status` is `COMPLETED`; a status says `state`.
    fn outcome(check: &Value) -> Option<&str> {
        let finished = check["status"].as_str().is_none_or(|s| s == "COMPLETED");
        let said = check["conclusion"]
            .as_str()
            .filter(|c| !c.is_empty())
            .or(check["state"].as_str());
        if finished { said } else { None }
    }
    if list
        .iter()
        .any(|c| outcome(c).is_some_and(|o| FAILED.contains(&o)))
    {
        "FAILURE"
    } else if list
        .iter()
        .all(|c| outcome(c).is_some_and(|o| PASSED.contains(&o)))
    {
        "SUCCESS"
    } else {
        "PENDING"
    }
}

/// `gh pr view --json` with [`FIELDS`] (or the first five of them), checked: a known state, a
/// base, and a merge date exactly when merged.
pub fn decode(text: &str, now: i64) -> Option<PullRequest> {
    let value: Value = serde_json::from_str(text).ok()?;
    let state = value["state"].as_str()?;
    if !["OPEN", "CLOSED", "MERGED"].contains(&state) {
        return None;
    }
    let base = value["baseRefName"].as_str().filter(|b| !b.is_empty())?;
    let merged_at = match value["mergedAt"].as_str().filter(|m| !m.is_empty()) {
        Some(date) => Some(obi_protocol::time::parse(date)?),
        None => None,
    };
    if (state == "MERGED") != merged_at.is_some() {
        return None;
    }
    let review_decision = value["reviewDecision"]
        .as_str()
        .filter(|d| ["REVIEW_REQUIRED", "CHANGES_REQUESTED", "APPROVED"].contains(d))
        .map(str::to_owned);
    let head = value["headRefOid"]
        .as_str()
        .filter(|h| crate::git::is_sha(h))
        .map(str::to_owned);
    Some(PullRequest {
        state: state.to_owned(),
        review_decision,
        base: base.to_owned(),
        merged_at,
        checked_at: now,
        head,
    })
}

fn git_sha(text: &str) -> bool {
    crate::git::is_sha(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gh Oby.app carries is found only beside an obid inside an app's Contents.
    #[test]
    fn the_bundled_gh_is_found_only_inside_the_app() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-bundled-gh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let contents = root.join("Oby.app/Contents");
        for dir in ["MacOS", "Helpers"] {
            std::fs::create_dir_all(contents.join(dir)).unwrap();
        }
        let gh = contents.join("MacOS/gh");
        std::fs::write(&gh, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(bundled_gh(&contents.join("Helpers/obid")), Some(gh.clone()));
        assert_eq!(bundled_gh(&contents.join("MacOS/obid")), Some(gh.clone()));
        assert_eq!(bundled_gh(&root.join("bin/obid")), None);
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(bundled_gh(&contents.join("Helpers/obid")), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_pull_requests_of_the_jobs_repository() {
        let repo = "deskree-inc/oby";
        assert_eq!(
            pull_request_number("https://github.com/deskree-inc/oby/pull/42", repo),
            Some(42)
        );
        for url in [
            "https://github.com/deskree-inc/oby/pull/0",
            "https://github.com/deskree-inc/oby/pull/",
            "https://github.com/deskree-inc/oby/pull/42/files",
            "https://github.com/deskree-inc/oby/pull/42?x=1",
            "https://github.com/deskree-inc/oby2/pull/42",
            "https://github.com/other/oby/pull/42",
            "http://github.com/deskree-inc/oby/pull/42",
            "https://github.com.evil/deskree-inc/oby/pull/42",
            "https://user@github.com/deskree-inc/oby/pull/42",
            "https://github.com/deskree-inc/oby/issues/42",
        ] {
            assert_eq!(pull_request_number(url, repo), None, "{url}");
        }
        assert_eq!(
            pull_request_number("https://github.com/../x/pull/1", "../x"),
            None
        );
    }

    /// Only a pull request from this repository's own branch, at a commit obid pushed: never a
    /// fork's pull request from a branch of the same name.
    #[test]
    fn only_its_own_open_pull_request_is_adopted() {
        let repo = "deskree-inc/oby";
        let head = "a".repeat(40);
        let before = "b".repeat(40);
        let pr = |url: &str, cross: bool, oid: &str| json!({"url": url, "isCrossRepository": cross, "headRefOid": oid, "baseRefName": "dev"});
        let ours = "https://github.com/deskree-inc/oby/pull/42";
        let list = |items: Vec<Value>| Value::Array(items).to_string();
        assert_eq!(
            own_pull_request(&list(vec![pr(ours, false, &head)]), repo, "dev", &[&head]),
            Some(ours.into())
        );
        assert_eq!(
            own_pull_request(
                &list(vec![pr(ours, false, &before)]),
                repo,
                "dev",
                &[&head, &before]
            ),
            Some(ours.into())
        );
        for listed in [
            list(vec![pr(ours, true, &head)]),
            list(vec![pr(ours, false, &"c".repeat(40))]),
            list(vec![pr(
                "https://github.com/other/oby/pull/42",
                false,
                &head,
            )]),
            list(vec![json!({"url": ours})]),
            // Someone's pull request of the same branch at obid's commit, into another base.
            list(vec![
                json!({"url": ours, "isCrossRepository": false, "headRefOid": head, "baseRefName": "main"}),
            ]),
            String::new(),
            "not json".into(),
        ] {
            assert_eq!(
                own_pull_request(&listed, repo, "dev", &[&head]),
                None,
                "{listed}"
            );
        }
    }

    #[test]
    fn only_githubs_remotes_get_pull_requests() {
        for remote in [
            "https://github.com/deskree-inc/oby.git",
            "ssh://git@github.com/deskree-inc/oby.git",
            "git@github.com:deskree-inc/oby.git",
            "HTTPS://GitHub.com/deskree-inc/oby",
        ] {
            assert!(is_remote(remote), "{remote}");
        }
        for remote in [
            "file:///srv/fixture.git",
            "/srv/fixture.git",
            "https://github.com.evil/deskree-inc/oby",
            "https://gitlab.com/deskree-inc/oby.git",
            "git@gitlab.com:deskree-inc/oby.git",
        ] {
            assert!(!is_remote(remote), "{remote}");
        }
    }

    #[test]
    fn repositories() {
        assert!(is_repo("obi-e2e/fixture-app"));
        assert!(is_repo("deskree-inc/oby.v2_x"));
        for bad in [
            "", "a", "a/", "/b", "a/b/c", "a/..", "a b/c", "a/b?c", "a/b#c",
        ] {
            assert!(!is_repo(bad), "{bad}");
        }
    }

    #[test]
    fn what_gh_says_about_a_pull_request() {
        let now = 1_791_582_173;
        let open = decode(
            r#"{"state":"OPEN","reviewDecision":"","mergedAt":null,"baseRefName":"dev","headRefOid":"0123456789abcdef0123456789abcdef01234567"}"#,
            now,
        )
        .unwrap();
        assert_eq!(open.state, "OPEN");
        assert_eq!(open.review_decision, None);
        assert_eq!(open.base, "dev");
        assert_eq!(
            open.to_json(),
            json!({"state": "OPEN", "reviewDecision": null, "base": "dev",
                   "checkedAt": "2026-10-09T21:42:53Z", "head": "0123456789abcdef0123456789abcdef01234567"})
        );
        let merged = decode(
            r#"{"state":"MERGED","reviewDecision":"APPROVED","mergedAt":"2026-10-09T22:00:00Z","baseRefName":"dev","headRefOid":""}"#,
            now,
        )
        .unwrap();
        assert_eq!(merged.review_decision.as_deref(), Some("APPROVED"));
        assert_eq!(merged.to_json()["mergedAt"], "2026-10-09T22:00:00Z");
        assert!(merged.to_json().get("head").is_none());
        for bad in [
            r#"{"state":"MERGED","mergedAt":null,"baseRefName":"dev"}"#,
            r#"{"state":"OPEN","mergedAt":"2026-10-09T22:00:00Z","baseRefName":"dev"}"#,
            r#"{"state":"OPEN","baseRefName":""}"#,
            r#"{"state":"DRAFT","baseRefName":"dev"}"#,
            "not json",
        ] {
            assert_eq!(decode(bad, now), None, "{bad}");
        }
    }

    /// What a merge needs, from the follower's `gh pr view`: the head's checks summed up, GitHub's
    /// words for whether it merges, and who opened it.
    #[test]
    fn what_a_merge_needs_to_know() {
        let read = |value: Value| decode_merging(&value.to_string()).unwrap();
        let checks = |list: Value| read(json!({"statusCheckRollup": list})).checks;
        let run = |status: &str, conclusion: &str| json!({"__typename": "CheckRun", "status": status, "conclusion": conclusion});
        let status = |state: &str| json!({"__typename": "StatusContext", "state": state});
        assert_eq!(checks(json!([])), Some("NONE"));
        assert_eq!(checks(json!(null)), None);
        assert_eq!(
            checks(json!([
                run("COMPLETED", "SUCCESS"),
                run("COMPLETED", "SKIPPED"),
                run("COMPLETED", "NEUTRAL"),
                status("SUCCESS")
            ])),
            Some("SUCCESS")
        );
        for red in [
            run("COMPLETED", "FAILURE"),
            run("COMPLETED", "TIMED_OUT"),
            run("COMPLETED", "CANCELLED"),
            run("COMPLETED", "ACTION_REQUIRED"),
            status("ERROR"),
            status("FAILURE"),
        ] {
            assert_eq!(
                checks(json!([run("IN_PROGRESS", ""), red.clone()])),
                Some("FAILURE"),
                "{red}"
            );
        }
        for pending in [
            run("IN_PROGRESS", ""),
            run("QUEUED", ""),
            status("PENDING"),
            status("EXPECTED"),
            json!({"name": "odd"}),
            run("COMPLETED", "SOMETHING_NEW"),
        ] {
            assert_eq!(
                checks(json!([status("SUCCESS"), pending.clone()])),
                Some("PENDING"),
                "{pending}"
            );
        }
        let merging = read(json!({
            "author": {"login": "wellprado"},
            "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
        }));
        assert_eq!(merging.author.as_deref(), Some("wellprado"));
        assert_eq!(merging.mergeable.as_deref(), Some("MERGEABLE"));
        assert_eq!(merging.merge_state.as_deref(), Some("CLEAN"));
        assert!(merging.reviews.is_empty());
        let odd = read(
            json!({"author": {"login": ""}, "mergeable": "maybe", "mergeStateStatus": "FINE"}),
        );
        assert_eq!(
            (odd.author, odd.mergeable, odd.merge_state),
            (None, None, None)
        );
        assert_eq!(decode_merging("not json"), None);
    }

    /// Classic protection's answers, as gh prints them: only GitHub's own "not required" (and the
    /// non-admin "Not Found") are 0; a rate limit, a 5xx or anything unread is unknown.
    #[test]
    fn classic_protection_fails_closed() {
        let failed = |body: &str, said: &str| Err((body.to_owned(), said.to_owned()));
        let doc = "https://docs.github.com/rest/branches/branch-protection#get-pull-request-review-protection";
        assert_eq!(
            classic_reviews(Ok(r#"{"required_approving_review_count": 2}"#.into())),
            Ok(2)
        );
        for message in [
            "Branch not protected",
            "Required pull request reviews not enabled",
            "Not Found",
        ] {
            let body =
                json!({"message": message, "documentation_url": doc, "status": "404"}).to_string();
            assert_eq!(
                classic_reviews(failed(&body, &format!("gh: {message} (HTTP 404)"))),
                Ok(0),
                "{message}"
            );
        }
        // May pass by itself: unknown, with nothing for the owner to do.
        let limited = json!({"message": "API rate limit exceeded for user ID 1.", "documentation_url": doc, "status": "403"});
        assert_eq!(
            classic_reviews(failed(
                &limited.to_string(),
                "gh: API rate limit exceeded (HTTP 403)"
            )),
            Err(None)
        );
        assert_eq!(
            classic_reviews(failed("", "gh: HTTP 502: Bad Gateway")),
            Err(None)
        );
        assert_eq!(
            classic_reviews(failed("<html>oops</html>", "gh: HTTP 500")),
            Err(None)
        );
        assert_eq!(
            classic_reviews(failed("", "gh took more than 120 seconds")),
            Err(None)
        );
        assert_eq!(classic_reviews(Ok("not json".into())), Err(None));
        // Won't pass by itself: GitHub's own words, for the owner.
        let sso = "Resource protected by organization SAML enforcement. You must grant your Personal Access token access to this organization.";
        let saml = json!({"message": sso, "documentation_url": doc, "status": "403"});
        assert_eq!(
            classic_reviews(failed(
                &saml.to_string(),
                "gh: Resource protected by organization SAML enforcement. (HTTP 403)"
            )),
            Err(Some(sso.into()))
        );
        let forbidden =
            json!({"message": "Resource not accessible by personal access token", "status": "403"});
        assert_eq!(
            classic_reviews(failed(
                &forbidden.to_string(),
                "gh: Resource not accessible (HTTP 403)"
            )),
            Err(Some(
                "Resource not accessible by personal access token".into()
            ))
        );
        assert_eq!(
            refused("", "gh: Bad credentials (HTTP 401)"),
            Some("gh: Bad credentials (HTTP 401)".into())
        );
    }

    /// What a branch's rules and classic protection require, as GitHub answers them.
    #[test]
    fn the_approvals_github_requires() {
        assert_eq!(decode_rules("[]"), Some(0));
        let rules = json!([
            {"type": "deletion", "ruleset_id": 1},
            {"type": "pull_request", "ruleset_id": 2, "parameters": {"required_approving_review_count": 1,
              "dismiss_stale_reviews_on_push": true, "require_code_owner_review": false}},
            {"type": "pull_request", "ruleset_id": 3, "parameters": {"required_approving_review_count": 2}},
        ]);
        assert_eq!(decode_rules(&rules.to_string()), Some(2));
        assert_eq!(decode_rules("not json"), None);
        assert_eq!(
            decode_protection(
                r#"{"required_approving_review_count": 1, "dismiss_stale_reviews": false}"#
            ),
            Some(1)
        );
        assert_eq!(
            decode_protection(r#"{"message": "Branch not protected"}"#),
            None
        );
    }

    /// One review as `GET repos/{o}/{r}/pulls/{n}/reviews` answers it (cli/cli#14805's, trimmed).
    fn rest_review(
        id: u64,
        login: &str,
        kind: &str,
        state: &str,
        commit: &str,
        association: &str,
    ) -> Value {
        json!({
            "id": id,
            "node_id": "PRR_kwDODKw3uc8AAAABT_Ie6w",
            "user": {"login": login, "id": 75_402_236, "type": kind, "site_admin": false},
            "body": "",
            "state": state,
            "html_url": format!("https://github.com/cli/cli/pull/14805#pullrequestreview-{id}"),
            "author_association": association,
            "submitted_at": "2026-10-23T19:55:21Z",
            "commit_id": commit,
        })
    }

    /// Each reviewer's latest review that counts, across pages: a comment or a pending review
    /// never replaces a change request.
    #[test]
    fn each_reviewers_latest_review_counts() {
        let head = "17ac598e6afc43bb6206f074b05de9d63563a42f";
        let older = "b".repeat(40);
        let pages = json!([
            [
                rest_review(
                    1,
                    "copilot-pull-request-reviewer[bot]",
                    "Bot",
                    "COMMENTED",
                    head,
                    "CONTRIBUTOR"
                ),
                rest_review(2, "ana", "User", "APPROVED", &older, "MEMBER"),
                rest_review(
                    3,
                    "joe",
                    "User",
                    "CHANGES_REQUESTED",
                    &older,
                    "COLLABORATOR"
                ),
            ],
            [
                rest_review(4, "ana", "User", "APPROVED", head, "MEMBER"),
                rest_review(5, "joe", "User", "COMMENTED", head, "COLLABORATOR"),
                rest_review(6, "max", "User", "PENDING", head, "MEMBER"),
                rest_review(7, "app[bot]", "Bot", "APPROVED", head, "NONE"),
            ],
        ]);
        let latest = decode_reviews(&pages.to_string()).unwrap();
        let read: Vec<_> = latest
            .iter()
            .map(|r| {
                (
                    r.login.as_str(),
                    r.state.as_str(),
                    r.commit.as_deref(),
                    r.association.as_str(),
                    r.bot,
                )
            })
            .collect();
        assert_eq!(
            read,
            [
                (
                    "joe",
                    "CHANGES_REQUESTED",
                    Some(older.as_str()),
                    "COLLABORATOR",
                    false
                ),
                ("ana", "APPROVED", Some(head), "MEMBER", false),
                ("app[bot]", "APPROVED", Some(head), "NONE", true),
            ]
        );
        // Dismissed, an approval no longer stands.
        let dismissed = json!([[
            rest_review(1, "ana", "User", "APPROVED", head, "MEMBER"),
            rest_review(2, "ana", "User", "DISMISSED", head, "MEMBER"),
        ]]);
        assert_eq!(
            decode_reviews(&dismissed.to_string()).unwrap()[0].state,
            "DISMISSED"
        );
        assert_eq!(decode_reviews("[]"), Some(vec![]));
        assert_eq!(decode_reviews("not json"), None);
    }

    #[test]
    fn what_a_tool_said_keeps_its_last_line() {
        assert_eq!(
            said("hint: one\nerror: it broke\nhint: try again\n"),
            "error: it broke"
        );
        assert_eq!(said(""), "no reason given");
        assert_eq!(said(&"x".repeat(400)).chars().count(), 300);
    }
}
