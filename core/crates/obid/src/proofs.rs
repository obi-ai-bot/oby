//! Kani proofs of obid's pure security deciders, over unnarrowed inputs: any bytes, any case,
//! any combination of a job's marks (docs/proofs.md). Each file names the source files it proves
//! on its `//! Proves:` lines; scripts/kani-required.sh names a file's harnesses whenever one of
//! those changes. Compiled only by `cargo kani`.
#![cfg(kani)]

mod accept;
mod lanes;
mod owner_key;
mod publish;
mod rebound;
mod sealed;
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

/// Any bytes, at most `N` of them, in `buf`: every byte value, every length. For deciders over
/// bytes (a `&str` wrapper hands them its own) that compare with words longer than [`any_str`]
/// reaches, `N` one past the longest word: every longer string differs from each in length, which
/// every comparison reads first. A symbolic `&str` that long would cost CBMC its UTF-8 check, which
/// it unrolls to the harness's whole bound.
pub(crate) fn any_bytes_in<const N: usize>(buf: &mut [u8; N]) -> &[u8] {
    *buf = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= N);
    &buf[..len]
}
