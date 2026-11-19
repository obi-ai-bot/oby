//! The lab's `lanes` feature (src/lab/mod.rs): the owner's own work run as T3 Code runs the CLIs,
//! on Linux and macOS alike, proven before obid takes it (the owner's words of 2026-11-19, "Extend
//! the lanes to Linux too" and "Yes, match T3"). Owned by the session building it; nothing else
//! edits this file.
//!
//! [`own`] is the logic obid can take as is: plain `std`, no crate of this suite's, one code path
//! for every OS. It builds an own run's launch (its program, arguments, environment and folder)
//! from the account's folder and Oby's mode, maps Oby's modes to Codex's and Claude Code's own
//! settings as T3 Code does, and sweeps what a run left running. The nodes around it compile the
//! same file into a small `lanes` program, here (`lab.lanes.local`) and on a lab machine
//! (`lab.lanes.install`), so a step runs the very code obid would.
#![allow(unexpected_cfgs)]

#[cfg(not(lab_lanes_helper))]
use anyhow::{bail, Context, Result};
#[cfg(not(lab_lanes_helper))]
use serde_json::{json, Value};

#[cfg(not(lab_lanes_helper))]
use crate::obid::Run;

/// Every `lab.lanes.*` node: its name, what it does.
#[cfg(not(lab_lanes_helper))]
pub const NODES: &[(&str, &str)] = &[
    ("lab.lanes.install", "Answers the bash command that writes this feature's own logic (src/lab/lanes.rs) onto a lab machine and compiles it there into ~/bin/lanes with the machine's rustc, for a lab.machine.run step."),
    ("lab.lanes.local", "Compiles the same logic into a lanes program on this computer (once a workflow) and runs a bash command with $LANES naming it: its exit status, stdout and stderr; with expect. Skipped without rustc."),
    ("lab.lanes.same", "Compares what two computers' lanes programs answered (each a JSON line), with each computer's home folder read as ~: equal, or the first difference."),
];

#[cfg(not(lab_lanes_helper))]
const SOURCE: &str = include_str!("lanes.rs");

/// The program the helper's `main` is: `lanes <command> …` (own::cli).
#[cfg(not(lab_lanes_helper))]
const MAIN: &str = "#[path = \"lanes.rs\"]\nmod lanes;\nfn main() {\n    std::process::exit(lanes::own::cli(std::env::args().skip(1).collect()));\n}\n";

#[cfg(not(lab_lanes_helper))]
pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    match name {
        "lab.lanes.install" => Ok(json!({ "command": install_command() })),
        "lab.lanes.local" => {
            let helper = match local_helper(run).await? {
                Ok(path) => path,
                Err(why) => return Ok(json!({ "skipped": why })),
            };
            let command = input["command"].as_str().context("command")?;
            let limit = std::time::Duration::from_secs(input["limit"].as_u64().unwrap_or(120));
            let child = tokio::process::Command::new("bash")
                .args(["-lc", command])
                .env("LANES", &helper)
                .env("LAB_SCRATCH", &run.scratch)
                .current_dir(&run.scratch)
                .stdin(std::process::Stdio::null())
                .kill_on_drop(true)
                .output();
            let out = tokio::time::timeout(limit, child)
                .await
                .with_context(|| format!("took over {}s", limit.as_secs()))??;
            Ok(json!({
                "status": out.status.code().unwrap_or(-1),
                "stdout": String::from_utf8_lossy(&out.stdout),
                "stderr": String::from_utf8_lossy(&out.stderr),
            }))
        }
        "lab.lanes.same" => {
            let read = |key: &str| -> Result<Value> {
                let text = input[key]["stdout"]
                    .as_str()
                    .or_else(|| input[key].as_str())
                    .with_context(|| format!("{key}: a lanes program's stdout"))?;
                let home = input[key]["home"]
                    .as_str()
                    .or(input[format!("{key}Home")].as_str());
                let text = match home {
                    Some(home) if !home.is_empty() => text.replace(home, "~"),
                    _ => text.to_owned(),
                };
                text.lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| serde_json::from_str(l).with_context(|| format!("{key}: {l}")))
                    .collect::<Result<Vec<Value>>>()
                    .map(Value::from)
            };
            let (a, b) = (read("a")?, read("b")?);
            if a != b {
                bail!("the two computers answered differently:\n{a}\n{b}");
            }
            Ok(json!({ "same": true, "lines": a.as_array().map_or(0, Vec::len) }))
        }
        other => bail!("{other} isn't a lanes node"),
    }
}

/// The bash command that writes this file and its `main` into `~/lanes-helper` on a machine and
/// compiles them into `~/bin/lanes` there (rustc on its `PATH`, or in `~/.cargo/bin`).
#[cfg(not(lab_lanes_helper))]
fn install_command() -> String {
    for delimiter in ["LANES_RS_EOF", "LANES_MAIN_EOF"] {
        assert!(!SOURCE.lines().any(|l| l == delimiter));
    }
    format!(
        "set -e; mkdir -p ~/lanes-helper ~/bin; cat > ~/lanes-helper/lanes.rs <<'LANES_RS_EOF'\n{SOURCE}\nLANES_RS_EOF\ncat > ~/lanes-helper/main.rs <<'LANES_MAIN_EOF'\n{MAIN}\nLANES_MAIN_EOF\nPATH=$HOME/.cargo/bin:$PATH rustc --edition 2021 -O --cfg lab_lanes_helper -A warnings ~/lanes-helper/main.rs -o ~/bin/lanes; ~/bin/lanes version"
    )
}

/// The lanes program compiled on this computer, once a workflow; Err(why) when it can't be.
#[cfg(not(lab_lanes_helper))]
async fn local_helper(run: &Run) -> Result<std::result::Result<std::path::PathBuf, String>> {
    let dir = run.scratch.join("lanes-helper");
    let helper = dir.join("lanes");
    if helper.exists() {
        return Ok(Ok(helper));
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("lanes.rs"), SOURCE)?;
    std::fs::write(dir.join("main.rs"), MAIN)?;
    let built = tokio::process::Command::new("rustc")
        .args([
            "--edition",
            "2021",
            "-O",
            "--cfg",
            "lab_lanes_helper",
            "-A",
            "warnings",
        ])
        .arg(dir.join("main.rs"))
        .arg("-o")
        .arg(&helper)
        .output()
        .await;
    match built {
        Err(e) => Ok(Err(format!("skipped: no rustc here ({e})"))),
        Ok(out) if !out.status.success() => bail!(
            "the lanes program didn't compile: {}",
            String::from_utf8_lossy(&out.stderr)
        ),
        Ok(_) => Ok(Ok(helper)),
    }
}

/// The owner's own run, as T3 Code runs the CLIs: obid takes this module as it is.
pub mod own {
    use std::path::{Path, PathBuf};

    /// Oby's permission modes (obid's `permissions::Mode`), by the names its API uses.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Mode {
        Ask,
        AcceptEdits,
        Plan,
        Sandbox,
        Bypass,
    }

    pub const MODES: [Mode; 5] = [
        Mode::Ask,
        Mode::AcceptEdits,
        Mode::Plan,
        Mode::Sandbox,
        Mode::Bypass,
    ];

    impl Mode {
        pub fn parse(name: &str) -> Option<Mode> {
            MODES.into_iter().find(|m| m.name() == name)
        }

        pub fn name(self) -> &'static str {
            match self {
                Mode::Ask => "ask",
                Mode::AcceptEdits => "acceptEdits",
                Mode::Plan => "plan",
                Mode::Sandbox => "sandbox",
                Mode::Bypass => "bypass",
            }
        }

        /// T3 Code's runtime mode for it (`RuntimeMode` in T3's
        /// packages/contracts/src/providerPolicy.ts, default `full-access`): Ask waits for the
        /// owner (`approval-required`), Accept edits lets edits through (`auto-accept-edits`),
        /// Sandbox lets what stays in the sandbox through (`auto`), Bypass is `full-access`. Plan
        /// is T3's `plan` interaction mode over `approval-required`: it reads, and asks the rest.
        pub fn t3_runtime_mode(self) -> &'static str {
            match self {
                Mode::Ask | Mode::Plan => "approval-required",
                Mode::AcceptEdits => "auto-accept-edits",
                Mode::Sandbox => "auto",
                Mode::Bypass => "full-access",
            }
        }
    }

    /// Codex's own settings for an own run in `mode`, as T3 Code's `codexRuntimeModeTurnDefaults`
    /// (apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts) sets them: Codex's own
    /// sandbox and approvals are the limit, and obid adds no profile of its own.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct CodexPolicy {
        /// `approval_policy`, as TOML.
        pub approval_policy: &'static str,
        /// T3's `sandboxPolicy` as Codex's `sandbox_mode` names it.
        pub sandbox_mode: &'static str,
        /// The built-in permission profile that is that sandbox (`default_permissions`, and
        /// `codex sandbox -P`), so the launch and a measurement name the very same one.
        pub profile: &'static str,
        /// Whether its commands reach the network: Codex's own default for that sandbox.
        pub network: bool,
    }

    /// T3's `full-access` approvals: Codex asks nobody, but an MCP server's own question
    /// (`mcp_elicitations`), which keeps Oby's own tools (`oby_ask_owner`) working (LANES-21).
    pub const FULL_ACCESS_APPROVALS: &str = "{granular={mcp_elicitations=true, request_permissions=false, rules=false, sandbox_approval=false, skill_approval=false}}";

    pub fn codex_policy(mode: Mode) -> CodexPolicy {
        match mode.t3_runtime_mode() {
            // T3 sends `untrusted` per turn over the app-server; codex-cli 0.163.0 refuses it in
            // its configuration ("approval_policy = \"untrusted\" is no longer supported"), so the
            // launch asks on request, in a sandbox that writes nothing: every write asks.
            "approval-required" => CodexPolicy {
                approval_policy: "\"on-request\"",
                sandbox_mode: "read-only",
                profile: ":read-only",
                network: false,
            },
            "auto-accept-edits" | "auto" => CodexPolicy {
                approval_policy: "\"on-request\"",
                sandbox_mode: "workspace-write",
                profile: ":workspace",
                network: false,
            },
            _ => CodexPolicy {
                approval_policy: FULL_ACCESS_APPROVALS,
                sandbox_mode: "danger-full-access",
                profile: ":danger-full-access",
                network: true,
            },
        }
    }

    /// Claude Code's `--permission-mode` for an own run in `mode`, as T3 Code's
    /// `permissionModeForClaudeRuntimePolicy` (ClaudeAdapterV2.ts) picks it with no sandbox of
    /// its own: `plan` for Plan, else by the runtime mode.
    pub fn claude_permission_mode(mode: Mode) -> &'static str {
        if mode == Mode::Plan {
            return "plan";
        }
        match mode.t3_runtime_mode() {
            "approval-required" => "default",
            "auto-accept-edits" => "acceptEdits",
            "auto" => "auto",
            _ => "bypassPermissions",
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Tool {
        Claude,
        Codex,
    }

    impl Tool {
        pub fn parse(name: &str) -> Option<Tool> {
            match name {
                "claude" => Some(Tool::Claude),
                "codex" => Some(Tool::Codex),
                _ => None,
            }
        }
    }

    /// What an own run is launched from.
    #[derive(Clone, Debug)]
    pub struct Input {
        pub tool: Tool,
        pub mode: Mode,
        /// The person's own home: the CLI's `HOME`, as in their terminal.
        pub home: PathBuf,
        /// The account's own folder, when it has one (`configDir`); none: the CLI's default.
        pub account_dir: Option<PathBuf>,
        /// The worktree or the room: the run's working folder.
        pub workdir: PathBuf,
        /// The owner's login shell's environment.
        pub owners_env: Vec<(String, String)>,
        /// The run's tag ([`RUN_TAG`]), which its processes inherit, for [`sweep`].
        pub run_tag: String,
    }

    /// An own run's launch: what obid starts, as it starts it.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Launch {
        pub program: String,
        pub args: Vec<String>,
        /// The whole environment, sorted by name: nothing else is inherited.
        pub env: Vec<(String, String)>,
        pub cwd: PathBuf,
    }

    /// The variable every process of an own run inherits, naming its run.
    pub const RUN_TAG: &str = "OBY_RUN_TAG";

    /// What of the owner's environment an own run never gets: obid's own (`OBY_*`, `OBID_*`),
    /// git's (`GIT_*`, obid pins the job's git folders), what publishes (`GH_*`, `SSH_AUTH_SOCK`;
    /// LANES-2), the account's folder, which is the dispatcher's choice, and a launching Claude
    /// Code session's own variables.
    pub fn withheld(tool: Tool, name: &str) -> bool {
        const PREFIXES: [&str; 4] = ["OBY_", "OBID_", "GIT_", "GH_"];
        const NAMES: [&str; 8] = [
            "SSH_AUTH_SOCK",
            "CLAUDECODE",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_SSE_PORT",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_SECURESTORAGE_CONFIG_DIR",
            "CODEX_HOME",
        ];
        let claudes = name.starts_with("CLAUDE") || name.starts_with("ANTHROPIC_");
        PREFIXES.iter().any(|p| name.starts_with(p))
            || NAMES.contains(&name)
            // Claude Code's gateway is Claude Code's alone (LANES-4).
            || (tool == Tool::Codex && claudes)
    }

    /// An own run's launch: the CLI on the account's own folder and login, the owner's
    /// environment and `HOME`, the CLI's own permissions as the limit, and no sandbox, settings
    /// source or MCP restriction of obid's (no `--settings`, `--setting-sources`,
    /// `--strict-mcp-config`, permission profile or `projects` trust). The same on every OS.
    pub fn launch(input: &Input) -> Launch {
        let mut env: Vec<(String, String)> = input
            .owners_env
            .iter()
            .filter(|(name, _)| !withheld(input.tool, name))
            .cloned()
            .collect();
        let mut set = |name: &str, value: String| {
            env.retain(|(n, _)| n != name);
            env.push((name.to_owned(), value));
        };
        set("HOME", text(&input.home));
        set(RUN_TAG, input.run_tag.clone());
        let (program, args) = match input.tool {
            Tool::Claude => {
                if let Some(dir) = &input.account_dir {
                    set("CLAUDE_CONFIG_DIR", text(dir));
                }
                let args = [
                    "-p",
                    "--input-format",
                    "stream-json",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--permission-mode",
                    claude_permission_mode(input.mode),
                ];
                ("claude", args.map(String::from).to_vec())
            }
            Tool::Codex => {
                let folder = (input.account_dir.clone()).unwrap_or(input.home.join(".codex"));
                set("CODEX_HOME", text(&folder));
                let policy = codex_policy(input.mode);
                let args = vec![
                    "app-server".to_owned(),
                    "-c".into(),
                    format!("approval_policy={}", policy.approval_policy),
                    "-c".into(),
                    format!("default_permissions=\"{}\"", policy.profile),
                ];
                ("codex", args)
            }
        };
        env.sort();
        Launch {
            program: program.into(),
            args,
            env,
            cwd: input.workdir.clone(),
        }
    }

    fn text(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// Where an own run reaches obid's own servers (Oby's tools, Tetrix), from obid's address for
    /// agents (`connections.api_url`): an own run is on the host, so on loopback, never at
    /// `10.0.2.2`, slirp4netns's host, which only Linux's box reaches (obid's `Jobs::tetrix_mcp`
    /// already does this for an own job's Tetrix; a conversation's `mcp_config` doesn't yet).
    pub fn agent_api(api: &str, own: bool) -> String {
        match api.strip_prefix("http://10.0.2.2:") {
            Some(port) if own => format!("http://127.0.0.1:{port}"),
            _ => api.to_owned(),
        }
    }

    // ---- The sweep: what an own run left running ends with it. ----

    extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
        fn getuid() -> u32;
    }
    #[cfg(target_os = "linux")]
    extern "C" {
        fn prctl(option: i32, ...) -> i32;
    }

    const SIGTERM: i32 = 15;
    const SIGKILL: i32 = 9;

    /// Makes this process the one an orphan of its descendants is handed to (Linux's child
    /// subreaper), so a `setsid` or `nohup` leftover stays its descendant after its parent ends.
    /// False where the OS has none (macOS).
    pub fn become_subreaper() -> bool {
        #[cfg(target_os = "linux")]
        {
            const PR_SET_CHILD_SUBREAPER: i32 = 36;
            // SAFETY: prctl with an integer argument changes only this process's own attribute.
            unsafe { prctl(PR_SET_CHILD_SUBREAPER, 1u64) == 0 }
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// One process as the sweep reads it.
    #[derive(Clone, Debug)]
    pub struct Proc {
        pub pid: u32,
        pub parent: u32,
        pub zombie: bool,
        pub mine: bool,
        pub tags: Vec<String>,
    }

    /// Every process of the machine the sweep may read (`/proc`); Err where there's no `/proc`.
    pub fn processes() -> Result<Vec<Proc>, String> {
        let uid = unsafe { getuid() };
        let dir = std::fs::read_dir("/proc").map_err(|e| format!("no /proc here: {e}"))?;
        let mut out = vec![];
        for entry in dir.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let path = entry.path();
            let Ok(stat) = std::fs::read_to_string(path.join("stat")) else {
                continue;
            };
            // `pid (comm) state ppid …`; comm may hold spaces and parentheses.
            let Some(rest) = stat.rfind(')').map(|at| &stat[at + 2..]) else {
                continue;
            };
            let mut fields = rest.split_whitespace();
            let state = fields.next().unwrap_or("");
            let parent = fields.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            let mine = std::os::unix::fs::MetadataExt::uid(&match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            }) == uid;
            let tags = std::fs::read(path.join("environ"))
                .unwrap_or_default()
                .split(|b| *b == 0)
                .filter_map(|v| std::str::from_utf8(v).ok())
                .filter_map(|v| v.strip_prefix(RUN_TAG).and_then(|v| v.strip_prefix('=')))
                .map(String::from)
                .collect();
            out.push(Proc {
                pid,
                parent,
                zombie: state == "Z",
                mine,
                tags,
            });
        }
        Ok(out)
    }

    /// The run's processes among `all`: the person's own, not a zombie, carrying `tag` or
    /// descending from `root` (the subreaper that started the run), never `root` or `me`.
    pub fn of_run(all: &[Proc], tag: &str, root: Option<u32>, me: u32) -> Vec<u32> {
        let descends = |mut pid: u32| -> bool {
            let Some(root) = root else { return false };
            for _ in 0..4096 {
                let Some(p) = all.iter().find(|p| p.pid == pid) else {
                    return false;
                };
                if p.parent == root {
                    return true;
                }
                if p.parent <= 1 {
                    return false;
                }
                pid = p.parent;
            }
            false
        };
        all.iter()
            .filter(|p| p.mine && !p.zombie && p.pid != me && Some(p.pid) != root)
            .filter(|p| p.tags.iter().any(|t| t == tag) || descends(p.pid))
            .map(|p| p.pid)
            .collect()
    }

    /// Ends what run `tag` left running: each round stops what it finds, waits up to a second,
    /// kills what's left, and reads again, until a round finds none (at most 10). How many it
    /// ended; Err where processes can't be read.
    pub fn sweep(tag: &str, root: Option<u32>) -> Result<usize, String> {
        let me = std::process::id();
        let mut ended = std::collections::BTreeSet::new();
        for _ in 0..10 {
            let found = of_run(&processes()?, tag, root, me);
            if found.is_empty() {
                return Ok(ended.len());
            }
            for pid in &found {
                // SAFETY: a signal to a process of this user's that the read above named.
                unsafe { kill(*pid as i32, SIGTERM) };
                ended.insert(*pid);
            }
            let until = std::time::Instant::now() + std::time::Duration::from_secs(1);
            while std::time::Instant::now() < until {
                let left = of_run(&processes()?, tag, root, me);
                if !found.iter().any(|p| left.contains(p)) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            for pid in of_run(&processes()?, tag, root, me) {
                unsafe { kill(pid as i32, SIGKILL) };
            }
        }
        Err(format!(
            "{} processes still ran after 10 sweeps",
            of_run(&processes()?, tag, root, me).len()
        ))
    }

    // ---- The lanes program (the lab's helper): `lanes <command> …`. ----

    /// A JSON string.
    pub fn quote(s: &str) -> String {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    /// A launch as one JSON line.
    pub fn launch_json(l: &Launch) -> String {
        let args: Vec<String> = l.args.iter().map(|a| quote(a)).collect();
        let env: Vec<String> = (l.env.iter())
            .map(|(k, v)| format!("{}:{}", quote(k), quote(v)))
            .collect();
        format!(
            "{{\"program\":{},\"args\":[{}],\"env\":{{{}}},\"cwd\":{}}}",
            quote(&l.program),
            args.join(","),
            env.join(","),
            quote(&text(&l.cwd))
        )
    }

    fn usage() -> i32 {
        eprintln!(
            "lanes version | modes | codex-profile <mode> | codex-approvals <mode> | claude-mode <mode> | mcp-url <api> <own|boxed>\n\
             | launch <claude|codex> <mode> <home> <workdir> [--account DIR] [--tag T] [--owners-env] [--env K=V]...\n\
             | exec <same as launch> -- <program> [args]   (runs program with the launch's env and folder)\n\
             | run <tag> [--no-sweep] -- <program> [args]   (a run: tagged, its leftovers swept when it ends)"
        );
        2
    }

    fn parse_launch(args: &[String]) -> Option<Input> {
        let (tool, mode) = (Tool::parse(args.first()?)?, Mode::parse(args.get(1)?)?);
        let (home, workdir) = (PathBuf::from(args.get(2)?), PathBuf::from(args.get(3)?));
        let mut input = Input {
            tool,
            mode,
            home,
            account_dir: None,
            workdir,
            owners_env: vec![],
            run_tag: "run".into(),
        };
        let mut rest = args[4..].iter();
        while let Some(flag) = rest.next() {
            match flag.as_str() {
                "--account" => input.account_dir = Some(rest.next()?.into()),
                "--tag" => input.run_tag = rest.next()?.clone(),
                "--owners-env" => input.owners_env.extend(std::env::vars()),
                "--env" => {
                    let (k, v) = rest.next()?.split_once('=')?;
                    input.owners_env.push((k.into(), v.into()));
                }
                _ => return None,
            }
        }
        Some(input)
    }

    /// The lanes program's commands; its exit status.
    pub fn cli(args: Vec<String>) -> i32 {
        let mode = |at: usize| args.get(at).and_then(|m| Mode::parse(m));
        match args.first().map(String::as_str) {
            Some("version") => {
                println!("lanes 1 on {}", std::env::consts::OS);
                0
            }
            Some("modes") => {
                for m in MODES {
                    let p = codex_policy(m);
                    println!(
                        "{{\"mode\":{},\"t3\":{},\"claude\":{},\"codexApprovals\":{},\"codexSandbox\":{},\"codexProfile\":{},\"codexNetwork\":{}}}",
                        quote(m.name()),
                        quote(m.t3_runtime_mode()),
                        quote(claude_permission_mode(m)),
                        quote(p.approval_policy),
                        quote(p.sandbox_mode),
                        quote(p.profile),
                        p.network
                    );
                }
                0
            }
            Some("codex-profile") => match mode(1) {
                Some(m) => {
                    println!("{}", codex_policy(m).profile);
                    0
                }
                None => usage(),
            },
            Some("codex-approvals") => match mode(1) {
                Some(m) => {
                    println!("approval_policy={}", codex_policy(m).approval_policy);
                    0
                }
                None => usage(),
            },
            Some("mcp-url") => match (args.get(1), args.get(2).map(String::as_str)) {
                (Some(api), Some(lane @ ("own" | "boxed"))) => {
                    println!("{}", agent_api(api, lane == "own"));
                    0
                }
                _ => usage(),
            },
            Some("claude-mode") => match mode(1) {
                Some(m) => {
                    println!("{}", claude_permission_mode(m));
                    0
                }
                None => usage(),
            },
            Some("launch") => match parse_launch(&args[1..]) {
                Some(input) => {
                    println!("{}", launch_json(&launch(&input)));
                    0
                }
                None => usage(),
            },
            Some("exec") => {
                let Some(at) = args.iter().position(|a| a == "--") else {
                    return usage();
                };
                let Some(input) = parse_launch(&args[1..at]) else {
                    return usage();
                };
                let Some(program) = args.get(at + 1) else {
                    return usage();
                };
                let l = launch(&input);
                let status = std::process::Command::new(program)
                    .args(&args[at + 2..])
                    .env_clear()
                    .envs(l.env.iter().map(|(k, v)| (k, v)))
                    .current_dir(&l.cwd)
                    .status();
                match status {
                    Ok(s) => s.code().unwrap_or(1),
                    Err(e) => {
                        eprintln!("lanes: {program} didn't start: {e}");
                        127
                    }
                }
            }
            Some("run") => {
                let (Some(tag), Some(at)) = (args.get(1), args.iter().position(|a| a == "--"))
                else {
                    return usage();
                };
                let swept = !args[2..at].iter().any(|a| a == "--no-sweep");
                let Some(program) = args.get(at + 1) else {
                    return usage();
                };
                let subreaper = become_subreaper();
                let status = std::process::Command::new(program)
                    .args(&args[at + 2..])
                    .env(RUN_TAG, tag)
                    .status();
                let exit = status.ok().and_then(|s| s.code()).unwrap_or(-1);
                let ended = if swept {
                    match sweep(tag, Some(std::process::id())) {
                        Ok(n) => n.to_string(),
                        Err(e) => quote(&e),
                    }
                } else {
                    "null".into()
                };
                println!("{{\"exit\":{exit},\"subreaper\":{subreaper},\"swept\":{ended}}}");
                0
            }
            _ => usage(),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn input(tool: Tool, mode: Mode) -> Input {
            Input {
                tool,
                mode,
                home: "/h".into(),
                account_dir: None,
                workdir: "/h/room".into(),
                owners_env: [
                    ("PATH", "/usr/bin"),
                    ("ANTHROPIC_BASE_URL", "https://gateway.example"),
                    ("GH_TOKEN", "x"),
                    ("SSH_AUTH_SOCK", "/s"),
                    ("OBY_SECRET", "x"),
                    ("GIT_DIR", "/elsewhere"),
                    ("CLAUDECODE", "1"),
                    ("HOME", "/not-the-person"),
                ]
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .to_vec(),
                run_tag: "t1".into(),
            }
        }

        fn has(l: &Launch, name: &str) -> Option<String> {
            l.env
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }

        /// LAB-LANES-1: Oby's modes become Codex's own settings as T3 Code's runtime modes do.
        #[test]
        fn lab_lanes_1_oby_modes_map_to_codex_as_t3s() {
            let got: Vec<(&str, &str)> = MODES
                .map(|m| {
                    (
                        codex_policy(m).sandbox_mode,
                        codex_policy(m).approval_policy,
                    )
                })
                .to_vec();
            assert_eq!(
                got,
                [
                    ("read-only", "\"on-request\""),
                    ("workspace-write", "\"on-request\""),
                    ("read-only", "\"on-request\""),
                    ("workspace-write", "\"on-request\""),
                    ("danger-full-access", FULL_ACCESS_APPROVALS),
                ]
            );
            assert!(codex_policy(Mode::Bypass).network && !codex_policy(Mode::Ask).network);
            let claude = MODES.map(claude_permission_mode);
            assert_eq!(
                claude,
                [
                    "default",
                    "acceptEdits",
                    "plan",
                    "auto",
                    "bypassPermissions"
                ]
            );
        }

        /// LAB-LANES-2: an own launch is the owner's environment and folder, and adds nothing of
        /// obid's that limits the CLI: no settings, settings sources, MCP restriction or profile.
        #[test]
        fn lab_lanes_2_an_own_launch_adds_no_sandbox_of_obids() {
            for mode in MODES {
                let l = launch(&input(Tool::Claude, mode));
                assert_eq!(l.program, "claude");
                for flag in [
                    "--settings",
                    "--setting-sources",
                    "--strict-mcp-config",
                    "--tools",
                ] {
                    assert!(!l.args.iter().any(|a| a == flag), "{flag}");
                }
                assert_eq!(has(&l, "HOME").as_deref(), Some("/h"));
                assert_eq!(
                    has(&l, "ANTHROPIC_BASE_URL").as_deref(),
                    Some("https://gateway.example")
                );
                for gone in [
                    "GH_TOKEN",
                    "SSH_AUTH_SOCK",
                    "OBY_SECRET",
                    "GIT_DIR",
                    "CLAUDECODE",
                    "CLAUDE_CONFIG_DIR",
                ] {
                    assert_eq!(has(&l, gone), None, "{gone}");
                }
                assert_eq!(has(&l, RUN_TAG).as_deref(), Some("t1"));
                assert_eq!(l.cwd, PathBuf::from("/h/room"));
                let c = launch(&input(Tool::Codex, mode));
                assert_eq!(has(&c, "CODEX_HOME").as_deref(), Some("/h/.codex"));
                assert_eq!(has(&c, "ANTHROPIC_BASE_URL"), None);
                assert!(!c
                    .args
                    .iter()
                    .any(|a| a.contains("oby_own") || a.starts_with("projects")));
            }
            let mut named = input(Tool::Claude, Mode::Bypass);
            named.account_dir = Some("/h/work".into());
            assert_eq!(
                has(&launch(&named), "CLAUDE_CONFIG_DIR").as_deref(),
                Some("/h/work")
            );
        }

        /// LAB-LANES-5: an own run reaches obid's servers on loopback; a boxed one keeps the box's
        /// host address, and macOS's loopback address stays as it is.
        #[test]
        fn lab_lanes_5_an_own_run_reaches_obid_on_loopback() {
            let linux = "http://10.0.2.2:7777";
            assert_eq!(agent_api(linux, true), "http://127.0.0.1:7777");
            assert_eq!(agent_api(linux, false), linux);
            assert_eq!(
                agent_api("http://127.0.0.1:7777", true),
                "http://127.0.0.1:7777"
            );
        }

        /// LAB-LANES-3: the sweep takes the run's own (its tag, or under its subreaper), never
        /// another run's, the person's other processes or a zombie.
        #[test]
        fn lab_lanes_3_the_sweep_takes_only_the_runs_own() {
            let p = |pid, parent, tag: &str, mine, zombie| Proc {
                pid,
                parent,
                zombie,
                mine,
                tags: if tag.is_empty() {
                    vec![]
                } else {
                    vec![tag.into()]
                },
            };
            let all = [
                p(10, 1, "", true, false),   // the run's subreaper
                p(11, 10, "a", true, false), // its agent
                p(12, 1, "a", true, false),  // a setsid leftover, handed to init
                p(13, 10, "", true, false),  // a leftover that dropped its environment
                p(14, 1, "b", true, false),  // another run's
                p(15, 1, "", true, false),   // the person's own
                p(16, 1, "a", false, false), // someone else's, tagged alike
                p(17, 10, "a", true, true),  // a zombie
                p(18, 13, "", true, false),  // under the untagged leftover
            ];
            let mut got = of_run(&all, "a", Some(10), 99);
            got.sort();
            assert_eq!(got, [11, 12, 13, 18]);
            assert_eq!(of_run(&all, "a", None, 99), [11, 12]);
        }
    }
}

#[cfg(all(test, not(lab_lanes_helper)))]
mod tests {
    #[test]
    fn the_helper_source_carries_no_heredoc_delimiter() {
        let command = super::install_command();
        assert!(command.contains("--cfg lab_lanes_helper"));
        assert_eq!(command.matches("\nLANES_RS_EOF\n").count(), 1);
    }
}
