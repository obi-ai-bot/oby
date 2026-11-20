//! The answer to the person's input (BROWSER-7, 13): the tab and the downloads it delivered. A wire
//! type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputAnswer {
    pub tab_id: String,
    /// Downloads this input started, delivered to the person.
    pub delivered: Vec<String>,
}
