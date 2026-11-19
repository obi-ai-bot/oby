//! The noreply email (crate::jobs, `github_email`): with no git email, obid asks gh for the GitHub
//! login's noreply address (`gh api user --jq …`) at its start, and a job's commits are by it.

use std::path::Path;

use serde_json::{Value, json};

use crate::support::*;

#[test]
fn github_recorded_noreply() {
    let (folder, recording, mut config) = crate::recorder::recorded_config("noreply", "noreply");
    let remote = fixture(&folder.0.join("fixture"));
    config["repos"][0]["remote"] = json!(format!("file://{}", remote.display()));
    config["agents"]["claude"]["bin"] =
        json!(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/publishing-claude"));
    signed_in(&config);
    let obid = start(&folder.0, &config, Some(&crate::recorder::bin()));
    recording.started(&obid);
    // obid's read at its start answered (the fake raises `user-read` once it has).
    within(30, "obid to ask gh for the login's email", || {
        recording.dir.join("flags/user-read").exists().then_some(())
    });

    let made = call_typed(
        &obid,
        "POST",
        "/v1/jobs",
        &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
                        "check": "test -f hello.txt"}}),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap();
    let job = ended_within(&obid, id, 120);
    assert_eq!(job["status"], "done", "{job}\n{}", obid.log());
    let branch = job["branch"].as_str().unwrap();
    let author = std::process::Command::new("git")
        .args(["log", "-1", "--format=%an <%ae>|%cn <%ce>"])
        .arg(format!("refs/heads/{branch}"))
        .current_dir(&remote)
        .output()
        .unwrap();
    assert!(author.status.success(), "{author:?}");
    let author = String::from_utf8_lossy(&author.stdout).trim().to_owned();

    recording.assert_calls(&recording.calls_starting(&["api user"]));
    recording.assert_answers(&Value::Array(vec![json!({
        "asked": "the author and committer of the commit the job pushed",
        "commit": author,
    })]));
}
