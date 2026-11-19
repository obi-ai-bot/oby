//! A job's task chain (docs/ARCHITECTURE.md, "State ownership"): whether a follow-up carries a
//! job's branch on, decided once by [`holds`] over a follower's facts and found by
//! [`branch_carrier`] (PUBLISHING-3, 32), and the walks along `follow_up_of` that make one line of
//! follow-ups ([`chain_tip`], [`follows_up`]), and whether a task's card settles while it carries
//! on ([`card_settles`], TASKS-26 to 28). They read only their arguments, never the clock, the
//! disk, a lock or the log.

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

/// Which of obid's cards an inbox item is, as [`card_settles`] asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardKind {
    /// obid's crash or review card (`inbox::is_crash_item`, `inbox::is_review_item`).
    Failed,
    /// A `merge` card.
    Merge,
    /// Anything else: questions, permissions, limits, confirms, offers, GitHub's, another Obi's.
    Other,
}

/// What [`card_settles`] reads of an open card and its task: a job and the owner's own jobs that
/// follow it up, at any depth ([`follows_up`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainCard {
    /// Which card it is.
    pub kind: CardKind,
    /// About one of the owner's own jobs obid has (not another Obi's).
    pub own: bool,
    /// Its job, or a later job of its task, is queued or running.
    pub at_work: bool,
    /// A later job of its task has an open card of the same kind.
    pub later_card: bool,
}

/// Whether the card settles for its task (TASKS-26 to 28): obid's crash, review or merge card
/// about the owner's own job, while its task carries on or a later job of it has one of its own.
pub fn card_settles(card: ChainCard) -> bool {
    card.own && card.kind != CardKind::Other && (card.at_work || card.later_card)
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

    /// TASKS-26 to 28: every one of the 24 cards settles exactly when it's obid's crash, review or
    /// merge card about the owner's own job, and its task is at work or has a later card.
    #[test]
    fn tasks_26_to_28_card_settles_cases() {
        for kind in [CardKind::Failed, CardKind::Merge, CardKind::Other] {
            for bits in 0..8u8 {
                let card = ChainCard {
                    kind,
                    own: bits & 1 != 0,
                    at_work: bits & 2 != 0,
                    later_card: bits & 4 != 0,
                };
                let settles = kind != CardKind::Other && bits & 1 != 0 && bits & 6 != 0;
                assert_eq!(card_settles(card), settles, "{card:?}");
            }
        }
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of a task's cards (TASKS-26 to 28): `kani::any()` stands for every value of
    //! the enum and bools of [`ChainCard`].

    use super::*;

    /// Any card, from a number and three bools.
    fn any_card() -> ChainCard {
        ChainCard {
            kind: match kani::any::<u8>() {
                0 => CardKind::Failed,
                1 => CardKind::Merge,
                _ => CardKind::Other,
            },
            own: kani::any(),
            at_work: kani::any(),
            later_card: kani::any(),
        }
    }

    /// TASKS-26: obid's crash, review or merge card about the owner's own job settles whenever
    /// its job or a later job of its task is queued or running.
    #[kani::proof]
    fn tasks_26_a_card_settles_while_its_task_is_at_work() {
        let card = any_card();
        if card.own && card.kind != CardKind::Other && card.at_work {
            assert!(card_settles(card));
        }
    }

    /// TASKS-27: such a card settles when a later job of its task has an open card of the same
    /// kind, and one with nothing of its task at work and no later card stays open.
    #[kani::proof]
    fn tasks_27_only_the_newest_card_of_a_task_stays() {
        let card = any_card();
        if card.own && card.kind != CardKind::Other {
            assert_eq!(card_settles(card), card.at_work || card.later_card);
        }
    }

    /// TASKS-28: a card that isn't obid's crash, review or merge card, or is about a job that
    /// isn't the owner's own, never settles for its task, whatever the task does.
    #[kani::proof]
    fn tasks_28_a_card_that_needs_the_owner_never_settles_for_its_task() {
        let card = any_card();
        if card.kind == CardKind::Other || !card.own {
            assert!(!card_settles(card));
        }
    }
}
