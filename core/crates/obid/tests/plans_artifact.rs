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

/// A page Oby's own turn made in a new conversation, on words the owner's app signed: the
/// conversation and the artifact.
fn made_page(obid: &Obid) -> (String, String) {
    let made = call_typed(
        obid,
        "POST",
        "/v1/plans",
        &json!({"said": "MAKEPAGE OBY oby_artifact {\"title\": \"Functions\", \"path\": \"page\"}", "new": true}),
    );
    assert_eq!(made.status, 200, "{}", made.text);
    (
        made.body["conversation"].as_str().unwrap().to_owned(),
        made.body["artifacts"][0]["id"].as_str().unwrap().to_owned(),
    )
}

/// The page's text as `MAKEPAGE` writes it (tests/agents/fake-harness).
const PAGE: &str = "<!doctype html><title>Functions</title><ul><li>greet</li></ul>\n";

/// The task the owner starts with `artifact` open, on words their app signed, whose agent edits
/// what it was given (tests/agents/fake-claude, EDITARTIFACT), waited on to its end.
fn edited_with_open(obid: &Obid, artifact: &str) -> Value {
    let started = call_typed(
        obid,
        "POST",
        "/v1/plans",
        &json!({"said": "OBY oby_dispatch {\"repo\": \"obi-e2e/fixture-app\", \"task\": \"EDITARTIFACT\", \"kind\": \"research\"}",
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
    // Made by Oby's own turn on the owner's signed words: nothing marks the task's turn.
    assert!(job.get("untrusted").is_none(), "{job}");
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
    // The path, never the files' text.
    assert!(!prompt.contains("<li>greet</li>"), "{prompt}");
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

/// ARTIFACTS-24: an artifact a marked job made (here one the API's unsigned words started)
/// makes the turn that opens it one that read something untrusted, so the task it starts is
/// marked untrusted; the task still versions it.
#[test]
fn artifacts_24_a_marked_jobs_version_taints_the_turn_that_opens_it() {
    let folder = Folder::new("artifacts-24");
    let obid = started_with_jobs(&folder);
    let made = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research", "task": "EDITARTIFACT"}}),
        ),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let made = ended_within(&obid, made.body["id"].as_str().unwrap(), 120);
    let artifact = made["artifacts"][0]["id"].as_str().unwrap().to_owned();
    let job = edited_with_open(&obid, &artifact);
    assert_eq!(job["untrusted"], json!(true), "{job}");
    assert_eq!(
        job["artifacts"],
        json!([{"id": artifact, "version": 2}]),
        "{job}"
    );
}

/// The page `first` made in a new conversation (`signed` as the owner's app signs its words),
/// that conversation deleted, then the page opened again with the owner's signed words that start
/// a task editing it (tests/agents/fake-claude, EDITARTIFACT): the task, ended, and its kept file.
fn reopened_after_delete(obid: &Obid, first: &str, signed: bool) -> (Value, Value) {
    let body = json!({"said": first, "new": true});
    let made = if signed {
        call_typed(obid, "POST", "/v1/plans", &body)
    } else {
        plan(obid, body)
    };
    assert_eq!(made.status, 200, "{}", made.text);
    let c1 = made.body["conversation"].as_str().unwrap().to_owned();
    let artifact = made.body["artifacts"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no artifact: {}", made.text))
        .to_owned();
    let gone = call(obid, "DELETE", &format!("/v1/conversations/{c1}"), None);
    assert!(gone.status < 300, "{}", gone.text);
    let started = call_typed(
        obid,
        "POST",
        "/v1/plans",
        &json!({"said": "OBY oby_dispatch {\"repo\": \"obi-e2e/fixture-app\", \"task\": \"EDITARTIFACT\", \"kind\": \"research\"}",
               "artifact": {"id": artifact}}),
    );
    assert_eq!(started.status, 200, "{}", started.text);
    assert_ne!(started.body["conversation"], json!(c1), "{}", started.text);
    let id = started.body["jobs"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no job: {}", started.text))
        .to_owned();
    let job = ended_within(obid, &id, 120);
    let kept: Value =
        serde_json::from_slice(&std::fs::read(obid.home.join(format!("jobs/{id}.json"))).unwrap())
            .unwrap();
    (job, kept)
}

/// ARTIFACTS-24, 25: a version Oby's turn made in a conversation that heard something off obid's
/// allowlist (here words no app of the owner's signed) keeps that mark once its conversation is
/// deleted: the task the owner's signed words start with it open, in a clean conversation, is
/// someone else's, and its turn read something untrusted.
#[test]
fn artifacts_25_a_foreign_turns_version_marks_the_task_after_its_conversation_is_deleted() {
    let folder = Folder::new("artifacts-25-foreign");
    let obid = started_with_jobs(&folder);
    let (job, kept) = reopened_after_delete(
        &obid,
        "MAKEPAGE OBY oby_artifact {\"title\": \"Functions\", \"path\": \"page\"}",
        false,
    );
    assert!(kept["foreign"].is_string(), "{kept}");
    assert_eq!(job["untrusted"], json!(true), "{job}");
}

/// ARTIFACTS-25: a version Oby's turn made after reading something untrusted (the web), in the
/// owner's own signed conversation, keeps that mark once its conversation is deleted and the
/// artifact is opened elsewhere.
#[test]
fn artifacts_25_a_tainted_turns_version_keeps_its_mark_after_its_conversation_is_deleted() {
    let folder = Folder::new("artifacts-25-tainted");
    let obid = started_with_jobs(&folder);
    let (job, kept) = reopened_after_delete(
        &obid,
        "WEB MAKEPAGE OBY oby_artifact {\"title\": \"Functions\", \"path\": \"page\"}",
        true,
    );
    assert!(kept["foreign"].is_string(), "{kept}");
    assert_eq!(job["untrusted"], json!(true), "{job}");
}

/// ARTIFACTS-24 (LANE-3a): a version an own-lane job made (no egress proxy of obid's) keeps
/// `unproxied`, and opening it makes no turn untrusted: the next task the owner starts with it
/// open isn't `untrusted`, and makes the next version.
#[test]
fn artifacts_24_an_unproxied_jobs_version_opened_in_a_turn_starts_no_untrusted_task() {
    let folder = Folder::new("artifacts-24-unproxied-turn");
    let obid = started_with_jobs(&folder);
    let (_, artifact) = made_page(&obid);
    let first = edited_with_open(&obid, &artifact);
    assert_eq!(first["artifacts"][0]["version"], json!(2), "{first}");
    let kept = artifact_of(&obid, &artifact);
    assert_eq!(
        kept["versions"][1]["marks"],
        json!({"unproxied": true}),
        "{kept}"
    );
    let second = edited_with_open(&obid, &artifact);
    assert!(second.get("untrusted").is_none(), "{second}");
    assert_eq!(
        second["artifacts"],
        json!([{"id": artifact, "version": 3}]),
        "{second}"
    );
}

/// `words` said in `conversation` on words the owner's app signed, or with none when `signed` is
/// false (the conversation then hears words off obid's allowlist): the answer.
fn told(obid: &Obid, conversation: &str, words: &str, signed: bool) -> Answer {
    let body = json!({"said": words, "conversation": conversation});
    let answer = if signed {
        call_typed(obid, "POST", "/v1/plans", &body)
    } else {
        plan(obid, body)
    };
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer
}

/// Whether job `id`'s kept file says it ran with no egress proxy of obid's (LANE-3a).
fn ran_unproxied(obid: &Obid, id: &str) -> bool {
    let kept: Value =
        serde_json::from_slice(&std::fs::read(obid.home.join(format!("jobs/{id}.json"))).unwrap())
            .unwrap();
    kept["ranUnproxied"] == json!(true)
}

/// A task with nothing to do that a turn of `conversation` starts after reading the web, so it
/// is `untrusted` and runs in Oby's box, behind obid's proxy, waited on to its end: its id.
fn boxed_task(obid: &Obid, conversation: &str) -> String {
    let started = told(
        obid,
        conversation,
        "WEB OBY oby_dispatch {\"repo\": \"obi-e2e/fixture-app\", \"task\": \"Look around\", \"kind\": \"research\"}",
        true,
    );
    assert_eq!(
        started.body["jobs"][0]["untrusted"],
        json!(true),
        "{}",
        started.text
    );
    let id = started.body["jobs"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no job: {}", started.text))
        .to_owned();
    ended_within(obid, &id, 120);
    id
}

/// ARTIFACTS-24 (LANE-3a): a conversation that had an unproxied version laid in its room keeps
/// that for good, as it keeps a taint, whatever its later turns open or name: what it publishes
/// from then on, from files copied anywhere in its room, records `unproxied`, and a job it plans
/// runs as LANE-3a's (no declared secrets for its check) though Oby's box would have kept it
/// behind obid's proxy; none of it is someone else's. A clean conversation's job isn't.
#[test]
fn artifacts_24_a_conversation_that_laid_an_unproxied_version_keeps_it() {
    let folder = Folder::new("artifacts-24-conversation");
    let obid = started_with_jobs(&folder);
    let (c1, artifact) = made_page(&obid);
    edited_with_open(&obid, &artifact);
    let opened = call_typed(
        &obid,
        "POST",
        "/v1/plans",
        &json!({"said": "SAY looked", "artifact": {"id": artifact}}),
    );
    assert_eq!(opened.status, 200, "{}", opened.text);
    // A later turn with nothing open publishes the laid files copied elsewhere in the room.
    let room = obid.home.join(format!("conversations/{c1}/room"));
    let laid = room.join(format!("artifacts/{}/index.html", &artifact[..8]));
    std::fs::create_dir_all(room.join("copy")).unwrap();
    std::fs::copy(&laid, room.join("copy/index.html")).unwrap();
    let copied = told(
        &obid,
        &c1,
        "OBY oby_artifact {\"title\": \"Copy\", \"path\": \"copy\"}",
        true,
    );
    let new = copied.body["artifacts"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no artifact: {}", copied.text))
        .to_owned();
    assert_ne!(new, artifact);
    let kept = artifact_of(&obid, &new);
    assert_eq!(
        kept["versions"][0]["marks"],
        json!({"unproxied": true}),
        "{kept}"
    );
    // A job it plans, boxed (so its own run never makes it LANE-3a's), is LANE-3a's; one of a
    // clean conversation isn't.
    let theirs = boxed_task(&obid, &c1);
    assert!(ran_unproxied(&obid, &theirs), "{theirs}");
    let clean = call_typed(
        &obid,
        "POST",
        "/v1/plans",
        &json!({"said": "SAY hello", "new": true}),
    );
    let c2 = clean.body["conversation"].as_str().unwrap().to_owned();
    let other = boxed_task(&obid, &c2);
    assert!(!ran_unproxied(&obid, &other), "{other}");
}

/// ARTIFACTS-24: what Oby publishes with a version open carries that version's marks, never a
/// later one's it never laid out: a copy of clean version 1, its version 2 someone else's, is
/// clean.
#[test]
fn artifacts_24_a_copy_of_the_open_version_carries_only_its_marks() {
    let folder = Folder::new("artifacts-24-laid-version");
    let obid = started_with_jobs(&folder);
    let (c1, artifact) = made_page(&obid);
    edited_with_open(&obid, &artifact);
    // Version 2 someone else's, as a marked job would have kept it.
    let file = obid
        .home
        .join(format!("artifacts/{artifact}/artifact.json"));
    let mut kept: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    kept["versions"][1]["marks"] = json!({"foreign": "a job that was someone else's"});
    std::fs::write(&file, serde_json::to_vec_pretty(&kept).unwrap()).unwrap();
    let said = format!(
        "OBY oby_artifact {{\"title\": \"Copy\", \"path\": \"artifacts/{}\"}}",
        &artifact[..8]
    );
    let copied = call_typed(
        &obid,
        "POST",
        "/v1/plans",
        &json!({"said": said, "conversation": c1, "artifact": {"id": artifact, "version": 1}}),
    );
    assert_eq!(copied.status, 200, "{}", copied.text);
    let new = copied.body["artifacts"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no artifact: {}", copied.text))
        .to_owned();
    let kept = artifact_of(&obid, &new);
    assert_eq!(kept["versions"][0]["marks"], json!({}), "{kept}");
}
