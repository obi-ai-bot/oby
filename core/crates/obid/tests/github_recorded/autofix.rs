//! After the pull request opened: the follower watching it, and a Fix CI round (AUTOFIX-1, 2, 3,
//! 5). The owner's own task, really run and pushed, reads red on GitHub at the head it pushed: obid
//! reads the failed run's log, the same job runs one more round, pushes over it, and reads it green.
//! As tests/merge_api.rs's `autofix_1_a_red_head_resumes_its_own_task_with_no_card`.

use std::path::Path;

use serde_json::{Value, json};

use crate::merge::{PULL, REPO, github_of, merge_cards, remember};
use crate::recorder::recorded_config;
use crate::support::*;

/// The workflow where a person's approval is the review.
const PERSON: &str =
    r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"person","checks":"github"}"#;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// The job's AUTOFIX lines, as obid keeps them in its file (the API never shows them).
fn autofix_lines(home: &Path, id: &str) -> Vec<String> {
    let kept: Value =
        serde_json::from_slice(&std::fs::read(home.join(format!("jobs/{id}.json"))).unwrap())
            .unwrap();
    (kept["autofix"]["lines"].as_array().into_iter().flatten())
        .filter_map(|l| l.as_str().map(str::to_owned))
        .collect()
}

#[test]
fn github_recorded_autofix() {
    let (folder, recording, mut config) = recorded_config("autofix", "autofix");
    let remote = fixture(&folder.0.join("fixture"));
    git(&remote, &["branch", "dev", "main"]);
    config["repos"][0]["remote"] = json!(format!("file://{}", remote.display()));
    config["repos"][0]["pullRequests"] = json!(false);
    config["repos"][0].as_object_mut().unwrap().remove("check");
    config["agents"]["claude"]["bin"] =
        json!(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude"));
    let home = Path::new(config["home"].as_str().unwrap()).to_path_buf();
    remember(&home, PERSON);
    std::fs::write(recording.dir.join("remote"), remote.to_str().unwrap()).unwrap();
    let env = [
        ("OBID_FOLLOW_SECONDS", "1"),
        ("OBID_RERUN_SETTLE_SECONDS", "0"),
    ];
    let bin = crate::recorder::bin();

    // The task runs and pushes `oby/fix-7` (a `file://` remote opens no pull request).
    let obid = start_with(&folder.0, &config, Some(&bin), &env);
    let task = json!({"job": {"repo": REPO, "kind": "build", "task": "COMMIT: fix.txt",
                              "branch": "oby/fix-7", "base": "dev", "check": null, "rounds": 1}});
    let posted = call(&obid, "POST", "/v1/jobs", Some(&task));
    assert_eq!(posted.status, 201, "{}", posted.text);
    let id = posted.body["id"].as_str().unwrap().to_owned();
    let done = ended_within(&obid, &id, 60);
    assert_eq!(done["status"], "done", "{done}");
    let head = git(&remote, &["rev-parse", "refs/heads/oby/fix-7"]);
    drop(obid);

    // Its file says it opened pull request #7, from the owner's own conversation; GitHub's check
    // `test` failed at the head it pushed (`red-at`), and the fake agent fixes checks when told.
    let file = home.join(format!("jobs/{id}.json"));
    let mut kept: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    kept["published"] = json!(PULL);
    kept["ownConversation"] = json!(true);
    kept.as_object_mut().unwrap().remove("unsigned");
    std::fs::write(&file, serde_json::to_vec(&kept).unwrap()).unwrap();
    let obi = home.join("worktrees").join(&id).join(".obi");
    std::fs::create_dir_all(&obi).unwrap();
    for leave in ["keep-prompts", "fix-checks"] {
        std::fs::write(obi.join(leave), "").unwrap();
    }
    std::fs::write(recording.dir.join("red-at"), &head).unwrap();
    // Rule: the path's recording begins with the obid that follows the published job; the calls
    // of the run that made it are the publishing path's, recorded there.
    std::fs::remove_file(recording.dir.join("calls.jsonl")).ok();

    let obid = start_with(&folder.0, &config, Some(&bin), &env);
    recording.started(&obid);
    let red = within(60, "the red head read", || {
        let shown = github_of(&obid, &id);
        (shown["pullRequest"]["head"] == head.as_str()).then_some(shown)
    });
    let lines = within(90, "its checks green after its round", || {
        let lines = autofix_lines(&home, &id);
        lines
            .iter()
            .any(|l| l == "Its checks pass on GitHub now.")
            .then_some(lines)
    });
    // Green, the person's workflow waits on a person's approval, which no teammate's Oby can give
    // out of an Obysphere: the card that says so is where the path settles. (A read made while the
    // round's push lands may show another card for a moment first.)
    let green = within(60, "its merge state settled on the green head", || {
        let shown = github_of(&obid, &id);
        let settled =
            shown["pullRequest"]["head"] != head.as_str()
                && shown["mergeError"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("Oby isn't in an Obysphere"))
                && merge_cards(&obid).as_array().unwrap().iter().any(|c| {
                    c["done"] == false && c["text"].as_str() == shown["mergeError"].as_str()
                });
        settled.then_some(shown)
    });
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    let answers = json!({
        "red": red,
        "green": green,
        "round": job["round"],
        "status": job["status"],
        "autofixInApi": job.get("autofix").cloned().unwrap_or(Value::Null),
        "autofixLines": lines,
        "mergeCards": merge_cards(&obid),
    });
    drop(obid);
    recording.assert_calls(&in_order_then_as_a_set(crate::merge::followed(&recording)));
    recording.assert_answers(&answers);
}

/// `calls` in their order up to obid's read of the failed run's log, then, from the round on, as a
/// set. Rule: once the round runs, its publishing and the follower read GitHub at the same time,
/// in no fixed order and as often as the machine's speed allows; what they ask is kept, each
/// distinct call once, sorted.
fn in_order_then_as_a_set(calls: Vec<Value>) -> Vec<Value> {
    let log = calls
        .iter()
        .position(|c| c["args"][0] == "run" && c["args"][1] == "view")
        .expect("the failed run's log read");
    let mut after: Vec<String> = calls[log + 1..]
        .iter()
        .map(|c| serde_json::to_string(c).unwrap())
        .collect();
    after.sort();
    after.dedup();
    let mut kept = calls[..=log].to_vec();
    kept.push(json!({
        "from the round on, each distinct call once, in no fixed order": after
            .iter()
            .map(|c| serde_json::from_str::<Value>(c).unwrap())
            .collect::<Vec<_>>()
    }));
    kept
}
