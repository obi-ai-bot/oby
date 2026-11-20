//! Where a download is (BROWSER-13): in the agent's profile, or delivered to the person. A wire
//! type: it owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// Where a download is: in the agent's profile (an agent's own), or delivered to the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    Profile,
    Delivered,
}
