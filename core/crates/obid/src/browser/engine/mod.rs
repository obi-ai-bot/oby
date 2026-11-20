//! The browser's engine core: every conversation's tabs, who holds each, every command's fate,
//! and the rules an agent's tool call passes before agent-browser runs it (BROWSER-5, 6, 9 to 11,
//! 13, 14, 27). Pure: it owns no file and starts no process; the time is always the caller's.

mod actor;
mod browsers;
mod certificate_page;
mod cmd_state;
mod download_fate;
mod engine_tab;
mod entry;
mod frame_slot;
mod in_room;
mod kind;
mod may_act;
mod outside;
mod refs;
mod refusal;
mod ticket;
mod tool;

pub use actor::Actor;
pub use browsers::{Browsers, DEFAULT_DEADLINE_MS, MAX_DEADLINE_MS};
pub use certificate_page::{is_certificate_error, is_error_page, works_the_page};
pub use cmd_state::CmdState;
pub use download_fate::{DownloadFate, download_rule};
pub use engine_tab::EngineTab;
pub use entry::Entry;
pub use frame_slot::FrameSlot;
pub use in_room::in_room;
pub use kind::Kind;
pub use may_act::may_act;
pub use outside::{OUTSIDE_NOTE, outside};
pub use refs::Refs;
pub use refusal::Refusal;
pub use ticket::Ticket;
pub use tool::{TOOLS, Tool, tool};
