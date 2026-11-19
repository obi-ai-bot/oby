//! The local servers a running job's agent started (docs/contracts/task-servers.md, TASKSERVERS-1
//! to 4): the TCP ports its own processes listen on that this Mac reaches as `localhost`, read on
//! the meter's tick (crate::jobs::metering) from the agent's process tree alone (crate::meter,
//! crate::listening). It owns `Job.servers`, set only while the job's worker runs and cleared when
//! it ends, never saved in the job's file; a change is the job's `job` event. It starts, stops,
//! opens and publishes nothing, and reads no process outside a running job's agent's tree.

use super::*;
use crate::job::Server;
use crate::listening::Listening;

impl Jobs {
    /// What each running job's agent listens on now, as the meter read it: each job's id and its
    /// processes' listens. A job whose list changed gets it, and its event; a job whose worker
    /// ended meanwhile gets none.
    pub(super) fn record_servers(&self, found: Vec<(String, Vec<Listening>)>) {
        let now = obi_protocol::time::now();
        for (id, listens) in found {
            let had = {
                let state = self.lock();
                if !state.workers.contains_key(&id) {
                    continue;
                }
                let Some(job) = state.jobs.get(&id) else {
                    continue;
                };
                job.servers.clone()
            };
            let servers = servers_now(&had, &listens, now);
            if servers == had {
                continue;
            }
            self.update(&id, |job| job.servers = servers);
            // Its worker ended between the read and the change: what it set goes too (TASKSERVERS-3).
            if !self.lock().workers.contains_key(&id) {
                self.servers_ended(&id);
            }
        }
    }

    /// Job `id`'s worker ended: none of its servers is its any more (TASKSERVERS-3).
    pub(super) fn servers_ended(&self, id: &str) {
        if self.get(id).is_some_and(|job| !job.servers.is_empty()) {
            self.update(id, |job| job.servers.clear());
        }
    }
}

/// A job's servers now, one a port in order, from what its processes listen on: a port seen
/// before keeps when it was first seen, a new one is seen `now`, and one no longer listened on is
/// gone (TASKSERVERS-2). A port several processes listen on is named after the lowest process id.
fn servers_now(had: &[Server], listens: &[Listening], now: i64) -> Vec<Server> {
    let mut first: std::collections::BTreeMap<u16, &Listening> = std::collections::BTreeMap::new();
    for listen in listens {
        let kept = first.entry(listen.port).or_insert(listen);
        if listen.pid < kept.pid {
            *kept = listen;
        }
    }
    first
        .into_iter()
        .map(|(port, listen)| Server {
            port,
            process: listen.process.clone(),
            first_seen: had
                .iter()
                .find(|s| s.port == port)
                .map_or(now, |s| s.first_seen),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen(pid: u32, process: &str, port: u16) -> Listening {
        Listening {
            pid,
            process: process.into(),
            port,
        }
    }

    /// TASKSERVERS-2: a port keeps when it was first seen, a new one is seen now, a closed one goes.
    #[test]
    fn taskservers_2_first_seen_is_kept_and_closed_ports_go() {
        let first = servers_now(
            &[],
            &[
                listen(9, "node", 5173),
                listen(7, "vite", 5173),
                listen(8, "node", 3000),
            ],
            100,
        );
        assert_eq!(
            first,
            [
                Server {
                    port: 3000,
                    process: "node".into(),
                    first_seen: 100
                },
                Server {
                    port: 5173,
                    process: "vite".into(),
                    first_seen: 100
                },
            ]
        );
        let later = servers_now(
            &first,
            &[listen(7, "vite", 5173), listen(10, "python3", 8000)],
            160,
        );
        assert_eq!(
            later,
            [
                Server {
                    port: 5173,
                    process: "vite".into(),
                    first_seen: 100
                },
                Server {
                    port: 8000,
                    process: "python3".into(),
                    first_seen: 160
                },
            ]
        );
        assert_eq!(servers_now(&later, &[], 200), []);
    }
}
