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
/// (`gh/dismissed`) and a person's approval (`gh/person`); the merge goes through. `gh/view-now.json`,
/// when a test writes one, is what `pr view` prints instead. With `gh/signed-out` the owner's gh is
/// signed in to nothing, as gh 2.102 says it (`{"hosts":{}}`, and `pr view` exits 4); with
/// `gh/offline` it can't reach GitHub at all.
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
    [ -f "$dir/offline" ] && {{ echo 'error connecting to api.github.com' >&2; exit 1; }}
    [ -z "$GH_CONFIG_DIR" ] && [ -f "$dir/signed-out" ] && {{ echo 'You are not logged into any GitHub hosts. To log in, run: gh auth login' >&2; echo '{{"hosts":{{}}}}'; exit 0; }}
    [ -n "$GH_CONFIG_DIR" ] && [ -f "$dir/reviewer-stale" ] && {{ echo '{{"hosts": {{"github.com": []}}}}'; exit 0; }}
    echo '{{"hosts": {{"github.com": [{{"active": true, "state": "success", "login": "'$who'"}}]}}}}' ;;
  "pr view 7 --repo {REPO} --json "*)
    [ -f "$dir/offline" ] && {{ echo 'error connecting to api.github.com' >&2; exit 1; }}
    [ -f "$dir/signed-out" ] && {{ printf 'To get started with GitHub CLI, please run:  gh auth login\n' >&2; exit 4; }}
    if [ -f "$dir/view-now.json" ]; then cat "$dir/view-now.json";
    elif [ -f "$dir/merged-on-github" ]; then cat "$dir/view-merged.json";
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
  "api repos/{REPO}/rules/branches/"*)
    if [ -f "$dir/requires-approval" ]; then echo '[{{"type": "pull_request", "parameters": {{"required_approving_review_count": 1}}}}]'; else echo '[]'; fi ;;
  "api repos/{REPO}/branches/"*"/protection/required_pull_request_reviews")
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
        two_reviews: None,
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
    started_with_jobs(folder, reviewer, workflow, &[job], gh)
}

/// [`started_with`] with every one of `jobs` kept.
fn started_with_jobs(
    folder: &Folder,
    reviewer: bool,
    workflow: Option<&str>,
    jobs: &[obid::job::Job],
    gh: &[&str],
) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &stub_agent());
    let home = Path::new(config["home"].as_str().unwrap()).to_path_buf();
    std::fs::create_dir_all(home.join("jobs")).unwrap();
    for job in jobs {
        std::fs::write(
            home.join(format!("jobs/{}.json", job.id)),
            serde_json::to_vec(job).unwrap(),
        )
        .unwrap();
    }
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
    job_of(obid, ID)
}

fn job_of(obid: &Obid, id: &str) -> Value {
    call(obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

const FOLLOW_UP: &str = "f1b2c3d4";

/// The task that opened the pull request at an earlier head, and its follow-up, which pushed
/// `head()` to the same branch since and added to that pull request (`addedTo`, #1663). Oby's own
/// review approved each one's head.
fn task_and_follow_up() -> [obid::job::Job; 2] {
    let earlier = "b".repeat(40);
    let mut task = published();
    task.outgoing = vec![
        obid::publish::Outgoing::new("push", REPO, &earlier, Some("oby/fix-7")),
        obid::publish::Outgoing::new("pullRequest", REPO, PULL, None),
    ];
    task.outgoing[0].date -= 60;
    task.review.as_mut().unwrap().commit = earlier;
    let mut follow_up = published();
    follow_up.id = FOLLOW_UP.into();
    follow_up.title = Some("Fix the thing follow-up".into());
    follow_up.follow_up_of = Some(ID.into());
    follow_up.created += 1;
    [task, follow_up]
}

/// The merge cards still open.
fn open_merge_cards(obid: &Obid) -> Vec<Value> {
    merge_cards(obid)
        .into_iter()
        .filter(|c| c["done"].is_null())
        .collect()
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

/// MERGING-21 and 20: in a repository Oby hasn't learned, a pull request into main whose checks
/// failed asks the owner: main merges only on their word, and nothing of Oby's fixes its own pull
/// request's checks. Its card offers Merge. An answer no app of theirs signed merges nothing (red
/// checks weren't Oby's to lift before); their signed Merge merges it as it is.
#[test]
fn merging_21_red_checks_into_main_ask_the_owner_and_their_merge_merges() {
    let folder = Folder::new("merge-red-main");
    let mut published = published();
    published.base = Some("main".into());
    let obid = started_with(&folder, false, None, published, &[]);
    let file = folder.0.join("gh/view.json");
    let mut view: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    view["baseRefName"] = json!("main");
    view["statusCheckRollup"][0]["conclusion"] = json!("FAILURE");
    std::fs::write(&file, view.to_string()).unwrap();
    let card = within(30, "the owner's card", || {
        merge_cards(&obid).into_iter().find(|c| {
            c["done"].is_null()
                && c["text"]
                    .as_str()
                    .is_some_and(|t| t.contains("Not merged yet: its checks failed on GitHub."))
        })
    });
    assert_eq!(card["options"], json!(["Merge", "Leave it open"]), "{card}");
    assert!(!calls(&folder.0).contains("-X PUT"));
    let pull = folder.0.join("gh/pull.json");
    let mut rest: Value = serde_json::from_slice(&std::fs::read(&pull).unwrap()).unwrap();
    rest["base"]["ref"] = json!("main");
    std::fs::write(&pull, rest.to_string()).unwrap();
    let unsigned = answer(&obid, &card, "Merge");
    assert_eq!(unsigned.status, 409, "{}", unsigned.text);
    assert!(
        unsigned.text.contains("your Merge didn't cover this"),
        "{}",
        unsigned.text
    );
    assert!(!calls(&folder.0).contains("-X PUT"));
    let card = within(30, "the card again", || {
        merge_cards(&obid)
            .into_iter()
            .find(|c| c["done"].is_null() && c["id"] == card["id"])
    });
    let merged = answer_signed(&obid, &card, "Merge");
    assert_eq!(merged.status, 200, "{}", merged.text);
    assert_eq!(job(&obid)["mergedBy"]["by"], "owner");
    assert_eq!(calls(&folder.0).matches("-X PUT").count(), 1);
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

/// TASKS-13: an Oby review's card (here its crash card) waits on the pull request it reviewed:
/// open, the card stays; once that pull request is closed on GitHub, the card comes back done
/// with no answer, and a failed review with nothing left for the owner is let go by obid, saying
/// so, so Home keeps neither. Each half alone: the carded review made a commit that never went
/// out, so obid keeps it (its card settles by the closed pull request), and the other has no card.
#[test]
fn tasks_13_an_oby_reviews_card_settles_once_its_pull_request_closes() {
    let folder = Folder::new("review-card-closed");
    let review = |id: &str, status: obid::job::Status| {
        let mut review = obid::job::Job::new(
            id.into(),
            REPO.into(),
            "review".into(),
            "Fix the thing".into(),
            "claude".into(),
            "stub".into(),
        );
        review.status = status;
        review.review_of = Some(ID.into());
        review
    };
    let mut carded = review("r9e8d7c6", obid::job::Status::Failed);
    carded.reason = Some(obid::job::LOOPED.into());
    carded.commits = Some(1);
    let bare = review("r5f4e3d2", obid::job::Status::Interrupted);
    let jobs = folder.0.join("home/jobs");
    std::fs::create_dir_all(&jobs).unwrap();
    for job in [&carded, &bare] {
        std::fs::write(
            jobs.join(format!("{}.json", job.id)),
            serde_json::to_vec(job).unwrap(),
        )
        .unwrap();
    }
    let obid = started_with(&folder, true, None, published(), &[]);
    let card = |state: &str| {
        call(&obid, "GET", &format!("/v1/inbox?state={state}"), None)
            .body
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .find(|i| i["id"].as_str().unwrap().starts_with("r9e8d7c6-crash-"))
    };
    let job = |id: &str| call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    let open = within(30, "the review's crash card", || card("open"));
    // Its pull request still open: the card waits for the owner, and neither review is let go.
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(card("open").is_some(), "{open}");
    for id in ["r9e8d7c6", "r5f4e3d2"] {
        assert_eq!(job(id)["dismissed"], Value::Null, "{}", job(id));
    }

    view_now(&folder, |view| view["state"] = json!("CLOSED"));
    let done = within(30, "the card settled", || card("done"));
    assert_eq!(done["id"], open["id"]);
    assert_eq!(done["answer"], Value::Null, "{done}");
    assert!(card("open").is_none());
    let bare = within(30, "the bare review let go", || {
        let bare = job("r5f4e3d2");
        bare["dismissed"].is_string().then_some(bare)
    });
    assert!(
        bare["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["what"] == obid::jobs::LET_GO),
        "{bare}"
    );
    // Its commit never went out: the carded review stays the owner's, its card settled all the same.
    let carded = job("r9e8d7c6");
    assert_eq!(carded["dismissed"], Value::Null, "{carded}");
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

/// The job's pull request into a dev whose rules require an approving review, in a repository
/// Oby hasn't learned (where the owner said yes to a teammate's review before, when `reviewed`),
/// with obid in a fake Obysphere where ernesto's Oby takes reviews.
async fn in_a_sphere(folder: &Folder, reviewed: bool) -> (FakeSphere, Obid, TestObi) {
    let sphere = FakeSphere::start().await;
    if reviewed {
        let dir = folder.0.join("home/memory/repos/obi-e2e/fixture-app");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("m5e6f7a8b.md"),
            format!(
                "---\nid: m5e6f7a8b\nscope: project\nrepo: {REPO}\nkind: review\nsource: user\ntitle: Who reviews fixture-app\nfrom: You asked a teammate's Oby to review fixture-app#6\ncreated: 2026-10-25T10:00:00Z\nupdated: 2026-10-25T10:00:00Z\nused: 0\n---\nWhere GitHub requires another person's approval on {REPO}, Oby asks a teammate's Oby to review it. It still asks you before it merges.\n"
            ),
        )
        .unwrap();
    }
    let obid = tokio::task::block_in_place(|| {
        started_with(folder, false, None, published(), &["requires-approval"])
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

/// [`answer`], signed by the owner's app as the Electron app signs it (`X-Oby-Words`).
fn answer_signed(obid: &Obid, card: &Value, answer: &str) -> Answer {
    tokio::task::block_in_place(|| {
        call_typed(
            obid,
            "POST",
            &format!("/v1/inbox/{}/answer", card["id"].as_str().unwrap()),
            &json!({"answer": answer}),
        )
    })
}

/// A repository Oby hasn't learned whose rules require another person's approval asks the owner
/// once, for that head, whether a teammate's Oby reviews it (the owner's decision of 2026-10-25).
/// Their yes keeps only who reviews (a `review` memory, never a workflow) and asks a teammate's
/// Oby right away. Once the teammate approved, how it merges is still the owner's to say: "Ready to
/// merge?" asks, and nothing merges by itself.
#[tokio::test(flavor = "multi_thread")]
async fn a_yes_remembers_a_teammate_for_the_repository_and_asks_one() {
    let folder = Folder::new("merge-teammate-yes");
    let (_sphere, obid, ernesto) = in_a_sphere(&folder, false).await;
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
    let memories = memory["memories"].as_array().unwrap();
    assert_eq!(memories.len(), 1, "{memory}");
    assert_eq!(memories[0]["kind"], "review");
    assert_eq!(memories[0]["repo"], REPO);
    assert_eq!(memories[0]["title"], "Who reviews fixture-app");
    assert_eq!(
        memories[0]["text"],
        "Where GitHub requires another person's approval on obi-e2e/fixture-app, Oby asks a teammate's Oby to review it. It still asks you before it merges."
    );
    assert_eq!(
        memories[0]["from"],
        "You asked a teammate's Oby to review fixture-app#7"
    );
    assert!(memories[0].get("workflow").is_none(), "{memory}");
    // ernesto's Oby has it: nothing waits on the owner, and nothing merged.
    tokio::task::block_in_place(|| {
        within(30, "nothing to wait on the owner", || {
            (merge_cards(&obid).is_empty() && job(&obid)["mergeNeedsYou"] == false).then_some(())
        })
    });
    // The teammate approved on GitHub: "Ready to merge?" asks, and nothing merges by itself.
    std::fs::write(folder.0.join("gh/person"), "").unwrap();
    let ready = tokio::task::block_in_place(|| {
        within(30, "the ready card", || {
            merge_cards(&obid)
                .into_iter()
                .find(|c| c["id"] == format!("merge-ready-{ID}-{}", head()))
        })
    });
    assert_eq!(ready["title"], "Ready to merge: Fix the thing");
    assert!(!calls(&folder.0).contains("-X PUT"));
    assert!(tokio::task::block_in_place(|| job(&obid))["mergedBy"].is_null());
}

/// A repository whose owner said yes to a teammate's review before: every later pull request
/// there that needs another person's approval goes to a teammate's Oby by itself, with no card.
#[tokio::test(flavor = "multi_thread")]
async fn a_remembered_teammate_review_asks_a_teammate_with_no_card() {
    let folder = Folder::new("merge-teammate-remembered");
    let (_sphere, obid, ernesto) = in_a_sphere(&folder, true).await;
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
    let (_sphere, obid, ernesto) = in_a_sphere(&folder, false).await;
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
        memory["memories"].as_array().is_none_or(|m| m.is_empty()),
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

/// `pr view`'s answer with `change` applied to the stand-in's own.
fn view_now(folder: &Folder, change: impl FnOnce(&mut Value)) {
    let dir = folder.0.join("gh");
    let mut view: Value =
        serde_json::from_slice(&std::fs::read(dir.join("view.json")).unwrap()).unwrap();
    change(&mut view);
    std::fs::write(dir.join("view-now.json"), view.to_string()).unwrap();
}

/// [`published`], pushed `seconds` ago.
fn pushed_ago(seconds: i64) -> obid::job::Job {
    let mut job = published();
    job.outgoing[0].date -= seconds;
    job
}

const MAC: &str =
    r#"{"base":"dev","method":"squash","rebaseFirst":false,"review":"oby","checks":"mac"}"#;

/// MERGING-1: a `mac` workflow in a repository with no GitHub checks merges by itself once the
/// job's own check passed in its box on exactly the head.
#[test]
fn merging_1_the_jobs_own_check_merges_a_repository_without_github_checks() {
    let folder = Folder::new("merge-mac-checked");
    let mut checked = pushed_ago(obid::merge::CHECKS_SETTLE);
    checked.check = Some("true".into());
    checked.checked = Some(obid::job::Checked {
        commit: head(),
        apart: None,
    });
    let obid = started_with(&folder, false, Some(MAC), checked, &[]);
    view_now(&folder, |v| v["statusCheckRollup"] = json!([]));
    within(30, "the job to be merged", || {
        (job(&obid)["mergedBy"]["by"] == "memory").then_some(())
    });
    assert!(merge_cards(&obid).is_empty());
}

/// MERGING-2: where the job's own check doesn't count (none passed on that head), the card offers
/// Merge, and the owner's Merge merges it.
#[test]
fn merging_2_the_owners_merge_merges_where_the_check_doesnt_count() {
    let folder = Folder::new("merge-mac-yours");
    let mut unchecked = pushed_ago(obid::merge::CHECKS_SETTLE);
    unchecked.check = Some("true".into());
    let obid = started_with(&folder, false, Some(MAC), unchecked, &[]);
    view_now(&folder, |v| v["statusCheckRollup"] = json!([]));
    let card = within(30, "the card", || merge_cards(&obid).into_iter().next());
    assert_eq!(card["title"], "Ready to merge: Fix the thing");
    assert_eq!(
        card["text"],
        "Its check didn't pass on exactly this commit, so it's yours to merge."
    );
    assert_eq!(card["options"], json!(["Merge", "Leave it open"]));
    let waiting = job(&obid);
    assert_eq!(waiting["mergeNeedsYou"], true);
    assert_eq!(waiting["mergeError"], card["text"]);
    assert!(!calls(&folder.0).contains("-X PUT"));
    let merged = answer(&obid, &card, "Merge");
    assert_eq!(merged.status, 200, "{}", merged.text);
    assert_eq!(job(&obid)["mergedBy"]["by"], "owner");
    assert_eq!(calls(&folder.0).matches("-X PUT").count(), 1);
}

/// MERGING-3: a conflict under a learned workflow ends in a card saying what to do, its words on
/// the Merged stage, and nothing is sent.
#[test]
fn merging_3_a_conflict_asks_the_owner() {
    let folder = Folder::new("merge-conflict");
    let obid = started(&folder, false, Some(OBY));
    view_now(&folder, |v| {
        v["mergeable"] = json!("CONFLICTING");
        v["mergeStateStatus"] = json!("DIRTY");
    });
    let why =
        "Not merged: it conflicts with dev. Ask Oby to rebase it onto dev, or settle it on GitHub.";
    let card = within(30, "the card", || {
        merge_cards(&obid).into_iter().find(|c| c["text"] == why)
    });
    assert_eq!(card["options"], json!(["Dismiss"]));
    let waiting = job(&obid);
    assert_eq!(waiting["mergeNeedsYou"], true);
    let merged = waiting["pipeline"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "merged")
        .cloned()
        .unwrap();
    assert_eq!(merged["detail"], why);
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// MERGING-6: the owner's gh signed out (or its login expired) can't read the pull request at all:
/// a card says so, and nothing merges. A gh that only can't reach GitHub raises none. Signed in
/// again, it carries on by itself.
#[test]
fn merging_6_a_signed_out_gh_asks_the_owner() {
    let folder = Folder::new("merge-signed-out");
    let obid = started_with(&folder, false, Some(OBY), published(), &["offline"]);
    within(30, "a few failed reads", || {
        (calls(&folder.0).matches("pr view 7").count() >= 3).then_some(())
    });
    let offline = job(&obid);
    assert!(offline["pullRequestError"].is_string(), "{offline}");
    assert_eq!(offline["mergeNeedsYou"], false);
    assert!(merge_cards(&obid).is_empty());
    std::fs::remove_file(folder.0.join("gh/offline")).unwrap();
    std::fs::write(folder.0.join("gh/signed-out"), "").unwrap();
    let why = "Not merged: obid's gh isn't signed in to GitHub. Sign in to GitHub again, and Oby carries on.";
    let card = within(30, "the card", || {
        merge_cards(&obid).into_iter().find(|c| c["text"] == why)
    });
    assert_eq!(card["title"], "Not merged: Fix the thing");
    assert_eq!(job(&obid)["mergeError"], why);
    assert!(!calls(&folder.0).contains("-X PUT"));
    std::fs::remove_file(folder.0.join("gh/signed-out")).unwrap();
    within(30, "the job to be merged", || {
        (job(&obid)["mergedBy"]["by"] == "memory").then_some(())
    });
    assert_eq!(job(&obid)["mergeNeedsYou"], false);
    assert!(merge_cards(&obid).iter().all(|c| !c["done"].is_null()));
}

/// MERGING-5: GitHub's checks still running an hour after obid pushed the head ask the owner;
/// once they pass, it merges by itself and the card is settled.
#[test]
fn merging_5_checks_that_run_long_ask_then_merge_by_themselves() {
    let folder = Folder::new("merge-checks-long");
    let obid = started_with(&folder, false, Some(OBY), pushed_ago(3600), &[]);
    view_now(&folder, |v| {
        v["statusCheckRollup"][0]["status"] = json!("IN_PROGRESS");
        v["statusCheckRollup"][0]["conclusion"] = json!("");
    });
    let why = "Not merged: its checks on GitHub are still running an hour after Oby pushed it. Look at them on GitHub.";
    let card = within(30, "the card", || {
        merge_cards(&obid).into_iter().find(|c| c["text"] == why)
    });
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
    std::fs::remove_file(folder.0.join("gh/view-now.json")).unwrap();
    within(30, "the job to be merged", || {
        (job(&obid)["mergedBy"]["by"] == "memory").then_some(())
    });
    assert_eq!(job(&obid)["mergeNeedsYou"], false);
    // Settled: not open any more (a settled card leaves the inbox's list).
    assert!(
        merge_cards(&obid)
            .iter()
            .all(|c| c["id"] != card["id"] || !c["done"].is_null())
    );
}

/// [`published`], but Oby's review of its head asks for changes: on its first round, with nothing
/// more happening by itself (a follow-up carries its branch, say).
fn changes_asked() -> obid::job::Job {
    let mut job = published();
    job.review.as_mut().unwrap().verdict = "changesRequested".into();
    job
}

/// The open items of Oby's review of the job.
fn review_items(obid: &Obid) -> Vec<Value> {
    call(obid, "GET", "/v1/inbox", None)
        .body
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i["id"] == format!("{ID}-review-r1b2c3d4") && i["done"].is_null())
        .collect()
}

/// MERGING-8 (#1658): Oby's review left asking for changes with no round to take them, after one
/// round as after three, gives the owner obid's own item, with One more round and, into dev,
/// Merge: never a question of Oby's that offers only Done.
#[test]
fn merging_8_changes_left_to_the_owner_after_any_round_ask_with_one_more_round() {
    let folder = Folder::new("merge-review-left");
    let obid = started_with(&folder, false, None, changes_asked(), &[]);
    let item = within(30, "the review's item", || {
        review_items(&obid).into_iter().next()
    });
    assert_eq!(item["kind"], "failed");
    assert_eq!(
        item["text"],
        "Oby's review still asks for changes. Take it from here."
    );
    assert_eq!(item["options"], json!(["One more round", "Merge"]));
    assert_eq!(job(&obid)["reviewNeedsYou"], true);
    // No question of the orchestrator's beside it.
    let questions: Vec<Value> = call(&obid, "GET", "/v1/inbox", None)
        .body
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| {
            i["id"]
                .as_str()
                .unwrap_or_default()
                .starts_with("orchestrator-")
        })
        .collect();
    assert!(questions.is_empty(), "{questions:?}");
}

/// MERGING-17 (#1658): Merge on that item is the owner's own Merge of the pull request as it is:
/// it merges with their GitHub login, `mergedBy` `owner`, in a repository Oby hasn't learned and
/// under a learned workflow alike, which never merges it by itself. No merge card of its own
/// stands beside the item.
#[test]
fn merging_17_merge_on_the_review_item_merges_it_as_it_is_on_the_owners_word() {
    for (name, workflow) in [("merge-as-it-is", None), ("merge-as-it-is-oby", Some(OBY))] {
        let folder = Folder::new(name);
        let obid = started_with(&folder, false, workflow, changes_asked(), &[]);
        let item = within(30, "the review's item", || {
            review_items(&obid).into_iter().next()
        });
        within(30, "a few reads", || {
            (calls(&folder.0).matches("pr view 7").count() >= 3).then_some(())
        });
        assert!(!calls(&folder.0).contains("-X PUT"), "{name}");
        assert!(
            merge_cards(&obid).iter().all(|c| !c["done"].is_null()),
            "{name}: {:?}",
            merge_cards(&obid)
        );
        let merged = answer(&obid, &item, "Merge");
        assert_eq!(merged.status, 200, "{name}: {}", merged.text);
        assert_eq!(merged.body["answer"], "Merge");
        assert_eq!(job(&obid)["mergedBy"]["by"], "owner", "{name}");
        assert_eq!(calls(&folder.0).matches("-X PUT").count(), 1, "{name}");
        assert!(review_items(&obid).is_empty(), "{name}");
    }
}

/// MERGING-17: Merge that can't go (a conflict) merges nothing, says why, and leaves the item open.
#[test]
fn merging_17_a_merge_that_cant_go_says_why_and_stays_open() {
    let folder = Folder::new("merge-as-it-is-conflict");
    let obid = started_with(&folder, false, None, changes_asked(), &[]);
    let item = within(30, "the review's item", || {
        review_items(&obid).into_iter().next()
    });
    view_now(&folder, |v| {
        v["mergeable"] = json!("CONFLICTING");
        v["mergeStateStatus"] = json!("DIRTY");
    });
    let refused = answer(&obid, &item, "Merge");
    assert_eq!(refused.status, 409, "{}", refused.text);
    assert!(
        refused.text.contains("conflicts with dev"),
        "{}",
        refused.text
    );
    assert!(!calls(&folder.0).contains("-X PUT"));
    assert_eq!(review_items(&obid).len(), 1);
}

/// MERGING-7 (the owner's decision of 2026-10-31, "Oby forbids nothing"): an approved, green pull
/// request into main never merges by itself; the owner gets `Ready to merge`, and their Merge
/// merges it with their own login.
#[test]
fn merging_7_into_main_only_the_owners_merge_merges() {
    let folder = Folder::new("merge-into-main");
    let mut into_main = published();
    into_main.base = Some("main".into());
    let obid = started_with(&folder, false, None, into_main, &[]);
    view_now(&folder, |v| v["baseRefName"] = json!("main"));
    let pull = folder.0.join("gh/pull.json");
    let mut rest: Value = serde_json::from_slice(&std::fs::read(&pull).unwrap()).unwrap();
    rest["base"]["ref"] = json!("main");
    std::fs::write(&pull, rest.to_string()).unwrap();
    let card = within(30, "the card", || {
        merge_cards(&obid).into_iter().find(|c| {
            c["done"].is_null() && c["text"].as_str().unwrap_or_default().contains("main")
        })
    });
    assert_eq!(card["title"], "Ready to merge: Fix the thing");
    assert_eq!(
        card["text"],
        "Its base is main, which Oby merges into only on your word, so it's yours to merge."
    );
    assert_eq!(card["options"], json!(["Merge", "Leave it open"]));
    within(30, "a few reads", || {
        (calls(&folder.0).matches("pr view 7").count() >= 3).then_some(())
    });
    assert!(!calls(&folder.0).contains("-X PUT"));
    let merged = answer(&obid, &card, "Merge");
    assert_eq!(merged.status, 200, "{}", merged.text);
    assert_eq!(job(&obid)["mergedBy"]["by"], "owner");
    assert_eq!(calls(&folder.0).matches("-X PUT").count(), 1);
}

/// Points the stand-in's pull request at `base`, in both of GitHub's answers.
fn into_base(folder: &Folder, base: &str) {
    view_now(folder, |v| v["baseRefName"] = json!(base));
    let pull = folder.0.join("gh/pull.json");
    let mut rest: Value = serde_json::from_slice(&std::fs::read(&pull).unwrap()).unwrap();
    rest["base"]["ref"] = json!(base);
    std::fs::write(&pull, rest.to_string()).unwrap();
}

/// MERGING-7 with a learned workflow into dev: a pull request into another base isn't that
/// workflow's, so the owner gets the card a repository Oby hasn't learned gets (into main its
/// base's reason, else that it goes elsewhere), with no remembering, and their Merge merges it.
#[test]
fn merging_7_another_base_than_the_learned_one_is_the_owners_merge() {
    for (base, why) in [
        (
            "main",
            "Its base is main, which Oby merges into only on your word, so it's yours to merge.",
        ),
        (
            "release",
            "It goes into release, and fixture-app merges into dev, so it's yours to merge.",
        ),
    ] {
        let folder = Folder::new(&format!("merge-learned-into-{base}"));
        let mut elsewhere = published();
        elsewhere.base = Some(base.into());
        let obid = started_with(&folder, false, Some(OBY), elsewhere, &[]);
        into_base(&folder, base);
        let card = within(30, "the card", || {
            merge_cards(&obid)
                .into_iter()
                .find(|c| c["done"].is_null() && c["text"] == why)
        });
        assert_eq!(card["options"], json!(["Merge", "Leave it open"]), "{base}");
        assert!(!calls(&folder.0).contains("-X PUT"), "{base}");
        let merged = answer(&obid, &card, "Merge");
        assert_eq!(merged.status, 200, "{base}: {}", merged.text);
        assert_eq!(job(&obid)["mergedBy"]["by"], "owner", "{base}");
        assert_eq!(calls(&folder.0).matches("-X PUT").count(), 1, "{base}");
    }
}

/// MERGING-17: Merge is tied to the head the card's review read: a head moved since merges nothing.
#[test]
fn merging_17_a_head_moved_since_merges_nothing() {
    let folder = Folder::new("merge-as-it-is-moved");
    let obid = started_with(&folder, false, None, changes_asked(), &[]);
    let item = within(30, "the review's item", || {
        review_items(&obid).into_iter().next()
    });
    view_now(&folder, |v| v["headRefOid"] = json!("d".repeat(40)));
    let refused = answer(&obid, &item, "Merge");
    assert_eq!(refused.status, 409, "{}", refused.text);
    assert!(refused.text.contains("head moved"), "{}", refused.text);
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// MERGING-17: what GitHub's own rules refuse reaches the owner as GitHub's, saying how to change
/// it: a required approval, and rules that still block the merge.
#[test]
fn merging_17_githubs_rules_say_how_to_change_them() {
    for (name, files, blocked) in [
        ("merge-as-it-is-required", &["requires-approval"][..], false),
        ("merge-as-it-is-blocked", &[][..], true),
    ] {
        let folder = Folder::new(name);
        let obid = started_with(&folder, false, None, changes_asked(), files);
        if blocked {
            view_now(&folder, |v| v["mergeStateStatus"] = json!("BLOCKED"));
        }
        let item = within(30, "the review's item", || {
            review_items(&obid).into_iter().next()
        });
        let refused = answer(&obid, &item, "Merge");
        assert_eq!(refused.status, 409, "{name}: {}", refused.text);
        assert!(
            refused
                .text
                .contains("change dev's branch protection there"),
            "{name}: {}",
            refused.text
        );
        assert!(!calls(&folder.0).contains("-X PUT"), "{name}");
        assert_eq!(review_items(&obid).len(), 1, "{name}");
    }
}

/// MERGING-8: an item kept open from an obid before #1658 (One more round alone) takes Merge too,
/// and any other answer names the item's own.
#[test]
fn merging_8_an_item_from_before_takes_merge() {
    let folder = Folder::new("merge-review-item-before");
    let inbox = folder.0.join("home/inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    let mut before = obid::sphere::inbox::reviews_ran_out_item(&changes_asked(), "r1b2c3d4");
    before.options.truncate(1);
    before.details.as_mut().unwrap().truncate(1);
    std::fs::write(
        inbox.join(format!("{}.json", before.id)),
        serde_json::to_vec(&before).unwrap(),
    )
    .unwrap();
    let obid = started_with(&folder, false, None, changes_asked(), &[]);
    let item = within(30, "Merge on it", || {
        review_items(&obid)
            .into_iter()
            .find(|i| i["options"] == json!(["One more round", "Merge"]))
    });
    let wrong = answer(&obid, &item, "Dismiss it");
    assert_eq!(wrong.status, 422, "{}", wrong.text);
    assert!(
        wrong
            .text
            .contains("The answer is One more round or Merge."),
        "{}",
        wrong.text
    );
}

/// MERGING-23: a follow-up's push to its task's pull request is Oby's own: with a workflow the
/// owner taught Oby, the pull request merges by itself at that head, on the follow-up's record,
/// and the task it followed up never says its head is a commit Oby didn't push.
#[test]
fn merging_23_a_follow_ups_push_merges_as_obys_own() {
    let folder = Folder::new("merge-follow-up");
    let obid = started_with_jobs(&folder, true, Some(OBIS), &task_and_follow_up(), &[]);
    let merged = within(30, "the follow-up to merge it", || {
        let job = job_of(&obid, FOLLOW_UP);
        (job["mergedBy"]["by"] == "memory").then_some(job)
    });
    assert_eq!(merged["addedTo"], 7, "{merged}");
    assert_eq!(calls(&folder.0).matches("-X PUT").count(), 1);
    let task = job(&obid);
    assert_eq!(task["mergeNeedsYou"], false, "{task}");
    assert!(
        task["mergeError"]
            .as_str()
            .is_none_or(|e| !e.contains("didn't push")),
        "{task}"
    );
    assert!(open_merge_cards(&obid).is_empty());
}

/// MERGING-24: one pull request gets one card: the follow-up's, the job that pushed to it last.
/// The task it followed up has none, and nothing about merging waits on it.
#[test]
fn merging_24_one_pull_request_gets_one_card() {
    let folder = Folder::new("merge-one-card");
    let obid = started_with_jobs(&folder, true, None, &task_and_follow_up(), &[]);
    let card = within(30, "the merge card", || {
        open_merge_cards(&obid).into_iter().next()
    });
    assert_eq!(card["id"], format!("merge-ready-{FOLLOW_UP}-{}", head()));
    // A few more reads: still one.
    std::thread::sleep(std::time::Duration::from_secs(3));
    let open = open_merge_cards(&obid);
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(job_of(&obid, FOLLOW_UP)["mergeNeedsYou"], true);
    let task = job(&obid);
    assert_eq!(task["mergeNeedsYou"], false, "{task}");
    assert_eq!(task["mergeError"], Value::Null, "{task}");
    // The owner's Merge merges it on the follow-up's record.
    let merged = answer_signed(&obid, &card, "Merge");
    assert_eq!(merged.status, 200, "{}", merged.text);
    assert_eq!(job_of(&obid, FOLLOW_UP)["mergedBy"]["by"], "owner");
    assert_eq!(calls(&folder.0).matches("-X PUT").count(), 1);
}

/// MERGING-25: a head no job of Oby's pushed, a follow-up's chain included, is still not Oby's:
/// nothing merges by itself, and the one card says so.
#[test]
fn merging_25_a_head_nobody_at_oby_pushed_is_still_refused() {
    let folder = Folder::new("merge-stranger");
    // GitHub's head is `head()`: the follow-up pushed another commit, approved by Oby's review.
    let ours = "c".repeat(40);
    let [task, mut follow_up] = task_and_follow_up();
    follow_up.outgoing[0].id = ours.clone();
    follow_up.review.as_mut().unwrap().commit = ours;
    let obid = started_with_jobs(&folder, true, Some(OBIS), &[task, follow_up], &[]);
    let card = within(30, "the card", || {
        open_merge_cards(&obid).into_iter().find(|c| {
            c["text"]
                .as_str()
                .is_some_and(|t| t.contains("its head is a commit Oby didn't push"))
        })
    });
    assert!(card["id"].as_str().unwrap().contains(FOLLOW_UP), "{card}");
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert_eq!(open_merge_cards(&obid).len(), 1);
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// MERGING-24: the follow-up that pushed the head was let go: the task before it, which Oby still
/// carries, has the pull request's one card, saying why.
#[test]
fn merging_24_a_follow_up_let_go_hands_the_card_back_saying_why() {
    let folder = Folder::new("merge-let-go");
    let [task, mut follow_up] = task_and_follow_up();
    follow_up.dismissed = Some(follow_up.created + 5);
    let obid = started_with_jobs(&folder, true, None, &[task, follow_up], &[]);
    let card = within(30, "the task's card", || {
        open_merge_cards(&obid).into_iter().next()
    });
    assert!(card["id"].as_str().unwrap().contains(ID), "{card}");
    assert_eq!(
        card["text"],
        "Not merged: its head is what Fix the thing follow-up pushed, and you let that task go. Tell Oby to carry on in it, or settle it on GitHub."
    );
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert_eq!(open_merge_cards(&obid).len(), 1);
    assert_eq!(job(&obid)["mergeNeedsYou"], true);
    assert_eq!(job_of(&obid, FOLLOW_UP)["mergeNeedsYou"], false);
    assert!(!calls(&folder.0).contains("-X PUT"));
}

/// MERGING-24, as the drive of 2026-10-31 had it: the task's card is open, then its follow-up
/// pushes to the same pull request. The task's card is settled, and the follow-up's is the one.
#[test]
fn merging_24_a_follow_ups_push_settles_the_tasks_open_card() {
    let folder = Folder::new("merge-drive-order");
    let mut task = published();
    task.outgoing.push(obid::publish::Outgoing::new(
        "pullRequest",
        REPO,
        PULL,
        None,
    ));
    let obid = started_with_jobs(&folder, true, None, &[task], &[]);
    let first = within(30, "the task's card", || {
        open_merge_cards(&obid).into_iter().next()
    });
    assert_eq!(first["id"], format!("merge-ready-{ID}-{}", head()));
    drop(obid);
    // The follow-up pushed `pushed` and added to the pull request: GitHub's head is that now.
    let pushed = "d".repeat(40);
    let [_, mut follow_up] = task_and_follow_up();
    follow_up.outgoing[0].id = pushed.clone();
    follow_up.review.as_mut().unwrap().commit = pushed.clone();
    let config: Value =
        serde_json::from_slice(&std::fs::read(folder.0.join("config.json")).unwrap()).unwrap();
    let home = Path::new(config["home"].as_str().unwrap()).to_path_buf();
    std::fs::write(
        home.join(format!("jobs/{FOLLOW_UP}.json")),
        serde_json::to_vec(&follow_up).unwrap(),
    )
    .unwrap();
    for (file, pointer) in [("view.json", "/headRefOid"), ("pull.json", "/head/sha")] {
        let path = folder.0.join("gh").join(file);
        let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        *value.pointer_mut(pointer).unwrap() = json!(pushed);
        std::fs::write(&path, value.to_string()).unwrap();
    }
    let obid = start_with(
        &folder.0,
        &config,
        Some(&folder.0.join("bin")),
        &[("OBID_FOLLOW_SECONDS", "1")],
    );
    let card = within(30, "the follow-up's card", || {
        open_merge_cards(&obid)
            .into_iter()
            .find(|c| c["id"] == format!("merge-ready-{FOLLOW_UP}-{pushed}"))
    });
    std::thread::sleep(std::time::Duration::from_secs(3));
    let open = open_merge_cards(&obid);
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0]["id"], card["id"]);
    // The task's card is settled, with no answer.
    let done = call(&obid, "GET", "/v1/inbox?state=done", None).body;
    let settled = done
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["id"] == first["id"])
        .cloned()
        .unwrap_or_else(|| panic!("the task's card is kept, settled: {done}"));
    assert!(!settled["done"].is_null(), "{settled}");
    assert!(settled["answer"].is_null(), "{settled}");
    let task = job(&obid);
    assert_eq!(task["mergeNeedsYou"], false, "{task}");
    assert_eq!(task["mergeError"], Value::Null, "{task}");
}

/// MERGING-24: the task's own review asked for changes on its old head, and its follow-up pushed
/// to the pull request since: the follow-up's review is the one, so the task offers no One more
/// round or Merge for a head that isn't the pull request's.
#[test]
fn merging_24_the_tasks_review_of_an_old_head_asks_nothing() {
    let folder = Folder::new("merge-old-review");
    let [mut task, follow_up] = task_and_follow_up();
    task.review.as_mut().unwrap().verdict = "changesRequested".into();
    let obid = started_with_jobs(&folder, true, None, &[task, follow_up], &[]);
    within(30, "the follow-up's card", || {
        open_merge_cards(&obid).into_iter().next()
    });
    std::thread::sleep(std::time::Duration::from_secs(3));
    let items = review_items(&obid);
    assert!(items.is_empty(), "{items:?}");
    assert_eq!(open_merge_cards(&obid).len(), 1);
}

/// The task, an untrusted follow-up of it, and a trusted one made beside it (before follow-ups
/// made one line) that pushed last, `head()`.
fn siblings() -> [obid::job::Job; 3] {
    let [task, mut later] = task_and_follow_up();
    let mut untrusted = later.clone();
    untrusted.id = "e1b2c3d4".into();
    untrusted.title = Some("Untrusted follow-up".into());
    untrusted.untrusted = true;
    untrusted.created -= 1;
    untrusted.outgoing[0].id = "e".repeat(40);
    untrusted.outgoing[0].date -= 30;
    untrusted.review.as_mut().unwrap().commit = "e".repeat(40);
    later.created += 1;
    [task, untrusted, later]
}

const UNFOLLOWED: &str = "Not merged: its head holds what Untrusted follow-up pushed, and this task doesn't follow that one up, so its review alone doesn't count for that work.";

/// MERGING-14, 20, 23: a follow-up made beside an untrusted one pushed last: it carries the pull
/// request's one card, and its single review merges nothing by itself, nor posts an approval. The
/// card names the hold with Merge: an answer no app of the owner's signed merges nothing, their
/// signed one merges it.
#[test]
fn merging_23_a_sibling_follow_up_never_merges_an_untrusted_ones_work_on_one_review() {
    let folder = Folder::new("merge-siblings");
    let obid = started_with_jobs(&folder, true, Some(OBIS), &siblings(), &[]);
    let card = within(30, "the card", || {
        open_merge_cards(&obid).into_iter().next()
    });
    assert!(card["id"].as_str().unwrap().contains(FOLLOW_UP), "{card}");
    assert!(
        card["text"].as_str().unwrap().contains(UNFOLLOWED),
        "{card}"
    );
    assert_eq!(card["options"][0], "Merge", "{card}");
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert_eq!(open_merge_cards(&obid).len(), 1);
    let calls_now = calls(&folder.0);
    assert!(!calls_now.contains("-X PUT"), "{calls_now}");
    assert!(!calls_now.contains("-X POST"), "{calls_now}");
    let unsigned = answer(&obid, &card, "Merge");
    assert_eq!(unsigned.status, 409, "{}", unsigned.text);
    assert!(
        unsigned.text.contains("your Merge didn't cover this")
            && unsigned.text.contains(UNFOLLOWED),
        "{}",
        unsigned.text
    );
    assert!(!calls(&folder.0).contains("-X PUT"));
    let card = within(30, "the card again", || {
        open_merge_cards(&obid)
            .into_iter()
            .find(|c| c["id"] == card["id"])
    });
    let merged = answer_signed(&obid, &card, "Merge");
    assert_eq!(merged.status, 200, "{}", merged.text);
    assert_eq!(job_of(&obid, FOLLOW_UP)["mergedBy"]["by"], "owner");
    assert_eq!(calls(&folder.0).matches("-X PUT").count(), 1);
}

/// MERGING-17, 23: in the same order, with the trusted follow-up's review asking for changes, the
/// review item's Merge no app of the owner's signed merges nothing: it didn't name the hold.
#[test]
fn merging_23_a_review_items_unsigned_merge_never_lifts_an_unfollowed_push() {
    let folder = Folder::new("merge-siblings-review");
    let [task, untrusted, mut later] = siblings();
    later.review.as_mut().unwrap().verdict = "changesRequested".into();
    let obid = started_with_jobs(&folder, true, Some(OBIS), &[task, untrusted, later], &[]);
    let item = within(30, "the follow-up's review item", || {
        call(&obid, "GET", "/v1/inbox", None)
            .body
            .as_array()
            .into_iter()
            .flatten()
            .find(|i| i["id"] == format!("{FOLLOW_UP}-review-r1b2c3d4") && i["done"].is_null())
            .cloned()
    });
    assert_eq!(
        item["options"],
        json!(["One more round", "Merge"]),
        "{item}"
    );
    let refused = answer(&obid, &item, "Merge");
    assert_eq!(refused.status, 409, "{}", refused.text);
    assert!(refused.text.contains(UNFOLLOWED), "{}", refused.text);
    assert!(!calls(&folder.0).contains("-X PUT"));
}
