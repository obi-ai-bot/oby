//! The GitHub work feed (docs/runner-api.md, "obid only: GitHub work"): review requests, assigned
//! issues and the owner's own pull requests, read with one `gh api graphql`, listed by
//! `GET /v1/github/work`, read again on `POST /v1/github/work/refresh`, and new work offered once as
//! `github` inbox items.

use serde_json::json;

use crate::support::*;

#[test]
fn github_recorded_work_feed() {
    let (_folder, recording, obid, _) = crate::recorder::recorded_obid("work-feed", "work_feed");
    let mut answers = Vec::new();

    // obid's own read at its start ends first: the feed then has a next check.
    within(30, "obid's first read of GitHub work", || {
        let feed = call(&obid, "GET", "/v1/github/work", None);
        (!feed.body["nextCheck"].is_null()).then_some(())
    });
    let feed = call(&obid, "GET", "/v1/github/work", None);
    answers.push(json!({"asked": "GET /v1/github/work", "status": feed.status, "body": feed.body}));
    // The first read on record notes what's there and offers nothing.
    answers.push(json!({"asked": "POST /v1/github/work/refresh", "body": refresh(&obid)}));
    answers.push(json!({"asked": "github items, open", "body": github_items(&obid, "open")}));

    // New work: offered once, however often it's read.
    recording.raise("second");
    answers
        .push(json!({"asked": "POST /v1/github/work/refresh, new work", "body": refresh(&obid)}));
    answers.push(json!({"asked": "POST /v1/github/work/refresh, again", "body": refresh(&obid)}));
    // The quick model sorts an assigned issue (crate::triage): its answer, not GitHub's, so the
    // items are read once it has, or obid stopped waiting for it.
    let mut items = within(150, "the new items sorted", || {
        let items = github_items(&obid, "open");
        items
            .iter()
            .all(|i| i["triage"] != "pending")
            .then_some(items)
    });
    items.sort_by_key(|i| i["title"].as_str().unwrap_or_default().to_owned());
    answers.push(json!({"asked": "github items, open, new work", "body": items}));

    // The feed's own calls: obid's other loops read GitHub at its start too, at the same time (the
    // noreply email's `api user --jq …`, the sign-in's `auth status`), each its own path.
    recording.assert_calls(&recording.calls_starting(&[
        "api graphql",
        "api user --jq .login",
        "api user/orgs",
    ]));
    recording.assert_answers(&json!(answers));
}
