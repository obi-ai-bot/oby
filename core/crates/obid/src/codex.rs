//! The Codex adapter: one turn of `codex app-server`, JSON-RPC over stdio (AGENTS.md, "Codex";
//! Engine/Codex.swift is the behaviour it follows). obid sends `initialize` and `initialized`, then
//! `thread/start` (or `thread/resume` with the job's session), then `turn/start` with the round's
//! message; it answers every approval Codex asks for, follows the turn's items as steps, and once
//! `turn/completed` comes for its thread it closes stdin, so the server exits.
//!
//! obid's job sandbox is Codex's sandbox, on Linux and on macOS: Codex's own (bubblewrap, Seatbelt)
//! can't run inside another, so the thread runs `danger-full-access` with `approvalPolicy`
//! `untrusted`, and every command that isn't a known read, and every file change, comes to obid as
//! an approval (as the Swift runner does inside bubblewrap). In Plan mode obid declines file changes.
//! An own run (the owner's Bypass job, LANES-18) runs unboxed instead, its commands in Codex's own
//! sandbox as its `-c` settings say (crate::jobs, `own_codex_settings`), with `approvalPolicy`
//! `never`, so it never asks to leave it: it starts only once Codex says those settings are the
//! ones that hold ([`own_config_problem`], [`own_thread_problem`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::watch;

use crate::agent::{self, Answer, Cause, Event, Failure, Launch, Ruling, Turn};
use crate::conn::{self, Conn, Cut, RpcError};
use crate::process;

/// What a turn needs besides its message.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The thread to pick up again (the job's `session`), or `None` for a new one.
    pub resume: Option<String>,
    /// The job's own model, or `None` for Codex's.
    pub model: Option<String>,
    /// The job's reasoning effort (its account's, #1574), or `None` for Codex's.
    pub effort: Option<String>,
    /// The job is in Plan mode: Codex reads and plans, and obid declines its file changes.
    pub plan: bool,
    /// What the thread is told after Codex's own instructions (`developerInstructions`): a
    /// conversation's orientation.
    pub instructions: Option<String>,
    /// A conversation's thread, whose shell is off: a command that runs anyway ends the turn.
    pub no_commands: bool,
    /// A job's run: it asks Codex for the account's usage as it starts (crate::usage). The
    /// brain's and conversations' runs don't: nothing keeps it there.
    pub usage: bool,
    /// How the job's account pays (`api` or `plan`), when obid knows it already: then the run
    /// doesn't ask `account/read`.
    pub priced: Option<String>,
    /// On a ChatGPT login (crate::chatgpt): the thread starts with the model (OpenAI's page has it
    /// so, and the provider has no default of its own) and Codex says it's Oby, the name it signed
    /// in under.
    pub chatgpt: bool,
}

/// The `command` line `codex app-server` runs as: the binary, then `app-server`. Oby's broker
/// (`mcp`) and Oby's Tetrix server (`tetrix`, crate::tetrix) take the job's token from the
/// environment, never an argument.
pub fn argv(
    bin: PathBuf,
    mcp: Option<(&str, &str)>,
    tetrix: Option<&str>,
) -> Vec<std::ffi::OsString> {
    let mut argv = vec![bin.into_os_string()];
    let servers = mcp
        .map(|(url, _)| ("oby", url))
        .into_iter()
        .chain(tetrix.map(|url| ("tetrix", url)));
    for (name, url) in servers {
        argv.extend([
            "-c".into(),
            format!("mcp_servers.{name}.url=\"{url}\"").into(),
            "-c".into(),
            format!("mcp_servers.{name}.bearer_token_env_var=\"OBY_MCP_TOKEN\"").into(),
        ]);
    }
    argv.push("app-server".into());
    argv
}

/// The MCP servers [`argv`] gives Codex, the only ones that may start or run.
const OWN_SERVERS: [&str; 2] = ["oby", "tetrix"];

/// Runs one turn: starts the server, sends `prompt`, answers approvals with `decide`, reports with
/// `on_event`, and ends the server once the turn completes. Ends it and everything it started when
/// `stop` turns or `limit` passes.
pub async fn run_turn(
    launch: Launch,
    prompt: &str,
    options: &Options,
    limit: impl Into<crate::conn::Limit>,
    stop: watch::Receiver<process::Signal>,
    decide: impl Fn(&str, &Value) -> Ruling,
    on_event: impl FnMut(Event),
) -> std::io::Result<Turn> {
    let conn = Conn::spawn(&launch, limit, stop, false).await?;
    let mut run = Run {
        conn,
        decide,
        on_event,
        worktree: launch.cwd.clone(),
        plan: options.plan,
        own: launch.own_sandbox,
        files: HashMap::new(),
        calls: HashMap::new(),
        no_commands: options.no_commands,
        limits_read: None,
        account_read: None,
        tokens_before: None,
        priced: options.priced.clone(),
        fatal: None,
        thread: None,
        helpers: Default::default(),
        turn: None,
        said: None,
        reasoning: HashMap::new(),
        ended: None,
        failure: None,
        resets: None,
        chatgpt: options.chatgpt,
    };
    let mut turn = Turn::default();
    match run.turn(prompt, options).await {
        Ok(()) => {}
        Err(Stop::Cut(Cut::Stopped)) => {
            // Courtesy only: the group ends a moment later whatever Codex does with it.
            if let (Some(thread), Some(id)) = (run.thread.clone(), run.turn.clone()) {
                run.conn
                    .request("turn/interrupt", json!({"threadId": thread, "turnId": id}))
                    .await;
            }
        }
        Err(Stop::Cut(_)) => {}
        Err(Stop::Failed(why)) => {
            run.failure.get_or_insert_with(|| Failure::said(&why));
            run.ended = Some((why, true));
        }
    }
    turn.session = run.thread.clone();
    turn.fatal = run.fatal.take();
    turn.resets = run.resets;
    turn.failure = run.failure.take();
    if let Some((text, failed)) = &run.ended {
        turn.result = Some(agent::result(text, *failed, run.thread.as_deref()));
    }
    // The turn is over: no more input, so the server exits. It gets what a Claude Code turn gets
    // after its result (`AFTER_RESULT`, #658) to read out and exit by itself, then is ended.
    run.conn.close_input();
    let until = std::time::Instant::now() + agent::AFTER_RESULT;
    if run.ended.is_some() && turn.fatal.is_none() {
        run.conn.drain_within(agent::AFTER_RESULT).await;
    }
    turn.timed_out = run.conn.timed_out;
    turn.quiet = run.conn.quiet_out;
    turn.idle = run.conn.idle_out;
    turn.stopped = run.conn.stopped;
    turn.exit = if turn.fatal.is_some() {
        // Ended for good: everything it started ends now, not after a grace.
        run.conn.finish_within(Duration::ZERO).await
    } else if run.ended.is_some() {
        run.conn
            .finish_within(until.saturating_duration_since(std::time::Instant::now()))
            .await
    } else {
        run.conn.finish().await
    };
    Ok(turn)
}

/// What `account/rateLimits/read` says of the account, from a server started for that alone: no
/// thread, no turn, so the read costs the account nothing. The server ends with the answer, or at
/// `limit`; why there's none, when it ends without one.
pub async fn read_usage(launch: &Launch, limit: Duration) -> Result<crate::usage::Asked, String> {
    use crate::usage::Asked;
    let read = ask_once(launch, limit, "account/rateLimits/read", json!({})).await?;
    Ok(crate::usage::codex_read(&read).map_or(Asked::Silent, Asked::Said))
}

/// Codex's answer to one request, from a server started for that alone: no thread, no turn, so it
/// costs the account nothing. The server ends with the answer, or at `limit`; why there's none,
/// when it ends without one.
pub async fn ask_once(
    launch: &Launch,
    limit: Duration,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    // Kept until the read ends: a stop channel whose sender is gone stops the server.
    let (_run, stop) = watch::channel(process::Signal::Run);
    let mut conn = Conn::spawn(launch, limit, stop, false)
        .await
        .map_err(|e| format!("Codex didn't start: {e}"))?;
    let version = env!("CARGO_PKG_VERSION");
    let started = conn
        .request(
            "initialize",
            json!({"clientInfo": {"name": "obid", "title": "obid", "version": version}}),
        )
        .await;
    let mut asked = None;
    let answer = loop {
        let Ok(message) = conn.next().await else {
            break Err("Codex ended without answering.".into());
        };
        if let Some(read) = asked.and_then(|id| conn::answer_to(&message, id)) {
            break read.map_err(|e| format!("Codex didn't say: {}", e.message));
        }
        if asked.is_none() && conn::answer_to(&message, started).is_some() {
            conn.notify("initialized", None).await;
            asked = Some(conn.request(method, params.clone()).await);
        } else if let Some(id) = conn::request_id(&message).cloned() {
            conn.refuse(&id).await;
        }
    };
    // With no more input, it exits.
    conn.finish().await;
    answer
}

/// Why a turn's steps stopped early.
enum Stop {
    Cut(Cut),
    /// Codex refused a step: the turn fails, saying why.
    Failed(String),
}

impl From<Cut> for Stop {
    fn from(cut: Cut) -> Stop {
        Stop::Cut(cut)
    }
}

struct Run<D, E> {
    conn: Conn,
    decide: D,
    on_event: E,
    worktree: PathBuf,
    plan: bool,
    /// An own run ([`Launch::own_sandbox`], LANES-18): Codex's sandbox, the owner's MCP servers.
    own: bool,
    /// The files each file change writes, by item id: an approval request names only the item.
    files: HashMap<String, Vec<String>>,
    /// The MCP tools each server was called for this turn, as their items started: the approval
    /// Codex asks names the tool only in its words.
    calls: HashMap<String, Vec<(String, Value)>>,
    no_commands: bool,
    /// The `account/rateLimits/read` request not answered yet.
    limits_read: Option<u64>,
    /// The `account/read` request not answered yet, and what it said the account pays by.
    account_read: Option<u64>,
    priced: Option<String>,
    /// What the thread had used before this turn (crate::usage::codex_turn).
    tokens_before: Option<crate::usage::Tokens>,
    /// Why obid ended the turn for good.
    fatal: Option<String>,
    thread: Option<String>,
    /// The turn under way.
    turn: Option<String>,
    /// The agent's last message this turn, from `item/completed`.
    said: Option<String>,
    /// When each reasoning item started, for how long it took.
    reasoning: HashMap<String, std::time::Instant>,
    /// The turn's end: what it said, and whether it failed.
    ended: Option<(String, bool)>,
    /// When Codex's own limit resets, once it said it's full (crate::limits).
    resets: Option<i64>,
    /// On a ChatGPT login ([`Options::chatgpt`]).
    chatgpt: bool,
    /// Why its turn failed, as Codex said.
    failure: Option<Failure>,
    /// The helper threads that spoke this turn, each said once as a tool call.
    helpers: std::collections::HashSet<String>,
}

/// The items that aren't a tool call: words, thinking, a plan, the owner's words, a compaction, a
/// pause. Any other item [`tool_call`] doesn't name is a tool call of a kind obid doesn't know.
const NOT_TOOLS: &[&str] = &[
    "agentMessage",
    "reasoning",
    "plan",
    "userMessage",
    "contextCompaction",
    "sleep",
];

impl<D: Fn(&str, &Value) -> Ruling, E: FnMut(Event)> Run<D, E> {
    async fn turn(&mut self, prompt: &str, options: &Options) -> Result<(), Stop> {
        let version = env!("CARGO_PKG_VERSION");
        let client = if options.chatgpt {
            crate::chatgpt::AGENT_NAME
        } else {
            "obid"
        };
        self.call(
            "initialize",
            json!({"clientInfo": {"name": client, "title": client, "version": version}}),
        )
        .await?
        .map_err(|e| Stop::Failed(format!("Codex couldn't start: {}", e.message)))?;
        self.conn.notify("initialized", None).await;
        // The account's usage, answered while the thread starts: reading it asks OpenAI, and the
        // job doesn't wait for that (`handle`).
        if options.usage {
            self.limits_read = Some(
                self.conn
                    .request("account/rateLimits/read", json!({}))
                    .await,
            );
            // How the account pays, once per account: obid keeps it (crate::usage). A job's run
            // reads nothing else of the answer: who is signed in is the accounts' read, from the
            // account's own folder (crate::accounts). Never a token refresh for it.
            if options.priced.is_none() {
                self.account_read = Some(
                    self.conn
                        .request("account/read", json!({"refreshToken": false}))
                        .await,
                );
            }
        }

        let cwd = self.worktree.to_string_lossy().into_owned();
        if self.own {
            let read = self.call("config/read", json!({"cwd": cwd})).await?;
            let read = read.map_err(|e| Stop::Failed(format!("Codex didn't say which settings it runs with, so obid started no run outside its box: {}", e.message)))?;
            if let Some(why) = own_config_problem(&read, &cwd) {
                return Err(Stop::Failed(why));
            }
        }
        let mut params = thread_params(&cwd, self.own);
        if let Some(instructions) = &options.instructions {
            params["developerInstructions"] = json!(instructions);
        }
        if options.chatgpt
            && let Some(model) = options.model.as_deref().filter(|m| !m.is_empty())
        {
            params["model"] = json!(model);
        }
        let (method, what) = match &options.resume {
            Some(thread) => {
                params["threadId"] = json!(thread);
                ("thread/resume", "pick up its thread")
            }
            None => ("thread/start", "start a thread"),
        };
        let started = self.call(method, params).await?.map_err(|e| {
            let why = format!("Codex couldn't {what}: {}", e.message);
            // Its own answer to `thread/resume`, whatever its words: that thread can't go on.
            if options.resume.is_some() {
                self.failure = Some(Failure {
                    words: why.clone(),
                    cause: Cause::SessionGone,
                });
            }
            Stop::Failed(why)
        })?;
        if let Some(why) = own_thread_problem(&started).filter(|_| self.own) {
            return Err(Stop::Failed(why));
        }
        let thread = started["thread"]["id"]
            .as_str()
            .ok_or_else(|| Stop::Failed("Codex didn't start a thread.".into()))?
            .to_owned();
        self.thread = Some(thread.clone());
        (self.on_event)(Event::Session(thread.clone()));

        let mut params = json!({"threadId": thread, "input": [text(prompt)]});
        if let Some(model) = options.model.as_deref().filter(|m| !m.is_empty()) {
            params["model"] = json!(model);
        }
        if let Some(effort) = &options.effort {
            params["effort"] = json!(effort);
        }
        let started = self
            .call("turn/start", params)
            .await?
            .map_err(|e| Stop::Failed(format!("Codex couldn't start the turn: {}", e.message)))?;
        if let Some(id) = started["turn"]["id"].as_str() {
            self.turn = Some(id.to_owned());
        }
        while self.ended.is_none() {
            let message = self.conn.next().await?;
            self.handle(message).await;
        }
        Ok(())
    }

    /// Sends a request and reads until its answer comes, handling everything else on the way.
    async fn call(&mut self, method: &str, params: Value) -> Result<Result<Value, RpcError>, Cut> {
        let id = self.conn.request(method, params).await;
        loop {
            let message = self.conn.next().await?;
            if let Some(answer) = conn::answer_to(&message, id) {
                return Ok(answer);
            }
            self.handle(message).await;
        }
    }

    async fn handle(&mut self, message: Value) {
        if let Some(answer) = self
            .limits_read
            .and_then(|id| conn::answer_to(&message, id))
        {
            self.limits_read = None;
            // An older server may not know the method: its notifications still report usage.
            if let Some(usage) = answer.ok().and_then(|read| crate::usage::codex_read(&read)) {
                (self.on_event)(Event::Usage(usage));
            }
            return;
        }
        if let Some(answer) = self
            .account_read
            .and_then(|id| conn::answer_to(&message, id))
        {
            self.account_read = None;
            self.priced = answer
                .ok()
                .and_then(|read| crate::usage::codex_priced(&read));
            if self.priced.is_some() {
                (self.on_event)(Event::Usage(crate::usage::Usage {
                    priced: self.priced.clone(),
                    ..Default::default()
                }));
            }
            return;
        }
        if message["method"] == "account/rateLimits/updated"
            && let Some(usage) = crate::usage::codex(&message["params"]["rateLimits"])
        {
            (self.on_event)(Event::Usage(usage));
        }
        if let Some(id) = conn::request_id(&message).cloned() {
            return self.answer(id, &message).await;
        }
        if let Some((item, paths)) = changes(&message) {
            self.files.insert(item, paths);
        }
        let params = &message["params"];
        // Fail closed, in any thread (a helper's too): a conversation's Codex runs no command, no
        // hook and no tool of another MCP server, whatever its settings did.
        if self.no_commands
            && let Some(what) = ran_outside(&message)
        {
            return self.end_for_good(what).await;
        }
        // Helpers Codex starts run in threads of their own: only this job's thread reports. What
        // a helper does isn't followed, so its first word is a tool call of a kind obid doesn't
        // know (crate::secretspec, fail closed).
        if let (Some(other), Some(thread)) = (params["threadId"].as_str(), &self.thread)
            && other != thread
        {
            if !self.helpers.contains(other) {
                self.helpers.insert(other.to_owned());
                (self.on_event)(Event::Tool {
                    id: format!("helper-{other}"),
                    name: "Agent".into(),
                    input: json!({"description": "a helper thread obid doesn't follow"}),
                });
            }
            return;
        }
        if message["method"] == "thread/tokenUsage/updated"
            && let Some(mut tokens) =
                crate::usage::codex_turn(&params["tokenUsage"], &mut self.tokens_before)
        {
            tokens.priced.clone_from(&self.priced);
            (self.on_event)(Event::Tokens(tokens));
        }
        let item = &params["item"];
        match message["method"].as_str() {
            Some("account/rateLimits/updated") => {
                if let Some(at) = crate::limits::codex_full_until(&params["rateLimits"]) {
                    self.resets = Some(at);
                }
            }
            Some("item/started") if item["type"] == "reasoning" => {
                if let Some(id) = item["id"].as_str() {
                    self.reasoning
                        .insert(id.to_owned(), std::time::Instant::now());
                }
            }
            Some("item/completed") if item["type"] == "reasoning" => {
                let started = item["id"].as_str().and_then(|id| self.reasoning.remove(id));
                let text = item["summary"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.as_str().or(s["text"].as_str()))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if !text.trim().is_empty() {
                    (self.on_event)(Event::Thought {
                        text,
                        seconds: started.map(|s| (s.elapsed().as_secs_f64() * 10.0).round() / 10.0),
                    });
                }
            }
            Some("item/started") => {
                if let Some(step) = step(item) {
                    (self.on_event)(Event::Step(step));
                }
                if item["type"] == "mcpToolCall"
                    && let (Some(server), Some(tool)) =
                        (item["server"].as_str(), item["tool"].as_str())
                {
                    self.calls
                        .entry(server.to_owned())
                        .or_default()
                        .push((tool.to_owned(), item["arguments"].clone()));
                }
                if let Some((name, input)) = tool_call(item) {
                    (self.on_event)(Event::Tool {
                        id: item["id"].as_str().unwrap_or("").to_owned(),
                        name,
                        input,
                    });
                } else if !NOT_TOOLS.contains(&item["type"].as_str().unwrap_or("")) {
                    // An item obid doesn't parse: a tool call of a kind it doesn't know.
                    (self.on_event)(Event::Tool {
                        id: item["id"].as_str().unwrap_or("").to_owned(),
                        name: "Tool".into(),
                        input: json!({"type": item["type"]}),
                    });
                }
            }
            Some("item/agentMessage/delta") => {
                if let Some(delta) = params["delta"].as_str().filter(|d| !d.is_empty()) {
                    (self.on_event)(Event::Delta(delta.to_owned()));
                }
            }
            Some("item/completed") if item["type"] == "agentMessage" => {
                if let Some(said) = item["text"].as_str().filter(|s| !s.is_empty()) {
                    self.said = Some(said.to_owned());
                    (self.on_event)(Event::Said(said.to_owned()));
                }
            }
            Some("item/completed") if tool_call(item).is_some() => {
                let diffs = item["changes"].as_array().map(|changes| {
                    changes.iter().fold((0, 0), |(a, r), c| {
                        let (added, removed) =
                            crate::transcript::diff_lines(c["diff"].as_str().unwrap_or(""));
                        (a + added, r + removed)
                    })
                });
                (self.on_event)(Event::ToolDone {
                    id: item["id"].as_str().unwrap_or("").to_owned(),
                    failed: ["failed", "declined"].contains(&item["status"].as_str().unwrap_or(""))
                        || !item["error"].is_null(),
                    exit: item["exitCode"].as_i64(),
                    lines: diffs,
                    output: output_of(item),
                    patches: item["changes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|c| {
                            let path = c["path"].as_str().unwrap_or("").to_owned();
                            let patch = unified(&path, c);
                            (path, patch)
                        })
                        .collect(),
                });
            }
            Some("turn/plan/updated") => {
                let todos = params["plan"].as_array().into_iter().flatten();
                (self.on_event)(Event::Plan(
                    todos
                        .map(|t| crate::transcript::Todo {
                            text: t["step"].as_str().unwrap_or("").to_owned(),
                            state: crate::transcript::todo_state(
                                t["status"].as_str().unwrap_or(""),
                            )
                            .into(),
                        })
                        .collect(),
                ));
            }
            Some("turn/completed") => {
                let turn = &params["turn"];
                // On a ChatGPT login nothing is read out of a failed turn's words, so they're
                // Codex's: the model's can say anything (docs/contracts/chatgpt-sign-in.md).
                self.ended = Some(if self.chatgpt && turn["status"] == "failed" {
                    (why_failed(turn), true)
                } else {
                    finished(turn, self.said.as_deref())
                });
                self.failure = failure(turn);
            }
            _ => {}
        }
    }

    /// Ends the turn for good because Codex `what` (ran a command, …): interrupts it, and the run
    /// ends at once.
    async fn end_for_good(&mut self, what: String) {
        let why = format!("{what}, which a conversation never does, so obid stopped it");
        if let (Some(thread), Some(turn)) = (self.thread.clone(), self.turn.clone()) {
            self.conn
                .request(
                    "turn/interrupt",
                    json!({"threadId": thread, "turnId": turn}),
                )
                .await;
        }
        self.fatal = Some(why.clone());
        self.failure = Some(Failure::said(&why));
        self.ended = Some((why, true));
    }

    /// obid's answer to an approval: the fixed denials, then the job's own rule. Its questions go to
    /// the owner as Claude Code's AskUserQuestion does. Anything else Codex asks (forms, logins) is
    /// refused, so it never waits on obid.
    async fn answer(&mut self, id: Value, request: &Value) {
        let params = &request["params"];
        let calls: Vec<(String, Value)> = match request["method"].as_str() {
            Some("item/commandExecution/requestApproval") => match command_of(&params["command"]) {
                Some(command) => vec![("Bash".into(), json!({"command": unwrap_shell(&command)}))],
                None => {
                    return self
                        .decline(&id, params, "obid couldn't see the command.".into())
                        .await;
                }
            },
            Some("item/fileChange/requestApproval") => {
                let mut paths = params["itemId"]
                    .as_str()
                    .and_then(|item| self.files.get(item))
                    .cloned()
                    .unwrap_or_default();
                if paths.is_empty() {
                    return self
                        .decline(
                            &id,
                            params,
                            "obid couldn't see which files this change writes.".into(),
                        )
                        .await;
                }
                if self.plan {
                    return self.decline(&id, params, "This job is in Plan mode: read and plan, and change no file. End with your plan as your summary.".into()).await;
                }
                if let Some(root) = params["grantRoot"].as_str() {
                    paths.push(root.to_owned());
                }
                paths
                    .into_iter()
                    .map(|path| ("Edit".to_owned(), json!({"file_path": path})))
                    .collect()
            }
            // Codex asks before an MCP tool runs: the tool its words name (`Allow the oby MCP
            // server to run tool "oby_job"?`), when its server was called for it this turn, with
            // the arguments it will run with.
            Some("mcpServer/elicitation/request")
                if params["_meta"]["codex_approval_kind"] == "mcp_tool_call" =>
            {
                let server = params["serverName"].as_str().unwrap_or("");
                let named = params["message"]
                    .as_str()
                    .and_then(|m| m.split_once("tool \""))
                    .and_then(|(_, rest)| rest.rsplit_once('"'))
                    .map(|(tool, _)| tool);
                let call = self
                    .calls
                    .get(server)
                    .and_then(|calls| calls.iter().rfind(|(tool, _)| Some(tool.as_str()) == named));
                let answer = match call.cloned() {
                    // Only Oby's own server: another's (ChatGPT's apps, the person's) is never
                    // run, as before Codex asked. An own run's are the owner's (LANE-12).
                    Some(_) if !OWN_SERVERS.contains(&server) && !self.own => {
                        Answer::Deny(format!("Only Oby's own tools run here, not {server}'s."))
                    }
                    Some((tool, arguments)) => {
                        let input = match &params["_meta"]["tool_params"] {
                            Value::Object(_) => params["_meta"]["tool_params"].clone(),
                            _ => arguments,
                        };
                        let ruling = (self.decide)(&format!("mcp__{server}__{tool}"), &input);
                        match self.conn.settle(ruling).await {
                            Ok(answer) => answer,
                            Err(_) => return,
                        }
                    }
                    None => Answer::Deny("obid couldn't see which tool this runs.".into()),
                };
                if let Answer::Deny(why) = &answer {
                    (self.on_event)(Event::Denied(why.clone()));
                }
                let action = if matches!(answer, Answer::Allow | Answer::AllowWith(_)) {
                    "accept"
                } else {
                    "decline"
                };
                return self
                    .conn
                    .reply(&id, json!({"action": action, "content": {}}))
                    .await;
            }
            Some("item/tool/requestUserInput") => return self.ask_owner(&id, params).await,
            _ => return self.conn.refuse(&id).await,
        };
        for (tool, input) in &calls {
            let ruling = (self.decide)(tool, input);
            match self.conn.settle(ruling).await {
                Ok(Answer::Allow | Answer::AllowWith(_)) => {}
                Ok(Answer::Deny(why)) => return self.decline(&id, params, why).await,
                // Stopped while the owner was asked: the turn ends.
                Err(_) => return,
            }
        }
        self.conn.reply(&id, json!({"decision": "accept"})).await;
    }

    /// Codex's questions (`request_user_input`) as Claude Code's AskUserQuestion (#858): the owner's
    /// answer to each, an option's label or their own words, goes back by the question's id. A
    /// secret is never asked: obid never collects one for an agent.
    async fn ask_owner(&mut self, id: &Value, params: &Value) {
        let questions = params["questions"].as_array().cloned().unwrap_or_default();
        let answer = if questions.iter().any(|q| q["isSecret"] == json!(true)) {
            Answer::Deny(
                "obid never collects a secret for an agent. Say in your summary what you need."
                    .into(),
            )
        } else {
            let asked: Vec<Value> = questions
                .iter()
                .map(|q| json!({"question": q["question"], "header": q["header"], "options": q["options"]}))
                .collect();
            let ruling = (self.decide)(
                crate::permissions::ASK_USER_QUESTION,
                &json!({ "questions": asked }),
            );
            match self.conn.settle(ruling).await {
                Ok(answer) => answer,
                Err(_) => return,
            }
        };
        let given = match &answer {
            Answer::AllowWith(given) => given["answers"].clone(),
            Answer::Allow => Value::Null,
            Answer::Deny(why) => {
                (self.on_event)(Event::Denied(why.clone()));
                Value::Null
            }
        };
        // Declined, each question hears why, which is all its answer can carry.
        let why = match &answer {
            Answer::Deny(why) => why.as_str(),
            _ => crate::permissions::QUESTION_DECLINED,
        };
        let answers: serde_json::Map<String, Value> = questions
            .iter()
            .filter_map(|q| {
                let said = q["question"]
                    .as_str()
                    .and_then(|text| given[text].as_str())
                    .unwrap_or(why);
                Some((q["id"].as_str()?.to_owned(), json!({ "answers": [said] })))
            })
            .collect();
        self.conn.reply(id, json!({ "answers": answers })).await;
    }

    /// Declines, and since the decision has no room for a reason, tells the turn why.
    async fn decline(&mut self, id: &Value, params: &Value, why: String) {
        self.conn.reply(id, json!({"decision": "decline"})).await;
        (self.on_event)(Event::Denied(why.clone()));
        if let (Some(thread), Some(turn)) = (params["threadId"].as_str(), params["turnId"].as_str())
        {
            self.conn
                .request(
                    "turn/steer",
                    json!({"threadId": thread, "expectedTurnId": turn,
                           "input": [text(&format!("obid declined that: {why}"))]}),
                )
                .await;
        }
    }
}

/// The permission profile an own run's commands are held to (crate::jobs, `own_codex_settings`).
pub(crate) const OWN_PROFILE: &str = "oby_own";

/// What obid tells the owner when an own run can't start on Codex's settings.
fn not_obids(what: &str) -> String {
    format!(
        "Codex's own settings on this Mac change what obid sets for a Bypass run outside its box ({what}), so it started none. Take that out of Codex's config.toml or its managed settings, or pick another mode: the job then runs in Oby's box."
    )
}

/// Why an own run can't start (LANES-18), from Codex's answer `read` to `config/read` for its
/// `cwd`: Codex merges obid's `-c` settings with its other layers, table by table, so obid starts no
/// thread unless they are what holds. Its approval policy, permission profile, no hooks and no
/// `notify` program; the worktree an untrusted project, as named and where it leads (none of the
/// repository's `.codex/` loads); and every key of obid's profile from obid's own `-c` (`origins`:
/// one the account's `config.toml` or a managed layer adds, an `auth.json` to read, a network,
/// would widen it). None when all of it holds.
fn own_config_problem(read: &Value, cwd: &str) -> Option<String> {
    let (config, origins) = (&read["config"], &read["origins"]);
    let real = Path::new(cwd).canonicalize().ok();
    let dirs = [Some(cwd), real.as_deref().and_then(Path::to_str)];
    let untrusted = (dirs.into_iter().flatten())
        .all(|dir| config["projects"][dir]["trust_level"] == "untrusted");
    let profile = format!("permissions.{OWN_PROFILE}.");
    let obids = |key: &str| origins[key]["name"]["type"] == "sessionFlags";
    let added = (origins.as_object().into_iter().flatten())
        .map(|(key, _)| key.as_str())
        .find(|key| key.starts_with(&profile) && !obids(key));
    let network = &config["permissions"][OWN_PROFILE]["network"]["enabled"];
    let held = [
        (config["approval_policy"] == "never", "approval_policy"),
        (
            config["default_permissions"] == OWN_PROFILE,
            "default_permissions",
        ),
        (config["features"]["hooks"] == false, "features.hooks"),
        (
            config["notify"].as_array().is_none_or(Vec::is_empty),
            "notify",
        ),
        (untrusted, "the worktree's trust in projects"),
        (
            network == false && obids(&format!("{profile}network.enabled")),
            "the profile's network",
        ),
        (added.is_none(), added.unwrap_or_default()),
    ];
    let (_, what) = held.into_iter().find(|(holds, _)| !holds)?;
    Some(not_obids(what))
}

/// Why an own run can't go on in the thread Codex `started` or resumed (LANES-18): not with
/// `never`, or not in obid's profile (a resumed thread's own, a managed requirement's). None when
/// it's obid's.
fn own_thread_problem(started: &Value) -> Option<String> {
    let obids = started["approvalPolicy"] == "never"
        && started["activePermissionProfile"]["id"] == OWN_PROFILE;
    (!obids).then(|| not_obids("the thread's approval policy or permission profile"))
}

/// The thread's settings: obid's sandbox is the sandbox, and every approval comes to obid. An `own`
/// run's sandbox is Codex's own, obid's profile named on a new thread and on a resumed one (which
/// would otherwise keep the one it was last saved with), and it never asks to leave it (LANES-18).
fn thread_params(cwd: &str, own: bool) -> Value {
    if own {
        return json!({"cwd": cwd, "approvalPolicy": "never", "approvalsReviewer": "user",
                      "config": {"default_permissions": OWN_PROFILE}});
    }
    json!({"cwd": cwd, "sandbox": "danger-full-access", "approvalPolicy": "untrusted",
           "approvalsReviewer": "user"})
}

/// A text input item.
fn text(text: &str) -> Value {
    json!({"type": "text", "text": text, "text_elements": []})
}

/// A command as Codex sends it: a string, or its words.
fn command_of(value: &Value) -> Option<String> {
    match value {
        Value::String(command) => Some(command.clone()),
        Value::Array(words) => Some(
            words
                .iter()
                .map(|w| {
                    w.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| w.to_string())
                })
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

/// The command without the shell Codex wraps it in: `/bin/zsh -lc 'git status'` and
/// `/bin/zsh -lc "git status"` are `git status`.
pub fn unwrap_shell(command: &str) -> String {
    static WRAPPED: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let wrapped = WRAPPED.get_or_init(|| {
        regex::Regex::new(r"(?s)^/\S*/(?:ba|z|da|k)?sh -l?c (.+)$").expect("a regex")
    });
    wrapped
        .captures(command)
        .and_then(|inner| one_word(&inner[1]))
        .unwrap_or_else(|| command.to_owned())
}

/// One shell word with its quotes taken out (`'…'`, `"…"` and bare characters, run together), or
/// none when it's more than one word or holds what the shell would expand.
fn one_word(text: &str) -> Option<String> {
    let mut word = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => loop {
                match chars.next()? {
                    '\'' => break,
                    c => word.push(c),
                }
            },
            '"' => loop {
                match chars.next()? {
                    '"' => break,
                    '\\' => match chars.next()? {
                        c @ ('"' | '\\' | '$' | '`') => word.push(c),
                        '\n' => {}
                        c => word.extend(['\\', c]),
                    },
                    '$' | '`' => return None,
                    c => word.push(c),
                }
            },
            '\\' => word.push(chars.next()?),
            c if c.is_whitespace() || "|&;<>()$`*?[~#".contains(c) => return None,
            c => word.push(c),
        }
    }
    Some(word)
}

/// The files a change writes: each change's `path`, and where a move goes.
fn files(change: &Value) -> Vec<String> {
    change["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|c| [c["path"].as_str(), c["kind"]["move_path"].as_str()])
        .flatten()
        .map(str::to_owned)
        .collect()
}

/// The files a change writes, as its item started or as its patch was updated.
fn changes(message: &Value) -> Option<(String, Vec<String>)> {
    let params = &message["params"];
    match message["method"].as_str()? {
        "item/started" if params["item"]["type"] == "fileChange" => Some((
            params["item"]["id"].as_str()?.to_owned(),
            files(&params["item"]),
        )),
        "item/fileChange/patchUpdated" => {
            Some((params["itemId"].as_str()?.to_owned(), files(params)))
        }
        _ => None,
    }
}

/// The items a conversation's Codex may make (codex-cli 0.162.1's `ThreadItem`): words, thinking,
/// a plan, a web search, a file change (each one obid's to approve), a tool of Oby's own MCP server
/// ([`ran_outside`] checks the server), a compaction, the owner's words and a pause. Anything else,
/// a kind Codex adds later too, ends the turn: an allowlist fails closed.
const ITEMS: &[&str] = &[
    "agentMessage",
    "reasoning",
    "plan",
    "webSearch",
    "fileChange",
    "mcpToolCall",
    "contextCompaction",
    "userMessage",
    "sleep",
];

/// The notifications that aren't an item's that a conversation's Codex may send (0.162.1's
/// `ServerNotification`): the thread's and the turn's own news, usage, warnings. Oby's MCP server
/// starting passes in [`ran_outside`]. An item's (`item/…`) pass when its kind is one of [`ITEMS`].
const NOTICES: &[&str] = &[
    "error",
    "warning",
    "configWarning",
    "deprecationNotice",
    "thread/started",
    "thread/status/changed",
    "thread/name/updated",
    "thread/settings/updated",
    "thread/tokenUsage/updated",
    "thread/compacted",
    "thread/queue/changed",
    "thread/closed",
    // codex 0.162 on every thread/resume, with no goal set: nothing to do with what runs
    "thread/goal/cleared",
    "turn/started",
    "turn/completed",
    "turn/diff/updated",
    "turn/plan/updated",
    "turn/moderationMetadata",
    "serverRequest/resolved",
    "account/updated",
    "account/rateLimits/updated",
    "model/rerouted",
    "model/verification",
    "model/safetyBuffering/updated",
];

/// What a conversation's Codex did that it never may, from one notification in any thread: a
/// command (an item of that kind in any `item/*`, or a command's own `item/commandExecution/*`), a
/// hook (`hook/*`), a tool of an MCP server that isn't Oby's, or any item or notification that
/// isn't on the lists ([`ITEMS`], [`NOTICES`]). Codex's own resource tools (`list_mcp_resources`,
/// `list_mcp_resource_templates`, `read_mcp_resource`, server `codex`) only reach the servers it
/// has, Oby's alone.
fn ran_outside(message: &Value) -> Option<String> {
    let method = message["method"].as_str()?;
    let item = &message["params"]["item"];
    if method.starts_with("hook/") {
        return Some("Codex ran a hook".into());
    }
    if method.starts_with("item/commandExecution/")
        || (method.starts_with("item/") && item["type"] == "commandExecution")
    {
        let command = unwrap_shell(&command_of(&item["command"]).unwrap_or_default());
        let command: String = command
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();
        return Some(match command.trim() {
            "" => "Codex ran a command".into(),
            command => format!("Codex ran a command ({command})"),
        });
    }
    match method.split('/').collect::<Vec<_>>()[..] {
        // An item: its kind must be on the list.
        ["item", "started" | "completed"] => match item["type"].as_str() {
            Some(kind) if ITEMS.contains(&kind) => {}
            Some(kind) => return Some(format!("Codex used {kind}")),
            None => return Some("Codex used an item of no kind".into()),
        },
        // An item's own news (`item/agentMessage/delta`): its kind, from the method.
        ["item", kind, _] if ITEMS.contains(&kind) => {}
        _ if NOTICES.contains(&method) => {}
        // Codex says at start whether something remote drives it: only "nobody" passes.
        ["remoteControl", "status", "changed"] if message["params"]["status"] == "disabled" => {}
        // Only Oby's MCP servers start: obid's own, and Tetrix's reads when it's connected. No other
        // is given (ChatGPT's apps are off).
        ["mcpServer", "startupStatus", "updated"]
            if OWN_SERVERS.contains(&message["params"]["name"].as_str().unwrap_or("")) => {}
        _ => return Some(format!("Codex sent {method}")),
    }
    if item["type"] == "mcpToolCall" {
        let (server, tool) = (
            item["server"].as_str().unwrap_or(""),
            item["tool"].as_str().unwrap_or(""),
        );
        let own = server == "codex"
            && matches!(
                tool,
                "list_mcp_resources" | "list_mcp_resource_templates" | "read_mcp_resource"
            );
        if !OWN_SERVERS.contains(&server) && !own {
            return Some(format!("Codex called {server}'s {tool}"));
        }
    }
    None
}

/// What Codex is doing, in the same words as Claude Code's steps.
pub fn step(item: &Value) -> Option<String> {
    match item["type"].as_str()? {
        "fileChange" if files(item).len() != 1 => {
            Some(format!("Editing {} files", files(item).len()))
        }
        "mcpToolCall" => Some(agent::step(
            item["tool"].as_str().unwrap_or("a tool"),
            &json!({}),
        )),
        _ => tool_call(item).map(|(name, input)| agent::step(&name, &input)),
    }
}

/// What a tool item printed or answered: a command's aggregated output, an MCP tool's text.
fn output_of(item: &Value) -> Option<String> {
    if let Some(text) = item["aggregatedOutput"].as_str() {
        return Some(text.to_owned());
    }
    let blocks = item["result"]["content"].as_array()?;
    Some(
        blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// A file change as a unified diff: Codex's own when it has its headers, its hunks under them
/// when not, and a new or deleted file's whole text as added or removed lines.
fn unified(path: &str, change: &Value) -> String {
    let diff = change["diff"].as_str().unwrap_or("");
    if diff.starts_with("--- ") || diff.starts_with("diff ") {
        return diff.to_owned();
    }
    if diff.starts_with("@@") {
        return format!("--- a/{path}\n+++ b/{path}\n{diff}");
    }
    match change["kind"]["type"].as_str() {
        Some("delete") => crate::detail::replaced(path, diff, ""),
        _ => crate::detail::replaced(path, "", diff),
    }
}

/// A tool item as the Claude Code tool call it is (its name and input), so a conversation shows
/// and rules on it the same whichever agent runs it: a command that only reads, searches or lists
/// is `Read`, `Grep` or `Glob`, any other `Bash`; a file change `Edit` (its first file); an MCP
/// tool `mcp__<server>__<tool>`; a web search `WebSearch`; a helper `Agent`.
pub fn tool_call(item: &Value) -> Option<(String, Value)> {
    let call = |name: &str, input: Value| Some((name.to_owned(), input));
    match item["type"].as_str()? {
        "commandExecution" => {
            if let Some([action]) = item["commandActions"].as_array().map(Vec::as_slice) {
                match action["type"].as_str() {
                    Some("read") => {
                        return call(
                            "Read",
                            json!({"file_path": action["path"].as_str().unwrap_or("")}),
                        );
                    }
                    Some("search") => {
                        let input = match action["query"].as_str() {
                            Some(query) => json!({"pattern": query}),
                            None => json!({}),
                        };
                        return call("Grep", input);
                    }
                    Some("listFiles") => return call("Glob", json!({})),
                    _ => {}
                }
            }
            let command = command_of(&item["command"]).unwrap_or_default();
            call("Bash", json!({"command": unwrap_shell(&command)}))
        }
        "fileChange" => call(
            "Edit",
            json!({"file_path": files(item).first().cloned().unwrap_or_default()}),
        ),
        "mcpToolCall" => Some((
            format!(
                "mcp__{}__{}",
                item["server"].as_str().unwrap_or(""),
                item["tool"].as_str().unwrap_or("")
            ),
            item["arguments"].clone(),
        )),
        // A picture it looked at is a file it read (the turn's taint, as Read's).
        "imageView" => call(
            "Read",
            json!({"file_path": item["path"].as_str().unwrap_or("")}),
        ),
        "webSearch" => call(
            "WebSearch",
            json!({"query": item["query"].as_str().unwrap_or("")}),
        ),
        "collabAgentToolCall" => call(
            "Agent",
            json!({"description": item["prompt"].as_str().unwrap_or("").lines().next().unwrap_or("")}),
        ),
        _ => None,
    }
}

/// How a turn ended: what the agent said last (from the turn's items, else its last message), and
/// whether it failed.
pub fn finished(turn: &Value, said: Option<&str>) -> (String, bool) {
    let status = turn["status"].as_str().unwrap_or("");
    let failed = status != "completed";
    let last = turn["items"]
        .as_array()
        .into_iter()
        .flatten()
        .rfind(|item| item["type"] == "agentMessage")
        .and_then(|item| item["text"].as_str())
        .filter(|text| !text.is_empty())
        .or(said.filter(|text| !text.is_empty()));
    if let Some(text) = last {
        return (text.to_owned(), failed);
    }
    if !failed {
        return ("Done.".into(), false);
    }
    (failed_words(turn), true)
}

/// What Codex itself says ended a turn that failed, never the agent's words.
fn failed_words(turn: &Value) -> String {
    if turn["status"] == "interrupted" {
        return "Stopped.".into();
    }
    why_failed(turn)
}

/// A failed turn's reason, in Codex's words, whatever the agent said before it.
fn why_failed(turn: &Value) -> String {
    // The API's own error arrives as JSON inside the message.
    let message = turn["error"]["message"].as_str().unwrap_or("");
    let reason = serde_json::from_str::<Value>(message)
        .ok()
        .and_then(|api| api["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| message.to_owned());
    if reason.is_empty() {
        "Codex hit an error.".into()
    } else {
        format!("Codex hit an error: {reason}")
    }
}

/// Why a turn failed, from its `error`'s `codexErrorInfo` (Codex 0.162.1's `TurnError`): a name, or
/// a name holding `httpStatusCode`. A Codex with none, or one obid doesn't know, leaves its words.
fn failure(turn: &Value) -> Option<Failure> {
    if turn["status"].as_str().unwrap_or("") == "completed" {
        return None;
    }
    let info = &turn["error"]["codexErrorInfo"];
    let (name, status) = match info.as_object().and_then(|o| o.iter().next()) {
        Some((name, held)) => (
            name.as_str(),
            held["httpStatusCode"]
                .as_u64()
                .and_then(|s| u16::try_from(s).ok()),
        ),
        None => (info.as_str().unwrap_or(""), None),
    };
    let words = failed_words(turn);
    let cause = match name {
        "contextWindowExceeded" => Cause::TooLong,
        "usageLimitExceeded" => Cause::Limit,
        "rateLimitExceeded" => Cause::Busy,
        "serverOverloaded"
        | "internalServerError"
        | "httpConnectionFailed"
        | "responseStreamConnectionFailed"
        | "responseStreamDisconnected"
        | "responseTooManyFailedAttempts" => match status {
            Some(429) => Cause::Busy,
            Some(401 | 403) => Cause::Login,
            Some(400..=499) => Cause::Other,
            _ => Cause::Passing,
        },
        "unauthorized" => Cause::Login,
        "badRequest" | "sandboxError" => Cause::Other,
        _ => return Some(Failure::said(words)),
    };
    Some(Failure { words, cause })
}

/// The folder Codex's standalone package puts first on its commands' `PATH` (`pathDir` in its
/// `codex-package.json`, beside `bin/codex`), where it ships its own `rg`; none for another install.
pub fn shipped_programs(bin: &Path) -> Option<PathBuf> {
    let root = std::fs::canonicalize(bin)
        .ok()?
        .parent()?
        .parent()?
        .to_path_buf();
    let package: Value =
        serde_json::from_slice(&std::fs::read(root.join("codex-package.json")).ok()?).ok()?;
    let dir = Path::new(package["pathDir"].as_str()?);
    if !dir
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    // A link inside the package that leads out of it is none either.
    std::fs::canonicalize(root.join(dir))
        .ok()
        .filter(|dir| dir.starts_with(&root))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codex's standalone package names the folder it ships `rg` in; one that leads out of the
    /// package is none.
    #[test]
    fn codex_ships_its_programs_beside_its_binary() {
        let root = std::env::temp_dir().join(format!("obid-codex-pkg-{}", std::process::id()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/codex"), "").unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        let package = |dir: &str| {
            std::fs::write(
                root.join("codex-package.json"),
                json!({"entrypoint": "bin/codex", "pathDir": dir}).to_string(),
            )
            .unwrap();
            shipped_programs(&root.join("bin/codex"))
        };
        std::fs::create_dir_all(root.join("codex-path")).unwrap();
        assert_eq!(package("codex-path"), Some(root.join("codex-path")));
        std::os::unix::fs::symlink(std::env::temp_dir(), root.join("away")).unwrap();
        assert_eq!(package("away"), None);
        assert_eq!(package("../elsewhere"), None);
        assert_eq!(package("/usr/bin"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// ConnectionTests' last part: Codex finds the broker in its config and reads the job's token
    /// from a variable it names; the token is never an argument (other processes read those).
    #[test]
    fn codex_reaches_the_broker_with_the_token_out_of_its_arguments() {
        let url = "http://127.0.0.1:7777/v1/agent/mcp/job1";
        let token = "job-token-0123456789abcdef";
        let args: Vec<String> = argv(PathBuf::from("/bin/codex"), Some((url, token)), None)
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        assert_eq!(
            args,
            [
                "/bin/codex",
                "-c",
                r#"mcp_servers.oby.url="http://127.0.0.1:7777/v1/agent/mcp/job1""#,
                "-c",
                r#"mcp_servers.oby.bearer_token_env_var="OBY_MCP_TOKEN""#,
                "app-server"
            ]
        );
        assert!(!args.iter().any(|a| a.contains(token)));
        assert_eq!(
            argv(PathBuf::from("/bin/codex"), None, None),
            ["/bin/codex", "app-server"]
        );
        // Oby's Tetrix server (crate::tetrix) takes the same token from the environment.
        assert_eq!(
            argv(
                PathBuf::from("/bin/codex"),
                None,
                Some("http://127.0.0.1:7777/v1/agent/tetrix/job1")
            ),
            [
                "/bin/codex",
                "-c",
                r#"mcp_servers.tetrix.url="http://127.0.0.1:7777/v1/agent/tetrix/job1""#,
                "-c",
                r#"mcp_servers.tetrix.bearer_token_env_var="OBY_MCP_TOKEN""#,
                "app-server"
            ]
        );
    }

    /// A Codex that never exits after its turn completed ends a few seconds later, as Claude Code
    /// does after its result (#658), not at the round's limit.
    #[tokio::test]
    async fn a_codex_that_never_exits_ends_soon_after_its_turn() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-codex-exit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bin = root.join("codex");
        std::fs::write(
            &bin,
            r#"#!/bin/sh
read l; echo '{"id":1,"result":{}}'
read l
read l; echo '{"id":2,"result":{"thread":{"id":"t1"}}}'
read l; echo '{"id":3,"result":{"turn":{"id":"u1"}}}'
echo '{"method":"turn/completed","params":{"threadId":"t1","turn":{"id":"u1","status":"completed","items":[{"type":"agentMessage","text":"Done."}]}}}'
exec sleep 30
"#,
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let launch = Launch {
            own_sandbox: false,
            group: None,
            argv: vec![bin.into_os_string()],
            env: vec![("PATH".into(), "/usr/bin:/bin".into())],
            cwd: root.clone(),
            stderr: root.join("agent.log"),
        };
        let (_stop, stop) = watch::channel(process::Signal::Run);
        let began = std::time::Instant::now();
        let turn = run_turn(
            launch,
            "Do it.",
            &Options::default(),
            Duration::from_secs(60),
            stop,
            |_, _| Ruling::Now(Answer::Allow),
            |_| {},
        )
        .await
        .unwrap();
        let took = began.elapsed();
        assert_eq!(turn.text(), "Done.");
        assert!(turn.succeeded());
        assert!(
            took < agent::AFTER_RESULT + Duration::from_secs(2),
            "{took:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// What Codex 0.162.1 answers `config/read` with for an own run's `-c` settings in `cwd`, as
    /// measured on a stand-in `CODEX_HOME` (the keys obid reads).
    fn own_config(cwd: &str) -> Value {
        let from_obid = json!({"name": {"type": "sessionFlags"}, "version": "v"});
        json!({"config": {"approval_policy": "never", "default_permissions": OWN_PROFILE,
                "features": {"hooks": false}, "notify": [],
                "projects": {cwd: {"trust_level": "untrusted"}},
                "permissions": {OWN_PROFILE: {"extends": null,
                    "filesystem": {"/": "read", cwd: "write"}, "network": {"enabled": false}}}},
            "origins": {"approval_policy": from_obid,
                format!("permissions.{OWN_PROFILE}.filesystem./"): from_obid,
                format!("permissions.{OWN_PROFILE}.filesystem.{cwd}"): from_obid,
                format!("permissions.{OWN_PROFILE}.network.enabled"): from_obid}})
    }

    /// LANES-18: obid starts an own run only on settings that are its own. What Codex 0.162.1
    /// merges in from the account's `config.toml` or a managed layer stops it, each named: an entry
    /// in obid's profile (an `auth.json` to read, the clone's refs to write), its network, a
    /// `notify` program, hooks, another approval policy or profile, the worktree trusted (its
    /// `.codex/` would load), or no word of where the profile's keys came from.
    #[test]
    fn lanes_18_settings_codex_merged_in_stop_an_own_run() {
        let cwd = "/private/obid-no-such-worktree";
        assert_eq!(own_config_problem(&own_config(cwd), cwd), None);
        let users = json!({"name": {"type": "user", "file": "/a/config.toml"}});
        let network = format!("permissions.{OWN_PROFILE}.network.enabled");
        let auth = format!("permissions.{OWN_PROFILE}.filesystem./a/auth.json");
        let extends = format!("permissions.{OWN_PROFILE}.extends");
        let (profile, policy) = (["config", "permissions", OWN_PROFILE], "approval_policy");
        let cases: [(&str, Vec<&str>, Value); 9] = [
            (&auth, vec!["origins", &auth], users.clone()),
            (&extends, vec!["origins", &extends], users.clone()),
            ("the profile's network", vec!["origins", &network], users),
            (
                "the profile's network",
                [&profile[..], &["network", "enabled"]].concat(),
                json!(true),
            ),
            (
                "notify",
                vec!["config", "notify"],
                json!(["python3", "scripts/notify.py"]),
            ),
            (
                "features.hooks",
                vec!["config", "features", "hooks"],
                json!(true),
            ),
            (policy, vec!["config", policy], json!("on-request")),
            (
                "default_permissions",
                vec!["config", "default_permissions"],
                json!(":workspace"),
            ),
            (
                "the worktree's trust in projects",
                vec!["config", "projects", cwd, "trust_level"],
                json!("trusted"),
            ),
        ];
        for (what, path, value) in cases {
            let mut read = own_config(cwd);
            let mut at = &mut read;
            for key in path {
                at = &mut at[key];
            }
            *at = value;
            let why = own_config_problem(&read, cwd).unwrap_or_default();
            assert!(why.contains(&format!("({what})")), "{what}: {why}");
        }
        let mut silent = own_config(cwd);
        silent["origins"] = Value::Null;
        assert!(own_config_problem(&silent, cwd).is_some());
        // A thread that comes back with another policy or profile (a resumed one's own) stops it too.
        let ours =
            json!({"approvalPolicy": "never", "activePermissionProfile": {"id": OWN_PROFILE}});
        assert_eq!(own_thread_problem(&ours), None);
        for (key, value) in [
            ("approvalPolicy", json!("on-request")),
            (
                "activePermissionProfile",
                json!({"id": ":danger-full-access"}),
            ),
            ("activePermissionProfile", Value::Null),
        ] {
            let mut theirs = ours.clone();
            theirs[key] = value;
            assert!(own_thread_problem(&theirs).is_some(), "{theirs}");
        }
    }

    /// LANES-18: an own run asks Codex which settings hold before any thread, then names obid's
    /// profile and `never` on a new thread and on a resumed one (Codex 0.162.1 resumes a thread in
    /// the profile it was saved with unless its permissions are named), sets no sandbox, and goes
    /// on only in a thread that comes back with both; a boxed run's is obid's box, every approval
    /// obid's, as before, and asks Codex nothing first. Settings that aren't obid's, or a thread
    /// that comes back in another profile, start no turn.
    #[tokio::test]
    async fn lanes_18_an_own_thread_is_held_to_obids_profile() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-codex-own-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let bin = root.join("codex");
        std::fs::write(
            &bin,
            r#"#!/bin/sh
while read l; do
  id=$(echo "$l" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$l" in
    *config/read*) echo "$l" >> asked.jsonl; echo "{\"id\":$id,\"result\":$(cat config.json)}";;
    *thread/*) echo "$l" >> asked.jsonl; echo "{\"id\":$id,\"result\":$(cat thread.json)}";;
    *turn/start*) echo "$l" >> asked.jsonl; echo "{\"id\":$id,\"result\":{\"turn\":{\"id\":\"u1\"}}}"
      echo '{"method":"turn/completed","params":{"threadId":"t1","turn":{"id":"u1","status":"completed","items":[{"type":"agentMessage","text":"Done."}]}}}';;
    *'"id"'*) echo "{\"id\":$id,\"result\":{}}";;
  esac
done
"#,
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cwd = root.to_str().unwrap();
        let ours = json!({"thread": {"id": "t1"}, "approvalPolicy": "never",
            "activePermissionProfile": {"id": OWN_PROFILE}});
        let run = async |own: bool, resume: Option<&str>, config: Value, thread: Value| {
            let _ = std::fs::remove_file(root.join("asked.jsonl"));
            std::fs::write(root.join("config.json"), config.to_string()).unwrap();
            std::fs::write(root.join("thread.json"), thread.to_string()).unwrap();
            let launch = Launch {
                own_sandbox: own,
                group: None,
                argv: vec![bin.clone().into_os_string()],
                env: vec![("PATH".into(), "/usr/bin:/bin".into())],
                cwd: root.clone(),
                stderr: root.join("agent.log"),
            };
            let options = Options {
                resume: resume.map(str::to_owned),
                ..Options::default()
            };
            let (_stop, stop) = watch::channel(process::Signal::Run);
            let limit = Duration::from_secs(60);
            let allow = |_: &str, _: &Value| Ruling::Now(Answer::Allow);
            let turn = run_turn(launch, "Do it.", &options, limit, stop, allow, |_| {})
                .await
                .unwrap();
            let asked: Vec<Value> = std::fs::read_to_string(root.join("asked.jsonl"))
                .unwrap_or_default()
                .lines()
                .map(|l| serde_json::from_str::<Value>(l).unwrap())
                .collect();
            (turn, asked)
        };
        for resume in [None, Some("t1")] {
            let (turn, asked) = run(true, resume, own_config(cwd), ours.clone()).await;
            assert_eq!(turn.text(), "Done.", "{}", turn.error());
            let methods: Vec<&str> = asked
                .iter()
                .map(|m| m["method"].as_str().unwrap())
                .collect();
            let thread = if resume.is_some() {
                "thread/resume"
            } else {
                "thread/start"
            };
            assert_eq!(methods, ["config/read", thread, "turn/start"]);
            assert_eq!(asked[0]["params"]["cwd"], cwd);
            let params = &asked[1]["params"];
            assert_eq!(params["approvalPolicy"], "never");
            assert_eq!(params["config"]["default_permissions"], OWN_PROFILE);
            assert!(params.get("sandbox").is_none(), "{params}");
        }
        let (turn, asked) = run(false, None, Value::Null, json!({"thread": {"id": "t1"}})).await;
        assert_eq!(turn.text(), "Done.");
        assert_eq!(asked[0]["method"], "thread/start");
        assert_eq!(asked[0]["params"]["approvalPolicy"], "untrusted");
        assert_eq!(asked[0]["params"]["sandbox"], "danger-full-access");
        // Its `notify` back from a managed layer: no thread. A resumed thread in the profile it was
        // saved with: no turn.
        let mut notifies = own_config(cwd);
        notifies["config"]["notify"] = json!(["/bin/sh", "notify.sh"]);
        let (turn, asked) = run(true, None, notifies, ours.clone()).await;
        assert!(turn.error().contains("(notify)"), "{}", turn.error());
        assert_eq!(asked.len(), 1, "{asked:?}");
        let mut wide = ours;
        wide["activePermissionProfile"]["id"] = json!(":danger-full-access");
        let (turn, asked) = run(true, Some("t1"), own_config(cwd), wide).await;
        assert!(
            turn.error().contains("permission profile"),
            "{}",
            turn.error()
        );
        assert_eq!(asked.len(), 2, "{asked:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// What Codex does that obid doesn't follow or parse still reaches the job as a tool call of a
    /// kind obid doesn't know (crate::secretspec, fail closed): a helper thread, said once, and an
    /// item of a kind obid doesn't name.
    #[tokio::test]
    async fn a_helper_thread_and_an_unknown_item_are_tool_calls() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-codex-unseen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bin = root.join("codex");
        std::fs::write(
            &bin,
            r#"#!/bin/sh
read l; echo '{"id":1,"result":{}}'
read l
read l; echo '{"id":2,"result":{"thread":{"id":"t1"}}}'
read l; echo '{"id":3,"result":{"turn":{"id":"u1"}}}'
echo '{"method":"item/started","params":{"threadId":"t2","turnId":"x","item":{"type":"agentMessage","id":"h1"}}}'
echo '{"method":"item/started","params":{"threadId":"t2","turnId":"x","item":{"type":"webSearch","id":"h2","query":"q"}}}'
echo '{"method":"item/started","params":{"threadId":"t1","turnId":"u1","item":{"type":"someItemLater","id":"i1"}}}'
echo '{"method":"turn/completed","params":{"threadId":"t1","turn":{"id":"u1","status":"completed","items":[{"type":"agentMessage","text":"Done."}]}}}'
exec sleep 30
"#,
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let launch = Launch {
            argv: vec![bin.into_os_string()],
            env: vec![("PATH".into(), "/usr/bin:/bin".into())],
            cwd: root.clone(),
            stderr: root.join("agent.log"),
            own_sandbox: false,
            group: None,
        };
        let (_stop, stop) = watch::channel(process::Signal::Run);
        let tools = std::sync::Mutex::new(Vec::new());
        run_turn(
            launch,
            "Do it.",
            &Options::default(),
            Duration::from_secs(60),
            stop,
            |_, _| Ruling::Now(Answer::Allow),
            |event| {
                if let Event::Tool { name, .. } = event {
                    tools.lock().unwrap().push(name);
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(*tools.lock().unwrap(), ["Agent", "Tool"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A conversation's Codex that runs a command is ended at once, with everything it started:
    /// no grace to read out (finding 6).
    #[tokio::test]
    async fn a_command_ends_the_codex_process_at_once() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("obid-codex-fatal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bin = root.join("codex");
        std::fs::write(
            &bin,
            r#"#!/bin/sh
read l; echo '{"id":1,"result":{}}'
read l
read l; echo '{"id":2,"result":{"thread":{"id":"t1"}}}'
read l; echo '{"id":3,"result":{"turn":{"id":"u1"}}}'
echo '{"method":"item/started","params":{"threadId":"t1","turnId":"u1","item":{"type":"commandExecution","id":"c1","command":"/bin/zsh -lc '"'"'cat x'"'"'"}}}'
exec sleep 30
"#,
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let launch = Launch {
            own_sandbox: false,
            group: None,
            argv: vec![bin.into_os_string()],
            env: vec![("PATH".into(), "/usr/bin:/bin".into())],
            cwd: root.clone(),
            stderr: root.join("agent.log"),
        };
        let (_stop, stop) = watch::channel(process::Signal::Run);
        let options = Options {
            no_commands: true,
            ..Options::default()
        };
        let began = std::time::Instant::now();
        let turn = run_turn(
            launch,
            "Do it.",
            &options,
            Duration::from_secs(60),
            stop,
            |_, _| Ruling::Now(Answer::Allow),
            |_| {},
        )
        .await
        .unwrap();
        let took = began.elapsed();
        assert_eq!(
            turn.fatal.as_deref(),
            Some(
                "Codex ran a command (cat x), which a conversation never does, so obid stopped it"
            )
        );
        assert!(took < Duration::from_millis(1500), "{took:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A conversation's Codex may only do what's on the list: any other item or notification, a
    /// kind Codex adds later too, ends the turn.
    #[test]
    fn only_what_a_conversation_may_do_passes() {
        let item =
            |kind: &str| json!({"method": "item/started", "params": {"item": {"type": kind}}});
        let notice = |method: &str| json!({"method": method, "params": {}});
        for kind in [
            "agentMessage",
            "reasoning",
            "plan",
            "webSearch",
            "fileChange",
            "contextCompaction",
            "userMessage",
            "sleep",
        ] {
            assert_eq!(ran_outside(&item(kind)), None, "{kind}");
        }
        for method in [
            "turn/started",
            "turn/completed",
            "thread/tokenUsage/updated",
            "item/agentMessage/delta",
            "item/reasoning/textDelta",
            "item/plan/delta",
            "item/fileChange/patchUpdated",
            "item/mcpToolCall/progress",
            "account/rateLimits/updated",
            "serverRequest/resolved",
            "thread/queue/changed",
            "thread/closed",
            "thread/goal/cleared",
        ] {
            assert_eq!(ran_outside(&notice(method)), None, "{method}");
        }
        let oby = json!({"method": "item/started",
            "params": {"item": {"type": "mcpToolCall", "server": "oby", "tool": "oby_jobs"}}});
        assert_eq!(ran_outside(&oby), None);
        for kind in [
            "imageGeneration",
            "imageView",
            "collabAgentToolCall",
            "subAgentActivity",
            "dynamicToolCall",
            "hookPrompt",
            "enteredReviewMode",
            "somethingNew",
        ] {
            assert_eq!(
                ran_outside(&item(kind)),
                Some(format!("Codex used {kind}")),
                "{kind}"
            );
        }
        assert_eq!(
            ran_outside(&json!({"method": "item/completed", "params": {"item": {}}})),
            Some("Codex used an item of no kind".into())
        );
        for method in [
            "process/outputDelta",
            "process/exited",
            "command/exec/outputDelta",
            "item/dynamicToolCall/progress",
            "thread/realtime/started",
            "somethingNew",
        ] {
            assert_eq!(
                ran_outside(&notice(method)),
                Some(format!("Codex sent {method}")),
                "{method}"
            );
        }
        let remote = |status: &str| json!({"method": "remoteControl/status/changed", "params": {"status": status}});
        assert_eq!(ran_outside(&remote("disabled")), None);
        // Only Oby's own MCP servers start: another (ChatGPT's apps, the person's) ends the turn.
        let starting = |name: &str| json!({"method": "mcpServer/startupStatus/updated", "params": {"name": name, "status": "starting"}});
        assert_eq!(ran_outside(&starting("oby")), None);
        // Tetrix's reads, given to a conversation when it's connected, start and run too.
        assert_eq!(ran_outside(&starting("tetrix")), None);
        let tetrix = json!({"method": "item/completed",
            "params": {"item": {"type": "mcpToolCall", "server": "tetrix", "tool": "search"}}});
        assert_eq!(ran_outside(&tetrix), None);
        for name in ["codex_apps", "elsewhere", ""] {
            assert_eq!(
                ran_outside(&starting(name)),
                Some("Codex sent mcpServer/startupStatus/updated".into()),
                "{name}"
            );
        }
        for status in ["connecting", "connected", "errored"] {
            assert_eq!(
                ran_outside(&remote(status)),
                Some("Codex sent remoteControl/status/changed".into()),
                "{status}"
            );
        }
        assert_eq!(
            ran_outside(&notice("hook/started")),
            Some("Codex ran a hook".into())
        );
        // An answer to obid's own request is no notification.
        assert_eq!(ran_outside(&json!({"id": 3, "result": {}})), None);
    }

    #[test]
    fn the_shell_codex_wraps_commands_in_comes_off() {
        assert_eq!(unwrap_shell("/bin/zsh -lc 'git status'"), "git status");
        assert_eq!(
            unwrap_shell("/bin/bash -c 'echo '\\''hi'\\'''"),
            "echo 'hi'"
        );
        assert_eq!(unwrap_shell("node --test"), "node --test");
        // Codex's newer commands are double-quoted (#634).
        assert_eq!(
            unwrap_shell(r#"/bin/zsh -lc "git commit -m \"fix: it\" && echo \$HOME""#),
            r#"git commit -m "fix: it" && echo $HOME"#
        );
        assert_eq!(unwrap_shell("/bin/bash -lc ls"), "ls");
        // What the outer shell would expand, or more than one word, stays as it is.
        for kept in [
            r#"/bin/zsh -lc "echo $(whoami)""#,
            "/bin/zsh -lc 'ls' extra",
            "/usr/bin/python3 -c 'print(1)'",
        ] {
            assert_eq!(unwrap_shell(kept), kept);
        }
        assert_eq!(
            command_of(&json!(["git", "push", "origin"])).as_deref(),
            Some("git push origin")
        );
    }

    #[test]
    fn items_become_steps() {
        assert_eq!(
            step(&json!({"type": "commandExecution", "command": "/bin/zsh -lc 'node --test'"})),
            Some("Running node --test".into())
        );
        assert_eq!(
            step(&json!({"type": "commandExecution", "command": "cat a",
                         "commandActions": [{"type": "read", "path": "/w/src/greet.js"}]})),
            Some("Reading greet.js".into())
        );
        assert_eq!(
            step(&json!({"type": "fileChange", "changes": [{"path": "/w/a.js"}]})),
            Some("Editing a.js".into())
        );
        assert_eq!(
            step(&json!({"type": "fileChange", "changes": [{"path": "a"}, {"path": "b"}]})),
            Some("Editing 2 files".into())
        );
        assert_eq!(step(&json!({"type": "reasoning"})), None);
    }

    /// A conversation shows Codex's items as the Claude Code tool calls they are.
    #[test]
    fn items_are_the_tool_calls_they_are() {
        let call = |item: Value| tool_call(&item).unwrap();
        assert_eq!(
            call(
                json!({"type": "commandExecution", "command": "/bin/zsh -lc 'cat a'",
                        "commandActions": [{"type": "read", "path": "/r/a"}]})
            ),
            ("Read".into(), json!({"file_path": "/r/a"}))
        );
        assert_eq!(
            call(json!({"type": "commandExecution", "command": "/bin/zsh -lc 'ls && rm x'"})),
            ("Bash".into(), json!({"command": "ls && rm x"}))
        );
        assert_eq!(
            call(
                json!({"type": "mcpToolCall", "server": "oby", "tool": "oby_job", "arguments": {"id": "j1"}})
            ),
            ("mcp__oby__oby_job".into(), json!({"id": "j1"}))
        );
        assert_eq!(
            call(json!({"type": "webSearch", "query": "rust"})),
            ("WebSearch".into(), json!({"query": "rust"}))
        );
        assert_eq!(
            call(json!({"type": "imageView", "path": "/r/a.png"})),
            ("Read".into(), json!({"file_path": "/r/a.png"}))
        );
        assert_eq!(tool_call(&json!({"type": "agentMessage"})), None);
    }

    #[test]
    fn a_change_names_its_files_and_where_a_move_goes() {
        let started = json!({"method": "item/started", "params": {"item": {"type": "fileChange", "id": "i1",
            "changes": [{"path": "/w/a"}, {"path": "/w/b", "kind": {"type": "update", "move_path": "/w/c"}}]}}});
        assert_eq!(
            changes(&started),
            Some((
                "i1".into(),
                vec!["/w/a".into(), "/w/b".into(), "/w/c".into()]
            ))
        );
        let updated = json!({"method": "item/fileChange/patchUpdated", "params": {"itemId": "i1", "changes": [{"path": "/w/d"}]}});
        assert_eq!(changes(&updated), Some(("i1".into(), vec!["/w/d".into()])));
    }

    #[test]
    fn turns_end_with_what_the_agent_said() {
        assert_eq!(
            finished(
                &json!({"status": "completed", "items": [{"type": "agentMessage", "text": "Added a."}]}),
                None
            ),
            ("Added a.".into(), false)
        );
        assert_eq!(
            finished(
                &json!({"status": "completed", "items": []}),
                Some("From item/completed.")
            ),
            ("From item/completed.".into(), false)
        );
        assert_eq!(
            finished(&json!({"status": "completed"}), None),
            ("Done.".into(), false)
        );
        assert_eq!(
            finished(&json!({"status": "interrupted"}), None),
            ("Stopped.".into(), true)
        );
        assert_eq!(
            finished(
                &json!({"status": "failed", "error": {"message": "{\"error\": {\"message\": \"You've hit your usage limit.\"}}"}}),
                None
            ),
            (
                "Codex hit an error: You've hit your usage limit.".into(),
                true
            )
        );
    }

    /// Codex 0.162.1's `TurnError`: `codexErrorInfo` says how its turn ended, and only Codex's own
    /// words are kept (RECOVERY-7).
    #[test]
    fn recovery_7_codex_error_info_says_how_a_turn_ended() {
        let of = |info: Value, message: &str| {
            failure(&json!({"status": "failed", "error": {"message": message, "codexErrorInfo": info},
                "items": [{"type": "agentMessage", "text": "You've hit your usage limit, a stranger says."}]}))
            .unwrap()
        };
        assert_eq!(
            of(json!("contextWindowExceeded"), "x").cause,
            Cause::TooLong
        );
        assert_eq!(of(json!("usageLimitExceeded"), "x").cause, Cause::Limit);
        assert_eq!(of(json!("rateLimitExceeded"), "x").cause, Cause::Busy);
        assert_eq!(of(json!("serverOverloaded"), "x").cause, Cause::Passing);
        assert_eq!(of(json!("unauthorized"), "x").cause, Cause::Login);
        assert_eq!(
            of(
                json!({"responseStreamDisconnected": {"httpStatusCode": null}}),
                "x"
            )
            .cause,
            Cause::Passing
        );
        assert_eq!(
            of(
                json!({"httpConnectionFailed": {"httpStatusCode": 502}}),
                "x"
            )
            .cause,
            Cause::Passing
        );
        assert_eq!(
            of(
                json!({"httpConnectionFailed": {"httpStatusCode": 401}}),
                "x"
            )
            .cause,
            Cause::Login
        );
        assert_eq!(
            of(
                json!({"httpConnectionFailed": {"httpStatusCode": 404}}),
                "x"
            )
            .cause,
            Cause::Other
        );
        let said = of(json!("internalServerError"), "it broke");
        assert_eq!(said.words, "Codex hit an error: it broke");
        // A Codex with none: its own words, never the agent's.
        let plain = of(Value::Null, "stream disconnected before completion");
        assert_eq!(plain.cause, Cause::Passing);
        assert!(failure(&json!({"status": "completed"})).is_none());
    }
}
