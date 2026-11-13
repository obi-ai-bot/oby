//! A job's tool version, stamped as each of its rounds starts (docs/contracts/cli-versions.md,
//! CLIVERSIONS-3): read in the background, so a round never waits on it or fails for it.

use super::*;

impl Jobs {
    /// Sets job `id`'s `agentVersion` to what `tool`'s binary says it is, once read: none when it
    /// can't be read, and nothing once the job moved to another tool. A change from its earlier
    /// round's is one `start` line (CLIVERSIONS-6), decided under the job's lock so overlapping
    /// reads say it once.
    pub(super) fn stamp_version(self: &Arc<Self>, id: &str, tool: Tool) {
        let (jobs, id) = (self.clone(), id.to_owned());
        let bin = crate::accounts::bin_of(&self.config.agents.get(), tool).to_owned();
        tokio::spawn(async move {
            let read =
                tokio::task::spawn_blocking(move || crate::cli_versions::read(tool, &bin, true));
            let version = read.await.ok().and_then(|read| read.version);
            let mut line = None;
            jobs.update(&id, |job| {
                if job.agent == tool.id() {
                    let was = job.agent_version.take();
                    line = crate::cli_versions::changed(tool, was.as_deref(), version.as_deref());
                    job.agent_version = version;
                }
            });
            if let Some(line) = line {
                jobs.transcribed(&id, jobs.transcripts.start(&id, &line));
            }
        });
    }
}
