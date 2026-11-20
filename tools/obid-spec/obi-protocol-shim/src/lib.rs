//! What obid's test stand-in OpenAI (core/crates/obid/tests/support/stand_in_openai.rs) uses of
//! obid's obi-protocol crate, the same as there: obid-spec imports that stand-in as it is
//! (src/coverage_accounts.rs) and checks obid from outside, so it doesn't depend on obid's crates.

pub mod time {
    /// Seconds since the epoch, as obi-protocol's `time::now`.
    pub fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

pub mod keys {
    use sha2::{Digest, Sha256};

    /// SHA-256, as obi-protocol's `keys::sha256`.
    pub fn sha256(bytes: impl AsRef<[u8]>) -> [u8; 32] {
        Sha256::digest(bytes.as_ref()).into()
    }
}
