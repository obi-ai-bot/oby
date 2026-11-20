//! How one agent-browser call ended (BROWSER-11): its JSON answer's data, its error, or killed at
//! its limit after so many milliseconds. It owns no state and writes nothing.

use serde_json::Value;

/// How one agent-browser call ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Ok(Value),
    Failed(String),
    /// Killed at its limit, after this many milliseconds.
    TimedOut(u64),
}
