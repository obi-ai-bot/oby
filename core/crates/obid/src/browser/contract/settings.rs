//! Settings › General's two switches on the wire (BROWSER-15), both on by default. A wire type: it
//! owns no state and writes nothing.

use serde::{Deserialize, Serialize};

/// Settings › General's two switches, both on by default (the owner's decision of 2026-11-19).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// "Agents can use the browser": off, every `oby_browser_*` tool refuses with `browser_off`.
    pub agents_use_browser: bool,
    /// "Agents can use this Mac": the desktop control's switch.
    pub agents_use_mac: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            agents_use_browser: true,
            agents_use_mac: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn browser_15_both_switches_are_on_by_default() {
        assert_eq!(
            serde_json::to_value(Settings::default()).unwrap(),
            json!({"agentsUseBrowser": true, "agentsUseMac": true})
        );
    }
}
