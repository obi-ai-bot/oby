//! The jobs' work a restart of obid would cut (crate::restart, docs/contracts/engine.md, ENGINE-13).

use super::*;
use crate::restart::{Kind, Waiting};

impl Jobs {
    /// Each job a worker runs, waits on or runs its check, has a prompt waiting for the owner, or
    /// publishes: one entry each, read under one lock.
    pub fn in_flight(&self) -> Vec<Waiting> {
        let state = self.lock();
        let mut waiting = Vec::new();
        for id in state.workers.keys() {
            let kind = if state.idle.contains(id) {
                Kind::Check
            } else {
                Kind::Job
            };
            waiting.push(Waiting::new(kind, id));
            let asks = state.asks.get(id).is_some_and(|a| !a.is_empty());
            if asks || state.held.get(id).is_some_and(|h| !h.is_empty()) {
                waiting.push(Waiting::new(Kind::Ask, id));
            }
        }
        let publishing = state.publishing.iter();
        waiting.extend(publishing.map(|id| Waiting::new(Kind::Publish, id)));
        waiting
    }
}
