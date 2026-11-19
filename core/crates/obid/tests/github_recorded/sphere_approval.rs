//! A teammate's pull request approved on GitHub through the Obysphere (crate::sphere::approval):
//! a teammate's Oby asks for a review with `approve_on_host`, the review passes, obid asks its
//! owner, and the owner's own signed Confirm posts the approval with the owner's `gh`.

use serde_json::{Value, json};

use crate::support::*;

/// The open inbox item `id`, once it is there.
fn item(obid: &Obid, id: &str) -> Value {
    within(60, id, || {
        call(obid, "GET", "/v1/inbox", None)
            .body
            .as_array()?
            .iter()
            .find(|i| i["id"] == id || (id == "obiRequest" && i["kind"] == "obiRequest"))
            .cloned()
    })
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn github_recorded_sphere_approval() {
    let (folder, recording, mut config) =
        crate::recorder::recorded_config("sphere-approval", "sphere_approval");
    let remote = fixture(&folder.0.join("fixture"));
    config["repos"][0]["remote"] = format!("file://{}", remote.display()).into();
    let sphere = FakeSphere::start().await;
    let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
    recording.started(&obid);
    let owner = Owner::new("o-owner");
    let joined = tokio::task::block_in_place(|| join_a(&obid, &sphere, &owner));
    assert_eq!(joined["state"], "joined", "{joined}");
    let me = joined["obiId"].as_str().unwrap().to_owned();
    let teammate = TestObi::join(&sphere, "obi-o-teammate", "o-teammate").await;
    let before = tokio::task::block_in_place(|| after_startup(&recording));

    // The teammate's Oby asks for a review of pull request #12 that approves it on GitHub.
    let mut asking = brief();
    asking["pr"] = json!(12);
    asking["approve_on_host"] = json!(true);
    let request = teammate.request(&me, "review_request", asking, 3600);
    let cid = request["id"].as_str().unwrap().to_owned();
    teammate.send(&request).await;
    let accepted = tokio::task::block_in_place(|| {
        let asked = item(&obid, "obiRequest");
        let id = asked["id"].as_str().unwrap();
        call_typed(
            &obid,
            "POST",
            &format!("/v1/inbox/{id}/answer"),
            &json!({"answer": "Accept"}),
        )
    });
    assert_eq!(accepted.status, 200, "{}", accepted.text);
    let replies = teammate.receive(2, 90).await;
    let result = replies
        .iter()
        .find(|r| !r["body"]["host_review"].is_null())
        .unwrap_or_else(|| panic!("no review result: {replies:?}"))
        .clone();
    assert_eq!(result["body"]["host_review"], "asked", "{replies:?}");

    // The ask, and the owner's own tap on it.
    let ask = tokio::task::block_in_place(|| item(&obid, &format!("obi-approve-{cid}")));
    let ask_id = ask["id"].as_str().unwrap().to_owned();
    let confirmed = tokio::task::block_in_place(|| {
        let tap = confirm_approval(&obid, &ask_id);
        call(
            &obid,
            "POST",
            &format!("/v1/inbox/{ask_id}/answer"),
            Some(&json!({"answer": "Confirm", "approval": tap})),
        )
    });
    assert_eq!(confirmed.status, 200, "{}", confirmed.text);
    let job = tokio::task::block_in_place(|| {
        let id = call(
            &obid,
            "GET",
            &format!("/v1/obid/sphere/exchanges/{cid}"),
            None,
        )
        .body["job"]
            .as_str()
            .unwrap()
            .to_owned();
        call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body
    });

    let job_id = job["id"].as_str().unwrap().to_owned();
    // This path's own rules, on top of recorder.rs's:
    // Rule: the review job's id is a fresh random one (8 hex), different on every run.
    let local = |value: Value| -> Value {
        let text = value
            .to_string()
            .replace(&format!("\"{job_id}\""), "\"<job>\"");
        serde_json::from_str(&text).unwrap()
    };
    let calls: Vec<Value> = recording.calls()[before..]
        .iter()
        .cloned()
        .map(&local)
        .collect();
    recording.assert_calls(&crate::recorder::polls_once(calls));
    let card = |item: &Value| {
        json!({"id": item["id"], "kind": item["kind"], "title": item["title"], "text": item["text"],
               "options": item["options"], "confirm": item["confirm"], "state": item["state"],
               "answer": item["answer"]})
    };
    let outgoing = job["outgoing"].clone();
    let answers = json!([
        {"asked": "the ask, open", "item": card(&ask)},
        {"asked": "POST /v1/inbox/{ask}/answer Confirm", "status": confirmed.status,
         "item": card(&confirmed.body)},
        {"asked": "GET /v1/jobs/{review job}", "outgoing": outgoing,
         "pullRequest": job["pullRequest"], "published": job["published"],
         "issue": job["issue"], "issueUrl": job["issueUrl"]},
        {"asked": "the teammate's Oby's reply", "host_review": result["body"]["host_review"]},
    ]);
    recording.assert_answers(&local(answers));
}
