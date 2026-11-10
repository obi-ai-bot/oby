use anyhow::{bail, Context, Result};
use blok::grpc_server::proto::{
    node_runtime_client::NodeRuntimeClient, ExecuteRequest, HealthRequest, NodeRef,
};
use oby_check_blok::{
    check_command, fold, parse_workflow, text_output, CheckInput, CheckResult, Step, Verdict,
    DEFAULT_WORKFLOW, GROUPS,
};
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
    /// Where the workflow came from: OBY_CHECK_WORKFLOW's path, the owner's saved copy, or built-in.
    workflow_file: String,
    nodes: BTreeMap<String, CheckResult>,
    verdict: Verdict,
    reason: String,
    started: String,
}

// The workflow file: OBY_CHECK_WORKFLOW, else the copy the pipelines editor saves, else this tool's own.
fn load_workflow() -> Result<(Vec<Step>, String)> {
    let saved = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".oby-flow/workflows/oby-check.json"));
    let path = std::env::var_os("OBY_CHECK_WORKFLOW")
        .map(PathBuf::from)
        .or(saved.filter(|p| p.exists()));
    match path {
        Some(path) => {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("can't read {}", path.display()))?;
            let steps = parse_workflow(&text).with_context(|| format!("{}", path.display()))?;
            Ok((steps, path.display().to_string()))
        }
        None => Ok((parse_workflow(DEFAULT_WORKFLOW)?, "built-in".into())),
    }
}

async fn workflow(
    input: &CheckInput,
    steps: &[Step],
    nodes: &mut BTreeMap<String, CheckResult>,
) -> Result<bool> {
    let mut needed = vec![];
    let mut has_steps = false;
    let mut has_groups = false;
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
        has_steps |= text.lines().any(|line| line.starts_with("==>"));
        has_groups |= text
            .lines()
            .any(|line| line.starts_with("oby-check: group "));
        if planned_group(&text, group) {
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
    if has_steps && !has_groups {
        bail!("the script names no groups (older than #2052)");
    }
    if needed.is_empty() {
        anyhow::ensure!(
            !nodes.is_empty() || !has_steps,
            "the plan has steps but no supported groups"
        );
        return Ok(nodes.is_empty() && !has_groups);
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
        // A group the script planned but no step runs can't be called judged.
        for name in &needed {
            if !steps.iter().any(|s| s.node == *name) {
                nodes.insert(
                    (*name).into(),
                    CheckResult::unjudged("the workflow has no step for this group", 0.0),
                );
            }
        }
        let mut stopped: Option<&str> = None;
        for step in steps.iter().filter(|s| needed.contains(&s.node)) {
            if let Some(at) = stopped {
                nodes.insert(
                    step.id.clone(),
                    CheckResult::unjudged(format!("the workflow stopped after {at}"), 0.0),
                );
                continue;
            }
            if !step.active {
                nodes.insert(
                    step.id.clone(),
                    CheckResult::unjudged("skipped in the workflow (active: false)", 0.0),
                );
                continue;
            }
            let mut result = CheckResult::unjudged("never ran", 0.0);
            // Retries are for a node that couldn't judge (a transport error, a timeout), never a failure.
            for _ in 0..step.attempts {
                result = execute(&mut client, step, &token).await?;
                if result.verdict != Verdict::Unjudged {
                    break;
                }
            }
            nodes.insert(step.id.clone(), result);
            if step.stop {
                stopped = Some(&step.id);
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    child.kill().await?;
    child.wait().await?;
    result.map(|()| false)
}

async fn execute(
    client: &mut NodeRuntimeClient<tonic::transport::Channel>,
    step: &Step,
    token: &str,
) -> Result<CheckResult> {
    let start = Instant::now();
    let request = ExecuteRequest {
        node: Some(NodeRef {
            name: step.node.into(),
            r#type: "runtime.rust".into(),
            version: String::new(),
        }),
        inputs: serde_json::to_vec(&serde_json::json!({"token": token}))?,
        ..Default::default()
    };
    let call = client.execute(request);
    let answer = match step.max {
        Some(max) => match tokio::time::timeout(max, call).await {
            Ok(answer) => answer,
            Err(_) => {
                return Ok(CheckResult::unjudged(
                    format!("ran past its maxDuration of {}s", max.as_secs()),
                    start.elapsed().as_secs_f64(),
                ))
            }
        },
        None => call.await,
    };
    Ok(match answer {
        Ok(response) if response.get_ref().success => {
            serde_json::from_slice(&response.into_inner().data).unwrap_or_else(|_| {
                CheckResult::unjudged("invalid node response", start.elapsed().as_secs_f64())
            })
        }
        _ => CheckResult::unjudged(
            "node execution or transport failed",
            start.elapsed().as_secs_f64(),
        ),
    })
}

fn planned_group(text: &str, group: &str) -> bool {
    text.lines()
        .any(|line| line == format!("oby-check: group {group}"))
}

async fn report(checkout: String, base: String) -> Result<Run> {
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
        workflow_file: String::new(),
        nodes: BTreeMap::new(),
        verdict: Verdict::Unjudged,
        reason: String::new(),
        started: chrono::Utc::now().to_rfc3339(),
    };
    let outcome = match load_workflow() {
        Ok((steps, file)) => {
            report.workflow_file = file;
            workflow(&input, &steps, &mut report.nodes).await
        }
        Err(error) => Err(error),
    };
    let nothing_to_check = matches!(outcome, Ok(true));
    if let Err(error) = outcome {
        eprintln!("shadow check: {error:#}");
        report.nodes.insert(
            "workflow".into(),
            CheckResult::unjudged(format!("{error:#}"), 0.0),
        );
    }
    report.verdict = fold(report.nodes.values().map(|n| n.verdict));
    report.reason = if nothing_to_check {
        report.verdict = Verdict::Passed;
        "nothing to check".into()
    } else if report.nodes.is_empty() {
        "no nodes ran".into()
    } else {
        report
            .nodes
            .values()
            .filter(|node| node.verdict != Verdict::Passed)
            .map(|node| node.reason.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    };
    Ok(report)
}

async fn affected(base: String) -> serde_json::Value {
    match report(".".into(), base).await {
        Ok(report) => {
            let failing: std::collections::BTreeSet<_> = report
                .nodes
                .values()
                .flat_map(|node| node.failing.iter().cloned())
                .collect();
            serde_json::json!({"verdict": report.verdict, "failing": failing, "reason": report.reason, "nodes": report.nodes})
        }
        Err(error) => {
            serde_json::json!({"verdict": "unjudged", "failing": [], "reason": format!("{error:#}"), "nodes": {}})
        }
    }
}

async fn mcp() -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                println!(
                    "{}",
                    serde_json::json!({"jsonrpc":"2.0", "id":null, "error":{"code":-32700,"message":"Parse error"}})
                );
                continue;
            }
        };
        let Some(id) = request.get("id") else {
            continue;
        };
        let result = match request["method"].as_str() {
            Some("initialize") => Ok(
                serde_json::json!({"protocolVersion":"2024-11-05", "capabilities":{"tools":{}}, "serverInfo":{"name":"oby-check-blok", "version":env!("CARGO_PKG_VERSION")}}),
            ),
            Some("ping") => Ok(serde_json::json!({})),
            Some("tools/list") => Ok(
                serde_json::json!({"tools":[{"name":"oby_check_affected", "description":"Run affected checks offline; obid runs the full check after the turn", "inputSchema":{"type":"object","properties":{"base":{"type":"string"}},"additionalProperties":false}}]}),
            ),
            Some("tools/call") => {
                let params = &request["params"];
                let args = &params["arguments"];
                if params["name"] != "oby_check_affected"
                    || !(args.is_null()
                        || args.as_object().is_some_and(|args| {
                            args.iter()
                                .all(|(key, value)| key == "base" && value.is_string())
                        }))
                {
                    Err((-32602, "Invalid tool or arguments"))
                } else {
                    let verdict =
                        affected(args["base"].as_str().unwrap_or("origin/dev").into()).await;
                    Ok(
                        serde_json::json!({"content":[{"type":"text", "text":serde_json::to_string(&verdict)?}]}),
                    )
                }
            }
            _ => Err((-32601, "Method not found")),
        };
        let response = match result {
            Ok(result) => serde_json::json!({"jsonrpc":"2.0", "id":id, "result":result}),
            Err((code, message)) => {
                serde_json::json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
            }
        };
        println!("{response}");
        std::io::stdout().flush()?;
    }
    Ok(())
}

async fn run() -> Result<Verdict> {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .context("usage: oby-check-blok --affected [base] | mcp | <checkout> [base]")?;
    if mode == "mcp" {
        anyhow::ensure!(args.next().is_none(), "mcp takes no arguments");
        mcp().await?;
        return Ok(Verdict::Passed);
    }
    let base = args.next().unwrap_or_else(|| "origin/dev".into());
    anyhow::ensure!(args.next().is_none(), "too many arguments");
    if mode == "--affected" {
        let result = affected(base).await;
        println!("{result}");
        eprintln!("Affected check: {}", result["verdict"].as_str().unwrap());
        return Ok(serde_json::from_value(result["verdict"].clone())?);
    }
    let report = report(mode, base).await?;
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
