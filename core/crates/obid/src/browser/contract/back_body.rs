//! `POST /v1/browser/tabs/{id}/back`'s body (BROWSER-26): Go back, nothing more to say. A wire
//! type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// Go back: nothing more to say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackBody {}
