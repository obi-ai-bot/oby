//! What the window tells obid over the relay (T3 Code's `DesktopBrowserEvent`), each for one tab of
//! one job's, keyed by obid's job id (`threadId`) and its tab id, and the body that carries them in
//! order (`POST /v1/browser/relay`). A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// One event from the window. `type` only grows: obid ignores one it doesn't know.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RelayEvent {
    /// The tab's `<webview>` is up and its debugger attached: obid may send it CDP.
    #[serde(rename_all = "camelCase")]
    Attached { thread_id: String, tab_id: String },
    /// Its `<webview>` went away: closed, crashed, swapped, or devtools took the debugger.
    #[serde(rename_all = "camelCase")]
    Detached { thread_id: String, tab_id: String },
    /// A CDP frame from the tab's page: an answer `{id, result|error}`, or an event
    /// `{method, params}`, as JSON text.
    #[serde(rename_all = "camelCase")]
    Cdp {
        thread_id: String,
        tab_id: String,
        message: String,
    },
    /// The tab's page failed to load (the window may see it before obid asks).
    #[serde(rename_all = "camelCase")]
    LoadFailed {
        thread_id: String,
        tab_id: String,
        url: String,
        code: i64,
        description: String,
    },
    /// Its `<webview>` started or stopped being on screen.
    #[serde(rename_all = "camelCase")]
    Shown {
        thread_id: String,
        tab_id: String,
        shown: bool,
    },
    /// A type this obid doesn't know yet: ignored.
    #[serde(other)]
    Unknown,
}

/// `POST /v1/browser/relay`'s body: the window's events, in the order they happened.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelayEvents {
    pub events: Vec<RelayEvent>,
}
