//! The `app.*` nodes (docs/contracts/obid-spec.md, OBIDSPEC-13): the real Electron app, driven
//! with Playwright through `apps/electron/scripts/drive-app.mjs`, a helper beside the human drive's
//! launcher (`drive-lib.mjs`). `app.launch` starts one helper per app and keeps it for the
//! workflow; every later step sends it one JSON line and reads one back. These nodes only
//! orchestrate: what an element is and where it sits, the helper finds out.
//!
//! The app runs on the workflow's own scratch obid (its port and home as `OBID_PORT` and
//! `OBID_HOME`, which only a development build reads), or on its demo data with no obid at all, a
//! port nothing listens on: never the owner's on 127.0.0.1:7777, their Oby.app or their app folder.

use crate::obid::{private_dir, Run};
use crate::procs;
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

/// What a step may take before the helper's answer is given up on.
const ANSWER: Duration = Duration::from_secs(120);

struct Helper {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    pid: u32,
}

type Helpers = HashMap<String, Arc<Mutex<Helper>>>;
static HELPERS: std::sync::Mutex<Option<Helpers>> = std::sync::Mutex::new(None);

/// Why an `app.*` workflow can't run on this machine, or None. The real app is driven on macOS:
/// GitHub's Linux runners have no display for Electron, and the merge queue's obid-spec job
/// installs no app.
pub fn unsupported() -> Option<String> {
    (!cfg!(target_os = "macos")).then(|| {
        format!(
            "skipped: the real app is driven on macOS only, and this is {}",
            std::env::consts::OS
        )
    })
}

/// The app folder a step drives: `OBID_SPEC_APP` (another checkout's `apps/electron`, built, to
/// see a workflow fail on the code before its fix), else this checkout's.
pub fn app_dir(run: &Run) -> PathBuf {
    std::env::var_os("OBID_SPEC_APP")
        .map(PathBuf::from)
        .unwrap_or_else(|| run.root.join("apps/electron"))
}

/// The helper is always this checkout's, whichever build it launches.
fn helper_script(run: &Run) -> PathBuf {
    run.root.join("apps/electron/scripts/drive-app.mjs")
}

fn name_of(input: &Value) -> Result<String> {
    // `app`: the launch step's output, or its name
    let name = match &input["app"] {
        Value::Object(o) => o.get("app").and_then(Value::as_str).map(str::to_string),
        Value::String(s) => Some(s.clone()),
        Value::Null => Some("app".to_string()),
        _ => None,
    }
    .context("app: the app.launch step's output (${state.<its id>}) or its name")?;
    anyhow::ensure!(
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "an app's name is letters, digits and dashes"
    );
    Ok(name)
}

fn helper(name: &str) -> Result<Arc<Mutex<Helper>>> {
    HELPERS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|h| h.get(name).cloned())
        .with_context(|| format!("no app named {name}: an app.launch step first"))
}

async fn ask(h: &Arc<Mutex<Helper>>, request: &Value) -> Result<Value> {
    let mut h = h.lock().await;
    let line = format!("{request}\n");
    h.stdin.write_all(line.as_bytes()).await?;
    h.stdin.flush().await?;
    let mut answer = String::new();
    let read = tokio::time::timeout(ANSWER, h.stdout.read_line(&mut answer))
        .await
        .with_context(|| {
            format!(
                "the app's helper answered nothing within {} s",
                ANSWER.as_secs()
            )
        })??;
    anyhow::ensure!(read > 0, "the app's helper ended (pid {})", h.pid);
    let said: Value = serde_json::from_str(answer.trim())
        .with_context(|| format!("the helper's answer: {}", answer.trim()))?;
    if said["ok"] == true {
        Ok(said["output"].clone())
    } else {
        bail!("{}", said["error"].as_str().unwrap_or("the helper failed"))
    }
}

/// A free port on 127.0.0.1 nothing listens on, for an app on its demo data: it never finds an
/// obid, the owner's least of all.
fn idle_port() -> Result<u16> {
    let l = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(l.local_addr()?.port())
}

pub async fn launch(run: &Run, input: &Value) -> Result<Value> {
    if let Some(why) = unsupported() {
        return Ok(json!({ "skipped": why }));
    }
    let name = input["name"].as_str().unwrap_or("app").to_string();
    anyhow::ensure!(
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "an app's name is letters, digits and dashes"
    );
    anyhow::ensure!(
        helper(&name).is_err(),
        "an app named {name} is running already"
    );
    let folder = run.scratch.join(format!("app-{name}"));
    private_dir(&folder)?;
    let mock = input["mock"] == true;
    // Its obid: the workflow's scratch one, or (on the demo data) a port nothing listens on.
    let (port, home) = match (&input["obid"], mock) {
        (Value::Object(o), _) => (
            o.get("port").and_then(Value::as_u64).context("obid.port")? as u16,
            PathBuf::from(o.get("home").and_then(Value::as_str).context("obid.home")?),
        ),
        (Value::Null, true) => {
            let home = folder.join("no-obid");
            private_dir(&home)?;
            (idle_port()?, home)
        }
        _ => bail!(
            "obid: the obid-start step's output, or mock: true for the app's demo data with no obid"
        ),
    };
    anyhow::ensure!(
        port != 7777,
        "7777 is the owner's obid: a workflow runs its own"
    );
    anyhow::ensure!(
        home.starts_with(&run.scratch),
        "obid's home {} isn't in this workflow's folder",
        home.display()
    );
    let dir = app_dir(run);
    let main = dir.join("out/main/index.js");
    anyhow::ensure!(
        main.is_file(),
        "{} isn't built: npm run build in {}",
        main.display(),
        dir.display()
    );
    let mut command = tokio::process::Command::new("node");
    command
        .arg(helper_script(run))
        .current_dir(run.root.join("apps/electron"))
        // the person's environment, as the human drive's launch gives it, but with no address of a
        // dev server or an obid but this workflow's (the helper sets OBID_PORT and OBID_HOME)
        .env_remove("ELECTRON_RENDERER_URL")
        .env_remove("OBID_PORT")
        .env_remove("OBID_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(std::fs::File::create(
            folder.join("helper.log"),
        )?))
        .kill_on_drop(true)
        .process_group(0);
    let mut child = command
        .spawn()
        .context("node didn't start the app's helper")?;
    let pid = child.id().context("the helper ended at once")?;
    procs::remember(pid, &format!("app {name}'s helper"));
    let h = Arc::new(Mutex::new(Helper {
        stdin: child.stdin.take().context("no stdin")?,
        stdout: BufReader::new(child.stdout.take().context("no stdout")?),
        pid,
    }));
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    HELPERS
        .lock()
        .unwrap()
        .get_or_insert_with(Default::default)
        .insert(name.clone(), h.clone());
    let mut request = json!({
        "op": "launch", "scratch": folder, "userData": folder.join("user-data"),
        "obid": {"port": port, "home": home}, "mock": mock, "main": main,
    });
    for k in ["skipFirstRun", "focusable", "size"] {
        if let Some(v) = input.get(k) {
            request[k] = v.clone();
        }
    }
    let launched = ask(&h, &request).await?;
    // Electron runs in its own process group (Playwright's launch): stopped by its PID too.
    let electron = launched["pid"].as_u64().context("the app's pid")? as u32;
    procs::remember(electron, &format!("app {name}"));
    Ok(json!({
        "app": name, "pid": electron, "helperPid": pid, "title": launched["title"],
        "folder": folder, "mock": mock, "obidPort": port, "build": dir,
    }))
}

/// A step's request to the helper, with the target and the like passed through.
fn request(op: &str, input: &Value, keys: &[&str]) -> Value {
    let mut r = json!({ "op": op });
    for k in keys {
        if let Some(v) = input.get(*k) {
            r[*k] = v.clone();
        }
    }
    r
}

pub async fn act(run: &Run, node: &str, input: &Value) -> Result<Value> {
    if let Some(why) = unsupported() {
        return Ok(json!({ "skipped": why }));
    }
    let name = name_of(input)?;
    let h = helper(&name)?;
    match node {
        "app.click" => {
            ask(
                &h,
                &request("click", input, &["target", "hover", "timeout"]),
            )
            .await
        }
        "app.type" => ask(&h, &request("type", input, &["target", "text", "timeout"])).await,
        "app.press" => ask(&h, &request("press", input, &["target", "keys", "timeout"])).await,
        "app.read" | "app.measure" => {
            let op = node.trim_start_matches("app.");
            let keys = ["target", "attributes", "against", "max", "timeout"];
            looked(&h, &request(op, input, &keys), input).await
        }
        "app.screenshot" => {
            let file = input["name"].as_str().unwrap_or("screen");
            anyhow::ensure!(
                file.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "name: letters, digits, dashes"
            );
            let path = run
                .scratch
                .join("screens")
                .join(format!("{name}-{file}.png"));
            let out = ask(&h, &json!({"op": "screenshot", "path": path})).await?;
            // OBID_SPEC_SHOTS: a folder the screenshots are copied to, kept when the run passes
            if let Some(dir) = std::env::var_os("OBID_SPEC_SHOTS") {
                let dir = Path::new(&dir);
                std::fs::create_dir_all(dir)?;
                std::fs::copy(&path, dir.join(path.file_name().unwrap()))?;
            }
            Ok(out)
        }
        "app.close" => {
            let out = ask(&h, &json!({"op": "close"})).await?;
            let removed = HELPERS
                .lock()
                .unwrap()
                .as_mut()
                .and_then(|m| m.remove(&name));
            if let Some(h) = removed {
                let pid = h.lock().await.pid;
                procs::stop(pid, Duration::from_secs(5)).await;
            }
            if let Some(pid) = out["closed"].as_u64() {
                let pid = pid as u32;
                let until = Instant::now() + Duration::from_secs(10);
                while procs::alive(pid) && Instant::now() < until {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                if procs::ours(pid) {
                    procs::stop(pid, Duration::from_secs(5)).await;
                }
            }
            Ok(out)
        }
        other => bail!("no node named {other}"),
    }
}

/// A read or a measure, judged by the step's `expect`: `within` seconds asks again until it holds
/// (an element that arrives a moment later); `hold` milliseconds asks once more after that long and
/// it must hold still (a toggle that flips back, a box that moves). The proof that the expectation
/// can fail is made by the node's caller (nodes::red_proof).
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

/// The app a desktop step acts on: its Electron PID, from the app.launch step's output.
pub fn pid_of(input: &Value) -> Result<u32> {
    let app = &input["app"];
    if let Some(pid) = app["pid"].as_u64() {
        return Ok(pid as u32);
    }
    let name = name_of(input)?;
    bail!("app: the app.launch step's output (${{state.<its id>}}), not only its name ({name})")
}
