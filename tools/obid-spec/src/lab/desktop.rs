//! The lab's `desktop` feature (src/lab/mod.rs): agents driving the person's Mac through
//! agent-desktop (vercel-labs, the accessibility tree), as Oby would serve it, built and proven here
//! before it is ported (the owner's decisions of 2026-11-19).
//!
//! Two halves, as in `browser.rs`, and only the first is Oby's to port:
//!
//! 1. **The core** ([`Desktop`], [`TOOLS`]): the tools obid serves, each marked read-only or not
//!    (agent-desktop's own MCP serves 75 tools and marks none), the agent-desktop commands Oby
//!    never serves ([`NEVER_SERVED`]), which apps a conversation may drive (only those the person
//!    granted it, by PID), and the person's hold on the desktop, which refuses every act while
//!    reads go on (the browser's `may_act`). An app's words are quoted as outside data.
//! 2. **The lab**: agent-desktop pinned and hash-checked in the workflow's folder (as
//!    `src/desktop.rs` installs it), a test app of the lab's own (a small AppKit window compiled
//!    into the workflow's folder, invisible, never activated), and the `lab.desktop.*` nodes.
//!
//! Every act is delivered headless: a click is an accessibility press and a key goes to the app's
//! process, so nothing takes the person's focus or pointer. Without macOS, agent-desktop or its
//! Accessibility and Screen Recording permissions a node answers `skipped`; no node asks for, grants
//! or changes a permission.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::browser::{may_act, outside, Kind, Refusal};
use crate::obid::{private_dir, Run};

// ════════════════════════════════════════════════════════════════════════════════════════════
// 1. The core: what obid takes as it is.
// ════════════════════════════════════════════════════════════════════════════════════════════

pub struct DesktopTool {
    pub name: &'static str,
    pub kind: Kind,
    /// The agent-desktop command it runs.
    pub command: &'static str,
    pub description: &'static str,
}

/// The tools obid serves every agent, each marked: `Kind::Read` only reads.
pub const TOOLS: &[DesktopTool] = &[
    DesktopTool {
        name: "oby_desktop_list_windows",
        kind: Kind::Read,
        command: "list-windows",
        description: "The windows of the apps the person let you drive.",
    },
    DesktopTool {
        name: "oby_desktop_snapshot",
        kind: Kind::Read,
        command: "snapshot",
        description: "An app's window as an accessibility tree with refs, quoted as outside data.",
    },
    DesktopTool {
        name: "oby_desktop_read",
        kind: Kind::Read,
        command: "get",
        description: "One element's text, value, title, bounds, role or states, by ref.",
    },
    DesktopTool {
        name: "oby_desktop_click",
        kind: Kind::Act,
        command: "click",
        description:
            "Press an element by ref (an accessibility press: no pointer moves, no focus taken).",
    },
    DesktopTool {
        name: "oby_desktop_type",
        kind: Kind::Act,
        command: "type",
        description: "Insert text into a text element by ref.",
    },
    DesktopTool {
        name: "oby_desktop_set_value",
        kind: Kind::Act,
        command: "set-value",
        description: "Set an element's value by ref.",
    },
    DesktopTool {
        name: "oby_desktop_toggle",
        kind: Kind::Act,
        command: "toggle",
        description: "Toggle a checkbox or switch by ref.",
    },
    DesktopTool {
        name: "oby_desktop_scroll",
        kind: Kind::Act,
        command: "scroll",
        description: "Scroll an element by ref (`direction`, `amount`).",
    },
    DesktopTool {
        name: "oby_desktop_press",
        kind: Kind::Act,
        command: "press",
        description: "Send a key combo (cmd+k) to the app's process, without focusing it.",
    },
];

/// agent-desktop commands Oby never serves an agent, and why.
pub const NEVER_SERVED: &[(&str, &str)] = &[
    (
        "launch, close-app",
        "starting or quitting the person's apps is theirs to do",
    ),
    (
        "clipboard-get, clipboard-set, clipboard-clear",
        "the clipboard holds what the person copied, passwords included",
    ),
    (
        "list-notifications, notification-action, dismiss-*",
        "notifications are other apps' and other people's words to the person",
    ),
    (
        "mouse-*, hover, drag, double-click, triple-click, key-down, key-up",
        "physical input moves the person's own pointer and keys",
    ),
    (
        "focus-window, resize-window, move-window, minimize, maximize, restore",
        "the person's windows stay where they put them",
    ),
    (
        "open-system-surface, list-apps, list-displays",
        "outside the apps the person granted",
    ),
    (
        "sandbox, session, trace, cursor-overlay, batch, skills, mcp",
        "agent-desktop's own tooling, not the person's desktop",
    ),
];

pub fn tool(name: &str) -> Option<&'static DesktopTool> {
    TOOLS.iter().find(|t| t.name == name)
}

/// The rules the desktop core keeps; each is on in obid, switched off in the lab only to watch its
/// workflow go red (`OBID_SPEC_LAB_DESKTOP_BREAK=control,grants`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub control: bool,
    pub grants: bool,
}

impl Rules {
    pub const ALL: Rules = Rules {
        control: true,
        grants: true,
    };

    pub fn broken(names: &str) -> Rules {
        let mut r = Rules::ALL;
        for n in names.split(',').map(str::trim) {
            match n {
                "control" => r.control = false,
                "grants" => r.grants = false,
                _ => {}
            }
        }
        r
    }
}

/// An app as Oby resolved it from its PID (never from its name alone).
#[derive(Clone, Debug)]
pub struct App {
    pub pid: u32,
    pub window: String,
    pub name: String,
    /// No other running app shares its name (a key combo goes to an app by name).
    pub name_unique: bool,
}

#[derive(Clone, Debug)]
pub struct Desktop {
    pub rules: Rules,
    granted: BTreeMap<String, BTreeSet<u32>>,
    refs: BTreeMap<String, (String, u32)>,
    person: Option<String>,
    generation: u64,
}

impl Desktop {
    pub fn new(rules: Rules) -> Desktop {
        Desktop {
            rules,
            granted: BTreeMap::new(),
            refs: BTreeMap::new(),
            person: None,
            generation: 0,
        }
    }

    /// The person lets a conversation drive an app.
    pub fn grant(&mut self, conversation: &str, pid: u32) {
        self.granted
            .entry(conversation.into())
            .or_default()
            .insert(pid);
    }

    pub fn granted(&self, conversation: &str, pid: u32) -> bool {
        !self.rules.grants
            || self
                .granted
                .get(conversation)
                .is_some_and(|g| g.contains(&pid))
    }

    /// The person takes the desktop: every agent's act is refused until they give it back.
    pub fn take(&mut self, person: &str) -> Result<u64, Refusal> {
        match &self.person {
            Some(p) if p != person => Err(Refusal::HeldByOther),
            Some(_) => Ok(self.generation),
            None => {
                self.person = Some(person.into());
                self.generation += 1;
                self.refs.clear();
                Ok(self.generation)
            }
        }
    }

    pub fn release(&mut self, person: &str) -> Result<u64, Refusal> {
        if self.person.as_deref() != Some(person) {
            return Err(Refusal::NotHolder);
        }
        self.person = None;
        self.generation += 1;
        Ok(self.generation)
    }

    /// The agent-desktop arguments for a tool call, if this conversation may make it on this app.
    pub fn plan(
        &self,
        conversation: &str,
        tool_name: &str,
        args: &Value,
        app: &App,
    ) -> Result<Vec<String>, Refusal> {
        let tool = tool(tool_name).ok_or_else(|| Refusal::UnknownTool(tool_name.into()))?;
        let granted = self.granted(conversation, app.pid);
        let held = self.rules.control && self.person.is_some();
        if !may_act(granted, held, tool.kind) {
            return Err(if granted {
                Refusal::HumanControl
            } else {
                Refusal::NotGranted
            });
        }
        let reference = || -> Result<String, Refusal> {
            let r = args["ref"].as_str().ok_or_else(|| {
                Refusal::BadArgs("`ref` (from oby_desktop_snapshot) is required.".into())
            })?;
            match self.refs.get(r) {
                Some((c, pid)) if c == conversation && *pid == app.pid => Ok(r.to_string()),
                _ => Err(Refusal::UnknownRef(r.into())),
            }
        };
        let text = |k: &str| -> Result<String, Refusal> {
            args[k]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| Refusal::BadArgs(format!("`{k}` is required.")))
        };
        let a = |s: &str| s.to_string();
        Ok(match tool.command {
            "list-windows" => vec![a("list-windows")],
            "snapshot" => vec![a("snapshot"), a("--window-id"), app.window.clone()],
            "get" => vec![
                a("get"),
                reference()?,
                a("--property"),
                args["property"].as_str().unwrap_or("text").into(),
            ],
            "click" | "toggle" => vec![a(tool.command), reference()?],
            "type" | "set-value" => vec![a(tool.command), reference()?, text("text")?],
            "scroll" => vec![
                a("scroll"),
                reference()?,
                a("--direction"),
                args["direction"].as_str().unwrap_or("down").into(),
                a("--amount"),
                args["amount"].as_u64().unwrap_or(3).to_string(),
            ],
            "press" => {
                if !app.name_unique {
                    return Err(Refusal::BadArgs(format!(
                        "another running app is also named {}; a key combo can't be sent to only this one",
                        app.name
                    )));
                }
                vec![a("press"), text("keys")?, a("--app"), app.name.clone()]
            }
            other => return Err(Refusal::UnknownTool(other.into())),
        })
    }

    /// A snapshot's refs, now this conversation's for this app (a new snapshot replaces them).
    pub fn snapshot_taken(
        &mut self,
        conversation: &str,
        pid: u32,
        refs: impl IntoIterator<Item = String>,
    ) {
        self.refs
            .retain(|_, (c, p)| !(c == conversation && *p == pid));
        for r in refs {
            self.refs.insert(r, (conversation.into(), pid));
        }
    }
}

/// Every ref in a snapshot's tree.
pub fn refs_in(tree: &Value, out: &mut Vec<String>) {
    if let Some(r) = tree["ref_id"].as_str() {
        out.push(r.into());
    }
    for c in tree["children"].as_array().into_iter().flatten() {
        refs_in(c, out);
    }
}

// ════════════════════════════════════════════════════════════════════════════════════════════
// 2. The lab: agent-desktop pinned, a test app of the lab's own, the nodes.
// ════════════════════════════════════════════════════════════════════════════════════════════

/// agent-desktop's macOS binaries at `crate::desktop::VERSION` (1.3.1): their SHA-256, as the
/// package's own `.sha256` files also say. A binary that doesn't match isn't run.
pub const PINNED: &[(&str, &str)] = &[
    (
        "darwin-arm64",
        "9a6b0829736a486da532a9d1ceed1e07192fa8967326c7b27125646adab6baf7",
    ),
    (
        "darwin-x64",
        "f762f8ed6a14b114f8a50b902205d3ecb6bcf554d8031f67abd7938af88663cb",
    ),
];

pub const NODES: &[(&str, &str)] = &[
    ("lab.desktop.app", "Compiles and starts the lab's test app (a small AppKit window, invisible, never activated) in this workflow's folder, and waits for its window. Skipped off macOS, without swiftc, agent-desktop or its permissions."),
    ("lab.desktop.upstream", "agent-desktop's own MCP server: how many tools it serves and how many it marks read-only; with expect."),
    ("lab.desktop.tools", "The desktop tools obid serves, each marked read-only or not, and the agent-desktop commands it never serves."),
    ("lab.desktop.grant", "The person lets a conversation drive an app (by the lab.desktop.app step's PID)."),
    ("lab.desktop.tool", "Calls one oby_desktop_* tool as a conversation on an app, through the core: {ok, result} or {ok: false, error}. `refFor` {role, name} picks a ref from `snapshot`; with expect."),
    ("lab.desktop.take", "The person takes the desktop; with expect."),
    ("lab.desktop.release", "The person gives the desktop back; with expect."),
];

struct Lab {
    core: Desktop,
}

static LAB: Mutex<Option<Lab>> = Mutex::new(None);

fn with_lab<T>(f: impl FnOnce(&mut Lab) -> T) -> T {
    let mut guard = LAB.lock().unwrap();
    let lab = guard.get_or_insert_with(|| Lab {
        core: Desktop::new(match std::env::var("OBID_SPEC_LAB_DESKTOP_BREAK") {
            Ok(n) if !n.is_empty() => Rules::broken(&n),
            _ => Rules::ALL,
        }),
    });
    f(lab)
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::Digest;
    let bytes = std::fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    Ok(hex::encode(sha2::Sha256::digest(&bytes)))
}

/// agent-desktop for this workflow, hash-checked, with both permissions, or why not (a skip).
async fn ready(run: &Run) -> Result<std::result::Result<PathBuf, String>> {
    if let Some(why) = crate::desktop::unsupported(std::env::consts::OS) {
        return Ok(Err(why));
    }
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    let bin = match std::env::var_os("OBID_SPEC_AGENT_DESKTOP") {
        Some(given) => PathBuf::from(given),
        None => {
            let prefix = run.scratch.join("desktop/npm");
            let bin = prefix.join(format!(
                "node_modules/agent-desktop/bin/agent-desktop-darwin-{arch}"
            ));
            if !bin.is_file() {
                private_dir(&prefix)?;
                let out = tokio::process::Command::new("npm")
                    .args(["install", "--prefix"])
                    .arg(&prefix)
                    .arg(format!("agent-desktop@{}", crate::desktop::VERSION))
                    .args([
                        "--ignore-scripts",
                        "--no-audit",
                        "--no-fund",
                        "--loglevel=error",
                    ])
                    .output()
                    .await;
                if !matches!(out, Ok(ref o) if o.status.success()) {
                    return Ok(Err(
                        "skipped: agent-desktop isn't installed (npm install --prefix failed)"
                            .into(),
                    ));
                }
            }
            bin
        }
    };
    if !bin.is_file() {
        return Ok(Err(format!(
            "skipped: agent-desktop isn't installed ({} isn't there)",
            bin.display()
        )));
    }
    let want = PINNED
        .iter()
        .find(|(p, _)| *p == format!("darwin-{arch}"))
        .map(|(_, h)| *h)
        .unwrap_or_default();
    let got = sha256_file(&bin)?;
    anyhow::ensure!(
        got == want,
        "{} isn't agent-desktop {} as pinned: its SHA-256 is {got}, not {want}",
        bin.display(),
        crate::desktop::VERSION
    );
    let said = call(run, &bin, &["permissions"]).await?;
    Ok(match crate::desktop::permission_skip(&said) {
        Some(why) => Err(why),
        None => Ok(bin),
    })
}

/// One agent-desktop call: its JSON envelope. Its state folder is the workflow's.
async fn call(run: &Run, bin: &Path, args: &[&str]) -> Result<Value> {
    let home = run.scratch.join("desktop/home");
    private_dir(&home)?;
    let out = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::process::Command::new(bin)
            .args(args)
            .env("HOME", &home)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .with_context(|| format!("agent-desktop {} took over 60 s", args.join(" ")))??;
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).with_context(|| {
        format!(
            "agent-desktop {}: {}",
            args.join(" "),
            text.trim().chars().take(300).collect::<String>()
        )
    })
}

/// The lab's test app: a counter with a button, a label and a menu item (Counter › Reset, ⌘K).
const TEST_APP: &str = r#"import AppKit
final class Lab: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    let label = NSTextField(labelWithString: "Count: 0")
    var count = 0
    func applicationDidFinishLaunching(_ n: Notification) {
        let menu = NSMenu()
        let appItem = NSMenuItem(); menu.addItem(appItem)
        let appMenu = NSMenu(title: "Lab desktop")
        appMenu.addItem(withTitle: "Quit Lab desktop", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        let counterItem = NSMenuItem(); menu.addItem(counterItem)
        let counter = NSMenu(title: "Counter")
        counter.addItem(withTitle: "Reset", action: #selector(reset), keyEquivalent: "k").target = self
        counterItem.submenu = counter
        NSApp.mainMenu = menu
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 260, height: 110), styleMask: [.titled], backing: .buffered, defer: false)
        window.title = "Lab desktop"
        let button = NSButton(title: "Count", target: self, action: #selector(bump))
        button.frame = NSRect(x: 20, y: 20, width: 100, height: 30)
        label.frame = NSRect(x: 20, y: 60, width: 200, height: 24)
        window.contentView?.addSubview(button)
        window.contentView?.addSubview(label)
        // Invisible and never activated: the person's screen and focus stay theirs.
        window.alphaValue = 0
        window.orderFrontRegardless()
    }
    @objc func bump() { count += 1; label.stringValue = "Count: \(count)" }
    @objc func reset() { count = 0; label.stringValue = "Count: 0" }
}
let app = NSApplication.shared
let lab = Lab()
app.delegate = lab
app.setActivationPolicy(.regular)
app.run()
"#;

/// The test app's binary name, which is also the name agent-desktop knows it by.
const APP_NAME: &str = "oby-lab-desktop";

async fn start_app(run: &Run, bin: &Path) -> Result<std::result::Result<Value, String>> {
    let dir = run.scratch.join("desktop/app");
    private_dir(&dir)?;
    std::fs::write(dir.join("main.swift"), TEST_APP)?;
    let exe = dir.join(APP_NAME);
    let built = tokio::process::Command::new("xcrun")
        .args(["swiftc", "-O", "main.swift", "-o", APP_NAME])
        .current_dir(&dir)
        .output()
        .await;
    match built {
        Ok(o) if o.status.success() && exe.is_file() => {}
        Ok(o) => {
            return Ok(Err(format!(
                "skipped: the test app didn't compile (swiftc: {})",
                String::from_utf8_lossy(&o.stderr)
                    .trim()
                    .chars()
                    .take(300)
                    .collect::<String>()
            )))
        }
        Err(e) => return Ok(Err(format!("skipped: no swiftc here ({e})"))),
    }
    let child = std::process::Command::new(&exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("the test app didn't start")?;
    let pid = child.id();
    crate::procs::remember(pid, "the lab's desktop test app");
    match window_of(run, bin, pid).await? {
        Ok(window) => Ok(Ok(json!({"pid": pid, "window": window, "name": APP_NAME}))),
        Err(why) => Ok(Err(why)),
    }
}

/// Why a desktop step can't read the test app's window.
const OFF_SCREEN: &str = "skipped: the test app's window isn't on screen where agent-desktop reads (the person is in a full-screen Space, or the screen is locked); run it again from a desktop Space";

/// The app's window, by its PID, or why it can't be read (a skip): agent-desktop reads only the
/// windows on the person's current Space and display, so a person in a full-screen Space, or a
/// locked screen, leaves the test window unreadable.
async fn window_of(run: &Run, bin: &Path, pid: u32) -> Result<std::result::Result<String, String>> {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let said = call(run, bin, &["list-windows"]).await?;
        let mine: Vec<&Value> = said["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|w| w["pid"] == pid)
            .collect();
        if let Some(id) = mine
            .iter()
            .find(|w| w["accessible"] == true)
            .and_then(|w| w["id"].as_str())
        {
            return Ok(Ok(id.to_string()));
        }
        if Instant::now() >= until {
            return Ok(Err(if mine.is_empty() {
                format!("skipped: agent-desktop lists no window of the test app (pid {pid})")
            } else {
                OFF_SCREEN.to_string()
            }));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The app a step names, resolved by its PID: its window and whether its name is unique.
async fn app_of(run: &Run, bin: &Path, input: &Value) -> Result<App> {
    let pid = input["app"]["pid"]
        .as_u64()
        .context("app: a lab.desktop.app step's output")? as u32;
    let windows = call(run, bin, &["list-windows"]).await?;
    let list: Vec<&Value> = windows["data"].as_array().into_iter().flatten().collect();
    let mine = list
        .iter()
        .find(|w| w["pid"] == pid)
        .with_context(|| format!("no window of pid {pid}"))?;
    let name = mine["app_name"].as_str().unwrap_or_default().to_string();
    let pids: BTreeSet<u64> = list
        .iter()
        .filter(|w| w["app_name"] == name.as_str())
        .filter_map(|w| w["pid"].as_u64())
        .collect();
    Ok(App {
        pid,
        window: mine["id"].as_str().unwrap_or_default().into(),
        name,
        name_unique: pids.len() == 1,
    })
}

async fn upstream(run: &Run, bin: &Path) -> Result<Value> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let home = run.scratch.join("desktop/home");
    private_dir(&home)?;
    let mut child = tokio::process::Command::new(bin)
        .arg("mcp")
        .env("HOME", &home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().context("stdin")?;
    let mut lines = tokio::io::BufReader::new(child.stdout.take().context("stdout")?).lines();
    for m in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "obid-spec-lab", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    ] {
        stdin.write_all(format!("{m}\n").as_bytes()).await?;
    }
    let listed = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(line) = lines.next_line().await? {
            let v: Value = serde_json::from_str(&line).unwrap_or_default();
            if v["id"] == 2 {
                return Ok::<Value, anyhow::Error>(v);
            }
        }
        bail!("agent-desktop mcp closed before listing its tools")
    })
    .await??;
    let _ = child.kill().await;
    let tools = listed["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let read_only = tools
        .iter()
        .filter(|t| t["annotations"]["readOnlyHint"] == true)
        .count();
    let annotated = tools.iter().filter(|t| !t["annotations"].is_null()).count();
    Ok(json!({"tools": tools.len(), "annotated": annotated, "markedReadOnly": read_only}))
}

fn refusal(r: Refusal) -> Value {
    r.to_json()
}

pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    match name {
        "lab.desktop.tools" => {
            let read_only: serde_json::Map<String, Value> = TOOLS
                .iter()
                .map(|t| (t.name.to_string(), json!(t.kind.read_only())))
                .collect();
            return Ok(json!({
                "tools": TOOLS.iter().map(|t| json!({"name": t.name, "command": t.command, "readOnly": t.kind.read_only()})).collect::<Vec<_>>(),
                "readOnly": read_only,
                "neverServed": NEVER_SERVED.iter().map(|(c, why)| json!({"commands": c, "why": why})).collect::<Vec<_>>(),
            }));
        }
        "lab.desktop.grant" => {
            let conversation = input["conversation"].as_str().context("conversation")?;
            let pid = input["app"]["pid"]
                .as_u64()
                .context("app: a lab.desktop.app step's output")? as u32;
            with_lab(|l| l.core.grant(conversation, pid));
            return Ok(json!({"ok": true, "result": {"conversation": conversation, "pid": pid}}));
        }
        "lab.desktop.take" | "lab.desktop.release" => {
            let person = input["person"].as_str().unwrap_or("owner");
            let said = with_lab(|l| {
                if name == "lab.desktop.take" {
                    l.core.take(person)
                } else {
                    l.core.release(person)
                }
            });
            return Ok(match said {
                Ok(g) => json!({"ok": true, "result": {"controlGeneration": g}}),
                Err(r) => refusal(r),
            });
        }
        _ => {}
    }
    let bin = match ready(run).await? {
        Ok(b) => b,
        Err(why) => return Ok(json!({"skipped": why})),
    };
    match name {
        "lab.desktop.app" => Ok(match start_app(run, &bin).await? {
            Ok(v) => v,
            Err(why) => json!({"skipped": why}),
        }),
        "lab.desktop.upstream" => upstream(run, &bin).await,
        "lab.desktop.tool" => {
            let conversation = input["conversation"].as_str().context("conversation")?;
            let tool_name = input["tool"].as_str().context("tool")?;
            let app = app_of(run, &bin, input).await?;
            let mut args = input.get("args").cloned().unwrap_or(json!({}));
            if let (Some(want), Some(snap)) = (input.get("refFor"), input.get("snapshot")) {
                fn find(node: &Value, want: &Value) -> Option<String> {
                    if node["role"] == want["role"] && node["name"] == want["name"] {
                        if let Some(r) = node["ref_id"].as_str() {
                            return Some(r.into());
                        }
                    }
                    node["children"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find_map(|c| find(c, want))
                }
                let tree = &snap["result"]["outside"]["data"];
                args["ref"] =
                    json!(find(tree, want).with_context(|| format!("no {want} in the snapshot"))?);
            }
            let argv = match with_lab(|l| l.core.plan(conversation, tool_name, &args, &app)) {
                Ok(a) => a,
                Err(r) => return Ok(refusal(r)),
            };
            let argv_ref: Vec<&str> = argv.iter().map(String::as_str).collect();
            let said = call(run, &bin, &argv_ref).await?;
            if said["ok"] != true {
                // The person moved to a full-screen Space (or locked the screen) mid-run: the
                // window is still there but off where agent-desktop reads. Not the code's failure.
                if said["error"]["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("not exposed through accessibility"))
                {
                    return Ok(json!({"skipped": OFF_SCREEN}));
                }
                return Ok(refusal(Refusal::Browser(format!(
                    "agent-desktop {}: {}",
                    argv[0], said["error"]
                ))));
            }
            let data = &said["data"];
            let kind = tool(tool_name).map(|t| t.kind).unwrap_or(Kind::Act);
            let mut result = json!({"app": app.pid, "readOnly": kind.read_only()});
            match argv[0].as_str() {
                "snapshot" => {
                    let mut refs = vec![];
                    refs_in(&data["tree"], &mut refs);
                    with_lab(|l| l.core.snapshot_taken(conversation, app.pid, refs));
                    result["outside"] =
                        outside(&app.name, "accessibility snapshot", data["tree"].clone());
                    result["marksTurnUntrusted"] = json!(true);
                }
                "get" => {
                    result["outside"] = outside(&app.name, "element", data.clone());
                    result["marksTurnUntrusted"] = json!(true);
                }
                "list-windows" => {
                    let mine: Vec<Value> = data
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|w| {
                            w["pid"].as_u64().is_some_and(|p| {
                                with_lab(|l| l.core.granted(conversation, p as u32))
                            })
                        })
                        .cloned()
                        .collect();
                    result["outside"] = outside("the desktop", "windows", json!(mine));
                    result["marksTurnUntrusted"] = json!(true);
                }
                _ => {
                    result["delivery"] = data["disposition"]["delivery"].clone();
                }
            }
            Ok(json!({"ok": true, "result": result}))
        }
        other => bail!("{other} isn't a desktop node"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(pid: u32) -> App {
        App {
            pid,
            window: "w-1".into(),
            name: "oby-lab-desktop".into(),
            name_unique: true,
        }
    }

    #[test]
    fn lab_desktop_1_every_tool_says_whether_it_only_reads() {
        let reads: Vec<&str> = TOOLS
            .iter()
            .filter(|t| t.kind.read_only())
            .map(|t| t.name)
            .collect();
        assert_eq!(
            reads,
            [
                "oby_desktop_list_windows",
                "oby_desktop_snapshot",
                "oby_desktop_read"
            ]
        );
        for t in TOOLS {
            assert!(t.name.starts_with("oby_desktop_"));
            assert!(matches!(t.kind, Kind::Read | Kind::Act), "{}", t.name);
        }
    }

    #[test]
    fn lab_desktop_4_the_persons_hold_refuses_acts_but_not_reads() {
        let mut d = Desktop::new(Rules::ALL);
        d.grant("c1", 7);
        d.snapshot_taken("c1", 7, ["@s:e1".to_string()]);
        let click = json!({"ref": "@s:e1"});
        assert_eq!(
            d.plan("c1", "oby_desktop_click", &click, &app(7)).unwrap(),
            ["click", "@s:e1"]
        );
        d.take("owner").unwrap();
        assert_eq!(
            d.plan("c1", "oby_desktop_click", &click, &app(7))
                .unwrap_err()
                .code(),
            "human_control"
        );
        assert_eq!(
            d.plan(
                "c1",
                "oby_desktop_press",
                &json!({"keys": "cmd+k"}),
                &app(7)
            )
            .unwrap_err()
            .code(),
            "human_control"
        );
        assert!(d
            .plan("c1", "oby_desktop_snapshot", &json!({}), &app(7))
            .is_ok());
        assert_eq!(d.take("guest").unwrap_err().code(), "held_by_other");
        assert_eq!(d.release("guest").unwrap_err().code(), "not_holder");
        d.release("owner").unwrap();
        // The handover killed the refs.
        assert_eq!(
            d.plan("c1", "oby_desktop_click", &click, &app(7))
                .unwrap_err()
                .code(),
            "unknown_ref"
        );
    }

    #[test]
    fn lab_desktop_5_a_conversation_drives_only_apps_granted_to_it() {
        let mut d = Desktop::new(Rules::ALL);
        d.grant("c1", 7);
        assert_eq!(
            d.plan("c2", "oby_desktop_snapshot", &json!({}), &app(7))
                .unwrap_err()
                .code(),
            "not_granted"
        );
        assert_eq!(
            d.plan("c1", "oby_desktop_snapshot", &json!({}), &app(8))
                .unwrap_err()
                .code(),
            "not_granted"
        );
        d.snapshot_taken("c1", 7, ["@s:e1".to_string()]);
        // A ref from another app's snapshot isn't this app's.
        let mut d2 = d.clone();
        d2.grant("c1", 8);
        assert_eq!(
            d2.plan("c1", "oby_desktop_click", &json!({"ref": "@s:e1"}), &app(8))
                .unwrap_err()
                .code(),
            "unknown_ref"
        );
        let mut shared = app(7);
        shared.name_unique = false;
        assert_eq!(
            d.plan(
                "c1",
                "oby_desktop_press",
                &json!({"keys": "cmd+k"}),
                &shared
            )
            .unwrap_err()
            .code(),
            "bad_args"
        );
    }
}
