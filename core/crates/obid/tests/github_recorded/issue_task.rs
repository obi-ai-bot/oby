//! Starting a task from an issue (docs/contracts/issue-tasks.md, ISSUETASK): `POST /v1/jobs` with
//! `issue`, its text read with obid's own gh and quoted as data in the job's worktree, the job run
//! to its end by the stand-in agent, and the job's answer keeping the issue and its page.

use serde_json::json;

use crate::support::*;

#[test]
fn github_recorded_issue_task() {
    let (folder, recording, mut config) =
        crate::recorder::recorded_config("issue-task", "issue_task");
    let remote = fixture(&folder.0.join("fixture"));
    config["repos"][0]["remote"] = format!("file://{}", remote.display()).into();
    config["agents"]["claude"]["bin"] = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/agents/fake-claude")
        .to_string_lossy()
        .into_owned()
        .into();
    let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
    recording.started(&obid);

    let made = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "fix",
                             "task": "KEEP PROMPTS", "issue": 140}}),
        ),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap().to_owned();
    let ended = call(&obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(ended.status, 200, "{}", ended.text);
    let read = call(&obid, "GET", &format!("/v1/jobs/{id}"), None);

    let github = |job: &serde_json::Value| {
        json!({"issue": job["issue"], "issueUrl": job["issueUrl"],
               "pullRequest": job["pullRequest"], "published": job["published"]})
    };
    let answers = json!([
        {"asked": "POST /v1/jobs", "status": made.status, "body": github(&made.body)},
        {"asked": "GET /v1/jobs/{id}/wait", "status": ended.status, "body": github(&ended.body)},
        {"asked": "GET /v1/jobs/{id}", "status": read.status, "body": github(&read.body)},
    ]);
    // The quoted file the agent reads, as obid wrote it from what gh answered.
    let quoted = std::fs::read_to_string(obid.home.join(format!(
        "worktrees/{id}/.obi/inputs/github-fixture-app-140.md"
    )))
    .unwrap();

    // The issue's own calls: obid's other loops read GitHub at its start too (the work feed, the
    // noreply email, the sign-in's status), each its own path.
    recording.assert_calls(&recording.calls_starting(&[
        "issue view",
        "api repos/obi-e2e/fixture-app/issues/",
        "api graphql -f query=query($owner",
    ]));
    recording.assert_answers(&json!({"answers": answers, "quoted": quoted_now(&quoted)}));
}

/// The quoted file with what obid makes up as it writes it replaced. Rules: the time it read the
/// issue (`as GitHub had it at <time>`) is its clock's, and the fence around each quote ends in a
/// random nonce (`quoted-<16 hex>`), so no quoted text can close it.
fn quoted_now(quoted: &str) -> String {
    let at = regex::Regex::new(r"as GitHub had it at \d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ").unwrap();
    let fence = regex::Regex::new(r"quoted-[0-9a-f]{16}").unwrap();
    let quoted = at.replace_all(quoted, "as GitHub had it at <TIME>");
    fence.replace_all(&quoted, "quoted-<NONCE>").into_owned()
}
