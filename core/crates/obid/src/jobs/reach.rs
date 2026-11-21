//! What a job reaches beyond its worktree that ends with it (docs/contracts/browser.md,
//! BROWSER-17, 25): its broker token (crate::connections), which every obid server for agents
//! takes, and its browser (crate::browser), given to its agent as obid's browser server while the
//! owner lets agents use it, whose daemon and Chrome stop and whose tabs leave the list. It owns
//! the handles to both, set once at obid's start; it writes nothing itself, each owner ending its
//! own.

use super::*;

impl Jobs {
    pub fn use_connections(&self, connections: Arc<crate::connections::Connections>) {
        let _ = self.connections.set(connections);
    }

    /// Oby's browser, told when a job ends (once; without it, a job has no browser to end).
    pub fn use_browser(&self, browser: Arc<crate::browser::Service>) {
        let _ = self.browser.set(browser);
    }

    /// obid's browser server for `job`'s agent, with its broker token (BROWSER-17), while "Agents
    /// can use the browser" is on: for the owner's own work only. Another Oby's task, or one
    /// marked `untrusted`, gets none until its browser runs in Oby's box (BROWSER-18).
    pub(super) fn browser_server(&self, job: &Job) -> Option<crate::browser::AgentServer> {
        if job.obi.is_some() || job.untrusted {
            return None;
        }
        let browser = self.browser.get()?;
        browser.api(|api| api.tools_allowed()).ok()?;
        Some(self.connections.get()?.open_browser(&job.id))
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
