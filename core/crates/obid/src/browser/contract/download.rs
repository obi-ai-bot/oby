//! A download on the wire (BROWSER-13): its job and tab, the page's name for it, who started it,
//! where it is and its size. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

use super::{Place, StartedBy};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: String,
    pub job: String,
    pub tab_id: Option<String>,
    /// The file's name as the page gave it (the page's words).
    pub name: String,
    pub started_by: StartedBy,
    pub place: Place,
    pub bytes: u64,
}
