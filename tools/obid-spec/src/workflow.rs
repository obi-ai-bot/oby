//! The suite's workflows: BLOK v2 files (`schemaVersion` "2"), so BLOK Studio can show and edit
//! them, under `workflows/routes/<area>/` (one per route, its trigger `http` naming the route's
//! method and path) and `workflows/triggers/` (one per background trigger, its trigger
//! `manual`). Steps are regular steps on this suite's nodes (`runtime.rust:<node>`) with their
//! `inputs`; each step's output is kept as `state.<id>`, as BLOK keeps `ctx.state[id]`.
//!
//! What this host adds, in each step's free-form `ui`: `proves`, the contract scenarios
//! (`SPHERE-21`) the step's assertion proves; `expectedRed`, a known bug's issue, so the step
//! is expected to fail until it's fixed (and a pass then fails the run, to be noticed).

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Workflow {
    pub name: String,
    pub file: PathBuf,
    /// `routes/<area>` → the area; `triggers` → `triggers`.
    pub area: String,
    /// The route it proves (`POST /v1/obid/sphere/join`), for a route's workflow.
    pub route: Option<String>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug)]
pub struct Step {
    pub id: String,
    pub node: String,
    pub inputs: Value,
    pub proves: Vec<String>,
    pub expected_red: Option<String>,
    /// `ui.flaky`: a known intermittent bug's issue. Its failure is listed and doesn't fail the
    /// run; its pass is a pass.
    pub flaky: Option<String>,
    pub active: bool,
}

const UNSUPPORTED: [&str; 8] = [
    "branch",
    "subworkflow",
    "wait",
    "forEach",
    "loop",
    "switch",
    "tryCatch",
    "parallel",
];

pub fn parse(text: &str, file: &Path, area: &str, nodes: &[&str]) -> Result<Workflow> {
    let doc: Value = serde_json::from_str(text).context("isn't JSON")?;
    if doc["schemaVersion"] != "2" {
        bail!("isn't BLOK schemaVersion 2");
    }
    let name = doc["name"].as_str().context("has no name")?.to_string();
    let route = match doc["trigger"].get("http") {
        Some(http) => {
            let method = http["method"]
                .as_str()
                .context("trigger.http has no method")?;
            let path = http["path"].as_str().context("trigger.http has no path")?;
            Some(format!("{} {path}", method.to_uppercase()))
        }
        None if doc["trigger"].get("manual").is_some() => None,
        None => bail!("its trigger is neither http nor manual"),
    };
    if area != "triggers" && route.is_none() {
        bail!("a route's workflow has an http trigger");
    }
    let list = doc["steps"]
        .as_array()
        .filter(|s| !s.is_empty())
        .context("has no steps")?;
    let mut steps: Vec<Step> = vec![];
    for raw in list {
        let id = raw["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("a step has no id")?
            .to_string();
        if let Some(kind) = UNSUPPORTED.iter().find(|k| raw.get(**k).is_some()) {
            bail!("step {id}: {kind} steps aren't supported by this host");
        }
        if steps.iter().any(|s| s.id == id) {
            bail!("two steps are named {id}");
        }
        let used = raw["use"]
            .as_str()
            .with_context(|| format!("step {id} has no use"))?;
        let node = used
            .strip_prefix("runtime.rust:")
            .unwrap_or(used)
            .to_string();
        if !nodes.contains(&node.as_str()) {
            bail!("step {id} uses {used}, which isn't one of this suite's nodes");
        }
        let proves = raw["ui"]["proves"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str().map(str::to_string))
            .collect();
        steps.push(Step {
            id,
            node,
            inputs: raw.get("inputs").cloned().unwrap_or(json!({})),
            proves,
            expected_red: raw["ui"]["expectedRed"].as_str().map(str::to_string),
            flaky: raw["ui"]["flaky"].as_str().map(str::to_string),
            active: raw["active"].as_bool().unwrap_or(true),
        });
    }
    Ok(Workflow {
        name,
        file: file.to_path_buf(),
        area: area.to_string(),
        route,
        steps,
    })
}

/// Every workflow under `dir` (`routes/<area>/*.json`, `triggers/*.json`), sorted by file.
pub fn load_all(dir: &Path, nodes: &[&str]) -> Result<Vec<Workflow>> {
    let mut files = vec![];
    let routes = dir.join("routes");
    if routes.is_dir() {
        for area in std::fs::read_dir(&routes)? {
            let area = area?.path();
            if area.is_dir() {
                for f in std::fs::read_dir(&area)? {
                    let f = f?.path();
                    if f.extension().is_some_and(|e| e == "json") {
                        let name = area.file_name().unwrap().to_string_lossy().to_string();
                        files.push((f, name));
                    }
                }
            }
        }
    }
    let triggers = dir.join("triggers");
    if triggers.is_dir() {
        for f in std::fs::read_dir(&triggers)? {
            let f = f?.path();
            if f.extension().is_some_and(|e| e == "json") {
                files.push((f, "triggers".to_string()));
            }
        }
    }
    files.sort();
    let mut all = vec![];
    for (file, area) in files {
        let text = std::fs::read_to_string(&file)?;
        all.push(parse(&text, &file, &area, nodes).with_context(|| format!("{}", file.display()))?);
    }
    Ok(all)
}

/// `${state.alex.port}` and the like in a step's inputs, from `ctx` (`{"state": {...}, "run":
/// {...}}`). A string that is one whole `${…}` takes the value itself; one with `${…}` inside it
/// takes their text. An expression that names nothing is an error.
pub fn resolve(value: &Value, ctx: &Value) -> Result<Value> {
    Ok(match value {
        Value::String(s) => {
            if let Some(expr) = s.strip_prefix("${").and_then(|r| r.strip_suffix('}')) {
                if !expr.contains("${") && !expr.contains('}') {
                    return lookup(ctx, expr);
                }
            }
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(start) = rest.find("${") {
                out.push_str(&rest[..start]);
                let end = rest[start..]
                    .find('}')
                    .with_context(|| format!("unclosed ${{ in {s:?}"))?;
                let v = lookup(ctx, &rest[start + 2..start + end])?;
                match v {
                    Value::String(t) => out.push_str(&t),
                    other => out.push_str(&other.to_string()),
                }
                rest = &rest[start + end + 1..];
            }
            out.push_str(rest);
            Value::String(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|i| resolve(i, ctx))
                .collect::<Result<_>>()?,
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), resolve(v, ctx)?);
            }
            Value::Object(out)
        }
        other => other.clone(),
    })
}

fn lookup(ctx: &Value, expr: &str) -> Result<Value> {
    let expr = expr.trim();
    crate::expect::at(ctx, expr)
        .cloned()
        .with_context(|| format!("${{{expr}}} names nothing yet"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NODES: [&str; 2] = ["obid-start", "call"];

    #[test]
    fn a_route_workflow_reads_its_route_and_steps() {
        let w = parse(
            r##"{"schemaVersion":"2","name":"PUT /v1/obid/sphere/owner","trigger":{"http":{"method":"put","path":"/v1/obid/sphere/owner"}},
               "steps":[{"id":"obid","use":"runtime.rust:obid-start"},
                        {"id":"no-owner","use":"call","inputs":{"x":1},"ui":{"proves":["SPHERE-21"],"expectedRed":"bug 1"}}]}"##,
            Path::new("x.json"),
            "sphere",
            &NODES,
        )
        .unwrap();
        assert_eq!(w.route.as_deref(), Some("PUT /v1/obid/sphere/owner"));
        assert_eq!(w.steps[1].proves, ["SPHERE-21"]);
        assert_eq!(w.steps[1].expected_red.as_deref(), Some("bug 1"));
    }

    #[test]
    fn obidspec_2_a_step_on_another_node_or_one_this_host_cant_run_is_refused() {
        for (text, why) in [
            (
                r#"{"schemaVersion":"1","name":"x","trigger":{"manual":{}},"steps":[{"id":"a","use":"call"}]}"#,
                "schemaVersion 2",
            ),
            (
                r#"{"schemaVersion":"2","name":"x","trigger":{"manual":{}},"steps":[{"id":"a","use":"nope"}]}"#,
                "isn't one of",
            ),
            (
                r#"{"schemaVersion":"2","name":"x","trigger":{"manual":{}},"steps":[{"id":"a","use":"call","switch":{}}]}"#,
                "switch",
            ),
            (
                r#"{"schemaVersion":"2","name":"x","trigger":{"manual":{}},"steps":[{"id":"a","use":"call"},{"id":"a","use":"call"}]}"#,
                "two steps",
            ),
            (
                r#"{"schemaVersion":"2","name":"x","trigger":{"manual":{}},"steps":[{"id":"a","use":"call"}]}"#,
                "http trigger",
            ),
        ] {
            let area = if why == "http trigger" {
                "sphere"
            } else {
                "triggers"
            };
            let e = format!(
                "{:#}",
                parse(text, Path::new("x"), area, &NODES).unwrap_err()
            );
            assert!(e.contains(why), "{e}");
        }
    }

    #[test]
    fn templates_take_values_and_text() {
        let ctx = json!({"state": {"a": {"port": 7, "obj": {"k": 1}, "s": "x"}}});
        let v = resolve(
            &json!({"whole": "${state.a.obj}", "text": "p${state.a.port}-${state.a.s}", "n": 3}),
            &ctx,
        )
        .unwrap();
        assert_eq!(v, json!({"whole": {"k": 1}, "text": "p7-x", "n": 3}));
        assert!(resolve(&json!("${state.missing}"), &ctx).is_err());
    }
}
