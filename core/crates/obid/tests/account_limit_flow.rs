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
