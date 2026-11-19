//! The owner's words to a Codex task at work (docs/contracts/task-conversations.md, TASKCONV-36
//! and 37), through the real obid binary, asked over HTTP, with a scripted stand-in for
//! `codex app-server` (tests/agents/fake-codex, `STEPS`): they go into the turn under way with
//! `turn/steer`, and words Codex refuses as its turn ends are the thread's next turn, in the same
//! Codex process, never a Codex started again for them.

mod support;

use std::path::{Path, PathBuf};

use serde_json::json;
use support::*;

fn agents() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents")
}

/// obid with the fixture repository and one Codex account (the stand-in), its quiet window 3 s.
fn obid(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-claude"));
    config["stallAfter"] = json!(3);
    let accounts = folder.0.join("home/accounts");
    config["agents"] = json!({
        "codex": {"bin": agents().join("fake-codex"),
                  "accounts": [{"id": "work", "name": "Work", "configDir": accounts.join("codex-work")}]}
    });
    start_with(&folder.0, &config, None, &[])
}

/// A Codex job on `task`, once its agent is at work: its first turn logged.
fn at_work(obid: &Obid, task: &str) -> (String, PathBuf) {
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "research", "check": "true",
                     "agent": "codex", "task": task});
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    let log = within(30, "its first turn", || {
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        let log = Path::new(job["worktree"].as_str()?).join(".obi/turns.log");
        log.exists().then_some(log)
    });
    (id, log)
}

fn tell(obid: &Obid, id: &str, text: &str) {
    let answer = call_typed(
        obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({ "text": text }),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
}

/// Each turn the stand-in ran: the pid of its process and its prompt's first line.
fn turns(log: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| {
            let (pid, prompt) = line.split_once(' ').unwrap_or((line, ""));
            (pid.to_owned(), prompt.to_owned())
        })
        .collect()
}

/// TASKCONV-36: words to a Codex task at work reach its turn under way, at its next step, by
/// `turn/steer`: the turn answers them, with no turn and no Codex started again for them.
#[test]
fn taskconv_36_words_reach_a_codex_job_in_the_turn_under_way() {
    let folder = Folder::new("taskconv-36");
    let obid = obid(&folder);
    let (id, log) = at_work(&obid, "STEPS: 60 250");
    tell(&obid, &id, "use SQLite");
    let job = ended_within(&obid, &id, 240);
    assert_eq!(job["status"], "done", "{job}");
    let summary = job["summary"].as_str().unwrap_or_default();
    assert!(summary.contains("heard: use SQLite"), "{job}");
    assert_eq!(turns(&log).len(), 1, "{:?}", turns(&log));
}

/// TASKCONV-37: words Codex refused because its turn was ending are the thread's next turn, in
/// the same Codex process: one process ran both turns, and the second heard the words.
#[test]
fn taskconv_37_words_codex_refused_as_its_turn_ended_are_its_next_turn_in_the_same_process() {
    let folder = Folder::new("taskconv-37");
    let obid = obid(&folder);
    let (id, log) = at_work(&obid, "STEPS: 40 250 REFUSE STEER");
    tell(&obid, &id, "use SQLite");
    let job = ended_within(&obid, &id, 240);
    assert_eq!(job["status"], "done", "{job}");
    let turns = turns(&log);
    assert_eq!(turns.len(), 2, "{turns:?}");
    assert_eq!(turns[0].0, turns[1].0, "one Codex process: {turns:?}");
    assert!(turns[1].1.contains("use SQLite"), "{turns:?}");
    let summary = job["summary"].as_str().unwrap_or_default();
    assert!(summary.contains("heard: use SQLite"), "{job}");
}
