//! What the follower hands to GitHub, and settles from it, about a pull request obid carries
//! (docs/contracts/merging.md): GitHub's auto-merge, once GitHub alone holds a merge a learned
//! workflow would make (MERGING-41), and what obid was sending when it last stopped.

use super::*;

use crate::merge::{Blocked, Merged, Reviewers};

impl Jobs {
    /// The merge `outcome` of the job's pull request under its learned `workflow` (`seen`: the pull
    /// request, what a merge needs, Oby's reviewer accounts), handed to GitHub's auto-merge when
    /// GitHub alone holds it: its checks still running ([`crate::merge::by_github`]), or a base it
    /// takes only through its merge queue ([`crate::merge::MERGE_QUEUE`]). Once per head, pinned to
    /// it (`Job::auto_merge`): armed, it waits with no card, its Merged stage saying GitHub merges
    /// it. A new head cancels it on GitHub; it's armed again once Oby's own review approved that
    /// head and every other rule holds. Never main, master or trunk, never unlearned (MERGING-7).
    /// Not armed (GitHub refused it), it waits as before; a merge queue's refusal asks the owner.
    pub(super) async fn by_github(
        self: &Arc<Self>,
        gh: &Path,
        job: &Job,
        workflow: &crate::memory::Workflow,
        seen: (
            &crate::github::PullRequest,
            &crate::github::Merging,
            Option<&Reviewers>,
        ),
        by: &crate::job::MergedBy,
        outcome: Result<Merged, Blocked>,
    ) -> Result<Merged, Blocked> {
        let (pr, merging, reviewers) = seen;
        let queue = matches!(&outcome, Err(b) if b.why == crate::merge::MERGE_QUEUE);
        let (Err(_), Some(head), Some(reviewers)) = (&outcome, pr.head.as_deref(), reviewers)
        else {
            return outcome;
        };
        if !queue && crate::merge::by_github(job, workflow, pr, merging, Some(reviewers)).is_none()
        {
            return outcome;
        }
        if job.auto_merge.as_ref().is_some_and(|a| a.head == head) {
            return Err(crate::merge::handed());
        }
        let id = job.id.as_str();
        let go = |action: &str| self.about_to_send(id, head, action, Some(by.clone()), None);
        let home = self.config.home.clone();
        let armed = crate::merge::arm(gh, &home, job, workflow, reviewers, &go).await;
        self.update(id, |job| {
            crate::merge::unmark(job, "merge");
            if let Ok(sent) = &armed {
                job.outgoing.extend(sent.outgoing.iter().cloned());
                job.auto_merge = Some(crate::job::Sending {
                    action: "autoMerge".into(),
                    head: head.into(),
                    by: Some(by.clone()),
                    login: None,
                });
            }
        });
        match armed {
            Ok(_) => {
                tracing::info!("job {id}: GitHub merges {head} once its checks pass");
                Err(crate::merge::handed())
            }
            Err(blocked) if queue => Err(blocked),
            Err(blocked) => {
                tracing::warn!("job {id}: GitHub's auto-merge isn't on: {}", blocked.why);
                outcome
            }
        }
    }

    /// What obid was sending for the job `id` when it last stopped, settled from what GitHub
    /// says now (the pull request is open): an approval it finds there is listed; a merge (and its
    /// branch's deletion) didn't land, and is tried again as anything is, unless a merge is under
    /// way right now (the owner's tap), whose marks stay.
    pub(super) fn settle_sending(&self, id: &str, merging: &crate::github::Merging) {
        let busy = crate::merge::merging_now(&self.config.home, id);
        self.update(id, |job| {
            if let Some(approval) = crate::merge::unmark(job, "pullRequestApproval")
                && crate::merge::approval_seen(&approval, &merging.reviews)
                && let Some(url) = job.published.clone()
            {
                job.outgoing.push(crate::publish::Outgoing::new(
                    "pullRequestApproval",
                    &job.repo,
                    &url,
                    None,
                ));
            }
            if !busy {
                crate::merge::unmark(job, "merge");
                crate::merge::unmark(job, "branchDelete");
            }
        });
    }
}
