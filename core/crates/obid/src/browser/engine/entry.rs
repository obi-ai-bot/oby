//! One command in the engine's ledger (BROWSER-11): its tool, tab, caller, deadline and fate. A
//! value [`super::Browsers`] owns; it writes nothing.

use super::{Actor, CmdState};

#[derive(Clone, Debug)]
pub struct Entry {
    pub tool: String,
    pub tab: Option<String>,
    pub actor: Actor,
    pub deadline_ms: u64,
    pub state: CmdState,
}
