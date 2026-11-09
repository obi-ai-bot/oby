//! A repository's setup (`repos[].setup`, the owner's line in config.json, never read from the
//! repository): run in the job's box before its agent starts, so a job doesn't spend its first
//! turns working out installs (PUBLISHING-23).

use super::*;

/// How long a setup may run before obid ends it and the job goes on without it.
const SETUP_LIMIT: Duration = Duration::from_secs(10 * 60);

impl Jobs {
    /// Runs job `id`'s repository's setup, if it has one, in `spec`'s worktree before a run whose
    /// agent hears `message` (none: a pick-up at its check or publishing, which starts no agent),
    /// with the agent's reach (the job's egress door) and none of the owner's declared secrets.
    /// How it went goes in the job's report (`changes`); what the agent hears of one that failed
    /// is added to `message`, and the job goes on either way. False when the job was stopped (or
    /// obid shut down) meanwhile: its run ends there, with no agent started.
    pub(super) async fn setup(
        &self,
        id: &str,
        repo: &str,
        spec: &Spec,
        message: &mut String,
        signal: &watch::Receiver<Signal>,
    ) -> bool {
        let Some(command) = self.repo(repo).and_then(|r| r.setup) else {
            return true;
        };
        if runs_no_setup(self.get(id).as_ref(), message) {
            return true;
        }
        tracing::info!("job {id}: running its repository's setup");
        let failed = failure(self.run_setup(id, &command, spec, signal).await);
        if *signal.borrow() != Signal::Run {
            return false;
        }
        self.update(id, |job| {
            job.changed(match &failed {
                None => format!("Ran its repository's setup, `{command}`."),
                Some(_) => format!("Its repository's setup, `{command}`, failed: it went on."),
            })
        });
        // A run obid went down with sends what it sent again (`recovery.sent`): its note is in it.
        if let Some(output) = failed.filter(|_| !message.contains(&failed_opening(&command))) {
            message.push_str(&format!("\n\n{}", setup_failed(&command, &output)));
        }
        true
    }

    async fn run_setup(
        &self,
        id: &str,
        command: &str,
        spec: &Spec,
        signal: &watch::Receiver<Signal>,
    ) -> Result<CheckRun, String> {
        let sandbox = self
            .sandbox
            .as_ref()
            .map_err(|why| format!("obid can't run it: {why}"))?;
        let mut spec = spec.clone();
        let mut extra = spec.git_env();
        // The agent's way off the Mac, as `Jobs::launch` gives it: obid's proxy, never the
        // network without it.
        if sandbox.confines_egress() {
            let egress = self
                .egress
                .get()
                .ok_or("obid's egress proxy isn't running")?;
            extra.extend(crate::egress::env(&egress.open(id)));
            spec.proxied = true;
        }
        extra.extend(process::show_rust(&spec.temp));
        extra.extend(self.build_env(id, false));
        let argv = sandbox.wrap(&spec, &["/bin/sh".into(), "-c".into(), command.into()]);
        let env = process::sandbox_env(&process::tools_path(), &spec.temp, &extra);
        let group = self.recording(id, &spec);
        run_check(
            &argv,
            &env,
            &spec.worktree,
            SETUP_LIMIT,
            signal.clone(),
            None,
            None,
            Some(&group),
        )
        .await
        .map_err(|e| format!("obid couldn't run it: {e}"))
    }
}

/// What a setup's run leaves for the agent to hear: nothing when it passed.
fn failure(run: Result<CheckRun, String>) -> Option<String> {
    match run {
        Ok(run) if run.passed => None,
        // Past its limit: run_check's words are a check's (its 45 minutes, Publish by hand).
        Ok(run) if run.unchecked.is_some() => Some(format!(
            "{}obid stopped it after {} minutes.",
            run.output
                .strip_suffix(CHECK_TOO_LONG)
                .unwrap_or(&run.output),
            SETUP_LIMIT.as_secs() / 60
        )),
        Ok(run) => Some(run.output),
        Err(why) => Some(why),
    }
}

fn failed_opening(command: &str) -> String {
    format!("The repository's setup, `{command}`, failed before you started")
}

/// What the agent hears of a setup that failed: the end of its output, the secrets obid knows
/// taken out, in a fence of its own, as a failed check's is ([`check_failed`]).
fn setup_failed(command: &str, output: &str) -> String {
    let output = match crate::vault::for_agent(tail(output, 8_000)) {
        Ok(text) => crate::secrets::known().redact(&text),
        Err(why) => format!("(obid left it out: {why})"),
    };
    let nonce = crate::brain::briefing::nonce();
    format!(
        "{}, so what it installs may be missing. Its output, from the end:\n\n{}\n{}",
        failed_opening(command),
        crate::brain::briefing::fence_rule(&nonce, "the setup's output"),
        crate::brain::briefing::fence(&nonce, &output)
    )
}

/// A run whose agent hears nothing (a pick-up) runs no setup, and neither does Oby's own review:
/// the reviewed commit's install scripts, with the job's door open, could rewrite what the review
/// judges (PUBLISHING-23).
fn runs_no_setup(job: Option<&Job>, message: &str) -> bool {
    message.is_empty() || job.is_some_and(|job| job.review_of.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PUBLISHING-23: Oby's own review runs no setup; an ordinary job's run does.
    #[test]
    fn publishing_23_a_review_runs_no_setup() {
        let new = || {
            Job::new(
                "j".into(),
                "o/r".into(),
                "fix".into(),
                "Fix it".into(),
                "claude".into(),
                "c".into(),
            )
        };
        let mut review = new();
        review.review_of = Some("author".into());
        assert!(runs_no_setup(Some(&review), "Review it"));
        assert!(!runs_no_setup(Some(&new()), "Fix it"));
        assert!(runs_no_setup(Some(&new()), ""));
    }

    /// A setup past its 10 minutes tells the agent so, never a check's 45 minutes or Publish by
    /// hand (run_check's words for a check).
    #[test]
    fn publishing_23_a_setup_past_its_limit_says_obid_stopped_it_after_10_minutes() {
        let run = CheckRun {
            passed: false,
            output: format!("fetching packages\n{CHECK_TOO_LONG}"),
            unchecked: Some("Not published: the check ran past its 10-minute limit.".into()),
        };
        let told = failure(Ok(run)).unwrap();
        assert_eq!(told, "fetching packages\nobid stopped it after 10 minutes.");
        let told = setup_failed("npm ci", &told);
        assert!(
            !told.contains("45 minutes") && !told.contains("Publish"),
            "{told}"
        );
    }
}
