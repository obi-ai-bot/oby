//! The local Sphere stand-in, as e2e/rig.mjs runs it: obi-sphere's own server at the commit
//! `e2e/rig-sphere.ref` pins, on this Mac, from a clone of the suite's own. One per run, shared by
//! every workflow, each of which makes its own Obysphere on it (its own id, admin, policy and
//! links), so no two workflows ever see each other's members.
//!
//! And the owner's side of joining (the Electron app's shared/sphere.ts `join` and
//! `keepMembership`): what the owner key signs, sent through obid's own routes.

use crate::http::{self, Request};
use crate::keys::OwnerKey;
use crate::obid::private_dir;
use crate::procs;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

pub const SPHERE_REMOTE: &str = "https://github.com/obi-ai-bot/obi-sphere.git";
/// What lets every member ask every other a question or a review, and hand it a task: the
/// Electron app's members rule (shared/policy.ts, `starterRule`).
pub fn members_rule() -> Value {
    json!({"id": "members-ask", "from_owner": "*", "to_owner": "*",
           "actions": ["question", "review_request", "handoff"], "repos": ["*"]})
}
pub const SCOPES: &str =
    "send:review_request send:question send:handoff reply receive directory:read audit:read";
pub const ACCEPTS: [&str; 3] = ["review_request", "question", "handoff"];

/// A clone of obi-sphere at `pin`, its server's packages installed: `OBID_SPEC_SPHERE_CLONE`
/// when set (a folder with `server/scripts/local.mjs`), else the suite's own under the user's
/// Caches. Its marker is written last, so a clone cut short is made again.
pub fn clone_at(pin: &str) -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("OBID_SPEC_SPHERE_CLONE") {
        let dir = PathBuf::from(dir);
        anyhow::ensure!(
            dir.join("server/scripts/local.mjs").is_file(),
            "OBID_SPEC_SPHERE_CLONE has no server/scripts/local.mjs"
        );
        return Ok(dir);
    }
    let home = std::env::var_os("HOME").context("no HOME")?;
    let root = PathBuf::from(home).join("Library/Caches/obid-spec");
    let clone = root.join(format!("obi-sphere-{}", &pin[..pin.len().min(12)]));
    let ready = clone.join(".obid-spec-ready");
    if std::fs::read_to_string(&ready).is_ok_and(|s| s.trim() == pin) {
        return Ok(clone);
    }
    let _ = std::fs::remove_dir_all(&clone);
    std::fs::create_dir_all(&clone)?;
    eprintln!(
        "obid-spec: cloning obi-sphere at {} into {}",
        &pin[..7],
        clone.display()
    );
    let run = |dir: &Path, cmd: &str, args: &[&str]| -> Result<()> {
        let status = std::process::Command::new(cmd)
            .current_dir(dir)
            .args(args)
            .stdout(Stdio::null())
            .status()
            .with_context(|| format!("{cmd} didn't start"))?;
        anyhow::ensure!(status.success(), "{cmd} {args:?} failed");
        Ok(())
    };
    run(&clone, "git", &["init", "-q"])?;
    run(
        &clone,
        "git",
        &["fetch", "-q", "--depth", "1", SPHERE_REMOTE, pin],
    )?;
    run(
        &clone,
        "git",
        &[
            "-c",
            "advice.detachedHead=false",
            "checkout",
            "-q",
            "FETCH_HEAD",
        ],
    )?;
    run(
        &clone.join("server"),
        "pnpm",
        &["install", "--frozen-lockfile", "--silent"],
    )?;
    std::fs::write(&ready, format!("{pin}\n"))?;
    Ok(clone)
}

pub fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// The Sphere started: its URL, operator token and PID.
#[derive(Clone, Debug)]
pub struct Served {
    pub url: String,
    pub operator: String,
    pub pid: u32,
}

/// Starts obi-sphere's `scripts/local.mjs` on a free port, its state under `state`, and waits for
/// its health (120 s: wrangler's first start is slow).
pub async fn serve(root: &Path, state: &Path) -> Result<Served> {
    let pin = std::fs::read_to_string(root.join("e2e/rig-sphere.ref"))?
        .trim()
        .to_string();
    let clone = clone_at(&pin)?;
    for d in ["state", "home", "tmp"] {
        private_dir(&state.join(d))?;
    }
    let port = free_port()?;
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    let operator = crate::keys::b64url(&bytes);
    let node = crate::obid::which("node").context("node isn't on PATH")?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state.join("sphere.log"))?;
    let child = tokio::process::Command::new(&node)
        .current_dir(clone.join("server"))
        .args([
            "scripts/local.mjs",
            "--port",
            &port.to_string(),
            "--persist-to",
        ])
        .arg(state.join("state"))
        .env_clear()
        .env("SPHERE_OPERATOR_TOKEN", &operator)
        .env("HOME", state.join("home"))
        .env("TMPDIR", state.join("tmp"))
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("WRANGLER_SEND_METRICS", "false")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0)
        .spawn()
        .context("the Sphere didn't start")?;
    let pid = child.id().context("the Sphere ended at once")?;
    procs::remember(pid, "sphere");
    drop(child);
    let url = format!("http://127.0.0.1:{port}");
    let until = Instant::now() + Duration::from_secs(120);
    loop {
        let health = call(&url, "GET", "/v0.1/health", None, None).await;
        if health.as_ref().is_ok_and(|a| a.status == 200) {
            break;
        }
        if Instant::now() > until || !procs::alive(pid) {
            procs::stop(pid, Duration::from_secs(5)).await;
            bail!(
                "the Sphere didn't answer its health within 120 s (its log: {})",
                state.join("sphere.log").display()
            );
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(Served { url, operator, pid })
}

pub enum Body {
    Json(Value),
    Form(Vec<(String, String)>),
}

pub async fn call(
    url: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Body>,
) -> Result<http::Answer> {
    let mut request = Request {
        method: method.into(),
        base: url.into(),
        target: path.into(),
        ..Default::default()
    };
    if let Some(token) = token {
        request
            .headers
            .push(("Authorization".into(), format!("Bearer {token}")));
    }
    match body {
        Some(Body::Json(v)) => request.body = Some(v.to_string().into_bytes()),
        Some(Body::Form(pairs)) => {
            let text = pairs
                .iter()
                .map(|(k, v)| format!("{}={}", form(k), form(v)))
                .collect::<Vec<_>>()
                .join("&");
            request.body = Some(text.into_bytes());
            request.content_type = Some("application/x-www-form-urlencoded".into());
        }
        None => {}
    }
    http::send(&request, Duration::from_secs(60)).await
}

fn form(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// An owner's token on the Sphere from a 5-minute assertion of theirs (identity.md 4.3).
pub async fn owner_token(
    url: &str,
    key: &OwnerKey,
    owner: &str,
    sphere: &str,
    scope: &str,
) -> Result<String> {
    let assertion = key.assertion(owner, sphere, scope, None, 300);
    let answer = call(
        url,
        "POST",
        "/v0.1/oauth/token",
        None,
        Some(Body::Form(vec![
            (
                "grant_type".into(),
                "urn:ietf:params:oauth:grant-type:token-exchange".into(),
            ),
            (
                "subject_token_type".into(),
                "urn:ietf:params:oauth:token-type:jwt".into(),
            ),
            ("subject_token".into(), assertion),
        ])),
    )
    .await?;
    answer.body["access_token"]
        .as_str()
        .map(str::to_string)
        .with_context(|| format!("{owner}'s token: {} {}", answer.status, answer.text))
}

/// A new Obysphere on the run's Sphere: its admin (owner `admin`), the members rule, and one open
/// link. Answers what later steps need: `{url, sphere, adminKey, invitation: {id, secret}, link}`.
pub async fn make(url: &str, operator: &str, rules: Option<Value>) -> Result<Value> {
    let sphere = format!("spec-{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let admin = OwnerKey::generate();
    let made = call(
        url,
        "POST",
        "/v0.1/spheres",
        Some(operator),
        Some(Body::Json(json!({"sphere": sphere, "name": "obid-spec",
            "admin": {"owner": "admin", "owner_key": admin.public()}}))),
    )
    .await?;
    anyhow::ensure!(
        made.status == 201,
        "the Obysphere: {} {}",
        made.status,
        made.text
    );
    let token = owner_token(
        url,
        &admin,
        "admin",
        &sphere,
        "admin directory:read audit:read",
    )
    .await?;
    let invitation = invite_with(url, &sphere, &token).await?;
    let mut out = json!({"url": url, "sphere": sphere, "adminKey": admin.seed_hex(),
        "invitation": invitation});
    out["link"] = json!(format!(
        "{url}/join#sphere={sphere}&name=obid-spec&by=admin&id={}&secret={}",
        invitation["id"].as_str().unwrap_or(""),
        invitation["secret"].as_str().unwrap_or("")
    ));
    // The members rule only once its members joined: a policy names only owners who have, but `*`
    // names nobody.
    set_policy(&out, rules.unwrap_or_else(|| json!([members_rule()]))).await?;
    Ok(out)
}

async fn invite_with(url: &str, sphere: &str, token: &str) -> Result<Value> {
    let answer = call(
        url,
        "POST",
        &format!("/v0.1/spheres/{sphere}/invitations"),
        Some(token),
        Some(Body::Json(json!({"expires_in": 2_592_000, "max_uses": 20}))),
    )
    .await?;
    anyhow::ensure!(
        answer.status == 201,
        "an invitation: {} {}",
        answer.status,
        answer.text
    );
    Ok(json!({"id": answer.body["invitation_id"], "secret": answer.body["secret"]}))
}

pub async fn admin_token(made: &Value) -> Result<String> {
    let (url, sphere) = (str_of(made, "url")?, str_of(made, "sphere")?);
    let admin = OwnerKey::from_hex(str_of(made, "adminKey")?)?;
    owner_token(
        url,
        &admin,
        "admin",
        sphere,
        "admin directory:read audit:read",
    )
    .await
}

pub async fn invite(made: &Value) -> Result<Value> {
    let token = admin_token(made).await?;
    invite_with(str_of(made, "url")?, str_of(made, "sphere")?, &token).await
}

pub async fn set_policy(made: &Value, rules: Value) -> Result<Value> {
    let token = admin_token(made).await?;
    let (url, sphere) = (str_of(made, "url")?, str_of(made, "sphere")?);
    let now = call(
        url,
        "GET",
        &format!("/v0.1/spheres/{sphere}/policy"),
        Some(&token),
        None,
    )
    .await?;
    let version = now.body["version"].clone();
    let put = call(
        url,
        "PUT",
        &format!("/v0.1/spheres/{sphere}/policy"),
        Some(&token),
        Some(Body::Json(
            json!({"expected_version": version, "rules": rules}),
        )),
    )
    .await?;
    anyhow::ensure!(put.status == 200, "the policy: {} {}", put.status, put.text);
    Ok(put.body)
}

/// A call on the Sphere's own API as its admin (a member's revoke, the directory as the Sphere
/// lists it).
pub async fn as_admin(
    made: &Value,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<http::Answer> {
    let token = admin_token(made).await?;
    call(
        str_of(made, "url")?,
        method,
        path,
        Some(&token),
        body.map(Body::Json),
    )
    .await
}

pub fn str_of<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .with_context(|| format!("no {key} in {}", short(v)))
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    s.chars().take(200).collect()
}

/// One call to obid with its token: the owner's app calling its own obid.
pub async fn obid_call(
    obid: &Value,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<http::Answer> {
    let mut request = Request {
        method: method.into(),
        base: str_of(obid, "base")?.into(),
        target: path.into(),
        headers: vec![(
            "Authorization".into(),
            format!("Bearer {}", str_of(obid, "token")?),
        )],
        ..Default::default()
    };
    request.body = body.map(|b| b.to_string().into_bytes());
    http::send(&request, Duration::from_secs(90)).await
}

async fn must(obid: &Value, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
    let a = obid_call(obid, method, path, body).await?;
    if !(200..300).contains(&a.status) {
        bail!("{method} {path}: {} {}", a.status, a.text.trim());
    }
    Ok(a.body)
}

/// The Electron app's join (shared/sphere.ts `join`, SPHERE-1): obid's key, its card signed by the
/// owner key, a 30-day delegation to its Oby, then obid redeems the invitation.
pub async fn join(
    obid: &Value,
    made: &Value,
    key: &OwnerKey,
    obi: Option<&str>,
    name: Option<&str>,
    accepts: Option<Vec<String>>,
    invitation: Option<&Value>,
) -> Result<Value> {
    let now = must(obid, "GET", "/v1/obid/sphere", None).await?;
    let owner = str_of(&now, "owner")?.to_string();
    must(obid, "POST", "/v1/obid/sphere/key", None).await?;
    let obi = obi
        .map(str::to_string)
        .unwrap_or_else(|| format!("obi-{}", uuid::Uuid::new_v4()));
    let accepts = accepts.unwrap_or_else(|| ACCEPTS.iter().map(|s| s.to_string()).collect());
    let (url, sphere) = (str_of(made, "url")?, str_of(made, "sphere")?);
    let card = must(
        obid,
        "POST",
        "/v1/obid/sphere/card",
        Some(&json!({"url": url, "sphere": sphere, "obiId": obi,
            "name": name.map(str::to_string).unwrap_or(format!("{owner}'s Oby")),
            "ownerKey": key.public(), "accepts": accepts})),
    )
    .await?;
    let invitation = invitation.unwrap_or(&made["invitation"]);
    let joined = must(
        obid,
        "POST",
        "/v1/obid/sphere/join",
        Some(&json!({"url": url, "sphere": sphere,
            "invitation": {"id": invitation["id"], "secret": invitation["secret"]},
            "card": key.sign_card(&card["card"])?,
            "delegation": key.assertion(&owner, sphere, SCOPES, Some(&obi), 2_592_000)})),
    )
    .await?;
    Ok(joined)
}

/// What keeps a member's words sealed (shared/sphere.ts `keepMembership`, SPHERE-6): when obid
/// says `card_needed`, a new card with the same accepts, signed by the owner key. Answers what was
/// signed.
pub async fn keep(obid: &Value, key: &OwnerKey) -> Result<Vec<String>> {
    let m = must(obid, "GET", "/v1/obid/sphere", None).await?;
    if m["state"] != "joined" || m["sealing"] != "card_needed" {
        return Ok(vec![]);
    }
    let card = must(
        obid,
        "POST",
        "/v1/obid/sphere/card",
        Some(
            &json!({"url": m["url"], "sphere": m["sphere"], "obiId": m["obiId"],
            "name": m["card"]["name"], "ownerKey": key.public(), "accepts": m["accepts"]}),
        ),
    )
    .await?;
    must(
        obid,
        "PUT",
        "/v1/obid/sphere/membership",
        Some(&json!({"card": key.sign_card(&card["card"])?})),
    )
    .await?;
    Ok(vec!["card".into()])
}

/// Every member kept until each says `sealing` `on`, as the rig's `joinBoth` waits (120 s).
pub async fn seal_all(members: &[(Value, OwnerKey)], seconds: u64) -> Result<Value> {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        let mut states = vec![];
        for (obid, key) in members {
            keep(obid, key).await?;
            let m = must(obid, "GET", "/v1/obid/sphere", None).await?;
            states.push(m["sealing"].clone());
        }
        if states.iter().all(|s| s == "on") {
            return Ok(json!({"sealing": states}));
        }
        if Instant::now() > until {
            bail!("sealing isn't on everywhere within {seconds} s: {states:?}");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
