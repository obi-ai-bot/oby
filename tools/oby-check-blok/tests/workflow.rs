use std::{
    path::Path,
    process::{Command, Output},
};

#[tokio::test]
async fn node_rejects_missing_token_and_foreign_checkout_without_running() {
    use oby_check_blok::{CheckInput, CheckNode, NodeInput, Verdict};
    let scratch = tempfile::tempdir().unwrap();
    let repo = scratch.path().join("repo");
    let foreign = scratch.path().join("foreign");
    for path in [&repo, &foreign] {
        std::fs::create_dir_all(path.join("scripts")).unwrap();
        std::fs::write(path.join("scripts/oby-check.sh"), "touch ran\n").unwrap();
    }
    let node = CheckNode(
        "test",
        "rust",
        CheckInput {
            checkout: repo.to_string_lossy().into_owned(),
            base: "HEAD".into(),
        },
        "spawn-token".into(),
    );
    for input in [
        serde_json::from_str::<NodeInput>("{}").unwrap(),
        NodeInput {
            token: "wrong".into(),
            checkout: None,
            base: None,
        },
        NodeInput {
            token: "spawn-token".into(),
            checkout: Some(foreign.to_string_lossy().into_owned()),
            base: None,
        },
        NodeInput {
            token: "spawn-token".into(),
            checkout: None,
            base: Some("other".into()),
        },
    ] {
        assert_eq!(node.execute(input).await.verdict, Verdict::Unjudged);
        assert!(!repo.join("ran").exists());
        assert!(!foreign.join("ran").exists());
    }
    assert_eq!(
        node.execute(NodeInput {
            token: "spawn-token".into(),
            checkout: None,
            base: None
        })
        .await
        .verdict,
        Verdict::Passed
    );
    assert!(repo.join("ran").exists());
    assert!(!foreign.join("ran").exists());
}

fn git(repo: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn plans_without_groups_are_unjudged_unless_nothing_would_run() {
    use std::io::Write;
    use std::process::Stdio;
    for (script, expected, reason, code) in [
        (
            "echo '==> cargo test'\n",
            "unjudged",
            "the script names no groups (older than #2052)",
            2,
        ),
        ("exit 0\n", "passed", "nothing to check", 0),
        (
            "echo 'oby-check: group unknown'\n",
            "unjudged",
            "no nodes ran",
            2,
        ),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let repo = scratch.path();
        std::fs::create_dir(repo.join("scripts")).unwrap();
        std::fs::write(repo.join("scripts/oby-check.sh"), script).unwrap();
        git(repo, &["init", "--quiet"]);
        git(repo, &["add", "."]);
        git(
            repo,
            &[
                "-c",
                "user.name=check",
                "-c",
                "user.email=check@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
        );
        for affected in [false, true] {
            let output = Command::new(env!("CARGO_BIN_EXE_oby-check-blok"))
                .current_dir(repo)
                .args([if affected { "--affected" } else { "." }, "HEAD"])
                .env("HOME", repo)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(code));
            let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["verdict"], expected);
            assert_eq!(report["reason"], reason);
            if expected == "passed" {
                assert_eq!(report["nodes"], serde_json::json!({}));
            }
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_oby-check-blok"))
            .current_dir(repo)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"oby_check_affected\",\"arguments\":{\"base\":\"HEAD\"}}}\n"
        ).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let report: serde_json::Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(report["verdict"], expected);
        assert_eq!(report["reason"], reason);
    }
}

#[test]
fn affected_cli_and_mcp_return_structured_verdicts() {
    use std::io::Write;
    use std::process::Stdio;
    let scratch = tempfile::tempdir().unwrap();
    let repo = scratch.path();
    std::fs::create_dir(repo.join("scripts")).unwrap();
    git(repo, &["init", "--quiet"]);
    git(
        repo,
        &[
            "-c",
            "user.name=check",
            "-c",
            "user.email=check@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    git(repo, &["update-ref", "refs/remotes/origin/dev", "HEAD"]);
    for failed in [false, true] {
        std::fs::write(repo.join("scripts/oby-check.sh"), if failed {
            "if [[ -n \"$OBY_CHECK_PLAN\" ]]; then echo \"oby-check: group $OBY_CHECK_ONLY\"; exit 0; fi\n echo 'test fixture::fails ... FAILED'; exit 1\n"
        } else {
            "if [[ -n \"$OBY_CHECK_PLAN\" ]]; then echo \"oby-check: group $OBY_CHECK_ONLY\"; fi\nexit 0\n"
        }).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_oby-check-blok"))
            .current_dir(repo)
            .args(["--affected", "HEAD"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(if failed { 1 } else { 2 })); // Swift has no pilot node.
        let verdict: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            verdict["verdict"],
            if failed { "failed" } else { "unjudged" }
        );
        assert!(verdict["reason"].is_string());
        if failed {
            assert_eq!(verdict["failing"], serde_json::json!(["fixture::fails"]));
        }
    }
    // A passing plan selects only the available Rust node.
    std::fs::write(repo.join("scripts/oby-check.sh"), "[[ $CARGO_NET_OFFLINE == true && $npm_config_offline == true && -z $OBY_CHECK_ALL && -z $OBY_CHECK_PATHS ]] || exit 1\nif [[ -n \"$OBY_CHECK_PLAN\" && $OBY_CHECK_ONLY == rust ]]; then echo 'oby-check: group rust'; fi\nexit 0\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_oby-check-blok"))
        .current_dir(repo)
        .args(["--affected"])
        .env("OBY_CHECK_ALL", "1")
        .env("OBY_CHECK_PATHS", "core/")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["verdict"],
        "passed"
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_oby-check-blok"))
        .current_dir(repo)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"oby_check_affected\",\"arguments\":{\"base\":\"HEAD\"}}}\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let responses: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[1]["result"]["tools"].as_array().unwrap().len(), 1);
    assert_eq!(
        responses[1]["result"]["tools"][0]["name"],
        "oby_check_affected"
    );
    let verdict: serde_json::Value = serde_json::from_str(
        responses[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(verdict["verdict"], "passed");
}

#[tokio::test]
async fn grpc_port_zero_and_spawn_configuration_reject_unauthorized_calls() {
    use blok::grpc_server::proto::{
        node_runtime_client::NodeRuntimeClient, ExecuteRequest, NodeRef,
    };
    use tokio::io::{AsyncBufReadExt, BufReader};
    let scratch = tempfile::tempdir().unwrap();
    let repo = scratch.path().join("repo");
    let foreign = scratch.path().join("foreign");
    for path in [&repo, &foreign] {
        std::fs::create_dir_all(path.join("scripts")).unwrap();
        std::fs::write(path.join("scripts/oby-check.sh"), "touch ran\n").unwrap();
    }
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_nodes"))
        .env("GRPC_PORT", "0")
        .env("OBY_CHECK_DIR", &repo)
        .env("OBY_CHECK_BASE", "HEAD")
        .env("OBY_CHECK_TOKEN", "private-token")
        .env_remove("BLOK_BLOB_DIR")
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let line = tokio::time::timeout(std::time::Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let address = line
        .split_once("listening on ")
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap();
    assert_ne!(address, "127.0.0.1:0");
    let mut client = NodeRuntimeClient::connect(format!("http://{address}"))
        .await
        .unwrap();
    for inputs in [
        serde_json::json!({}),
        serde_json::json!({"token": "wrong"}),
        serde_json::json!({"token": "private-token", "checkout": foreign}),
        serde_json::json!({"token": "private-token", "base": "other"}),
        serde_json::json!({"token": "private-token"}),
    ] {
        let allowed = inputs == serde_json::json!({"token": "private-token"});
        let response = client
            .execute(ExecuteRequest {
                node: Some(NodeRef {
                    name: "rust-affected-tests".into(),
                    r#type: "runtime.rust".into(),
                    version: String::new(),
                }),
                inputs: serde_json::to_vec(&inputs).unwrap(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let result: oby_check_blok::CheckResult = serde_json::from_slice(&response.data).unwrap();
        assert_eq!(
            result.verdict,
            if allowed {
                oby_check_blok::Verdict::Passed
            } else {
                oby_check_blok::Verdict::Unjudged
            }
        );
        assert_eq!(repo.join("ran").exists(), allowed);
        assert!(!foreign.join("ran").exists());
    }
    child.kill().await.unwrap();
    child.wait().await.unwrap();
}

#[test]
fn publishing_26_the_workflows_verdict_matches_the_scripts() {
    let scratch = tempfile::tempdir().unwrap();
    let repo = scratch.path().join("repo");
    std::fs::create_dir_all(repo.join("scripts")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for path in ["scripts/oby-check.sh", ".gitleaks.toml"] {
        std::fs::copy(root.join(path), repo.join(path)).unwrap();
    }
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=check",
            "-c",
            "user.email=check@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "base",
        ],
    );
    // Only a documentation change: the real planner chooses the real secret scan.
    std::fs::write(repo.join("README.md"), "Scratch check fixture\n").unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=check",
            "-c",
            "user.email=check@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "docs",
        ],
    );
    let script = Command::new("bash")
        .current_dir(&repo)
        .args(["scripts/oby-check.sh", "HEAD~1"])
        .env_remove("OBY_CHECK_PLAN")
        .env_remove("OBY_CHECK_ONLY")
        .env_remove("OBY_CHECK_PATHS")
        .env_remove("OBY_CHECK_ALL")
        .output()
        .unwrap();
    assert!(
        script.status.success(),
        "{}",
        oby_check_blok::text_output(&script)
    );
    let workflow = Command::new(env!("CARGO_BIN_EXE_oby-check-blok"))
        .arg(&repo)
        .arg("HEAD~1")
        .env("HOME", scratch.path())
        .env_remove("OBY_CHECK_ALL")
        .env_remove("OBY_CHECK_PATHS")
        .output()
        .unwrap();
    assert_eq!(
        workflow.status.code(),
        script.status.code(),
        "{}",
        oby_check_blok::text_output(&workflow)
    );
    let report: serde_json::Value = serde_json::from_slice(&workflow.stdout).unwrap();
    assert_eq!(report["verdict"], "passed");
    assert_eq!(report["workflow"], "oby-check");
    assert_eq!(report["base"], "HEAD~1");
    assert_eq!(report["nodes"]["secret-scan"]["verdict"], "passed");
    assert_eq!(report["nodes"].as_object().unwrap().len(), 1);
    assert_eq!(
        report["commit"],
        String::from_utf8(git(&repo, &["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim()
    );
    let logged =
        std::fs::read_to_string(scratch.path().join(".oby-flow/pipelines/runs.jsonl")).unwrap();
    assert_eq!(logged.as_bytes(), workflow.stdout);
    assert_eq!(logged.lines().count(), 1);
}

#[test]
fn failed_nodes_win_over_unjudged_and_the_scan_still_runs() {
    let scratch = tempfile::tempdir().unwrap();
    let repo = scratch.path().join("repo");
    std::fs::create_dir_all(repo.join("scripts")).unwrap();
    std::fs::write(
        repo.join("scripts/oby-check.sh"),
        include_str!("fixtures/check.sh"),
    )
    .unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=check",
            "-c",
            "user.email=check@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    let output = Command::new(env!("CARGO_BIN_EXE_oby-check-blok"))
        .arg(&repo)
        .arg("HEAD")
        .env("HOME", scratch.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        oby_check_blok::text_output(&output)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["nodes"]["rust-affected-tests"]["verdict"],
        "unjudged"
    );
    assert_eq!(report["nodes"]["ts-checks"]["verdict"], "failed");
    assert_eq!(
        report["nodes"]["ts-checks"]["failing"],
        serde_json::json!(["src/example.test.ts > fails"])
    );
    assert_eq!(report["nodes"]["secret-scan"]["verdict"], "passed");
    assert_eq!(report["nodes"]["swift"]["verdict"], "unjudged");
    assert_eq!(report["verdict"], "failed");
}
