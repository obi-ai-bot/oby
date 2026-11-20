//! An agent's act let through (BROWSER-6): which tab, under which hold generation. It owns no state
//! and writes nothing.

/// An act let through: which tab, under which hold generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    pub tab: String,
    pub generation: u64,
}
