//! `POST /v1/browser/tabs/{id}/reload`'s body (BROWSER-26): Try again and Reload, nothing more to
//! say. A wire type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// Try again and Reload: nothing more to say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReloadBody {}
