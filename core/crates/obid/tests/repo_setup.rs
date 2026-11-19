//! A repository's setup from Settings (docs/contracts/setup.md, SETUP-2 and SETUP-3): the real
//! obid binary. `POST /v1/repos/setup` asks for the owner's command with a `set_setup` Confirm,
//! keeps nothing until their signed Confirm, then keeps it in the configuration file, where every
//! later job's run reads it (PUBLISHING-23). Taking it out asks the same way.
mod support;

use std::path::Path;

use serde_json::{Value, json};
use support::*;

const APP: &str = "obi-e2e/fixture-app";

/// SETUP-2: the owner's setup is asked for, kept only on their signed Confirm, listed by
/// `GET /v1/repos` and written to `config.json`; taking it out asks too.
#[test]
fn setup_2_a_setup_from_settings_is_kept_on_the_owners_confirm() {
    let folder = Folder::new("repo-setup");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let mut config = config(&folder.0, &remote, &agent);
    config["repos"] = json!([
        {"repo": APP, "remote": format!("file://{}", remote.display()), "defaultBranch": "main"},
    ]);
    let obid = start(&folder.0, &config, None);
    let change = |setup: Value| {
        call(
            &obid,
            "POST",
            "/v1/repos/setup",
            Some(&json!({"repo": APP, "setup": setup})),
        )
    };
    let file = || -> Value {
        serde_json::from_slice(&std::fs::read(folder.0.join("config.json")).unwrap()).unwrap()
    };
    let listed = || -> Value {
        call(&obid, "GET", "/v1/repos", None).body["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["repo"] == APP)
            .cloned()
            .unwrap()
    };
    // add-only: every repository lists its setup, null while it has none
    assert_eq!(listed()["setup"], Value::Null);

    // refused: a command of two lines, a repository obid doesn't know, a body that isn't one
    assert_eq!(change(json!("npm ci\nrm -rf ~")).status, 422);
    assert_eq!(change(json!(1)).status, 422);
    let unknown = call(
        &obid,
        "POST",
        "/v1/repos/setup",
        Some(&json!({"repo": "someone/else", "setup": "npm ci"})),
    );
    assert_eq!(unknown.status, 422, "{}", unknown.text);

    // asked, nothing kept yet; an unsigned Confirm is refused
    let asked = change(json!(" npm ci "));
    assert_eq!(asked.status, 200, "{}", asked.text);
    let ask = &asked.body["ask"];
    assert_eq!(ask["kind"], "confirm");
    assert_eq!(ask["confirm"]["action"], "set_setup");
    assert_eq!(ask["confirm"]["target"], APP);
    assert_eq!(
        ask["confirm"]["payload"],
        json!({"setup": "npm ci", "was": null})
    );
    assert_eq!(
        ask["title"],
        "Run `npm ci` in each new task's worktree in obi-e2e/fixture-app?"
    );
    assert!(
        ask["text"]
            .as_str()
            .unwrap()
            .contains("on this Mac, as your user"),
        "{ask}"
    );
    assert_eq!(asked.body["repo"]["setup"], Value::Null);
    assert!(file()["repos"][0].get("setup").is_none(), "{}", file());
    let id = ask["id"].as_str().unwrap();
    let unsigned = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{id}/answer"),
        Some(&json!({"answer": "Confirm"})),
    );
    assert_eq!(unsigned.status, 403, "{}", unsigned.text);
    assert_eq!(listed()["setup"], Value::Null);

    // the owner's signed Confirm keeps it, in the file and the list
    let kept = confirm(&obid, id);
    assert_eq!(kept.status, 200, "{}", kept.text);
    assert_eq!(file()["repos"][0]["setup"], "npm ci");
    assert_eq!(listed()["setup"], "npm ci");

    // the same setup again asks nothing
    let same = change(json!("npm ci"));
    assert_eq!(same.status, 200, "{}", same.text);
    assert_eq!(same.body["ask"], Value::Null);

    // taking it out asks too, and the Confirm takes it out of the file
    let out = change(Value::Null);
    assert_eq!(
        out.body["ask"]["title"],
        "Take `npm ci` out as obi-e2e/fixture-app's setup?"
    );
    let taken = confirm(&obid, out.body["ask"]["id"].as_str().unwrap());
    assert_eq!(taken.status, 200, "{}", taken.text);
    assert!(file()["repos"][0].get("setup").is_none(), "{}", file());
    assert_eq!(listed()["setup"], Value::Null);
}

/// SETUP-3: a newer ask for a repository's setup closes the one still open, and the older one's
/// Confirm keeps nothing: only the command the owner was shown last can become the setup.
#[test]
fn setup_3_an_older_ask_keeps_nothing() {
    let folder = Folder::new("repo-setup-older");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let mut config = config(&folder.0, &remote, &agent);
    config["repos"] = json!([
        {"repo": APP, "remote": format!("file://{}", remote.display()), "defaultBranch": "main", "setup": "npm ci"},
    ]);
    let obid = start(&folder.0, &config, None);
    let change = |setup: Value| {
        call(
            &obid,
            "POST",
            "/v1/repos/setup",
            Some(&json!({"repo": APP, "setup": setup})),
        )
    };
    // a setup the configuration names is listed as it is
    let repos = call(&obid, "GET", "/v1/repos", None).body;
    assert_eq!(repos["repos"][0]["setup"], "npm ci");
    let first = change(json!("pnpm install")).body["ask"].clone();
    assert_eq!(
        first["confirm"]["payload"],
        json!({"setup": "pnpm install", "was": "npm ci"})
    );
    assert!(
        first["text"]
            .as_str()
            .unwrap()
            .ends_with("Cancel keeps `npm ci`."),
        "{first}"
    );
    let second = change(json!("yarn")).body["ask"].clone();
    assert_ne!(first["id"], second["id"]);
    let inbox = call(&obid, "GET", "/v1/inbox", None).body;
    let open: Vec<&Value> = inbox
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["confirm"]["action"] == "set_setup" && i["done"].is_null())
        .collect();
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0]["id"], second["id"]);
    let stale = confirm(&obid, first["id"].as_str().unwrap());
    assert_ne!(stale.status, 200, "{}", stale.text);
    let repos = call(&obid, "GET", "/v1/repos", None).body;
    assert_eq!(repos["repos"][0]["setup"], "npm ci");
    assert_eq!(confirm(&obid, second["id"].as_str().unwrap()).status, 200);
    let repos = call(&obid, "GET", "/v1/repos", None).body;
    assert_eq!(repos["repos"][0]["setup"], "yarn");
}
