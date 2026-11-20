//! One frame of a tab's live stream (BROWSER-14). A wire type: it owns no state and writes
//! nothing.

use serde::{Deserialize, Serialize};

/// One frame of a tab, `event: frame` on its stream. A reader that falls behind gets the newest
/// frame next and the ones between are dropped, never queued (`dropped` counts them for this
/// reader); one stuck reader never holds another back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamFrame {
    pub tab_id: String,
    /// The browser's own frame number: a gap is a dropped frame.
    pub seq: u64,
    /// The image, base64.
    pub data: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    /// Frames dropped for this reader so far.
    pub dropped: u64,
}
