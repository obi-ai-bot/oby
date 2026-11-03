//! Account-limit decisions exercise the real daemon and stand-in agents (#1563).
mod support;

use serde_json::{Value, json};
use std::path::Path;
use support::*;

#[test]
fn owner_moves_a_limit_stopped_job_to_a_connected_account_of_another_tool() {
    let folder = Folder::new("limit-move-tool");
    let remote = fixture(&folder.0.join("fixture"));
    let agents = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents");
    let mut config = config(&folder.0, &remote, &agents.join("fake-claude"));
    let home = folder.0.join("home");
    let claude = home.join("accounts/stub");
    let codex = home.join("accounts/codex");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(claude.join(".credentials.json"), json!({"claudeAiOauth":{"accessToken":"fake-claude","expiresAt":(obi_protocol::time::now()+86400)*1000}}).to_string()).unwrap();
    std::fs::write(
        codex.join("auth.json"),
        json!({"OPENAI_API_KEY":"fake-codex"}).to_string(),
    )
    .unwrap();
    config["agents"]["codex"] = json!({"bin":agents.join("fake-codex"),"accounts":[{"id":"codex","name":"Codex account","configDir":codex}]});
    let now = obi_protocol::time::now();
    std::fs::write(home.join("usage.json"), json!({"codex":{"fiveHour":0.1,"sevenDay":0.2,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now,"sevenDayObservedAt":now}}).to_string()).unwrap();
    let obid = start(&folder.0, &config, None);
    let posted = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job":{"repo":"obi-e2e/fixture-app","kind":"research","task":"LIMITED WEEK: 86400","agent":"claude","account":"stub"}}),
        ),
    );
    assert_eq!(posted.status, 201, "{}", posted.text);
    let id = posted.body["id"].as_str().unwrap().to_owned();
    let stopped = within(60, "Claude hits limit", || {
        let j = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (j["status"] == "interrupted").then_some(j)
    });
    assert!(stopped["resumesAt"].is_null());
    assert!(stopped["moves"].as_array().unwrap().is_empty());
    let branch = stopped["branch"].clone();
    let decision = within(20, "the grouped decision offers Codex", || {
        let v = call(&obid, "GET", "/v1/limit-decisions", None).body;
        v.as_array()?
            .iter()
            .find(|d| {
                d["jobs"]
                    .as_array()
                    .is_some_and(|jobs| jobs.iter().any(|j| j["id"] == id))
                    && d["options"]
                        .as_array()
                        .is_some_and(|options| options.iter().any(|o| o["account"] == "codex"))
            })
            .cloned()
    });
    let decision_id = decision["id"].as_str().unwrap();
    let revision = decision["revision"].as_u64().unwrap();
    let signed_at = obi_protocol::time::format(obi_protocol::time::now());
    let mut approval = owner_signs(
        &obid,
        &format!("oby-limit-decision\n{decision_id}\n{revision}\nmove\ncodex\n{signed_at}"),
        &signed_at,
    );
    approval["action"] = json!("move");
    // AGENTS-6: the job takes the account it moves to's model and effort (docs/contracts/agents.md).
    let tuned = call(
        &obid,
        "PATCH",
        "/v1/accounts/codex",
        Some(&json!({"model": "gpt-6-astra", "effort": "low"})),
    );
    assert_eq!(tuned.status, 200, "{}", tuned.text);
    let answer = call(
        &obid,
        "POST",
        &format!("/v1/limit-decisions/{decision_id}/answer"),
        Some(&json!({"revision":revision,"choice":"move","account":"codex","approval":approval})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let moved = within(60, "Codex continues the same job", || {
        let j = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (j["agent"] == "codex" && j["status"] == "done").then_some(j)
    });
    assert_eq!(moved["branch"], branch);
    assert_eq!(moved["account"], "codex");
    assert_eq!(moved["model"], "gpt-6-astra");
    assert_eq!(moved["effort"], "low");
    assert_eq!(moved["moves"].as_array().unwrap().len(), 1);
    let threads = home.join("logins").join(&id).join("threads");
    let text = std::fs::read_dir(&threads)
        .unwrap()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("hit its account's usage limit") && text.contains("same worktree and branch"),
        "{text}"
    );
    assert!(
        text.contains(r#""model":"gpt-6-astra""#) && text.contains(r#""effort":"low""#),
        "{text}"
    );
    assert!(matches!(moved["resumesAt"], Value::Null));
}

#[test]
fn owner_opt_in_survives_restart_and_resumes_once_when_fresh_usage_shows_room() {
    after_restart("limit-resume-restart", true, Some("resume"), false);
}

#[test]
fn unknown_reset_resumes_after_a_fresh_zero_usage_report_without_reset_times() {
    after_restart("limit-resume-unknown", false, Some("resume"), false);
}

#[test]
fn keeping_the_group_stopped_survives_restart_without_another_prompt() {
    after_restart("limit-keep-stopped", true, Some("stop"), false);
}

#[test]
fn a_pending_decision_never_resumes_without_an_owner_answer() {
    after_restart("limit-unanswered", true, None, false);
}

#[test]
fn a_disconnected_account_returns_an_approved_resume_to_a_visible_decision() {
    after_restart("limit-disconnected", true, Some("resume"), true);
}

fn after_restart(name: &str, known_reset: bool, choice: Option<&str>, disconnect: bool) {
    use obid::job::{Job, Status};
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let config = config(&folder.0, &remote, &agent);
    let home = folder.0.join("home");
    let source = home.join("accounts/stub");
    let jobs = home.join("jobs");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&jobs).unwrap();
    std::fs::write(source.join(".credentials.json"), json!({"claudeAiOauth":{"accessToken":if known_reset { "fake-claude" } else { "zero-no-reset" },"expiresAt":(obi_protocol::time::now()+86400)*1000}}).to_string()).unwrap();
    let now = obi_protocol::time::now();
    let mut job = Job::new(
        "aaaa0123".into(),
        "obi-e2e/fixture-app".into(),
        "research".into(),
        "Look around".into(),
        "claude".into(),
        "stub".into(),
    );
    job.status = Status::Interrupted;
    job.account_limit_blocked = true;
    job.account_limit_nonce = Some("reset-one".into());
    job.account_limit_reset = known_reset.then_some(now - 120);
    job.branch = "oby/research-aaaa0123".into();
    job.round = Some(1);
    std::fs::write(
        jobs.join("aaaa0123.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    let usage = if known_reset {
        json!({"stub":{"fiveHour":0.1,"sevenDay":0.2,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now,"sevenDayObservedAt":now}})
    } else {
        json!({"stub":{"fiveHour":1.0,"sevenDay":0.2,"fullUntil":now+3600,"fullUntilInferred":true,"fiveHourObservedAt":now-120,"sevenDayObservedAt":now-120}})
    };
    std::fs::write(home.join("usage.json"), usage.to_string()).unwrap();
    let mut obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    let decisions = call(&obid, "GET", "/v1/limit-decisions", None).body;
    let decision = &decisions.as_array().unwrap()[0];
    let id = decision["id"].as_str().unwrap();
    let revision = decision["revision"].as_u64().unwrap();
    if let Some(choice) = choice {
        let signed_at = obi_protocol::time::format(obi_protocol::time::now());
        let mut approval = owner_signs(
            &obid,
            &format!("oby-limit-decision\n{id}\n{revision}\n{choice}\n\n{signed_at}"),
            &signed_at,
        );
        approval["action"] = json!(choice);
        let answer = call(
            &obid,
            "POST",
            &format!("/v1/limit-decisions/{id}/answer"),
            Some(&json!({"revision":revision,"choice":choice,"approval":approval})),
        );
        assert_eq!(answer.status, 200, "{}", answer.text);
    }
    obid.stop();
    if disconnect {
        std::fs::remove_file(source.join(".credentials.json")).unwrap();
    }
    let obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    if disconnect {
        let pending = within(
            30,
            "disconnected account offers a visible new decision",
            || {
                let pending = call(&obid, "GET", "/v1/limit-decisions", None).body;
                (pending.as_array()?.len() == 1).then_some(pending)
            },
        );
        assert_eq!(pending[0]["jobs"][0]["id"], "aaaa0123");
        assert!(
            pending[0]["options"]
                .as_array()
                .unwrap()
                .iter()
                .all(|option| option["kind"] != "resume")
        );
        assert!(
            pending[0]["problem"]
                .as_str()
                .unwrap()
                .contains("isn't connected")
        );
        let stopped = call(&obid, "GET", "/v1/jobs/aaaa0123", None).body;
        assert_eq!(stopped["status"], "interrupted");
        assert_eq!(stopped["round"], 1);
        assert!(
            stopped["reason"]
                .as_str()
                .unwrap()
                .contains("no longer connected"),
            "{stopped}"
        );
        return;
    }
    if choice != Some("resume") {
        // Cross a scheduler tick with fresh room: neither a pending nor a declined group starts.
        std::thread::sleep(std::time::Duration::from_secs(17));
        let stopped = call(&obid, "GET", "/v1/jobs/aaaa0123", None).body;
        assert_eq!(stopped["status"], "interrupted", "{stopped}");
        assert_eq!(stopped["round"], 1, "{stopped}");
        assert_eq!(
            stopped["accountLimit"]["state"],
            choice.unwrap_or("pending"),
            "{stopped}"
        );
        let pending = call(&obid, "GET", "/v1/limit-decisions", None).body;
        assert_eq!(
            pending.as_array().unwrap().len(),
            usize::from(choice.is_none())
        );
        return;
    }
    let resumed = within(45, "approved reset resumes after restart", || {
        let j = call(&obid, "GET", "/v1/jobs/aaaa0123", None).body;
        (j["status"] == "done").then_some(j)
    });
    assert_eq!(resumed["round"], 2, "{resumed}");
    assert!(resumed["moves"].as_array().unwrap().is_empty());
}

/// A job its account's limit stopped, as obid keeps it, on the Claude account `stub`.
fn blocked_job(jobs: &Path, id: &str, kind: &str, reset: Option<i64>) {
    use obid::job::{Job, Status};
    let mut job = Job::new(
        id.into(),
        "obi-e2e/fixture-app".into(),
        kind.into(),
        "Look around".into(),
        "claude".into(),
        "stub".into(),
    );
    job.status = Status::Interrupted;
    job.account_limit_blocked = true;
    job.account_limit_nonce = Some(format!("nonce-{id}"));
    job.account_limit_reset = reset;
    job.branch = format!("oby/{kind}-{id}");
    job.round = Some(1);
    std::fs::create_dir_all(jobs).unwrap();
    std::fs::write(
        jobs.join(format!("{id}.json")),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
}

/// The Claude account `stub`, signed in with `token`, and a Codex account `codex` with room (by
/// `home/usage.json`), when `codex`.
fn limit_config(folder: &Path, token: &str, codex: bool) -> Value {
    let remote = fixture(&folder.join("fixture"));
    let agents = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents");
    let mut config = config(folder, &remote, &agents.join("fake-claude"));
    let home = folder.join("home");
    let claude = home.join("accounts/stub");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(claude.join(".credentials.json"), json!({"claudeAiOauth":{"accessToken":token,"expiresAt":(obi_protocol::time::now()+86400)*1000}}).to_string()).unwrap();
    let now = obi_protocol::time::now();
    if codex {
        let dir = home.join("accounts/codex");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("auth.json"),
            json!({"OPENAI_API_KEY":"fake-codex"}).to_string(),
        )
        .unwrap();
        config["agents"]["codex"] = json!({"bin":agents.join("fake-codex"),"accounts":[{"id":"codex","name":"Codex account","configDir":dir}]});
        std::fs::write(home.join("usage.json"), json!({"codex":{"fiveHour":0.1,"sevenDay":0.2,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now,"sevenDayObservedAt":now}}).to_string()).unwrap();
    }
    config
}

/// The owner's signed answer to `decision` at `revision`.
fn answer(
    obid: &Obid,
    decision: &str,
    revision: u64,
    choice: &str,
    account: Option<&str>,
) -> Answer {
    let signed_at = obi_protocol::time::format(obi_protocol::time::now());
    let mut approval = owner_signs(
        obid,
        &format!(
            "oby-limit-decision\n{decision}\n{revision}\n{choice}\n{}\n{signed_at}",
            account.unwrap_or_default()
        ),
        &signed_at,
    );
    approval["action"] = json!(choice);
    let mut body = json!({"revision": revision, "choice": choice, "approval": approval});
    if let Some(account) = account {
        body["account"] = json!(account);
    }
    call(
        obid,
        "POST",
        &format!("/v1/limit-decisions/{decision}/answer"),
        Some(&body),
    )
}

/// LIMITS-1 (#1645): an approved Resume on an account that has reported no usage since its reset (Cursor, an
/// API key, a read that says nothing) runs again 30 minutes after the reset, instead of waiting
/// for a reading that never comes.
#[test]
fn limits_1_a_resume_on_an_account_that_reports_no_usage_runs_again_on_time() {
    let folder = Folder::new("limit-resume-unread");
    let config = limit_config(&folder.0, "silent", false);
    let now = obi_protocol::time::now();
    blocked_job(
        &folder.0.join("home/jobs"),
        "cccc0001",
        "research",
        Some(now - 31 * 60),
    );
    let obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    let decisions = call(&obid, "GET", "/v1/limit-decisions", None).body;
    let decision = &decisions[0];
    let id = decision["id"].as_str().unwrap();
    let answered = answer(
        &obid,
        id,
        decision["revision"].as_u64().unwrap(),
        "resume",
        None,
    );
    assert_eq!(answered.status, 200, "{}", answered.text);
    let resumed = within(45, "the Resume runs on time", || {
        let j = call(&obid, "GET", "/v1/jobs/cccc0001", None).body;
        (j["status"] == "done").then_some(j)
    });
    assert_eq!(resumed["round"], 2, "{resumed}");
}

/// LIMITS-1: before those 30 minutes, or while the week obid last read is at its reserve, it
/// waits.
#[test]
fn limits_1_a_resume_with_no_usage_waits_its_30_minutes_and_a_known_hold() {
    let folder = Folder::new("limit-resume-unread-wait");
    let config = limit_config(&folder.0, "silent", false);
    let now = obi_protocol::time::now();
    let home = folder.0.join("home");
    blocked_job(
        &home.join("jobs"),
        "cccc0003",
        "research",
        Some(now - 5 * 60),
    );
    let obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    let decision = &call(&obid, "GET", "/v1/limit-decisions", None).body[0];
    let id = decision["id"].as_str().unwrap();
    let answered = answer(
        &obid,
        id,
        decision["revision"].as_u64().unwrap(),
        "resume",
        None,
    );
    assert_eq!(answered.status, 200, "{}", answered.text);
    std::thread::sleep(std::time::Duration::from_secs(17));
    let waiting = call(&obid, "GET", "/v1/jobs/cccc0003", None).body;
    assert_eq!(waiting["status"], "interrupted", "{waiting}");
    assert_eq!(waiting["round"], 1, "{waiting}");
    drop(obid);

    let folder = Folder::new("limit-resume-unread-reserve");
    let config = limit_config(&folder.0, "silent", false);
    let home = folder.0.join("home");
    blocked_job(
        &home.join("jobs"),
        "cccc0004",
        "research",
        Some(now - 31 * 60),
    );
    // The week read before the limit, at its reserve and not reset: it still holds.
    std::fs::write(home.join("usage.json"), json!({"stub":{"fiveHour":0.1,"sevenDay":0.85,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now-3600,"sevenDayObservedAt":now-3600}}).to_string()).unwrap();
    let obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    let decision = &call(&obid, "GET", "/v1/limit-decisions", None).body[0];
    let id = decision["id"].as_str().unwrap();
    let answered = answer(
        &obid,
        id,
        decision["revision"].as_u64().unwrap(),
        "resume",
        None,
    );
    assert_eq!(answered.status, 200, "{}", answered.text);
    let held = within(45, "the wait names the reserve", || {
        let j = call(&obid, "GET", "/v1/jobs/cccc0004", None).body;
        j["reason"]
            .as_str()?
            .contains("weekly reserve")
            .then_some(j)
    });
    assert_eq!(held["status"], "interrupted", "{held}");
    assert_eq!(held["round"], 1, "{held}");
}

/// LIMITS-2 (#1645): an approved Resume its account's weekly reserve alone holds says so, and waits.
#[test]
fn limits_2_a_resume_held_by_the_weekly_reserve_says_so() {
    let folder = Folder::new("limit-resume-reserve");
    let config = limit_config(&folder.0, "reserved", false);
    let now = obi_protocol::time::now();
    blocked_job(
        &folder.0.join("home/jobs"),
        "cccc0002",
        "research",
        Some(now - 120),
    );
    let obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    let decisions = call(&obid, "GET", "/v1/limit-decisions", None).body;
    let decision = &decisions[0];
    let id = decision["id"].as_str().unwrap();
    let answered = answer(
        &obid,
        id,
        decision["revision"].as_u64().unwrap(),
        "resume",
        None,
    );
    assert_eq!(answered.status, 200, "{}", answered.text);
    let waiting = within(45, "the wait names the reserve", || {
        let j = call(&obid, "GET", "/v1/jobs/cccc0002", None).body;
        j["reason"]
            .as_str()?
            .contains("weekly reserve")
            .then_some(j)
    });
    assert_eq!(waiting["status"], "interrupted", "{waiting}");
    assert_eq!(waiting["round"], 1, "{waiting}");
}

/// LIMITS-3 (#1645): a review stopped by the same limit as a task doesn't take Move away from the task: the
/// task moves, and the review alone is asked about again, with Resume and Stop.
#[test]
fn limits_3_a_review_in_the_group_leaves_the_task_its_move() {
    let folder = Folder::new("limit-review-move");
    let config = limit_config(&folder.0, "fake-claude", true);
    let jobs = folder.0.join("home/jobs");
    let now = obi_protocol::time::now();
    blocked_job(&jobs, "dddd0001", "research", Some(now + 3600));
    blocked_job(&jobs, "dddd0002", "review", Some(now + 3600));
    let obid = start(&folder.0, &config, None);
    let decision = within(20, "one decision for both offers Codex", || {
        let v = call(&obid, "GET", "/v1/limit-decisions", None).body;
        v.as_array()?
            .iter()
            .find(|d| {
                d["jobs"].as_array().is_some_and(|j| j.len() == 2)
                    && d["options"]
                        .as_array()
                        .is_some_and(|o| o.iter().any(|o| o["account"] == "codex"))
            })
            .cloned()
    });
    let id = decision["id"].as_str().unwrap();
    let revision = decision["revision"].as_u64().unwrap();
    let answered = answer(&obid, id, revision, "move", Some("codex"));
    assert_eq!(answered.status, 200, "{}", answered.text);
    let moved = within(60, "the task moves to Codex", || {
        let j = call(&obid, "GET", "/v1/jobs/dddd0001", None).body;
        (j["agent"] == "codex").then_some(j)
    });
    assert_eq!(moved["account"], "codex", "{moved}");
    let again = within(20, "the review is asked about alone", || {
        let v = call(&obid, "GET", "/v1/limit-decisions", None).body;
        v.as_array()?
            .iter()
            .find(|d| d["jobs"] == json!([{"id": "dddd0002", "title": d["jobs"][0]["title"]}]))
            .cloned()
    });
    let kinds: Vec<&str> = again["options"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|o| o["kind"].as_str())
        .collect();
    assert_eq!(kinds, ["resume", "stop"], "{again}");
    let review = call(&obid, "GET", "/v1/jobs/dddd0002", None).body;
    assert_eq!(review["status"], "interrupted", "{review}");
    assert_eq!(review["agent"], "claude", "{review}");
}

/// LIMITS-4 (#1645): options that change after the owner looked (a usage read, an account gone) don't
/// refuse their answer: only the jobs shown move the revision, and only a choice no longer
/// offered is refused.
#[test]
fn limits_4_an_answer_stands_when_only_other_options_changed() {
    let folder = Folder::new("limit-stale-answer");
    let config = limit_config(&folder.0, "fake-claude", true);
    let now = obi_protocol::time::now();
    blocked_job(
        &folder.0.join("home/jobs"),
        "eeee0001",
        "research",
        Some(now + 3600),
    );
    let obid = start(&folder.0, &config, None);
    let shown = within(20, "the decision offers Codex", || {
        let v = call(&obid, "GET", "/v1/limit-decisions", None).body;
        v.as_array()?
            .iter()
            .find(|d| {
                d["options"]
                    .as_array()
                    .is_some_and(|o| o.iter().any(|o| o["account"] == "codex"))
            })
            .cloned()
    });
    let id = shown["id"].as_str().unwrap();
    let revision = shown["revision"].as_u64().unwrap();
    std::fs::remove_file(folder.0.join("home/accounts/codex/auth.json")).unwrap();
    let now_shown = call(&obid, "GET", "/v1/limit-decisions", None).body;
    let now_shown = &now_shown[0];
    assert!(
        now_shown["options"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["account"] != "codex"),
        "{now_shown}"
    );
    assert_eq!(now_shown["revision"], revision, "{now_shown}");
    let gone = answer(&obid, id, revision, "move", Some("codex"));
    assert_ne!(gone.status, 200, "{}", gone.text);
    assert!(gone.text.contains("no longer available"), "{}", gone.text);
    let kept = answer(&obid, id, revision, "stop", None);
    assert_eq!(kept.status, 200, "{}", kept.text);
    let job = call(&obid, "GET", "/v1/jobs/eeee0001", None).body;
    assert_eq!(job["accountLimit"]["state"], "stop", "{job}");
}

/// A folder and config with the Claude account `stub` signed in, and its obid.
fn on_stub(name: &str) -> (Folder, Value, Obid) {
    let folder = Folder::new(name);
    let config = limit_config(&folder.0, "fake-claude", false);
    let obid = start(&folder.0, &config, None);
    (folder, config, obid)
}

fn post(obid: &Obid, task: &str) -> String {
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "investigate", "task": task, "account": "stub"});
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

fn job_now(obid: &Obid, id: &str) -> Value {
    call(obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

/// Waits until job `id` waits for its reset.
fn waiting_for_reset(obid: &Obid, id: &str) -> Value {
    within(60, "the job waits for its reset", || {
        let job = job_now(obid, id);
        (job["status"] == "queued" && !job["resumesAt"].is_null()).then_some(job)
    })
}

fn ended(obid: &Obid, id: &str) -> Value {
    within(90, "the job ends", || {
        let job = job_now(obid, id);
        (job["status"] != "queued" && job["status"] != "running").then_some(job)
    })
}

/// Waits until job `id` waits for the owner's decision.
fn asked(obid: &Obid, id: &str) -> Value {
    within(90, "the owner is asked", || {
        let job = job_now(obid, id);
        (!job["accountLimit"].is_null()).then_some(job)
    })
}

fn pending_decisions(obid: &Obid) -> Vec<Value> {
    call(obid, "GET", "/v1/limit-decisions", None)
        .body
        .as_array()
        .unwrap()
        .clone()
}

fn prompts_of(job: &Value) -> Vec<String> {
    let log = Path::new(job["worktree"].as_str().unwrap()).join(".obi/prompts.log");
    std::fs::read_to_string(log)
        .unwrap()
        .split("<<<end of prompt>>>\n")
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

fn read_job(folder: &Folder, id: &str) -> obid::job::Job {
    let file = folder.0.join("home/jobs").join(format!("{id}.json"));
    serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
}

fn write_job(folder: &Folder, job: &obid::job::Job) {
    let dir = folder.0.join("home/jobs");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{}.json", job.id));
    std::fs::write(file, serde_json::to_vec(job).unwrap()).unwrap();
}

/// A restart with job `id`'s time to resume come, as the Mac's clock would bring it, and `usage` as
/// `home/usage.json` says it then, when given.
fn its_time_comes(
    folder: &Folder,
    config: &Value,
    mut obid: Obid,
    id: &str,
    usage: Option<Value>,
) -> Obid {
    obid.stop();
    let mut job = read_job(folder, id);
    assert!(job.resumes_at.is_some(), "{job:?}");
    job.resumes_at = Some(obi_protocol::time::now() - 1);
    write_job(folder, &job);
    if let Some(usage) = usage {
        std::fs::write(folder.0.join("home/usage.json"), usage.to_string()).unwrap();
    }
    start(&folder.0, config, None)
}

/// LIMITS-5: a five-hour limit's job waits, with no card, 90 seconds past the reset its tool said,
/// its line saying when; then goes on in its same session, told so by Oby, with what the cut turn
/// was sent.
#[test]
fn limits_5_a_five_hour_limit_resumes_by_itself_in_its_session() {
    let (folder, config, obid) = on_stub("limits-5");
    let id = post(&obid, "LIMITED ONCE: 2 KEEP PROMPTS");
    let waits = waiting_for_reset(&obid, &id);
    let usage: Value =
        serde_json::from_slice(&std::fs::read(folder.0.join("home/usage.json")).unwrap()).unwrap();
    let reset = usage["stub"]["fiveHourResets"].as_i64().unwrap();
    assert_eq!(
        waits["resumesAt"],
        obi_protocol::time::format(reset + 90),
        "{waits}"
    );
    let step = waits["step"].as_str().unwrap();
    assert!(
        step.starts_with("Resuming at ") && (step.ends_with(" AM") || step.ends_with(" PM")),
        "{waits}"
    );
    // A kind the phones decode, the same words as its line.
    assert_eq!(waits["waiting"], json!({"kind": "cap", "text": step}));
    assert!(waits["accountLimit"].is_null(), "{waits}");
    assert!(waits["reason"].is_null(), "{waits}");
    assert!(pending_decisions(&obid).is_empty());
    let inbox = call(&obid, "GET", "/v1/inbox", None).body;
    assert!(!inbox.to_string().contains(&id), "{inbox}");

    let session = read_job(&folder, &id).session.unwrap();
    let branch = waits["branch"].clone();
    let obid = its_time_comes(&folder, &config, obid, &id, None);
    let done = ended(&obid, &id);
    assert_eq!(done["status"], "done", "{done}");
    assert!(
        done["summary"]
            .as_str()
            .unwrap()
            .contains(&format!("went on: resume={session} marked=true")),
        "{done}"
    );
    assert_eq!(done["branch"], branch);
    assert_eq!(done["account"], "stub");
    assert!(done["moves"].as_array().unwrap().is_empty(), "{done}");
    let prompts = prompts_of(&done);
    assert_eq!(prompts.len(), 2, "{done}");
    assert!(prompts[1].contains("five-hour usage limit"), "{prompts:?}");
    assert!(
        prompts[1].contains("What you were last sent"),
        "{prompts:?}"
    );
    assert!(prompts[1].contains("LIMITED ONCE: 2"), "{prompts:?}");
}

/// LIMITS-5: a job that starts on an account a five-hour limit holds waits for the same reset, so
/// the next job on the account brings no card either.
#[test]
fn limits_5_a_job_that_starts_on_a_full_account_waits_too() {
    let (_folder, _config, obid) = on_stub("limits-5-full");
    let first = waiting_for_reset(&obid, &post(&obid, "LIMITED: 3600"));
    let second = waiting_for_reset(&obid, &post(&obid, "Look around"));
    assert_eq!(second["resumesAt"], first["resumesAt"]);
    assert!(pending_decisions(&obid).is_empty());
}

/// LIMITS-5: while it waits it takes no slot and holds no job behind it.
#[test]
fn limits_5_a_waiting_job_takes_no_slot() {
    let (_folder, _config, obid) = on_stub("limits-5-slot");
    let waits = post(&obid, "LIMITED: 2");
    waiting_for_reset(&obid, &waits);
    std::thread::sleep(std::time::Duration::from_secs(3));
    let one = call(
        &obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"maxRunning": 1})),
    );
    assert_eq!(one.status, 200, "{}", one.text);
    let next = post(&obid, "Look around");
    assert_eq!(ended(&obid, &next)["status"], "done");
    assert_eq!(job_now(&obid, &waits)["status"], "queued");
}

/// LIMITS-6: Stop ends the wait, for good.
#[test]
fn limits_6_stop_ends_the_wait() {
    let (_folder, _config, obid) = on_stub("limits-6");
    let id = post(&obid, "LIMITED: 2 KEEP PROMPTS");
    waiting_for_reset(&obid, &id);
    let stopped = call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    assert_eq!(stopped.status, 200, "{}", stopped.text);
    assert_eq!(stopped.body["status"], "stopped", "{}", stopped.text);
    assert!(stopped.body["resumesAt"].is_null(), "{}", stopped.text);
    assert!(stopped.body["waiting"].is_null(), "{}", stopped.text);
    assert_eq!(prompts_of(&stopped.body).len(), 1);
}

/// LIMITS-7: the wait survives a restart: the same time on its line, nothing counted as cut, and it
/// still resumes.
#[test]
fn limits_7_the_wait_survives_a_restart() {
    let (folder, config, mut obid) = on_stub("limits-7");
    let id = post(&obid, "LIMITED ONCE: 2 KEEP PROMPTS");
    let waits = waiting_for_reset(&obid, &id);
    obid.stop();
    let obid = start(&folder.0, &config, None);
    // Past a tick of the clock that looks for resets, its time not come.
    std::thread::sleep(std::time::Duration::from_secs(17));
    let still = job_now(&obid, &id);
    assert_eq!(still["status"], "queued", "{still}");
    assert_eq!(still["resumesAt"], waits["resumesAt"], "{still}");
    assert_eq!(still["step"], waits["step"], "{still}");
    assert_eq!(still["waiting"], waits["waiting"], "{still}");
    assert_eq!(prompts_of(&still).len(), 1, "{still}");
    let kept = read_job(&folder, &id);
    assert_eq!(kept.recovery.restarts, 0, "{kept:?}");
    assert!(kept.crash_resumes.is_empty(), "{kept:?}");
    let obid = its_time_comes(&folder, &config, obid, &id, None);
    assert_eq!(ended(&obid, &id)["status"], "done");
}

/// LIMITS-8: jobs whose reset came resume one at a time, a tick of the clock apart.
#[test]
fn limits_8_jobs_resume_one_at_a_time() {
    let folder = Folder::new("limits-8");
    let config = limit_config(&folder.0, "fake-claude", false);
    for id in ["aaaa0801", "aaaa0802"] {
        let mut job = obid::job::Job::new(
            id.into(),
            "obi-e2e/fixture-app".into(),
            "investigate".into(),
            "Look around".into(),
            "claude".into(),
            "stub".into(),
        );
        job.branch = format!("oby/investigate-{id}");
        job.status = obid::job::Status::Queued;
        job.round = Some(1);
        job.resumes_at = Some(obi_protocol::time::now() - 1);
        job.waiting = Some(obid::job::Waiting {
            kind: "cap".into(),
            text: "Resuming at 3:40 PM".into(),
            reserved: None,
            tool: None,
        });
        write_job(&folder, &job);
    }
    let obid = start(&folder.0, &config, None);
    let resumed = |id: &str| job_now(&obid, id)["resumesAt"].is_null();
    within(60, "the first resumes", || {
        (resumed("aaaa0801") || resumed("aaaa0802")).then_some(())
    });
    let first = std::time::Instant::now();
    assert!(
        !(resumed("aaaa0801") && resumed("aaaa0802")),
        "both resumed at once"
    );
    within(60, "the second resumes", || {
        (resumed("aaaa0801") && resumed("aaaa0802")).then_some(())
    });
    assert!(
        first.elapsed().as_secs() >= 10,
        "the second resumed after {:?}",
        first.elapsed()
    );
}

/// LIMITS-9: a job waits out at most 3 five-hour limits; the next asks, as any limit does, and only
/// the owner's signed choice carries it on.
#[test]
fn limits_9_three_tries_then_the_owner_is_asked() {
    let (folder, config, mut obid) = on_stub("limits-9");
    let id = post(&obid, "LIMITED AGAIN: 2 KEEP PROMPTS");
    for tries in 1..=3 {
        waiting_for_reset(&obid, &id);
        within(10, "the try is counted", || {
            (read_job(&folder, &id).recovery.limit_resumes == tries).then_some(())
        });
        obid = its_time_comes(&folder, &config, obid, &id, None);
        within(60, "it runs again", || {
            (prompts_of(&job_now(&obid, &id)).len() == tries + 1).then_some(())
        });
    }
    let asks = asked(&obid, &id);
    assert_eq!(asks["status"], "interrupted", "{asks}");
    assert!(asks["resumesAt"].is_null(), "{asks}");
    assert_eq!(asks["accountLimit"]["state"], "pending", "{asks}");
    assert_eq!(prompts_of(&asks).len(), 4, "{asks}");
    let decisions = pending_decisions(&obid);
    assert_eq!(decisions[0]["jobs"][0]["id"], id.as_str(), "{decisions:?}");
    let unsigned = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": "Try again"})),
    );
    assert_eq!(unsigned.status, 403, "{}", unsigned.text);
}

/// LIMITS-10: a weekly limit, a limit with no known reset, or one further off than five hours: the
/// owner's decision, as before.
#[test]
fn limits_10_any_other_limit_asks_as_before() {
    for (name, task) in [
        ("limits-10-week", "LIMITED WEEK: 86400"),
        ("limits-10-near-week", "LIMITED WEEK: 3600"),
        ("limits-10-none", "LIMITED NO RESET"),
        ("limits-10-far", "LIMITED: 21600"),
    ] {
        let (_folder, _config, obid) = on_stub(name);
        let id = post(&obid, task);
        let asks = asked(&obid, &id);
        assert_eq!(asks["status"], "interrupted", "{task}: {asks}");
        assert!(asks["resumesAt"].is_null(), "{task}: {asks}");
        assert_eq!(asks["accountLimit"]["state"], "pending", "{task}");
        let decisions = pending_decisions(&obid);
        assert_eq!(decisions[0]["jobs"][0]["id"], id.as_str(), "{task}");
    }
}

/// LIMITS-11: at its time, an account a later five-hour reset still holds gets a new wait, no try
/// counted; a week that filled meanwhile, or an account that's gone, goes to the owner.
#[test]
fn limits_11_an_account_still_held_at_the_reset() {
    let now = obi_protocol::time::now();

    let (folder, config, obid) = on_stub("limits-11-later");
    let id = post(&obid, "LIMITED: 2");
    waiting_for_reset(&obid, &id);
    let later = now + 1800;
    let usage = json!({"stub": {"fiveHour": 1.0, "sevenDay": 0.4, "fiveHourResets": later,
        "sevenDayResets": now + 86_400, "fullUntil": later}});
    let obid = its_time_comes(&folder, &config, obid, &id, Some(usage));
    let at = obi_protocol::time::format(later + 90);
    within(40, "it waits for the later reset", || {
        (job_now(&obid, &id)["resumesAt"] == at).then_some(())
    });
    let job = job_now(&obid, &id);
    assert_eq!(job["status"], "queued", "{job}");
    assert_eq!(read_job(&folder, &id).recovery.limit_resumes, 1);
    drop(obid);

    let (folder, config, obid) = on_stub("limits-11-week");
    let id = post(&obid, "LIMITED: 2");
    waiting_for_reset(&obid, &id);
    let usage =
        json!({"stub": {"fiveHour": 0.1, "sevenDay": 0.97, "sevenDayResets": now + 86_400}});
    let obid = its_time_comes(&folder, &config, obid, &id, Some(usage));
    let job = asked(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["accountLimit"]["state"], "pending", "{job}");
    assert_eq!(pending_decisions(&obid).len(), 1);
    drop(obid);

    let (folder, config, obid) = on_stub("limits-11-gone");
    let id = post(&obid, "LIMITED: 2");
    waiting_for_reset(&obid, &id);
    std::fs::remove_file(folder.0.join("home/accounts/stub/.credentials.json")).unwrap();
    let obid = its_time_comes(&folder, &config, obid, &id, None);
    let job = asked(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["accountLimit"]["state"], "pending", "{job}");
    assert!(
        job["reason"]
            .as_str()
            .unwrap()
            .contains("no longer connected"),
        "{job}"
    );
}

/// LIMITS-12: resuming changes nothing else: the mode, the untrusted mark, the budget, the ceiling.
/// A limit isn't a passing error, and words for a waiting job go with its resume.
#[test]
fn limits_12_resuming_keeps_everything_else() {
    let (folder, config, obid) = on_stub("limits-12");
    let id = post(&obid, "LIMITED ONCE: 2 KEEP PROMPTS");
    waiting_for_reset(&obid, &id);
    let told = call_typed(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({"text": "Also look at the readme."}),
    );
    assert_eq!(told.status, 200, "{}", told.text);
    let still = job_now(&obid, &id);
    assert_eq!(still["status"], "queued", "{still}");
    assert!(!still["resumesAt"].is_null(), "{still}");
    drop(obid);
    let mut job = read_job(&folder, &id);
    job.mode = Some("plan".into());
    job.untrusted = true;
    write_job(&folder, &job);
    let obid = start(&folder.0, &config, None);
    let obid = its_time_comes(&folder, &config, obid, &id, None);
    let done = ended(&obid, &id);
    assert_eq!(done["mode"], "plan", "{done}");
    assert_eq!(done["untrusted"], true, "{done}");
    let prompts = prompts_of(&done);
    assert!(
        prompts[1].contains("Also look at the readme."),
        "{prompts:?}"
    );
    let runs = std::fs::read_to_string(
        Path::new(done["worktree"].as_str().unwrap()).join(".obi/args.log"),
    )
    .unwrap();
    assert!(runs.contains("--permission-mode plan"), "{runs}");
    let kept = read_job(&folder, &id);
    assert_eq!(kept.recovery.errors, 0, "{kept:?}");

    let waiting = |folder: &Folder, id: &str| {
        let mut job = obid::job::Job::new(
            id.into(),
            "obi-e2e/fixture-app".into(),
            "investigate".into(),
            "Look around".into(),
            "claude".into(),
            "stub".into(),
        );
        job.branch = format!("oby/investigate-{id}");
        job.status = obid::job::Status::Queued;
        job.round = Some(1);
        job.session = Some("00000000-0000-4000-8000-000000000001".into());
        job.started = Some(obi_protocol::time::now() - 200);
        job.resumes_at = Some(obi_protocol::time::now() - 1);
        job.waiting = Some(obid::job::Waiting {
            kind: "cap".into(),
            text: "Resuming at 3:40 PM".into(),
            reserved: None,
            tool: None,
        });
        write_job(folder, &job);
        job
    };
    // The ceiling counts what it ran, and the wait none of it: it holds as it did.
    let folder = Folder::new("limits-12-ceiling");
    let mut config = limit_config(&folder.0, "fake-claude", false);
    config["jobCeilingHours"] = json!(1);
    let mut job = waiting(&folder, "aaaa1201");
    job.ran_secs = Some(3600);
    write_job(&folder, &job);
    let obid = start(&folder.0, &config, None);
    let job = ended(&obid, "aaaa1201");
    assert_eq!(job["stalled"]["why"], "ceiling", "{job}");
    drop(obid);
    // The budget holds as it did.
    let folder = Folder::new("limits-12-budget");
    let mut config = limit_config(&folder.0, "fake-claude", false);
    config["jobTokenBudget"] = json!(1000);
    let mut job = waiting(&folder, "aaaa1202");
    job.usage = Some(serde_json::from_value(json!({"input": 2000})).unwrap());
    write_job(&folder, &job);
    let obid = start(&folder.0, &config, None);
    let job = ended(&obid, "aaaa1202");
    assert_eq!(job["stalled"]["why"], "budget", "{job}");
}

/// LIMITS-13: a wait an older release left a pending decision stays one, whatever its reset says.
/// `resets::old_automatic_retries_become_one_pending_decision_after_restart` holds the older still.
#[test]
fn limits_13_an_older_wait_stays_the_owners() {
    after_restart("limits-13", true, None, false);
}
