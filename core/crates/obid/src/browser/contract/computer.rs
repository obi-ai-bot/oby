//! Which computer a tab runs on (BROWSER-3; board 14). A wire type: it owns no state and writes
//! nothing.

use serde::{Deserialize, Serialize};

/// Which computer the tab runs on (board 14: "on devbox").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Computer {
    /// `local` for this Mac, else the other computer's id.
    pub id: String,
    /// What the screens say: `This Mac`, `devbox`.
    pub name: String,
}

impl Computer {
    pub fn this_mac() -> Computer {
        Computer {
            id: "local".into(),
            name: "This Mac".into(),
        }
    }
}
