//! Waiting to try again (RECOVERY-1, RECOVERY-14): a turn the network cut, and a job's first fetch.

use super::slots::Idle;
use super::*;

impl Jobs {
    /// The owner asked in a conversation (RECOVERY-17): obid may carry `id` on by itself again,
    /// though their words reach it as an event's, so a #1563 hold still needs their signed choice.
    pub fn owner_asked(&self, id: &str) {
        self.update(id, owner_spoke);
    }

    /// Waits `wait` (capped by the stall window) before try `attempt` of `of` after `error`
    /// (RECOVERY-1, RECOVERY-14), its step saying when and `each` run on the job every minute.
    /// Words for it end the wait: they're its next try's. False when it was stopped meanwhile.
    pub(super) async fn wait_to_try(
        self: &Arc<Self>,
        id: &str,
        signal: &watch::Receiver<Signal>,
        error: &str,
        wait: Duration,
        (attempt, of): (usize, usize),
        each: impl Fn(&mut Job),
    ) -> bool {
        let mut left = wait.min(self.config.stall_after);
        tracing::info!("job {id}: {error}: trying again in {left:?}");
        let heard = || self.get(id).is_some_and(|j| !j.heard.is_empty());
        'wait: while !left.is_zero() && !heard() {
            if *signal.borrow() != Signal::Run {
                return false;
            }
            let step = trying_again(error, left, attempt, of);
            self.update(id, |job| {
                job.step = step.clone();
                each(job);
            });
            let tick = left.min(Duration::from_secs(60));
            let sleep = tokio::time::sleep(tick);
            tokio::pin!(sleep);
            loop {
                let mut stop = signal.clone();
                tokio::select! {
                    () = &mut sleep => break,
                    _ = stop.wait_for(|s| *s != Signal::Run) => return false,
                    () = self.changed.notified() => {
                        if heard() {
                            break 'wait;
                        }
                    }
                }
            }
            left -= tick;
        }
        true
    }

    /// RECOVERY-14's wait before try `attempt` after `error`: queued, its slot free meanwhile
    /// (ADMISSION-4) and taken back before it goes on (ADMISSION-5), and one of the job's tries in
    /// all (RECOVERY-17): with none left, it's interrupted with the cap's reason. False when it was
    /// stopped meanwhile.
    pub(super) async fn wait_queued(
        self: &Arc<Self>,
        id: &str,
        signal: &watch::Receiver<Signal>,
        error: &str,
        attempt: usize,
    ) -> Result<bool, End> {
        if self.get(id).is_some_and(|job| job.recovery.spent()) {
            let (reason, next) = (crate::job::capped(error), Some(crate::job::TRY_AGAIN));
            return Err(End::NeedsYou {
                reason,
                next,
                resumes: None,
            });
        }
        self.update(id, |job| job.recovery.tries += 1);
        let (wait, of) = (RETRY_WAITS[attempt - 1], (attempt, RETRY_WAITS.len()));
        let idle = Idle::new(self, id);
        if !self.wait_to_try(id, signal, error, wait, of, queued).await {
            return Ok(false);
        }
        idle.back(signal).await;
        Ok(*signal.borrow() == Signal::Run)
    }

    /// RECOVERY-14: the job's clone of `repo`, fetched with `refspecs`, under the clone's lock
    /// (`prepared`, of `preparing`), given back with it. A fetch git's own words say the network
    /// failed waits and tries again, [`RETRY_WAITS`]', queued and taking no slot, the lock free
    /// meanwhile. None when it was stopped meanwhile.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn ready_clone<'a>(
        self: &Arc<Self>,
        id: &str,
        signal: &watch::Receiver<Signal>,
        job: &Job,
        repo: &Repo,
        refspecs: &[String],
        preparing: &'a tokio::sync::Mutex<()>,
        mut prepared: tokio::sync::MutexGuard<'a, ()>,
    ) -> Result<Option<(PathBuf, tokio::sync::MutexGuard<'a, ()>)>, End> {
        for tries in 1.. {
            let home = self.clone_home(job);
            let failed = match git::ensure_clone(&home, &repo.repo, &repo.remote, refspecs).await {
                Ok(clone) => return Ok(Some((clone, prepared))),
                Err(failed) => failed,
            };
            let e = format!("Couldn't get {} ready: {failed}", repo.repo);
            if tries > RETRY_WAITS.len() || !crate::publish::passes_later(&failed.said()) {
                return Err(End::Failed(e));
            }
            drop(prepared);
            if !self.wait_queued(id, signal, &e, tries).await? {
                return Ok(None);
            }
            self.update(id, |job| {
                job.step = format!("Trying again ({tries} of {})", RETRY_WAITS.len())
            });
            prepared = preparing.lock().await;
        }
        unreachable!("tries are counted out above")
    }

    /// RECOVERY-14: why a job's `tool` that ended before the job had a session or `turn` a result
    /// may start on a later try: the last line it wrote (`agent.log` past `from`) is the network's,
    /// in the tool's own phrases ([`agent::Cause::read`]), and neither the tool nor the account's
    /// usage says it's full, which is #1563's.
    pub(super) fn offline(
        &self,
        id: &str,
        tool: Tool,
        turn: &agent::Turn,
        from: u64,
    ) -> Option<String> {
        let job = self.get(id).filter(|j| j.session.is_none())?;
        let usage = self.usage.get(&job.account);
        let at = obi_protocol::time::now();
        let full =
            turn.resets.is_some() || usage.full_until_at(at).is_some() || usage.window_full(at);
        if full || turn.session.is_some() || turn.result.is_some() {
            return None;
        }
        let note = self.agent_note_from(id, from);
        let last = note.lines().rfind(|l| !l.trim().is_empty())?.trim();
        let passing = matches!(
            agent::Cause::read(last),
            agent::Cause::Passing | agent::Cause::Busy
        );
        passing.then(|| format!("{} stopped before it started: {last}", tool.name()))
    }
}

/// A job waiting for its first fetch or launch (RECOVERY-14) is queued again, unless it was
/// stopped meanwhile.
fn queued(job: &mut Job) {
    if job.status.is_active() {
        job.status = Status::Queued;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RECOVERY-14, RECOVERY-17: a job that got its 10 continuations doesn't wait to fetch again:
    /// a first fetch the network failed leaves it interrupted with the cap's reason, at once.
    #[tokio::test]
    async fn recovery_14_a_job_at_the_cap_doesnt_retry_its_prepare() {
        let root = std::env::temp_dir().join(format!("obid-cap-14-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        std::fs::create_dir_all(home.join("jobs")).unwrap();
        let id = "eeee0001";
        let mut job = Job::new(
            id.into(),
            "o/r".into(),
            "fix".into(),
            "Fix it".into(),
            "claude".into(),
            "c".into(),
        );
        job.status = Status::Stopped;
        job.recovery.tries = crate::job::Recovery::ALL_TRIES;
        std::fs::write(
            home.join(format!("jobs/{id}.json")),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://127.0.0.1:9/o/r.git"}]})
            .to_string(),
        )
        .unwrap();
        let jobs = Jobs::open(config, Events::new(), Err("no sandbox".into())).unwrap();
        jobs.update(id, |job| job.status = Status::Running);
        let job = jobs.get(id).unwrap();
        let repo = jobs.repo("o/r").unwrap();
        let refspecs = ["+refs/heads/main:refs/remotes/origin/main".to_string()];
        let (_run, signal) = watch::channel(Signal::Run);
        let preparing = tokio::sync::Mutex::new(());
        let held = preparing.lock().await;
        let ready = jobs.ready_clone(id, &signal, &job, &repo, &refspecs, &preparing, held);
        // Without the cap it would wait 30 seconds to try again.
        let ended = tokio::time::timeout(Duration::from_secs(20), ready)
            .await
            .expect("it waited to try again");
        match ended {
            Err(End::NeedsYou { reason, next, .. }) => {
                assert!(reason.contains("by itself 10 times"), "{reason}");
                assert!(reason.contains("Couldn't get o/r ready"), "{reason}");
                assert_eq!(next, Some(crate::job::TRY_AGAIN));
            }
            Err(other) => panic!("not interrupted at the cap: {other:?}"),
            Ok(_) => panic!("the fetch worked"),
        }
        let after = jobs.get(id).unwrap();
        assert_eq!(after.recovery.tries, crate::job::Recovery::ALL_TRIES);
        assert_eq!(after.status, Status::Running, "{after:?}");
        let _ = std::fs::remove_dir_all(root);
    }
}
