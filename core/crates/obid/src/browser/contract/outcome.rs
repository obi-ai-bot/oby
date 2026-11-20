//! How an agent's action ended (BROWSER-3; board 6). A wire type: it owns no state and writes
//! nothing.

use serde::{Deserialize, Serialize};

/// How an agent's action ended, as its log on the screens shows it (board 6: "refused").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Started,
    Done,
    Refused,
    Failed,
}
