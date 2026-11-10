use anyhow::{bail, Context, Result};
use blok::grpc_server::proto::{node_runtime_client::NodeRuntimeClient, ExecuteRequest, HealthRequest, NodeRef};
use oby_check_blok::{check_command, fold, text_output, CheckInput, CheckResult, Verdict, GROUPS};
use serde::Serialize;
use std::{collections::BTreeMap, io::Write, net::TcpListener, path::PathBuf, process::Stdio, time::{Duration, Instant}};
use tokio::process::Command;

#[derive(Serialize)]
struct Run {
    commit: String,
    base: String,
    workflow: &'static str,
    nodes: BTreeMap<String, CheckResult>,
    verdict: Verdict,
    started: String,
}

async fn workflow(input: &CheckInput, nodes: &mut BTreeMap<String, CheckResult>) -> Result<()> {
    let mut needed = vec![];
    // Planning, including skipped suites and line limits, belongs to the shell.
    for (name, group) in GROUPS.into_iter().chain([("swift", "swift")]) {
        let output = check_command(input, group).env("OBY_CHECK_PLAN", "1").output().await?;
        if !output.status.success() { bail!("script planning failed for {group}"); }
        let text = text_output(&output);
        let has_steps = text.lines().any(|l| l.starts_with("==> ") || l.starts_with("couldn't judge:") || l.starts_with("oby-check: the Rust core") || l.starts_with("oby-check: jobs.rs") || l.contains("gitleaks isn't installed") || l.contains("Apple apps and swift test don't run"));
        if has_steps {
            if group == "swift" {
                nodes.insert(name.into(), CheckResult::unjudged("Swift has planned work but no shadow node in this pilot", 0.0));
            } else { needed.push(name); }
        }
    }
    if needed.is_empty() { return Ok(()); }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let binary = std::env::current_exe()?.with_file_name("nodes");
    drop(listener);
    let mut child = Command::new(binary).env("GRPC_PORT", port.to_string())
        .env_remove("BLOK_BLOB_DIR").stdin(Stdio::null()).stdout(Stdio::null())
        .kill_on_drop(true).spawn().context("start sibling nodes binary (build both binaries first)")?;
    let result = async {
        let endpoint = format!("http://127.0.0.1:{port}");
        let start = Instant::now();
        let mut client = loop {
            if let Some(status) = child.try_wait()? { bail!("nodes exited before ready: {status}"); }
            if let Ok(mut client) = NodeRuntimeClient::connect(endpoint.clone()).await {
                if client.health(HealthRequest::default()).await.is_ok() { break client; }
            }
            if start.elapsed() > Duration::from_secs(10) { bail!("nodes did not become ready"); }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        for name in needed {
            let start = Instant::now();
            let request = ExecuteRequest {
                node: Some(NodeRef { name: name.into(), r#type: "runtime.rust".into(), version: String::new() }),
                inputs: serde_json::to_vec(input)?, ..Default::default()
            };
            let result = match client.execute(request).await {
                Ok(response) if response.get_ref().success => serde_json::from_slice(&response.into_inner().data)
                    .unwrap_or_else(|_| CheckResult::unjudged("invalid node response", start.elapsed().as_secs_f64())),
                _ => CheckResult::unjudged("node execution or transport failed", start.elapsed().as_secs_f64()),
            };
            nodes.insert(name.into(), result);
        }
        Ok(())
    }.await;
    child.kill().await?;
    result
}

async fn run() -> Result<Verdict> {
    let mut args = std::env::args().skip(1);
    let checkout = args.next().context("usage: oby-check-blok <checkout> [base]")?;
    let base = args.next().unwrap_or_else(|| "origin/dev".into());
    if args.next().is_some() { bail!("usage: oby-check-blok <checkout> [base]"); }
    let checkout = std::fs::canonicalize(checkout)?.to_string_lossy().into_owned();
    let input = CheckInput { checkout, base: base.clone() };
    let commit = Command::new("git").current_dir(&input.checkout).args(["rev-parse", "HEAD"]).kill_on_drop(true).output().await?;
    if !commit.status.success() { bail!("checkout has no HEAD"); }
    let mut report = Run {
        commit: String::from_utf8(commit.stdout)?.trim().into(), base, workflow: "oby-check",
        nodes: BTreeMap::new(), verdict: Verdict::Unjudged, started: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(error) = workflow(&input, &mut report.nodes).await {
        eprintln!("shadow check: {error:#}");
        report.nodes.insert("workflow".into(), CheckResult::unjudged("workflow could not complete; see stderr", 0.0));
    }
    report.verdict = fold(report.nodes.values().map(|n| n.verdict));
    let line = serde_json::to_string(&report)?;
    let folder = PathBuf::from(std::env::var_os("HOME").context("HOME is unset")?).join(".oby-flow/pipelines");
    std::fs::create_dir_all(&folder)?;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(folder.join("runs.jsonl"))?;
    writeln!(file, "{line}")?;
    println!("{line}");
    Ok(report.verdict)
}

#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(verdict) => verdict.exit_code(),
        Err(error) => { eprintln!("shadow check: {error:#}"); 2 }
    };
    std::process::exit(code);
}
