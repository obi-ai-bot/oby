//! Oby's ask for a repository with no check (#1062; docs/runner-api.md, "Job" and `set_check`):
//! the real obid binary with the stand-in agent (tests/agents/fake-claude). The fixture is two
//! repositories on GitHub with no check (git's own `insteadOf` sends their GitHub URLs to it, as a
//! test can't reach GitHub). Their jobs ask once, never waiting for the answer; the owner's own
//! command (`POST /v1/repos/check`) is asked in its place, Use it keeps it and the next job runs
//! it; taking it out asks too; No check is remembered until the owner starts over.
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
        assert_eq!(kept(&obid, id)["checkAsked"], true);
    }
    let asked = asks(&obid, APP);
    assert_eq!(asked.len(), 1, "{asked:?}");
    let ask = &asked[0];
    assert_eq!(
        ask["title"],
        "Use `npm test` as obi-e2e/fixture-app's check before publishing?"
    );
    assert!(
        ask["text"]
            .as_str()
            .unwrap()
            .ends_with("No check keeps none, and Oby won't ask again."),
        "{ask}"
    );
    assert_eq!(ask["kind"], "confirm");
    assert_eq!(ask["options"], json!(["Confirm", "Cancel"]));
    assert_eq!(
        ask["confirm"]["payload"],
        json!({"check": "npm test", "was": null})
    );
    assert!(first.contains(&ask["job"].as_str().unwrap().to_owned()));
    let answer = |id: &str, body: Value| {
        call(
            &obid,
            "POST",
            &format!("/v1/inbox/{id}/answer"),
            Some(&body),
        )
    };
    let change = |check: Value| {
        call(
            &obid,
            "POST",
            "/v1/repos/check",
            Some(&json!({"repo": APP, "check": check})),
        )
    };
    let file = || -> Value {
        serde_json::from_slice(&std::fs::read(folder.0.join("config.json")).unwrap()).unwrap()
    };
    let listed = |name: &str| -> Value {
        let repos = call(&obid, "GET", "/v1/repos", None).body;
        repos["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["repo"] == name)
            .cloned()
            .unwrap()
    };

    // The ask takes Confirm or Cancel only: another command goes through the route, which asks
    // for it in place of the open ask. Nothing is kept before the owner's signed Confirm.
    let id = ask["id"].as_str().unwrap();
    let refused = answer(id, json!({"answer": "node --test"}));
    assert_eq!(refused.status, 422, "{}", refused.text);
    let refused = change(json!("make\nrm -rf ~"));
    assert_eq!(refused.status, 422, "{}", refused.text);
    let edited = change(json!(" node --test "));
    assert_eq!(edited.status, 200, "{}", edited.text);
    let next = &edited.body["ask"];
    assert_ne!(next["id"], ask["id"]);
    assert_eq!(
        next["title"],
        "Use `node --test` as obi-e2e/fixture-app's check before publishing?"
    );
    assert_eq!(next["job"], "");
    let open: Vec<Value> = asks(&obid, APP).iter().map(|i| i["id"].clone()).collect();
    assert_eq!(open, [next["id"].clone()]);
    assert!(file()["repos"][0]["check"].is_null());
    assert_eq!(edited.body["repo"]["check"], Value::Null);
    let next = next["id"].as_str().unwrap();
    let unsigned = answer(next, json!({"answer": "Confirm"}));
    assert_eq!(unsigned.status, 403, "{}", unsigned.text);

    // Use it, the owner's tap: kept on the repository, and the next job runs it.
    let used = confirm(&obid, next);
    assert_eq!(used.status, 200, "{}", used.text);
    assert_eq!(file()["repos"][0]["check"], "node --test");
    assert_eq!(
        listed(APP),
        json!({"repo": APP, "defaultBranch": "main", "listed": true, "check": "node --test", "noCheck": false})
    );
    let later = post(&obid, APP);
    let job = ended(&obid, &later);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["check"], "node --test");
    assert!(
        obid.home.join(format!("logs/{later}/check.log")).is_file(),
        "{job}"
    );
    assert_eq!(kept(&obid, &later)["checkAsked"], Value::Null);
    assert!(asks(&obid, APP).is_empty());

    // Taking it out asks too: Cancel keeps it (and isn't No check), Confirm takes it out, and the
    // next job asks again.
    let out = change(Value::Null);
    assert_eq!(out.status, 200, "{}", out.text);
    assert_eq!(
        out.body["ask"]["title"],
        "Take `node --test` out as obi-e2e/fixture-app's check?"
    );
    let kept_it = answer(
        out.body["ask"]["id"].as_str().unwrap(),
        json!({"answer": "Cancel"}),
    );
    assert_eq!(kept_it.status, 200, "{}", kept_it.text);
    assert_eq!(listed(APP)["check"], "node --test");
    assert_eq!(listed(APP)["noCheck"], false);
    let out = change(Value::Null);
    let taken = confirm(&obid, out.body["ask"]["id"].as_str().unwrap());
    assert_eq!(taken.status, 200, "{}", taken.text);
    assert!(file()["repos"][0].get("check").is_none(), "{}", file());
    let again = post(&obid, APP);
    let job = ended(&obid, &again);
    assert!(job["check"].is_null(), "{job}");
    assert_eq!(asks(&obid, APP).len(), 1);

    // No check: remembered, so a later job there never asks again, until the owner starts over.
    let id = post(&obid, OTHER);
    ended(&obid, &id);
    let asked = asks(&obid, OTHER);
    assert_eq!(asked.len(), 1, "{asked:?}");
    let no = answer(
        asked[0]["id"].as_str().unwrap(),
        json!({"answer": "Cancel"}),
    );
    assert_eq!(no.status, 200, "{}", no.text);
    let settings = || -> Value {
        serde_json::from_slice(&std::fs::read(obid.home.join("settings.json")).unwrap()).unwrap()
    };
    assert_eq!(settings()["noCheck"], json!([OTHER]));
    assert_eq!(listed(OTHER)["noCheck"], true);
    let again = post(&obid, OTHER);
    let job = ended(&obid, &again);
    assert_eq!(job["status"], "done", "{job}");
    assert!(job["check"].is_null(), "{job}");
    assert_eq!(kept(&obid, &again)["checkAsked"], Value::Null);
    assert!(asks(&obid, OTHER).is_empty());
    assert!(file()["repos"][1]["check"].is_null());
    let over = call(
        &obid,
        "POST",
        "/v1/repos/check",
        Some(&json!({"repo": OTHER, "check": null})),
    );
    assert_eq!(over.status, 200, "{}", over.text);
    assert_eq!(over.body["ask"], Value::Null);
    assert_eq!(over.body["repo"]["noCheck"], false);
    assert!(settings().get("noCheck").is_none(), "{}", settings());
}
