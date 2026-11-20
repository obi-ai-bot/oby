//! B00-69's formal property (BROWSER-5): an agent may act on a tab only when it is its own and no
//! person holds it. Kept apart so a model checker can take it alone; it owns no state and writes
//! nothing.

use super::Kind;

/// B00-69's formal property, kept apart so a model checker can take it alone: an agent may act on
/// a tab only when it is its own and no person holds it; reads need only that it is its own.
pub fn may_act(own_tab: bool, held_by_person: bool, kind: Kind) -> bool {
    match kind {
        Kind::Act => own_tab && !held_by_person,
        Kind::Read => own_tab,
        Kind::Open | Kind::Oby => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b00_69_may_act_is_false_for_every_act_while_the_person_holds_the_tab() {
        for own in [true, false] {
            assert!(!may_act(own, true, Kind::Act));
            assert!(!may_act(false, false, Kind::Act));
            assert!(!may_act(false, false, Kind::Read));
        }
        assert!(may_act(true, false, Kind::Act));
        assert!(may_act(true, true, Kind::Read));
    }
}
