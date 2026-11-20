//! Oby's browser: agents drive a browser of their own, the person watches it and takes a tab
//! over, and what a page says reaches the agent as quoted outside data. Its contract is
//! [docs/contracts/browser.md](../../../../../docs/contracts/browser.md) (BROWSER-1 to 25).
//!
//! This module is the browser, ported from the lab (`tools/obid-spec/src/lab/`,
//! `browser.rs`, `browser_contract.rs` and `browserapi.rs`) where it was
//! built and proven:
//!
//! - [`engine`]: the tool surface, tab ownership per conversation and take control, refs that die
//!   on navigation, a new snapshot or a handover, per-command deadlines never replayed after a
//!   timeout, page data marked as outside data, the download rule and the latest-frame slot.
//! - [`contract`]: the wire types, routes and reasons the browser's screens and the agents' tools
//!   talk to, add-only like the rest of obid's API.
//! - [`api`]: the browser's API over the engine: holds with their generation, asks, downloads,
//!   settings and the event log, and bringing a job's browser up within its own allowance.
//! - [`install`]: agent-browser and chrome-headless-shell downloaded on first use into obid's home,
//!   each checked against its pinned SHA-256 before anything of it is unpacked (BROWSER-23).
//! - [`driver`]: one agent-browser daemon and its Chrome per job, at most four live, run per the
//!   core's tickets under their deadlines, stopped at the job's end and reaped at obid's start
//!   (BROWSER-24, 25), its downloads and its live stream.
//!
//! The core ([`engine`], [`contract`], [`api`]) is pure: no IO, no files, no processes, no clock of
//! its own (the caller passes the time, and [`api::browser_up`] waits on tokio's clock only). It
//! keeps every rule on; the lab's switches that turn one off for a red proof stay in the lab.
//! [`install`] and [`driver`] are where files and processes are. [`Service`] serves them: the API
//! core, the driver and the events stream, created at obid's start, told when a job ends; its
//! [`routes`] are the contract's paths under `/v1/browser` and the agents' MCP server
//! (`POST /v1/browser/mcp`), and its tools run the agents' calls over the driver.
//! [`AgentServer`] is that server as each agent's configuration names it, with its permission
//! rules (BROWSER-17): Cursor's jobs are given it; Claude Code's and Codex's come next.

pub mod agent_server;
pub mod api;
pub mod contract;
pub mod driver;
pub mod engine;
pub mod install;
pub mod routes;
pub mod send_buffer;
mod service;
mod tools;

pub use agent_server::AgentServer;
pub use service::{SWEEP_EVERY, Service};
