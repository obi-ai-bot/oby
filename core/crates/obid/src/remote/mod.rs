//! Other computers (docs/contracts/remote.md, COMPUTERS-1 onward; the owner's decisions of
//! 2026-11-19, docs/decisions/2026-11-19-other-computers.md): the owner's window on this Mac uses
//! another Mac's or Linux box's obid, as T3 Code's environments. It reaches it over any network
//! the person already has (a direct address, always HTTPS with its certificate pinned) or an SSH
//! tunnel (`ssh -N -L` to that obid's own 127.0.0.1), never through the Obysphere, and moves
//! between those routes by itself as they come and go.
//!
//! Ported from the lab, where it was proven first (tools/obid-spec/src/lab/remote.rs,
//! `computers`; workflows/lab/remote/). The dispatcher's: which computer and which route, never
//! what an agent does there.
//!
//! - [`route`]: what a route is, what kind its address is, and the owner's order.
//! - [`pin`]: a direct address's certificate, pinned by its SHA-256.
//! - [`link`]: what the other computer shows to be added without SSH.
//! - [`supervisor`]: the route the window uses, and when it moves.
//! - [`ssh`] and [`trouble`]: the person's own ssh as a tunnel only, and what its errors mean.
//! - [`pair`]: `obid pair-computer`, which the person's SSH runs on the other computer.
//! - [`addresses`]: where this obid answers, kept for `obid pair-computer`.
//!
//! It owns `home/addresses.json` (written once each start) and writes `home/pairing` only from
//! `obid pair-computer`, as the person's own presence on that machine. It never writes
//! `~/.ssh`, the person's ssh configuration or `known_hosts`, and installs nothing over SSH.

pub mod addresses;
pub mod link;
pub mod pair;
pub mod pin;
pub mod route;
pub mod ssh;
pub mod supervisor;
pub mod trouble;
