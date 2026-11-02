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
        .filter(|e| e.file_name().to_string_lossy().starts_with("talk-"))
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
    // Its session stays where it was, since Codex can't run: nothing moved.
    let second = said(&obid, json!({"said": "REMEMBER", "conversation": id}));
    assert_ne!(text(&second), "resume=none", "{}", second.text);

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
            .contains("/home/logins/talk-"),
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
    let plant = said(&obid, json!({"said": "PLANT", "conversation": id}));
    assert_eq!(text(&plant), "planted", "{}", plant.text);
    let look = said(&obid, json!({"said": "LOOK", "conversation": id}));
    assert_eq!(text(&look), "look: nothing", "{}", look.text);
    // Its login copy goes when the turn ends, and never held the refresh token.
    let talk = std::fs::read_dir(obid.home.join("logins"))
        .unwrap()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("talk-"))
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
    assert_eq!(
        call(&obid, "GET", "/v1/settings/jobs", None).body["talkAccount"],
        Value::Null
    );
    let answer = said(&obid, json!({"said": "REMEMBER", "conversation": id}));
    assert!(text(&answer).starts_with("resume="), "{}", answer.text);
    assert_eq!(listed(&obid, &id)["talkedOn"], CLAUDE);
}
