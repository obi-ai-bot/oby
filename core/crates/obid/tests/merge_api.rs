//! Oby carries a task the whole way (#853, #854; docs/runner-api.md, "obid only: merging"): once
//! Oby's review approved a published job's head, its reviewer account approves the pull request on
//! GitHub, and the follower merges it the way the repository's workflow says, or asks the owner
//! first where Oby hasn't learned it. Against the real obid, with a stand-in gh.
mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

const REPO: &str = "obi-e2e/fixture-app";
const PULL: &str = "https://github.com/obi-e2e/fixture-app/pull/7";
const ID: &str = "a1b2c3d4";

fn head() -> String {
    "a".repeat(40)
}

/// A stand-in gh that records every call: the owner is `octo-cat`, the reviewer (with
/// `GH_CONFIG_DIR`) `oby-reviewer`; the pull request is open, green and clean into dev at
/// `head()`; its reviews are the reviewer's approval once it posted one; the merge goes through.
fn stand_in_gh(folder: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = folder.join("bin");
    let state = folder.join("gh");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let head = head();
    let view = json!({
        "state": "OPEN", "reviewDecision": "", "mergedAt": null, "baseRefName": "dev",
        "headRefOid": head,
        "statusCheckRollup": [{"__typename": "CheckRun", "name": "test", "status": "COMPLETED", "conclusion": "SUCCESS"}],
        "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN", "author": {"login": "octo-cat"},
    });
    let approval = json!([[{
        "id": 1, "user": {"login": "oby-reviewer", "type": "User"}, "state": "APPROVED",
        "commit_id": head, "author_association": "COLLABORATOR", "body": "",
    }]]);
    let pull = json!({
        "state": "open", "merged": false, "title": "Fix the thing",
        "base": {"ref": "dev", "repo": {"full_name": REPO}},
        "head": {"ref": "oby/fix-7", "sha": head, "repo": {"full_name": REPO}},
        "mergeable": true, "mergeable_state": "clean",
    });
    let commits = json!([[{"commit": {"message": "Fix the thing\n\nIt broke. Closes #12"}}]]);
    std::fs::write(state.join("view.json"), view.to_string()).unwrap();
    std::fs::write(state.join("approval.json"), approval.to_string()).unwrap();
    std::fs::write(state.join("pull.json"), pull.to_string()).unwrap();
    std::fs::write(state.join("commits.json"), commits.to_string()).unwrap();
    let script = format!(
        r#"#!/bin/sh
dir='{state}'
printf '%s|%s\n' "$GH_CONFIG_DIR" "$*" >> "$dir/calls"
case "$*" in
  "auth status --hostname github.com --active --json hosts")
    who=octo-cat; [ -n "$GH_CONFIG_DIR" ] && who=oby-reviewer
    echo '{{"hosts": {{"github.com": [{{"active": true, "state": "success", "login": "'$who'"}}]}}}}' ;;
  "pr view 7 --repo {REPO} --json "*) cat "$dir/view.json" ;;
  "api --paginate --slurp repos/{REPO}/pulls/7/reviews?per_page=100")
    if [ -f "$dir/approved" ]; then cat "$dir/approval.json"; else echo '[[]]'; fi ;;
  "api --paginate --slurp repos/{REPO}/pulls/7/commits?per_page=100") cat "$dir/commits.json" ;;
  "api repos/{REPO} --jq .permissions.push") echo true ;;
  "api -X POST repos/{REPO}/pulls/7/reviews "*) touch "$dir/approved"; echo '{{"id": 1}}' ;;
  "api repos/{REPO}/pulls/7") cat "$dir/pull.json" ;;
  "api -X PUT repos/{REPO}/pulls/7/merge "*) touch "$dir/merged"; echo '{{"merged": true}}' ;;
  "api -X DELETE repos/{REPO}/git/refs/heads/oby/fix-7") exit 0 ;;
  *) echo "gh: not in this test" >&2; exit 1 ;;
esac
"#,
        state = state.display()
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn calls(folder: &Path) -> String {
    std::fs::read_to_string(folder.join("gh/calls")).unwrap_or_default()
}

/// A job done and published at `head()` into dev, scanned clean, which Oby's own review approved.
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
    });
    job
}

/// obid with the job kept for its start, the stand-in gh first, and, with `reviewer`, Oby's
/// reviewer account signed in.
fn started(folder: &Folder, reviewer: bool, workflow: Option<&str>) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &stub_agent());
    let home = Path::new(config["home"].as_str().unwrap()).to_path_buf();
    std::fs::create_dir_all(home.join("jobs")).unwrap();
    std::fs::write(
        home.join(format!("jobs/{ID}.json")),
        serde_json::to_vec(&published()).unwrap(),
    )
    .unwrap();
    if reviewer {
        std::fs::create_dir_all(home.join("github/reviewer")).unwrap();
        std::fs::write(
            home.join("github/reviewer/hosts.yml"),
            "github.com:\n    user: oby-reviewer\n",
        )
        .unwrap();
    }
    if let Some(workflow) = workflow {
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
    let bin = stand_in_gh(&folder.0);
    start(&folder.0, &config, Some(&bin))
}

fn job(obid: &Obid) -> Value {
    call(obid, "GET", &format!("/v1/jobs/{ID}"), None).body
}

fn merge_cards(obid: &Obid) -> Vec<Value> {
    call(obid, "GET", "/v1/inbox", None)
        .body
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i["kind"] == "merge")
        .collect()
}

const OBIS: &str =
    r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"reviewer","checks":"github"}"#;

/// With a workflow the owner taught Oby, an approved job's pull request merges by itself: the
/// reviewer account approves the head Oby's review approved, then it squash-merges into dev, says
/// so, and lists what went out.
#[test]
fn a_remembered_workflow_merges_an_approved_pull_request_by_itself_and_says_so() {
    let folder = Folder::new("merge-by-memory");
    let obid = started(&folder, true, Some(OBIS));
    let merged = within(30, "the job to be merged", || {
        let job = job(&obid);
        (job["mergedBy"]["by"] == "memory").then_some(job)
    });
    assert_eq!(merged["mergedBy"]["memory"], "m1a2b3c4d");
    assert_eq!(merged["mergeError"], Value::Null);
    assert_eq!(merged["mergeNeedsYou"], false);
    assert_eq!(merged["pullRequest"]["state"], "MERGED");
    let stage = merged["pipeline"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "merged")
        .cloned()
        .unwrap();
    assert_eq!(stage["state"], "done");
    assert_eq!(stage["detail"], "Merged into dev, as you like");
    let actions: Vec<&str> = merged["outgoing"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|o| o["action"].as_str())
        .collect();
    assert_eq!(
        actions,
        ["push", "pullRequestApproval", "merge", "branchDelete"]
    );
    // The approval went as the reviewer, pinned to the head, before the merge.
    let calls = calls(&folder.0);
    let approval = calls
        .lines()
        .position(|l| l.contains("-X POST repos/obi-e2e/fixture-app/pulls/7/reviews"))
        .unwrap();
    let merge = calls
        .lines()
        .position(|l| l.contains("-X PUT repos/obi-e2e/fixture-app/pulls/7/merge"))
        .unwrap();
    assert!(approval < merge, "{calls}");
    let posted = calls.lines().nth(approval).unwrap();
    assert!(posted.contains("/github/reviewer|"), "{posted}");
    assert!(
        posted.contains(&format!("-f commit_id={} -f event=APPROVE", head())),
        "{posted}"
    );
    assert!(!calls.contains("REQUEST_CHANGES"));
    // The memory it followed counts the use, and no card asked anything.
    let memory = call(&obid, "GET", "/v1/memory", None).body;
    assert_eq!(memory["memories"][0]["used"], 1);
    assert!(merge_cards(&obid).iter().all(|c| c["done"] != Value::Null));
}

/// Without a workflow, nothing merges by itself: the job asks once, "Ready to merge?", and the
/// owner's Merge, and remember merges it and keeps how, typed. The brain can't answer it.
#[test]
fn merge_and_remember_merges_and_keeps_the_workflow() {
    let folder = Folder::new("merge-ask-first");
    let obid = started(&folder, true, None);
    let card = within(30, "the merge card", || {
        merge_cards(&obid).into_iter().next()
    });
    assert_eq!(card["title"], "Ready to merge: Fix the thing");
    assert_eq!(
        card["options"],
        json!(["Merge", "Merge, and remember", "Leave it open"])
    );
    assert_eq!(card["id"], format!("merge-{ID}-{}", head()));
    let asking = job(&obid);
    assert_eq!(asking["mergeNeedsYou"], true);
    assert!(!folder.0.join("gh/merged").exists());
    let wrong = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{}/answer", card["id"].as_str().unwrap()),
        Some(&json!({"answer": "Merge into main"})),
    );
    assert_eq!(wrong.status, 422, "{}", wrong.text);
    let answered = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{}/answer", card["id"].as_str().unwrap()),
        Some(&json!({"answer": "Merge, and remember"})),
    );
    assert_eq!(answered.status, 200, "{}", answered.text);
    assert!(folder.0.join("gh/merged").exists());
    let merged = job(&obid);
    assert_eq!(merged["mergedBy"]["by"], "owner");
    assert_eq!(merged["pullRequest"]["state"], "MERGED");
    let memory = call(&obid, "GET", "/v1/memory", None).body;
    assert_eq!(memory["memories"][0]["kind"], "workflow");
    assert_eq!(
        memory["memories"][0]["workflow"],
        json!({"base": "dev", "method": "squash", "rebaseFirst": false, "review": "reviewer", "checks": "github"})
    );
    assert_eq!(
        memory["memories"][0]["from"],
        "You merged fixture-app#7 this way"
    );
    // A second tap merges nothing.
    let again = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{}/answer", card["id"].as_str().unwrap()),
        Some(&json!({"answer": "Merge"})),
    );
    assert_ne!(again.status, 200, "{}", again.text);
    assert_eq!(calls(&folder.0).matches("-X PUT").count(), 1);
}

/// With no reviewer account signed in, Oby posts nothing and merges nothing: one card says what
/// the owner can do.
#[test]
fn no_reviewer_account_posts_nothing_and_asks_once() {
    let folder = Folder::new("merge-no-reviewer");
    let obid = started(&folder, false, Some(OBIS));
    let card = within(30, "the card", || merge_cards(&obid).into_iter().next());
    assert_eq!(card["title"], "Not merged: Fix the thing");
    assert_eq!(
        card["text"],
        "Sign in Oby's reviewer account to let it approve."
    );
    assert_eq!(card["options"], json!(["Dismiss"]));
    let waiting = job(&obid);
    assert_eq!(waiting["mergeNeedsYou"], true);
    assert_eq!(
        waiting["mergeError"],
        "Sign in Oby's reviewer account to let it approve."
    );
    let calls = calls(&folder.0);
    assert!(!calls.contains("-X POST"), "{calls}");
    assert!(!calls.contains("-X PUT"), "{calls}");
    assert_eq!(merge_cards(&obid).len(), 1);
}
