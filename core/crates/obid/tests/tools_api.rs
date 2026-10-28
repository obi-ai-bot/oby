//! Cursor's agent set up by obid itself (docs/runner-api.md, "obid only: tools"), on the real obid
//! binary: with no `cursor-agent` on this Mac, `POST /v1/tools/cursor/install` reads a stand-in
//! of Cursor's install script for the package, downloads and unpacks it where Cursor's own
//! installer puts it (`~/.local/share/cursor-agent/versions/<version>`, `~/.local/bin/cursor-agent`),
//! `status.tools.cursor` says so, and a Cursor account then signs in through it.

mod support;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

const VERSION: &str = "2026.10.22-27dcf5f";

/// A package like Cursor's: one top folder with the stand-in `cursor-agent` (tests/agents) and its
/// login in it, as a gzipped tar. Its bytes.
fn package(folder: &Path) -> Vec<u8> {
    let agents = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents");
    let top = folder.join("package/agent-cli-package");
    std::fs::create_dir_all(&top).unwrap();
    for (from, to) in [
        ("fake-cursor", "cursor-agent"),
        ("fake-login.mjs", "fake-login.mjs"),
    ] {
        std::fs::copy(agents.join(from), top.join(to)).unwrap();
    }
    let tarball = folder.join("package.tar.gz");
    let made = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(folder.join("package"))
        .arg("agent-cli-package")
        .status()
        .unwrap();
    assert!(made.success());
    std::fs::read(&tarball).unwrap()
}

/// A stand-in for cursor.com: `/install` is a script naming the package, which is at the address
/// the script says. Its address.
fn cursor_site(package: Vec<u8>) -> String {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let script = format!(
        "#!/usr/bin/env bash\necho \"Cursor Agent Installer\"\nDOWNLOAD_URL=\"{url}/lab/{VERSION}/${{OS}}/${{ARCH}}/agent-cli-package.tar.gz\"\n"
    );
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut head = [0u8; 4096];
            let n = stream.read(&mut head).unwrap_or(0);
            let request = String::from_utf8_lossy(&head[..n]).into_owned();
            let path = request
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_owned();
            let (status, body): (&str, Vec<u8>) = if path == "/install" {
                ("200 OK", script.clone().into_bytes())
            } else if path.ends_with("/agent-cli-package.tar.gz") {
                ("200 OK", package.clone())
            } else {
                ("404 Not Found", Vec::new())
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&body);
        }
    });
    url
}

fn tools(obid: &Obid) -> Value {
    call(obid, "GET", "/v1/status", None).body["tools"]["cursor"].clone()
}

#[test]
fn cursors_agent_is_set_up_by_obid_and_signs_an_account_in() {
    let folder = Folder::new("tools-cursor");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &agent_path("fake-claude"));
    let site = cursor_site(package(&folder.0));
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_CURSOR_INSTALLER", &format!("{site}/install"))],
    );
    let events = follow_events(&obid);

    // No cursor-agent anywhere on this obid's PATH: Cursor's set-up is wanted. (A Mac with one on
    // its login shell's PATH can't run this: obid finds that one.)
    let before = tools(&obid);
    if before["state"] == "installed" {
        eprintln!("skipped: this machine has {}", before["path"]);
        return;
    }
    assert_eq!(before["state"], "missing", "{before}");
    assert_eq!(before["path"], Value::Null);
    let other = call(&obid, "POST", "/v1/tools/claude/install", None);
    assert_eq!(other.status, 404, "{}", other.text);

    let started = call(&obid, "POST", "/v1/tools/cursor/install", None);
    assert_eq!(started.status, 202, "{}", started.text);
    assert_eq!(started.body["cursor"]["state"], "installing");
    // Asked again meanwhile: a moment; once it's there, the agent as it is.
    let again = call(&obid, "POST", "/v1/tools/cursor/install", None);
    assert!(
        again.status == 409 || again.status == 200,
        "{} {}",
        again.status,
        again.text
    );
    let ended = events_named(&events, "tools", 60, |got| {
        got.iter().any(|t| t["cursor"]["state"] != "installing")
    });
    let last = ended.last().unwrap();
    assert_eq!(last["cursor"]["state"], "installed", "{last}");
    assert_eq!(last["cursor"]["version"], VERSION);
    let home = folder.0.join("env-home");
    let link = home.join(".local/bin/cursor-agent");
    assert_eq!(last["cursor"]["path"], json!(link));
    // Where Cursor's own installer puts it, so its own `update` and the person's shell see it.
    let kept = home
        .join(".local/share/cursor-agent/versions")
        .join(VERSION);
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        kept.join("cursor-agent")
    );
    assert_eq!(
        std::fs::read_link(home.join(".local/bin/agent")).unwrap(),
        kept.join("cursor-agent")
    );
    assert!(kept.join("fake-login.mjs").is_file());
    assert!(!kept.join("agent-cli-package.tar.gz").exists());
    let now = tools(&obid);
    assert_eq!(now["state"], "installed", "{now}");
    let done = call(&obid, "POST", "/v1/tools/cursor/install", None);
    assert_eq!(done.status, 200, "{}", done.text);

    // The agent obid set up signs a Cursor account in, as one on the person's PATH would.
    let added = call(
        &obid,
        "POST",
        "/v1/accounts",
        Some(&json!({"agent": "cursor", "name": "Personal"})),
    );
    assert_eq!(added.status, 201, "{}", added.text);
    let id = added.body["id"].as_str().unwrap().to_owned();
    let page = call(&obid, "GET", &format!("/v1/accounts/{id}/sign-in"), None);
    assert_eq!(page.status, 200, "{}", page.text);
    assert_eq!(
        page.body["url"],
        "https://cursor.com/loginDeepControl?challenge=fake&mode=login"
    );
    // The stand-in's login wrote its file in the account's folder, as Cursor's does (the stand-in
    // holds no token, so obid doesn't read it as signed in: tests/agents/fake-login.mjs).
    let login = obid
        .home
        .join("accounts")
        .join(&id)
        .join(if cfg!(target_os = "macos") {
            ".cursor/auth.json"
        } else {
            ".config/cursor/auth.json"
        });
    within(15, "Cursor's login was written", || {
        login.is_file().then_some(())
    });
    let saw: Value = serde_json::from_slice(&std::fs::read(&login).unwrap()).unwrap();
    assert_eq!(saw["saw"]["env"]["AGENT_CLI_CREDENTIAL_STORE"], "file");
    let path = saw["saw"]["env"]["PATH"].as_str().unwrap();
    assert!(path.contains(kept.to_str().unwrap()), "{path}");
}

/// Without Cursor's agent, a Cursor sign-in says what Connect does about it.
#[test]
fn a_cursor_sign_in_without_the_agent_says_how_it_is_set_up() {
    let folder = Folder::new("tools-cursor-missing");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &agent_path("fake-claude"));
    let obid = start(&folder.0, &config, None);
    let added = call(
        &obid,
        "POST",
        "/v1/accounts",
        Some(&json!({"agent": "cursor", "name": "Personal"})),
    );
    assert_eq!(added.status, 201, "{}", added.text);
    let id = added.body["id"].as_str().unwrap();
    let refused = call(&obid, "GET", &format!("/v1/accounts/{id}/sign-in"), None);
    assert_eq!(refused.status, 503, "{}", refused.text);
    assert!(
        refused.body["error"]
            .as_str()
            .unwrap()
            .contains("POST /v1/tools/cursor/install"),
        "{}",
        refused.text
    );
}

fn agent_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/agents/{name}"))
}
