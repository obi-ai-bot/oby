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

    /// What it does, in obid's words: a card's title.
    pub fn title(&self, base: &str) -> String {
        format!("Merge {}#{} into {base}", self.repo, self.pr)
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

/// Merges `asked` at `head`, the commit the owner asked about: a job's own pull request through
/// obid's merge path, as the owner's Merge on its card (`Jobs::merge_by_owner`: its record, its
/// branch, every rule that isn't only the owner's say-so); anyone else's with GitHub's merge,
/// pinned to `head`. What GitHub refuses (branch protection, a required review, a conflict) is
/// said as GitHub said it.
pub async fn merge(
    jobs: &Arc<Jobs>,
    home: &Path,
    asked: &Merge,
    head: &str,
) -> Result<String, String> {
    let name = format!("{}#{}", asked.repo, asked.pr);
    let job = jobs.list(usize::MAX).into_iter().find(|j| {
        j.obi.is_none()
            && j.repo == asked.repo
            && j.published
                .as_deref()
                .and_then(|url| github::pull_request_number(url, &j.repo))
                == Some(asked.pr)
    });
    if let Some(job) = job {
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
        tracing::info!("conversation: merged {name} at {head} ({method}), on the owner's word");
        Ok(format!("Merged {name} ({method})."))
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
            "approve" => (
                "POST",
                format!("repos/{repo}/pulls/{n}/reviews"),
                json!({"event": "APPROVE", "body": body.unwrap_or("")}),
            ),
            "request_changes" => (
                "POST",
                format!("repos/{repo}/pulls/{n}/reviews"),
                json!({"event": "REQUEST_CHANGES", "body": body.ok_or("request_changes needs a body: what to change.")?}),
            ),
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

    /// What it does, in obid's words: a card's title. Never the body, which is quoted.
    pub fn title(&self) -> String {
        let on = format!("{}#{}", self.repo, self.number.unwrap_or_default());
        match self.action.as_str() {
            "comment" => format!("Comment on {on}"),
            "approve" => format!("Approve {on} on GitHub"),
            "request_changes" => format!("Request changes on {on}"),
            "close" => format!("Close {on}"),
            "label" => format!("Label {on}"),
            _ => format!("Open an issue in {}", self.repo),
        }
    }
}

/// Writes `asked` on GitHub as the owner, once the secret scan passes what goes: its address on
/// GitHub, or why not (GitHub's own words when it refused).
pub async fn write(home: &Path, asked: &Write) -> Result<String, String> {
    let (method, path, body) = asked.request()?;
    let gh = github::gh().ok_or("GitHub's CLI (gh) isn't installed.")?;
    let what = asked.title();
    let answer = send(&gh, home, method, &path, &body)
        .await
        .map_err(|said| format!("{what} didn't happen: {said}"))?;
    tracing::info!("conversation: {what}, on the owner's word");
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

    /// It lets jobs do more without asking than they do now: a default mode that asks less.
    pub fn widens(&self, now: Mode) -> bool {
        self.mode()
            .ok()
            .flatten()
            .is_some_and(|m| m.reach() > now.reach())
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

    pub fn apply(&self, jobs: &Arc<Jobs>, home: &Path) -> Result<String, String> {
        let mode = self.mode()?;
        if self.max_running.is_none() && self.account_failover.is_none() && mode.is_none() {
            return Err("Say what to change: maxRunning, accountFailover or defaultMode.".into());
        }
        if let Some(max) = self.max_running {
            jobs.set_max_running(max).map_err(|e| match e {
                crate::jobs::SettingsError::Refused(why) => why,
                e => format!("It couldn't be kept: {e}"),
            })?;
        }
        if let Some(on) = self.account_failover {
            crate::settings::update(home, |s| s.account_failover = Some(on))
                .map_err(|e| format!("It couldn't be kept: {e}"))?;
        }
        if let Some(mode) = mode {
            jobs.set_default_mode(mode)
                .map_err(|e| format!("It couldn't be kept: {e}"))?;
        }
        Ok(format!("Saved: {}.", self.words()))
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
        assert!(write("delete", Some(7), None).request().is_err());
        assert_eq!(write("close", Some(7), None).title(), "Close o/r#7");
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
        assert_eq!(merge(None).title("main"), "Merge o/r#3 into main");
    }

    /// TASKCONV-10: a default mode that asks less widens trust; one that asks as much or more, and
    /// every other setting, doesn't.
    #[test]
    fn taskconv_10_only_a_default_mode_that_asks_less_widens() {
        let mode = |m: &str| Settings {
            default_mode: Some(m.into()),
            ..Default::default()
        };
        assert!(mode("bypass").widens(Mode::Ask));
        assert!(!mode("ask").widens(Mode::Bypass));
        assert!(!mode("acceptEdits").widens(Mode::AcceptEdits));
        let max = Settings {
            max_running: Some(8),
            account_failover: Some(true),
            ..Default::default()
        };
        assert!(!max.widens(Mode::Plan));
        assert_eq!(max.words(), "8 jobs at once, account failover on");
        assert!(mode("yolo").mode().is_err());
    }
}
