//! SPH-10: the non-Claude answerers. Only the two quoted files may be read by the model.
//! Codex gets an obid-served dynamic Read tool; Cursor gets Workspace read access and Ask mode,
//! with every permission decided here. Neither gets a broker, MCP servers, commands or web tools.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use crate::agent::{Launch, Tool};
use crate::config::{Account, Agent, Config};
use crate::conn::{self, Conn};
use crate::{jobs, process, store};

use super::{PROMPT, RUN_LIMIT, SYSTEM, schema};

/// Prefer a signed-in account across tools. Retain Claude's old first-account fallback, which
/// lets its CLI report a missing login. A ChatGPT marker excludes an account before any launch.
pub(super) fn account(config: &Config) -> Result<(Tool, Agent, Account), String> {
    let agents = config.agents.get();
    let home = jobs::login_home();
    let mut fallback = None;
    for (tool, agent) in [
        (Tool::Claude, agents.claude),
        (Tool::Codex, agents.codex),
        (Tool::Cursor, agents.cursor),
    ] {
        let Some(agent) = agent.filter(|a| process::resolve_bin(&a.bin).is_some()) else {
            continue;
        };
        for account in &agent.accounts {
            if crate::chatgpt::marker_of(account).is_some() {
                continue;
            }
            if jobs::signed_in(tool, account, home.as_deref()) {
                return Ok((tool, agent.clone(), account.clone()));
            }
            if tool == Tool::Claude && fallback.is_none() {
                fallback = Some((tool, agent.clone(), account.clone()));
            }
        }
    }
    fallback.ok_or_else(|| "no eligible account can answer another person's question".into())
}

/// Closes the proxy on every exit, including cancellation of the enclosing future.
struct Door<'a>(&'a jobs::Jobs, String);

impl Drop for Door<'_> {
    fn drop(&mut self) {
        self.0.close_door(&self.1);
    }
}

const EXTRA_OFF: &[&str] = &[
    "apply_patch_freeform",
    "code_mode_host",
    "goals",
    "default_mode_request_user_input",
];

fn codex_flags() -> Vec<OsString> {
    let mut flags = Vec::new();
    for feature in crate::brain::harness_codex::OFF.iter().chain(EXTRA_OFF) {
        flags.extend(["-c".into(), format!("features.{feature}=false").into()]);
    }
    flags.extend(["-c".into(), r#"web_search="disabled""#.into()]);
    flags
}

pub(super) async fn run(
    jobs: &jobs::Jobs,
    tool: Tool,
    account: &Account,
    dir: &Path,
) -> Result<Value, String> {
    // Defense in depth: the selector is not the authority to start a ChatGPT run.
    if crate::chatgpt::marker_of(account).is_some() {
        return Err("a ChatGPT account doesn't answer another person's question".into());
    }
    // Cursor's native reads use its existing confined, provider-only proxy door. Codex's model
    // has no native file or network tools: obid serves its only executable tool, Read.
    let door = Door(
        jobs,
        format!(
            "{}peer-answer-{}",
            crate::brain::harness_cursor::DOOR,
            uuid::Uuid::new_v4()
        ),
    );
    let flags = if tool == Tool::Codex {
        codex_flags()
    } else {
        Vec::new()
    };
    let reads = dir.join("question");
    let login = dir.join("login");
    let mut frozen = vec![reads.clone()];
    if tool == Tool::Cursor {
        cursor_config(&login)?;
        frozen.extend([
            login.join("cli-config.json"),
            login.join(".cursor/hooks.json"),
            login.join("hooks.json"),
            login.join("answer-read.sh"),
        ]);
    }
    let launch = if tool == Tool::Codex {
        jobs.conversation_launch(
            tool,
            account,
            &reads,
            &dir.join("tmp"),
            &login,
            frozen,
            flags,
            dir.join("agent.log"),
        )?
    } else {
        jobs.conversation_cursor_launch(
            account,
            &reads,
            &dir.join("tmp"),
            &login,
            frozen,
            dir.join("agent.log"),
            &door.1,
        )?
    };
    if tool == Tool::Codex {
        crate::brain::harness_codex::features_off_with(&launch, EXTRA_OFF).await?;
    } else {
        // Copying a Cursor login resets its permissions; restore the answer's stricter settings
        // before the sandbox starts and freezes them.
        cursor_config(&login)?;
    }
    let (_keep_running, stop) = tokio::sync::watch::channel(process::Signal::Run);
    let conn = Conn::spawn(&launch, RUN_LIMIT, stop, tool == Tool::Cursor)
        .await
        .map_err(|_| "the answering model couldn't start")?;
    let mut run = Reader {
        conn,
        tool,
        reads: launch.cwd.clone(),
        thread: None,
        text: String::new(),
    };
    let result = match tool {
        Tool::Codex => run.codex(&launch).await,
        Tool::Cursor => run.cursor(&launch).await,
        Tool::Claude => unreachable!(),
    };
    run.conn.close_input();
    // No session is resumed; no child may survive a failed or completed answer.
    run.conn.finish_within(Duration::ZERO).await;
    let text = result?;
    serde_json::from_str(&text).map_err(|_| "the answering model returned no JSON answer".into())
}

/// Only obid's settings, never the account's hooks, MCP servers, rules or permissions. Ask mode
/// removes writes; the immutable workspace also stops them at the OS boundary. Workspace read
/// access covers native reads and searches; reads outside it ask and are refused below. Explicit
/// denies cover the login and Cursor's own workspace-adjacent data as well.
fn cursor_config(login: &Path) -> Result<(), String> {
    store::make_private_dir(&login.join(".cursor"))
        .map_err(|_| "obid couldn't make Cursor's private settings folder")?;
    let config = json!({
        "version": 1, "editor": {"vimMode": false}, "approvalMode": "allowlist",
        "sandbox": {"readBoundary": "workspace"},
        "permissions": {
            "allow": ["Read(question.json)", "Read(material.json)"],
            "deny": ["Shell(*)", "Write(**)", "WebFetch(*)", "Mcp(*:*)",
                     format!("Read({}/**)", login.display())]
        }
    });
    store::write_private(
        &login.join("cli-config.json"),
        config.to_string().as_bytes(),
    )
    .map_err(|_| "obid couldn't confine Cursor's file tools".to_owned())?;
    store::write_private(&login.join("answer-read.sh"), CURSOR_READ_HOOK.as_bytes())
        .map_err(|_| "obid couldn't write Cursor's read gate")?;
    // Cursor's native web search is performed by its provider. A proxy alone cannot stop it:
    // preToolUse refuses every tool but the two reads, before any provider-side execution too.
    let script = login
        .join("answer-read.sh")
        .to_string_lossy()
        .replace('\'', "'\\''");
    let reads = login
        .parent()
        .ok_or("the answer's folder has no parent")?
        .join("question");
    let reads = reads.to_string_lossy().replace('\'', "'\\''");
    let hook = json!({"command": format!("/bin/sh '{script}' '{reads}'"), "failClosed": true});
    let hooks =
        json!({"version": 1, "hooks": {"preToolUse": [hook.clone()], "beforeReadFile": [hook]}})
            .to_string();
    // Both documented home layout and the CLI's custom config-directory layout.
    for path in [login.join(".cursor/hooks.json"), login.join("hooks.json")] {
        store::write_private(&path, hooks.as_bytes())
            .map_err(|_| "obid couldn't configure Cursor's read gate")?;
    }
    Ok(())
}

/// macOS's JSON parser is part of the OS: no agent, node or Python is needed by this gate.
/// Input is data on stdin, never shell source. Any missing/unknown field is a denial.
const CURSOR_READ_HOOK: &str = r#"#!/bin/sh
payload=$(/bin/cat) || exit 2
field() { printf '%s' "$payload" | /usr/bin/plutil -extract "$1" raw -o - - 2>/dev/null; }
event=$(field hook_event_name) || exit 2
case "$event" in
  preToolUse)
    tool=$(field tool_name) || exit 2
    [ "$tool" = Read ] || exit 2
    path=$(field tool_input.file_path) || path=$(field tool_input.path) || exit 2
    ;;
  beforeReadFile) path=$(field file_path) || exit 2 ;;
  *) exit 2 ;;
esac
case "$path" in
  question.json|material.json|"$1/question.json"|"$1/material.json")
    printf '%s\n' '{"permission":"allow"}' ;;
  *) exit 2 ;;
esac
"#;

struct Reader {
    conn: Conn,
    tool: Tool,
    reads: PathBuf,
    thread: Option<String>,
    text: String,
}

impl Reader {
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.conn.request(method, params).await;
        loop {
            let message = self.next().await?;
            if let Some(answer) = conn::answer_to(&message, id) {
                return answer.map_err(|_| "the answering model refused its confined run".into());
            }
            self.handle(&message).await?;
        }
    }

    async fn next(&mut self) -> Result<Value, String> {
        self.conn
            .next()
            .await
            .map_err(|_| "the answering model stopped or took too long".into())
    }

    async fn codex(&mut self, launch: &Launch) -> Result<String, String> {
        self.call(
            "initialize",
            json!({
                "clientInfo": {"name": "obid", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"experimentalApi": true}
            }),
        )
        .await?;
        self.conn.notify("initialized", None).await;
        let started = self.call("thread/start", json!({
            "cwd": launch.cwd, "sandbox": "danger-full-access", "approvalPolicy": "untrusted",
            "approvalsReviewer": "user", "ephemeral": true, "baseInstructions": SYSTEM,
            "dynamicTools": [{"type": "function", "name": "Read", "description": "Read question.json or material.json in the working folder.",
                "inputSchema": {"type": "object", "properties": {"file_path": {"type": "string", "enum": ["question.json", "material.json"]}},
                    "required": ["file_path"], "additionalProperties": false}}]
        })).await?;
        let thread = started["thread"]["id"]
            .as_str()
            .ok_or("Codex started no answering thread")?
            .to_owned();
        self.thread = Some(thread.clone());
        self.call("turn/start", json!({"threadId": thread,
            "input": [{"type": "text", "text": PROMPT, "text_elements": []}], "outputSchema": schema()
        })).await?;
        loop {
            let message = self.next().await?;
            self.handle(&message).await?;
            if message["method"] == "turn/completed" {
                if message["params"]["threadId"] != thread
                    || message["params"]["turn"]["status"] != "completed"
                {
                    return Err("Codex didn't complete its answering turn".into());
                }
                return Ok(self.text.clone());
            }
        }
    }

    async fn cursor(&mut self, launch: &Launch) -> Result<String, String> {
        self.call(
            "initialize",
            json!({"protocolVersion": 1, "clientCapabilities": {},
            "clientInfo": {"name": "oby", "version": env!("CARGO_PKG_VERSION")}}),
        )
        .await?;
        let started = self
            .call("session/new", json!({"cwd": launch.cwd, "mcpServers": []}))
            .await?;
        let session = started["sessionId"]
            .as_str()
            .ok_or("Cursor started no answering session")?
            .to_owned();
        self.thread = Some(session.clone());
        // A missing Ask mode fails closed rather than silently using Agent mode.
        let found = crate::acp::found(&started);
        if !found.modes.iter().any(|m| m == "ask") {
            return Err("Cursor offers no read-only Ask mode".into());
        }
        self.call(
            "session/set_mode",
            json!({"sessionId": session, "modeId": "ask"}),
        )
        .await?;
        // ACP has no system prompt: the owner authorized the same fixed text as the first
        // message. No word of the question or material is interpolated into it.
        let completed = self
            .call(
                "session/prompt",
                json!({"sessionId": session, "prompt": [{"type": "text",
            "text": format!("{SYSTEM}\n\n{PROMPT}\nJSON schema: {}", schema())}]}),
            )
            .await?;
        if completed["stopReason"] != "end_turn" {
            return Err("Cursor didn't complete its answering turn".into());
        }
        Ok(self.text.clone())
    }

    fn allowed_path(&self, path: &str) -> bool {
        ["question.json", "material.json"]
            .iter()
            .any(|name| path == *name || Path::new(path) == self.reads.join(name))
    }

    async fn handle(&mut self, message: &Value) -> Result<(), String> {
        let params = &message["params"];
        if let Some(id) = conn::request_id(message) {
            match (self.tool, message["method"].as_str()) {
                (Tool::Codex, Some("item/tool/call")) => {
                    let input = &params["arguments"];
                    let file = input["file_path"].as_str().filter(|p| self.allowed_path(p));
                    let text = file
                        .filter(|_| params["tool"] == "Read")
                        .and_then(|p| std::fs::read_to_string(self.reads.join(p)).ok());
                    self.conn.reply(id, json!({"success": text.is_some(), "contentItems": [{"type": "inputText",
                        "text": text.unwrap_or_else(|| "Only question.json and material.json may be read.".into())}]})).await;
                    if params["tool"] != "Read" {
                        return Err("Codex asked for a tool outside Read while answering".into());
                    }
                    return Ok(());
                }
                (Tool::Cursor, Some("session/request_permission")) => {
                    let mut call = crate::acp::ToolCall::default();
                    call.merge(&params["toolCall"]);
                    let allowed = call.kind.as_deref() == Some("read")
                        && !call.paths.is_empty()
                        && call.paths.iter().all(|p| self.allowed_path(p));
                    let option = params["options"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|o| o["kind"] == if allowed { "allow_once" } else { "reject_once" });
                    let outcome = option.map_or_else(
                        || json!({"outcome": "cancelled"}),
                        |o| json!({"outcome": "selected", "optionId": o["optionId"]}),
                    );
                    self.conn.reply(id, json!({"outcome": outcome})).await;
                    return if allowed {
                        Ok(())
                    } else {
                        Err("Cursor asked for a tool outside the two answer files".into())
                    };
                }
                _ => {
                    self.conn.refuse(id).await;
                    return Err("the answering model asked for an unavailable tool".into());
                }
            }
        }
        match self.tool {
            Tool::Codex => {
                if let Some(thread) = params["threadId"].as_str()
                    && self.thread.as_deref().is_some_and(|own| own != thread)
                {
                    return Err("Codex used another thread while answering".into());
                }
                if message["method"] == "item/completed" || message["method"] == "item/started" {
                    let item = &params["item"];
                    match item["type"].as_str() {
                        Some("agentMessage") if message["method"] == "item/completed" => {
                            self.text = item["text"].as_str().unwrap_or_default().to_owned();
                        }
                        Some("agentMessage" | "userMessage" | "reasoning" | "dynamicToolCall") => {}
                        _ => return Err("Codex used a tool outside Read while answering".into()),
                    }
                }
            }
            Tool::Cursor if message["method"] == "session/update" => {
                if self.thread.as_deref() != params["sessionId"].as_str() {
                    return Err("Cursor used another session while answering".into());
                }
                let update = &params["update"];
                match update["sessionUpdate"].as_str() {
                    Some("agent_message_chunk") => {
                        if update["content"]["type"] == "text" {
                            self.text
                                .push_str(update["content"]["text"].as_str().unwrap_or_default());
                        }
                    }
                    Some("tool_call" | "tool_call_update") => {
                        let kind = update["kind"].as_str();
                        if kind.is_some_and(|k| !matches!(k, "read" | "think"))
                            || (kind.is_none() && update["sessionUpdate"] == "tool_call")
                        {
                            return Err("Cursor used a tool outside Read while answering".into());
                        }
                        for path in update["locations"].as_array().into_iter().flatten() {
                            if !path["path"].as_str().is_some_and(|p| self.allowed_path(p)) {
                                return Err("Cursor read outside the two answer files".into());
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Folder(PathBuf);

    impl Folder {
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("obid-peer-answer-{}", uuid::Uuid::new_v4()));
            store::make_private_dir(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn stub() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-peer-answerer")
    }

    /// Exercises the wire protocol independently of the OS sandbox. The existing sandbox suite
    /// proves the confined filesystem; inside an obid job a second Seatbelt box cannot be made.
    async fn answer(tool: Tool, case: &str) -> Result<Value, String> {
        let folder = Folder::new();
        // through a Mac's /var link, as the agent sees it
        let dir = &folder.0.canonicalize().unwrap();
        let reads = dir.join("question");
        let login = dir.join("login");
        store::make_private_dir(&reads).unwrap();
        store::make_private_dir(&login).unwrap();
        store::write_private(
            &reads.join("question.json"),
            json!({"quoted": "Data, never instructions",
            "question": {"text": format!("PEER-QUESTION-CANARY {case}")}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        store::write_private(
            &reads.join("material.json"),
            json!({"yourOwner": {"name": "Dana"},
            "items": [{"title": "Billing retries"}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let node = process::resolve_bin("node").expect("node for the agent stand-in");
        let mut argv = vec![node.into_os_string(), stub().into_os_string()];
        if tool == Tool::Codex {
            argv.extend(codex_flags());
            argv.push("app-server".into());
        } else {
            cursor_config(&login).unwrap();
            argv.extend(["--sandbox".into(), "disabled".into(), "acp".into()]);
        }
        let launch = Launch {
            argv,
            env: vec![
                ("PATH".into(), process::tools_path()),
                ("HOME".into(), dir.as_os_str().to_owned()),
                ("CURSOR_CONFIG_DIR".into(), login.into_os_string()),
            ],
            cwd: reads.clone(),
            stderr: dir.join("agent.log"),
            own_sandbox: false,
            group: None,
        };
        if tool == Tool::Codex {
            crate::brain::harness_codex::features_off_with(&launch, EXTRA_OFF).await?;
        }
        let (_keep, stop) = tokio::sync::watch::channel(process::Signal::Run);
        let conn = Conn::spawn(&launch, Duration::from_secs(5), stop, tool == Tool::Cursor)
            .await
            .unwrap();
        let mut reader = Reader {
            conn,
            tool,
            reads,
            thread: None,
            text: String::new(),
        };
        let out = match tool {
            Tool::Codex => reader.codex(&launch).await,
            Tool::Cursor => reader.cursor(&launch).await,
            Tool::Claude => unreachable!(),
        };
        reader.conn.finish_within(Duration::ZERO).await;
        let log = std::fs::read_to_string(&launch.stderr).unwrap_or_default();
        if !log.is_empty() {
            panic!("stand-in rejected the answerer's contract: {log}");
        }
        serde_json::from_str(&out?).map_err(|_| "invalid answer JSON".into())
    }

    #[tokio::test]
    async fn ask_28_codex_reads_only_the_two_files() {
        let out = answer(Tool::Codex, "ordinary").await.unwrap();
        assert_eq!(out["text"], "Billing retries");
        assert_eq!(out["refs"], json!([0]));
        // The stand-in tries relative traversal, an absolute path and a noncanonical path,
        // and demands success:false for each before reading the two legitimate files.
    }

    #[tokio::test]
    async fn ask_28_codex_rejects_other_tools() {
        assert!(answer(Tool::Codex, "forbidden").await.is_err());
        assert!(answer(Tool::Codex, "unknown-tool").await.is_err());
        assert!(answer(Tool::Codex, "native-tool").await.is_err());
        assert!(answer(Tool::Codex, "features-on").await.is_err());
    }

    #[tokio::test]
    async fn ask_29_cursor_uses_the_fixed_first_message() {
        let out = answer(Tool::Cursor, "ordinary").await.unwrap();
        assert_eq!(out["text"], "Billing retries");
        assert_eq!(out["choice"], 0);
    }

    #[tokio::test]
    async fn ask_29_cursor_rejects_outside_tools() {
        for case in [
            "forbidden",
            "forbidden-shell",
            "forbidden-web",
            "forbidden-write",
            "forbidden-mcp",
            "native-outside",
        ] {
            assert!(answer(Tool::Cursor, case).await.is_err(), "{case}");
        }
        assert!(answer(Tool::Cursor, "no-ask").await.is_err());
    }

    #[tokio::test]
    async fn ask_30_failed_answers_are_not_usable() {
        for tool in [Tool::Codex, Tool::Cursor] {
            for case in ["invalid-json", "failed-turn"] {
                assert!(answer(tool, case).await.is_err(), "{tool:?} {case}");
            }
            assert_eq!(
                answer(tool, "need-owner").await.unwrap()["answerable"],
                false
            );
        }
    }

    #[tokio::test]
    async fn ask_31_provider_answers_reach_the_common_secret_scan() {
        let folder = Folder::new();
        for tool in [Tool::Codex, Tool::Cursor] {
            let out = super::super::checked(answer(tool, "leak-case").await.unwrap()).unwrap();
            let answer = super::super::written(out, &json!({}), &[])
                .unwrap()
                .unwrap();
            let verdict = crate::scan::scan(&[("answer".into(), answer.text)], &folder.0).await;
            assert!(
                matches!(verdict, crate::scan::Verdict::Found(_)),
                "{verdict:?}"
            );
        }
    }

    fn configuration(folder: &Folder, other: Option<Tool>) -> Config {
        let cg = folder.0.join("chatgpt");
        store::make_private_dir(&cg).unwrap();
        let marker = crate::chatgpt::Marker {
            signed: true,
            plan_use: true,
            refreshed_at: obi_protocol::time::now(),
            ..Default::default()
        };
        store::write_private(
            &cg.join(crate::chatgpt::MARKER),
            &serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
        let account = |id: &str, dir: &Path| json!({"id": id, "name": id, "configDir": dir});
        let mut agents = json!({"codex": {"bin": stub(), "accounts": [account("chatgpt", &cg)]}});
        if let Some(tool) = other {
            let dir = folder.0.join("other");
            store::make_private_dir(&dir).unwrap();
            let file = if tool == Tool::Codex {
                "auth.json"
            } else {
                jobs::cursor_login_file()
            };
            let path = dir.join(file);
            store::make_private_dir(path.parent().unwrap()).unwrap();
            store::write_private(
                &path,
                json!({"OPENAI_API_KEY": "test-key", "apiKey": "test-key"})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
            if tool == Tool::Codex {
                agents["codex"]["accounts"]
                    .as_array_mut()
                    .unwrap()
                    .push(account("other", &dir));
            } else {
                agents[tool.id()] = json!({"bin": stub(), "accounts": [account("other", &dir)]});
            }
        }
        Config::parse(
            &json!({"home": folder.0.join("home"), "listen": "127.0.0.1:0",
            "owner": "dana", "agents": agents, "repos": []})
            .to_string(),
        )
        .unwrap()
    }

    #[test]
    fn ask_30_chatgpt_is_skipped_for_another_account() {
        for tool in [Tool::Codex, Tool::Cursor] {
            let folder = Folder::new();
            let config = configuration(&folder, Some(tool));
            let (selected, _, account) = account(&config).unwrap();
            assert_eq!(selected, tool);
            assert_eq!(account.id, "other");
        }
    }

    #[test]
    fn ask_30_chatgpt_only_has_no_answerer() {
        let folder = Folder::new();
        let config = configuration(&folder, None);
        assert!(account(&config).is_err());
    }
}
