//! Sealed by default (docs/contracts/obysphere.md, SPHERE-18 and 19): a request obid sends another
//! Oby goes sealed for its card's seal key; with none, it goes in the clear only when this owner's
//! signed word let it go so for that member, and never to an Obi known to seal. Letting it go
//! unsealed takes an app of the owner's signature; taking it back doesn't. Every combination.
//! What isn't proved here: that `Pins::allows_clear` holds only what a signed `PUT …/unsealed` kept,
//! and that accepting an owner's new key forgets it, are `Pins::allow_clear` and `Pins::accept`
//! (a set kept on disk); `sphere_api`'s tests cover them. Replies (`sender.rs`) keep their own rule:
//! sealed when the card has a seal key, refused to an Obi known to seal, in the clear otherwise.
//! Proves: core/crates/obid/src/sphere/link/reviews.rs
//! Proves: core/crates/obid/src/sphere/routes.rs

use crate::sphere::link::{Delivery, delivery};
use crate::sphere::routes::unsigned_unsealing;

/// Nothing goes in the clear to a member whose card has no seal key unless this owner allowed it,
/// nor ever to an Obi known to seal; a card with a seal key always gets it sealed.
#[kani::proof]
fn sphere_18_no_request_goes_in_the_clear_without_the_owners_word() {
    let (has_seal_key, seals, allowed_clear): (bool, bool, bool) =
        (kani::any(), kani::any(), kani::any());
    let goes = delivery(has_seal_key, seals, allowed_clear);
    assert_eq!(goes == Delivery::Sealed, has_seal_key);
    if goes == Delivery::Clear {
        assert!(!has_seal_key && !seals && allowed_clear);
    }
    if !has_seal_key && !allowed_clear {
        assert!(matches!(goes, Delivery::NotSealed | Delivery::KnownToSeal));
    }
    if !has_seal_key && seals {
        assert_eq!(goes, Delivery::KnownToSeal);
    }
}

/// Letting a member have words in the clear is refused without the owner's signature, which is
/// checked (and so used up) only then; taking it back is never refused.
#[kani::proof]
fn sphere_19_only_a_signed_word_lets_words_go_unsealed() {
    let (allowing, signed): (bool, bool) = (kani::any(), kani::any());
    let mut asked = false;
    let refused = unsigned_unsealing(allowing, || {
        asked = true;
        signed
    });
    assert_eq!(refused, allowing && !signed);
    assert_eq!(asked, allowing);
}
