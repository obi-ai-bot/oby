//! A job's launch waiting for one of the four live browsers to end (BROWSER-24), counted while it
//! waits, and no longer once it got in or gave up. It owns only that count's share and writes
//! nothing.

use std::sync::atomic::{AtomicUsize, Ordering};

/// A launch waiting for a live browser to end, counted while it waits.
pub struct Waiting<'a>(&'a AtomicUsize);

impl<'a> Waiting<'a> {
    pub fn new(count: &'a AtomicUsize) -> Waiting<'a> {
        count.fetch_add(1, Ordering::SeqCst);
        Waiting(count)
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
