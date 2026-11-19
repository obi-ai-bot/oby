//! obid-spec's own scenarios (docs/contracts/obid-spec.md), proven without a real obid: the
//! shipped workflows, coverage on a scratch repository, the host's verdicts on workflows of `check`
//! steps, a scratch obid's environment through a stand-in binary, and the lying Sphere against a
//! stand-in upstream.

use obid_spec::{coverage, node_names, obid, workflow};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

fn suite() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn root() -> PathBuf {
    suite().join("../..").canonicalize().unwrap()
}

#[test]
fn obidspec_1_every_shipped_workflow_parses_and_names_a_route_obid_serves() {
    let all = workflow::load_all(&suite().join("workflows"), &node_names()).unwrap();
    assert!(!all.is_empty());
    let routes = coverage::routes(&root()).unwrap();
    for w in &all {
        match &w.route {
            Some(r) => assert!(
                routes.contains(r),
                "{}: {r} isn't in docs/api/routes.txt",
                w.file.display()
            ),
            None => assert!(
                workflow::MANUAL_AREAS.contains(&w.area.as_str())
                    || obid_spec::lab::is_lab(&w.area),
                "{}",
                w.file.display()
            ),
        }
        assert!(!w.steps.is_empty(), "{}", w.file.display());
    }
}

/// A repository with three routes, one contract and a scope that covers two of them.
fn scratch_repo(dir: &Path, contract: &str) {
    std::fs::create_dir_all(dir.join("docs/api")).unwrap();
    std::fs::create_dir_all(dir.join("docs/contracts")).unwrap();
    std::fs::write(
        dir.join("docs/api/routes.txt"),
        "# header\nGET /v1/a\nPOST /v1/a\nGET /v1/b/{id}\n",
    )
    .unwrap();
    std::fs::write(dir.join("docs/contracts/x.md"), contract).unwrap();
}

fn wf(name: &str, route: Option<&str>, proves: &[&str]) -> workflow::Workflow {
    let trigger = match route {
        Some(r) => {
            let (m, p) = r.split_once(' ').unwrap();
            json!({"http": {"method": m, "path": p}})
        }
        None => json!({"manual": {}}),
    };
    let text = json!({"schemaVersion": "2", "name": name, "trigger": trigger,
        "steps": [{"id": "s", "use": "check", "ui": {"proves": proves}}]})
    .to_string();
    let area = if route.is_some() { "a" } else { "triggers" };
    workflow::parse(
        &text,
        Path::new(&format!("{name}.json")),
        area,
        &node_names(),
    )
    .unwrap()
}

fn scope(obid: &[&str], elsewhere: &[&str]) -> Value {
    let elsewhere: serde_json::Map<String, Value> = elsewhere
        .iter()
        .map(|s| (s.to_string(), json!("another app's")))
        .collect();
    json!({"areas": {"a": {"routes": ["/v1/a", "/v1/b"], "contracts": {"x.md": {"obid": obid, "elsewhere": elsewhere}}}},
           "triggers": {"the feed": {"area": "a", "workflows": ["feed.json"]}}})
}

#[test]
fn obidspec_3_coverage_fails_on_a_route_a_trigger_or_a_scenario_left_out() {
    let dir = tempfile::tempdir().unwrap();
    scratch_repo(dir.path(), "- **X-1** one\n- **X-2** two\n");
    let full = vec![
        wf("GET /v1/a", Some("GET /v1/a"), &["X-1"]),
        wf("POST /v1/a", Some("POST /v1/a"), &[]),
        wf("GET /v1/b/{id}", Some("GET /v1/b/{id}"), &[]),
        wf("feed", None, &[]),
    ];
    let ok = coverage::report(dir.path(), &scope(&["X-1"], &["X-2"]), &full, None).unwrap();
    assert!(ok.ok(), "{ok:?}");
    assert_eq!((ok.routes_covered, ok.routes_total), (3, 3));
    // A route without a workflow.
    let r = coverage::report(dir.path(), &scope(&["X-1"], &["X-2"]), &full[1..], None).unwrap();
    assert!(!r.ok());
    // A trigger without one.
    let r = coverage::report(dir.path(), &scope(&["X-1"], &["X-2"]), &full[..3], None).unwrap();
    assert!(!r.ok());
    // A scenario of obid's that no step names.
    let r = coverage::report(dir.path(), &scope(&["X-1", "X-2"], &[]), &full, None).unwrap();
    assert!(!r.ok());
    // --only judges that area alone.
    let r = coverage::report(dir.path(), &scope(&["X-1"], &["X-2"]), &full, Some("a")).unwrap();
    assert!(r.ok());
}

/// The check's gate judges what must hold now and only reports the scenarios still waiting for an
/// assertion (OBIDSPEC-11).
#[test]
fn obidspec_11_the_gate_fails_on_what_must_hold_and_reports_the_backlog() {
    let dir = tempfile::tempdir().unwrap();
    scratch_repo(
        dir.path(),
        "- **X-1** one\n- **X-2** two\n- **X-3** three\n",
    );
    let full = vec![
        wf("GET /v1/a", Some("GET /v1/a"), &["X-1"]),
        wf("POST /v1/a", Some("POST /v1/a"), &[]),
        wf("GET /v1/b/{id}", Some("GET /v1/b/{id}"), &[]),
        wf("feed", None, &[]),
    ];
    // Two scenarios of obid's with no assertion: not ok, but the gate holds, and they're the backlog.
    let r = coverage::report(dir.path(), &scope(&["X-1", "X-2", "X-3"], &[]), &full, None).unwrap();
    assert!(!r.ok());
    assert!(r.gate_ok(), "{r:?}");
    assert_eq!(coverage::backlog(&r), 2);
    // A route without a workflow fails the gate.
    let r = coverage::report(
        dir.path(),
        &scope(&["X-1"], &["X-2", "X-3"]),
        &full[1..],
        None,
    )
    .unwrap();
    assert!(!r.gate_ok());
    // So does a trigger without one.
    let r = coverage::report(
        dir.path(),
        &scope(&["X-1"], &["X-2", "X-3"]),
        &full[..3],
        None,
    )
    .unwrap();
    assert!(!r.gate_ok());
    // And a scenario left unclassified.
    let r = coverage::report(dir.path(), &scope(&["X-1"], &["X-2"]), &full, None).unwrap();
    assert!(!r.gate_ok());
    // And a step naming a scenario no contract lists.
    let mut unknown = full.clone();
    unknown.push(wf("other", None, &["X-9"]));
    let r = coverage::report(
        dir.path(),
        &scope(&["X-1"], &["X-2", "X-3"]),
        &unknown,
        None,
    )
    .unwrap();
    assert!(!r.gate_ok());
}

#[test]
fn obidspec_4_nothing_in_scope_goes_unread_silently() {
    let dir = tempfile::tempdir().unwrap();
    scratch_repo(dir.path(), "- **X-1** one\n- **X-2** two\n");
    let mut full = vec![
        wf("GET /v1/a", Some("GET /v1/a"), &["X-1"]),
        wf("POST /v1/a", Some("POST /v1/a"), &[]),
        wf("GET /v1/b/{id}", Some("GET /v1/b/{id}"), &[]),
        wf("feed", None, &[]),
    ];
    // X-2 in neither list.
    let r = coverage::report(dir.path(), &scope(&["X-1"], &[]), &full, None).unwrap();
    assert!(!r.ok() && r.areas[0].unclassified.iter().any(|u| u.contains("X-2")));
    // scope.json naming a scenario the contract doesn't list.
    let r = coverage::report(dir.path(), &scope(&["X-1", "X-9"], &["X-2"]), &full, None).unwrap();
    assert!(!r.ok());
    // A step naming a scenario no contract in scope lists.
    full.push(wf("other", None, &["Y-1"]));
    let r = coverage::report(dir.path(), &scope(&["X-1"], &["X-2"]), &full, None).unwrap();
    assert!(!r.ok() && r.areas[0].unknown_proves.iter().any(|u| u.contains("Y-1")));
}

/// Runs the built host on workflows made of `check` steps (no obid starts: `--obid` is any file).
fn run_checks(workflows: &[(&str, Value)]) -> (i32, String) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("wf/triggers")).unwrap();
    for (name, steps) in workflows {
        let doc =
            json!({"schemaVersion": "2", "name": name, "trigger": {"manual": {}}, "steps": steps});
        std::fs::write(
            dir.path().join(format!("wf/triggers/{name}.json")),
            doc.to_string(),
        )
        .unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_obid-spec"))
        .args(["run", "--obid", "/bin/echo"])
        .env("OBID_SPEC_WORKFLOWS", dir.path().join("wf"))
        .env("TMPDIR", dir.path())
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn check(id: &str, value: Value, expect: Value, extra: Value) -> Value {
    let mut s = json!({"id": id, "use": "runtime.rust:check", "inputs": {"value": value, "expect": expect}});
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    s
}

#[test]
fn obidspec_5_a_run_prints_a_table_and_exits_non_zero_on_a_failure() {
    let good = check(
        "a",
        json!({"state": "joined"}),
        json!({"body": {"state": "joined"}}),
        json!({}),
    );
    let later = check(
        "b",
        json!("${state.a.passed}"),
        json!({"body": true}),
        json!({}),
    );
    let (code, out) = run_checks(&[("good", json!([good, later]))]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("pass")
            && out.contains("good")
            && out.contains("1 workflows: 1 passed, 0 failed"),
        "{out}"
    );
    let bad = check(
        "a",
        json!({"state": "none"}),
        json!({"body": {"state": "joined"}}),
        json!({}),
    );
    let skipped = check("b", json!(1), json!({"body": 1}), json!({}));
    let (code, out) = run_checks(&[("good", json!([good])), ("bad", json!([bad, skipped]))]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("FAIL") && out.contains("bad › a") && out.contains("1 failed"),
        "{out}"
    );
}

#[test]
fn obidspec_6_a_vacuous_assertion_or_one_that_asserts_nothing_fails_its_step() {
    // Whatever the value, one of these is it: changed to anything, it still passes.
    let vacuous = check(
        "v",
        json!({"a": 1}),
        json!({"paths": {"a": {"$oneOf": [1, {"obid-spec": "mutated"}]}}}),
        json!({}),
    );
    let (code, out) = run_checks(&[("vacuous", json!([vacuous]))]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("a vacuous assertion"), "{out}");
    let nothing = check(
        "n",
        json!({"a": 1}),
        json!({}),
        json!({"ui": {"proves": ["X-1"]}}),
    );
    let (code, out) = run_checks(&[("nothing", json!([nothing]))]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("asserts nothing"), "{out}");
}

#[test]
fn obidspec_7_an_expected_red_step_fails_quietly_until_it_passes() {
    let red = check(
        "r",
        json!({"state": "joined"}),
        json!({"body": {"state": "revoked"}}),
        json!({"ui": {"expectedRed": "#1"}}),
    );
    let after = check("after", json!(1), json!({"body": 1}), json!({}));
    let (code, out) = run_checks(&[("red", json!([red, after]))]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("expected red: red › r: #1"), "{out}");
    let fixed = check(
        "r",
        json!({"state": "revoked"}),
        json!({"body": {"state": "revoked"}}),
        json!({"ui": {"expectedRed": "#1"}}),
    );
    let (code, out) = run_checks(&[("fixed", json!([fixed]))]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("remove expectedRed"), "{out}");
}

#[tokio::test]
async fn obidspec_8_a_scratch_obid_sees_only_its_stand_ins_and_stops_by_pid() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("fake-obid");
    std::fs::write(&fake, "#!/bin/sh\nenv > \"$TMPDIR/env.txt\"\necho 'obid listening on http://127.0.0.1:1'\nexec sleep 60\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run = obid::Run {
        obid: fake,
        root: root(),
        scratch: dir.path().join("wf"),
        sphere_url: None,
        sphere_operator: None,
    };
    // Something of the person's that a scratch obid must never see.
    std::env::set_var("ANTHROPIC_API_KEY", "not-for-obid");
    let started = obid::start(
        &run,
        serde_json::from_value(json!({"name": "alex"})).unwrap(),
    )
    .await
    .unwrap();
    let folder = PathBuf::from(started["folder"].as_str().unwrap());
    let env = std::fs::read_to_string(folder.join("tmp/env.txt")).unwrap();
    assert!(!env.contains("ANTHROPIC_API_KEY"), "{env}");
    let path = env.lines().find_map(|l| l.strip_prefix("PATH=")).unwrap();
    let dirs: Vec<&str> = path.split(':').collect();
    assert!(dirs[0].ends_with("core/crates/obid/tests/agents"), "{path}");
    assert_eq!(
        &dirs[2..],
        ["/usr/bin", "/bin", "/usr/sbin", "/sbin"],
        "{path}"
    );
    let tools: Vec<String> = std::fs::read_dir(dirs[1])
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        {
            let mut t = tools.clone();
            t.sort();
            t
        },
        ["gitleaks", "node"]
    );
    assert!(
        env.contains(&format!("HOME={}", folder.join("env-home").display())),
        "{env}"
    );
    assert!(env.contains("OBID_SECURITY_BIN="), "{env}");
    // The login shell obid reads the person's PATH from is a stand-in answering this obid's own.
    let shell = env
        .lines()
        .find_map(|l| l.strip_prefix("OBID_LOGIN_SHELL="))
        .unwrap();
    let said = std::process::Command::new(shell)
        .args(["-l", "-c", "/usr/bin/env -0"])
        .env_clear()
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&said.stdout).to_string();
    let login_path = said
        .split('\0')
        .find_map(|l| l.strip_prefix("PATH="))
        .unwrap();
    assert_eq!(login_path, path, "{said}");
    assert!(
        !login_path.contains("homebrew") && !login_path.contains(".local"),
        "{login_path}"
    );
    let mode = std::fs::metadata(folder.join("home/api-token"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    let pid = started["pid"].as_u64().unwrap() as u32;
    assert!(
        obid::stop(1, std::time::Duration::from_secs(1))
            .await
            .is_err(),
        "a PID the suite didn't start"
    );
    let stopped = obid::stop(pid, std::time::Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(stopped["stopped"], true);
    assert!(!obid_spec::procs::alive(pid));
}

#[tokio::test]
async fn obidspec_9_the_lying_sphere_resigns_only_the_named_owners_card() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", upstream.local_addr().unwrap().port());
    let card = |owner: &str| json!({"name": owner, "capabilities": {"extensions": [{"params": {"owner": owner, "owner_key": "real"}}]}, "signatures": [{"signature": "real"}]});
    let directory = json!({"members": [
        {"owner": "dana", "owner_key": "real", "owner_key_id": "real", "card": card("dana")},
        {"owner": "erin", "owner_key": "real", "owner_key_id": "real", "card": card("erin")}]});
    tokio::spawn(async move {
        loop {
            let (mut s, _) = upstream.accept().await.unwrap();
            let directory = directory.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap();
                let asked = String::from_utf8_lossy(&buf[..n]).to_string();
                let body = if asked.starts_with("GET /v0.1/spheres/s/members") {
                    directory.to_string()
                } else {
                    json!({"other": true}).to_string()
                };
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await;
            });
        }
    });
    let liar = obid_spec::liar::start(&url).await.unwrap();
    let get = |path: &str| obid_spec::http::Request {
        method: "GET".into(),
        base: liar.clone(),
        target: path.into(),
        ..Default::default()
    };
    let before = obid_spec::http::send(
        &get("/v0.1/spheres/s/members"),
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(before.body["members"][0]["owner_key"], "real");
    let key = obid_spec::keys::OwnerKey::generate();
    obid_spec::liar::swap(&liar, "dana", &key.seed_hex()).unwrap();
    let after = obid_spec::http::send(
        &get("/v0.1/spheres/s/members"),
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap();
    let dana = &after.body["members"][0];
    assert_eq!(dana["owner_key"], key.public());
    assert_eq!(dana["owner_key_id"], key.key_id());
    assert_eq!(
        dana["card"]["capabilities"]["extensions"][0]["params"]["owner_key"],
        key.public()
    );
    assert_eq!(dana["card"]["signatures"].as_array().unwrap().len(), 1);
    assert_ne!(dana["card"]["signatures"][0]["signature"], "real");
    assert_eq!(
        after.body["members"][1]["owner_key"], "real",
        "erin's card is left as it is"
    );
    let other = obid_spec::http::send(&get("/v0.1/health"), std::time::Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(other.body, json!({"other": true}));
}

#[test]
fn obidspec_10_a_flaky_step_fails_quietly_and_passes_as_a_pass() {
    let flaky = check(
        "f",
        json!({"state": "joined"}),
        json!({"body": {"state": "revoked"}}),
        json!({"ui": {"flaky": "#2"}}),
    );
    let after = check("after", json!(1), json!({"body": 1}), json!({}));
    let (code, out) = run_checks(&[("flaky", json!([flaky, after]))]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("flaky, failed this run: flaky › f: #2"),
        "{out}"
    );
    let passing = check(
        "f",
        json!({"state": "revoked"}),
        json!({"body": {"state": "revoked"}}),
        json!({"ui": {"flaky": "#2"}}),
    );
    let (code, out) = run_checks(&[("passing", json!([passing]))]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("flaky, failed"), "{out}");
}

/// `run --tag` runs only the workflows that carry the tag, and the shipped launch set is the
/// Obysphere's and the launch flows' (OBIDSPEC-12).
#[test]
fn obidspec_12_a_tag_runs_only_its_workflows() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("wf/triggers")).unwrap();
    let good = check("a", json!(1), json!({"body": 1}), json!({}));
    let bad = check("b", json!(1), json!({"body": 2}), json!({}));
    for (name, step, tags) in [
        ("tagged", good, json!(["launch"])),
        ("untagged", bad, Value::Null),
    ] {
        let mut doc =
            json!({"schemaVersion": "2", "name": name, "trigger": {"manual": {}}, "steps": [step]});
        if !tags.is_null() {
            doc["tags"] = tags;
        }
        std::fs::write(
            dir.path().join(format!("wf/triggers/{name}.json")),
            doc.to_string(),
        )
        .unwrap();
    }
    let run = |tag: Option<&str>| {
        let mut args = vec!["run", "--obid", "/bin/echo"];
        if let Some(t) = tag {
            args.extend(["--tag", t]);
        }
        let out = Command::new(env!("CARGO_BIN_EXE_obid-spec"))
            .args(&args)
            .env("OBID_SPEC_WORKFLOWS", dir.path().join("wf"))
            .env("TMPDIR", dir.path())
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    };
    let (code, out) = run(Some("launch"));
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1 workflows: 1 passed"), "{out}");
    let (code, out) = run(None);
    assert_eq!(code, 1, "{out}");
    let (code, _) = run(Some("nothing-has-it"));
    assert_ne!(code, 0);
    // The shipped launch set: every Obysphere workflow and the launch flows, nothing else.
    let shipped = workflow::load_all(&suite().join("workflows"), &obid_spec::node_names()).unwrap();
    let launch: Vec<_> = shipped
        .iter()
        .filter(|w| w.tags.iter().any(|t| t == "launch"))
        .collect();
    assert!(launch.iter().any(|w| w.area == "sphere"));
    for w in &shipped {
        let file = w.file.file_name().unwrap().to_string_lossy();
        let should = w.area == "sphere"
            || file.starts_with("sphere-")
            || LAUNCH_FLOWS.contains(&file.as_ref());
        assert_eq!(
            w.tags.iter().any(|t| t == "launch"),
            should,
            "{}",
            w.file.display()
        );
    }
}

/// The launch flows' workflows (the owner's freeze of 2026-11-18 keeps the set to these).
const LAUNCH_FLOWS: &[&str] = &[
    "conversation-deliveries.json",
    "conversation-tasks.json",
    "task-states.json",
    "task-admission-and-receipts.json",
    "account-limits-and-busy.json",
    // the window's and the Mac's, proving today's fixes (#2572, #2560)
    "copy-not-over-card.json",
    "release-menu.json",
    "cmd-b.json",
];

/// No shipped workflow ever has obid look for a tool by a bare name (a `bin` in `PUT
/// /v1/settings/agents/{agent}` or an `obid-start` configuration): obid would search the person's
/// install folders for it and run what it finds (OBIDSPEC-8).
#[test]
fn obidspec_8_no_workflow_asks_obid_to_search_for_a_tool() {
    fn bins(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    if k == "bin" {
                        if let Some(s) = x.as_str() {
                            out.push(s.to_string());
                        }
                    }
                    bins(x, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| bins(x, out)),
            _ => {}
        }
    }
    let all = workflow::load_all(&suite().join("workflows"), &node_names()).unwrap();
    for w in &all {
        for s in &w.steps {
            let mut found = vec![];
            bins(&s.inputs, &mut found);
            for b in found {
                assert!(
                    b.starts_with('/') || b.starts_with("${"),
                    "{} › {}: bin {b:?} would have obid search the person's folders",
                    w.name,
                    s.id
                );
            }
        }
    }
}

#[tokio::test]
async fn obidspec_8_a_bare_tool_name_is_refused_before_obid_starts() {
    let dir = tempfile::tempdir().unwrap();
    let run = obid::Run {
        obid: "/bin/echo".into(),
        root: root(),
        scratch: dir.path().join("wf"),
        sphere_url: None,
        sphere_operator: None,
    };
    let input = json!({"name": "o", "config": {"agents": {"codex": {"bin": "codex"}}}});
    let refused = obid::start(&run, serde_json::from_value(input).unwrap()).await;
    assert!(format!("{:#}", refused.unwrap_err()).contains("absolute path only"));
}

fn scratch_run(dir: &Path) -> obid::Run {
    obid::Run {
        obid: "/bin/echo".into(),
        root: root(),
        scratch: dir.to_path_buf(),
        sphere_url: None,
        sphere_operator: None,
    }
}

/// The app's nodes reach no obid but the workflow's own: never the owner's on 7777, never a home
/// outside the workflow's folder; a step on an app no launch started is refused; off macOS they
/// skip. The app workflows are manual, in `workflows/app/` (OBIDSPEC-13).
#[tokio::test]
async fn obidspec_13_the_app_runs_only_on_the_workflows_own_obid() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().canonicalize().unwrap();
    let run = scratch_run(&scratch);
    let launch = |input: Value| {
        let run = run.clone();
        async move { obid_spec::nodes::run_node(&run, "app.launch", input).await }
    };
    if let Some(why) = obid_spec::app::unsupported() {
        let out = launch(json!({"mock": true})).await.unwrap();
        assert_eq!(out["skipped"], json!(why));
        return;
    }
    let home = scratch.join("o/home");
    let e = launch(json!({"obid": {"port": 7777, "home": home}}))
        .await
        .unwrap_err();
    assert!(format!("{e:#}").contains("7777 is the owner"), "{e:#}");
    let e = launch(json!({"obid": {"port": 45123, "home": "/tmp/elsewhere"}}))
        .await
        .unwrap_err();
    assert!(
        format!("{e:#}").contains("isn't in this workflow's folder"),
        "{e:#}"
    );
    let e = launch(json!({})).await.unwrap_err();
    assert!(format!("{e:#}").contains("mock: true"), "{e:#}");
    let e = obid_spec::nodes::run_node(
        &run,
        "app.click",
        json!({"app": "nobody", "target": {"text": "x"}}),
    )
    .await
    .unwrap_err();
    assert!(format!("{e:#}").contains("no app named nobody"), "{e:#}");
    let shipped = workflow::load_all(&suite().join("workflows"), &node_names()).unwrap();
    let apps: Vec<_> = shipped.iter().filter(|w| w.area == "app").collect();
    assert!(!apps.is_empty());
    assert!(apps.iter().all(|w| w.route.is_none()
        && w.steps.iter().any(|s| s.node == "app.launch")
        && w.steps
            .iter()
            .any(|s| s.node == "app.measure" && !s.proves.is_empty())));
}

/// Without agent-desktop (or off macOS, or without both permissions: desktop.rs's tests) a desktop
/// step answers why it is skipped, and touches nothing (OBIDSPEC-14).
#[tokio::test]
async fn obidspec_14_a_desktop_step_without_agent_desktop_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let run = scratch_run(dir.path());
    std::env::set_var(
        "OBID_SPEC_AGENT_DESKTOP",
        dir.path().join("absent/agent-desktop"),
    );
    for node in [
        "desktop.snapshot",
        "desktop.read",
        "desktop.click",
        "desktop.press",
    ] {
        let out = obid_spec::nodes::run_node(
            &run,
            node,
            json!({"app": {"pid": 1}, "menu": ["View"], "keys": "cmd+b"}),
        )
        .await
        .unwrap();
        let why = out["skipped"].as_str().unwrap_or_default();
        assert!(
            why.contains("agent-desktop isn't installed")
                || why.contains("desktop steps drive macOS"),
            "{node}: {out}"
        );
    }
}

/// A workflow whose step can't run here is marked skipped: never passed, never failed, its later
/// steps not run, the run's exit 0 when nothing failed (OBIDSPEC-15).
#[test]
fn obidspec_15_a_skipped_workflow_is_neither_passed_nor_failed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("wf/desktop")).unwrap();
    let skips = json!({"id": "menu", "use": "runtime.rust:desktop.read", "ui": {"proves": ["SHORTCUTS-10"]},
        "inputs": {"app": {"pid": 1}, "menu": "View", "expect": {"paths": {"items": {"$len": 5}}}}});
    let after = check("after", json!(1), json!({"body": 2}), json!({}));
    let doc = json!({"schemaVersion": "2", "name": "needs the Mac", "trigger": {"manual": {}}, "steps": [skips, after]});
    std::fs::write(dir.path().join("wf/desktop/menu.json"), doc.to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_obid-spec"))
        .args(["run", "--obid", "/bin/echo"])
        .env("OBID_SPEC_WORKFLOWS", dir.path().join("wf"))
        .env("OBID_SPEC_AGENT_DESKTOP", dir.path().join("absent"))
        .env("TMPDIR", dir.path())
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("skip   desktop"), "{text}");
    assert!(text.contains("0 passed, 0 failed, 1 skipped"), "{text}");
    assert!(
        text.contains("skipped, not run here: needs the Mac: skipped:"),
        "{text}"
    );
    // the failing check after it never ran
    assert!(!text.contains("needs the Mac › after"), "{text}");
}
