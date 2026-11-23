//! The accounts area's own nodes (docs/contracts/accounts.md, sign-in.md, cli-versions.md,
//! keychain.md and the rest scope.json lists under `accounts`): what the suite's nodes can't do to
//! set a scenario up or to read what it must prove. They only act or read, never judge: what obid
//! answers is asserted with `call`, `home-file` and `check`.
//!
//! - `accounts.exec`: an executable script in a scratch obid's person's home (or its own home), a
//!   tool's binary with another version, one that fails or hangs.
//! - `accounts.link`: a symbolic link there, a login file that is a link (SIGN-IN-3).
//! - `accounts.log`: the scratch obid's own log, so a `check` can prove a login's secret never
//!   reached it.
//! - `accounts.openai` and its siblings: a stand-in OpenAI for Continue with ChatGPT
//!   (src/coverage_accounts_openai.rs).

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[path = "coverage_accounts_openai.rs"]
mod openai;

/// The accounts area's nodes: their names and what they do.
pub const NODES: &[(&str, &str)] = &[
    ("accounts.exec", "Writes an executable script (0700) at `path` in a scratch obid's person's home (`in`: person, the default, or home), its `text` as given: {path: where it is}."),
    ("accounts.link", "Makes a symbolic link at `path` in a scratch obid's person's home (`in`: person, the default, or home) pointing to `to` (a path in the same folder): {path, to}."),
    ("accounts.log", "Reads a scratch obid's own log (obid.log beside its home): {text, lines}; with `has`, also `with`: its lines that hold that text, so a `check` can count them."),
    ("accounts.openai", "Starts a stand-in OpenAI on 127.0.0.1 (a free port) for this workflow: {origin}, for obid-start's env as OBID_TEST_CHATGPT_ORIGIN."),
    ("accounts.openai-set", "Changes what the stand-in OpenAI (`openai`, an accounts.openai output) does next: sub, email, scope, deny, forged, wrongAud, callbackClient, holdMs, expiresIn; appends tokenErrors [{status, body}], revokeStatuses, responses ({said} | {failed: {code, message}} | {status, json} | {status, text} | {cut: [events]}) and responseHoldsMs; refuse (first-access, last-access, all-access, or none to clear them), refuseAll."),
    ("accounts.openai-saw", "What the stand-in OpenAI saw: {authorized, refreshes, tokens, access, refresh, firstAccess, lastAccess, lastRefresh, revoked, bearers, posts, models}; with expect (body/paths) and within (seconds, read again until it holds)."),
    ("accounts.visit", "GETs a loopback URL as the person's browser does, following no redirect: `url`, `host` (another Host header), `replace` [from, to] in the URL, `dropQuery`; `background` answers a handle at once and `wait` (a handle) its answer: {status, location, text}; with expect."),
    ("accounts.tokens", "Where the stand-in OpenAI's tokens are: every file under a scratch obid's folder that holds one (paths relative to it, sorted), how many are in any process's arguments or environment, and which of `look` (other texts) are: {files, inProcesses, tokens, found}; with expect and within."),
];

pub async fn run_node(name: &str, input: &Value) -> Result<Value> {
    if openai::NAMES.contains(&name) {
        return openai::run_node(name, input).await;
    }
    match name {
        "accounts.exec" => exec(input),
        "accounts.link" => link(input),
        "accounts.log" => log(input),
        other => bail!("{other} isn't an accounts node"),
    }
}

/// `rel` inside the obid's person's home (or its own), never above it.
fn inside(input: &Value, rel: &str) -> Result<PathBuf> {
    let key = match input["in"].as_str().unwrap_or("person") {
        "home" => "home",
        "person" => "person",
        other => bail!("in: {other}? person or home"),
    };
    let base = input["obid"][key]
        .as_str()
        .with_context(|| format!("obid.{key}: an obid-start step's output"))?;
    anyhow::ensure!(
        !rel.is_empty() && !rel.starts_with('/') && !rel.split('/').any(|p| p == ".."),
        "{rel:?} isn't a path inside that folder"
    );
    Ok(Path::new(base).join(rel))
}

fn exec(input: &Value) -> Result<Value> {
    let path = inside(input, input["path"].as_str().context("path")?)?;
    let text = input["text"].as_str().context("text")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Written beside it and renamed in, so a binary changed under a running obid is never seen
    // half written, and its modification time is new.
    let partial = path.with_extension("partial");
    std::fs::write(&partial, text)?;
    std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o700))?;
    std::fs::rename(&partial, &path)?;
    Ok(json!({"path": path}))
}

fn link(input: &Value) -> Result<Value> {
    let path = inside(input, input["path"].as_str().context("path")?)?;
    let to = inside(input, input["to"].as_str().context("to")?)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&path);
    std::os::unix::fs::symlink(&to, &path)?;
    Ok(json!({"path": path, "to": to}))
}

fn log(input: &Value) -> Result<Value> {
    let path = input["obid"]["log"]
        .as_str()
        .context("obid.log: an obid-start step's output")?;
    let text = std::fs::read_to_string(path).with_context(|| path.to_string())?;
    let mut out = json!({"lines": text.lines().count(), "text": text});
    if let Some(has) = input["has"].as_str() {
        out["with"] = json!(text.lines().filter(|l| l.contains(has)).collect::<Vec<_>>());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_never_leaves_its_folder() {
        let obid = json!({"person": "/tmp/p", "home": "/tmp/h"});
        for rel in ["", "/etc/passwd", "../x", "a/../../x"] {
            assert!(inside(&json!({"obid": obid}), rel).is_err(), "{rel}");
        }
        assert_eq!(
            inside(&json!({"obid": obid, "in": "home"}), "a/b").unwrap(),
            Path::new("/tmp/h/a/b")
        );
        assert!(inside(&json!({"obid": obid, "in": "elsewhere"}), "a").is_err());
    }
}
