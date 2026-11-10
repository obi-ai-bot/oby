use anyhow::{bail, Context, Result};
use blok::grpc_server::proto::{
    node_runtime_client::NodeRuntimeClient, ExecuteRequest, HealthRequest, NodeRef,
};
use oby_check_blok::{check_command, fold, text_output, CheckInput, CheckResult, Verdict, GROUPS};
use serde::Serialize;
use std::io::Read;
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::io::{AsyncBufReadExt, BufReader};
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
        let output = check_command(input, group)
            .env("OBY_CHECK_PLAN", "1")
            .output()
            .await?;
        if !output.status.success() {
            bail!("script planning failed for {group}");
        }
        let text = text_output(&output);
        let has_steps = planned_group(&text, group);
        if has_steps {
            if group == "swift" {
                nodes.insert(
                    name.into(),
                    CheckResult::unjudged(
                        "Swift has planned work but no shadow node in this pilot",
                        0.0,
                    ),
                );
            } else {
                needed.push(name);
            }
        }
    }
    if needed.is_empty() {
        return Ok(());
    }
    let mut random = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let binary = std::env::current_exe()?.with_file_name("nodes");
    let mut child = Command::new(&binary)
        .env("GRPC_PORT", "0")
        .env("OBY_CHECK_DIR", &input.checkout)
        .env("OBY_CHECK_BASE", &input.base)
        .env("OBY_CHECK_TOKEN", &token)
        .env_remove("BLOK_BLOB_DIR")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("start sibling nodes binary (build both binaries first)")?;
    // blok-rs logs the actual local address after binding, including with port 0.
    let mut stdout = BufReader::new(child.stdout.take().context("nodes stdout missing")?).lines();
    let port = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(line) = stdout.next_line().await? {
            if let Some(rest) = line
                .split_once("listening on 127.0.0.1:")
                .map(|(_, rest)| rest)
            {
                let port: u16 = rest
                    .split_whitespace()
                    .next()
                    .context("missing bound port")?
                    .parse()?;
                anyhow::ensure!(port != 0, "nodes reported port zero");
                return Ok::<_, anyhow::Error>(port);
            }
        }
        bail!("nodes exited before reporting its bound port")
    })
    .await
    .context("nodes did not report its bound port")??;
    let endpoint = format!("http://127.0.0.1:{port}");
    let start = Instant::now();
    let mut client = loop {
        if let Some(status) = child.try_wait()? {
            bail!("nodes exited before ready: {status}");
        }
        if let Ok(mut client) = NodeRuntimeClient::connect(endpoint.clone()).await {
            if client.health(HealthRequest::default()).await.is_ok() && child.try_wait()?.is_none()
            {
                break client;
            }
        }
        if start.elapsed() > Duration::from_secs(10) {
            bail!("nodes did not become ready");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let result = async {
        for name in needed {
            let start = Instant::now();
            let request = ExecuteRequest {
                node: Some(NodeRef {
                    name: name.into(),
                    r#type: "runtime.rust".into(),
                    version: String::new(),
                }),
                inputs: serde_json::to_vec(&serde_json::json!({"token": token}))?,
                ..Default::default()
            };
            let result = match client.execute(request).await {
                Ok(response) if response.get_ref().success => {
                    serde_json::from_slice(&response.into_inner().data).unwrap_or_else(|_| {
                        CheckResult::unjudged(
                            "invalid node response",
                            start.elapsed().as_secs_f64(),
                        )
                    })
                }
                _ => CheckResult::unjudged(
                    "node execution or transport failed",
                    start.elapsed().as_secs_f64(),
                ),
            };
            nodes.insert(name.into(), result);
        }
        Ok(())
    }
    .await;
    child.kill().await?;
    child.wait().await?;
    result
}

fn planned_group(text: &str, group: &str) -> bool {
    text.lines()
        .any(|line| line == format!("oby-check: group {group}"))
}

async fn run() -> Result<Verdict> {
    let mut args = std::env::args().skip(1);
    let checkout = args
        .next()
        .context("usage: oby-check-blok <checkout> [base]")?;
    let base = args.next().unwrap_or_else(|| "origin/dev".into());
    if args.next().is_some() {
        bail!("usage: oby-check-blok <checkout> [base]");
    }
    let checkout = std::fs::canonicalize(checkout)?
        .to_string_lossy()
        .into_owned();
    let input = CheckInput {
        checkout,
        base: base.clone(),
    };
    let commit = Command::new("git")
        .current_dir(&input.checkout)
        .args(["rev-parse", "HEAD"])
        .kill_on_drop(true)
        .output()
        .await?;
    if !commit.status.success() {
        bail!("checkout has no HEAD");
    }
    let mut report = Run {
        commit: String::from_utf8(commit.stdout)?.trim().into(),
        base,
        workflow: "oby-check",
        nodes: BTreeMap::new(),
        verdict: Verdict::Unjudged,
        started: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(error) = workflow(&input, &mut report.nodes).await {
        eprintln!("shadow check: {error:#}");
        report.nodes.insert(
            "workflow".into(),
            CheckResult::unjudged("workflow could not complete; see stderr", 0.0),
        );
    }
    report.verdict = fold(report.nodes.values().map(|n| n.verdict));
    let line = serde_json::to_string(&report)?;
    let folder = PathBuf::from(std::env::var_os("HOME").context("HOME is unset")?)
        .join(".oby-flow/pipelines");
    std::fs::create_dir_all(&folder)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(folder.join("runs.jsonl"))?;
    writeln!(file, "{line}")?;
    println!("{line}");
    Ok(report.verdict)
}

#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(verdict) => verdict.exit_code(),
        Err(error) => {
            eprintln!("shadow check: {error:#}");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_exact_group_lines_select_nodes() {
        let text = "==> cargo test\noby-check: the Rust core has lines\ncouldn't judge: test\noby-check: jobs.rs\ngitleaks isn't installed\nApple apps and swift test don't run\n oby-check: group rust\noby-check: group rust extra\noby-check: group scan\n";
        assert!(!super::planned_group(text, "rust"));
        assert!(!super::planned_group(text, "swift"));
        assert!(super::planned_group(text, "scan"));
    }
}
