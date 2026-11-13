//! Keeping a job: a change reaches memory and the apps only once its file is saved, never before
//! (docs/contracts/home.md, HOME-1, HOME-2). A save that fails changes nothing, and nothing that
//! follows the change happens.

use super::*;

/// Why a job wasn't made: a refusal a person can read (`422`), or a job obid couldn't save
/// (`500`), which then doesn't exist.
#[derive(Debug, thiserror::Error)]
pub enum NotMade {
    #[error(transparent)]
    Refused(#[from] Refused),
    #[error("The job couldn't be kept: {0}")]
    Unkept(String),
}

impl Jobs {
    /// Saves `next`, the job `id` changed on a copy and one revision on, then puts it in memory, all
    /// under the state lock it was changed under: two changes to one job never lose each other. A
    /// save that fails leaves the job in memory as it was, logs it, and keeps its reason for
    /// [`Jobs::unkept`] until a save of the job succeeds.
    pub(super) fn keep(&self, state: &mut State, id: &str, next: &Job) -> Result<(), String> {
        if let Err(e) = self.store.save(next) {
            tracing::error!("can't keep job {id}: {e}");
            state.unkept.insert(id.to_owned(), e.to_string());
            return Err(e.to_string());
        }
        state.unkept.remove(id);
        state.jobs.insert(id.to_owned(), next.clone());
        Ok(())
    }

    /// Keeps a new job and tells the apps. One whose save fails doesn't exist, the place
    /// [`Jobs::create`] took for it neither (HOME-2): `Err` with why.
    pub(super) fn insert(&self, mut job: Job) -> Result<Job, String> {
        job.revision += 1;
        let mut state = self.lock();
        if let Err(why) = self.keep(&mut state, &job.id, &job) {
            state.jobs.remove(&job.id);
            state.unkept.remove(&job.id);
            return Err(why);
        }
        drop(state);
        self.events.send("job", job.to_json());
        self.changed.notify_waiters();
        Ok(job)
    }

    /// Why the last save of the job `id` failed, while no later one succeeded: a route answers
    /// `500` with it (crate::routes).
    pub fn unkept(&self, id: &str) -> Option<String> {
        self.lock().unkept.get(id).cloned()
    }
}
