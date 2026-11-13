//! The Swift client against obid: tests/swift-client/main.swift is built with the iPhone's and the
//! Apple Watch's own client code (apps/shared/Runner.swift and what it needs, what those apps
//! compile) and follows a running obid: `GET /v1/events`, then status, the jobs, the inbox, the
//! conversation and a job by id, read into the phones' types; Clear done; and Stop all on a job
//! that started while it followed. obid's API stays add-only for those apps: an answer they can't
//! read fails here. macOS only: swiftc comes with Xcode.
//!
//! The client is given an isolated obid home and address, and reads the private token file there,
//! so it touches no keychain and no port another obid holds.

#![cfg(target_os = "macos")]

mod support;

use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::json;
use support::*;

/// The apps' files the client is built from, under the repository root.
const APP_SOURCES: &[&str] = &[
    "apps/shared/Runner.swift",
    "apps/shared/Harness.swift",
    "apps/shared/Decisions.swift",
    "apps/shared/JobPage.swift",
    "apps/shared/JobList.swift",
    "apps/shared/Elapsed.swift",
    "apps/shared/DeviceKey.swift",
    "apps/shared/TetrixStatus.swift",
    "apps/shared/GitHubWorkModel.swift",
    "apps/shared/PhoneRelay.swift",
    "apps/shared/RemoteRoute.swift",
    "apps/shared/Usage.swift",
];

/// What the Xcode project compiles Oby's Swift with.
const SWIFT_FLAGS: [&str; 18] = [
    "-swift-version",
    "5",
    "-enable-upcoming-feature",
    "MemberImportVisibility",
    "-enable-upcoming-feature",
    "BareSlashRegexLiterals",
    "-enable-upcoming-feature",
    "DisableOutwardActorInference",
    "-enable-upcoming-feature",
    "GlobalActorIsolatedTypesUsability",
    "-enable-upcoming-feature",
    "InferIsolatedConformances",
    "-enable-upcoming-feature",
    "InferSendableFromCaptures",
    "-enable-upcoming-feature",
    "NonisolatedNonsendingByDefault",
    "-default-isolation",
    "MainActor",
];

/// Every Swift file the client is built from.
fn sources() -> Vec<PathBuf> {
    let root = obi();
    let mut files =
        vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/swift-client/main.swift")];
    files.extend(APP_SOURCES.iter().map(|f| root.join(f)));
    files
}

/// The client, built once for these sources and shared by the
/// tests in this file.
fn client() -> PathBuf {
    static BUILT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BUILT.get_or_init(build).clone()
}

fn build() -> PathBuf {
    let files = sources();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file in &files {
        file.hash(&mut hasher);
        std::fs::read(file).unwrap().hash(&mut hasher);
    }
    let built = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("swift-client-{:016x}", hasher.finish()));
    if built.exists() {
        return built;
    }
    let partial = built.with_extension("building");
    let output = Command::new("xcrun")
        .arg("swiftc")
        .args(SWIFT_FLAGS)
        .args(["-Onone", "-module-name", "ObySwiftClient", "-o"])
        .arg(&partial)
        .args(&files)
        .output()
        .expect("xcrun swiftc runs (Xcode is installed)");
    assert!(
        output.status.success(),
        "the apps' client didn't build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::rename(&partial, &built).unwrap();
    built
}

/// The client's lines, as they come.
fn lines(child: &mut std::process::Child) -> Receiver<String> {
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { return };
            eprintln!("swift client: {line}");
            if sender.send(line).is_err() {
                return;
            }
        }
    });
    receiver
}

/// The next line that starts with `prefix`, within `seconds`; the lines before it go to `seen`.
fn next(lines: &Receiver<String>, prefix: &str, seconds: u64, seen: &mut Vec<String>) -> String {
    let give_up_at = Instant::now() + Duration::from_secs(seconds);
    loop {
        let left = give_up_at.saturating_duration_since(Instant::now());
        match lines.recv_timeout(left) {
            Ok(line) if line.starts_with(prefix) => return line,
            Ok(line) => seen.push(line),
            Err(_) => panic!("no {prefix:?} line within {seconds} seconds; before it: {seen:#?}"),
        }
    }
}

fn fake_claude() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude")
}

/// CHATGPT-18: the phones as they are decode a ChatGPT account: its new keys are ignored, its `agent`
/// is still `codex`, and no enum they read as closed has a new value.
#[test]
fn chatgpt_18_older_phones_decode_the_new_keys() {
    let client = client();
    let folder = Folder::new("swift-client-chatgpt");
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &fake_claude());
    let account = folder.0.join("home/accounts/plan");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(
        account.join("chatgpt.json"),
        json!({"email": "person@example.com", "signed": true, "planUse": true,
               "expiresAt": 0, "refreshedAt": obi_protocol::time::now()})
        .to_string(),
    )
    .unwrap();
    config["agents"]["codex"] = json!({
        "bin": Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-codex"),
        "accounts": [{"id": "plan", "name": "Plan", "configDir": account}]
    });
    let obid = start(&folder.0, &config, None);
    let status = call(&obid, "GET", "/v1/status", None).body;
    let plan = status["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "plan")
        .unwrap();
    assert_eq!(plan["login"], "chatgpt", "{plan}");
    assert_eq!(plan["loginEmail"], "person@example.com", "{plan}");
    assert_eq!(plan["agent"], "codex", "{plan}");
    let mut child = Command::new(&client)
        .env("OBID_URL", format!("http://127.0.0.1:{}", obid.port))
        .env("OBID_HOME", &obid.home)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let lines = lines(&mut child);
    let mut seen = Vec::new();
    let line = next(&lines, "status ", 60, &mut seen);
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        line.contains("accounts=stub,plan") || line.contains("accounts=plan,stub"),
        "{line}"
    );
}

/// The phones' client follows obid, lists its jobs, clears and stops all, every answer read into
/// their own types.
#[test]
fn the_swift_client_follows_obid_and_lists_its_jobs() {
    let client = client();
    let folder = Folder::new("swift-client");
    let remote = fixture(&folder.0.join("fixture"));
    let obid = start(&folder.0, &config(&folder.0, &remote, &fake_claude()), None);
    let made = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "investigate",
                             "task": "Look around", "title": "Job progress screen"}}),
        ),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    let id = made.body["id"].as_str().unwrap().to_owned();
    let done = call(&obid, "GET", &format!("/v1/jobs/{id}/wait"), None).body;
    assert_eq!(done["status"], "done");

    // obid has measured its worktrees (it may answer before), so the client's status has `cleanup`.
    let deadline = Instant::now() + Duration::from_secs(10);
    while call(&obid, "GET", "/v1/status", None)
        .body
        .get("cleanup")
        .is_none()
    {
        assert!(
            Instant::now() < deadline,
            "obid never measured its worktrees"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let mut child = Command::new(&client)
        .env("OBID_URL", format!("http://127.0.0.1:{}", obid.port))
        .env("OBID_HOME", &obid.home)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let lines = lines(&mut child);
    let mut seen = Vec::new();
    let host = gethostname();
    assert_eq!(
        next(&lines, "status ", 60, &mut seen),
        format!(
            "status host={host} version=0.2.0 accounts=stub clearedAt=null mode=sandbox cleanup=true"
        )
    );
    assert_eq!(
        next(&lines, "jobs ", 30, &mut seen),
        format!("jobs {id}:done:Job progress screen")
    );
    assert_eq!(next(&lines, "inbox ", 30, &mut seen), "inbox 0");
    // The latest conversation is the job's own (a task started on its own has one, TASKCONV-1):
    // the owner's words, Oby's line saying it started, and how it ended.
    assert_eq!(
        next(&lines, "conversation ", 30, &mut seen),
        "conversation 3"
    );
    assert_eq!(next(&lines, "carded ", 30, &mut seen), "carded true oby");
    assert_eq!(
        next(&lines, "job ", 30, &mut seen),
        format!("job {id} done heading=Job progress screen")
    );
    // The device's approval key, registered with pairing naming it.
    assert_eq!(next(&lines, "devices ", 30, &mut seen), "devices 200");
    assert!(
        !obid.home.join("pairing").exists(),
        "the device used pairing up"
    );
    assert!(
        std::fs::read_to_string(obid.home.join("devices.json"))
            .unwrap()
            .contains("\"Test iPhone\"")
    );
    let cleared = next(&lines, "cleared ", 30, &mut seen);
    let time = cleared.strip_prefix("cleared ").unwrap().to_owned();
    assert_eq!(
        call(&obid, "GET", "/v1/status", None).body["clearedAt"],
        time.as_str()
    );
    next(&lines, "ready", 30, &mut seen);
    assert_eq!(
        next(&lines, "event clear ", 30, &mut seen),
        format!("event clear {time}")
    );

    // A job that starts while the client follows: it hears it run, stops all, and hears it stop.
    let sleeper = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "investigate", "task": "SLEEP"}}),
        ),
    );
    assert_eq!(sleeper.status, 201, "{}", sleeper.text);
    let sleeper = sleeper.body["id"].as_str().unwrap().to_owned();
    assert_eq!(
        next(
            &lines,
            &format!("event job {sleeper} running"),
            60,
            &mut seen
        ),
        format!("event job {sleeper} running")
    );
    assert_eq!(
        next(&lines, "stop-all ", 30, &mut seen),
        format!("stop-all 1 {sleeper}")
    );
    next(
        &lines,
        &format!("event job {sleeper} stopped"),
        30,
        &mut seen,
    );
    next(&lines, "done", 30, &mut seen);
    let status = child.wait().unwrap();
    assert!(status.success(), "{status}");
    assert_eq!(
        call(&obid, "GET", &format!("/v1/jobs/{sleeper}"), None).body["status"],
        "stopped"
    );
}

fn gethostname() -> String {
    let output = Command::new("hostname").output().unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
