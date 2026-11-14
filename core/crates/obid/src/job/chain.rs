//! A job's task chain (docs/ARCHITECTURE.md, "State ownership"): whether a follow-up carries a
//! job's branch on, decided once by [`holds`] over a follower's facts and found by
//! [`branch_carrier`] (PUBLISHING-3, 32), and the walks along `follow_up_of` that make one line of
//! follow-ups ([`chain_tip`], [`follows_up`]); each job's chain, its root's [`Record`], and where
//! its next follow-up starts ([`start`], PUBLISHING-33 to 37). Only they write `chain_record` and
//! `carried`, and they read only their arguments, never the clock, the disk, a lock or the log.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use super::{Job, Status};
use crate::jobs::NewJob;

/// What a chain's root keeps in its file (`chainRecord`, never in the API) for when its jobs are
/// gone: its branch, its pull request's URL, where its clone was moved, its newest job.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Record {
    pub branch: String,
    pub made_branch: bool,
    pub pull_request: Option<String>,
    pub clone_root: Option<String>,
    pub tip: String,
}

/// What a chain's facts give its next follow-up: whether it carries the chain's branch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Line {
    /// A job of the chain has commits on the chain's branch.
    pub work: bool,
    /// The chain's pull request was read merged.
    pub merged: bool,
    /// The chain's pull request was read closed without merging.
    pub closed: bool,
}

/// Where a follow-up starts (PUBLISHING-33, 34, 27).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// Its base, on a branch of its own.
    Base,
    /// The chain's branch head, on a branch of its own: a new pull request.
    OwnBranch,
    /// The chain's branch itself: publishing updates the chain's pull request.
    Carry,
}

/// Where the next follow-up of a chain with these facts starts.
pub fn start(line: Line) -> Start {
    if !line.work || line.merged {
        Start::Base
    } else if line.closed {
        Start::OwnBranch
    } else {
        Start::Carry
    }
}

/// The id of `job`'s chain: its root's, or its own before it has one.
pub fn of(job: &Job) -> &str {
    job.chain.as_deref().unwrap_or(&job.id)
}

/// The jobs of chain `chain` that obid has, newest first (by `created`, then `id`).
fn jobs_of<'a>(jobs: &'a HashMap<String, Job>, chain: &str) -> Vec<&'a Job> {
    let mut of: Vec<&Job> = jobs.values().filter(|j| self::of(j) == chain).collect();
    of.sort_by(|a, b| (b.created, &b.id).cmp(&(a.created, &a.id)));
    of
}

/// Those of `jobs` on `branch`, letter case aside.
fn on_branch<'a>(jobs: &[&'a Job], branch: &str) -> impl Iterator<Item = &'a Job> {
    jobs.iter()
        .copied()
        .filter(move |j| same_branch(&j.branch, branch))
}

/// Chain `chain`'s record from its jobs and its root's kept one: branch the newest commits', else
/// kept, else the tip's; pull request the newest on it; moved clone kept, else the newest a job has.
pub fn refreshed(jobs: &HashMap<String, Job>, chain: &str) -> Record {
    let kept = jobs.get(chain).and_then(|root| root.chain_record.clone());
    let of = jobs_of(jobs, chain);
    let (branch, made_branch) = match (of.iter().find(|j| j.commits > Some(0)), &kept, of.first()) {
        (Some(j), ..) | (None, None, Some(j)) => (j.branch.clone(), j.made_branch),
        (None, Some(kept), _) => (kept.branch.clone(), kept.made_branch),
        (None, None, None) => Default::default(),
    };
    let pull_request = on_branch(&of, &branch).find_map(|j| j.published.clone());
    Record {
        pull_request,
        clone_root: kept
            .and_then(|kept| kept.clone_root)
            .or_else(|| of.iter().find_map(|j| j.clone_root.clone())),
        tip: of.first().map_or(chain, |j| &j.id).to_owned(),
        branch,
        made_branch,
    }
}

/// The facts [`start`] reads of chain `chain` along `record`'s branch, its pull request as obid
/// last read it on the newest job that holds it.
pub fn line(jobs: &HashMap<String, Job>, chain: &str, record: &Record) -> Line {
    let of = jobs_of(jobs, chain);
    let holds = |j: &&&Job| j.published.is_some() && j.published == record.pull_request;
    let read = of.iter().find(holds).and_then(|j| j.pull_request.as_ref());
    Line {
        work: on_branch(&of, &record.branch).any(|j| j.commits > Some(0)),
        merged: read.is_some_and(|pr| pr.state == "MERGED"),
        closed: read.is_some_and(|pr| pr.state == "CLOSED"),
    }
}

/// Where words to any job of a chain start their follow-up `next` ([`start`]): on its branch, it
/// follows up the newest job there, so their marks are its own (MERGING-14, 23).
pub fn place(jobs: &HashMap<String, Job>, job: &Job, next: &mut NewJob) {
    let record = refreshed(jobs, of(job));
    let start = start(line(jobs, of(job), &record));
    if start != Start::Base {
        let newest = on_branch(&jobs_of(jobs, of(job)), &record.branch).next();
        next.follow_up_of = newest.map(|j| j.id.clone()).or(next.follow_up_of.take());
        next.start_from = Some(record.branch.clone());
    }
    if start == Start::Carry {
        (next.branch, next.made_branch) = (Some(record.branch), record.made_branch);
    }
}

/// Gives new job `job` its chain: the job's it follows up (that one's id when it has none), else
/// its own, with the record of itself.
pub fn join(jobs: &HashMap<String, Job>, job: &mut Job) {
    let parent = job
        .follow_up_of
        .as_deref()
        .map(|p| jobs.get(p).map_or(p, of));
    job.chain = Some(parent.unwrap_or(&job.id).to_owned());
    if job.follow_up_of.is_none() {
        let alone = HashMap::from([(job.id.clone(), job.clone())]);
        job.chain_record = Some(refreshed(&alone, &job.id));
    }
}

/// Chain `chain`'s root, its record refreshed, when obid has it and the record changed.
pub fn renewed(jobs: &HashMap<String, Job>, chain: &str) -> Option<Job> {
    let mut root = jobs.get(chain).filter(|root| of(root) == root.id)?.clone();
    let kept = root.chain_record.replace(refreshed(jobs, chain));
    (kept != root.chain_record).then_some(root)
}

/// The owner's own roots of `repo`'s chains with unmerged work and no moved clone yet, worktree or
/// not, each with `root` (where obid just moved the clone) in its record (PUBLISHING-35).
pub fn moved(jobs: &HashMap<String, Job>, repo: &str, root: &str) -> Vec<Job> {
    let owners = |j: &&Job| j.repo == repo && !j.apart && j.obi.is_none() && of(j) == j.id;
    jobs.values()
        .filter(owners)
        .filter_map(|j| {
            let mut moved = j.clone();
            let record = moved.chain_record.insert(refreshed(jobs, &j.id));
            let line = line(jobs, &j.id, record);
            (record.clone_root.is_none() && line.work && !line.merged).then_some(())?;
            record.clone_root = Some(root.to_owned());
            Some(moved)
        })
        .collect()
}

/// Job `id`'s chain along `follow_up_of`: the first `chain` on the way, else the job where the walk
/// stops (none followed up, a parent obid doesn't have, or 64 steps along).
pub fn root_of(jobs: &HashMap<String, Job>, id: &str) -> String {
    let mut at = id;
    for _ in 0..64 {
        match jobs.get(at).map(|j| (&j.chain, &j.follow_up_of)) {
            Some((Some(chain), _)) => return chain.clone(),
            Some((None, Some(parent))) if jobs.contains_key(parent) => at = parent,
            _ => break,
        }
    }
    at.to_owned()
}

/// Gives each job loaded with no `chain` one ([`root_of`]) and each root its [`renewed`] record:
/// the ids whose file changes (PUBLISHING-37). Nothing else changes, and no job goes.
pub fn settle(jobs: &mut HashMap<String, Job>) -> Vec<String> {
    let unchained = jobs.values().filter(|j| j.chain.is_none());
    let found: Vec<(String, String)> = unchained
        .map(|j| (j.id.clone(), root_of(jobs, &j.id)))
        .collect();
    let mut changed: BTreeSet<String> = BTreeSet::new();
    for (id, root) in found {
        jobs.entry(id.clone()).and_modify(|j| j.chain = Some(root));
        changed.insert(id);
    }
    let roots: BTreeSet<String> = jobs.values().map(|j| of(j).to_owned()).collect();
    for root in roots {
        if let Some(renewed) = renewed(jobs, &root) {
            jobs.insert(root.clone(), renewed);
            changed.insert(root);
        }
    }
    changed.into_iter().collect()
}

/// Sets `carried` on job `id` and each job it follows up, as [`branch_carrier`] finds a follow-up
/// at work or done on its branch, Publish's own rule (PUBLISHING-36): the ids whose flag flipped.
pub fn mark_carried(jobs: &mut HashMap<String, Job>, id: &str) -> Vec<String> {
    let (mut flipped, mut at) = (Vec::new(), Some(id.to_owned()));
    // A chain is short; this only stops a loop.
    for _ in 0..64 {
        let Some(job) = at.as_deref().and_then(|id| jobs.get(id)) else {
            break;
        };
        let carried = branch_carrier(jobs, job, Holds::AtWorkOrDone).is_some();
        let id = job.id.clone();
        at = job.follow_up_of.clone();
        if let Some(job) = jobs.get_mut(&id).filter(|j| j.carried != carried) {
            job.carried = carried;
            flipped.push(id);
        }
    }
    flipped
}

/// Each job's `carried` from `from` onto the same job in `to`, which a serde round trip
/// (`push::for_alerts`' scrub) left without it.
pub fn carried_onto(from: &[Job], to: &mut [Job]) {
    for job in to {
        job.carried = from.iter().any(|f| f.carried && f.id == job.id);
    }
}

/// Which follow-ups hold a job's branch, as each place that asks needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holds {
    /// Any, whatever its status: Try again (`message_as`), `reviewed`, Delete (`undeletable_in`),
    /// `apply_limit_job` and `may_round`.
    Any,
    /// Queued or running: `review_round_again` and `review_left_to_owner`.
    AtWork,
    /// Not let go, and queued, running or done: Publish (`publish_action_of`).
    AtWorkOrDone,
    /// Queued or running, needing the owner, or its pull request open: `done_with` (cleanup).
    Open,
}

/// What [`holds`] reads of a follow-up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Follower {
    /// Queued or running.
    pub active: bool,
    /// Ended done.
    pub done: bool,
    /// Let go.
    pub dismissed: bool,
    /// Waits on the owner ([`Job::needs_you`]).
    pub needs_you: bool,
    /// Its pull request open ([`pull_request_open`]).
    pub pr_open: bool,
}

impl Follower {
    /// The facts of the follow-up `job`.
    pub fn of(job: &Job) -> Self {
        Self {
            active: job.status.is_active(),
            done: job.status == Status::Done,
            dismissed: job.dismissed.is_some(),
            needs_you: job.needs_you(),
            pr_open: pull_request_open(job),
        }
    }
}

/// Whether a follow-up with these facts holds its job's branch, as `holds` asks.
pub fn holds(holds: Holds, follower: Follower) -> bool {
    match holds {
        Holds::Any => true,
        Holds::AtWork => follower.active,
        Holds::AtWorkOrDone => !follower.dismissed && (follower.active || follower.done),
        Holds::Open => follower.active || follower.needs_you || follower.pr_open,
    }
}

/// The follow-up of `job`, at any depth ([`follows_up`]), of every job obid has (`jobs`), that
/// carries its branch on: on that branch, letter case aside ([`same_branch`]), and held as
/// `holds` says, the newest of them. Never `job` itself.
pub fn branch_carrier<'a>(
    jobs: &'a HashMap<String, Job>,
    job: &Job,
    holds: Holds,
) -> Option<&'a Job> {
    jobs.values()
        .filter(|j| {
            j.id != job.id
                && same_branch(&j.branch, &job.branch)
                && follows_up(jobs, j, &job.id)
                && self::holds(holds, Follower::of(j))
        })
        .max_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)))
}

/// Two branch names that are one ref in obid's clone: a Mac's disk ignores letter case, so
/// `oby/Fix-34` is `oby/fix-34` there.
pub(crate) fn same_branch(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Published, and its pull request open as obid last read it (or never read).
pub(crate) fn pull_request_open(job: &Job) -> bool {
    job.published.is_some()
        && job
            .pull_request
            .as_ref()
            .is_none_or(|pr| pr.state != "MERGED" && pr.state != "CLOSED")
}

/// Whether `job` follows up `of`, at any depth, or is it.
pub(crate) fn follows_up(jobs: &HashMap<String, Job>, job: &Job, of: &str) -> bool {
    let mut at = Some(job.id.as_str());
    // ponytail: a chain longer than 64 follow-ups counts as not following up.
    for _ in 0..64 {
        match at {
            Some(id) if id == of => return true,
            Some(id) => {
                at = jobs
                    .get(id)
                    .or((id == job.id).then_some(job))
                    .and_then(|j| j.follow_up_of.as_deref())
            }
            None => return false,
        }
    }
    false
}

/// The job a new follow-up of `of` follows up: the newest of the owner's own jobs on its branch
/// that follows `of` up, at any depth, or `of` itself. Its branch holds their commits, so their
/// marks are the follow-up's (MERGING-14, 23): follow-ups make one line, never siblings.
pub(crate) fn chain_tip(jobs: &HashMap<String, Job>, of: &str) -> String {
    let Some(first) = jobs.get(of) else {
        return of.to_owned();
    };
    jobs.values()
        .filter(|j| {
            j.obi.is_none()
                && j.repo == first.repo
                && same_branch(&j.branch, &first.branch)
                && follows_up(jobs, j, of)
        })
        .max_by(|a, b| a.created.cmp(&b.created).then_with(|| a.id.cmp(&b.id)))
        .map_or_else(|| of.to_owned(), |j| j.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each case holds for the facts it names and for no other (PUBLISHING-32), and the follow-up
    /// that carries a branch is found on it in any letter case, never on another branch, of
    /// another job, or as the job itself.
    #[test]
    fn chain_carrier_cases() {
        let none = Follower::default();
        let active = Follower {
            active: true,
            ..none
        };
        let done = Follower { done: true, ..none };
        let dismissed_done = Follower {
            dismissed: true,
            ..done
        };
        let dismissed_active = Follower {
            dismissed: true,
            ..active
        };
        let needs_you = Follower {
            needs_you: true,
            ..none
        };
        let pr_open = Follower {
            pr_open: true,
            ..none
        };
        let cases = [
            (none, [true, false, false, false]),
            (active, [true, true, true, true]),
            (done, [true, false, true, false]),
            (dismissed_done, [true, false, false, false]),
            (dismissed_active, [true, true, false, true]),
            (needs_you, [true, false, false, true]),
            (pr_open, [true, false, false, true]),
        ];
        for (follower, held) in cases {
            for (case, held) in [Holds::Any, Holds::AtWork, Holds::AtWorkOrDone, Holds::Open]
                .into_iter()
                .zip(held)
            {
                assert_eq!(holds(case, follower), held, "{case:?} of {follower:?}");
            }
        }

        let job = |id: &str, of: Option<&str>, branch: &str, status: Status| Job {
            id: id.into(),
            follow_up_of: of.map(str::to_owned),
            branch: branch.into(),
            status,
            ..Job::default()
        };
        let map = |jobs: Vec<Job>| -> HashMap<String, Job> {
            jobs.into_iter().map(|j| (j.id.clone(), j)).collect()
        };
        let first = job("a", None, "oby/Fix-34", Status::Done);
        let jobs = map(vec![
            first.clone(),
            job("b", Some("a"), "oby/fix-34", Status::Running),
        ]);
        for case in [Holds::Any, Holds::AtWork, Holds::AtWorkOrDone, Holds::Open] {
            assert_eq!(
                branch_carrier(&jobs, &first, case).map(|j| j.id.as_str()),
                Some("b")
            );
        }
        // Another branch, or a follow-up of another job: none carries it.
        let jobs = map(vec![
            first.clone(),
            job("b", Some("a"), "oby/fix-35", Status::Running),
            job("c", Some("x"), "oby/fix-34", Status::Running),
        ]);
        assert!(branch_carrier(&jobs, &first, Holds::Any).is_none());
        // A job never carries its own branch, even one whose record says it follows itself up.
        let own = job("a", Some("a"), "oby/fix-34", Status::Running);
        let jobs = map(vec![own.clone()]);
        assert!(branch_carrier(&jobs, &own, Holds::Any).is_none());
        // A follow-up that ended otherwise holds it only for the cases that take any status.
        let jobs = map(vec![
            first.clone(),
            job("b", Some("a"), "OBY/FIX-34", Status::Stopped),
        ]);
        assert!(branch_carrier(&jobs, &first, Holds::Any).is_some());
        assert!(branch_carrier(&jobs, &first, Holds::AtWork).is_none());
        assert!(branch_carrier(&jobs, &first, Holds::AtWorkOrDone).is_none());
        assert!(branch_carrier(&jobs, &first, Holds::Open).is_none());
    }

    const URL: &str = "https://github.com/obi-e2e/fixture-app/pull/7";

    /// Job `id`, made at `created`, following up `of` on `branch`, ended `status`.
    fn made(id: &str, created: i64, of: Option<&str>, branch: &str, status: Status) -> Job {
        Job {
            id: id.into(),
            created,
            repo: "obi-e2e/fixture-app".into(),
            follow_up_of: of.map(str::to_owned),
            branch: branch.into(),
            status,
            ..Job::default()
        }
    }

    /// `jobs`, oldest first, each kept in its chain as obid keeps a new job ([`join`]).
    fn chained(jobs: Vec<Job>) -> HashMap<String, Job> {
        let mut map = HashMap::new();
        for mut job in jobs {
            join(&map, &mut job);
            map.insert(job.id.clone(), job);
        }
        map
    }

    /// A pull request obid last read in `state`.
    fn read(state: &str) -> Option<crate::github::PullRequest> {
        Some(crate::github::PullRequest {
            state: state.into(),
            review_decision: None,
            base: "main".into(),
            merged_at: None,
            checked_at: 0,
            head: None,
        })
    }

    /// Where the owner's words to `to` start their follow-up: its branch, the one it starts from
    /// and the job it follows up.
    fn placed(jobs: &HashMap<String, Job>, to: &str) -> (Option<String>, Option<String>, String) {
        let mut next = NewJob {
            follow_up_of: Some(to.into()),
            ..NewJob::default()
        };
        place(jobs, &jobs[to], &mut next);
        (next.branch, next.start_from, next.follow_up_of.unwrap())
    }

    /// PUBLISHING-33, 34, 27: each of the 8 lines gives its start, and only these.
    #[test]
    fn publishing_33_each_line_gives_its_start() {
        let cases = [
            ((false, false, false), Start::Base),
            ((false, false, true), Start::Base),
            ((false, true, false), Start::Base),
            ((false, true, true), Start::Base),
            ((true, false, false), Start::Carry),
            ((true, false, true), Start::OwnBranch),
            ((true, true, false), Start::Base),
            ((true, true, true), Start::Base),
        ];
        for ((work, merged, closed), started) in cases {
            let line = Line {
                work,
                merged,
                closed,
            };
            assert_eq!(start(line), started, "{line:?}");
        }
    }

    /// PUBLISHING-33 (#1859): a follow-up that failed with no commits keeps the line. The chain's
    /// branch and pull request are its task's, and the owner's words to the failed one carry that
    /// branch on, following it up.
    #[test]
    fn publishing_33_a_failed_follow_up_keeps_the_chains_branch() {
        let mut task = made("a", 1, None, "oby/build-a", Status::Done);
        (task.commits, task.published, task.pull_request) =
            (Some(1), Some(URL.into()), read("OPEN"));
        let jobs = chained(vec![
            task,
            made("b", 2, Some("a"), "oby/build-a", Status::Failed),
        ]);
        let record = refreshed(&jobs, "a");
        assert_eq!(record.branch, "oby/build-a");
        assert_eq!(record.pull_request.as_deref(), Some(URL));
        assert_eq!(record.tip, "b");
        let facts = Line {
            work: true,
            merged: false,
            closed: false,
        };
        assert_eq!(line(&jobs, "a", &record), facts);
        let branch = Some("oby/build-a".to_owned());
        assert_eq!(
            placed(&jobs, "b"),
            (branch.clone(), branch.clone(), "b".into())
        );
        // Words to the task itself give the same follow-up.
        assert_eq!(placed(&jobs, "a"), (branch.clone(), branch, "b".into()));
    }

    /// PUBLISHING-34 (#1861): an unpublished follow-up with commits, stopped, keeps its branch:
    /// with no pull request at all, the line is still open.
    #[test]
    fn publishing_34_an_unpublished_line_is_carried() {
        let mut task = made("a", 1, None, "oby/build-a", Status::Done);
        task.commits = Some(1);
        let mut carrier = made("b", 2, Some("a"), "oby/build-a", Status::Stopped);
        carrier.commits = Some(2);
        let jobs = chained(vec![task, carrier]);
        let record = refreshed(&jobs, "a");
        assert_eq!(record.pull_request, None);
        assert_eq!(start(line(&jobs, "a", &record)), Start::Carry);
        let branch = Some("oby/build-a".to_owned());
        assert_eq!(placed(&jobs, "b"), (branch.clone(), branch, "b".into()));
    }

    /// PUBLISHING-27: a chain whose pull request was read merged starts its follow-up from its
    /// base; one read closed, from its branch's head on a branch of its own. A chain with no
    /// commits starts from its base.
    #[test]
    fn publishing_27_a_merged_or_closed_line_isnt_carried() {
        for (state, started) in [("MERGED", Start::Base), ("CLOSED", Start::OwnBranch)] {
            let mut task = made("a", 1, None, "oby/build-a", Status::Done);
            (task.commits, task.published, task.pull_request) =
                (Some(1), Some(URL.into()), read(state));
            let jobs = chained(vec![
                task,
                made("b", 2, Some("a"), "oby/build-a", Status::Done),
            ]);
            let record = refreshed(&jobs, "a");
            let facts = line(&jobs, "a", &record);
            assert_eq!(
                (facts.merged, facts.closed),
                (state == "MERGED", state == "CLOSED")
            );
            assert_eq!(start(facts), started, "{state}");
            // From the branch's head, it follows up the newest job there; from its base, as told.
            let from = (started == Start::OwnBranch).then(|| "oby/build-a".to_owned());
            let of = if started == Start::Base { "a" } else { "b" };
            assert_eq!(placed(&jobs, "a"), (None, from, of.into()));
        }
        let jobs = chained(vec![made("a", 1, None, "oby/build-a", Status::Done)]);
        assert_eq!(start(line(&jobs, "a", &refreshed(&jobs, "a"))), Start::Base);
        assert_eq!(placed(&jobs, "a"), (None, None, "a".into()));
    }

    /// The kept record gives the branch once the jobs that committed on it are gone, and its moved
    /// clone stays (PUBLISHING-35).
    #[test]
    fn a_kept_record_outlives_the_jobs_that_made_it() {
        let mut root = made("a", 1, None, "oby/build-x", Status::Done);
        root.chain = Some("a".into());
        root.chain_record = Some(Record {
            branch: "oby/build-a".into(),
            made_branch: true,
            pull_request: None,
            clone_root: Some("/marked/1".into()),
            tip: "a".into(),
        });
        let jobs = HashMap::from([("a".to_owned(), root)]);
        let record = refreshed(&jobs, "a");
        assert_eq!(
            (record.branch.as_str(), record.made_branch),
            ("oby/build-a", true)
        );
        assert_eq!(record.clone_root.as_deref(), Some("/marked/1"));
    }

    /// PUBLISHING-15: a follow-up finds the clone its task's was moved to through its chain, past
    /// one between them that failed before it had a worktree; and a chain with work and no worktree
    /// left is given the clone obid moves aside (PUBLISHING-35), one merged is not.
    #[test]
    fn publishing_15_a_follow_up_finds_a_moved_clone_along_its_chain() {
        let mut task = made("task", 1, None, "oby/fix-task", Status::Done);
        task.clone_root = Some("/marked/1".into());
        let jobs = chained(vec![
            task,
            made("failed", 2, Some("task"), "oby/fix-task", Status::Failed),
            made("next", 3, Some("failed"), "oby/fix-task", Status::Queued),
        ]);
        for id in ["next", "failed", "task"] {
            let moved = refreshed(&jobs, of(&jobs[id])).clone_root;
            assert_eq!(moved.as_deref(), Some("/marked/1"), "{id}");
        }
        // Moved before it was told: its worktree was gone.
        let mut task = made("task", 1, None, "oby/fix-task", Status::Done);
        task.commits = Some(1);
        let mut merged = made("old", 0, None, "oby/fix-old", Status::Done);
        (merged.commits, merged.published, merged.pull_request) =
            (Some(1), Some(URL.into()), read("MERGED"));
        let mut jobs = chained(vec![merged, task]);
        let roots = moved(&jobs, "obi-e2e/fixture-app", "/marked/2");
        assert_eq!(
            roots.iter().map(|j| j.id.as_str()).collect::<Vec<_>>(),
            ["task"]
        );
        jobs.extend(roots.into_iter().map(|j| (j.id.clone(), j)));
        let mut next = made("next", 2, Some("task"), "oby/fix-task", Status::Queued);
        join(&jobs, &mut next);
        let moved = refreshed(&jobs, of(&next)).clone_root;
        assert_eq!(moved.as_deref(), Some("/marked/2"));
        // Told once: a later move doesn't take it elsewhere.
        assert!(super::moved(&jobs, "obi-e2e/fixture-app", "/marked/3").is_empty());
    }

    /// The jobs of one fixture shape (tests/fixtures/chains/<shape>), as obid loads them.
    fn shape(name: &str) -> HashMap<String, Job> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chains");
        let files = std::fs::read_dir(dir.join(name)).unwrap();
        let jobs = files.flatten().map(|file| {
            let job: Job = serde_json::from_slice(&std::fs::read(file.path()).unwrap()).unwrap();
            (job.id.clone(), job)
        });
        jobs.collect()
    }

    /// PUBLISHING-37: old job files get their chain from their links, in each shape (a chain of
    /// three, a lone job, a follow-up whose parent's file is gone), and each root its record; no
    /// branch changes and no job goes. A tip a crash left stale (the follow-up kept, its root not)
    /// is put right at the next start, and nothing else is saved again.
    #[test]
    fn publishing_37_old_files_get_their_chain_from_their_links() {
        let shapes = [
            (
                "three",
                vec![
                    ("c0000001", "c0000001"),
                    ("c0000002", "c0000001"),
                    ("c0000003", "c0000001"),
                ],
            ),
            ("lone", vec![("a0000001", "a0000001")]),
            ("orphan", vec![("b0000002", "b0000002")]),
        ];
        for (name, chains) in shapes {
            let loaded = shape(name);
            let mut jobs = loaded.clone();
            let changed = settle(&mut jobs);
            assert_eq!(changed.len(), chains.len(), "{name}");
            assert_eq!(jobs.len(), loaded.len());
            for (id, chain) in chains {
                assert_eq!(root_of(&loaded, id), chain, "{name} {id}");
                assert_eq!(jobs[id].chain.as_deref(), Some(chain), "{name} {id}");
                assert_eq!(jobs[id].branch, loaded[id].branch);
                let root = id == chain;
                assert_eq!(jobs[id].chain_record.is_some(), root, "{name} {id}");
                assert_eq!(
                    jobs[id],
                    Job {
                        chain: jobs[id].chain.clone(),
                        chain_record: jobs[id].chain_record.clone(),
                        ..loaded[id].clone()
                    }
                );
            }
            assert!(settle(&mut jobs).is_empty(), "{name}: settled once");
        }
        let mut jobs = shape("three");
        settle(&mut jobs);
        assert_eq!(
            jobs["c0000001"].chain_record.as_ref().unwrap().tip,
            "c0000003"
        );
        let mut late = made(
            "c0000004",
            1794000500,
            Some("c0000003"),
            "oby/build-c0000001",
            Status::Done,
        );
        join(&jobs, &mut late);
        jobs.insert(late.id.clone(), late);
        assert_eq!(settle(&mut jobs), ["c0000001"]);
        assert_eq!(
            jobs["c0000001"].chain_record.as_ref().unwrap().tip,
            "c0000004"
        );
    }

    /// PUBLISHING-36: a follow-up at work two follow-ups down, past one that failed, carries the
    /// first job's branch on, and holds Publish there; marking it flips the first job's
    /// `carried`, and its `canPublish` with it, and back once it's let go.
    #[test]
    fn publishing_36_a_carrier_past_a_failed_follow_up_marks_the_first_job() {
        let mut first = made("a", 1, None, "oby/build-a", Status::Done);
        (first.base, first.commits, first.checked_head) =
            (Some("main".into()), Some(1), Some("c".repeat(40)));
        let mut jobs = chained(vec![
            first,
            made("b", 2, Some("a"), "oby/build-a", Status::Failed),
            made("c", 3, Some("b"), "OBY/build-a", Status::Running),
        ]);
        let carrier = branch_carrier(&jobs, &jobs["a"], Holds::AtWorkOrDone);
        assert_eq!(carrier.map(|j| j.id.as_str()), Some("c"));
        assert!(jobs["a"].can_publish());
        // The failed one between them is carried too, from the newest up.
        assert_eq!(mark_carried(&mut jobs, "c"), ["b", "a"]);
        assert!(jobs["a"].carried && jobs["b"].carried && !jobs["c"].carried);
        assert!(!jobs["a"].can_publish() && jobs["a"].publishable());
        assert!(jobs["a"].can_publish_anyway());
        assert!(mark_carried(&mut jobs, "c").is_empty());
        jobs.get_mut("c").unwrap().dismissed = Some(4);
        jobs.get_mut("c").unwrap().status = Status::Stopped;
        assert_eq!(mark_carried(&mut jobs, "c"), ["b", "a"]);
        assert!(jobs["a"].can_publish());
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of where a follow-up starts ([`start`]): `kani::any()` stands for every value
    //! of a chain's three facts.

    use super::*;

    /// Any line: every work, merged and closed.
    fn any_line() -> Line {
        Line {
            work: kani::any(),
            merged: kani::any(),
            closed: kani::any(),
        }
    }

    /// PROOF-1 (PUBLISHING-33): a follow-up carries the chain's branch exactly when the chain has
    /// work on it and its pull request was neither merged nor closed, published or not, whatever
    /// the job before it did.
    #[kani::proof]
    fn follow_up_carries_exactly_an_open_line() {
        let line = any_line();
        assert_eq!(
            start(line) == Start::Carry,
            line.work && !line.merged && !line.closed
        );
    }

    /// PROOF-2 (PUBLISHING-27): a follow-up starts from its base whenever the chain has no work or
    /// its pull request was merged, and on a branch of its own from the chain's head exactly when
    /// the work's pull request was closed without merging.
    #[kani::proof]
    fn follow_up_leaves_a_closed_or_merged_line() {
        let line = any_line();
        let start = start(line);
        assert_eq!(start == Start::Base, !line.work || line.merged);
        assert_eq!(
            start == Start::OwnBranch,
            line.work && !line.merged && line.closed
        );
    }
}
