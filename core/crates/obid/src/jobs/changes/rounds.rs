//! A task's rounds (docs/contracts/task-changes.md, CHANGES-1 and 2): it owns `Job::checkpoints`,
//! each an agent's turn on an owner's job that moved its worktree's HEAD. It never writes a
//! worktree or a branch; taking a round back is [`undo`]'s.

pub(crate) mod undo;

use crate::job::Checkpoint;
use crate::jobs::*;

/// One round of a task, numbered from its first job's first (CHANGES-1).
pub(crate) struct Round {
    pub number: usize,
    pub job: String,
    pub from: String,
    pub to: String,
    pub at: i64,
}

impl Round {
    pub(crate) fn to_json(&self) -> Value {
        json!({"round": self.number, "job": self.job, "from": self.from, "to": self.to,
               "at": obi_protocol::time::format(self.at)})
    }
}

impl Jobs {
    /// Keeps the round that just ended on the owner's job `id` (CHANGES-1): when its worktree's
    /// HEAD moved from `before`, the turn that started at `started` made one.
    pub(crate) async fn checkpoint(&self, id: &str, before: &str, started: i64) {
        if !git::is_sha(before) {
            return;
        }
        let Ok(after) = self
            .sandboxed_git(id, &["rev-parse", "--verify", "HEAD"], None)
            .await
        else {
            return;
        };
        let after = String::from_utf8_lossy(&after).trim().to_owned();
        if after == before || !git::is_sha(&after) {
            return;
        }
        self.update(id, |job| {
            job.checkpoints.push(Checkpoint {
                from: before.to_owned(),
                to: after,
                at: started,
            })
        });
    }

    /// The task's rounds as job `job`'s HEAD `head` has them, oldest first and numbered: its own
    /// and the jobs' it follows up on its branch, each a round whose end is still on `head`
    /// (CHANGES-1). A round a rebase rewrote, or one undone, isn't one any more.
    pub(crate) async fn rounds_at(&self, job: &Job, head: &str) -> Vec<Round> {
        let mut rounds = Vec::new();
        for of in self.task_chain(job) {
            for c in &of.checkpoints {
                if c.to == head || self.is_ancestor(&job.id, &c.to, head).await {
                    rounds.push(Round {
                        number: rounds.len() + 1,
                        job: of.id.clone(),
                        from: c.from.clone(),
                        to: c.to.clone(),
                        at: c.at,
                    });
                }
            }
        }
        rounds
    }

    /// `job` and the jobs it follows up whose branch it started from, oldest first.
    fn task_chain(&self, job: &Job) -> Vec<Job> {
        let state = self.lock();
        let mut chain = vec![job.clone()];
        while let Some(parent) = chain
            .last()
            .and_then(|j| j.follow_up_of.as_ref().map(|p| (j, p)))
            .and_then(|(j, p)| {
                state.jobs.get(p).filter(|p| {
                    p.repo == j.repo && j.start_from.as_deref() == Some(p.branch.as_str())
                })
            })
        {
            if chain.iter().any(|j| j.id == parent.id) {
                break;
            }
            chain.push(parent.clone());
        }
        chain.reverse();
        chain
    }

    /// Whether `commit` is `of` or comes before it, in job `id`'s worktree.
    pub(super) async fn is_ancestor(&self, id: &str, commit: &str, of: &str) -> bool {
        git::is_sha(commit)
            && (commit == of
                || self
                    .sandboxed_git(id, &["merge-base", "--is-ancestor", commit, of], None)
                    .await
                    .is_ok())
    }
}
