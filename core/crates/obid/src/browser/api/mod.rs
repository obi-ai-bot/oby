//! The browser's API core (BROWSER-2 to 7, 11 to 16, 25): who holds a tab and under which
//! generation, asks, downloads, settings, the event log, the routes parsed, and a job's browser
//! brought up within its own allowance. Pure: it owns that state in memory only, writes no file,
//! starts no process and reads no clock but tokio's, in [`browser_up`]. It speaks the wire types of
//! [`crate::browser::contract`]; the routes that serve them come in a later pull request.
//!
//! Who calls is not decided here: obid's owner token and the device's signed words are checked by
//! `crate::approval` (`WORDS`, `words_message`), and an agent's job by its broker token
//! (`crate::connections`), before a route reaches [`Api`].

mod job;
mod lease;
mod mcp;
mod refused;
mod route;
mod start;
mod state;
mod tab_rec;
mod take;

pub use job::Job;
pub use lease::Lease;
pub use mcp::{mcp_refusal, mcp_result};
pub use refused::{Refused, browser_off};
pub use route::{Route, route};
pub use start::{Launch, Start, browser_up};
pub use state::{Api, EVENTS_KEPT, failure_state};
pub use take::Take;
