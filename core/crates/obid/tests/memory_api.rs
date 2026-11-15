//! Memory (#378, first part; docs/runner-api.md, "obid only: memory"): a suggestion becomes a
//! memory only on the owner's Remember, checked and secret-scanned on its way in, kept 0600 in
//! `home/memory/`, listed and deleted. Finding itself is covered in src/memory.rs.
mod support;

use std::os::unix::fs::PermissionsExt;

use serde_json::{Value, json};
use support::*;

fn suggestion(id: &str, title: &str, text: &str) -> Value {
    json!({"id": id, "scope": "user", "repo": null, "kind": "preference", "title": title,
           "text": text, "from": "You said it in 2 conversations",
           "sources": [{"quote": text, "place": "You, in “Release planning”", "link": null}]})
}

#[test]
fn a_suggestion_is_saved_only_on_remember_and_never_a_secret_or_a_wider_rule() {
    let folder = Folder::new("memory-api");
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &stub_agent());
    let memory = folder.0.join("home/memory");
    std::fs::create_dir_all(&memory).unwrap();
    // Built here, so no token sits in the repository: what the scan must stop.
    let token = format!("ghp_{}", "Zq9Xw2Lm7Rt4".repeat(3));
    std::fs::write(
        memory.join("suggestions.json"),
        json!([
            suggestion(
                "s0000000a",
                "Tests with every fix",
                "Write tests with every fix."
            ),
            suggestion(
                "s0000000b",
                "Straight to main",
                "Push straight to main when it's green."
            ),
            suggestion(
                "s0000000c",
                "The token",
                &format!("Use {token} for GitHub.")
            ),
        ])
        .to_string(),
    )
    .unwrap();
    let obid = start(&folder.0, &config, None);

    let listed = call(&obid, "GET", "/v1/memory", None);
    assert_eq!(listed.status, 200, "{}", listed.text);
    assert_eq!(listed.body["memories"], json!([]));
    assert_eq!(listed.body["suggestions"].as_array().unwrap().len(), 3);
    assert_eq!(listed.body["question"], "Can I remember that?");

    // A rule AGENTS.md keeps fixed isn't widened, and a secret isn't kept: nothing is saved.
    let wider = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000b/remember",
        None,
    );
    assert_eq!(wider.status, 422, "{}", wider.text);
    assert!(
        wider.text.contains("rule that stays fixed"),
        "{}",
        wider.text
    );
    let secret = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000c/remember",
        None,
    );
    assert_eq!(secret.status, 422, "{}", secret.text);
    assert!(
        secret.text.contains("looks like a secret"),
        "{}",
        secret.text
    );
    assert!(
        !memory.join("user").exists()
            || std::fs::read_dir(memory.join("user"))
                .unwrap()
                .next()
                .is_none()
    );

    // Remember: one memory, 0600, listed with where it came from; the suggestion goes.
    let saved = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000a/remember",
        None,
    );
    assert_eq!(saved.status, 201, "{}", saved.text);
    let id = saved.body["id"].as_str().unwrap().to_owned();
    assert_eq!(saved.body["source"], "found");
    let file = memory.join(format!("user/{id}.md"));
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .ends_with("---\nWrite tests with every fix.\n")
    );
    let listed = call(&obid, "GET", "/v1/memory", None);
    assert_eq!(
        listed.body["memories"][0]["text"],
        "Write tests with every fix."
    );
    assert_eq!(
        listed.body["memories"][0]["from"],
        "You said it in 2 conversations"
    );
    let waiting: Vec<&str> = listed.body["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(waiting, ["s0000000b", "s0000000c"]);
    let again = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000a/remember",
        None,
    );
    assert_eq!(again.status, 404, "{}", again.text);

    // Not this: gone, nothing saved.
    let dismissed = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000b/dismiss",
        None,
    );
    assert_eq!(dismissed.status, 204, "{}", dismissed.text);
    let listed = call(&obid, "GET", "/v1/memory", None);
    assert_eq!(listed.body["memories"].as_array().unwrap().len(), 1);
    assert_eq!(listed.body["suggestions"].as_array().unwrap().len(), 1);

    let deleted = call(&obid, "DELETE", &format!("/v1/memory/{id}"), None);
    assert_eq!(deleted.status, 204, "{}", deleted.text);
    assert!(!file.exists());
    let gone = call(&obid, "DELETE", &format!("/v1/memory/{id}"), None);
    assert_eq!(gone.status, 404, "{}", gone.text);
    let odd = call(&obid, "DELETE", "/v1/memory/..%2Fsuggestions", None);
    assert_eq!(odd.status, 404, "{}", odd.text);
    assert!(memory.join("suggestions.json").exists());
}

#[test]
fn a_scan_that_cant_run_saves_nothing() {
    let folder = Folder::new("memory-api-broken-scan");
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &stub_agent());
    let memory = folder.0.join("home/memory");
    std::fs::create_dir_all(&memory).unwrap();
    std::fs::write(
        memory.join("suggestions.json"),
        json!([suggestion(
            "s0000000a",
            "Tests with every fix",
            "Write tests with every fix."
        )])
        .to_string(),
    )
    .unwrap();
    let broken = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bin-broken-gitleaks");
    let obid = start(&folder.0, &config, Some(&broken));

    let refused = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000a/remember",
        None,
    );
    assert_eq!(refused.status, 503, "{}", refused.text);
    assert!(
        refused.text.contains("Nothing was saved"),
        "{}",
        refused.text
    );
    assert!(!memory.join("user").exists());
    let listed = call(&obid, "GET", "/v1/memory", None);
    assert_eq!(listed.body["memories"], json!([]));
    assert_eq!(listed.body["suggestions"].as_array().unwrap().len(), 1);
}

/// How a repository merges (#854) is kept typed, only on the owner's Remember on its own
/// question, never into main, and read back as written.
#[test]
fn merging_22_a_workflow_memory_keeps_its_typed_merge_and_main_only_on_the_owners_word() {
    let folder = Folder::new("memory-api-workflow");
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &stub_agent());
    let memory = folder.0.join("home/memory");
    std::fs::create_dir_all(&memory).unwrap();
    let workflow = |base: &str| json!({"base": base, "method": "squash", "rebaseFirst": false, "review": "reviewer", "checks": "mac"});
    let asked = |id: &str, base: &str| {
        json!({"id": id, "scope": "project", "repo": "obi-e2e/fixture-app", "kind": "workflow",
               "title": "How fixture-app merges", "text": "Merge whenever.",
               "from": "You merged fixture-app#7 this way", "sources": [], "workflow": workflow(base)})
    };
    std::fs::write(
        memory.join("suggestions.json"),
        json!([asked("s0000000a", "dev"), asked("s0000000b", "main")]).to_string(),
    )
    .unwrap();
    let obid = start(&folder.0, &config, None);

    let main = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000b/remember",
        None,
    );
    // A Remember no app of the owner's signed would widen what Oby does by itself: refused.
    assert_eq!(main.status, 422, "{}", main.text);
    assert!(main.text.contains("stays fixed"), "{}", main.text);
    // The owner's own signed Remember keeps it (MERGING-22).
    let main = call_signed(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000b/remember",
        None,
    );
    assert_eq!(main.status, 201, "{}", main.text);
    assert_eq!(main.body["workflow"], workflow("main"));

    let saved = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000a/remember",
        None,
    );
    assert_eq!(saved.status, 201, "{}", saved.text);
    assert_eq!(saved.body["kind"], "workflow");
    assert_eq!(saved.body["source"], "user");
    assert_eq!(saved.body["workflow"], workflow("dev"));
    // Its words are obid's own, made from the workflow, never the suggestion's.
    assert_eq!(
        saved.body["text"],
        "Squash-merge into dev after Oby's reviewer account approves and your Mac's checks pass on the same commit."
    );
    let id = saved.body["id"].as_str().unwrap();
    let file =
        std::fs::read_to_string(memory.join(format!("repos/obi-e2e/fixture-app/{id}.md"))).unwrap();
    assert!(
        file.contains(r#"workflow: {"base":"dev","method":"squash","rebaseFirst":false,"review":"reviewer","checks":"mac"}"#),
        "{file}"
    );
    let listed = call(&obid, "GET", "/v1/memory", None);
    // dev's replaced main's: one workflow per repository.
    assert_eq!(listed.body["memories"].as_array().unwrap().len(), 1);
    assert_eq!(listed.body["memories"][0]["workflow"], workflow("dev"));
    assert_eq!(listed.body["suggestions"], json!([]));
}

/// Starts obid on a home whose suggestions are `suggestions`, and the memory folder.
fn with_suggestions(name: &str, suggestions: Value) -> (Folder, Obid, std::path::PathBuf) {
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &stub_agent());
    let memory = folder.0.join("home/memory");
    std::fs::create_dir_all(&memory).unwrap();
    std::fs::write(memory.join("suggestions.json"), suggestions.to_string()).unwrap();
    let obid = start(&folder.0, &config, None);
    (folder, obid, memory)
}

const DECLINED_TEXT: &str = "Rebase first, then run the tests.";

/// The MEMORY-1 candidate: the declined text in other case and accents, quoted word for word
/// from two of the owner's conversations, as `POST /v1/memory/find` checks the brain's answer.
fn asked_again(declined: &[obid::memory::Declined]) -> Vec<obid::memory::Suggestion> {
    use obid::memory::{Place, Said};
    let said = |id: &str| Said {
        place: Place::Conversation(id.into()),
        label: format!("You, in “{id}”"),
        link: None,
        who: None,
        text: "Please rebase first, then run the tests, as always.".into(),
    };
    let answer = json!({"candidates": [{
        "title": "Rebase first", "text": "RÉBASE first, then run the TESTS.", "scope": "user",
        "repo": null, "kind": "preference",
        "quotes": [{"source": "S1", "quote": "rebase first, then run the tests"},
                   {"source": "S2", "quote": "rebase first, then run the tests"}]}]});
    obid::memory::verify(&answer, &[said("c1"), said("c2")], &[], &[], declined)
}

#[test]
fn memory_1_a_declined_text_is_not_asked_again() {
    let (_folder, obid, _) = with_suggestions(
        "memory-api-declined",
        json!([suggestion("s0000000a", "Rebase first", DECLINED_TEXT)]),
    );
    let dismissed = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000a/dismiss",
        None,
    );
    assert_eq!(dismissed.status, 204, "{}", dismissed.text);
    let declined = obid::memory::declined(&obid.home);
    assert_eq!(declined.len(), 1);
    assert_eq!(declined[0].scope, "user");
    assert_eq!(declined[0].repo, None);
    assert!(asked_again(&declined).is_empty());
    // Only the declined list drops it.
    assert_eq!(asked_again(&[]).len(), 1);
    // Another repository's decline doesn't silence the owner's.
    let elsewhere = obid::memory::Declined {
        repo: Some("obi-e2e/fixture-app".into()),
        scope: "project".into(),
        ..declined[0].clone()
    };
    assert_eq!(asked_again(&[elsewhere]).len(), 1);
}

#[test]
fn memory_2_the_declined_list_keeps_the_newest_200() {
    let (_folder, obid, memory) = with_suggestions(
        "memory-api-declined-200",
        json!([suggestion("s0000000a", "Rebase first", DECLINED_TEXT)]),
    );
    let file = memory.join("declined.jsonl");
    let kept: String = (0..200)
        .map(|n| {
            format!(
                "{}\n",
                json!({"text": format!("old {n}"), "scope": "user", "repo": null})
            )
        })
        .collect();
    std::fs::write(&file, kept).unwrap();
    let dismissed = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000a/dismiss",
        None,
    );
    assert_eq!(dismissed.status, 204, "{}", dismissed.text);
    let lines: Vec<Value> = std::fs::read_to_string(&file)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 200);
    assert_eq!(lines[0]["text"], "old 1");
    assert_eq!(
        lines[199],
        json!({"text": "rebase first, then run the tests.", "scope": "user", "repo": null})
    );
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn memory_3_clearing_the_declined_list_lets_a_text_be_asked_again() {
    let (_folder, obid, _) = with_suggestions(
        "memory-api-declined-clear",
        json!([suggestion("s0000000a", "Rebase first", DECLINED_TEXT)]),
    );
    let dismissed = call(
        &obid,
        "POST",
        "/v1/memory/suggestions/s0000000a/dismiss",
        None,
    );
    assert_eq!(dismissed.status, 204, "{}", dismissed.text);
    assert!(asked_again(&obid::memory::declined(&obid.home)).is_empty());
    let cleared = call(&obid, "DELETE", "/v1/memory/declined", None);
    assert_eq!(cleared.status, 204, "{}", cleared.text);
    assert!(obid::memory::declined(&obid.home).is_empty());
    assert_eq!(asked_again(&obid::memory::declined(&obid.home)).len(), 1);
    let missing = call(&obid, "DELETE", "/v1/memory/m0000000z", None);
    assert_eq!(missing.status, 404, "{}", missing.text);
}

#[test]
fn memory_4_each_memory_change_is_one_event_after_its_save() {
    let (_folder, obid, _) = with_suggestions(
        "memory-api-events",
        json!([
            suggestion(
                "s0000000a",
                "Tests with every fix",
                "Write tests with every fix."
            ),
            suggestion("s0000000b", "Rebase first", DECLINED_TEXT),
        ]),
    );
    let events = follow_events(&obid);
    // Each change is made on a thread of its own, its event read here as it comes, and the
    // memory read then: what the event says is already saved.
    let next = |method: &'static str, target: String, status: u16| -> (Value, Value) {
        std::thread::scope(|scope| {
            let answer = scope.spawn(|| call(&obid, method, &target, None));
            let event = events_named(&events, "memory", 30, |got| !got.is_empty()).remove(0);
            let listed = call(&obid, "GET", "/v1/memory", None).body;
            let answer = answer.join().unwrap();
            assert_eq!(answer.status, status, "{}", answer.text);
            (event, listed)
        })
    };
    let (added, listed) = next(
        "POST",
        "/v1/memory/suggestions/s0000000a/remember".into(),
        201,
    );
    let id = listed["memories"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        added,
        json!({"change": "added", "id": id, "suggestion": "s0000000a"})
    );
    let (removed, listed) = next("DELETE", format!("/v1/memory/{id}"), 204);
    assert_eq!(removed, json!({"change": "removed", "id": id}));
    assert_eq!(listed["memories"], json!([]));
    let (declined, listed) = next(
        "POST",
        "/v1/memory/suggestions/s0000000b/dismiss".into(),
        204,
    );
    assert_eq!(declined, json!({"change": "declined", "id": "s0000000b"}));
    assert_eq!(listed["suggestions"], json!([]));
    // Three changes, three events: nothing more, and no words.
    let more: Vec<Value> =
        std::iter::from_fn(|| events.recv_timeout(std::time::Duration::from_secs(1)).ok())
            .filter(|(name, _)| name == "memory")
            .map(|(_, data)| data)
            .collect();
    assert!(more.is_empty(), "{more:?}");
    for event in [&added, &removed, &declined] {
        let data = event.to_string();
        assert!(!data.contains("tests"), "{data}");
        assert!(!data.contains("Tests"), "{data}");
    }
}

#[test]
fn memory_5_the_gateway_allows_memory_reads() {
    use axum::http::Method;
    for (method, path) in [
        (Method::GET, "/v1/memory"),
        (Method::DELETE, "/v1/memory/m1a2b3c4d"),
        // A paired device may clear the declined list too (MEMORY-5).
        (Method::DELETE, "/v1/memory/declined"),
        (Method::POST, "/v1/memory/suggestions/s1a2b3c4d/remember"),
        (Method::POST, "/v1/memory/suggestions/s1a2b3c4d/dismiss"),
    ] {
        assert!(obid::gateway::allows(&method, path), "{method} {path}");
    }
    for (method, path) in [
        (Method::PUT, "/v1/memory"),
        (Method::PUT, "/v1/memory/m1a2b3c4d"),
        (Method::POST, "/v1/memory/find"),
    ] {
        assert!(!obid::gateway::allows(&method, path), "{method} {path}");
    }
}
