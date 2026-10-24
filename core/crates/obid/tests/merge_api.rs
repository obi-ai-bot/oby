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
/// `head()` (merged on GitHub once `gh/merged-on-github` is there, still working out whether it
/// merges while `gh/unknown` is); its reviews are the
/// reviewer's approval once it posted one (`gh/approved`), someone's dismissal of it
/// (`gh/dismissed`) and a person's approval (`gh/person`); the merge goes through.
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
    let mut unknown = view.clone();
    unknown["mergeable"] = json!("UNKNOWN");
    unknown["mergeStateStatus"] = json!("UNKNOWN");
    std::fs::write(state.join("view-unknown.json"), unknown.to_string()).unwrap();
    let mut merged = view.clone();
    merged["state"] = json!("MERGED");
    merged["mergedAt"] = json!("2026-10-24T18:00:00Z");
    let review = |id: u64, login: &str, state: &str, association: &str| {
        json!({"id": id, "user": {"login": login, "type": "User"}, "state": state,
               "commit_id": head, "author_association": association, "body": ""})
    };
    std::fs::write(state.join("view-merged.json"), merged.to_string()).unwrap();
    for (file, review) in [
        (
            "approval.json",
            review(1, "oby-reviewer", "APPROVED", "COLLABORATOR"),
        ),
        (
            "dismissal.json",
            review(2, "oby-reviewer", "DISMISSED", "COLLABORATOR"),
        ),
        ("person.json", review(3, "ana", "APPROVED", "MEMBER")),
    ] {
        std::fs::write(state.join(file), review.to_string()).unwrap();
    }
    let pull = json!({
        "state": "open", "merged": false, "title": "Fix the thing",
        "base": {"ref": "dev", "repo": {"full_name": REPO}},
        "head": {"ref": "oby/fix-7", "sha": head, "repo": {"full_name": REPO}},
        "mergeable": true, "mergeable_state": "clean",
    });
    let commits = json!([[{"commit": {"message": "Fix the thing\n\nIt broke. Closes #12"}}]]);
    std::fs::write(state.join("view.json"), view.to_string()).unwrap();
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
  "pr view 7 --repo {REPO} --json "*)
    if [ -f "$dir/merged-on-github" ]; then cat "$dir/view-merged.json";
    elif [ -f "$dir/unknown" ]; then cat "$dir/view-unknown.json";
    else cat "$dir/view.json"; fi ;;
  "api --paginate --slurp repos/{REPO}/pulls/7/reviews?per_page=100")
    r=
    [ -f "$dir/approved" ] && r="$(cat "$dir/approval.json")"
    [ -f "$dir/dismissed" ] && r="$r,$(cat "$dir/dismissal.json")"
    [ -f "$dir/person" ] && r="$r,$(cat "$dir/person.json")"
    echo "[[${{r#,}}]]" ;;
  "api --paginate --slurp repos/{REPO}/pulls/7/commits?per_page=100") cat "$dir/commits.json" ;;
  "api repos/{REPO} --jq .permissions.push") echo true ;;
  "api -X POST repos/{REPO}/pulls/7/reviews "*) touch "$dir/approved"; echo '{{"id": 1}}' ;;
  "api repos/{REPO}/pulls/7") cat "$dir/pull.json" ;;
  "api -X PUT repos/{REPO}/pulls/7/merge "*) touch "$dir/merged"; echo '{{"merged": true}}' ;;
  "api -X DELETE repos/{REPO}/git/refs/heads/oby/fix-7") touch "$dir/deleted"; exit 0 ;;
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
    started_with(folder, reviewer, workflow, published(), &[])
}

/// [`started`] with `job` kept, and `gh` files there before it starts. It reads the pull request
/// every second.
fn started_with(
    folder: &Folder,
    reviewer: bool,
    workflow: Option<&str>,
    job: obid::job::Job,
    gh: &[&str],
) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &stub_agent());
    let home = Path::new(config["home"].as_str().unwrap()).to_path_buf();
    std::fs::create_dir_all(home.join("jobs")).unwrap();
    std::fs::write(
        home.join(format!("jobs/{ID}.json")),
        serde_json::to_vec(&job).unwrap(),
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
    for file in gh {
        std::fs::write(folder.0.join("gh").join(file), "").unwrap();
    }
    start_with(
        &folder.0,
        &config,
        Some(&bin),
        &[("OBID_FOLLOW_SECONDS", "1")],
    )
}

/// Signs Oby's reviewer account in, as a sign-in leaves it.
fn sign_in_reviewer(folder: &Folder) {
    let dir = folder.0.join("home/github/reviewer");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("hosts.yml"),
        "github.com:\n    user: oby-reviewer\n",
    )
    .unwrap();
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
    assert_eq!(card["id"], format!("merge-ready-{ID}-{}", head()));
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

const PERSON: &str =
    r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"person","checks":"github"}"#;

/// A workflow a person approves merges with no reviewer account signed in: Oby's approval isn't
/// what it waits on, so nobody is asked to sign one in.
#[test]
fn a_person_workflow_merges_without_a_reviewer_account() {
    let folder = Folder::new("merge-person");
    let obid = started_with(&folder, false, Some(PERSON), published(), &["person"]);
    let merged = within(30, "the job to be merged", || {
        let job = job(&obid);
        (job["mergedBy"]["by"] == "memory").then_some(job)
    });
    assert_eq!(merged["mergeNeedsYou"], false);
    assert!(!calls(&folder.0).contains("-X POST"));
    assert!(merge_cards(&obid).is_empty());
}

/// "Sign in Oby's reviewer account" gives way to "Ready to merge?" once it's signed in: each its
/// own card, the first settled.
#[test]
fn signing_the_reviewer_in_turns_the_card_into_ready_to_merge() {
    let folder = Folder::new("merge-card-switch");
    let obid = started_with(&folder, false, None, published(), &[]);
    let signin = within(30, "the sign-in card", || {
        merge_cards(&obid)
            .into_iter()
            .find(|c| c["text"] == "Sign in Oby's reviewer account to let it approve.")
    });
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
    sign_in_reviewer(&folder);
    let ready = within(30, "the ready card", || {
        merge_cards(&obid)
            .into_iter()
            .find(|c| c["id"] == format!("merge-ready-{ID}-{}", head()) && c["done"].is_null())
    });
    assert_eq!(ready["title"], "Ready to merge: Fix the thing");
    // The inbox shows open items: the sign-in card is settled.
    let open: Vec<Value> = merge_cards(&obid)
        .into_iter()
        .filter(|c| c["done"].is_null())
        .collect();
    assert_eq!(open.len(), 1, "{open:?}");
    assert_ne!(open[0]["id"], signin["id"]);
    let now = job(&obid);
    assert_eq!(now["mergeNeedsYou"], true);
    assert_eq!(now["mergeError"], ready["text"]);
    assert_eq!(calls(&folder.0).matches("-X POST").count(), 1);
}

/// An approval someone dismissed on GitHub isn't posted again: the merge waits on the owner, with
/// one card, however many times it's read.
#[test]
fn a_dismissed_approval_is_never_posted_again() {
    let folder = Folder::new("merge-dismissed");
    let obid = started_with(
        &folder,
        true,
        Some(OBIS),
        published(),
        &["approved", "dismissed"],
    );
    let card = within(30, "the card", || merge_cards(&obid).into_iter().next());
    assert!(
        card["text"]
            .as_str()
            .unwrap()
            .starts_with("Someone dismissed Oby's approval of this commit"),
        "{card}"
    );
    // A few more reads.
    std::thread::sleep(std::time::Duration::from_secs(3));
    let calls = calls(&folder.0);
    assert!(calls.matches("pr view 7").count() >= 2, "{calls}");
    assert!(!calls.contains("-X POST"), "{calls}");
    assert!(!calls.contains("-X PUT"), "{calls}");
    assert_eq!(merge_cards(&obid).len(), 1);
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
}

/// The owner merges it on GitHub while "Ready to merge?" is up: nothing waits on them any more,
/// and the job says GitHub merged it.
#[test]
fn a_pull_request_merged_on_github_needs_nobody() {
    let folder = Folder::new("merge-on-github");
    let obid = started_with(&folder, true, None, published(), &[]);
    let card = within(30, "the ready card", || {
        merge_cards(&obid).into_iter().next()
    });
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
    std::fs::write(folder.0.join("gh/merged-on-github"), "").unwrap();
    let merged = within(30, "the job to see it merged", || {
        let job = job(&obid);
        (job["pullRequest"]["state"] == "MERGED").then_some(job)
    });
    assert_eq!(merged["mergeNeedsYou"], false);
    assert_eq!(merged["mergeError"], Value::Null);
    assert_eq!(merged["mergedBy"]["by"], "github");
    assert_eq!(merged["mergedBy"]["at"], "2026-10-24T18:00:00Z");
    // Its card is settled: no open one is left.
    assert!(
        merge_cards(&obid)
            .iter()
            .all(|c| c["id"] != card["id"] || !c["done"].is_null()),
        "{card}"
    );
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// What obid was sending when it stopped is settled from GitHub when it starts again: a merge
/// that landed is listed, as its workflow's; an approval that went out is listed once, and
/// neither is sent again.
#[test]
fn a_restart_between_sending_and_saving_lists_what_went_out() {
    let folder = Folder::new("merge-restart-merge");
    let mut sending = published();
    sending.sending = vec![
        obid::job::Sending {
            action: "merge".into(),
            head: head(),
            by: Some(obid::job::MergedBy {
                by: "memory".into(),
                memory: Some("m1a2b3c4d".into()),
                at: 0,
            }),
            login: None,
        },
        // Its branch was being deleted too: deleted again, listed once.
        obid::job::Sending {
            action: "branchDelete".into(),
            head: head(),
            by: None,
            login: None,
        },
    ];
    let obid = started_with(
        &folder,
        true,
        Some(OBIS),
        sending,
        &["approved", "merged-on-github"],
    );
    let merged = within(30, "the merge to be listed", || {
        let job = job(&obid);
        (job["mergedBy"]["by"] == "memory").then_some(job)
    });
    assert_eq!(merged["mergedBy"]["at"], "2026-10-24T18:00:00Z");
    let deleted = within(30, "the branch's deletion to be listed", || {
        let job = job(&obid);
        let actions: Vec<String> = job["outgoing"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| o["action"].as_str().map(str::to_owned))
            .collect();
        (actions.len() == 3).then_some(actions)
    });
    assert_eq!(deleted, ["push", "merge", "branchDelete"]);
    assert!(folder.0.join("gh/deleted").exists());
    assert!(!calls(&folder.0).contains("-X PUT"));
    drop(obid);

    let folder = Folder::new("merge-restart-approval");
    let mut sending = published();
    sending.sending = vec![obid::job::Sending {
        action: "pullRequestApproval".into(),
        head: head(),
        by: None,
        login: Some("oby-reviewer".into()),
    }];
    let obid = started_with(&folder, true, None, sending, &["approved"]);
    let listed = within(30, "the approval to be listed", || {
        let job = job(&obid);
        job["outgoing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["action"] == "pullRequestApproval")
            .then_some(job)
    });
    std::thread::sleep(std::time::Duration::from_secs(2));
    let actions: Vec<Value> = job(&obid)["outgoing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["action"].clone())
        .collect();
    assert_eq!(
        actions,
        [json!("push"), json!("pullRequestApproval")],
        "{listed}"
    );
    assert!(!calls(&folder.0).contains("-X POST"));
}

/// A "Ready to merge?" card the follower settled by itself, while GitHub worked out whether it
/// merges, is offered again once it does: only the owner's own answer leaves it.
#[test]
fn a_ready_card_comes_back_when_the_pull_request_is_ready_again() {
    let folder = Folder::new("merge-card-back");
    let obid = started_with(&folder, true, None, published(), &[]);
    let open = |obid: &Obid| {
        merge_cards(obid)
            .into_iter()
            .find(|c| c["id"] == format!("merge-ready-{ID}-{}", head()) && c["done"].is_null())
    };
    within(30, "the ready card", || open(&obid));
    std::fs::write(folder.0.join("gh/unknown"), "").unwrap();
    within(30, "the card to be settled", || {
        (open(&obid).is_none() && job(&obid)["mergeNeedsYou"] == false).then_some(())
    });
    std::fs::remove_file(folder.0.join("gh/unknown")).unwrap();
    let back = within(30, "the card to come back", || open(&obid));
    assert_eq!(back["title"], "Ready to merge: Fix the thing");
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
    // Left open by the owner: not offered again, and nothing waits on them.
    let left = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{}/answer", back["id"].as_str().unwrap()),
        Some(&json!({"answer": "Leave it open"})),
    );
    assert_eq!(left.status, 200, "{}", left.text);
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(open(&obid).is_none());
    assert_eq!(job(&obid)["mergeNeedsYou"], false);
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// The owner's Dismiss of a card saying why holds for that reason and head: nothing waits on
/// them, and the card isn't raised again.
#[test]
fn a_dismissed_reason_card_stays_dismissed() {
    let folder = Folder::new("merge-reason-dismissed");
    let obid = started(&folder, false, Some(OBIS));
    let card = within(30, "the sign-in card", || {
        merge_cards(&obid).into_iter().next()
    });
    let dismissed = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{}/answer", card["id"].as_str().unwrap()),
        Some(&json!({"done": true})),
    );
    assert_eq!(dismissed.status, 200, "{}", dismissed.text);
    assert_eq!(dismissed.body["answer"], "Dismiss");
    within(30, "nothing to wait on the owner", || {
        (job(&obid)["mergeNeedsYou"] == false).then_some(())
    });
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(merge_cards(&obid).iter().all(|c| !c["done"].is_null()));
    assert_eq!(job(&obid)["mergeNeedsYou"], false);
}

/// An approval obid sent right before it stopped, on a pull request merged before it started
/// again, is listed: the mark is settled from GitHub, not dropped.
#[test]
fn an_approval_sent_before_the_pull_request_merged_is_listed() {
    let folder = Folder::new("merge-restart-merged-approval");
    let mut sending = published();
    sending.sending = vec![obid::job::Sending {
        action: "pullRequestApproval".into(),
        head: head(),
        by: None,
        login: Some("oby-reviewer".into()),
    }];
    let obid = started_with(
        &folder,
        true,
        None,
        sending,
        &["approved", "merged-on-github"],
    );
    let merged = within(30, "the job to see it merged", || {
        let job = job(&obid);
        (job["mergedBy"]["by"] == "github").then_some(job)
    });
    let actions: Vec<&str> = merged["outgoing"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|o| o["action"].as_str())
        .collect();
    assert_eq!(actions, ["push", "pullRequestApproval"]);
    assert!(!calls(&folder.0).contains("-X POST"));
}
