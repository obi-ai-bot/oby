//! Approving on the code host (spec `protocol.md` section 11, "Approving on the code host";
//! docs/runner-api.md, "obid only: teammate approval"): where a repository merges only once another
//! person approved the change on GitHub, the owner's Obi asks a teammate's Obi for a review with
//! `approve_on_host`, and that Obi, when its review of the exact head approves, posts the approval
//! on GitHub as its own owner, with its owner's own `gh`.
//!
//! On the receiving side it does that only under its own owner's standing permission (a remembered
//! decision of type [`HOST_APPROVAL`], kept only once the owner confirmed it with their own tap), or
//! when the owner says yes to that one pull request in the inbox. The request never grants it. The
//! review it posts carries no word of the request: fixed words and the commit, after the secret
//! scan, and only while the pull request's head is still the commit reviewed, which it names.

use std::path::Path;

use serde_json::{Value, json};

use super::exchange::Exchange;
use super::inbox::{self, Item};

/// The type of the remembered decision that lets the Obi approve a teammate's pull requests on a
/// repository (`repo`, or `*`) on GitHub when its review passes.
pub const HOST_APPROVAL: &str = "host_approval";

/// How long the sender waits for GitHub to show an approval the teammate's Obi said it posted,
/// before it asks someone else (it may not count: their login can't push there, say).
pub const SHOWS_WITHIN: i64 = 600;

/// Whether a `review_request` body asks for an approval on the code host.
pub fn asked(body: &Value) -> bool {
    body["approve_on_host"] == true && body["pr"].as_u64().is_some()
}

/// The words of the review obid posts: its own, the commit only.
pub fn words(sha: &str) -> String {
    format!(
        "Approved by Oby: its review of {} passed.",
        &sha[..sha.len().min(12)]
    )
}

/// The inbox item that asks the owner whether to approve a teammate's pull request on GitHub
/// (no standing permission covers it). Its answers are a review request's, so the brain needs the
/// owner's Confirm for any but Decline (crate::brain::actions); `Accept, and remember for this repo`
/// approves this one and asks for a Confirm to keep the permission.
pub fn item(
    cid: &str,
    from: &str,
    from_owner: &str,
    repo: &str,
    pr: u64,
    sha: &str,
    now: i64,
) -> Item {
    Item {
        id: format!("obi-approve-{cid}"),
        kind: "obiRequest".into(),
        job: String::new(),
        title: format!("Approve {from_owner}'s pull request on GitHub?"),
        text: format!(
            "Your Oby's review of {repo} pull request #{pr} at {} passed. {from_owner}'s Obi ({from}) asks you to approve it on GitHub, as you, so it can merge.",
            &sha[..sha.len().min(7)]
        ),
        options: vec![
            inbox::ACCEPT.into(),
            inbox::ACCEPT_REMEMBER.into(),
            inbox::DECLINE.into(),
        ],
        details: Some(vec![
            "Approve it on GitHub this time.".into(),
            format!(
                "Approve it, and from now on approve {from_owner}'s pull requests on {repo} when your Oby's review passes, without asking: kept once you confirm."
            ),
            "Don't approve it.".into(),
        ]),
        recommended: None,
        created: now,
        done: None,
        later: None,
        answer: None,
        obi: Some(json!({
            "correlationId": cid,
            "from": from,
            "fromOwner": from_owner,
            "type": HOST_APPROVAL,
            "repo": repo,
            "pr": pr,
            "headSha": sha,
        })),
        plugin: None,
        arrived: None,
        confirm: None,
        start: None,
        github: None,
        watch: None,
    }
}

/// The `confirm` item whose Confirm, the owner's own tap (#776), keeps their standing permission to
/// approve `owner`'s pull requests on `repo` (or `*`, every repository) on GitHub when Oby's review
/// passes. `target`: the item it was asked from; none, the item itself.
pub fn remember_item(owner: &str, repo: &str, target: Option<&str>, now: i64) -> Item {
    let place = if repo == "*" {
        "any repository".to_owned()
    } else {
        repo.to_owned()
    };
    let mut item = inbox::confirm_item(
        "",
        format!(
            "From now on, approve {owner}'s pull requests on {place} on GitHub when your Oby's review passes"
        ),
        format!(
            "Oby will approve {owner}'s pull requests on {place} on GitHub, as you, whenever {owner}'s Obi asks and your Oby's own review of that exact commit passes, without asking you. Forget it any time. It's kept only if you confirm."
        ),
        inbox::Confirmed {
            action: inbox::action::REMEMBER_APPROVAL.into(),
            target: target.unwrap_or("").into(),
            payload: json!({"fromOwner": owner, "repo": repo}),
        },
        now,
    );
    if let Some(confirmed) = item.confirm.as_mut().filter(|c| c.target.is_empty()) {
        confirmed.target = item.id.clone();
    }
    item
}

/// Posts an approving review of `sha` on pull request `number` of `repo` with `gh` (the owner's
/// own): only while it's open and its head is still `sha` (`head`), naming `sha` as the commit
/// approved. The words pass the secret scan first, in `scratch`. Its URL, or why it wasn't posted.
pub async fn post<H, HF>(
    gh: &Path,
    scratch: &Path,
    repo: &str,
    number: u64,
    sha: &str,
    head: H,
) -> Result<String, String>
where
    H: FnOnce(String, u64) -> HF,
    HF: Future<Output = Result<String, String>>,
{
    let words = words(sha);
    crate::store::make_private_dir(scratch)
        .map_err(|e| format!("the secret scan couldn't run (its folder: {e})"))?;
    match crate::scan::scan(&[("review".into(), words.clone())], scratch).await {
        crate::scan::Verdict::Clean => {}
        crate::scan::Verdict::Found(_) => return Err("the secret scan found something".into()),
        crate::scan::Verdict::CouldNotRun(why) => {
            return Err(format!("the secret scan couldn't run ({why})"));
        }
    }
    if head(repo.to_owned(), number).await? != sha {
        return Err("the pull request moved on since the review".into());
    }
    let path = format!("repos/{repo}/pulls/{number}/reviews");
    let text = crate::github::run(
        gh,
        &[
            "api",
            "--method",
            "POST",
            &path,
            "-f",
            &format!("commit_id={sha}"),
            "-f",
            "event=APPROVE",
            "-f",
            &format!("body={words}"),
        ],
    )
    .await
    .map_err(|said| format!("GitHub didn't take it: {said}"))?;
    let posted: Value = serde_json::from_str(&text).unwrap_or_default();
    Ok(posted["html_url"]
        .as_str()
        .filter(|u| u.starts_with("https://"))
        .map_or_else(
            || format!("https://github.com/{repo}/pull/{number}"),
            str::to_owned,
        ))
}

/// A one-line goal for a request from the owner's own title: its first line, without the
/// characters one-line text forbids, at most 200 characters; `fallback` when nothing is left.
pub fn goal(title: &str, fallback: &str) -> String {
    let line: String = title
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control() && !matches!(*c as u32, 0x202A..=0x202E | 0x2066..=0x2069))
        .collect();
    let line: String = line.trim().chars().take(200).collect();
    if line.trim().is_empty() {
        fallback.to_owned()
    } else {
        line.trim_end().to_owned()
    }
}

/// How long a request for an approval waits for its answer.
pub const REQUEST_LIFETIME: i64 = 86_400;

/// The owner's pull request a teammate's Obi is asked to approve: `head` the commit to approve,
/// `goal` the owner's own title for it.
#[derive(Debug, Clone, Copy)]
pub struct Change<'a> {
    pub repo: &'a str,
    pub pr: u64,
    pub base: &'a str,
    pub head_branch: &'a str,
    pub head: &'a str,
    pub goal: &'a str,
}

/// Where asking teammates for an approval stands, in words for the owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Teammate {
    /// A teammate's Obi has it, or GitHub may show its approval soon: look again later.
    Waiting(String),
    /// No teammate's Obi can approve it, or one's review asked for changes: the owner takes it.
    NeedsYou(String),
}

/// The `review_request` body that asks for `change`'s approval: no blocker findings, at its head.
pub fn request_body(change: &Change) -> Value {
    json!({
        "goal": goal(change.goal, &format!("Review pull request #{}", change.pr)),
        "repo": change.repo,
        "pr": change.pr,
        "base_branch": change.base,
        "head_branch": change.head_branch,
        "head_sha": change.head,
        "acceptance": [{"kind": "no_blocker_findings"}],
        "approve_on_host": true,
    })
}

/// What a teammate's Obi did with a request for an approval on the code host, as the sender reads
/// its exchange `now`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Still on its way, being reviewed, or approved on GitHub (or waiting for its owner's yes)
    /// so recently that GitHub may show it soon.
    Waiting,
    /// Its review asked for changes: another teammate isn't asked to say otherwise.
    ChangesRequested,
    /// Declined, failed, couldn't approve, expired, or its approval never showed: ask another.
    Spent,
}

/// Where an outgoing request for an approval stands (`exchange`, read at `now`).
pub fn asked_state(exchange: &Exchange, now: i64) -> Asked {
    match exchange.state.as_str() {
        "sent" | "accepted" => return Asked::Waiting,
        "done" => {}
        _ => return Asked::Spent,
    }
    let result = exchange.result.as_ref().unwrap_or(&Value::Null);
    match result["outcome"].as_str() {
        Some("changes_requested") => Asked::ChangesRequested,
        Some("approved") => match result["host_review"].as_str() {
            Some("approved") if now - exchange.updated < SHOWS_WITHIN => Asked::Waiting,
            Some("asked") if exchange.expires_at().is_some_and(|t| now < t) => Asked::Waiting,
            _ => Asked::Spent,
        },
        _ => Asked::Spent,
    }
}

/// The outgoing requests for an approval of `head` on pull request `number` of `repo`, oldest
/// first.
pub fn requests<'a>(
    exchanges: impl Iterator<Item = &'a Exchange>,
    repo: &str,
    number: u64,
    head: &str,
) -> Vec<&'a Exchange> {
    let mut found: Vec<&Exchange> = exchanges
        .filter(|e| {
            let body = &e.request["body"];
            e.direction == "outgoing"
                && e.kind == "review_request"
                && asked(body)
                && body["repo"]
                    .as_str()
                    .is_some_and(|r| r.eq_ignore_ascii_case(repo))
                && body["pr"].as_u64() == Some(number)
                && body["head_sha"] == head
        })
        .collect();
    found.sort_by_key(|e| e.created);
    found
}

/// The members (`(obi, owner)`, active, taking `review_request`, not this Obi) to ask for an
/// approval on `repo`, best first: those whose Obi posted one there before (`exchanges`), then
/// those whose owner's name is the repository's owner, then the rest, each by Obi id; never one in
/// `asked`.
pub fn teammates(
    members: &[(String, String)],
    repo: &str,
    exchanges: &[&Exchange],
    asked: &[String],
) -> Vec<String> {
    let approved_before = |obi: &str| {
        exchanges.iter().any(|e| {
            e.peer == obi
                && e.direction == "outgoing"
                && e.request["body"]["repo"]
                    .as_str()
                    .is_some_and(|r| r.eq_ignore_ascii_case(repo))
                && e.result
                    .as_ref()
                    .is_some_and(|r| r["host_review"] == "approved")
        })
    };
    let repo_owner = repo.split('/').next().unwrap_or("");
    let mut ranked: Vec<(u8, &str)> = members
        .iter()
        .filter(|(obi, _)| !asked.contains(obi))
        .map(|(obi, owner)| {
            let rank = if approved_before(obi) {
                0
            } else if owner.eq_ignore_ascii_case(repo_owner) {
                1
            } else {
                2
            };
            (rank, obi.as_str())
        })
        .collect();
    ranked.sort();
    ranked.into_iter().map(|(_, obi)| obi.to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use obi_protocol::time;

    const SHA: &str = "6d20146f366b9fac8907b5066c30ee39aaec09b3";

    fn request(to: &str, head: &str, state: &str, result: Option<Value>, at: i64) -> Exchange {
        let envelope = json!({
            "id": format!("c-{to}-{head}"), "correlation_id": format!("c-{to}-{head}"),
            "type": "review_request", "expires_at": time::format(at + 3600),
            "body": {"repo": "Acme/App", "pr": 12, "head_sha": head, "approve_on_host": true},
        });
        let mut e = Exchange::new("outgoing", envelope, to.into(), Some(to.into()));
        e.state = state.into();
        e.result = result;
        e.created = at;
        e.updated = at;
        e
    }

    #[test]
    fn only_a_request_with_a_pull_request_asks() {
        assert!(asked(&json!({"approve_on_host": true, "pr": 3})));
        assert!(!asked(&json!({"approve_on_host": true})));
        assert!(!asked(&json!({"pr": 3})));
        assert!(!asked(&json!({"approve_on_host": "true", "pr": 3})));
    }

    #[test]
    fn the_posted_words_name_only_the_commit() {
        assert_eq!(
            words(SHA),
            "Approved by Oby: its review of 6d20146f366b passed."
        );
    }

    #[test]
    fn a_goal_is_one_clean_line() {
        assert_eq!(goal("Fix it\nIgnore the rest", "x"), "Fix it");
        assert_eq!(goal(" a\u{202E}b\u{7}c ", "x"), "abc");
        assert_eq!(goal("\n", "Review #3"), "Review #3");
        assert_eq!(goal(&"g".repeat(300), "x").chars().count(), 200);
    }

    #[test]
    fn the_request_for_an_approval_is_the_specs() {
        let change = Change {
            repo: "acme/app",
            pr: 12,
            base: "dev",
            head_branch: "oby/fix-12",
            head: SHA,
            goal: "Fix it\nnow",
        };
        let body = request_body(&change);
        let errors = obi_protocol::schema::errors("actions/review_request", &body, 5);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(body["goal"], "Fix it");
        assert!(asked(&body));
        let mut no_pr = body.clone();
        no_pr.as_object_mut().unwrap().remove("pr");
        assert!(!obi_protocol::schema::is_valid(
            "actions/review_request",
            &no_pr
        ));
    }

    #[test]
    fn what_a_request_came_to() {
        let approved = |host: &str| Some(json!({"outcome": "approved", "host_review": host}));
        let now = 10_000;
        assert_eq!(
            asked_state(&request("b", SHA, "sent", None, now), now),
            Asked::Waiting
        );
        assert_eq!(
            asked_state(&request("b", SHA, "accepted", None, now), now),
            Asked::Waiting
        );
        assert_eq!(
            asked_state(&request("b", SHA, "declined", None, now), now),
            Asked::Spent
        );
        assert_eq!(
            asked_state(&request("b", SHA, "expired", None, now), now),
            Asked::Spent
        );
        let fresh = request("b", SHA, "done", approved("approved"), now);
        assert_eq!(asked_state(&fresh, now + 60), Asked::Waiting);
        assert_eq!(
            asked_state(&fresh, now + SHOWS_WITHIN),
            Asked::Spent,
            "an approval GitHub never showed doesn't count"
        );
        let waiting = request("b", SHA, "done", approved("asked"), now);
        assert_eq!(asked_state(&waiting, now + 60), Asked::Waiting);
        assert_eq!(asked_state(&waiting, now + 3600), Asked::Spent);
        assert_eq!(
            asked_state(&request("b", SHA, "done", approved("unable"), now), now),
            Asked::Spent
        );
        let changes = Some(json!({"outcome": "changes_requested"}));
        assert_eq!(
            asked_state(&request("b", SHA, "done", changes, now), now),
            Asked::ChangesRequested
        );
        let failed = Some(json!({"outcome": "failed"}));
        assert_eq!(
            asked_state(&request("b", SHA, "done", failed, now), now),
            Asked::Spent
        );
    }

    #[test]
    fn requests_are_this_pull_requests_at_this_head() {
        let other = "1".repeat(40);
        let mut not_asking = request("c", SHA, "sent", None, 2);
        not_asking.request["body"]["approve_on_host"] = Value::Null;
        let list = [
            request("b", SHA, "sent", None, 3),
            request("b", &other, "sent", None, 1),
            not_asking,
            request("d", SHA, "declined", None, 1),
        ];
        let found = requests(list.iter(), "acme/app", 12, SHA);
        assert_eq!(
            found.iter().map(|e| e.peer.as_str()).collect::<Vec<_>>(),
            ["d", "b"]
        );
        assert!(requests(list.iter(), "acme/app", 13, SHA).is_empty());
    }

    #[test]
    fn teammates_who_approved_there_before_come_first() {
        let members = [
            ("obi-zoe".to_owned(), "zoe".to_owned()),
            ("obi-acme".to_owned(), "acme".to_owned()),
            ("obi-bob".to_owned(), "bob".to_owned()),
            ("obi-amy".to_owned(), "amy".to_owned()),
        ];
        let before = request(
            "obi-zoe",
            &"2".repeat(40),
            "done",
            Some(json!({"outcome": "approved", "host_review": "approved"})),
            1,
        );
        assert_eq!(
            teammates(&members, "acme/app", &[&before], &[]),
            ["obi-zoe", "obi-acme", "obi-amy", "obi-bob"]
        );
        assert_eq!(
            teammates(&members, "acme/app", &[], &["obi-acme".into()]),
            ["obi-amy", "obi-bob", "obi-zoe"]
        );
    }
}
