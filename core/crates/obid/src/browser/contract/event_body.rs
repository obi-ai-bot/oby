//! What an event says (BROWSER-2 to 4, 9, 13, 15), tagged by `type`. Decoded by the clients as
//! open: an unknown type is one to ignore. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

use super::{Ask, Download, JobBrowser, Outcome, Outside, Point, Settings, Tab};

// A `tab` event carries the whole tab by value, as the lab's has it and the API's event log keeps
// it; boxing it would change how every event is built in Rust, though not the wire.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventBody {
    /// A tab opened or changed (its URL, title, state, hold, ask or dialog).
    Tab {
        tab: Tab,
    },
    #[serde(rename_all = "camelCase")]
    TabClosed {
        tab_id: String,
    },
    /// The agent's action and pointer on a tab: "Clicking Sign in".
    #[serde(rename_all = "camelCase")]
    Action {
        tab_id: Option<String>,
        job: String,
        tool: String,
        /// What the screens say it does: `Clicking Sign in`, `Opening example.com`.
        text: String,
        pointer: Option<Point>,
        outcome: Outcome,
        /// For `refused` and `failed`: the reason's code.
        reason: Option<String>,
    },
    Ask {
        ask: Ask,
    },
    #[serde(rename_all = "camelCase")]
    AskAnswered {
        ask_id: String,
    },
    Download {
        download: Download,
    },
    /// An agent ran a script in a page (board 11): the script, and its result as outside data.
    #[serde(rename_all = "camelCase")]
    ScriptRun {
        tab_id: String,
        job: String,
        expression: String,
        result: Outside,
    },
    Settings {
        settings: Settings,
    },
    /// A job's browser starting, up, or not started after all.
    Browser {
        browser: JobBrowser,
    },
}

impl EventBody {
    pub fn kind(&self) -> &'static str {
        match self {
            EventBody::Tab { .. } => "tab",
            EventBody::TabClosed { .. } => "tabClosed",
            EventBody::Action { .. } => "action",
            EventBody::Ask { .. } => "ask",
            EventBody::AskAnswered { .. } => "askAnswered",
            EventBody::Download { .. } => "download",
            EventBody::ScriptRun { .. } => "scriptRun",
            EventBody::Settings { .. } => "settings",
            EventBody::Browser { .. } => "browser",
        }
    }
}
