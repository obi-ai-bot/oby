//! `GET /v1/browser/downloads`'s answer (BROWSER-4, 13): every download and the last event the
//! answer reflects. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

use super::Download;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsAnswer {
    pub downloads: Vec<Download>,
    /// The last event this answer reflects, as [`TabsAnswer::seq`].
    #[serde(default)]
    pub seq: u64,
}
