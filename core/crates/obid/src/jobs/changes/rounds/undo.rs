//! Undo this round (docs/contracts/task-changes.md, CHANGES-8 to 12): taking a job's last round
//! back. It owns `Job::undone` and pops `Job::checkpoints`; it writes only the job it undoes, and
//! pushes only through crate::publish, to that job's own `oby/` branch, never a new commit.

use super::*;

/// What Undo this round would do (`GET /v1/jobs/{id}/undo`, CHANGES-8): obid's own sentence for
/// it, the commit the branch goes back to, the one it's at, and whether it's force-pushed.
#[derive(Debug, Clone, PartialEq)]
pub struct Undo {
    pub action: String,
    pub from: String,
    pub to: String,
    pub branch: String,
    pub push: bool,
}

impl Undo {
    pub fn to_json(&self) -> Value {
        json!({"action": self.action, "from": self.from, "to": self.to,
               "branch": self.branch, "push": self.push})
    }
}

/// Why a job has no round to undo (CHANGES-9).
const NOTHING: &str = "Nothing this task did is left to undo.";

impl Jobs {
    /// `GET /v1/jobs/{id}/undo`: what Undo this round would do to job `id`, or why it can't (a
    /// `409`); none when there's no such job (CHANGES-8 to 11).
    pub async fn undo_sentence(&self, id: &str) -> Option<Result<Undo, String>> {
        let job = self.get(id)?;
        Some(self.undo_of(&job).await)
    }

    async fn undo_of(&self, job: &Job) -> Result<Undo, String> {
        let round = job.checkpoints.last().ok_or(NOTHING)?;
        if job.status.is_active() {
            return Err(
                "It's still working: undo its last round once it ends, or stop it first.".into(),
            );
        }
        if job.merged() {
            return Err("Its pull request is merged, so its rounds are on the base now: undo them with a revert.".into());
        }
        let worktree = self.worktree_path(&job.id);
        if !worktree.join(".git").exists() {
            return Err("Its worktree is gone, so Oby can't put its branch back.".into());
        }
        let head = self
            .sandboxed_git(&job.id, &["rev-parse", "--verify", "HEAD"], None)
            .await
            .map(|h| String::from_utf8_lossy(&h).trim().to_owned())
            .unwrap_or_default();
        if head != round.to {
            return Err(
                "Its branch moved since its last round ended, so Oby leaves it as it is.".into(),
            );
        }
        let branch_ref = format!("refs/heads/{}", job.branch);
        if let Ok(kept) = self
            .sandboxed_git(
                &job.id,
                &["rev-parse", "--verify", "--quiet", &branch_ref],
                None,
            )
            .await
        {
            let kept = String::from_utf8_lossy(&kept).trim().to_owned();
            if kept != round.to && !self.is_ancestor(&job.id, &kept, &round.from).await {
                return Err(format!(
                    "{} moved past this round, by a later task: undo that one's round first.",
                    job.branch
                ));
            }
        }
        let pushed = crate::publish::BranchChain::of(job, &self.lock().jobs)
            .pushed()
            .map(str::to_owned);
        let push = match pushed.as_deref() {
            None => false,
            Some(p) if p == round.to => true,
            Some(p) if self.is_ancestor(&job.id, p, &round.from).await => false,
            Some(_) => {
                return Err(format!(
                    "What Oby pushed to {} last isn't this round's end, so Oby leaves it as it is.",
                    job.branch
                ));
            }
        };
        if push && !job.branch.starts_with("oby/") {
            return Err(format!(
                "This round is on {} on GitHub, a branch Oby didn't make, and Oby force-pushes only its own oby/ branches: undo it there with a revert.",
                job.branch
            ));
        }
        let range = format!("{}..{}", round.from, round.to);
        let commits = self
            .sandboxed_git(&job.id, &["rev-list", "--count", &range], None)
            .await
            .ok()
            .and_then(|n| String::from_utf8_lossy(&n).trim().parse::<u64>().ok())
            .unwrap_or(0);
        let dirty = self
            .sandboxed_git(
                &job.id,
                &["status", "--porcelain", "--untracked-files=normal"],
                None,
            )
            .await
            .is_ok_and(|s| !s.iter().all(u8::is_ascii_whitespace));
        let short = &round.from[..round.from.len().min(7)];
        let mut action = format!(
            "{} goes back to {short}, as it was before this round: its {commits} commit{} leave{} the branch",
            job.branch,
            if commits == 1 { "" } else { "s" },
            if commits == 1 { "s" } else { "" },
        );
        if dirty {
            action.push_str(", and what's left uncommitted in its worktree is dropped");
        }
        if push {
            action.push_str(&format!(
                ". Oby force-pushes {} to {short}, so its pull request loses {} too.",
                job.branch,
                if commits == 1 { "it" } else { "them" }
            ));
        } else {
            action.push_str(". Nothing is pushed.");
        }
        Ok(Undo {
            action,
            from: round.from.clone(),
            to: round.to.clone(),
            branch: job.branch.clone(),
            push,
        })
    }

    /// `POST /v1/jobs/{id}/undo`, once a device approved `action` (CHANGES-8, 10 and 11): only when
    /// it's still obid's own sentence for job `id`. Force-pushes first, when the round went out,
    /// so a push that fails leaves everything as it was; then puts the worktree and the clone's
    /// branch back. Answers the job, or the line saying why not.
    pub async fn undo(self: &Arc<Self>, id: &str, action: &str) -> Result<Job, String> {
        let job = self.get(id).ok_or("There's no such job.")?;
        let undo = self.undo_of(&job).await?;
        if undo.action != action {
            return Err(
                "This task changed since you were asked: look at Undo this round again.".into(),
            );
        }
        if undo.push {
            let target = self.publish_target(&job)?;
            let pushed = crate::publish::push_back(&undo.from, &job, &target, &undo.to).await;
            let _ = std::fs::remove_dir_all(&target.scratch);
            pushed.map_err(|f| f.line())?;
            let sent =
                crate::publish::Outgoing::new("push", &job.repo, &undo.from, Some(&job.branch));
            tracing::info!(
                "job {id}: sent push {} {} {} (undo)",
                job.repo,
                job.branch,
                undo.from
            );
            self.pushed(&sent);
            self.update(id, |j| j.outgoing.push(sent));
        }
        let branch_ref = format!("refs/heads/{}", job.branch);
        let kept = self
            .sandboxed_git(id, &["rev-parse", "--verify", "--quiet", &branch_ref], None)
            .await
            .is_ok_and(|k| String::from_utf8_lossy(&k).trim() == undo.to);
        self.sandboxed_git(id, &["reset", "--hard", "--quiet", &undo.from], None)
            .await
            .map_err(|e| format!("Oby couldn't put its worktree back: {e}"))?;
        // Only a branch that had this round's commits moves back; one behind them stays.
        if kept {
            self.keep_commits(id).await.map_err(|e| {
                format!(
                    "Its worktree is back at {}, but Oby couldn't move its branch: {e}",
                    undo.from
                )
            })?;
        }
        let range = format!("{}..{}", undo.from, undo.to);
        let gone = self
            .sandboxed_git(id, &["rev-list", "--count", &range], None)
            .await
            .ok()
            .and_then(|n| String::from_utf8_lossy(&n).trim().parse::<u64>().ok())
            .unwrap_or(0);
        self.update(id, |j| {
            j.undo_round();
            j.commits = j.commits.map(|c| c.saturating_sub(gone));
            if undo.push {
                j.pushed = j.pushed.map(|p| p.saturating_sub(gone));
            }
            if j.checked_head.as_deref() == Some(undo.to.as_str()) {
                j.checked_head = None;
            }
        })
        .ok_or_else(|| "There's no such job.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CHANGES-12: words to a task after an Undo start a follow-up whose agent hears that the owner
    /// undid its rounds and where its branch is now; before any undo it hears nothing of it.
    #[test]
    fn changes_12_a_follow_up_after_an_undo_hears_of_it() {
        let mut job = Job::new(
            "first".into(),
            "o/r".into(),
            "build".into(),
            "Build".into(),
            "claude".into(),
            "a".into(),
        );
        job.summary = "Added hello.txt.".into();
        let earlier = |job: &Job| follow_up(job, "Now this").earlier.unwrap_or_default();
        assert!(!earlier(&job).contains("undid"), "{}", earlier(&job));
        let round = |c: char| Checkpoint {
            from: c.to_string().repeat(40),
            to: "f".repeat(40),
            at: 0,
        };
        job.checkpoints = vec![round('a'), round('b')];
        job.undo_round();
        let heard = earlier(&job);
        assert!(
            heard.contains(&format!(
                "Its report: The owner undid one of its rounds: its branch is back at {}, and what that round committed is gone from it.\n\nAdded hello.txt.",
                "b".repeat(12)
            )),
            "{heard}"
        );
        // a second undo says so once, the report kept under it
        job.undo_round();
        assert_eq!(
            job.summary,
            format!(
                "The owner undid 2 of its rounds: its branch is back at {}, and what those rounds committed is gone from it.\n\nAdded hello.txt.",
                "a".repeat(12)
            )
        );
        assert!(job.checkpoints.is_empty());
    }
}
