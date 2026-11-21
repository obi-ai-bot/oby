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

/// [`started`], with jobs run by the stand-in job agent (tests/agents/fake-harness-jobs).
fn started_with_jobs(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness-jobs");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    start(&folder.0, &config, Some(harness.parent().unwrap()))
}

/// A page Oby's own turn made in a new conversation: the conversation and the artifact.
fn made_page(obid: &Obid) -> (String, String) {
    let made = plan(
        obid,
        json!({"said": "MAKEPAGE OBY oby_artifact {\"title\": \"Functions\", \"path\": \"page\"}", "new": true}),
    );
    assert_eq!(made.status, 200, "{}", made.text);
    (
        made.body["conversation"].as_str().unwrap().to_owned(),
        made.body["artifacts"][0]["id"].as_str().unwrap().to_owned(),
    )
}

/// The page's text as `MAKEPAGE` writes it (tests/agents/fake-harness).
const PAGE: &str = "<!doctype html><title>Functions</title><ul><li>greet</li></ul>\n";

/// The task the owner starts with `artifact` open, whose agent edits what it was given
/// (tests/agents/fake-claude, EDITARTIFACT), waited on to its end.
fn edited_with_open(obid: &Obid, artifact: &str) -> Value {
    let started = plan(
        obid,
        json!({"said": "OBY oby_dispatch {\"repo\": \"obi-e2e/fixture-app\", \"task\": \"EDITARTIFACT\", \"kind\": \"research\"}",
               "artifact": {"id": artifact}}),
    );
    assert_eq!(started.status, 200, "{}", started.text);
    let id = started.body["jobs"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no job: {}", started.text))
        .to_owned();
    ended_within(obid, &id, 120)
}

fn artifact_of(obid: &Obid, id: &str) -> Value {
    let got = call(obid, "GET", &format!("/v1/artifacts/{id}"), None);
    assert_eq!(got.status, 200, "{}", got.text);
    got.body
}

fn page_of(obid: &Obid, id: &str, version: u32) -> String {
    call(
        obid,
        "GET",
        &format!("/v1/artifacts/{id}/{version}/index.html"),
        None,
    )
    .text
}

/// ARTIFACTS-19: a task the owner starts with an artifact open starts with that version's files
/// in its worktree, and what its agent changed there is the artifact's next version, made by the
/// job: no second artifact.
#[test]
fn artifacts_19_a_task_started_about_the_open_artifact_makes_its_next_version() {
    let folder = Folder::new("artifacts-19");
    let obid = started_with_jobs(&folder);
    let (c1, artifact) = made_page(&obid);
    let job = edited_with_open(&obid, &artifact);
    let id = job["id"].as_str().unwrap();
    assert_eq!(job["status"], "done", "{job}");
    let kept = artifact_of(&obid, &artifact);
    let versions = kept["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 2, "{kept}");
    assert_eq!(versions[1]["job"], json!(id), "{kept}");
    let listed = call(
        &obid,
        "GET",
        &format!("/v1/artifacts?conversation={c1}"),
        None,
    );
    assert_eq!(listed.body.as_array().unwrap().len(), 1, "{}", listed.text);
    assert_eq!(
        job["artifacts"],
        json!([{"id": artifact, "version": 2}]),
        "{job}"
    );
    assert_eq!(
        page_of(&obid, &artifact, 2),
        format!("{PAGE}<p>edit 1</p>\n")
    );
}

/// ARTIFACTS-20 (and ARTIFACTS-7): the owner's words to that task start a follow-up that carries
/// its artifacts and starts from their latest files: its publish is the version after.
#[test]
fn artifacts_20_a_follow_up_publishes_the_version_after_from_the_latest() {
    let folder = Folder::new("artifacts-20");
    let obid = started_with_jobs(&folder);
    let (_, artifact) = made_page(&obid);
    let job = edited_with_open(&obid, &artifact);
    let id = job["id"].as_str().unwrap();
    let told = call_typed(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({"text": "EDITARTIFACT again"}),
    );
    assert_eq!(told.status, 201, "{}", told.text);
    let follow_up = told.body["id"].as_str().unwrap().to_owned();
    let ended = ended_within(&obid, &follow_up, 120);
    assert_eq!(
        ended["artifacts"],
        json!([{"id": artifact, "version": 3}]),
        "{ended}"
    );
    let kept = artifact_of(&obid, &artifact);
    assert_eq!(kept["versions"].as_array().unwrap().len(), 3, "{kept}");
    assert_eq!(
        page_of(&obid, &artifact, 3),
        format!("{PAGE}<p>edit 1</p>\n<p>edit 2</p>\n")
    );
}

/// ARTIFACTS-21: Oby's own turn with an artifact open finds that version's files in its room, at
/// the path it is told, and what it publishes from there is the same artifact's next version.
#[test]
fn artifacts_21_obys_own_turn_finds_the_open_version_in_its_room() {
    let folder = Folder::new("artifacts-21");
    let obid = started(&folder);
    let (c1, artifact) = made_page(&obid);
    let entry = &artifact[..8];
    let open = plan(
        &obid,
        json!({"said": "SAY about the page", "artifact": {"id": artifact}}),
    );
    assert_eq!(open.status, 200, "{}", open.text);
    let prompt = last_prompt(&obid, &c1);
    assert!(prompt.contains(&format!("artifacts/{entry}")), "{prompt}");
    let page = obid.home.join(format!(
        "conversations/{c1}/room/artifacts/{entry}/index.html"
    ));
    assert_eq!(std::fs::read_to_string(&page).unwrap(), PAGE);
    // The edit, then the publish from the told path.
    std::fs::write(&page, format!("{PAGE}<p>blue</p>\n")).unwrap();
    let said = format!(
        "OBY oby_artifact {{\"title\": \"Functions\", \"path\": \"artifacts/{entry}\", \"artifact\": \"{artifact}\"}}"
    );
    let published = plan(&obid, json!({"said": said, "conversation": c1}));
    assert_eq!(
        published.body["artifacts"][0],
        json!({"id": artifact, "version": 2}),
        "{}",
        published.text
    );
}

/// ARTIFACTS-23: `POST /v1/jobs` can't name an artifact: the key is ignored, so a job in one
/// conversation never starts with another conversation's artifact or its files.
#[test]
fn artifacts_23_post_jobs_cant_seed_another_conversations_artifact() {
    let folder = Folder::new("artifacts-23");
    let obid = started_with_jobs(&folder);
    let (_, theirs) = made_page(&obid);
    let c1 = plan(&obid, json!({"said": "SAY hello", "new": true}));
    let c1 = c1.body["conversation"].as_str().unwrap().to_owned();
    let entry = &theirs[..8];
    let answer = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research",
                             "task": format!("SEES: .obi/artifacts/{entry}"), "conversation": c1,
                             "artifacts": [{"id": theirs, "version": 1, "entry": entry}]}}),
        ),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    assert!(!answer.text.contains(&theirs), "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    let ended = ended_within(&obid, &id, 120);
    assert!(!ended.to_string().contains(&theirs), "{ended}");
    assert!(
        ended["summary"]
            .as_str()
            .unwrap()
            .contains(&format!("sees .obi/artifacts/{entry}: false")),
        "{ended}"
    );
    let kept = artifact_of(&obid, &theirs);
    assert_eq!(kept["versions"].as_array().unwrap().len(), 1, "{kept}");
}
