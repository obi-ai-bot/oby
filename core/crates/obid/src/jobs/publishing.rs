//! A job's end and its publishing: by itself once its check passed, and by hand on a device's
//! signed approval (PUBLISHING-8 to 11), its artifacts and the commits it counts. It merges
//! nothing: merging is the pull request follower's.

use super::*;

impl Jobs {
    /// Publishes each entry of the job's `.obi/artifacts/` (crate::artifacts): a new artifact of
    /// the job, or a new version of the one that entry made before; the same files again change
    /// nothing. What was refused is said in `artifact_problems`, entry by entry.
    pub(super) async fn publish_artifacts(&self, id: &str) {
        let (Some(artifacts), Some(mut job)) = (self.artifacts.get(), self.get(id)) else {
            return;
        };
        // Its run with no proxy of obid's, however it came to be one (LANE-3a).
        job.ran_unproxied |= self.ran_unproxied(id);
        let worktree = self.worktree_path(id);
        let mut listed = job.artifacts.clone();
        let mut problems = Vec::new();
        let entries = crate::artifacts::job_entries(&worktree).unwrap_or_else(|why| {
            problems.push(why);
            Vec::new()
        });
        let more = entries
            .len()
            .saturating_sub(crate::artifacts::MAX_JOB_ENTRIES);
        let taken = entries.len().min(crate::artifacts::MAX_JOB_ENTRIES);
        for (done, entry) in entries.into_iter().take(taken).enumerate() {
            let path = format!("{}/artifacts/{entry}", crate::obi_dir::OBI);
            let title = crate::artifacts::title_of(&entry);
            let before = listed.iter().position(|a| a.entry == entry);
            let published = artifacts
                .publish(crate::artifacts::Publish {
                    root: &worktree,
                    path: &path,
                    title: &title,
                    kind: None,
                    conversation: job.conversation.as_deref(),
                    job: Some(id),
                    artifact: before.map(|i| listed[i].id.as_str()),
                    kept: before.map(|i| listed[i].version),
                    marks: shaped_by(&job),
                })
                .await;
            match (published, before) {
                (Ok((made, version)), Some(i)) => {
                    listed[i] = JobArtifact {
                        id: made,
                        version,
                        entry,
                    }
                }
                (Ok((made, version)), None) => listed.push(JobArtifact {
                    id: made,
                    version,
                    entry,
                }),
                // A name refused is never said, the entry's least of all.
                (Err(why), _) if why == crate::artifacts::NAME_REFUSED => problems.push(why),
                // The job's artifacts are full: one line for this entry and the rest.
                (Err(why), _) if why.starts_with(crate::artifacts::JOB_FULL) => {
                    problems.push(format!("{} entries: {why}", taken - done));
                    break;
                }
                // Any other refusal names the entry, only when no part of it looks like a
                // secret (crate::artifacts, `speakable`, reads each part on its own).
                (Err(why), _) => {
                    problems.push(artifacts.speakable(format!("{entry}: {why}")).await)
                }
            }
        }
        if more > 0 {
            problems.push(format!(
                "{more} more entries: Not published: a job publishes at most {} artifacts.",
                crate::artifacts::MAX_JOB_ENTRIES
            ));
        }
        if listed != job.artifacts || problems != job.artifact_problems {
            self.update(id, |job| {
                job.artifacts = listed;
                job.artifact_problems = problems;
            });
        }
    }

    /// Ends the job `id` done in its round `ours` with `commits` (and `pushed` of them on its pull
    /// request), then `change`: unless it was stopped meanwhile or its round moved on (TASKS-22).
    pub(super) fn end_done(
        &self,
        id: &str,
        ours: u32,
        commits: u64,
        pushed: Option<u64>,
        change: impl FnOnce(&mut Job),
    ) {
        self.update(id, |job| {
            if !job.status.is_active() || job.current_round() != ours {
                return;
            }
            job.status = Status::Done;
            job.step = String::new();
            job.asked = false;
            job.commits = Some(commits);
            job.pushed = pushed;
            job.publish_step = None;
            job.finished = Some(obi_protocol::time::now());
            change(job);
            // Its issue unread, it worked from its task alone (ISSUETASK-2).
            if let Some(unread) = &job.issue_unread {
                let line = format!("{unread}: its agent worked from the task alone.");
                if !job.summary.contains(&line) {
                    job.summary = format!(
                        "{}

{line}",
                        job.summary.trim_end()
                    );
                }
            }
        });
    }

    /// The commits `job` publishes, up to `head` in `worktree`: its own from `start_commit`, and,
    /// for a follow-up that started from its first job's work, those it carries (#770): what its
    /// branch has over the last commit any job pushed to that branch or to the one it started from
    /// (unless that work's pull request was closed unmerged), or, when none is under `head` (a
    /// branch of its own, or one rebased since), over its base or its pull request's head.
    pub(super) async fn counted(
        &self,
        job: &Job,
        worktree: &Path,
        folders: &[(&'static str, String)],
        head: &str,
    ) -> u64 {
        let start_commit = job.start_commit.as_deref().unwrap_or_default();
        if job.start_from.is_none() || head.is_empty() {
            return count_commits(worktree, folders, start_commit, head).await;
        }
        let from_out = self.first_turned_down(job).is_none();
        let out = |branch: Option<&str>| {
            branch == Some(job.branch.as_str()) || from_out && branch == job.start_from.as_deref()
        };
        let pushed: Vec<String> = self
            .lock()
            .jobs
            .values()
            .filter(|j| j.repo == job.repo)
            .flat_map(|j| j.outgoing.iter())
            .filter(|o| o.action == "push" && out(o.branch.as_deref()))
            .map(|o| o.id.clone())
            .collect();
        let mut least: Option<u64> = None;
        for there in pushed.iter().filter(|there| git::is_sha(there)) {
            let under = !folders.is_empty()
                && git::pinned(
                    worktree,
                    folders,
                    &["merge-base", "--is-ancestor", there, head],
                )
                .await
                .is_ok();
            if under {
                let count = count_commits(worktree, folders, there, head).await;
                least = Some(least.map_or(count, |least| least.min(count)));
            }
        }
        if let Some(count) = least {
            return count;
        }
        let from = match job.pr {
            Some(pr) => format!("refs/remotes/origin/pr/{pr}"),
            None => format!(
                "refs/remotes/origin/{}",
                job.base.as_deref().unwrap_or_default()
            ),
        };
        count_commits(worktree, folders, &from, head).await
    }

    /// The title of the job whose work `job` carries, through the follow-ups between them, when
    /// that one's pull request was closed without merging: the owner turned its commits down, so a
    /// follow-up that adds none doesn't send them again by itself (#770).
    pub(super) fn first_turned_down(&self, job: &Job) -> Option<String> {
        let mut job = job.clone();
        // ponytail: a bounded walk; follow-up chains are short.
        for _ in 0..64 {
            job.start_from.as_ref()?;
            let first = self.get(job.follow_up_of.as_deref()?)?;
            if first.published.is_some() {
                let closed = first
                    .pull_request
                    .as_ref()
                    .is_some_and(|pr| pr.state == "CLOSED");
                return closed.then(|| first.short_title());
            }
            job = first;
        }
        None
    }

    /// How many of `commits` (up to `head`) the remote has: those in the commit obid pushed last,
    /// with a pull request or not. A rebase rewrites them, so then it's none, and they're published
    /// again. The count from its last publish when there's no pushed commit to compare with or git
    /// can't say.
    pub(super) async fn still_pushed(&self, job: &Job, head: &str, commits: u64) -> Option<u64> {
        let Some(pushed_head) = job.pushed_head() else {
            return job.pushed;
        };
        let clone = self.clone_for(job);
        let unpushed = git::git(
            &clone,
            &["rev-list", "--count", &format!("{pushed_head}..{head}")],
        )
        .await
        .ok()
        .and_then(|n| n.parse::<u64>().ok());
        match unpushed {
            Some(unpushed) => Some(commits.saturating_sub(unpushed)),
            None => job.pushed,
        }
    }

    pub(super) fn publish_target(&self, job: &Job) -> Result<crate::publish::Target, String> {
        let repo = self
            .repo(&job.repo)
            .ok_or_else(|| format!("Not published: {} isn't a repository obid knows.", job.repo))?;
        let scratch = self
            .config
            .home
            .join("tmp")
            .join(format!("publish-{}", job.id));
        let _ = std::fs::remove_dir_all(&scratch);
        crate::store::make_private_dir(&scratch)
            .map_err(|e| format!("Not published: obid couldn't make its scan's folder ({e})."))?;
        let chain = crate::publish::BranchChain::of(job, &self.lock().jobs);
        Ok(crate::publish::Target {
            chain,
            clone: self.clone_for(job),
            remote: repo.remote.clone(),
            default_branch: repo.default_branch.clone(),
            scratch,
            gh: crate::github::gh(),
            pull_requests: repo
                .pull_requests
                .unwrap_or_else(|| crate::github::is_remote(&repo.remote)),
            fetching: self.preparing(&repo.repo),
            workflow: crate::memory::workflow(&self.config.home, &repo.repo)
                .and_then(|m| m.workflow),
        })
    }

    /// Publishes once per job; `signal` stops it before the push and pull request.
    pub(super) async fn publish_job(
        self: &Arc<Self>,
        id: &str,
        signal: Option<watch::Receiver<Signal>>,
        may: impl FnOnce(&Job, &HashMap<String, Job>) -> Result<(), crate::publish::Failure>,
        by_hand: &crate::publish::ByHand,
    ) -> Result<Option<String>, crate::publish::Failure> {
        use crate::publish::Failure;
        // `may` and the claim under one lock: nothing changes the job between them.
        let mut job = {
            let mut state = self.lock();
            let job = state
                .jobs
                .get(id)
                .cloned()
                .ok_or_else(|| Failure::Failed("There's no such job.".into()))?;
            may(&job, &state.jobs)?;
            if !state.publishing.insert(id.to_owned()) {
                return Err(Failure::Busy);
            }
            job
        };
        // Dropping a publish never leaves its claim until restart (#914), nor
        // showing a publish step nothing runs any more (MERGING-29).
        struct Claim<'a>(&'a Jobs, &'a str, bool);
        impl Drop for Claim<'_> {
            fn drop(&mut self) {
                // The step first: a publish that marks its step before it claims the job (a
                // job's own) can't have its step cleared once the claim is free.
                if !self.2 {
                    self.0.update(self.1, |job| job.publish_step = None);
                }
                self.0.lock().publishing.remove(self.1);
            }
        }
        let mut claim = Claim(self, id, false);
        let Some(base) = job.base.clone() else {
            return Err(Failure::Failed(
                "Nothing to publish: it has no base.".into(),
            ));
        };
        let outcome = match self.publish_target(&job) {
            Ok(target) => {
                let hooks = PublishHooks {
                    jobs: self.clone(),
                    id: id.to_owned(),
                    signal,
                    stages: Stages::new(self.log_path(id, "stages.log")),
                };
                if crate::publish::stopped_being_own(&job, &target, by_hand).await {
                    self.update(id, |job| job.made_branch = false);
                    job.made_branch = false;
                }
                let outcome = crate::publish::publish(&job, &base, &target, &hooks, by_hand).await;
                let _ = std::fs::remove_dir_all(&target.scratch);
                outcome
            }
            Err(why) => Err(Failure::Failed(why)),
        };
        claim.2 = true;
        drop(claim);
        if let Err(failure) = &outcome {
            tracing::info!("job {id}: not published: {}", failure.line());
        }
        outcome
    }

    /// What publishing the job `id` by hand would do, with what the owner asks beyond the default
    /// (`by_hand`): obid's own sentence and the base, or why there's nothing to publish (`409`).
    pub fn publish_action(
        &self,
        id: &str,
        by_hand: &crate::publish::ByHand,
    ) -> Option<Result<(String, String), String>> {
        let state = self.lock();
        Some(publish_action_of(state.jobs.get(id)?, &state.jobs, by_hand))
    }

    /// What the owner is shown before a Publish that asks more (`by_hand`): the commit Publish
    /// anyway sends (the worktree's head for a job that didn't end done, else its branch's in
    /// obid's clone), and the commit a force-push replaces on the remote (empty when there's
    /// none), so their signature names both (PUBLISHING-9, 10).
    pub async fn shown(
        &self,
        id: &str,
        mut by_hand: crate::publish::ByHand,
    ) -> Result<crate::publish::ByHand, String> {
        let job = self.get(id).ok_or("There's no such job.")?;
        if by_hand.anyway {
            let worktree = self.worktree_path(id);
            let in_worktree = match job.status != Status::Done && worktree.join(".git").exists() {
                true => match self.own_git_dirs(id).await {
                    Ok((admin, common)) => git::pinned(
                        &worktree,
                        &git_folders(&admin, &common, &worktree),
                        &["rev-parse", "--verify", "--quiet", "HEAD"],
                    )
                    .await
                    .ok(),
                    Err(_) => None,
                },
                false => None,
            };
            let head = match in_worktree.filter(|h| git::is_sha(h)) {
                Some(head) => head,
                None => crate::publish::git_in(
                    &self.clone_for(&job),
                    &[
                        "rev-parse",
                        "--verify",
                        &format!("refs/heads/{}^{{commit}}", job.branch),
                    ],
                )
                .await
                .ok()
                .filter(|h| git::is_sha(h))
                .ok_or_else(|| format!("Can't read {} in obid's clone.", job.branch))?,
            };
            by_hand.head = Some(head);
        }
        if by_hand.force {
            // Its scan's folder is the publish's own: not while one runs.
            if self.lock().publishing.contains(id) {
                return Err("obid is publishing it now. Ask again once that's done.".into());
            }
            let target = self.publish_target(&job)?;
            let there = crate::publish::remote_head(by_hand.branch(&job), &target).await;
            let _ = std::fs::remove_dir_all(&target.scratch);
            by_hand.over = Some(there.map_err(|f| f.line())?.unwrap_or_default());
        }
        Ok(by_hand)
    }

    /// [`Jobs::publish_action`], said to a device that will ask its owner to approve it: obid
    /// keeps, with the job, which of its runs it described and when (`Job::offers`).
    pub fn offer_publish(
        &self,
        id: &str,
        by_hand: &crate::publish::ByHand,
    ) -> Option<Result<(String, String), String>> {
        let answer = {
            let state = self.lock();
            publish_action_of(state.jobs.get(id)?, &state.jobs, by_hand)
        };
        if answer.is_ok() {
            self.update(id, |job| job.offered(obi_protocol::time::now()))?;
        }
        Some(answer)
    }

    /// The approvals devices give to publish by hand.
    pub fn approvals(&self) -> &crate::approval::Approvals {
        &self.approvals
    }

    /// Publishes the job `id` by hand, once a device approved `action` at `signed_at`: only when
    /// that is still obid's own sentence for it, and it can only have been said of the job's
    /// latest run (`Job::approvable_at`: the sentence names the branch, not its commits). Answers
    /// the pull request's URL (none for a remote that has no pull requests), or the line saying why
    /// not. Never a merge.
    pub async fn publish_by_hand(
        self: &Arc<Self>,
        id: &str,
        action: &str,
        signed_at: i64,
        by_hand: &crate::publish::ByHand,
    ) -> Result<Option<String>, String> {
        use crate::publish::Failure;
        // Still obid's own sentence for the run that was approved, checked as it's claimed.
        let may = |job: &Job, jobs: &HashMap<String, Job>| {
            match publish_action_of(job, jobs, by_hand) {
            Ok(_) if !job.approvable_at(signed_at) => Err(Failure::Refused(
                "The job ran again since obid said what publishing it would do. Approve it again in two minutes.".into(),
            )),
            Ok((now, _)) if now == action => Ok(()),
            Ok(_) => Err(Failure::Refused(
                "The job changed since it was approved. Approve it again.".into(),
            )),
            Err(why) => Err(Failure::Refused(why)),
        }
        };
        // Publish anyway: a job whose check failed never took its commits into the clone, so its
        // worktree's head goes there first (PUBLISHING-9); a done one's are there already. One
        // whose worktree is gone publishes its branch as it is.
        if by_hand.anyway
            && self
                .get(id)
                .is_some_and(|j| !j.status.is_active() && j.status != Status::Done)
            && let Err(why) = self.keep_commits(id).await
        {
            tracing::info!("job {id}: publish anyway takes its branch as it is: {why}");
        }
        match self.publish_job(id, None, may, by_hand).await {
            Ok(Some(url)) => {
                self.update(id, |job| {
                    job.publish_step = None;
                    job.pushed = job.commits;
                    job.was_published(url.clone());
                });
                self.follow.notify_one();
                Ok(Some(url))
            }
            Ok(None) => {
                self.update(id, |job| {
                    job.publish_step = None;
                    job.pushed = job.commits;
                    job.publish_error = None;
                });
                Ok(None)
            }
            Err(Failure::AlreadyOnBase(base)) => {
                let line = Failure::AlreadyOnBase(base).line();
                self.update(id, |job| {
                    job.publish_step = None;
                    if job.can_publish() {
                        job.summary = format!("{line}\n\n{}", job.summary);
                        job.commits = Some(0);
                        job.publish_error = None;
                    }
                });
                Err(line)
            }
            // Nothing ran: the job is as it was (the publish under way says how it went).
            Err(failure @ (Failure::Busy | Failure::Refused(_))) => Err(failure.line()),
            Err(failure) => {
                let line = failure.line();
                let pushed = matches!(failure, Failure::PullRequest(_));
                self.update(id, |job| {
                    job.publish_step = None;
                    if pushed {
                        job.pushed = job.commits;
                    }
                    if job.can_publish() {
                        job.publish_error = Some(line.clone());
                    }
                });
                Err(line)
            }
        }
    }
}

/// What publishing tells a job as it goes (crate::publish::Hooks).
struct PublishHooks {
    jobs: Arc<Jobs>,
    id: String,
    /// The job's worker's signal, when it publishes by itself: a stop means nothing more goes out.
    signal: Option<watch::Receiver<Signal>>,
    /// Each step's time, in the job's stages.log.
    stages: Stages,
}

impl crate::publish::Hooks for PublishHooks {
    fn progress(&self, step: crate::publish::Step) {
        self.stages.begin(step.as_str());
        self.jobs.update(&self.id, |job| {
            job.publish_step = Some(step);
            // A new publish: the last one's scan no longer says how this one went.
            if step == crate::publish::Step::Scan {
                job.scan = None;
            }
        });
    }

    fn scanned(&self, scanned: crate::publish::Scanned) {
        self.jobs.update(&self.id, |job| job.scan = Some(scanned));
    }

    fn proceed(&self) -> bool {
        self.signal
            .as_ref()
            .is_none_or(|signal| *signal.borrow() == Signal::Run)
    }

    fn by_itself(&self) -> bool {
        self.signal.is_some()
    }

    fn named_on_github(&self, name: &str) {
        self.jobs
            .update(&self.id, |job| job.repo_on_github = Some(name.into()));
    }

    fn sent(&self, outgoing: crate::publish::Outgoing) {
        // The journal: one line per action, ids only.
        tracing::info!(
            "job {}: sent {} {} {} {}",
            self.id,
            outgoing.action,
            outgoing.repo,
            outgoing.branch.as_deref().unwrap_or(""),
            outgoing.id
        );
        if outgoing.action == "push" {
            self.jobs.pushed(&outgoing);
        }
        if matches!(outgoing.action.as_str(), "pullRequest" | "pullRequestEdit") {
            self.jobs.note_described(&self.id, &outgoing.id);
        }
        self.jobs
            .update(&self.id, |job| job.outgoing.push(outgoing));
    }
}

/// What a version `job` publishes keeps of it (ARTIFACTS-24, 25): its someone-else's mark, its
/// owner's words unsigned or a memory from others included ([`Job::untrusted_why`]), its reach
/// past the registries, and its run with no egress proxy (LANE-3a), as they are when it
/// publishes.
pub(super) fn shaped_by(job: &Job) -> crate::artifacts::Marks {
    let why = |why: String| format!("an artifact's files a job made: {why}");
    crate::artifacts::Marks {
        foreign: job.untrusted_why().map(why),
        reached: job.reached.clone().map(why),
        unproxied: job.ran_unproxied,
    }
}
