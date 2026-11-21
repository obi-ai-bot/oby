//! The browser's API core (BROWSER-2 to 4, 11 to 13, 15, 16, 25, 26): each agent's tabs as the
//! engine reports them, asks, downloads, settings, the event log and the routes parsed. Pure: it
//! owns that state in memory only, writes no file, starts no process and reads no clock. It speaks
//! the wire types of [`crate::browser::contract`].
//!
//! Who calls is not decided here: obid's owner token and the device's signed words are checked by
//! `crate::approval` (`WORDS`, `words_message`), and an agent's job by its broker token
//! (`crate::connections`), before a route reaches [`Api`].

mod job;
mod lease;
mod mcp;
mod refused;
mod route;
mod state;
mod tab_rec;
mod take;

pub use job::Job;
pub use lease::Lease;
pub use mcp::{mcp_refusal, mcp_result};
pub use refused::{Refused, browser_off};
pub use route::{Route, route};
pub use state::{Api, EVENTS_KEPT, failure_state};
pub use take::Take;
