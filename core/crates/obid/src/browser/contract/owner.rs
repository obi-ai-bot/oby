//! Whose tab it is on the wire (BROWSER-3): an agent's (its job, which agent) or the person's own.
//! A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// Whose tab it is: an agent's (its job, which agent) or the person's own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Owner {
    #[serde(rename_all = "camelCase")]
    Agent {
        job: String,
        /// `claude`, `codex`, `cursor`, … (open: a string, never a closed enum).
        agent: String,
    },
    Person,
}
