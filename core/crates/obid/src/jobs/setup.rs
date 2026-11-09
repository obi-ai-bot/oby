//! A repository's setup (`repos[].setup`, the owner's line in config.json, never read from the
//! repository): run in the job's box before its agent starts, so a job doesn't spend its first
//! turns working out installs (PUBLISHING-23), and in its check's checkout before the check
//! (PUBLISHING-24).

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
        let failed = failure(self.run_setup(id, &command, spec, false, signal).await);
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

    /// [`Jobs::base_checkout`]'s checkout for job `id`'s check, at `at`, then its repository's
    /// setup, if it has one, run in it before the check (PUBLISHING-24), the base's scripts already
    /// there: with the job's egress door, never the declared secrets, which only the check's own
    /// environment gets. None before a check that gets declared secrets (what the change installs
    /// could keep the door for the check to send them out through) or where obid can't confine
    /// the setup's network to that door (Linux): that check runs without it, the job's report
    /// says so. One that fails judged nothing: why, for the owner's Publish by hand with no fix
    /// round spent, its output in the check's log. Its checkout is gone after one that failed or
    /// was stopped, unless it's the one kept across the job's checks.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn checkout(
        &self,
        id: &str,
        command: &str,
        head: &str,
        spec: &Spec,
        gate: Option<(String, String)>,
        at: &Path,
        signal: &watch::Receiver<Signal>,
    ) -> Result<(Spec, Vec<String>, String), String> {
        let gated = gate.is_some();
        let checkout = Box::pin(self.base_checkout(id, command, head, spec, gate, at)).await?;
        let Some(setup) = self
            .get(id)
            .and_then(|job| self.repo(&job.repo))
            .and_then(|r| r.setup)
        else {
            return Ok(checkout);
        };
        if gated || !self.confines_egress() {
            let why = match gated {
                true => "it gets your declared secrets",
                false => "obid can't keep its setup's network to the job's door here",
            };
            let line = format!("Its check ran without its repository's setup, `{setup}`: {why}.");
            self.update(id, |job| {
                if !job.changes.iter().any(|c| c.what == line) {
                    job.changed(line);
                }
            });
            return Ok(checkout);
        }
        tracing::info!("job {id}: running its repository's setup before its check");
        let failed = failure(self.run_setup(id, &setup, &checkout.0, true, signal).await);
        let stopped = *signal.borrow() != Signal::Run;
        if failed.is_none() && !stopped {
            return Ok(checkout);
        }
        if at != self.check_home(id)
            && let Err(why) = Box::pin(crate::secretspec::remove_checkout(&checkout.0, at)).await
        {
            tracing::warn!("job {id}: {why}");
        }
        // Stopped: its caller ends the job there, as for a stop during the check.
        let Some(output) = failed.filter(|_| !stopped) else {
            return Err("It stopped during its repository's setup, before its check.".into());
        };
        let _ = std::fs::write(
            self.log_path(id, "check.log"),
            crate::secrets::known().redact(&output),
        );
        Err(super::unjudged::unchecked(format!(
            "its setup failed: `{setup}` (its output is in the check's log)"
        )))
    }

    async fn run_setup(
        &self,
        id: &str,
        command: &str,
        spec: &Spec,
        check: bool,
        signal: &watch::Receiver<Signal>,
    ) -> Result<CheckRun, String> {
        let sandbox = self
            .sandbox
            .as_ref()
            .map_err(|why| format!("obid can't run it: {why}"))?;
        let mut spec = spec.clone();
        let mut extra = spec.git_env();
        // The agent's way off the Mac, as `Jobs::launch` gives it: obid's proxy, never the
        // network without it. Before the agent, on the job's unchanged start commit, through a
        // door of its own: what the owner's line reaches is a reach only. On commits an agent
        // made (a later run, a check) the agent's door: its package.json is the agent's.
        let egress = (sandbox.confines_egress())
            .then(|| self.egress.get().ok_or("obid's egress proxy isn't running"))
            .transpose()?;
        let owners = egress.is_some()
            && owners_line(
                check,
                crate::secretspec::head_of(&spec).await.ok().as_deref(),
                self.get(id).and_then(|job| job.start_commit).as_deref(),
            );
        if let Some(egress) = egress {
            let door = match owners {
                true => egress.open_setup(id),
                false => egress.open(id),
            };
            extra.extend(crate::egress::env(&door));
            // A check's checkout has no network: its setup gets the door all the same.
            (spec.network, spec.proxied) = (true, true);
        }
        extra.extend(process::show_rust(&spec.temp));
        extra.extend(self.build_env(id, check));
        let argv = sandbox.wrap(&spec, &["/bin/sh".into(), "-c".into(), command.into()]);
        let env = process::sandbox_env(&process::tools_path(), &spec.temp, &extra);
        let group = self.recording(id, &spec);
        let run = run_check(
            &argv,
            &env,
            &spec.worktree,
            SETUP_LIMIT,
            signal.clone(),
            None,
            None,
            Some(&group),
        )
        .await;
        if let Some(egress) = egress.filter(|_| owners) {
            egress.close_setup(id);
        }
        run.map_err(|e| format!("obid couldn't run it: {e}"))
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

/// A run whose agent hears nothing (a pick-up) runs no setup, and neither does any review (Oby's
/// own, another Oby's request, one dispatched by hand): the reviewed commit's install scripts, with
/// the job's door open, could rewrite what the review judges (PUBLISHING-23).
fn runs_no_setup(job: Option<&Job>, message: &str) -> bool {
    message.is_empty()
        || job.is_some_and(|job| {
            crate::job::READ_ONLY.contains(&job.kind.as_str()) || job.review_of.is_some()
        })
}

/// Whether the setup runs the owner's line on what the owner gave (PUBLISHING-23): before the
/// agent, on the job's unchanged start commit. A check's, or a later run's on the agent's commits,
/// runs what the agent wrote.
fn owners_line(check: bool, head: Option<&str>, start: Option<&str>) -> bool {
    !check && head.is_some() && head == start
}

#[cfg(test)]
mod tests {
    use super::owners_line;

    /// PUBLISHING-23: only the setup before the agent, on the job's start commit, is the owner's.
    #[test]
    fn publishing_23_a_setup_on_the_agents_commits_is_the_agents() {
        assert!(owners_line(false, Some("a"), Some("a")));
        assert!(!owners_line(false, Some("b"), Some("a")));
        assert!(!owners_line(true, Some("a"), Some("a")));
        assert!(!owners_line(false, None, None));
    }

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
        // Another Oby's request, or one dispatched by hand: a review all the same.
        let mut asked = new();
        asked.kind = "review".into();
        assert!(runs_no_setup(Some(&asked), "Review it"));
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
