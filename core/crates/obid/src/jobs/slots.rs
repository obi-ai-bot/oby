//! Slots: which jobs take one of `maxRunning`, and how a job waiting for its repository's one
//! check at a time gives its own up and takes one back (docs/contracts/admission.md, ADMISSION-4, ADMISSION-5).

use super::*;

impl State {
    /// The slots taken: jobs a worker runs, less those idle ([`Idle`], ADMISSION-4).
    pub(super) fn taken(&self) -> usize {
        self.workers.len().saturating_sub(self.idle.len())
    }
}

/// A job waiting on the check queue and checking, until this is dropped: its slot is free for
/// another job meanwhile, and it takes one back ([`Idle::back`]) before it goes on, so no more than
/// `maxRunning` jobs ever work at once (ADMISSION-4, ADMISSION-5).
struct Idle<'a> {
    jobs: &'a Arc<Jobs>,
    id: &'a str,
}

impl<'a> Idle<'a> {
    fn new(jobs: &'a Arc<Jobs>, id: &'a str) -> Self {
        jobs.lock().idle.insert(id.to_string());
        jobs.schedule();
        Idle { jobs, id }
    }

    /// Back from the wait: a slot again as soon as one is free, before any queued job's that looks
    /// after it, or until the job is stopped.
    async fn back(self, signal: &watch::Receiver<Signal>) {
        let mut stop = signal.clone();
        loop {
            // Every slot freed tells `changed` ([`Jobs::schedule`]); listened to before looking.
            let free = self.jobs.changed.notified();
            tokio::pin!(free);
            free.as_mut().enable();
            let max = self.jobs.max_running();
            let step = {
                let mut state = self.jobs.lock();
                if state.taken() < max {
                    // Out of both under this one lock: a `schedule` between can't count it twice.
                    state.idle.remove(self.id);
                    state.returning.remove(self.id);
                    return;
                }
                state.returning.insert(self.id.to_string());
                format!(
                    "Waiting for a free slot: {} of {max} jobs running",
                    state.taken()
                )
            };
            if self.jobs.get(self.id).is_some_and(|job| job.step != step) {
                self.jobs.update(self.id, |job| job.step = step);
            }
            tokio::select! {
                () = free => {}
                _ = stop.wait_for(|s| *s != Signal::Run) => return,
            }
        }
    }
}

impl Drop for Idle<'_> {
    fn drop(&mut self) {
        let mut state = self.jobs.lock();
        state.idle.remove(self.id);
        state.returning.remove(self.id);
    }
}

impl Jobs {
    /// Runs the repository's check on commit `head` ([`Jobs::checking_with_turn`]), one of each
    /// repository's at a time: a job that waits for its turn gives its slot up until it has given the turn back,
    /// then waits for a free one, so none holds the one check while it waits for a slot.
    pub(super) async fn check(
        self: &Arc<Self>,
        id: &str,
        command: &str,
        head: &str,
        spec: &Spec,
        signal: &watch::Receiver<Signal>,
    ) -> Result<CheckRun, String> {
        // One check at a time per repository: two of this repository's, each building obid with
        // every core, swapped a 16 GB Mac until both ran out of their 20 minutes (2026-10-24); a
        // check of another repository doesn't wait behind them (PUBLISHING-25). The wait isn't
        // part of the limit, and a stop ends it. Memory never holds a check back: it runs as soon
        // as it has its turn, its build sized to what's there next to the other running jobs
        // ([`Jobs::build_env`]). ponytail: one per repository on every machine, and two big
        // repositories' checks share the Mac's memory; a setting when a machine wants otherwise.
        let repo = self.get(id).map(|job| job.repo).unwrap_or_default();
        let checking = self.check_turn(&repo);
        let (turn, idle) = match checking.try_acquire() {
            Ok(turn) => (turn, None),
            Err(_) => {
                let step = self.get(id).map(|job| job.step).unwrap_or_default();
                let waiting = "Waiting for another job's check to finish";
                self.update(id, |job| job.step = waiting.into());
                let idle = Idle::new(self, id);
                let mut stop = signal.clone();
                let turn = tokio::select! {
                    turn = checking.acquire() => {
                        turn.map_err(|e| format!("obid couldn't run the check: {e}"))?
                    }
                    _ = stop.wait_for(|s| *s != Signal::Run) => return Ok(CheckRun::default()),
                };
                self.update(id, |job| job.step = step);
                (turn, Some(idle))
            }
        };
        let run = self
            .checking_with_turn(id, command, head, spec, signal)
            .await;
        drop(turn);
        if let Some(idle) = idle {
            idle.back(signal).await;
        }
        run
    }

    /// The one check turn of `repo`'s jobs, made on first use (PUBLISHING-25).
    pub(super) fn check_turn(&self, repo: &str) -> Arc<tokio::sync::Semaphore> {
        let mut turns = self.checking.lock().unwrap();
        turns
            .entry(repo.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(1)))
            .clone()
    }
}
