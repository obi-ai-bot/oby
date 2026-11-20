//! Whether a job's browser is up (BROWSER-2). Decoded by the clients as open. A wire type: it owns
//! no state and writes nothing.

use serde::{Deserialize, Serialize};

/// Whether a job's browser is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserState {
    NotStarted,
    Starting,
    Ready,
    Failed,
}
