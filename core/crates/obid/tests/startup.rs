//! `obid::serve` while it starts: a stop that comes while obid measures `home/worktrees` stops it
//! there, without its saying it listens (docs/runner-api.md, "obid only: configuration": SIGTERM
//! or SIGINT exits 0 within 10 seconds).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use obid::config::Config;

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Folder {
        let path = std::env::temp_dir().join(format!("obid-startup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Folder(path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config(folder: &Path) -> Config {
    let text = serde_json::json!({
        "listen": "127.0.0.1:0",
        "home": folder.join("home"),
        "owner": "wellington",
        // Set, so obid never asks the person's real Keychain whether to turn it on (in process,
        // these tests have no stand-in `security`, unlike support::start).
        "claudeKeychain": false,
    })
    .to_string();
    Config::parse(&text).unwrap()
}

/// Worktrees enough that measuring them isn't instant.
fn fill_worktrees(home: &Path, folders: usize, files: usize) {
    for d in 0..folders {
        let dir = home.join("worktrees").join(format!("wt-{d}"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..files {
            std::fs::write(dir.join(format!("f{f}")), b"x").unwrap();
        }
    }
}

/// A stop that is there before obid has measured `home/worktrees` ends `serve` with Ok, and obid
/// never says it listens. (Before, obid finished the walk, said it listened, and only then
/// stopped: on a large worktrees folder, past the 10 seconds.)
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_before_obid_is_ready_stops_it_without_saying_it_listens() {
    let folder = Folder::new("stop-first");
    fill_worktrees(&folder.0.join("home"), 20, 100);
    let said_ready = Arc::new(AtomicBool::new(false));
    let told = said_ready.clone();
    let result = obid::serve(
        config(&folder.0),
        "t".repeat(64),
        std::future::ready(()),
        move |_| told.store(true, Ordering::SeqCst),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert!(
        !said_ready.load(Ordering::SeqCst),
        "obid said it listens after it was told to stop"
    );
}

/// A stop that comes while obid measures ends `serve` about then, not once the walk is done:
/// the walk here takes far longer than the time `serve` is given to return.
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_while_obid_measures_ends_the_walk() {
    let folder = Folder::new("stop-while-measuring");
    let home = folder.0.join("home");
    fill_worktrees(&home, 50, 400);
    // How long one whole measurement of this folder takes on this machine.
    let started = Instant::now();
    let _ = obid::api::Cleanup::measure(&home);
    let walk = started.elapsed();

    let said_ready = Arc::new(AtomicBool::new(false));
    let told = said_ready.clone();
    let started = Instant::now();
    let result = obid::serve(
        config(&folder.0),
        "t".repeat(64),
        tokio::time::sleep(Duration::from_millis(1)),
        move |_| told.store(true, Ordering::SeqCst),
    )
    .await;
    let took = started.elapsed();
    assert!(result.is_ok(), "{result:?}");
    if !said_ready.load(Ordering::SeqCst) {
        // The stop came first, as it does unless the walk is quicker than a millisecond: serve
        // returned without waiting for the rest of it.
        assert!(
            took < walk + Duration::from_secs(1),
            "serve took {took:?} after a stop; one walk takes {walk:?}"
        );
    }
}

/// obid answers as soon as it listens: `GET /v1/status` says it's starting (`503`, `starting`)
/// while the slow steps run, a call made then waits for the API rather than fail, and once
/// obid is ready status answers in full.
#[tokio::test(flavor = "multi_thread")]
async fn status_answers_before_the_slow_steps_finish() {
    let folder = Folder::new("answers-first");
    let home = folder.0.join("home");
    // A walk that takes the whole MEASURE_FIRST: obid isn't ready for at least that long.
    fill_worktrees(&home, 50, 400);
    let port = free_port();
    let mut config = config(&folder.0);
    config.listen = format!("127.0.0.1:{port}").parse().unwrap();
    let token = "t".repeat(64);
    let said_ready = Arc::new(AtomicBool::new(false));
    let told = said_ready.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(obid::serve(
        config,
        token.clone(),
        async move {
            let _ = stopped.await;
        },
        move |_| told.store(true, Ordering::SeqCst),
    ));
    let client = reqwest::Client::new();
    let url = |path: &str| format!("http://127.0.0.1:{port}{path}");
    let status = loop {
        if let Ok(answer) = client
            .get(url("/v1/status"))
            .bearer_auth(&token)
            .send()
            .await
        {
            break answer;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    };
    assert!(
        !said_ready.load(Ordering::SeqCst),
        "obid was ready before its first answer"
    );
    assert_eq!(status.status(), 503);
    assert_eq!(status.headers()["retry-after"], "1");
    let body: serde_json::Value = status.json().await.unwrap();
    assert_eq!(body["error"], "Oby’s engine is starting.");
    assert_eq!(body["starting"]["done"][0]["step"], "listen");
    assert!(body["starting"]["now"].is_string(), "{body}");
    // Without the token, the door says no as the API does.
    let refused = client.get(url("/v1/status")).send().await.unwrap();
    assert_eq!(refused.status(), 401);
    // A call made while obid starts waits for it.
    let jobs = client
        .get(url("/v1/jobs"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(jobs.status(), 200);
    let status = client
        .get(url("/v1/status"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    let _ = stop.send(());
    assert!(serving.await.unwrap().is_ok());
}

/// A free port on loopback, for a config whose address a test must know before obid is ready.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Waits for obid's first answer to `GET /v1/status`, whatever it is.
async fn first_status(client: &reqwest::Client, port: u16, token: &str) -> reqwest::Response {
    loop {
        if let Ok(answer) = client
            .get(format!("http://127.0.0.1:{port}/v1/status"))
            .bearer_auth(token)
            .send()
            .await
        {
            return answer;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// While obid starts, a call its gate would refuse is refused at once, never held: no token is
/// `401`, a name other than loopback's `403`.
#[tokio::test(flavor = "multi_thread")]
async fn the_door_refuses_before_anything_waits() {
    let folder = Folder::new("door-refuses");
    fill_worktrees(&folder.0.join("home"), 50, 400);
    let port = free_port();
    let mut config = config(&folder.0);
    config.listen = format!("127.0.0.1:{port}").parse().unwrap();
    let token = "t".repeat(64);
    let said_ready = Arc::new(AtomicBool::new(false));
    let told = said_ready.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(obid::serve(
        config,
        token.clone(),
        async move {
            let _ = stopped.await;
        },
        move |_| told.store(true, Ordering::SeqCst),
    ));
    let client = reqwest::Client::new();
    assert_eq!(first_status(&client, port, &token).await.status(), 503);
    let jobs = format!("http://127.0.0.1:{port}/v1/jobs");
    let tokenless = client.get(&jobs).send().await.unwrap();
    let elsewhere = client
        .get(&jobs)
        .bearer_auth(&token)
        .header("host", "rebound.example")
        .send()
        .await
        .unwrap();
    assert!(
        !said_ready.load(Ordering::SeqCst),
        "obid was ready before the door answered"
    );
    assert_eq!(tokenless.status(), 401);
    assert_eq!(tokenless.headers()["cache-control"], "no-store");
    assert_eq!(elsewhere.status(), 403);
    let _ = stop.send(());
    assert!(serving.await.unwrap().is_ok());
}

/// A start that fails after obid listens answers the calls it held `503` with why, and `serve`
/// returns the failure.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_start_answers_what_the_door_held() {
    let folder = Folder::new("door-fails");
    let home = folder.0.join("home");
    fill_worktrees(&home, 50, 400);
    // The jobs' folder can't be made: Jobs::open fails.
    std::fs::write(home.join("jobs"), b"not a folder").unwrap();
    let port = free_port();
    let mut config = config(&folder.0);
    config.listen = format!("127.0.0.1:{port}").parse().unwrap();
    let token = "t".repeat(64);
    let serving = tokio::spawn(obid::serve(
        config,
        token.clone(),
        std::future::pending(),
        |_| {},
    ));
    let client = reqwest::Client::new();
    assert_eq!(first_status(&client, port, &token).await.status(), 503);
    let held = client
        .get(format!("http://127.0.0.1:{port}/v1/jobs"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(held.status(), 503);
    let body: serde_json::Value = held.json().await.unwrap();
    assert_eq!(body["error"], "Oby’s engine didn’t start.");
    assert!(matches!(
        serving.await.unwrap(),
        Err(obid::StartError::Jobs { .. })
    ));
}

/// An agent's own routes (its MCP server, Tetrix's) wait for the API while obid starts, rather
/// than be refused: a job resumed at the start may call them before obid opens, and the API's
/// gate checks their tokens then.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_route_waits_for_the_api() {
    let folder = Folder::new("door-agent");
    fill_worktrees(&folder.0.join("home"), 50, 400);
    let port = free_port();
    let mut config = config(&folder.0);
    config.listen = format!("127.0.0.1:{port}").parse().unwrap();
    let token = "t".repeat(64);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(obid::serve(
        config,
        token.clone(),
        async move {
            let _ = stopped.await;
        },
        |_| {},
    ));
    let client = reqwest::Client::new();
    assert_eq!(first_status(&client, port, &token).await.status(), 503);
    // What the gate wouldn't take to an agent's token is refused at once, as any other call: no
    // Bearer token, a name other than loopback's, an id of more than one part.
    let mcp = format!("http://127.0.0.1:{port}/v1/agent/mcp/job-1");
    let tokenless = client.post(&mcp).send().await.unwrap();
    let elsewhere = client
        .post(&mcp)
        .bearer_auth("not-a-broker-token")
        .header("host", "rebound.example")
        .send()
        .await
        .unwrap();
    let deeper = client
        .post(format!("{mcp}/more"))
        .bearer_auth("not-a-broker-token")
        .send()
        .await
        .unwrap();
    assert_eq!(first_status(&client, port, &token).await.status(), 503);
    assert_eq!(tokenless.status(), 401);
    assert_eq!(elsewhere.status(), 403);
    assert_eq!(deeper.status(), 401);
    let agent = client
        .post(format!("http://127.0.0.1:{port}/v1/agent/mcp/job-1"))
        .bearer_auth("not-a-broker-token")
        .send()
        .await
        .unwrap();
    // Answered by the API's gate, so only once obid is open.
    assert_eq!(agent.status(), 401);
    assert_eq!(first_status(&client, port, &token).await.status(), 200);
    let _ = stop.send(());
    assert!(serving.await.unwrap().is_ok());
}

/// `home/schema.json`, read as JSON.
fn schema(home: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(home.join("schema.json")).unwrap()).unwrap()
}

/// HOME-5: a home with no `schema.json` gets this obid's format and version (0600) at start.
#[tokio::test(flavor = "multi_thread")]
async fn home_5_a_new_home_gets_a_format() {
    let folder = Folder::new("home-5");
    let config = config(&folder.0);
    let home = config.home.clone();
    assert!(!home.exists());
    let result = obid::serve(config, "t".repeat(64), std::future::ready(()), |_| {}).await;
    assert!(result.is_ok(), "{result:?}");
    let written = schema(&home);
    assert_eq!(written["format"], 1, "{written}");
    assert_eq!(written["writtenBy"], env!("CARGO_PKG_VERSION"), "{written}");
    assert_eq!(written.as_object().unwrap().len(), 2, "{written}");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(home.join("schema.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

/// Every path under `dir`, `dir` itself included and `last-start-error` left out: its bytes (a
/// file's), modification time and mode.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime, u32)> {
    use std::os::unix::fs::PermissionsExt;
    let mut all = Vec::new();
    let mut left = vec![dir.to_path_buf()];
    while let Some(path) = left.pop() {
        if path
            .file_name()
            .is_some_and(|name| name == "last-start-error")
        {
            continue;
        }
        let meta = std::fs::symlink_metadata(&path).unwrap();
        let bytes = if meta.is_file() {
            std::fs::read(&path).unwrap()
        } else {
            Vec::new()
        };
        if meta.is_dir() {
            left.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
        }
        let mode = meta.permissions().mode();
        all.push((path, bytes, meta.modified().unwrap(), mode));
    }
    all.sort();
    all
}

/// HOME-6: a home a newer obid wrote, or whose `schema.json` obid can't read, stops obid with
/// exit 3 before it changes anything in the home but `last-start-error`, which names the version.
/// `listen` is held here, so an obid that went on would fail at bind rather than serve.
#[test]
fn home_6_a_newer_home_stops_obid() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new("home-6");
    let home = folder.0.join("home");
    std::fs::create_dir_all(home.join("jobs")).unwrap();
    std::fs::write(home.join("jobs/job-1.json"), r#"{"id":"job-1"}"#).unwrap();
    std::fs::write(home.join("api-token"), format!("{}\n", "t".repeat(64))).unwrap();
    std::fs::set_permissions(home.join("api-token"), PermissionsExt::from_mode(0o644)).unwrap();
    std::fs::write(home.join("last-start-error"), "an older start\n").unwrap();
    std::fs::set_permissions(&home, PermissionsExt::from_mode(0o755)).unwrap();
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let config_path = folder.0.join("config.json");
    let config = serde_json::json!({
        "listen": held.local_addr().unwrap().to_string(),
        "home": home,
        "owner": "wellington",
        "claudeKeychain": false,
    });
    std::fs::write(&config_path, config.to_string()).unwrap();
    std::fs::create_dir_all(folder.0.join("env-home")).unwrap();
    for (written, says) in [
        (r#"{"format": 2, "writtenBy": "9.9.9"}"#, "9.9.9"),
        ("not json", "can't read"),
    ] {
        std::fs::write(home.join("schema.json"), written).unwrap();
        let before = snapshot(&home);
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_obid"))
            .args(["serve", "--config"])
            .arg(&config_path)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", folder.0.join("env-home"))
            .env("TMPDIR", &folder.0)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(3), "{written}: {stderr}");
        assert!(stderr.contains(says), "{written}: {stderr}");
        let reason = std::fs::read_to_string(home.join("last-start-error")).unwrap();
        assert!(reason.contains(says), "{written}: {reason}");
        assert!(reason.contains("schema.json"), "{written}: {reason}");
        assert_eq!(snapshot(&home), before, "{written}: the home changed");
        assert_eq!(
            std::fs::read(home.join("schema.json")).unwrap(),
            written.as_bytes()
        );
    }
}

/// HOME-7: a home an older obid wrote is raised to this obid's format, and obid goes on.
#[tokio::test(flavor = "multi_thread")]
async fn home_7_an_older_home_is_raised() {
    let folder = Folder::new("home-7");
    let config = config(&folder.0);
    let home = config.home.clone();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("schema.json"),
        r#"{"format": 0, "writtenBy": "0.0.0"}"#,
    )
    .unwrap();
    let result = obid::serve(config, "t".repeat(64), std::future::ready(()), |_| {}).await;
    assert!(result.is_ok(), "{result:?}");
    let raised = schema(&home);
    assert_eq!(raised["format"], 1, "{raised}");
    assert_eq!(raised["writtenBy"], env!("CARGO_PKG_VERSION"), "{raised}");
}
