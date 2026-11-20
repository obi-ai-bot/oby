//! One tool as the agents' `tools/list` gives it (BROWSER-1). A wire type: it owns no state and
//! writes nothing.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::McpAnnotations;

/// One tool as `tools/list` gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub annotations: McpAnnotations,
}
