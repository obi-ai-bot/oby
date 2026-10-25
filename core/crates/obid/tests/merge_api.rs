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
/// merges while `gh/unknown` is), into a dev whose rules ask for no approving review (one while
/// `gh/requires-approval` is there, and whose protection GitHub won't show while
/// `gh/protection-403`, a rate limit, or `gh/protection-sso`, an organization's SSO, is); its
/// reviews are the
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
    [ -n "$GH_CONFIG_DIR" ] && [ -f "$dir/reviewer-stale" ] && {{ echo '{{"hosts": {{"github.com": []}}}}'; exit 0; }}
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
  "api repos/{REPO}/rules/branches/dev")
    if [ -f "$dir/requires-approval" ]; then echo '[{{"type": "pull_request", "parameters": {{"required_approving_review_count": 1}}}}]'; else echo '[]'; fi ;;
  "api repos/{REPO}/branches/dev/protection/required_pull_request_reviews")
    if [ -f "$dir/protection-403" ]; then echo '{{"message": "API rate limit exceeded for user ID 1.", "status": "403"}}'; echo 'gh: API rate limit exceeded (HTTP 403)' >&2; exit 1; fi
    if [ -f "$dir/protection-sso" ]; then echo '{{"message": "Resource protected by organization SAML enforcement. You must grant your Personal Access token access to this organization.", "status": "403"}}'; echo 'gh: Resource protected by organization SAML enforcement. (HTTP 403)' >&2; exit 1; fi
    echo '{{"message": "Branch not protected", "status": "404"}}'; echo 'gh: Branch not protected (HTTP 404)' >&2; exit 1 ;;
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
        json!({"base": "dev", "method": "squash", "rebaseFirst": false, "review": "oby", "checks": "github"})
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
    // Nothing posted as a reviewer: Oby's own review was the review.
    assert!(!calls(&folder.0).contains("-X POST"));
}

const OBY: &str =
    r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"oby","checks":"github"}"#;

/// Oby's own review of the exact head is the review for the owner's own pull request, and the
/// owner's own login merges it (the owner's decision of 2026-10-24): no reviewer account, no
/// card. A head its review didn't approve doesn't merge.
#[test]
fn an_oby_workflow_merges_on_obys_own_review_of_the_head() {
    let folder = Folder::new("merge-oby");
    let obid = started(&folder, false, Some(OBY));
    let merged = within(30, "the job to be merged", || {
        let job = job(&obid);
        (job["mergedBy"]["by"] == "memory").then_some(job)
    });
    assert_eq!(merged["mergeNeedsYou"], false);
    let sent = calls(&folder.0);
    assert!(!sent.contains("-X POST"), "{sent}");
    assert!(
        sent.contains(&format!("-X PUT repos/{REPO}/pulls/7/merge")),
        "{sent}"
    );
    assert!(merge_cards(&obid).is_empty());
    // The same with a reviewer workflow and no reviewer signed in: that account is an extra.
    drop(obid);
    let folder = Folder::new("merge-reviewer-optional");
    let obid = started(&folder, false, Some(OBIS));
    within(30, "the job to be merged", || {
        (job(&obid)["mergedBy"]["by"] == "memory").then_some(())
    });
    assert!(merge_cards(&obid).is_empty());
    drop(obid);
    // Its review approved an older commit: the head GitHub has waits for the next review.
    let folder = Folder::new("merge-oby-moved");
    let mut moved = published();
    moved.review.as_mut().unwrap().commit = "c".repeat(40);
    let obid = started_with(&folder, false, Some(OBY), moved, &[]);
    within(30, "a few reads", || {
        (calls(&folder.0).matches("pr view 7").count() >= 3).then_some(())
    });
    let waiting = job(&obid);
    assert_eq!(waiting["mergeNeedsYou"], false);
    assert!(waiting["mergedBy"].is_null());
    assert!(!calls(&folder.0).contains("-X PUT"));
    assert!(merge_cards(&obid).is_empty());
}

/// Where GitHub's rules require an approving review, the owner's own can't count: a teammate's
/// Oby is asked, and when none can, the owner hears it once. Another person's approval merges it.
#[test]
fn a_required_approval_goes_to_a_teammate_or_the_owner() {
    let folder = Folder::new("merge-required");
    let obid = started_with(
        &folder,
        false,
        Some(OBY),
        published(),
        &["requires-approval"],
    );
    let card = within(30, "the card", || merge_cards(&obid).into_iter().next());
    assert_eq!(
        card["text"],
        "GitHub requires another person's approval here. Oby isn't in an Obysphere, so no teammate's Oby can approve it. Invite a teammate who can push to obi-e2e/fixture-app to an Obysphere with you, or get their approval on GitHub yourself."
    );
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert_eq!(merge_cards(&obid).len(), 1);
    assert!(!calls(&folder.0).contains("-X PUT"));
    std::fs::write(folder.0.join("gh/person"), "").unwrap();
    within(30, "the job to be merged", || {
        (job(&obid)["mergedBy"]["by"] == "memory").then_some(())
    });
    assert_eq!(job(&obid)["mergeNeedsYou"], false);
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

/// A repository Oby hasn't learned asks first, with no reviewer account at all: "Ready to merge?"
/// proposes Oby's own review, and nothing is posted as a reviewer.
#[test]
fn an_unlearned_repository_asks_first_with_no_reviewer() {
    let folder = Folder::new("merge-ask-no-reviewer");
    let obid = started_with(&folder, false, None, published(), &[]);
    let ready = within(30, "the ready card", || {
        merge_cards(&obid)
            .into_iter()
            .find(|c| c["id"] == format!("merge-ready-{ID}-{}", head()) && c["done"].is_null())
    });
    assert!(
        ready["details"][1]
            .as_str()
            .unwrap()
            .contains("after Oby's own review approves"),
        "{ready}"
    );
    assert_eq!(job(&obid)["mergeError"], ready["text"]);
    assert!(!calls(&folder.0).contains("-X POST"));
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
    let obid = started_with(
        &folder,
        false,
        Some(OBY),
        published(),
        &["requires-approval"],
    );
    let card = within(30, "the card saying why", || {
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

const TEAMMATE: &str =
    r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"teammate","checks":"github"}"#;

/// A repository a teammate approves (Tetrix's): with no teammate's approval of the head, the
/// follower asks a teammate's Oby over the Obysphere; one not in an Obysphere can't, and the owner
/// hears why, once. Oby's own reviewer account posts nothing there: GitHub's branch rules would
/// count its approval, and it never stands in for a teammate's.
#[test]
fn a_teammate_workflow_asks_a_teammates_oby() {
    let folder = Folder::new("merge-teammate");
    let obid = started_with(&folder, true, Some(TEAMMATE), published(), &[]);
    let card = within(30, "the card", || merge_cards(&obid).into_iter().next());
    assert_eq!(
        card["text"],
        "Oby isn't in an Obysphere, so no teammate's Oby can approve it. Invite a teammate who can push to obi-e2e/fixture-app to an Obysphere with you, or get their approval on GitHub yourself."
    );
    let waiting = job(&obid);
    assert_eq!(waiting["mergeNeedsYou"], true);
    // Its reviewer account (signed in, able to push) posted nothing, and nothing merged.
    let calls = calls(&folder.0);
    assert!(!calls.contains("-X POST"), "{calls}");
    assert!(!calls.contains("-X PUT"), "{calls}");
    // A teammate approved it on GitHub meanwhile: it merges, and nobody is asked.
    std::fs::write(folder.0.join("gh/person"), "").unwrap();
    let merged = within(30, "the job to be merged", || {
        let job = job(&obid);
        (job["mergedBy"]["by"] == "memory").then_some(job)
    });
    assert_eq!(merged["mergeNeedsYou"], false);
}

/// GitHub's protection that can't be read (a rate limit, a 5xx) is never taken for none: Oby's own
/// review waits, and nobody is asked anything.
#[test]
fn unreadable_protection_merges_nothing() {
    let folder = Folder::new("merge-protection-403");
    let obid = started_with(&folder, false, Some(OBY), published(), &["protection-403"]);
    within(30, "a few reads", || {
        (calls(&folder.0).matches("pr view 7").count() >= 3).then_some(())
    });
    let waiting = job(&obid);
    assert_eq!(waiting["mergeNeedsYou"], false);
    assert!(waiting["mergedBy"].is_null());
    assert!(!calls(&folder.0).contains("-X PUT"));
    assert!(merge_cards(&obid).is_empty());
}

/// Merge rules GitHub won't show in a way that won't pass by itself (an organization's SSO not
/// authorized for the owner's login) ask the owner once for that head, in GitHub's words, and
/// nothing merges; once GitHub shows them, the card settles and it merges.
#[test]
fn merge_rules_github_wont_show_ask_the_owner_once() {
    let folder = Folder::new("merge-protection-sso");
    let obid = started_with(&folder, false, Some(OBY), published(), &["protection-sso"]);
    let card = within(30, "the card", || merge_cards(&obid).into_iter().next());
    assert_eq!(
        card["text"],
        "Oby can't read this repository's merge rules (GitHub said: Resource protected by organization SAML enforcement. You must grant your Personal Access token access to this organization.)."
    );
    let read = calls(&folder.0).matches("pr view 7").count();
    within(30, "a few more reads", || {
        (calls(&folder.0).matches("pr view 7").count() >= read + 3).then_some(())
    });
    let cards = merge_cards(&obid);
    assert_eq!(cards.len(), 1, "{cards:?}");
    assert_eq!(cards[0]["id"], card["id"]);
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
    assert!(!calls(&folder.0).contains("-X PUT"));
    std::fs::remove_file(folder.0.join("gh/protection-sso")).unwrap();
    within(30, "the job to be merged", || {
        (job(&obid)["mergedBy"]["by"] == "memory").then_some(())
    });
    // The merge is saved, then the card settles.
    within(30, "the card to settle", || {
        merge_cards(&obid).is_empty().then_some(())
    });
}

/// A `reviewer` workflow whose reviewer login isn't signed in any more waits on that account for
/// good: the owner hears so once for that head, and nothing goes out; signed in again, the card
/// settles, the reviewer approves and it merges.
#[test]
fn a_stale_reviewer_login_on_a_reviewer_workflow_asks_the_owner_once() {
    let folder = Folder::new("merge-stale-reviewer-workflow");
    let obid = started_with(&folder, true, Some(OBIS), published(), &["reviewer-stale"]);
    let card = within(30, "the card", || merge_cards(&obid).into_iter().next());
    assert_eq!(
        card["text"],
        "Oby's reviewer account needs signing in again, or signing out (the reviewer's GitHub login isn't signed in any more)."
    );
    let read = calls(&folder.0).matches("pr view 7").count();
    within(30, "a few more reads", || {
        (calls(&folder.0).matches("pr view 7").count() >= read + 3).then_some(())
    });
    let cards = merge_cards(&obid);
    assert_eq!(cards.len(), 1, "{cards:?}");
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
    let sent = calls(&folder.0);
    assert!(!sent.contains("-X POST"), "{sent}");
    assert!(!sent.contains("-X PUT"), "{sent}");
    std::fs::remove_file(folder.0.join("gh/reviewer-stale")).unwrap();
    within(30, "the job to be merged", || {
        (job(&obid)["mergedBy"]["by"] == "memory").then_some(())
    });
    // The merge is saved, then the card settles.
    within(30, "the card to settle", || {
        merge_cards(&obid).is_empty().then_some(())
    });
}

/// A repository Oby hasn't learned whose rules require an approving review, out of an
/// Obysphere: the owner hears, once, that no teammate's Oby can review it and what to do instead;
/// nothing is asked or sent.
#[test]
fn an_unlearned_repository_out_of_a_sphere_says_what_to_do() {
    let folder = Folder::new("merge-required-unlearned");
    let obid = started_with(&folder, false, None, published(), &["requires-approval"]);
    let card = within(30, "the card", || merge_cards(&obid).into_iter().next());
    assert_eq!(
        card["text"],
        "GitHub requires another person's approval on obi-e2e/fixture-app, and Oby isn't in an Obysphere, so no teammate's Oby can review it. Invite a teammate who can push to obi-e2e/fixture-app to an Obysphere with you, or get their approval on GitHub yourself."
    );
    assert_eq!(merge_cards(&obid).len(), 1);
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// The job's pull request into a dev whose rules require an approving review (`workflow` the one
/// Oby learned, if any), with obid in a fake Obysphere where ernesto's Oby takes reviews.
async fn in_a_sphere(folder: &Folder, workflow: Option<&str>) -> (FakeSphere, Obid, TestObi) {
    let sphere = FakeSphere::start().await;
    let obid = tokio::task::block_in_place(|| {
        started_with(folder, false, workflow, published(), &["requires-approval"])
    });
    let joined = tokio::task::block_in_place(|| join_a(&obid, &sphere, &Owner::new("wellington")));
    assert_eq!(joined["state"], "joined");
    let ernesto = TestObi::join(&sphere, "obi-ernesto", "ernesto").await;
    (sphere, obid, ernesto)
}

/// The card asking whether a teammate's Oby reviews the job's head.
fn teammate_card(obid: &Obid) -> Value {
    let id = format!("merge-teammate-{ID}-{}", head());
    tokio::task::block_in_place(|| {
        within(30, "the teammate card", || {
            merge_cards(obid).into_iter().find(|c| c["id"] == id)
        })
    })
}

fn answer(obid: &Obid, card: &Value, answer: &str) -> Answer {
    tokio::task::block_in_place(|| {
        call(
            obid,
            "POST",
            &format!("/v1/inbox/{}/answer", card["id"].as_str().unwrap()),
            Some(&json!({"answer": answer})),
        )
    })
}

/// A repository Oby hasn't learned whose rules require another person's approval asks the owner
/// once, for that head, whether a teammate's Oby reviews it (the owner's decision of 2026-10-25).
/// Their yes keeps the repository's workflow with a teammate's review, the rest as proposed, and
/// asks a teammate's Oby right away; nothing waits on the owner after that.
#[tokio::test(flavor = "multi_thread")]
async fn a_yes_remembers_a_teammate_for_the_repository_and_asks_one() {
    let folder = Folder::new("merge-teammate-yes");
    let (_sphere, obid, ernesto) = in_a_sphere(&folder, None).await;
    let card = teammate_card(&obid);
    assert_eq!(card["title"], "Needs a teammate: Fix the thing");
    assert_eq!(
        card["text"],
        "GitHub requires another person's approval on obi-e2e/fixture-app. Ask a teammate's Oby to review it, and remember that for this repository?"
    );
    assert_eq!(
        card["options"],
        json!(["Ask a teammate, and remember", "Not now"])
    );
    assert_eq!(
        tokio::task::block_in_place(|| job(&obid))["mergeNeedsYou"],
        true
    );
    let answered = answer(&obid, &card, "Ask a teammate, and remember");
    assert_eq!(answered.status, 200, "{}", answered.text);
    assert_eq!(
        answered.body["note"],
        "Asked ernesto's Oby to review it and approve it on GitHub."
    );
    let asked = ernesto.receive(1, 20).await;
    assert_eq!(asked[0]["type"], "review_request");
    assert_eq!(asked[0]["body"]["repo"], REPO);
    assert_eq!(asked[0]["body"]["pr"], 7);
    assert_eq!(asked[0]["body"]["head_sha"], head());
    let memory = tokio::task::block_in_place(|| call(&obid, "GET", "/v1/memory", None).body);
    assert_eq!(memory["memories"][0]["kind"], "workflow");
    assert_eq!(
        memory["memories"][0]["workflow"],
        json!({"base": "dev", "method": "squash", "rebaseFirst": false, "review": "teammate", "checks": "github"})
    );
    assert_eq!(
        memory["memories"][0]["from"],
        "You asked a teammate's Oby to review fixture-app#7"
    );
    // ernesto's Oby has it: nothing waits on the owner, and nothing merged.
    tokio::task::block_in_place(|| {
        within(30, "nothing to wait on the owner", || {
            (merge_cards(&obid).is_empty() && job(&obid)["mergeNeedsYou"] == false).then_some(())
        })
    });
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// A repository whose workflow the owner's yes made a teammate's: every pull request there goes to
/// a teammate's Oby by itself, with no card.
#[tokio::test(flavor = "multi_thread")]
async fn a_learned_teammate_workflow_asks_a_teammate_with_no_card() {
    let folder = Folder::new("merge-teammate-learned");
    let (_sphere, obid, ernesto) = in_a_sphere(&folder, Some(TEAMMATE)).await;
    let asked = ernesto.receive(1, 30).await;
    assert_eq!(asked[0]["type"], "review_request");
    assert_eq!(asked[0]["body"]["pr"], 7);
    tokio::task::block_in_place(|| {
        within(30, "nothing to wait on the owner", || {
            (merge_cards(&obid).is_empty() && job(&obid)["mergeNeedsYou"] == false).then_some(())
        })
    });
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// "Not now": nothing is asked, sent or remembered, and that head waits quietly.
#[tokio::test(flavor = "multi_thread")]
async fn not_now_keeps_waiting_quietly() {
    let folder = Folder::new("merge-teammate-not-now");
    let (_sphere, obid, ernesto) = in_a_sphere(&folder, None).await;
    let card = teammate_card(&obid);
    let answered = answer(&obid, &card, "Not now");
    assert_eq!(answered.status, 200, "{}", answered.text);
    let read = calls(&folder.0).matches("pr view 7").count();
    tokio::task::block_in_place(|| {
        within(30, "a few more reads", || {
            (calls(&folder.0).matches("pr view 7").count() >= read + 3).then_some(())
        })
    });
    let quiet = tokio::task::block_in_place(|| (merge_cards(&obid), job(&obid)));
    assert!(quiet.0.is_empty(), "{:?}", quiet.0);
    assert_eq!(quiet.1["mergeNeedsYou"], false);
    assert!(quiet.1["mergeError"].is_null());
    let memory = tokio::task::block_in_place(|| call(&obid, "GET", "/v1/memory", None).body);
    assert!(
        memory["memories"]
            .as_array()
            .is_none_or(|m| m.iter().all(|m| m["kind"] != "workflow")),
        "{memory}"
    );
    let sent = ernesto
        .client
        .deliveries(&ernesto.token, 50, 1)
        .await
        .unwrap();
    assert!(sent.is_empty(), "{sent:?}");
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// A reviewer account that signed in once and isn't signed in any more never stalls an `oby`
/// merge: that account matters only to a `reviewer` workflow.
#[test]
fn a_stale_reviewer_login_doesnt_stall_an_oby_merge() {
    let folder = Folder::new("merge-stale-reviewer");
    let obid = started_with(&folder, true, Some(OBY), published(), &["reviewer-stale"]);
    within(30, "the job to be merged", || {
        (job(&obid)["mergedBy"]["by"] == "memory").then_some(())
    });
    assert!(!calls(&folder.0).contains("-X POST"));
}
