//! What a job reaches beyond its worktree that ends with it (docs/contracts/browser.md,
//! BROWSER-17, 18, 25): its broker token (crate::connections), which every obid server for agents
//! takes, and its browser (crate::browser), given to its agent as obid's browser server while the
//! owner lets agents use it, whose daemon and Chrome stop and whose tabs leave the list. A foreign
//! job's box partition gets its own egress door, the browser's (crate::egress), handed to the
//! window on the relay and forgotten there when the job ends; the door itself closes with the
//! job's others. It owns the handles to both, set once at obid's start; it writes nothing itself,
//! each owner ending its own.

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
    /// can use the browser" is on. An own run reaches it on loopback, as Oby's other servers
    /// (LANES-26). Another Oby's task, one marked `untrusted` or Oby's own review browses in its
    /// box partition (BROWSER-18): a boxed run behind obid's egress proxy gets the job's browser
    /// door, which the window sets on that partition, and the host's listeners it refuses. Any
    /// other run opens no door, so its box pages reach nothing (the window holds their requests,
    /// then cancels them).
    pub(super) fn browser_server(&self, job: &Job) -> Option<crate::browser::AgentServer> {
        use crate::browser::box_edge;
        let origin = box_edge::origin_of(job, &Value::Null, |id| self.get(id));
        let browser = self.browser.get()?;
        browser.api(|api| api.tools_allowed()).ok()?;
        let server = self.connections.get()?.open_browser(&job.id);
        let own =
            matches!(self.job_account(job), Ok((tool, a)) if lanes::launches_own(job, tool, &a));
        // Its worktree as it is now, before its agent starts: its uploads are read only from it.
        if origin.foreign()
            && let Some(worktree) = &job.worktree
        {
            browser.keep_room(&job.id, std::path::Path::new(worktree));
        }
        if origin.foreign()
            && !own
            && self.confines_egress()
            && let Some(egress) = self.egress.get()
        {
            let proxy = egress.open_browser(&job.id, browser.host_ports());
            let partition = origin.partition(&job.id, &job.agent);
            browser.relay().set_box(&job.id, &partition, &proxy);
        }
        let url = Self::agent_api(server.url(), own);
        Some(server.reached_at(url))
    }

    /// Job `id` ended: its broker token dies, so no server for agents answers it again, then its
    /// browser stops and the window forgets its box's door (BROWSER-18, 25).
    pub(super) async fn end_reach(&self, id: &str) {
        if let Some(connections) = self.connections.get() {
            connections.close_job(id).await;
        }
        if let Some(browser) = self.browser.get() {
            browser.relay().forget_box(id);
            browser.end_job(id).await;
        }
    }
}
