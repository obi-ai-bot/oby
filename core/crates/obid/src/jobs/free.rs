//! Freeing a job's worktree once it's done with (docs/contracts/cleanup.md, CLEANUP-3 to 6): when
//! a job settled, the half of the cleanup pass that removes its worktree and its HOME, and the one
//! rule it removes them by ([`may_free`]). The job's record and its branch stay.

use super::*;

/// How long a job that ended keeps its worktree unless it's put away or freed at once
/// ([`free_at_once`]; docs/runner-api.md, `cleanup`).
pub(super) const SETTLE_AFTER: i64 = 24 * 60 * 60;
/// What [`Jobs::worktree_holds_work`] says of a worktree with uncommitted changes.
pub(super) const UNCOMMITTED: &str = "It has uncommitted changes.";

/// Whether the pass may remove a job's worktree and HOME now (CLEANUP-4): it settled, as read again
/// under the lock its worktree is made with, it isn't running, queued or publishing, and its
/// worktree holds nothing that exists nowhere else, published or committed.
pub(crate) fn may_free(settled: bool, running: bool, unpublished: bool, uncommitted: bool) -> bool {
    settled && !running && !unpublished && !uncommitted
}

/// Whether `job` is settled, as far as obid's records tell (docs/runner-api.md, `cleanup`): it
/// ended and needs nothing, no worker or publish holds it, its pull request isn't open, no
/// follow-up still at work carries its branch on, and it was put away (dismissed, archived, or
/// cleared by Clear done), ended [`SETTLE_AFTER`] ago or may go at once ([`free_at_once`]); an
/// Oby review as soon as it ends. Its worktree itself is looked at next
/// ([`Jobs::worktree_holds_work`]).
pub(super) fn settled_in(state: &State, job: &Job, now: i64, cleared: Option<i64>) -> bool {
    let Some(finished) = job.finished else {
        return false;
    };
    let put_away = job.dismissed.is_some()
        || job.archived.is_some()
        || cleared.is_some_and(|at| finished <= at)
        || job.kind == "review" && job.obi.is_none();
    done_with(state, job) && (put_away || now - finished >= SETTLE_AFTER || free_at_once(job))
}

/// Whether `job` goes on the next pass, without waiting [`SETTLE_AFTER`] (CLEANUP-3): done, not
/// another Oby's, nothing unpublished, and its pull request merged or closed, or none to open.
fn free_at_once(job: &Job) -> bool {
    job.status == Status::Done
        && job.obi.is_none()
        && job.unpublished() == 0
        && (job.pull_request_ended() || job.published.is_none())
}

/// Whether `job` is done with, as far as obid's records tell: it ended ([`at_rest`]) and needs
/// nothing, its pull request isn't open, and no follow-up still at work carries its branch on.
fn done_with(state: &State, job: &Job) -> bool {
    at_rest(state, job)
        && nothing_waits(job)
        && !pull_request_open(job)
        && chain::branch_carrier(&state.jobs, job, Holds::Open).is_none()
}

/// Nothing of `job` waits on the owner: it doesn't need them, or it's archived with no decision
/// open. An archived failed or interrupted job (TASKS-31) is put away as a dismissed one is, so it
/// goes on the next pass (CLEANUP-5).
fn nothing_waits(job: &Job) -> bool {
    if job.archived.is_some() {
        !job.decision()
    } else {
        !job.needs_you()
    }
}

/// Whether `job` ended (done, failed, stopped or interrupted) and no worker or publish holds it.
pub(super) fn at_rest(state: &State, job: &Job) -> bool {
    job.finished.is_some()
        && !job.status.is_active()
        && !state.workers.contains_key(&job.id)
        && !state.publishing.contains(&job.id)
}

impl Jobs {
    /// The worktree half of the cleanup pass ([`Jobs::clean_worktrees`]): removes the worktree and
    /// the HOME of every settled job ([`settled_in`]) whose worktree holds nothing that exists
    /// nowhere else ([`Jobs::worktree_holds_work`]), through obid's clone, only when [`may_free`]
    /// says so, and notes when on the job (`worktree_freed`, CLEANUP-6). Answers how many it
    /// removed, and the ones kept, with why.
    pub(super) async fn free_worktrees(
        &self,
        now: i64,
        cleared: Option<i64>,
    ) -> (usize, Vec<KeptWorktree>) {
        let settled: Vec<Job> = {
            let state = self.lock();
            state
                .jobs
                .values()
                .filter(|job| settled_in(&state, job, now, cleared))
                .filter(|job| self.worktree_path(&job.id).exists())
                .cloned()
                .collect()
        };
        let (mut removed, mut kept) = (0, Vec::new());
        let still_settled = |id: &str| {
            let state = self.lock();
            state
                .jobs
                .get(id)
                .filter(|j| settled_in(&state, j, now, cleared))
                .cloned()
        };
        for job in settled {
            if self.shutting_down.load(Ordering::Relaxed) {
                break;
            }
            // Under the lock a job's worktree is made with, checked again: one that runs again
            // meanwhile waits, then makes it afresh from its branch.
            let preparing = self.preparing(&job.repo);
            let _preparing = preparing.lock().await;
            let Some(job) = still_settled(&job.id) else {
                continue;
            };
            let held = self.worktree_holds_work(&job).await;
            let (settled, running, unpublished) = {
                let state = self.lock();
                let now_job = state.jobs.get(&job.id);
                let running = now_job.is_some_and(|j| j.status.is_active())
                    || state.workers.contains_key(&job.id)
                    || state.publishing.contains(&job.id);
                (
                    now_job.is_some_and(|j| settled_in(&state, j, now, cleared)),
                    running,
                    now_job.is_some_and(|j| j.unpublished() > 0),
                )
            };
            let uncommitted = held.as_deref() == Some(UNCOMMITTED);
            let unpublished = unpublished || held.is_some() && !uncommitted;
            let free = may_free(settled, running, unpublished, uncommitted);
            if let Some(why) = held {
                kept.push(KeptWorktree {
                    job: job.id.clone(),
                    why,
                });
            }
            if !free {
                continue;
            }
            let clone = self.clone_for(&job);
            // Its HOME first: once the worktree is gone, so is what its runs kept there.
            let _ = std::fs::remove_dir_all(self.temp_path(&job.id));
            remove_worktree(&clone, &self.worktree_path(&job.id)).await;
            tracing::info!("job {}: its worktree was cleaned up", job.id);
            self.update(&job.id, |j| {
                (j.worktree, j.worktree_freed) = (None, Some(now))
            });
            removed += 1;
        }
        (removed, kept)
    }

    /// Notes the merge `outcome` of job `id` ([`crate::merge::note`]); one that merged wakes the
    /// cleanup pass, as the follower reading it merged does: the follower reads it no more
    /// (CLEANUP-3).
    pub(super) fn note_merge(
        &self,
        id: &str,
        outcome: &Result<crate::merge::Merged, crate::merge::Blocked>,
        by: crate::job::MergedBy,
    ) {
        self.update(id, |job| crate::merge::note(job, outcome, by));
        if outcome.is_ok() {
            self.cleaning.notify_one();
        }
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of [`may_free`] (CLEANUP-4): `kani::any()` stands for every value of the
    //! three inputs each proof doesn't fix.

    use super::*;

    /// A job running, queued or publishing never loses its worktree.
    #[kani::proof]
    fn may_free_never_while_running() {
        let (settled, unpublished, uncommitted) = (kani::any(), kani::any(), kani::any());
        assert!(!may_free(settled, true, unpublished, uncommitted));
    }

    /// A worktree with commits or artifacts that weren't published, or one the guard can't vouch
    /// for, is never removed.
    #[kani::proof]
    fn may_free_never_with_unpublished_work() {
        let (settled, running, uncommitted) = (kani::any(), kani::any(), kani::any());
        assert!(!may_free(settled, running, true, uncommitted));
    }

    /// A worktree with uncommitted changes is never removed.
    #[kani::proof]
    fn may_free_never_with_uncommitted_work() {
        let (settled, running, unpublished) = (kani::any(), kani::any(), kani::any());
        assert!(!may_free(settled, running, unpublished, true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed() -> Job {
        let mut job = Job::new(
            "aaaa0031".into(),
            "o/r".into(),
            "build".into(),
            "Do it".into(),
            "claude".into(),
            "stub".into(),
        );
        job.status = Status::Failed;
        job.finished = Some(1);
        job
    }

    /// TASKS-31, CLEANUP-5: a failed job archived (not dismissed) is let go, as a dismissed one is;
    /// one neither archived nor dismissed, or archived with its merge waiting on the owner, isn't.
    #[test]
    fn tasks_31_an_archived_failed_job_is_put_away() {
        let mut job = failed();
        assert!(!nothing_waits(&job), "failed, not put away");
        job.archived = Some(2);
        assert!(nothing_waits(&job), "archived");
        job.merge_needs_you = true;
        assert!(!nothing_waits(&job), "a decision open");
        job.archived = None;
        job.merge_needs_you = false;
        job.dismissed = Some(2);
        assert!(nothing_waits(&job), "dismissed, as before");
    }
}
