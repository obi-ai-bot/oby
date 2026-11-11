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

// How long the nodes child may take to report its port, then to answer (a cold Mac is slow).
const STARTUP: Duration = Duration::from_secs(60);

// The workflow file: OBY_CHECK_WORKFLOW, else the copy the pipelines editor saves, else this tool's
// own. The real check (`check`) runs only its own (BLOK-2).
fn load_workflow(check: bool) -> Result<(Vec<Step>, String)> {
    if check {
        return Ok((parse_workflow(DEFAULT_WORKFLOW)?, "built-in".into()));
    }
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
    check: bool,
) -> Result<bool> {
    // The real check plans what obid asked for (OBY_CHECK_PATHS on a job's base, PUBLISHING-18;
    // OBY_CHECK_ALL); the shadow and affected runs plan what changed.
    let scope = |command: &mut Command| {
        if !check {
            command
                .env_remove("OBY_CHECK_ALL")
                .env_remove("OBY_CHECK_PATHS");
        }
    };
    let mut needed = vec![];
    let mut has_steps = false;
    let mut has_groups = false;
    // Planning, including skipped suites and line limits, belongs to the shell.
    for (name, group) in GROUPS {
        let mut plan = check_command(input, group);
        scope(plan.env("OBY_CHECK_PLAN", "1"));
        let output = plan.output().await?;
        let text = text_output(&output);
        if !output.status.success() {
            let said = text.trim().lines().last().unwrap_or_default();
            bail!("script planning failed for {group}: {said}");
        }
        has_steps |= text.lines().any(|line| line.starts_with("==>"));
        has_groups |= text
            .lines()
            .any(|line| line.starts_with("oby-check: group "));
        if planned_group(&text, group) {
            needed.push(name);
        }
    }
    if has_steps && !has_groups {
        bail!("the script names no groups (older than #2052)");
    }
    if needed.is_empty() {
        anyhow::ensure!(!has_steps, "the plan has steps but no supported groups");
        return Ok(!has_groups);
    }
    let mut random = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let binary = std::env::current_exe()?.with_file_name("oby-check-blok-nodes");
    let mut nodes_child = Command::new(&binary);
    scope(&mut nodes_child);
    let mut child = nodes_child
        .env("OBY_CHECK_DIR", &input.checkout)
        .env("OBY_CHECK_BASE", &input.base)
        .env("OBY_CHECK_TOKEN", &token)
        .env_remove("BLOK_BLOB_DIR")
        // gRPC over these two pipes, never a port (BLOK-3).
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // The real check shows each node's script as it runs (PUBLISHING-16); the others print
        // only their verdict.
        .stderr(if check {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .kill_on_drop(true)
        .spawn()
        .context("start sibling oby-check-blok-nodes binary (build both binaries first)")?;
    let mut client = oby_check_blok::pipes_client(&mut child).await?;
    // Its first answer waits for it to start (a cold Mac is slow); one that broke the pipes, its
    // exit.
    tokio::time::timeout(STARTUP, async {
        loop {
            if client.health(HealthRequest::default()).await.is_ok() {
                return Ok(());
            }
            if let Some(status) = child.try_wait()? {
                bail!("nodes exited before ready: {status}");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .context("nodes did not become ready")??;
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

async fn report(checkout: String, base: String, check: bool) -> Result<Run> {
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
    let outcome = match load_workflow(check) {
        Ok((steps, file)) => {
            report.workflow_file = file;
            workflow(&input, &steps, &mut report.nodes, check).await
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
    match report(".".into(), base, false).await {
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

/// The script's whole run, as obid reads it: each node's output as it comes, then `oby-check:
/// passed` (0), or `couldn't judge: <why>` (2) or nothing, and `oby-check: failed: <nodes>` (1).
/// No record in runs.jsonl.
async fn check(base: String) -> Result<Verdict> {
    let report = report(".".into(), base, true).await?;
    let (mut verdict, mut why) = (report.verdict, report.reason);
    // Never a pass with nothing scanned (F-M2-35): a plan with no work, or one without the scan.
    let scanned = report
        .nodes
        .get("secret-scan")
        .is_some_and(|node| node.verdict == Verdict::Passed);
    if verdict == Verdict::Passed && !scanned {
        verdict = Verdict::Unjudged;
        why = "the secret scan judged nothing, and nothing passes unscanned".into();
    }
    if verdict == Verdict::Passed {
        println!("oby-check: passed");
        return Ok(verdict);
    }
    if verdict == Verdict::Unjudged {
        println!("couldn't judge: {why}");
    }
    let names: Vec<_> = report
        .nodes
        .iter()
        .filter(|(_, node)| node.verdict != Verdict::Passed)
        .map(|(name, _)| name.as_str())
        .collect();
    let names = if names.is_empty() {
        why
    } else {
        names.join(", ")
    };
    eprintln!("oby-check: failed: {names}");
    Ok(verdict)
}

async fn run() -> Result<Verdict> {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .context("usage: oby-check-blok check [base] | --affected [base] | mcp | validate <workflow.json> | <checkout> [base]")?;
    // The same rules a run applies, for an editor checking a draft before it saves it.
    if mode == "validate" {
        let file = args
            .next()
            .context("usage: oby-check-blok validate <workflow.json>")?;
        anyhow::ensure!(args.next().is_none(), "validate takes one file");
        let checked = std::fs::read_to_string(&file)
            .map_err(anyhow::Error::from)
            .and_then(|text| parse_workflow(&text));
        let ok = checked.is_ok();
        let line = match checked {
            Ok(steps) => {
                serde_json::json!({"valid": true, "steps": steps.iter().map(|s| &s.id).collect::<Vec<_>>()})
            }
            Err(error) => serde_json::json!({"valid": false, "error": format!("{error:#}")}),
        };
        println!("{line}");
        return Ok(if ok {
            Verdict::Passed
        } else {
            Verdict::Unjudged
        });
    }
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
    if mode == "check" {
        return check(base).await;
    }
    let report = report(mode, base, false).await?;
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
