//! `PUT /v1/browser/settings`'s body (BROWSER-15): only the switches named change. A wire type: it
//! owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// `PUT /v1/browser/settings`: only the switches named change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub agents_use_browser: Option<bool>,
    pub agents_use_mac: Option<bool>,
}
