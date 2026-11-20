//! What a take does next (BROWSER-5, 6): held now, or held once the acts already running finish.
//! It owns no state and writes nothing.

use crate::browser::contract as c;

/// What a take does next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Take {
    /// Held now; no act of the agent's is running.
    Held(c::HoldAnswer),
    /// Held now (no new act is let through), but `n` acts let through before are still running:
    /// the take answers once they've finished.
    Wait(u32),
}
