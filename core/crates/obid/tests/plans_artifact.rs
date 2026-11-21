//! The turn's artifact (docs/contracts/artifacts.md, ARTIFACTS-12), on the real obid binary with
//! the stand-in conversation agent: words said with an artifact open land in its conversation,
//! the agent is told which artifact is open, and an artifact with no conversation gets the one
//! its first words start.

mod support;

use std::path::Path;

use serde_json::{Value, json};
use support::*;

fn started(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    start(&folder.0, &config, None)
}

fn plan(obid: &Obid, body: Value) -> Answer {
    call(obid, "POST", "/v1/plans", Some(&body))
}

/// The prompt the conversation's agent heard last.
fn last_prompt(obid: &Obid, conversation: &str) -> String {
    let recorded = std::fs::read_to_string(
        obid.home
            .join(format!("conversations/{conversation}/config/turns.jsonl")),
    )
    .unwrap();
    let last: Value = serde_json::from_str(recorded.lines().last().unwrap()).unwrap();
    last["prompt"].as_str().unwrap().to_owned()
}

/// An artifact kept with no conversation and no job, of `from`'s kind: one kept from before, or
/// one whose maker is gone. Its page isn't `from`'s, so publishing that page versions it.
fn orphan_of(obid: &Obid, from: &str) -> String {
    let id = "5d6e7f80-1a2b-4c3d-8e9f-0a1b2c3d4e5f";
    let page = "<!doctype html><title>Kept</title><p>kept from before</p>\n";
    let dir = obid.home.join("artifacts");
    let to = dir.join(id);
    std::fs::create_dir_all(to.join("1")).unwrap();
    std::fs::write(to.join("1/index.html"), page).unwrap();
    let mut artifact: Value =
        serde_json::from_slice(&std::fs::read(dir.join(from).join("artifact.json")).unwrap())
            .unwrap();
    artifact["id"] = json!(id);
    artifact["conversation"] = Value::Null;
    artifact["job"] = Value::Null;
    artifact["versions"] = json!([{"version": 1, "created": artifact["created"], "files": ["index.html"], "bytes": page.len()}]);
    std::fs::write(
        to.join("artifact.json"),
        serde_json::to_vec_pretty(&artifact).unwrap(),
    )
    .unwrap();
    id.to_owned()
}

/// ARTIFACTS-12: words said with an artifact open land in its conversation, whatever conversation
/// the app had open, and its agent is told the artifact, its version and its title quoted; the
/// same words without it land where the app sent them. An artifact with no conversation gets the
/// one its first words start, its next words land there, and a job of it versions the artifact.
/// An artifact or a version obid hasn't is 404, `new` with it 422, a key it doesn't know 400.
#[test]
fn artifacts_12_words_with_an_artifact_open_land_in_its_conversation() {
    let folder = Folder::new("plans-artifact");
    let obid = started(&folder);

    let made = plan(
        &obid,
        json!({"said": "MAKEPAGE OBY oby_artifact {\"title\": \"Functions\", \"path\": \"page\"}", "new": true}),
    );
    assert_eq!(made.status, 200, "{}", made.text);
    let c1 = made.body["conversation"].as_str().unwrap().to_owned();
    let artifact = made.body["artifacts"][0]["id"].as_str().unwrap().to_owned();
    let other = plan(&obid, json!({"said": "SAY other", "new": true}));
    let other = other.body["conversation"].as_str().unwrap().to_owned();
    assert_ne!(c1, other);

    // With the artifact open, the words land in its conversation, and its agent is told.
    let open = plan(
        &obid,
        json!({"said": "SAY about the page", "conversation": other, "artifact": {"id": artifact, "version": 1}}),
    );
    assert_eq!(open.status, 200, "{}", open.text);
    assert_eq!(open.body["conversation"], json!(c1));
    let prompt = last_prompt(&obid, &c1);
    assert!(
        prompt.contains(&format!(
            "The owner said these words with artifact {artifact} open (version 1, a html)"
        )),
        "{prompt}"
    );
    assert!(prompt.contains("\nFunctions\nquoted-"), "{prompt}");

    // Without it, where the app sent them, and nothing is said of an artifact.
    let plain = plan(&obid, json!({"said": "SAY plain", "conversation": other}));
    assert_eq!(plain.body["conversation"], json!(other));
    assert!(!last_prompt(&obid, &other).contains("with artifact"));

    // An artifact with no conversation gets the one its first words start.
    let orphan = orphan_of(&obid, &artifact);
    let first = plan(
        &obid,
        json!({"said": "SAY first", "conversation": other, "artifact": {"id": orphan}}),
    );
    assert_eq!(first.status, 200, "{}", first.text);
    let its = first.body["conversation"].as_str().unwrap().to_owned();
    assert!(its != c1 && its != other, "{its}");
    let kept = call(&obid, "GET", &format!("/v1/artifacts/{orphan}"), None);
    assert_eq!(kept.body["conversation"], json!(its));
    let next = plan(
        &obid,
        json!({"said": "SAY next", "conversation": other, "artifact": {"id": orphan}}),
    );
    assert_eq!(next.body["conversation"], json!(its));
    // Its conversation's agent versions it.
    let versioned = plan(
        &obid,
        json!({"said": format!("MAKEPAGE OBY oby_artifact {{\"title\": \"Functions\", \"path\": \"page\", \"artifact\": \"{orphan}\"}}"), "conversation": its}),
    );
    assert_eq!(
        versioned.body["artifacts"][0],
        json!({"id": orphan, "version": 2}),
        "{}",
        versioned.text
    );

    // Refusals, before anything is kept.
    let none = plan(
        &obid,
        json!({"said": "SAY hm", "artifact": {"id": "00000000-0000-4000-8000-000000000000"}}),
    );
    assert_eq!(
        (none.status, none.body["error"].clone()),
        (404, json!("There's no such artifact."))
    );
    let version = plan(
        &obid,
        json!({"said": "SAY hm", "artifact": {"id": artifact, "version": 9}}),
    );
    assert_eq!(
        (version.status, version.body["error"].clone()),
        (404, json!("That artifact has no version 9."))
    );
    let new = plan(
        &obid,
        json!({"said": "SAY hm", "new": true, "artifact": {"id": artifact}}),
    );
    assert_eq!(new.status, 422, "{}", new.text);
    let unknown = plan(
        &obid,
        json!({"said": "SAY hm", "artifact": {"id": artifact, "board": "Main"}}),
    );
    assert_eq!(unknown.status, 400, "{}", unknown.text);
    // None of the refused words is kept, in the artifact's conversation or anywhere: the words it
    // took are there, so the read is one that would show them.
    let kept = call(&obid, "GET", &format!("/v1/conversation?id={c1}"), None);
    assert!(kept.text.contains("SAY about the page"), "{}", kept.text);
    let all = call(&obid, "GET", "/v1/conversations", None);
    assert_eq!(all.status, 200, "{}", all.text);
    for id in [&c1, &other, &its] {
        let one = call(&obid, "GET", &format!("/v1/conversation?id={id}"), None);
        assert!(!one.text.contains("SAY hm"), "{id}: {}", one.text);
    }
}
