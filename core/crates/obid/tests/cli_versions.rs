//! Each agent CLI's version (docs/contracts/cli-versions.md, CLIVERSIONS-1 to 4), through the
//! real obid binary: `GET /v1/status`'s `tools`, a version that fails or hangs, a job's
//! `agentVersion` stamped as its round starts, and a binary that changed read again and logged once.

mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::*;

fn agents() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents")
}

/// obid with the fixture repository, Claude Code as fake-claude and `codex` (a binary) on an
/// account in a folder of its own; `more` over its `agents`.
fn obid(folder: &Folder, codex: &Path, more: Value) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-claude"));
    let accounts = folder.0.join("home/accounts");
    config["agents"]["codex"] = json!({"bin": codex,
        "accounts": [{"id": "work", "name": "Work", "configDir": accounts.join("codex-work")}]});
    for (key, value) in more.as_object().unwrap() {
        config["agents"][key] = value.clone();
    }
    start(&folder.0, &config, None)
}

/// A shell script at `path`, executable.
fn script(path: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `GET /v1/status`'s `tools`, and how long the answer took.
fn read_tools(obid: &Obid) -> (Vec<Value>, Duration) {
    let started = Instant::now();
    let status = call(obid, "GET", "/v1/status", None);
    assert_eq!(status.status, 200, "{}", status.text);
    let tools = status.body["tools"].as_array().cloned().unwrap_or_default();
    (tools, started.elapsed())
}

/// A Codex job told `task`, once it has ended done.
fn codex_job(obid: &Obid, task: &str) -> String {
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "research", "check": "true",
                     "agent": "codex", "task": task});
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    let id = answer.body["id"].as_str().unwrap().to_owned();
    let ended = ended_within(obid, &id, 240);
    assert_eq!(ended["status"], "done", "{ended}");
    id
}

/// Job `id`'s `agentVersion` in its file and in `GET /v1/jobs/{id}`, once it's stamped (the round
/// doesn't wait for it): both the same.
fn agent_version(obid: &Obid, id: &str) -> Value {
    within(10, "the job's agentVersion is stamped", || {
        let file = std::fs::read(obid.home.join(format!("jobs/{id}.json"))).ok()?;
        let kept: Value = serde_json::from_slice(&file).ok()?;
        let job = call(obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (!kept["agentVersion"].is_null() && kept["agentVersion"] == job["agentVersion"])
            .then(|| job["agentVersion"].clone())
    })
}

/// CLIVERSIONS-1: status lists each tool, in order, with Claude Code's resolved path and the
/// version its `--version` printed.
#[test]
fn cliversions_1_status_lists_each_tool_version() {
    let folder = Folder::new("cliversions-1");
    let obid = obid(&folder, &agents().join("fake-codex"), json!({}));
    let (tools, _) = read_tools(&obid);
    let agents: Vec<&str> = tools.iter().map(|t| t["agent"].as_str().unwrap()).collect();
    assert_eq!(agents, ["claude", "codex", "cursor"], "{tools:?}");
    let claude = &tools[0];
    assert_eq!(claude["version"], "2.1.296", "{claude}");
    let path = claude["path"].as_str().unwrap();
    assert!(Path::new(path).is_absolute(), "{claude}");
    assert!(path.ends_with("tests/agents/fake-claude"), "{claude}");
    assert_eq!(tools[1]["version"], "0.170.0", "{tools:?}");
}

/// CLIVERSIONS-2: a `--version` that fails, and one that hangs past 5 seconds, are null, with
/// their paths, and status still answers: the first within the hang's 5 seconds, the next at once.
#[test]
fn cliversions_2_a_failing_version_is_null() {
    let folder = Folder::new("cliversions-2");
    let failing = folder.0.join("failing-codex");
    script(&failing, "#!/bin/sh\nexit 1\n");
    let hanging = folder.0.join("hanging-cursor");
    script(
        &hanging,
        "#!/bin/sh\n[ \"$1\" = --version ] && exec sleep 30\nexit 1\n",
    );
    let obid = obid(
        &folder,
        &failing,
        json!({"cursor": {"bin": hanging, "accounts": []}}),
    );
    let (tools, took) = read_tools(&obid);
    assert!(took < Duration::from_secs(10), "{took:?}");
    assert_eq!(tools[1]["agent"], "codex");
    assert_eq!(tools[1]["path"], failing.to_str().unwrap(), "{tools:?}");
    assert_eq!(tools[1]["version"], Value::Null, "{tools:?}");
    assert_eq!(tools[2]["agent"], "cursor");
    assert_eq!(tools[2]["path"], hanging.to_str().unwrap(), "{tools:?}");
    assert_eq!(tools[2]["version"], Value::Null, "{tools:?}");
    let (again, took) = read_tools(&obid);
    assert!(took < Duration::from_secs(2), "{took:?}");
    assert_eq!(again, tools);
}

/// CLIVERSIONS-3: a job that ran a round on Codex 0.170.0 says so in its file and in the API.
#[test]
fn cliversions_3_a_round_stamps_agent_version() {
    let folder = Folder::new("cliversions-3");
    let obid = obid(&folder, &agents().join("fake-codex"), json!({}));
    let id = codex_job(&obid, "Say hello.");
    assert_eq!(agent_version(&obid, &id), "0.170.0");
}

/// CLIVERSIONS-4: the binary at the same path changed after a round; the next round reads its
/// version again, and obid says the change once.
#[test]
fn cliversions_4_a_changed_binary_is_read_again() {
    let folder = Folder::new("cliversions-4");
    let codex = folder.0.join("codex");
    let wrapper = |version: &str| {
        format!(
            "#!/bin/sh\n[ \"$1\" = --version ] && echo \"codex-cli {version}\" && exit 0\nexec {:?} \"$@\"\n",
            agents().join("fake-codex")
        )
    };
    script(&codex, &wrapper("0.171.0"));
    let obid = obid(&folder, &codex, json!({}));
    let first = codex_job(&obid, "Say hello.");
    assert_eq!(agent_version(&obid, &first), "0.171.0");

    script(&codex, &wrapper("0.172.10"));
    let next = codex_job(&obid, "Say hello again.");
    assert_eq!(agent_version(&obid, &next), "0.172.10");
    let said: Vec<String> = obid
        .log()
        .lines()
        .filter(|line| line.contains("0.172.10"))
        .map(str::to_owned)
        .collect();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains(&format!("Codex at {} is version 0.172.10", codex.display())));
}
