//! `obid-spec`: the host. It reads the workflows, builds obid once, starts the run's local Sphere
//! stand-in when a workflow needs one, and runs the workflows in parallel, each with its own nodes
//! child (`obid-spec-nodes`, BLOK's gRPC over its stdin and stdout) and its own scratch folder, so
//! every obid it starts has its own home and port.
//!
//!   obid-spec run [--only <area>] [--tag <tag>] [--workflow <text>] [--jobs <n>] [--obid <path>] [--keep]
//!   obid-spec coverage [--only <area>] [--gate]
//!   obid-spec list
//!   obid-spec nodes

use anyhow::{bail, Context, Result};
use blok::grpc_server::proto::{node_runtime_client::NodeRuntimeClient, ExecuteRequest, NodeRef};
use obid_spec::{coverage, node_names, nodes::NODES, sphere, workflow, workflow::Workflow};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long one step may take before the host gives up on it.
const STEP_LIMIT: Duration = Duration::from_secs(300);
/// The most a lab step may ask for with its own `limit` (OBIDSPEC-16).
const LAB_STEP_MOST: Duration = Duration::from_secs(1800);

/// How long `node`'s step may take: [`STEP_LIMIT`], or for a lab node (src/lab/mod.rs) the step's
/// own `limit` in seconds, at most [`LAB_STEP_MOST`]: a machine building a Linux obid from nothing
/// takes longer than 300 s. Any other node's `limit` is its own input, never the host's.
fn step_limit(node: &str, inputs: &Value) -> Duration {
    match inputs.get("limit").and_then(Value::as_u64) {
        Some(seconds) if node.starts_with("lab.") => {
            Duration::from_secs(seconds).min(LAB_STEP_MOST)
        }
        _ => STEP_LIMIT,
    }
}
/// The most workflows at once: the Mac is shared (each runs one or two obids).
const MAX_JOBS: usize = 6;

fn root() -> PathBuf {
    std::env::var_os("OBID_SPEC_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .canonicalize()
        .expect("the repository's root")
}

fn suite() -> PathBuf {
    root().join("tools/obid-spec")
}

/// The workflows: `OBID_SPEC_WORKFLOWS` when set (a copy with a deliberately wrong expectation,
/// to see a step go red), else the suite's own.
fn workflows_dir() -> PathBuf {
    std::env::var_os("OBID_SPEC_WORKFLOWS")
        .map(PathBuf::from)
        .unwrap_or_else(|| suite().join("workflows"))
}

fn scope() -> Result<Value> {
    Ok(serde_json::from_str(&std::fs::read_to_string(
        suite().join("scope.json"),
    )?)?)
}

struct Args {
    command: String,
    only: Option<String>,
    tag: Option<String>,
    workflow: Option<String>,
    jobs: usize,
    obid: Option<PathBuf>,
    keep: bool,
    gate: bool,
}

fn args() -> Result<Args> {
    let mut list = std::env::args().skip(1);
    let command = list.next().unwrap_or_else(|| "help".into());
    let mut a = Args {
        command,
        only: None,
        tag: None,
        workflow: None,
        jobs: MAX_JOBS,
        obid: std::env::var_os("OBID_SPEC_OBID").map(PathBuf::from),
        keep: false,
        gate: false,
    };
    while let Some(flag) = list.next() {
        let mut value = || list.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--only" => a.only = Some(value()?),
            "--tag" => a.tag = Some(value()?),
            "--workflow" => a.workflow = Some(value()?),
            "--jobs" => a.jobs = value()?.parse::<usize>()?.clamp(1, MAX_JOBS),
            "--obid" => a.obid = Some(value()?.into()),
            "--keep" => a.keep = true,
            "--gate" => a.gate = true,
            other => bail!("what's {other}?"),
        }
    }
    Ok(a)
}

#[tokio::main]
async fn main() {
    let code = match run_main().await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("obid-spec: {e:#}");
            2
        }
    };
    std::process::exit(code);
}

async fn run_main() -> Result<i32> {
    let a = args()?;
    let names = node_names();
    match a.command.as_str() {
        "list" => {
            for w in workflow::load_all(&workflows_dir(), &names)? {
                let proves: usize = w.steps.iter().map(|s| s.proves.len()).sum();
                println!(
                    "{:<10} {:<55} {:>3} steps, {:>3} assertions",
                    w.area,
                    w.name,
                    w.steps.len(),
                    proves
                );
            }
            Ok(0)
        }
        "nodes" => {
            for (n, d) in NODES
                .iter()
                .copied()
                .chain(obid_spec::lab::nodes())
                .chain(obid_spec::t3_conversations::NODES.iter().copied())
                .chain(obid_spec::coverage_conversations::NODES.iter().copied())
                .chain(obid_spec::coverage_accounts::NODES.iter().copied())
            {
                println!("runtime.rust:{n:<16} {d}");
            }
            for (n, d) in obid_spec::t3_tasks::NODES {
                println!("runtime.rust:{n:<16} {d}");
            }
            Ok(0)
        }
        "coverage" => {
            // The lab proves features not yet in obid: no contract lists its scenarios yet.
            let mut workflows = workflow::load_all(&workflows_dir(), &names)?;
            workflows.retain(|w| !obid_spec::lab::is_lab(&w.area) && !workflow::is_t3(&w.area));
            let report = coverage::report(&root(), &scope()?, &workflows, a.only.as_deref())?;
            if a.gate {
                // The check's gate (OBIDSPEC-11): only what must hold now is listed and judged.
                coverage::print_gate(&report);
                Ok(if report.gate_ok() { 0 } else { 1 })
            } else {
                coverage::print(&report);
                Ok(if report.ok() { 0 } else { 1 })
            }
        }
        "run" => run(a).await,
        _ => {
            println!("obid-spec run [--only <area>] [--tag <tag>] [--workflow <text>] [--jobs <n>] [--obid <path>] [--keep]\nobid-spec coverage [--only <area>] [--gate]\nobid-spec list\nobid-spec nodes");
            Ok(if a.command == "help" { 0 } else { 2 })
        }
    }
}

/// obid, built once from this checkout (`CARGO_INCREMENTAL=0`: the Mac's disk is shared), unless
/// `--obid` or `OBID_SPEC_OBID` names one.
fn build_obid(given: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = given {
        anyhow::ensure!(p.is_file(), "{} isn't a file", p.display());
        return Ok(p);
    }
    let core = root().join("core");
    eprintln!("obid-spec: building obid once (cargo build -p obid --bin obid)");
    let status = std::process::Command::new("cargo")
        .current_dir(&core)
        .args(["build", "-q", "-p", "obid", "--bin", "obid"])
        .env("CARGO_INCREMENTAL", "0")
        .status()
        .context("cargo didn't start")?;
    anyhow::ensure!(status.success(), "obid didn't build");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| core.join("target"));
    Ok(target.join("debug/obid"))
}

/// The app, built once from this checkout for the `app.*` workflows (`npm run build` in
/// apps/electron, its packages installed offline when missing), on macOS and unless
/// `OBID_SPEC_APP` names another build. Elsewhere those workflows are skipped (OBIDSPEC-15).
fn build_app() -> Result<()> {
    if obid_spec::app::unsupported().is_some() || std::env::var_os("OBID_SPEC_APP").is_some() {
        return Ok(());
    }
    let dir = root().join("apps/electron");
    eprintln!("obid-spec: building the app once (npm run build in apps/electron)");
    let out = std::process::Command::new("npm")
        .current_dir(&dir)
        .args(["run", "--silent", "build"])
        .stdin(Stdio::null())
        .output()
        .context("npm didn't start")?;
    anyhow::ensure!(
        out.status.success(),
        "the app didn't build: {}{}",
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .rev()
            .take(15)
            .collect::<Vec<_>>()
            .join("\n"),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

#[derive(Clone, Debug, serde::Serialize)]
struct StepResult {
    id: String,
    node: String,
    proves: Vec<String>,
    /// Whether the step asserts something (an `expect`, or a check).
    asserts: bool,
    /// passed, failed, expected-red, skipped (after a failure), inactive
    result: String,
    why: Option<String>,
    seconds: f64,
    /// What the node answered, cut to 8 KB, for reading a run back (report.json).
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<Value>,
}

fn cut(v: &Value) -> Value {
    let text = v.to_string();
    if text.len() <= 8192 {
        v.clone()
    } else {
        Value::String(format!("{}…", text.chars().take(8192).collect::<String>()))
    }
}

#[derive(Clone, Debug, serde::Serialize)]
struct WorkflowResult {
    name: String,
    area: String,
    file: String,
    passed: bool,
    /// Why it didn't run here (OBIDSPEC-15): a step answered `skipped` (no macOS, no permission, no
    /// agent-desktop). A skipped workflow is neither passed nor failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<String>,
    steps: Vec<StepResult>,
    seconds: f64,
    folder: String,
}

async fn run(a: Args) -> Result<i32> {
    let names = node_names();
    let mut workflows = workflow::load_all(&workflows_dir(), &names)?;
    // The lab's workflows run only when asked for by their tag or area (src/lab/mod.rs): the merge
    // queue's, a pull request's and the nightly run never make a machine.
    let wants_lab = a.tag.as_deref() == Some(obid_spec::lab::TAG)
        || a.only.as_deref().is_some_and(obid_spec::lab::is_lab);
    if !wants_lab {
        workflows.retain(|w| !obid_spec::lab::is_lab(&w.area));
    }
    // T3 Code's bugs (workflows/t3/) the same: only when asked for by their tag or area.
    let wants_t3 = a.tag.as_deref() == Some(workflow::T3_TAG)
        || a.only.as_deref().is_some_and(workflow::is_t3);
    if !wants_t3 {
        workflows.retain(|w| !workflow::is_t3(&w.area));
    }
    if let Some(only) = &a.only {
        workflows.retain(|w| &w.area == only);
    }
    if let Some(tag) = &a.tag {
        workflows.retain(|w| w.tags.iter().any(|t| t == tag));
    }
    if let Some(text) = &a.workflow {
        workflows.retain(|w| {
            w.name.contains(text.as_str()) || w.file.to_string_lossy().contains(text.as_str())
        });
    }
    anyhow::ensure!(!workflows.is_empty(), "no workflow to run");
    let obid = build_obid(a.obid.clone())?.canonicalize()?;
    if workflows
        .iter()
        .any(|w| w.steps.iter().any(|s| s.node.starts_with("app.")))
    {
        build_app()?;
    }
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(format!("obid-spec-{stamp}-{}", std::process::id()));
    obid_spec::obid::private_dir(&base)?;
    let base = base.canonicalize()?;
    eprintln!(
        "obid-spec: {} workflows, run folder {}",
        workflows.len(),
        base.display()
    );
    let needs_sphere = workflows
        .iter()
        .any(|w| w.steps.iter().any(|s| s.node.starts_with("sphere-")));
    let served = if needs_sphere {
        eprintln!("obid-spec: starting the local Sphere stand-in");
        Some(sphere::serve(&root(), &base.join("sphere")).await?)
    } else {
        None
    };
    let nodes_bin = std::env::current_exe()?.with_file_name("obid-spec-nodes");
    anyhow::ensure!(nodes_bin.is_file(), "{} isn't built", nodes_bin.display());
    let gate = Arc::new(tokio::sync::Semaphore::new(a.jobs));
    let started = Instant::now();
    let mut handles = vec![];
    for (i, w) in workflows.into_iter().enumerate() {
        let gate = gate.clone();
        let folder = base.join(format!("{i:03}-{}", slug(&w.name)));
        let env = RunEnv {
            obid: obid.clone(),
            root: root(),
            scratch: folder,
            nodes: nodes_bin.clone(),
            sphere: served.as_ref().map(|s| (s.url.clone(), s.operator.clone())),
        };
        handles.push(tokio::spawn(async move {
            let _permit = gate.acquire().await.expect("the gate");
            let result = run_workflow(&w, &env).await;
            print_one(&result);
            result
        }));
    }
    let mut results = vec![];
    for h in handles {
        results.push(h.await?);
    }
    if let Some(s) = &served {
        obid_spec::procs::stop(s.pid, Duration::from_secs(10)).await;
    }
    std::fs::write(
        base.join("report.json"),
        serde_json::to_string_pretty(&results)?,
    )?;
    print_table(&results, started.elapsed());
    let failed = results.iter().any(failed);
    if !a.keep {
        for r in results.iter().filter(|r| !self::failed(r)) {
            let _ = std::fs::remove_dir_all(&r.folder);
        }
        if !failed {
            let _ = std::fs::remove_dir_all(&base);
        }
    }
    if failed || a.keep {
        eprintln!("obid-spec: logs and homes kept in {}", base.display());
    }
    Ok(if failed { 1 } else { 0 })
}

fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let mut out = String::new();
    for part in s.split('-').filter(|p| !p.is_empty()) {
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(part);
    }
    out.chars().take(60).collect()
}

struct RunEnv {
    obid: PathBuf,
    root: PathBuf,
    scratch: PathBuf,
    nodes: PathBuf,
    sphere: Option<(String, String)>,
}

async fn run_workflow(w: &Workflow, env: &RunEnv) -> WorkflowResult {
    let start = Instant::now();
    let mut result = WorkflowResult {
        name: w.name.clone(),
        area: w.area.clone(),
        file: w.file.display().to_string(),
        passed: false,
        skipped: None,
        steps: vec![],
        seconds: 0.0,
        folder: env.scratch.display().to_string(),
    };
    if let Err(e) = obid_spec::obid::private_dir(&env.scratch) {
        result.steps.push(failed_step("setup", &format!("{e:#}")));
        return result;
    }
    let mut command = tokio::process::Command::new(&env.nodes);
    command
        .env("OBID_SPEC_OBID", &env.obid)
        .env("OBID_SPEC_ROOT", &env.root)
        .env("OBID_SPEC_SCRATCH", &env.scratch)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    if let Some((url, operator)) = &env.sphere {
        command
            .env("OBID_SPEC_SPHERE_URL", url)
            .env("OBID_SPEC_SPHERE_OPERATOR", operator);
    }
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            result.steps.push(failed_step(
                "setup",
                &format!("the nodes didn't start: {e}"),
            ));
            return result;
        }
    };
    let mut client = match obid_spec::pipes_client(&mut child).await {
        Ok(c) => c,
        Err(e) => {
            result.steps.push(failed_step(
                "setup",
                &format!("the nodes didn't answer: {e:#}"),
            ));
            return result;
        }
    };
    let mut ctx = json!({"state": {}, "run": {"scratch": env.scratch, "root": env.root}});
    let mut broken = false;
    for step in &w.steps {
        let t = Instant::now();
        let mut r = StepResult {
            id: step.id.clone(),
            node: step.node.clone(),
            proves: step.proves.clone(),
            asserts: step.inputs.get("expect").is_some(),
            result: String::new(),
            why: None,
            seconds: 0.0,
            output: None,
        };
        if !step.active {
            r.result = "inactive".into();
            result.steps.push(r);
            continue;
        }
        if broken || result.skipped.is_some() {
            r.result = "skipped".into();
            result.steps.push(r);
            continue;
        }
        let outcome = match workflow::resolve(&step.inputs, &ctx) {
            Ok(inputs) => execute(&mut client, &step.node, &inputs).await,
            Err(e) => Err(format!("{e:#}")),
        };
        // A step that can't run here (OBIDSPEC-15): the rest of the workflow is skipped and the
        // workflow is marked skipped, never passed.
        let outcome = match outcome {
            Ok(out) if out["skipped"].is_string() => {
                let why = out["skipped"].as_str().unwrap_or_default().to_string();
                r.result = "skipped".into();
                r.why = Some(why.clone());
                result.skipped = Some(why);
                r.seconds = t.elapsed().as_secs_f64();
                result.steps.push(r);
                continue;
            }
            other => other,
        };
        // The step's assertion proven able to fail (OBIDSPEC-6): the answer changed at each place
        // it looks fails it every time, and a step that proves a scenario asserts something.
        let outcome = outcome.and_then(|out| {
            let vacuous = out["red"]["vacuous"].as_array().map(Vec::len).unwrap_or(0);
            if vacuous > 0 {
                return Err(format!(
                    "a vacuous assertion: it passes with {} of its answer's places made wrong: {}",
                    vacuous, out["red"]["vacuous"]
                ));
            }
            if !step.proves.is_empty() && out["red"]["mutants"].as_u64().unwrap_or(0) == 0 {
                return Err("it proves a scenario but asserts nothing (no expect)".into());
            }
            Ok(out)
        });
        r.seconds = t.elapsed().as_secs_f64();
        match (outcome, &step.expected_red) {
            (Ok(out), None) => {
                r.output = Some(cut(&out));
                ctx["state"][&step.id] = out;
                r.result = "passed".into();
            }
            (Ok(out), Some(issue)) => {
                ctx["state"][&step.id] = out;
                r.result = "failed".into();
                r.why = Some(format!(
                    "expected red ({issue}) but it passed: the bug is fixed, so remove expectedRed"
                ));
                broken = true;
            }
            (Err(why), Some(issue)) => {
                r.result = "expected-red".into();
                r.why = Some(format!(
                    "{issue}: {}",
                    why.chars().take(400).collect::<String>()
                ));
            }
            (Err(why), None) => match &step.flaky {
                Some(issue) => {
                    r.result = "flaky".into();
                    r.why = Some(format!(
                        "{issue}: {}",
                        why.chars().take(400).collect::<String>()
                    ));
                }
                None => {
                    r.result = "failed".into();
                    r.why = Some(why);
                    broken = true;
                }
            },
        }
        result.steps.push(r);
    }
    // Whatever the workflow ended in, what it started stops.
    let cleanup = execute(&mut client, "cleanup", &json!({})).await;
    if let Err(e) = cleanup {
        result.steps.push(failed_step("cleanup", &e));
    }
    let _ = child.kill().await;
    result.passed = result.skipped.is_none() && result.steps.iter().all(|s| s.result != "failed");
    result.seconds = start.elapsed().as_secs_f64();
    result
}

fn failed_step(id: &str, why: &str) -> StepResult {
    StepResult {
        id: id.into(),
        node: String::new(),
        proves: vec![],
        asserts: false,
        result: "failed".into(),
        why: Some(why.into()),
        seconds: 0.0,
        output: None,
    }
}

async fn execute(
    client: &mut NodeRuntimeClient<tonic::transport::Channel>,
    node: &str,
    inputs: &Value,
) -> std::result::Result<Value, String> {
    let request = ExecuteRequest {
        node: Some(NodeRef {
            name: node.into(),
            r#type: "runtime.rust".into(),
            version: String::new(),
        }),
        inputs: serde_json::to_vec(inputs).map_err(|e| e.to_string())?,
        ..Default::default()
    };
    let limit = step_limit(node, inputs);
    let answer = tokio::time::timeout(limit, client.execute(request))
        .await
        .map_err(|_| format!("ran past {} s", limit.as_secs()))?
        .map_err(|e| format!("the node's transport failed: {e}"))?
        .into_inner();
    if !answer.success {
        return Err(format!(
            "the node failed: {}",
            String::from_utf8_lossy(&answer.data)
                .chars()
                .take(600)
                .collect::<String>()
        ));
    }
    let said: Value =
        serde_json::from_slice(&answer.data).map_err(|e| format!("the node's answer: {e}"))?;
    if said["ok"] == true {
        Ok(said["output"].clone())
    } else {
        Err(said["error"]
            .as_str()
            .unwrap_or("the node failed")
            .to_string())
    }
}

/// Failed: neither passed nor skipped.
fn failed(r: &WorkflowResult) -> bool {
    !r.passed && r.skipped.is_none()
}

fn mark(r: &WorkflowResult) -> &'static str {
    match (r.passed, &r.skipped) {
        (true, _) => "pass",
        (false, Some(_)) => "skip",
        (false, None) => "FAIL",
    }
}

fn print_one(r: &WorkflowResult) {
    eprintln!("obid-spec: {} {} ({:.1} s)", mark(r), r.name, r.seconds);
    if let Some(why) = &r.skipped {
        eprintln!("    {} › {why}", r.name);
    }
    for s in r.steps.iter().filter(|s| s.result == "failed") {
        eprintln!(
            "    {} › {}: {}",
            r.name,
            s.id,
            s.why.as_deref().unwrap_or("")
        );
    }
    // On CI the run folder is only in a large artifact: a failed workflow shows the end of each
    // scratch obid's own log here, so the reason is in the job's log (obid takes secrets out of
    // its log lines).
    if failed(r) && std::env::var_os("GITHUB_ACTIONS").is_some() {
        for log in obid_logs(std::path::Path::new(&r.folder)) {
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            eprintln!("    {} › the end of {}:", r.name, log.display());
            for l in &lines[lines.len().saturating_sub(40)..] {
                eprintln!("      {l}");
            }
        }
    }
}

/// Every `obid.log` under a workflow's folder, at most three levels down.
fn obid_logs(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = vec![];
    let mut dirs = vec![(folder.to_path_buf(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() && depth < 3 {
                dirs.push((p, depth + 1));
            } else if p.file_name().is_some_and(|n| n == "obid.log") {
                found.push(p);
            }
        }
    }
    found.sort();
    found
}

fn print_table(results: &[WorkflowResult], took: Duration) {
    println!();
    println!(
        "{:<6} {:<10} {:<52} {:>7} {:>10} {:>7}",
        "", "area", "workflow", "steps", "asserts", "red"
    );
    let mut sorted: Vec<&WorkflowResult> = results.iter().collect();
    sorted.sort_by(|a, b| {
        (a.area.as_str(), a.name.as_str()).cmp(&(b.area.as_str(), b.name.as_str()))
    });
    for r in &sorted {
        let passed = r.steps.iter().filter(|s| s.result == "passed").count();
        let asserts = r
            .steps
            .iter()
            .filter(|s| s.asserts && s.result == "passed")
            .count();
        let all_asserts = r.steps.iter().filter(|s| s.asserts).count();
        let red = r
            .steps
            .iter()
            .filter(|s| s.result == "expected-red")
            .count();
        println!(
            "{:<6} {:<10} {:<52} {:>7} {:>10} {:>7}",
            mark(r),
            r.area,
            r.name.chars().take(52).collect::<String>(),
            format!("{passed}/{}", r.steps.len()),
            format!("{asserts}/{all_asserts}"),
            red
        );
    }
    let failed = results.iter().filter(|r| failed(r)).count();
    let skipped: Vec<&WorkflowResult> = results.iter().filter(|r| r.skipped.is_some()).collect();
    for r in &skipped {
        println!(
            "  skipped, not run here: {}: {}",
            r.name,
            r.skipped.as_deref().unwrap_or("")
        );
    }
    let reds: Vec<String> = results
        .iter()
        .flat_map(|r| {
            r.steps
                .iter()
                .filter(|s| s.result == "expected-red")
                .map(move |s| {
                    format!(
                        "  expected red: {} › {}: {}",
                        r.name,
                        s.id,
                        s.why.as_deref().unwrap_or("")
                    )
                })
        })
        .collect();
    for line in reds {
        println!("{line}");
    }
    for r in results {
        for s in r.steps.iter().filter(|s| s.result == "flaky") {
            println!(
                "  flaky, failed this run: {} › {}: {}",
                r.name,
                s.id,
                s.why.as_deref().unwrap_or("")
            );
        }
    }
    println!(
        "\n{} workflows: {} passed, {failed} failed, {} skipped, in {:.0} s",
        results.len(),
        results.iter().filter(|r| r.passed).count(),
        skipped.len(),
        took.as_secs_f64()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obidspec_16_a_lab_step_takes_its_own_limit_capped_and_no_other_step_does() {
        let limit = |n: u64| json!({ "limit": n });
        assert_eq!(
            step_limit("lab.machine.obid-build", &limit(1200)),
            Duration::from_secs(1200)
        );
        assert_eq!(
            step_limit("lab.machine.obid-build", &limit(5000)),
            LAB_STEP_MOST,
            "capped"
        );
        assert_eq!(step_limit("lab.machine.run", &json!({})), STEP_LIMIT);
        assert_eq!(
            step_limit("call", &limit(1200)),
            STEP_LIMIT,
            "not a lab node"
        );
        assert_eq!(step_limit("app.read", &limit(60)), STEP_LIMIT);
        assert_eq!(
            step_limit("lab.machine.run", &json!({ "limit": "1200" })),
            STEP_LIMIT,
            "seconds as a number only"
        );
    }
}
