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
            &json!({"job":{"repo":"obi-e2e/fixture-app","kind":"research","task":"LIMITED: 3600","agent":"claude","account":"stub"}}),
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
