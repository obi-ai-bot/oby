//! A tool's annotations in `tools/list` (BROWSER-1): whether it only reads. A wire type: it owns no
//! state and writes nothing.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAnnotations {
    pub read_only_hint: bool,
}
