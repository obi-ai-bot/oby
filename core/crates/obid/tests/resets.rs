//! A job a usage limit stopped is tried again by itself once its reset comes (#740), from its job
//! file, so a restart keeps waiting; one the owner dismissed or took on in a follow-up isn't. A job
//! a restart cut short picks up at the phase it was in. The real obid binary with the stand-in
//! agent (tests/agents/fake-claude).
mod support;

use std::path::Path;

use obid::job::{Job, Status};
use serde_json::{Value, json};
use support::*;

/// An interrupted job whose usage limit stopped it, due to be tried again a second ago.
fn limited(id: &str) -> Job {
    let mut job = Job::new(
        id.into(),
        "obi-e2e/fixture-app".into(),
        "research".into(),
        "Look around".into(),
        "claude".into(),
        "stub".into(),
    );
    let now = obi_protocol::time::now();
    job.branch = format!("oby/research-{id}");
    job.status = Status::Interrupted;
    job.session = Some(format!("{id}-0000-4000-8000-000000000000"));
    job.round = Some(1);
    job.started = Some(now - 120);
    job.finished = Some(now - 60);
    job.reason =
        Some("Claude AI usage limit reached. Oby tries again by itself after the reset.".into());
    job.next_step = Some(obid::job::TRY_AGAIN.into());
    job.resumes_at = Some(now - 1);
    job
}

fn job(obid: &Obid, id: &str) -> Value {
    call(obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

/// LIMITS-13: waits an older release left (`interrupted` with a `resumesAt`) settle when obid
/// starts: the due ones wait for their account's reset or room, in their round, with no card; the
/// ones the owner dismissed or a follow-up took over are let go.
#[test]
fn limits_13_older_retries_wait_after_restart() {
    let folder = Folder::new("resets");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let config = config(&folder.0, &remote, &agent);
    let due = limited("aaaa0001");
    let second = limited("aaaa0005");
    let mut dismissed = limited("aaaa0002");
    dismissed.dismissed = Some(obi_protocol::time::now() - 30);
    let followed = limited("aaaa0003");
    let mut follow_up = limited("aaaa0004");
    follow_up.status = Status::Done;
    follow_up.resumes_at = None;
    follow_up.follow_up_of = Some(followed.id.clone());
    put(
        &folder.0,
        &[&due, &second, &dismissed, &followed, &follow_up],
    );
    let obid = start(&folder.0, &config, None);
    for j in [&due, &second] {
        let current = job(&obid, &j.id);
        assert_eq!(current["status"], "queued", "{current}");
        assert_eq!(current["round"], 1, "{current}");
        assert!(!current["resumesAt"].is_null(), "{current}");
        assert!(current["accountLimit"].is_null(), "{current}");
    }
    for j in [&dismissed, &followed] {
        let current = job(&obid, &j.id);
        assert_eq!(current["status"], "interrupted", "{current}");
        assert!(current["resumesAt"].is_null(), "{current}");
    }
    let decisions = call(&obid, "GET", "/v1/limit-decisions", None).body;
    assert_eq!(decisions, json!([]));
}

/// A job an older release stopped at `stub`'s limit, waiting on the owner's #1563 decision.
fn blocked(id: &str) -> Job {
    let mut job = limited(id);
    job.resumes_at = None;
    job.worktree = Some(format!("/nonexistent/worktrees/{id}"));
    job.account_limit_blocked = true;
    job.account_limit_nonce = Some(format!("nonce-{id}"));
    job.account_limit_reset = Some(obi_protocol::time::now() + 3600);
    job
}

/// LIMITS-26: an older release's decisions settle when obid starts. A pending card, an approved
/// Resume or Move and an older wait wait on the hold table, in their round, branch and worktree,
/// with no card, and a later tick moves one where there's room; a kept Stop stays stopped, its
/// choice copied from the file; a dismissed job stays as it was; and the file goes.
#[test]
fn limits_26_an_older_pending_decision_resolves_into_holds() {
    for codex in [false, true] {
        let folder = Folder::new(&format!("limits-26-{codex}"));
        let config = limit_config(&folder.0, "fake-claude", codex);
        let (a, b, c, d) = (
            blocked("dddd0001"),
            blocked("dddd0002"),
            blocked("dddd0003"),
            blocked("dddd0004"),
        );
        let mut e = blocked("dddd0005");
        e.dismissed = Some(obi_protocol::time::now() - 30);
        let mut f = limited("dddd0006");
        f.resumes_at = Some(obi_protocol::time::now() + 600);
        put(&folder.0, &[&a, &b, &c, &d, &e, &f]);
        older_limit_decisions(
            &folder.0,
            &[
                ("d-pending", &["dddd0001"], "pending"),
                ("d-resume", &["dddd0002"], "resume"),
                ("d-move", &["dddd0003"], "move"),
                ("d-stop", &["dddd0004"], "stop"),
                ("d-dismissed", &["dddd0005"], "pending"),
            ],
        );
        let file = folder.0.join("home/limit-decisions.json");
        let mut obid = start(&folder.0, &config, None);
        let settled = |obid: &Obid| {
            let stopped = job(obid, &d.id);
            assert_eq!(stopped["status"], "interrupted", "{stopped}");
            assert_eq!(stopped["accountLimit"]["state"], "stop", "{stopped}");
            let let_be = job(obid, &e.id);
            assert_eq!(let_be["status"], "interrupted", "{let_be}");
            assert!(!let_be["dismissed"].is_null(), "{let_be}");
            assert!(let_be["accountLimit"].is_null(), "{let_be}");
            let decisions = call(obid, "GET", "/v1/limit-decisions", None).body;
            assert_eq!(decisions, json!([]));
            assert!(!file.exists());
        };
        settled(&obid);
        if codex {
            within(45, "the job moves to the Codex account with room", || {
                (job(&obid, &a.id)["agent"] == "codex").then_some(())
            });
            continue;
        }
        let waits = |obid: &Obid| {
            for j in [&a, &b, &c, &f] {
                let current = job(obid, &j.id);
                assert_eq!(current["status"], "queued", "{current}");
                assert!(!current["resumesAt"].is_null(), "{current}");
                assert!(current["accountLimit"].is_null(), "{current}");
                assert_eq!(current["round"], 1, "{current}");
                assert_eq!(current["branch"], j.branch.as_str(), "{current}");
                assert_eq!(current["worktree"], json!(j.worktree), "{current}");
            }
        };
        waits(&obid);
        obid.stop();
        let obid = start(&folder.0, &config, None);
        settled(&obid);
        waits(&obid);
    }
}

/// LIMITS-26: a decision file obid can't read keeps obid starting and drops no job: each goes by
/// its own kept choice, and the file stays.
#[test]
fn limits_26_an_unreadable_decision_file_keeps_obid_starting() {
    let folder = Folder::new("limits-26-unreadable");
    let config = limit_config(&folder.0, "fake-claude", false);
    let mut kept = blocked("dddd0101");
    kept.account_limit_episode = Some("d-stop".into());
    kept.account_limit_choice = Some(serde_json::from_value(json!("stop")).unwrap());
    let other = blocked("dddd0102");
    put(&folder.0, &[&kept, &other]);
    let file = folder.0.join("home/limit-decisions.json");
    std::fs::write(&file, "not json").unwrap();
    let obid = start(&folder.0, &config, None);
    let jobs = call(&obid, "GET", "/v1/jobs", None);
    assert_eq!(jobs.status, 200, "{}", jobs.text);
    let stopped = job(&obid, &kept.id);
    assert_eq!(stopped["status"], "interrupted", "{stopped}");
    assert_eq!(stopped["accountLimit"]["state"], "stop", "{stopped}");
    let waits = job(&obid, &other.id);
    assert_eq!(waits["status"], "queued", "{waits}");
    assert!(!waits["resumesAt"].is_null(), "{waits}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "not json");
}

/// A job a restart cut short while it ran its check picks up at that phase ("After any restart"):
/// its check runs again on what it committed, never its agent's turn (here the agent would fail:
/// `FAIL`), and a pass goes on to the end of its run, as after its last round. Its round and
/// session stay. One cut short while its agent worked picks its session up as before.
#[test]
fn a_job_cut_short_while_checking_picks_up_there() {
    let folder = Folder::new("cut-short");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let mut config = config(&folder.0, &remote, &agent);
    // A quiet window of 3 seconds: the failing turn's one try waits that long (RECOVERY-1).
    config["stallAfter"] = json!(3);
    let checking = cut("bbbb0001", "check", Some("true"));
    let working = cut("bbbb0003", "work", Some("true"));
    put(&folder.0, &[&checking, &working]);
    let obid = start(&folder.0, &config, None);

    let checked = ended(&obid, &checking.id);
    assert_eq!(checked["status"], "done", "{checked}");
    assert_eq!(checked["round"], 1, "{checked}");
    assert_eq!(checked["summary"], "Looked around.", "{checked}");
    assert_eq!(checked["summaries"], json!(["Looked around."]), "{checked}");
    assert_eq!(
        checked["checks"].as_array().map_or(0, Vec::len),
        1,
        "{checked}"
    );
    // Cut short while its agent worked, it picks its session up again: here the agent fails.
    assert_eq!(ended(&obid, &working.id)["status"], "interrupted");
}

/// A build job a restart cut short while it published, with no check, goes straight on to publish
/// its commits, no agent turn: running at once, so another restart would cut it there again.
#[test]
fn a_job_cut_short_while_publishing_publishes() {
    let folder = Folder::new("cut-publishing");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let config = config(&folder.0, &remote, &agent);
    // A first run commits a.txt and stops at its check, nothing pushed.
    let mut obid = start(&folder.0, &config, None);
    let posted = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "build",
                             "task": "COMMIT: a.txt", "check": "false", "rounds": 1}}),
        ),
    );
    assert_eq!(posted.status, 201, "{}", posted.text);
    let id = posted.body["id"].as_str().unwrap().to_owned();
    let first = ended(&obid, &id);
    assert_eq!(first["status"], "interrupted", "{first}");
    assert_eq!(first["commits"], 1, "{first}");
    obid.stop();
    // As a restart leaves it mid-publish: running, at its publish, no check left; its agent, run
    // again, would fail. A job ahead of it holds the one slot, so its pick-up can be followed.
    let file = folder.0.join(format!("home/jobs/{id}.json"));
    let mut kept: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    kept["status"] = json!("running");
    kept["phase"] = json!("publish");
    kept["check"] = Value::Null;
    kept["task"] = json!("FAIL");
    kept["reason"] = Value::Null;
    kept["nextStep"] = Value::Null;
    std::fs::write(&file, serde_json::to_vec(&kept).unwrap()).unwrap();
    let mut sleeper = cut("bbbb0004", "work", None);
    sleeper.task = "SLEEP".into();
    sleeper.created -= 3600;
    put(&folder.0, &[&sleeper]);
    std::fs::write(
        folder.0.join("home/settings.json"),
        json!({"maxRunning": 1}).to_string(),
    )
    .unwrap();
    let obid = start(&folder.0, &config, None);
    within(30, "the job ahead runs", || {
        (job(&obid, &sleeper.id)["status"] == "running").then_some(())
    });
    let events = follow_events(&obid);
    assert_eq!(
        call(
            &obid,
            "POST",
            &format!("/v1/jobs/{}/stop", sleeper.id),
            None
        )
        .status,
        200
    );
    let published = ended(&obid, &id);
    assert_eq!(published["status"], "done", "{published}");
    assert_eq!(published["round"], 1, "{published}");
    assert_eq!(published["summary"], first["summary"], "{published}");
    assert!(
        published["outgoing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["action"] == "push"),
        "{published}"
    );
    let statuses: Vec<String> = events
        .try_iter()
        .filter(|(name, job)| name == "job" && job["id"] == id.as_str())
        .filter_map(|(_, job)| job["status"].as_str().map(str::to_owned))
        .collect();
    assert!(statuses.iter().any(|s| s == "running"), "{statuses:?}");
}

/// A job as a restart left it in `phase`, its agent's task `FAIL` (a turn of it fails).
fn cut(id: &str, phase: &str, check: Option<&str>) -> Job {
    let mut job = Job::new(
        id.into(),
        "obi-e2e/fixture-app".into(),
        "investigate".into(), // research runs no check since PUBLISHING-28 (#2058)
        "FAIL".into(),
        "claude".into(),
        "stub".into(),
    );
    let now = obi_protocol::time::now();
    job.branch = format!("oby/investigate-{id}");
    job.status = Status::Running;
    job.phase = serde_json::from_value(json!(phase)).unwrap();
    job.check = check.map(str::to_owned);
    job.session = Some(format!("{id}-0000-4000-8000-000000000000"));
    job.round = Some(1);
    job.started = Some(now - 120);
    job.summary = "Looked around.".into();
    job.summaries = Some(vec!["Looked around.".into()]);
    job
}

/// The jobs, kept in their files before obid starts.
fn put(folder: &Path, jobs: &[&Job]) {
    let dir = folder.join("home/jobs");
    std::fs::create_dir_all(&dir).unwrap();
    for job in jobs {
        let path = dir.join(format!("{}.json", job.id));
        std::fs::write(path, serde_json::to_vec(job).unwrap()).unwrap();
    }
}

/// The job once it has ended.
fn ended(obid: &Obid, id: &str) -> Value {
    within(60, &format!("{id} ended"), || {
        let job = job(obid, id);
        (job["status"] != "queued" && job["status"] != "running").then_some(job)
    })
}

/// LIMITS-23: a job file as dev 952bc03d wrote it, waiting on `second` with `scopedHolds` keeping
/// it off `stub`, loads with the same hold in the hold table: it waits until the same time, isn't
/// moved back onto `stub` (as the same job without the hold is), and its file loses the key.
#[test]
fn limits_23_an_older_job_files_holds_are_kept() {
    let folder = Folder::new("limits-23");
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let mut config = config(&folder.0, &remote, &agent);
    let home = folder.0.join("home");
    let now = obi_protocol::time::now();
    let login = json!({"claudeAiOauth": {"accessToken": "fake-claude", "expiresAt": (now + 86_400) * 1000}});
    for account in ["stub", "second"] {
        let dir = home.join("accounts").join(account);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".credentials.json"), login.to_string()).unwrap();
    }
    (config["agents"]["claude"]["accounts"]
        .as_array_mut()
        .unwrap())
    .push(json!({"id": "second", "name": "Second", "configDir": home.join("accounts/second")}));
    std::fs::write(
        home.join("usage.json"),
        json!({"second": {"fullUntil": now + 7200}}).to_string(),
    )
    .unwrap();
    let (held_until, resumes) = (now + 3600, now + 7200 + 90);
    let waiting = |id: &str| {
        let mut job = Job::new(
            id.into(),
            "obi-e2e/fixture-app".into(),
            "investigate".into(),
            "Look around".into(),
            "claude".into(),
            "second".into(),
        );
        let line = "Every account is at its limit: resuming when one has room".to_owned();
        job.branch = format!("oby/investigate-{id}");
        job.status = Status::Queued;
        job.round = Some(1);
        job.resumes_at = Some(resumes);
        job.step = line.clone();
        job.waiting = Some(obid::job::Waiting {
            kind: "cap".into(),
            text: line,
            reserved: None,
            tool: None,
        });
        job
    };
    let (older, plain) = (waiting("cccc0023"), waiting("cccc0024"));
    put(&folder.0, &[&plain]);
    let mut file = serde_json::to_value(&older).unwrap();
    file["recovery"]["scopedHolds"] = json!([["stub", held_until]]);
    let path = home.join("jobs/cccc0023.json");
    std::fs::write(&path, file.to_string()).unwrap();
    let obid = start(&folder.0, &config, None);
    // The same job without the hold moves onto stub, which has room, and runs there.
    let moved = ended(&obid, &plain.id);
    assert_eq!(
        (&moved["account"], &moved["status"]),
        (&json!("stub"), &json!("done"))
    );
    let kept = job(&obid, &older.id);
    assert_eq!(kept["status"], "queued", "{kept}");
    assert_eq!(kept["account"], "second", "{kept}");
    assert_eq!(
        kept["resumesAt"],
        obi_protocol::time::format(resumes),
        "{kept}"
    );
    let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(file["recovery"].get("scopedHolds").is_none(), "{file}");
    let usage: Value =
        serde_json::from_slice(&std::fs::read(home.join("usage.json")).unwrap()).unwrap();
    assert_eq!(
        usage["stub"]["holds"]["job:cccc0023"], held_until,
        "{usage}"
    );
}
