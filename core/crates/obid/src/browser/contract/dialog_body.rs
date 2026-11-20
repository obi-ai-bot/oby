//! `POST /v1/browser/tabs/{id}/dialog`'s body (BROWSER-26): the page's own dialog answered by the
//! person. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// The page's own dialog answered: `accept` is Leave (OK), else Stay (Cancel); `promptText` for a
/// prompt.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogBody {
    pub accept: bool,
    #[serde(default)]
    pub prompt_text: Option<String>,
}
