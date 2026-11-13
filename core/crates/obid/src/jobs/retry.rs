//! Waiting to try again (RECOVERY-1, RECOVERY-14): a turn the network cut, and a job's first fetch.

use super::*;

impl Jobs {
    /// The owner asked in a conversation (RECOVERY-17): obid may carry `id` on by itself again,
    /// though their words reach it as an event's, so a #1563 hold still needs their signed choice.
    pub fn owner_asked(&self, id: &str) {
        self.update(id, owner_spoke);
    }

    /// Job `id` was made by a `POST /v1/jobs` with request key `key` (RECEIPTS-5): a retry of it
    /// gets this job back (docs/contracts/receipts.md).
    pub fn mark_request_key(&self, id: &str, key: &str) -> Option<Job> {
        self.update(id, |job| job.client_request_id = Some(key.to_owned()))
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

    /// RECOVERY-14's wait before try `attempt` after `error`: queued, keeping its slot as every
    /// retry's wait does (ADMISSION-4: only the check queue's frees one, so an outage starts no new
    /// jobs in their place), and one of the job's tries in all (RECOVERY-17): with none left, it's
    /// interrupted with the cap's reason. False when it was stopped meanwhile.
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
        Ok(self.wait_to_try(id, signal, error, wait, of, queued).await)
    }

    /// RECOVERY-14: the job's clone of `repo`, fetched with `refspecs`, under the clone's lock
    /// (`prepared`, of `preparing`), given back with it. A fetch git's own words say the network
    /// failed waits and tries again, [`RETRY_WAITS`]', queued and keeping its slot, the lock free
    /// meanwhile. None when it was stopped meanwhile, waiting for the lock again too.
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
            // Another job's fetch may hold it for minutes: a stop doesn't wait for that.
            let mut stop = signal.clone();
            prepared = tokio::select! {
                prepared = preparing.lock() => prepared,
                _ = stop.wait_for(|s| *s != Signal::Run) => return Ok(None),
            };
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

    const ID: &str = "eeee0001";

    /// obid with job [`ID`] running on `o/r`, whose remote refuses every connection, its
    /// continuations so far `tries`, and `stallAfter` `stall` seconds; and the folder it's all in.
    fn running(tries: usize, stall: u64) -> (PathBuf, Arc<Jobs>) {
        let root = std::env::temp_dir().join(format!("obid-retry-14-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        std::fs::create_dir_all(home.join("jobs")).unwrap();
        let mut job = Job::new(
            ID.into(),
            "o/r".into(),
            "fix".into(),
            "Fix it".into(),
            "claude".into(),
            "c".into(),
        );
        job.status = Status::Stopped;
        job.recovery.tries = tries;
        std::fs::write(
            home.join(format!("jobs/{ID}.json")),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "o", "stallAfter": stall,
                "repos": [{"repo": "o/r", "remote": "https://127.0.0.1:9/o/r.git"}]})
            .to_string(),
        )
        .unwrap();
        let jobs = Jobs::open(config, Events::new(), Err("no sandbox".into())).unwrap();
        jobs.update(ID, |job| job.status = Status::Running);
        (root, jobs)
    }

    /// RECOVERY-14: a stop while the job waits for its clone again, after a wait to fetch again,
    /// ends it there, though another job's fetch holds the clone.
    #[tokio::test]
    async fn recovery_14_stop_while_waiting_for_the_clone_again_ends_it() {
        let (root, jobs) = running(0, 1);
        let job = jobs.get(ID).unwrap();
        let repo = jobs.repo("o/r").unwrap();
        let refspecs = ["+refs/heads/main:refs/remotes/origin/main".to_string()];
        let (run, signal) = watch::channel(Signal::Run);
        let preparing = tokio::sync::Mutex::new(());
        let held = preparing.lock().await;
        let ready = jobs.ready_clone(ID, &signal, &job, &repo, &refspecs, &preparing, held);
        tokio::pin!(ready);
        let step = || jobs.get(ID).map(|job| job.step).unwrap_or_default();
        let (mut other, mut stopped) = (None, false);
        let ended = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                tokio::select! {
                    ended = &mut ready => break ended,
                    () = tokio::time::sleep(Duration::from_millis(20)) => {}
                }
                // Another job takes the clone while this one waits to fetch again.
                if other.is_none() && step().starts_with("Trying again in ") {
                    other = preparing.try_lock().ok();
                }
                // It has waited, and now waits for the clone: stopped there.
                if other.is_some() && !stopped && step().starts_with("Trying again (") {
                    run.send(Signal::Stop).unwrap();
                    stopped = true;
                }
            }
        })
        .await
        .expect("a stop didn't end its wait for the clone");
        assert!(stopped, "it never waited for the clone again: {}", step());
        assert!(matches!(ended, Ok(None)), "{ended:?}");
        drop(other);
        let _ = std::fs::remove_dir_all(root);
    }

    /// RECOVERY-14, RECOVERY-17: a job that got its 10 continuations doesn't wait to fetch again:
    /// a first fetch the network failed leaves it interrupted with the cap's reason, at once.
    #[tokio::test]
    async fn recovery_14_a_job_at_the_cap_doesnt_retry_its_prepare() {
        let (root, jobs) = running(crate::job::Recovery::ALL_TRIES, 900);
        let id = ID;
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
