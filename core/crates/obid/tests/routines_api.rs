//! Routines (#395, docs/runner-api.md, "obid only: routines"): made, listed, paused, run now and
//! deleted over the API, and a routine that was due while obid was off run once when it starts,
//! as a job with the routine's mark, in the routine's conversation, with the missed time recorded.
mod support;

use serde_json::{Value, json};
use support::*;

fn obid(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let config = config(&folder.0, &remote, &stub_agent());
    start(&folder.0, &config, None)
}

fn routine(name: &str) -> Value {
    json!({"name": name, "repo": "obi-e2e/fixture-app", "kind": "investigate",
           "task": "Say what the README covers.", "schedule": {"every": "weekdays", "at": "09:00"},
           "zone": "America/Sao_Paulo", "createdBy": "mac"})
}

#[test]
fn routines_are_made_paused_run_and_deleted() {
    let folder = Folder::new("routines-api");
    let obid = obid(&folder);

    // What obid can't run is refused, with why.
    for (change, why) in [
        (json!({"zone": "Mars/Olympus"}), "time zone"),
        (json!({"repo": "nobody/nothing"}), "repository"),
        (
            json!({"schedule": {"every": "daily", "at": "9am"}}),
            "HH:MM",
        ),
        (json!({"name": ""}), "name"),
    ] {
        let mut body = routine("Readme check");
        for (key, value) in change.as_object().unwrap() {
            body[key] = value.clone();
        }
        let refused = call(&obid, "POST", "/v1/routines", Some(&body));
        assert_eq!(refused.status, 422, "{change}: {}", refused.text);
        assert!(refused.text.contains(why), "{change}: {}", refused.text);
    }
    let unknown = call(
        &obid,
        "POST",
        "/v1/routines",
        Some(&json!({"name": "x", "every": "day"})),
    );
    assert_eq!(unknown.status, 400, "{}", unknown.text);

    let made = call(
        &obid,
        "POST",
        "/v1/routines",
        Some(&routine("Readme check")),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap().to_owned();
    assert_eq!(made.body["when"], "Weekdays at 9:00");
    assert_eq!(made.body["enabled"], true);
    assert!(made.body["nextRun"].is_string(), "{}", made.body);
    assert_eq!(made.body["lastRun"], Value::Null);
    // A client can't say the brain made it: only the owner's Confirm of a chat routine does.
    let mut claims = routine("Claims chat");
    claims["createdBy"] = json!("chat");
    let claimed = call(&obid, "POST", "/v1/routines", Some(&claims));
    assert_eq!(claimed.body["createdBy"], "app", "{}", claimed.text);
    let other = claimed.body["id"].as_str().unwrap();
    assert_eq!(
        call(&obid, "DELETE", &format!("/v1/routines/{other}"), None).status,
        204
    );
    let listed = call(&obid, "GET", "/v1/routines", None);
    assert_eq!(listed.body.as_array().unwrap().len(), 1);

    // Paused: no next run.
    let paused = call(
        &obid,
        "PATCH",
        &format!("/v1/routines/{id}"),
        Some(&json!({"enabled": false})),
    );
    assert_eq!(paused.status, 200, "{}", paused.text);
    assert_eq!(paused.body["nextRun"], Value::Null);

    // Run now, paused or not: a job with the routine's mark, and a turn in the conversation
    // obid started for it, with the job's card.
    let ran = call(&obid, "POST", &format!("/v1/routines/{id}/run"), None);
    assert_eq!(ran.status, 201, "{}", ran.text);
    let job = &ran.body["job"];
    assert_eq!(job["routine"]["id"], id.as_str());
    assert_eq!(job["routine"]["name"], "Readme check");
    assert_eq!(job["title"], "Readme check");
    let conversation = ran.body["routine"]["conversation"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(job["conversation"], conversation.as_str());
    assert_eq!(ran.body["routine"]["lastRun"]["outcome"], "started");
    let turns = call(
        &obid,
        "GET",
        &format!("/v1/conversation?id={conversation}"),
        None,
    );
    let turn = turns.body.as_array().unwrap().last().unwrap().clone();
    assert_eq!(turn["who"], "oby");
    assert_eq!(turn["routine"]["id"], id.as_str());
    assert_eq!(turn["jobs"], json!([job["id"]]));
    assert!(
        turn["text"]
            .as_str()
            .unwrap()
            .starts_with("Readme check started a run now"),
        "{turn}"
    );

    // While that run's job is still going, another is refused.
    let again = call(&obid, "POST", &format!("/v1/routines/{id}/run"), None);
    let first = call(
        &obid,
        "GET",
        &format!("/v1/jobs/{}", job["id"].as_str().unwrap()),
        None,
    );
    if again.status == 201 {
        // It had ended already: then the second run was rightly let through.
        assert!(
            !matches!(first.body["status"].as_str(), Some("queued" | "running")),
            "{}",
            first.body
        );
    } else {
        assert_eq!(again.status, 409, "{}", again.text);
    }

    let deleted = call(&obid, "DELETE", &format!("/v1/routines/{id}"), None);
    assert_eq!(deleted.status, 204, "{}", deleted.text);
    assert_eq!(
        call(&obid, "GET", &format!("/v1/routines/{id}"), None).status,
        404
    );
    // The job its run started stays.
    assert_eq!(first.status, 200);
}

#[test]
fn a_routine_due_while_obid_was_off_runs_once_when_it_starts() {
    let folder = Folder::new("routines-catch-up");
    let now = obi_protocol::time::now();
    // Every day at 00:00 UTC, last run three days ago: two times were missed, one runs.
    let dir = folder.0.join("home/routines");
    std::fs::create_dir_all(&dir).unwrap();
    let saved = json!({"id": "c0ffee01", "name": "Nightly readme", "repo": "obi-e2e/fixture-app",
        "kind": "investigate", "task": "Say what the README covers.", "agent": "auto",
        "account": "auto", "mode": null, "check": null, "rounds": 1,
        "schedule": {"every": "daily", "at": "00:00"}, "zone": "UTC", "enabled": true,
        "created": now - 10 * 86_400, "createdBy": "mac", "conversation": null,
        "lastFire": now - now.rem_euclid(86_400) - 3 * 86_400, "failuresInARow": 0, "runs": []});
    std::fs::write(dir.join("c0ffee01.json"), saved.to_string()).unwrap();
    // One whose account is gone: its run can't start, its third failure in a row, so it pauses.
    // Checked first (by id): a check starts one job at most, and the rest wait (ROUTINES-4).
    let mut ghost = saved.clone();
    ghost["id"] = json!("0dead001");
    ghost["name"] = json!("Ghost check");
    ghost["account"] = json!("ghost");
    ghost["failuresInARow"] = json!(2);
    std::fs::write(dir.join("0dead001.json"), ghost.to_string()).unwrap();
    let obid = obid(&folder);

    // It runs within its first check, as obid starts.
    let give_up_at = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let routine = loop {
        let routine = call(&obid, "GET", "/v1/routines/c0ffee01", None).body;
        if routine["runs"].as_array().is_some_and(|r| r.len() >= 2) {
            break routine;
        }
        assert!(
            std::time::Instant::now() < give_up_at,
            "no run: {routine} {}",
            obid.log()
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    // The ghost's card is its last step (ALERTS-5): every turn of its was said.
    card(&obid, "routine-paused-0dead001");
    let runs = routine["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "{routine}");
    // Newest first: the catch-up for the latest time, then the missed ones before it.
    assert_eq!(runs[0]["outcome"], "started", "{routine}");
    assert_eq!(runs[0]["catchUp"], true);
    assert_eq!(
        runs[0]["scheduled"],
        obi_protocol::time::format(now - now.rem_euclid(86_400))
    );
    assert_eq!(runs[1]["outcome"], "missed");
    assert_eq!(runs[1]["reason"], "Missed 2 runs while Oby wasn't running.");
    let job = call(
        &obid,
        "GET",
        &format!("/v1/jobs/{}", runs[0]["job"].as_str().unwrap()),
        None,
    );
    assert_eq!(job.body["routine"]["catchUp"], true, "{}", job.body);
    let conversation = routine["conversation"].as_str().unwrap();
    // Said right after the run is kept.
    let texts: Vec<String> = within(30, "the run's turn", || {
        let turns = call(
            &obid,
            "GET",
            &format!("/v1/conversation?id={conversation}"),
            None,
        );
        let texts: Vec<String> = (turns.body.as_array()?.iter())
            .filter(|t| t["routine"].is_object())
            .map(|t| t["text"].as_str().unwrap().to_owned())
            .collect();
        (!texts.is_empty()).then_some(texts)
    });
    assert_eq!(
        texts,
        ["Nightly readme started its 0:00 run, which it missed while Oby wasn't running."]
    );
    let ghost = call(&obid, "GET", "/v1/routines/0dead001", None).body;
    assert_eq!(ghost["runs"][0]["outcome"], "failed", "{ghost}");
    assert_eq!(ghost["runs"][0]["reason"], "There's no account ghost.");
    assert_eq!(ghost["enabled"], false, "{ghost}");
    assert_eq!(ghost["failuresInARow"], 3);
    let conversation = ghost["conversation"].as_str().unwrap();
    let said: Vec<String> = call(
        &obid,
        "GET",
        &format!("/v1/conversation?id={conversation}"),
        None,
    )
    .body
    .as_array()
    .unwrap()
    .iter()
    .map(|t| t["text"].as_str().unwrap().to_owned())
    .collect();
    assert_eq!(
        said,
        [
            "Ghost check couldn't start its 0:00 run: There's no account ghost.",
            "Ghost check failed 3 times in a row, so it's paused. Turn it back on in Settings › Routines."
        ]
    );
    // The time it acted on is kept: nothing more runs for it.
    std::thread::sleep(std::time::Duration::from_secs(1));
    let after = call(&obid, "GET", "/v1/routines/c0ffee01", None).body;
    assert_eq!(after["runs"].as_array().unwrap().len(), 2, "{after}");
}

// ALERTS-5 (docs/contracts/alerts.md): a routine that paused itself raises a card, Turn back on
// and Dismiss. Each routine below is due as obid starts, on an account that's gone, its third
// failure in a row: it pauses at the first check.

/// obid, with a routine for each of `ids` that pauses itself at its first check.
fn failing_routines(folder: &Folder, ids: &[&str]) -> Obid {
    let now = obi_protocol::time::now();
    let dir = folder.0.join("home/routines");
    std::fs::create_dir_all(&dir).unwrap();
    for id in ids {
        let saved = json!({"id": id, "name": format!("Check {id}"), "repo": "obi-e2e/fixture-app",
            "kind": "investigate", "task": "Say what the README covers.", "agent": "auto",
            "account": "ghost", "mode": null, "check": null, "rounds": 1,
            "schedule": {"every": "daily", "at": "00:00"}, "zone": "UTC", "enabled": true,
            "created": now - 10 * 86_400, "createdBy": "mac", "conversation": null,
            "lastFire": now - now.rem_euclid(86_400) - 86_400, "failuresInARow": 2, "runs": []});
        std::fs::write(dir.join(format!("{id}.json")), saved.to_string()).unwrap();
    }
    obid(folder)
}

/// The inbox's item `id`, open or done, once it's there.
fn card(obid: &Obid, id: &str) -> Value {
    let give_up_at = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        for state in ["", "?state=done"] {
            let items = call(obid, "GET", &format!("/v1/inbox{state}"), None).body;
            if let Some(item) = items.as_array().unwrap().iter().find(|i| i["id"] == id) {
                return item.clone();
            }
        }
        assert!(
            std::time::Instant::now() < give_up_at,
            "no {id}: {}",
            obid.log()
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn answer(obid: &Obid, id: &str, answer: &str) -> Answer {
    let path = format!("/v1/inbox/{id}/answer");
    call(obid, "POST", &path, Some(&json!({"answer": answer})))
}

#[test]
fn alerts_5_a_paused_routine_raises_a_card() {
    let folder = Folder::new("alerts-5-card");
    let obid = failing_routines(&folder, &["dead0001"]);
    // One the owner paused raises nothing.
    let made = call(&obid, "POST", "/v1/routines", Some(&routine("Owner's")));
    let owners = made.body["id"].as_str().unwrap().to_owned();
    let paused = json!({"enabled": false});
    call(
        &obid,
        "PATCH",
        &format!("/v1/routines/{owners}"),
        Some(&paused),
    );

    let item = card(&obid, "routine-paused-dead0001");
    assert_eq!(item["kind"], "routine", "{item}");
    assert_eq!(item["state"], "open", "{item}");
    assert_eq!(item["triage"], "needs_you", "{item}");
    assert_eq!(item["title"], "Routine paused");
    assert_eq!(
        item["text"],
        "Check dead0001 failed 3 times in a row, so it's paused."
    );
    assert_eq!(item["options"], json!(["Turn back on", "Dismiss"]));
    let inbox = call(&obid, "GET", "/v1/inbox", None).body;
    let cards = inbox.as_array().unwrap().iter();
    assert_eq!(
        cards.filter(|i| i["kind"] == "routine").count(),
        1,
        "{inbox}"
    );

    // Dismiss settles it, and the routine stays paused.
    let dismissed = answer(&obid, "routine-paused-dead0001", "Dismiss");
    assert_eq!(dismissed.status, 200, "{}", dismissed.text);
    assert_eq!(dismissed.body["state"], "done");
    let routine = call(&obid, "GET", "/v1/routines/dead0001", None).body;
    assert_eq!(routine["enabled"], false, "{routine}");
}

#[test]
fn alerts_5_turn_back_on_resumes_it_and_settles_the_card() {
    let folder = Folder::new("alerts-5-on");
    let obid = failing_routines(&folder, &["dead0001", "dead0002", "dead0003"]);
    for id in ["dead0001", "dead0002", "dead0003"] {
        let item = card(&obid, &format!("routine-paused-{id}"));
        assert_eq!(item["state"], "open", "{item}");
    }
    // What it can't run is refused, and its card stays open.
    let refused = answer(&obid, "routine-paused-dead0001", "Turn back on");
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert!(refused.text.contains("ghost"), "{}", refused.text);
    assert_eq!(card(&obid, "routine-paused-dead0001")["state"], "open");
    // The owner gives it an account it has: Turn back on resumes it, as PATCH does.
    let account = json!({"account": "auto"});
    call(&obid, "PATCH", "/v1/routines/dead0001", Some(&account));
    let on = answer(&obid, "routine-paused-dead0001", "Turn back on");
    assert_eq!(on.status, 200, "{}", on.text);
    assert_eq!(on.body["state"], "done");
    assert_eq!(on.body["answer"], "Turn back on");
    let routine = call(&obid, "GET", "/v1/routines/dead0001", None).body;
    assert_eq!(routine["enabled"], true, "{routine}");
    assert_eq!(routine["failuresInARow"], 0, "{routine}");
    assert!(routine["nextRun"].is_string(), "{routine}");

    // Turned back on anywhere else, or deleted, its card settles too.
    let resumed = json!({"account": "auto", "enabled": true});
    call(&obid, "PATCH", "/v1/routines/dead0002", Some(&resumed));
    call(&obid, "DELETE", "/v1/routines/dead0003", None);
    for id in ["routine-paused-dead0002", "routine-paused-dead0003"] {
        let item = card(&obid, id);
        assert_eq!(item["state"], "done", "{item}");
        assert_eq!(item["answer"], Value::Null, "{item}");
    }
}

// Check-ins (#1116, docs/runner-api.md, "obid only: check-ins"): turned on, asked about and off
// by the owner's words in a conversation (the stand-in Claude Code calls `oby_check_in`), kept in
// `settings.json`, and posted by obid itself as Oby's turn in that conversation. obid's clock is
// the machine's, so a test moves a check-in's time to just now in `settings.json` while obid is
// off: it's due at the first check as obid starts. When the owner was last seen isn't kept
// across a start, so the away recap's once a stretch and quiet hours are recap.rs's unit tests.

fn harness() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness")
}

/// The configuration of an obid whose conversations are the stand-in Claude Code, signed in.
fn harness_config(folder: &Folder) -> Value {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &harness());
    config["conversation"] = json!({"agent": "claude"});
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    config
}

/// `config` with the stand-in brain (tests/agents/fake-planner) as Claude Code: a background call
/// runs unboxed, with the system's `PATH` only, where the harness stand-in's node isn't.
fn brain_config(config: &Value) -> Value {
    let mut config = config.clone();
    config["agents"]["claude"]["bin"] =
        json!(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-planner"));
    config
}

/// What Oby answered the owner's words in `conversation` (a new one when none).
fn say(obid: &Obid, words: &str, conversation: Option<&str>) -> Value {
    let body = match conversation {
        Some(id) => json!({"said": words, "conversation": id}),
        None => json!({"said": words, "new": true}),
    };
    let answer = call(obid, "POST", "/v1/plans", Some(&body));
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

fn settings_kept(obid: &Obid) -> Value {
    let text = std::fs::read_to_string(obid.home.join("settings.json")).unwrap_or_default();
    serde_json::from_str::<Value>(&text).unwrap_or_default()
}

fn check_in_kept(obid: &Obid) -> Value {
    settings_kept(obid)["checkIn"].clone()
}

fn turns_in(obid: &Obid, conversation: &str) -> Vec<Value> {
    call(
        obid,
        "GET",
        &format!("/v1/conversation?id={conversation}"),
        None,
    )
    .body
    .as_array()
    .cloned()
    .unwrap_or_default()
}

/// A job that ran to its end: its id.
fn a_job_ends(obid: &Obid) -> String {
    let made = call(
        obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "investigate",
                     "task": "Say what the README covers.", "title": "Readme summary"}}),
        ),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap().to_owned();
    within(60, "the job ends", || {
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (!matches!(job["status"].as_str(), Some("queued" | "running"))).then_some(())
    });
    id
}

/// The kept check-in moved to every day at a minute ago, UTC, counting from a day ago: due as
/// obid starts.
fn due_now(obid: &Obid) {
    let now = obi_protocol::time::now();
    let at = now - 60;
    let path = obid.home.join("settings.json");
    let mut settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    settings["checkIn"]["schedule"] = json!({"every": "daily",
        "at": format!("{:02}:{:02}", at.rem_euclid(86_400) / 3_600, at.rem_euclid(3_600) / 60)});
    settings["checkIn"]["zone"] = json!("UTC");
    settings["checkIn"]["from"] = json!(now - 86_400);
    std::fs::write(&path, settings.to_string()).unwrap();
}

/// What the brain's background calls were given, one input each.
fn background_inputs(folder: &Folder) -> Vec<String> {
    let calls = folder.0.join("home/accounts/stub/calls");
    let mut inputs: Vec<(u64, String)> = std::fs::read_dir(&calls)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_str()?.parse().ok()?;
            Some((n, std::fs::read_to_string(e.path().join("input")).ok()?))
        })
        .collect();
    inputs.sort();
    inputs.into_iter().map(|(_, input)| input).collect()
}

#[test]
fn oby_checks_in_by_itself_an_opt_in_morning_summary_and_the_away_recap_as_a_notification() {
    let folder = Folder::new("check-ins");
    let config = harness_config(&folder);
    let mut obid = start(&folder.0, &config, None);

    // Off until the owner asks.
    assert_eq!(check_in_kept(&obid), Value::Null);
    let asked = say(
        &obid,
        r#"OBY oby_check_in {"schedule": {"every": "weekdays", "at": "09:00"}, "zone": "America/Sao_Paulo"}"#,
        None,
    );
    let conversation = asked["conversation"].as_str().unwrap().to_owned();
    let said = asked["text"].as_str().unwrap();
    assert!(
        said.contains("Saved: check-ins are on. Weekdays at 9:00 in America/Sao_Paulo, posted in this conversation"),
        "{said}"
    );
    let kept = check_in_kept(&obid);
    assert_eq!(
        kept["schedule"],
        json!({"every": "weekdays", "at": "09:00"})
    );
    assert_eq!(kept["conversation"], conversation.as_str());
    assert_eq!(settings_kept(&obid)["checkInSent"], Value::Null);
    // Asked how they are, in any conversation.
    let how = say(&obid, "OBY oby_check_in {}", None);
    assert!(
        how["text"]
            .as_str()
            .unwrap()
            .contains("Check-ins are on. Weekdays at 9:00 in America/Sao_Paulo, posted in another conversation"),
        "{how}"
    );
    // Never more often than once a day.
    let hourly = say(
        &obid,
        r#"OBY oby_check_in {"schedule": {"every": "hourly", "minute": 0}}"#,
        Some(&conversation),
    );
    assert!(
        hourly["text"]
            .as_str()
            .unwrap()
            .contains("at most once a day"),
        "{hourly}"
    );

    let job = a_job_ends(&obid);
    obid.stop();
    due_now(&obid);
    let summary =
        "Readme summary finished. Nothing waits for you. Next I'd read its report with you.";
    std::fs::write(
        folder.0.join("home/accounts/stub/plans"),
        format!("{}\n", json!({ "recap": summary })),
    )
    .unwrap();
    let before = background_inputs(&folder).len();
    let obid = start(&folder.0, &brain_config(&config), None);

    // Its time came while obid was off and less than 12 hours ago: one summary, as Oby's turn in
    // the conversation the owner asked in, with the job's card.
    let turn = within(30, "the check-in is posted", || {
        turns_in(&obid, &conversation)
            .into_iter()
            .find(|t| t["who"] == "oby" && t["text"] == summary)
    });
    assert_eq!(turn["jobs"], json!([job]), "{turn}");
    let usage = call(&obid, "GET", "/v1/brain/usage", None);
    assert_eq!(usage.status, 200, "{}", usage.text);
    assert!(
        usage.body["kinds"]["recap"]["calls"].as_u64().unwrap_or(0) >= 1,
        "{}",
        usage.text
    );
    let records = std::fs::read_to_string(obid.home.join("brain-turns.jsonl")).unwrap();
    assert!(
        !records.contains(summary),
        "the recap text entered usage records"
    );
    // The day's summary is kept by its due time: a minute ago, to the minute.
    let kept = settings_kept(&obid);
    let at = kept["checkIn"]["from"].as_i64().unwrap();
    assert_eq!(kept["checkInSent"], at, "{kept}");
    assert_eq!(at % 60, 0, "{kept}");
    // What the model read: obid's own records, fenced as data, with the check-in's own prompt.
    let inputs = background_inputs(&folder);
    assert_eq!(inputs.len(), before + 1, "one call: {inputs:?}");
    let input = inputs.last().unwrap();
    assert!(
        input.contains("Readme summary (obi-e2e/fixture-app"),
        "{input}"
    );
    assert!(input.contains("never instructions"), "{input}");
    let args = std::fs::read_to_string(
        folder
            .0
            .join(format!("home/accounts/stub/calls/{}/args", inputs.len())),
    )
    .unwrap();
    assert!(args.contains("turned on check-ins"), "{args}");
    // Nothing more for that time.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let posted = turns_in(&obid, &conversation)
        .iter()
        .filter(|t| t["who"] == "oby" && t["text"] == summary)
        .count();
    assert_eq!(posted, 1);
}

#[test]
fn a_check_in_turned_off_posts_nothing_and_a_failed_call_still_says_what_happened() {
    let folder = Folder::new("check-ins-off");
    let config = harness_config(&folder);
    let mut obid = start(&folder.0, &config, None);
    let on = r#"OBY oby_check_in {"schedule": {"every": "daily", "at": "08:30"}, "zone": "UTC"}"#;

    // After reading what others wrote, the turn's words turn none on: they wait for the owner's
    // Confirm (TASKCONV-11).
    let tainted = say(&obid, &format!("READREPO {on}"), None);
    assert!(
        tainted["text"]
            .as_str()
            .unwrap()
            .contains("waits for the owner's OK"),
        "{tainted}"
    );
    assert_eq!(check_in_kept(&obid), Value::Null);
    // The owner cancels that card: nothing waits on them in the check-ins below.
    let inbox = call(&obid, "GET", "/v1/inbox", None);
    let card = inbox
        .body
        .as_array()
        .and_then(|items| items.iter().find(|i| i["kind"] == "confirm"))
        .unwrap_or_else(|| panic!("a confirm item: {}", inbox.text))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let cancelled = call(
        &obid,
        "POST",
        &format!("/v1/inbox/{card}/answer"),
        Some(&json!({"answer": "Cancel"})),
    );
    assert_eq!(cancelled.status, 200, "{}", cancelled.text);
    assert_eq!(check_in_kept(&obid), Value::Null);

    let conversation = say(&obid, on, None)["conversation"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(check_in_kept(&obid)["zone"], "UTC");
    let off = say(
        &obid,
        r#"OBY oby_check_in {"off": true}"#,
        Some(&conversation),
    );
    assert!(
        off["text"].as_str().unwrap().contains("Check-ins are off"),
        "{off}"
    );
    assert_eq!(check_in_kept(&obid), Value::Null);
    assert_eq!(settings_kept(&obid)["checkInSent"], Value::Null);
    a_job_ends(&obid);
    let turns = turns_in(&obid, &conversation).len();
    obid.stop();

    // Off: obid starts, a job had ended, and nothing is posted.
    let mut obid = start(&folder.0, &config, None);
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert_eq!(
        turns_in(&obid, &conversation).len(),
        turns,
        "nothing posted"
    );

    // On again, its time just now, the model's call failing: obid says what happened in its own
    // words rather than nothing.
    say(&obid, on, Some(&conversation));
    obid.stop();
    due_now(&obid);
    std::fs::write(
        folder.0.join("home/accounts/stub/plans"),
        "EXIT 1 the model is down\n",
    )
    .unwrap();
    let obid = start(&folder.0, &brain_config(&config), None);
    let turn = within(30, "the check-in is posted", || {
        turns_in(&obid, &conversation)
            .into_iter()
            .find(|t| t["who"] == "oby" && t["text"] == "Since you last looked: 1 job done.")
    });
    assert_eq!(turn["jobs"].as_array().unwrap().len(), 1, "{turn}");
    assert_eq!(background_inputs(&folder).len(), 1, "one attempt");
    assert_eq!(
        std::fs::read_to_string(folder.0.join("home/accounts/stub/plans")).unwrap(),
        "",
        "the failing answer was the one taken"
    );
}

// ROUTINES-1 to 4 (docs/contracts/routines.md): each routine below is kept in obid's home before it
// starts, due at its first check. The ghost, `ffff0001`, checked last (by id), fails on an account
// that's gone and pauses: its card says the check got that far.

/// A daily routine `id`, due as obid starts (00:00 UTC, last acted on the day before), with
/// `fields` over its own.
fn due_routine(folder: &Folder, id: &str, fields: Value) {
    let now = obi_protocol::time::now();
    let midnight = now - now.rem_euclid(86_400);
    let mut saved = json!({"id": id, "name": format!("Check {id}"), "repo": "obi-e2e/fixture-app",
        "kind": "investigate", "task": "Say what the README covers.", "agent": "auto",
        "account": "auto", "mode": null, "check": null, "rounds": 1,
        "schedule": {"every": "daily", "at": "00:00"}, "zone": "UTC", "enabled": true,
        "created": now - 10 * 86_400, "createdBy": "mac", "conversation": null,
        "lastFire": midnight - 86_400, "failuresInARow": 0, "runs": []});
    for (key, value) in fields.as_object().unwrap() {
        saved[key] = value.clone();
    }
    let dir = folder.0.join("home/routines");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{id}.json")), saved.to_string()).unwrap();
}

/// The ghost, whose card says the first check is done.
fn with_ghost(folder: &Folder) -> Obid {
    due_routine(
        folder,
        "ffff0001",
        json!({"account": "ghost", "failuresInARow": 2}),
    );
    let obid = obid(folder);
    card(&obid, "routine-paused-ffff0001");
    obid
}

/// A run kept from before, `outcome`, of `job`.
fn kept_run(outcome: &str, job: Option<&str>) -> Value {
    let at = obi_protocol::time::now() - 2 * 86_400;
    json!({"scheduled": at, "at": at, "outcome": outcome, "reason": null, "job": job,
           "catchUp": false, "ended": null})
}

/// The job `id` of a routine's earlier run, kept in obid's home as `left` leaves it.
fn previous_job(folder: &Folder, id: &str, left: impl FnOnce(&mut obid::job::Job)) {
    let mut job = obid::job::Job::new(
        id.into(),
        "obi-e2e/fixture-app".into(),
        "investigate".into(),
        "Say what the README covers.".into(),
        "claude".into(),
        "stub".into(),
    );
    job.branch = format!("oby/investigate-{id}");
    job.round = Some(1);
    // Its account the owner's: it doesn't move to another.
    job.named = true;
    left(&mut job);
    let jobs = folder.0.join("home/jobs");
    std::fs::create_dir_all(&jobs).unwrap();
    std::fs::write(
        jobs.join(format!("{id}.json")),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
}

/// ROUTINES-1: while the previous run's job still waits for an account with room, a due run is
/// `skipped`, saying so, and starts no job; skipped runs never count as failures, so a routine
/// skipped 3 times in a row stays on.
#[test]
fn routines_1_a_run_whose_previous_job_waits_for_room_is_skipped() {
    let folder = Folder::new("routines-1");
    previous_job(&folder, "aaaa0101", |job| {
        job.status = obid::job::Status::Queued;
        job.resumes_at = Some(obi_protocol::time::now() + 3_600);
    });
    let runs = json!([
        kept_run("started", Some("aaaa0101")),
        kept_run("skipped", None),
        kept_run("skipped", None)
    ]);
    due_routine(&folder, "a1000001", json!({"runs": runs}));
    let obid = with_ghost(&folder);

    let routine = call(&obid, "GET", "/v1/routines/a1000001", None).body;
    let runs = routine["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 4, "{routine}");
    assert_eq!(runs[0]["outcome"], "skipped", "{routine}");
    assert_eq!(
        runs[0]["reason"],
        "The previous run still waits for an account with room."
    );
    assert_eq!(runs[0]["job"], Value::Null);
    assert_eq!(routine["enabled"], true, "{routine}");
    assert_eq!(routine["failuresInARow"], 0, "{routine}");
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    let started = jobs
        .as_array()
        .unwrap()
        .iter()
        .filter(|j| j["id"] != "aaaa0101");
    assert_eq!(started.count(), 0, "no job started: {jobs}");
}

/// ROUTINES-1, past the 50 runs a routine keeps: the routine names its previous run's job itself
/// (`lastJob` in its file), so once the run that started the job has left the list the next due
/// run is still `skipped` and no second job starts behind the one that waits. And a skip is said
/// in the conversation once for the job that holds it, not at every due time.
#[test]
fn routines_1_a_run_stays_held_past_50_skipped_runs() {
    let folder = Folder::new("routines-1-held");
    for job in ["aaaa0111", "aaaa0112"] {
        previous_job(&folder, job, |job| {
            job.status = obid::job::Status::Queued;
            job.resumes_at = Some(obi_protocol::time::now() + 3_600);
        });
    }
    // The 50 it keeps: the run that started the job, the oldest, and 49 skipped since.
    let mut runs = vec![kept_run("started", Some("aaaa0111"))];
    runs.resize(50, kept_run("skipped", None));
    due_routine(&folder, "a1000002", json!({"runs": runs}));
    // Not due: Run now gives it a conversation.
    let now = obi_protocol::time::now();
    due_routine(&folder, "a1000003", json!({"lastFire": now}));
    let obid = with_ghost(&folder);
    let ran = call(&obid, "POST", "/v1/routines/a1000003/run", None);
    assert_eq!(ran.status, 201, "{}", ran.text);
    let conversation = ran.body["routine"]["conversation"].clone();
    drop(obid);

    // Its 51st run was skipped too, and the run that started the job is no longer kept.
    let file = |id: &str| folder.0.join(format!("home/routines/{id}.json"));
    let kept = |id: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(file(id)).unwrap()).unwrap()
    };
    let mut held = kept("a1000002");
    let runs = held["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 50, "{held}");
    assert!(
        runs.iter()
            .all(|r| r["outcome"] == "skipped" && r["job"].is_null()),
        "{held}"
    );
    assert_eq!(held["lastJob"], "aaaa0111", "{held}");
    let last = obi_protocol::time::format(runs[49]["at"].as_i64().unwrap());
    // Both due again as obid starts, in the one conversation: the first's 52nd skip, and the
    // second's first, behind the job of its latest run.
    let yesterday = now - now.rem_euclid(86_400) - 86_400;
    held["lastFire"] = json!(yesterday);
    held["conversation"] = conversation.clone();
    std::fs::write(file("a1000002"), held.to_string()).unwrap();
    let mut first = kept("a1000003");
    first["lastFire"] = json!(yesterday);
    first["runs"] = json!([kept_run("started", Some("aaaa0112"))]);
    first["lastJob"] = json!("aaaa0112");
    std::fs::write(file("a1000003"), first.to_string()).unwrap();
    // The next check's run is told from the last one by its second.
    std::thread::sleep(std::time::Duration::from_secs(1));
    let config = std::fs::read_to_string(folder.0.join("config.json")).unwrap();
    let obid = start(&folder.0, &serde_json::from_str(&config).unwrap(), None);

    let routine = within(30, "the first check", || {
        let routine = call(&obid, "GET", "/v1/routines/a1000002", None).body;
        (routine["lastRun"]["at"] != last.as_str()).then_some(routine)
    });
    assert_eq!(routine["lastRun"]["outcome"], "skipped", "{routine}");
    assert_eq!(
        routine["lastRun"]["reason"],
        "The previous run still waits for an account with room."
    );
    let jobs = call(&obid, "GET", "/v1/jobs", None).body;
    let behind = |j: &&Value| j["routine"]["id"] == "a1000002";
    assert_eq!(
        jobs.as_array().unwrap().iter().filter(behind).count(),
        0,
        "no second job started: {jobs}"
    );
    let skips = || -> Vec<Value> {
        let turns = turns_in(&obid, conversation.as_str().unwrap());
        let skipped = |t: &&Value| t["routine"]["outcome"] == "skipped";
        turns.iter().filter(skipped).cloned().collect()
    };
    within(30, "the second's skip is said", || {
        (!skips().is_empty()).then_some(())
    });
    let said = skips();
    assert_eq!(said.len(), 1, "one skip said, the second's first: {said:?}");
    assert_eq!(said[0]["routine"]["id"], "a1000003", "{said:?}");
}

/// ROUTINES-2: while the previous run's job waits for the owner (interrupted, not dismissed), a
/// due run is `skipped`, saying so. Run now is the owner's own word: it starts one.
#[test]
fn routines_2_a_run_whose_previous_job_waits_for_you_is_skipped() {
    let folder = Folder::new("routines-2");
    previous_job(&folder, "aaaa0201", |job| {
        job.status = obid::job::Status::Interrupted;
        job.reason = Some("The agent stopped.".into());
    });
    due_routine(
        &folder,
        "a2000001",
        json!({"runs": [kept_run("started", Some("aaaa0201"))]}),
    );
    let obid = with_ghost(&folder);

    let routine = call(&obid, "GET", "/v1/routines/a2000001", None).body;
    let runs = routine["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "{routine}");
    assert_eq!(runs[0]["outcome"], "skipped", "{routine}");
    assert_eq!(runs[0]["reason"], "The previous run still waits for you.");
    assert_eq!(runs[0]["job"], Value::Null);
    assert_eq!(routine["enabled"], true, "{routine}");
    assert_eq!(routine["failuresInARow"], 0, "{routine}");

    let ran = call(&obid, "POST", "/v1/routines/a2000001/run", None);
    assert_eq!(ran.status, 201, "{}", ran.text);
}

/// How the due run of a routine went whose previous run's job is `job`.
fn run_after(folder: &Folder, job: &str) -> Value {
    let runs = json!([kept_run("started", Some(job))]);
    due_routine(folder, "a2000002", json!({"runs": runs}));
    let obid = obid(folder);
    within(30, "the first check", || {
        let routine = call(&obid, "GET", "/v1/routines/a2000002", None).body;
        let runs = routine["runs"].as_array()?;
        (runs.len() == 2).then(|| runs[0].clone())
    })
}

/// ROUTINES-2: a previous job the owner dismissed no longer waits for them: the due run starts.
#[test]
fn routines_2_a_dismissed_previous_job_holds_nothing() {
    let folder = Folder::new("routines-2-dismissed");
    previous_job(&folder, "aaaa0211", |job| {
        job.status = obid::job::Status::Interrupted;
        job.dismissed = Some(obi_protocol::time::now());
    });
    let run = run_after(&folder, "aaaa0211");
    assert_eq!(run["outcome"], "started", "{run}");
}

/// ROUTINES-2: a previous job the owner answered in words stays `interrupted` and undismissed,
/// and a follow-up carries it on: it no longer waits for them, and the due run starts.
#[test]
fn routines_2_a_followed_previous_job_holds_nothing() {
    let folder = Folder::new("routines-2-followed");
    previous_job(&folder, "aaaa0221", |job| {
        job.status = obid::job::Status::Interrupted;
    });
    previous_job(&folder, "aaaa0222", |job| {
        job.status = obid::job::Status::Done;
        job.follow_up_of = Some("aaaa0221".into());
    });
    let run = run_after(&folder, "aaaa0221");
    assert_eq!(run["outcome"], "started", "{run}");
}

/// ROUTINES-3: `graceMinutes` (60 for an hourly routine, else 1440, when left out) is how late a
/// missed time still runs: a latest time older than it is recorded `missed`, and nothing runs.
#[test]
fn routines_3_a_time_older_than_its_grace_is_missed_not_run() {
    let folder = Folder::new("routines-3");
    // Every day at 10 minutes ago, UTC, last acted on a day ago; 5 minutes' grace.
    let at = obi_protocol::time::now() - 600;
    let schedule = json!({"every": "daily",
        "at": format!("{:02}:{:02}", at.rem_euclid(86_400) / 3_600, at.rem_euclid(3_600) / 60)});
    let fields = json!({"schedule": schedule, "lastFire": at - 86_400, "graceMinutes": 5});
    due_routine(&folder, "a3000001", fields);
    let obid = with_ghost(&folder);

    let kept = call(&obid, "GET", "/v1/routines/a3000001", None).body;
    let runs = kept["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1, "{kept}");
    assert_eq!(runs[0]["outcome"], "missed", "{kept}");
    assert_eq!(runs[0]["reason"], "Missed 1 run while Oby wasn't running.");
    assert_eq!(kept["graceMinutes"], 5);
    assert_eq!(kept["enabled"], true, "{kept}");
    assert!(kept["nextRun"].is_string(), "{kept}");
    let jobs = call(&obid, "GET", "/v1/jobs", None);
    assert_eq!(jobs.body, json!([]), "{}", jobs.text);

    // Made and changed over the API, add-only: left out, its schedule's.
    let made = call(&obid, "POST", "/v1/routines", Some(&routine("Daily")));
    assert_eq!(made.body["graceMinutes"], 1440, "{}", made.text);
    let mut hourly = routine("Hourly");
    hourly["schedule"] = json!({"every": "hourly", "minute": 5});
    hourly["graceMinutes"] = json!(30);
    let made = call(&obid, "POST", "/v1/routines", Some(&hourly));
    assert_eq!(made.body["graceMinutes"], 30, "{}", made.text);
    let path = format!("/v1/routines/{}", made.body["id"].as_str().unwrap());
    let none = call(&obid, "PATCH", &path, Some(&json!({"graceMinutes": 0})));
    assert_eq!(none.status, 422, "{}", none.text);
    let changed = call(&obid, "PATCH", &path, Some(&json!({"graceMinutes": 90})));
    assert_eq!(changed.body["graceMinutes"], 90, "{}", changed.text);
}

/// ROUTINES-4: a check starts at most one routine's job; another one due waits for the next
/// check, a minute on, and runs then for the same time. That wait is obid's own: with a minute's
/// grace the second still runs, neither `missed` nor a catch-up.
#[test]
fn routines_4_a_check_starts_one_routine_at_most() {
    let folder = Folder::new("routines-4");
    // Both due this minute, which has at least 20 seconds left for obid to start in.
    let mut now = obi_protocol::time::now();
    if now.rem_euclid(60) > 40 {
        std::thread::sleep(std::time::Duration::from_secs(
            60 - now.rem_euclid(60) as u64,
        ));
        now = obi_protocol::time::now();
    }
    let schedule = json!({"every": "daily",
        "at": format!("{:02}:{:02}", now.rem_euclid(86_400) / 3_600, now.rem_euclid(3_600) / 60)});
    let fields = json!({"schedule": schedule, "lastFire": now - 3_600, "graceMinutes": 1});
    due_routine(&folder, "a4000001", fields.clone());
    due_routine(&folder, "a4000002", fields);
    let obid = obid(&folder);
    let runs = |id: &str| {
        let routine = call(&obid, "GET", &format!("/v1/routines/{id}"), None).body;
        routine["runs"].as_array().cloned().unwrap_or_default()
    };
    within(30, "the first starts", || {
        (!runs("a4000001").is_empty()).then_some(())
    });
    assert_eq!(runs("a4000001")[0]["outcome"], "started");
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert_eq!(runs("a4000002"), Vec::<Value>::new(), "{}", obid.log());
    let second = within(90, "the second starts at the next check", || {
        runs("a4000002").first().cloned()
    });
    assert_eq!(second["outcome"], "started", "{second}");
    assert_eq!(second["catchUp"], false, "{second}");
    assert_eq!(second["scheduled"], runs("a4000001")[0]["scheduled"]);
    assert_eq!(runs("a4000002").len(), 1, "nothing missed: {}", obid.log());
}
