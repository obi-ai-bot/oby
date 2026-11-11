//! Claude Code's `rate_limit_event` and what it does to a job and its account (docs/contracts/usage.md,
//! USAGE-12 and 13): the real obid binary with the stand-in agent (tests/agents/fake-claude), sent
//! the shapes of tests/fixtures/claude-rate-limit-*.json with times from now.
mod support;

use std::path::Path;

use serde_json::{Value, json};
use support::*;

fn obid(name: &str) -> (Folder, Obid) {
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let obid = start(&folder.0, &config(&folder.0, &remote, &agent), None);
    (folder, obid)
}

fn post_job(obid: &Obid, task: &str) -> String {
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "investigate", "task": task});
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

/// The one account, `stub`, as `GET /v1/status` shows it.
fn account(obid: &Obid) -> Value {
    call(obid, "GET", "/v1/status", None).body["accounts"][0].clone()
}

/// USAGE-12: past its 5-hour window on its paid overage, a passing error is tried again as any is:
/// the 100% window isn't full, so it isn't the account's limit, and the account isn't marked.
#[test]
fn usage_12_overage_in_use_isnt_full_a_passing_error_is_tried_again() {
    let (_folder, obid) = obid("usage-12-overage");
    let now = obi_protocol::time::now();
    let usage = json!({"status": "rejected", "resetsAt": now + 3600, "rateLimitType": "five_hour",
        "overageStatus": "allowed", "isUsingOverage": true, "overageInUse": true,
        "unifiedWindows": {"five_hour": {"utilization": 1.0, "resetsAt": now + 3600},
            "seven_day": {"utilization": 0.62, "resetsAt": now + 86_400}}, "overageEnabled": true});
    let id = post_job(&obid, &format!("USAGE: {usage} TRANSIENT KEEP PROMPTS"));
    let job = ended_within(&obid, &id, 240);
    assert_eq!(job["status"], "done", "{job}");
    let log = Path::new(job["worktree"].as_str().unwrap()).join(".obi/prompts.log");
    let told = std::fs::read_to_string(log).unwrap();
    assert_eq!(told.matches("<<<end of prompt>>>").count(), 2, "{told}");
    let account = account(&obid);
    assert_eq!(account["fiveHour"], 1.0, "{account}");
    assert!(account["fullUntil"].is_null(), "{account}");
}

/// USAGE-13: a turn one model's week stopped (`seven_day_opus`) ends asking the owner, with Try
/// again, and its account isn't marked full: the next job on it runs.
#[test]
fn usage_13_a_scoped_limit_doesnt_fill_the_account_the_next_job_runs() {
    let (_folder, obid) = obid("usage-13-scoped");
    let now = obi_protocol::time::now();
    let usage = json!({"status": "rejected", "resetsAt": now + 86_400,
        "rateLimitType": "seven_day_opus", "utilization": 1.0, "overageStatus": "rejected",
        "overageDisabledReason": "out_of_credits", "isUsingOverage": false,
        "unifiedWindows": {"five_hour": {"utilization": 0.31, "resetsAt": now + 3600},
            "seven_day": {"utilization": 0.58, "resetsAt": now + 86_400}}, "overageEnabled": false});
    let id = post_job(&obid, &format!("USAGE: {usage} LIMITED NO EVENT"));
    let job = ended_within(&obid, &id, 120);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["nextStep"], "Try again", "{job}");
    assert!(job["resumesAt"].is_null(), "{job}");
    let account = account(&obid);
    assert_eq!(account["sevenDay"], 0.58, "{account}");
    assert!(account["fullUntil"].is_null(), "{account}");
    let next = post_job(&obid, "Look around");
    let job = ended_within(&obid, &next, 120);
    assert_eq!(job["status"], "done", "{job}");
}
