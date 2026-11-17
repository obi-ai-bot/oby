//! Kani proofs of obid's pure security deciders, over unnarrowed inputs: any bytes, any case,
//! any combination of a job's marks (docs/proofs.md). Each file names the source files it proves
//! on its `//! Proves:` lines; CI runs a file's harnesses whenever one of those changes
//! (scripts/kani-required.sh), and every harness nightly. Compiled only by `cargo kani`.
#![cfg(kani)]

mod lanes;
mod publish;
mod token;
mod within;

/// Any string of at most `N` bytes: every length from 0 to `N`, every byte value, kept only
/// when it is UTF-8 (as a `&str` must be).
pub(crate) fn any_str<const N: usize>() -> String {
    let bytes: [u8; N] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= N);
    match std::str::from_utf8(&bytes[..len]) {
        Ok(s) => s.to_owned(),
        Err(_) => {
            kani::assume(false);
            unreachable!()
        }
    }
}
