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
    post_kind(obid, "research", task)
}

fn post_kind(obid: &Obid, kind: &str, task: &str) -> String {
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

/// A 10 GiB Mac with `available` bytes free: 2 GiB of it kept for the owner's apps.
fn on_a_mac_with(name: &str, available: u64, seeded: &[obid::job::Job]) -> (Folder, Obid) {
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let config = config(&folder.0, &remote, &agent);
    let jobs = folder.0.join("home/jobs");
    std::fs::create_dir_all(&jobs).unwrap();
    for job in seeded {
        std::fs::write(
            jobs.join(format!("{}.json", job.id)),
            serde_json::to_vec(job).unwrap(),
        )
        .unwrap();
    }
    let (total, available) = ((10 * GIB).to_string(), available.to_string());
    let env = [
        ("OBID_MEMORY_TOTAL", total.as_str()),
        ("OBID_MEMORY_AVAILABLE", available.as_str()),
    ];
    let obid = start_with(&folder.0, &config, None, &env);
    (folder, obid)
}

/// With 2 GiB available and no job measured yet (each may take 1.5 GiB), the first job starts
/// (nothing of obid's runs), the second waits for memory without a word in the inbox, status
/// says so, and it starts once the first ends. Never one stopped for memory.
#[test]
fn a_job_waits_for_memory_and_starts_once_there_is_room() {
    let (_folder, obid) = on_a_mac_with("admission", 2 * GIB, &[]);
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

/// Oldest first: a big job that waits for memory holds back the younger ones here, even one small
/// enough to fit, until it starts. Research jobs in the fixture measured 100 MiB (their agents and
/// their checks), fixes 3 GiB: with 1 GiB spare, a research job fits next to another, a fix
/// doesn't.
#[test]
fn younger_jobs_wait_behind_one_waiting_for_memory() {
    let now = obi_protocol::time::now();
    let measured = |n: i64, kind: &str, agent: u64| {
        let mut job = obid::job::Job::new(
            format!("seed-{kind}-{n}"),
            "obi-e2e/fixture-app".into(),
            kind.into(),
            "Look around".into(),
            "claude".into(),
            "stub".into(),
        );
        job.status = obid::job::Status::Done;
        job.finished = Some(now - 600 + n);
        job.agent_peak_rss = Some(agent);
        job.check_peak_rss = Some(100 * 1024 * 1024);
        job
    };
    let seeded: Vec<obid::job::Job> = (0..3)
        .flat_map(|n| {
            [
                measured(n, "research", 100 * 1024 * 1024),
                measured(n, "fix", 3 * GIB),
            ]
        })
        .collect();
    let (_folder, obid) = on_a_mac_with("oldest-first", 3 * GIB, &seeded);
    let small = post(&obid, "SLEEP");
    until(&obid, &small, |j| j["status"] == "running");
    let big = post_kind(&obid, "fix", "SLEEP");
    until(&obid, &big, |j| j["step"] == "Waiting for memory");
    // A second later: jobs queue by when they came, to the second.
    std::thread::sleep(Duration::from_millis(1100));
    let behind = post(&obid, "SLEEP");
    until(&obid, &behind, |j| j["step"] == "Waiting for memory");
    // Both listed, each with the room it waits for (the same second can start both waits).
    let waits = call(&obid, "GET", "/v1/status", None).body["memory"]["waiting"].clone();
    let budget_of = |id: &str| {
        waits
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["id"] == id)
            .map(|w| w["budget"].clone())
    };
    assert_eq!(waits.as_array().unwrap().len(), 2, "{waits}");
    assert_eq!(budget_of(&big), Some(json!(3 * GIB)));
    assert_eq!(budget_of(&behind), Some(json!(100 * 1024 * 1024)));

    // The big one starts once it's alone; the small one behind it now waits for room.
    assert_eq!(
        call(&obid, "POST", &format!("/v1/jobs/{small}/stop"), None).status,
        200
    );
    until(&obid, &big, |j| j["status"] == "running");
    assert_eq!(job(&obid, &behind)["step"], "Waiting for memory");
    assert_eq!(
        call(&obid, "POST", &format!("/v1/jobs/{big}/stop"), None).status,
        200
    );
    until(&obid, &behind, |j| j["status"] == "running");
    assert_eq!(
        call(&obid, "POST", &format!("/v1/jobs/{behind}/stop"), None).status,
        200
    );
}
