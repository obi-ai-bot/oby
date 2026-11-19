//! A teammate's changed owner key (docs/contracts/obysphere.md, SPHERE-8 and 23): what the Obysphere's
//! owner event says changed it maps, for any string, to obid's closed set of reasons (`rotated` and
//! anything else: none), each with obid's own fixed sentence (`inbox::tests`); and whatever the event says, a
//! card signed with a key other than the one pinned is never trusted by itself. Reasons are any
//! bytes up to 16, one past `signed_in_again` (`any_bytes_in`; `owner_event_reason` hands
//! `owner_event_code` its bytes); keys any string up to 4 bytes.
//! What isn't proved here: trusting the new key is the owner's signed Confirm of the card
//! (`Pins::accept`, a fingerprint compared), which `sphere_api`'s tests cover.
//! Proves: core/crates/obid/src/sphere/link.rs
//! Proves: core/crates/obid/src/sphere/inbox.rs
//! Proves: core/crates/obid/src/sphere/pins.rs

use super::{any_bytes_in, any_str};
use crate::sphere::link::owner_event_code;
use crate::sphere::pins::{Sight, sight};

/// The mapping is total and closed: a reason is kept only when it is one of the three, as itself.
#[kani::proof]
#[kani::unwind(17)]
fn sphere_23_an_owner_events_reason_is_obids_closed_set() {
    let mut buf = [0u8; 16];
    let reason = any_bytes_in(&mut buf);
    let known = matches!(
        reason,
        b"signed_in_again" | b"admin_reset" | b"operator_reset"
    );
    match owner_event_code(reason) {
        Some(code) => assert!(known && code.as_bytes() == reason),
        None => assert!(!known),
    }
}

/// Only the pinned key, or the first one seen, is trusted; a changed key never is, newly seen or
/// already waiting, whatever reason the Sphere gave: `sight` takes no reason, so none can change it.
#[kani::proof]
#[kani::unwind(8)]
fn sphere_23_a_changed_key_is_never_trusted_by_itself() {
    let pin = kani::any::<bool>().then(|| (any_str::<4>(), kani::any::<bool>().then(any_str::<4>)));
    let key = any_str::<4>();
    let seen = sight(
        pin.as_ref()
            .map(|(k, waiting)| (k.as_str(), waiting.as_deref())),
        &key,
    );
    let trusted = matches!(seen, Sight::Pinned | Sight::First);
    match &pin {
        None => assert_eq!(seen, Sight::First),
        Some((pinned, _)) if *pinned == key => assert_eq!(seen, Sight::Pinned),
        Some((_, waiting)) => {
            assert!(!trusted);
            assert_eq!(
                seen == Sight::StillChanged,
                waiting.as_deref() == Some(key.as_str())
            );
        }
    }
}
