//! What a job reaches beyond its worktree that ends with it (docs/contracts/browser.md,
//! BROWSER-25): its broker token (crate::connections), which every obid server for agents takes,
//! and its browser (crate::browser), whose daemon and Chrome stop and whose tabs leave the list. It
//! owns the handles to both, set once at obid's start; it writes nothing itself, each owner ending
//! its own.

use super::*;

impl Jobs {
    pub fn use_connections(&self, connections: Arc<crate::connections::Connections>) {
        let _ = self.connections.set(connections);
    }

    /// Oby's browser, told when a job ends (once; without it, a job has no browser to end).
    pub fn use_browser(&self, browser: Arc<crate::browser::Service>) {
        let _ = self.browser.set(browser);
    }

    /// Job `id` ended: its broker token dies, so no server for agents answers it again, then its
    /// browser stops (BROWSER-25).
    pub(super) async fn end_reach(&self, id: &str) {
        if let Some(connections) = self.connections.get() {
            connections.close_job(id).await;
        }
        if let Some(browser) = self.browser.get() {
            browser.end_job(id).await;
        }
    }
}
