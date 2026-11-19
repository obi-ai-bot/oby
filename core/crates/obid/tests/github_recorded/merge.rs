//! Merging (crate::merge): a remembered workflow merging an approved pull request (the reviewer's
//! approval, the merge, the branch's deletion), GitHub's auto-merge armed while its checks run and
//! the merge GitHub then made, and a base that takes it only through its merge queue. Each drives
//! the follower of a published job kept for obid's start, as tests/merge_api.rs does.

use std::path::Path;

use serde_json::{Value, json};

use crate::recorder::{Recording, recorded_config};
use crate::support::*;

pub const REPO: &str = "obi-e2e/fixture-app";
pub const PULL: &str = "https://github.com/obi-e2e/fixture-app/pull/7";
pub const ID: &str = "a1b2c3d4";

/// The workflow where Oby's reviewer account approves and GitHub's checks pass.
const REVIEWER: &str =
    r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"reviewer","checks":"github"}"#;
/// The workflow where Oby's own review of the head is the review.
pub const OBY: &str =
    r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"oby","checks":"github"}"#;

fn head() -> String {
    "a".repeat(40)
}

/// A job done and published at `head()` into dev, scanned clean, which Oby's own review approved
/// (tests/merge_api.rs, `published`).
fn published() -> obid::job::Job {
    let mut job = obid::job::Job::new(
        ID.into(),
        REPO.into(),
        "fix".into(),
        "Fix the thing".into(),
        "claude".into(),
        "stub".into(),
    );
    job.status = obid::job::Status::Done;
    job.branch = "oby/fix-7".into();
    job.base = Some("dev".into());
    job.commits = Some(1);
    job.pushed = Some(1);
    job.published = Some(PULL.into());
    job.finished = Some(job.created);
    job.outgoing.push(obid::publish::Outgoing::new(
        "push",
        REPO,
        &head(),
        Some("oby/fix-7"),
    ));
    job.scan = Some(obid::publish::Scanned {
        ran: true,
        clean: Some(true),
        ..Default::default()
    });
    job.review = Some(obid::job::Review {
        verdict: "approved".into(),
        job: "r1b2c3d4".into(),
        agent: "codex".into(),
        account: "default".into(),
        same_account: false,
        round: 1,
        date: job.created,
        commit: head(),
        two_reviews: None,
    });
    job
}

/// The owner taught Oby how fixture-app merges: `workflow`.
pub fn remember(home: &Path, workflow: &str) {
    let dir = home.join("memory/repos/obi-e2e/fixture-app");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("m1a2b3c4d.md"),
        format!(
            "---\nid: m1a2b3c4d\nscope: project\nrepo: {REPO}\nkind: workflow\nsource: user\ntitle: How fixture-app merges\nfrom: You merged fixture-app#6 this way\ncreated: 2026-10-24T10:00:00Z\nupdated: 2026-10-24T10:00:00Z\nused: 0\nworkflow: {workflow}\n---\nSquash-merge into dev after Oby's reviewer account approves and GitHub's checks pass on the same commit.\n"
        ),
    )
    .unwrap();
}

/// A scratch obid following `job`, kept for its start, with `workflow` remembered and, with
/// `reviewer`, Oby's reviewer account (`o-reviewer`) signed in; it reads the pull request every
/// second.
fn following(
    name: &str,
    path: &str,
    job: obid::job::Job,
    workflow: &str,
    reviewer: bool,
) -> (Folder, Recording, Obid) {
    let (folder, recording, mut config) = recorded_config(name, path);
    let remote = fixture(&folder.0.join("fixture"));
    config["repos"][0]["remote"] = json!(format!("file://{}", remote.display()));
    let home = Path::new(config["home"].as_str().unwrap()).to_path_buf();
    std::fs::create_dir_all(home.join("jobs")).unwrap();
    std::fs::write(
        home.join(format!("jobs/{}.json", job.id)),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    if reviewer {
        std::fs::create_dir_all(home.join("github/reviewer")).unwrap();
        std::fs::write(
            home.join("github/reviewer/hosts.yml"),
            "github.com:\n    user: o-reviewer\n",
        )
        .unwrap();
    }
    remember(&home, workflow);
    let obid = start_with(
        &folder.0,
        &config,
        Some(&crate::recorder::bin()),
        &[
            ("OBID_FOLLOW_SECONDS", "1"),
            ("OBID_RERUN_SETTLE_SECONDS", "0"),
        ],
    );
    recording.started(&obid);
    (folder, recording, obid)
}

/// What the job `id` shows of GitHub and of its merge.
pub fn github_of(obid: &Obid, id: &str) -> Value {
    let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    let mut shown = json!({});
    for key in [
        "published",
        "pullRequest",
        "issue",
        "issueUrl",
        "mergedBy",
        "mergeError",
        "mergeNeedsYou",
        "autoMerge",
        "autoMergeOff",
        "outgoing",
    ] {
        shown[key] = job.get(key).cloned().unwrap_or(Value::Null);
    }
    // Rule: a pull request obid merged itself is `mergedAt` obid's clock then; one GitHub merged
    // keeps GitHub's time (the fixtures' 2026-10-24T18:00:00Z).
    if let Some(at) = shown["pullRequest"]["mergedAt"].as_str()
        && !at.starts_with("2026-10-24T")
    {
        shown["pullRequest"]["mergedAt"] = json!("<TIME>");
    }
    let mut stage = job["pipeline"]["stages"]
        .as_array()
        .and_then(|stages| stages.iter().find(|s| s["id"] == "merged").cloned())
        .unwrap_or(Value::Null);
    // Rule: a stage's times are obid's clock, and how long it took the machine's speed.
    for key in ["started", "finished", "seconds", "waited"] {
        if stage.get(key).is_some() {
            stage[key] = json!("<TIME>");
        }
    }
    shown["mergedStage"] = stage;
    shown
}

/// The follower's calls for a path: [`MERGE_CALLS`], from obid's start to the path's last write.
pub fn follower_calls(recording: &Recording) -> Vec<Value> {
    let mut calls = followed(recording);
    // Rule: past the path's last write the follower only reads the pull request again each
    // second, until the test stops obid: how many of those reads a run catches is the machine's.
    // A merge queue's base is read once more after it, for the runs GitHub's queue made of it
    // (crate::jobs::autofix, AUTOFIX-11, 12): the path ends with that read.
    if let Some(last) = calls.iter().rposition(crate::recorder::writes) {
        let queue = calls[last..]
            .iter()
            .position(reads_the_queue)
            .map(|n| last + n);
        calls.truncate(queue.unwrap_or(last) + 1);
    }
    calls
}

/// The follower's calls for a path: [`MERGE_CALLS`] from obid's start, the repository list's own
/// read of the login left out.
pub fn followed(recording: &Recording) -> Vec<Value> {
    // Rule: obid's repository list reads gh's login once at start (crate::repos::spawn), at the
    // same time as the follower's first read, in either order: that first `auth status` as the
    // owner is the list's, not the path's, and is left out before anything is compared.
    let raw = recording.dir.join("calls.jsonl");
    let text = std::fs::read_to_string(&raw).unwrap_or_default();
    let mut lines: Vec<&str> = text.lines().collect();
    if let Some(first) = lines.iter().position(|line| {
        let call: Value = serde_json::from_str(line).unwrap();
        call["args"][0] == "auth"
            && call["args"][1] == "status"
            && !call["env"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n == "GH_CONFIG_DIR")
    }) {
        lines.remove(first);
    }
    std::fs::write(
        &raw,
        lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
    )
    .unwrap();
    recording.calls_starting(MERGE_CALLS)
}

/// Whether `call` reads the runs of GitHub's merge queue.
fn reads_the_queue(call: &Value) -> bool {
    let args: Vec<&str> = (call["args"].as_array().unwrap().iter())
        .filter_map(Value::as_str)
        .collect();
    args.starts_with(&["run", "list"]) && args.contains(&"merge_group")
}

/// The merge cards, as the inbox shows them (open and done).
pub fn merge_cards(obid: &Obid) -> Value {
    let cards: Vec<Value> = call(obid, "GET", "/v1/inbox", None)
        .body
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i["kind"] == "merge")
        .map(|c| json!({"title": c["title"], "text": c["text"], "options": c["options"], "done": !c["done"].is_null()}))
        .collect();
    json!(cards)
}

/// The calls of the follower's merge path: the pull request's reads and writes and the logins it
/// asks about, apart from the work feed's and the commit email's reads obid makes meanwhile.
pub const MERGE_CALLS: &[&str] = &[
    "auth status",
    "pr ",
    "run ",
    "api -X",
    "api --paginate --slurp repos/",
    "api repos/",
    "api user/orgs",
    "api user --jq .login",
];

#[test]
fn github_recorded_merge() {
    let (_folder, recording, obid) = following("merge", "merge", published(), REVIEWER, true);
    let merged = within(30, "the job to be merged", || {
        let shown = crate::merge::github_of(&obid, ID);
        (shown["mergedBy"]["by"] == "memory").then_some(shown)
    });
    let answers = json!({"job": merged, "mergeCards": merge_cards(&obid)});
    drop(obid);
    recording.assert_calls(&follower_calls(&recording));
    recording.assert_answers(&answers);
}

#[test]
fn github_recorded_auto_merge() {
    let (_folder, recording, obid) = following("auto-merge", "auto_merge", published(), OBY, false);
    let armed = within(30, "the job handed to GitHub", || {
        let shown = github_of(&obid, ID);
        (shown["autoMerge"]["head"] == head()).then_some(shown)
    });
    recording.raise("merged-on-github");
    let merged = within(30, "the job merged by GitHub", || {
        let shown = github_of(&obid, ID);
        (shown["mergedBy"]["by"] == "memory"
            && shown["outgoing"]
                .as_array()
                .is_some_and(|o| o.iter().any(|s| s["action"] == "branchDelete")))
        .then_some(shown)
    });
    let answers = json!({"armed": armed, "merged": merged, "mergeCards": merge_cards(&obid)});
    drop(obid);
    recording.assert_calls(&follower_calls(&recording));
    recording.assert_answers(&answers);
}

#[test]
fn github_recorded_merge_queue() {
    let mut own = published();
    own.own_conversation = true;
    let (_folder, recording, obid) = following("merge-queue", "merge_queue", own, OBY, false);
    let queued = within(30, "the job handed to the merge queue", || {
        let shown = github_of(&obid, ID);
        (shown["autoMerge"]["head"] == head()).then_some(shown)
    });
    // Armed, the follower reads the queue's runs on its next pass.
    within(30, "the merge queue's runs read", || {
        let raw = std::fs::read_to_string(recording.dir.join("calls.jsonl")).unwrap_or_default();
        raw.contains("merge_group").then_some(())
    });
    let answers = json!({"queued": queued, "mergeCards": merge_cards(&obid)});
    drop(obid);
    recording.assert_calls(&follower_calls(&recording));
    recording.assert_answers(&answers);
}
