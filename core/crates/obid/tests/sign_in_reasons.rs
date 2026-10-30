//! Why a sign-in didn't start, as a stable reason (docs/contracts/sign-in.md, SIGN-IN-1 to
//! SIGN-IN-9), on the real obid binary: every refusal of the agents', GitHub's and Tetrix's
//! sign-in routes carries `reason`, a word the app shows its own sentence for, beside `error`.

mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

fn agent(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/agents/{name}"))
}

/// A script at `folder/bin/<name>` that prints nothing and ends at once.
fn quiet_bin(folder: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = folder.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let path = bin.join(name);
    std::fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// The tests' configuration with the stand-in agents (`claudeKeychain` on, on macOS, as
/// accounts_api's).
fn agents_config(folder: &Path) -> Value {
    let remote = folder.join("remote.git");
    let mut config = config(folder, &remote, &agent("fake-claude"));
    config["agents"]["codex"] = json!({"bin": agent("fake-codex"), "accounts": []});
    config["agents"]["cursor"] = json!({"bin": agent("fake-cursor"), "accounts": []});
    if cfg!(target_os = "macos") {
        config["claudeKeychain"] = json!(true);
    }
    config
}

fn added(obid: &Obid, agent: &str) -> String {
    let added = call(
        obid,
        "POST",
        "/v1/accounts",
        Some(&json!({"agent": agent, "name": "Personal"})),
    );
    assert_eq!(added.status, 201, "{}", added.text);
    added.body["id"].as_str().unwrap().to_owned()
}

fn refused(answer: &Answer, status: u16, reason: &str) {
    assert_eq!(answer.status, status, "{}", answer.text);
    assert_eq!(answer.body["reason"], reason, "{}", answer.text);
    assert!(answer.body["error"].is_string(), "{}", answer.text);
}

#[test]
fn sign_in_1_a_tool_that_isnt_on_this_mac() {
    let folder = Folder::new("reasons-missing");
    let mut config = agents_config(&folder.0);
    config["agents"]["cursor"]["bin"] = json!(folder.0.join("nowhere/cursor-agent"));
    let obid = start(&folder.0, &config, None);
    let id = added(&obid, "cursor");
    let answer = call(&obid, "GET", &format!("/v1/accounts/{id}/sign-in"), None);
    refused(&answer, 503, "notInstalled");
}

#[test]
fn sign_in_2_the_tools_own_login_is_signed_in_by_the_tool() {
    let folder = Folder::new("reasons-own");
    let mut config = agents_config(&folder.0);
    config["agents"]["claude"]["accounts"] =
        json!([{"id": "default", "name": "Default", "configDir": null}]);
    let obid = start(&folder.0, &config, None);
    let answer = call(&obid, "GET", "/v1/accounts/default/sign-in", None);
    refused(&answer, 409, "ownLogin");
}

#[test]
fn sign_in_3_a_login_folder_obid_wont_sign_in_through() {
    let folder = Folder::new("reasons-link");
    let obid = start(&folder.0, &agents_config(&folder.0), None);
    let id = added(&obid, "codex");
    let dir = obid.home.join("accounts").join(&id);
    std::fs::write(folder.0.join("elsewhere.json"), "{}").unwrap();
    std::os::unix::fs::symlink(folder.0.join("elsewhere.json"), dir.join("auth.json")).unwrap();
    let answer = call(&obid, "GET", &format!("/v1/accounts/{id}/sign-in"), None);
    refused(&answer, 409, "folderUnsafe");
}

#[test]
fn sign_in_4_an_account_a_task_is_using() {
    let folder = Folder::new("reasons-busy");
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agent("fake-claude"));
    if cfg!(target_os = "macos") {
        config["claudeKeychain"] = json!(true);
    }
    let obid = start(&folder.0, &config, None);
    let job = call(
        &obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "investigate", "task": "SLEEP", "account": "stub"}}),
        ),
    );
    assert_eq!(job.status, 201, "{}", job.text);
    let answer = call(&obid, "GET", "/v1/accounts/stub/sign-in", None);
    refused(&answer, 409, "busy");
    let id = job.body["id"].as_str().unwrap();
    call(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
}

#[test]
fn sign_in_5_claude_codes_keychain_login_off() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let folder = Folder::new("reasons-keychain");
    let mut config = agents_config(&folder.0);
    config["claudeKeychain"] = json!(false);
    let obid = start(&folder.0, &config, None);
    let id = added(&obid, "claude");
    let answer = call(&obid, "GET", &format!("/v1/accounts/{id}/sign-in"), None);
    refused(&answer, 409, "keychainOff");
}

#[test]
fn sign_in_6_a_sign_in_that_gives_nothing_to_open() {
    let folder = Folder::new("reasons-quiet");
    let mut config = agents_config(&folder.0);
    config["agents"]["cursor"]["bin"] = json!(quiet_bin(&folder.0, "cursor-agent"));
    let gh = quiet_bin(&folder.0, "gh");
    let obid = start(&folder.0, &config, Some(gh.parent().unwrap()));
    let id = added(&obid, "cursor");
    let answer = call(&obid, "GET", &format!("/v1/accounts/{id}/sign-in"), None);
    refused(&answer, 502, "didNotStart");
    let answer = call(&obid, "POST", "/v1/github/sign-in", None);
    refused(&answer, 502, "didNotStart");
}

#[test]
fn sign_in_7_a_code_claude_code_refused() {
    let folder = Folder::new("reasons-code");
    let obid = start(&folder.0, &agents_config(&folder.0), None);
    let id = added(&obid, "claude");
    let target = format!("/v1/accounts/{id}/sign-in");
    assert_eq!(call(&obid, "GET", &target, None).status, 200);
    let answer = call(&obid, "POST", &target, Some(&json!({"code": "bad#fake"})));
    refused(&answer, 422, "codeRefused");
}

#[test]
fn sign_in_8_an_address_that_isnt_a_tetrix() {
    let folder = Folder::new("reasons-tetrix-url");
    let remote = folder.0.join("remote.git");
    let obid = start(&folder.0, &config(&folder.0, &remote, &stub_agent()), None);
    let answer = call(
        &obid,
        "POST",
        "/v1/tetrix/sign-in",
        Some(&json!({"url": "http://tetrix.example/mcp"})),
    );
    refused(&answer, 422, "badAddress");
}

#[test]
fn sign_in_9_a_tetrix_that_says_nowhere_to_sign_in() {
    let folder = Folder::new("reasons-tetrix-none");
    let remote = folder.0.join("remote.git");
    let obid = start(&folder.0, &config(&folder.0, &remote, &stub_agent()), None);
    // Nothing listens on port 1: no metadata, so nowhere to sign in.
    let answer = call(
        &obid,
        "POST",
        "/v1/tetrix/sign-in",
        Some(&json!({"url": "http://127.0.0.1:1/mcp"})),
    );
    refused(&answer, 502, "noSignIn");
}
