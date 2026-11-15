//! What a restart of obid would cut now: `GET /v1/status`'s `restart`, which the app waits on
//! before it restarts an obid older than its helper (docs/contracts/engine.md, ENGINE-13).

use serde::Serialize;

use crate::api::Shared;

/// How much of each kind of work is in flight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InFlight {
    pub jobs: u32,
    pub checks: u32,
    pub asks: u32,
    pub publishes: u32,
    pub merges: u32,
    pub turns: u32,
    pub words: u32,
    pub routines: u32,
}

/// A restart loses nothing only with nothing in flight: no kind is exempt, so one added later
/// holds a restart too (ENGINE-13).
pub fn ready(work: InFlight) -> bool {
    work == InFlight::default()
}

/// A kind of work a restart would cut, in the order `waitingFor` lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A job its worker runs.
    Job,
    /// A job waiting on the check queue, or checking.
    Check,
    /// A job's prompt waiting for the owner.
    Ask,
    /// A job publishing.
    Publish,
    /// A job being merged.
    Merge,
    /// A conversation turn under way.
    Turn,
    /// The owner's words no turn has taken yet.
    Words,
    /// A routine due within 10 minutes.
    Routine,
}

/// `status.restart.waitingFor[]`: the work, and its job, turn or routine id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Waiting {
    pub kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl Waiting {
    pub fn new(kind: Kind, id: &str) -> Self {
        Waiting {
            kind,
            id: Some(id.to_owned()),
        }
    }
}

/// `status.restart`: `ready` only when `waitingFor` is empty.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartJson {
    pub ready: bool,
    pub waiting_for: Vec<Waiting>,
}

/// Whether a restart is ready with `waiting` in flight, the list in kind then id order.
pub fn decide(mut waiting: Vec<Waiting>) -> RestartJson {
    waiting.sort();
    let mut work = InFlight::default();
    for w in &waiting {
        let count = match w.kind {
            Kind::Job => &mut work.jobs,
            Kind::Check => &mut work.checks,
            Kind::Ask => &mut work.asks,
            Kind::Publish => &mut work.publishes,
            Kind::Merge => &mut work.merges,
            Kind::Turn => &mut work.turns,
            Kind::Words => &mut work.words,
            Kind::Routine => &mut work.routines,
        };
        *count = count.saturating_add(1);
    }
    RestartJson {
        ready: ready(work),
        waiting_for: waiting,
    }
}

/// Whether a routine whose next run is `next_run` runs within 10 minutes of `now`, or is past due.
pub fn routine_soon(next_run: Option<i64>, now: i64) -> bool {
    next_run.is_some_and(|t| t <= now.saturating_add(600))
}

/// What a restart would cut now, read from obid's own state.
pub fn status(shared: &Shared, now: i64) -> RestartJson {
    let mut waiting = shared.jobs.in_flight();
    let merges = crate::merge::merging_jobs(&shared.config.home);
    waiting.extend(merges.iter().map(|id| Waiting::new(Kind::Merge, id)));
    let (turns, words) = shared.turns.in_flight();
    waiting.extend(turns.iter().map(|id| Waiting::new(Kind::Turn, id)));
    waiting.extend(words.iter().map(|id| Waiting::new(Kind::Words, id)));
    let routines = shared.routines.list().into_iter();
    let soon = routines.filter(|r| routine_soon(r.next_run(), now));
    waiting.extend(soon.map(|r| Waiting::new(Kind::Routine, &r.id)));
    decide(waiting)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_13_ready_only_with_nothing_in_flight() {
        let none = InFlight::default();
        assert!(ready(none));
        assert!(!ready(InFlight { jobs: 1, ..none }));
        assert!(!ready(InFlight { checks: 1, ..none }));
        assert!(!ready(InFlight { asks: 1, ..none }));
        assert!(!ready(InFlight {
            publishes: 1,
            ..none
        }));
        assert!(!ready(InFlight { merges: 1, ..none }));
        assert!(!ready(InFlight { turns: 1, ..none }));
        assert!(!ready(InFlight { words: 1, ..none }));
        assert!(!ready(InFlight {
            routines: 1,
            ..none
        }));
    }

    #[test]
    fn engine_13_a_routine_counts_from_10_minutes_before() {
        let now = 1_000_000;
        assert!(!routine_soon(None, now));
        assert!(routine_soon(Some(now + 600), now));
        assert!(!routine_soon(Some(now + 601), now));
        assert!(routine_soon(Some(now - 3600), now));
        assert!(routine_soon(Some(i64::MAX), i64::MAX));
        assert!(!routine_soon(Some(i64::MAX), i64::MAX - 601));
    }

    #[test]
    fn engine_13_decide_orders_tallies_and_serializes() {
        let decided = decide(vec![
            Waiting::new(Kind::Routine, "r1"),
            Waiting::new(Kind::Turn, "t2"),
            Waiting::new(Kind::Job, "j1"),
            Waiting::new(Kind::Turn, "t1"),
        ]);
        assert!(!decided.ready);
        let order: Vec<_> = decided
            .waiting_for
            .iter()
            .map(|w| w.id.as_deref())
            .collect();
        assert_eq!(order, [Some("j1"), Some("t1"), Some("t2"), Some("r1")]);

        let one = decide(vec![Waiting::new(Kind::Turn, "t1")]);
        assert_eq!(
            serde_json::to_string(&one).unwrap(),
            r#"{"ready":false,"waitingFor":[{"kind":"turn","id":"t1"}]}"#
        );
        let none = decide(Vec::new());
        assert_eq!(
            serde_json::to_string(&none).unwrap(),
            r#"{"ready":true,"waitingFor":[]}"#
        );
        let bare = decide(vec![Waiting {
            kind: Kind::Words,
            id: None,
        }]);
        assert_eq!(
            serde_json::to_string(&bare).unwrap(),
            r#"{"ready":false,"waitingFor":[{"kind":"words"}]}"#
        );
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proof of [`ready`]: `kani::any()` stands for every count of every kind.

    use super::*;

    /// ENGINE-13: a restart is ready exactly when every count is zero, so a running conversation
    /// turn holds it even with no job running.
    #[kani::proof]
    fn engine_13_a_restart_is_ready_only_with_nothing_in_flight() {
        let w = InFlight {
            jobs: kani::any(),
            checks: kani::any(),
            asks: kani::any(),
            publishes: kani::any(),
            merges: kani::any(),
            turns: kani::any(),
            words: kani::any(),
            routines: kani::any(),
        };
        let none = w.jobs == 0
            && w.checks == 0
            && w.asks == 0
            && w.publishes == 0
            && w.merges == 0
            && w.turns == 0
            && w.words == 0
            && w.routines == 0;
        assert_eq!(ready(w), none);
        if w.turns > 0 {
            assert!(!ready(w));
        }
    }
}
