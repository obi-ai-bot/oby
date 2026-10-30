//! Jobs start as soon as they have a slot, whatever the Mac's memory (docs/runner-api.md, "Jobs
//! run side by side"): obid once held a queued job back until the machine had room for what jobs
//! like it held at most (`step` "Waiting for memory"), and no longer does (2026-10-28). The real
//! obid binary with the stand-in agent (tests/agents/fake-claude), the Mac's available memory said
//! by `OBID_MEMORY_AVAILABLE`, set tight enough that the old admission would have held the second
//! job back.
mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::*;

fn post(obid: &Obid, kind: &str, task: &str) -> String {
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": kind, "task": task});
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

fn job(obid: &Obid, id: &str) -> Value {
    call(obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

/// Polls until `ready` holds for job `id`, for at most 30 seconds.
fn until(obid: &Obid, id: &str, ready: impl Fn(&Value) -> bool) -> Value {
    let give_up_at = Instant::now() + Duration::from_secs(30);
    loop {
        let job = job(obid, id);
        if ready(&job) {
            return job;
        }
        assert!(
            Instant::now() < give_up_at,
            "the job never got there: {job}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

const GIB: u64 = 1024 * 1024 * 1024;

/// A 10 GiB Mac with `available` bytes free.
fn on_a_mac_with(name: &str, available: u64) -> (Folder, Obid) {
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let config = config(&folder.0, &remote, &agent);
    let (total, available) = ((10 * GIB).to_string(), available.to_string());
    let env = [
        ("OBID_MEMORY_TOTAL", total.as_str()),
        ("OBID_MEMORY_AVAILABLE", available.as_str()),
    ];
    let obid = start_with(&folder.0, &config, None, &env);
    (folder, obid)
}

/// With 2 GiB available and no job measured yet (each would once have been budgeted 1.5 GiB), a
/// second and a third job start next to the first instead of waiting for memory: none is ever
/// `queued` for memory, none says "Waiting for memory", and status lists nothing waiting for it.
#[test]
fn jobs_start_side_by_side_whatever_the_memory() {
    let (_folder, obid) = on_a_mac_with("no-admission", 2 * GIB);
    let first = post(&obid, "research", "SLEEP");
    until(&obid, &first, |j| j["status"] == "running");
    let second = post(&obid, "fix", "SLEEP");
    let third = post(&obid, "research", "SLEEP");
    for id in [&second, &third] {
        let started = until(&obid, id, |j| j["status"] == "running");
        assert_eq!(started["waiting"], Value::Null, "{started}");
        assert_ne!(started["step"], "Waiting for memory", "{started}");
    }
    // All three run at once: memory holds no new work back.
    for id in [&first, &second, &third] {
        let running = job(&obid, id);
        assert_eq!(running["status"], "running", "{running}");
    }
    let status = call(&obid, "GET", "/v1/status", None).body;
    assert_eq!(status["running"], 3, "{status}");
    assert_eq!(status["memory"]["waiting"], json!([]), "{status}");
    for id in [&first, &second, &third] {
        assert_eq!(
            call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None).status,
            200
        );
    }
}
