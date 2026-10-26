//! Approving on the code host (spec `protocol.md` section 11, "Approving on the code host";
//! docs/runner-api.md, "obid only: teammate approval"): where a repository merges only once another
//! person approved the change on GitHub, the owner's Obi asks a teammate's Obi for a review with
//! `approve_on_host`, and that Obi, when its review of the exact head approves, posts the approval
//! on GitHub as its own owner, with its owner's own `gh`.
//!
//! On the receiving side it does that only when its owner confirmed that one exchange, at that one
//! commit, with their own tap ([`item`], a `confirm` item signed by the owner's device), or, under
//! its owner's standing permission (a remembered decision of type [`HOST_APPROVAL`], kept only once
//! the owner confirmed it with their own tap), next to approvals of that commit by the people who
//! own every file it changes ([`code_host`]). Both go through `obi_protocol::host_approval::decide`,
//! the spec's rule. The request never grants it. The review it posts carries no word of the
//! request: fixed words and the commit, after the secret scan, and only while the pull request's
//! head is still the commit reviewed, which it names.

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

/// The words of the review obid posts: its own, the commit only, with Oby's marker
/// (`obi_protocol::host_approval::OBI_REVIEW_MARKER`), so no other Oby counts it as a person's.
pub fn words(sha: &str) -> String {
    format!(
        "Approved by Oby: its review of {} passed.",
        &sha[..sha.len().min(12)]
    )
}

/// The id of the item that asks the owner about exchange `cid`: one per exchange, and the device
/// signs it with the title, so a Confirm covers that exchange only.
pub fn item_id(cid: &str) -> String {
    format!("obi-approve-{cid}")
}

/// The `confirm` item that asks the owner whether to approve a teammate's pull request on GitHub
/// (no code owner's approval stands next to it). Its Confirm is the owner's own tap, signed by
/// their device over its id ([`item_id`], the exchange) and its title (the commit), within two
/// minutes and once (crate::approval): what `obi_protocol::host_approval::decide` takes as the
/// owner's confirmation. `job` is the review job's id. Cancel approves nothing.
pub fn item(
    cid: &str,
    from: &str,
    from_owner: &str,
    request: &Value,
    expires_at: &str,
    job: &str,
    now: i64,
) -> Item {
    let text = |key: &str| request[key].as_str().unwrap_or("").to_owned();
    let (repo, base, sha) = (text("repo"), text("base_branch"), text("head_sha"));
    let pr = request["pr"].as_u64().unwrap_or(0);
    let short = &sha[..sha.len().min(7)];
    let title =
        format!("Approve {from_owner}'s pull request #{pr} on {repo} at {short} on GitHub, as you");
    let mut item = inbox::confirm_item(
        "",
        title.clone(),
        format!(
            "Your Oby's review of {repo} pull request #{pr} at {short} passed. {from_owner}'s Obi ({from}) asks you to approve it on GitHub, as you, so it can merge. Your Confirm approves this commit only, for this request."
        ),
        inbox::Confirmed {
            action: inbox::action::APPROVE_ON_HOST.into(),
            target: item_id(cid),
            payload: json!({
                "correlationId": cid,
                "from": from,
                "fromOwner": from_owner,
                "repo": repo,
                "pr": pr,
                "baseBranch": base,
                "headSha": sha,
                "expiresAt": expires_at,
                "job": job,
            }),
        },
        now,
    );
    item.id = item_id(cid);
    item.details = Some(vec![
        format!("{title}."),
        "Don't approve it: nothing is posted on GitHub.".into(),
    ]);
    item
}

/// When the ask about approving on GitHub, `item`, lapses: its request's expiry. `None` for an
/// item that isn't one; `Some(None)` for one whose expiry doesn't read. Both the `confirm` item
/// ([`item`]) and the `obiRequest` one obid made before it.
pub fn ask_expiry(item: &Item) -> Option<Option<i64>> {
    let expires = |v: &Value| v["expiresAt"].as_str().and_then(obi_protocol::time::parse);
    if let Some(c) = item
        .confirm
        .as_ref()
        .filter(|c| c.action == inbox::action::APPROVE_ON_HOST)
    {
        return Some(expires(&c.payload));
    }
    item.obi
        .as_ref()
        .filter(|o| o["type"] == HOST_APPROVAL)
        .map(expires)
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
            "From now on, approve on GitHub the pull requests {owner}'s Oby asks you to on {place}, when your Oby's review passes and their code owners approved"
        ),
        format!(
            "Whenever {owner}'s Oby asks, Oby will approve that pull request on {place} on GitHub, as you, without asking you, once your Oby's own review of its exact commit passes, the pull request still goes where the review looked, and the people who own every file it changes (its CODEOWNERS) already approved that commit on GitHub. Otherwise Oby asks you each time. Forget it any time. It's kept only if you confirm."
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

/// Whether GitHub's pull request `number` of `repo` (`view`: `gh pr view --json
/// state,headRefOid,baseRefName,url`) is still the change obid reviewed: open, `repo`'s own (its
/// URL's repository), going into `base`, the branch the review compared it with, and its head still
/// `sha`. Why not, else.
pub fn as_reviewed(
    view: &Value,
    repo: &str,
    number: u64,
    base: &str,
    sha: &str,
) -> Result<(), String> {
    if view["state"] != "OPEN" {
        return Err("the pull request isn't open".into());
    }
    let url = format!("https://github.com/{repo}/pull/{number}");
    if !view["url"]
        .as_str()
        .is_some_and(|u| u.eq_ignore_ascii_case(&url))
    {
        return Err(format!(
            "GitHub doesn't show it as {repo}'s pull request #{number}"
        ));
    }
    if view["baseRefName"] != base {
        return Err(format!(
            "it goes into {}, not {base}, the branch the review compared it with",
            view["baseRefName"].as_str().unwrap_or("another branch")
        ));
    }
    if view["headRefOid"] != sha {
        return Err("the pull request moved on since the review".into());
    }
    Ok(())
}

/// Posts an approving review of `sha` on pull request `number` of `repo` with `gh` as the owner
/// (crate::review::create_review, pinned to `sha` by `commit_id`): only while the pull request is
/// still the change reviewed ([`as_reviewed`], into `base`), and when the owner's login can push to
/// `repo` (an approval without that wouldn't count). The words pass the secret scan first and are
/// written in `scratch`. `still` says, right before GitHub is asked, whether the approval may still
/// go (obid in its Sphere, the request unexpired). The review posted, or why it wasn't.
pub async fn post(
    gh: &Path,
    scratch: &Path,
    repo: &str,
    number: u64,
    base: &str,
    sha: &str,
    still: impl Fn() -> bool,
) -> Result<Posted, String> {
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
    still_as_reviewed(gh, repo, number, base, sha).await?;
    if !crate::github::can_push(gh, None, repo).await? {
        return Err(format!(
            "your GitHub login can't push to {repo}, so its approval wouldn't count"
        ));
    }
    let file = scratch.join("review.md");
    crate::store::write_private(&file, words.as_bytes())
        .map_err(|e| format!("obid couldn't write the review: {e}"))?;
    if !still() {
        return Err(
            "the request expired, or this Oby left its Obysphere, before GitHub was asked".into(),
        );
    }
    let text = crate::review::create_review(gh, None, repo, number, &file, "APPROVE", sha)
        .await
        .map_err(|(_, why)| why)?;
    let posted: Value = serde_json::from_str(&text).unwrap_or_default();
    Ok(Posted {
        url: posted["html_url"]
            .as_str()
            .filter(|u| u.starts_with("https://"))
            .map_or_else(
                || format!("https://github.com/{repo}/pull/{number}"),
                str::to_owned,
            ),
        id: posted["id"].as_u64(),
    })
}

/// An approving review obid posted: its URL, and its id, to dismiss it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posted {
    pub url: String,
    pub id: Option<u64>,
}

/// Whether pull request `number` of `repo` is, on GitHub now, still the change reviewed
/// ([`as_reviewed`]): read with the owner's `gh`.
pub async fn still_as_reviewed(
    gh: &Path,
    repo: &str,
    number: u64,
    base: &str,
    sha: &str,
) -> Result<(), String> {
    let n = number.to_string();
    let view = crate::github::run(
        gh,
        &[
            "pr",
            "view",
            &n,
            "-R",
            repo,
            "--json",
            "state,headRefOid,baseRefName,url",
        ],
    )
    .await?;
    let view: Value = serde_json::from_str(&view).map_err(|_| "gh's answer wasn't JSON")?;
    as_reviewed(&view, repo, number, base, sha)
}

/// The words obid dismisses its own approval with: its own, carrying Oby's marker.
pub const DISMISSAL: &str = "Dismissed by Oby: the pull request changed while Oby approved it, so this approval no longer stands.";

/// Dismisses obid's own approving review `id` on pull request `number` of `repo`, as the owner.
pub async fn dismiss(gh: &Path, repo: &str, number: u64, id: u64) -> Result<(), String> {
    if !crate::github::is_repo(repo) {
        return Err(format!("{repo} isn't a repository name"));
    }
    let path = format!("repos/{repo}/pulls/{number}/reviews/{id}/dismissals");
    let message = format!("message={DISMISSAL}");
    crate::github::run(
        gh,
        &[
            "api",
            "-X",
            "PUT",
            &path,
            "-f",
            &message,
            "-f",
            "event=DISMISS",
        ],
    )
    .await
    .map(|_| ())
}

/// Where GitHub keeps a repository's code owners, in the order it looks.
pub const CODEOWNERS_PATHS: [&str; 3] = [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];

/// The values of `gh api --paginate`'s answer, the pages' arrays run together: each page is one
/// JSON array, written one after another.
fn pages(text: &str) -> Result<Vec<Value>, String> {
    let mut items = Vec::new();
    for page in serde_json::Deserializer::from_str(text).into_iter::<Value>() {
        match page.map_err(|_| "gh's answer wasn't JSON")? {
            Value::Array(page) => items.extend(page),
            _ => return Err("gh's answer wasn't a list".into()),
        }
    }
    Ok(items)
}

/// What GitHub shows, with the owner's own `gh`, about whether the people who own every file of
/// pull request `number` of `repo` approved `head` (spec `protocol.md` section 11, "Who counts as
/// a code owner"): the `CODEOWNERS` file on `base`, every changed path (a renamed file's old one
/// too) and whether that's all of them, the reviews in order as GitHub lists them, and, for the
/// people whose approval may count, their access to `repo` and the teams the file names that
/// they're in. With the owner's own login, which counts for nobody. Read raw, so what counts is
/// decided by `obi_protocol::host_approval` alone. Why it couldn't read it, else: then no code
/// owner's approval counts.
pub async fn code_host(
    gh: &Path,
    repo: &str,
    number: u64,
    base: &str,
    head: &str,
) -> Result<(obi_protocol::host_approval::CodeHost, String), String> {
    use obi_protocol::host_approval::{CodeHost, Review, counting_approvers, parse_codeowners};
    if !crate::github::is_repo(repo) {
        return Err(format!("{repo} isn't a repository name"));
    }
    let own = crate::github::run(gh, &["api", "user", "--jq", ".login"]).await?;
    let mut host = CodeHost::default();
    for path in CODEOWNERS_PATHS {
        let at = format!("repos/{repo}/contents/{path}");
        let reference = format!("ref={base}");
        let read = [
            "api",
            &at,
            "--method",
            "GET",
            "-f",
            &reference,
            "-H",
            "Accept: application/vnd.github.raw",
        ];
        match crate::github::run(gh, &read).await {
            Ok(text) => {
                host.codeowners = Some(text);
                break;
            }
            Err(why) if why.contains("HTTP 404") => {}
            Err(why) => return Err(format!("GitHub didn't show {path}: {why}")),
        }
    }
    let Some(rules) = host.codeowners.as_deref().map(parse_codeowners) else {
        return Ok((host, own));
    };
    let pr = format!("repos/{repo}/pulls/{number}");
    let files = format!("{pr}/files");
    let files = pages(&crate::github::run(gh, &["api", &files, "--paginate"]).await?)?;
    for file in &files {
        for key in ["filename", "previous_filename"] {
            if let Some(path) = file[key].as_str() {
                host.changed_files.push(path.to_owned());
            }
        }
    }
    let pull: Value = serde_json::from_str(&crate::github::run(gh, &["api", &pr]).await?)
        .map_err(|_| "gh's answer wasn't JSON")?;
    host.files_complete = pull["changed_files"]
        .as_u64()
        .is_some_and(|n| n == files.len() as u64);
    let reviews = format!("{pr}/reviews");
    for review in pages(&crate::github::run(gh, &["api", &reviews, "--paginate"]).await?)? {
        // A review by an account since deleted has no user: it counts for nobody.
        let Some(login) = review["user"]["login"].as_str() else {
            continue;
        };
        let text = |v: &Value| v.as_str().unwrap_or("").to_owned();
        host.reviews.push(Review {
            login: login.to_owned(),
            account_type: text(&review["user"]["type"]),
            body: text(&review["body"]),
            state: text(&review["state"]),
            commit_id: text(&review["commit_id"]),
        });
    }
    // The people whose approval may count, as if each could write: their access and teams are
    // asked for them alone.
    let anyone = host
        .reviews
        .iter()
        .map(|r| (r.login.clone(), "write".to_owned()))
        .collect();
    let candidates = counting_approvers(&host.reviews, head, &own, &anyone);
    let named = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    for login in candidates.iter().filter(|l| named(l)) {
        let path = format!("repos/{repo}/collaborators/{login}/permission");
        // GitHub answers 404 for someone who isn't a collaborator: no access.
        if let Ok(said) = crate::github::run(gh, &["api", &path]).await {
            let said: Value = serde_json::from_str(&said).unwrap_or_default();
            if let Some(access) = said["permission"].as_str() {
                host.permissions.insert(login.clone(), access.to_owned());
            }
        }
    }
    let mut teams: Vec<&str> = rules
        .iter()
        .flatten()
        .flat_map(|r| r.owners.iter())
        .filter_map(|o| o.strip_prefix('@'))
        .filter(|o| o.contains('/'))
        .collect();
    teams.sort_unstable();
    teams.dedup();
    for team in teams {
        let Some((org, slug)) = team.split_once('/').filter(|(o, t)| named(o) && named(t)) else {
            continue;
        };
        for login in candidates.iter().filter(|l| named(l)) {
            let path = format!("orgs/{org}/teams/{slug}/memberships/{login}");
            let said = crate::github::run(gh, &["api", &path]).await;
            let state = said
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .and_then(|v| v["state"].as_str().map(str::to_owned));
            if state.as_deref() == Some("active") {
                host.teams
                    .entry(team.to_owned())
                    .or_default()
                    .push(login.clone());
            }
        }
    }
    Ok((host, own))
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

/// How long a request for an approval waits for its answer: then the next teammate is asked (a
/// teammate whose Oby is away doesn't hold the merge for long).
pub const REQUEST_LIFETIME: i64 = 7_200;

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

/// What the owner can do when no teammate's Oby can approve a pull request on `repo`: who to
/// invite (to an Obysphere with them when `no_sphere`), or get that approval on GitHub.
pub fn instead(repo: &str, no_sphere: bool) -> String {
    let to = if no_sphere {
        "to an Obysphere with you"
    } else {
        "to your Obysphere"
    };
    format!(
        "Invite a teammate who can push to {repo} {to}, or get their approval on GitHub yourself."
    )
}

/// The `review_request` body that asks for `change`'s approval: its checks pass on the teammate's
/// Mac and no blocker findings, at its head.
pub fn request_body(change: &Change) -> Value {
    json!({
        "goal": goal(change.goal, &format!("Review pull request #{}", change.pr)),
        "repo": change.repo,
        "pr": change.pr,
        "base_branch": change.base,
        "head_branch": change.head_branch,
        "head_sha": change.head,
        "acceptance": [{"kind": "tests_pass"}, {"kind": "no_blocker_findings"}],
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
/// approval on `repo`, best first: those whose Obi said it posted one there before (`exchanges`),
/// then the rest, each by Obi id; never one in `asked`. The Sphere's directory names no GitHub
/// login, so who can push there is known only from what a teammate's Obi answers.
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
    let mut ranked: Vec<(bool, &str)> = members
        .iter()
        .filter(|(obi, _)| !asked.contains(obi))
        .map(|(obi, _)| (!approved_before(obi), obi.as_str()))
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
    fn every_word_obid_posts_on_github_carries_obys_marker() {
        use obi_protocol::host_approval::OBI_REVIEW_MARKER;
        for posted in [
            words(SHA),
            DISMISSAL.to_owned(),
            crate::review::SIGNED.to_owned(),
            crate::merge::APPROVAL.to_owned(),
        ] {
            assert!(
                posted.to_lowercase().contains(OBI_REVIEW_MARKER),
                "{posted}"
            );
        }
        assert_eq!(crate::github_work::OBY_SIGNS, OBI_REVIEW_MARKER);
    }

    #[test]
    fn paginated_answers_run_together() {
        assert_eq!(
            pages("[{\"a\":1}][{\"a\":2}]\n[]").unwrap(),
            [json!({"a": 1}), json!({"a": 2})]
        );
        assert!(pages("{\"a\":1}").is_err());
        assert!(pages("[1").is_err());
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

    #[test]
    fn only_the_change_reviewed_is_approved() {
        let view = json!({"state": "OPEN", "headRefOid": SHA, "baseRefName": "main",
                          "url": "https://github.com/Acme/App/pull/12"});
        assert_eq!(as_reviewed(&view, "acme/app", 12, "main", SHA), Ok(()));
        let with = |key: &str, value: Value| {
            let mut changed = view.clone();
            changed[key] = value;
            as_reviewed(&changed, "acme/app", 12, "main", SHA).unwrap_err()
        };
        assert_eq!(
            with("baseRefName", json!("staging")),
            "it goes into staging, not main, the branch the review compared it with"
        );
        assert_eq!(
            with("url", json!("https://github.com/evil/app/pull/12")),
            "GitHub doesn't show it as acme/app's pull request #12"
        );
        assert_eq!(
            with("headRefOid", json!("1".repeat(40))),
            "the pull request moved on since the review"
        );
        assert_eq!(
            with("state", json!("MERGED")),
            "the pull request isn't open"
        );
    }
}
