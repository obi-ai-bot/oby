//! An agent's ask on a tab (BROWSER-19; boards 7 to 9): its kind, the agent's reason, a choice's
//! choices, a secret's field, and the answer. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

use super::AskKind;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    pub id: String,
    pub tab_id: String,
    pub job: String,
    pub kind: AskKind,
    /// The agent's words, why it asks.
    pub reason: String,
    /// For `choice`: what the person picks from.
    #[serde(default)]
    pub choices: Vec<String>,
    /// For `signIn` and `code`: the page's field the secret goes into (a CSS selector).
    pub field: Option<String>,
    pub answered: bool,
    /// For `choice`, once answered: what the person picked (the agent reads it in list_tabs).
    pub choice: Option<String>,
}
