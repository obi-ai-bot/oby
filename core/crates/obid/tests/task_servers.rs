//! The local servers a task's agent started (docs/contracts/task-servers.md, TASKSERVERS-1 to 6),
//! on the real obid binary asked over HTTP: a stand-in agent (`SERVE PORT` in tests/agents/
//! fake-claude and fake-codex) starts a child that listens on 127.0.0.1, as `npm run dev` does,
//! and the job's `servers` shows its port while it listens, and only its own.

mod support;

use std::net::TcpListener;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::*;

fn agents() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents")
}

/// obid with Claude Code's stand-in (the owner's own work runs in its own lane: unboxed) and
/// Codex's (in Sandbox, as these tests start, Codex runs in Oby's box).
fn obid(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agents().join("fake-claude"));
    let accounts = folder.0.join("home/accounts");
    config["agents"]["codex"] = json!({"bin": agents().join("fake-codex"),
        "accounts": [{"id": "work", "name": "Work", "configDir": accounts.join("codex-work")}]});
    start(&folder.0, &config, None)
}

fn serve(obid: &Obid, agent: &str) -> String {
    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "research", "check": "true",
                     "task": "SERVE PORT", "agent": agent});
    let answer = call(obid, "POST", "/v1/jobs", Some(&json!({ "job": job })));
    assert_eq!(answer.status, 201, "{}", answer.text);
    answer.body["id"].as_str().unwrap().to_owned()
}

/// A file the stand-in writes in job `id`'s worktree.
fn worktree_file(obid: &Obid, id: &str, name: &str) -> PathBuf {
    obid.home.join("worktrees").join(id).join(".obi").join(name)
}

/// The port job `id`'s stand-in server listens on, once it does.
fn serving(obid: &Obid, id: &str) -> u16 {
    let file = worktree_file(obid, id, "serving");
    within(30, "the stand-in server listens", || {
        std::fs::read_to_string(&file).ok()?.trim().parse().ok()
    })
}

fn job(obid: &Obid, id: &str) -> Value {
    call(obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

/// Job `id`'s servers' ports, none when it has no `servers`.
fn ports(job: &Value) -> Vec<u64> {
    job["servers"]
        .as_array()
        .map(|list| list.iter().filter_map(|s| s["port"].as_u64()).collect())
        .unwrap_or_default()
}

/// Waits (the meter reads every 5 seconds) until job `id`'s ports are `want`.
fn until_ports(obid: &Obid, id: &str, want: &[u16]) -> Value {
    let want: Vec<u64> = want.iter().map(|&p| u64::from(p)).collect();
    within(30, &format!("job {id}'s servers are {want:?}"), || {
        let job = job(obid, id);
        (ports(&job) == want).then_some(job)
    })
}

/// TASKSERVERS-1 and 2: a running job whose agent's child listens on 127.0.0.1 has that server in
/// its `servers`, with the process's name and when obid first saw it, and a `job` event carries
/// it; once the server stops listening it goes from the list, and an event says so, while the job
/// still runs.
#[test]
fn taskservers_1_and_2_a_running_jobs_server_shows_and_goes_when_it_closes() {
    let folder = Folder::new("servers-show");
    let obid = obid(&folder);
    let events = follow_events(&obid);
    let id = serve(&obid, "claude");
    let port = serving(&obid, &id);

    let shown = until_ports(&obid, &id, &[port]);
    assert_eq!(shown["status"], "running", "{shown}");
    let server = &shown["servers"][0];
    assert_eq!(server["process"], "node", "{shown}");
    assert!(
        server["firstSeen"]
            .as_str()
            .is_some_and(|at| !at.is_empty()),
        "{shown}"
    );
    let carried = events_named(&events, "job", 30, |got| {
        got.iter()
            .any(|j| j["id"] == id.as_str() && ports(j) == [u64::from(port)])
    });
    assert!(!carried.is_empty());

    std::fs::write(worktree_file(&obid, &id, "stop-serving"), "").unwrap();
    let gone = until_ports(&obid, &id, &[]);
    assert_eq!(gone["status"], "running", "{gone}");
    assert!(gone.get("servers").is_none(), "{gone}");
    events_named(&events, "job", 30, |got| {
        got.iter()
            .rev()
            .find(|j| j["id"] == id.as_str())
            .is_some_and(|j| ports(j).is_empty())
    });
}

/// TASKSERVERS-3 and 4: each job lists only what its own agent's processes listen on, never
/// another job's server nor one of the person's other processes (here the test's own listener);
/// a job that ends has none.
#[test]
fn taskservers_3_and_4_only_its_own_and_none_once_it_ends() {
    let folder = Folder::new("servers-own");
    let obid = obid(&folder);
    let mine = TcpListener::bind("127.0.0.1:0").unwrap();
    let outside = mine.local_addr().unwrap().port();
    let (first, second) = (serve(&obid, "claude"), serve(&obid, "claude"));
    let (one, two) = (serving(&obid, &first), serving(&obid, &second));
    assert_ne!(one, two);

    let a = until_ports(&obid, &first, &[one]);
    let b = until_ports(&obid, &second, &[two]);
    for job in [&a, &b] {
        assert!(!ports(job).contains(&u64::from(outside)), "{job}");
    }

    let stopped = call(&obid, "POST", &format!("/v1/jobs/{first}/stop"), None);
    assert_eq!(stopped.status, 200, "{}", stopped.text);
    let ended = until_ports(&obid, &first, &[]);
    assert_eq!(ended["status"], "stopped", "{ended}");
    // The other still runs and still shows its own.
    assert_eq!(ports(&job(&obid, &second)), [u64::from(two)]);
    drop(mine);
}

/// TASKSERVERS-6: a job in Oby's box lists its server only where the Mac reaches it. On macOS the
/// box shares the Mac's network, and Seatbelt leaves a listen alone: listed. On Linux the box has a
/// network namespace of its own, which the host doesn't reach: never listed. The own-lane job's
/// server, read on the same tick after the boxed one listened, shows the reading came after.
#[test]
fn taskservers_6_a_boxed_jobs_server_is_listed_only_where_the_mac_reaches_it() {
    let folder = Folder::new("servers-boxed");
    let obid = obid(&folder);
    let boxed = serve(&obid, "codex");
    let boxed_port = serving(&obid, &boxed);
    assert_eq!(job(&obid, &boxed)["agent"], "codex");
    let own = serve(&obid, "claude");
    let own_port = serving(&obid, &own);
    until_ports(&obid, &own, &[own_port]);
    if cfg!(target_os = "macos") {
        until_ports(&obid, &boxed, &[boxed_port]);
    } else {
        let read = job(&obid, &boxed);
        assert!(read.get("servers").is_none(), "{read}");
    }
}
