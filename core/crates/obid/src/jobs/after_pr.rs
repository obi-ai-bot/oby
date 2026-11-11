//! What obid tries by itself once a pull request it opened stops on something a round of its own
//! may settle, before any card asks the owner (docs/contracts/merging.md, MERGING-3, 4 and 38):
//! today a conflict with its base, or a branch behind it, gets one rebase round per head. And the
//! card that says obid's gh is signed out, so nothing about the pull request can be read.

use super::*;

impl Jobs {
    /// One rebase round for the job `id`'s pull request at `head` (`why`, what GitHub said), when
    /// it may have one: obid's own branch, never another's (a rebase force-pushes it), that
    /// [`Jobs::onto_base`] rebases (no pull request's or follow-up's start), its worktree still
    /// there, the job done, nothing else on its branch (no follow-up carrying it, no job at work:
    /// PUBLISHING-4), short of a safety stall, and no round for this head yet. It's queued at its
    /// check, as a restart cut there picks up: on its base as it is now with no agent turn, a
    /// conflict its agent's one round ([`rebase_prompt`]), then checked and pushed again over the
    /// head obid pushed. Whether it started: when not, the card asks. A round that leaves the head
    /// as it was gets the card next time, never another round.
    pub(super) fn round_after_pr(self: &Arc<Self>, id: &str, head: &str, why: &str) -> bool {
        if !self.worktree_path(id).join(".git").exists() {
            return false;
        }
        let may = |jobs: &HashMap<String, Job>| {
            jobs.get(id).is_some_and(|job| {
                job.status == Status::Done
                    && crate::merge::own_branch(job, &job.branch)
                    && job.pr.is_none()
                    && job.start_from.is_none()
                    && job.review_of.is_none()
                    && job.after_pr_rebase.as_deref() != Some(head)
                    && !past_safety(job)
                    && branch_busy(jobs, job).is_none()
                    && !jobs.values().any(|j| {
                        j.follow_up_of.as_deref() == Some(id) && same_branch(&j.branch, &job.branch)
                    })
            })
        };
        let mut started = false;
        self.update_after(id, may, |job, may| {
            if !may {
                return;
            }
            started = true;
            next_round(job);
            job.cut = Some(Phase::Check);
            job.after_pr_rebase = Some(head.to_owned());
            job.merge_needs_you = false;
            job.merge_error = None;
            job.step = format!(
                "Rebasing onto {}",
                job.base.as_deref().unwrap_or("its base")
            );
        });
        if started {
            tracing::info!("job {id}: one rebase round for {head}: {why}");
            self.schedule();
        }
        started
    }

    /// The job `id`'s card about `head` saying obid's gh is signed out ([`crate::merge::SIGNED_OUT`]).
    pub(super) fn signed_out_card(&self, id: &str, head: &str) {
        let Some(job) = self.get(id) else { return };
        let link = self.link.get().and_then(std::sync::Weak::upgrade);
        let card = crate::merge::stopped_card(&job, head, crate::merge::SIGNED_OUT);
        self.offer_merge_card(id, link.as_deref(), Some(card));
    }
}
