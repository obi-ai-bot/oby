//! Reading each account's usage and models (USAGE-8), and metering the machine's memory and each
//! running job's processes (`peakRss`), which builds are sized by, and on the same tick what its
//! agent's processes listen on (crate::jobs::servers). It keeps what it read; it starts, stops and
//! publishes no job.

use super::*;

impl Jobs {
    /// Reads the machine's memory and each running job's processes every [`METER_TICK`] while
    /// any job runs, and nothing otherwise (crate::meter).
    pub async fn meter(self: Arc<Self>, mut stopping: watch::Receiver<bool>) {
        loop {
            // Every worker, idle ones too: one back from its wait sends no word to start again.
            if self.lock().workers.is_empty() {
                tokio::select! {
                    () = self.metering.notified() => continue,
                    _ = stopping.changed() => return,
                }
            }
            self.measure().await;
            tokio::select! {
                () = tokio::time::sleep(METER_TICK) => {}
                _ = stopping.changed() => return,
            }
        }
    }

    /// An app looks at the accounts (`GET /v1/status`, its event stream opening): each signed-in
    /// account not asked in the last [`usage_every`] is asked now, one after another in the
    /// background, so the apps show its usage before any job runs on it: Claude Code's own
    /// `/usage`, Codex's `account/rateLimits/read` and Cursor's dashboard (crate::cursor_usage),
    /// none asking a model. While no app looks, nothing is read: a running job reports its own.
    /// `OBID_USAGE_READS=off` (obid's tests set it) reads none.
    pub fn want_usage(self: &Arc<Self>) {
        if std::env::var_os("OBID_USAGE_READS").is_some_and(|v| v == "off")
            || self.shutting_down.load(Ordering::Relaxed)
            || std::mem::replace(&mut self.usage_reads().reading, true)
        {
            return;
        }
        let jobs = self.clone();
        tokio::spawn(async move {
            let _reading = Reading(&jobs);
            jobs.read_due_usages().await;
        });
    }

    pub(super) fn usage_reads(&self) -> std::sync::MutexGuard<'_, UsageReads> {
        self.usage_reads.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// [`Jobs::want_usage`]'s reads.
    pub(super) async fn read_due_usages(&self) {
        self.read_usages(false).await;
    }

    /// Each signed-in account's usage not asked in the last [`usage_every`]; `routing` (a task about
    /// to be routed, #2484), only those whose kept reading is older than that or missing, which
    /// routing would otherwise take for room, and never a figure that holds nothing (Cursor's) nor
    /// the models.
    pub(super) async fn read_usages(&self, routing: bool) {
        use crate::usage::Asked;
        let home = login_home();
        let every = usage_every();
        let now = obi_protocol::time::now();
        for tool in Tool::ALL {
            for account in self.accounts(tool) {
                if self.shutting_down.load(Ordering::Relaxed) {
                    return;
                }
                let id = account.id.clone();
                let kept = self.usage.get(&id);
                let seen = kept.five_hour_observed_at.max(kept.seven_day_observed_at);
                let fresh = seen.is_some_and(|at| now - at < every.as_secs() as i64);
                if routing && (fresh || kept.advisory || tool == Tool::Cursor) {
                    continue;
                }
                let due = {
                    let reads = self.usage_reads();
                    !reads.never.contains(&id)
                        && reads.asked.get(&id).is_none_or(|at| at.elapsed() >= every)
                };
                // A ChatGPT login has no usage to read (docs/contracts/chatgpt-sign-in.md).
                if !due || self.being_changed(&id) || crate::chatgpt::marker_of(&account).is_some()
                {
                    continue;
                }
                // Off the runtime, as `GET /v1/status`'s: for Cursor's own login this reads the
                // Keychain, which can wait on macOS's prompt.
                let signed = {
                    let (account, home) = (account.clone(), home.clone());
                    tokio::task::spawn_blocking(move || signed_in(tool, &account, home.as_deref()))
                        .await
                        .unwrap_or(false)
                };
                if !signed {
                    self.forget_advisory_usage(&id);
                    continue;
                }
                self.usage_reads()
                    .asked
                    .insert(id.clone(), std::time::Instant::now());
                let name = tool.name();
                let read = self.read_usage(tool, &account).await;
                if !matches!(read, Ok(Asked::Said(_))) {
                    self.forget_advisory_usage(&id);
                }
                match read {
                    // Not for an account removed or being changed meanwhile: its usage is forgotten.
                    Ok(Asked::Said(usage))
                        if !self.being_changed(&id)
                            && self.accounts(tool).iter().any(|a| a.id == id) =>
                    {
                        let (was, now) = self.usage.record(&id, &usage);
                        if !usage.advisory {
                            self.share_rise(&id, &was, &now);
                        }
                        self.usage_changed.notify_one();
                    }
                    Ok(Asked::Said(_)) => {}
                    // An API key's account, or one its tool reports none for now: asked again
                    // after the interval, as any read is.
                    Ok(Asked::Silent) => tracing::info!("{name} account {id} reports no usage"),
                    Ok(Asked::ModelRan) => {
                        self.usage_reads().never.insert(id.clone());
                        tracing::warn!(
                            "{name} account {id}: /usage asked a model, so this Claude Code has no /usage of its own: not asked again until obid restarts"
                        );
                    }
                    Err(why) => tracing::info!("{name} account {id}: usage not read: {why}"),
                }
                // Same cadence as usage: each account's own model list, never blocking a turn or job.
                if !routing
                    && !self.being_changed(&id)
                    && self.accounts(tool).iter().any(|a| a.id == id)
                {
                    self.refresh_models(tool, &account).await;
                }
            }
        }
    }

    /// One account's models from its CLI (crate::model_lists). Keeps the last good list; a failed
    /// read changes nothing. Notifies when the list changed so apps hear it on `accounts`.
    async fn refresh_models(&self, tool: Tool, account: &crate::config::Account) {
        let Some(bin) = self.agent_bin(tool).filter(|bin| bin.is_absolute()) else {
            return;
        };
        let dir = self
            .config
            .home
            .join("plans")
            .join(uuid::Uuid::new_v4().to_string());
        if crate::store::make_private_dir(&dir).is_err() {
            return;
        }
        let scratch = crate::model_lists::Scratch(dir);
        if let Some(listed) = crate::model_lists::read(tool, &bin, account, &scratch.0).await
            && self.model_lists.keep(&account.id, listed)
        {
            self.usage_changed.notify_one();
        }
    }

    /// A read that gave no figure leaves none (USAGE-8): Cursor's isn't kept past its read.
    fn forget_advisory_usage(&self, id: &str) {
        if self.usage.forget_advisory(id) {
            self.usage_changed.notify_one();
        }
    }

    /// One account's usage: Cursor's from its dashboard with its access token, by obid itself and
    /// never in a box (crate::cursor_usage); the others' from the tool boxed as the brain's run is,
    /// in a folder of its own in `home/plans` with its own copy of the account's login, removed
    /// however the read ends. A
    /// Claude Code login is made ready first as a job's is ([`Jobs::claude_login_ready`]): one in
    /// the Keychain that ended or ends within 5 minutes is refreshed by the Mac's own Claude Code
    /// running its own `/usage`, which asks no model (USAGE-2), so a Mac whose CLI hasn't run in
    /// days shows its usage too. The boxed read itself never refreshes: a copy that ends within
    /// 10 minutes isn't read with (a run refreshes in its last 5, rotating the Mac's own away).
    async fn read_usage(
        &self,
        tool: Tool,
        account: &crate::config::Account,
    ) -> Result<crate::usage::Asked, String> {
        if tool == Tool::Cursor {
            let (account, home) = (account.clone(), login_home());
            // Not on the runtime: a Keychain read can wait on macOS's prompt.
            let token =
                tokio::task::spawn_blocking(move || cursor_access_token(&account, home.as_deref()))
                    .await
                    .ok()
                    .flatten()
                    .ok_or("its login has no access token that hasn't ended")?;
            return crate::cursor_usage::read_usage(&token).await;
        }
        if tool == Tool::Claude {
            self.claude_login_ready(Some(account)).await?;
        }
        let dir = ReadFolder(
            self.config
                .home
                .join("plans")
                .join(uuid::Uuid::new_v4().to_string()),
        );
        let temp = dir.0.join("home");
        store::make_private_dir(&temp)
            .map_err(|e| format!("obid can't make the read's folder: {e}"))?;
        let flags = match tool {
            Tool::Claude => agent::USAGE_FLAGS.map(OsString::from).to_vec(),
            // No plug-in sync, as a job's Codex (`launch`).
            _ => vec!["-c".into(), "features.plugins=false".into()],
        };
        let login = dir.0.join("login");
        let launch = self.boxed_launch(Boxed {
            tool,
            account,
            dir: &dir.0,
            temp: &temp,
            login: &login,
            frozen: Vec::new(),
            flags,
            stderr: dir.0.join("stderr.log"),
            confined: false,
            door: None,
        })?;
        match tool {
            Tool::Claude if !login_copy_lasts_past(&login, now_ms() + USAGE_LOGIN_MARGIN_MS) => {
                Err("its login ends within 10 minutes: read once it's refreshed".into())
            }
            Tool::Claude => agent::read_usage(&launch, USAGE_READ_LIMIT).await,
            _ => codex::read_usage(&launch, USAGE_READ_LIMIT).await,
        }
    }

    /// One reading: the machine, and each running job's processes, those running in its worktree.
    async fn measure(&self) {
        let folders: Vec<(String, PathBuf)> = {
            let state = self.lock();
            state
                .workers
                .keys()
                .filter_map(|id| {
                    let worktree = state.jobs.get(id)?.worktree.as_ref()?;
                    Some((id.clone(), PathBuf::from(worktree)))
                })
                .collect()
        };
        let Ok((machine, rss, servers)) = tokio::task::spawn_blocking(move || {
            let table = crate::meter::processes();
            let tree = |folder: &Path, checks| {
                crate::meter::tree_rss(&table, &crate::meter::roots_in(folder, checks))
            };
            let rss: Vec<(String, u64, u64)> = folders
                .iter()
                .map(|(id, folder)| {
                    let (agent, check) = (tree(folder, false), tree(folder, true));
                    (id.clone(), agent, check)
                })
                .collect();
            (crate::meter::machine(), rss, listening(&table, folders))
        })
        .await
        else {
            return;
        };
        for memory in self.record(machine, rss) {
            self.keep_peaks(&memory);
        }
        if let Some(servers) = servers {
            self.record_servers(servers);
        }
    }

    /// Keeps a reading: the machine's, and each job's that still has a worker. A job that ended
    /// while it was read is left out: the jobs' lock is held throughout, as a worker's end holds it
    /// to give its slot back before [`Jobs::metered`] takes the job's entry away, so an entry
    /// never comes back for a job that ended. `rss` is each job's agent's tree and its check's.
    /// Answers the jobs whose peaks grew past what they keep by [`PEAK_STEP`] (or a quarter), to be
    /// kept now ([`Jobs::keep_peaks`]).
    pub(super) fn record(
        &self,
        machine: Option<crate::meter::Machine>,
        rss: Vec<(String, u64, u64)>,
    ) -> Vec<JobMemory> {
        let state = self.lock();
        let mut measured = self.measured();
        if let Some(machine) = machine {
            measured.machine = Some((std::time::Instant::now(), machine));
        }
        let grew = |kept: Option<u64>, now: u64| {
            let kept = kept.unwrap_or(0);
            now > kept + PEAK_STEP.max(kept / 4)
        };
        let mut due = Vec::new();
        for (id, agent, check) in rss {
            let Some(job) = state
                .jobs
                .get(&id)
                .filter(|_| state.workers.contains_key(&id))
            else {
                continue;
            };
            let memory = measured.jobs.entry(id.clone()).or_insert(JobMemory {
                id,
                rss: 0,
                peak_rss: 0,
                agent_peak_rss: 0,
                check_peak_rss: 0,
                agent_rss: 0,
                check_rss: 0,
            });
            memory.agent_rss = agent;
            memory.check_rss = check;
            memory.rss = agent + check;
            memory.peak_rss = memory.peak_rss.max(memory.rss);
            memory.agent_peak_rss = memory.agent_peak_rss.max(agent);
            memory.check_peak_rss = memory.check_peak_rss.max(check);
            if grew(job.agent_peak_rss, memory.agent_peak_rss)
                || grew(job.check_peak_rss, memory.check_peak_rss)
            {
                due.push(memory.clone());
            }
        }
        due
    }

    /// Keeps a job's peaks with it (`peakRss`, `agentPeakRss`, `checkPeakRss`): the larger of
    /// what it kept and `memory`'s.
    fn keep_peaks(&self, memory: &JobMemory) {
        let larger =
            |kept: Option<u64>, now: u64| Some(kept.unwrap_or(0).max(now)).filter(|p| *p > 0);
        self.update(&memory.id, |job| {
            job.peak_rss = larger(job.peak_rss, memory.peak_rss);
            job.agent_peak_rss = larger(job.agent_peak_rss, memory.agent_peak_rss);
            job.check_peak_rss = larger(job.check_peak_rss, memory.check_peak_rss);
        });
    }

    pub(super) fn measured(&self) -> std::sync::MutexGuard<'_, Measured> {
        self.measured
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A job's worker ended and gave its slot back: the most its processes held at once is kept
    /// with it (`peakRss`), and it's no longer measured.
    pub(super) fn metered(&self, id: &str) {
        self.servers_ended(id);
        self.measured().granted.remove(id);
        let Some(memory) = self.measured().jobs.remove(id) else {
            return;
        };
        if memory.peak_rss > 0 {
            self.keep_peaks(&memory);
        }
    }

    /// The machine's memory, as the meter last read it or now when that's older than a tick
    /// (nothing runs, so the meter sleeps), and each running job's; `None` when it can't be read.
    pub async fn memory(&self) -> Option<(crate::meter::Machine, Vec<JobMemory>)> {
        let fresh = self
            .measured()
            .machine
            .filter(|(at, _)| at.elapsed() < METER_TICK);
        let machine = match fresh {
            Some((_, machine)) => machine,
            None => {
                let machine = tokio::task::spawn_blocking(crate::meter::machine)
                    .await
                    .ok()??;
                self.measured().machine = Some((std::time::Instant::now(), machine));
                machine
            }
        };
        let measured = self.measured();
        let mut jobs: Vec<JobMemory> = measured.jobs.values().cloned().collect();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        Some((machine, jobs))
    }

    /// The machine's memory to size builds by: the meter's reading while it's at most two ticks
    /// old, else read now. ponytail: a few milliseconds of `sysctl` and `vm_stat` on the caller's
    /// thread, only when the meter sleeps (nothing runs).
    pub(super) fn reading(&self) -> Option<crate::meter::Machine> {
        if let Some((at, machine)) = self.measured().machine
            && at.elapsed() < 2 * METER_TICK
        {
            return Some(machine);
        }
        let machine = crate::meter::machine()?;
        self.measured().machine = Some((std::time::Instant::now(), machine));
        Some(machine)
    }
}

/// What each job's agent's processes listen on (crate::jobs::servers): one read for every running
/// job's agent together, of their own processes alone, or `None` when it couldn't be read.
fn listening(
    table: &[crate::meter::Process],
    folders: Vec<(String, PathBuf)>,
) -> Option<Vec<(String, Vec<crate::listening::Listening>)>> {
    let trees: Vec<(String, Vec<u32>)> = folders
        .into_iter()
        .map(|(id, folder)| {
            let roots = crate::meter::roots_in(&folder, false);
            (id, crate::meter::tree_pids(table, &roots))
        })
        .collect();
    let pids: Vec<u32> = trees
        .iter()
        .flat_map(|(_, pids)| pids.iter().copied())
        .collect();
    let found = crate::listening::listening(&pids)?;
    Some(
        trees
            .into_iter()
            .map(|(id, pids)| {
                let own = found.iter().filter(|l| pids.contains(&l.pid)).cloned();
                (id, own.collect())
            })
            .collect(),
    )
}

/// How often at most each account's usage is asked of its tool ([`Jobs::want_usage`]): every 10
/// minutes, or `OBID_USAGE_EVERY_SECONDS` (obid's tests set less). A running job's own reports
/// come between.
fn usage_every() -> Duration {
    std::env::var("OBID_USAGE_EVERY_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(Duration::from_secs(600), Duration::from_secs)
}
