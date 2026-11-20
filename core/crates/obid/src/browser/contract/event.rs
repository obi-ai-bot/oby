//! One message of `GET /v1/browser/events` (BROWSER-3, 4): its `seq` and body. A wire type: it owns
//! no state and writes nothing.

use serde::{Deserialize, Serialize};

use super::EventBody;

/// One message of `GET /v1/browser/events`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// Grows by one per event; the SSE `id`.
    pub seq: u64,
    #[serde(flatten)]
    pub body: EventBody,
}
