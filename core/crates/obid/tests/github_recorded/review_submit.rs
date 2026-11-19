//! Review submit: the owner's Approve on a finished review's item (crate::review::post_review_with_gh,
//! MERGING-42 and 43) posts the review it shows on the pull request, with the owner's `gh`: who
//! opened it, its head still the commit Oby reviewed, then the review pinned to that commit.

use serde_json::json;

use crate::support::*;

/// How many calls obid had made once its own reads at start were in (the GitHub work feed, the
/// noreply email, the sign-in status, the repositories the login can push to): this path's calls
/// are those after it. Rule: those reads run on obid's own tasks, in no fixed order with each
/// other, and are other paths' recordings (github_work, noreply).
fn after_startup(recording: &crate::recorder::Recording) -> usize {
    within(30, "obid's own reads of GitHub at its start", || {
        let calls = recording.calls();
        let made = |prefix: &str| {
            calls.iter().any(|c| {
                let args: Vec<&str> = c["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap())
                    .collect();
                args.join(" ").starts_with(prefix)
            })
        };
        [
            "api graphql",
            "api user --jq",
            "auth status",
            "api user/repos",
        ]
        .iter()
        .all(|p| made(p))
        .then_some(calls.len())
    })
}

#[test]
fn github_recorded_review_submit() {
    let (folder, recording, config) =
        crate::recorder::recorded_config("review-submit", "review_submit");
    // The item a finished review of someone else's pull request raises (crate::jobs::
    // owner_review_item), kept as obid keeps it, so obid starts with it open.
    let inbox = folder.0.join("home/inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::copy(
        crate::recorder::kept("review_submit").join("item.json"),
        inbox.join("feed-review-r1b2c3d4.json"),
    )
    .unwrap();
    let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
    recording.started(&obid);

    let before = after_startup(&recording);
    let open = within(20, "the review's item", || {
        call(&obid, "GET", "/v1/inbox", None)
            .body
            .as_array()?
            .iter()
            .find(|i| i["id"] == "feed-review-r1b2c3d4")
            .cloned()
    });
    let approved = call_typed(
        &obid,
        "POST",
        "/v1/inbox/feed-review-r1b2c3d4/answer",
        &json!({"answer": "Approve"}),
    );
    assert_eq!(approved.status, 200, "{}", approved.text);

    let calls = recording.calls()[before..].to_vec();
    recording.assert_calls(&crate::recorder::polls_once(calls));
    let shown = |item: &serde_json::Value| {
        let watch = item["watch"].clone();
        json!({"id": item["id"], "kind": item["kind"], "title": item["title"], "text": item["text"],
               "options": item["options"], "watch": watch, "state": item["state"],
               "answer": item["answer"]})
    };
    recording.assert_answers(&json!([
        {"asked": "GET /v1/inbox, the review's item", "item": shown(&open)},
        {"asked": "POST /v1/inbox/{item}/answer Approve", "status": approved.status,
         "item": shown(&approved.body)},
    ]));
}
