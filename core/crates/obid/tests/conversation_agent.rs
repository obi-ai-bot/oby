//! Which account a conversation talks through (docs/contracts/conversation-agent.md): any connected
//! account of any tool, the owner's choice or Auto, moving by itself when one fills. The agents are
//! stand-ins (tests/agents/fake-harness, fake-codex-harness, fake-cursor-harness).
mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

fn agent(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/agents")
        .join(name)
}

const CLAUDE: &str = "stub";
const CODEX: &str = "cstub";
const CURSOR: &str = "ustub";

/// The configuration of obid with one signed-in account of each tool.
fn configured(folder: &Folder) -> Value {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agent("fake-harness"));
    config["conversation"] = json!({"agent": "claude"});
    let accounts = folder.0.join("home/accounts");
    let ends = (obi_protocol::time::now() + 10 * 86_400) * 1000;
    let claude = accounts.join(CLAUDE);
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join(".credentials.json"),
        json!({"claudeAiOauth": {"accessToken": "tok-stub-plain-test-login", "expiresAt": ends}})
            .to_string(),
    )
    .unwrap();
    let codex = accounts.join(CODEX);
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("auth.json"), "{}").unwrap();
    config["agents"]["codex"] = json!({"bin": agent("fake-codex-harness"),
        "accounts": [{"id": CODEX, "name": "Codex stub", "configDir": codex}]});
    let cursor = accounts.join(CURSOR);
    let login = if cfg!(target_os = "macos") {
        ".cursor/auth.json"
    } else {
        ".config/cursor/auth.json"
    };
    let file = cursor.join(login);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        file,
        json!({"accessToken": "cursor-access-plain-test-login", "refreshToken": "never-copied"})
            .to_string(),
    )
    .unwrap();
    config["agents"]["cursor"] = json!({"bin": agent("fake-cursor-harness"),
        "accounts": [{"id": CURSOR, "name": "Cursor stub", "configDir": cursor}]});
    config
}

fn obid(name: &str) -> (Folder, Obid) {
    let folder = Folder::new(name);
    let config = configured(&folder);
    let obid = start(&folder.0, &config, None);
    (folder, obid)
}

/// CONVERSATION-AGENT-22: usage reads also refresh each account's CLI model list; status adds
/// `models` when it differs; PATCH accepts an id from that list.
#[test]
fn conversation_agent_22_account_lists_come_from_the_cli_and_reach_status() {
    let folder = Folder::new("talk-model-lists");
    let config = configured(&folder);
    // A Codex login that `signed_in` accepts (JWT with a far-off exp).
    use base64::Engine as _;
    let ends = (obi_protocol::time::now() + 10 * 86_400) as i64;
    let claims =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{ends}}}"#));
    std::fs::write(
        folder.0.join("home/accounts").join(CODEX).join("auth.json"),
        json!({"tokens":{"access_token": format!("h.{claims}.s"), "refresh_token":"", "account_id":"a"}}).to_string(),
    )
    .unwrap();
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[
            ("OBID_USAGE_READS", "on"),
            ("OBID_USAGE_EVERY_SECONDS", "0"),
        ],
    );
    let account = |id: &str| {
        within(30, &format!("{id} models"), || {
            call(&obid, "GET", "/v1/status", None);
            std::thread::sleep(std::time::Duration::from_millis(100));
            call(&obid, "GET", "/v1/status", None).body["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == id)
                .and_then(|a| a.get("models").cloned())
        })
    };
    let codex = account(CODEX);
    assert!(
        codex
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "gpt-6-luna"),
        "{codex}"
    );
    assert!(
        !codex
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "gpt-hidden"),
        "{codex}"
    );
    let cursor = account(CURSOR);
    // AGENTS-6: each says its own list is known, so a client may judge a saved model against it.
    let status = call(&obid, "GET", "/v1/status", None).body;
    for id in [CODEX, CURSOR] {
        let row = status["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id);
        assert_eq!(
            row.map(|a| a["modelsRead"].clone()),
            Some(json!(true)),
            "{id}"
        );
    }
    assert!(
        cursor
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "gpt-5.6-sol-high"),
        "{cursor}"
    );
    let set = call(
        &obid,
        "PATCH",
        &format!("/v1/accounts/{CODEX}"),
        Some(&json!({"model": "gpt-6-luna"})),
    );
    assert_eq!(set.status, 200, "{}", set.text);
    assert_eq!(set.body["model"], "gpt-6-luna");
    let refused = call(
        &obid,
        "PATCH",
        &format!("/v1/accounts/{CODEX}"),
        Some(&json!({"model": "not-on-list"})),
    );
    assert_eq!(refused.status, 422, "{}", refused.text);
}

#[test]
fn conversation_agent_21_turns_use_the_accounts_model_and_fall_back_to_status_defaults() {
    let folder = Folder::new("talk-models");
    let config = configured(&folder);
    let mut obid = start(&folder.0, &config, None);
    let defaults = call(&obid, "GET", "/v1/status", None).body["models"].clone();
    let patch = |obid: &Obid, account: &str, model: Value| {
        let answer = call(
            obid,
            "PATCH",
            &format!("/v1/accounts/{account}"),
            Some(&json!({"model": model})),
        );
        assert_eq!(answer.status, 200, "{}", answer.text);
    };
    patch(&obid, CLAUDE, json!("claude-haiku-5-5"));
    patch(&obid, CODEX, json!("gpt-6-astra"));
    obid.stop();
    obid = start(&folder.0, &config, None);
    let status = call(&obid, "GET", "/v1/status", None).body;
    assert_eq!(
        status["models"], defaults,
        "account choices leave tool defaults intact"
    );
    for (account, tool, expected) in [
        (CLAUDE, "claude", "claude-haiku-5-5"),
        (CODEX, "codex", "gpt-6-astra"),
    ] {
        assert_eq!(
            status["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == account)
                .unwrap()["model"],
            expected
        );
        let set = call(
            &obid,
            "PUT",
            "/v1/settings/jobs",
            Some(&json!({"talkAccount": account})),
        );
        assert_eq!(set.status, 200, "{}", set.text);
        let answer = said(&obid, json!({"said": "HELLO", "new": true}));
        let id = conversation(&answer);
        let turns = recorded(
            &obid
                .home
                .join("conversations")
                .join(&id)
                .join("config/turns.jsonl"),
        );
        if tool == "claude" {
            let argv = turns.last().unwrap()["argv"].as_array().unwrap();
            let at = argv.iter().position(|a| a == "--model").unwrap();
            assert_eq!(argv[at + 1], expected);
        } else {
            assert_eq!(config_turns(&obid, &id).last().unwrap()["model"], expected);
        }
    }
    patch(&obid, CODEX, Value::Null);
    let answer = said(&obid, json!({"said": "HELLO", "new": true}));
    assert_eq!(
        config_turns(&obid, &conversation(&answer)).last().unwrap()["model"],
        defaults["codex"]["model"]
    );
}

/// CONVERSATION-AGENT-21, AGENTS-6: right after a restart no Codex list has been read, so a model
/// saved from one (off the fixed list) is still what its conversations talk on.
#[test]
fn conversation_agent_21_a_restart_keeps_a_saved_model_off_the_fixed_list() {
    let folder = Folder::new("talk-models-restart");
    let config = configured(&folder);
    let home = folder.0.join("home");
    std::fs::create_dir_all(&home).unwrap();
    obid::settings::update(&home, |s| {
        s.tuning.entry(CODEX.into()).or_default().model = Some("gpt-6-luna".into())
    })
    .unwrap();
    let obid = start(&folder.0, &config, None);
    let set = call(
        &obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"talkAccount": CODEX})),
    );
    assert_eq!(set.status, 200, "{}", set.text);
    let answer = said(&obid, json!({"said": "HELLO", "new": true}));
    assert_eq!(
        config_turns(&obid, &conversation(&answer)).last().unwrap()["model"],
        "gpt-6-luna"
    );
}

fn said(obid: &Obid, body: Value) -> Answer {
    let answer = call(obid, "POST", "/v1/plans", Some(&body));
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer
}

fn text(answer: &Answer) -> String {
    answer.body["text"].as_str().unwrap_or_default().to_owned()
}

fn conversation(answer: &Answer) -> String {
    answer.body["conversation"].as_str().unwrap().to_owned()
}

/// `conversation` talks through `account` from now on.
fn choose(obid: &Obid, conversation: &str, account: Value) -> Answer {
    let answer = call(
        obid,
        "PATCH",
        &format!("/v1/conversations/{conversation}"),
        Some(&json!({ "talkAccount": account })),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer
}

fn listed(obid: &Obid, id: &str) -> Value {
    call(obid, "GET", "/v1/conversations", None)
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .cloned()
        .unwrap()
}

fn recorded(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// What the Cursor stand-in recorded of each prompt, in its short folder under `logins/`.
fn cursor_turns(obid: &Obid) -> Vec<Value> {
    std::fs::read_dir(obid.home.join("logins"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("t-"))
        .flat_map(|e| recorded(&e.path().join("turns.jsonl")))
        .collect()
}

/// What Codex's stand-in recorded in `conversation`'s own config folder (Claude Code's records
/// there too, with no `app-server` in its arguments).
fn config_turns(obid: &Obid, conversation: &str) -> Vec<Value> {
    recorded(
        &obid
            .home
            .join("conversations")
            .join(conversation)
            .join("config/turns.jsonl"),
    )
    .into_iter()
    .filter(|t| {
        t["argv"]
            .as_array()
            .is_some_and(|argv| argv.last() == Some(&json!("app-server")))
    })
    .collect()
}

fn job(obid: &Obid, answer: &Answer) -> Value {
    let id = answer.body["jobs"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no job: {}", answer.text));
    call(obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

/// obid's usage file with each of `accounts` full for an hour, for it to read as it starts.
fn full_from_the_start(folder: &Folder, accounts: &[&str]) {
    let until = obi_protocol::time::now() + 3600;
    let usage: serde_json::Map<String, Value> = accounts
        .iter()
        .map(|a| ((*a).to_owned(), json!({"fullUntil": until})))
        .collect();
    std::fs::create_dir_all(folder.0.join("home")).unwrap();
    std::fs::write(
        folder.0.join("home/usage.json"),
        Value::Object(usage).to_string(),
    )
    .unwrap();
}

const DISPATCH: &str = r#"OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#;

/// The setting (Auto, or an account) and each conversation's own choice are the owner's to change,
/// outlive a restart, and take only an account there is. A conversation that chose none follows
/// the setting; `auto` there is Auto whatever the setting says.
#[test]
fn conversation_agent_1_the_setting_and_a_choice_outlive_a_restart() {
    let folder = Folder::new("talk-kept");
    let config = configured(&folder);
    let mut obid = start(&folder.0, &config, None);

    let status = call(&obid, "GET", "/v1/status", None).body;
    assert_eq!(status["talkAccount"], Value::Null, "Auto until they choose");
    assert_eq!(
        status["talkNext"], CLAUDE,
        "the configured tool comes first"
    );

    let put = |obid: &Obid, body: Value| call(obid, "PUT", "/v1/settings/jobs", Some(&body));
    let refused = put(&obid, json!({"talkAccount": "nobody"}));
    assert_eq!(refused.status, 422, "{}", refused.text);
    let set = put(&obid, json!({"talkAccount": CODEX}));
    assert_eq!(set.status, 200, "{}", set.text);
    assert_eq!(set.body["talkAccount"], CODEX);
    let status = call(&obid, "GET", "/v1/status", None).body;
    assert_eq!(status["talkAccount"], CODEX);
    assert_eq!(status["talkNext"], CODEX);

    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let id = conversation(&first);
    assert_eq!(listed(&obid, &id)["talkAccount"], Value::Null);
    assert_eq!(listed(&obid, &id)["talkedOn"], CODEX, "{}", first.text);

    let refused = call(
        &obid,
        "PATCH",
        &format!("/v1/conversations/{id}"),
        Some(&json!({"talkAccount": "nobody"})),
    );
    assert_eq!(refused.status, 422, "{}", refused.text);
    let chosen = choose(&obid, &id, json!(CLAUDE));
    assert_eq!(chosen.body["talkAccount"], CLAUDE, "{}", chosen.text);

    obid.stop();
    let obid = start(&folder.0, &config, None);
    assert_eq!(
        call(&obid, "GET", "/v1/settings/jobs", None).body["talkAccount"],
        CODEX
    );
    assert_eq!(listed(&obid, &id)["talkAccount"], CLAUDE);
    assert_eq!(listed(&obid, &id)["talkedOn"], CODEX);

    // Null follows the setting again, `auto` and null on the setting are Auto.
    assert_eq!(
        choose(&obid, &id, json!("auto")).body["talkAccount"],
        "auto"
    );
    assert_eq!(
        choose(&obid, &id, Value::Null).body["talkAccount"],
        Value::Null
    );
    assert_eq!(
        put(&obid, json!({"talkAccount": "auto"})).body["talkAccount"],
        Value::Null
    );
}

/// A choice takes effect at the next turn. Another agent's or account's session can't be picked up,
/// so the turn starts a new one with the earlier turns handed over, quoted, and says nothing about
/// moving: the owner did it. Auto then stays where the session is while that account has room.
#[test]
fn conversation_agent_2_a_choice_takes_effect_at_the_next_turn_with_the_earlier_turns() {
    let (_folder, obid) = obid("talk-choice");
    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    assert_eq!(text(&first), "resume=none", "{}", first.text);
    let id = conversation(&first);
    assert_eq!(listed(&obid, &id)["talkedOn"], CLAUDE);

    choose(&obid, &id, json!(CODEX));
    let second = said(&obid, json!({"said": "REMEMBER", "conversation": id}));
    assert_eq!(text(&second), "resume=none", "{}", second.text);
    assert_eq!(listed(&obid, &id)["talkedOn"], CODEX);
    let on_codex = config_turns(&obid, &id);
    let prompt = on_codex.last().unwrap()["prompt"].as_str().unwrap();
    assert!(prompt.contains("Oby: resume=none"), "{prompt}");

    // Back to Auto: it stays on Codex, in the same thread.
    choose(&obid, &id, json!("auto"));
    let third = said(&obid, json!({"said": "REMEMBER", "conversation": id}));
    assert!(text(&third).starts_with("resume="), "{}", third.text);
    assert_ne!(text(&third), "resume=none", "{}", third.text);

    choose(&obid, &id, json!(CLAUDE));
    let fourth = said(&obid, json!({"said": "REMEMBER", "conversation": id}));
    assert_eq!(text(&fourth), "resume=none", "{}", fourth.text);
    assert_eq!(listed(&obid, &id)["talkedOn"], CLAUDE);
}

/// The chosen account fills: Oby moves to another connected account, of any tool, says so in a
/// line, and the chat goes on there without saying it again. The owner's choice stays.
#[test]
fn conversation_agent_3_a_full_account_moves_to_another_tool_and_says_so() {
    let (_folder, obid) = obid("talk-full");
    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let id = conversation(&first);
    choose(&obid, &id, json!(CLAUDE));
    let moved = said(&obid, json!({"said": "FULL ON stub", "conversation": id}));
    assert_eq!(
        text(&moved),
        "Nothing to do.\nStub hit its limit, so I moved to Codex · Codex stub.",
        "{}",
        moved.text
    );
    assert_eq!(listed(&obid, &id)["talkedOn"], CODEX);
    assert_eq!(listed(&obid, &id)["talkAccount"], CLAUDE);
    assert!(
        config_turns(&obid, &id).last().unwrap()["prompt"]
            .as_str()
            .unwrap()
            .contains("Oby: resume=none"),
        "the next agent has the earlier turns"
    );
    let on = said(&obid, json!({"said": "SOMETHING", "conversation": id}));
    assert_eq!(text(&on), "Nothing to do.", "{}", on.text);
}

/// The chosen account can't run at all (signed out, say): Oby moves as well, and says so.
#[test]
fn conversation_agent_3_a_chosen_account_that_cant_run_moves_too() {
    let folder = Folder::new("talk-signed-out");
    let mut config = configured(&folder);
    config["agents"]["codex"]["accounts"][0]["configDir"] =
        json!(folder.0.join("home/accounts/nobody"));
    std::fs::create_dir_all(folder.0.join("home/accounts/nobody")).unwrap();
    let obid = start(&folder.0, &config, None);
    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let id = conversation(&first);
    choose(&obid, &id, json!(CODEX));
    // Its session stays where it was, since Codex can't run, and that's said, once.
    let second = said(&obid, json!({"said": "REMEMBER", "conversation": id}));
    assert_ne!(text(&second), "resume=none", "{}", second.text);
    assert!(
        text(&second).ends_with("\nCodex stub couldn't run, so I moved to Claude Code · Stub."),
        "{}",
        second.text
    );
    // The apps' pill follows it: it names where the answers come from, not the chosen one.
    assert_eq!(listed(&obid, &id)["movedOff"], CODEX);
    let third = said(&obid, json!({"said": "HELLO", "conversation": id}));
    assert_eq!(text(&third), "Hello there.", "{}", third.text);
    // Choosing again names the choice at once.
    choose(&obid, &id, json!(CODEX));
    assert!(listed(&obid, &id).get("movedOff").is_none());

    let fresh = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let fresh = conversation(&fresh);
    call(
        &obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"talkAccount": CODEX})),
    );
    let new = said(&obid, json!({"said": "REMEMBER", "new": true}));
    assert_eq!(
        text(&new),
        "resume=none\nCodex stub couldn't run, so I moved to Claude Code · Stub.",
        "{}",
        new.text
    );
    assert_ne!(fresh, conversation(&new));
}

/// A chosen account full before the turn starts is said once too, while the session stays elsewhere;
/// when it's where the answer comes from again, leaving it is news again.
#[test]
fn conversation_agent_3_choosing_a_full_account_says_so_once() {
    let folder = Folder::new("talk-chose-full");
    let config = configured(&folder);
    full_from_the_start(&folder, &[CODEX]);
    let obid = start(&folder.0, &config, None);
    let id = conversation(&said(&obid, json!({"said": "HELLO", "new": true})));
    choose(&obid, &id, json!(CODEX));
    let first = said(&obid, json!({"said": "HELLO", "conversation": id}));
    assert_eq!(
        text(&first),
        "Hello there.\nCodex stub hit its limit, so I moved to Claude Code · Stub.",
        "{}",
        first.text
    );
    let second = said(&obid, json!({"said": "HELLO", "conversation": id}));
    assert_eq!(text(&second), "Hello there.", "{}", second.text);
}

/// A turn that did something stays where it is: on another account, under the same words, it would
/// do it again. One that only read moves.
#[test]
fn conversation_agent_3_a_turn_that_started_a_job_stays_when_it_fills() {
    let (_folder, obid) = obid("talk-dispatched-full");
    let id = conversation(&said(&obid, json!({"said": "HELLO", "new": true})));
    let words = format!("REPEAT 1 {DISPATCH} THEN FULL ON stub");
    let answer = said(&obid, json!({"said": words, "conversation": id}));
    assert!(
        text(&answer).starts_with("I couldn't answer: "),
        "{}",
        answer.text
    );
    assert!(config_turns(&obid, &id).is_empty(), "Codex never ran");
    let jobs = call(&obid, "GET", "/v1/jobs", None);
    assert_eq!(jobs.body.as_array().unwrap().len(), 1, "{}", jobs.text);
}

#[test]
fn conversation_agent_3_a_turn_that_acted_stays_and_one_that_read_moves() {
    let (_folder, obid) = obid("talk-acted-full");
    let id = conversation(&said(&obid, json!({"said": "HELLO", "new": true})));
    let words = r#"REPEAT 1 OBY oby_settings {} THEN FAIL"#;
    let answer = said(&obid, json!({"said": words, "conversation": id}));
    assert!(
        text(&answer).starts_with("I couldn't answer: "),
        "{}",
        answer.text
    );
    assert!(config_turns(&obid, &id).is_empty(), "Codex never ran");

    let words = r#"REPEAT 1 OBY oby_jobs {} THEN FULL ON stub"#;
    let answer = said(&obid, json!({"said": words, "conversation": id}));
    assert!(
        text(&answer).ends_with("Stub hit its limit, so I moved to Codex · Codex stub."),
        "{}",
        answer.text
    );
    assert_eq!(config_turns(&obid, &id).len(), 1);
}

/// Auto goes where there's room, whatever the tools' order, and a turn it moves says so.
#[test]
fn conversation_agent_auto_goes_where_there_is_room() {
    let folder = Folder::new("talk-room");
    let config = configured(&folder);
    let now = obi_protocol::time::now();
    let window = |used: f64| {
        json!({"fiveHour": used, "sevenDay": used, "fiveHourResets": now + 3600,
               "sevenDayResets": now + 86_400, "fiveHourObservedAt": now, "sevenDayObservedAt": now})
    };
    std::fs::create_dir_all(folder.0.join("home")).unwrap();
    std::fs::write(
        folder.0.join("home/usage.json"),
        json!({CLAUDE: window(0.97), CODEX: window(0.05)}).to_string(),
    )
    .unwrap();
    let obid = start(&folder.0, &config, None);
    assert_eq!(
        call(&obid, "GET", "/v1/status", None).body["talkNext"],
        CODEX
    );
    let id = conversation(&said(&obid, json!({"said": "HELLO", "new": true})));
    assert_eq!(listed(&obid, &id)["talkedOn"], CODEX);
}

/// Nothing can run: the turn names what holds each signed-in account back, of every tool.
#[test]
fn conversation_agent_nothing_can_run_names_what_holds_each_account() {
    let folder = Folder::new("talk-none");
    let mut config = configured(&folder);
    config["agents"].as_object_mut().unwrap().remove("cursor");
    full_from_the_start(&folder, &[CLAUDE, CODEX]);
    let obid = start(&folder.0, &config, None);
    let answer = said(&obid, json!({"said": "HELLO", "new": true}));
    let words = text(&answer);
    assert!(
        words.contains("Claude Code · Stub hit its usage limit")
            && words.contains("Codex · Codex stub hit its usage limit"),
        "{words}"
    );
}

/// `accountFailover` off: one account only, across tools too; the turn says why.
#[test]
fn conversation_agent_4_failover_off_keeps_a_turn_on_one_account() {
    let (_folder, obid) = obid("talk-one");
    let off = call(
        &obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"accountFailover": false})),
    );
    assert_eq!(off.status, 200, "{}", off.text);
    let first = said(&obid, json!({"said": "FULL ON stub", "new": true}));
    let id = conversation(&first);
    assert!(
        text(&first).starts_with("I couldn't answer: Claude Code · Stub"),
        "{}",
        first.text
    );
    assert!(config_turns(&obid, &id).is_empty(), "Codex never ran");

    // The account they chose is full: nothing else is tried for them.
    choose(&obid, &id, json!(CLAUDE));
    let second = said(&obid, json!({"said": "REMEMBER", "conversation": id}));
    assert!(
        text(&second).starts_with("I couldn't answer: Claude Code · Stub"),
        "{}",
        second.text
    );
    assert!(config_turns(&obid, &id).is_empty(), "Codex never ran");
}

/// What a conversation read stays with it on another agent: its turns' work starts marked
/// untrusted there too.
#[test]
fn conversation_agent_5_what_it_read_goes_with_it_to_another_agent() {
    let (_folder, obid) = obid("talk-taint");
    let first = said(&obid, json!({"said": "WEB REMEMBER", "new": true}));
    let id = conversation(&first);
    choose(&obid, &id, json!(CODEX));
    let on_codex = said(&obid, json!({"said": DISPATCH, "conversation": id}));
    assert_eq!(
        job(&obid, &on_codex)["untrusted"],
        true,
        "{}",
        on_codex.text
    );
}

/// Cursor talks as the others do: in its room and box, one session per conversation picked up with
/// `session/load`, its words streaming (never what a load replays), told what it is in its first
/// prompt, with Oby's tools, and the turn's token only in `session/new`.
#[cfg(target_os = "macos")]
#[test]
fn conversation_agent_6_cursor_talks_with_obys_tools_and_keeps_its_session() {
    let (_folder, obid) = obid("talk-cursor");
    let events = follow_events(&obid);
    let first = said(&obid, json!({"said": "HELLO", "new": true}));
    let id = conversation(&first);
    assert_eq!(listed(&obid, &id)["talkedOn"], CLAUDE);
    choose(&obid, &id, json!(CURSOR));

    let hello = said(&obid, json!({"said": "HELLO", "conversation": id}));
    assert_eq!(text(&hello), "Hello there.", "{}", hello.text);
    assert_eq!(listed(&obid, &id)["talkedOn"], CURSOR);
    let tools = said(&obid, json!({"said": "TOOLS", "conversation": id}));
    assert!(text(&tools).contains("oby_dispatch"), "{}", tools.text);
    let remember = said(&obid, json!({"said": "REMEMBER", "conversation": id}));
    assert!(text(&remember).starts_with("resume="), "{}", remember.text);
    assert_ne!(text(&remember), "resume=none", "{}", remember.text);

    let turns = cursor_turns(&obid);
    assert_eq!(turns.len(), 3, "{turns:?}");
    assert_eq!(
        turns[0]["argv"],
        json!(["--sandbox", "disabled", "acp"]),
        "{turns:?}"
    );
    let prompt = turns[0]["prompt"].as_str().unwrap();
    assert!(prompt.contains("You are Oby"), "{prompt}");
    assert!(prompt.contains("You run on Cursor here"), "{prompt}");
    assert!(
        prompt.contains("Owner: HELLO"),
        "the earlier turns: {prompt}"
    );
    assert_eq!(turns[0]["resumed"], Value::Null);
    let later = turns[1]["prompt"].as_str().unwrap();
    assert!(!later.contains("You are Oby"), "{later}");
    assert_eq!(turns[0]["servers"], json!(["oby"]));
    assert_eq!(turns[0]["serverHeaders"], json!(["Authorization"]));
    assert_eq!(turns[0]["env"], json!([]), "no token in its environment");
    assert!(
        turns[0]["home"]
            .as_str()
            .unwrap()
            .contains("/home/logins/t-"),
        "{turns:?}"
    );

    let mut streamed = String::new();
    for (name, data) in events.try_iter() {
        if name == "delta" {
            streamed.push_str(data["text"].as_str().unwrap_or(""));
        }
    }
    assert!(streamed.contains("Hello there."), "{streamed}");
    assert!(!streamed.contains("REPLAYED"), "{streamed}");
}

/// A Cursor that never started read nothing: the conversation keeps no mark from it, and the turn
/// moves on. One that started, signed or not, marks it.
#[cfg(target_os = "macos")]
#[test]
fn conversation_agent_7_a_cursor_that_never_started_leaves_no_mark() {
    let (_folder, obid) = obid("talk-cursor-no-mark");
    let foreign = |id: &str| {
        let file = obid.home.join("conversations").join(format!("{id}.json"));
        serde_json::from_slice::<Value>(&std::fs::read(file).unwrap()).unwrap()["foreign"].clone()
    };
    let first = call_typed(
        &obid,
        "POST",
        "/v1/plans",
        &json!({"said": "HELLO", "new": true}),
    );
    assert_eq!(first.status, 200, "{}", first.text);
    let id = conversation(&first);
    assert_eq!(foreign(&id), Value::Null);
    let room = obid.home.join("conversations").join(&id).join("room");
    std::fs::write(room.join("no-session"), "").unwrap();
    choose(&obid, &id, json!(CURSOR));
    let words = json!({"said": "HELLO", "conversation": id});
    let moved = call_typed(&obid, "POST", "/v1/plans", &words);
    assert!(
        text(&moved).ends_with("Cursor stub couldn't run, so I moved to Claude Code · Stub."),
        "{}",
        moved.text
    );
    assert_eq!(foreign(&id), Value::Null, "{}", moved.text);

    std::fs::remove_file(room.join("no-session")).unwrap();
    let on = call_typed(&obid, "POST", "/v1/plans", &words);
    assert_eq!(listed(&obid, &id)["talkedOn"], CURSOR, "{}", on.text);
    assert!(foreign(&id).as_str().unwrap().contains("Cursor"));
}

/// Moved onto Cursor with nobody choosing it: the line says what stays with the conversation.
#[cfg(target_os = "macos")]
#[test]
fn conversation_agent_7_reaching_cursor_unasked_says_what_it_costs() {
    let folder = Folder::new("talk-cursor-unasked");
    let mut config = configured(&folder);
    config["agents"].as_object_mut().unwrap().remove("codex");
    let obid = start(&folder.0, &config, None);
    let id = conversation(&said(&obid, json!({"said": "HELLO", "new": true})));
    let moved = said(&obid, json!({"said": "FULL ON stub", "conversation": id}));
    assert_eq!(
        text(&moved),
        "Nothing to do.\nStub hit its limit, so I moved to Cursor · Cursor stub.\nOn Cursor, what you ask through my tools needs your OK from now on in this conversation.",
        "{}",
        moved.text
    );
    let on = said(&obid, json!({"said": "HELLO", "conversation": id}));
    assert_eq!(text(&on), "Hello there.", "{}", on.text);
}

/// A Cursor that can't load its session starts a new one with the earlier turns handed over, never
/// one that knows only the owner's last words.
#[cfg(target_os = "macos")]
#[test]
fn conversation_agent_6_a_cursor_with_no_session_load_gets_the_earlier_turns() {
    let (_folder, obid) = obid("talk-cursor-no-load");
    let id = conversation(&said(&obid, json!({"said": "HELLO", "new": true})));
    let room = obid.home.join("conversations").join(&id).join("room");
    std::fs::write(room.join("no-load-session"), "").unwrap();
    choose(&obid, &id, json!(CURSOR));
    said(&obid, json!({"said": "HELLO", "conversation": id}));
    said(&obid, json!({"said": "THIRD", "conversation": id}));
    let turns = cursor_turns(&obid);
    assert_eq!(turns.len(), 2, "{turns:?}");
    let later = turns[1]["prompt"].as_str().unwrap();
    assert!(later.contains("You are Oby"), "{later}");
    assert!(later.contains("Owner: HELLO"), "{later}");
}

/// What Cursor's own tools read obid can't see: a turn on it counts as having read something
/// untrusted from its start, and that stays with the conversation once it leaves Cursor.
#[cfg(target_os = "macos")]
#[test]
fn conversation_agent_7_cursor_counts_as_untrusted_from_its_first_turn() {
    let (_folder, obid) = obid("talk-cursor-taint");
    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let id = conversation(&first);
    choose(&obid, &id, json!(CURSOR));
    let dispatched = said(&obid, json!({"said": DISPATCH, "conversation": id}));
    assert_eq!(
        job(&obid, &dispatched)["untrusted"],
        true,
        "{}",
        dispatched.text
    );
    choose(&obid, &id, json!(CLAUDE));
    let back = said(&obid, json!({"said": DISPATCH, "conversation": id}));
    assert_eq!(job(&obid, &back)["untrusted"], true, "{}", back.text);
}

/// A conversation's Cursor gets out through obid's proxy only, and it lets through only Cursor's
/// own services: what it read has nowhere to go.
#[cfg(target_os = "macos")]
#[test]
fn conversation_agent_8_cursor_reaches_only_its_own_services() {
    let (_folder, obid) = obid("talk-cursor-egress");
    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let id = conversation(&first);
    choose(&obid, &id, json!(CURSOR));
    let elsewhere = said(
        &obid,
        json!({"said": "REACH example.com", "conversation": id}),
    );
    assert_eq!(text(&elsewhere), "reach: 403", "{}", elsewhere.text);
    let own = said(
        &obid,
        json!({"said": "REACH api2.cursor.sh", "conversation": id}),
    );
    assert_ne!(text(&own), "reach: 403", "{}", own.text);
    assert_ne!(text(&own), "reach: no proxy", "{}", own.text);
    let direct = said(&obid, json!({"said": "DIRECT 1.1.1.1", "conversation": id}));
    assert_ne!(text(&direct), "direct: open", "{}", direct.text);
}

/// Cursor's commands ask, and obid's rule refuses every one (`oby_shell` is how a conversation runs
/// one). Its edits don't ask, so what it plants for its next run goes before that run.
#[cfg(target_os = "macos")]
#[test]
fn conversation_agent_9_cursors_commands_are_refused_and_what_it_plants_goes() {
    let (_folder, obid) = obid("talk-cursor-plants");
    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let id = conversation(&first);
    choose(&obid, &id, json!(CURSOR));
    let shell = said(&obid, json!({"said": "SHELL echo hi", "conversation": id}));
    assert_eq!(text(&shell), "shell: reject-once", "{}", shell.text);
    // A command titled as an MCP tool is a command.
    let titled = said(
        &obid,
        json!({"said": "SHELL mcp.oby.anything", "conversation": id}),
    );
    assert_eq!(text(&titled), "shell: reject-once", "{}", titled.text);
    let plant = said(&obid, json!({"said": "PLANT", "conversation": id}));
    assert_eq!(text(&plant), "planted", "{}", plant.text);
    let look = said(&obid, json!({"said": "LOOK", "conversation": id}));
    assert_eq!(text(&look), "look: nothing", "{}", look.text);
    // Its login copy goes when the turn ends, and never held the refresh token.
    let talk = std::fs::read_dir(obid.home.join("logins"))
        .unwrap()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("t-"))
        .unwrap()
        .path();
    assert!(!talk.join(".cursor/auth.json").exists());
    // And its folder goes with the conversation.
    assert_eq!(
        call(&obid, "DELETE", &format!("/v1/conversations/{id}"), None).status,
        204
    );
    assert!(!talk.exists());
}

/// Cursor fills or can't run: Oby moves off it as off any other.
#[cfg(target_os = "macos")]
#[test]
fn conversation_agent_3_a_full_cursor_moves_to_another_tool() {
    let (_folder, obid) = obid("talk-cursor-full");
    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let id = conversation(&first);
    choose(&obid, &id, json!(CURSOR));
    let moved = said(&obid, json!({"said": "FULL", "conversation": id}));
    assert!(
        text(&moved).ends_with("Cursor stub hit its limit, so I moved to Claude Code · Stub."),
        "{}",
        moved.text
    );
}

/// Where the box can't hold Cursor to obid's proxy, it doesn't talk: choosing it says why, with a
/// word the app can tell.
#[cfg(not(target_os = "macos"))]
#[test]
fn conversation_agent_8_without_the_proxys_box_cursor_is_refused() {
    let (_folder, obid) = obid("talk-cursor-linux");
    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let id = conversation(&first);
    let refused = call(
        &obid,
        "PATCH",
        &format!("/v1/conversations/{id}"),
        Some(&json!({"talkAccount": CURSOR})),
    );
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(refused.body["reason"], "talk-needs-egress-box");
}

/// An account that goes is no choice any more: the setting's cleared, and a conversation that chose
/// it talks on Auto.
#[test]
fn conversation_agent_11_a_removed_account_is_no_choice() {
    let (_folder, obid) = obid("talk-removed");
    let first = said(&obid, json!({"said": "REMEMBER", "new": true}));
    let id = conversation(&first);
    choose(&obid, &id, json!(CODEX));
    let set = call(
        &obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"talkAccount": CODEX})),
    );
    assert_eq!(set.status, 200, "{}", set.text);
    assert_eq!(
        call(&obid, "DELETE", &format!("/v1/accounts/{CODEX}"), None).status,
        204
    );
    assert_eq!(listed(&obid, &id)["talkAccount"], Value::Null);
    assert_eq!(
        call(&obid, "GET", "/v1/settings/jobs", None).body["talkAccount"],
        Value::Null
    );
    let answer = said(&obid, json!({"said": "REMEMBER", "conversation": id}));
    assert!(text(&answer).starts_with("resume="), "{}", answer.text);
    assert_eq!(listed(&obid, &id)["talkedOn"], CLAUDE);
}
