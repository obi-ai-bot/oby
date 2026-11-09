//! Long jobs (docs/runner-api.md, `limit` and `stalled`): no clock ends a job that makes progress;
//! one that goes quiet, loops or hits its turn cap stalls, with a handoff note, and carries on in
//! its session when it's tried again. The real obid binary with the stand-in agent
//! (tests/agents/fake-claude), its quiet window cut to 3 seconds (`stallAfter`) so progress past
//! several windows stands for a job running past the 45 minutes rounds used to be cut at.
mod support;

use std::path::Path;

use serde_json::{Value, json};
use support::*;

const TRY_AGAIN: &str =
    "Try again, picking up where you left off. Finish with a short summary as asked.";

fn obid(name: &str) -> (Folder, Obid) {
    obid_with(name, json!({}), &[])
}

/// obid with `more` in its configuration and `jobs` kept in its home before it starts.
fn obid_with(name: &str, more: Value, jobs: &[obid::job::Job]) -> (Folder, Obid) {
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let mut config = config(&folder.0, &remote, &agent);
    config["stallAfter"] = json!(3);
    // A key that starts with `/` is a JSON pointer into it (`/repos/0/check`).
    for (key, value) in more.as_object().unwrap() {
        match key.starts_with('/') {
            true => *config.pointer_mut(key).unwrap() = value.clone(),
            false => config[key] = value.clone(),
        }
    }
    let dir = folder.0.join("home/jobs");
    std::fs::create_dir_all(&dir).unwrap();
    for job in jobs {
        let path = dir.join(format!("{}.json", job.id));
        std::fs::write(path, serde_json::to_vec(job).unwrap()).unwrap();
    }
    let obid = start(&folder.0, &config, None);
    (folder, obid)
}

fn post_job(obid: &Obid, task: &str) -> String {
    post_kind(obid, "investigate", task)
}

fn post_kind(obid: &Obid, kind: &str, task: &str) -> String {
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": kind, "task": task});
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

fn wait(obid: &Obid, id: &str) -> Value {
    let answer = call(obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

/// Streaming words for 4 quiet windows, a job keeps running and ends done; one that only writes
/// files in its worktree (a quiet build) keeps running too, but not past 4 windows without a word
/// (a loop touching a file isn't progress).
#[test]
fn a_job_making_progress_runs_past_its_quiet_window() {
    let (_folder, obid) = obid("long-progress");
    let id = post_job(&obid, "CHATTY: 12000");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["summary"], "Chatted.");
    assert!(job["stalled"].is_null() && job["limit"].is_null(), "{job}");

    let beating = post_job(&obid, "SLEEP");
    within(30, "the beating job runs", || {
        (call(&obid, "GET", &format!("/v1/jobs/{beating}"), None).body["status"] == "running")
            .then_some(())
    });
    std::thread::sleep(std::time::Duration::from_secs(8));
    let job = call(&obid, "GET", &format!("/v1/jobs/{beating}"), None).body;
    assert_eq!(job["status"], "running", "{job}");
    let job = wait(&obid, &beating);
    assert_eq!(job["stalled"]["why"], "quiet", "{job}");
}

/// A silent agent stalls after its quiet window: it writes its handoff note in one more turn of
/// its session, the job needs you (`interrupted`, #725) with `limit` and `stalled` (and a `stalled`
/// event), keeps its branch and the agent's own summary, and the Try-again words carry it on in
/// the same session.
#[test]
fn a_silent_job_stalls_with_a_handoff_note_and_carries_on() {
    let (_folder, obid) = obid("long-quiet");
    let events = follow_events(&obid);
    let id = post_job(&obid, "QUIET");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["limit"], "no progress for 3 seconds");
    let stalled = &job["stalled"];
    assert_eq!(stalled["why"], "quiet", "{job}");
    assert_eq!(stalled["text"], job["limit"]);
    assert_eq!(stalled["noteBy"], "agent");
    assert_eq!(stalled["session"], true);
    assert_eq!(stalled["branch"], job["branch"]);
    assert_eq!(stalled["round"], 1);
    assert!(
        stalled["note"]
            .as_str()
            .unwrap()
            .contains("Left: the tests."),
        "{job}"
    );
    assert_eq!(
        job["reason"], "Stalled: no progress for 3 seconds.",
        "{job}"
    );
    assert_eq!(job["nextStep"], "Try again", "{job}");
    assert!(
        stalled["note"]
            .as_str()
            .unwrap()
            .contains("Failed: npm test"),
        "{job}"
    );
    // Its agent reported nothing before it went quiet: its summary is why it stopped.
    assert_eq!(job["summary"], job["reason"], "{job}");
    assert!(job["published"].is_null(), "{job}");
    let sent = events_named(&events, "stalled", 10, |got| !got.is_empty());
    assert_eq!(sent[0]["id"], id.as_str());
    assert_eq!(sent[0]["stalled"]["why"], "quiet");

    let answer = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert!(answer.body["stalled"].is_null(), "{}", answer.text);
    assert!(answer.body["limit"].is_null(), "{}", answer.text);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    // The fake's one session: the job carried on in it.
    assert_eq!(job["session"], "00000000-0000-4000-8000-000000000001");
    assert!(job["stalled"].is_null(), "{job}");
}

/// An agent that can't write its note (it stays quiet) gets obid's, from what obid saw.
#[test]
fn obid_writes_the_note_when_the_agent_cant() {
    let (_folder, obid) = obid("long-no-note");
    let id = post_job(&obid, "QUIET NO NOTE");
    let job = wait(&obid, &id);
    assert_eq!(job["stalled"]["why"], "quiet", "{job}");
    assert_eq!(job["stalled"]["noteBy"], "obid", "{job}");
    let note = job["stalled"]["note"].as_str().unwrap();
    assert!(
        note.contains("obid did") && note.contains("round 1"),
        "{note}"
    );
}

/// A turn that ends while a background task of Claude Code's runs isn't the job's end (job b121d098:
/// "The build is still going. I'll continue when the monitor reports it's done."): the job waits,
/// past several quiet windows while the task is alive, says what it waits for, and goes on in the
/// turn Claude Code wakes for when the task ends.
#[test]
fn a_job_waits_for_its_background_task() {
    let (_folder, obid) = obid("long-background");
    let id = post_job(&obid, "BACKGROUND: 8000");
    within(30, "the job waits for its build", || {
        (call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body["step"]
            == "Waiting for npm run build")
            .then_some(())
    });
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["summary"], "Built.", "{job}");
    assert!(job["stalled"].is_null(), "{job}");
}

/// Waiting counts as progress only while the task is alive, and for 4 quiet windows at most with
/// no word from the agent, as a change in the worktree does: once the task ended and the agent
/// doesn't go on, or when it never ends (a server, a `--watch`), the job stalls as a quiet one does.
#[test]
fn a_background_task_is_progress_only_for_a_while() {
    let (_folder, obid) = obid("long-background-silent");
    let silent = post_job(&obid, "BACKGROUND: 1000 SILENT");
    let forever = post_job(&obid, "BACKGROUND: 0 FOREVER");
    for id in [silent, forever] {
        let job = wait(&obid, &id);
        assert_eq!(job["status"], "interrupted", "{job}");
        assert_eq!(job["stalled"]["why"], "quiet", "{job}");
    }
}

/// A Claude Code that ends before its background task does (it exits, so nothing can wake it) is
/// told once, in its session, to wait in the foreground, and goes on, the transcript saying it was
/// Oby's words; one that ends that way again needs you, never "nothing committed".
#[test]
fn a_turn_that_ends_before_its_task_goes_on_or_needs_you() {
    let (_folder, obid) = obid("long-waits");
    let id = post_job(&obid, "BACKGROUND: 0 EXIT");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["summary"], "Built in the foreground.", "{job}");
    let transcript = call(&obid, "GET", &format!("/v1/jobs/{id}/transcript"), None);
    assert!(
        transcript
            .text
            .contains("Oby told Claude Code to wait for its work in the foreground"),
        "{}",
        transcript.text
    );

    let again = post_kind(&obid, "build", "BACKGROUND: 0 EXIT ALWAYS");
    let job = wait(&obid, &again);
    assert_eq!(job["status"], "interrupted", "{job}");
    let reason = job["reason"].as_str().unwrap();
    assert!(
        reason.starts_with("Stopped waiting: The build is still going.")
            && reason.contains("nothing wakes it"),
        "{job}"
    );
    assert_eq!(job["nextStep"], "Try again", "{job}");
}

/// The same failing call 5 times in a row stalls the job as looping, with the agent's note.
#[test]
fn a_looping_job_stalls() {
    let (_folder, obid) = obid("long-loop");
    let id = post_job(&obid, "LOOP");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["limit"], "the same failing call 5 times");
    assert_eq!(job["stalled"]["why"], "loop");
    assert_eq!(job["stalled"]["noteBy"], "agent");
}

/// Claude Code's turn cap stalls the job instead of failing it.
#[test]
fn the_turn_cap_stalls_the_job() {
    let (_folder, obid) = obid("long-turns");
    let id = post_job(&obid, "MAXTURNS");
    let job = wait(&obid, &id);
    assert_eq!(job["limit"], "200 turns in one round", "{job}");
    assert_eq!(job["stalled"]["why"], "turns");
}

/// An overloaded API's error ends no job: the round runs again in its session.
#[test]
fn a_transient_error_is_retried() {
    let (_folder, obid) = obid("long-transient");
    let id = post_job(&obid, "TRANSIENT");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["summary"], "Nothing to do.");
}

/// A committing job that stalls keeps its commit on its branch and publishes nothing.
#[test]
fn a_committing_job_that_stalls_publishes_nothing() {
    // A quiet window git's commit fits in on a busy machine (it took over 10 s at load 500).
    let (_folder, obid) = obid_with("long-commit-quiet", json!({"stallAfter": 30}), &[]);
    let id = post_kind(&obid, "build", "COMMIT: a.txt QUIET");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["stalled"]["why"], "quiet", "{job}");
    assert_eq!(job["stalled"]["commits"], 1, "{job}");
    assert!(job["stalled"]["commit"].is_string(), "{job}");
    assert_eq!(job["commits"], 1, "{job}");
    assert!(
        job["published"].is_null() && job["pushed"].is_null(),
        "{job}"
    );
    assert!(job["outgoing"].as_array().unwrap().is_empty(), "{job}");
}

/// RECOVERY-1: an error obid can't place (not a login, a usage limit, or a session too long or
/// gone) gets one try in its session, after a minute (its quiet window here), then the job ends
/// with the tool's words, never stalled.
#[test]
fn recovery_1_an_unknown_error_gets_one_try_after_a_minute() {
    let (_folder, obid) = obid("recovery-1-unknown");
    let events = follow_events(&obid);
    let id = post_job(&obid, "FAIL KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["summary"], "it broke");
    assert_eq!(job["nextStep"], "Try again", "{job}");
    assert!(job["stalled"].is_null(), "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[1].contains("> it broke"), "{told:?}");
    let steps: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .filter(|(name, _)| name == "job")
        .filter_map(|(_, job)| job["step"].as_str().map(str::to_owned))
        .filter(|s| s.starts_with("Trying again"))
        .collect();
    assert!(!steps.is_empty(), "no wait shown");
    assert!(
        steps
            .iter()
            .all(|s| s == "Trying again in 3 seconds (1 of 1)"),
        "{steps:?}"
    );
}

/// A transient error every time: three retries, then the job needs you with it (not stalled).
#[test]
fn retries_run_out() {
    let (_folder, obid) = obid("long-retries-out");
    let events = follow_events(&obid);
    let id = post_job(&obid, "TRANSIENT ALWAYS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert!(job["summary"].as_str().unwrap().contains("529"), "{job}");
    assert!(job["stalled"].is_null(), "{job}");
    let steps: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .filter(|(name, _)| name == "job")
        .filter_map(|(_, job)| job["step"].as_str().map(str::to_owned))
        .filter(|step| step.starts_with("Trying again"))
        .collect();
    for n in 1..=3 {
        assert!(
            steps.iter().any(|s| s.contains(&format!("({n} of 3)"))),
            "{steps:?}"
        );
    }
}

/// A job its agent was at work on when obid went down, kept in obid's home as `id`.
fn stored(id: &str, task: &str) -> obid::job::Job {
    let mut job = obid::job::Job::new(
        id.into(),
        "obi-e2e/fixture-app".into(),
        "investigate".into(),
        task.into(),
        "claude".into(),
        "stub".into(),
    );
    job.branch = format!("oby/investigate-{id}");
    job.status = obid::job::Status::Running;
    job.started = Some(obi_protocol::time::now() - 200);
    job.session = Some("00000000-0000-4000-8000-000000000001".into());
    job
}

/// Every prompt job `id`'s agent got, with KEEP PROMPTS in its task.
fn prompts(job: &Value) -> Vec<String> {
    let log = Path::new(job["worktree"].as_str().unwrap()).join(".obi/prompts.log");
    std::fs::read_to_string(log)
        .unwrap()
        .split("<<<end of prompt>>>\n")
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// RECOVERY-1: a passing error, or a CLI that ends with no result, leaves the job running in its
/// session: the marked message quotes the error as data and says to check what ran.
#[test]
fn recovery_1_a_passing_error_continues_the_same_session() {
    let (_folder, obid) = obid("recovery-1");
    let id = post_job(&obid, "TRANSIENT KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[1].contains("not the owner's words"), "{told:?}");
    assert!(told[1].contains("> API Error: 529"), "{told:?}");
    assert!(told[1].contains("check what really ran"), "{told:?}");

    let id = post_job(&obid, "DIE KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[1].contains("stopped without finishing"), "{told:?}");
}

/// RECOVERY-1: tries count together: 3 in a row, then today's end with the real reason. The
/// owner's own words start them again, never words that aren't theirs.
#[test]
fn recovery_1_three_tries_then_the_real_reason() {
    let (_folder, obid) = obid("recovery-1-out");
    let id = post_job(&obid, "DIE ALWAYS KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["nextStep"], "Try again", "{job}");
    let reason = job["reason"].as_str().unwrap();
    assert!(reason.contains("stopped without finishing"), "{job}");
    assert_eq!(prompts(&job).len(), 4, "{job}");
    // Words that aren't the owner's own take it on, but give it no more tries.
    let answer = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(&json!({"text": TRY_AGAIN})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(prompts(&job).len(), 5, "no retry of its own: {job}");
    // The owner's own press gives it 3 tries again.
    let answer = call_typed(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({"text": TRY_AGAIN}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(prompts(&job).len(), 9, "{job}");
}

/// RECOVERY-1: tries count cuts in a row: a good turn between two cuts gives the 3 back, so a
/// long job isn't stopped by passing errors far apart. Its rounds still bound it.
#[test]
fn recovery_1_a_good_turn_gives_the_tries_back() {
    let (_folder, obid) = obid("recovery-1-flaky");
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "investigate", "rounds": 9,
                     "check": "false", "task": "DIE FLAKY KEEP PROMPTS"});
    let answer = call(&obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    let job = wait(&obid, answer.body["id"].as_str().unwrap());
    let reason = job["reason"].as_str().unwrap_or_default();
    assert!(!reason.contains("stopped without finishing"), "{job}");
    // Each round cut and continued, well past 3 cuts: every continued turn ended well.
    let told = prompts(&job);
    let continued = told
        .iter()
        .filter(|p| p.contains("Your run was cut"))
        .count();
    assert!(continued > 3, "{told:?}");
    assert_eq!(told.len(), 2 * continued, "{told:?}");
}

/// RECOVERY-1: a login isn't tried again, and a usage limit goes to its own decision (#1563),
/// whatever cut the turn.
#[test]
fn recovery_1_a_login_or_a_limit_is_not_retried() {
    let (_folder, obid) = obid("recovery-1-login");
    let id = post_job(&obid, "DIE LOGIN KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert!(
        job["reason"].as_str().unwrap().contains("Not logged in"),
        "{job}"
    );
    assert_eq!(prompts(&job).len(), 1, "{job}");

    let id = post_job(&obid, "LIMITED WEEK: 86400 KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(prompts(&job).len(), 1, "{job}");
    assert!(!job["accountLimit"].is_null(), "{job}");

    // A passing error on an account its usage says is full is its limit's, not a retry's.
    let (_folder, obid) = obid_with("recovery-1-full", json!({}), &[]);
    let reset = obi_protocol::time::now() + 86_400;
    let usage = json!({"status": "allowed_warning", "unifiedWindows": {
        "seven_day": {"utilization": 1.0, "resetsAt": reset}}});
    let id = post_job(&obid, &format!("USAGE: {usage} TRANSIENT KEEP PROMPTS"));
    let job = wait(&obid, &id);
    assert_eq!(prompts(&job).len(), 1, "{job}");
    assert!(!job["accountLimit"].is_null(), "{job}");
}

/// RECOVERY-7: how a turn ended is read from the tool's fields: a 429 whose words name no status
/// obid could read is a passing error, not an account's limit.
#[test]
fn recovery_7_a_429_is_read_from_its_status() {
    let (_folder, obid) = obid("recovery-7-429");
    let id = post_job(&obid, "REJECTED 429 KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert!(job["accountLimit"].is_null(), "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[1].contains("Request rejected (429)"), "{told:?}");
}

/// RECOVERY-7: a 429 past its tries ends as any passing error does, with the tool's words and Try
/// again: its words say "rate limit", and no account is full, so nothing asks about one.
#[test]
fn recovery_7_a_429_past_its_tries_is_no_limit() {
    let (_folder, obid) = obid("recovery-7-429-out");
    let id = post_job(&obid, "REJECTED 429 ALWAYS KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["nextStep"], "Try again", "{job}");
    assert!(job["accountLimit"].is_null(), "{job}");
    assert_eq!(prompts(&job).len(), 4, "{job}");
    assert!(
        job["reason"]
            .as_str()
            .unwrap()
            .starts_with("API Error: Request rejected (429)"),
        "{job}"
    );
}

/// RECOVERY-7: words of the agent's that name a limit, before an error of the tool's own, mark no
/// account full: the tool's error decides.
#[test]
fn recovery_7_the_agents_words_are_no_limit() {
    let (_folder, obid) = obid("recovery-7-words");
    let id = post_job(&obid, "LAST WORDS LIMIT KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert!(job["accountLimit"].is_null(), "{job}");
    let told = prompts(&job);
    assert!(told[1].contains("> API Error: 529"), "{told:?}");
    assert!(!told[1].contains("stranger"), "{told:?}");
}

/// RECOVERY-8: a session too long for its model goes on in a new one, told so and given its whole
/// task, with nobody asked. It isn't told to Try again in that session.
#[test]
fn recovery_8_a_session_too_long_goes_on_in_a_new_one() {
    let (_folder, obid) = obid("recovery-8");
    let id = post_job(&obid, "TOO LONG KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[1].contains("not the owner's words"), "{told:?}");
    assert!(told[1].contains("new one"), "{told:?}");
    assert!(
        told[1].contains("What the user asked for: TOO LONG"),
        "{told:?}"
    );
    let args =
        std::fs::read_to_string(Path::new(job["worktree"].as_str().unwrap()).join(".obi/args.log"))
            .unwrap();
    assert_eq!(args.matches("--max-turns").count(), 2, "{args}");
    assert!(!args.contains("--resume"), "{args}");
    assert_eq!(
        told[1].matches("What the user asked for").count(),
        1,
        "{told:?}"
    );
}

/// RECOVERY-8: a new session hears all a job's first message said, whatever its last message was:
/// here Try again's, with the owner's memories and the repository's rules nowhere else.
#[test]
fn recovery_8_a_new_session_hears_all_it_was_first_told() {
    let (_folder, obid) = obid("recovery-8-told");
    let memories = obid.home.join("memory/user");
    std::fs::create_dir_all(&memories).unwrap();
    std::fs::write(
        memories.join("m0000000a.md"),
        "---\nid: m0000000a\nscope: user\nkind: preference\nsource: found\ntitle: Short answers\nfrom: You said it in 2 conversations\ncreated: 2026-10-22T10:00:00Z\nupdated: 2026-10-22T10:00:00Z\nused: 0\n---\nAnswer in one sentence.\n",
    )
    .unwrap();
    let id = post_job(&obid, "LONG WHEN TOLD KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    let answer = call_typed(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({"text": TRY_AGAIN}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 3, "{told:?}");
    let new = &told[2];
    assert!(new.contains("grew too long"), "{new}");
    assert!(new.contains("What the user asked for"), "{new}");
    assert!(new.contains(".obi/inputs/memories.md"), "{new}");
    assert!(new.contains(TRY_AGAIN), "{new}");
}

/// RECOVERY-8: one new session a job, until the owner's words: too long in that one too, it ends
/// with the tool's words, and the owner's Try again starts a new one again.
#[test]
fn recovery_8_too_long_again_tells_the_owner() {
    let (_folder, obid) = obid("recovery-8-out");
    let id = post_job(&obid, "TOO LONG ALWAYS KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["nextStep"], "Try again", "{job}");
    assert!(
        job["reason"]
            .as_str()
            .unwrap()
            .starts_with("Prompt is too long"),
        "{job}"
    );
    assert_eq!(prompts(&job).len(), 2, "{job}");
    let answer = call_typed(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({"text": TRY_AGAIN}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(prompts(&job).len(), 4, "{job}");
}

/// TASKS-15: every run gets the job's standing rules in its system prompt, a resumed one too, which
/// would otherwise have none; and none of them is in the first message any more.
#[test]
fn tasks_15_every_run_gets_the_standing_rules() {
    let (_folder, obid) = obid("tasks-15");
    let id = post_job(&obid, "TRANSIENT KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let args =
        std::fs::read_to_string(Path::new(job["worktree"].as_str().unwrap()).join(".obi/args.log"))
            .unwrap();
    assert_eq!(args.matches("--max-turns").count(), 2, "{args}");
    assert_eq!(args.matches("--resume").count(), 1, "{args}");
    assert!(
        args.contains("--exclude-dynamic-system-prompt-sections"),
        "{args}"
    );
    let rules = "--append-system-prompt";
    assert_eq!(args.matches(rules).count(), 2, "{args}");
    assert!(args.contains("Finish with a short summary."), "{args}");
    assert!(
        !prompts(&job)[0].contains("Finish with a short summary."),
        "{job}"
    );
}

/// Waits until job `id`'s agent got its first prompt.
fn at_work(obid: &Obid, id: &str) {
    within(30, "its first turn", || {
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        Path::new(job["worktree"].as_str()?)
            .join(".obi/prompts.log")
            .exists()
            .then_some(())
    });
}

/// RECOVERY-1: what a cut turn was sent goes with its next try: words a CLI died on aren't lost,
/// and the job doesn't end `done` without them.
#[test]
fn recovery_1_the_words_a_turn_died_on_go_with_its_next_try() {
    let (_folder, obid) = obid("recovery-1-words");
    let id = post_job(&obid, "PAUSE: 2000 KEEP PROMPTS");
    at_work(&obid, &id);
    let answer = call_typed(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({"text": "DIE use SQLite"}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 3, "{told:?}");
    assert_eq!(told[1].trim(), "DIE use SQLite", "{told:?}");
    assert!(told[2].contains("Your run was cut"), "{told:?}");
    assert!(told[2].contains("End of Oby's note"), "{told:?}");
    assert!(told[2].contains("What you were last sent"), "{told:?}");
    assert!(told[2].trim_end().ends_with("DIE use SQLite"), "{told:?}");
}

/// RECOVERY-1: a wait ends when words come for it, they go with its next try, and being the
/// owner's own they start its tries again.
#[test]
fn recovery_1_the_owners_words_end_a_wait_and_give_more_tries() {
    let (_folder, obid) = obid_with("recovery-1-typed", json!({"stallAfter": 600}), &[]);
    let id = post_job(&obid, "DIE ALWAYS KEEP PROMPTS");
    let step = |want: &'static str| {
        within(30, "its wait to try again", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
            job["step"]
                .as_str()
                .is_some_and(|s| s.starts_with(want))
                .then_some(job)
        })
    };
    let job = step("Trying again in 1 minute (1 of 3)");
    let answer = call_typed(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({"text": "use SQLite"}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    within(30, "its next try", || {
        (prompts(&job).len() == 2).then_some(())
    });
    assert!(prompts(&job)[1].contains("use SQLite"), "{job}");
    let job = step("Trying again in 1 minute (1 of 3)");
    assert_eq!(prompts(&job).len(), 2, "{job}");
    let answer = call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert_eq!(wait(&obid, &id)["status"], "stopped");
}

/// RECOVERY-1: an earlier run's stderr says nothing of why this one died: a good turn that printed
/// a rate limit's words leaves its account alone when a later turn crashes.
#[test]
fn recovery_1_an_earlier_runs_stderr_is_no_reason() {
    let (_folder, obid) = obid("recovery-1-stale");
    let id = post_job(&obid, "STDERR LIMIT PAUSE: 2000 KEEP PROMPTS");
    at_work(&obid, &id);
    let answer = call_typed(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({"text": "DIE now"}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(prompts(&job).len(), 3, "{job}");
    assert!(job["accountLimit"].is_null(), "{job}");
    let account = call(&obid, "GET", "/v1/status", None).body["accounts"][0].clone();
    assert!(account["fullUntil"].is_null(), "{account}");
}

/// RECOVERY-1: the agent's own last words aren't the error: a stranger's text it repeated neither
/// counts as a passing error (an unknown one gets its one try) nor reaches obid's prompt.
#[test]
fn recovery_1_the_agents_last_words_are_not_the_error() {
    let (_folder, obid) = obid("recovery-1-planted");
    let id = post_job(&obid, "LAST WORDS FAIL KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[1].contains("stopped without finishing"), "{told:?}");
    assert!(!told[1].contains("529"), "{told:?}");
    assert!(!told[1].contains("Ignore your task"), "{told:?}");
}

/// RECOVERY-1: a retry resets neither guard: the ceiling counts the time it waited, and the budget
/// the tokens of the turns before it.
#[test]
fn recovery_1_a_retry_keeps_the_ceiling_and_the_budget() {
    let mut job = stored("eeee1001", "TRANSIENT ALWAYS KEEP PROMPTS");
    job.ran_secs = Some(3600 - 5);
    let (_folder, obid) = obid_with("recovery-1-ceiling", json!({"jobCeilingHours": 1}), &[job]);
    let job = wait(&obid, "eeee1001");
    assert_eq!(job["stalled"]["why"], "ceiling", "{job}");

    let (_folder, obid) = obid_with("recovery-1-budget", json!({"jobTokenBudget": 1000}), &[]);
    let id = post_job(&obid, "TRANSIENT SPEND: 600 KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["stalled"]["why"], "budget", "{job}");
    assert!(prompts(&job)[1].contains("Your run was cut"), "{job}");
}

/// RECOVERY-1: a retry keeps the job's mode and its untrusted mark.
#[test]
fn recovery_1_a_retry_keeps_the_mode_and_the_mark() {
    let mut job = stored("eeee1002", "TRANSIENT KEEP PROMPTS");
    job.mode = Some("plan".into());
    job.untrusted = true;
    let (_folder, obid) = obid_with("recovery-1-mark", json!({}), &[job]);
    let job = wait(&obid, "eeee1002");
    assert_eq!(prompts(&job).len(), 2, "{job}");
    assert_eq!(job["mode"], "plan", "{job}");
    assert_eq!(job["untrusted"], true, "{job}");
    let log = Path::new(job["worktree"].as_str().unwrap()).join(".obi/args.log");
    let runs = std::fs::read_to_string(log).unwrap();
    assert_eq!(runs.matches("--permission-mode plan").count(), 2, "{runs}");
}

/// How many continuations obid made by itself for job `id`, as its file keeps them (RECOVERY-17).
fn tries(obid: &Obid, id: &str) -> u64 {
    let file = obid.home.join(format!("jobs/{id}.json"));
    let kept: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    kept["recovery"]["tries"].as_u64().unwrap_or(0)
}

/// RECOVERY-17: obid carries a job on by itself 10 times in all, whatever cut it and however well
/// it went between, then the job waits for the owner, interrupted, its branch kept, the reason
/// naming the cap and the last error. An unclean restart takes one from every job at work, one
/// started again as well as one continued; an error's try and a stall's take one; a job at its
/// 10th when obid went down isn't picked up. The owner's Try again gives them back.
#[test]
fn recovery_17_ten_continuations_in_all_then_the_owner() {
    let job = |id: &str, task: &str, tries: usize, status: obid::job::Status| {
        let mut job = stored(id, task);
        job.status = status;
        job.recovery.tries = tries;
        job
    };
    let (_folder, obid) = obid_with(
        "recovery-17",
        json!({}),
        &[
            // Queued when obid went down, so started again with its task: the restart takes one.
            job(
                "ffff0001",
                "DIE ALWAYS KEEP PROMPTS",
                8,
                obid::job::Status::Queued,
            ),
            job("ffff0002", "KEEP PROMPTS", 10, obid::job::Status::Running),
            job("ffff0003", "KEEP PROMPTS", 9, obid::job::Status::Running),
            job(
                "ffff0004",
                "QUIET KEEP PROMPTS",
                9,
                obid::job::Status::Queued,
            ),
            job(
                "ffff0005",
                "QUIET ONCE KEEP PROMPTS",
                8,
                obid::job::Status::Queued,
            ),
        ],
    );
    let capped = |job: &Value, last: &str| {
        assert_eq!(job["status"], "interrupted", "{job}");
        assert_eq!(job["nextStep"], "Try again", "{job}");
        let id = job["id"].as_str().unwrap();
        assert_eq!(job["branch"], format!("oby/investigate-{id}"), "{job}");
        let reason = job["reason"].as_str().unwrap();
        assert!(
            reason.contains("by itself 10 times") && reason.contains(last),
            "{reason}"
        );
    };
    let died = wait(&obid, "ffff0001");
    capped(&died, "stopped without finishing");
    // Its first turn, and the one try the restart left it.
    assert_eq!(prompts(&died).len(), 2, "{died}");
    let left = wait(&obid, "ffff0002");
    capped(&left, "obid or the Mac went down while it ran");
    let restarted = wait(&obid, "ffff0003");
    assert_eq!(restarted["status"], "done", "{restarted}");
    assert!(
        prompts(&restarted)[0].contains("Your run was cut"),
        "{restarted}"
    );
    assert_eq!(tries(&obid, "ffff0003"), 10);
    // Quiet with none left: its first turn, then its handoff note.
    let quiet = wait(&obid, "ffff0004");
    capped(&quiet, "no progress for 3 seconds");
    assert_eq!(prompts(&quiet).len(), 2, "{quiet}");
    // Quiet once with one left: continued, and that takes the 10th.
    let continued = wait(&obid, "ffff0005");
    assert_eq!(continued["status"], "done", "{continued}");
    assert_eq!(prompts(&continued).len(), 2, "{continued}");
    assert_eq!(tries(&obid, "ffff0005"), 10);
    // The owner's Try again starts the count again.
    let again = call_typed(
        &obid,
        "POST",
        "/v1/jobs/ffff0002/messages",
        &json!({"text": TRY_AGAIN}),
    );
    assert_eq!(again.status, 200, "{}", again.text);
    let done = wait(&obid, "ffff0002");
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(tries(&obid, "ffff0002"), 0);
}

/// RECOVERY-17: a fix round and a new session (RECOVERY-8) take none of a job's 10; a clean restart
/// takes none either (`recovery_2_a_clean_restart_costs_no_try`), nor does a five-hour limit's
/// resume (`account_limit_flow::limits_5_a_five_hour_limit_resumes_by_itself_in_its_session`).
#[test]
fn recovery_17_rounds_and_a_new_session_take_no_try() {
    let (_folder, obid) = obid("recovery-17-free");
    let rounds = json!({"repo": "obi-e2e/fixture-app", "kind": "investigate", "rounds": 3,
                        "check": "false", "task": "KEEP PROMPTS"});
    let answer = call(&obid, "POST", "/v1/jobs", Some(&json!({ "job": rounds })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    let job = wait(&obid, &id);
    assert_eq!(job["round"], 3, "{job}");
    assert_eq!(tries(&obid, &id), 0);
    let id = post_job(&obid, "TOO LONG KEEP PROMPTS");
    assert_eq!(wait(&obid, &id)["status"], "done");
    assert_eq!(tries(&obid, &id), 0);
}

/// RECOVERY-2: a run obid went down with goes on in its session when obid starts again, told what
/// happened; at most 3 times, then it stays for the owner. A job the owner stopped never does.
#[test]
fn recovery_2_a_run_cut_by_a_restart_continues_at_most_three_times() {
    let job = |id: &str, restarts: usize, status: obid::job::Status| {
        let mut job = stored(id, "KEEP PROMPTS");
        job.status = status;
        job.recovery.restarts = restarts;
        job
    };
    let (_folder, obid) = obid_with(
        "recovery-2",
        json!({}),
        &[
            job("eeee0001", 2, obid::job::Status::Running),
            job("eeee0002", 3, obid::job::Status::Running),
            job("eeee0003", 0, obid::job::Status::Stopped),
            // Waiting for a slot, it wasn't cut: no restart counts, and it isn't told so.
            {
                let mut queued = job("eeee0004", 3, obid::job::Status::Queued);
                queued.heard = vec!["KEEP PROMPTS".into()];
                queued
            },
        ],
    );
    let queued = wait(&obid, "eeee0004");
    assert_eq!(queued["status"], "done", "{queued}");
    assert!(
        !prompts(&queued)[0].contains("Your run was cut"),
        "{queued}"
    );
    // Left there, it's the owner's: one item, as #915's guard leaves.
    let inbox = call(&obid, "GET", "/v1/inbox", None).body.to_string();
    assert!(inbox.contains("eeee0002-crash-"), "{inbox}");
    let left = call(&obid, "GET", "/v1/jobs/eeee0002", None).body;
    assert_eq!(left["status"], "interrupted", "{left}");
    assert_eq!(left["reason"], obid::job::RESTARTS_SPENT, "{left}");
    assert_eq!(left["nextStep"], "Try again", "{left}");
    let stopped = call(&obid, "GET", "/v1/jobs/eeee0003", None).body;
    assert_eq!(stopped["status"], "stopped", "{stopped}");
    let job = wait(&obid, "eeee0001");
    assert_eq!(job["status"], "done", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("Your run was cut"), "{told:?}");
    assert!(told[0].contains("went down"), "{told:?}");
    assert!(told[0].contains("check what really ran"), "{told:?}");
    assert!(
        job.get("recovery").is_none(),
        "never in the API, which stays add-only: {job}"
    );
}

/// RECOVERY-2: a stop obid was asked for (a quit, an update) costs no try: four in a row leave the
/// job running in its session, told each time it was stopped, never left for the owner.
#[test]
fn recovery_2_a_clean_restart_costs_no_try() {
    let (folder, mut obid) = obid("recovery-2-clean");
    // SLEEP works on in every run that continues it until it's ended, as a job cut mid-work does; a
    // PAUSE continued ends by itself at once ("Nothing to do."), racing the next restart's stop.
    let id = post_job(&obid, "SLEEP KEEP PROMPTS");
    at_work(&obid, &id);
    let settings = std::fs::read_to_string(folder.0.join("config.json")).unwrap();
    let settings: Value = serde_json::from_str(&settings).unwrap();
    for restart in 1..=4 {
        obid.stop();
        obid = start(&folder.0, &settings, None);
        let job = within(30, "its turn after the restart", || {
            let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
            (job["status"] == "running" && prompts(&job).len() == restart + 1).then_some(job)
        });
        let told = prompts(&job);
        assert!(told[restart].contains("Your run was cut"), "{told:?}");
        assert!(told[restart].contains("was stopped"), "{told:?}");
    }
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["status"], "running", "{job}");
    assert!(job["reason"].is_null(), "{job}");
    // Nor one of the 10 obid carries it on by itself in all (RECOVERY-17).
    assert_eq!(tries(&obid, &id), 0);
}

/// RECOVERY-2: obid going down mid-turn loses nothing that turn was sent: the owner's words were
/// taken from the job to send, so its next run hears them again.
#[test]
fn recovery_2_the_words_a_run_went_down_with_go_with_the_next() {
    let (folder, mut obid) = obid("recovery-2-words");
    let id = post_job(&obid, "PAUSE: 2000 KEEP PROMPTS");
    at_work(&obid, &id);
    let answer = call_typed(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        &json!({"text": "PAUSE: 60000 use SQLite"}),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    within(30, "its turn with the words", || {
        (prompts(&job).len() == 2).then_some(())
    });
    obid.child.kill().unwrap();
    let _ = obid.child.wait();
    let settings = std::fs::read_to_string(folder.0.join("config.json")).unwrap();
    let settings: Value = serde_json::from_str(&settings).unwrap();
    let obid = start(&folder.0, &settings, None);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 3, "{told:?}");
    assert!(told[2].contains("Your run was cut"), "{told:?}");
    assert!(told[2].contains("What you were last sent"), "{told:?}");
    assert!(told[2].contains("use SQLite"), "{told:?}");
}

/// RECOVERY-2: after a panic before publishing began (`jobs::tests::recovery_2_a_real_panic_is_
/// scheduled_again`), its next run goes on in its session, told what happened.
#[test]
fn recovery_2_the_run_after_a_panic_hears_of_it() {
    let mut job = stored("eeee2001", "KEEP PROMPTS");
    job.status = obid::job::Status::Queued;
    job.recovery.restarts = 1;
    job.recovery.cut = Some("obid hit a bug and ended your run".into());
    let (_folder, obid) = obid_with("recovery-2-panic", json!({}), &[job]);
    let job = wait(&obid, "eeee2001");
    assert_eq!(job["status"], "done", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("obid hit a bug"), "{told:?}");
    assert!(told[0].contains("End of Oby's note"), "{told:?}");
}

/// RECOVERY-3: a job gone quiet is stopped and continued in its session, with no model asked.
#[test]
fn recovery_3_a_quiet_job_continues_in_its_session() {
    let (_folder, obid) = obid("recovery-3");
    let events = follow_events(&obid);
    let id = post_job(&obid, "QUIET ONCE KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert!(job["stalled"].is_null(), "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[1].contains("no progress for 3 seconds"), "{told:?}");
    assert!(told[1].contains("check what really ran"), "{told:?}");
    assert!(told[1].contains("What you were last sent"), "{told:?}");
    assert!(told[1].contains("QUIET ONCE"), "{told:?}");
    let seen: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(!seen.iter().any(|(name, _)| name == "stalled"), "{seen:?}");
}

/// RECOVERY-3: after 5 tries it stalls on its own card, though a conversation started it: no turn
/// takes it on by itself. Its agent stays busy without a word past the windows CPU counts for; an
/// idle one gets 2 (RECOVERY-12).
#[test]
fn recovery_3_five_tries_then_it_stalls() {
    let (_folder, obid) = obid_with("recovery-3-out", json!({"stallAfter": 1}), &[]);
    // It writes in its worktree without a word: busy, so 5 tries (spinning gets 2, RECOVERY-16).
    let id = post_job(&obid, "SLEEP KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["stalled"]["why"], "quiet", "{job}");
    // POST /v1/jobs gives it a conversation of its own (TASKCONV-1).
    assert!(job["conversation"].is_string(), "{job}");
    let told = prompts(&job);
    // Its first turn, 5 continued, and the handoff note's.
    assert_eq!(told.len(), 7, "{told:?}");
    assert!(told[6].contains("handoff note"), "{told:?}");
    // The deleted supervisor took a stall on within seconds; nothing does now.
    std::thread::sleep(std::time::Duration::from_secs(5));
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["status"], "interrupted", "{job}");
    assert!(!job["stalled"].is_null(), "{job}");
    assert_eq!(job["supervised"], json!([]), "{job}");
    let inbox = call(&obid, "GET", "/v1/inbox", None).body;
    assert!(
        !inbox.as_array().unwrap().iter().any(|i| i["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("supervisor-"))),
        "{inbox}"
    );
}

/// RECOVERY-12: an agent busy without a word or a change in its worktree (a command building
/// outside it) uses CPU, which counts as progress: it runs past 2 quiet windows and ends done in its
/// one turn.
#[test]
fn recovery_12_a_silent_busy_command_keeps_its_slot() {
    let (_folder, obid) = obid("recovery-12-busy");
    let id = post_job(&obid, "BUSY: 8000 KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["summary"], "Built quietly.", "{job}");
    assert!(job["stalled"].is_null(), "{job}");
    assert_eq!(prompts(&job).len(), 1, "{job}");
}

/// RECOVERY-12: an agent quiet and idle (no CPU, nothing changed: a hung CLI) is continued at once,
/// twice at most, then stalls with its handoff note.
#[test]
fn recovery_12_a_silent_idle_agent_is_continued_at_most_twice() {
    let (_folder, obid) = obid("recovery-12-idle");
    let id = post_job(&obid, "QUIET KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["stalled"]["why"], "quiet", "{job}");
    let told = prompts(&job);
    // Its first turn, 2 continued, and the handoff note's.
    assert_eq!(told.len(), 4, "{told:?}");
    assert!(told[1].contains("no progress for 3 seconds"), "{told:?}");
    assert!(told[3].contains("handoff note"), "{told:?}");
}

/// RECOVERY-16: an agent spinning its CPU without a word or a change in its worktree is continued
/// twice at most, as an idle one is, then stalls with its handoff note.
#[test]
fn recovery_16_a_spinning_agent_is_continued_at_most_twice() {
    let (_folder, obid) = obid_with("recovery-14-spin", json!({"stallAfter": 1}), &[]);
    let id = post_job(&obid, "BUSY KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["stalled"]["why"], "quiet", "{job}");
    let told = prompts(&job);
    // Its first turn, 2 continued, and the handoff note's.
    assert_eq!(told.len(), 4, "{told:?}");
    assert!(told[3].contains("handoff note"), "{told:?}");
}

/// RECOVERY-12: the CPU an idle agent's window starts from is read once it has gone quiet, so what
/// it used starting and talking isn't counted: its first cut comes after one window, not two.
#[test]
fn recovery_12_an_idle_jobs_first_cut_comes_after_one_window() {
    let (_folder, obid) = obid_with("recovery-12-one-window", json!({"stallAfter": 6}), &[]);
    let id = post_job(&obid, "QUIET ONCE");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    // Its rounds' starts: the first, and the one Oby continued once the window cut it.
    let transcript = call(&obid, "GET", &format!("/v1/jobs/{id}/transcript"), None).body;
    let starts: Vec<i64> = transcript["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "start")
        .map(|e| {
            e["at"]
                .as_str()
                .unwrap()
                .parse::<jiff::Timestamp>()
                .unwrap()
                .as_second()
        })
        .collect();
    assert_eq!(starts.len(), 2, "{transcript}");
    // One 6-second window and the seconds the first round takes to start its agent (`at` is to
    // the second); two windows would be 12 seconds and more.
    let took = starts[1] - starts[0];
    assert!((6..12).contains(&took), "{took} seconds: {transcript}");
}

/// RECOVERY-4: the turn limit continues the same session, and counts with a quiet job's tries.
#[test]
fn recovery_4_the_turn_limit_continues_and_counts_with_quiet() {
    let (_folder, obid) = obid("recovery-4");
    let id = post_job(&obid, "MAXTURNS ONCE KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[1].contains("200 turns in one round"), "{told:?}");

    let id = post_job(&obid, "MAXTURNS KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["stalled"]["why"], "turns", "{job}");
    assert_eq!(prompts(&job).len(), 7, "{job}");
}

/// RECOVERY-5: while it's tried again it stays running, its step says so and when, the real error
/// only from the third try; nothing says it failed, and Stop works during a wait.
#[test]
fn recovery_5_a_retry_shows_as_running_with_the_error_held_back() {
    let (_folder, obid) = obid("recovery-5");
    let events = follow_events(&obid);
    let id = post_job(&obid, "TRANSIENT ALWAYS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    let seen: Vec<(String, Value)> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    let retrying: Vec<&Value> = seen
        .iter()
        .filter(|(name, job)| {
            name == "job"
                && job["step"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("Trying again"))
        })
        .map(|(_, job)| job)
        .collect();
    assert!(!retrying.is_empty(), "{seen:?}");
    for job in &retrying {
        assert_eq!(job["status"], "running", "{job}");
        assert!(job["finished"].is_null(), "{job}");
        let step = job["step"].as_str().unwrap();
        assert!(step.contains(" in 3 seconds "), "{step}");
        assert_eq!(step.contains("529"), step.contains("(3 of 3)"), "{step}");
    }
    assert!(
        retrying
            .iter()
            .any(|j| j["step"].as_str().unwrap().contains("(3 of 3)")),
        "{retrying:?}"
    );

    let (_folder, obid) = obid_with("recovery-5-stop", json!({"stallAfter": 600}), &[]);
    let id = post_job(&obid, "TRANSIENT ALWAYS");
    within(30, "its wait to try again", || {
        let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        job["step"]
            .as_str()
            .is_some_and(|s| s.starts_with("Trying again in 1 minute (1 of 3)"))
            .then_some(())
    });
    let answer = call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "stopped", "{job}");
}

/// RECOVERY-6: Claude Code retrying an API call by itself is activity: past the quiet window with
/// nothing else said, the job runs on and shows it.
#[test]
fn recovery_6_claude_codes_own_retrying_is_activity() {
    let (_folder, obid) = obid("recovery-6");
    let events = follow_events(&obid);
    let id = post_job(&obid, "API RETRY: 5000");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["summary"], "Retried.", "{job}");
    let steps: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|(_, job)| job["step"].as_str().map(str::to_owned))
        .collect();
    assert!(
        steps
            .iter()
            .any(|s| s == "Trying again in 8 seconds (1 of 10)"),
        "{steps:?}"
    );
    assert!(
        steps
            .iter()
            .any(|s| s == "Trying again in 8 seconds (3 of 10): overloaded"),
        "{steps:?}"
    );
    assert!(
        steps.iter().any(|s| s == "Working"),
        "back at work, it no longer says it's trying again: {steps:?}"
    );
}

/// The token budget stalls a job. Past it, Try again or other words through the API (what oby-mcp
/// and other sessions call) do nothing but ask the owner in the inbox, once per kind however often
/// they're sent; their Confirm carries it on with a new budget.
#[test]
fn the_token_budget_stalls_and_only_the_owners_confirm_carries_on() {
    let (_folder, obid) = obid_with("long-budget", json!({"jobTokenBudget": 1000}), &[]);
    let id = post_job(&obid, "SPEND: 5000");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert_eq!(job["limit"], "the task used 1000 tokens", "{job}");
    assert_eq!(job["stalled"]["why"], "budget");
    // A safety stall is never tried again by itself, whatever any reset says (#740).
    assert_eq!(job["resumesAt"], Value::Null, "{job}");
    let confirms = || -> Vec<Value> {
        let inbox = call(&obid, "GET", "/v1/inbox", None).body;
        inbox
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["kind"] == "confirm" && i["job"] == id.as_str())
            .cloned()
            .collect()
    };
    // Asked again and again (a looping caller), each kind of item comes once.
    for words in [
        TRY_AGAIN,
        "start it again from scratch",
        TRY_AGAIN,
        "start it again from scratch",
    ] {
        let answer = call(
            &obid,
            "POST",
            &format!("/v1/jobs/{id}/messages"),
            Some(&json!({"text": words})),
        );
        assert_eq!(answer.status, 422, "{}", answer.text);
        assert!(
            answer.text.contains("waits for your Confirm"),
            "{}",
            answer.text
        );
    }
    // Files with the words go with its Confirm (ATTACH-17), which still waits for the owner.
    let file = obid.home.join("attachments/shot.png");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"png").unwrap();
    let with_files = call(
        &obid,
        "POST",
        &format!("/v1/jobs/{id}/messages"),
        Some(
            &json!({"text": "start it again from scratch", "attachments": [file.to_str().unwrap()]}),
        ),
    );
    assert_eq!(with_files.status, 422, "{}", with_files.text);
    assert!(
        with_files.text.contains("waits for your Confirm"),
        "{}",
        with_files.text
    );
    let job = call(&obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["status"], "interrupted", "{job}");
    assert!(job["followUpOf"].is_null());
    let items = confirms();
    assert_eq!(items.len(), 2, "{items:?}");
    assert!(
        items
            .iter()
            .all(|i| i["text"].as_str().unwrap().contains("safety limit"))
    );
    let retry = items
        .iter()
        .find(|i| i["title"].as_str().unwrap().starts_with("Try "))
        .expect("Try again's item");
    let answer = confirm(&obid, retry["id"].as_str().unwrap());
    assert_eq!(answer.status, 200, "{}", answer.text);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert!(job["stalled"].is_null(), "{job}");
}

/// The ceiling counts the time of every run before (a restart, a Try again): a job that already
/// ran its hour stalls at once, and obid writes its note.
#[test]
fn the_ceiling_counts_every_run() {
    let mut job = obid::job::Job::new(
        "ce111ng0".into(),
        "obi-e2e/fixture-app".into(),
        "investigate".into(),
        "CHATTY: 600000".into(),
        "claude".into(),
        "stub".into(),
    );
    job.branch = "oby/investigate-ce111ng0".into();
    job.ran_secs = Some(3600);
    let (_folder, obid) = obid_with("long-ceiling", json!({"jobCeilingHours": 1}), &[job]);
    let job = wait(&obid, "ce111ng0");
    // It never had a session to pick up: a dead end (#725), its stall kept.
    assert_eq!(job["status"], "failed", "{job}");
    assert_eq!(job["limit"], "the task ran 1 hour", "{job}");
    assert_eq!(job["stalled"]["why"], "ceiling");
    assert_eq!(job["stalled"]["noteBy"], "obid");
}

/// While a job's worker runs, obid holds `caffeinate -i -w <its pid>` so idle sleep doesn't freeze
/// it, and lets it go once no worker is left (RECOVERY-10).
#[cfg(target_os = "macos")]
#[test]
fn recovery_10_a_running_job_keeps_the_mac_awake() {
    let (_folder, obid) = obid("long-awake");
    let pattern = format!("/usr/bin/caffeinate -i -w {}", obid.child.id());
    let awake = || {
        std::process::Command::new("/usr/bin/pgrep")
            .args(["-fx", &pattern])
            .output()
            .unwrap()
            .status
            .success()
    };
    assert!(!awake(), "no job, no caffeinate");
    let id = post_job(&obid, "CHATTY: 4000");
    within(30, "caffeinate while the job runs", || {
        awake().then_some(())
    });
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    within(30, "caffeinate gone after the job", || {
        (!awake()).then_some(())
    });
}

/// TASKS-19: a committing job with no check whose agent ends `GOAL NOT MET: …` doesn't go out by
/// itself: it needs you, its commits kept and its line the reason, and the owner's Publish still
/// sends them (PUBLISHING-9).
#[test]
fn tasks_19_goal_not_met_with_commits_ends_interrupted_and_unpublished() {
    let (_folder, obid) = obid_with("tasks-19-not-met", json!({"/repos/0/check": null}), &[]);
    let id = post_kind(
        &obid,
        "build",
        "COMMIT: a.txt ENDS: GOAL NOT MET: the tests are left",
    );
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "interrupted", "{job}");
    assert!(
        job["reason"]
            .as_str()
            .unwrap()
            .starts_with("GOAL NOT MET: the tests are left"),
        "{job}"
    );
    assert!(job["nextStep"].is_null(), "{job}");
    assert_eq!(job["commits"], 1, "{job}");
    assert!(
        job["published"].is_null() && job["pushed"].is_null(),
        "{job}"
    );
    assert!(job["outgoing"].as_array().unwrap().is_empty(), "{job}");
    assert_eq!(job["canPublishAnyway"], true, "{job}");
    assert_eq!(
        job["summary"],
        "Added a.txt.\n\nGOAL NOT MET: the tests are left"
    );
}

/// TASKS-19: a round's `GOAL NOT MET` holds only that round: the next round that ends with no line
/// is told once to say it, rather than ending on the earlier round's line.
#[test]
fn tasks_19_a_silent_round_after_goal_not_met_is_asked_again() {
    let (_folder, obid) = obid_with("tasks-19-silent", json!({"/repos/0/check": null}), &[]);
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "build", "rounds": 2,
                     "task": "COMMIT: a.txt SILENT ROUND KEEP PROMPTS ENDS: GOAL NOT MET: the tests"});
    let answer = call(&obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    let job = wait(&obid, answer.body["id"].as_str().unwrap());
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["round"], 2, "{job}");
    let told = prompts(&job);
    assert_eq!(told.len(), 3, "{told:?}");
    assert!(
        told[1..]
            .iter()
            .all(|p| p.starts_with("Your last line didn't say `GOAL MET`")),
        "{told:?}"
    );
}

/// TASKS-19: a one-round committing job with no check is asked for its GOAL line, and a turn that
/// ends without one is told once, spending no round, before it goes out.
#[test]
fn tasks_19_a_one_round_job_with_no_check_is_asked_for_its_goal_line() {
    let (_folder, obid) = obid_with("tasks-19-asked", json!({"/repos/0/check": null}), &[]);
    let id = post_kind(&obid, "build", "COMMIT: a.txt KEEP PROMPTS");
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["rounds"], 1, "{job}");
    assert_eq!(job["round"], 1, "{job}");
    assert_eq!(job["commits"], 1, "{job}");
    let args =
        std::fs::read_to_string(Path::new(job["worktree"].as_str().unwrap()).join(".obi/args.log"))
            .unwrap();
    assert!(
        args.contains("`GOAL MET` when the user's goal is fully met"),
        "{args}"
    );
    let told = prompts(&job);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(
        told[1].starts_with("Your last line didn't say `GOAL MET`"),
        "{told:?}"
    );
    // Its reply to obid's word leaves the job's summary to the work's.
    assert_eq!(job["summary"], "Added a.txt.", "{job}");
}
