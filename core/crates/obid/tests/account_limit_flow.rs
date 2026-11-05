//! Account-limit decisions exercise the real daemon and stand-in agents (#1563).
mod support;

use serde_json::{Value, json};
use std::path::Path;
use support::*;

#[test]
fn owner_moves_a_limit_stopped_job_to_a_connected_account_of_another_tool() {
    // ACC-6: ordinary live limits move or wait by themselves. This keeps the signed Move path
    // (LIMITS-10 / older ask): a pending decision, Codex offered, owner moves (AGENTS-6).
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
    let id = "aaaa0mov";
    blocked_job(
        &folder.0.join("home/jobs"),
        id,
        "research",
        Some(now + 3600),
    );
    let mut job = read_job(&folder, id);
    job.task = "Look around".into();
    write_job(&folder, &job);
    let branch = job.branch.clone();
    let obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    let stopped = job_now(&obid, id);
    assert_eq!(stopped["status"], "interrupted", "{stopped}");
    assert!(stopped["resumesAt"].is_null());
    assert!(stopped["moves"].as_array().unwrap().is_empty());
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
    let threads = home.join("logins").join(id).join("threads");
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
    let usage_file = folder.0.join("home/usage.json");
    let usage = usage.unwrap_or_else(|| {
        // Advance the account's clock together with the job's, rather than relying on
        // a two-second reset expiring while obid shuts down and starts again.
        let mut usage: Value =
            serde_json::from_slice(&std::fs::read(&usage_file).unwrap()).unwrap();
        let account = usage.get_mut(&job.account).unwrap();
        account["fiveHourResets"] = json!(obi_protocol::time::now() - 1);
        account["fullUntil"] = json!(obi_protocol::time::now() - 1);
        usage
    });
    std::fs::write(usage_file, usage.to_string()).unwrap();
    start(&folder.0, config, None)
}

/// LIMITS-5: a five-hour limit's job waits, with no card, 90 seconds past the reset its tool said,
/// its line saying when; then goes on in its same session, told so by Oby, with what the cut turn
/// was sent.
#[test]
fn limits_5_a_five_hour_limit_resumes_by_itself_in_its_session() {
    let (folder, config, obid) = on_stub("limits-5");
    let id = post(&obid, "LIMITED ONCE: 300 KEEP PROMPTS");
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
    assert!(prompts[1].contains("LIMITED ONCE: 300"), "{prompts:?}");
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
    let folder = Folder::new("limits-5-slot");
    let config = limit_config(&folder.0, "fake-claude", true);
    // Codex's reading is an hour old: room nobody verified, so the job waits instead of moving (LIMITS-5).
    let now = obi_protocol::time::now();
    std::fs::write(
        folder.0.join("home/usage.json"),
        json!({"codex":{"fiveHour":0.1,"sevenDay":0.2,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now-3600,"sevenDayObservedAt":now-3600}}).to_string(),
    )
    .unwrap();
    let obid = start(&folder.0, &config, None);
    let waits = post(&obid, "LIMITED: 300");
    waiting_for_reset(&obid, &waits);
    let one = call(
        &obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"maxRunning": 1})),
    );
    assert_eq!(one.status, 200, "{}", one.text);
    // An eligible account can work while the first account remains at its limit.
    let next = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "investigate",
            "task": "Look around", "agent": "codex", "account": "codex"}}),
        ),
    );
    assert_eq!(next.status, 201, "{}", next.text);
    let next = next.body["id"].as_str().unwrap();
    assert_eq!(ended(&obid, next)["status"], "done");
    let still = job_now(&obid, &waits);
    assert_eq!(still["status"], "queued", "{still}");
    assert_eq!(still["account"], "stub", "{still}");
}

/// LIMITS-6: Stop ends the wait, for good.
#[test]
fn limits_6_stop_ends_the_wait() {
    let (_folder, _config, obid) = on_stub("limits-6");
    let id = post(&obid, "LIMITED: 300 KEEP PROMPTS");
    waiting_for_reset(&obid, &id);
    let stopped = call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    assert_eq!(stopped.status, 200, "{}", stopped.text);
    assert_eq!(stopped.body["status"], "stopped", "{}", stopped.text);
    assert!(stopped.body["resumesAt"].is_null(), "{}", stopped.text);
    assert!(stopped.body["waiting"].is_null(), "{}", stopped.text);
    assert_eq!(prompts_of(&stopped.body).len(), 1);
}

/// LIMITS-6: while it waits its account is busy, as with any queued job: Stop is what frees it, since
/// a queued job can't move.
#[test]
fn limits_6_stop_frees_the_waiting_jobs_account() {
    let (_folder, _config, obid) = on_stub("limits-6-account");
    let id = post(&obid, "LIMITED: 3600");
    waiting_for_reset(&obid, &id);
    let busy = call(&obid, "DELETE", "/v1/accounts/stub", None);
    assert_eq!(busy.status, 409, "{}", busy.text);
    assert!(busy.text.contains("stop it first"), "{}", busy.text);
    assert!(!busy.text.contains("or move it"), "{}", busy.text);
    call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    assert_eq!(call(&obid, "DELETE", "/v1/accounts/stub", None).status, 204);
}

/// LIMITS-7: the wait survives a restart: the same time on its line, nothing counted as cut, and it
/// still resumes.
#[test]
fn limits_7_the_wait_survives_a_restart() {
    let (folder, config, obid) = on_stub("limits-7");
    let id = post(&obid, "LIMITED ONCE: 300 KEEP PROMPTS");
    let waits = waiting_for_reset(&obid, &id);
    within(10, "the wait is kept", || {
        read_job(&folder, &id).resumes_at.is_some().then_some(())
    });
    // Going down with no clean stop: a job at work counts that against it, and this one isn't.
    drop(obid);
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

/// LIMITS-9: a job waits out at most 3 five-hour limits; the next tries ACC-6 (move or wait), with
/// no question when nowhere else has room.
#[test]
fn limits_9_past_three_tries_moves_or_waits_with_no_question() {
    let (folder, config, mut obid) = on_stub("limits-9");
    let id = post(&obid, "LIMITED AGAIN: 300 KEEP PROMPTS");
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
    let waits = waiting_for_reset(&obid, &id);
    assert_eq!(waits["status"], "queued", "{waits}");
    assert!(waits["accountLimit"].is_null(), "{waits}");
    assert!(
        pending_decisions(&obid).is_empty(),
        "{:?}",
        pending_decisions(&obid)
    );
    assert_eq!(read_job(&folder, &id).recovery.limit_resumes, 3);
}

/// Job `id`, of `kind` and asked `task`, its account's limit stopped it `tries` waits in, and the owner's signed
/// Resume of it: the decision they answered, and obid running it.
fn signed_resume(
    folder: &Folder,
    id: &str,
    kind: &str,
    task: &str,
    tries: usize,
) -> (String, Value, Obid) {
    let config = limit_config(&folder.0, "silent", false);
    blocked_job(
        &folder.0.join("home/jobs"),
        id,
        kind,
        Some(obi_protocol::time::now() - 31 * 60),
    );
    let mut job = read_job(folder, id);
    job.task = task.into();
    job.recovery.limit_resumes = tries;
    write_job(folder, &job);
    let obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    let decision = pending_decisions(&obid).remove(0);
    let first = decision["id"].as_str().unwrap().to_owned();
    let answered = answer(
        &obid,
        &first,
        decision["revision"].as_u64().unwrap(),
        "resume",
        None,
    );
    assert_eq!(answered.status, 200, "{}", answered.text);
    (first, config, obid)
}

/// LIMITS-9: a signed Resume (left-alone / older ask) carries the job on and gives it no new
/// five-hour tries; its next limit follows ACC-6 (wait with no question when nowhere has room).
#[test]
fn limits_9_a_signed_resume_gives_no_new_tries() {
    let folder = Folder::new("limits-9-signed");
    let (_first, _config, obid) =
        signed_resume(&folder, "cccc0901", "investigate", "LIMITED AGAIN: 2", 3);
    let waits = waiting_for_reset(&obid, "cccc0901");
    assert_eq!(waits["status"], "queued", "{waits}");
    assert!(waits["accountLimit"].is_null(), "{waits}");
    assert!(
        pending_decisions(&obid).is_empty(),
        "{:?}",
        pending_decisions(&obid)
    );
    assert_eq!(read_job(&folder, "cccc0901").recovery.limit_resumes, 3);
}

/// LIMITS-9: the owner's own words start the count again; anyone else's don't.
#[test]
fn limits_9_only_the_owners_own_words_start_the_count_again() {
    let (folder, _config, obid) = on_stub("limits-9-words");
    let id = post(&obid, "LIMITED: 3600");
    waiting_for_reset(&obid, &id);
    let target = format!("/v1/jobs/{id}/messages");
    let tries = || read_job(&folder, &id).recovery.limit_resumes;
    within(10, "the try is counted", || (tries() == 1).then_some(()));
    let unsigned = call(&obid, "POST", &target, Some(&json!({"text": "Look here"})));
    assert_eq!(unsigned.status, 200, "{}", unsigned.text);
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert_eq!(tries(), 1);
    let typed = call_typed(&obid, "POST", &target, &json!({"text": "Look there"}));
    assert_eq!(typed.status, 200, "{}", typed.text);
    within(10, "the count starts again", || {
        (tries() == 0).then_some(())
    });
    assert_eq!(job_now(&obid, &id)["status"], "queued");
}

/// LIMITS-10 / LIMITS-17: a weekly limit, a limit with no known reset, or one further off than five
/// hours: with nowhere to move it waits for the reset (or an hour), with no question.
#[test]
fn limits_10_any_other_limit_moves_or_waits_with_no_question() {
    for (name, task) in [
        ("limits-10-week", "LIMITED WEEK: 86400"),
        ("limits-10-near-week", "LIMITED WEEK: 3600"),
        ("limits-10-none", "LIMITED NO RESET"),
        ("limits-10-words", "LIMITED NO EVENT"),
        ("limits-10-far", "LIMITED: 21600"),
    ] {
        let (_folder, _config, obid) = on_stub(name);
        let id = post(&obid, task);
        let waits = waiting_for_reset(&obid, &id);
        assert_eq!(waits["status"], "queued", "{task}: {waits}");
        assert!(waits["accountLimit"].is_null(), "{task}: {waits}");
        assert!(
            pending_decisions(&obid).is_empty(),
            "{task}: {:?}",
            pending_decisions(&obid)
        );
    }
}

/// LIMITS-10: a job that starts on an account a week holds waits with no question, as ACC-6.
#[test]
fn limits_10_a_job_that_starts_on_an_account_a_week_holds_waits_with_no_question() {
    let (_folder, _config, obid) = on_stub("limits-10-start");
    let first = waiting_for_reset(&obid, &post(&obid, "LIMITED WEEK: 86400"));
    assert!(!first["resumesAt"].is_null(), "{first}");
    let second = waiting_for_reset(&obid, &post(&obid, "Look around"));
    assert!(!second["resumesAt"].is_null(), "{second}");
    assert!(second["accountLimit"].is_null(), "{second}");
}

/// LIMITS-11: at its time, an account a later five-hour reset still holds gets a new wait, no try
/// counted; a week that filled meanwhile, or an account that's gone, goes to the owner.
#[test]
fn limits_11_an_account_still_held_at_the_reset() {
    let now = obi_protocol::time::now();

    let (folder, config, obid) = on_stub("limits-11-later");
    let id = post(&obid, "LIMITED: 300");
    waiting_for_reset(&obid, &id);
    let later = now + 1800;
    let usage = json!({"stub": {"fiveHour": 1.0, "sevenDay": 0.4, "fiveHourResets": later,
        "sevenDayResets": now + 86_400, "fullUntil": later}});
    let obid = its_time_comes(&folder, &config, obid, &id, Some(usage));
    let at = obi_protocol::time::format(later + 90);
    within(60, "it waits for the later reset", || {
        (job_now(&obid, &id)["resumesAt"] == at).then_some(())
    });
    let job = job_now(&obid, &id);
    assert_eq!(job["status"], "queued", "{job}");
    assert_eq!(read_job(&folder, &id).recovery.limit_resumes, 1);
    drop(obid);

    let (folder, config, obid) = on_stub("limits-11-week");
    let id = post(&obid, "LIMITED: 300");
    waiting_for_reset(&obid, &id);
    let usage =
        json!({"stub": {"fiveHour": 0.1, "sevenDay": 0.97, "sevenDayResets": now + 86_400}});
    let obid = its_time_comes(&folder, &config, obid, &id, Some(usage));
    // ACC-6: nowhere else has room, so it waits for the week's reset, with no question.
    let job = waiting_for_reset(&obid, &id);
    assert_eq!(job["status"], "queued", "{job}");
    assert!(job["accountLimit"].is_null(), "{job}");
    assert!(pending_decisions(&obid).is_empty());
    drop(obid);

    let (folder, config, obid) = on_stub("limits-11-gone");
    let id = post(&obid, "LIMITED: 300");
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
    let (folder, config, mut obid) = on_stub("limits-12");
    let id = post(&obid, "LIMITED ONCE: 300 KEEP PROMPTS");
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
    obid.stop();
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

/// LIMITS-12: a resumed run is the round it was: uncounted, and it ends as that round would. A round
/// of the agent's own says nothing was committed; one the owner asked for leaves that to their words.
#[test]
fn limits_12_a_resumed_run_ends_as_its_round_would() {
    let (folder, config, obid) = on_stub("limits-12-round");
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "build", "account": "stub", "rounds": 3,
        "task": "LIMITED ONCE: 300"});
    let posted = call(&obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(posted.status, 201, "{}", posted.text);
    let id = posted.body["id"].as_str().unwrap().to_owned();
    waiting_for_reset(&obid, &id);
    let obid = its_time_comes(&folder, &config, obid, &id, None);
    let done = ended(&obid, &id);
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(
        done["reason"], "Nothing committed, so nothing was published.",
        "{done}"
    );
    assert_eq!(done["nothingDone"], true, "{done}");
    assert_eq!(done["round"], 1, "{done}");
    assert_eq!(done["rounds"], 3, "{done}");
    drop(obid);

    let folder = Folder::new("limits-12-asked");
    let id = "cccc1201";
    let (_, config, obid) = signed_resume(&folder, id, "build", "LIMITED ONCE: 300", 0);
    waiting_for_reset(&obid, id);
    let obid = its_time_comes(&folder, &config, obid, id, None);
    let done = ended(&obid, id);
    assert_eq!(done["status"], "done", "{done}");
    assert!(done["reason"].is_null(), "{done}");
}

/// LIMITS-14: a job waiting on a pull request merged or closed since spends no fresh window: it's
/// let go, or, with commits that never went out, stopped in sight. Words for it still go.
#[test]
fn limits_14_a_wait_on_an_ended_pull_request_doesnt_run() {
    let folder = Folder::new("limits-14");
    let config = limit_config(&folder.0, "fake-claude", false);
    let waiting = |id: &str, pushed: u64, heard: &[&str]| {
        let mut job = obid::job::Job::new(
            id.into(),
            "obi-e2e/fixture-app".into(),
            "fix".into(),
            "Look around".into(),
            "claude".into(),
            "stub".into(),
        );
        job.branch = format!("oby/fix-{id}");
        job.base = Some("dev".into());
        job.status = obid::job::Status::Queued;
        job.round = Some(2);
        job.commits = Some(2);
        job.pushed = Some(pushed);
        job.published = Some("https://github.com/obi-e2e/fixture-app/pull/7".into());
        job.pull_request = Some(
            serde_json::from_value(json!({"state": "MERGED", "base": "dev",
                "checkedAt": obi_protocol::time::now()}))
            .unwrap(),
        );
        job.heard = heard.iter().map(|h| (*h).to_owned()).collect();
        job.resumes_at = Some(obi_protocol::time::now() + 3600);
        job.waiting = Some(obid::job::Waiting {
            kind: "cap".into(),
            text: "Resuming at 3:40 PM".into(),
            reserved: None,
            tool: None,
        });
        write_job(&folder, &job);
    };
    waiting("aaaa1401", 2, &[]);
    waiting("aaaa1402", 1, &[]);
    waiting("aaaa1403", 2, &["Also look at the readme."]);
    let obid = start(&folder.0, &config, None);
    let gone = within(30, "the wait is let go", || {
        let job = job_now(&obid, "aaaa1401");
        job["dismissed"].is_string().then_some(job)
    });
    assert_eq!(gone["status"], "stopped", "{gone}");
    assert!(gone["resumesAt"].is_null(), "{gone}");
    assert!(
        gone["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["what"] == obid::jobs::LET_GO),
        "{gone}"
    );
    let kept = job_now(&obid, "aaaa1402");
    assert_eq!(kept["status"], "stopped", "{kept}");
    assert!(kept["dismissed"].is_null(), "{kept}");
    assert!(kept["resumesAt"].is_null(), "{kept}");
    assert!(
        kept["reason"]
            .as_str()
            .unwrap()
            .contains("merged or closed"),
        "{kept}"
    );
    let told = job_now(&obid, "aaaa1403");
    assert_eq!(told["status"], "queued", "{told}");
    assert!(!told["resumesAt"].is_null(), "{told}");
}

/// LIMITS-13: a wait an older release left a pending decision stays one, whatever its reset says.
/// `resets::old_automatic_retries_become_one_pending_decision_after_restart` holds the older still.
#[test]
fn limits_13_an_older_wait_stays_the_owners() {
    after_restart("limits-13", true, None, false);
}

/// Two Claude accounts: `stub` (will hit the limit) and `room` (most room), both signed in.
fn on_two_claude(name: &str) -> (Folder, Value, Obid) {
    let folder = Folder::new(name);
    let mut config = limit_config(&folder.0, "fake-claude", false);
    let home = folder.0.join("home");
    let room = home.join("accounts/room");
    std::fs::create_dir_all(&room).unwrap();
    let now = obi_protocol::time::now();
    std::fs::write(
        room.join(".credentials.json"),
        json!({"claudeAiOauth":{"accessToken":"fake-room","expiresAt":(now+86400)*1000}})
            .to_string(),
    )
    .unwrap();
    config["agents"]["claude"]["accounts"] = json!([
        {"id":"stub","name":"Stub","configDir": home.join("accounts/stub")},
        {"id":"room","name":"Room","configDir": room},
    ]);
    std::fs::write(
        home.join("usage.json"),
        json!({
            "stub":{"fiveHour":0.99,"sevenDay":0.5,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now,"sevenDayObservedAt":now},
            "room":{"fiveHour":0.1,"sevenDay":0.2,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now,"sevenDayObservedAt":now}
        })
        .to_string(),
    )
    .unwrap();
    let tuned = call_settings_before_start(&folder.0, &config, "room", "claude-fable-5-1");
    let obid = start_with(&folder.0, &tuned, None, &[("OBID_USAGE_READS", "on")]);
    (folder, tuned, obid)
}

/// Writes Settings › Agents model for `account` before obid starts.
fn call_settings_before_start(folder: &Path, config: &Value, account: &str, model: &str) -> Value {
    let home = folder.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let settings = json!({"tuning": {account: {"model": model}}});
    std::fs::write(home.join("settings.json"), settings.to_string()).unwrap();
    config.clone()
}

/// LIMITS-15 (ACC-6): a limit moves by itself to the same tool's account with the most room.
#[test]
fn limits_15_a_limit_moves_by_itself_to_the_same_tools_account_with_most_room() {
    let (_folder, _config, obid) = on_two_claude("limits-15");
    let id = post(&obid, "LIMITED WEEK: 86400");
    let moved = within(90, "it moved to room by itself", || {
        let j = job_now(&obid, &id);
        (j["account"] == "room"
            && (j["status"] == "done" || j["status"] == "running" || j["status"] == "queued"))
            .then_some(j)
    });
    assert_eq!(moved["agent"], "claude", "{moved}");
    assert_eq!(moved["model"], "claude-fable-5-1", "{moved}");
    assert_eq!(moved["moves"].as_array().unwrap().len(), 1, "{moved}");
    assert!(moved["accountLimit"].is_null(), "{moved}");
    assert!(pending_decisions(&obid).is_empty());
    let conversation = moved["conversation"].as_str().unwrap();
    let turns = call(
        &obid,
        "GET",
        &format!("/v1/conversation?id={conversation}"),
        None,
    )
    .body;
    let text = turns
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["who"] == "oby")
        .map(|t| t["text"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Claude Code hit its limit, so this moved to Claude Code."),
        "{text}"
    );
}

/// LIMITS-16 (ACC-6): with no room on the same tool, it moves to another tool from its handoff.
#[test]
fn limits_16_a_limit_moves_by_itself_to_another_tool() {
    let folder = Folder::new("limits-16");
    let config = limit_config(&folder.0, "fake-claude", true);
    let home = folder.0.join("home");
    let now = obi_protocol::time::now();
    std::fs::write(
        home.join("settings.json"),
        json!({"tuning": {"codex": {"model": "gpt-6-astra", "effort": "low"}}}).to_string(),
    )
    .unwrap();
    // stub has no room; only Codex does.
    std::fs::write(
        home.join("usage.json"),
        json!({
            "stub":{"fiveHour":0.99,"sevenDay":0.9,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now,"sevenDayObservedAt":now},
            "codex":{"fiveHour":0.1,"sevenDay":0.2,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now,"sevenDayObservedAt":now}
        })
        .to_string(),
    )
    .unwrap();
    let obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    let id = post(&obid, "LIMITED WEEK: 86400");
    let moved = within(90, "it moved to Codex by itself", || {
        let j = job_now(&obid, &id);
        (j["agent"] == "codex").then_some(j)
    });
    assert_eq!(moved["account"], "codex", "{moved}");
    assert_eq!(moved["model"], "gpt-6-astra", "{moved}");
    assert_eq!(moved["effort"], "low", "{moved}");
    assert_eq!(moved["moves"].as_array().unwrap().len(), 1, "{moved}");
    let done = within(90, "Codex finishes", || {
        let j = job_now(&obid, &id);
        (j["status"] == "done").then_some(j)
    });
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
    let conversation = done["conversation"].as_str().unwrap();
    let turns = call(
        &obid,
        "GET",
        &format!("/v1/conversation?id={conversation}"),
        None,
    )
    .body;
    let said = turns
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["who"] == "oby")
        .map(|t| t["text"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("Claude Code hit its limit, so this moved to Codex."),
        "{said}"
    );
}

/// LIMITS-17 (ACC-6): with no account with room, it waits for the reset with no question.
#[test]
fn limits_17_with_no_room_it_waits_for_the_reset_with_no_question() {
    let (_folder, _config, obid) = on_stub("limits-17");
    let id = post(&obid, "LIMITED WEEK: 86400");
    let waits = waiting_for_reset(&obid, &id);
    assert!(waits["accountLimit"].is_null(), "{waits}");
    assert!(pending_decisions(&obid).is_empty());
}

/// LIMITS-18 (ACC-6): a waiting job moves when room appears on another account.
#[test]
fn limits_18_a_waiting_job_moves_when_room_appears() {
    let (folder, config, obid) = on_stub("limits-18");
    let id = post(&obid, "LIMITED WEEK: 86400");
    waiting_for_reset(&obid, &id);
    let home = folder.0.join("home");
    let room = home.join("accounts/room");
    std::fs::create_dir_all(&room).unwrap();
    let now = obi_protocol::time::now();
    std::fs::write(
        room.join(".credentials.json"),
        json!({"claudeAiOauth":{"accessToken":"fake-room","expiresAt":(now+86400)*1000}})
            .to_string(),
    )
    .unwrap();
    // Restart with the second account and its room, so ACC-6 can pick it.
    drop(obid);
    let mut config = config;
    config["agents"]["claude"]["accounts"] = json!([
        {"id":"stub","name":"Stub","configDir": home.join("accounts/stub")},
        {"id":"room","name":"Room","configDir": room},
    ]);
    std::fs::write(
        home.join("usage.json"),
        json!({
            "stub":{"fiveHour":0.99,"sevenDay":0.9,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now,"sevenDayObservedAt":now},
            "room":{"fiveHour":0.1,"sevenDay":0.2,"fiveHourResets":now+3600,"sevenDayResets":now+86400,"fiveHourObservedAt":now,"sevenDayObservedAt":now}
        })
        .to_string(),
    )
    .unwrap();
    let obid = start_with(&folder.0, &config, None, &[("OBID_USAGE_READS", "on")]);
    let moved = within(90, "the waiting job moved to room", || {
        let j = job_now(&obid, &id);
        (j["account"] == "room").then_some(j)
    });
    assert!(
        moved["resumesAt"].is_null()
            || moved["status"] != "queued"
            || moved["waiting"].is_null()
            || moved["moves"].as_array().unwrap().len() == 1,
        "{moved}"
    );
    assert_eq!(moved["moves"].as_array().unwrap().len(), 1, "{moved}");
}

/// LIMITS-19 (ACC-6): a review still asks; it is not auto-moved.
#[test]
fn limits_19_reviews_own_sandbox_and_api_key_accounts_still_ask() {
    let folder = Folder::new("limits-19");
    let config = limit_config(&folder.0, "fake-claude", true);
    blocked_job(
        &folder.0.join("home/jobs"),
        "ffff1901",
        "review",
        Some(obi_protocol::time::now() + 3600),
    );
    let obid = start(&folder.0, &config, None);
    let asks = asked(&obid, "ffff1901");
    assert_eq!(asks["status"], "interrupted", "{asks}");
    assert_eq!(asks["accountLimit"]["state"], "pending", "{asks}");
    assert_eq!(asks["account"], "stub", "{asks}");
    assert!(asks["moves"].as_array().unwrap().is_empty(), "{asks}");
}
