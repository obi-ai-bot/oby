//! A tab on the wire (BROWSER-3, 5): its owner, the page's title and URL, its state, who holds it
//! and under which generation, its computer, ask and dialog. A wire type: it owns no state and
//! writes nothing.

use serde::{Deserialize, Serialize};

use super::{Ask, Computer, Owner, TabState};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    pub id: String,
    pub owner: Owner,
    /// The page's own words: shown to the person as they are, quoted to an agent.
    pub title: String,
    pub url: String,
    pub state: TabState,
    /// The device whose person holds the tab (take control), or null.
    pub held_by: Option<String>,
    /// Changes on every take and give back. An agent's act let through under one generation is
    /// refused under the next, never half-applied.
    pub hold_generation: u64,
    pub computer: Computer,
    /// The agent's ask on this tab still waiting for the person, if any.
    pub ask: Option<Ask>,
    /// The dialog the page shows (an alert's words, the page's), if any.
    pub dialog: Option<String>,
    /// For an agent's tab: the webview partition the window opens it in (the window's relay).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition: Option<String>,
}
