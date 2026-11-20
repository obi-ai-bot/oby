//! What a tab shows on the wire (BROWSER-11, 12; board 10), each with its own way out on the
//! screens. Decoded by the clients as open: an unknown value reads as `ready`. A wire type: it owns
//! no state and writes nothing.

use serde::{Deserialize, Serialize};

/// What the tab shows, each with its own way out on the screens (board 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabState {
    Loading,
    Ready,
    Failed,
    TimedOut,
    Dialog,
    Certificate,
    Crashed,
}
