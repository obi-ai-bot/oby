//! Every model call obid makes as Oby's brain, and what each spent. The quick model's one-shot
//! side calls (triage sorting the inbox, memory finding what the owner keeps repeating, the recap
//! after time away) run here, one attempt, on the signed-in account with the most room of any tool
//! that takes them, as a new job on Auto is routed (USAGE-11, B00-15): Claude Code's `claude -p`
//! with a JSON schema, boxed when it reads a quoted file, or a Codex `app-server` turn with an
//! output schema, always boxed, reading the quoted file only through a Read tool obid serves. With
//! no such account none runs, and [`thinker`] says why. Every brain call, those and each run of a
//! harness turn, is kept in `home/brain-turns.jsonl` with the
//! numbers its CLI reported (#533), and `GET /v1/brain/usage` adds them up.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::agent::{Launch, Tool};
use crate::api::Shared;

/// The model for side calls.
pub const QUICK_MODEL: &str = "claude-haiku-5-5";
/// How hard it thinks (#536, Claude Code's `--effort`): a side call only sorts or summarizes.
const QUICK_EFFORT: &str = "low";
/// The file in the call's folder that quotes what others wrote, when there is any.
pub const QUOTED_FILE: &str = "work.md";
/// How many turns a call may take when it may read the quoted file: one to read, more to think
/// and answer (#535: three left it no room after a read).
const READ_TURNS: &str = "6";
/// How long a background call may take (#537), one attempt: Haiku answers in 3 to 4 seconds.
pub const BACKGROUND_TIMEOUT: Duration = Duration::from_secs(15);
/// How many records are kept.
pub const KEPT: usize = 500;

/// Why a side call has no answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    Failed(String),
    /// It took too long: no answer within its time.
    TimedOut(String),
}

/// The tools a side call runs on, each with its own prompt shape: Claude Code (`--system-prompt`,
/// `--json-schema`) and Codex (`baseInstructions`, `outputSchema`). Cursor isn't one yet: its
/// confined read is the read gate and egress door of another Oby's question's answerer
/// (crate::sphere::answering), not shared with the brain yet.
const TOOLS: [Tool; 2] = [Tool::Claude, Tool::Codex];

/// Why no side call runs: no account of [`TOOLS`] is signed in.
pub const NO_ACCOUNT: &str = "no Claude Code or Codex account is signed in";

/// Codex's features a side call runs without, besides a conversation's
/// ([`super::harness_codex::OFF`]): no patch tool, code mode, goals or questions for the owner. It
/// only fills the schema.
const CODEX_OFF: &[&str] = &[
    "apply_patch_freeform",
    "code_mode_host",
    "goals",
    "default_mode_request_user_input",
];

/// The user's own Claude Code or Codex, with no tools, no settings and no MCP: it only fills the
/// schema. With a quoted file to read, it has Read on that file and nothing else, boxed like a
/// job.
pub struct Quick {
    tool: Tool,
    bin: PathBuf,
    /// The account to think with: Claude Code's folder is `CLAUDE_CONFIG_DIR` (none for the
    /// default login); Codex's is copied into the box's `CODEX_HOME`.
    account: crate::config::Account,
    home: PathBuf,
    jobs: Arc<crate::jobs::Jobs>,
    /// What the call is for, in its record: `triage`, `memory`, `recap`.
    kind: &'static str,
}

/// The quick model for `kind`'s calls ([`thinker`]); none with no account to think with.
pub fn quick(shared: &Shared, kind: &'static str) -> Option<Quick> {
    thinker(shared, kind).ok()
}

/// The quick model for `kind`'s calls: of each tool of [`TOOLS`], the account Oby thinks with
/// first (signed in, not full: [`crate::jobs::Jobs::thinking_accounts`]), then of those the one
/// with the most room, as a new job on Auto is routed (USAGE-11). A ChatGPT login isn't one: its
/// runs go through obid's relay with a run's own token, and background work waits for the owner's
/// yes on it (CHATGPT-20). With none, why ([`NO_ACCOUNT`]): nothing is guessed.
pub fn thinker(shared: &Shared, kind: &'static str) -> Result<Quick, String> {
    let jobs = &shared.jobs;
    // Run by its absolute path, never one a folder it starts in could change.
    let bin = |tool: Tool| jobs.agent_bin(tool).filter(|bin| bin.is_absolute());
    let first: Vec<(Tool, crate::config::Account)> = TOOLS
        .into_iter()
        .filter(|tool| bin(*tool).is_some())
        .filter_map(|tool| {
            let account = jobs
                .thinking_accounts(tool)
                .into_iter()
                .find(|a| crate::chatgpt::marker_of(a).is_none())?;
            Some((tool, account))
        })
        .collect();
    let (tool, account) = jobs
        .most_room_first(first)
        .into_iter()
        .next()
        .ok_or_else(|| NO_ACCOUNT.to_owned())?;
    Ok(Quick {
        tool,
        bin: bin(tool).ok_or_else(|| NO_ACCOUNT.to_owned())?,
        account,
        home: shared.config.home.clone(),
        jobs: shared.jobs.clone(),
        kind,
    })
}

/// A folder for one call in `home/plans`, removed when it goes.
struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What one process printed and how it ended.
struct Output {
    out: Vec<u8>,
    err: String,
    status: Option<i32>,
    timed_out: bool,
}

impl Quick {
    /// A side call nobody waits on (#537): a title, a summary, a sort. One attempt within
    /// [`BACKGROUND_TIMEOUT`], so during an outage it adds no load on the account the owner's turn
    /// needs.
    pub async fn background(
        &self,
        system: &str,
        user: &str,
        quoted: Option<&str>,
        schema: &str,
    ) -> Result<String, Failure> {
        self.side_call(system, user, quoted, schema, BACKGROUND_TIMEOUT)
            .await
    }

    /// [`Quick::background`] within `timeout`: a side call the owner may wait on, with more to
    /// read than a title (crate::memory, "Find what I keep repeating"). Like the conversation, it
    /// goes through the vault first, and a vault that can't be read runs nothing (#570). The call
    /// is kept either way; when it fails the caller shows what it had without the model's words.
    pub async fn side_call(
        &self,
        system: &str,
        user: &str,
        quoted: Option<&str>,
        schema: &str,
        timeout: Duration,
    ) -> Result<String, Failure> {
        let (system, user, schema, quoted) = for_brain(system, user, schema, quoted)
            .map_err(|why| Failure::Failed(format!("the vault can't be read: {why}")))?;
        let began = Instant::now();
        let (result, mut call) = match self.tool {
            Tool::Codex => {
                let model = self.codex_model();
                let (result, tokens) = self
                    .codex(&system, &user, quoted.as_deref(), &schema, timeout, &model)
                    .await;
                let call = Call::read("codex", Some(&model), None, began).with(tokens.as_ref());
                (result, call)
            }
            _ => {
                let ran = self
                    .complete(&system, &user, quoted.as_deref(), &schema, timeout)
                    .await;
                let (result, answer) = match ran {
                    Ok((answer, value)) => (Ok(answer), value),
                    Err((failure, value)) => (Err(failure), value),
                };
                let call = Call::read("claude", Some(QUICK_MODEL), answer.as_ref(), began);
                (result, call)
            }
        };
        if let Err(Failure::Failed(why) | Failure::TimedOut(why)) = &result {
            call.outcome = CallOutcome::Failed;
            tracing::warn!("brain: a {} call failed: {}: {why}", self.kind, call.line());
        }
        Record::new(self.kind, None, None, vec![call]).keep(&self.home);
        result
    }

    /// The answer as JSON, with Claude Code's result line when it printed one.
    async fn complete(
        &self,
        system: &str,
        user: &str,
        quoted: Option<&str>,
        schema: &str,
        timeout: Duration,
    ) -> Result<(String, Option<Value>), (Failure, Option<Value>)> {
        let failed = |why: String| (Failure::Failed(why), None);
        // What the model reads has the secrets obid knows taken out.
        let secrets = crate::secrets::known();
        let dir = self
            .home
            .join("plans")
            .join(uuid::Uuid::new_v4().to_string());
        crate::store::make_private_dir(&dir)
            .map_err(|e| failed(format!("the brain's folder couldn't be made: {e}")))?;
        let folder = Folder(dir);
        let mut flags: Vec<OsString> = vec![
            "-p".into(),
            "--model".into(),
            QUICK_MODEL.into(),
            "--effort".into(),
            QUICK_EFFORT.into(),
        ];
        if let Some(quoted) = quoted {
            std::fs::write(folder.0.join(QUOTED_FILE), secrets.redact(quoted))
                .map_err(|e| failed(format!("the quoted file couldn't be written: {e}")))?;
            flags.extend(read_flags().into_iter().map(OsString::from));
        } else {
            flags.extend(["--tools".into(), "".into()]);
        }
        for flag in [
            "--strict-mcp-config",
            "--setting-sources",
            "",
            "--no-session-persistence",
            "--output-format",
            "json",
        ] {
            flags.push(flag.into());
        }
        flags.extend([
            "--system-prompt".into(),
            secrets.redact(system).into(),
            "--json-schema".into(),
            secrets.redact(schema).into(),
        ]);
        // Either way it runs on a copy of its login, which nothing renews once it runs: the
        // login is refreshed first when it's due (KEYCHAIN-10). Boxed, it's the brain's account.
        let ready = if quoted.is_some() {
            None
        } else {
            Some(&self.account)
        };
        self.jobs.claude_login_ready(ready).await.map_err(failed)?;
        // With nothing to read it has no tool at all; with the quoted file it reads, boxed.
        let (launch, own_group) = if quoted.is_some() {
            let launch = self
                .jobs
                .brain_launch(Tool::Claude, &folder.0, flags)
                .map_err(failed)?;
            (launch, false)
        } else {
            let env = unboxed_env(
                |name| std::env::var_os(name),
                self.account.config_dir.as_deref(),
                &folder.0,
            )
            .map_err(failed)?;
            let launch = Launch {
                own_sandbox: false,
                group: None,
                argv: std::iter::once(self.bin.clone().into_os_string())
                    .chain(flags)
                    .collect(),
                env,
                cwd: folder.0.clone(),
                stderr: folder.0.join("stderr.log"),
            };
            (launch, true)
        };
        let result = run(&launch, own_group, secrets.redact(user).as_bytes(), timeout)
            .await
            .map_err(|e| failed(format!("Claude Code couldn't start: {e}")))?;
        let answer = serde_json::from_slice::<Value>(&result.out).ok();
        // Claude Code can report success without `structured_output`: the field is what counts.
        if let Some(plan) = answer
            .as_ref()
            .and_then(|a| a.get("structured_output"))
            .filter(|p| p.is_object())
        {
            return Ok((plan.to_string(), answer));
        }
        let subtype = answer
            .as_ref()
            .and_then(|a| a["subtype"].as_str())
            .filter(|s| *s != "success");
        let reason = if result.timed_out {
            "timeout".to_owned()
        } else if let Some(subtype) = subtype {
            // error_max_turns and the like say more than an exit code.
            subtype.to_owned()
        } else if result.out.iter().all(u8::is_ascii_whitespace) {
            "empty".to_owned()
        } else {
            format!("exit code {}", result.status.unwrap_or(-1))
        };
        // What Claude said, like "Not logged in · Please run /login" (#626): only its own error,
        // never text the model wrote nor anything that looks like a credential, on one line, the
        // vault's secrets taken out before it's cut (#640's security review).
        let reason = match answer
            .as_ref()
            .filter(|a| a["is_error"] == true)
            .and_then(|a| a["result"].as_str())
        {
            Some(said) if !said.trim().is_empty() && crate::scan::matches(said).is_empty() => {
                let said = secrets
                    .redact(said)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{reason}: {}", said.chars().take(300).collect::<String>())
            }
            _ => reason,
        };
        let err = if result.err.is_empty() {
            "<empty>".to_owned()
        } else {
            let chars: Vec<char> = secrets.redact(&result.err).chars().collect();
            chars[chars.len().saturating_sub(1000)..].iter().collect()
        };
        let why = format!("claude returned no answer: {reason}, stderr: {err}");
        Err(if result.timed_out {
            (Failure::TimedOut(why), answer)
        } else {
            (Failure::Failed(why), answer)
        })
    }

    /// The Codex account's model, as the owner set it in Settings › Accounts, else Codex's own.
    fn codex_model(&self) -> String {
        let offered = self
            .jobs
            .model_lists
            .to_check(Tool::Codex, &self.account.id);
        crate::settings::account_model(&self.home, Tool::Codex, &self.account.id, &offered)
    }

    /// The answer as JSON from one Codex turn on `model`, with what it used: boxed like a
    /// conversation's Codex (a copy of the login as its `CODEX_HOME`, nothing else of the
    /// account's), every feature of [`super::harness_codex::OFF`] and [`CODEX_OFF`] off and no web
    /// search, checked before it runs. Its working folder is read-only and holds only the quoted
    /// file, which it reads through obid's own Read tool, on that file alone (a Read of any other
    /// is refused). Any other tool, or a request obid doesn't serve, ends the call with no answer.
    async fn codex(
        &self,
        system: &str,
        user: &str,
        quoted: Option<&str>,
        schema: &str,
        timeout: Duration,
        model: &str,
    ) -> (Result<String, Failure>, Option<crate::usage::Tokens>) {
        let failed = |why: String| (Err(Failure::Failed(why)), None);
        let secrets = crate::secrets::known();
        let Ok(schema) = serde_json::from_str::<Value>(&secrets.redact(schema)) else {
            return failed("the side call's schema isn't JSON".into());
        };
        let dir = self
            .home
            .join("plans")
            .join(uuid::Uuid::new_v4().to_string());
        let (reads, temp) = (dir.join("reads"), dir.join("home"));
        for made in [&dir, &reads, &temp] {
            if let Err(e) = crate::store::make_private_dir(made) {
                return failed(format!("the brain's folder couldn't be made: {e}"));
            }
        }
        let folder = Folder(dir);
        if let Some(quoted) = quoted
            && let Err(e) = std::fs::write(reads.join(QUOTED_FILE), secrets.redact(quoted))
        {
            return failed(format!("the quoted file couldn't be written: {e}"));
        }
        let mut flags: Vec<OsString> = vec![];
        for feature in super::harness_codex::OFF.iter().chain(CODEX_OFF) {
            flags.extend(["-c".into(), format!("features.{feature}=false").into()]);
        }
        flags.extend(["-c".into(), r#"web_search="disabled""#.into()]);
        let launch = match self.jobs.conversation_launch(
            Tool::Codex,
            &self.account,
            &reads,
            &temp,
            &folder.0.join("login"),
            vec![reads.clone()],
            flags,
            folder.0.join("stderr.log"),
        ) {
            Ok(launch) => launch,
            Err(why) => return failed(why),
        };
        if let Err(why) = super::harness_codex::features_off_with(&launch, CODEX_OFF).await {
            return failed(why);
        }
        // Kept until the call ends: a stop channel whose sender is gone stops the server.
        let (_run, stop) = tokio::sync::watch::channel(crate::process::Signal::Run);
        let conn = match crate::conn::Conn::spawn(&launch, timeout, stop, false).await {
            Ok(conn) => conn,
            Err(e) => return failed(format!("Codex couldn't start: {e}")),
        };
        let mut turn = CodexTurn {
            conn,
            reads,
            thread: None,
            said: None,
            tokens: None,
            before: None,
        };
        let user = secrets.redact(user);
        let system = secrets.redact(system);
        let answer = turn
            .run(&system, &user, quoted.is_some(), schema, model)
            .await;
        let tokens = turn.tokens.take();
        // Nothing of the call outlives it: no thread is kept (it's ephemeral) nor resumed.
        turn.conn.finish_within(Duration::ZERO).await;
        let answer = answer.map_err(|(timed_out, why)| {
            let why = format!("codex returned no answer: {}", secrets.redact(&why));
            match timed_out {
                true => Failure::TimedOut(why),
                false => Failure::Failed(why),
            }
        });
        (answer, tokens)
    }
}

/// The one tool a Codex side call has when it may read the quoted file: Read, on that file alone.
fn codex_read_tool() -> Value {
    serde_json::json!([{"type": "function", "name": "Read",
        "description": format!("Read {QUOTED_FILE} in the working folder."),
        "inputSchema": {"type": "object", "additionalProperties": false, "required": ["file_path"],
            "properties": {"file_path": {"type": "string", "enum": [QUOTED_FILE]}}}}])
}

/// Codex's items a side call may make: its words and thinking, and obid's Read
/// (crate::codex's items that are no tool).
const CODEX_ITEMS: &[&str] = &[
    "agentMessage",
    "userMessage",
    "reasoning",
    "plan",
    "contextCompaction",
    "dynamicToolCall",
];

/// One Codex side call's turn, over its `app-server`.
struct CodexTurn {
    conn: crate::conn::Conn,
    /// Its working folder, holding the quoted file alone.
    reads: PathBuf,
    thread: Option<String>,
    /// Its last words, the answer.
    said: Option<String>,
    /// What the turn used, as Codex counted it.
    tokens: Option<crate::usage::Tokens>,
    before: Option<crate::usage::Tokens>,
}

/// Why a Codex side call has no answer: whether it ran out of time, and why.
type Why = (bool, String);

impl CodexTurn {
    /// The answer, a JSON object as `schema` asks: a new ephemeral thread told `system`, one turn
    /// of `user` with `schema` as its output schema, Read offered when `reads_file`.
    async fn run(
        &mut self,
        system: &str,
        user: &str,
        reads_file: bool,
        schema: Value,
        model: &str,
    ) -> Result<String, Why> {
        use serde_json::json;
        let version = env!("CARGO_PKG_VERSION");
        self.call(
            "initialize",
            json!({"clientInfo": {"name": "obid", "title": "obid", "version": version},
                   "capabilities": {"experimentalApi": true}}),
        )
        .await?;
        self.conn.notify("initialized", None).await;
        // Its own sandbox can't run inside obid's box: every approval is obid's, and it has no
        // shell to ask one for.
        let mut thread = json!({"cwd": self.reads, "sandbox": "danger-full-access",
            "approvalPolicy": "untrusted", "approvalsReviewer": "user", "ephemeral": true,
            "baseInstructions": system});
        if !model.is_empty() {
            thread["model"] = json!(model);
        }
        if reads_file {
            thread["dynamicTools"] = codex_read_tool();
        }
        let started = self.call("thread/start", thread).await?;
        let thread = started["thread"]["id"]
            .as_str()
            .ok_or((false, "Codex started no thread".to_owned()))?
            .to_owned();
        self.thread = Some(thread.clone());
        let mut turn = json!({"threadId": thread, "outputSchema": schema, "effort": QUICK_EFFORT,
            "input": [{"type": "text", "text": user, "text_elements": []}]});
        if !model.is_empty() {
            turn["model"] = json!(model);
        }
        self.call("turn/start", turn).await?;
        loop {
            let message = self.next().await?;
            self.handle(&message).await?;
            if message["method"] != "turn/completed" {
                continue;
            }
            let turn = &message["params"]["turn"];
            if turn["status"] != "completed" {
                let said = turn["error"]["message"].as_str().unwrap_or_default();
                let status = turn["status"].as_str().unwrap_or("with no status");
                return Err((false, format!("its turn ended {status}: {said}")));
            }
            return self
                .said
                .take()
                .filter(|said| serde_json::from_str::<Value>(said).is_ok_and(|v| v.is_object()))
                .ok_or((false, "its answer isn't the schema's".to_owned()));
        }
    }

    /// Sends a request and reads until its answer comes, handling everything else on the way.
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, Why> {
        let id = self.conn.request(method, params).await;
        loop {
            let message = self.next().await?;
            if let Some(answer) = crate::conn::answer_to(&message, id) {
                return answer
                    .map_err(|e| (false, format!("Codex refused {method}: {}", e.message)));
            }
            self.handle(&message).await?;
        }
    }

    async fn next(&mut self) -> Result<Value, Why> {
        self.conn.next().await.map_err(|cut| match cut {
            crate::conn::Cut::TimedOut => (true, "timeout".to_owned()),
            _ => (false, "Codex ended before its answer".to_owned()),
        })
    }

    /// One message of Codex's: its Read of the quoted file served, its words kept, what it used
    /// counted. Anything else it asks for, or does, ends the call.
    async fn handle(&mut self, message: &Value) -> Result<(), Why> {
        let params = &message["params"];
        let method = message["method"].as_str().unwrap_or_default();
        if let Some(id) = crate::conn::request_id(message).cloned() {
            if method != "item/tool/call" || params["tool"] != "Read" {
                self.conn.refuse(&id).await;
                return Err((false, format!("Codex asked for {method} outside Read")));
            }
            // Any other file, its login's among them, is refused: the call goes on without it.
            let text = params["arguments"]["file_path"]
                .as_str()
                .filter(|path| {
                    *path == QUOTED_FILE || Path::new(path) == self.reads.join(QUOTED_FILE)
                })
                .and_then(|_| std::fs::read_to_string(self.reads.join(QUOTED_FILE)).ok());
            let answer = serde_json::json!({"success": text.is_some(), "contentItems": [{
                "type": "inputText", "text": text.unwrap_or_else(|| format!("Only {QUOTED_FILE} may be read."))}]});
            self.conn.reply(&id, answer).await;
            return Ok(());
        }
        if let Some(thread) = params["threadId"].as_str()
            && self.thread.as_deref().is_some_and(|own| own != thread)
        {
            return Err((false, "Codex spoke on another thread".into()));
        }
        match method {
            "item/started" | "item/completed" => {
                let item = &params["item"];
                let kind = item["type"].as_str().unwrap_or_default();
                if !CODEX_ITEMS.contains(&kind) {
                    return Err((false, format!("Codex used {kind} outside Read")));
                }
                if method == "item/completed" && kind == "agentMessage" {
                    self.said = item["text"].as_str().map(str::to_owned);
                }
            }
            "thread/tokenUsage/updated" => {
                self.tokens = crate::usage::codex_turn(&params["tokenUsage"], &mut self.before);
            }
            _ => {}
        }
        Ok(())
    }
}

/// What the model gets, with the vault's secrets taken out ([`crate::vault::for_agent`]), or why
/// the vault can't be read.
fn for_brain(
    system: &str,
    user: &str,
    schema: &str,
    quoted: Option<&str>,
) -> Result<(String, String, String, Option<String>), String> {
    crate::vault::check().map_err(|why| {
        why.trim_start_matches("the vault can't be read: ")
            .to_owned()
    })?;
    let clean = |text: &str| {
        crate::vault::for_agent(text).map_err(|_| "it couldn't be read just now.".to_owned())
    };
    Ok((
        clean(system)?,
        clean(user)?,
        clean(schema)?,
        quoted.map(clean).transpose()?,
    ))
}

/// A side call's tools when it has a quoted file (#642): Read, allowed on that file alone, and
/// denied on its box's login and home ([`crate::jobs::Jobs::brain_launch`] copies the login into
/// its working folder, where Claude Code reads without asking) and on any login file, so text
/// quoted in the file can't have it read the login into its answer. A deny rule wins over any
/// allow.
fn read_flags() -> Vec<String> {
    vec![
        "--tools".into(),
        "Read".into(),
        "--allowedTools".into(),
        format!("Read(./{QUOTED_FILE})"),
        "--disallowedTools".into(),
        "Read(./login/**)".into(),
        "Read(./home/**)".into(),
        "Read(//**/.credentials.json)".into(),
        "--max-turns".into(),
        READ_TURNS.into(),
    ]
}

/// A side call's environment when it runs unboxed, from `get` (obid's own environment): what a job
/// keeps of it (`USER` and `LOGNAME` among them: on macOS Claude Code finds its login in the
/// keychain with them, #564) and the person's `HOME`. Never a Claude Code, Anthropic or GitHub
/// variable (AGENTS.md, "Spawning claude").
///
/// With the default login (`account` none), Claude Code's own `~/.claude`, which no box can write,
/// and the tools' `PATH`. With an account's own folder, never that folder (#651's re-review): the
/// Mac app's Swift engine still runs agents unboxed with it (#655's open item), and Claude Code
/// applies a `.claude.json`'s `env` from its config folder whatever `--setting-sources` says, so
/// whatever is planted there (a `PATH` to a `security` of its own, plug-in folders) would run
/// outside any box. Its config folder is then a new private one in the call's folder `dir`,
/// holding only obid's copy of the login, what calls the API (`crate::jobs::account_login`; none:
/// Claude Code says it isn't signed in), and `PATH` is the system's folders. Throws why that
/// folder couldn't be made.
fn unboxed_env(
    get: impl Fn(&str) -> Option<OsString>,
    account: Option<&Path>,
    dir: &Path,
) -> Result<Vec<(OsString, OsString)>, String> {
    let mut env = crate::process::kept(&get);
    env.push(("HOME".into(), get("HOME").unwrap_or_default()));
    match account {
        None => env.push(("PATH".into(), crate::process::tools_path())),
        Some(account) => {
            let config = dir.join("config");
            let mut builder = std::fs::DirBuilder::new();
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            builder
                .create(&config)
                .map_err(|e| format!("the brain's config folder couldn't be made: {e}"))?;
            if let Some(login) = crate::jobs::account_login(account) {
                crate::jobs::write_login(&config.join(".credentials.json"), &login)
                    .map_err(|e| format!("the brain's login couldn't be copied: {e}"))?;
            }
            env.push(("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()));
            env.push(("CLAUDE_CONFIG_DIR".into(), config.into_os_string()));
        }
    }
    env.retain(|(name, _)| !crate::process::is_forbidden(&name.to_string_lossy()));
    Ok(env)
}

/// Ends the process group of a run whose wait was dropped.
struct EndsGroup(u32);

impl Drop for EndsGroup {
    fn drop(&mut self) {
        if self.0 != 0 {
            crate::process::signal_group(self.0, nix::sys::signal::Signal::SIGKILL);
        }
    }
}

/// Runs `launch` with `input` on its stdin, for at most `limit`: what it printed and how it ended.
async fn run(
    launch: &Launch,
    own_group: bool,
    input: &[u8],
    limit: Duration,
) -> std::io::Result<Output> {
    let mut command = crate::process::command(&launch.argv, &launch.env, &launch.cwd)?;
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // A boxed command starts a session of its own, so a process group whose id is the child's.
    if own_group {
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let pid = child.id().unwrap_or(0);
    let mut group = EndsGroup(pid);
    if let Some(mut stdin) = child.stdin.take() {
        // An answer that comes before it read everything is still an answer.
        let _ = stdin.write_all(input).await;
    }
    let waited = tokio::time::timeout(limit, child.wait_with_output()).await;
    // It ended (or timed out, below): nothing is left to end when this returns.
    group.0 = 0;
    match waited {
        Ok(output) => {
            let output = output?;
            Ok(Output {
                out: output.stdout,
                err: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                status: output.status.code(),
                timed_out: false,
            })
        }
        Err(_) => {
            if pid != 0 {
                crate::process::end_group(pid, Duration::from_millis(100)).await;
            }
            Ok(Output {
                out: vec![],
                err: String::new(),
                status: None,
                timed_out: true,
            })
        }
    }
}

/// How one model call ended.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallOutcome {
    /// It answered (the plan loop's records before 2026-10-27 say `plan` or `refused`).
    #[serde(alias = "plan", alias = "refused")]
    Answered,
    /// No answer: the process failed, timed out, or ended without one (`error_max_turns`).
    Failed,
}

/// One model call of the brain: one process, one Codex turn, or one run of a harness turn (#533).
/// The numbers are what the CLI reported (Claude Code's `result` line; Codex's `tokenUsage`),
/// `None` when it reported nothing, never zero; `wall_ms` is what obid measured, from start to
/// end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub adapter: String,
    pub model: Option<String>,
    pub outcome: CallOutcome,
    pub subtype: Option<String>,
    pub duration_ms: Option<u64>,
    pub wall_ms: u64,
    pub num_turns: Option<u64>,
    pub total_cost_usd: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
}

impl Call {
    /// The call `answer` reports (Claude Code's result line, when it printed one), which began at
    /// `began`. It counts as answered until its caller says otherwise.
    pub fn read(
        adapter: &str,
        model: Option<&str>,
        answer: Option<&Value>,
        began: Instant,
    ) -> Call {
        let field = |key: &str| answer.map_or(&Value::Null, |a| &a[key]);
        let usage = |key: &str| field("usage")[key].as_u64();
        Call {
            adapter: adapter.to_owned(),
            model: model.filter(|m| !m.is_empty()).map(str::to_owned),
            outcome: CallOutcome::Answered,
            subtype: field("subtype").as_str().map(str::to_owned),
            duration_ms: field("duration_ms").as_u64(),
            wall_ms: u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX),
            num_turns: field("num_turns").as_u64(),
            total_cost_usd: field("total_cost_usd").as_f64(),
            input_tokens: usage("input_tokens"),
            output_tokens: usage("output_tokens"),
            cache_read_input_tokens: usage("cache_read_input_tokens"),
            cache_creation_input_tokens: usage("cache_creation_input_tokens"),
        }
    }

    /// The call, its missing numbers taken from what the agent's events said it used
    /// (crate::usage::Tokens: Codex's `tokenUsage`, whose turn has no result line of Claude
    /// Code's).
    pub fn with(mut self, tokens: Option<&crate::usage::Tokens>) -> Call {
        if let Some(tokens) = tokens {
            self.input_tokens = self.input_tokens.or(tokens.input);
            self.output_tokens = self.output_tokens.or(tokens.output);
            self.cache_read_input_tokens = self.cache_read_input_tokens.or(tokens.cache_read);
            self.cache_creation_input_tokens =
                self.cache_creation_input_tokens.or(tokens.cache_write);
            self.total_cost_usd = self.total_cost_usd.or(tokens.cost_usd);
        }
        self
    }

    /// The call in a few words, with only the numbers it has: `claude claude-haiku-5-5 answered
    /// (success, 3120 ms, 1 turn, 1200 in, 80 out, 900 cache read, 300 cache written, $0.0042,
    /// 3400 ms waited)`.
    pub fn line(&self) -> String {
        let mut numbers: Vec<String> = vec![];
        if let Some(subtype) = &self.subtype {
            numbers.push(subtype.clone());
        }
        if let Some(ms) = self.duration_ms {
            numbers.push(format!("{ms} ms"));
        }
        if let Some(turns) = self.num_turns {
            numbers.push(format!("{turns} turn{}", if turns == 1 { "" } else { "s" }));
        }
        for (count, what) in [
            (self.input_tokens, "in"),
            (self.output_tokens, "out"),
            (self.cache_read_input_tokens, "cache read"),
            (self.cache_creation_input_tokens, "cache written"),
        ] {
            if let Some(count) = count {
                numbers.push(format!("{count} {what}"));
            }
        }
        if let Some(cost) = self.total_cost_usd {
            numbers.push(format!("${cost:.4}"));
        }
        numbers.push(format!("{} ms waited", self.wall_ms));
        let outcome = match self.outcome {
            CallOutcome::Answered => "answered",
            CallOutcome::Failed => "failed",
        };
        let model = self
            .model
            .as_deref()
            .map_or(String::new(), |m| format!(" {m}"));
        format!("{}{model} {outcome} ({})", self.adapter, numbers.join(", "))
    }
}

/// One record of the brain's spend: what the calls were for and each call's numbers. Kept in
/// `home/brain-turns.jsonl`, the last [`KEPT`], never with anyone's words.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub at: String,
    /// `conversation` for a harness turn (`supervisor` or `event` in records from before they
    /// were retired); `triage`, `memory` or `recap`
    /// for a side call; `plan` for the plan loop's records from before 2026-10-27.
    #[serde(default = "plan_loop")]
    pub kind: String,
    #[serde(default)]
    pub conversation: Option<String>,
    #[serde(default)]
    pub turn: Option<String>,
    #[serde(default)]
    pub calls: Vec<Call>,
}

fn plan_loop() -> String {
    "plan".into()
}

impl Record {
    pub fn new(
        kind: &str,
        conversation: Option<&str>,
        turn: Option<&str>,
        calls: Vec<Call>,
    ) -> Record {
        Record {
            at: obi_protocol::time::format(obi_protocol::time::now()),
            kind: kind.to_owned(),
            conversation: conversation.map(str::to_owned),
            turn: turn.map(str::to_owned),
            calls,
        }
    }

    /// Logs the record and keeps it, the last [`KEPT`] in `home/brain-turns.jsonl` (0600).
    // ponytail: rewrites the whole file each time; an append-only log once 500 lines are slow.
    pub fn keep(&self, home: &Path) {
        let calls: Vec<String> = self.calls.iter().map(Call::line).collect();
        tracing::info!("brain: {} {}", self.kind, calls.join(" | "));
        let file = home.join("brain-turns.jsonl");
        // Kept one at a time: two calls ending together never lose one.
        static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _writing = WRITING.lock().unwrap_or_else(|e| e.into_inner());
        let mut lines: Vec<String> = std::fs::read_to_string(&file)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        match serde_json::to_string(self) {
            Ok(line) => lines.push(line),
            Err(e) => return tracing::warn!("brain: can't keep the record: {e}"),
        }
        let excess = lines.len().saturating_sub(KEPT);
        lines.drain(..excess);
        let mut text = lines.join("\n");
        text.push('\n');
        if let Err(e) = crate::store::write_private(&file, text.as_bytes()) {
            tracing::warn!("brain: can't keep the record in {}: {e}", file.display());
        }
    }
}

/// The records kept, oldest first.
pub fn records(home: &Path) -> Vec<Record> {
    std::fs::read_to_string(home.join("brain-turns.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// What some calls spent, added up.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cost_usd: f64,
}

impl Totals {
    fn add(&mut self, call: &Call) {
        self.calls += 1;
        self.input_tokens += call.input_tokens.unwrap_or(0);
        self.output_tokens += call.output_tokens.unwrap_or(0);
        self.cache_read_input_tokens += call.cache_read_input_tokens.unwrap_or(0);
        self.cache_creation_input_tokens += call.cache_creation_input_tokens.unwrap_or(0);
        self.cost_usd += call.total_cost_usd.unwrap_or(0.0);
    }
}

/// Every kept call added up, and each kind's: what the brain spent since the oldest record.
pub fn totals(records: &[Record]) -> (Totals, BTreeMap<String, Totals>) {
    let mut all = Totals::default();
    let mut kinds: BTreeMap<String, Totals> = BTreeMap::new();
    for record in records {
        for call in &record.calls {
            all.add(call);
            kinds.entry(record.kind.clone()).or_default().add(call);
        }
    }
    (all, kinds)
}

/// `GET /v1/brain/usage`: what the brain spent since the oldest record kept (`since`, none with
/// no record), in all and by kind, as the CLIs reported it.
pub async fn usage(State(shared): State<Arc<Shared>>) -> axum::Json<Value> {
    let records = records(&shared.config.home);
    let (all, kinds) = totals(&records);
    let mut body = serde_json::to_value(all).unwrap_or_default();
    body["since"] = records.first().map(|r| r.at.clone()).into();
    body["kinds"] = serde_json::to_value(kinds).unwrap_or_default();
    axum::Json(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #642: a quoted file asking a side call to read its login gets nothing from it. Its Read is
    /// allowed on work.md alone, and denied on the box's login and home, where obid copies the
    /// login, and on any login file anywhere: a deny rule wins over Claude Code's own reads in its
    /// working folder.
    #[test]
    fn a_side_call_may_read_its_quoted_file_and_never_the_login() {
        let flags = read_flags();
        let after = |flag: &str| -> Vec<String> {
            flags
                .iter()
                .skip_while(|f| *f != flag)
                .skip(1)
                .take_while(|f| !f.starts_with("--"))
                .cloned()
                .collect()
        };
        assert_eq!(after("--tools"), ["Read"]);
        assert_eq!(after("--allowedTools"), [format!("Read(./{QUOTED_FILE})")]);
        let denied = after("--disallowedTools");
        for rule in [
            "Read(./login/**)",
            "Read(./home/**)",
            "Read(//**/.credentials.json)",
        ] {
            assert!(denied.iter().any(|d| d == rule), "{rule} in {denied:?}");
        }
        assert!(!denied.iter().any(|rule| rule.contains(QUOTED_FILE)));
    }

    /// #564: unboxed, a side call keeps what a job keeps of obid's environment, `USER` and
    /// `LOGNAME` among them (on macOS Claude Code finds its login with them), and never a Claude
    /// Code, Anthropic or GitHub variable, even when obid has one.
    #[test]
    fn the_unboxed_brain_keeps_user_and_logname_and_nothing_forbidden() {
        let obids = [
            ("HOME", "/Users/someone"),
            ("USER", "someone"),
            ("LOGNAME", "someone"),
            ("LANG", "en_US.UTF-8"),
            ("HTTPS_PROXY", "http://proxy:3128"),
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_ENTRYPOINT", "sdk"),
            ("ANTHROPIC_API_KEY", "sk-test"),
            ("GH_TOKEN", "gho_test"),
            ("SECRET_OF_OBID", "x"),
        ];
        let get = |name: &str| {
            obids
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| OsString::from(v))
        };
        let env = unboxed_env(get, None, Path::new("/nowhere")).unwrap();
        let value = |name: &str| {
            env.iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.to_string_lossy().into_owned())
        };
        for (name, expected) in [
            ("USER", "someone"),
            ("LOGNAME", "someone"),
            ("LANG", "en_US.UTF-8"),
            ("HTTPS_PROXY", "http://proxy:3128"),
            ("HOME", "/Users/someone"),
        ] {
            assert_eq!(value(name).as_deref(), Some(expected), "{name} in {env:?}");
        }
        assert!(value("PATH").is_some());
        // The default login: its own folder, ~/.claude, which no box can write.
        for name in [
            "CLAUDE_CONFIG_DIR",
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "ANTHROPIC_API_KEY",
            "GH_TOKEN",
            "SECRET_OF_OBID",
        ] {
            assert_eq!(value(name), None, "{name} in {env:?}");
        }
    }

    /// #651's re-review: unboxed, a side call never reads an account's folder, which the Mac app's
    /// Swift engine can still write unboxed (#655's open item). A `.claude.json` planted there has
    /// its `env` applied by Claude Code whatever `--setting-sources` says (a `PATH` to a
    /// `security` of its own, plug-in folders), which would run outside any box on the next call.
    /// Its config folder is a new private one in the call's folder, holding only obid's copy of
    /// the login (what calls the API), and its `PATH` is the system's folders.
    #[test]
    fn the_unboxed_brain_is_never_steered_by_the_accounts_folder() {
        let root = std::env::temp_dir().join(format!("obid-brain-steer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let account = root.join("account");
        std::fs::create_dir_all(account.join("bin")).unwrap();
        std::fs::write(account.join("bin/security"), "#!/bin/sh\n").unwrap();
        let planted = serde_json::json!({"env": {"PATH": account.join("bin"),
            "CLAUDE_CODE_PLUGIN_DIRS": account.join("plugins")}});
        std::fs::write(account.join(".claude.json"), planted.to_string()).unwrap();
        let login = serde_json::json!({"claudeAiOauth": {"accessToken": "a", "refreshToken": "r",
            "scopes": ["user:inference"]}});
        std::fs::write(account.join(".credentials.json"), login.to_string()).unwrap();
        let call = root.join("call");
        std::fs::create_dir_all(&call).unwrap();
        let get = |name: &str| (name == "HOME").then(|| OsString::from("/Users/someone"));
        let env = unboxed_env(get, Some(&account), &call).unwrap();
        let value = |name: &str| {
            env.iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| PathBuf::from(v))
        };
        let config = value("CLAUDE_CONFIG_DIR").unwrap();
        assert!(config.starts_with(&call), "{}", config.display());
        assert_eq!(
            value("PATH"),
            Some(PathBuf::from("/usr/bin:/bin:/usr/sbin:/sbin"))
        );
        for (name, value) in &env {
            assert!(
                !value.to_string_lossy().contains("account"),
                "{name:?}={value:?}"
            );
        }
        // Only the login, without its refresh token: nothing else of the account's folder.
        let names: Vec<_> = std::fs::read_dir(&config)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [".credentials.json"]);
        let copy: serde_json::Value =
            serde_json::from_slice(&std::fs::read(config.join(".credentials.json")).unwrap())
                .unwrap();
        assert_eq!(
            copy,
            serde_json::json!({"claudeAiOauth": {"accessToken": "a", "scopes": ["user:inference"]}})
        );
        let mode = std::fs::metadata(&config).unwrap().permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o700
        );
        // A second call in the same folder doesn't take a folder already there.
        assert!(unboxed_env(get, Some(&account), &call).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A call's numbers come from Claude Code's `result` line, those it lacks from the agent's
    /// own count (Codex's), and the kept records add up by kind, the plan loop's old ones too.
    #[test]
    fn calls_are_recorded_from_the_result_line_and_added_up() {
        let result = serde_json::json!({"type": "result", "subtype": "success", "num_turns": 2,
            "duration_ms": 900, "total_cost_usd": 0.01,
            "usage": {"input_tokens": 3, "output_tokens": 40,
                      "cache_read_input_tokens": 8000, "cache_creation_input_tokens": 500}});
        let claude = Call::read(
            "claude",
            Some("claude-sonnet-5-5"),
            Some(&result),
            Instant::now(),
        );
        assert_eq!(
            (
                claude.input_tokens,
                claude.output_tokens,
                claude.cache_read_input_tokens,
                claude.cache_creation_input_tokens
            ),
            (Some(3), Some(40), Some(8000), Some(500))
        );
        assert_eq!(claude.num_turns, Some(2));
        let codex_tokens = crate::usage::Tokens {
            input: Some(100),
            output: Some(20),
            cache_read: Some(900),
            ..Default::default()
        };
        let codex = Call::read("codex", Some(""), None, Instant::now()).with(Some(&codex_tokens));
        assert_eq!(codex.model, None);
        assert_eq!(codex.input_tokens, Some(100));
        assert_eq!(codex.cache_creation_input_tokens, None);
        let home = std::env::temp_dir().join(format!("obid-calls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        // A line the plan loop kept before: its words and plan are left out of what's read.
        std::fs::write(
            home.join("brain-turns.jsonl"),
            r#"{"at":"2026-10-27T23:30:39Z","conversation":"orchestrator","turn":"t","step":2,"said":"x","by":"claude","plan":{"action":"read_job"},"refused":[],"calls":[{"adapter":"claude","model":"claude-sonnet-5-5","attempt":1,"outcome":"plan","subtype":"success","duration_ms":3190,"wall_ms":6368,"num_turns":2,"total_cost_usd":0.02,"input_tokens":2,"output_tokens":390,"cache_read_input_tokens":8519,"cache_creation_input_tokens":3446}]}"#.to_owned() + "\n",
        )
        .unwrap();
        Record::new("conversation", Some("c"), Some("t"), vec![claude]).keep(&home);
        Record::new("event", None, None, vec![codex]).keep(&home);
        let kept = records(&home);
        assert_eq!(
            kept.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(),
            ["plan", "conversation", "event"]
        );
        assert_eq!(kept[0].calls[0].outcome, CallOutcome::Answered);
        let (all, kinds) = totals(&kept);
        assert_eq!(all.calls, 3);
        assert_eq!(all.input_tokens, 2 + 3 + 100);
        assert_eq!(all.cache_read_input_tokens, 8519 + 8000 + 900);
        assert_eq!(kinds["event"].output_tokens, 20);
        assert_eq!(kinds["plan"].cache_creation_input_tokens, 3446);
        let _ = std::fs::remove_dir_all(&home);
    }
}
