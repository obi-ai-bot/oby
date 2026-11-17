//! The runner's token check: a caller's token is accepted exactly when it is the token, for any
//! two strings of up to 6 bytes (every byte value, every length).
//! Proves: core/crates/obid/src/token.rs

use super::any_str;
use crate::token::same;

/// `same` is equality: no shorter, longer or different token gets in, and the token always does.
#[kani::proof]
#[kani::unwind(8)]
fn token_same_is_exactly_equality() {
    let (given, token) = (any_str::<6>(), any_str::<6>());
    assert_eq!(same(&given, &token), given == token);
}
