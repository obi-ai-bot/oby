//! The answer to take and give back (BROWSER-5, 6). A wire type: it owns no state and writes
//! nothing.

use serde::{Deserialize, Serialize};

/// The answer to take and give back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldAnswer {
    pub tab_id: String,
    pub held_by: Option<String>,
    pub hold_generation: u64,
}
