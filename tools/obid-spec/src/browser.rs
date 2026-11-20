//! obid's browser, proven from outside (docs/contracts/browser.md): the lab's browser API workflows
//! (`workflows/lab/browserapi/`, LAB-BROWSERAPI-1 to 7) pointed at the real obid. Its nodes are the
//! lab's clients (src/lab/browserapi.rs: the MCP call, the events stream, the race, the stream's
//! readers) given a scratch obid (`obid-start`'s output, whose `port` and `token` they take) in
//! place of the lab's server, the lab API's fixture sites (the plain one, the one that fails then
//! works at one address, and the HTTPS one whose self-signed certificate is made at run time and
//! swapped behind its port, Chrome refusing it on macOS and Linux alike), and what only obid's side
//! needs: a field
//! read through the job's own agent, the secret and the scan for it under obid's home, and the
//! downloads in a job's profile and the person's Downloads.
//!
//! The page's field is read the way an agent would (`oby_browser_evaluate` with the job's token),
//! the tab given back first when the person's device holds it and taken again after; a secret's
//! field is compared by its length and a hash computed in the page, so the value never leaves this
//! process. Its nodes are registered in src/nodes.rs, `browser-*`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::lab::browserapi as lab;

/// The secrets this workflow sent, kept in this process only, for the scan.
static SECRETS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Every answer and event the browser nodes read, for the scan.
static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The fixture sites this workflow serves.
static SITES: Mutex<Vec<tokio::task::JoinHandle<()>>> = Mutex::new(Vec::new());

fn seen(v: &Value) {
    SEEN.lock().unwrap().push(v.to_string());
}

/// The lab clients' input, with the scratch obid as their API (its `port` and owner `token`).
fn as_lab(input: &Value) -> Value {
    let mut v = input.clone();
    v["api"] = input["obid"].clone();
    if let Some(t) = input["token"].as_str() {
        v["token"] = json!(token(t));
    }
    v
}

/// A token as a file or header held it: no `Bearer `, no white space around.
fn token(t: &str) -> String {
    t.trim().trim_start_matches("Bearer ").trim().to_string()
}

pub async fn cleanup() -> Value {
    let sites: Vec<_> = SITES.lock().unwrap().drain(..).collect();
    for s in &sites {
        s.abort();
    }
    SECRETS.lock().unwrap().clear();
    SEEN.lock().unwrap().clear();
    json!({"sites": sites.len()})
}

pub async fn run_node(name: &str, input: &Value) -> Result<Value> {
    // `within`: asked again until its expectation holds, for what obid does a moment later.
    let until = Instant::now() + Duration::from_secs(input["within"].as_u64().unwrap_or(0));
    loop {
        let out = run_once(name, input).await?;
        let holds = match input.get("expect") {
            Some(want) => crate::expect::check(want, &out, "").is_empty(),
            None => true,
        };
        // A browser that failed to start (`browser`, not `browser_starting`) won't start by being
        // asked again: the step fails now, with obid's reason, instead of at its time limit.
        let failed_for_good =
            name == "browser-mcp" && out["structured"]["error"]["code"] == "browser";
        if holds || failed_for_good || Instant::now() >= until {
            return Ok(out);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

async fn run_once(name: &str, input: &Value) -> Result<Value> {
    match name {
        "browser-site" => {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let port = listener.local_addr()?.port();
            SITES
                .lock()
                .unwrap()
                .push(tokio::spawn(lab::fixture(listener)));
            Ok(json!({
                "url": format!("http://127.0.0.1:{port}"), "port": port,
                "name": {"x": lab::FIXTURE_NAME.0, "y": lab::FIXTURE_NAME.1},
                "download": {"x": lab::FIXTURE_DOWNLOAD.0, "y": lab::FIXTURE_DOWNLOAD.1},
            }))
        }
        "browser-https-site" => lab::https(input).await,
        "browser-flaky-site" => lab::flaky(input).await,
        "browser-job" => job(input).await,
        "browser-agent-job" => agent_job(input).await,
        "browser-mcp" => {
            let out = lab::mcp_call(&as_lab(input)).await?;
            seen(&out);
            Ok(out)
        }
        "browser-events" => {
            let out = lab::events(&as_lab(input)).await?;
            seen(&out);
            Ok(out)
        }
        "browser-race" => {
            let out = lab::race(&as_lab(input)).await?;
            seen(&out);
            Ok(out)
        }
        "browser-watch" => lab::watch(&as_lab(input)).await,
        "browser-read" => {
            let selector = input["selector"].as_str().context("selector")?;
            let expression = format!(
                "(document.querySelector({}) || {{}}).value ?? null",
                serde_json::to_string(selector)?
            );
            let v = read_field(input, &expression).await?;
            Ok(json!({"value": v}))
        }
        "browser-secret" => secret(input).await,
        "browser-scan" => Ok(scan(input)),
        "browser-downloads" => downloads(input),
        other => anyhow::bail!("{other} isn't one of the browser's nodes"),
    }
}

/// A stand-in job that stays active, and its broker token; or, with `end`, that job stopped.
async fn job(input: &Value) -> Result<Value> {
    let obid = &input["obid"];
    let home = PathBuf::from(crate::sphere::str_of(obid, "home")?);
    let until = |s: u64| Instant::now() + Duration::from_secs(s);
    if input["end"] == true {
        let id = crate::sphere::str_of(input, "job")?;
        let stopped = crate::sphere::obid_call(
            obid,
            "POST",
            &format!("/v1/jobs/{id}/stop"),
            Some(&json!({})),
        )
        .await?;
        anyhow::ensure!(
            stopped.status == 200,
            "stop {id}: {} {}",
            stopped.status,
            stopped.text
        );
        let deadline = until(60);
        loop {
            let j = crate::sphere::obid_call(obid, "GET", &format!("/v1/jobs/{id}"), None).await?;
            let status = j.body["status"].as_str().unwrap_or_default().to_string();
            if status != "running" && status != "queued" {
                return Ok(json!({"job": id, "ended": true, "status": status}));
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "job {id} is still {status} after 60 s"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    let made = crate::sphere::obid_call(
        obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": input["repo"].as_str().unwrap_or("obi-e2e/fixture-app"),
            "kind": "build", "task": "SLEEP TOKEN FILE", "agent": "codex"}}),
        ),
    )
    .await?;
    anyhow::ensure!(made.status == 201, "the job: {} {}", made.status, made.text);
    let id = made.body["id"]
        .as_str()
        .context("the job's id")?
        .to_string();
    // Its stand-in writes its broker token in its worktree once its turn runs.
    let file = home.join("worktrees").join(&id).join(".obi/mcp-token");
    let deadline = until(90);
    loop {
        let t = std::fs::read_to_string(&file).unwrap_or_default();
        let t = t.trim();
        if t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
            let j = crate::sphere::obid_call(obid, "GET", &format!("/v1/jobs/{id}"), None).await?;
            anyhow::ensure!(
                j.body["status"] == "running",
                "job {id} is {}",
                j.body["status"]
            );
            return Ok(json!({"job": id, "token": t}));
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "job {id}'s stand-in wrote no broker token in 90 s (obid-start's `browser: true` gives it a Codex account)"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// A job of `agent` whose stand-in browses `url` through its own configuration (BROWSE, BROWSER-17),
/// and what it wrote: the server and rules it found, and each call's answer.
async fn agent_job(input: &Value) -> Result<Value> {
    let obid = &input["obid"];
    let home = PathBuf::from(crate::sphere::str_of(obid, "home")?);
    let agent = crate::sphere::str_of(input, "agent")?;
    let url = crate::sphere::str_of(input, "url")?;
    let mut job = json!({"repo": "obi-e2e/fixture-app", "kind": "build",
        "task": format!("BROWSE: {url}"), "agent": agent});
    if let Some(mode) = input["mode"].as_str() {
        job["mode"] = json!(mode);
    }
    let made =
        crate::sphere::obid_call(obid, "POST", "/v1/jobs", Some(&json!({"job": job}))).await?;
    anyhow::ensure!(made.status == 201, "the job: {} {}", made.status, made.text);
    let id = made.body["id"]
        .as_str()
        .context("the job's id")?
        .to_string();
    let file = home.join("worktrees").join(&id).join(".obi/browsed.json");
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if let Ok(browsed) =
            serde_json::from_str::<Value>(&std::fs::read_to_string(&file).unwrap_or_default())
        {
            let j = crate::sphere::obid_call(obid, "GET", &format!("/v1/jobs/{id}"), None).await?;
            return Ok(json!({"job": id, "status": j.body["status"], "browsed": browsed}));
        }
        let j = crate::sphere::obid_call(obid, "GET", &format!("/v1/jobs/{id}"), None).await?;
        let status = j.body["status"].as_str().unwrap_or_default();
        anyhow::ensure!(
            status == "running" || status == "queued",
            "job {id} is {status} and its stand-in wrote no .obi/browsed.json: {}",
            j.body["step"]
        );
        anyhow::ensure!(
            Instant::now() < deadline,
            "job {id}'s stand-in wrote no .obi/browsed.json in 180 s"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The tab's holder now, by `GET /v1/browser`.
async fn held_by(input: &Value, tab: &str) -> Result<Option<String>> {
    let r = lab_call(input, "GET", "/v1/browser", None).await?;
    Ok(r.body["tabs"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["id"] == tab)
        .and_then(|t| t["heldBy"].as_str().map(str::to_string)))
}

/// One request through the lab's client: `as` owner, or `device` (its signed words).
async fn lab_call(
    input: &Value,
    method: &str,
    path: &str,
    as_device: Option<&Value>,
) -> Result<lab::Reply> {
    let port = input["obid"]["port"].as_u64().context("obid.port")? as u16;
    let mut auth_in = as_lab(input);
    let body: &[u8] = if method == "GET" { b"" } else { b"{}" };
    match as_device {
        Some(d) => {
            auth_in["as"] = json!("device");
            auth_in["device"] = d.clone();
        }
        None => auth_in["as"] = json!("owner"),
    }
    let headers = lab::auth(&auth_in, method, path, body)?;
    lab::http(port, method, path, &headers, body).await
}

/// `expression`'s value in the job's tab, read by its own agent; the tab given back first when the
/// person's `device` holds it, and taken again after.
async fn read_field(input: &Value, expression: &str) -> Result<Value> {
    let tab = input["tab"].as_str();
    let held = match tab {
        Some(t) => held_by(input, t).await?,
        None => None,
    };
    let device = input.get("device").filter(|d| d.is_object());
    let give = held.is_some() && device.is_some_and(|d| d["id"].as_str() == held.as_deref());
    if let (true, Some(t), Some(d)) = (give, tab, device) {
        let r = lab_call(
            input,
            "POST",
            &format!("/v1/browser/tabs/{t}/giveBack"),
            Some(d),
        )
        .await?;
        anyhow::ensure!(
            r.status == 200,
            "give back before the read: {} {}",
            r.status,
            r.body
        );
    }
    let mut args = json!({"expression": expression});
    if let Some(t) = tab {
        args["tab"] = json!(t);
    }
    let mut call = as_lab(input);
    call["tool"] = json!("oby_browser_evaluate");
    call["args"] = args;
    call["method"] = json!("tools/call");
    let out = lab::mcp_call(&call).await?;
    if let (true, Some(t), Some(d)) = (give, tab, device) {
        let r = lab_call(
            input,
            "POST",
            &format!("/v1/browser/tabs/{t}/take"),
            Some(d),
        )
        .await?;
        anyhow::ensure!(
            r.status == 200,
            "take again after the read: {} {}",
            r.status,
            r.body
        );
    }
    anyhow::ensure!(
        out["structured"]["ok"] == true,
        "the agent's read: {}",
        out["structured"]
    );
    Ok(out["structured"]["result"]["outside"]["data"].clone())
}

/// The page's own hash of a value (as [`HASH_JS`] computes it): its length and a 32-bit hash.
fn hash(value: &str) -> String {
    let mut h: u32 = 5381;
    for ch in value.chars() {
        h = h.wrapping_mul(33) ^ (ch as u32);
    }
    format!("{}:{h}", value.chars().count())
}

const HASH_JS: &str = "(v=>{let h=5381;for(const ch of v){h=(Math.imul(h,33)^ch.charCodeAt(0))>>>0}return v.length+':'+h})";

async fn secret(input: &Value) -> Result<Value> {
    let port = input["obid"]["port"].as_u64().context("obid.port")? as u16;
    let tab = input["tab"].as_str().context("tab")?;
    let field = input["field"].as_str().unwrap_or("#pw");
    let value = format!("obid-secret-{}", hex::encode(rand::random::<[u8; 12]>()));
    SECRETS.lock().unwrap().push(value.clone());
    let path = format!("/v1/browser/tabs/{tab}/secret");
    let body = json!({"field": field, "value": value}).to_string();
    let headers = lab::auth(&as_lab(input), "POST", &path, body.as_bytes())?;
    let r = lab::http(port, "POST", &path, &headers, body.as_bytes()).await?;
    let expression = format!(
        "{HASH_JS}((document.querySelector({}) || {{}}).value || '')",
        serde_json::to_string(field)?
    );
    let read = read_field(input, &expression).await?;
    let out = json!({
        "status": r.status, "body": r.body,
        "fieldHasIt": read.as_str() == Some(hash(&value).as_str()),
        "answerHasIt": r.body.to_string().contains(&value),
    });
    seen(&out);
    Ok(out)
}

fn scan(input: &Value) -> Value {
    use base64::Engine as _;
    let secrets = SECRETS.lock().unwrap().clone();
    let needles: Vec<Vec<u8>> = secrets
        .iter()
        .flat_map(|s| {
            [
                s.as_bytes().to_vec(),
                base64::engine::general_purpose::STANDARD
                    .encode(s)
                    .into_bytes(),
                hex::encode(s).into_bytes(),
            ]
        })
        .collect();
    let finders: Vec<memchr::memmem::Finder> =
        needles.iter().map(memchr::memmem::Finder::new).collect();
    let has = |bytes: &[u8]| finders.iter().any(|f| f.find(bytes).is_some());
    let root = PathBuf::from(input["obid"]["folder"].as_str().unwrap_or_default());
    let mut scanned = 0u64;
    let mut found = vec![];
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(meta) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(p);
            } else if meta.is_file() && meta.len() < 256 * 1024 * 1024 {
                if let Ok(bytes) = std::fs::read(&p) {
                    scanned += 1;
                    if has(&bytes) {
                        found.push(p.strip_prefix(&root).unwrap_or(&p).display().to_string());
                    }
                }
            }
        }
    }
    let answers = SEEN.lock().unwrap().clone();
    for (n, a) in answers.iter().enumerate() {
        if has(a.as_bytes()) {
            found.push(format!("answer or event {n}"));
        }
    }
    json!({"secrets": secrets.len(), "filesScanned": scanned, "answersScanned": answers.len(),
        "found": found, "clean": found.is_empty() && !secrets.is_empty() && scanned > 0})
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.') && !n.ends_with(".crdownload"))
        .collect();
    names.sort();
    names
}

fn downloads(input: &Value) -> Result<Value> {
    let home = PathBuf::from(input["obid"]["home"].as_str().context("obid.home")?);
    let person = PathBuf::from(input["obid"]["person"].as_str().context("obid.person")?);
    let job = input["job"].as_str().context("job")?;
    anyhow::ensure!(
        !job.is_empty() && !job.contains('/') && !job.contains(".."),
        "{job} isn't a job's id"
    );
    let profile = home
        .join("browser/jobs")
        .join(job)
        .join("profile/Downloads");
    Ok(json!({
        "inProfile": names_in(&profile),
        "delivered": names_in(&person.join("Downloads")),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_the_pages() {
        // As the page's HASH_JS computes it for "ab": ((5381*33)^97)*33 ^ 98, kept to 32 bits.
        let h = ((5381u32.wrapping_mul(33) ^ 97).wrapping_mul(33)) ^ 98;
        assert_eq!(hash("ab"), format!("2:{h}"));
        assert_eq!(token("Bearer abc\n"), "abc");
    }

    #[test]
    fn the_scan_finds_a_secret_as_it_is_base64_or_hex_and_passes_a_clean_folder() {
        use base64::Engine as _;
        let dir = tempfile::tempdir().unwrap();
        let secret = "obid-secret-scan-test";
        SECRETS.lock().unwrap().push(secret.into());
        std::fs::write(dir.path().join("clean.bin"), vec![0u8; 1 << 20]).unwrap();
        let input = json!({"obid": {"folder": dir.path()}});
        assert_eq!(scan(&input)["clean"], true, "{}", scan(&input));
        for (n, bytes) in [
            secret.as_bytes().to_vec(),
            base64::engine::general_purpose::STANDARD
                .encode(secret)
                .into_bytes(),
            hex::encode(secret).into_bytes(),
        ]
        .into_iter()
        .enumerate()
        {
            let mut big = vec![7u8; 1 << 20];
            big.extend(&bytes);
            let file = dir.path().join(format!("found-{n}.bin"));
            std::fs::write(&file, big).unwrap();
            let out = scan(&input);
            assert_eq!(out["clean"], false, "{out}");
            std::fs::remove_file(file).unwrap();
        }
        SECRETS.lock().unwrap().clear();
    }
}
