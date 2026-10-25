//! Admission by memory (docs/runner-api.md, "Jobs run side by side"): a queued job starts only
//! once the machine has room for what jobs like it held at most, and otherwise waits, `step`
//! "Waiting for memory". The real obid binary with the stand-in agent (tests/agents/fake-claude),
//! the Mac's available memory said by `OBID_MEMORY_AVAILABLE`: a test can't fill the Mac.
mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::*;

fn post(obid: &Obid, task: &str) -> String {
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "research", "task": task});
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

/// With 2 GiB available and no job measured yet (each may take 1.5 GiB), the first job starts
/// (nothing of obid's runs), the second waits for memory without a word in the inbox, status
/// says so, and it starts once the first ends. Never one stopped for memory.
#[test]
fn a_job_waits_for_memory_and_starts_once_there_is_room() {
    let folder = Folder::new("admission");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let config = config(&folder.0, &remote, &agent);
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_MEMORY_AVAILABLE", "2147483648")],
    );
    let first = post(&obid, "SLEEP");
    until(&obid, &first, |j| j["status"] == "running");
    let second = post(&obid, "SLEEP");
    let waiting = until(&obid, &second, |j| j["step"] == "Waiting for memory");
    assert_eq!(waiting["status"], "queued");
    // The kind the apps already know, saying what it waits for.
    assert_eq!(
        waiting["waiting"],
        json!({"kind": "cap", "text": "memory frees up"})
    );
    let status = call(&obid, "GET", "/v1/status", None).body;
    assert_eq!(status["running"], 1);
    let waits = status["memory"]["waiting"].as_array().unwrap();
    assert_eq!(waits.len(), 1, "{status}");
    assert_eq!(waits[0]["id"], second.as_str());
    assert_eq!(waits[0]["budget"], 1536 * 1024 * 1024);
    assert!(waits[0]["since"].is_string());
    // The first keeps running: memory only holds new work back.
    assert_eq!(job(&obid, &first)["status"], "running");
    let inbox = call(&obid, "GET", "/v1/inbox", None).body;
    assert!(
        !inbox.to_string().contains(&second),
        "a wait for memory is no inbox item: {inbox}"
    );

    assert_eq!(
        call(&obid, "POST", &format!("/v1/jobs/{first}/stop"), None).status,
        200
    );
    let started = until(&obid, &second, |j| j["status"] == "running");
    assert_eq!(started["waiting"], Value::Null);
    let status = call(&obid, "GET", "/v1/status", None).body;
    assert_eq!(status["memory"]["waiting"], json!([]));
    assert_eq!(
        call(&obid, "POST", &format!("/v1/jobs/{second}/stop"), None).status,
        200
    );
}
