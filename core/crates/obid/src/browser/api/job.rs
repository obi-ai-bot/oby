//! An agent's job as the browser's API knows it (BROWSER-2, 3): its agent, current tab, the names
//! of its last snapshot's refs, the downloads seen in its profile, and whether its browser is up.
//! A value [`super::Api`] owns; it writes nothing.

use std::collections::{BTreeMap, BTreeSet};

use crate::browser::contract as c;

/// An agent's job, as the API knows it.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: String,
    pub agent: String,
    pub current_tab: Option<String>,
    /// The names of the refs its last snapshot handed out, for the action's words.
    pub refs: BTreeMap<String, String>,
    /// Downloads already seen in its profile.
    pub seen_downloads: BTreeSet<String>,
    /// Whether its browser is up, and why not when it failed.
    pub browser: c::BrowserState,
    pub browser_reason: Option<String>,
}
