//! A job publishing: the push, the pull request it opens (`gh pr list`, `gh pr create`), the
//! follow read of it (`gh pr view`), and a follow-up's push onto the same branch, which brings that
//! pull request's description up to date (`gh pr edit`).
//!
//! Not recorded: `gh` as git's credential helper. obid adds it (`credential.helper=!env … gh auth
//! git-credential`, crate::publish::credentials) only for an `https://` remote, and these tests'
//! remote is the fixture's `file://` one, so git never asks it; obid's API doesn't show the setting.

use std::path::Path;

use serde_json::{Value, json};

use crate::support::*;

/// The stand-in agent that commits what its task names (`COMMIT: <file>`).
fn agent() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/publishing-claude")
}

/// What a job's answer says of GitHub: its pull request as obid read it, where it published, what
/// went out, and why not.
fn github_of(job: &Value) -> Value {
    let mut picked = serde_json::Map::new();
    for key in [
        "branch",
        "published",
        "publishError",
        "pullRequest",
        "pullRequestError",
        "outgoing",
        "issue",
        "issueUrl",
    ] {
        if let Some(value) = job.get(key) {
            picked.insert(key.into(), value.clone());
        }
    }
    Value::Object(picked)
}

/// The job once it has ended.
fn ended(obid: &Obid, id: &str) -> Value {
    ended_within(obid, id, 120)
}

/// Rule: a job's id is 8 random hex digits, and its branch is named after it (`oby/build-<id>`):
/// each job's id as `<job-n>`, in the order the test made them.
fn jobs_named(text: &str, ids: &[&str]) -> String {
    let mut out = text.to_owned();
    for (n, id) in ids.iter().enumerate() {
        out = out.replace(id, &format!("<job-{}>", n + 1));
    }
    out
}

fn named(value: Value, ids: &[&str]) -> Value {
    serde_json::from_str(&jobs_named(&value.to_string(), ids)).unwrap()
}

#[test]
fn github_recorded_publish() {
    let (folder, recording, mut config) = crate::recorder::recorded_config("publish", "publish");
    let remote = fixture(&folder.0.join("fixture"));
    config["repos"][0]["remote"] = json!(format!("file://{}", remote.display()));
    config["agents"]["claude"]["bin"] = json!(agent());
    signed_in(&config);
    let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
    recording.started(&obid);

    let made = call_typed(
        &obid,
        "POST",
        "/v1/jobs",
        &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
                        "check": "test -f hello.txt", "title": "Hello file"}}),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let first = made.body["id"].as_str().unwrap().to_owned();
    let job = ended(&obid, &first);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    // obid reads the pull request right after it opened it.
    let opened = within(30, "obid reads the pull request", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{first}"), None).body;
        job["pullRequest"].is_object().then_some(job)
    });

    let told = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{first}/messages"),
        Some(&json!({"text": "COMMIT: second.txt"})),
    );
    assert_eq!(told.status, 201, "{}", told.text);
    let next = told.body["id"].as_str().unwrap().to_owned();
    let follow = ended(&obid, &next);
    assert_eq!(follow["status"], "done", "{follow}\n{}", obid.log());
    let edited = within(30, "the follow-up's pull request read", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{next}"), None).body;
        job["pullRequest"].is_object().then_some(job)
    });

    let ids = [first.as_str(), next.as_str()];
    // Publishing's own calls: not obid's read of the login's email at its start (noreply.rs) nor
    // its read of the GitHub work feed (`api graphql`, the feed's own recording).
    let calls: Vec<Value> = recording
        .calls_starting(&["pr ", "repo ", "issue ", "api"])
        .into_iter()
        .filter(|c| c["args"][1] != "user" && c["args"][1] != "graphql")
        .map(|c| named(c, &ids))
        .collect();
    recording.assert_calls(&calls);
    recording.assert_answers(&named(
        json!([
            {"asked": "the job, published", "job": github_of(&opened)},
            {"asked": "its follow-up, published onto its pull request", "job": github_of(&edited)},
        ]),
        &ids,
    ));
}
