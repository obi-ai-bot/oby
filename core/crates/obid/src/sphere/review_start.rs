//! The first answer to another Oby's review request (docs/contracts/teammate-review.md,
//! TEAMREVIEW-3): accept it only when this Oby can start the review, else decline it with one of
//! obid's own sentences. Pure, so Kani proves it: what this Oby has is read by
//! `Jobs::can_review`, and the reply is the Link's.

/// obid's words for a review it can't start, a decline's `note`, in [`CantStart`]'s order. None is
/// built from anything a request wrote.
pub const NOTES: [&str; 3] = [
    "This Oby's owner can't push to that repository, so it can't review it.",
    "This Oby has no signed-in account that can review it now.",
    "This Oby can't make its job sandbox on this machine, so it can't review it.",
];

/// What this Oby has for a review request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Can {
    /// The repository is one obid lists, or one of GitHub's that the owner's login can push to.
    pub pushable: bool,
    /// A signed-in account can take the review now.
    pub account: bool,
    /// obid can make the job sandbox on this machine.
    pub sandbox: bool,
}

/// Why a review can't start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CantStart {
    NoRepository,
    NoAccount,
    NoSandbox,
}

impl CantStart {
    /// obid's sentence for it, from [`NOTES`].
    pub fn note(self) -> &'static str {
        NOTES[self as usize]
    }
}

/// The first reply to a review request a person or a decision accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstAnswer {
    Accept,
    Decline(CantStart),
}

/// Accepts only when the repository, an account and the sandbox are all there; else declines,
/// naming the first one missing in that order.
pub fn first_answer(can: Can) -> FirstAnswer {
    if !can.pushable {
        return FirstAnswer::Decline(CantStart::NoRepository);
    }
    if !can.account {
        return FirstAnswer::Decline(CantStart::NoAccount);
    }
    if !can.sandbox {
        return FirstAnswer::Decline(CantStart::NoSandbox);
    }
    FirstAnswer::Accept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn can(pushable: bool, account: bool, sandbox: bool) -> Can {
        Can {
            pushable,
            account,
            sandbox,
        }
    }

    #[test]
    fn each_missing_fact_declines_with_its_sentence_and_all_three_accept() {
        assert_eq!(first_answer(can(true, true, true)), FirstAnswer::Accept);
        let declined = |c| match first_answer(c) {
            FirstAnswer::Decline(why) => why.note(),
            FirstAnswer::Accept => panic!("accepted {c:?}"),
        };
        assert_eq!(
            declined(can(false, true, true)),
            "This Oby's owner can't push to that repository, so it can't review it."
        );
        assert_eq!(
            declined(can(true, false, true)),
            "This Oby has no signed-in account that can review it now."
        );
        assert_eq!(
            declined(can(true, true, false)),
            "This Oby can't make its job sandbox on this machine, so it can't review it."
        );
    }

    #[test]
    fn the_first_missing_in_order_is_named() {
        let no = FirstAnswer::Decline;
        assert_eq!(
            first_answer(can(false, false, false)),
            no(CantStart::NoRepository)
        );
        assert_eq!(
            first_answer(can(false, true, false)),
            no(CantStart::NoRepository)
        );
        assert_eq!(
            first_answer(can(false, false, true)),
            no(CantStart::NoRepository)
        );
        assert_eq!(
            first_answer(can(true, false, false)),
            no(CantStart::NoAccount)
        );
    }

    #[test]
    fn every_note_is_one_line_of_1_to_200_characters() {
        for note in NOTES {
            let length = note.chars().count();
            assert!((1..=200).contains(&length), "{note}");
            assert!(!note.contains(['\n', '\r']), "{note}");
            assert!(note.is_ascii(), "{note}");
        }
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of [`first_answer`]: `kani::any()` stands for every value of every fact.

    use super::*;

    /// Anything this Oby may have.
    fn any_can() -> Can {
        Can {
            pushable: kani::any(),
            account: kani::any(),
            sandbox: kani::any(),
        }
    }

    /// TEAMREVIEW-3: a request is accepted only when all three are there, and always then.
    #[kani::proof]
    fn teamreview_3_never_accepts_what_it_cant_start() {
        let can = any_can();
        if !can.pushable || !can.account || !can.sandbox {
            assert!(first_answer(can) != FirstAnswer::Accept);
        } else {
            assert!(first_answer(can) == FirstAnswer::Accept);
        }
    }

    /// TEAMREVIEW-3: a decline's note is its reason's entry of `NOTES`, never anything else.
    #[kani::proof]
    fn teamreview_3_a_decline_says_only_obids_sentences() {
        if let FirstAnswer::Decline(why) = first_answer(any_can()) {
            let note = why.note();
            assert!(!note.is_empty());
            assert!(note.len() <= 200);
            assert!(core::ptr::eq(note, NOTES[why as usize]));
        }
    }
}
