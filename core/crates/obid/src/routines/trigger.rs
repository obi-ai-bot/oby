//! What one GitHub event does for a routine on GitHub events (docs/contracts/routines.md,
//! ROUTINES-9 to 14): start its job, ignore it or hold it, from a few facts about it. Pure, so
//! Kani proves it: the receipts, the jobs and the clock are routines.rs's.

use serde::{Deserialize, Serialize};

/// The most jobs one routine starts on one item in 24 hours (ROUTINES-12).
pub const STARTS_A_DAY: u32 = 3;

/// What came of an event, as its receipt keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Started,
    Ignored,
    Held,
    Failed,
}

/// Why an event started nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    OwnPush,
    NoMarker,
    OpenJob,
    DayLimit,
}

impl Why {
    /// obid's words for it, a receipt's `reason`.
    pub fn reason(self) -> &'static str {
        match self {
            Why::OwnPush => "Oby's own push or pull request caused it.",
            Why::NoMarker => "GitHub didn't say which commit it's on.",
            Why::OpenJob => "This routine's job on it is still open.",
            Why::DayLimit => "This routine started 3 jobs on it in the last 24 hours.",
        }
    }
}

/// What to do with an event: nothing (its receipt says it was dealt with), start its job, or keep
/// a receipt saying why not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Done,
    Start,
    Ignore(Why),
    Hold(Why),
}

/// What [`decide`] reads of one event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    /// The routine's receipt for this event's key, if it has one.
    pub receipt: Option<Outcome>,
    /// Oby caused the event: a head an obid job pushed, or a pull request an obid job opened.
    pub by_oby: bool,
    /// The event has its marker (a `head` where the event needs one).
    pub marker: bool,
    /// One of this routine's receipts on the same repository and number has a queued or running job.
    pub open_job: bool,
    /// This routine's `started` receipts on the same repository and number in the last 24 hours.
    pub starts_today: u32,
}

/// What one event does: a key dealt with is done; Oby's own and markerless events are ignored; one
/// behind an open job or past the day's starts is held; any other starts its job.
pub fn decide(event: Facts) -> Decision {
    match event.receipt {
        None | Some(Outcome::Held) => {}
        Some(Outcome::Started | Outcome::Ignored | Outcome::Failed) => return Decision::Done,
    }
    if event.by_oby {
        return Decision::Ignore(Why::OwnPush);
    }
    if !event.marker {
        return Decision::Ignore(Why::NoMarker);
    }
    if event.open_job {
        return Decision::Hold(Why::OpenJob);
    }
    if event.starts_today >= STARTS_A_DAY {
        return Decision::Hold(Why::DayLimit);
    }
    Decision::Start
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEW: Facts = Facts {
        receipt: None,
        by_oby: false,
        marker: true,
        open_job: false,
        starts_today: 0,
    };

    #[test]
    fn routines_10_to_14_each_rule_decides() {
        // ROUTINES-9: a new event, nothing against it, starts; a held one starts once it's free.
        assert_eq!(decide(NEW), Decision::Start);
        let held = Facts {
            receipt: Some(Outcome::Held),
            starts_today: 2,
            ..NEW
        };
        assert_eq!(decide(held), Decision::Start);
        // ROUTINES-10: a key dealt with is done, whatever else holds.
        for outcome in [Outcome::Started, Outcome::Ignored, Outcome::Failed] {
            let dealt = Facts {
                receipt: Some(outcome),
                by_oby: true,
                marker: false,
                open_job: true,
                starts_today: 9,
            };
            assert_eq!(decide(dealt), Decision::Done, "{outcome:?}");
        }
        // ROUTINES-14 before the rest: Oby's own event is ignored, open job or not.
        let own = Facts {
            by_oby: true,
            open_job: true,
            ..NEW
        };
        assert_eq!(decide(own), Decision::Ignore(Why::OwnPush));
        let unmarked = Facts {
            marker: false,
            open_job: true,
            ..NEW
        };
        assert_eq!(decide(unmarked), Decision::Ignore(Why::NoMarker));
        // ROUTINES-11, then ROUTINES-12 at exactly 3.
        let open = Facts {
            open_job: true,
            starts_today: 3,
            ..NEW
        };
        assert_eq!(decide(open), Decision::Hold(Why::OpenJob));
        for starts_today in [3, 4, u32::MAX] {
            let full = Facts {
                starts_today,
                ..NEW
            };
            assert_eq!(decide(full), Decision::Hold(Why::DayLimit));
        }
        assert_eq!(
            Why::DayLimit.reason(),
            "This routine started 3 jobs on it in the last 24 hours."
        );
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of [`decide`]: `kani::any()` stands for every value of every fact.

    use super::*;

    /// Any receipt, or none.
    fn any_receipt() -> Option<Outcome> {
        match kani::any::<u8>() {
            0 => None,
            1 => Some(Outcome::Started),
            2 => Some(Outcome::Ignored),
            3 => Some(Outcome::Held),
            _ => Some(Outcome::Failed),
        }
    }

    /// Any facts at all.
    fn any_facts() -> Facts {
        Facts {
            receipt: any_receipt(),
            by_oby: kani::any(),
            marker: kani::any(),
            open_job: kani::any(),
            starts_today: kani::any(),
        }
    }

    /// ROUTINES-10: a key with a `started`, `ignored` or `failed` receipt is always done.
    #[kani::proof]
    fn routines_10_a_receipt_never_starts_twice() {
        let facts = any_facts();
        if matches!(
            facts.receipt,
            Some(Outcome::Started | Outcome::Ignored | Outcome::Failed)
        ) {
            assert_eq!(decide(facts), Decision::Done);
        }
    }

    /// ROUTINES-11: while the routine has an open job on the item, nothing starts.
    #[kani::proof]
    fn routines_11_an_open_job_holds() {
        let facts = any_facts();
        if facts.open_job {
            assert_ne!(decide(facts), Decision::Start);
        }
    }

    /// ROUTINES-12: with 3 or more starts on the item in 24 hours, nothing starts.
    #[kani::proof]
    fn routines_12_no_fourth_start_in_a_day() {
        assert_eq!(STARTS_A_DAY, 3);
        let facts = any_facts();
        if facts.starts_today >= STARTS_A_DAY {
            assert_ne!(decide(facts), Decision::Start);
        }
    }

    /// ROUTINES-14: an event Oby caused itself never starts a job.
    #[kani::proof]
    fn routines_14_obys_own_events_start_nothing() {
        let facts = any_facts();
        if facts.by_oby {
            assert_ne!(decide(facts), Decision::Start);
        }
    }
}
