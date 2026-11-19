//! Accepting another Oby's work (docs/contracts/obysphere.md, SPHERE-17): an Accept, or Accept and
//! remember, of an open review request or handoff goes through `POST /v1/inbox/{id}/answer` only
//! when an app of the owner's signed that exact request, or a phone paired through the gateway sent
//! it; Decline, Later and every other answer need neither. Every answer: any bytes up to 35, one
//! past `Accept, and remember for this repo` (`any_bytes_in`), or none; every request type any bytes
//! up to 15, one past `review_request`, or none. `inbox::accepts` and the route hand these deciders
//! the strings' bytes. Bound 36, for the comparisons with those words.
//! What isn't proved here: that the signature is valid over that exact request is
//! `Approvals::check_words` (an ECDSA check over `approval::words_message`: its method, path, body
//! hash and time), which CBMC can't run; `approval::tests` cover it.
//! Proves: core/crates/obid/src/sphere/routes.rs
//! Proves: core/crates/obid/src/sphere/inbox.rs

use super::any_bytes_in;
use crate::sphere::inbox::accepts_bytes;
use crate::sphere::routes::unsigned_accept;

/// What the inbox carries out as an Accept (`inbox::accepts`, read by `Link::answer`'s accept and,
/// as `accepts_bytes`, by the route) is exactly the two Accept answers, written out here, not taken
/// from the constants.
#[kani::proof]
#[kani::unwind(36)]
fn sphere_17_only_the_two_accepts_accept() {
    let mut buf = [0u8; 35];
    let answer = any_bytes_in(&mut buf);
    assert_eq!(
        accepts_bytes(answer),
        answer == b"Accept" || answer == b"Accept, and remember for this repo"
    );
}

/// The route refuses exactly an Accept of an open review request or handoff that neither a signature
/// of the owner's app nor a paired phone carries; Decline and Later (no answer) never are.
#[kani::proof]
#[kani::unwind(36)]
fn sphere_17_an_accept_of_another_obys_work_needs_the_owners_hand() {
    let (signed, paired): (bool, bool) = (kani::any(), kani::any());
    let (mut asked, mut said) = ([0u8; 15], [0u8; 35]);
    let request = if kani::any() {
        Some(any_bytes_in(&mut asked))
    } else {
        None
    };
    let answer = if kani::any() {
        Some(any_bytes_in(&mut said))
    } else {
        None
    };
    let refused = unsigned_accept(signed, paired, request, answer);
    let work = matches!(request, Some(b"review_request" | b"handoff"));
    let accepting = matches!(
        answer,
        Some(b"Accept" | b"Accept, and remember for this repo")
    );
    assert_eq!(refused, work && accepting && !signed && !paired);
    // Saying no, or putting it off, never needs the owner's hand.
    if matches!(
        answer,
        None | Some(b"Decline" | b"Decline, and remember for this repo")
    ) {
        assert!(!refused);
    }
}
