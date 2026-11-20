//! The lab's `ui` feature (src/lab/mod.rs): the browser's screens (lane B of the browser plan,
//! https://claude.ai/artifact/TVVtw8kq77vnm81B5UfcXq), a Vite + React app in `lab-ui/browser/` on
//! the real app's own styles, driven in Chrome for Testing with Playwright the way `app.*` drives
//! the real app (src/app.rs): `lab.ui.serve` builds the app into the workflow's folder and serves it
//! on 127.0.0.1 on a free port; `lab.ui.open` starts one helper (`lab-ui/browser/scripts/
//! drive-lab.mjs`) per screen and keeps it; every later step sends it one JSON line and reads one
//! back. These nodes only orchestrate: what an element is and where it sits, the helper finds out.
//!
//! Nothing here touches the person's Chrome, its profile, their screen, their obid or Oby.app: the
//! browser is Chrome for Testing from obid-spec's own cache, headless, its profile and temporary
//! files in the workflow's folder, and every picture is of the page the helper opened. Owned by the
//! session building the screens; nothing else edits this file.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::obid::{private_dir, Run};
use crate::procs;

/// Every `lab.ui.*` node: its name, what it does.
pub const NODES: &[(&str, &str)] = &[
    ("lab.ui.serve", "Builds the browser's screens (tools/obid-spec/lab-ui/browser) into the workflow's folder and serves them on 127.0.0.1 on a free port, first installing their packages from the lockfile (npm ci) when missing or stale; `api`: lane A's browser API, proxied for live mode; `slowReadsMs` holds each answer to GET /v1/browser and /v1/browser/downloads that long (a busy main process). Skipped only without npm."),
    ("lab.ui.open", "Opens a board's screen (`board`: 1 to 17, `live` for lane A's API) in Chrome for Testing, headless, at the board's own size, with a profile in the workflow's folder; one helper per `name`. Skipped without Chrome for Testing."),
    ("lab.ui.click", "Clicks (or `hover`s) one element of the screen, at its middle or at `position` {x, y} in it: a target by role and name, label, text or css."),
    ("lab.ui.type", "Types `text` into the screen (into `target` first, when given); answers only how many characters."),
    ("lab.ui.press", "Presses `keys` (`times` times) on the screen."),
    ("lab.ui.read", "Reads the elements a target finds: their count, words, attributes and computed `styles` (a style's value against a token: `{\"background-color\": \"--held\"}`); with expect, `within` and `hold`."),
    ("lab.ui.measure", "Measures one element's box, and against others (`against`) whether they overlap and the gap; with expect, `within` and `hold`."),
    ("lab.ui.compare", "Compares the screen with its design board (design/boards/<board>.dc.html) box by box and word by word: every browser piece within `tolerance` px (4 by default); today's app pieces reported."),
    ("lab.ui.screenshot", "Saves a picture of the screen (only the page the helper opened) into the workflow's folder, copied to OBID_SPEC_SHOTS when it's set."),
    ("lab.ui.sidebyside", "Saves the screen beside its design board as one PNG into the workflow's folder (and OBID_SPEC_SHOTS), for the owner's look."),
    ("lab.ui.reload", "Reloads the screen's page (the screens come back to the same state from the API in live mode)."),
    ("lab.ui.close", "Closes a screen's browser and its helper."),
    ("lab.ui.processes", "The processes this feature started that are still alive (servers, helpers, Chrome), by PID and by the workflow's folder in their command line; with expect."),
    ("lab.ui.down", "Stops everything this feature started, as the suite's cleanup does, mid-workflow (to prove the cleanup)."),
    ("lab.ui.meanwhile", "Starts a lab.browserapi step (`node`, its `inputs`) and answers at once with its handle, so the screen can be read while it runs (an agent's act the screen shows as it happens)."),
    ("lab.ui.wait", "Waits for a lab.ui.meanwhile step's answer (`meanwhile`: its output), at most `seconds`; with expect."),
];

type Pending = HashMap<String, tokio::task::JoinHandle<std::result::Result<Value, String>>>;
static PENDING: std::sync::Mutex<Option<Pending>> = std::sync::Mutex::new(None);

/// What a step may take before the helper's answer is given up on.
const ANSWER: Duration = Duration::from_secs(90);

struct Helper {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    pid: u32,
    chrome: Option<u32>,
}

#[derive(Default)]
struct Ui {
    helpers: HashMap<String, Arc<Mutex<Helper>>>,
    /// the servers lab.ui.serve started: PID → its URL
    servers: Vec<(u32, String)>,
    /// every PID this feature started, still looked for at cleanup
    pids: Vec<(u32, String)>,
    scratch: Option<PathBuf>,
}

static UI: std::sync::Mutex<Option<Ui>> = std::sync::Mutex::new(None);

fn with_ui<T>(f: impl FnOnce(&mut Ui) -> T) -> T {
    let mut g = UI.lock().unwrap();
    f(g.get_or_insert_with(Default::default))
}

fn started(pid: u32, what: &str) {
    procs::remember(pid, what);
    with_ui(|u| u.pids.push((pid, what.to_string())));
}

/// The screens' app folder: `OBID_SPEC_LAB_UI` (another checkout's, to see a workflow fail on the
/// code before a fix), else this checkout's.
fn app_dir(run: &Run) -> PathBuf {
    std::env::var_os("OBID_SPEC_LAB_UI")
        .map(PathBuf::from)
        .unwrap_or_else(|| run.root.join("tools/obid-spec/lab-ui/browser"))
}

/// Chrome for Testing: `OBID_SPEC_CHROME`, else the one obid-spec's lab.browser nodes keep in their
/// cache (agent-browser's install). Never the person's Chrome.
fn chrome() -> std::result::Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("OBID_SPEC_CHROME") {
        let p = PathBuf::from(p);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(format!(
                "skipped: OBID_SPEC_CHROME ({}) isn't a file",
                p.display()
            ))
        };
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("skipped: no HOME")?;
    let cache = if cfg!(target_os = "macos") {
        home.join("Library/Caches/obid-spec/agent-browser-chrome/.agent-browser/browsers")
    } else {
        home.join(".cache/obid-spec/agent-browser-chrome/.agent-browser/browsers")
    };
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&cache)
        .map_err(|_| format!("skipped: Chrome for Testing isn't in obid-spec's cache ({}); a lab.browser workflow installs it", cache.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("chrome-")))
        .collect();
    dirs.sort();
    let dir = dirs
        .pop()
        .ok_or_else(|| format!("skipped: no Chrome for Testing in {}", cache.display()))?;
    let exe = if cfg!(target_os = "macos") {
        dir.join("Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing")
    } else {
        dir.join("chrome-linux64/chrome")
    };
    if exe.is_file() {
        Ok(exe)
    } else {
        Err(format!(
            "skipped: Chrome for Testing isn't at {}",
            exe.display()
        ))
    }
}

fn node() -> std::result::Result<PathBuf, String> {
    crate::obid::which("node").ok_or_else(|| "skipped: node isn't installed".to_string())
}

fn free_port() -> Result<u16> {
    let l = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(l.local_addr()?.port())
}

fn name_of(input: &Value) -> Result<String> {
    let name = match &input["screen"] {
        Value::Object(o) => o.get("screen").and_then(Value::as_str).map(str::to_string),
        Value::String(s) => Some(s.clone()),
        Value::Null => Some("screen".to_string()),
        _ => None,
    }
    .context("screen: the lab.ui.open step's output (${state.<its id>}) or its name")?;
    anyhow::ensure!(
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "a screen's name is letters, digits and dashes"
    );
    Ok(name)
}

fn helper(name: &str) -> Result<Arc<Mutex<Helper>>> {
    with_ui(|u| u.helpers.get(name).cloned())
        .with_context(|| format!("no screen named {name}: a lab.ui.open step first"))
}

async fn ask(h: &Arc<Mutex<Helper>>, request: &Value) -> Result<Value> {
    let mut h = h.lock().await;
    h.stdin.write_all(format!("{request}\n").as_bytes()).await?;
    h.stdin.flush().await?;
    let mut answer = String::new();
    let read = tokio::time::timeout(ANSWER, h.stdout.read_line(&mut answer))
        .await
        .with_context(|| {
            format!(
                "the screen's helper answered nothing within {} s",
                ANSWER.as_secs()
            )
        })??;
    anyhow::ensure!(read > 0, "the screen's helper ended (pid {})", h.pid);
    let said: Value = serde_json::from_str(answer.trim())
        .with_context(|| format!("the helper's answer: {}", answer.trim()))?;
    if said["ok"] == true {
        Ok(said["output"].clone())
    } else {
        bail!("{}", said["error"].as_str().unwrap_or("the helper failed"))
    }
}

/// lab.ui.serve: build into the workflow's folder, then `vite preview` on 127.0.0.1.
async fn serve(run: &Run, input: &Value) -> Result<Value> {
    let node = match node() {
        Ok(n) => n,
        Err(why) => return Ok(json!({ "skipped": why })),
    };
    let dir = app_dir(run);
    let folder = run.scratch.join("lab-ui");
    private_dir(&folder)?;
    with_ui(|u| u.scratch = Some(folder.clone()));
    // the screens' packages, from the lockfile, installed here when missing or older than it: a run
    // that skipped every workflow for want of them would prove nothing
    let installed = match install(&dir, &folder).await? {
        Ok(i) => i,
        Err(why) => return Ok(json!({ "skipped": why })),
    };
    let vite = dir.join("node_modules/vite/bin/vite.js");
    anyhow::ensure!(vite.is_file(), "npm ci left no {}", vite.display());
    let out = folder.join(format!("dist-{}", with_ui(|u| u.servers.len())));
    // live mode: lane A's API (the lab.browserapi.up step's output: its URL and owner token) and a
    // device of the owner's (lab.browserapi.device), held by the screens' server, never the page
    let api = input["api"]["url"].as_str().map(str::to_string);
    let token = input["api"]["token"].as_str().map(str::to_string);
    if let Some(a) = &api {
        anyhow::ensure!(
            a.starts_with("http://127.0.0.1:"),
            "api: lane A's server on 127.0.0.1"
        );
        anyhow::ensure!(
            token.is_some(),
            "api.token: the lab.browserapi.up step's output"
        );
    }
    let device = match &input["device"] {
        Value::Null => None,
        d => Some(format!(
            "{} {} {}",
            d["id"].as_str().context("device.id")?,
            d["key"].as_str().context("device.key")?,
            d["publicKey"].as_str().context("device.publicKey")?
        )),
    };
    let log = std::fs::File::create(folder.join("build.log"))?;
    let built = tokio::time::timeout(
        Duration::from_secs(180),
        tokio::process::Command::new(&node)
            .arg(&vite)
            .arg("build")
            .arg("--logLevel")
            .arg("warn")
            .current_dir(&dir)
            .env("LAB_UI_OUT", &out)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .status(),
    )
    .await
    .context("the screens didn't build within 180 s")??;
    anyhow::ensure!(
        built.success(),
        "the screens didn't build: {}",
        folder.join("build.log").display()
    );
    let port = free_port()?;
    let mut cmd = tokio::process::Command::new(&node);
    cmd.arg(&vite)
        .arg("preview")
        .arg("--port")
        .arg(port.to_string())
        .arg("--strictPort")
        .current_dir(&dir)
        .env("LAB_UI_OUT", &out)
        .env_remove("LAB_BROWSER_API")
        .env_remove("LAB_BROWSER_TOKEN")
        .env_remove("LAB_BROWSER_DEVICE")
        .env_remove("LAB_BROWSER_SLOW_READS_MS")
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(
            folder.join(format!("serve-{port}.log")),
        )?))
        .stderr(Stdio::from(std::fs::File::create(
            folder.join(format!("serve-{port}.err")),
        )?))
        .kill_on_drop(true)
        .process_group(0);
    if let (Some(a), Some(t)) = (&api, &token) {
        cmd.env("LAB_BROWSER_API", a).env("LAB_BROWSER_TOKEN", t);
    }
    if let Some(d) = &device {
        cmd.env("LAB_BROWSER_DEVICE", d);
    }
    // A busy main process: the list's answers held this long (LAB-UI-LIVE-6).
    if let Some(ms) = input["slowReadsMs"].as_u64() {
        cmd.env("LAB_BROWSER_SLOW_READS_MS", ms.to_string());
    }
    let mut child = cmd.spawn().context("vite preview didn't start")?;
    let pid = child.id().context("vite preview ended at once")?;
    started(pid, &format!("the screens' server on {port}"));
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    let url = format!("http://127.0.0.1:{port}/");
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            break;
        }
        anyhow::ensure!(
            Instant::now() < until,
            "the screens' server didn't answer on {port} within 30 s"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    with_ui(|u| u.servers.push((pid, url.clone())));
    Ok(
        json!({ "url": url, "port": port, "pid": pid, "live": api.is_some(), "api": api, "signs": device.is_some(), "build": dir, "installed": installed }),
    )
}

/// The file in node_modules naming the lockfile it was installed from (its sha256).
const STAMP: &str = "node_modules/.lab-ui-lockfile.sha256";

/// `npm ci` in the screens' folder (`--no-audit --no-fund`, from package-lock.json) when its
/// node_modules is missing or was installed from another lockfile, under a lock on the folder so
/// workflows running side by side install once. `true` when it installed, `false` when already
/// current; a skip only when npm itself isn't there.
async fn install(dir: &Path, folder: &Path) -> Result<std::result::Result<bool, String>> {
    let lockfile = dir.join("package-lock.json");
    let want = {
        use sha2::Digest;
        let bytes = std::fs::read(&lockfile)
            .with_context(|| format!("{} isn't there", lockfile.display()))?;
        hex::encode(sha2::Sha256::digest(bytes))
    };
    let current = || {
        std::fs::read_to_string(dir.join(STAMP))
            .ok()
            .is_some_and(|s| s.trim() == want)
            && dir.join("node_modules/vite/bin/vite.js").is_file()
    };
    if current() {
        return Ok(Ok(false));
    }
    let Some(npm) = crate::obid::which("npm") else {
        return Ok(Err(format!(
            "skipped: npm isn't installed, and the screens' packages aren't ({})",
            dir.display()
        )));
    };
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("node_modules.lock"))?;
    let lock = tokio::task::spawn_blocking(move || {
        use std::os::fd::AsRawFd;
        // waits while another workflow installs
        let r = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) };
        (r == 0).then_some(lock)
    })
    .await?
    .context("couldn't lock the screens' folder to install its packages")?;
    if current() {
        drop(lock);
        return Ok(Ok(false));
    }
    let log = std::fs::File::create(folder.join("npm-ci.log"))?;
    let status = tokio::time::timeout(
        Duration::from_secs(600),
        tokio::process::Command::new(npm)
            .args(["ci", "--no-audit", "--no-fund"])
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .status(),
    )
    .await
    .context("npm ci didn't finish within 600 s")??;
    anyhow::ensure!(
        status.success(),
        "npm ci failed in {}: {}",
        dir.display(),
        folder.join("npm-ci.log").display()
    );
    std::fs::write(dir.join(STAMP), &want)?;
    drop(lock);
    Ok(Ok(true))
}

/// lab.ui.open: the helper (one per name), then the board's page at the board's size.
async fn open(run: &Run, input: &Value) -> Result<Value> {
    let chrome = match chrome() {
        Ok(c) => c,
        Err(why) => return Ok(json!({ "skipped": why })),
    };
    let node = match node() {
        Ok(n) => n,
        Err(why) => return Ok(json!({ "skipped": why })),
    };
    let base = input["server"]["url"]
        .as_str()
        .context("server: the lab.ui.serve step's output")?;
    anyhow::ensure!(
        base.starts_with("http://127.0.0.1:"),
        "server: on 127.0.0.1"
    );
    let board = input["board"].as_u64().context("board: 1 to 17")?;
    anyhow::ensure!((1..=17).contains(&board), "board: 1 to 17");
    let name = input["name"].as_str().unwrap_or("screen").to_string();
    anyhow::ensure!(
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "a screen's name is letters, digits and dashes"
    );
    let live = input["live"] == true;
    let url = format!("{base}#/{board}{}", if live { "?live" } else { "" });
    let folder = run.scratch.join("lab-ui").join(format!("screen-{name}"));
    private_dir(&folder)?;
    let h = match with_ui(|u| u.helpers.get(&name).cloned()) {
        Some(h) => h,
        None => {
            let tmp = folder.join("tmp");
            private_dir(&tmp)?;
            let dir = app_dir(run);
            let mut command = tokio::process::Command::new(&node);
            command
                .arg(dir.join("scripts/drive-lab.mjs"))
                .current_dir(&dir)
                // Chrome's own temporary profile lands in the workflow's folder
                .env("TMPDIR", &tmp)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::from(std::fs::File::create(
                    folder.join("helper.log"),
                )?))
                .kill_on_drop(true)
                .process_group(0);
            let mut child = command
                .spawn()
                .context("node didn't start the screen's helper")?;
            let pid = child.id().context("the helper ended at once")?;
            started(pid, &format!("screen {name}'s helper"));
            let h = Arc::new(Mutex::new(Helper {
                stdin: child.stdin.take().context("no stdin")?,
                stdout: BufReader::new(child.stdout.take().context("no stdout")?),
                pid,
                chrome: None,
            }));
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            with_ui(|u| u.helpers.insert(name.clone(), h.clone()));
            h
        }
    };
    let mut request = json!({ "op": "open", "chrome": chrome, "url": url });
    if let Some(s) = input.get("size") {
        request["size"] = s.clone();
    }
    let opened = ask(&h, &request).await?;
    // Chrome runs in its own process group (Playwright launches it detached): stopped by its PID.
    if let Some(pid) = opened["pid"].as_u64() {
        let pid = pid as u32;
        let mut g = h.lock().await;
        if g.chrome != Some(pid) {
            g.chrome = Some(pid);
            started(pid, &format!("screen {name}'s Chrome for Testing"));
        }
    }
    Ok(json!({
        "screen": name, "board": board, "live": live, "url": opened["url"], "size": opened["size"],
        "chromePid": opened["pid"], "chrome": chrome, "profileInWorkflow": true,
    }))
}

fn request(op: &str, input: &Value, keys: &[&str]) -> Value {
    let mut r = json!({ "op": op });
    for k in keys {
        if let Some(v) = input.get(*k) {
            r[*k] = v.clone();
        }
    }
    r
}

/// A picture's file in the workflow's folder, copied to OBID_SPEC_SHOTS when that's set.
fn shot_path(run: &Run, name: &str, file: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        file.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "name: letters, digits, dashes"
    );
    Ok(run
        .scratch
        .join("lab-ui/screens")
        .join(format!("{name}-{file}.png")))
}

fn keep(path: &Path) -> Result<Option<PathBuf>> {
    let Some(dir) = std::env::var_os("OBID_SPEC_SHOTS") else {
        return Ok(None);
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir)?;
    let to = dir.join(path.file_name().context("a file name")?);
    std::fs::copy(path, &to)?;
    Ok(Some(to))
}

/// A read or a measure judged by its `expect`: `within` seconds asks again until it holds; `hold`
/// milliseconds asks once more after that long and it must hold still (as app.rs's `looked`).
async fn looked(h: &Arc<Mutex<Helper>>, request: &Value, input: &Value) -> Result<Value> {
    let want = input.get("expect");
    let within = input["within"].as_u64().unwrap_or(0);
    let until = Instant::now() + Duration::from_secs(within);
    loop {
        let got = ask(h, request).await;
        let failures = match (&got, want) {
            (Ok(v), Some(w)) => crate::expect::check(w, v, ""),
            (Ok(_), None) => vec![],
            (Err(e), _) => vec![format!("{e:#}")],
        };
        if failures.is_empty() {
            let got = got?;
            if let (Some(ms), Some(w)) = (input["hold"].as_u64(), want) {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                let again = ask(h, request).await?;
                let changed = crate::expect::check(w, &again, "");
                anyhow::ensure!(
                    changed.is_empty(),
                    "it held, then {ms} ms later didn't: {}",
                    changed.join("; ")
                );
            }
            return Ok(got);
        }
        if Instant::now() >= until {
            bail!(
                "{}{}",
                failures.join("; "),
                if within > 0 {
                    format!(" (still, after {within} s)")
                } else {
                    String::new()
                }
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn close(name: &str) -> Result<Value> {
    let h = helper(name)?;
    let out = ask(&h, &json!({"op": "close"}))
        .await
        .unwrap_or_else(|e| json!({"error": format!("{e:#}")}));
    with_ui(|u| u.helpers.remove(name));
    let (pid, chrome) = {
        let g = h.lock().await;
        (g.pid, g.chrome)
    };
    procs::stop(pid, Duration::from_secs(5)).await;
    if let Some(c) = chrome {
        procs::stop(c, Duration::from_secs(5)).await;
    }
    Ok(json!({ "closed": name, "helper": pid, "chrome": chrome, "pageErrors": out["pageErrors"] }))
}

/// The processes still alive that this feature started: by PID, and by the workflow's folder in
/// their command line (what a lost PID still names).
fn ours_alive() -> Vec<Value> {
    let (pids, scratch) = with_ui(|u| (u.pids.clone(), u.scratch.clone()));
    let mut found: Vec<Value> = pids
        .iter()
        .filter(|(p, _)| procs::alive(*p))
        .map(|(p, what)| json!({"pid": p, "what": what}))
        .collect();
    if let Some(folder) = scratch {
        let needle = folder.to_string_lossy().to_string();
        if let Ok(out) = std::process::Command::new("ps")
            .args(["-axo", "pid=,command="])
            .output()
        {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let line = line.trim_start();
                let (pid, cmd) = line.split_once(' ').unwrap_or((line, ""));
                let Ok(pid) = pid.parse::<u32>() else {
                    continue;
                };
                if cmd.contains(&needle)
                    && pid != std::process::id()
                    && !found.iter().any(|f| f["pid"] == pid)
                {
                    let what = if cmd.contains("Chrome") || cmd.contains("chrome") {
                        "Chrome for Testing"
                    } else if cmd.contains("vite") {
                        "the screens' server"
                    } else {
                        "a lab-ui process"
                    };
                    found.push(json!({"pid": pid, "what": what}));
                }
            }
        }
    }
    found
}

pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    match name {
        "lab.ui.serve" => serve(run, input).await,
        "lab.ui.open" => open(run, input).await,
        "lab.ui.processes" => {
            let found = ours_alive();
            let count = |w: &str| {
                found
                    .iter()
                    .filter(|p| p["what"].as_str().is_some_and(|s| s.contains(w)))
                    .count()
            };
            Ok(
                json!({ "alive": found.len(), "any": !found.is_empty(), "servers": count("server"), "helpers": count("helper"), "chromes": count("Chrome"), "processes": found }),
            )
        }
        "lab.ui.down" => Ok(json!({ "stopped": cleanup().await })),
        "lab.ui.meanwhile" => {
            let node = input["node"]
                .as_str()
                .context("node: a lab step's node name")?
                .to_string();
            anyhow::ensure!(
                node.starts_with("lab.browserapi."),
                "node: a lab.browserapi step (an agent's act through lane A's API)"
            );
            let inputs = input["inputs"].clone();
            let run = run.clone();
            let handle = format!(
                "meanwhile-{}",
                PENDING
                    .lock()
                    .unwrap()
                    .get_or_insert_with(Default::default)
                    .len()
            );
            let task = tokio::spawn(async move {
                crate::lab::browserapi::run_node(&run, &node, &inputs)
                    .await
                    .map_err(|e| format!("{e:#}"))
            });
            PENDING
                .lock()
                .unwrap()
                .get_or_insert_with(Default::default)
                .insert(handle.clone(), task);
            Ok(json!({ "handle": handle }))
        }
        "lab.ui.wait" => {
            let handle = input["meanwhile"]["handle"]
                .as_str()
                .context("meanwhile: the lab.ui.meanwhile step's output")?;
            let task = PENDING
                .lock()
                .unwrap()
                .as_mut()
                .and_then(|p| p.remove(handle))
                .context("no such step running (or it was waited for already)")?;
            let seconds = input["seconds"].as_u64().unwrap_or(60);
            tokio::time::timeout(Duration::from_secs(seconds), task)
                .await
                .with_context(|| format!("the step answered nothing within {seconds} s"))??
                .map_err(|e| anyhow::anyhow!(e))
        }
        _ => {
            let screen = name_of(input)?;
            let h = helper(&screen)?;
            match name {
                "lab.ui.click" => {
                    ask(
                        &h,
                        &request("click", input, &["target", "hover", "position", "timeout"]),
                    )
                    .await
                }
                "lab.ui.type" => {
                    ask(&h, &request("type", input, &["target", "text", "timeout"])).await
                }
                "lab.ui.press" => {
                    ask(
                        &h,
                        &request("press", input, &["target", "keys", "times", "timeout"]),
                    )
                    .await
                }
                "lab.ui.read" => {
                    looked(
                        &h,
                        &request(
                            "read",
                            input,
                            &["target", "attributes", "styles", "max", "timeout"],
                        ),
                        input,
                    )
                    .await
                }
                "lab.ui.measure" => {
                    looked(
                        &h,
                        &request("measure", input, &["target", "against", "timeout"]),
                        input,
                    )
                    .await
                }
                "lab.ui.compare" => {
                    ask(
                        &h,
                        &request("compare", input, &["board", "state", "tolerance", "skip"]),
                    )
                    .await
                }
                "lab.ui.reload" => ask(&h, &json!({"op": "reload"})).await,
                "lab.ui.screenshot" => {
                    let path = shot_path(run, &screen, input["name"].as_str().unwrap_or("screen"))?;
                    let mut out = ask(&h, &json!({"op": "screenshot", "path": path})).await?;
                    out["kept"] = json!(keep(&path)?);
                    Ok(out)
                }
                "lab.ui.sidebyside" => {
                    let path = shot_path(
                        run,
                        &screen,
                        input["name"].as_str().unwrap_or("beside-board"),
                    )?;
                    let mut r = request("sidebyside", input, &["board", "state", "label"]);
                    r["path"] = json!(path);
                    let mut out = ask(&h, &r).await?;
                    out["kept"] = json!(keep(&path)?);
                    Ok(out)
                }
                "lab.ui.close" => close(&screen).await,
                other => bail!("{other} isn't a lab.ui node"),
            }
        }
    }
}

/// What this feature started for the workflow and must stop at its end, whatever it ended in: each
/// screen's browser and helper, then the servers, then anything still naming the workflow's folder.
pub async fn cleanup() -> Value {
    let names: Vec<String> = with_ui(|u| u.helpers.keys().cloned().collect());
    let mut closed = vec![];
    for n in names {
        closed.push(
            close(&n)
                .await
                .unwrap_or_else(|e| json!({"screen": n, "error": format!("{e:#}")})),
        );
    }
    let servers: Vec<(u32, String)> = with_ui(|u| std::mem::take(&mut u.servers));
    let mut stopped = vec![];
    for (pid, url) in servers {
        procs::stop(pid, Duration::from_secs(5)).await;
        stopped.push(json!({"server": url, "pid": pid}));
    }
    // a PID a stop missed, or a process a lost helper left: by the workflow's folder
    let mut strays = vec![];
    for p in ours_alive() {
        if let Some(pid) = p["pid"].as_u64() {
            procs::stop(pid as u32, Duration::from_secs(5)).await;
            strays.push(p);
        }
    }
    if closed.is_empty() && stopped.is_empty() && strays.is_empty() {
        return Value::Null;
    }
    json!({ "screens": closed, "servers": stopped, "strays": strays })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_node_is_lab_ui() {
        for (n, _) in NODES {
            assert!(n.starts_with("lab.ui."), "{n}");
        }
    }

    #[test]
    fn a_screen_is_named_by_its_open_step_or_a_name() {
        assert_eq!(name_of(&json!({"screen": {"screen": "s1"}})).unwrap(), "s1");
        assert_eq!(name_of(&json!({"screen": "s2"})).unwrap(), "s2");
        assert_eq!(name_of(&json!({})).unwrap(), "screen");
        assert!(name_of(&json!({"screen": "../x"})).is_err());
    }
}
