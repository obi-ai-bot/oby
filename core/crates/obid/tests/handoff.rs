//! Another Oby's task (docs/contracts/handoff.md), through the real obid binary and the fake
//! Sphere. The test plays obi-ernesto, whose Oby hands wellington's a task; Claude Code is the
//! stand-in (tests/agents/fake-claude), which commits what the quoted request's goal names and
//! exits 3 when that goal reaches its prompt.

mod support;

use serde_json::{Value, json};
use support::*;

const REPO: &str = "obi-e2e/fixture-app";
const ACCEPT_REMEMBER: &str = "Accept, and remember for this repo";
const GOAL: &str = "Add a greeting file. COMMIT: greeting.txt";

/// obid (wellington), its card accepting handoffs, and obi-ernesto, both joined to a fake Sphere.
async fn setup(folder: &Folder) -> (FakeSphere, Obid, TestObi) {
    let sphere = FakeSphere::start().await;
    let remote = fixture(&folder.0.join("fixture"));
    let agent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let obid = start(&folder.0, &config(&folder.0, &remote, &agent), None);
    let wellington = Owner::new("wellington");
    let joined = tokio::task::block_in_place(|| {
        join_a_accepting(
            &obid,
            &sphere,
            &wellington,
            &["review_request", "question", "handoff"],
        )
    });
    assert_eq!(joined["state"], "joined");
    let ernesto = TestObi::join_accepting(&sphere, "obi-ernesto", "ernesto", &["question"]).await;
    (sphere, obid, ernesto)
}

fn handoff(repo: &str) -> Value {
    json!({"goal": GOAL, "repo": repo, "base_branch": "main",
           "acceptance": [{"kind": "tests_pass", "note": "the suite stays green"}],
           "summary": "Ernesto's demo needs it."})
}

fn get(obid: &Obid, target: &str) -> Value {
    let answer = call(obid, "GET", target, None);
    assert_eq!(answer.status, 200, "GET {target}: {}", answer.text);
    answer.body
}

/// The open obiRequest item of exchange `cid`, once it is there.
fn wait_item(obid: &Obid, cid: &str) -> Value {
    within(20, "the handoff in the inbox", || {
        get(obid, "/v1/inbox")
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["obi"]["correlationId"] == cid)
            .cloned()
    })
}

fn answer_item(obid: &Obid, item: &Value, answer: &str) -> Answer {
    let id = item["id"].as_str().unwrap();
    call_typed(
        obid,
        "POST",
        &format!("/v1/inbox/{id}/answer"),
        &json!({"answer": answer}),
    )
}

fn jobs(obid: &Obid) -> Vec<Value> {
    get(obid, "/v1/jobs").as_array().unwrap().clone()
}

/// Ernesto sends a handoff for `repo`: its correlation id.
async fn send(ernesto: &TestObi, repo: &str) -> String {
    let request = ernesto.request("obi-wellington", "handoff", handoff(repo), 3600);
    ernesto.send(&request).await;
    request["id"].as_str().unwrap().to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn handoff_1_a_handoff_waits_in_the_inbox_quoted_and_starts_nothing() {
    let folder = Folder::new("handoff-1");
    let (_sphere, obid, ernesto) = setup(&folder).await;
    let cid = send(&ernesto, REPO).await;
    tokio::task::block_in_place(|| {
        let item = wait_item(&obid, &cid);
        assert_eq!(item["kind"], "obiRequest");
        assert_eq!(item["title"], "Task from ernesto");
        assert_eq!(
            item["text"],
            format!(
                "ernesto's Obi (obi-ernesto) asks your Oby to do a task in {REPO} from main, quoted below. Conditions: tests pass."
            )
        );
        for field in ["title", "text"] {
            assert!(!item[field].as_str().unwrap().contains("greeting"));
        }
        assert_eq!(
            item["options"],
            json!([
                "Accept",
                ACCEPT_REMEMBER,
                "Decline",
                "Decline, and remember for this repo"
            ])
        );
        assert_eq!(item["obi"]["type"], "handoff");
        assert_eq!(item["obi"]["repo"], REPO);
        assert_eq!(item["obi"]["baseBranch"], "main");
        assert_eq!(item["obi"]["quoted"]["goal"], GOAL);
        assert_eq!(item["obi"]["quoted"]["summary"], "Ernesto's demo needs it.");
        assert_eq!(
            item["obi"]["quoted"]["acceptance"],
            json!([{"index": 0, "note": "the suite stays green"}])
        );
        let exchange = get(&obid, &format!("/v1/obid/sphere/exchanges/{cid}"));
        assert_eq!(exchange["state"], "inbox");
        assert!(
            jobs(&obid).is_empty(),
            "nothing starts before the owner's Accept"
        );
        // SPHERE-17: an Accept obid's token alone sends starts nothing either: only the owner's
        // signed one does.
        let id = item["id"].as_str().unwrap();
        for unsigned in ["Accept", ACCEPT_REMEMBER] {
            let refused = call(
                &obid,
                "POST",
                &format!("/v1/inbox/{id}/answer"),
                Some(&json!({"answer": unsigned})),
            );
            assert_eq!(refused.status, 403, "{unsigned}: {}", refused.text);
        }
        assert_eq!(
            get(&obid, &format!("/v1/obid/sphere/exchanges/{cid}"))["state"],
            "inbox"
        );
        assert_eq!(get(&obid, "/v1/obid/decisions"), json!([]));
        assert!(jobs(&obid).is_empty(), "an unsigned Accept starts nothing");
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn handoff_2_3_accept_runs_it_in_the_foreign_lane_and_the_result_goes_back() {
    let folder = Folder::new("handoff-2");
    let (_sphere, obid, ernesto) = setup(&folder).await;
    let cid = send(&ernesto, REPO).await;
    let job = tokio::task::block_in_place(|| {
        let item = wait_item(&obid, &cid);
        let answered = answer_item(&obid, &item, "Accept");
        assert_eq!(answered.status, 200, "{}", answered.text);
        let exchange = within(20, "the exchange accepted", || {
            let e = call(
                &obid,
                "GET",
                &format!("/v1/obid/sphere/exchanges/{cid}"),
                None,
            )
            .body;
            (e["state"] == "accepted" || e["state"] == "done").then_some(e)
        });
        let id = exchange["job"].as_str().expect("its job").to_owned();
        let job = get(&obid, &format!("/v1/jobs/{id}"));
        // HANDOFF-2: a build job, untrusted, from obid's own words; the request is a file.
        assert_eq!(job["kind"], "build");
        assert_eq!(job["repo"], REPO);
        assert_eq!(job["untrusted"], true);
        assert_eq!(job["title"], "Task for ernesto: fixture-app");
        for field in ["task", "title"] {
            assert!(
                !job[field].as_str().unwrap().contains("greeting"),
                "the request's words in its {field}"
            );
        }
        assert!(
            job["task"]
                .as_str()
                .unwrap()
                .contains(".obi/inputs/handoff.json")
        );
        let ended = call(&obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
        assert_eq!(ended.status, 200, "{}", ended.text);
        get(&obid, &format!("/v1/jobs/{id}"))
    });
    assert_eq!(job["status"], "done", "{job}");
    let replies = ernesto.receive(2, 60).await;
    assert_eq!(replies[0]["type"], "accept", "{replies:?}");
    assert_eq!(replies[1]["type"], "result", "{replies:?}");
    let result = &replies[1]["body"];
    // HANDOFF-3: done, its one condition met, the branch and the commit it made.
    assert_eq!(result["for"], "handoff");
    assert_eq!(result["outcome"], "done", "{result}");
    assert_eq!(result["repo"], REPO);
    assert_eq!(
        result["acceptance"],
        json!([{"index": 0, "kind": "tests_pass", "met": true}])
    );
    assert_eq!(result["head_branch"], job["branch"]);
    assert_eq!(result["head_sha"].as_str().map(str::len), Some(40));
    assert!(result["summary"].as_str().unwrap().contains("greeting.txt"));
    tokio::task::block_in_place(|| {
        let done = get(&obid, &format!("/v1/obid/sphere/exchanges/{cid}"));
        assert_eq!(done["state"], "done");
        assert_eq!(done["publishScan"], json!({"ran": true, "clean": true}));
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn handoff_4_decline_starts_nothing_and_a_remembered_accept_needs_no_item() {
    let folder = Folder::new("handoff-4");
    let (_sphere, obid, ernesto) = setup(&folder).await;
    let cid = send(&ernesto, REPO).await;
    tokio::task::block_in_place(|| {
        let item = wait_item(&obid, &cid);
        let answered = answer_item(&obid, &item, "Decline");
        assert_eq!(answered.status, 200, "{}", answered.text);
    });
    let replies = ernesto.receive(1, 30).await;
    assert_eq!(replies[0]["type"], "decline");
    assert_eq!(replies[0]["body"]["reason"], "owner_declined");
    tokio::task::block_in_place(|| assert!(jobs(&obid).is_empty()));

    // Accept, and remember: the next one from ernesto on that repository is accepted by itself.
    let cid = send(&ernesto, REPO).await;
    tokio::task::block_in_place(|| {
        let item = wait_item(&obid, &cid);
        let answered = answer_item(&obid, &item, ACCEPT_REMEMBER);
        assert_eq!(answered.status, 200, "{}", answered.text);
        let decisions = get(&obid, "/v1/obid/decisions");
        assert_eq!(decisions[0]["type"], "handoff");
        assert_eq!(decisions[0]["fromOwner"], "ernesto");
        assert_eq!(decisions[0]["repo"], REPO);
    });
    let cid = send(&ernesto, REPO).await;
    let exchange = tokio::task::block_in_place(|| {
        within(20, "the remembered accept", || {
            let e = call(
                &obid,
                "GET",
                &format!("/v1/obid/sphere/exchanges/{cid}"),
                None,
            )
            .body;
            (e["decidedBy"] == "rule").then_some(e)
        })
    });
    assert!(exchange["job"].is_string(), "{exchange}");
    assert!(exchange["inboxItem"].is_null(), "{exchange}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn handoff_5_a_repository_obid_doesnt_know_is_declined_unavailable() {
    let folder = Folder::new("handoff-5");
    let (_sphere, obid, ernesto) = setup(&folder).await;
    let cid = send(&ernesto, "someone/elsewhere").await;
    tokio::task::block_in_place(|| {
        let item = wait_item(&obid, &cid);
        let answered = answer_item(&obid, &item, "Accept");
        assert_eq!(answered.status, 200, "{}", answered.text);
        assert!(
            answered.body["text"]
                .as_str()
                .unwrap()
                .ends_with("This Oby doesn't work in that repository, so it can't do this task.")
        );
    });
    let replies = ernesto.receive(1, 30).await;
    assert_eq!(replies[0]["type"], "decline", "{replies:?}");
    assert_eq!(replies[0]["body"]["reason"], "unavailable");
    tokio::task::block_in_place(|| assert!(jobs(&obid).is_empty()));
}
