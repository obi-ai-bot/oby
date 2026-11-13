//! A change to a job reaches memory and the apps only once its file is saved (docs/contracts/home.md).

use super::*;

/// Why a job wasn't made: a refusal (`422`), or a job obid couldn't save (`500`), which doesn't exist.
#[derive(Debug, thiserror::Error)]
pub enum NotMade {
    #[error(transparent)]
    Refused(#[from] Refused),
    #[error("The job couldn't be kept: {0}")]
    Unkept(String),
}

impl Jobs {
    /// Saves `next`, job `id` changed a revision on, and puts it in memory, under its state lock. A
    /// failed save carries only the restricting marks ([`carry_marks`]); [`Jobs::unkept`] says why.
    pub(super) fn keep(&self, state: &mut State, id: &str, next: Job) -> Result<Job, String> {
        if let Err(e) = self.store.save(&next) {
            tracing::error!("can't keep job {id}: {e}");
            if let Some(job) = state.jobs.get_mut(id) {
                carry_marks(job, &next);
            }
            state.unkept.insert(id.to_owned(), e.to_string());
            return Err(e.to_string());
        }
        state.unkept.remove(id);
        state.jobs.insert(id.to_owned(), next.clone());
        Ok(next)
    }

    /// Keeps a new job and tells the apps; one not saved doesn't exist, nor its place (HOME-2).
    pub(super) fn insert(&self, mut job: Job) -> Result<Job, String> {
        let (mut state, id) = (self.lock(), job.id.clone());
        let kept = self.keep(&mut state, &id, job.revised());
        if kept.is_err() {
            state.jobs.remove(&id);
            state.unkept.remove(&id);
        }
        drop(state);
        let job = kept?;
        self.events.send("job", job.to_json());
        self.changed.notify_waiters();
        Ok(job)
    }

    /// Why the last save of job `id` failed, while no later one succeeded (routes answer `500`).
    pub fn unkept(&self, id: &str) -> Option<String> {
        self.lock().unkept.get(id).cloned()
    }
}

/// The marks that only restrict a job, from unsaved `next` onto `job`, the newest `check_withheld`
/// reason winning: a job is never treated as less marked than it was, and its next save keeps them.
fn carry_marks(job: &mut Job, next: &Job) {
    if job.foreign.is_none() {
        job.foreign_by_api = next.foreign_by_api;
    } else if next.foreign.is_some() {
        job.foreign_by_api &= next.foreign_by_api;
    }
    job.foreign = job.foreign.take().or_else(|| next.foreign.clone());
    job.reached = job.reached.take().or_else(|| next.reached.clone());
    job.untrusted |= next.untrusted;
    job.unsigned |= next.unsigned;
    job.memory_from_others |= next.memory_from_others;
    job.ran_unproxied |= next.ran_unproxied;
    job.vetted &= next.vetted;
    job.withheld = job.withheld.take().or_else(|| next.withheld.clone());
    job.check_withheld = next.check_withheld.clone().or(job.check_withheld.take());
}
