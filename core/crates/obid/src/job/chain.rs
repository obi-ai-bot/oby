//! A job's task chain (docs/ARCHITECTURE.md, "State ownership"): whether a follow-up carries a
//! job's branch on, decided once by [`holds`] over a follower's facts and found by
//! [`branch_carrier`] (PUBLISHING-3, 32), and the walks along `follow_up_of` that make one line of
//! follow-ups ([`chain_tip`], [`follows_up`]). They read only their arguments, never the clock,
//! the disk, a lock or the log.

use std::collections::HashMap;

use super::{Job, Status};

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

/// The follow-up of `job`, of every job obid has (`jobs`), that carries its branch on: on that
/// branch, letter case aside ([`same_branch`]), and held as `holds` says. Never `job` itself.
pub fn branch_carrier<'a>(
    jobs: &'a HashMap<String, Job>,
    job: &Job,
    holds: Holds,
) -> Option<&'a Job> {
    jobs.values().find(|j| {
        j.id != job.id
            && j.follow_up_of.as_deref() == Some(job.id.as_str())
            && same_branch(&j.branch, &job.branch)
            && self::holds(holds, Follower::of(j))
    })
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
}
