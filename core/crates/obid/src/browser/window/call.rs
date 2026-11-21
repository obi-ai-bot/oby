//! How one command to a tab's page ended (BROWSER-11): its answer's data, its error, or ended at
//! its limit after so many milliseconds. It owns no state and writes nothing.

use serde_json::Value;

/// How one command to a page ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Ok(Value),
    Failed(String),
    /// Ended at its limit, after this many milliseconds.
    TimedOut(u64),
}
