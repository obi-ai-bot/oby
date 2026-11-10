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
