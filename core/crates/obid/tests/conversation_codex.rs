//! Codex as a conversation's agent (docs/conversation-harness.md, step 4): `codex app-server` in
//! the conversation's room, boxed like the Claude Code session, one thread per conversation, with
//! the same Oby tools over MCP and the same rules. The agent here is a stand-in
//! (tests/agents/fake-codex-harness).
mod support;

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::*;

fn agent(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/agents")
        .join(name)
}

/// obid with conversations on Codex (the stand-in), and Claude Code's stand-in for the plan loop's
/// brain; `signed_in` puts a login in the Codex account's folder.
fn obid(name: &str, signed_in: bool) -> (Folder, Obid) {
    obid_with(name, signed_in, "fake-codex-harness", None)
}

/// [`obid`] with `bin` as Codex, and `login` as the account's auth.json when there is one. In Ask,
/// so its conversations stay in Oby's box (LANES-13), what these tests are about; [`mode`] sets
/// another.
fn obid_with(name: &str, signed_in: bool, bin: &str, login: Option<Value>) -> (Folder, Obid) {
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agent("fake-harness"));
    config["conversation"] = json!({"agent": "codex"});
    let account = folder.0.join("home/accounts/codex");
    config["agents"]["codex"] = json!({"bin": agent(bin),
        "accounts": [{"id": "cstub", "name": "Codex stub", "configDir": account}]});
    std::fs::create_dir_all(&account).unwrap();
    if signed_in {
        let login = login.unwrap_or_else(|| json!({}));
        std::fs::write(account.join("auth.json"), login.to_string()).unwrap();
    }
    let obid = start(&folder.0, &config, None);
    mode(&obid, "ask");
    (folder, obid)
}

/// Settings › Permissions: the mode new jobs get, and the lane of the owner's Codex conversation.
fn mode(obid: &Obid, mode: &str) {
    let body = json!({ "mode": mode });
    let set = call_signed(obid, "PUT", "/v1/settings/permissions", Some(&body));
    assert_eq!(set.body, body, "{}", set.text);
}

/// Every event until Oby's turn in `conversation` (not one telling how a job ended), in order.
fn until_oby_turn(events: &Receiver<(String, Value)>, seconds: u64) -> Vec<(String, Value)> {
    let give_up_at = Instant::now() + Duration::from_secs(seconds);
    let mut got = vec![];
    loop {
        let left = give_up_at.saturating_duration_since(Instant::now());
        let (name, data) = events
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("no Oby turn within {seconds} s: {got:?}"));
        let done = name == "turn" && data["who"] == "oby" && data["ended"] != true;
        got.push((name, data));
        if done {
            return got;
        }
    }
}

fn said(obid: &Obid, body: Value) -> Answer {
    let answer = call(obid, "POST", "/v1/plans", Some(&body));
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer
}

/// What the stand-in recorded of each turn, oldest first: in each conversation's own CODEX_HOME.
fn turns(obid: &Obid) -> Vec<Value> {
    let mut all: Vec<Value> = std::fs::read_dir(obid.home.join("conversations"))
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|entry| {
            std::fs::read_to_string(entry.path().join("config/turns.jsonl"))
                .unwrap_or_default()
                .lines()
                .map(|l| serde_json::from_str::<Value>(l).unwrap())
                .collect::<Vec<_>>()
        })
        .collect();
    all.sort_by_key(|t| t["at"].as_u64());
    all
}

/// What the conversation answered: a reply's or a question's words.
fn words(answer: &Answer) -> String {
    answer.body["text"]
        .as_str()
        .or(answer.body["question"]["text"].as_str())
        .unwrap_or("")
        .to_owned()
}

/// A ChatGPT access token (a JWT) that ends `seconds` from now.
fn access_token(seconds: i64) -> String {
    use base64::Engine;
    let encode = |v: Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v.to_string());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    format!(
        "{}.{}.{}",
        encode(json!({"alg": "RS256"})),
        encode(json!({"exp": now + seconds})),
        "s".repeat(40)
    )
}

fn inbox(obid: &Obid) -> Vec<Value> {
    let inbox = call(obid, "GET", "/v1/inbox", None);
    inbox.body.as_array().cloned().unwrap_or_default()
}

/// A turn on Codex streams its words as `delta` events, each tool item is a `step` with its tool
/// and detail, and the turn ends with Oby's `turn`, as on Claude Code. Codex runs `app-server` in
/// the room, its own sandbox off inside obid's, every approval obid's, with Oby's MCP server from
/// its arguments and the turn's token only in its environment.
#[test]
fn a_codex_turn_streams_its_words_and_ends_in_a_turn() {
    let (_folder, obid) = obid("codex-conv-stream", true);
    let events = follow_events(&obid);
    let answer = said(&obid, json!({"said": "HELLO", "new": true}));
    let conversation = answer.body["conversation"].as_str().unwrap().to_owned();
    assert_eq!(answer.body["kind"], "reply", "{}", answer.text);
    assert_eq!(answer.body["text"], "Hello there.", "{}", answer.text);

    let seen = until_oby_turn(&events, 30);
    let deltas: Vec<&Value> = seen
        .iter()
        .filter(|(n, _)| n == "delta")
        .map(|(_, d)| d)
        .collect();
    assert!(deltas.len() >= 2, "{seen:?}");
    assert!(
        deltas
            .iter()
            .all(|d| d["conversation"] == conversation.as_str())
    );
    let streamed: String = deltas.iter().map(|d| d["text"].as_str().unwrap()).collect();
    assert_eq!(streamed, "Hello there.");
    let steps: Vec<&Value> = seen
        .iter()
        .filter(|(n, d)| n == "step" && d["tool"].is_string())
        .map(|(_, d)| d)
        .collect();
    assert_eq!(steps.len(), 2, "{seen:?}");
    assert_eq!(steps[0]["tool"], "web_search");
    assert_eq!(steps[0]["detail"], "greetings");
    assert_eq!(steps[0]["state"], "running");
    assert_eq!(steps[1]["state"], "done");
    let (_, last) = seen.last().unwrap();
    assert_eq!(last["text"], "Hello there.");

    let recorded = turns(&obid);
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    let argv: Vec<&str> = recorded[0]["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(argv.last(), Some(&"app-server"), "{argv:?}");
    assert!(
        argv.iter().any(|a| a.starts_with("mcp_servers.oby.url=")
            && a.ends_with(&format!("/v1/agent/mcp/conversation-{conversation}\""))),
        "{argv:?}"
    );
    assert!(
        argv.contains(&r#"mcp_servers.oby.bearer_token_env_var="OBY_MCP_TOKEN""#),
        "{argv:?}"
    );
    assert!(argv.contains(&r#"web_search="live""#), "{argv:?}");
    // No shell (F2: under `untrusted` Codex runs its "safe" reads without asking), no ChatGPT apps
    // (a code-host login), and the turn's token kept out of any command's environment (F6).
    for setting in [
        "features.shell_tool=false",
        "features.shell_snapshot=false",
        "features.apps=false",
        "features.plugins=false",
        "features.multi_agent=false",
        "features.hooks=false",
        "features.browser_use=false",
        "features.browser_use_full_cdp_access=false",
        "features.computer_use=false",
        "features.in_app_browser=false",
        "features.in_app_local_automation=false",
        "features.view_image=false",
        "features.skill_mcp_dependency_install=false",
        "features.workspace_dependencies=false",
        "features.daemon_auto_start=false",
        "features.worktrees=false",
        "features.memories=false",
        "features.external_agent_memory_import=false",
        "features.guardian_conversation_history_tools=false",
        "features.analytics_plan_history=false",
        r#"shell_environment_policy.exclude=["OBY_MCP_TOKEN"]"#,
    ] {
        assert!(argv.contains(&setting), "{setting}: {argv:?}");
    }
    assert!(recorded[0]["resumed"].is_null());
    assert_eq!(recorded[0]["sandbox"], "danger-full-access");
    assert_eq!(recorded[0]["approvalPolicy"], "untrusted");
    assert!(
        recorded[0]["instructions"]
            .as_str()
            .unwrap()
            .contains("oby_dispatch"),
        "{recorded:?}"
    );
    let folder = obid.home.join(format!("conversations/{conversation}"));
    assert_eq!(
        Path::new(recorded[0]["cwd"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        folder.join("room").canonicalize().unwrap()
    );
    // Its CODEX_HOME is the conversation's own, with a copy of the account's login and nothing
    // else of the account's; the copy goes when the turn ends.
    assert_eq!(
        recorded[0]["codexHome"].as_str().map(PathBuf::from),
        Some(folder.join("config"))
    );
    // (`threads` is the stand-in's own, made when its thread started.)
    assert_eq!(
        recorded[0]["holds"],
        json!(["auth.json", "threads"]),
        "{recorded:?}"
    );
    assert!(!folder.join("config/auth.json").exists());
}

/// What a turn leaves for the next Codex to load (its settings, instructions) is gone before the
/// next run.
#[test]
fn what_codex_plants_for_its_next_run_is_taken_out() {
    let (_folder, obid) = obid("codex-conv-scrub", true);
    let planted = said(&obid, json!({"said": "PLANT", "new": true}));
    let conversation = planted.body["conversation"].as_str().unwrap().to_owned();
    assert_eq!(planted.body["text"], "planted", "{}", planted.text);
    let look = said(&obid, json!({"said": "LOOK", "conversation": conversation}));
    assert_eq!(look.body["text"], "look: nothing", "{}", look.text);
    // A HOME whose `.config` is a link to fish's config: the link goes.
    let planted = said(
        &obid,
        json!({"said": "PLANTLINK", "conversation": conversation}),
    );
    assert_eq!(planted.body["text"], "planted", "{}", planted.text);
    let look = said(&obid, json!({"said": "LOOK", "conversation": conversation}));
    assert_eq!(look.body["text"], "look: nothing", "{}", look.text);
}

/// The tokens of Codex's login and the turn's MCP token never reach what obid sends (F3, F6).
#[test]
fn codex_tokens_never_reach_what_obid_sends() {
    let (_folder, obid) = obid("codex-conv-leak", true);
    let (access, refresh, id, key) = (
        access_token(10 * 24 * 3600),
        format!("rt_{}", "b".repeat(40)),
        format!("eyJhbGciOiJSUzI1NiJ9.{}", "c".repeat(40)),
        format!("sk-proj-{}", "d".repeat(40)),
    );
    std::fs::write(
        obid.home.join("accounts/codex/auth.json"),
        json!({"auth_mode": "chatgpt", "OPENAI_API_KEY": key,
               "tokens": {"access_token": access, "refresh_token": refresh, "id_token": id, "account_id": "acct"}})
            .to_string(),
    )
    .unwrap();
    let events = follow_events(&obid);
    let answer = said(&obid, json!({"said": "LEAK", "new": true}));
    let seen = until_oby_turn(&events, 30);
    let sent = format!("{} {seen:?}", answer.text);
    assert!(sent.contains("leak: "), "{sent}");
    for token in [&access, &refresh, &id, &key] {
        assert!(!sent.contains(token.as_str()), "{token} in {sent}");
    }
    // The three login tokens in the copy and the turn's MCP token, each taken out (the copy has
    // no refresh token).
    let said_back = answer.body["text"].as_str().unwrap();
    assert_eq!(said_back.matches("[redacted: ").count(), 4, "{said_back}");
}

/// A command item is never part of a conversation: Codex's shell is off, and if one runs anyway
/// (a setting Codex stopped knowing) the turn ends at once, isn't tried again, its thread is never
/// picked up again, and obid says why (G1).
#[test]
fn a_command_ends_a_codex_turn() {
    let (_folder, obid) = obid("codex-conv-command", true);
    let answer = said(
        &obid,
        json!({"said": r#"RUN OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#,
               "new": true}),
    );
    let conversation = answer.body["conversation"].as_str().unwrap().to_owned();
    assert!(
        words(&answer).ends_with(
            "I couldn't answer: Codex ran a command (echo hi), which a conversation never does, so obid stopped it."
        ),
        "{}",
        answer.text
    );
    assert_eq!(turns(&obid).len(), 1, "not tried again");
    let jobs = call(&obid, "GET", "/v1/jobs", None);
    assert_eq!(jobs.body.as_array().map_or(0, Vec::len), 0, "{}", jobs.text);
    // Its thread is never picked up again, but the taint stays (finding 5): work the next turn
    // plans is marked untrusted.
    let kept: Value = serde_json::from_slice(
        &std::fs::read(
            obid.home
                .join(format!("conversations/{conversation}/session.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(kept["session"], "", "{kept}");
    assert_eq!(kept["web"], true, "{kept}");
    let next = said(
        &obid,
        json!({"said": r#"OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#,
               "conversation": conversation}),
    );
    assert_untrusted(&obid, &next);
    assert!(turns(&obid)[1]["resumed"].is_null());
}

/// The one job `answer` started after its turn read something untrusted: started at once, and
/// marked so (the owner's decision of 2026-10-23), in the owner's mode (2026-10-27); it publishes by itself
/// as every job does (the owner's decision of 2026-10-24).
fn assert_untrusted(obid: &Obid, answer: &Answer) {
    let cards = answer.body["jobs"].as_array().unwrap();
    assert_eq!(cards.len(), 1, "{}", answer.text);
    let id = cards[0]["id"].as_str().unwrap();
    let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
    assert_eq!(job["untrusted"], true, "{job}");
    assert_eq!(
        inbox(obid).iter().filter(|i| i["kind"] == "start").count(),
        0
    );
}

/// However a command, a hook or another server's tool shows up (a helper's thread, output with no
/// item before it, a hook, an MCP call to a server that isn't Oby's), the turn ends (findings 2-4).
#[test]
fn whatever_runs_outside_obys_tools_ends_a_codex_turn() {
    let (_folder, obid) = obid("codex-conv-fatal", true);
    for (words_said, why) in [
        ("HELPERRUN", "Codex ran a command (echo helper)"),
        ("OUTPUT", "Codex ran a command"),
        ("HOOK", "Codex ran a hook"),
        ("ELSEWHERE", "Codex called elsewhere's run"),
        // Anything that isn't on the list of what a conversation's Codex may do, new kinds too.
        ("ITEM imageGeneration", "Codex used imageGeneration"),
        ("ITEM collabAgentToolCall", "Codex used collabAgentToolCall"),
        ("ITEM subAgentActivity", "Codex used subAgentActivity"),
        ("ITEM dynamicToolCall", "Codex used dynamicToolCall"),
        ("ITEM somethingNew", "Codex used somethingNew"),
        (
            "NOTICE process/outputDelta",
            "Codex sent process/outputDelta",
        ),
        (
            "NOTICE somethingNew/started",
            "Codex sent somethingNew/started",
        ),
    ] {
        let answer = said(&obid, json!({"said": words_said, "new": true}));
        assert!(
            words(&answer).ends_with(&format!(
                "I couldn't answer: {why}, which a conversation never does, so obid stopped it."
            )),
            "{words_said}: {}",
            answer.text
        );
    }
}

/// What's on the list ends no turn: thinking, a plan, a compaction, a pause, the owner's words.
#[test]
fn what_a_conversation_may_do_ends_no_codex_turn() {
    let (_folder, obid) = obid("codex-conv-allowed", true);
    for kind in [
        "reasoning",
        "plan",
        "contextCompaction",
        "sleep",
        "userMessage",
    ] {
        let answer = said(&obid, json!({"said": format!("ITEM {kind}"), "new": true}));
        assert_eq!(answer.body["text"], "item ran", "{kind}: {}", answer.text);
    }
    let answer = said(
        &obid,
        json!({"said": "NOTICE thread/tokenUsage/updated", "new": true}),
    );
    assert_eq!(answer.body["text"], "notice sent", "{}", answer.text);
}

/// A Codex that keeps a feature on whatever obid sets (`features list`, run in the conversation's
/// own box, says so) never runs a conversation: its shell (G1), its hooks (finding 4).
#[test]
fn codex_that_keeps_a_feature_on_is_refused() {
    for (bin, feature) in [
        ("fake-codex-on-shell_tool", "shell_tool"),
        ("fake-codex-on-hooks", "hooks"),
        ("fake-codex-on-image_generation", "image_generation"),
    ] {
        let (_folder, obid) = obid_with(&format!("codex-conv-{feature}"), true, bin, None);
        let answer = said(&obid, json!({"said": "HELLO", "new": true}));
        assert!(
            words(&answer).ends_with(&format!(
                "I couldn't answer: Codex's features list says {feature} is on whatever obid sets, so a conversation doesn't run on it."
            )),
            "{}",
            answer.text
        );
        assert!(turns(&obid).is_empty());
    }
}

/// The login copy never carries the refresh token, which the provider rotates (G2); a login whose
/// access token ends within 5 minutes is refused with the one line that says so.
#[test]
fn codex_login_copies_have_no_refresh_token() {
    let login = |seconds| {
        json!({"auth_mode": "chatgpt", "last_refresh": "2026-10-12T23:13:02Z",
               "tokens": {"access_token": access_token(seconds), "refresh_token": format!("rt_{}", "r".repeat(40)),
                          "id_token": access_token(seconds), "account_id": "acct"}})
    };
    let (_folder, obid) = obid_with(
        "codex-conv-copy",
        true,
        "fake-codex-harness",
        Some(login(10 * 24 * 3600)),
    );
    let copy = said(&obid, json!({"said": "COPY", "new": true}));
    assert_eq!(
        copy.body["text"], "copy: access=yes refresh=no",
        "{}",
        copy.text
    );

    std::fs::write(
        obid.home.join("accounts/codex/auth.json"),
        login(60).to_string(),
    )
    .unwrap();
    let ending = said(&obid, json!({"said": "COPY", "new": true}));
    assert!(
        words(&ending).ends_with(
            "I couldn't answer: The Mac's Codex login for Codex stub needs refreshing: run codex once."
        ),
        "{}",
        ending.text
    );
    assert_eq!(turns(&obid).len(), 1);
}

/// Work planned after Codex searched the web starts at once, marked untrusted, as on Claude Code.
#[test]
fn work_codex_plans_after_the_web_starts_marked_untrusted() {
    let (_folder, obid) = obid("codex-conv-web", true);
    let answer = said(
        &obid,
        json!({"said": r#"WEB OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#,
               "new": true}),
    );
    assert_untrusted(&obid, &answer);
}

/// #1686 on Codex: a call that reaches Oby's tools before obid has read the web search Codex
/// reported first waits for its own item, and the work it plans starts marked untrusted.
#[test]
fn a_codex_call_waits_for_the_reader_and_sees_the_web_before_it() {
    let (_folder, obid) = obid("codex-conv-late-web", true);
    let at = Instant::now();
    let answer = said(
        &obid,
        json!({"said": r#"LATEWEB OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#,
               "new": true}),
    );
    assert_untrusted(&obid, &answer);
    // Its own item came, so it never waited out obid's cap (5 s).
    assert!(at.elapsed() < Duration::from_secs(4), "{:?}", at.elapsed());
}

/// Later turns resume the conversation's own thread, also after obid restarts.
#[test]
fn a_codex_conversation_resumes_its_thread() {
    let (folder, obid) = obid("codex-conv-resume", true);
    let first = said(&obid, json!({"said": "HELLO", "new": true}));
    let conversation = first.body["conversation"].as_str().unwrap().to_owned();
    let session = obid
        .home
        .join(format!("conversations/{conversation}/session.json"));
    let kept: Value = serde_json::from_slice(&std::fs::read(&session).unwrap()).unwrap();
    let id = kept["session"].as_str().unwrap().to_owned();
    assert_eq!(kept["account"], "cstub");
    let again = said(
        &obid,
        json!({"said": "REMEMBER", "conversation": conversation}),
    );
    assert_eq!(again.body["text"], format!("resume={id}"), "{}", again.text);
    let config: Value =
        serde_json::from_slice(&std::fs::read(folder.0.join("config.json")).unwrap()).unwrap();
    drop(obid);
    let restarted = start(&folder.0, &config, None);
    let after = said(
        &restarted,
        json!({"said": "REMEMBER", "conversation": conversation}),
    );
    assert_eq!(after.body["text"], format!("resume={id}"), "{}", after.text);
    let other = said(&restarted, json!({"said": "REMEMBER", "new": true}));
    assert_eq!(other.body["text"], "resume=none", "{}", other.text);
}

/// Codex gets the same Oby tools: oby_dispatch, approved by obid's rule, starts a job whose card
/// is on the turn.
#[test]
fn oby_dispatch_through_codex_starts_a_job() {
    let (_folder, obid) = obid("codex-conv-dispatch", true);
    let events = follow_events(&obid);
    let tools = said(&obid, json!({"said": "TOOLS", "new": true}));
    let conversation = tools.body["conversation"].as_str().unwrap().to_owned();
    assert_eq!(
        tools.body["text"],
        "tools: oby_jobs,oby_job,oby_dispatch,oby_offer,oby_tell_job,oby_stop_job,oby_continue_job,oby_archive_job,oby_delete_job,oby_change_job,oby_tasks,oby_inbox,oby_github,oby_answer,oby_merge,oby_github_write,oby_settings,oby_routines,oby_routine,oby_change_routine,oby_check_in,oby_room_read,oby_room_post,oby_conversations,oby_delete_conversations,oby_members,oby_ask,oby_asks,oby_artifact,oby_read,oby_search,oby_list,oby_shell,oby_shell_output,oby_shell_stop",
        "{}",
        tools.text
    );
    let _ = until_oby_turn(&events, 30);

    let answer = said(
        &obid,
        json!({"said": r#"OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build", "title": "Greeting"}"#,
               "conversation": conversation}),
    );
    let jobs = answer.body["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1, "{}", answer.text);
    assert_eq!(jobs[0]["conversation"], conversation.as_str());
    let id = jobs[0]["id"].as_str().unwrap();
    let seen = until_oby_turn(&events, 30);
    let (_, turn) = seen.last().unwrap();
    assert_eq!(turn["jobs"], json!([id]));
    let dispatch: Vec<&Value> = seen
        .iter()
        .filter(|(n, d)| n == "step" && d["tool"] == "oby_dispatch")
        .map(|(_, d)| d)
        .collect();
    assert_eq!(dispatch.len(), 2, "{seen:?}");
    assert_eq!(dispatch[0]["detail"], "obi-e2e/fixture-app");
    assert_eq!(dispatch[1]["state"], "done");

    // #660's oby_artifact is Codex's too: a folder of the room becomes this conversation's artifact.
    let page = said(
        &obid,
        json!({"said": r#"MAKEPAGE OBY oby_artifact {"title": "Functions", "path": "page"}"#,
               "conversation": conversation}),
    );
    let text = page.body["text"].as_str().unwrap();
    assert!(
        text.starts_with("oby: Published artifact ") && text.ends_with(", version 1."),
        "{}",
        page.text
    );
    let listed = call(
        &obid,
        "GET",
        &format!("/v1/artifacts?conversation={conversation}"),
        None,
    );
    assert_eq!(
        listed.body.as_array().map_or(0, Vec::len),
        1,
        "{}",
        listed.text
    );

    // An approval for a tool that wasn't called is declined.
    let misnamed = said(
        &obid,
        json!({"said": "MISNAMED", "conversation": conversation}),
    );
    assert_eq!(
        misnamed.body["text"], "misnamed: declined (decline)",
        "{}",
        misnamed.text
    );
}

/// Codex reads through Oby's tools (its shell is off): oby_read of a repository's checkout gives
/// its lines, fenced as data, shows as a `read` step naming the file, and taints the turn, as
/// Claude Code's Read does; a path out of the room (its CODEX_HOME's login copy) is refused and
/// taints nothing.
#[test]
fn codex_reads_the_room_through_oby_read() {
    let (_folder, obid) = obid("codex-conv-read", true);
    let events = follow_events(&obid);
    let answer = said(
        &obid,
        json!({"said": r#"OBY oby_read {"path": "repos/obi-e2e/fixture-app/README.md"}"#, "new": true}),
    );
    let conversation = answer.body["conversation"].as_str().unwrap().to_owned();
    let text = answer.body["text"].as_str().unwrap();
    assert!(
        text.starts_with("oby: Everything between a line <<<quoted-"),
        "{}",
        answer.text
    );
    assert!(text.contains("     1\t# Fixture app\n"), "{}", answer.text);
    let seen = until_oby_turn(&events, 30);
    let read: Vec<&Value> = seen
        .iter()
        .filter(|(n, d)| n == "step" && d["tool"] == "read")
        .map(|(_, d)| d)
        .collect();
    assert_eq!(read.len(), 2, "{seen:?}");
    assert_eq!(read[0]["detail"], "repos/obi-e2e/fixture-app/README.md");
    assert_eq!(read[0]["text"], "Reading README.md");
    assert_eq!(read[1]["state"], "done");
    let kept = |conversation: &str| -> Value {
        serde_json::from_slice(
            &std::fs::read(
                obid.home
                    .join(format!("conversations/{conversation}/session.json")),
            )
            .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(
        kept(&conversation)["web"],
        true,
        "reading repos/ taints the turn"
    );

    let prying = said(
        &obid,
        json!({"said": r#"OBY oby_read {"path": "../config/auth.json"}"#, "new": true}),
    );
    // The refusal names the agent's own path, so it comes quoted too.
    let refused = prying.body["text"].as_str().unwrap();
    assert!(
        refused.starts_with("oby: refused: Everything between a line <<<quoted-")
            && refused.contains(
                "\n../config/auth.json isn't a path in your room: name one relative to it, with no .. and no ~.\nquoted-"
            ),
        "{}",
        prying.text
    );
    let other = prying.body["conversation"].as_str().unwrap();
    assert_eq!(kept(other)["web"], false);
}

/// Codex's approvals are answered by the conversation's own rule: no command at all, not even one
/// in the room (F1); a write inside the room yes, outside and in repos/ no; never the owner's to
/// answer.
#[test]
fn codex_approvals_follow_the_conversation_rule() {
    let (_folder, obid) = obid("codex-conv-perms", true);
    let perms = said(&obid, json!({"said": "PERMS", "new": true}));
    assert_eq!(
        perms.body["text"],
        "perms: inside=decline write=accept outside=decline repos=decline secret=decline",
        "{}",
        perms.text
    );
    assert_eq!(
        inbox(&obid),
        Vec::<Value>::new(),
        "the owner is never asked mid-turn"
    );
}

/// A Codex turn's run is kept as the brain's with the tokens Codex's `tokenUsage` reported: the
/// input not read from the cache, the cache's, and the output.
#[test]
fn a_codex_turns_usage_is_kept_from_its_token_usage() {
    let (_folder, obid) = obid("codex-conv-usage", true);
    let answer = said(&obid, json!({"said": "USAGE", "new": true}));
    assert_eq!(words(&answer), "used", "{}", answer.text);
    let record: Value = std::fs::read_to_string(obid.home.join("brain-turns.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .rfind(|r| r["kind"] == "conversation")
        .expect("the turn's record");
    let call = &record["calls"][0];
    assert_eq!(call["adapter"], "codex", "{record}");
    assert_eq!(
        call["model"], "gpt-6.1-sol",
        "status.models default: {record}"
    );
    assert_eq!(call["input_tokens"], 20, "{record}");
    assert_eq!(call["cache_read_input_tokens"], 100, "{record}");
    assert_eq!(call["output_tokens"], 9, "{record}");
}

/// With no Codex account signed in, or a thread that fails twice, the turn says why in one line.
#[test]
fn a_codex_turn_says_why_when_codex_cant_run() {
    let (_folder, obid) = obid("codex-conv-fallback", false);
    let answer = said(&obid, json!({"said": "HELLO", "new": true}));
    assert_eq!(
        words(&answer),
        "I couldn't answer: no Codex account is signed in for conversations.",
        "{}",
        answer.text
    );
    assert!(turns(&obid).is_empty());

    std::fs::write(obid.home.join("accounts/codex/auth.json"), "{}").unwrap();
    let failed = said(&obid, json!({"said": "FAIL", "new": true}));
    assert_eq!(
        words(&failed),
        "I couldn't answer: its agent session failed twice.",
        "{}",
        failed.text
    );
    assert_eq!(turns(&obid).len(), 2);
}

/// Codex's own error ends the turn, whatever the agent said before it: words that name a limit mark
/// no account full, and the try stays on its thread (RECOVERY-7, CONVERSATION-AGENT-15).
#[test]
fn recovery_7_the_agents_own_words_never_say_a_limit() {
    let (_folder, obid) = obid("codex-words", true);
    let first = said(&obid, json!({"said": "HELLO", "new": true}));
    let id = first.body["conversation"].as_str().unwrap().to_owned();
    let answer = said(&obid, json!({"said": "LIMIT WORDS", "conversation": id}));
    assert!(
        words(&answer).starts_with("carried on: resume="),
        "{}",
        answer.text
    );
    assert!(!words(&answer).ends_with("none"), "{}", answer.text);
}

/// Codex's full context window, on a thread with history: a new thread goes on, and on a new one it
/// isn't tried again (CONVERSATION-AGENT-16).
#[test]
fn conversation_agent_16_codex_out_of_room_carries_on_in_a_new_thread() {
    let (_folder, obid) = obid("codex-room", true);
    let first = said(&obid, json!({"said": "HELLO", "new": true}));
    let id = first.body["conversation"].as_str().unwrap().to_owned();
    let answer = said(&obid, json!({"said": "OUT OF ROOM", "conversation": id}));
    assert_eq!(words(&answer), "new thread", "{}", answer.text);
    let all = turns(&obid);
    assert!(all.last().unwrap()["resumed"].is_null(), "{all:?}");
}

/// The `-c` settings a turn's Codex was started with.
fn settings(turn: &Value) -> Vec<String> {
    let argv: Vec<&str> = turn["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    argv.windows(2)
        .filter(|w| w[0] == "-c")
        .map(|w| w[1].to_owned())
        .collect()
}

/// LANES-13 (docs/contracts/lanes.md, the owner's CODEX-2, "Same as jobs"): on macOS, the owner's
/// Codex conversation in Bypass runs as an own Bypass Codex job does (LANES-18): on the account's
/// own folder as `CODEX_HOME`, with no login copy, its shell on and its commands in Codex's own
/// sandbox under obid's profile, with no network, writing only its room (never `repos/`) and its
/// temporary folder, `approval_policy` `never`, no hooks, no `notify`, the room an untrusted
/// project, and Codex asked which settings hold before each thread. A command runs and the turn
/// goes on; what it read marks the conversation, which stays own (CONVO-1): the job it starts is
/// `untrusted`, and its next turn resumes the same thread, in the same profile.
#[cfg(target_os = "macos")]
#[test]
fn lanes_13_an_own_bypass_codex_conversation_runs_its_shell_in_codexs_sandbox() {
    let (folder, obid) = obid("codex-conv-own", true);
    mode(&obid, "bypass");
    let answer = said(
        &obid,
        json!({"said": r#"RUN OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#,
               "new": true}),
    );
    assert_untrusted(&obid, &answer);
    let conversation = answer.body["conversation"].as_str().unwrap().to_owned();
    let again = said(
        &obid,
        json!({"said": "REMEMBER", "conversation": conversation}),
    );
    assert!(words(&again).starts_with("resume="), "{}", again.text);
    assert_ne!(words(&again), "resume=none", "{}", again.text);
    let account = folder.0.join("home/accounts/codex");
    let place = obid.home.join(format!("conversations/{conversation}"));
    let real = |p: PathBuf| json!(p.canonicalize().unwrap()).to_string();
    let recorded = turns(&obid);
    assert_eq!(recorded.len(), 2, "{recorded:?}");
    for turn in &recorded {
        assert_eq!(
            turn["codexHome"].as_str().map(PathBuf::from),
            Some(account.clone()),
            "{turn}"
        );
        assert_eq!(turn["approvalPolicy"], "never", "{turn}");
        assert_eq!(turn["permissions"], "oby_own", "{turn}");
        assert!(turn["sandbox"].is_null(), "{turn}");
        let set = settings(turn);
        for want in [
            r#"approval_policy="never""#.to_owned(),
            r#"default_permissions="oby_own""#.into(),
            "permissions.oby_own.network.enabled=false".into(),
            "features.hooks=false".into(),
            "features.shell_snapshot=false".into(),
            "notify=[]".into(),
            format!(
                "projects={{{} = {{trust_level = \"untrusted\"}}}}",
                real(place.join("room"))
            ),
        ] {
            assert!(set.contains(&want), "{want}: {set:?}");
        }
        assert!(!set.contains(&"features.shell_tool=false".to_owned()));
        let profile = set
            .iter()
            .find_map(|s| s.strip_prefix("permissions.oby_own.filesystem="))
            .unwrap();
        for (path, access) in [
            (place.join("room"), "write"),
            (place.join("room/repos"), "read"),
            (place.join("home"), "write"),
            (obid.home.clone(), "none"),
            (account.clone(), "none"),
        ] {
            let entry = format!("{} = \"{access}\"", real(path));
            assert!(profile.contains(&entry), "{entry} in {profile}");
        }
    }
    // No login copy: Codex reads and refreshes the account's own.
    assert!(!place.join("config/auth.json").exists());
    // Codex was asked which settings hold, for the room, before each thread.
    let asked = std::fs::read_to_string(account.join("asked.jsonl")).unwrap();
    assert_eq!(asked.lines().count(), 2, "{asked}");
    for line in asked.lines() {
        let cwd = serde_json::from_str::<Value>(line).unwrap()["cwd"].clone();
        assert_eq!(cwd, json!(place.join("room")), "{asked}");
    }
}

/// LANES-13: the owner's Codex conversation in any other mode stays in Oby's box, as before: its
/// `CODEX_HOME` the conversation's own with a login copy, `danger-full-access` with every approval
/// obid's, its shell off and Codex asked nothing first, so a command it runs anyway ends the turn.
#[test]
fn lanes_13_an_ask_codex_conversation_stays_boxed() {
    let (folder, obid) = obid("codex-conv-ask", true);
    let answer = said(&obid, json!({"said": "RUN HELLO", "new": true}));
    assert!(
        words(&answer).ends_with(
            "Codex ran a command (echo hi), which a conversation never does, so obid stopped it."
        ),
        "{}",
        answer.text
    );
    let conversation = answer.body["conversation"].as_str().unwrap();
    let recorded = turns(&obid);
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(
        recorded[0]["codexHome"].as_str().map(PathBuf::from),
        Some(
            obid.home
                .join(format!("conversations/{conversation}/config"))
        )
    );
    assert_eq!(recorded[0]["sandbox"], "danger-full-access");
    assert_eq!(recorded[0]["approvalPolicy"], "untrusted");
    let set = settings(&recorded[0]);
    assert!(
        set.contains(&"features.shell_tool=false".to_owned()),
        "{set:?}"
    );
    assert!(
        !set.iter().any(|s| s.starts_with("approval_policy=")),
        "{set:?}"
    );
    assert!(!folder.0.join("home/accounts/codex/asked.jsonl").exists());
}

/// LANES-13 (CONVO-1): every command an own Codex conversation runs is an untrusted read, one Codex
/// shows as a read of a file outside the room, one whose output alone obid saw, and an item obid
/// doesn't know, too: the mark
/// outlives a restart, and the job a later turn starts is `untrusted`.
#[cfg(target_os = "macos")]
#[test]
fn lanes_13_an_own_codex_conversations_read_marks_it() {
    for (case, (said_first, answered)) in [
        ("READ /private/tmp/notes.md DONE", "Nothing to do."),
        ("OUTPUT", "output came"),
        ("ITEM somethingNew", "item ran"),
    ]
    .into_iter()
    .enumerate()
    {
        let (folder, mut obid) = obid(&format!("codex-conv-own-read-{case}"), true);
        mode(&obid, "bypass");
        let first = said(&obid, json!({"said": said_first, "new": true}));
        assert_eq!(words(&first), answered, "{}", first.text);
        let conversation = first.body["conversation"].as_str().unwrap().to_owned();
        obid.stop();
        let config: Value =
            serde_json::from_slice(&std::fs::read(folder.0.join("config.json")).unwrap()).unwrap();
        let obid = start(&folder.0, &config, None);
        let answer = said(
            &obid,
            json!({"said": r#"OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#,
                   "conversation": conversation}),
        );
        assert_untrusted(&obid, &answer);
    }
}

/// LANES-13, as LANES-18's jobs (the owner's CODEX-2, "Same as jobs"): an own Codex turn asks obid
/// nothing, so leaving Bypass ends it within a second, saying why, and the next turn runs boxed,
/// in a new thread (the own one is in the account's `CODEX_HOME`).
#[cfg(target_os = "macos")]
#[test]
fn lanes_13_leaving_bypass_ends_an_own_codex_turn() {
    let (_folder, obid) = obid("codex-conv-own-leave", true);
    mode(&obid, "bypass");
    let obid = std::sync::Arc::new(obid);
    let (sent, answered) = std::sync::mpsc::channel();
    let asking = obid.clone();
    std::thread::spawn(move || sent.send(said(&asking, json!({"said": "SLEEP", "new": true}))));
    let give_up = Instant::now() + Duration::from_secs(30);
    while turns(&obid).is_empty() {
        assert!(Instant::now() < give_up, "the turn never started");
        std::thread::sleep(Duration::from_millis(100));
    }
    mode(&obid, "ask");
    let answer = answered
        .recv_timeout(Duration::from_secs(3))
        .expect("the turn went on after the owner left Bypass");
    assert!(
        words(&answer).ends_with("Your next message runs in Oby's box."),
        "{}",
        answer.text
    );
    let conversation = answer.body["conversation"].as_str().unwrap();
    let again = said(
        &obid,
        json!({"said": "REMEMBER", "conversation": conversation}),
    );
    assert_eq!(words(&again), "resume=none", "{}", again.text);
    let recorded = turns(&obid);
    assert_eq!(recorded.len(), 2, "{recorded:?}");
    assert_eq!(recorded[1]["sandbox"], "danger-full-access");
    assert_eq!(recorded[1]["approvalPolicy"], "untrusted");
}
