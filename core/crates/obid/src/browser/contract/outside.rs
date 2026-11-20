//! Data from a page, quoted, on the wire (BROWSER-9): as the engine's
//! [`crate::browser::engine::outside`] builds it. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Data from a page, quoted ([`crate::browser::engine::outside`]): where it came from, what it is,
/// the note, and the data itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outside {
    pub from: String,
    pub what: String,
    pub note: String,
    pub data: Value,
}
