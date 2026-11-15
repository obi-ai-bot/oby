//! The cards obid raises for the owner by itself, and the ones the owner's own conversation work
//! (`crate::merge::own_work`) skips: the check offer (PUBLISHING-33), the safety limit's Confirm
//! (RECOVERY-18), the Confirm for words a task never heard (TASKCONV-21) and `oby_offer`'s Start
//! card (TASKCONV-22). Each is decided here on bools and a `Source` alone.

use super::Source;

/// PUBLISHING-33: a repository with no check is offered one (`unanswered`: no check anywhere, a
/// GitHub remote, not research or a review, no `noCheck`), but never for the owner's own work.
pub fn offers_check(own: bool, unanswered: bool) -> bool {
    unanswered && !own
}

/// What words do to a job stopped at its safety limit, its ceiling or budget (RECOVERY-18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PastSafety {
    /// It carries on now, as the owner's Confirm does.
    CarriesOn,
    /// A Confirm asks the owner first.
    Asks,
    /// Not the owner's words, so nothing carries it on.
    Refused,
}

/// RECOVERY-18: past its safety limit, the owner's Confirm and their signed words to their own
/// work carry a job on; other direct words ask; untrusted and event words don't.
pub fn past_safety_by(source: Source, own: bool) -> PastSafety {
    match source {
        Source::Confirmed => PastSafety::CarriesOn,
        Source::Owner if own => PastSafety::CarriesOn,
        Source::Owner | Source::Api => PastSafety::Asks,
        Source::Untrusted | Source::Event => PastSafety::Refused,
    }
}

/// What the owner's words a job that ended never heard do when they couldn't start its follow-up
/// (TASKCONV-21).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unheard {
    /// A Confirm tries again with them.
    Asks,
    /// The safety limit's Confirm already does.
    SafetyAsks,
    /// No card: the job's step says why, and its next follow-up takes them.
    Says,
}

/// TASKCONV-21: the owner's signed or confirmed words their own work never heard make no Confirm.
pub fn unheard_words(signed: bool, own: bool, past_safety: bool) -> Unheard {
    match (signed && own, past_safety) {
        (true, _) => Unheard::Says,
        (false, true) => Unheard::SafetyAsks,
        (false, false) => Unheard::Asks,
    }
}

/// TASKCONV-22: `oby_offer` makes a Start card only in an event turn or one no app of the owner's
/// signed.
pub fn offer_card(signed: bool, event: bool) -> bool {
    event || !signed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishing_33_own_work_never_offers_a_check() {
        for (own, unanswered, offers) in [
            (false, false, false),
            (false, true, true),
            (true, false, false),
            (true, true, false),
        ] {
            assert_eq!(offers_check(own, unanswered), offers, "{own} {unanswered}");
        }
    }

    #[test]
    fn recovery_18_only_signed_words_carry_own_work_past_its_safety_limit() {
        use PastSafety::{Asks, CarriesOn, Refused};
        for (source, own, want) in [
            (Source::Confirmed, false, CarriesOn),
            (Source::Confirmed, true, CarriesOn),
            (Source::Owner, true, CarriesOn),
            (Source::Owner, false, Asks),
            (Source::Api, false, Asks),
            (Source::Api, true, Asks),
            (Source::Untrusted, false, Refused),
            (Source::Untrusted, true, Refused),
            (Source::Event, false, Refused),
            (Source::Event, true, Refused),
        ] {
            assert_eq!(past_safety_by(source, own), want, "{source:?} {own}");
        }
    }

    #[test]
    fn taskconv_21_unheard_words_ask_only_outside_own_work() {
        for past in [false, true] {
            assert_eq!(unheard_words(true, true, past), Unheard::Says);
            for (signed, own) in [(false, false), (false, true), (true, false)] {
                let want = if past {
                    Unheard::SafetyAsks
                } else {
                    Unheard::Asks
                };
                assert_eq!(unheard_words(signed, own, past), want, "{signed} {own}");
            }
        }
    }

    #[test]
    fn taskconv_22_a_signed_turn_offers_no_card() {
        assert!(!offer_card(true, false));
        assert!(offer_card(false, false));
        assert!(offer_card(false, true));
        assert!(offer_card(true, true));
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of the cards the owner's own conversation work skips (PUBLISHING-33,
    //! RECOVERY-18, TASKCONV-21, TASKCONV-22): `kani::any()` stands for every value of the bools
    //! and of `Source`.

    use super::*;

    /// Who said it, from a number.
    fn any_source() -> Source {
        match kani::any::<u8>() {
            0 => Source::Owner,
            1 => Source::Api,
            2 => Source::Untrusted,
            3 => Source::Event,
            _ => Source::Confirmed,
        }
    }

    /// PUBLISHING-33: own work is never offered a check; any other job exactly when today's
    /// conditions all hold.
    #[kani::proof]
    fn publishing_33_own_work_never_offers_a_check() {
        let (own, unanswered): (bool, bool) = (kani::any(), kani::any());
        let offers = offers_check(own, unanswered);
        if own {
            assert!(!offers);
        } else {
            assert_eq!(offers, unanswered);
        }
    }

    /// RECOVERY-18: past its safety limit a job carries on with no Confirm exactly on the owner's
    /// Confirm or their signed words to their own work; other direct words ask; untrusted and
    /// event words are refused.
    #[kani::proof]
    fn recovery_18_only_signed_words_carry_own_work_past_its_safety_limit() {
        let (source, own) = (any_source(), kani::any::<bool>());
        let by = past_safety_by(source, own);
        let carries = source == Source::Confirmed || (source == Source::Owner && own);
        assert_eq!(by == PastSafety::CarriesOn, carries);
        if matches!(source, Source::Untrusted | Source::Event) {
            assert_eq!(by, PastSafety::Refused);
        } else if !carries {
            assert_eq!(by, PastSafety::Asks);
        }
    }

    /// TASKCONV-21: unheard words make no Confirm exactly when signed and the job is own work;
    /// otherwise a job past its safety limit adds nothing and any other asks.
    #[kani::proof]
    fn taskconv_21_unheard_words_ask_only_outside_own_work() {
        let (signed, own, past): (bool, bool, bool) = (kani::any(), kani::any(), kani::any());
        let does = unheard_words(signed, own, past);
        if signed && own {
            assert_eq!(does, Unheard::Says);
        } else if past {
            assert_eq!(does, Unheard::SafetyAsks);
        } else {
            assert_eq!(does, Unheard::Asks);
        }
    }

    /// TASKCONV-22: an offer makes a Start card exactly in an event turn or an unsigned one.
    #[kani::proof]
    fn taskconv_22_a_signed_turn_offers_no_card() {
        let (signed, event): (bool, bool) = (kani::any(), kani::any());
        let card = offer_card(signed, event);
        if signed && !event {
            assert!(!card);
        } else {
            assert!(card);
        }
    }
}
