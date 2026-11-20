//! A job's browser while it runs (BROWSER-24): its session, its daemon's and Chrome's PIDs, and its
//! place among the four live ones, given back when this is dropped. It owns that place and writes
//! nothing.

use tokio::sync::OwnedSemaphorePermit;

use super::Session;

/// A job's browser while it runs.
#[derive(Debug)]
pub struct Live {
    pub session: Session,
    pub daemon: Option<u32>,
    pub chrome: Option<u32>,
    /// Its place among the live browsers: dropped with it.
    pub _place: OwnedSemaphorePermit,
}
