//! The owner's declared secrets for a job's check (crate::secretspec): whether the code its check
//! runs is its owner's own, and the values it gets when it is.

use super::*;

impl Jobs {
    /// Why the code job `job`'s check would run isn't only its owner's own, so the check gets none
    /// of the owner's secrets, or none (crate::secretspec, "the threat model"). The check gets
    /// values only on macOS, when the job's agent had no network of its own; when the clone holds
    /// nothing but what obid knows is the owner's ([`crate::git::foreign_in`]); when the base is a
    /// branch the repository lets in only through a reviewed pull request, as GitHub says (never an
    /// `oby/` branch, one a job made, or the job's own); and when every commit between that base and `head` is in
    /// obid's own record of what this job, and the follow-ups it continues, made
    /// ([`Jobs::record_of`]). What can't be told is a no.
    #[allow(clippy::too_many_arguments)]
    async fn code_refuses_secrets(
        &self,
        job: &Job,
        repo: &crate::config::Repo,
        base: &str,
        base_commit: &str,
        head: &str,
        spec: &Spec,
        raw_network: bool,
    ) -> Option<String> {
        if let Some(why) = crate::secretspec::platform_refusal() {
            return Some(why);
        }
        if raw_network {
            return Some("its agent had a network of its own, past obid's egress proxy".into());
        }
        if let Some(why) = crate::secretspec::agent_written_base(base, &job.branch) {
            return Some(why);
        }
        if base_a_job_made(&self.lock().jobs, &job.repo, base) {
            return Some(format!(
                "its base, {base}, is a branch an agent may have written"
            ));
        }
        if let Some(why) = git::foreign_in(&self.clone_for(job)).await {
            return Some(why);
        }
        let Some(gh) = crate::github::gh() else {
            return Some(format!(
                "obid has no gh to ask GitHub whether its base, {base}, is one {} protects",
                repo.repo
            ));
        };
        // What someone else wrote in the issue or pull request it works from may steer it.
        if let Some((why, _)) = self.others_wrote(&gh, job, None).await {
            return Some(why);
        }
        if let Err(why) = crate::secretspec::protected_base(&gh, &repo.repo, base).await {
            return Some(why);
        }
        // The default branch is in every clone: unreviewed commits there could be copied in.
        if base != repo.default_branch
            && let Err(why) =
                crate::secretspec::protected_base(&gh, &repo.repo, &repo.default_branch).await
        {
            return Some(format!("the clone's default branch isn't reviewed: {why}"));
        }
        crate::secretspec::on_record(spec, base_commit, head, &self.record_of(job))
            .await
            .err()
    }

    /// The secrets job `id`'s repository declares for its check in `secretspec.toml` on the job's
    /// base branch, as obid's clone has it, with their values from the owner's vault
    /// (crate::secretspec): none when the base has no such file, and none unless the job and the
    /// code its check runs are its owner's own ([`Job::check_refuses_secrets`],
    /// [`Jobs::code_refuses_secrets`]).
    /// With them, the base's commit and the job's HEAD the gate passed: what the check's own
    /// checkout is made from.
    pub(super) async fn declared_secrets(
        &self,
        id: &str,
        spec: &Spec,
        raw_network: bool,
    ) -> (crate::secretspec::Filled, Option<(String, String)>) {
        use crate::secretspec::{FILE, Filled};
        let Some(job) = self.get(id) else {
            return (Filled::default(), None);
        };
        let Some(repo) = self.repo(&job.repo) else {
            return (Filled::default(), None);
        };
        let base = job.base.clone().unwrap_or(repo.default_branch.clone());
        let clone = self.clone_for(&job);
        // This check's own answer, whatever an earlier one's was: each path below says why it holds.
        self.update(id, |j| j.check_withheld = None);
        let (base_commit, names) = match crate::secretspec::at_base(&clone, &base).await {
            Ok((_, None)) => return (Filled::default(), None),
            Ok((commit, Some(text))) => (commit, crate::secretspec::names(&text)),
            Err(why) => (String::new(), Err(why)),
        };
        let names = match names {
            Ok(names) if names.is_empty() => return (Filled::default(), None),
            Ok(names) => names,
            Err(why) => {
                // Unread, the declared secrets are refused all the same: the check holds its merge.
                self.update(id, |j| {
                    j.check_withheld = Some(format!("{FILE} couldn't be read: {why}"))
                });
                return (
                    Filled::refused(format!("obid gave this check no secrets: {why}")),
                    None,
                );
            }
        };
        // The commit the check runs, taken once, after the job's turn ended: its sandbox's watcher
        // has killed what the turn started (crate::sandbox, `WATCHER`).
        let head = crate::secretspec::head_of(spec).await;
        let refused = match (job.own_check_refuses_secrets(), &head) {
            (Some(why), _) => Some(why),
            (None, Err(why)) => Some(why.clone()),
            (None, Ok(head)) => {
                Box::pin(self.code_refuses_secrets(
                    &job,
                    &repo,
                    &base,
                    &base_commit,
                    head,
                    spec,
                    raw_network,
                ))
                .await
            }
        };
        if let Some(why) = refused {
            self.update(id, |j| j.check_withheld = Some(why.clone()));
            return (
                Filled::refused(format!(
                    "{FILE} on {base} declares {}; obid gave this check none of its owner's secrets: {why}.",
                    names.join(", ")
                )),
                None,
            );
        }
        let filled = Filled::fill(&names, crate::vault::value);
        if filled.names().len() < names.len() {
            let short = format!(
                "the vault holds {} of its {} declared secrets",
                filled.names().len(),
                names.len()
            );
            self.update(id, |j| j.check_withheld = Some(short));
        }
        tracing::info!(
            "job {id}: its check gets {} of the {} secret(s) {FILE} on {base} declares",
            filled.names().len(),
            names.len()
        );
        (filled, head.ok().map(|head| (base_commit, head)))
    }
}
