//! Oby's browser, T3 Code's design ([docs/plans/browser-t3.md](../../../../../docs/plans/browser-t3.md)):
//! every agent tab is a `<webview>` in Oby's window, the person uses it as any page, and obid
//! drives it for the agent over the window's relay in page-level CDP. What a page says reaches the
//! agent as quoted outside data. obid downloads, installs and runs no browser of its own, and with
//! no window an agent's page tool is refused `no_window`: there is no fallback. Its contract is
//! [docs/contracts/browser.md](../../../../../docs/contracts/browser.md).
//!
//! - [`engine`]: the tool surface, tab ownership per job, refs that die on navigation or a new
//!   snapshot, per-command deadlines never replayed after a timeout, page data marked as outside
//!   data, and the download rule. Pure: no IO and no clock of its own.
//! - [`contract`]: the wire types, routes and reasons the browser's screens, the window and the
//!   agents' tools talk to, add-only like the rest of obid's API.
//! - [`api`]: the browser's API over the engine: asks, downloads, settings and the event log.
//! - [`relay`]: the window's relay, obid's half: its commands, its events, and the CDP calls
//!   waiting for their answers.
//! - [`window`]: the window's browser: the engine's commands said in CDP over the relay, a page's
//!   accessibility snapshot, an agent's downloads in the job's own folder.
//!
//! [`Service`] serves them, created at obid's start and told when a job ends; its [`routes`] are
//! the contract's paths under `/v1/browser` and the agents' MCP server (`POST /v1/browser/mcp`).
//! [`AgentServer`] is that server as each agent's configuration names it, with its permission
//! rules (BROWSER-17).

pub mod agent_server;
pub mod api;
pub mod contract;
pub mod engine;
pub mod relay;
pub mod routes;
pub mod send_buffer;
mod service;
mod tools;
pub mod window;

pub use agent_server::AgentServer;
pub use service::Service;
