//! The refs a snapshot handed out and what they are good for (BROWSER-10): the page and control
//! generations they were taken under. A value [`super::Browsers`] keeps per tab; it writes nothing.

use std::collections::BTreeSet;

/// The refs a snapshot handed out, and what they are good for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refs {
    pub names: BTreeSet<String>,
    pub url: String,
    pub page_generation: u64,
    pub control_generation: u64,
}
