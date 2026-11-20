//! The person's input on a held tab (BROWSER-7, 14), in the page's CSS pixels. A wire type: it owns
//! no state and writes nothing.

use serde::{Deserialize, Serialize};

/// The person's input on a held tab, in the page's CSS pixels (agent-browser's stream input).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StreamInput {
    #[serde(rename_all = "camelCase")]
    Mouse {
        /// `mousePressed`, `mouseReleased`, `mouseMoved`, `mouseWheel`.
        event_type: String,
        x: f64,
        y: f64,
        #[serde(default)]
        button: Option<String>,
        #[serde(default)]
        click_count: Option<u32>,
        #[serde(default)]
        delta_x: Option<f64>,
        #[serde(default)]
        delta_y: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    Key {
        /// `keyDown`, `keyUp`, `char`.
        event_type: String,
        key: String,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        code: Option<String>,
    },
}
