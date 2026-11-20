//! Who started a download (BROWSER-13). A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartedBy {
    Agent,
    Person,
}
