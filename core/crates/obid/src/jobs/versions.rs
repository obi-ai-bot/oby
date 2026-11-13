//! A job's tool version, stamped as each of its rounds starts (docs/contracts/cli-versions.md,
//! CLIVERSIONS-3): read in the background, so a round never waits on it or fails for it.

use super::*;

impl Jobs {
    /// Sets job `id`'s `agentVersion` to what `tool`'s binary says it is, once read: none when it
    /// can't be read.
    pub(super) fn stamp_version(self: &Arc<Self>, id: &str, tool: Tool) {
        let (jobs, id) = (self.clone(), id.to_owned());
        let bin = crate::accounts::bin_of(&self.config.agents.get(), tool).to_owned();
        tokio::spawn(async move {
            let read =
                tokio::task::spawn_blocking(move || crate::cli_versions::read(tool, &bin, true));
            let version = read.await.ok().and_then(|read| read.version);
            jobs.update(&id, |job| job.agent_version = version);
        });
    }
}
