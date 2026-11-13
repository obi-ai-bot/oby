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
    /// Saves `next`, job `id` changed on a copy one revision on, then puts it in memory, all under
    /// the state lock it was changed under. A failed save changes nothing; why is kept for
    /// [`Jobs::unkept`] until a save of the job succeeds.
    pub(super) fn keep(&self, state: &mut State, id: &str, next: Job) -> Result<Job, String> {
        if let Err(e) = self.store.save(&next) {
            tracing::error!("can't keep job {id}: {e}");
            state.unkept.insert(id.to_owned(), e.to_string());
            return Err(e.to_string());
        }
        state.unkept.remove(id);
        state.jobs.insert(id.to_owned(), next.clone());
        Ok(next)
    }

    /// Keeps a new job and tells the apps: one whose save fails doesn't exist, nor the place
    /// [`Jobs::create`] took for it (HOME-2).
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
