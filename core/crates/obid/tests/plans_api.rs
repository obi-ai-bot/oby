//! The plan and conversation endpoints for #442.
mod support;

use std::path::{Path, PathBuf};

use serde_json::json;
use support::*;

#[test]
fn plan_uses_recent_turns_and_conversation_paginates() {
    let folder = Folder::new("plans-api");
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let mut config = config(&folder.0, &remote, &harness);
    config["home"] = json!(folder.0.join("home"));
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    let obid = start(&folder.0, &config, None);
    let events = follow_events(&obid);
    let answer = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({
            "said": "SAY Fine.",
            "turns": [
                {"who":"user", "text":"how is the inbox job going?"},
                {"who":"oby", "text":"Inbox is at its check."}
            ]
        })),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let conversation = answer.body["conversation"].clone();
    assert!(conversation.is_string(), "{}", answer.text);
    assert_eq!(
        answer.body,
        json!({"kind":"reply","text":"Fine.","jobs":[],"drafts":[],"conversation":conversation})
    );
    let prompt = std::fs::read_to_string(obid.home.join(format!(
        "conversations/{}/config/turns.jsonl",
        conversation.as_str().unwrap()
    )))
    .unwrap();
    assert!(prompt.contains("Owner: how is the inbox job going?"));
    assert!(prompt.contains("Oby: Inbox is at its check."));
    assert!(prompt.contains("SAY Fine."));
    let heard = events_named(&events, "turn", 10, |got| got.len() == 2);
    assert_eq!(heard[0]["who"], "user");
    assert_eq!(heard[0]["text"], "SAY Fine.");
    assert_eq!(heard[1]["who"], "oby");
    assert_eq!(heard[1]["text"], "Fine.");
    assert!(
        heard
            .iter()
            .all(|event| event["jobs"].is_array() && event["at"].is_string())
    );

    let page = call(&obid, "GET", "/v1/conversation?limit=1", None);
    assert_eq!(page.status, 200, "{}", page.text);
    assert_eq!(page.body.as_array().unwrap().len(), 1);
    assert_eq!(page.body[0]["who"], "oby");
    let before = page.body[0]["id"].as_str().unwrap();
    let prior = call(
        &obid,
        "GET",
        &format!("/v1/conversation?before={before}&limit=1"),
        None,
    );
    assert_eq!(prior.status, 200, "{}", prior.text);
    assert_eq!(prior.body[0]["who"], "user");

    for turns in [
        json!([{"who":"robot", "text":"hello"}]),
        json!([{"who":"user", "text":" "}]),
        json!(std::iter::repeat_n(json!({"who":"user", "text":"x"}), 51).collect::<Vec<_>>()),
    ] {
        let refused = call(
            &obid,
            "POST",
            "/v1/plans",
            Some(&json!({"said":"hi", "turns": turns})),
        );
        assert_eq!(refused.status, 422, "{}", refused.text);
    }
    let malformed = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said":"hi", "turns":"no"})),
    );
    assert_eq!(malformed.status, 400, "{}", malformed.text);

    let oversized = call(&obid, "GET", "/v1/conversation?limit=9999", None);
    assert_eq!(oversized.status, 200);
    assert_eq!(oversized.body.as_array().unwrap().len(), 2);
    assert_eq!(
        call(&obid, "GET", "/v1/conversation?limit=0", None).status,
        422
    );
    assert_eq!(
        call(&obid, "GET", "/v1/conversation?before=nope", None).status,
        404
    );
}

#[test]
fn plans_map_replies_cancellations_and_dispatched_jobs() {
    let folder = Folder::new("plan-kinds");
    let obid = started_harness_jobs(&folder);
    let post = |said: &str| call(&obid, "POST", "/v1/plans", Some(&json!({"said": said})));
    assert_eq!(post(&"x".repeat(50_000)).status, 200);
    let reply = post("SAY Fine.");
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["kind"], "reply");
    assert_eq!(reply.body["text"], "Fine.");
    let conversation = reply.body["conversation"].clone();
    let none = post("stop");
    assert_eq!(none.body["kind"], "none");
    assert_eq!(none.body["conversation"], conversation);
    let dispatch = post(
        r#"REPEAT 2 OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"fix","task":"Fix the greeting"}"#,
    );
    assert_eq!(dispatch.status, 200, "{}", dispatch.text);
    assert_eq!(dispatch.body["kind"], "reply");
    assert_eq!(
        dispatch.body["jobs"].as_array().unwrap().len(),
        2,
        "{}",
        dispatch.text
    );
    assert_eq!(dispatch.body["drafts"], json!([]));
}

#[test]
fn plans_refuse_attachments_outside_the_apps_folder() {
    let folder = Folder::new("plan-attachments");
    let obid = started_harness(&folder);
    let answer = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said":"Look at this", "attachments":["/etc/hosts"]})),
    );
    assert_eq!(answer.status, 422, "{}", answer.text);
    assert!(
        answer
            .text
            .contains("isn't one of the files the Oby app attached")
    );
    // Where the Mac app saves them (`LocalRunner.attachments`, #891): a folder of its own in obid's.
    let mac = obid.home.join("attachments/mac");
    std::fs::create_dir_all(&mac).unwrap();
    let screenshot = mac.join("screenshot-1a2b3c.png");
    std::fs::write(&screenshot, b"\x89PNG\r\n\x1a\n").unwrap();
    let answer = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said":"SAY hello", "attachments":[screenshot]})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
}

#[test]
fn conversation_turns_are_kept_between_runner_restarts() {
    let folder = Folder::new("conversation-kept");
    let obid = started_harness(&folder);
    let answer = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said":"SAY Fine."})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let home = obid.home.clone();
    drop(obid);
    let config =
        serde_json::from_slice(&std::fs::read(folder.0.join("config.json")).unwrap()).unwrap();
    let restarted = start(&folder.0, &config, None);
    let turns = call(&restarted, "GET", "/v1/conversation", None);
    assert_eq!(turns.status, 200, "{}", turns.text);
    assert_eq!(turns.body.as_array().unwrap().len(), 2);
    assert_eq!(turns.body[0]["text"], "SAY Fine.");
    assert_eq!(turns.body[1]["text"], "Fine.");
    let conversation = answer.body["conversation"].as_str().unwrap();
    assert!(
        home.join(format!("conversations/{conversation}.json"))
            .is_file()
    );
}

// ---------------------------------------------------------------------------------------------
// The brain (#546): the plan, its checks, retries and fallbacks.

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The brain's turns, as obid keeps them.
fn records(obid: &Obid) -> Vec<serde_json::Value> {
    read(obid.home.join("brain-turns.jsonl"))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn started_harness(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    start(&folder.0, &config, None)
}

fn started_harness_jobs(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness-jobs");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    start(&folder.0, &config, Some(harness.parent().unwrap()))
}

fn said(obid: &Obid, words: &str) -> serde_json::Value {
    let answer = call(obid, "POST", "/v1/plans", Some(&json!({"said": words})));
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

/// [`said`], in a new conversation: since every task has one of its own (TASKCONV-1), the latest
/// conversation may be a task's.
fn said_anew(obid: &Obid, words: &str) -> serde_json::Value {
    let answer = call(
        obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": words, "new": true})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

// ---------------------------------------------------------------------------------------------
// What each brain turn spent (#533): one entry per model call in the turn's record.

/// #533's acceptance: a result carrying usage, cost, duration and turns is recorded with exactly
/// those values, next to the model and the conversation's turn.
#[test]
fn a_brain_turn_records_what_its_call_reported() {
    let folder = Folder::new("brain-numbers");
    let obid = started_harness(&folder);
    let answer = said(&obid, "USAGE");
    assert_eq!(answer["text"], "used");
    let record = records(&obid).pop().unwrap();
    assert_eq!(record["conversation"], answer["conversation"]);
    assert!(
        record["turn"].as_str().is_some_and(|t| !t.is_empty()),
        "{record}"
    );
    let calls = record["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1, "{record}");
    let mut call = calls[0].clone();
    assert!(call["wall_ms"].as_u64().is_some(), "{call}");
    call.as_object_mut().unwrap().remove("wall_ms");
    assert_eq!(
        call,
        json!({"adapter":"claude", "model":"claude-sonnet-5-5", "outcome":"answered", "subtype":"success", "duration_ms":40, "num_turns":2, "total_cost_usd":0.002, "input_tokens":11, "output_tokens":7, "cache_read_input_tokens":5, "cache_creation_input_tokens":3})
    );
}

/// #533's acceptance: a result with no usage leaves the numbers null, never zero.
#[test]
fn a_result_without_usage_leaves_the_numbers_null() {
    let folder = Folder::new("brain-no-usage");
    let obid = started_harness(&folder);
    said(&obid, "SAY Ready");
    let record = records(&obid).pop().unwrap();
    let call = &record["calls"][0];
    for field in [
        "total_cost_usd",
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ] {
        assert!(call[field].is_null(), "{field} in {call}");
        assert!(call.get(field).is_some(), "{field} missing in {call}");
    }
    assert_eq!(call["outcome"], "answered");
    assert_eq!(call["subtype"], "success");
}

/// #533's acceptance: a run that ran out of turns without a plan is recorded with its subtype,
/// as a failed call.
#[test]
fn a_run_out_of_turns_is_a_failed_call_with_its_subtype() {
    let folder = Folder::new("brain-max-turns");
    let obid = started_harness(&folder);
    let answer = said(&obid, "MAXTURNS");
    assert_eq!(
        answer["text"],
        "I couldn't answer: its agent session failed twice."
    );
    let records = records(&obid);
    let calls: Vec<_> = records
        .iter()
        .filter(|r| r["kind"] == "conversation")
        .flat_map(|r| r["calls"].as_array().unwrap())
        .collect();
    assert_eq!(calls.len(), 2, "{records:?}");
    for call in calls {
        assert_eq!(call["outcome"], "failed", "{call}");
        assert_eq!(call["subtype"], "error_max_turns", "{call}");
        assert_eq!(call["num_turns"], 2, "{call}");
    }
}

/// Each failed harness attempt is recorded separately, so retries are counted in usage.
#[test]
fn every_attempt_is_its_own_call_in_the_turns_record() {
    let folder = Folder::new("brain-attempts");
    let obid = started_harness(&folder);
    let answer = said(&obid, "FAIL");
    assert_eq!(
        answer["text"],
        "I couldn't answer: its agent session failed twice."
    );
    let attempts: Vec<_> = records(&obid)
        .into_iter()
        .filter(|r| r["kind"] == "conversation")
        .collect();
    assert_eq!(attempts.len(), 2, "{attempts:?}");
    assert_eq!(attempts[0]["turn"], attempts[1]["turn"]);
    for attempt in attempts {
        assert_eq!(attempt["calls"].as_array().unwrap().len(), 1, "{attempt}");
        assert_eq!(attempt["calls"][0]["outcome"], "failed", "{attempt}");
        assert_eq!(attempt["calls"][0]["adapter"], "claude", "{attempt}");
    }
}

// ---------------------------------------------------------------------------------------------
// The briefing (#546, step 2): what the brain is told before the user's words.

/// A job whose agent works until it's stopped, titled `title`, running.
fn running(obid: &Obid, title: &str) -> serde_json::Value {
    let answer = call(
        obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research",
                             "task": "SLEEP", "title": title}}),
        ),
    );
    assert!(
        answer.status == 200 || answer.status == 201,
        "{}",
        answer.text
    );
    let id = answer.body["id"].as_str().unwrap().to_owned();
    within(30, "the job runs", || {
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["status"] == "running" && !job["step"].as_str().unwrap_or("").is_empty())
            .then_some(job)
    })
}

fn host(obid: &Obid) -> String {
    call(obid, "GET", "/v1/status", None).body["runner"]["host"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The harness system prompt stays byte-for-byte stable when a job starts, so it can be cached.
#[test]
fn the_harness_prompt_stays_the_same_when_jobs_change() {
    let folder = Folder::new("brain-prompt-cache");
    let obid = started_harness_jobs(&folder);
    let first = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said":"SAY first", "new":true})),
    )
    .body;
    let job = running(&obid, "Vault secrets");
    let second = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said":"SAY second", "new":true})),
    )
    .body;
    let prompt = |answer: &serde_json::Value| {
        let id = answer["conversation"].as_str().unwrap();
        let log = std::fs::read_to_string(
            obid.home
                .join(format!("conversations/{id}/config/turns.jsonl")),
        )
        .unwrap();
        let entry: serde_json::Value = serde_json::from_str(log.lines().next().unwrap()).unwrap();
        let argv = entry["argv"].as_array().unwrap();
        let at = argv
            .iter()
            .position(|a| a == "--append-system-prompt")
            .unwrap();
        argv[at + 1].as_str().unwrap().to_owned()
    };
    let before = prompt(&first);
    let after = prompt(&second);
    assert_eq!(before, after);
    assert!(!after.contains(job["id"].as_str().unwrap()));
    assert!(!after.contains("Vault secrets"));
}

/// ConversationTests.theBrainReadsTheConversationAndMayStopOneJob, and #546's acceptance: "how
/// are the jobs going?" with a running job is told about it, and "cancel it" right after stops it.
#[test]
fn the_brain_reads_the_conversation_and_may_stop_one_job() {
    let folder = Folder::new("brain-briefing");
    let obid = started_harness_jobs(&folder);
    let job = running(&obid, "Vault secrets");
    let id = job["id"].as_str().unwrap();
    let first = said(&obid, "OBY oby_jobs {}");
    assert!(
        first["text"].as_str().unwrap().contains("Vault secrets"),
        "{first}"
    );
    let second = said(&obid, &format!(r#"OBY oby_stop_job {{"id":"{id}"}}"#));
    assert!(
        second["text"]
            .as_str()
            .unwrap()
            .contains("Stopped Vault secrets"),
        "{second}"
    );
    assert_eq!(
        call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body["status"],
        "stopped"
    );
    assert_eq!(second["conversation"], first["conversation"]);
}

/// ConversationTests.theReplyNeverContradictsTheHeaderAboutRunningJobs: with an agent working,
/// Oby never says nothing is running; it says what the header counts.
#[test]
fn the_reply_never_contradicts_the_header_about_running_jobs() {
    let folder = Folder::new("brain-header");
    let obid = started_harness_jobs(&folder);
    let job = running(&obid, "Vault secrets");
    let id = job["id"].as_str().unwrap();
    let line = format!("1 agent is working: Vault secrets on {}.", host(&obid));
    assert_eq!(
        said(&obid, "SAY No jobs are running right now.")["text"],
        line
    );
    let page = call(&obid, "GET", "/v1/conversation", None).body;
    assert_eq!(page.as_array().unwrap().last().unwrap()["text"], line);
    assert_eq!(
        said(&obid, "SAY No jobs failed; Vault secrets is still running.")["text"],
        "No jobs failed; Vault secrets is still running."
    );
    assert_eq!(
        call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None).status,
        200
    );
    assert_eq!(
        said(&obid, "SAY No jobs are running right now.")["text"],
        "No jobs are running right now."
    );
}

// ---------------------------------------------------------------------------------------------
// The loop (#546, step 3): several actions in one turn, their step events, Stop and cancel.

/// The id of the user's turn that said `words`.
fn turn_of(obid: &Obid, words: &str) -> String {
    call(obid, "GET", "/v1/conversation?limit=500", None)
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["who"] == "user" && t["text"] == words)
        .map(|t| t["id"].as_str().unwrap().to_owned())
        .unwrap()
}

/// BrainRetryTests.cancellationDoesNotRestartClaude: a turn stopped while Claude failed isn't
/// tried again.
#[test]
fn a_stop_does_not_restart_claude() {
    let folder = Folder::new("brain-stop-retry");
    let obid = started_harness(&folder);
    let events = follow_events(&obid);
    let answer = std::thread::scope(|scope| {
        let asking = scope.spawn(|| said(&obid, "SLOWFAIL 2000"));
        events_named(&events, "thinking", 10, |got| !got.is_empty());
        call(&obid, "POST", "/v1/conversation/stop", None);
        asking.join().unwrap()
    });
    assert_eq!(answer["kind"], "none", "{answer}");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let attempts: Vec<_> = records(&obid)
        .into_iter()
        .filter(|r| r["kind"] == "conversation")
        .collect();
    assert_eq!(attempts.len(), 1, "{attempts:?}");
}

/// ConversationTests.aCancelledTurnStopsNothing: cancelled while the brain thought, the turn ends
/// as `none` and stops no running job.
#[test]
fn a_cancelled_turn_stops_nothing() {
    let folder = Folder::new("brain-cancelled");
    let obid = started_harness_jobs(&folder);
    let id = running(&obid, "Vault secrets")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let events = follow_events(&obid);
    let answer = std::thread::scope(|scope| {
        let asking = scope.spawn(|| said(&obid, "LINGER 2000"));
        events_named(&events, "thinking", 10, |got| !got.is_empty());
        let cancelled = call(&obid, "POST", "/v1/plans/cancel", None);
        assert_eq!(cancelled.body, json!({"cancelled": true}));
        asking.join().unwrap()
    });
    assert_eq!(answer["kind"], "none", "{answer}");
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["status"], "running");
}

/// ConversationTests.neverMindAndStopEndTheTurnAndStopNoJob: a bare "never mind" or "stop" never
/// reaches the brain and stops no job; with a turn under way, it stops that turn.
#[test]
fn never_mind_and_stop_end_the_turn_and_stop_no_job() {
    let folder = Folder::new("brain-never-mind");
    let obid = started_harness_jobs(&folder);
    let id = running(&obid, "Vault secrets")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for words in ["stop", "Cancel.", "never mind"] {
        let answer = said(&obid, words);
        assert_eq!(answer["kind"], "none", "{words}: {answer}");
    }
    assert!(records(&obid).is_empty());
    // nothing said joined a conversation (the latest is the task's own, TASKCONV-1)
    let latest = call(&obid, "GET", "/v1/conversation", None).body;
    assert!(
        latest
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["who"] != "user" || t["text"] == "SLEEP"),
        "{latest}"
    );
    let events = follow_events(&obid);
    let answer = std::thread::scope(|scope| {
        let asking = scope.spawn(|| said(&obid, "LINGER 2000"));
        events_named(&events, "thinking", 10, |got| !got.is_empty());
        assert_eq!(said(&obid, "never mind")["kind"], "none");
        asking.join().unwrap()
    });
    assert_eq!(answer["kind"], "none", "{answer}");
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["status"], "running");
}

/// StopTests.stopAgentStopsRunningAndQueuedJobs: `oby_stop_job` stops both running and queued jobs.
#[test]
fn stop_everything_stops_running_and_queued_jobs() {
    let folder = Folder::new("brain-stop-all");
    let obid = started_harness_jobs(&folder);
    let settings = call(
        &obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"maxRunning": 1})),
    );
    assert_eq!(settings.status, 200, "{}", settings.text);
    let first = running(&obid, "Vault secrets")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let queued = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(&json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research", "task": "SLEEP", "title": "Watch home"}})),
    )
    .body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        call(&obid, "GET", &format!("/v1/jobs/{queued}"), None).body["status"],
        "queued"
    );
    let answer = said(
        &obid,
        &format!(
            r#"REPEAT 1 OBY oby_stop_job {{"id":"{first}"}} THEN OBY oby_stop_job {{"id":"{queued}"}}"#
        ),
    );
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("Stopped Watch home"),
        "{answer}"
    );
    for id in [&first, &queued] {
        assert_eq!(
            call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body["status"],
            "stopped"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// What persists or widens trust waits for the owner's own Confirm (#546, the owner's decision of
// 2026-10-16): the brain never does it itself, however it was talked into it.

/// A job from `POST /v1/jobs` in `mode`, running `task`.
fn job_in(obid: &Obid, title: &str, task: &str, mode: &str) -> String {
    let answer = call(
        obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research", "task": task,
                             "title": title, "mode": mode}}),
        ),
    );
    assert!(
        answer.status == 200 || answer.status == 201,
        "{}",
        answer.text
    );
    answer.body["id"].as_str().unwrap().to_owned()
}

/// The open inbox items of `kind`.
fn items_of(obid: &Obid, kind: &str) -> Vec<serde_json::Value> {
    call(obid, "GET", "/v1/inbox", None)
        .body
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == kind)
        .cloned()
        .collect()
}

/// The owner's own answer: Confirm on a confirm item is their tap, signed on a device (#776).
fn owner_answers(obid: &Obid, item: &str, answer: &str) -> Answer {
    let open = call(obid, "GET", "/v1/inbox", None).body;
    let is_confirm = open
        .as_array()
        .is_some_and(|a| a.iter().any(|i| i["id"] == item && i["kind"] == "confirm"));
    if answer == "Confirm" && is_confirm {
        return confirm(obid, item);
    }
    call(
        obid,
        "POST",
        &format!("/v1/inbox/{item}/answer"),
        Some(&json!({"answer": answer})),
    )
}

/// set_mode bypass leaves the mode as it is: a confirm item waits, and the owner's Confirm sets
/// it. Other modes are set at once.
#[test]
fn bypass_mode_waits_for_the_owners_confirm() {
    let folder = Folder::new("brain-confirm-bypass");
    let obid = started_harness_jobs(&folder);
    let id = job_in(&obid, "Vault secrets", "SLEEP", "ask");
    within(30, "the job runs", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["status"] == "running").then_some(())
    });
    let mode = |mode: &str| format!(r#"OBY oby_change_job {{"id":"{id}","mode":"{mode}"}}"#);
    let answer = said(&obid, &mode("bypass"));
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("waits for your OK"),
        "{answer}"
    );
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["mode"], "ask");
    let confirms = items_of(&obid, "confirm");
    assert_eq!(confirms.len(), 1, "{confirms:?}");
    assert_eq!(confirms[0]["title"], "Put Vault secrets in Bypass mode");
    assert_eq!(confirms[0]["confirm"]["action"], "change_job");
    assert_eq!(confirms[0]["confirm"]["target"], id);
    assert_eq!(confirms[0]["confirm"]["payload"]["mode"], "bypass");
    assert_eq!(confirms[0]["confirm"]["payload"]["snapshot"]["mode"], "ask");
    // Cancel does nothing; a second card's Confirm does it.
    assert_eq!(
        owner_answers(&obid, confirms[0]["id"].as_str().unwrap(), "Cancel").status,
        200
    );
    assert_eq!(
        call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body["mode"],
        "ask"
    );
    said(&obid, &mode("bypass"));
    let confirm = items_of(&obid, "confirm").pop().unwrap();
    assert_eq!(
        owner_answers(&obid, confirm["id"].as_str().unwrap(), "Confirm").status,
        200
    );
    assert_eq!(
        call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body["mode"],
        "bypass"
    );
    assert!(
        items_of(&obid, "confirm").is_empty(),
        "closed once confirmed"
    );
    // The other modes are done in the turn.
    assert!(
        said(&obid, &mode("plan"))["text"]
            .as_str()
            .unwrap()
            .contains("Vault secrets is in Plan mode now.")
    );
}

/// Stop and cancel reach the turn under way in one conversation, not the others (review on #551).
#[test]
fn stop_reaches_only_its_own_conversation() {
    let folder = Folder::new("brain-stop-scope");
    let obid = started_harness(&folder);
    let first = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said":"SAY first", "new":true})),
    )
    .body["conversation"]
        .as_str()
        .unwrap()
        .to_owned();
    let second = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said":"SAY second", "new":true})),
    )
    .body["conversation"]
        .as_str()
        .unwrap()
        .to_owned();
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            call(
                &obid,
                "POST",
                "/v1/plans",
                Some(&json!({"said":"LINGER 5000", "conversation":first})),
            )
            .body
        });
        let b = scope.spawn(|| {
            call(
                &obid,
                "POST",
                "/v1/plans",
                Some(&json!({"said":"LINGER 2000", "conversation":second})),
            )
            .body
        });
        for id in [&first, &second] {
            let log = obid
                .home
                .join(format!("conversations/{id}/config/turns.jsonl"));
            within(10, "the turn starts", || {
                std::fs::read_to_string(&log)
                    .ok()?
                    .contains("LINGER")
                    .then_some(())
            });
        }
        let stopped = call(
            &obid,
            "POST",
            "/v1/conversation/stop",
            Some(&json!({"conversation":first})),
        );
        assert_eq!(stopped.body, json!({"stopped":true}));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(a["kind"], "none", "{a}");
    assert_eq!(b["text"], "waited", "{b}");
}

/// A brain dispatch can't start a job in Bypass mode: it waits as a confirm item whose title,
/// made by obid from the job, says so; the owner's Confirm starts it (review on #551, N1).
#[test]
fn a_bypass_dispatch_waits_for_the_owners_confirm() {
    let folder = Folder::new("brain-confirm-dispatch");
    let obid = started_harness_jobs(&folder);
    let answer = said(
        &obid,
        r#"OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"fix","title":"Greeting fix","task":"Fix the greeting","check":"node --test","mode":"bypass"}"#,
    );
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("confirm it in your inbox"),
        "{answer}"
    );
    assert_eq!(answer["jobs"], json!([]));
    assert_eq!(call(&obid, "GET", "/v1/jobs", None).body, json!([]));
    let confirm = items_of(&obid, "confirm").pop().unwrap();
    assert_eq!(
        confirm["title"],
        "Start fix fixture-app (Greeting fix) in Bypass mode, on auto · auto, check node --test"
    );
    assert_eq!(confirm["confirm"]["action"], "start_job");
    let id = confirm["id"].as_str().unwrap().to_owned();
    // Owner UI retries can race at the HTTP boundary. Both responses may describe the same
    // completed card, but only one proposed job may be created.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let port = obid.port;
    // Two taps a second apart: both signatures hold, so the claim decides (#776).
    let now = obi_protocol::time::now();
    let confirmers: Vec<_> = (0..2)
        .map(|n| {
            let barrier = barrier.clone();
            let id = id.clone();
            let approval = confirm_approval_at(&obid, &id, now - n);
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                use std::net::TcpStream;
                barrier.wait();
                let body = json!({"answer": "Confirm", "approval": approval}).to_string();
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                write!(
                    stream,
                    "POST /v1/inbox/{id}/answer HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    TOKEN,
                    body.len()
                )
                .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                response
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .parse::<u16>()
                    .unwrap()
            })
        })
        .collect();
    for confirmer in confirmers {
        assert!(matches!(confirmer.join().unwrap(), 200 | 409));
    }
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    assert_eq!(jobs.as_array().unwrap().len(), 1, "{jobs}");
    assert_eq!(jobs[0]["mode"], "bypass");
    assert_eq!(jobs[0]["title"], "Greeting fix");
    // The item is closed once confirmed.
    assert!(items_of(&obid, "confirm").is_empty());
}

/// A confirm item whose target moved on is closed with why, not left open (review on #551).
#[test]
fn a_stale_confirm_item_is_closed_with_why() {
    let folder = Folder::new("brain-confirm-stale");
    let obid = started_harness_jobs(&folder);
    let sleeping = job_in(&obid, "Watch home", "SLEEP", "ask");
    within(30, "the job runs", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{sleeping}"), None).body;
        (job["status"] == "running").then_some(())
    });
    let answer = said(
        &obid,
        &format!(r#"OBY oby_change_job {{"id":"{sleeping}","mode":"bypass"}}"#),
    );
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("waits for your OK"),
        "{answer}"
    );
    let confirm = items_of(&obid, "confirm").pop().unwrap();
    assert_eq!(
        call(&obid, "POST", &format!("/v1/jobs/{sleeping}/stop"), None).status,
        200
    );
    let late = owner_answers(&obid, confirm["id"].as_str().unwrap(), "Confirm");
    assert_eq!(late.status, 409, "{}", late.text);
    assert_eq!(
        call(&obid, "GET", &format!("/v1/jobs/{sleeping}"), None).body["mode"],
        "ask"
    );
    assert!(items_of(&obid, "confirm").is_empty());
}

/// A bare "stop" said to start a new conversation stops no turn elsewhere (review on #551).
#[test]
fn a_bare_stop_in_a_new_conversation_stops_nothing_elsewhere() {
    let folder = Folder::new("brain-stop-new");
    let obid = started_harness(&folder);
    let events = follow_events(&obid);
    let answer = std::thread::scope(|scope| {
        let asking = scope.spawn(|| said(&obid, "LINGER 2000"));
        events_named(&events, "thinking", 10, |got| !got.is_empty());
        let stop = call(
            &obid,
            "POST",
            "/v1/plans",
            Some(&json!({"said":"stop", "new":true})),
        );
        assert_eq!(stop.body["kind"], "none", "{}", stop.text);
        asking.join().unwrap()
    });
    assert_eq!(answer["text"], "waited", "{answer}");
}

/// Writes obid's vault file with `secret` (as `obid vault add` would): obid reads it from the file.
fn vault_holds(obid: &Obid, secret: &str) {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let dir = obid.home.join("vault");
    if !dir.exists() {
        std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dir.join("secrets.json"))
        .unwrap();
    std::io::Write::write_all(
        &mut file,
        json!({"secrets": {"staging-api-key": secret}})
            .to_string()
            .as_bytes(),
    )
    .unwrap();
}

/// The harness never receives a vault secret, and an unreadable vault starts no session.
#[test]
fn no_vault_secret_reaches_the_brain_and_an_unreadable_vault_runs_none() {
    let folder = Folder::new("brain-vault");
    let obid = started_harness(&folder);
    let secret = ["vault-", "brain/", "Q7m2+a=b?c&d"].concat();
    vault_holds(&obid, &secret);
    let answer = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": format!("SAY {secret}"), "new": true})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert!(!answer.text.contains(&secret), "{}", answer.text);
    let id = answer.body["conversation"].as_str().unwrap();
    let log = obid
        .home
        .join(format!("conversations/{id}/config/turns.jsonl"));
    let sent = std::fs::read_to_string(&log).unwrap();
    assert!(
        !sent.contains(&secret),
        "the vault secret reached the harness"
    );
    assert!(
        !sent.contains("Q7m2"),
        "part of the vault secret reached the harness"
    );
    std::thread::scope(|scope| {
        let turn = scope.spawn(|| {
            call(
                &obid,
                "POST",
                "/v1/plans",
                Some(&json!({"said": "LINGER 1000", "conversation": id})),
            )
        });
        within(10, "the harness starts", || {
            std::fs::read_to_string(&log)
                .ok()?
                .contains("LINGER 1000")
                .then_some(())
        });
        std::fs::write(obid.home.join("vault/secrets.json"), "not the vault").unwrap();
        let answer = turn.join().unwrap();
        assert_eq!(answer.status, 503, "{}", answer.text);
    });
    let sent = std::fs::read_to_string(&log).unwrap();
    let failed = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "SAY Should not run", "conversation": id})),
    );
    assert_eq!(failed.status, 503, "{}", failed.text);
    assert_eq!(
        std::fs::read_to_string(log).unwrap(),
        sent,
        "the harness ran with an unreadable vault"
    );
}

// ---------------------------------------------------------------------------------------------
// The actions on jobs and the inbox (#546, step 4).

/// A job from `POST /v1/jobs` with `task`, once its status is `status`.
fn job_that(
    obid: &Obid,
    title: &str,
    task: &str,
    mode: Option<&str>,
    status: &str,
) -> serde_json::Value {
    let mut job =
        json!({"repo": "obi-e2e/fixture-app", "kind": "research", "task": task, "title": title});
    if let Some(mode) = mode {
        job["mode"] = json!(mode);
    }
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({"job": job})));
    assert!(
        answer.status == 200 || answer.status == 201,
        "{}",
        answer.text
    );
    let id = answer.body["id"].as_str().unwrap().to_owned();
    within(30, &format!("the job is {status}"), || {
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["status"] == status).then_some(job)
    })
}

fn get_job(obid: &Obid, id: &str) -> serde_json::Value {
    call(obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

/// One turn continues a failed job and archives another.
#[test]
fn one_message_tries_a_failed_job_again_and_dismisses_the_old_one() {
    let folder = Folder::new("brain-try-again");
    let obid = started_harness_jobs(&folder);
    // both started in one conversation, the turn's own: another conversation's job (a task started
    // on its own has its own, TASKCONV-1) is acted on only on the owner's Confirm (#631)
    let first = said_anew(
        &obid,
        r#"OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"research","title":"Notch three screens","task":"FAIL"}"#,
    );
    let conversation = first["conversation"].as_str().unwrap().to_owned();
    let in_it = |words: String| {
        let answer = call(
            &obid,
            "POST",
            "/v1/plans",
            Some(&json!({"said": words, "conversation": conversation})),
        );
        assert_eq!(answer.status, 200, "{}", answer.text);
        answer.body
    };
    let second = in_it(
        r#"OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"research","title":"Old PR job","task":"Nothing"}"#
            .to_owned(),
    );
    let notch = first["jobs"][0]["id"].as_str().unwrap().to_owned();
    let old = second["jobs"][0]["id"].as_str().unwrap().to_owned();
    for (id, status) in [(&notch, "interrupted"), (&old, "done")] {
        within(30, &format!("the job is {status}"), || {
            (get_job(&obid, id)["status"] == status).then_some(())
        });
    }
    let (notch, old) = (notch.as_str(), old.as_str());
    let answer = in_it(format!(
        r#"REPEAT 1 OBY oby_continue_job {{"id":"{notch}"}} THEN OBY oby_archive_job {{"id":"{old}"}}"#
    ));
    let text = answer["text"].as_str().unwrap();
    assert!(text.contains("Archived Old PR job."), "{text}");
    assert!(get_job(&obid, old)["archived"].is_string());
    let notes = get_job(&obid, notch)["notes"].to_string();
    assert!(
        notes.contains("Try again, picking up where you left off."),
        "{notes}"
    );
}

/// #858: a job's agent's question is the owner's to answer: oby_tell_job's words that aren't
/// their own signed word wait for their Confirm, and it still waits. The owner's answer reaches the agent without the vault's secrets, and none
/// goes while the vault can't be read.
#[test]
fn only_the_owner_answers_a_jobs_question_and_without_vault_secrets() {
    let folder = Folder::new("brain-question");
    let obid = started_harness_jobs(&folder);
    let ask = json!({"tool": "AskUserQuestion", "input": {"questions": [{"question": "Which store?",
        "options": [{"label": "SQLite"}, {"label": "Postgres"}]}]}});
    let id = job_in(&obid, "Asker", &format!("ASK: {ask}"), "ask");
    let waiting = within(30, "the job asks its question", || {
        let job = get_job(&obid, &id);
        job["permission"]["question"].is_object().then_some(job)
    });
    let permission = waiting["permission"]["id"].clone();
    let answer = said(
        &obid,
        &format!(r#"OBY oby_tell_job {{"id":"{id}","words":"SQLite"}}"#),
    );
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("waits for your OK"),
        "{answer}"
    );
    let job = get_job(&obid, &id);
    assert_eq!(job["permission"]["id"], permission, "{job}");
    assert_eq!(job["status"], "running", "{job}");

    let secret = ["vault-", "question/", "Z9k4+a=b"].concat();
    vault_holds(&obid, &secret);
    let post = |answer: &str| {
        call(
            &obid,
            "POST",
            &format!("/v1/jobs/{id}/permission"),
            Some(&json!({"id": permission, "answer": answer})),
        )
    };
    // A vault that can't be read passes nothing on, and the question still waits.
    std::fs::write(obid.home.join("vault/secrets.json"), "not the vault").unwrap();
    let refused = post("SQLite");
    assert_eq!(refused.status, 503, "{}", refused.text);
    vault_holds(&obid, &secret);
    assert_eq!(get_job(&obid, &id)["permission"]["id"], permission);
    let answered = post(&format!("Use the key {secret}"));
    assert_eq!(answered.status, 200, "{}", answered.text);
    let done = call(&obid, "GET", &format!("/v1/jobs/{id}/wait"), None).body;
    let summary = done["summary"].as_str().unwrap();
    assert!(summary.starts_with("answer: allow answers:"), "{done}");
    assert!(!summary.contains("Z9k4"), "{summary}");
}

/// What the fake agent of job `id` reported it heard last (`REPORT`).
fn reported(obid: &Obid, id: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(
        obid.home
            .join("worktrees")
            .join(id)
            .join(".obi/report.json"),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// #542: telling a running job something passes the user's words to its agent at once, with no
/// Confirm item waiting.
#[test]
fn the_user_tells_a_running_job_something() {
    let folder = Folder::new("brain-message-running");
    let obid = started_harness_jobs(&folder);
    let answer = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research",
                             "task": "PAUSE: 3000", "title": "Vault secrets"}}),
        ),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    within(60, "its agent is at work", || {
        let job = get_job(&obid, &id);
        (job["status"] == "running" && job["step"] == "Starting Claude Code").then_some(())
    });
    let words = format!(r#"OBY oby_tell_job {{"id":"{id}","words":"REPORT"}}"#);
    let answer = said(&obid, &words);
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("Told Vault secrets what you said."),
        "{answer}"
    );
    let done = call(&obid, "GET", &format!("/v1/jobs/{id}/wait"), None).body;
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(done["round"], 1);
    assert!(
        reported(&obid, &id)["prompt"]
            .as_str()
            .unwrap()
            .contains("REPORT")
    );
    // A running job hears the words at once: nothing waits for a Confirm.
    assert!(items_of(&obid, "confirm").is_empty());
}

/// Sangam's TaskView: words said in a task page's box (`POST /v1/jobs/{id}/messages` with
/// `"conversation": true`) go on the end of the job's conversation, Oby's answer after them with
/// the follow-up's card; files go with a follow-up only, and only from obid's attachments folder.
#[test]
fn words_from_a_task_page_join_its_conversation() {
    let folder = Folder::new("brain-message-page");
    let obid = started_harness_jobs(&folder);
    let answer = said(
        &obid,
        r#"OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"investigate","title":"Docs line","task":"Look around"}"#,
    );
    let conversation = answer["conversation"].as_str().unwrap().to_owned();
    let id = answer["jobs"][0]["id"].as_str().unwrap().to_owned();
    within(120, "the job is done", || {
        (get_job(&obid, &id)["status"] == "done").then_some(())
    });
    let post = |body: serde_json::Value| {
        call(
            &obid,
            "POST",
            &format!("/v1/jobs/{id}/messages"),
            Some(&body),
        )
    };

    let outside = post(json!({"text": "and this", "attachments": ["/etc/hosts"]}));
    assert_eq!(outside.status, 422, "{}", outside.text);
    let attached = obid.home.join("attachments/shot.png");
    std::fs::create_dir_all(attached.parent().unwrap()).unwrap();
    std::fs::write(&attached, b"png").unwrap();
    let path = attached.to_str().unwrap();
    let followed =
        post(json!({"text": "add one more line", "conversation": true, "attachments": [path]}));
    assert_eq!(followed.status, 201, "{}", followed.text);
    let next = followed.body["id"].as_str().unwrap().to_owned();
    assert_eq!(followed.body["followUpOf"], json!(id));
    let input = obid
        .home
        .join(format!("worktrees/{next}/.obi/inputs/shot.png"));
    within(120, "the follow-up has the file", || {
        input.is_file().then_some(())
    });
    let turns = call(
        &obid,
        "GET",
        &format!("/v1/conversation?id={conversation}"),
        None,
    )
    .body;
    let turns = turns.as_array().unwrap();
    let (you, oby) = (&turns[turns.len() - 2], &turns[turns.len() - 1]);
    assert_eq!(
        (you["who"].clone(), you["text"].clone()),
        (json!("user"), json!("add one more line"))
    );
    assert_eq!(oby["who"], "oby");
    assert_eq!(
        oby["text"],
        "This one’s finished, so I started a follow-up. Claude Code has it in fixture-app on Stub."
    );
    assert_eq!(oby["jobs"], json!([next]));
    // Try again picks its own session up: no new job, so no files.
    let again = post(json!({"text": obid::brain::actions::TRY_AGAIN, "attachments": [path]}));
    assert_eq!(again.status, 422, "{}", again.text);
    // Without "conversation": true, nothing joins the conversation, whatever the words do.
    post(json!({"text": "and one more", "conversation": false}));
    let after = call(
        &obid,
        "GET",
        &format!("/v1/conversation?id={conversation}"),
        None,
    )
    .body;
    assert_eq!(after.as_array().unwrap().len(), turns.len(), "{after}");

    // A job at work takes words, never files.
    let running = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "investigate", "task": "PAUSE: 60000", "title": "Slow"}}),
        ),
    );
    assert_eq!(running.status, 201, "{}", running.text);
    let slow = running.body["id"].as_str().unwrap().to_owned();
    within(120, "its agent is at work", || {
        let job = get_job(&obid, &slow);
        (job["status"] == "running" && job["step"] == "Starting Claude Code").then_some(())
    });
    let busy = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{slow}/messages"),
        Some(&json!({"text": "look at this", "attachments": [path]})),
    );
    assert_eq!(busy.status, 422, "{}", busy.text);
    assert!(
        busy.text.contains("Files go with a follow-up"),
        "{}",
        busy.text
    );
}

/// A job the owner let go (Dismiss) is finished too: "tell it…" starts a follow-up, and the job
/// stays let go (a field test, 2026-10-17, kept being offered one more round of a dismissed job
/// that had failed for good).
#[test]
fn a_dismissed_job_gets_a_follow_up_too() {
    let folder = Folder::new("brain-message-dismissed");
    let obid = started_harness_jobs(&folder);
    let job = job_that(&obid, "Docs line", "FAIL", None, "interrupted");
    let id = job["id"].as_str().unwrap().to_owned();
    let dismissed = call(&obid, "POST", &format!("/v1/jobs/{id}/dismiss"), None);
    assert_eq!(dismissed.status, 200, "{}", dismissed.text);
    let words = format!(r#"OBY oby_tell_job {{"id":"{id}","words":"Add one more line"}}"#);
    let answer = said(&obid, &words);
    let cards = answer["jobs"].as_array().unwrap();
    assert_eq!(cards.len(), 1, "{answer}");
    let next = get_job(&obid, cards[0]["id"].as_str().unwrap());
    assert_eq!(next["followUpOf"], json!(id));
    assert_eq!(next["request"], words);
    let first = get_job(&obid, &id);
    assert_eq!(first["status"], "interrupted");
    assert!(first["dismissed"].is_string(), "{first}");
    assert!(items_of(&obid, "confirm").is_empty());
}

/// The owner's decision of 2026-10-18 (Sangam's design leads; it replaced #644's one more round
/// in the job's own worktree): telling a job that ended something starts a follow-up job at once,
/// with the same kind, agent, account, repository and conversation, the owner's words as its
/// request (a secret obid holds taken out), and what the first job did quoted in its first prompt.
/// The first job stays as it ended. Oby's answer names the follow-up and carries its card, and
/// nothing waits for a Confirm.
#[test]
fn telling_a_finished_job_something_starts_a_follow_up() {
    let folder = Folder::new("brain-message-done");
    let obid = started_harness_jobs(&folder);
    let job = job_that(&obid, "Vault secrets", "Nothing", None, "done");
    let id = job["id"].as_str().unwrap().to_owned();
    let token = std::fs::read_to_string(obid.home.join("api-token")).unwrap();
    let token = token.trim();
    let words =
        format!(r#"OBY oby_tell_job {{"id":"{id}","words":"REPORT and {token} isn't yours"}}"#);
    let answer = said_anew(&obid, &words);
    let cards = answer["jobs"].as_array().unwrap();
    assert_eq!(cards.len(), 1, "{answer}");
    let next = cards[0]["id"].as_str().unwrap().to_owned();
    assert_ne!(next, id);
    // Titled as a follow-up, never with the words themselves.
    let title = cards[0]["title"].as_str().unwrap();
    assert_eq!(title, "Follow-up: Vault secrets");
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("started a follow-up"),
        "{answer}"
    );
    assert!(items_of(&obid, "confirm").is_empty());
    assert!(items_of(&obid, "start").is_empty());
    let follow = within(30, "the follow-up ended", || {
        let job = get_job(&obid, &next);
        (job["status"] == "done").then_some(job)
    });
    assert_eq!(follow["followUpOf"], json!(id));
    // Its parent isn't this conversation's (POST /v1/jobs), so neither is it: no turn here names it.
    assert!(follow["turn"].is_null(), "{follow}");
    for key in [
        "kind",
        "agent",
        "account",
        "repo",
        "base",
        "mode",
        "conversation",
    ] {
        assert_eq!(follow[key], job[key], "{key}");
    }
    assert_ne!(follow["branch"], job["branch"]);
    assert_ne!(follow["worktree"], job["worktree"]);
    let request = follow["request"].as_str().unwrap();
    assert!(
        request.contains("[redacted: ") && !request.contains(token),
        "{request}"
    );
    // The first job stays as it ended.
    let first = get_job(&obid, &id);
    assert_eq!(
        (first["status"].clone(), first["round"].clone()),
        (json!("done"), json!(1))
    );
    assert!(first["notes"].is_null(), "{first}");
    assert!(first["followUpOf"].is_null(), "{first}");
    // Its first prompt: the owner's words, then what the first job did, quoted.
    let prompt = reported(&obid, &next)["prompt"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!prompt.contains(token));
    assert!(
        prompt.contains(&format!("What the user asked for: {request}")),
        "{prompt}"
    );
    let (_, earlier) = prompt
        .split_once("This follows up an earlier job")
        .expect("what the first job did is in the prompt");
    let open = earlier
        .lines()
        .find(|l| l.starts_with("<<<quoted-"))
        .expect("quoted");
    let quoted = earlier.split_once(&format!("{open}\n")).unwrap().1;
    assert!(
        quoted.contains("Vault secrets") && quoted.contains("Nothing to do."),
        "{quoted}"
    );
}

/// A turn that looked something up (a lookup reads the web, GitHub or a repository: data, never
/// the owner's words) starts the follow-up of a job that ended, marked untrusted (the owner's
/// decision of 2026-10-23, replacing #374's Start item).
#[test]
fn a_follow_up_after_research_starts_marked_untrusted() {
    let folder = Folder::new("brain-follow-up-research");
    let obid = started_harness_jobs(&folder);
    let job = job_that(&obid, "Vault secrets", "Nothing", None, "done");
    let id = job["id"].as_str().unwrap().to_owned();
    let words = format!(r#"WEB OBY oby_tell_job {{"id":"{id}","words":"add a test"}}"#);
    let answer = said(&obid, &words);
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("started a follow-up"),
        "{answer}"
    );
    assert!(items_of(&obid, "start").is_empty());
    let next = answer["jobs"][0]["id"].as_str().expect("its card");
    let next = get_job(&obid, next);
    assert_eq!(next["followUpOf"], json!(id));
    assert_eq!(next["untrusted"], true, "{next}");
    assert_eq!(next["request"], words);
}

/// A turn that looked something up can't carry a job past its safety limit either: its follow-up
/// would start the counters again, so it waits for the owner's Confirm, and nothing starts.
#[test]
fn research_cant_carry_a_job_past_its_safety_limit() {
    let folder = Folder::new("brain-research-safety");
    let mut spent = ended(
        "b0d6e7a1",
        "Spent it all",
        obid::job::Status::Interrupted,
        0,
        None,
    );
    spent.stalled = Some(obid::job::Stalled {
        why: "budget".into(),
        text: "the task used 1000 tokens".into(),
        at: obi_protocol::time::now(),
        round: 1,
        branch: spent.branch.clone(),
        commit: None,
        commits: 0,
        session: true,
        note: String::new(),
        note_by: "obid".into(),
    });
    spent.limit = Some("the task used 1000 tokens".into());
    let obid = started_with(&folder, &[spent]);
    let answer = said(
        &obid,
        r#"WEB OBY oby_tell_job {"id":"b0d6e7a1","words":"go on"}"#,
    );
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("waits for your Confirm in the inbox"),
        "{answer}"
    );
    let confirms = items_of(&obid, "confirm");
    assert_eq!(confirms.len(), 1, "{confirms:?}");
    assert_eq!(confirms[0]["title"], "Start a follow-up of Spent it all");
    assert_eq!(
        call(&obid, "GET", "/v1/jobs", None)
            .body
            .as_array()
            .unwrap()
            .len(),
        1,
        "nothing started"
    );
}

/// What a job can't do says why, and fails.
#[test]
fn what_a_job_can_t_do_says_why() {
    let folder = Folder::new("brain-cant");
    let obid = started_harness_jobs(&folder);
    let vault = running(&obid, "Vault secrets")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let answer = said(
        &obid,
        &format!(r#"OBY oby_continue_job {{"id":"{vault}"}}"#),
    );
    assert_eq!(
        answer["text"],
        "oby: refused: Couldn't continue Vault secrets: It's still running: tell it something with oby_tell_job."
    );
    let answer = said(&obid, &format!(r#"OBY oby_archive_job {{"id":"{vault}"}}"#));
    assert_eq!(
        answer["text"],
        "oby: refused: Couldn't archive Vault secrets: It's still running: stop it first, or archive it once it ends."
    );
}

/// Answering an inbox item that Oby may answer (`Deny` on a permission) settles it in the turn.
#[test]
fn answering_an_inbox_item_settles_it() {
    let folder = Folder::new("brain-inbox");
    let obid = started_harness_jobs(&folder);
    let ask = r#"ASKS: [{"tool": "Bash", "input": {"command": "make lint"}}]"#;
    let job = job_that(&obid, "Vault secrets", ask, Some("ask"), "running");
    let id = job["id"].as_str().unwrap().to_owned();
    let item = within(30, "a permission waits", || {
        call(&obid, "GET", "/v1/inbox", None)
            .body
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["job"] == id.as_str() && i["text"] == "Run make lint")
            .cloned()
    });
    let item_id = item["id"].as_str().unwrap().to_owned();
    let answer = said(
        &obid,
        &format!(r#"OBY oby_answer {{"item":"{item_id}","answer":"deny"}}"#),
    );
    assert_eq!(answer["text"], "oby: Answered Vault secrets's item: Deny.");
    let settled = call(&obid, "GET", "/v1/inbox?state=done", None).body;
    assert!(
        settled
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == item_id.as_str() && i["answer"] == "Deny")
    );
}

/// NoDispatchCardTests.aPlanStartsItsJobsAndLinksThemToTheTurn: a plan's jobs start as it's made,
/// with their issue's files, joined to Oby's turn, and Oby says what started.
#[test]
fn a_plan_starts_its_jobs_and_links_them_to_the_turn() {
    let folder = Folder::new("brain-no-card");
    let obid = started_harness_jobs(&folder);
    let words = r#"OBY oby_dispatch {"repo":"obi-e2e/fixture-app","issue":176,"kind":"fix","title":"Login screen","task":"Fix #176"}"#;
    let answer = said(&obid, words);
    let jobs = answer["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1, "{answer}");
    assert_eq!(
        jobs[0]["files"],
        json!([
            "Engine/Accounts.swift",
            "Engine/Review.swift",
            "RunnerTests/"
        ])
    );
    let id = jobs[0]["id"].as_str().unwrap();
    let turns = call(&obid, "GET", "/v1/conversation", None).body;
    assert_eq!(
        turns.as_array().unwrap().last().unwrap()["jobs"],
        json!([id])
    );
    assert_eq!(jobs[0]["turn"], turn_of(&obid, words));
    assert_eq!(call(&obid, "GET", "/v1/inbox", None).body, json!([]));
}

/// #630: the reviews the chat plans only read, in Plan mode, even when the default is Bypass;
/// other work gets the default, investigations included: they may run a test to reproduce a bug
/// (the owner's decision of 2026-10-18).
#[test]
fn the_chats_reviews_and_investigations_run_in_plan() {
    let folder = Folder::new("brain-read-only");
    let obid = started_harness_jobs(&folder);
    let put = call(
        &obid,
        "PUT",
        "/v1/settings/permissions",
        Some(&json!({"mode":"bypass"})),
    );
    assert_eq!(put.status, 200, "{}", put.text);
    for (kind, expected) in [
        ("review", "plan"),
        ("investigate", "bypass"),
        ("fix", "bypass"),
    ] {
        let words = format!(
            r#"OBY oby_dispatch {{"repo":"obi-e2e/fixture-app","kind":"{kind}","task":"Check the greeting"}}"#
        );
        let answer = said(&obid, &words);
        let id = answer["jobs"][0]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("{answer}"));
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        assert_eq!(job["kind"], kind, "{job}");
        assert_eq!(job["mode"], expected, "{job}");
    }
}

/// NoDispatchCardTests.aCancelledOrLateTurnStartsNothing: a turn cancelled while its jobs' issues
/// were read starts nothing.
#[test]
fn a_turn_cancelled_while_its_jobs_are_made_starts_nothing() {
    let folder = Folder::new("brain-cancel-files");
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    let obid = start(&folder.0, &config, Some(harness.parent().unwrap()));
    let events = follow_events(&obid);
    let answer = std::thread::scope(|scope| {
        let asking = scope.spawn(|| {
            said(
                &obid,
                r#"OBY oby_dispatch {"repo":"obi-e2e/fixture-app","issue":777,"kind":"fix","title":"Slow issue","task":"Fix #777"}"#,
            )
        });
        events_named(&events, "step", 10, |got| {
            got.iter()
                .any(|e| e["tool"] == "oby_dispatch" && e["state"] == "running")
        });
        // The fake gh takes two seconds to read issue 777.
        std::thread::sleep(std::time::Duration::from_millis(300));
        call(&obid, "POST", "/v1/plans/cancel", None);
        asking.join().unwrap()
    });
    assert_eq!(answer["kind"], "none", "{answer}");
    assert_eq!(call(&obid, "GET", "/v1/jobs", None).body, json!([]));
}

// ---------------------------------------------------------------------------------------------
// Answering as soon as Oby knows (#592): a thinking line per brain call, then the reply.

/// The `thinking` events as `index:state:text`.
fn thoughts(events: &[serde_json::Value]) -> Vec<String> {
    events
        .iter()
        .map(|e| {
            format!(
                "{}:{}:{}",
                e["index"],
                e["state"].as_str().unwrap(),
                e["text"].as_str().unwrap()
            )
        })
        .collect()
}

/// #592's acceptance: however long the brain thinks, the turn says so within 500 ms of
/// `POST /v1/plans`, and the line is done when the plan comes.
#[test]
fn a_turn_says_it_is_thinking_within_half_a_second() {
    let folder = Folder::new("brain-thinking-first");
    let obid = started_harness(&folder);
    let events = follow_events(&obid);
    let answer = std::thread::scope(|scope| {
        let begun = std::time::Instant::now();
        let asking = scope.spawn(|| said(&obid, "LINGER 2000"));
        let first = events_named(&events, "thinking", 5, |got| !got.is_empty());
        let took = begun.elapsed();
        assert!(
            took < std::time::Duration::from_millis(500),
            "the first thinking event came after {took:?}"
        );
        assert_eq!(thoughts(&first), ["1:running:Thinking"]);
        assert!(first[0]["at"].is_string());
        asking.join().unwrap()
    });
    assert_eq!(answer["text"], "waited");
    let done = events_named(&events, "thinking", 5, |got| !got.is_empty());
    assert_eq!(thoughts(&done), ["1:done:Thinking"]);
    let turn = turn_of(&obid, "LINGER 2000");
    assert_eq!(done[0]["turn"], turn.as_str());
    assert_eq!(done[0]["conversation"], answer["conversation"]);
}

/// #592's acceptance: a turn the user cancels while its brain thinks says nothing: its thinking
/// line fails and no reply goes out.
#[test]
fn a_cancelled_turn_sends_no_reply() {
    let folder = Folder::new("brain-cancel-no-reply");
    let obid = started_harness(&folder);
    let events = follow_events(&obid);
    let answer = std::thread::scope(|scope| {
        let asking = scope.spawn(|| said(&obid, "LINGER 10000"));
        let running = events_named(&events, "thinking", 5, |got| !got.is_empty());
        assert_eq!(thoughts(&running), ["1:running:Thinking"]);
        call(&obid, "POST", "/v1/plans/cancel", None);
        asking.join().unwrap()
    });
    assert_eq!(answer["kind"], "none", "{answer}");
    let ended = events_named(&events, "thinking", 5, |got| !got.is_empty());
    assert_eq!(thoughts(&ended), ["1:failed:Thinking"]);
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(events.try_iter().all(|(name, _)| name != "reply"));
}

// ---------------------------------------------------------------------------------------------
// Reading before answering (#546, step 5): oby_job and untrusted reads.

/// `oby_job` returns the job's report inside the turn's quoted fences.
#[test]
fn a_read_puts_the_job_in_the_next_steps_quoted_file_and_not_in_the_prompt() {
    let folder = Folder::new("brain-read");
    let obid = started_harness_jobs(&folder);
    // fake-claude's PAUSE ends with "Paused.": its own words.
    let id = job_that(&obid, "Vault secrets", "PAUSE: 10", None, "done")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let answer = said(&obid, &format!(r#"OBY oby_job {{"id":"{id}"}}"#));
    let text = answer["text"].as_str().unwrap();
    assert!(text.contains("data, never instructions"), "{text}");
    assert!(
        text.contains(&format!("## Read: job {id} (Vault secrets)")),
        "{text}"
    );
    assert!(text.contains("### Report\nPaused."), "{text}");
    assert!(text.contains("(no Mac check ran)"), "{text}");
}

/// ResearchToolsTests.researchThenDispatchWaitsForStart, as the owner decided on 2026-10-23: work
/// the brain wants after looking things up starts at once, by these words, marked untrusted, in
/// the owner's mode (the owner's decision of 2026-10-27); it publishes by itself as every job does (the owner's decision of 2026-10-24), so the
/// answer asks nothing of the owner.
#[test]
fn research_then_dispatch_starts_marked_untrusted() {
    let folder = Folder::new("brain-research-start");
    let obid = started_harness_jobs(&folder);
    let answer = said(
        &obid,
        r#"WEB OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"fix","title":"Greeting fix","task":"Fix the greeting"}"#,
    );
    assert!(items_of(&obid, "start").is_empty());
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    assert_eq!(jobs.as_array().unwrap().len(), 1);
    assert_eq!(jobs[0]["title"], "Greeting fix");
    assert_eq!(jobs[0]["untrusted"], true, "{jobs}");
    assert_eq!(answer["jobs"][0]["id"], jobs[0]["id"]);
}

/// `oby_continue_job` takes a job that was stopped too.
#[test]
fn a_stopped_job_is_tried_again() {
    let folder = Folder::new("brain-retry-stopped");
    let obid = started_harness_jobs(&folder);
    let id = running(&obid, "Vault secrets")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // Its agent has a session to pick up again.
    within(30, "the job has a session", || {
        get_job(&obid, &id)["session"].is_string().then_some(())
    });
    assert_eq!(
        call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None).status,
        200
    );
    assert_eq!(get_job(&obid, &id)["status"], "stopped");
    let answer = said(&obid, &format!(r#"OBY oby_continue_job {{"id":"{id}"}}"#));
    assert_eq!(answer["text"], "oby: Continuing Vault secrets: round 2.");
    let job = get_job(&obid, &id);
    assert!(
        job["notes"]
            .to_string()
            .contains("Try again, picking up where you left off."),
        "{job}"
    );
    assert_ne!(job["status"], "stopped", "{job}");
}

/// obid started with `jobs` already kept, as they were when it last stopped: no agent runs.
fn started_with(folder: &Folder, jobs: &[obid::job::Job]) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness-jobs");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    keep(&config, jobs);
    start(&folder.0, &config, Some(harness.parent().unwrap()))
}

/// Writes `jobs` where obid keeps them, for its next start.
fn keep(config: &serde_json::Value, jobs: &[obid::job::Job]) {
    let dir = Path::new(config["home"].as_str().unwrap()).join("jobs");
    std::fs::create_dir_all(&dir).unwrap();
    for job in jobs {
        std::fs::write(
            dir.join(format!("{}.json", job.id)),
            serde_json::to_vec_pretty(job).unwrap(),
        )
        .unwrap();
    }
}

/// A job that ended `status` over a day ago with `commits`, started in `conversation`: too long
/// ago for a failure record, which the brain would read quoted, boxed.
fn ended(
    id: &str,
    title: &str,
    status: obid::job::Status,
    commits: u64,
    conversation: Option<&str>,
) -> obid::job::Job {
    let mut job = obid::job::Job::new(
        id.into(),
        "obi-e2e/fixture-app".into(),
        "fix".into(),
        title.into(),
        "claude".into(),
        "stub".into(),
    );
    let now = obi_protocol::time::now();
    job.status = status;
    job.mode = Some("sandbox".into());
    job.branch = format!("oby/fix-{id}");
    job.base = Some("main".into());
    job.commits = (commits > 0).then_some(commits);
    job.created = now - 90_000;
    job.started = Some(now - 90_000);
    job.finished = Some(now - 89_000);
    job.conversation = conversation.map(str::to_owned);
    job
}

/// #631: `oby_jobs` marks jobs started in another conversation, and continuing or changing another
/// conversation's job waits for the owner's Confirm.
#[test]
fn another_conversations_job_waits_for_the_owners_confirm() {
    let folder = Folder::new("brain-other-conversation");
    let theirs = ended(
        "c0ffee00",
        "Vault tests",
        obid::job::Status::Failed,
        0,
        Some("another-conversation"),
    );
    let obid = started_with(&folder, &[theirs]);
    let listed = said(&obid, "OBY oby_jobs {}");
    let conversation = listed["conversation"].as_str().unwrap().to_owned();
    assert!(
        listed["text"]
            .as_str()
            .unwrap()
            .contains("\"thisConversation\": false"),
        "{listed}"
    );
    let plan = |words: &str| {
        let answer = call(
            &obid,
            "POST",
            "/v1/plans",
            Some(&json!({"said": words, "conversation": conversation})),
        );
        assert_eq!(answer.status, 200, "{}", answer.text);
        answer.body["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{}", answer.text))
            .to_owned()
    };
    assert_eq!(
        plan(r#"OBY oby_continue_job {"id":"c0ffee00"}"#),
        "oby: Continue Vault tests waits for your OK: confirm it in your inbox."
    );
    assert_eq!(get_job(&obid, "c0ffee00")["status"], "failed");
    let answer = |title: &str, answer: &str| {
        let confirms = items_of(&obid, "confirm");
        let item = confirms
            .iter()
            .find(|i| i["title"] == title && i["done"].is_null())
            .unwrap_or_else(|| panic!("{confirms:?}"));
        assert_eq!(item["job"], "c0ffee00");
        let answered = owner_answers(&obid, item["id"].as_str().unwrap(), answer);
        assert_eq!(answered.status, 200, "{}", answered.text);
    };
    answer("Continue Vault tests", "Cancel");
    assert_eq!(get_job(&obid, "c0ffee00")["status"], "failed");
    assert_eq!(
        plan(r#"OBY oby_change_job {"id":"c0ffee00","mode":"plan"}"#),
        "oby: Put Vault tests in Plan mode waits for your OK: confirm it in your inbox."
    );
    assert_eq!(get_job(&obid, "c0ffee00")["mode"], "sandbox");
    answer("Put Vault tests in Plan mode", "Confirm");
    assert_eq!(get_job(&obid, "c0ffee00")["mode"], "plan");
}

/// `oby_jobs` marks a job this conversation started, and `oby_archive_job` archives it at once.
#[test]
fn the_briefing_marks_this_conversations_jobs() {
    let folder = Folder::new("brain-own-conversation");
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness-jobs");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    let obid = start(&folder.0, &config, Some(harness.parent().unwrap()));
    let conversation = said(&obid, "SAY Fine.")["conversation"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(obid);
    let mut ours = ended(
        "deadbeef",
        "Vault tests",
        obid::job::Status::Failed,
        0,
        Some(&conversation),
    );
    ours.dismissed = Some(obi_protocol::time::now());
    keep(&config, &[ours]);
    let obid = start(&folder.0, &config, Some(harness.parent().unwrap()));
    let listed = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "OBY oby_jobs {}", "conversation": conversation})),
    );
    assert!(
        listed.body["text"]
            .as_str()
            .unwrap()
            .contains("\"thisConversation\": true"),
        "{}",
        listed.text
    );
    let answer = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(
            &json!({"said": r#"OBY oby_archive_job {"id":"deadbeef"}"#, "conversation": conversation}),
        ),
    );
    assert_eq!(
        answer.body["text"], "oby: Archived Vault tests.",
        "{}",
        answer.text
    );
}

// ---------------------------------------------------------------------------------------------
// The inbox the brain may and may not answer (the owner's decision of 2026-10-16, review on
// #552): what persists or widens trust waits for the owner's own Confirm.

/// A conversation's words that aren't the owner's own signed word grant no permission and save no
/// allow rule: each is a Confirm the owner answers.
#[test]
fn only_the_owner_may_allow_a_job_permission() {
    let folder = Folder::new("brain-inbox-always");
    let obid = started_harness_jobs(&folder);
    let ask = r#"ASKS: [{"tool": "Bash", "input": {"command": "make lint"}}]"#;
    let id = job_that(&obid, "Vault secrets", ask, Some("ask"), "running")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let item = within(30, "a permission waits", || {
        items_of(&obid, "permission")
            .into_iter()
            .find(|i| i["job"] == id.as_str())
    });
    let item_id = item["id"].as_str().unwrap();
    // Words no app of the owner's signed: a Confirm of the answer, the permission still waiting
    // (TASKCONV-9).
    for answer in ["Allow", "Always allow"] {
        let said = said(
            &obid,
            &format!(r#"OBY oby_answer {{"item":"{item_id}","answer":"{answer}"}}"#),
        );
        assert!(
            said["text"].as_str().unwrap().contains("waits for your OK"),
            "{said}"
        );
        assert!(
            items_of(&obid, "permission")
                .iter()
                .any(|i| i["id"] == item_id)
        );
        assert_eq!(
            call(&obid, "GET", "/v1/permissions/rules", None).body,
            json!([])
        );
    }
    let allowed = owner_answers(&obid, item_id, "Allow");
    assert_eq!(allowed.status, 200, "{}", allowed.text);
}

/// A job in Plan mode doesn't approve its own plan (#609): Claude Code's `ExitPlanMode` waits as
/// a `plan` item holding the plan, which the brain never answers. Keep planning sends the
/// owner's words back; Approve goes on in Ask mode.
#[test]
fn a_plan_waits_for_the_owners_approval() {
    let folder = Folder::new("brain-plan-approval");
    let obid = started_harness_jobs(&folder);
    let ask = r#"ASKS: [{"tool": "ExitPlanMode", "input": {"plan": "1. Add src/average.js"}}, {"tool": "ExitPlanMode", "input": {"plan": "1. Add src/average.js\n2. Test an empty list"}}]"#;
    let id = job_in(&obid, "Average", ask, "plan");
    let first = within(30, "the plan waits", || {
        items_of(&obid, "plan")
            .into_iter()
            .find(|i| i["job"] == id.as_str())
    });
    let first_id = first["id"].as_str().unwrap().to_owned();
    assert_eq!(first["options"], json!(["Approve", "Keep planning"]));
    assert_eq!(first["details"], json!(["1. Add src/average.js"]));
    // The inbox has it a moment before the job shows it (Jobs::show).
    let job = within(10, "the job shows its plan", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["step"] == "Waiting for your OK on the plan").then_some(job)
    });
    assert_eq!(job["status"], "running", "{job}");
    // The brain answers it neither way.
    for option in ["Approve", "Keep planning"] {
        let words = format!(r#"OBY oby_answer {{"item":"{first_id}","answer":"{option}"}}"#);
        said(&obid, &words);
        assert!(
            items_of(&obid, "plan")
                .iter()
                .any(|i| i["id"] == first_id.as_str()),
            "the brain answered option {option}"
        );
    }
    // Keep planning, in the owner's words: the agent hears them and plans again.
    let kept = owner_answers(&obid, &first_id, "Cover an empty list too");
    assert_eq!(kept.status, 200, "{}", kept.text);
    let second = within(30, "the new plan waits", || {
        items_of(&obid, "plan")
            .into_iter()
            .find(|i| i["job"] == id.as_str() && i["id"] != first_id.as_str())
    });
    assert_eq!(
        second["details"],
        json!(["1. Add src/average.js\n2. Test an empty list"])
    );
    let approved = owner_answers(&obid, second["id"].as_str().unwrap(), "Approve");
    assert_eq!(approved.status, 200, "{}", approved.text);
    let job = within(30, "the job ends", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["status"] == "done").then_some(job)
    });
    assert_eq!(
        job["summary"],
        "answers: deny: Cover an empty list too, allow"
    );
    assert_eq!(job["mode"], "sandbox", "the owner's default mode");
}

/// The review's probe: answering another Obi's request with "Accept, and remember for this
/// repo" saves no decision and starts no job; a confirm item waits, and the owner's Confirm does
/// both. A plain Decline is done in the turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn accepting_another_obis_request_waits_for_the_owners_confirm() {
    let folder = Folder::new("brain-inbox-obi");
    let sphere = FakeSphere::start().await;
    let obid = started_harness_jobs(&folder);
    let wellington = Owner::new("wellington");
    let joined = tokio::task::block_in_place(|| join_a(&obid, &sphere, &wellington));
    assert_eq!(joined["state"], "joined");
    let ernesto = TestObi::join(&sphere, "obi-ernesto", "ernesto").await;
    // A plain Decline is one-off: done in the turn.
    let first = ernesto.request("obi-wellington", "review_request", brief(), 3600);
    ernesto.send(&first).await;
    tokio::task::block_in_place(|| {
        let item = within(20, "the request waits", || {
            items_of(&obid, "obiRequest").pop()
        });
        let item_id = item["id"].as_str().unwrap().to_owned();
        let answer = said(
            &obid,
            &format!(r#"OBY oby_answer {{"item":"{item_id}","answer":"Decline"}}"#),
        );
        assert_eq!(
            answer["text"],
            "oby: Answered Review request from ernesto's item: Decline."
        );
        assert!(items_of(&obid, "obiRequest").is_empty());
        assert!(items_of(&obid, "confirm").is_empty());
        assert_eq!(
            call(&obid, "GET", "/v1/obid/decisions", None).body,
            json!([])
        );
    });
    let request = ernesto.request("obi-wellington", "review_request", brief(), 3600);
    ernesto.send(&request).await;
    tokio::task::block_in_place(|| {
        let item = within(20, "the request waits", || {
            items_of(&obid, "obiRequest").pop()
        });
        let item_id = item["id"].as_str().unwrap().to_owned();
        let answer = said(
            &obid,
            &format!(
                r#"OBY oby_answer {{"item":"{item_id}","answer":"Accept, and remember for this repo"}}"#
            ),
        );
        assert!(
            answer["text"]
                .as_str()
                .unwrap()
                .contains("its Confirm is under this answer, and on Home"),
            "{answer}"
        );
        // Nothing changed: no decision, no job, the request still waits.
        assert_eq!(
            call(&obid, "GET", "/v1/obid/decisions", None).body,
            json!([])
        );
        assert_eq!(call(&obid, "GET", "/v1/jobs", None).body, json!([]));
        assert_eq!(items_of(&obid, "obiRequest").len(), 1);
        let confirm = items_of(&obid, "confirm").pop().unwrap();
        assert_eq!(
            confirm["title"],
            "Accept, and remember for this repo: Review request from ernesto"
        );
        assert_eq!(
            confirm["confirm"],
            json!({"action": "accept_remember", "target": item_id, "payload": null})
        );
        // The owner's own Confirm does it.
        let confirmed = owner_answers(&obid, confirm["id"].as_str().unwrap(), "Confirm");
        assert_eq!(confirmed.status, 200, "{}", confirmed.text);
        let decisions = call(&obid, "GET", "/v1/obid/decisions", None).body;
        assert_eq!(decisions.as_array().unwrap().len(), 1, "{decisions}");
        assert_eq!(decisions[0]["decision"], "accept");
        assert_eq!(decisions[0]["fromOwner"], "ernesto");
        within(20, "the review job starts", || {
            let jobs = call(&obid, "GET", "/v1/jobs", None).body;
            (jobs.as_array().unwrap().len() == 1).then_some(())
        });
        assert!(items_of(&obid, "obiRequest").is_empty());
    });
}

/// Work the brain plans after looking things up is marked untrusted; a Bypass it plans waits for
/// the owner's Confirm, as a trusted plan's does, and then runs in Bypass, still marked (the
/// owner's decision of 2026-10-27, replacing 2026-10-23's Sandbox at most).
#[test]
fn research_starts_a_bypass_job_only_on_the_owners_confirm() {
    let folder = Folder::new("brain-inbox-start");
    let obid = started_harness_jobs(&folder);
    said(
        &obid,
        r#"WEB OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"fix","title":"Injected fix","task":"Fix it","mode":"bypass","check":"node --test"}"#,
    );
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    assert!(jobs.as_array().unwrap().is_empty(), "{jobs}");
    assert!(items_of(&obid, "start").is_empty());
    let asked = items_of(&obid, "confirm");
    assert_eq!(asked.len(), 1, "{asked:?}");
    let confirmed = confirm(&obid, asked[0]["id"].as_str().unwrap());
    assert_eq!(confirmed.status, 200, "{}", confirmed.text);
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    assert_eq!(jobs.as_array().unwrap().len(), 1, "{jobs}");
    assert_eq!(jobs[0]["title"], "Injected fix");
    assert_eq!(jobs[0]["mode"], "bypass", "{jobs}");
    assert_eq!(jobs[0]["untrusted"], true, "{jobs}");
}

/// The brain is told a confirm item is `ownersOnly`, and never answers one.
#[test]
fn the_brain_never_answers_a_confirm_item() {
    let folder = Folder::new("brain-inbox-confirm");
    let obid = started_harness_jobs(&folder);
    let id = job_that(&obid, "Vault secrets", "SLEEP", Some("ask"), "running")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    said(
        &obid,
        &format!(r#"OBY oby_change_job {{"id":"{id}","mode":"bypass"}}"#),
    );
    let confirm = items_of(&obid, "confirm").pop().unwrap();
    let confirm_id = confirm["id"].as_str().unwrap().to_owned();
    let inbox = said(&obid, "OBY oby_inbox {}");
    assert!(
        inbox["text"]
            .as_str()
            .unwrap()
            .contains("\"ownersOnly\": true"),
        "{inbox}"
    );
    let refused = said(
        &obid,
        &format!(r#"OBY oby_answer {{"item":"{confirm_id}","answer":"Confirm"}}"#),
    );
    assert!(
        refused["text"]
            .as_str()
            .unwrap()
            .contains("waits for your Confirm on its card"),
        "{refused}"
    );
    assert_eq!(get_job(&obid, &id)["mode"], "ask");
    assert_eq!(items_of(&obid, "confirm").len(), 1);
}

/// #586: `sandbox` stops a job asking the owner about what stays inside its sandbox, so it widens
/// trust. `oby_dispatch` cannot name `sandbox` as a mode, and `oby_change_job` from `ask` to
/// `sandbox` waits for the owner's Confirm rather than taking effect in the turn.
#[test]
fn the_brain_cannot_set_sandbox_mode() {
    let folder = Folder::new("brain-no-sandbox");
    let obid = started_harness_jobs(&folder);
    let id = job_in(&obid, "Vault secrets", "SLEEP", "ask");
    within(30, "the job runs", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["status"] == "running").then_some(())
    });
    let dispatch = said(
        &obid,
        r#"OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"fix","title":"Greeting fix","task":"Fix the greeting","check":"node --test","mode":"sandbox"}"#,
    );
    assert!(
        dispatch["text"]
            .as_str()
            .unwrap()
            .contains("mode is ask, acceptEdits, plan or bypass"),
        "{dispatch}"
    );
    let change = said(
        &obid,
        &format!(r#"OBY oby_change_job {{"id":"{id}","mode":"sandbox"}}"#),
    );
    assert!(
        change["text"]
            .as_str()
            .unwrap()
            .contains("waits for your OK"),
        "{change}"
    );
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["mode"], "ask", "{job}");
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    assert_eq!(jobs.as_array().unwrap().len(), 1, "{jobs}");
}

/// #586: the brain never lets a secret through, not even once. "Allow" in chat leaves the
/// permission waiting for the owner's own answer in the inbox, and the agent hears nothing.
#[test]
fn the_brain_never_lets_a_secret_through() {
    let folder = Folder::new("brain-no-secret");
    let obid = started_harness_jobs(&folder);
    let id = job_in(
        &obid,
        "Key reader",
        r#"ASK: {"tool": "Bash", "input": {"command": "cat ~/.ssh/id_rsa"}}"#,
        "sandbox",
    );
    let permission = within(30, "a permission waits", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        job["permission"]["id"].as_str().map(str::to_owned)
    });
    let item = within(30, "the permission item is in the inbox", || {
        items_of(&obid, "permission").pop()
    });
    let item_id = item["id"].as_str().unwrap();
    let answer = said(
        &obid,
        &format!(r#"OBY oby_answer {{"item":"{item_id}","answer":"Allow"}}"#),
    );
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("it touches secrets: it waits for your click on its card"),
        "{answer}"
    );
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["permission"]["id"], permission.as_str(), "{job}");
    assert_eq!(job["status"], "running");
    assert_eq!(items_of(&obid, "permission").len(), 1);
}

/// Re-review of #586, S2: the brain can't settle a secret by changing the job's mode. After it
/// sets Ask, Plan and Accept edits, the card still waits and the agent heard nothing.
#[test]
fn the_brain_cannot_settle_a_secret_by_changing_the_mode() {
    let folder = Folder::new("brain-secret-mode");
    let obid = started_harness_jobs(&folder);
    let id = job_in(
        &obid,
        "Env reader",
        // A read: Ask, Plan and Accept edits all let reads through without asking.
        r#"ASK: {"tool": "Read", "input": {"file_path": ".env"}}"#,
        "sandbox",
    );
    let permission = within(30, "a permission waits", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        job["permission"]["id"].as_str().map(str::to_owned)
    });
    for mode in ["ask", "plan"] {
        said(
            &obid,
            &format!(r#"OBY oby_change_job {{"id":"{id}","mode":"{mode}"}}"#),
        );
    }
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["mode"], "plan", "{job}");
    assert_eq!(job["permission"]["id"], permission.as_str(), "{job}");
    assert_eq!(job["status"], "running");
    assert_eq!(job["summary"], "", "the agent heard an answer: {job}");
}

/// An app's Try again, after an answer it lost, sends the same words with the same `id`: obid has
/// them, adds nothing and starts nothing, wherever the app sends them. The user's turn takes the
/// id the app gave it, so the app's own copy of the words gives way to it; an id that isn't a
/// lower-case UUID is refused.
#[test]
fn the_same_words_sent_again_with_their_id_are_kept_once() {
    let folder = Folder::new("brain-repeat-id");
    let obid = started_harness(&folder);
    let calls = || {
        records(&obid)
            .into_iter()
            .filter(|r| r["kind"] == "conversation")
            .count()
    };
    let id = "0b1e0c0a-0000-4000-8000-000000000001";
    let first = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "SAY Ready", "id": id})),
    );
    assert_eq!(first.status, 200, "{}", first.text);
    assert_eq!(first.body["text"], "Ready");
    let conversation = first.body["conversation"].as_str().unwrap().to_owned();
    let again = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "SAY Ready", "id": id, "new": true})),
    );
    assert_eq!(again.status, 200, "{}", again.text);
    assert_eq!(
        (
            &again.body["kind"],
            &again.body["repeated"],
            &again.body["conversation"]
        ),
        (&json!("none"), &json!(true), &json!(conversation)),
        "{}",
        again.text
    );
    assert_eq!(calls(), 1, "the brain ran once");
    let page = call(
        &obid,
        "GET",
        &format!("/v1/conversation?id={conversation}"),
        None,
    )
    .body;
    let users: Vec<&serde_json::Value> = page
        .as_array()
        .unwrap_or_else(|| panic!("{page}"))
        .iter()
        .filter(|t| t["who"] == "user")
        .collect();
    assert_eq!(users.len(), 1, "{page}");
    assert_eq!(users[0]["id"], id);
    let bad = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "SAY Ready", "id": "not-a-uuid"})),
    );
    assert_eq!(bad.status, 422, "{}", bad.text);
    // The id with other words, or in another conversation, isn't these words again: refused.
    let other = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "goodbye", "id": id, "conversation": conversation})),
    );
    assert_eq!(other.status, 409, "{}", other.text);
    let elsewhere = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "elsewhere", "new": true})),
    );
    assert_eq!(elsewhere.status, 200, "{}", elsewhere.text);
    let elsewhere = elsewhere.body["conversation"].as_str().unwrap().to_owned();
    let moved = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "SAY Ready", "id": id, "conversation": elsewhere})),
    );
    assert_eq!(moved.status, 409, "{}", moved.text);
    assert_eq!(calls(), 2, "only the new conversation's words ran");
}

/// Claude Code's thinking shows while it goes on, though its stream carries no thinking text: a
/// `thought` entry that's `running`, then `done` with how long it took.
#[test]
fn a_jobs_thinking_shows_with_no_words() {
    let folder = Folder::new("thinking");
    let obid = started_harness_jobs(&folder);
    let answer = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research",
                             "task": "THINKING", "title": "Thinker"}}),
        ),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    let ended = call(&obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(ended.status, 200, "{}", ended.text);
    let transcript = call(&obid, "GET", &format!("/v1/jobs/{id}/transcript"), None);
    let thought = transcript.body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "thought")
        .cloned()
        .unwrap_or_else(|| panic!("no thought: {}", transcript.text));
    assert_eq!(thought["summary"], "Thinking", "{thought}");
    assert_eq!(thought["state"], "done", "{thought}");
    assert!(thought["seconds"].is_number(), "{thought}");
    assert!(
        thought.get("text").is_none_or(serde_json::Value::is_null),
        "{thought}"
    );
}

/// #784: what a job's call printed is kept for its transcript (opened per entry), and never
/// reaches the brain: not its prompt, its arguments or its quoted file, even while it reads about
/// that very job.
#[test]
fn a_calls_output_never_reaches_the_brain() {
    let folder = Folder::new("brain-detail");
    let obid = started_harness_jobs(&folder);
    let answer = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research",
                             "task": "DETAIL", "title": "Lister"}}),
        ),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    let ended = call(&obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(ended.status, 200, "{}", ended.text);
    let canary = "DETAIL-ONLY-CANARY-7f3a";
    let opened = call(&obid, "GET", &format!("/v1/jobs/{id}/transcript/3"), None);
    assert!(opened.text.contains(canary), "{}", opened.text);
    let answer = said(&obid, &format!(r#"OBY oby_job {{"id":"{id}"}}"#));
    assert!(!answer.to_string().contains(canary), "{answer}");
    let conversation = answer["conversation"].as_str().unwrap();
    let prompt = std::fs::read_to_string(
        obid.home
            .join(format!("conversations/{conversation}/config/turns.jsonl")),
    )
    .unwrap();
    assert!(
        !prompt.contains(canary),
        "job tool output reached the conversation prompt"
    );
}

/// #776: only the owner's tap confirms. A Confirm with the API token alone is refused with the
/// title to sign and leaves the item open; a registered device's signature over this item's own
/// title carries it out once; another item's signature, or a publish signature over the same
/// words, doesn't pass; Cancel and later need no signature.
#[test]
fn a_confirm_needs_the_owners_tap_on_a_device() {
    use base64::Engine as _;
    use p256::ecdsa::signature::Signer as _;
    let folder = Folder::new("brain-confirm-tap");
    let mut first = ended(
        "c1c1c1c1",
        "Average helper empty lists",
        obid::job::Status::Done,
        2,
        None,
    );
    first.publish_error = Some("Not published: the secret scan couldn't run.".into());
    let mut second = ended("c2c2c2c2", "Retry jitter", obid::job::Status::Done, 2, None);
    second.publish_error = Some("Not published: the secret scan couldn't run.".into());
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    keep(&config, &[first, second]);
    let obid = start(&folder.0, &config, None);
    for job in ["c1c1c1c1", "c2c2c2c2"] {
        said(&obid, &format!(r#"OBY oby_delete_job {{"id":"{job}"}}"#));
    }
    let confirms = items_of(&obid, "confirm");
    assert_eq!(confirms.len(), 2, "{confirms:?}");
    let item_of = |job: &str| confirms.iter().find(|i| i["job"] == job).unwrap().clone();
    let (a, b) = (item_of("c1c1c1c1"), item_of("c2c2c2c2"));
    let (a_id, b_id) = (a["id"].as_str().unwrap(), b["id"].as_str().unwrap());
    let answer = |item: &str, body: serde_json::Value| {
        call(
            &obid,
            "POST",
            &format!("/v1/inbox/{item}/answer"),
            Some(&body),
        )
    };

    // The token alone: refused, with the title to sign, and the item stays open.
    let refused = answer(a_id, json!({"answer": "Confirm"}));
    assert_eq!(refused.status, 403, "{}", refused.text);
    assert_eq!(refused.body["action"], a["title"]);
    assert!(refused.text.contains("Only your tap"), "{}", refused.text);
    assert_eq!(items_of(&obid, "confirm").len(), 2);
    assert!(get_job(&obid, "c1c1c1c1")["dismissed"].is_null());

    // Another item's approval doesn't confirm this one.
    let for_b = confirm_approval(&obid, b_id);
    let wrong = answer(a_id, json!({"answer": "Confirm", "approval": for_b}));
    assert_eq!(wrong.status, 403, "{}", wrong.text);
    assert!(get_job(&obid, "c1c1c1c1")["dismissed"].is_null());

    // A publish signature by the same device over the same words isn't a Confirm.
    let mut publish = confirm_approval(&obid, a_id);
    let secret = p256::SecretKey::from_slice(&[42; 32]).unwrap();
    let signed_at = publish["signedAt"].as_str().unwrap().to_owned();
    let title = a["title"].as_str().unwrap();
    let signature: p256::ecdsa::Signature = p256::ecdsa::SigningKey::from(secret)
        .sign(format!("oby-publish\n{a_id}\n{title}\n{signed_at}").as_bytes());
    publish["signature"] =
        json!(base64::engine::general_purpose::STANDARD.encode(signature.to_der()));
    let wrong = answer(a_id, json!({"answer": "Confirm", "approval": publish}));
    assert_eq!(wrong.status, 403, "{}", wrong.text);
    assert!(get_job(&obid, "c1c1c1c1")["dismissed"].is_null());

    // The owner's tap: carried out once, and the same approval can't do it again.
    let tap = confirm_approval(&obid, a_id);
    let confirmed = answer(a_id, json!({"answer": "Confirm", "approval": tap.clone()}));
    assert_eq!(confirmed.status, 200, "{}", confirmed.text);
    assert_eq!(call(&obid, "GET", "/v1/jobs/c1c1c1c1", None).status, 404);
    let again = answer(a_id, json!({"answer": "Confirm", "approval": tap}));
    assert_ne!(again.status, 200, "{}", again.text);

    // An approval with anything but Confirm is refused; later and Cancel need none.
    let odd = answer(
        b_id,
        json!({"answer": "Cancel", "approval": confirm_approval(&obid, b_id)}),
    );
    assert_eq!(odd.status, 422, "{}", odd.text);
    let later = answer(b_id, json!({"later": "2099-01-01T00:00:00Z"}));
    assert_eq!(later.status, 200, "{}", later.text);
    let cancelled = answer(b_id, json!({"answer": "Cancel"}));
    assert_eq!(cancelled.status, 200, "{}", cancelled.text);
    assert!(get_job(&obid, "c2c2c2c2")["dismissed"].is_null());
}

// ---------------------------------------------------------------------------------------------
// Every task has a conversation (docs/contracts/task-conversations.md; Wellington, 2026-10-30).

/// The conversation `id`'s turns.
fn conversation_turns(obid: &Obid, id: &str) -> Vec<serde_json::Value> {
    let answer = call(obid, "GET", &format!("/v1/conversation?id={id}"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body.as_array().unwrap().clone()
}

/// A task from the Tasks view (`POST /v1/jobs`), finished.
fn task_from_the_tasks_view(obid: &Obid, title: &str) -> (String, String) {
    let answer = call(
        obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "investigate", "task": "Look around the fixture", "title": title}}),
        ),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    let conversation = answer.body["conversation"]
        .as_str()
        .expect("a task from the Tasks view has a conversation")
        .to_owned();
    within(120, "the job is done", || {
        (get_job(obid, &id)["status"] == "done").then_some(())
    });
    (id, conversation)
}

/// TASKCONV-1: a task started from the Tasks view gets a conversation of its own, named after it:
/// the owner's words that started it, then Oby's line with the task's card.
#[test]
fn taskconv_1_a_task_from_the_tasks_view_gets_its_own_conversation() {
    let folder = Folder::new("taskconv-1");
    let obid = started_harness_jobs(&folder);
    let (id, conversation) = task_from_the_tasks_view(&obid, "Fix CI leccoflow #1");
    let listed = call(&obid, "GET", "/v1/conversations", None).body;
    let listed = listed.as_array().unwrap();
    let it = listed
        .iter()
        .find(|c| c["id"] == conversation.as_str())
        .unwrap_or_else(|| panic!("{conversation} isn't listed: {listed:?}"));
    assert_eq!(it["title"], "Fix CI leccoflow #1");
    let turns = conversation_turns(&obid, &conversation);
    assert_eq!(turns[0]["who"], "user");
    assert_eq!(turns[0]["text"], "Look around the fixture");
    let oby = turns
        .iter()
        .find(|t| t["who"] == "oby")
        .expect("Oby's line");
    assert!(
        oby["text"].as_str().unwrap().starts_with("On it. "),
        "{oby}"
    );
    assert_eq!(oby["jobs"], json!([id]));
    // its card shows under the words that started it
    assert_eq!(get_job(&obid, &id)["turn"], turns[0]["id"]);
}

/// TASKCONV-3: words to a finished task from its page start a follow-up in the same conversation,
/// with the words and Oby's answer on its end: never a task of its own, never lost.
#[test]
fn taskconv_3_a_follow_up_joins_its_tasks_conversation() {
    let folder = Folder::new("taskconv-3");
    let obid = started_harness_jobs(&folder);
    let (id, conversation) = task_from_the_tasks_view(&obid, "Fix CI leccoflow #1");
    let before = call(&obid, "GET", "/v1/conversations", None)
        .body
        .as_array()
        .unwrap()
        .len();
    let followed = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": "why wasn't this merged?", "conversation": true})),
    );
    assert_eq!(followed.status, 201, "{}", followed.text);
    let next = followed.body["id"].as_str().unwrap().to_owned();
    assert_eq!(followed.body["followUpOf"], json!(id));
    assert_eq!(followed.body["conversation"], json!(conversation));
    let turns = conversation_turns(&obid, &conversation);
    let (you, oby) = (&turns[turns.len() - 2], &turns[turns.len() - 1]);
    assert_eq!(
        (you["who"].clone(), you["text"].clone()),
        (json!("user"), json!("why wasn't this merged?"))
    );
    assert_eq!(
        (oby["who"].clone(), oby["jobs"].clone()),
        (json!("oby"), json!([next]))
    );
    // and no other conversation was made for it
    let after = call(&obid, "GET", "/v1/conversations", None)
        .body
        .as_array()
        .unwrap()
        .len();
    assert_eq!(after, before);
}

/// TASKCONV-4: a task with no conversation (one kept from before every task had one; here, its
/// conversation deleted) gets one the first time its page's words reach it, and the follow-up
/// they start is in it too.
#[test]
fn taskconv_4_a_task_with_no_conversation_gets_one_when_told_something() {
    let folder = Folder::new("taskconv-4");
    let obid = started_harness_jobs(&folder);
    let (id, gone) = task_from_the_tasks_view(&obid, "Old task");
    let deleted = call(&obid, "DELETE", &format!("/v1/conversations/{gone}"), None);
    assert!(deleted.status < 300, "{}", deleted.text);
    let followed = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": "why wasn't this merged?", "conversation": true})),
    );
    assert_eq!(followed.status, 201, "{}", followed.text);
    let next = followed.body["id"].as_str().unwrap().to_owned();
    let conversation = get_job(&obid, &id)["conversation"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(conversation, gone);
    assert_eq!(get_job(&obid, &next)["conversation"], json!(conversation));
    let turns = conversation_turns(&obid, &conversation);
    // Oby's line naming the task, then the words and Oby's answer with the follow-up's card
    assert_eq!(turns[0]["jobs"], json!([id]));
    let (you, oby) = (&turns[turns.len() - 2], &turns[turns.len() - 1]);
    assert_eq!(you["text"], "why wasn't this merged?");
    assert_eq!(oby["jobs"], json!([next]));
}

/// TASKCONV-5: a task started in a conversation stays in that one: no second conversation.
#[test]
fn taskconv_5_a_task_from_a_conversation_keeps_it() {
    let folder = Folder::new("taskconv-5");
    let obid = started_harness_jobs(&folder);
    let answer = said(
        &obid,
        r#"OBY oby_dispatch {"repo":"obi-e2e/fixture-app","kind":"investigate","title":"Docs line","task":"Look around"}"#,
    );
    let conversation = answer["conversation"].as_str().unwrap().to_owned();
    let id = answer["jobs"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(get_job(&obid, &id)["conversation"], json!(conversation));
    let listed = call(&obid, "GET", "/v1/conversations", None).body;
    assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
}
