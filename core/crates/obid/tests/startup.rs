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
