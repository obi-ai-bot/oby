//! Oby's ask for a repository with no check (#1062; docs/runner-api.md, "Job" and `set_check`):
//! the real obid binary with the stand-in agent (tests/agents/fake-claude). The fixture is two
//! repositories on GitHub with no check (git's own `insteadOf` sends their GitHub URLs to it, as a
//! test can't reach GitHub). Their jobs ask once, never waiting for the answer; Edit asks again
//! with the owner's command, Use it keeps it and the next job runs it; No check is remembered.
mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::*;

const APP: &str = "obi-e2e/fixture-app";
const OTHER: &str = "obi-e2e/other";

fn post(obid: &Obid, repo: &str) -> String {
    let job = json!({"repo": repo, "kind": "fix", "task": "Look around"});
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

/// Job `id` once it ended, within 60 seconds.
fn ended(obid: &Obid, id: &str) -> Value {
    let give_up_at = Instant::now() + Duration::from_secs(60);
    loop {
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        if !matches!(job["status"].as_str(), Some("queued" | "running")) {
            return job;
        }
        assert!(Instant::now() < give_up_at, "the job never ended: {job}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The open asks for `repo`'s check.
fn asks(obid: &Obid, repo: &str) -> Vec<Value> {
    let inbox = call(obid, "GET", "/v1/inbox", None).body;
    inbox
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["confirm"]["action"] == "set_check" && i["confirm"]["target"] == repo)
        .cloned()
        .collect()
}

/// What obid keeps of job `id` in its file: `checkAsked` is never in the API.
fn kept(obid: &Obid, id: &str) -> Value {
    serde_json::from_slice(&std::fs::read(obid.home.join(format!("jobs/{id}.json"))).unwrap())
        .unwrap()
}

#[test]
fn a_repository_with_no_check_is_asked_once_and_the_answer_kept() {
    let folder = Folder::new("repo-check");
    let remote = fixture(&folder.0.join("fixture"));
    std::fs::create_dir_all(folder.0.join("env-home")).unwrap();
    std::fs::write(
        folder.0.join("env-home/.gitconfig"),
        format!(
            "[url \"file://{}\"]\n\tinsteadOf = https://github.com/{APP}.git\n\tinsteadOf = https://github.com/{OTHER}.git\n",
            remote.display()
        ),
    )
    .unwrap();
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let mut config = config(&folder.0, &remote, &agent);
    config["repos"] = json!([
        {"repo": APP, "remote": format!("https://github.com/{APP}.git"), "defaultBranch": "main"},
        {"repo": OTHER, "remote": format!("https://github.com/{OTHER}.git"), "defaultBranch": "main"},
    ]);
    let obid = start(&folder.0, &config, None);

    // Two jobs before any answer: one ask, guessed from the fixture's package.json (a `test`
    // script, no lockfile). Neither waits for it: each ends on its own, with no check.
    let first = [post(&obid, APP), post(&obid, APP)];
    for id in &first {
        let job = ended(&obid, id);
        assert_eq!(job["status"], "done", "{job}");
        assert!(job["check"].is_null(), "{job}");
        assert_eq!(kept(&obid, id)["checkAsked"], "npm test");
    }
    let asked = asks(&obid, APP);
    assert_eq!(asked.len(), 1, "{asked:?}");
    let ask = &asked[0];
    assert_eq!(
        ask["title"],
        "Use `npm test` as obi-e2e/fixture-app's check before publishing?"
    );
    assert_eq!(ask["kind"], "confirm");
    assert_eq!(ask["options"], json!(["Confirm", "Cancel"]));
    assert_eq!(ask["confirm"]["payload"], json!({"check": "npm test"}));
    assert!(first.contains(&ask["job"].as_str().unwrap().to_owned()));
    let answer = |id: &str, body: Value| {
        call(
            &obid,
            "POST",
            &format!("/v1/inbox/{id}/answer"),
            Some(&body),
        )
    };

    // Edit: a command that can't be a check is refused; the owner's own is asked again as an item
    // of its own, whose Confirm signs its title. Nothing is kept before that Confirm.
    let id = ask["id"].as_str().unwrap();
    let refused = answer(id, json!({"answer": "make\nrm -rf ~"}));
    assert_eq!(refused.status, 422, "{}", refused.text);
    let edited = answer(id, json!({"answer": " node --test "}));
    assert_eq!(edited.status, 200, "{}", edited.text);
    assert_ne!(edited.body["id"], ask["id"]);
    assert_eq!(
        edited.body["title"],
        "Use `node --test` as obi-e2e/fixture-app's check before publishing?"
    );
    let open: Vec<Value> = asks(&obid, APP).iter().map(|i| i["id"].clone()).collect();
    assert_eq!(open, [edited.body["id"].clone()]);
    let file = || -> Value {
        serde_json::from_slice(&std::fs::read(folder.0.join("config.json")).unwrap()).unwrap()
    };
    assert!(file()["repos"][0]["check"].is_null());
    let unsigned = answer(
        edited.body["id"].as_str().unwrap(),
        json!({"answer": "Confirm"}),
    );
    assert_eq!(unsigned.status, 403, "{}", unsigned.text);

    // Use it, the owner's tap: kept on the repository, and the next job runs it.
    let used = confirm(&obid, edited.body["id"].as_str().unwrap());
    assert_eq!(used.status, 200, "{}", used.text);
    assert_eq!(file()["repos"][0]["check"], "node --test");
    let later = post(&obid, APP);
    let job = ended(&obid, &later);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["check"], "node --test");
    assert!(
        obid.home.join(format!("logs/{later}/check.log")).is_file(),
        "{job}"
    );
    assert!(kept(&obid, &later)["checkAsked"].is_null());
    assert!(asks(&obid, APP).is_empty());

    // No check: remembered, so a later job there never asks again.
    let id = post(&obid, OTHER);
    ended(&obid, &id);
    let asked = asks(&obid, OTHER);
    assert_eq!(asked.len(), 1, "{asked:?}");
    let no = answer(
        asked[0]["id"].as_str().unwrap(),
        json!({"answer": "Cancel"}),
    );
    assert_eq!(no.status, 200, "{}", no.text);
    let settings: Value =
        serde_json::from_slice(&std::fs::read(obid.home.join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings["noCheck"], json!([OTHER]));
    let again = post(&obid, OTHER);
    let job = ended(&obid, &again);
    assert_eq!(job["status"], "done", "{job}");
    assert!(job["check"].is_null(), "{job}");
    assert!(kept(&obid, &again)["checkAsked"].is_null());
    assert!(asks(&obid, OTHER).is_empty());
    assert!(file()["repos"][1]["check"].is_null());
}
