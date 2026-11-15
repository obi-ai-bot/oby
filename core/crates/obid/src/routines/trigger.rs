//! What one GitHub event does for a routine on GitHub events (docs/contracts/routines.md,
//! ROUTINES-9 to 17): start its job, ignore it or hold it, from a few facts about it. Pure, so
//! Kani proves it: the receipts, the jobs and the clock are routines.rs's.

use serde::{Deserialize, Serialize};

/// The most jobs one routine starts on one item in 24 hours (ROUTINES-12).
pub const STARTS_A_DAY: u32 = 3;

/// The most jobs one routine starts at one read of the GitHub work feed (ROUTINES-15).
pub const STARTS_A_READ: u32 = 3;

/// The most jobs one routine starts across all its items in 24 hours (ROUTINES-16).
pub const ROUTINE_STARTS_A_DAY: u32 = 10;

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
    RoutineDayLimit,
    ReadLimit,
}

impl Why {
    /// obid's words for it, a receipt's `reason`.
    pub fn reason(self) -> &'static str {
        match self {
            Why::OwnPush => "Oby's own push or pull request caused it.",
            Why::NoMarker => "GitHub didn't say which commit it's on.",
            Why::OpenJob => "This routine's job on it is still open.",
            Why::DayLimit => "This routine started 3 jobs on it in the last 24 hours.",
            Why::RoutineDayLimit => "This routine started 10 jobs in the last 24 hours.",
            Why::ReadLimit => "This routine started 3 jobs at this read of GitHub.",
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
    /// This routine's `started` receipts on any item in the last 24 hours.
    pub routine_starts_today: u32,
    /// The jobs this routine started earlier in this read.
    pub starts_this_read: u32,
}

/// What one event does: a key dealt with is done; Oby's own and markerless events are ignored; one
/// behind an open job, past the item's or the routine's day's starts or past the read's starts is
/// held; any other starts its job.
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
    if event.routine_starts_today >= ROUTINE_STARTS_A_DAY {
        return Decision::Hold(Why::RoutineDayLimit);
    }
    if event.starts_this_read >= STARTS_A_READ {
        return Decision::Hold(Why::ReadLimit);
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
        routine_starts_today: 0,
        starts_this_read: 0,
    };

    #[test]
    fn routines_10_to_16_each_rule_decides() {
        // ROUTINES-9: a new event, nothing against it, starts; a held one starts once it's free.
        assert_eq!(decide(NEW), Decision::Start);
        let held = Facts {
            receipt: Some(Outcome::Held),
            starts_today: 2,
            ..NEW
        };
        assert_eq!(decide(held), Decision::Start);
        let under = Facts {
            receipt: Some(Outcome::Held),
            routine_starts_today: 9,
            starts_this_read: 2,
            ..NEW
        };
        assert_eq!(decide(under), Decision::Start);
        // ROUTINES-10: a key dealt with is done, whatever else holds.
        for outcome in [Outcome::Started, Outcome::Ignored, Outcome::Failed] {
            let dealt = Facts {
                receipt: Some(outcome),
                by_oby: true,
                marker: false,
                open_job: true,
                starts_today: 9,
                routine_starts_today: u32::MAX,
                starts_this_read: u32::MAX,
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
        // ROUTINES-16 at exactly 10, then ROUTINES-15 at exactly 3, each after ROUTINES-12.
        let capped = Facts {
            starts_today: 3,
            routine_starts_today: 10,
            starts_this_read: 3,
            ..NEW
        };
        assert_eq!(decide(capped), Decision::Hold(Why::DayLimit));
        let both = Facts {
            starts_today: 0,
            ..capped
        };
        assert_eq!(decide(both), Decision::Hold(Why::RoutineDayLimit));
        for routine_starts_today in [10, 11, u32::MAX] {
            let full = Facts {
                routine_starts_today,
                ..NEW
            };
            assert_eq!(decide(full), Decision::Hold(Why::RoutineDayLimit));
        }
        for starts_this_read in [3, 4, u32::MAX] {
            let full = Facts {
                starts_this_read,
                ..NEW
            };
            assert_eq!(decide(full), Decision::Hold(Why::ReadLimit));
        }
        assert_eq!(
            Why::RoutineDayLimit.reason(),
            "This routine started 10 jobs in the last 24 hours."
        );
        assert_eq!(
            Why::ReadLimit.reason(),
            "This routine started 3 jobs at this read of GitHub."
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
            routine_starts_today: kani::any(),
            starts_this_read: kani::any(),
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

    /// ROUTINES-15: with 3 or more starts at this read, nothing starts.
    #[kani::proof]
    fn routines_15_no_fourth_start_in_a_read() {
        assert_eq!(STARTS_A_READ, 3);
        let facts = any_facts();
        if facts.starts_this_read >= STARTS_A_READ {
            assert_ne!(decide(facts), Decision::Start);
        }
    }

    /// ROUTINES-15: an event only the read's cap stops is held, so a later read starts it.
    #[kani::proof]
    fn routines_15_a_capped_event_is_held() {
        let facts = any_facts();
        if matches!(facts.receipt, None | Some(Outcome::Held))
            && !facts.by_oby
            && facts.marker
            && !facts.open_job
            && facts.starts_today < STARTS_A_DAY
            && facts.routine_starts_today < ROUTINE_STARTS_A_DAY
            && facts.starts_this_read >= STARTS_A_READ
        {
            assert_eq!(decide(facts), Decision::Hold(Why::ReadLimit));
        }
    }

    /// ROUTINES-16: with 10 or more starts of the routine in 24 hours, nothing starts.
    #[kani::proof]
    fn routines_16_no_eleventh_start_in_a_day() {
        assert_eq!(ROUTINE_STARTS_A_DAY, 10);
        let facts = any_facts();
        if facts.routine_starts_today >= ROUTINE_STARTS_A_DAY {
            assert_ne!(decide(facts), Decision::Start);
        }
    }

    /// ROUTINES-16: an event only the routine's day cap stops is held, so it starts once the
    /// routine is under the cap again.
    #[kani::proof]
    fn routines_16_a_capped_event_is_held() {
        let facts = any_facts();
        if matches!(facts.receipt, None | Some(Outcome::Held))
            && !facts.by_oby
            && facts.marker
            && !facts.open_job
            && facts.starts_today < STARTS_A_DAY
            && facts.routine_starts_today >= ROUTINE_STARTS_A_DAY
        {
            assert_eq!(decide(facts), Decision::Hold(Why::RoutineDayLimit));
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
