//! A job's browser on the wire (BROWSER-2): its job, agent, state and, when it failed, why. A wire
//! type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

use super::BrowserState;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobBrowser {
    pub job: String,
    pub agent: String,
    pub state: BrowserState,
    /// For `failed`: why, in a sentence.
    pub reason: Option<String>,
}
