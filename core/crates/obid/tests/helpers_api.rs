//! Helpers (docs/contracts/helpers.md, docs/api/helpers.md): `/v1/helpers` and a helper's turns,
//! on its tool only, told its instructions, with only its granted connectors. The agents are
//! stand-ins (tests/agents/fake-harness, fake-codex-harness, fake-cursor-harness).
#[path = "support/fake_tetrix.rs"]
mod fake_tetrix;
mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

const CLAUDE: &str = "stub";
const CODEX: &str = "cstub";
const CURSOR: &str = "ustub";
const GITHUB: [&str; 3] = ["oby_github", "oby_github_write", "oby_merge"];
const DISPATCH: &str = r#"OBY oby_dispatch {"repo": "obi-e2e/fixture-app", "task": "Add a greeting", "kind": "build"}"#;

fn agent(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/agents")
        .join(name)
}

/// obid's configuration with one signed-in account of each tool, as conversation_agent.rs's.
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

/// A helper's body: Inbox helper on `tool` with `connectors`.
fn asked(tool: &str, connectors: Value) -> Value {
    json!({"name": "Inbox helper", "color": "#3a5a8c", "instructions": "Sort my inbox.",
           "tool": tool, "connectors": connectors})
}

fn make(obid: &Obid, body: Value) -> Value {
    let made = call(obid, "POST", "/v1/helpers", Some(&body));
    assert_eq!(made.status, 201, "{}", made.text);
    made.body
}

fn patch(obid: &Obid, helper: &Value, body: Value) -> Answer {
    call(
        obid,
        "PATCH",
        &format!("/v1/helpers/{}", helper["id"].as_str().unwrap()),
        Some(&body),
    )
}

fn helpers(obid: &Obid) -> Value {
    call(obid, "GET", "/v1/helpers", None).body
}

fn conversations(obid: &Obid) -> Value {
    call(obid, "GET", "/v1/conversations", None).body
}

fn listed(obid: &Obid, id: &str) -> Option<Value> {
    let all = conversations(obid);
    all.as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .cloned()
}

/// `words` in `helper`'s conversation: the answer's text.
fn say(obid: &Obid, helper: &Value, words: &str) -> String {
    let body = json!({"said": words, "conversation": helper["conversation"]});
    let answer = call(obid, "POST", "/v1/plans", Some(&body));
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body["text"].as_str().unwrap_or_default().to_owned()
}

/// The tools a `TOOLS` turn listed.
fn names(said: &str) -> Vec<String> {
    let listed = said
        .strip_prefix("tools: ")
        .unwrap_or_else(|| panic!("{said}"));
    listed.split(',').map(str::to_owned).collect()
}

fn recorded(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// What the stand-ins recorded in `helper`'s conversation's config folder: Claude Code's runs and
/// Codex's (`app-server` last in its arguments).
fn turns(obid: &Obid, helper: &Value) -> Vec<Value> {
    let conversation = helper["conversation"].as_str().unwrap();
    recorded(
        &obid
            .home
            .join("conversations")
            .join(conversation)
            .join("config/turns.jsonl"),
    )
}

fn on_codex(turn: &Value) -> bool {
    turn["argv"]
        .as_array()
        .is_some_and(|argv| argv.last() == Some(&json!("app-server")))
}

fn argv(turn: &Value) -> Vec<String> {
    let argv = turn["argv"].as_array().unwrap().iter();
    argv.map(|a| a.as_str().unwrap().to_owned()).collect()
}

/// The value after `flag` in a turn's arguments.
fn flag(turn: &Value, flag: &str) -> String {
    let argv = argv(turn);
    argv[argv.iter().position(|a| a == flag).unwrap() + 1].clone()
}

/// What `helper`'s conversation's MCP configuration says now.
fn mcp(obid: &Obid, helper: &Value) -> Value {
    let folder = if cfg!(target_os = "macos") {
        "login"
    } else {
        "home"
    };
    let file = obid.home.join(format!(
        "conversations/{}/{folder}/oby-mcp.json",
        helper["conversation"].as_str().unwrap()
    ));
    serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
}

/// What Cursor's stand-in recorded, in its short folders under `logins/`.
fn cursor_turns(obid: &Obid) -> Vec<Value> {
    std::fs::read_dir(obid.home.join("logins"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("t-"))
        .flat_map(|e| recorded(&e.path().join("turns.jsonl")))
        .collect()
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

/// HELPERS-1: a helper is made with its connectors in order, once each, in the default profile,
/// with its own conversation; it is listed, outlives a restart, and its file is private.
#[test]
fn helpers_1_make_list_restart() {
    let folder = Folder::new("helpers-1");
    let config = configured(&folder);
    let mut obid = start(&folder.0, &config, None);
    let made = make(&obid, asked("claude", json!(["web", "github", "web"])));
    let profiles = call(&obid, "GET", "/v1/profiles", None).body;
    let default = profiles
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["default"] == true);
    assert_eq!(made["profile"], default.unwrap()["id"], "{made}");
    assert_eq!(made["name"], "Inbox helper");
    assert_eq!(made["color"], "#3a5a8c");
    assert_eq!(made["instructions"], "Sort my inbox.");
    assert_eq!(made["tool"], "claude");
    assert_eq!(made["connectors"], json!(["github", "web"]));
    assert!(made["id"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(made["created"].as_str().is_some_and(|at| at.ends_with('Z')));
    assert_eq!(made.as_object().unwrap().len(), 9, "{made}");
    assert_eq!(helpers(&obid), json!([made]));
    let conversation = listed(&obid, made["conversation"].as_str().unwrap()).unwrap();
    assert_eq!(conversation["helper"], made["id"], "{conversation}");
    assert_eq!(conversation["title"], "Inbox helper");

    obid.stop();
    let obid = start(&folder.0, &config, None);
    assert_eq!(helpers(&obid), json!([made]));
    let mode = std::fs::metadata(obid.home.join("helpers.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

/// HELPERS-2: each rule's refusal is a 422 with its reason, a body that isn't a helper's is a 400,
/// and a refused request keeps nothing and makes no conversation.
#[test]
fn helpers_2_refusals_keep_nothing() {
    let (_folder, obid) = obid("helpers-2");
    let with = |key: &str, value: Value| {
        let mut body = asked("claude", json!([]));
        body[key] = value;
        body
    };
    let (kept, talked) = (helpers(&obid), conversations(&obid));
    for (body, status) in [
        (with("name", json!("")), 422),
        (with("name", json!("two\nlines")), 422),
        (with("name", json!("n".repeat(101))), 422),
        (with("color", json!("blue")), 422),
        (with("instructions", json!("")), 422),
        (with("instructions", json!("i".repeat(4001))), 422),
        (with("tool", json!("gemini")), 422),
        (with("connectors", json!(["gmail"])), 422),
        (with("nickname", json!("x")), 400),
        (with("connectors", json!([1])), 400),
        (with("connectors", json!("github")), 400),
    ] {
        let refused = call(&obid, "POST", "/v1/helpers", Some(&body));
        assert_eq!(refused.status, status, "{body}: {}", refused.text);
        assert!(
            refused.body["error"]
                .as_str()
                .is_some_and(|e| !e.is_empty())
        );
        assert_eq!(helpers(&obid), kept, "{body}");
        assert_eq!(conversations(&obid), talked, "{body}");
    }
    for _ in 0..50 {
        make(&obid, asked("claude", json!([])));
    }
    let (kept, talked) = (helpers(&obid), conversations(&obid));
    let more = call(
        &obid,
        "POST",
        "/v1/helpers",
        Some(&asked("claude", json!([]))),
    );
    assert_eq!(more.status, 422, "{}", more.text);
    assert!(more.text.contains("at most 50"), "{}", more.text);
    assert_eq!(helpers(&obid), kept);
    assert_eq!(conversations(&obid), talked);
}

/// HELPERS-3: a helper is in a profile on this Mac, set when it's made and never changed.
#[test]
fn helpers_3_one_profile() {
    let (_folder, obid) = obid("helpers-3");
    let mut body = asked("claude", json!([]));
    body["profile"] = json!("no-such-profile");
    let refused = call(&obid, "POST", "/v1/helpers", Some(&body));
    assert_eq!(refused.status, 422, "{}", refused.text);
    let profiles = call(&obid, "GET", "/v1/profiles", None).body;
    let profile = profiles[0]["id"].clone();
    body["profile"] = profile.clone();
    let made = make(&obid, body);
    assert_eq!(made["profile"], profile);
    let moved = patch(&obid, &made, json!({"profile": "another"}));
    assert_eq!(moved.status, 422, "{}", moved.text);
    assert_eq!(helpers(&obid)[0]["profile"], profile);
}

/// HELPERS-4: a change answers the helper as changed, under the same checks; the next turn runs
/// on a new tool.
#[test]
fn helpers_4_change() {
    let (_folder, obid) = obid("helpers-4");
    let made = make(&obid, asked("claude", json!([])));
    let changed = patch(
        &obid,
        &made,
        json!({"name": "Mail", "color": "#7a4a8c", "instructions": "File it.",
               "tool": "codex", "connectors": ["tetrix", "github"]}),
    );
    assert_eq!(changed.status, 200, "{}", changed.text);
    assert_eq!(changed.body["name"], "Mail");
    assert_eq!(changed.body["color"], "#7a4a8c");
    assert_eq!(changed.body["instructions"], "File it.");
    assert_eq!(changed.body["tool"], "codex");
    assert_eq!(changed.body["connectors"], json!(["github", "tetrix"]));
    assert_eq!(changed.body["conversation"], made["conversation"]);
    assert_eq!(helpers(&obid), json!([changed.body]));
    let refused = patch(&obid, &made, json!({"color": "blue"}));
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(patch(&obid, &made, json!({})).status, 422);
    let gone = call(
        &obid,
        "PATCH",
        "/v1/helpers/nobody",
        Some(&json!({"name": "x"})),
    );
    assert_eq!(gone.status, 404, "{}", gone.text);

    say(&obid, &made, "HELLO");
    let conversation = listed(&obid, made["conversation"].as_str().unwrap()).unwrap();
    assert_eq!(conversation["talkedOn"], CODEX, "{conversation}");
}

/// HELPERS-5: a helper's conversation goes only with its helper; the jobs it started stay.
#[test]
fn helpers_5_delete() {
    let (_folder, obid) = obid("helpers-5");
    let made = make(&obid, asked("claude", json!([])));
    let conversation = made["conversation"].as_str().unwrap();
    let body = json!({"said": DISPATCH, "conversation": conversation});
    let answer = call(&obid, "POST", "/v1/plans", Some(&body));
    let job = answer.body["jobs"][0]["id"].as_str().unwrap().to_owned();

    let refused = call(
        &obid,
        "DELETE",
        &format!("/v1/conversations/{conversation}"),
        None,
    );
    assert_eq!(refused.status, 409, "{}", refused.text);
    assert_eq!(refused.body["reason"], "helper-conversation");
    assert!(listed(&obid, conversation).is_some());

    let path = format!("/v1/helpers/{}", made["id"].as_str().unwrap());
    let deleted = call(&obid, "DELETE", &path, None);
    assert_eq!(deleted.status, 204, "{}", deleted.text);
    assert_eq!(helpers(&obid), json!([]));
    assert!(listed(&obid, conversation).is_none());
    assert_eq!(
        call(&obid, "GET", &format!("/v1/jobs/{job}"), None).status,
        200
    );
    assert_eq!(call(&obid, "DELETE", &path, None).status, 404);
}

/// HELPERS-6: a helper's turn tries only its tool's accounts, and is told its name and
/// instructions, on Claude Code and on Codex.
#[test]
fn helpers_6_its_tool_and_its_instructions() {
    let folder = Folder::new("helpers-6-full");
    let config = configured(&folder);
    full_from_the_start(&folder, &[CLAUDE]);
    let full = start(&folder.0, &config, None);
    let claude = make(&full, asked("claude", json!([])));
    let why = say(&full, &claude, "HELLO");
    assert!(why.starts_with("I couldn't answer: "), "{why}");
    assert_eq!(why.lines().count(), 1, "{why}");
    assert!(!why.contains("Codex") && !why.contains("Cursor"), "{why}");
    assert!(turns(&full, &claude).is_empty(), "no other tool ran");
    assert!(cursor_turns(&full).is_empty(), "no Cursor ran");
    drop(full);

    let (_folder, obid) = obid("helpers-6");
    let claude = make(&obid, asked("claude", json!([])));
    say(&obid, &claude, "HELLO");
    let ran = turns(&obid, &claude);
    assert!(ran.iter().all(|t| !on_codex(t)), "{ran:?}");
    let told = flag(ran.last().unwrap(), "--append-system-prompt");
    assert!(
        told.contains("Inbox helper") && told.contains("Sort my inbox."),
        "{told}"
    );
    let conversation = listed(&obid, claude["conversation"].as_str().unwrap()).unwrap();
    assert_eq!(conversation["talkedOn"], CLAUDE);

    let codex = make(&obid, asked("codex", json!([])));
    say(&obid, &codex, "HELLO");
    let ran: Vec<Value> = turns(&obid, &codex).into_iter().filter(on_codex).collect();
    let told = ran.last().unwrap()["instructions"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        told.contains("Inbox helper") && told.contains("Sort my inbox."),
        "{told}"
    );
}

/// HELPERS-7: a helper's turn is offered and allowed only its granted connectors: Tetrix's server,
/// the web's tools and Oby's GitHub tools; given them, the next turn has them all.
#[test]
fn helpers_7_only_granted_connectors() {
    let (_folder, obid) = obid("helpers-7");
    let (base, _seen) = fake_tetrix::fake_tetrix();
    fake_tetrix::sign_in(&obid, &base);
    let helper = make(&obid, asked("claude", json!([])));
    let listed_tools = names(&say(&obid, &helper, "TOOLS"));
    let servers = mcp(&obid, &helper)["mcpServers"].clone();
    assert_eq!(
        servers.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["oby"]
    );
    let tools = flag(turns(&obid, &helper).last().unwrap(), "--tools");
    assert!(
        !tools.contains("WebSearch") && !tools.contains("WebFetch"),
        "{tools}"
    );
    assert!(
        listed_tools.contains(&"oby_jobs".to_owned()),
        "{listed_tools:?}"
    );
    for github in GITHUB {
        assert!(
            !listed_tools.contains(&github.to_owned()),
            "{listed_tools:?}"
        );
    }
    let called = say(&obid, &helper, "OBY oby_github {}");
    assert!(
        called.starts_with("oby: refused: This helper wasn't given"),
        "{called}"
    );

    // Between turns its waiting Claude Code lists Oby's tools without GitHub's.
    let oby = &servers["oby"];
    let url = oby["url"].as_str().unwrap();
    let path = url[url.find("/v1/").unwrap()..].to_owned();
    let bearer = oby["headers"]["Authorization"].as_str().unwrap();
    let token = bearer.strip_prefix("Bearer ").unwrap().to_owned();
    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let between = call_with_token(&obid, &token, "POST", &path, Some(&list));
    assert!(between.text.contains("oby_jobs"), "{}", between.text);
    assert!(!between.text.contains("oby_github"), "{}", between.text);

    // During a turn, Tetrix refuses its token's calls.
    let tetrix = path.replace("/v1/agent/mcp/", "/v1/agent/tetrix/");
    let ask = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                     "params": {"name": "search_entities", "arguments": {"query": "x"}}});
    std::thread::scope(|scope| {
        let lingering = scope.spawn(|| say(&obid, &helper, "LINGER 4000"));
        let refused = within(10, "a turn under way", || {
            let servers = mcp(&obid, &helper)["mcpServers"].clone();
            let bearer = servers["oby"]["headers"]["Authorization"].as_str().unwrap();
            let token = bearer.strip_prefix("Bearer ").unwrap();
            let answer = call_with_token(&obid, token, "POST", &tetrix, Some(&ask));
            (answer.body["result"]["isError"] == true).then_some(answer)
        });
        assert!(
            refused
                .text
                .contains("This helper wasn't given that connector"),
            "{}",
            refused.text
        );
        assert_eq!(lingering.join().unwrap(), "waited");
    });

    let codex = make(&obid, asked("codex", json!([])));
    say(&obid, &codex, "HELLO");
    let ran: Vec<Value> = turns(&obid, &codex).into_iter().filter(on_codex).collect();
    let args = argv(ran.last().unwrap());
    assert!(
        args.contains(&r#"web_search="disabled""#.to_owned()),
        "{args:?}"
    );

    let given = patch(
        &obid,
        &helper,
        json!({"connectors": ["github", "tetrix", "web"]}),
    );
    assert_eq!(given.status, 200, "{}", given.text);
    let listed_tools = names(&say(&obid, &helper, "TOOLS"));
    for github in GITHUB {
        assert!(
            listed_tools.contains(&github.to_owned()),
            "{listed_tools:?}"
        );
    }
    assert!(mcp(&obid, &helper)["mcpServers"]["tetrix"].is_object());
    let tools = flag(turns(&obid, &helper).last().unwrap(), "--tools");
    assert!(
        tools.contains("WebSearch") && tools.contains("WebFetch"),
        "{tools}"
    );
}

/// HELPERS-9: a vault secret in a helper's instructions reaches neither its tool's arguments nor
/// its MCP configuration, nor any `/v1/helpers` answer.
#[test]
fn helpers_9_no_secret_reaches_the_tool() {
    let folder = Folder::new("helpers-9");
    let config = configured(&folder);
    put_token(&config, TOKEN);
    let path = folder.0.join("config.json");
    std::fs::write(&path, config.to_string()).unwrap();
    let value = format!("tok/{}+a=b?c&d", std::process::id() % 1_000_000 + 100_000);
    let added = std::process::Command::new(OBID)
        .args(["vault", "--config"])
        .arg(&path)
        .args(["add", "staging-api-key"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(value.as_bytes())?;
            child.wait()
        })
        .unwrap();
    assert!(added.success());
    let obid = start(&folder.0, &config, None);
    let mut body = asked("claude", json!(["tetrix"]));
    body["instructions"] = json!(format!("Use {value} for staging."));
    let made = call(&obid, "POST", "/v1/helpers", Some(&body));
    assert_eq!(made.status, 201, "{}", made.text);
    assert!(!made.text.contains(&value), "{}", made.text);
    let helper = made.body;
    say(&obid, &helper, "HELLO");
    let ran = turns(&obid, &helper);
    let told = flag(ran.last().unwrap(), "--append-system-prompt");
    assert!(told.contains("[vault: staging-api-key]"), "{told}");
    for turn in &ran {
        assert!(!turn.to_string().contains(&value), "{turn}");
    }
    assert!(!mcp(&obid, &helper).to_string().contains(&value));
    let listed = call(&obid, "GET", "/v1/helpers", None);
    assert!(!listed.text.contains(&value), "{}", listed.text);
}

/// HELPERS-10: a Cursor helper keeps the web, since obid can't take Cursor's own search away.
#[test]
fn helpers_10_cursor_keeps_the_web() {
    let (_folder, obid) = obid("helpers-10");
    let refused = call(
        &obid,
        "POST",
        "/v1/helpers",
        Some(&asked("cursor", json!([]))),
    );
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(refused.body["reason"], "cursor-needs-web");
    let cursor = make(&obid, asked("cursor", json!(["web"])));
    let removed = patch(&obid, &cursor, json!({"connectors": ["github"]}));
    assert_eq!(removed.status, 422, "{}", removed.text);
    assert_eq!(removed.body["reason"], "cursor-needs-web");
    let claude = make(&obid, asked("claude", json!(["github"])));
    let switched = patch(&obid, &claude, json!({"tool": "cursor"}));
    assert_eq!(switched.status, 422, "{}", switched.text);
    assert_eq!(switched.body["reason"], "cursor-needs-web");
    assert_eq!(helpers(&obid), json!([cursor, claude]));
}
