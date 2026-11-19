//! The `desktop.*` nodes (docs/contracts/obid-spec.md, OBIDSPEC-14): what is outside the page, the
//! macOS menu bar and the keys it takes, read and driven the way a person sees them, through
//! agent-desktop's CLI (vercel-labs/agent-desktop, the accessibility tree).
//!
//! agent-desktop is installed into the workflow's own folder (`npm install --prefix`, never
//! globally), or named by `OBID_SPEC_AGENT_DESKTOP`. It reads the screen through macOS's
//! Accessibility and Screen Recording permissions, granted once by the owner to the terminal that
//! runs the suite. These nodes never ask for, grant or change one: without them, or off macOS, or
//! with agent-desktop absent, a node answers `skipped` with why, and the host marks the workflow
//! skipped, never passed (OBIDSPEC-15).
//!
//! A step that runs one of the app's menu items brings the app's window forward for that moment
//! (an item's click goes to the focused window, as a person's does), then gives the front back to
//! the app that had it. The window stays invisible all along (drive-lib.mjs's hidden launch).

use crate::obid::{private_dir, Run};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The agent-desktop the suite was written against.
pub const VERSION: &str = "1.3.1";

pub const NO_PERMISSION: &str =
    "skipped: agent-desktop has no Accessibility/Screen Recording permission on this Mac";

/// Why desktop steps can't run on this OS, or None.
pub fn unsupported(os: &str) -> Option<String> {
    (os != "macos").then(|| {
        format!(
            "skipped: desktop steps drive macOS (agent-desktop's macOS adapter), and this is {os}"
        )
    })
}

/// What `agent-desktop permissions` said, judged: None when both are granted, else the skip.
pub fn permission_skip(said: &Value) -> Option<String> {
    let granted = |k: &str| said["data"][k]["state"] == "granted";
    (!(granted("accessibility") && granted("screen_recording"))).then(|| {
        format!(
            "{NO_PERMISSION} (accessibility {}, screen recording {})",
            said["data"]["accessibility"]["state"], said["data"]["screen_recording"]["state"]
        )
    })
}

/// agent-desktop's binary for this workflow, or why there is none (a skip).
async fn binary(run: &Run) -> std::result::Result<PathBuf, String> {
    if let Some(given) = std::env::var_os("OBID_SPEC_AGENT_DESKTOP") {
        let given = PathBuf::from(given);
        return if given.is_file() {
            Ok(given)
        } else {
            Err(format!(
                "skipped: agent-desktop isn't installed (OBID_SPEC_AGENT_DESKTOP names {}, which isn't there)",
                given.display()
            ))
        };
    }
    let prefix = run.scratch.join("agent-desktop");
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    let bin = prefix.join(format!(
        "node_modules/agent-desktop/bin/agent-desktop-darwin-{arch}"
    ));
    if bin.is_file() {
        return Ok(bin);
    }
    private_dir(&prefix).map_err(|e| format!("{e:#}"))?;
    // The package carries its binaries; its install script only picks one, which is done here.
    let out = tokio::process::Command::new("npm")
        .args(["install", "--prefix"])
        .arg(&prefix)
        .arg(format!("agent-desktop@{VERSION}"))
        .args([
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--loglevel=error",
        ])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() && bin.is_file() => Ok(bin),
        Ok(o) => Err(format!(
            "skipped: agent-desktop isn't installed (npm install --prefix {} agent-desktop@{VERSION}: {})",
            prefix.display(),
            String::from_utf8_lossy(&o.stderr).trim().chars().take(300).collect::<String>()
        )),
        Err(e) => Err(format!(
            "skipped: agent-desktop isn't installed (npm didn't start: {e})"
        )),
    }
}

/// One agent-desktop call: its JSON envelope (`{"ok": true, "data": …}` or an error). Its state
/// folder is the workflow's, not the person's `~/.agent-desktop`.
async fn call(run: &Run, bin: &Path, args: &[&str]) -> Result<Value> {
    let home = run.scratch.join("agent-desktop/home");
    private_dir(&home)?;
    let out = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::process::Command::new(bin)
            .args(args)
            .env("HOME", &home)
            .output(),
    )
    .await
    .with_context(|| format!("agent-desktop {} took over 60 s", args.join(" ")))??;
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).with_context(|| {
        format!(
            "agent-desktop {}: {} {}",
            args.join(" "),
            text.trim().chars().take(300).collect::<String>(),
            String::from_utf8_lossy(&out.stderr)
                .trim()
                .chars()
                .take(300)
                .collect::<String>()
        )
    })
}

fn data(said: Value, what: &str) -> Result<Value> {
    if said["ok"] == true {
        Ok(said["data"].clone())
    } else {
        bail!("agent-desktop {what}: {}", said["error"])
    }
}

/// Ready to drive: the binary, and the permissions granted, or the skip that says why not.
async fn ready(run: &Run) -> Result<std::result::Result<PathBuf, String>> {
    if let Some(why) = unsupported(std::env::consts::OS) {
        return Ok(Err(why));
    }
    let bin = match binary(run).await {
        Ok(b) => b,
        Err(why) => return Ok(Err(why)),
    };
    let said = call(run, &bin, &["permissions"]).await?;
    Ok(match permission_skip(&said) {
        Some(why) => Err(why),
        None => Ok(bin),
    })
}

/// The app's window, by its Electron PID (another Electron app may be running: never by name).
async fn window_of(run: &Run, bin: &Path, pid: u32) -> Result<String> {
    let until = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let windows = data(call(run, bin, &["list-windows"]).await?, "list-windows")?;
        // Its main window: the largest of its windows exposed through accessibility (a tooltip or
        // a frame of its own may come and go beside it)
        let area = |w: &&Value| {
            let b = &w["bounds"];
            (b["width"].as_f64().unwrap_or(0.0) * b["height"].as_f64().unwrap_or(0.0)) as u64
        };
        let found = windows
            .as_array()
            .into_iter()
            .flatten()
            .filter(|w| w["pid"] == pid && w["accessible"] == true)
            .max_by_key(area)
            .and_then(|w| w["id"].as_str())
            .map(str::to_string);
        if let Some(id) = found {
            return Ok(id);
        }
        if std::time::Instant::now() >= until {
            bail!("agent-desktop lists no window of the app (pid {pid})");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// A surface's tree cut to what a person reads: each element's role, name and states, its
/// children, and the ref a later action takes.
fn plain(node: &Value) -> Value {
    let mut out = json!({});
    for k in ["role", "name", "value", "states", "ref_id"] {
        if let Some(v) = node.get(k) {
            out[k] = v.clone();
        }
    }
    let children: Vec<Value> = node["children"]
        .as_array()
        .into_iter()
        .flatten()
        .map(plain)
        .collect();
    if !children.is_empty() {
        out["children"] = json!(children);
    }
    out
}

/// The menu bar as a person reads it: each menu's title and the items in it, by name, in order,
/// separators left out (`{"View": ["Toggle Sidebar", "Actual Size", …]}`).
pub fn menus(tree: &Value) -> Value {
    fn items(node: &Value, out: &mut Vec<Value>) {
        for c in node["children"].as_array().into_iter().flatten() {
            if c["role"] == "menuitem" {
                if let Some(n) = c["name"].as_str() {
                    out.push(json!(n));
                }
            } else {
                items(c, out);
            }
        }
    }
    let mut map = serde_json::Map::new();
    for top in tree["children"].as_array().into_iter().flatten() {
        if let Some(title) = top["name"].as_str() {
            let mut list = vec![];
            items(top, &mut list);
            map.insert(title.to_string(), json!(list));
        }
    }
    Value::Object(map)
}

/// The ref of the element at `path` (menu titles then an item, `["View", "Toggle Sidebar"]`), or of
/// the first one with `role` and `name`.
fn find_ref(tree: &Value, path: &[String]) -> Option<String> {
    let (first, rest) = path.split_first()?;
    fn named<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
        for c in node["children"].as_array().into_iter().flatten() {
            if c["name"] == name {
                return Some(c);
            }
            if c["role"] != "menuitem" {
                if let Some(found) = named(c, name) {
                    return Some(found);
                }
            }
        }
        None
    }
    let here = named(tree, first)?;
    if rest.is_empty() {
        here["ref_id"].as_str().map(str::to_string)
    } else {
        find_ref(here, rest)
    }
}

fn find_role(node: &Value, role: &str, name: &str) -> Option<Value> {
    if node["role"] == role && node["name"] == name {
        return Some(node.clone());
    }
    node["children"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|c| find_role(c, role, name))
}

async fn snapshot(run: &Run, bin: &Path, window: &str, surface: &str) -> Result<Value> {
    let said = call(
        run,
        bin,
        &["snapshot", "--window-id", window, "--surface", surface],
    )
    .await?;
    Ok(data(said, "snapshot")?["tree"].clone())
}

/// The app in front now (its launch services number), to give the front back after a step that
/// needed the app's window forward.
fn front() -> Option<String> {
    let out = std::process::Command::new("/usr/bin/lsappinfo")
        .arg("front")
        .output()
        .ok()?;
    let asn = String::from_utf8_lossy(&out.stdout).trim().to_string();
    asn.starts_with("ASN:").then_some(asn)
}

fn give_front_back(before: Option<String>) {
    if let Some(asn) = before {
        if front().as_deref() != Some(asn.as_str()) {
            let _ = std::process::Command::new("/usr/bin/lsappinfo")
                .args(["setfront", &asn])
                .output();
        }
    }
}

fn path_of(input: &Value) -> Result<Vec<String>> {
    input["menu"]
        .as_array()
        .context("menu: the menu's title then the item (\"View\", \"Toggle Sidebar\")")?
        .iter()
        .map(|s| {
            s.as_str()
                .map(str::to_string)
                .context("a menu path is words")
        })
        .collect()
}

pub async fn run_node(run: &Run, node: &str, input: &Value) -> Result<Value> {
    let bin = match ready(run).await? {
        Ok(b) => b,
        Err(why) => return Ok(json!({ "skipped": why })),
    };
    let pid = crate::app::pid_of(input)?;
    let window = window_of(run, &bin, pid).await?;
    let surface = input["surface"].as_str().unwrap_or("menubar");
    anyhow::ensure!(
        ["menubar", "window", "menu", "focused", "sheet", "alert", "popover"].contains(&surface),
        "surface: menubar, window, menu, focused, sheet, alert or popover"
    );
    match node {
        "desktop.snapshot" => {
            let tree = snapshot(run, &bin, &window, surface).await?;
            let mut out = json!({"window": window, "surface": surface, "tree": plain(&tree)});
            if surface == "menubar" {
                out["menus"] = menus(&tree);
            }
            Ok(out)
        }
        "desktop.read" => {
            let tree = snapshot(run, &bin, &window, surface).await?;
            if let Some(title) = input["menu"].as_str() {
                let all = menus(&tree);
                let items = all
                    .get(title)
                    .cloned()
                    .with_context(|| format!("no menu {title} in the menu bar: {all}"))?;
                return Ok(json!({"window": window, "menu": title, "items": items}));
            }
            let role = input["role"].as_str().context("role, or menu")?;
            let name = input["name"].as_str().context("name")?;
            let found = find_role(&tree, role, name);
            Ok(
                json!({"window": window, "found": found.is_some(), "element": found.map(|f| plain(&f))}),
            )
        }
        "desktop.click" => {
            let path = path_of(input)?;
            let tree = snapshot(run, &bin, &window, surface).await?;
            let r = find_ref(&tree, &path)
                .with_context(|| format!("nothing at {path:?} in the {surface}"))?;
            let before = front();
            let said = call(run, &bin, &["--headed", "click", &r]).await;
            give_front_back(before);
            let said = data(said?, "click")?;
            Ok(
                json!({"window": window, "clicked": path, "delivery": said["disposition"]["delivery"], "steps": said["steps"]}),
            )
        }
        "desktop.press" => {
            let keys = input["keys"]
                .as_str()
                .context("keys: a chord as agent-desktop names it (cmd+b)")?;
            // Its window forward for the keys, as a person's are typed into the window in front;
            // agent-desktop runs the menu item that carries the chord, as macOS does.
            let before = front();
            let said = call(
                run,
                &bin,
                &["--headed", "press", keys, "--window-id", &window],
            )
            .await;
            give_front_back(before);
            let said = data(said?, "press")?;
            Ok(
                json!({"window": window, "pressed": keys, "delivery": said["disposition"]["delivery"], "steps": said["steps"]}),
            )
        }
        other => bail!("no node named {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obidspec_14_off_macos_or_without_both_permissions_a_desktop_step_skips() {
        assert!(unsupported("macos").is_none());
        assert!(unsupported("linux").unwrap().contains("this is linux"));
        let granted = json!({"ok": true, "data": {"accessibility": {"state": "granted"}, "screen_recording": {"state": "granted"}}});
        assert!(permission_skip(&granted).is_none());
        let denied = json!({"ok": true, "data": {"accessibility": {"state": "denied"}, "screen_recording": {"state": "granted"}}});
        let why = permission_skip(&denied).unwrap();
        assert!(why.starts_with(NO_PERMISSION), "{why}");
        let none = json!({"ok": false, "error": {"code": "PERM_DENIED"}});
        assert!(permission_skip(&none).is_some());
    }

    #[test]
    fn obidspec_14_the_menu_bar_reads_as_titles_and_item_names() {
        let tree = json!({"children": [
            {"role": "menuitem", "name": "View", "ref_id": "@s:e1", "children": [{"role": "menu", "children": [
                {"role": "menuitem", "name": "Toggle Sidebar", "ref_id": "@s:e2"},
                {"role": "menuitem", "ref_id": "@s:e3"},
                {"role": "menuitem", "name": "Zoom In", "ref_id": "@s:e4"}]}]},
            {"role": "menuitem", "name": "Help", "ref_id": "@s:e5", "children": []}]});
        assert_eq!(
            menus(&tree),
            json!({"View": ["Toggle Sidebar", "Zoom In"], "Help": []})
        );
        let path = ["View".to_string(), "Zoom In".to_string()];
        assert_eq!(find_ref(&tree, &path).as_deref(), Some("@s:e4"));
        assert_eq!(find_ref(&tree, &["Go".to_string()]), None);
    }
}
