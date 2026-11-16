//! What the follower hands to GitHub, and settles from it, about a pull request obid carries
//! (docs/contracts/merging.md): the merge it makes by itself, GitHub's auto-merge once GitHub
//! alone holds that merge (MERGING-41), a head its merge queue dropped (MERGING-55 to 57), and
//! what obid was sending when it last stopped.

use super::*;

use crate::merge::{Blocked, Merged, Reviewers, TakenBy};

/// How long a read of the job's pull request stands ([`follow_interval`]): at most 30 seconds in
/// the 10 minutes after obid armed GitHub's auto-merge on it, so a hold or a head someone else
/// pushed turns it off sooner; then the usual, for GitHub's rate limits (MERGING-41).
pub(super) fn follow_every(job: &Job, now: i64) -> i64 {
    let soon = job.auto_merge.is_some() && armed_at(job).is_some_and(|at| now - at < 600);
    follow_interval().min(if soon { 30 } else { i64::MAX })
}

/// When obid last armed GitHub's auto-merge on the job's pull request (`autoMerge` in `outgoing`).
fn armed_at(job: &Job) -> Option<i64> {
    let armed = (job.outgoing.iter().rev()).find(|o| o.action == "autoMerge");
    armed.map(|o| o.date)
}

/// How long after obid armed it a pull request with no auto-merge request is given to show in
/// GitHub's merge queue before the queue is read for a drop (MERGING-55, 58).
const QUEUE_GRACE: i64 = 120;

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

    /// The merge the job's pull request makes by itself by `workflow` (`seen`: the pull request,
    /// what a merge needs, Oby's reviewer accounts): the learned one's, `memory` (MERGING-45), or
    /// the proposed one for the owner's own conversation work, remembering nothing (`mergedBy`
    /// `oby`, MERGING-46). Kept on the job ([`crate::merge::note`]); the card, where the owner
    /// takes it from here.
    pub(super) async fn merges(
        self: &Arc<Self>,
        gh: &Path,
        job: &Job,
        workflow: &crate::memory::Workflow,
        seen: (
            &crate::github::PullRequest,
            &crate::github::Merging,
            Option<&Reviewers>,
        ),
        memory: Option<&str>,
    ) -> Option<crate::sphere::inbox::Item> {
        let (pr, merging, reviewers) = seen;
        let (id, head, home) = (job.id.as_str(), pr.head.as_deref()?, &self.config.home);
        let by = crate::job::MergedBy {
            by: if memory.is_some() { "memory" } else { "oby" }.into(),
            memory: memory.map(str::to_owned),
            at: obi_protocol::time::now(),
        };
        let outcome = match crate::merge::ready(job, Some(workflow), pr, merging, reviewers) {
            Ok(()) => {
                let go =
                    |action: &str| self.about_to_send(id, head, action, Some(by.clone()), None);
                match reviewers {
                    Some(r) => crate::merge::merge(gh, home, job, workflow, r, None, &go).await,
                    None => Err(crate::merge::unread_reviewers()),
                }
            }
            Err(blocked) => Err(blocked),
        };
        let outcome = self.by_github(gh, job, workflow, seen, &by, outcome).await;
        // A head GitHub's merge queue dropped gets one round by itself, else stops (MERGING-56, 57).
        let dropped = matches!(&outcome, Err(b) if crate::merge::is_dropped(b));
        let round = dropped && self.drop_round(id, gh, head, &pr.base).await;
        if dropped && !round {
            self.stop_drop(id, head);
        }
        if let Ok(merged) = &outcome {
            for sent in &merged.outgoing {
                tracing::info!("job {id}: sent {} {} {}", sent.action, sent.repo, sent.id);
            }
            if let Some(memory) = memory {
                crate::memory::note_used(home, memory);
            }
        }
        self.note_merge(id, &outcome, by);
        outcome
            .err()
            .filter(|b| b.needs_you && !round && !crate::merge::on_review_item(b))
            .filter(|b| !(b.rebase && self.round_after_pr(id, head, &b.why)))
            .map(|b| crate::merge::blocked_card(job, head, &b, workflow, &pr.base))
    }

    /// The merge `outcome` of the job's pull request under its `workflow` (`seen`: the pull
    /// request, what a merge needs, Oby's reviewer accounts), handed to GitHub's auto-merge when
    /// GitHub alone holds it: its required checks still running ([`crate::merge::by_github`]), or a
    /// base it takes only through its merge queue ([`crate::merge::MERGE_QUEUE`]). Once per head,
    /// pinned to it (`Job::auto_merge`): armed, it waits with no card, its Merged stage saying
    /// GitHub merges it, and the pass keeps it armed ([`Jobs::follow`]). Never main, master or
    /// trunk, never unlearned, but for the owner's own conversation work (MERGING-7, 46). Refused
    /// by GitHub, once per head
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
            let taken = match queue {
                true => self.taken_out(gh, job, head, merging).await,
                false => TakenBy::Unknown,
            };
            match taken {
                TakenBy::Queue => {
                    let job = self.get(id);
                    return Err(
                        job.map_or_else(crate::merge::handed, |j| crate::merge::dropped(&j))
                    );
                }
                TakenBy::Hand => return Err(crate::merge::off_by_hand()),
                TakenBy::Unknown => {}
            }
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
        // A head GitHub's merge queue dropped is never armed again by itself (MERGING-56, 57), nor
        // one whose auto-merge someone turned off on GitHub (MERGING-60).
        if job.queue_drops.iter().any(|d| d.head == head) {
            return Err(crate::merge::dropped(job));
        }
        if job.auto_merge_off.as_deref() == Some(head) {
            return Err(crate::merge::off_by_hand());
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

    /// Who took out `head`, which obid armed on the job's pull request (MERGING-55, 58, 60): its
    /// read (`merging`) has no auto-merge request at least [`QUEUE_GRACE`] after obid armed it, and
    /// one read of the queue ([`crate::github::queue_read`]) finds it open at that head, neither
    /// requested nor queued ([`crate::merge::queue_state`]); then the last time GitHub let it go,
    /// after obid armed it, says who ([`crate::merge::taken_by`]). The queue's own drop: `autoMerge`
    /// is gone and the drop is noted on the job, with GitHub's `mergeStateStatus`. Someone's hand:
    /// `autoMerge` is gone and that head stays off (`Job::auto_merge_off`). Anything else is
    /// [`TakenBy::Unknown`]: it stays handed, and is read again.
    async fn taken_out(
        &self,
        gh: &Path,
        job: &Job,
        head: &str,
        merging: &crate::github::Merging,
    ) -> TakenBy {
        use crate::merge::PrState;
        let now = obi_protocol::time::now();
        let armed = armed_at(job).filter(|at| now - at >= QUEUE_GRACE);
        let number = (job.published.as_deref()).and_then(|url| job.pull_request_number(url));
        let (Some(number), Some(armed), Some(false)) = (number, armed, merging.auto_merge_request)
        else {
            return TakenBy::Unknown;
        };
        let Some(read) = crate::github::queue_read(gh, &job.repo, number).await else {
            return TakenBy::Unknown;
        };
        let state = match read.state {
            Some("OPEN") => PrState::Open,
            Some("CLOSED") => PrState::Closed,
            Some("MERGED") => PrState::Merged,
            _ => return TakenBy::Unknown,
        };
        let same_head = read.head.as_deref() == Some(head);
        let queue = crate::merge::queue_state(true, same_head, state, read.request, read.entry);
        if queue != crate::merge::Queue::Dropped {
            return TakenBy::Unknown;
        }
        let last = read.taken_out.map(|t| (t.by_queue, t.at));
        let by = crate::merge::taken_by(last, armed);
        match by {
            TakenBy::Queue => tracing::info!("job {}: GitHub's merge queue dropped {head}", job.id),
            TakenBy::Hand => tracing::info!("job {}: auto-merge turned off at {head}", job.id),
            TakenBy::Unknown => return by,
        }
        self.update(&job.id, |job| {
            if job.auto_merge.as_ref().is_some_and(|a| a.head == head) {
                job.auto_merge = None;
            }
            match by {
                TakenBy::Queue => job.queue_drops.push(crate::job::QueueDrop {
                    head: head.into(),
                    state: merging.merge_state.clone(),
                    round: false,
                    stopped: false,
                    told: false,
                }),
                _ => job.auto_merge_off = Some(head.into()),
            }
        });
        by
    }

    /// The job `id`'s drop at `head` got no round (MERGING-57): nothing more happens by itself for
    /// it. A drop already stopped is left as it is.
    fn stop_drop(&self, id: &str, head: &str) {
        let open = |j: &Job| (j.queue_drops.iter()).any(|d| d.head == head && !d.stopped);
        if self.get(id).is_some_and(|j| open(&j)) {
            self.update(id, |job| {
                for drop in job.queue_drops.iter_mut().filter(|d| d.head == head) {
                    drop.stopped = true;
                }
            });
        }
    }

    /// The job `id`'s conversation was told GitHub's merge queue dropped `head` and nothing more
    /// happens by itself (crate::merge::queue_stopped, MERGING-57): that head tells it nothing more.
    pub fn mark_told_queue_stopped(&self, id: &str, head: &str) {
        self.update(id, |job| {
            for drop in job.queue_drops.iter_mut().filter(|d| d.head == head) {
                drop.told = true;
            }
        });
    }

    /// Whether `job` merges past Oby's review that asks for changes once nothing more happens by
    /// itself (MERGING-49), so no review item asks the owner: the owner's own one-review work,
    /// but under a workflow learned for its base that a person or a teammate approves (MERGING-45).
    pub(super) fn merges_past_review(&self, job: &Job) -> bool {
        let base =
            (job.pull_request.as_ref()).map_or(job.base.as_deref(), |pr| Some(pr.base.as_str()));
        let learned = crate::memory::workflow(&self.config.home, &job.repo)
            .and_then(|m| m.workflow)
            .filter(|w| base == Some(w.base.as_str()))
            .map(|w| w.review);
        let (own, two) = (crate::merge::own_work(job), job.two_reviews_why().is_some());
        crate::merge::flagged(true, own, two) == crate::merge::Flagged::MergesPast
            && crate::merge::merges_past_by(learned)
    }

    /// The job `id`'s conversation was told it merged at `head` past Oby's review
    /// (crate::merge::merged_past, MERGING-49): that head tells it nothing more.
    pub fn mark_told_merged_past(&self, id: &str, head: &str) {
        self.update(id, |job| job.told_merged_past = Some(head.to_owned()));
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
