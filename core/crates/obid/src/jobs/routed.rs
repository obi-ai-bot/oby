//! Where routing put a new task (docs/contracts/accounts.md, ACCOUNTS-30 to 33): its tool and
//! account, what it waits for, and the line that says why when that isn't where the owner's words
//! put it. Made by [`super::Jobs::route_new`]; it owns no state and writes nothing.

use crate::agent::Tool;
use crate::job::Waiting;

/// A new task's place, as [`super::Jobs::route_new`] found it.
#[derive(Debug, Clone)]
pub struct Routed {
    pub tool: Tool,
    pub account: crate::config::Account,
    /// What it waits for before it starts (room, a reserve lifted, the owner's yes), or none.
    pub waiting: Option<Waiting>,
    /// Why it runs somewhere other than the agent the owner named (ACCOUNTS-31): "Claude Code is
    /// at 97% of its week until Thu 9:00 AM, so this runs on Codex · Personal."
    pub line: Option<String>,
}
