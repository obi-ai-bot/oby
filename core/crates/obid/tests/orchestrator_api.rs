//! The event orchestrator (#590): what happens to a job goes to a turn nobody watches on the
//! conversation harness, which reads the job, then takes one allowed action or leaves the owner one
//! decision. The agent is a stand-in (tests/agents/fake-harness): what the event turn does is the
//! script in its room's supervise.txt.
mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

fn harness() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness")
}

/// obid with the stand-in as Claude Code, signed in, and events brought together for one second.
fn started(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &harness());
    config["orchestratorDelay"] = json!(1);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    start(&folder.0, &config, None)
}

/// What the event turn does: one line per step, as the stand-in reads supervise.txt.
fn script(obid: &Obid, lines: &[&str]) {
    let room = obid.home.join("conversations/orchestrator/room");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::write(room.join("supervise.txt"), lines.join("\n") + "\n").unwrap();
}

fn job(obid: &Obid, task: &str, title: &str) -> String {
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "investigate", "task": task,
                     "title": title, "rounds": 1});
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({"job": job})));
    assert!(
        answer.status == 200 || answer.status == 201,
        "{}",
        answer.text
    );
    answer.body["id"].as_str().unwrap().to_owned()
}

/// A job whose agent fails its first turn (a retry's "Try again" doesn't hold the word).
fn failing(obid: &Obid) -> String {
    job(obid, "JOB-FAILS", "Flaky thing")
}

/// A job whose agent keeps working, quiet, for two minutes.
fn working(obid: &Obid) -> String {
    job(obid, "STALLS-ALWAYS", "Another thing")
}

fn get(obid: &Obid, id: &str) -> Value {
    call(obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

/// The orchestrator's open items in the inbox.
fn escalations(obid: &Obid) -> Vec<Value> {
    call(obid, "GET", "/v1/inbox", None)
        .body
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i["id"].as_str().unwrap_or("").starts_with("orchestrator-"))
        .collect()
}

/// What the event turns answered, as obid logged them.
fn answered(obid: &Obid) -> Vec<String> {
    obid.log()
        .lines()
        .filter_map(|l| l.split_once(" events): ").map(|(_, said)| said.to_owned()))
        .collect()
}

/// A job that fails is read, then tried again: one allowed action and nothing else, and the job
/// recovers. Its run is kept as the brain's, an `event` record.
#[test]
fn a_failed_job_is_read_then_retried_once() {
    let folder = Folder::new("orchestrator-retry");
    let obid = started(&folder);
    script(
        &obid,
        &[
            r#"OBY oby_job {"id": "$EVENTJOB"}"#,
            r#"OBY oby_continue_job {"id": "$EVENTJOB"}"#,
            "SAY Tried it again.",
        ],
    );
    let id = failing(&obid);
    within(60, "the retried job is done", || {
        (get(&obid, &id)["status"] == "done").then_some(())
    });
    assert_eq!(get(&obid, &id)["round"], 2);
    assert!(escalations(&obid).is_empty());
    within(10, "the turn's answer", || {
        answered(&obid)
            .iter()
            .any(|a| a == "Tried it again.")
            .then_some(())
    });
    let records = std::fs::read_to_string(obid.home.join("brain-turns.jsonl")).unwrap();
    let event: Value = records
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|r| r["kind"] == "event")
        .expect("the event turn's record");
    assert_eq!(event["conversation"], "orchestrator");
    assert_eq!(event["calls"][0]["adapter"], "claude");
    assert!(event.get("said").is_none(), "{event}");
}

/// An event no session can answer is left for the owner, saying why.
#[test]
fn an_event_the_brain_cant_answer_escalates_saying_why() {
    let folder = Folder::new("orchestrator-brain-failed");
    let obid = started(&folder);
    script(&obid, &["FAIL"]);
    let id = failing(&obid);
    let item = within(60, "it escalates", || escalations(&obid).pop());
    assert!(
        item["text"].as_str().unwrap().starts_with(
            "Oby couldn't work out what to do here: its agent session failed twice. Oby recommends that you look at the task."
        ),
        "{item}"
    );
    assert_eq!(get(&obid, &id)["round"], 1);
}

/// An event turn reads only its event's jobs, and an action on another job leaves the event for
/// the owner and changes nothing.
#[test]
fn an_event_it_cant_match_escalates_and_changes_nothing() {
    let folder = Folder::new("orchestrator-unmatched");
    let obid = started(&folder);
    let other = working(&obid);
    let read_other = format!(r#"OBY oby_job {{"id": "{other}"}}"#);
    let retry_other = format!(r#"OBY oby_continue_job {{"id": "{other}"}}"#);
    script(
        &obid,
        &[
            &read_other,
            r#"OBY oby_job {"id": "$EVENTJOB"}"#,
            &retry_other,
            r#"OBY oby_continue_job {"id": "$EVENTJOB"}"#,
            "SAY Nothing more.",
        ],
    );
    let id = failing(&obid);
    let item = within(60, "it escalates", || escalations(&obid).pop());
    assert!(
        item["text"]
            .as_str()
            .unwrap()
            .starts_with("Oby couldn't match what it would do to what happened."),
        "{item}"
    );
    within(10, "the turn ends", || {
        (!answered(&obid).is_empty()).then_some(())
    });
    // Once it's the owner's, the turn's own job isn't tried again either.
    let job = get(&obid, &id);
    assert_ne!(job["status"], "done", "{job}");
    assert_eq!(job["round"], 1);
    assert_eq!(get(&obid, &other)["round"], 1);
    assert_eq!(escalations(&obid).len(), 1, "one item a turn");
    // The owner settles it, and it goes (#710: it answered "settled already" and stayed).
    assert_eq!(item["options"], json!(["Done"]));
    let path = format!("/v1/inbox/{}/answer", item["id"].as_str().unwrap());
    let settled = call(&obid, "POST", &path, Some(&json!({"done": true})));
    assert_eq!(settled.status, 200, "{}", settled.text);
    assert!(escalations(&obid).is_empty());
}

/// Telling a job that ended something starts a follow-up (the owner's decision of 2026-10-18),
/// which only the owner's own words start: an event turn's message to one escalates, and nothing
/// starts.
#[test]
fn an_event_turn_never_starts_a_follow_up() {
    let folder = Folder::new("orchestrator-follow-up");
    let obid = started(&folder);
    script(
        &obid,
        &[
            r#"OBY oby_job {"id": "$EVENTJOB"}"#,
            r#"OBY oby_tell_job {"id": "$EVENTJOB", "words": "Rerun the flaky test."}"#,
            "SAY Told it.",
        ],
    );
    let id = failing(&obid);
    let item = within(60, "it escalates", || escalations(&obid).pop());
    let text = item["text"].as_str().unwrap();
    assert!(text.contains("only your own words start"), "{item}");
    assert!(text.contains("Rerun the flaky test."), "{item}");
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    assert_eq!(jobs.as_array().unwrap().len(), 1, "{jobs}");
    assert_eq!(get(&obid, &id)["round"], 1);
}

/// What the owner decides is left for them with the turn's diagnosis and recommendation, titled
/// by the task's name, never its id (TASKS-14); a tool off the allow-list is refused, and the turn
/// lists only the allow-list.
#[test]
fn tasks_14_an_event_turn_escalates_with_its_diagnosis_and_calls_only_its_tools() {
    let folder = Folder::new("orchestrator-escalates");
    let obid = started(&folder);
    script(
        &obid,
        &[
            r#"OBY oby_job {"id": "$EVENTJOB"}"#,
            r#"OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Start over."}"#,
            r#"OBY oby_stop_job {"id": "$EVENTJOB"}"#,
            r#"OBY oby_escalate {"why": "It needs a secret it doesn't have.", "recommend": "Add the secret"}"#,
            r#"OBY oby_continue_job {"id": "$EVENTJOB"}"#,
            "TOOLS",
        ],
    );
    let id = failing(&obid);
    let item = within(60, "it escalates", || escalations(&obid).pop());
    assert!(
        item["text"].as_str().unwrap().starts_with(
            "Oby looked at it. It needs a secret it doesn't have. Oby recommends: Add the secret."
        ),
        "{item}"
    );
    assert_eq!(item["title"], "Flaky thing needs your decision", "{item}");
    assert!(!item["text"].as_str().unwrap().contains(&id), "{item}");
    let tools = within(10, "the turn's answer", || {
        answered(&obid)
            .into_iter()
            .find(|a| a.starts_with("tools: "))
    });
    let mut names: Vec<_> = tools.strip_prefix("tools: ").unwrap().split(',').collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "oby_answer",
            "oby_continue_job",
            "oby_escalate",
            "oby_job",
            "oby_tell_job"
        ]
    );
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    assert_eq!(jobs.as_array().unwrap().len(), 1, "{jobs}");
    // Left for the owner, the job isn't tried again in the same turn.
    assert_eq!(get(&obid, &id)["round"], 1);
    assert_eq!(escalations(&obid).len(), 1);
}

/// The user's words preempt an event turn: it stops, and acts on nothing.
#[test]
fn a_user_message_preempts_an_event_turn() {
    let folder = Folder::new("orchestrator-preempted");
    let obid = started(&folder);
    script(
        &obid,
        &[r#"OBY oby_job {"id": "$EVENTJOB"}"#, "LINGER 8000"],
    );
    let id = failing(&obid);
    let turns = obid
        .home
        .join("conversations/orchestrator/config/turns.jsonl");
    within(60, "the event turn is under way", || {
        turns.is_file().then_some(())
    });
    let answer = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "SAY Fine.", "new": true})),
    );
    assert_eq!(answer.body["text"], "Fine.", "{}", answer.text);
    within(30, "the event turn stops", || {
        obid.log()
            .contains("ended in its session: stopped")
            .then_some(())
    });
    assert_eq!(get(&obid, &id)["round"], 1);
    assert!(escalations(&obid).is_empty());
}
