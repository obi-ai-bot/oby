//! What the owner's own word has Oby do at once from a conversation (AGENTS.md, "Oby forbids
//! nothing"; docs/contracts/task-conversations.md, TASKCONV-6 to TASKCONV-12): merge a pull request,
//! write on GitHub, change a setting. obid does it with Oby's own GitHub login, which no agent
//! ever gets; the code host's own rules (branch protection, required reviews) are the limit, and
//! when GitHub refuses, Oby says what it said. The tools (`oby_merge`, `oby_github_write`,
//! `oby_settings`, super::tools) do it on the owner's signed words in a turn that read nothing
//! untrusted; on any other turn they make a Confirm, which carries it out here
//! (crate::sphere::link) on the owner's click.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::github;
use crate::jobs::Jobs;
use crate::permissions::Mode;

/// How a pull request merges on GitHub.
pub const METHODS: [&str; 3] = ["squash", "merge", "rebase"];

/// `oby_merge`'s request: any pull request, into any base, `main` included.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Merge {
    pub repo: String,
    pub pr: u64,
    /// squash, merge or rebase; left out, a job's workflow, else squash.
    #[serde(default)]
    pub method: Option<String>,
}

impl Merge {
    pub fn problem(&self) -> Option<String> {
        if !github::is_repo(&self.repo) {
            return Some(format!("repo is owner/name, not {}.", self.repo));
        }
        if self.pr == 0 {
            return Some("pr is the pull request's number.".into());
        }
        match self.method.as_deref() {
            Some(m) if !METHODS.contains(&m) => {
                Some(format!("method is squash, merge or rebase, not {m}."))
            }
            _ => None,
        }
    }

    /// What it does, in obid's words: a card's title, with the base and the head it merges, and
    /// how (a job's pull request goes its workflow's way).
    pub fn title(&self, at: &Open, job: bool) -> String {
        let way = match (job, self.method.as_deref()) {
            (true, _) => "its job's workflow",
            (false, Some(method)) => method,
            (false, None) => "squash",
        };
        let short: String = at.head.chars().take(7).collect();
        format!(
            "Merge {}#{} into {} at {short} ({way})",
            self.repo, self.pr, at.base
        )
    }
}

/// An open pull request as GitHub has it now: typed status only, never what a person wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct Open {
    pub head: String,
    pub base: String,
}

/// The pull request, open, or why it can't merge (closed, merged, unread).
pub async fn open(repo: &str, pr: u64) -> Result<Open, String> {
    let gh = github::gh().ok_or("GitHub's CLI (gh) isn't installed.")?;
    let read = github::run(&gh, &["api", &format!("repos/{repo}/pulls/{pr}")])
        .await
        .map_err(|said| format!("Oby couldn't read {repo}#{pr} on GitHub ({said})."))?;
    open_of(repo, pr, &read)
}

fn open_of(repo: &str, pr: u64, read: &str) -> Result<Open, String> {
    let pull: Value = serde_json::from_str(read)
        .map_err(|_| format!("GitHub's answer about {repo}#{pr} wasn't JSON."))?;
    if pull["merged"] == true {
        return Err(format!("{repo}#{pr} is merged already."));
    }
    if pull["state"] != "open" {
        return Err(format!("{repo}#{pr} is closed: reopen it on GitHub first."));
    }
    let text = |v: &Value| v.as_str().map(str::to_owned);
    match (text(&pull["head"]["sha"]), text(&pull["base"]["ref"])) {
        (Some(head), Some(base)) => Ok(Open { head, base }),
        _ => Err(format!("GitHub didn't say {repo}#{pr}'s head and base.")),
    }
}

/// The job whose own pull request `asked` is, of any Obi.
pub fn job_of(jobs: &Jobs, asked: &Merge) -> Option<crate::job::Job> {
    pick(jobs.list(usize::MAX), asked)
}

/// Of `jobs`, the one `asked` is about: one whose own published pull request it is, or a review
/// another Oby asked for of it (which has `pr`, never `published`). That review wins, so its pull
/// request is never merged from here.
fn pick(jobs: Vec<crate::job::Job>, asked: &Merge) -> Option<crate::job::Job> {
    let mut about = jobs.into_iter().filter(|j| {
        j.repo == asked.repo
            && (j
                .published
                .as_deref()
                .and_then(|url| github::pull_request_number(url, &j.repo))
                == Some(asked.pr)
                || (j.obi.is_some() && j.pr == Some(asked.pr)))
    });
    let first = about.next()?;
    if first.obi.is_some() {
        return Some(first);
    }
    Some(about.find(|j| j.obi.is_some()).unwrap_or(first))
}

/// The Confirm of `asked` the owner clicked: GitHub read again, and merged only while the pull
/// request still has the base and head the card named.
pub async fn merge_confirmed(
    jobs: &Arc<Jobs>,
    home: &Path,
    conversation: &str,
    asked: &Merge,
    pinned: &Open,
) -> Result<String, String> {
    let now = open(&asked.repo, asked.pr).await;
    let moved = match &now {
        Ok(now) if now == pinned => None,
        Ok(now) => Some(format!(
            "it moved since the card: it's now into {} at {}. Ask again.",
            now.base,
            now.head.chars().take(7).collect::<String>()
        )),
        Err(why) => Some(why.clone()),
    };
    if let Some(why) = moved {
        record(jobs, conversation, "merge", false);
        return Err(format!("Nothing merged: {why}"));
    }
    merge(jobs, home, conversation, asked, pinned).await
}

/// Merges `asked` at `at`, the base and head the owner asked about (TASKCONV-7): a job's own pull
/// request through obid's merge path, as the owner's Merge on its card (`Jobs::merge_by_owner`:
/// its record, its branch, and every merge gate of obid's that isn't only the owner's say-so);
/// anyone else's with GitHub's merge pinned to that head, its only limit GitHub's own protection.
/// Never one of a review another Oby asked for. What GitHub refuses is said as GitHub said it.
/// Every attempt goes in the use log, `conversation-<id>`'s.
pub async fn merge(
    jobs: &Arc<Jobs>,
    home: &Path,
    conversation: &str,
    asked: &Merge,
    at: &Open,
) -> Result<String, String> {
    let merged = merge_at(jobs, home, asked, at).await;
    record(jobs, conversation, "merge", merged.is_ok());
    merged
}

async fn merge_at(
    jobs: &Arc<Jobs>,
    home: &Path,
    asked: &Merge,
    at: &Open,
) -> Result<String, String> {
    let name = format!("{}#{}", asked.repo, asked.pr);
    let head = at.head.as_str();
    if let Some(job) = job_of(jobs, asked) {
        if job.obi.is_some() {
            return Err(format!(
                "{name} is the pull request of a review another Oby asked for: it isn't yours to merge from here."
            ));
        }
        return match jobs.merge_by_owner(&job.id, head, false).await? {
            Some(note) => Ok(note),
            None => Ok(format!(
                "Merged {name}, {}'s pull request.",
                job.short_title()
            )),
        };
    }
    let gh = github::gh().ok_or("GitHub's CLI (gh) isn't installed.")?;
    let method = asked.method.as_deref().unwrap_or("squash");
    let answer = send(
        &gh,
        home,
        "PUT",
        &format!("repos/{}/pulls/{}/merge", asked.repo, asked.pr),
        &json!({"merge_method": method, "sha": head}),
    )
    .await
    .map_err(|said| format!("GitHub didn't merge {name}: {said}"))?;
    if serde_json::from_str::<Value>(&answer).is_ok_and(|v| v["merged"] == true) {
        Ok(format!("Merged {name} into {} ({method}).", at.base))
    } else {
        Err(format!(
            "GitHub didn't say it merged {name}: look at it on GitHub."
        ))
    }
}

/// `oby_github_write`'s request: one write on GitHub, as the owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Write {
    pub repo: String,
    /// comment, approve, request_changes, close, label or create_issue.
    pub action: String,
    /// The issue or pull request; none for create_issue.
    #[serde(default)]
    pub number: Option<u64>,
    #[serde(default)]
    pub body: Option<String>,
    /// create_issue's title.
    #[serde(default)]
    pub title: Option<String>,
    /// label's labels, or create_issue's.
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    /// The pull request's head a review is of (`commit_id`): obid sets it from GitHub, never the
    /// brain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

pub const ACTIONS: [&str; 6] = [
    "comment",
    "approve",
    "request_changes",
    "close",
    "label",
    "create_issue",
];

impl Write {
    /// The REST call it is: method, path and body, or why it isn't one.
    pub fn request(&self) -> Result<(&'static str, String, Value), String> {
        if !github::is_repo(&self.repo) {
            return Err(format!("repo is owner/name, not {}.", self.repo));
        }
        fn text(v: &Option<String>) -> Option<&str> {
            v.as_deref().map(str::trim).filter(|t| !t.is_empty())
        }
        let body = text(&self.body);
        let labels = self.labels.clone().unwrap_or_default();
        let repo = &self.repo;
        if self.action == "create_issue" {
            let title = text(&self.title).ok_or("create_issue needs a title.")?;
            let mut issue = json!({"title": title, "body": body.unwrap_or("")});
            if !labels.is_empty() {
                issue["labels"] = json!(labels);
            }
            return Ok(("POST", format!("repos/{repo}/issues"), issue));
        }
        let n = self.number.filter(|n| *n > 0).ok_or_else(|| {
            format!(
                "{} needs the issue's or pull request's number.",
                self.action
            )
        })?;
        Ok(match self.action.as_str() {
            "comment" => (
                "POST",
                format!("repos/{repo}/issues/{n}/comments"),
                json!({"body": body.ok_or("comment needs a body.")?}),
            ),
            "approve" | "request_changes" => {
                let mut review = if self.action == "approve" {
                    json!({"event": "APPROVE", "body": body.unwrap_or("")})
                } else {
                    json!({"event": "REQUEST_CHANGES", "body": body.ok_or("request_changes needs a body: what to change.")?})
                };
                if let Some(commit) = &self.commit {
                    review["commit_id"] = json!(commit);
                }
                ("POST", format!("repos/{repo}/pulls/{n}/reviews"), review)
            }
            "close" => (
                "PATCH",
                format!("repos/{repo}/issues/{n}"),
                json!({"state": "closed"}),
            ),
            "label" if labels.is_empty() => return Err("label needs labels.".into()),
            "label" => (
                "POST",
                format!("repos/{repo}/issues/{n}/labels"),
                json!({ "labels": labels }),
            ),
            other => {
                return Err(format!(
                    "action is comment, approve, request_changes, close, label or create_issue, not {other}."
                ));
            }
        })
    }

    /// A review's: the pull request's head as GitHub has it now, which it is of.
    pub fn reviews(&self) -> bool {
        self.action == "approve" || self.action == "request_changes"
    }

    /// What a card quotes of it: the title, the body and the labels, as they'd go.
    pub fn shown(&self) -> String {
        let mut parts: Vec<String> = [&self.title, &self.body]
            .into_iter()
            .flatten()
            .filter(|t| !t.trim().is_empty())
            .cloned()
            .collect();
        if let Some(labels) = self.labels.as_ref().filter(|l| !l.is_empty()) {
            parts.push(format!("Labels: {}", labels.join(", ")));
        }
        parts.join("\n\n")
    }

    /// What it does, in obid's words: a card's title. Never the body, which is quoted.
    pub fn title(&self) -> String {
        let on = format!("{}#{}", self.repo, self.number.unwrap_or_default());
        match self.action.as_str() {
            "comment" => format!("Comment on {on}"),
            "approve" | "request_changes" => {
                let at = self
                    .commit
                    .as_deref()
                    .map(|c| format!(" at {}", c.chars().take(7).collect::<String>()))
                    .unwrap_or_default();
                if self.action == "approve" {
                    format!("Approve {on}{at} on GitHub")
                } else {
                    format!("Request changes on {on}{at}")
                }
            }
            "close" => format!("Close {on}"),
            "label" => format!("Label {on}"),
            _ => format!("Open an issue in {}", self.repo),
        }
    }
}

/// Writes `asked` on GitHub as the owner, once the secret scan passes what goes: its address on
/// GitHub, or why not (GitHub's own words when it refused). Every attempt goes in the use log,
/// `conversation-<id>`'s.
pub async fn write(
    jobs: &Jobs,
    home: &Path,
    conversation: &str,
    asked: &Write,
) -> Result<String, String> {
    let written = write_now(home, asked).await;
    record(jobs, conversation, &asked.action, written.is_ok());
    written
}

/// One write of the conversation's on GitHub in obid's connection-use log (`vault/uses.jsonl`), as
/// Tetrix's calls are: job `conversation-<id>`, connection `github`, a `write`, allowed or not.
pub(super) fn record(jobs: &Jobs, conversation: &str, tool: &str, allowed: bool) {
    jobs.record_use(crate::connections::ConnectionUse {
        job: format!("conversation-{conversation}"),
        connection: "github".into(),
        tool: tool.chars().take(64).collect(),
        access: "write".into(),
        allowed,
        secret: "gh".into(),
        host: "github.com".into(),
        date: obi_protocol::time::format(obi_protocol::time::now()),
    });
}

async fn write_now(home: &Path, asked: &Write) -> Result<String, String> {
    let (method, path, body) = asked.request()?;
    let gh = github::gh().ok_or("GitHub's CLI (gh) isn't installed.")?;
    let what = asked.title();
    let answer = send(&gh, home, method, &path, &body)
        .await
        .map_err(|said| format!("{what} didn't happen: {said}"))?;
    let url = serde_json::from_str::<Value>(&answer)
        .ok()
        .and_then(|v| v["html_url"].as_str().map(str::to_owned));
    Ok(match url {
        Some(url) => format!("Done: {what} ({url})."),
        None => format!("Done: {what}."),
    })
}

/// `gh api -X <method> <path>` with `body`, scanned for secrets first and written to a 0600 file,
/// never an argument. Nothing goes when the scan finds something or can't run.
async fn send(
    gh: &Path,
    home: &Path,
    method: &str,
    path: &str,
    body: &Value,
) -> Result<String, String> {
    let scratch = home
        .join("tmp")
        .join(format!("owners-word-{}", crate::permissions::new_id()));
    crate::store::make_private_dir(&scratch)
        .map_err(|e| format!("its request couldn't be written ({e})"))?;
    let sent = async {
        match crate::scan::scan(&[("github.json".into(), body.to_string())], &scratch).await {
            crate::scan::Verdict::Clean => {}
            crate::scan::Verdict::Found(_) => {
                return Err("the secret scan found something in it, so nothing was sent".into());
            }
            crate::scan::Verdict::CouldNotRun(why) => {
                return Err(format!(
                    "the secret scan couldn't run ({why}), so nothing was sent"
                ));
            }
        }
        let input = scratch.join("request.json");
        crate::store::write_private(&input, body.to_string().as_bytes())
            .map_err(|e| format!("its request couldn't be written ({e})"))?;
        github::run(
            gh,
            &[
                "api",
                "-X",
                method,
                path,
                "--input",
                &input.to_string_lossy(),
            ],
        )
        .await
    }
    .await;
    let _ = std::fs::remove_dir_all(&scratch);
    sent
}

/// `oby_settings`'s change: any of these, as Settings has them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub max_running: Option<u32>,
    #[serde(default)]
    pub account_failover: Option<bool>,
    /// ask, acceptEdits, plan, sandbox or bypass: Settings › Permissions.
    #[serde(default)]
    pub default_mode: Option<String>,
}

impl Settings {
    pub fn mode(&self) -> Result<Option<Mode>, String> {
        match self.default_mode.as_deref() {
            None => Ok(None),
            Some(text) => Mode::parse(text).map(Some).ok_or_else(|| {
                format!("defaultMode is ask, acceptEdits, plan, sandbox or bypass, not {text}.")
            }),
        }
    }

    /// It lets Oby do more than it does now without asking (TASKCONV-10): a default mode that
    /// asks less, more jobs at once, or account failover turned on.
    pub fn widens(&self, now: &Now) -> bool {
        self.mode()
            .ok()
            .flatten()
            .is_some_and(|m| m.reach() > now.mode.reach())
            || self.max_running.is_some_and(|max| max > now.max_running)
            || (self.account_failover == Some(true) && !now.account_failover)
    }

    /// What it changes, in obid's words.
    pub fn words(&self) -> String {
        let mut parts = vec![];
        if let Some(max) = self.max_running {
            parts.push(format!("{max} jobs at once"));
        }
        if let Some(on) = self.account_failover {
            parts.push(format!(
                "account failover {}",
                if on { "on" } else { "off" }
            ));
        }
        if let Ok(Some(mode)) = self.mode() {
            parts.push(format!("{} as the default mode", mode.title()));
        }
        parts.join(", ")
    }

    /// All of it or none of it: every value is checked first, and a step that can't be kept puts
    /// back the ones kept before it.
    pub fn apply(&self, jobs: &Arc<Jobs>, home: &Path) -> Result<String, String> {
        let mode = self.mode()?;
        if self.max_running.is_none() && self.account_failover.is_none() && mode.is_none() {
            return Err("Say what to change: maxRunning, accountFailover or defaultMode.".into());
        }
        if let Some(max) = self.max_running
            && !crate::settings::MAX_RUNNING_RANGE.contains(&max)
        {
            return Err("maxRunning is 1 to 20.".into());
        }
        let was = Now::of(jobs);
        let unkept =
            |e: &dyn std::fmt::Display| format!("Nothing changed: it couldn't be kept ({e}).");
        let failover = |on: bool| crate::settings::update(home, |s| s.account_failover = Some(on));
        if let Some(on) = self.account_failover {
            failover(on).map_err(|e| unkept(&e))?;
        }
        if let Some(max) = self.max_running
            && let Err(e) = jobs.set_max_running(max)
        {
            let _ = failover(was.account_failover);
            return Err(unkept(&e));
        }
        if let Some(mode) = mode
            && let Err(e) = jobs.set_default_mode(mode)
        {
            let _ = failover(was.account_failover);
            let _ = jobs.set_max_running(was.max_running);
            return Err(unkept(&e));
        }
        Ok(format!("Saved: {}.", self.words()))
    }
}

/// Settings as they are now, what [`Settings::widens`] is measured against.
#[derive(Debug, Clone, PartialEq)]
pub struct Now {
    pub mode: Mode,
    pub max_running: u32,
    pub account_failover: bool,
}

impl Now {
    pub fn of(jobs: &Jobs) -> Now {
        Now {
            mode: jobs.default_mode(),
            max_running: u32::try_from(jobs.max_running()).unwrap_or(u32::MAX),
            account_failover: jobs.account_failover(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(action: &str, number: Option<u64>, body: Option<&str>) -> Write {
        Write {
            repo: "o/r".into(),
            action: action.into(),
            number,
            body: body.map(str::to_owned),
            title: None,
            labels: None,
            commit: None,
        }
    }

    /// TASKCONV-8: each write is the one REST call it names, and one missing what it needs is
    /// refused with why before anything is sent.
    #[test]
    fn taskconv_8_each_github_write_is_its_rest_call() {
        let comment = write("comment", Some(7), Some("Looks good"))
            .request()
            .unwrap();
        assert_eq!(comment.0, "POST");
        assert_eq!(comment.1, "repos/o/r/issues/7/comments");
        assert_eq!(comment.2, json!({"body": "Looks good"}));
        let approve = write("approve", Some(7), None).request().unwrap();
        assert_eq!(approve.2["event"], "APPROVE");
        assert_eq!(approve.1, "repos/o/r/pulls/7/reviews");
        assert!(write("request_changes", Some(7), None).request().is_err());
        assert_eq!(
            write("request_changes", Some(7), Some("Rename it"))
                .request()
                .unwrap()
                .2["event"],
            "REQUEST_CHANGES"
        );
        let close = write("close", Some(7), None).request().unwrap();
        assert_eq!((close.0, close.2), ("PATCH", json!({"state": "closed"})));
        let mut label = write("label", Some(7), None);
        assert!(label.request().is_err());
        label.labels = Some(vec!["bug".into()]);
        assert_eq!(label.request().unwrap().2, json!({"labels": ["bug"]}));
        let mut issue = write("create_issue", None, Some("Steps"));
        assert!(issue.request().is_err());
        issue.title = Some("It breaks".into());
        assert_eq!(
            issue.request().unwrap(),
            (
                "POST",
                "repos/o/r/issues".into(),
                json!({"title": "It breaks", "body": "Steps"})
            )
        );
        assert!(write("comment", None, Some("x")).request().is_err());
        // A review is of the head GitHub had: obid's commit, never the brain's.
        let mut pinned = write("approve", Some(7), None);
        pinned.commit = Some("abcdef0123".into());
        assert_eq!(pinned.request().unwrap().2["commit_id"], "abcdef0123");
        assert_eq!(pinned.title(), "Approve o/r#7 at abcdef0 on GitHub");
        let mut labelled = write("label", Some(7), None);
        labelled.labels = Some(vec!["bug".into(), "p1".into()]);
        assert_eq!(labelled.shown(), "Labels: bug, p1");
        assert!(write("delete", Some(7), None).request().is_err());
        assert_eq!(write("close", Some(7), None).title(), "Close o/r#7");
    }

    /// TASKCONV-7: a review another Oby asked for of a pull request (`pr`, no `published`) is what
    /// it's about, before the owner's own job on it: so it's refused, never merged from here.
    #[test]
    fn taskconv_7_a_teammates_review_is_found_by_its_pull_request() {
        let job = |id: &str, repo: &str| {
            crate::job::Job::new(
                id.into(),
                repo.into(),
                "review".into(),
                "Review it".into(),
                "claude".into(),
                "a".into(),
            )
        };
        let asked = Merge {
            repo: "o/r".into(),
            pr: 3,
            method: None,
        };
        let mut teammates = job("t1", "o/r");
        teammates.pr = Some(3);
        teammates.obi = Some(crate::job::ObiRef {
            correlation_id: "c".into(),
            from: "f".into(),
            from_owner: "ernesto".into(),
            request_id: "r".into(),
        });
        let mut owners = job("j1", "o/r");
        owners.published = Some("https://github.com/o/r/pull/3".into());
        let mut elsewhere = teammates.clone();
        elsewhere.id = "t2".into();
        elsewhere.pr = Some(4);
        assert_eq!(pick(vec![teammates.clone()], &asked).unwrap().id, "t1");
        assert_eq!(
            pick(vec![owners.clone(), teammates.clone()], &asked)
                .unwrap()
                .id,
            "t1"
        );
        assert_eq!(
            pick(vec![owners.clone(), elsewhere.clone()], &asked)
                .unwrap()
                .id,
            "j1"
        );
        assert!(pick(vec![elsewhere], &asked).is_none());
        // A job of the owner's with that number but no pull request of its own isn't it.
        let mut unpublished = job("j2", "o/r");
        unpublished.pr = Some(3);
        assert!(pick(vec![unpublished], &asked).is_none());
    }

    /// TASKCONV-7: the card names the base, the head and how it merges.
    #[test]
    fn taskconv_7_a_merge_card_pins_its_base_and_head() {
        let asked = Merge {
            repo: "o/r".into(),
            pr: 3,
            method: Some("rebase".into()),
        };
        let at = Open {
            head: "abcdef0123456789".into(),
            base: "main".into(),
        };
        assert_eq!(
            asked.title(&at, false),
            "Merge o/r#3 into main at abcdef0 (rebase)"
        );
        assert_eq!(
            asked.title(&at, true),
            "Merge o/r#3 into main at abcdef0 (its job's workflow)"
        );
    }

    /// TASKCONV-7: any base merges on the owner's word, main included; one that's closed or merged
    /// says so, and a method GitHub doesn't have is refused.
    #[test]
    fn taskconv_7_any_open_pull_request_merges_into_any_base() {
        let open = |state: &str, merged: bool| {
            json!({"state": state, "merged": merged, "head": {"sha": "abc"}, "base": {"ref": "main"}})
                .to_string()
        };
        assert_eq!(
            open_of("o/r", 3, &open("open", false)),
            Ok(Open {
                head: "abc".into(),
                base: "main".into()
            })
        );
        assert!(
            open_of("o/r", 3, &open("closed", true))
                .unwrap_err()
                .contains("merged already")
        );
        assert!(
            open_of("o/r", 3, &open("closed", false))
                .unwrap_err()
                .contains("closed")
        );
        let merge = |method: Option<&str>| Merge {
            repo: "o/r".into(),
            pr: 3,
            method: method.map(str::to_owned),
        };
        assert_eq!(merge(None).problem(), None);
        assert_eq!(merge(Some("rebase")).problem(), None);
        assert!(merge(Some("octopus")).problem().is_some());
    }

    /// TASKCONV-10: a default mode that asks less widens trust; one that asks as much or more, and
    /// every other setting, doesn't.
    #[test]
    fn taskconv_10_only_a_default_mode_that_asks_less_widens() {
        let mode = |m: &str| Settings {
            default_mode: Some(m.into()),
            ..Default::default()
        };
        let now = |mode: Mode| Now {
            mode,
            max_running: 6,
            account_failover: false,
        };
        assert!(mode("bypass").widens(&now(Mode::Ask)));
        assert!(!mode("ask").widens(&now(Mode::Bypass)));
        assert!(!mode("acceptEdits").widens(&now(Mode::AcceptEdits)));
        let set = |max: Option<u32>, failover: Option<bool>| Settings {
            max_running: max,
            account_failover: failover,
            ..Default::default()
        };
        // More jobs at once, or failover turned on, let Oby do more by itself.
        assert!(set(Some(8), None).widens(&now(Mode::Plan)));
        assert!(set(None, Some(true)).widens(&now(Mode::Plan)));
        // Fewer, as many, or failover off don't.
        assert!(!set(Some(3), Some(false)).widens(&now(Mode::Plan)));
        assert!(!set(Some(6), None).widens(&now(Mode::Plan)));
        let on = Now {
            account_failover: true,
            ..now(Mode::Plan)
        };
        assert!(!set(None, Some(true)).widens(&on));
        assert_eq!(
            set(Some(8), Some(true)).words(),
            "8 jobs at once, account failover on"
        );
        assert!(mode("yolo").mode().is_err());
    }
}
