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
//! - `accounts.openai*`: a stand-in OpenAI for Continue with ChatGPT (docs/contracts/
//!   chatgpt-sign-in.md), obid's own tests' `tests/support/stand_in_openai.rs` imported as it is,
//!   never copied, so the suite and obid's tests sign in to the same OpenAI. A scratch obid reaches
//!   it through `OBID_TEST_CHATGPT_ORIGIN`, which only a debug build reads.
//! - `accounts.scan`: every file under a scratch obid's folder (its home, its person's home, its
//!   log) searched for each secret, as it is, base64 and hex: where each one is.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The accounts area's nodes: their names and what they do.
pub const NODES: &[(&str, &str)] = &[
    ("accounts.exec", "Writes an executable script (0700) at `path` in a scratch obid's person's home (`in`: person, the default, or home), its `text` as given: {path: where it is}."),
    ("accounts.link", "Makes a symbolic link at `path` in a scratch obid's person's home (`in`: person, the default, or home) pointing to `to` (a path in the same folder): {path, to}."),
    ("accounts.openai", "Starts a stand-in OpenAI (obid's own tests' stand_in_openai.rs) for this workflow, kept as `name` (default `openai`): {name, origin}; start the obid with env OBID_TEST_CHATGPT_ORIGIN set to its origin."),
    ("accounts.openai-consent", "The person consents on the stand-in OpenAI's sign-in page at `url` (or, with `deny` set first, refuses): {location: where the browser goes next, obid's callback}. With `visit` true, the browser then goes there, as accounts.visit does: {location, status, text}."),
    ("accounts.visit", "The browser opens `url` (obid's sign-in callback), with `host` as its Host header and `query` replacing or, empty, dropping its parameters when given: {status, url, text}."),
    ("accounts.openai-script", "Sets how the stand-in OpenAI `name` behaves next (`set`: sub, email, scope, deny, forged, wrongAud, callbackClient, expiresIn, refuseAll, holdMs; `refuse` and `accept` [token]: 401 for those access tokens, or not; `tokenErrors` [[status, body]], `revokeStatuses` [status], `responses` [{said}|{failed: [code, message]}|{json: [status, body]}|{text: [status, text]}|{cut: [events]}]): {done}."),
    ("accounts.openai-seen", "What the stand-in OpenAI `name` saw: {authorized, refreshes, tokens, revoked, bearers: [{token, method, path}], bodies, posts}."),
    ("accounts.scan", "Searches every file under a scratch obid's folder for each of `secrets` (or, with `openai`, every token that stand-in made), as it is, base64 and hex: {found: [{secret, path}], files} with each `path` relative to the folder."),
    ("accounts.log", "Reads a scratch obid's own log (obid.log beside its home): {text, lines}; with `has`, also `with`: its lines that hold that text, so a `check` can count them."),
];

pub async fn run_node(name: &str, input: &Value) -> Result<Value> {
    match name {
        "accounts.exec" => exec(input),
        "accounts.link" => link(input),
        "accounts.log" => log(input),
        "accounts.openai" => openai::start(input),
        "accounts.openai-consent" => openai::consent(input).await,
        "accounts.visit" => openai::visit(input).await,
        "accounts.openai-script" => openai::script(input),
        "accounts.openai-seen" => openai::seen(input),
        "accounts.scan" => scan(input),
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

/// Every file under `dir`, with its path.
fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => files(&path, out),
            Ok(t) if t.is_file() => out.push(path),
            _ => {}
        }
    }
}

fn scan(input: &Value) -> Result<Value> {
    let folder = PathBuf::from(
        input["obid"]["folder"]
            .as_str()
            .context("obid.folder: an obid-start step's output")?,
    );
    let mut secrets: Vec<String> = input["secrets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect();
    if let Some(name) = input["openai"].as_str() {
        secrets.extend(openai::tokens(name)?);
    }
    anyhow::ensure!(!secrets.is_empty(), "no secret to look for");
    let mut all = vec![];
    files(&folder, &mut all);
    let mut found = vec![];
    for path in &all {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        for secret in &secrets {
            let forms = [
                secret.clone(),
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, secret),
                hex::encode(secret),
            ];
            if forms
                .iter()
                .any(|f| memchr::memmem::find(&bytes, f.as_bytes()).is_some())
            {
                let rel = path.strip_prefix(&folder).unwrap_or(path);
                found.push(json!({"secret": secret, "path": rel}));
            }
        }
    }
    Ok(json!({"found": found, "files": all.len(), "secrets": secrets.len()}))
}

/// obid's own tests' stand-in OpenAI, as it is (a path from this file's folder).
/// Left as obid formats it (`rustfmt::skip`): this crate's edition would sort its imports otherwise.
#[allow(dead_code, clippy::all)]
#[rustfmt::skip]
#[path = "../../../core/crates/obid/tests/support/stand_in_openai.rs"]
mod stand_in;

/// The stand-in OpenAI, one per name for the workflow (the nodes process is the workflow's own).
mod openai {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};

    use super::stand_in;
    use stand_in::{OpenAi, Reply};

    fn all() -> &'static Mutex<HashMap<String, Arc<OpenAi>>> {
        static ALL: OnceLock<Mutex<HashMap<String, Arc<OpenAi>>>> = OnceLock::new();
        ALL.get_or_init(Default::default)
    }

    fn named(input: &Value) -> String {
        input["name"].as_str().unwrap_or("openai").to_string()
    }

    fn get(name: &str) -> Result<Arc<OpenAi>> {
        all()
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .with_context(|| format!("no stand-in OpenAI {name}: start it with accounts.openai"))
    }

    pub fn start(input: &Value) -> Result<Value> {
        let name = named(input);
        let made = Arc::new(OpenAi::start());
        let origin = made.origin.clone();
        all().lock().unwrap().insert(name.clone(), made);
        Ok(json!({"name": name, "origin": origin}))
    }

    pub fn tokens(name: &str) -> Result<Vec<String>> {
        Ok(get(name)?.with(|s| s.tokens.clone()))
    }

    pub async fn consent(input: &Value) -> Result<Value> {
        let openai = get(&named(input))?;
        let url = input["url"]
            .as_str()
            .context("url: the sign-in page")?
            .to_string();
        let location = tokio::task::spawn_blocking(move || openai.consent(&url))
            .await
            .context("the consent page")?;
        if input["visit"] != true {
            return Ok(json!({"location": location}));
        }
        let mut visited =
            visit(&json!({"url": location, "host": input["host"], "query": input["query"]}))
                .await?;
        visited["location"] = json!(location);
        Ok(visited)
    }

    /// `url` with each of `replace`'s parameters set, or dropped when empty.
    fn with_query(url: &str, replace: &serde_json::Map<String, Value>) -> String {
        let (path, q) = url.split_once('?').unwrap_or((url, ""));
        let mut pairs: Vec<(String, String)> = q
            .split('&')
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (k, v) in replace {
            let v = v.as_str().unwrap_or_default().to_string();
            match pairs.iter_mut().find(|(n, _)| n == k) {
                Some(pair) => pair.1 = v,
                None => pairs.push((k.clone(), v)),
            }
        }
        pairs.retain(|(_, v)| !v.is_empty());
        if pairs.is_empty() {
            return path.to_string();
        }
        let q: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{path}?{}", q.join("&"))
    }

    pub async fn visit(input: &Value) -> Result<Value> {
        let mut url = input["url"].as_str().context("url")?.to_string();
        anyhow::ensure!(
            url.starts_with("http://127.0.0.1:"),
            "only obid's loopback callback: {url}"
        );
        if let Some(replace) = input["query"].as_object() {
            url = with_query(&url, replace);
        }
        let host = input["host"].as_str().map(str::to_string);
        tokio::task::spawn_blocking(move || {
            let headers: Vec<(&str, &str)> = host.iter().map(|h| ("Host", h.as_str())).collect();
            let (status, _, text) = stand_in::http("GET", &url, &headers, None);
            json!({"status": status, "url": url, "text": text.chars().take(2000).collect::<String>()})
        })
        .await
        .context("the browser's step")
    }

    pub fn script(input: &Value) -> Result<Value> {
        let openai = get(&named(input))?;
        openai.with(|s| -> Result<()> {
            let set = &input["set"];
            if let Some(v) = set["sub"].as_str() {
                s.sub = v.into();
            }
            if let Some(v) = set["email"].as_str() {
                s.email = v.into();
            }
            if let Some(v) = set["scope"].as_str() {
                s.scope = v.into();
            }
            if let Some(v) = set["deny"].as_bool() {
                s.deny = v;
            }
            if let Some(v) = set["forged"].as_bool() {
                s.forged = v;
            }
            if let Some(v) = set["wrongAud"].as_bool() {
                s.wrong_aud = v;
            }
            if let Some(v) = set.get("callbackClient") {
                s.callback_client = v.as_str().map(str::to_string);
            }
            if let Some(v) = set["expiresIn"].as_i64() {
                s.expires_in = v;
            }
            if let Some(v) = set["refuseAll"].as_bool() {
                s.refuse_all = v;
            }
            if let Some(v) = set["holdMs"].as_u64() {
                s.hold = std::time::Duration::from_millis(v);
            }
            for e in input["tokenErrors"].as_array().into_iter().flatten() {
                s.token_errors
                    .push_back((e[0].as_u64().context("a status")? as u16, e[1].clone()));
            }
            for token in input["refuse"].as_array().into_iter().flatten() {
                s.refused
                    .insert(token.as_str().context("a token")?.to_string());
            }
            for token in input["accept"].as_array().into_iter().flatten() {
                s.refused.remove(token.as_str().context("a token")?);
            }
            for e in input["revokeStatuses"].as_array().into_iter().flatten() {
                s.revoke_statuses
                    .push_back(e.as_u64().context("a status")? as u16);
            }
            for r in input["responses"].as_array().into_iter().flatten() {
                let reply = if let Some(w) = r["said"].as_str() {
                    Reply::Sse(stand_in::said(w))
                } else if let Some(f) = r["failed"].as_array() {
                    Reply::Sse(stand_in::failed(
                        f[0].as_str().unwrap_or_default(),
                        f[1].as_str().unwrap_or_default(),
                    ))
                } else if let Some(j) = r["json"].as_array() {
                    Reply::Json(j[0].as_u64().unwrap_or(200) as u16, j[1].clone())
                } else if let Some(t) = r["text"].as_array() {
                    Reply::Text(
                        t[0].as_u64().unwrap_or(200) as u16,
                        t[1].as_str().unwrap_or_default().into(),
                    )
                } else if let Some(c) = r["cut"].as_array() {
                    Reply::Cut(c.clone())
                } else {
                    bail!("a response is said, failed, json, text or cut: {r}")
                };
                s.responses.push_back(reply);
            }
            Ok(())
        })?;
        Ok(json!({"done": true}))
    }

    pub fn seen(input: &Value) -> Result<Value> {
        let openai = get(&named(input))?;
        Ok(openai.with(|s| {
            json!({
                "authorized": s.authorized,
                "refreshes": s.refreshes,
                "tokens": s.tokens,
                "revoked": s.revoked,
                "bearers": s.bearers.iter().map(|(t, m, p)| json!({"token": t, "method": m, "path": p})).collect::<Vec<_>>(),
                "bodies": s.bodies,
                "posts": s.bodies.len(),
            })
        }))
    }
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
