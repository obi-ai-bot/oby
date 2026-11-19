//! The lab's `browser` feature (src/lab/mod.rs): its nodes, `lab.browser.<verb>`, and what they run.
//! Owned by the session building it; nothing else edits this file.

use anyhow::{bail, Result};
use serde_json::Value;

use crate::obid::Run;

/// Every `lab.browser.*` node: its name, what it does.
pub const NODES: &[(&str, &str)] = &[];

pub async fn run_node(_run: &Run, name: &str, _input: &Value) -> Result<Value> {
    bail!("{name} isn't built yet")
}
