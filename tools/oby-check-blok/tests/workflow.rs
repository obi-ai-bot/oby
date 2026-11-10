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
