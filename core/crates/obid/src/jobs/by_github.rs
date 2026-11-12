//! What the follower hands to GitHub, and settles from it, about a pull request obid carries
//! (docs/contracts/merging.md): GitHub's auto-merge, once GitHub alone holds a merge a learned
//! workflow would make (MERGING-41), and what obid was sending when it last stopped.

use super::*;

use crate::merge::{Blocked, Merged, Reviewers};

/// How long a read of the job's pull request stands ([`follow_interval`]): at most 30 seconds in
/// the 10 minutes after obid armed GitHub's auto-merge on it, so a hold or a head someone else
/// pushed turns it off sooner; then the usual, for GitHub's rate limits (MERGING-41).
pub(super) fn follow_every(job: &Job, now: i64) -> i64 {
    let armed = (job.outgoing.iter().rev()).find(|o| o.action == "autoMerge");
    let soon = job.auto_merge.is_some() && armed.is_some_and(|o| now - o.date < 600);
    follow_interval().min(if soon { 30 } else { i64::MAX })
}

impl Jobs {
    /// One follower pass over the job `id`'s open pull request ([`Jobs::carry`]): GitHub's
    /// auto-merge obid armed that the pass didn't keep armed (a hold of Oby's, a new head, a job
    /// no longer carried) is turned off (MERGING-41), and the card or wait it held back applies.
    pub(super) async fn follow(
        self: &Arc<Self>,
        id: &str,
        gh: &Path,
        pr: &crate::github::PullRequest,
        merging: crate::github::Merging,
    ) {
        self.lock().handed.remove(id);
        self.carry(id, gh, pr, merging).await;
        let kept = self.lock().handed.remove(id);
        if !kept {
            self.disarm(gh, id).await;
        }
    }

    /// A push obid just made (`sent`): GitHub's auto-merge armed at another head of that branch is
    /// turned off at once (MERGING-41), since GitHub cancels it only for a push by someone without
    /// write access, and obid pushes with the owner's.
    pub(super) fn pushed(self: &Arc<Self>, sent: &crate::publish::Outgoing) {
        let armed: Vec<String> = (self.lock().jobs.values())
            .filter(|j| {
                j.repo == sent.repo && j.auto_merge.as_ref().is_some_and(|a| a.head != sent.id)
            })
            .filter(|j| (j.outgoing.iter()).any(|o| o.action == "push" && o.branch == sent.branch))
            .map(|j| j.id.clone())
            .collect();
        for id in armed {
            self.disarm_now(&id);
        }
    }

    /// GitHub's auto-merge obid armed on the job `id`'s pull request, turned off at once, with no
    /// read first: after obid's own push, and when the owner lets the job go or answers `Leave it
    /// open` on its card (MERGING-41).
    pub fn disarm_now(self: &Arc<Self>, id: &str) {
        let armed = self.get(id).is_some_and(|j| j.auto_merge.is_some());
        let Some(gh) = crate::github::gh().filter(|_| armed) else {
            return;
        };
        let (jobs, id) = (self.clone(), id.to_owned());
        tokio::spawn(async move { jobs.disarm(&gh, &id).await });
    }

    /// Turns off GitHub's auto-merge obid armed on the job `id`'s open pull request, once, listed
    /// in `outgoing` (`autoMergeOff`).
    async fn disarm(&self, gh: &Path, id: &str) {
        let Some(job) = self.get(id).filter(|j| {
            j.auto_merge.is_some() && j.pull_request.as_ref().is_none_or(|pr| pr.is_open())
        }) else {
            return;
        };
        let head = job.auto_merge.as_ref().map(|a| a.head.clone());
        match crate::merge::disarm(gh, &job).await {
            Ok(sent) => {
                tracing::info!("job {id}: sent {} {} {}", sent.action, sent.repo, sent.id);
                self.update(id, |job| {
                    if job.auto_merge.as_ref().map(|a| &a.head) == head.as_ref() {
                        job.auto_merge = None;
                    }
                    job.outgoing.push(sent);
                });
            }
            // ponytail: tried again on the next pass until GitHub takes it.
            Err(why) => tracing::warn!("job {id}: GitHub's auto-merge stays on for now: {why}"),
        }
    }

    /// The merge `outcome` of the job's pull request under its learned `workflow` (`seen`: the pull
    /// request, what a merge needs, Oby's reviewer accounts), handed to GitHub's auto-merge when
    /// GitHub alone holds it: its required checks still running ([`crate::merge::by_github`]), or a
    /// base it takes only through its merge queue ([`crate::merge::MERGE_QUEUE`]). Once per head,
    /// pinned to it (`Job::auto_merge`): armed, it waits with no card, its Merged stage saying
    /// GitHub merges it, and the pass keeps it armed ([`Jobs::follow`]). Never main, master or
    /// trunk, never unlearned (MERGING-7). Refused by GitHub, once per head
    /// (`Job::auto_merge_refused`): it gets the card it had before, and a merge queue's refusal asks
    /// the owner.
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
        let id = job.id.as_str();
        if job.auto_merge.as_ref().is_some_and(|a| a.head == head) {
            self.lock().handed.insert(id.to_owned());
            return Err(crate::merge::handed());
        }
        if let Some((at, why)) = &job.auto_merge_refused
            && at == head
        {
            return if queue {
                Err(crate::merge::needs_you(why.clone()))
            } else {
                outcome
            };
        }
        let go = |action: &str| self.about_to_send(id, head, action, Some(by.clone()), None);
        let home = self.config.home.clone();
        let armed = crate::merge::arm(gh, &home, job, workflow, reviewers, &go).await;
        self.update(id, |job| {
            crate::merge::unmark(job, "merge");
            match &armed {
                Ok(sent) => {
                    job.outgoing.extend(sent.outgoing.iter().cloned());
                    job.auto_merge = Some(crate::job::Sending {
                        action: "autoMerge".into(),
                        head: head.into(),
                        by: Some(by.clone()),
                        login: None,
                    });
                }
                Err(refused) if refused.why.starts_with(crate::merge::AUTO_REFUSED) => {
                    job.auto_merge_refused = Some((head.into(), refused.why.clone()));
                }
                Err(_) => {}
            }
        });
        match armed {
            Ok(_) => {
                tracing::info!("job {id}: GitHub merges {head} once its checks pass");
                self.lock().handed.insert(id.to_owned());
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

#[cfg(test)]
mod tests {
    use super::*;

    /// MERGING-41: a pull request obid armed GitHub's auto-merge on is read every 30 seconds for
    /// the 10 minutes after, then at the usual interval, as one it never armed or turned off is.
    #[test]
    fn merging_41_an_armed_pull_request_is_read_every_30_seconds_for_10_minutes() {
        let (a, r) = ("claude".to_owned(), "o/r".to_owned());
        let mut job = Job::new("j".into(), r, "fix".into(), "Fix it".into(), a.clone(), a);
        let usual = follow_interval();
        assert!(usual > 30, "{usual}");
        let armed = crate::publish::Outgoing::new("autoMerge", "o/r", "url", Some("dev"));
        let now = armed.date;
        assert_eq!(follow_every(&job, now), usual);
        job.outgoing = vec![
            crate::publish::Outgoing::new("push", "o/r", "abc", Some("oby/fix")),
            armed,
        ];
        job.auto_merge = Some(crate::job::Sending {
            action: "autoMerge".into(),
            head: "abc".into(),
            by: None,
            login: None,
        });
        assert_eq!(follow_every(&job, now), 30);
        assert_eq!(follow_every(&job, now + 599), 30);
        assert_eq!(follow_every(&job, now + 600), usual);
        job.auto_merge = None;
        assert_eq!(follow_every(&job, now), usual);
    }
}
