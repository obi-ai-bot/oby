//! The answer to a secret (BROWSER-8): the tab, the field and whether it was filled, never the
//! value. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretAnswer {
    pub tab_id: String,
    pub field: String,
    pub filled: bool,
}
