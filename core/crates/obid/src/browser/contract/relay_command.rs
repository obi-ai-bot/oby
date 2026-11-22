//! What obid sends the window over the relay (T3 Code's `DesktopBrowserCommand`): a command for
//! one tab of one job's, keyed by obid's job id (`threadId`) and its tab id. Sent once
//! to whoever follows `GET /v1/browser/relay` now, never kept or replayed. A wire type: it owns no
//! state and writes nothing.

use serde::{Deserialize, Serialize};

/// Whether the agent's pointer moves to a point or clicks there: it only draws the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PointerPhase {
    Move,
    Click,
}

/// A command for the window. `type` only grows: the window ignores one it doesn't know.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RelayCommand {
    /// A CDP frame for the tab's page, `{id, method, params}`, as JSON text. Its answer comes back
    /// as the window's own `cdp`, with the same `id`.
    #[serde(rename_all = "camelCase")]
    Cdp {
        thread_id: String,
        tab_id: String,
        message: String,
        /// The frame is the agent's own script (`oby_browser_evaluate`), sent as `true`; a window
        /// may refuse it. Left out for obid's own frames (BROWSER-33).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        agent_code: bool,
    },
    /// obid is done with the tab: the window drops its relay (and, for a box tab, the partition's
    /// storage once it was the job's last).
    #[serde(rename_all = "camelCase")]
    Release { thread_id: String, tab_id: String },
    /// Where to draw the agent's pointer, in the page's CSS pixels: `move`, then `click`.
    #[serde(rename_all = "camelCase")]
    Pointer {
        thread_id: String,
        tab_id: String,
        phase: PointerPhase,
        x: f64,
        y: f64,
    },
    /// Whether an agent's turn runs for the job, so the tabs it uses keep focus between its steps.
    #[serde(rename_all = "camelCase")]
    Turn { thread_id: String, active: bool },
    /// The door a foreign job's box partition is held to (BROWSER-18): main sets `proxy`, the
    /// job's browser door with its token in it, as that partition's session's proxy, and alone
    /// reads it. Sent only on `GET /v1/browser/relay`, never put on `/v1/browser/events` or into
    /// `browser-event`.
    #[serde(rename_all = "camelCase")]
    Box {
        thread_id: String,
        partition: String,
        proxy: String,
    },
}

impl RelayCommand {
    pub fn kind(&self) -> &'static str {
        match self {
            RelayCommand::Cdp { .. } => "cdp",
            RelayCommand::Release { .. } => "release",
            RelayCommand::Pointer { .. } => "pointer",
            RelayCommand::Turn { .. } => "turn",
            RelayCommand::Box { .. } => "box",
        }
    }
}
