//! A call the engine has let start (BROWSER-6, 10, 11): what the driver needs to run it, the hold
//! generation and deadline it was let start under, and the refs checked again right before it
//! runs. It owns no state and writes nothing.

/// A call the core has let start: what the driver needs to run it.
#[derive(Clone, Debug)]
pub struct Ticket {
    pub cmd: u64,
    pub tool: &'static str,
    pub tab: Option<String>,
    pub target: Option<String>,
    pub control_generation: u64,
    /// When the command must have finished, on the caller's clock (ms).
    pub deadline_ms: u64,
    /// The agent-browser arguments, after the global flags.
    pub argv: Vec<String>,
    /// Refs the call names, checked against the tab's URL right before it runs.
    pub refs: Vec<String>,
}
