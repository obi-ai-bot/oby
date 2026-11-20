//! Save a copy's answer (BROWSER-13): the copy's name in the person's Downloads. A wire type: it
//! owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// Save a copy: the agent's file stays in its profile, a copy lands in the person's Downloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCopyAnswer {
    pub id: String,
    /// The copy's name in the person's Downloads (a number added when the name is taken).
    pub saved_as: String,
}
