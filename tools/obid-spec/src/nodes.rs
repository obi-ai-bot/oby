//! The suite's nodes, written once and used by every workflow (docs/contracts/obid-spec.md,
//! OBIDSPEC-2). Each takes its step's `inputs` (a JSON object, templated by the host) and answers
//! a JSON value the host keeps as `state.<step id>`. A node that finds what it checks false
//! answers an error saying every way it fell short.

use crate::expect;
use crate::http::{self, Request};
use crate::keys::{self, DeviceKey, OwnerKey};
use crate::obid::{self, Run};
use crate::{events, sphere};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// Every node: its name, what it does, the inputs it takes.
pub const NODES: &[(&str, &str)] = &[
    ("obid-start", "Starts a scratch obid on its own home and port, with stand-in agents (again with the same name: a restart on the same home)."),
    ("obid-stop", "Stops a scratch obid this suite started, by PID: SIGTERM, then SIGKILL."),
    ("call", "Calls an obid endpoint as the owner, a paired device's signed words, another token or none; optionally waits until it answers as expected."),
    ("call-wait", "Waits for the answer of a call sent in the background, and asserts it."),
    ("check", "Asserts a value (a former step's output) against an expectation."),
    ("events-follow", "Follows GET /v1/events from now on."),
    ("events-wait", "Waits for an event on /v1/events matching an expectation, or that none comes."),
    ("home-file", "Checks a file in obid's home: there or not, its mode, its JSON or text."),
    ("write-file", "Writes a file in a scratch obid's home or its person's home (what a stand-in reads next: the fake gh's answers)."),
    ("device-pair", "Registers a device of the owner's (P-256), as the Mac app adds its own."),
    ("device-sign", "Signs as the device: X-Oby-Words for a request, or a Confirm tap's approval."),
    ("owner-key", "Makes an owner key (Ed25519), as the Electron app's main makes one."),
    ("sign-card", "Signs a card with an owner key (a detached JWS over its canonical JSON)."),
    ("delegation", "Signs an owner's delegation (or, without obi, a 5-minute assertion)."),
    ("sphere-make", "Makes a fresh Obysphere on the run's local Sphere stand-in: its admin, the members rule, an open link."),
    ("sphere-invite", "Makes another link to an Obysphere."),
    ("sphere-policy", "Writes an Obysphere's rules as its admin."),
    ("sphere-admin", "Calls the Sphere's own API as an Obysphere's admin."),
    ("sphere-owner", "Calls the Sphere's own API with an owner's own token."),
    ("sphere-join", "Joins an obid as the owner's app does: its key, its card signed by the owner key, a delegation, the redeem."),
    ("sphere-seal", "Keeps each member's card signed until every one says sealing on."),
    ("fixture-git", "Runs git in the workflow's fixture remote (a branch, a pull request's ref), answering what it printed."),
    ("clock", "The time now, in seconds since the epoch, with today's and yesterday's UTC midnight, for a file a step seeds."),
    ("next-second", "Waits until the clock enters its next second, so what obid stamps to the second after it is later than what it stamped before."),
    ("sphere-liar", "Puts a Sphere that lies in front of an Obysphere: every call passes through, but the directory can serve a member's card re-signed with another owner key."),
    ("sphere-lie", "Has the lying Sphere serve an owner's cards signed by a new owner key from now on."),
    ("app.launch", "Starts the real Electron app (this checkout's build), hidden, with its own user-data folder, on a scratch obid or its demo data (mock), through drive-lib.mjs's launcher."),
    ("app.click", "Clicks (or, with hover, points at) one element of the app's window, found by role and name, label, text or css."),
    ("app.type", "Types words into the app's window, into a target first clicked when one is named."),
    ("app.press", "Presses keys in the app's window (Enter, Meta+B), as Playwright names them."),
    ("app.read", "Reads the elements a target finds: how many, their text, whether visible, the attributes asked for; with expect, within and hold."),
    ("app.measure", "Measures an element's box and, against other targets, whether they overlap, the area they share and the gap between them; with expect."),
    ("app.screenshot", "Saves a screenshot of the app's window in the workflow's folder (and OBID_SPEC_SHOTS when it's set)."),
    ("app.compare", "Compares the app's window with a design board (tools/obid-spec/lab-ui/browser/design/boards/<board>.dc.html) box by box and word by word, as lab.ui.compare does the lab's screens: every held piece within `tolerance` px (4 by default); today's app pieces reported; with expect and within."),
    ("app.sidebyside", "Saves the app's window beside a design board as one PNG into the workflow's folder (and OBID_SPEC_SHOTS), for the owner's look."),
    ("app.close", "Quits the app and stops its helper, by PID."),
    ("desktop.snapshot", "Reads a surface of the app's (the menu bar by default) through agent-desktop: its tree and, for the menu bar, each menu's items by name. macOS only; skipped without the permissions."),
    ("desktop.read", "Reads one menu's items by name, or one element by role and name, in a surface of the app's, through agent-desktop; with expect."),
    ("desktop.click", "Runs one of the app's menu items through agent-desktop (its window brought forward for the moment, the front given back)."),
    ("desktop.press", "Presses a key chord through macOS with agent-desktop (cmd+b), which runs the app's menu item that carries it, as a person's keys do."),
];

type Pending =
    std::collections::HashMap<String, tokio::task::JoinHandle<std::result::Result<Value, String>>>;
static PENDING: std::sync::Mutex<Option<Pending>> = std::sync::Mutex::new(None);

pub async fn run_node(run: &Run, name: &str, input: Value) -> Result<Value> {
    if name == "app.launch" {
        return crate::app::launch(run, &input).await;
    }
    if name.starts_with("app.") || name.starts_with("desktop.") || name.starts_with("lab.") {
        let mut out = if name.starts_with("app.") {
            crate::app::act(run, name, &input).await?
        } else if name.starts_with("lab.") {
            crate::lab::run_node(run, name, &input).await?
        } else {
            crate::desktop::run_node(run, name, &input).await?
        };
        // A skip (no macOS, no permission, no agent-desktop, no OrbStack) is answered as it is: the host marks
        // the workflow skipped (OBIDSPEC-15).
        if out.get("skipped").is_some() {
            return Ok(out);
        }
        if let Some(want) = input.get("expect") {
            // app.read and app.measure judged it already, with within and hold
            let failures = expect::check(want, &out, "");
            if !failures.is_empty() {
                bail!("{}", failures.join("; "));
            }
            out["red"] = red_proof(Some(want), 0, &out, "");
        }
        return Ok(out);
    }
    match name {
        "obid-start" => obid::start(run, serde_json::from_value(input)?).await,
        "obid-stop" => {
            let pid = input["obid"]["pid"].as_u64().context("obid.pid")? as u32;
            // `kill`: SIGKILL, obid ending with no chance to save anything (a crash, a power cut).
            if input["kill"] == true {
                anyhow::ensure!(
                    crate::procs::ours(pid),
                    "process {pid} isn't one this suite started"
                );
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
            obid::stop(pid, Duration::from_secs(10)).await
        }
        // `background`: the request goes now and the step answers its handle at once; a
        // `call-wait` step later waits for its answer (a turn's words, which block until it ends,
        // while a later step stops or cancels it).
        "call" if input["background"] == true => {
            let handle = uuid::Uuid::new_v4().to_string();
            let mut request = input.clone();
            request.as_object_mut().unwrap().remove("background");
            let task =
                tokio::spawn(async move { call(&request).await.map_err(|e| format!("{e:#}")) });
            PENDING
                .lock()
                .unwrap()
                .get_or_insert_with(Default::default)
                .insert(handle.clone(), task);
            Ok(json!({"handle": handle}))
        }
        "call" => call(&input).await,
        "call-wait" => {
            let handle = sphere::str_of(&input["call"], "handle")?;
            let task = PENDING
                .lock()
                .unwrap()
                .as_mut()
                .and_then(|p| p.remove(handle))
                .context("no background call with that handle (or it was waited for already)")?;
            let seconds = input["seconds"].as_u64().unwrap_or(120);
            let answer = tokio::time::timeout(Duration::from_secs(seconds), task)
                .await
                .with_context(|| {
                    format!("the background call answered nothing within {seconds} s")
                })??
                .map_err(|e| anyhow::anyhow!(e))?;
            let want = input.get("expect");
            let text = answer["text"].as_str().unwrap_or_default().to_string();
            let status = answer["status"].as_u64().unwrap_or(0) as u16;
            if let Some(w) = want {
                let failures = answer_failures(w, status, &answer["body"], &text);
                if !failures.is_empty() {
                    bail!(
                        "{}: {}",
                        failures.join("; "),
                        text.chars().take(600).collect::<String>()
                    );
                }
            }
            let mut out = answer.clone();
            out["red"] = red_proof(want, status, &answer["body"], &text);
            Ok(out)
        }
        "check" => {
            let failures = expect::check(&input["expect"], &input["value"], "");
            if failures.is_empty() {
                let red = red_proof(input.get("expect"), 0, &input["value"], "");
                Ok(json!({"passed": true, "red": red}))
            } else {
                bail!("{}", failures.join("; "))
            }
        }
        "events-follow" => {
            let base = sphere::str_of(&input["obid"], "base")?;
            let key = input["name"].as_str().unwrap_or(base).to_string();
            events::follow_as(
                base,
                sphere::str_of(&input["obid"], "token")?,
                input["after"].as_str(),
                &key,
            )
            .await
        }
        "events-wait" => {
            let none = input["none"].as_bool().unwrap_or(false);
            let mut found = events::wait(
                input["name"]
                    .as_str()
                    .map(Ok)
                    .unwrap_or_else(|| sphere::str_of(&input["obid"], "base"))?,
                sphere::str_of(&input, "event")?,
                input.get("expect").unwrap_or(&json!({})),
                input["seconds"].as_u64().unwrap_or(30),
                none,
            )
            .await?;
            if !none {
                let data = found["data"].clone();
                found["red"] = red_proof(input.get("expect"), 0, &data, "");
            }
            Ok(found)
        }
        "cleanup" => Ok(json!({
            "stopped": crate::procs::stop_all().await,
            "lab": crate::lab::cleanup().await,
        })),
        // `within`: asked again until it holds, for a file obid writes a moment later.
        "home-file" => {
            let until = std::time::Instant::now()
                + Duration::from_secs(input["within"].as_u64().unwrap_or(0));
            loop {
                match home_file(&input) {
                    Ok(v) => break Ok(v),
                    Err(e) if std::time::Instant::now() >= until => break Err(e),
                    Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
                }
            }
        }
        "device-pair" => device_pair(&input).await,
        "write-file" => {
            let base = match input["in"].as_str().unwrap_or("person") {
                "person" => sphere::str_of(&input["obid"], "person")?,
                "home" => sphere::str_of(&input["obid"], "home")?,
                other => bail!("in: {other}? person or home"),
            };
            let rel = sphere::str_of(&input, "path")?;
            anyhow::ensure!(
                !rel.starts_with('/') && !rel.split('/').any(|p| p == ".."),
                "{rel} isn't a path inside that folder"
            );
            let path = std::path::Path::new(base).join(rel);
            obid::private_dir(path.parent().context("no folder")?)?;
            let text = match &input["content"] {
                Value::String(t) => t.clone(),
                other => other.to_string(),
            };
            obid::private_file(&path, &text)?;
            Ok(json!({"written": rel}))
        }
        "device-sign" => device_sign(&input),
        "owner-key" => {
            let key = OwnerKey::generate();
            Ok(json!({"seed": key.seed_hex(), "public": key.public(), "keyId": key.key_id()}))
        }
        "sign-card" => {
            let key = owner_key(&input)?;
            key.sign_card(&input["card"])
        }
        "delegation" => {
            let key = owner_key(&input)?;
            let jwt = key.assertion(
                sphere::str_of(&input, "owner")?,
                sphere::str_of(&input, "sphere")?,
                input["scope"].as_str().unwrap_or(sphere::SCOPES),
                input["obi"].as_str(),
                input["life"].as_i64().unwrap_or(2_592_000),
            );
            Ok(json!({"jwt": jwt}))
        }
        "sphere-make" => {
            let url = run
                .sphere_url
                .as_deref()
                .context("this run has no Sphere")?;
            let operator = run
                .sphere_operator
                .as_deref()
                .context("no operator token")?;
            sphere::make(url, operator, input.get("rules").cloned()).await
        }
        "sphere-invite" => sphere::invite(&input["sphere"]).await,
        "sphere-policy" => sphere::set_policy(&input["sphere"], input["rules"].clone()).await,
        "sphere-admin" => {
            let a = sphere::as_admin(
                &input["sphere"],
                sphere::str_of(&input, "method")?,
                sphere::str_of(&input, "path")?,
                input.get("body").cloned(),
            )
            .await?;
            answered(&input, &a)
        }
        "sphere-owner" => {
            let made = &input["sphere"];
            let key = owner_key(&input)?;
            let token = sphere::owner_token(
                sphere::str_of(made, "url")?,
                &key,
                sphere::str_of(&input, "owner")?,
                sphere::str_of(made, "sphere")?,
                input["scope"].as_str().unwrap_or("directory:read"),
            )
            .await?;
            let a = sphere::call(
                sphere::str_of(made, "url")?,
                sphere::str_of(&input, "method")?,
                sphere::str_of(&input, "path")?,
                Some(&token),
                input.get("body").cloned().map(sphere::Body::Json),
            )
            .await?;
            answered(&input, &a)
        }
        "sphere-join" => {
            let key = owner_key(&input)?;
            let accepts = input.get("accepts").map(|a| {
                a.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            });
            sphere::join(
                &input["obid"],
                &input["sphere"],
                &key,
                input["obiId"].as_str(),
                input["name"].as_str(),
                accepts,
                input.get("invitation"),
            )
            .await
        }
        "sphere-seal" => {
            let mut members = vec![];
            for m in input["members"].as_array().context("members")? {
                members.push((m["obid"].clone(), owner_key(m)?));
            }
            sphere::seal_all(&members, input["seconds"].as_u64().unwrap_or(120)).await
        }
        "fixture-git" => {
            let args: Vec<String> = input["args"]
                .as_array()
                .context("args")?
                .iter()
                .map(|a| a.as_str().unwrap_or_default().to_string())
                .collect();
            let remote = run.scratch.join("fixture/remote.git");
            anyhow::ensure!(
                remote.is_dir(),
                "no fixture remote yet: an obid-start step first"
            );
            let out = std::process::Command::new("/usr/bin/git")
                .arg("-C")
                .arg(&remote)
                .args(&args)
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", &run.scratch)
                .output()?;
            anyhow::ensure!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            Ok(json!({"out": String::from_utf8_lossy(&out.stdout).trim()}))
        }
        "clock" => {
            let now = chrono::Utc::now().timestamp();
            let midnight = now - now.rem_euclid(86_400);
            Ok(
                json!({"now": now, "midnight": midnight, "yesterday": midnight - 86_400, "tenDaysAgo": now - 10 * 86_400}),
            )
        }
        "next-second" => {
            let now = chrono::Utc::now().timestamp();
            while chrono::Utc::now().timestamp() == now {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok(json!({"second": chrono::Utc::now().timestamp()}))
        }
        "sphere-liar" => {
            let mut made = input["sphere"].clone();
            let upstream = sphere::str_of(&made, "url")?.to_string();
            made["url"] = json!(crate::liar::start(&upstream).await?);
            made["upstream"] = json!(upstream);
            Ok(made)
        }
        "sphere-lie" => {
            let key = OwnerKey::generate();
            crate::liar::swap(
                sphere::str_of(&input["sphere"], "url")?,
                sphere::str_of(&input, "owner")?,
                &key.seed_hex(),
            )?;
            Ok(json!({"seed": key.seed_hex(), "public": key.public(), "keyId": key.key_id()}))
        }
        other => bail!("no node named {other}"),
    }
}

fn owner_key(input: &Value) -> Result<OwnerKey> {
    let key = &input["ownerKey"];
    let seed = key["seed"].as_str().or(key.as_str()).context("ownerKey")?;
    OwnerKey::from_hex(seed)
}

/// The proof that an assertion can fail (OBIDSPEC-6): the answer it passed on, changed at each
/// place it looks (crate::expect::mutants), must fail it every time. A mutant that passes too is
/// vacuous: that part of the assertion would pass whatever obid said.
fn red_proof(want: Option<&Value>, status: u16, body: &Value, text: &str) -> Value {
    let Some(want) = want else {
        return json!({"mutants": 0, "vacuous": []});
    };
    let all = expect::mutants(want, status, body, text);
    let vacuous: Vec<String> = all
        .iter()
        .filter(|m| answer_failures(want, m.status, &m.body, &m.text).is_empty())
        .map(|m| m.what.clone())
        .collect();
    json!({"mutants": all.len(), "vacuous": vacuous})
}

fn answer_failures(want: &Value, status: u16, body: &Value, text: &str) -> Vec<String> {
    let mut failures: Vec<String> = want
        .get("status")
        .and_then(|s| expect::check_status(s, status))
        .into_iter()
        .collect();
    failures.extend(expect::check(want, body, text));
    failures
}

/// A Sphere answer checked against the step's `expect`, when it has one.
fn answered(input: &Value, a: &http::Answer) -> Result<Value> {
    let red = red_proof(input.get("expect"), a.status, &a.body, &a.text);
    let out = json!({"status": a.status, "body": a.body, "red": red});
    if let Some(want) = input.get("expect") {
        let failures = answer_failures(want, a.status, &a.body, &a.text);
        if !failures.is_empty() {
            bail!(
                "{}: {}",
                failures.join("; "),
                a.text.chars().take(400).collect::<String>()
            );
        }
    }
    Ok(out)
}

/// `pick`: the first element of an array in the answer (`from`, a path, `$` by default) that
/// matches `where`, kept as the step's `picked` (an inbox item's id for a later step).
fn pick(input: &Value, value: &Value) -> Result<Option<Value>> {
    let Some(p) = input.get("pick") else {
        return Ok(None);
    };
    let from = p["from"].as_str().unwrap_or("$");
    let list = expect::at(value, from)
        .and_then(Value::as_array)
        .with_context(|| format!("pick: {from} isn't an array"))?;
    let found = list.iter().find(|item| {
        let mut f = vec![];
        expect::matches("pick", &p["where"], item, &mut f);
        f.is_empty()
    });
    found
        .cloned()
        .map(Some)
        .with_context(|| format!("pick: nothing in {from} matches {}", p["where"]))
}

fn device_of(v: &Value) -> Result<(String, DeviceKey)> {
    Ok((
        sphere::str_of(v, "id")?.to_string(),
        DeviceKey::from_hex(sphere::str_of(v, "key")?)?,
    ))
}

/// The request a `call` step makes, and how it's authenticated (`as`):
/// `owner` (obid's token, the default), `device` (the token and the device's `X-Oby-Words` over
/// the exact body), `none` (no Authorization), `wrong` (a token obid never gave) or `token`
/// (the step's `token`, as an agent's broker token).
async fn call(input: &Value) -> Result<Value> {
    let obid = &input["obid"];
    let method = sphere::str_of(input, "method")?.to_uppercase();
    let path = sphere::str_of(input, "path")?;
    let mut body: Option<String> = match (input.get("raw"), input.get("body")) {
        (Some(Value::String(raw)), _) => Some(raw.clone()),
        (_, Some(b)) => Some(b.to_string()),
        _ => None,
    };
    if let Some(confirm) = input.get("confirm") {
        // The owner's tap on Confirm: an approval over the item's own title, as GET /v1/inbox has it.
        let (id, key) = device_of(&confirm["device"])?;
        let item = sphere::str_of(confirm, "item")?;
        // Over the title given (an item no longer listed), else the item's own.
        let title = match confirm["title"].as_str() {
            Some(t) => t.to_string(),
            None => {
                let inbox = sphere::obid_call(obid, "GET", "/v1/inbox", None)
                    .await?
                    .body;
                let items = inbox.get("items").cloned().unwrap_or(inbox);
                items
                    .as_array()
                    .and_then(|a| a.iter().find(|i| i["id"] == item))
                    .and_then(|i| i["title"].as_str())
                    .with_context(|| format!("no inbox item {item}"))?
                    .to_string()
            }
        };
        let mut b: Value = body
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_else(|| json!({"answer": "Confirm"}));
        b["approval"] = keys::confirm_approval(&id, &key, item, &title);
        body = Some(b.to_string());
    }
    let within = input["within"].as_u64().unwrap_or(0);
    let until = Instant::now() + Duration::from_secs(within);
    loop {
        let mut request = Request {
            method: method.clone(),
            base: sphere::str_of(obid, "base")?.into(),
            target: path.into(),
            body: body.clone().map(String::into_bytes),
            ..Default::default()
        };
        let auth = input["as"].as_str().unwrap_or("owner");
        let token = match auth {
            "owner" | "device" => Some(sphere::str_of(obid, "token")?.to_string()),
            "none" => None,
            "wrong" => Some("0".repeat(64)),
            "token" => Some(sphere::str_of(input, "token")?.to_string()),
            other => bail!("as: {other}? owner, device, none, wrong or token"),
        };
        if let Some(token) = token {
            request
                .headers
                .push(("Authorization".into(), format!("Bearer {token}")));
        }
        if auth == "device" {
            let (id, key) = device_of(&input["device"])?;
            let header =
                keys::words_header(&id, &key, &method, path, body.as_deref().unwrap_or(""));
            request.headers.push(("X-Oby-Words".into(), header));
        }
        for (k, v) in input["headers"].as_object().into_iter().flatten() {
            request
                .headers
                .push((k.clone(), v.as_str().unwrap_or_default().to_string()));
        }
        let answer = http::send(&request, Duration::from_secs(90)).await?;
        let failures = input
            .get("expect")
            .map(|w| answer_failures(w, answer.status, &answer.body, &answer.text))
            .unwrap_or_default();
        let mut failures = failures;
        if failures.is_empty() {
            match pick(input, &answer.body) {
                Ok(picked) => {
                    let red = red_proof(
                        input.get("expect"),
                        answer.status,
                        &answer.body,
                        &answer.text,
                    );
                    return Ok(json!({"status": answer.status, "body": answer.body,
                        "text": answer.text.chars().take(4000).collect::<String>(), "red": red, "picked": picked}));
                }
                Err(e) => failures.push(format!("{e:#}")),
            }
        }
        if Instant::now() >= until {
            bail!(
                "{method} {path}: {}{}: {}",
                failures.join("; "),
                if within > 0 {
                    format!(" (still, after {within} s)")
                } else {
                    String::new()
                },
                answer.text.trim().chars().take(600).collect::<String>()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn home_file(input: &Value) -> Result<Value> {
    let home = std::path::PathBuf::from(sphere::str_of(&input["obid"], "home")?);
    let rel = sphere::str_of(input, "path")?;
    anyhow::ensure!(
        !rel.starts_with('/') && !rel.split('/').any(|p| p == ".."),
        "{rel} isn't a path in obid's home"
    );
    let path = home.join(rel);
    let exists = path.exists();
    let mode = obid::mode_of(&path);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let out = json!({"path": rel, "exists": exists, "mode": mode, "json": parsed,
        "text": text.chars().take(4000).collect::<String>()});
    let judge = |want: &Value| {
        let mut failures = vec![];
        if let Some(e) = want.get("exists").and_then(Value::as_bool) {
            if e != exists {
                failures.push(format!("{rel}: exists is {exists}, expected {e}"));
            }
        }
        if let Some(m) = want.get("mode") {
            if mode.as_ref().map(|s| json!(s)) != Some(m.clone()) {
                failures.push(format!("{rel}: mode {mode:?}, expected {m}"));
            }
        }
        if let Some(j) = want.get("json") {
            let j = if j.get("body").is_some() || j.get("paths").is_some() {
                j.clone()
            } else {
                json!({"body": j})
            };
            failures.extend(expect::check(&j, &parsed, &text));
        }
        if let Some(t) = want.get("text") {
            let mut f = vec![];
            expect::matches("text", t, &Value::String(text.clone()), &mut f);
            failures.extend(f);
        }
        failures
    };
    let failures = input.get("expect").map(judge).unwrap_or_default();
    if failures.is_empty() {
        let mut out = out;
        out["red"] = home_red_proof(input.get("expect"), exists, mode.as_deref(), &parsed, &text);
        Ok(out)
    } else {
        bail!("{}", failures.join("; "))
    }
}

/// A file's assertion proven able to fail: there or not flipped, another mode, its JSON and its
/// text each made wrong (OBIDSPEC-6).
fn home_red_proof(
    want: Option<&Value>,
    exists: bool,
    mode: Option<&str>,
    parsed: &Value,
    text: &str,
) -> Value {
    let Some(want) = want else {
        return json!({"mutants": 0, "vacuous": []});
    };
    let judge = |exists: bool, mode: Option<&str>, parsed: &Value, text: &str| -> bool {
        let mut ok = true;
        if let Some(e) = want.get("exists").and_then(Value::as_bool) {
            ok &= e == exists;
        }
        if let Some(m) = want.get("mode") {
            ok &= mode.map(|s| json!(s)).as_ref() == Some(m);
        }
        if let Some(j) = want.get("json") {
            let j = if j.get("body").is_some() || j.get("paths").is_some() {
                j.clone()
            } else {
                json!({"body": j})
            };
            ok &= expect::check(&j, parsed, text).is_empty();
        }
        if let Some(t) = want.get("text") {
            let mut f = vec![];
            expect::matches("text", t, &Value::String(text.to_string()), &mut f);
            ok &= f.is_empty();
        }
        ok
    };
    let mut cases: Vec<(String, bool)> = vec![];
    if want.get("exists").is_some() {
        cases.push(("exists flipped".into(), judge(!exists, mode, parsed, text)));
    }
    if want.get("mode").is_some() {
        cases.push((
            "mode 0777".into(),
            judge(exists, Some("0777"), parsed, text),
        ));
    }
    if let Some(j) = want.get("json") {
        let j = if j.get("body").is_some() || j.get("paths").is_some() {
            j.clone()
        } else {
            json!({"body": j})
        };
        for m in expect::mutants(&j, 0, parsed, text) {
            cases.push((m.what.clone(), judge(exists, mode, &m.body, text)));
        }
    }
    if let Some(t) = want.get("text") {
        for m in expect::mutants(&json!({"text": t}), 0, &Value::Null, text) {
            cases.push((m.what.clone(), judge(exists, mode, parsed, &m.text)));
        }
    }
    let vacuous: Vec<String> = cases
        .iter()
        .filter(|(_, passed)| *passed)
        .map(|(w, _)| w.clone())
        .collect();
    json!({"mutants": cases.len(), "vacuous": vacuous})
}

/// The owner's Mac app adding its device: its public key named in `home/pairing`, then
/// `POST /v1/devices` with the token.
async fn device_pair(input: &Value) -> Result<Value> {
    let obid = &input["obid"];
    let key = DeviceKey::generate();
    let home = std::path::PathBuf::from(sphere::str_of(obid, "home")?);
    obid::private_file(&home.join("pairing"), &key.public())?;
    let added = sphere::obid_call(
        obid,
        "POST",
        "/v1/devices",
        Some(&json!({"name": input["name"].as_str().unwrap_or("obid-spec's Mac"), "publicKey": key.public()})),
    )
    .await?;
    let _ = std::fs::remove_file(home.join("pairing"));
    anyhow::ensure!(
        added.status == 200,
        "the device: {} {}",
        added.status,
        added.text
    );
    Ok(json!({"id": added.body["id"], "key": key.secret_hex(), "publicKey": key.public()}))
}

fn device_sign(input: &Value) -> Result<Value> {
    let (id, key) = device_of(&input["device"])?;
    // `lines`: any of the owner's device approvals, the lines given and then the time it's signed
    // at, joined by newlines (`oby-revoke-device`, `<id>` → `oby-revoke-device\n<id>\n<signedAt>`).
    if let Some(lines) = input.get("lines").and_then(Value::as_array) {
        let at = keys::now_seconds();
        let mut text: Vec<String> = lines
            .iter()
            .map(|l| l.as_str().unwrap_or_default().to_string())
            .collect();
        text.push(at.clone());
        return Ok(
            json!({"approval": {"device": id, "signedAt": at, "signature": key.sign(&text.join("\n"))}}),
        );
    }
    if let Some(confirm) = input.get("confirm") {
        return Ok(json!({"approval": keys::confirm_approval(
            &id, &key, sphere::str_of(confirm, "id")?, sphere::str_of(confirm, "title")?)}));
    }
    let body = match input.get("body") {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    Ok(json!({"header": keys::words_header(
        &id, &key, &sphere::str_of(input, "method")?.to_uppercase(), sphere::str_of(input, "path")?, &body)}))
}
