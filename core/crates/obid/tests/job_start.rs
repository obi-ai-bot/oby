//! What obid does between a task being asked for and its agent starting
//! (docs/contracts/task-conversations.md, TASKCONV-38 to 40), through the real obid binary, asked
//! over HTTP, with a scripted stand-in for Claude Code (tests/agents/fake-claude).

mod support;

use std::path::{Path, PathBuf};

use serde_json::json;
use support::*;

fn agents() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents")
}

/// TASKCONV-40: obid reads a job's HEAD before its turn with its own git, pinned to the job's git
/// folders, never in a box of its own: by the time its agent is at work, no box has run in the
/// job's temporary folder (each one's watcher leaves its exit there, `.obid-status`).
#[test]
fn taskconv_40_no_box_runs_before_a_jobs_first_turn() {
    let folder = Folder::new("taskconv-40");
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &agents().join("fake-claude"));
    let obid = start_with(&folder.0, &config, None, &[]);
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "build", "check": "true",
                     "task": "SLEEP"});
    let answer = call(&obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    // Its agent is at work: beating in its worktree.
    let worktree = within(30, "its agent at work", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        let worktree = PathBuf::from(job["worktree"].as_str()?);
        worktree.join(".obi/beat").exists().then_some(worktree)
    });
    let exit = folder.0.join("home/tmp").join(&id).join(".obid-status");
    assert!(
        !exit.exists(),
        "a box ran before the job's first turn: {}",
        std::fs::read_to_string(&exit).unwrap_or_default()
    );
    assert!(worktree.join(".obi/beat").exists());
    let stopped = call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    assert_eq!(stopped.status, 200, "{}", stopped.text);
    let job = ended_within(&obid, &id, 60);
    assert_eq!(job["status"], "stopped", "{job}");
}
