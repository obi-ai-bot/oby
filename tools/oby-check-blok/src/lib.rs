use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, process::Output, time::Instant};
use tokio::process::Command;

pub const GROUPS: [(&str, &str); 3] = [
    ("rust-affected-tests", "rust"),
    ("ts-checks", "electron"),
    ("secret-scan", "scan"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Passed,
    Failed,
    Unjudged,
}

impl Verdict {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Passed => 0,
            Self::Failed => 1,
            Self::Unjudged => 2,
        }
    }
}

pub fn fold(verdicts: impl IntoIterator<Item = Verdict>) -> Verdict {
    let mut verdicts = verdicts.into_iter();
    let Some(first) = verdicts.next() else {
        return Verdict::Unjudged;
    };
    verdicts.fold(first, |a, b| match (a, b) {
        (Verdict::Failed, _) | (_, Verdict::Failed) => Verdict::Failed,
        (Verdict::Unjudged, _) | (_, Verdict::Unjudged) => Verdict::Unjudged,
        _ => Verdict::Passed,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CheckInput {
    pub checkout: String,
    pub base: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NodeInput {
    #[serde(default)]
    pub token: String,
    pub checkout: Option<String>,
    pub base: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CheckResult {
    pub verdict: Verdict,
    pub failing: Vec<String>,
    pub reason: String,
    pub seconds: f64,
}

impl CheckResult {
    pub fn unjudged(reason: impl Into<String>, seconds: f64) -> Self {
        Self {
            verdict: Verdict::Unjudged,
            failing: vec![],
            reason: reason.into(),
            seconds,
        }
    }
}

/// The workflow this tool runs by default: a BLOK v2 file, so BLOK Studio can show and edit it.
pub const DEFAULT_WORKFLOW: &str = include_str!("../workflows/oby-check.json");

/// One step of the workflow, as this host runs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    /// The node it runs (one of GROUPS' names) and that node's check group.
    pub node: &'static str,
    pub group: &'static str,
    /// BLOK's `active: false` skips the step; `stop: true` halts the workflow after it.
    pub active: bool,
    pub stop: bool,
    /// BLOK's `retry.maxAttempts`: tries for a node that couldn't judge (never for a failure).
    pub attempts: u32,
    /// BLOK's `maxDuration`, per attempt.
    pub max: Option<std::time::Duration>,
}

const UNSUPPORTED: [&str; 7] = [
    "branch",
    "subworkflow",
    "wait",
    "forEach",
    "loop",
    "switch",
    "tryCatch",
];

/// Reads a BLOK v2 workflow named `oby-check` whose steps are regular steps on this tool's nodes.
/// Anything this host can't run is refused with a reason rather than run differently.
pub fn parse_workflow(text: &str) -> anyhow::Result<Vec<Step>> {
    use anyhow::{bail, Context};
    let doc: serde_json::Value = serde_json::from_str(text).context("the workflow isn't JSON")?;
    if doc["schemaVersion"] != "2" {
        bail!("the workflow isn't BLOK schemaVersion 2");
    }
    if doc["name"] != "oby-check" {
        bail!("the workflow is named {}, not oby-check", doc["name"]);
    }
    let list = doc["steps"]
        .as_array()
        .filter(|s| !s.is_empty())
        .context("the workflow has no steps")?;
    let mut steps: Vec<Step> = vec![];
    for raw in list {
        let id = raw["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("a step has no id")?
            .to_string();
        if let Some(kind) = UNSUPPORTED.iter().find(|k| raw.get(**k).is_some()) {
            bail!("step {id}: {kind} steps aren't supported by this host yet");
        }
        if steps.iter().any(|s| s.id == id) {
            bail!("two steps are named {id}");
        }
        let used = raw["use"]
            .as_str()
            .context(format!("step {id} has no use"))?;
        let name = used.strip_prefix("runtime.rust:").unwrap_or(used);
        if raw.get("type").is_some_and(|t| t != "runtime.rust") {
            bail!("step {id}: only runtime.rust nodes run here");
        }
        let (node, group) = GROUPS
            .into_iter()
            .find(|(n, _)| *n == name)
            .with_context(|| {
                format!("step {id} uses {used}, which isn't one of this tool's nodes")
            })?;
        if steps.iter().any(|s| s.node == node) {
            bail!("step {id}: {node} already has a step");
        }
        let flag = |key: &str, default: bool| match raw.get(key) {
            None => Ok(default),
            Some(v) => v
                .as_bool()
                .with_context(|| format!("step {id}: {key} must be true or false")),
        };
        let attempts = match raw.get("retry") {
            None => 1,
            Some(r) => r["maxAttempts"]
                .as_u64()
                .filter(|n| (1..=20).contains(n))
                .with_context(|| format!("step {id}: retry.maxAttempts must be 1 to 20"))?
                as u32,
        };
        let max = match raw.get("maxDuration") {
            None => None,
            Some(v) => {
                Some(parse_duration(v).with_context(|| format!("step {id}: bad maxDuration"))?)
            }
        };
        let (active, stop) = (flag("active", true)?, flag("stop", false)?);
        steps.push(Step {
            id,
            node,
            group,
            active,
            stop,
            attempts,
            max,
        });
    }
    Ok(steps)
}

/// BLOK's duration: whole milliseconds, or a number and a unit ("500ms", "30s", "5m", "2h", "1d").
pub fn parse_duration(v: &serde_json::Value) -> anyhow::Result<std::time::Duration> {
    use std::time::Duration;
    if let Some(ms) = v.as_u64() {
        return Ok(Duration::from_millis(ms));
    }
    let s = v.as_str().unwrap_or_default();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (n, unit) = s.split_at(split);
    let n: u64 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("{s:?} isn't a duration"))?;
    let ms = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => anyhow::bail!("{s:?} needs a unit: ms, s, m, h or d"),
    };
    Ok(Duration::from_millis(n * ms))
}

// Extract names only: never persist test output or a scanner's secret-bearing finding.
pub fn failing_tests(output: &str) -> Vec<String> {
    let mut names = BTreeSet::new();
    for line in output.lines() {
        let line = line.trim();
        if let Some(name) = line
            .strip_prefix("test ")
            .and_then(|s| s.strip_suffix(" ... FAILED"))
        {
            names.insert(name.to_owned());
        } else if let Some(name) = line.strip_suffix(" --- FAILED") {
            names.insert(name.to_owned());
        } else if let Some(name) = line
            .strip_prefix("FAIL ")
            .or_else(|| line.strip_prefix("❯ "))
            .or_else(|| line.strip_prefix("× "))
        {
            names.insert(name.trim().to_owned());
        } else if let Some(name) = line.strip_prefix("RuleID:") {
            names.insert(format!("gitleaks:{}", name.trim()));
        } else if let Some(name) = line
            .strip_prefix("oby-check: tests/")
            .and_then(|s| s.strip_suffix(" failed"))
        {
            names.insert(format!("tests/{name}"));
        }
    }
    names.into_iter().collect()
}

pub fn text_output(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

pub fn check_command(input: &CheckInput, group: &str) -> Command {
    let mut command = Command::new("bash");
    command
        .current_dir(&input.checkout)
        .arg("scripts/oby-check.sh")
        .arg(&input.base)
        .env_remove("OBY_CHECK_PLAN")
        .env_remove("OBY_CHECK_TOKEN")
        .env_remove("OBY_CHECK_ALL")
        .env_remove("OBY_CHECK_PATHS")
        .env("CARGO_NET_OFFLINE", "true")
        .env("npm_config_offline", "true")
        .env("OBY_CHECK_ONLY", group)
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0")
        .kill_on_drop(true);
    command
}

pub async fn run_group(input: &CheckInput, group: &str) -> CheckResult {
    let start = Instant::now();
    let output = match check_command(input, group).output().await {
        Ok(output) => output,
        Err(error) => {
            return CheckResult::unjudged(
                format!("could not start check: {error}"),
                start.elapsed().as_secs_f64(),
            )
        }
    };
    let text = text_output(&output);
    let failing = failing_tests(&text);
    let cache_missing = failing.is_empty()
        && [
            "no matching package named",
            "attempting to make an HTTP request",
            "ENOTCACHED",
        ]
        .iter()
        .any(|message| text.contains(message));
    let verdict = if cache_missing || text.lines().any(|l| l.starts_with("couldn't judge:")) {
        Verdict::Unjudged
    } else {
        match output.status.code() {
            Some(0) => Verdict::Passed,
            Some(1) => Verdict::Failed,
            _ => Verdict::Unjudged,
        }
    };
    CheckResult {
        verdict,
        failing,
        reason: match verdict {
            Verdict::Passed => "script passed",
            Verdict::Failed => "script failed",
            Verdict::Unjudged if cache_missing => "offline dependency cache is incomplete",
            Verdict::Unjudged => {
                "script could not judge in this environment; obid must run the full check"
            }
        }
        .into(),
        seconds: start.elapsed().as_secs_f64(),
    }
}

pub struct CheckNode(
    pub &'static str,
    pub &'static str,
    pub CheckInput,
    pub String,
);

#[async_trait::async_trait]
impl blok::TypedNode for CheckNode {
    type Input = NodeInput;
    type Output = CheckResult;
    fn name(&self) -> &str {
        self.0
    }
    fn description(&self) -> &str {
        "Shadow check: delegates planning and execution to oby-check.sh"
    }
    async fn run(
        &self,
        _: &mut blok::Context,
        input: NodeInput,
    ) -> Result<CheckResult, blok::BlokError> {
        Ok(self.execute(input).await)
    }
}

impl CheckNode {
    pub async fn execute(&self, input: NodeInput) -> CheckResult {
        if self.3.is_empty() || input.token != self.3 {
            return CheckResult::unjudged("unauthorized node request", 0.0);
        }
        if input
            .checkout
            .as_ref()
            .is_some_and(|v| v != &self.2.checkout)
            || input.base.as_ref().is_some_and(|v| v != &self.2.base)
        {
            return CheckResult::unjudged("request does not match spawned check", 0.0);
        }
        run_group(&self.2, self.1).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_default_workflow_runs_every_node_once_in_order() {
        let steps = parse_workflow(DEFAULT_WORKFLOW).unwrap();
        assert_eq!(
            steps.iter().map(|s| s.node).collect::<Vec<_>>(),
            GROUPS.map(|(n, _)| n)
        );
        assert!(steps
            .iter()
            .all(|s| s.active && !s.stop && s.attempts == 1 && s.max.is_some()));
    }

    #[test]
    fn a_workflow_reads_blok_step_settings_and_refuses_what_it_cant_run() {
        use std::time::Duration;
        let wf = |steps: &str| {
            format!(r#"{{"schemaVersion":"2","name":"oby-check","steps":[{steps}]}}"#)
        };
        let steps = parse_workflow(&wf(r#"{"id":"scan","use":"secret-scan","active":false,"stop":true,"retry":{"maxAttempts":3},"maxDuration":"90s"},{"id":"rust","use":"runtime.rust:rust-affected-tests","maxDuration":500}"#)).unwrap();
        assert_eq!(
            steps[0],
            Step {
                id: "scan".into(),
                node: "secret-scan",
                group: "scan",
                active: false,
                stop: true,
                attempts: 3,
                max: Some(Duration::from_secs(90))
            }
        );
        assert_eq!(steps[1].max, Some(Duration::from_millis(500)));
        for (steps, why) in [
            (
                r#"{"id":"a","use":"secret-scan","switch":{}}"#,
                "switch steps aren't supported",
            ),
            (
                r#"{"id":"a","use":"@blokjs/api-call"}"#,
                "isn't one of this tool's nodes",
            ),
            (
                r#"{"id":"a","use":"secret-scan"},{"id":"a","use":"ts-checks"}"#,
                "two steps are named a",
            ),
            (
                r#"{"id":"a","use":"secret-scan"},{"id":"b","use":"secret-scan"}"#,
                "already has a step",
            ),
            (
                r#"{"id":"a","use":"secret-scan","retry":{"maxAttempts":0}}"#,
                "maxAttempts must be 1 to 20",
            ),
            (
                r#"{"id":"a","use":"secret-scan","maxDuration":"5 minutes"}"#,
                "bad maxDuration",
            ),
            (
                r#"{"id":"a","use":"secret-scan","type":"module"}"#,
                "only runtime.rust",
            ),
        ] {
            let error = format!("{:#}", parse_workflow(&wf(steps)).unwrap_err());
            assert!(error.contains(why), "{error}");
        }
        assert!(format!(
            "{:#}",
            parse_workflow(r#"{"schemaVersion":"2","name":"other","steps":[]}"#).unwrap_err()
        )
        .contains("not oby-check"));
    }

    #[test]
    fn fold_prioritizes_failed_then_unjudged() {
        use Verdict::*;
        assert_eq!(fold([]), Unjudged);
        for a in [Passed, Failed, Unjudged] {
            for b in [Passed, Failed, Unjudged] {
                let expected = if [a, b].contains(&Failed) {
                    Failed
                } else if [a, b].contains(&Unjudged) {
                    Unjudged
                } else {
                    Passed
                };
                assert_eq!(fold([a, b]), expected);
            }
        }
    }
    #[test]
    fn parses_cargo_vitest_and_gitleaks_names() {
        let output = "test domain::fails ... FAILED\ndomain::quiet --- FAILED\n FAIL  src/app.test.ts > sends\n ❯ src/app.test.ts (2 tests | 1 failed)\n × sends a request\nRuleID: generic-api-key\nSecret: do-not-record\ntest domain::passes ... ok\ntest domain::fails ... FAILED";
        assert_eq!(
            failing_tests(output),
            vec![
                "domain::fails",
                "domain::quiet",
                "gitleaks:generic-api-key",
                "sends a request",
                "src/app.test.ts (2 tests | 1 failed)",
                "src/app.test.ts > sends"
            ]
        );
    }
}
