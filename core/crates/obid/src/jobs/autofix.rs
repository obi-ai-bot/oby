//! AUTOFIX, the dispatcher's side (docs/contracts/autofix.md, AUTOFIX-1 to 20): when the owner's
//! own task's pull request reads red, the same task's next step, decided by crate::autofix on
//! typed facts. A rerun on GitHub with no agent turn, one more round of its own agent in its own
//! session (the failure quoted as data), the merge queue again, or, past its limits, one line and
//! the card. It writes the job's `autofix` state, its `heard` and `step` through
//! [`Jobs::fix_round`], and `outgoing` for a rerun; never the branch (the round's publish pushes)
//! and never what the agent fixes.

use super::*;

use crate::autofix::{GaveUp, Red, Step, give_up_line};
use crate::github::{Dropped, Merging};

/// What the conversation hears when a fix round starts, in obid's words until Sangam's screen has
/// them: the round and how many it may have.
fn fixing_line(round: u32, of: u32, queue: bool) -> String {
    match queue {
        false => format!("Its checks failed on GitHub. Fixing them: round {round} of {of}."),
        true => format!(
            "It failed its checks in GitHub's merge queue. Fixing them: round {round} of {of}."
        ),
    }
}

/// The merge queue's answer to a drop (AUTOFIX-11, 12).
pub(super) enum Queue {
    /// Back in: armed again at the same head.
    Again,
    /// A fix round started, or a rerun settling: nothing more this pass.
    Working,
    /// Over to the owner, why.
    GaveUp(String),
}

impl Jobs {
    /// Whether the job `id`'s red checks are AUTOFIX's: the owner's own work (MERGING-44), with
    /// `autoFixChecks` on. Anything else keeps MERGING-39 (AUTOFIX-16).
    fn autofixes(&self, job: &Job) -> bool {
        crate::merge::own_work(job) && self.auto_fix_checks()
    }

    /// `autoFixChecks` (`PUT /v1/settings/jobs`), on unless the owner turned it off (AUTOFIX-4).
    pub fn auto_fix_checks(&self) -> bool {
        crate::settings::load(&self.config.home).auto_fix_checks()
    }

    /// What a read that stopped on `stop` at `head` does before any card: red checks of the
    /// owner's own work go to AUTOFIX ([`Self::autofix`]); the rest to MERGING-39 and 40's one
    /// round ([`Self::red_round`]). Whether nothing waits on the owner now.
    pub(super) async fn after_red(
        self: &Arc<Self>,
        id: &str,
        gh: &Path,
        head: &str,
        stop: &Option<String>,
        merging: &Merging,
    ) -> bool {
        let red = stop.as_deref() == Some(crate::merge::RED_CHECKS);
        match self.get(id).filter(|job| red && self.autofixes(job)) {
            Some(job) => self.autofix(&job, gh, head, merging).await,
            None => self.red_round(id, gh, head, stop, merging).await,
        }
    }

    /// AUTOFIX-1 to 10: the owner's own task `job`'s pull request is red at `head`, or the owner's
    /// own pull request a Fix CI task pushed onto (AUTOFIX-14). While its round works, nothing;
    /// else crate::autofix decides: a rerun on GitHub, a fix round, or giving up, which says so
    /// once and leaves the card. Whether nothing waits on the owner now.
    pub(super) async fn autofix(
        self: &Arc<Self>,
        job: &Job,
        gh: &Path,
        head: &str,
        merging: &Merging,
    ) -> bool {
        let id = job.id.as_str();
        if job.status.is_active() {
            return true;
        }
        // Someone else's push is theirs (MERGING-25): no round, the card as before (AUTOFIX-15).
        if onto_pr::head(job) != Some(head) {
            return false;
        }
        let red = Red {
            head,
            failed: &merging.failed,
            infra: merging.infra,
            checked_here: job.checked.as_ref().is_some_and(|c| c.commit == head),
            now: obi_protocol::time::now(),
        };
        let mut step = Step::Wait;
        self.update(id, |job| step = job.autofix.next(&red));
        match step {
            Step::Wait => true,
            Step::GiveUp(why) => {
                self.gave_up(id, head, why);
                false
            }
            Step::Rerun => match crate::github::rerun_failed(gh, &job.repo, head).await {
                Ok(runs) => {
                    self.update(id, |job| {
                        job.autofix.reran(head, red.now);
                        for run in &runs {
                            let sent = crate::publish::Outgoing::new(
                                "checksRerun",
                                &job.repo,
                                &run.to_string(),
                                None,
                            );
                            job.outgoing.push(sent);
                        }
                        job.autofix.say(match merging.infra {
                            true => "Its checks stopped on GitHub's side. Running them again there.",
                            false => "Its checks failed on GitHub, but passed here. Running them again there once before fixing anything.",
                        });
                        job.merge_needs_you = false;
                        job.merge_error = None;
                    });
                    tracing::info!("job {id}: reran {runs:?} at {head} (AUTOFIX)");
                    true
                }
                Err(why) => {
                    // Rerun refused: the head counts as rerun, so its next read fixes instead.
                    tracing::warn!("job {id}: its checks weren't rerun at {head}: {why}");
                    self.update(id, |job| {
                        let past = red.now - crate::autofix::rerun_settle();
                        while job.autofix.next(&red) == Step::Rerun {
                            job.autofix.reran(head, past);
                        }
                    });
                    true
                }
            },
            Step::Fix => {
                // What failed, its log too: the same failure after a fix round gives up (AUTOFIX-9).
                let text = crate::github::failed_logs(gh, &job.repo, head, &merging.failed).await;
                let failed = crate::autofix::failure(&merging.failed, &text);
                let red = Red {
                    failed: &failed,
                    ..red
                };
                self.update(id, |job| step = job.autofix.next(&red));
                if let Step::GiveUp(why) = step {
                    self.gave_up(id, head, why);
                    return false;
                }
                self.fix(id, head, &failed, &text, after_pr::CHECKS_PROMPT, false)
            }
        }
    }

    /// One fix round at `head` (AUTOFIX-1, 11): `text` (what GitHub logged) quoted as data into
    /// `checks.md`, counted, its line said, then its agent's round with `prompt`. When it can't
    /// start (outside MERGING-38's bounds, the vault unreadable), it gives up. Whether it started.
    fn fix(
        self: &Arc<Self>,
        id: &str,
        head: &str,
        failed: &[String],
        text: &str,
        prompt: &'static str,
        queue: bool,
    ) -> bool {
        let quoted = self.may_fix(id, head)
            && self.quote_for_round(
                id,
                head,
                "checks.md",
                (
                    "GitHub's failed checks",
                    "GitHub's logs of the failed checks",
                ),
                text,
            );
        if !quoted {
            self.gave_up(id, head, GaveUp::NoRound);
            return false;
        }
        self.update(id, |job| {
            job.autofix.fixing(head, failed);
            let (round, of) = (job.autofix.fixes, job.autofix.allowed());
            job.autofix.say(fixing_line(round, of, queue));
        });
        let started = self.fix_round(id, head, prompt);
        if !started {
            self.gave_up(id, head, GaveUp::NoRound);
        }
        started
    }

    /// Gives up at `head` for `why`: its line said once (AUTOFIX-10); the card follows from the
    /// pass, with `One more round` on it ([`Self::one_more_card`]).
    fn gave_up(&self, id: &str, head: &str, why: GaveUp) {
        self.update(id, |job| {
            if job.autofix.give_up(head, why) {
                let line = give_up_line(why, job.autofix.allowed());
                job.autofix.say(line);
            }
        });
    }

    /// A green read of the job `id`'s pull request: red reads count afresh, and one AUTOFIX acted
    /// on says it passes now (AUTOFIX-2).
    pub(super) fn autofix_green(&self, id: &str, merging: &Merging) {
        if merging.checks != Some("SUCCESS") {
            return;
        }
        let red = self.get(id).is_some_and(|j| j.autofix.red_since.is_some());
        if !red {
            return;
        }
        self.update(id, |job| {
            let acted = job.autofix.fixes > 0 || !job.autofix.reruns.is_empty();
            if job.autofix.red_since.take().is_some() && acted {
                job.autofix.say("Its checks pass on GitHub now.");
            }
        });
    }

    /// AUTOFIX-11, 12: the job's pull request at `head`, which obid handed to GitHub's merge queue
    /// `since` then, read again. When the queue took it out on a failed run ([`crate::github::dropped`]),
    /// another entry's failure, or its runner's, puts it back in (at most
    /// [`crate::autofix::REQUEUES_PER_HEAD`] times), and its own failure gets a fix round on that
    /// run's log. None while it's still queued, or for work that isn't the owner's own.
    pub(super) async fn queue_dropped(
        self: &Arc<Self>,
        job: &Job,
        gh: &Path,
        (base, number): (&str, u64),
        head: &str,
        since: i64,
    ) -> Option<Queue> {
        if !self.autofixes(job) || job.status.is_active() || job.pushed_head() != Some(head) {
            return None;
        }
        let now = obi_protocol::time::now();
        if now - since < crate::autofix::rerun_settle() {
            return None;
        }
        let runs = crate::github::queue_runs(gh, &job.repo).await.ok()?;
        let dropped = crate::github::dropped(&runs, base, number, since)?;
        let id = job.id.as_str();
        tracing::info!("job {id}: GitHub's merge queue took {head} out: {dropped:?}");
        match dropped {
            Dropped::Another(_) | Dropped::Runner => {
                let line = match dropped {
                    Dropped::Another(Some(n)) => format!(
                        "Back in GitHub's merge queue: #{n}, queued ahead of it, failed its checks."
                    ),
                    Dropped::Another(None) => "Back in GitHub's merge queue: a change queued ahead of it failed its checks.".into(),
                    _ => "Back in GitHub's merge queue: its run there stopped on GitHub's side.".into(),
                };
                let mut again = false;
                self.update(id, |job| {
                    again = job.autofix.requeue(head);
                    if again {
                        job.auto_merge = None;
                        job.autofix.say(line);
                    }
                });
                if again {
                    return Some(Queue::Again);
                }
                self.gave_up(id, head, GaveUp::Queue);
                Some(Queue::GaveUp(give_up_line(GaveUp::Queue, 0)))
            }
            Dropped::Own(run) => {
                let failed = vec![format!("merge queue {base}")];
                let red = Red {
                    head,
                    failed: &failed,
                    infra: false,
                    checked_here: false,
                    now,
                };
                let mut step = Step::Wait;
                self.update(id, |job| step = job.autofix.next(&red));
                match step {
                    Step::Fix => {
                        let logs = crate::github::run_logs(gh, &job.repo, &[run]).await;
                        let text = match logs.is_empty() {
                            true => format!("No log of run {run} could be read."),
                            false => logs.join("\n\n"),
                        };
                        let failed = crate::autofix::failure(&failed, &text);
                        let prompt = after_pr::QUEUE_PROMPT;
                        self.update(id, |job| job.auto_merge = None);
                        if self.fix(id, head, &failed, &text, prompt, true) {
                            return Some(Queue::Working);
                        }
                        let why = (self.get(id).and_then(|j| j.autofix.gave_up))
                            .map_or(GaveUp::NoRound, |(_, why)| why);
                        Some(Queue::GaveUp(give_up_line(why, 0)))
                    }
                    Step::GiveUp(why) => {
                        self.gave_up(id, head, why);
                        let allowed = self.get(id).map_or(0, |j| j.autofix.allowed());
                        Some(Queue::GaveUp(give_up_line(why, allowed)))
                    }
                    Step::Wait | Step::Rerun => Some(Queue::Working),
                }
            }
        }
    }

    /// The owner's `One more round` on the card AUTOFIX left at `head` (AUTOFIX-10): as many fix
    /// rounds again, and the follower reads it at once. Whether it was the job's to take.
    pub fn one_more_round(&self, id: &str, head: &str) -> bool {
        let mut taken = false;
        self.update(id, |job| {
            if job.autofix.gave_up.as_ref().is_some_and(|(h, _)| h == head) {
                job.autofix.more();
                job.autofix.say("One more round, as you asked.");
                job.merge_needs_you = false;
                job.merge_error = None;
                taken = true;
            }
        });
        if taken {
            self.follow.notify_one();
        }
        taken
    }

    /// The card the pass offers for the job `id` (`card`), with `One more round` first when
    /// AUTOFIX gave up at its head (AUTOFIX-10).
    pub(super) fn one_more_card(
        &self,
        id: &str,
        card: Option<crate::sphere::inbox::Item>,
    ) -> Option<crate::sphere::inbox::Item> {
        let mut card = card?;
        let head = crate::merge::item_head(&card.id).map(str::to_owned);
        let gave_up = self.get(id).and_then(|j| j.autofix.gave_up);
        let here = gave_up.is_some_and(|(h, _)| Some(h) == head);
        if here
            && !card
                .options
                .iter()
                .any(|o| o == crate::merge::ONE_MORE_ROUND)
        {
            card.options.insert(0, crate::merge::ONE_MORE_ROUND.into());
            let more = format!(
                "Oby fixes its checks for {} more rounds by itself.",
                crate::autofix::FIX_ROUNDS
            );
            if let Some(details) = card.details.as_mut() {
                details.insert(0, more);
            }
        }
        Some(card)
    }

    /// The conversation was told `told` more of the job `id`'s AUTOFIX lines (crate::conversation).
    pub fn mark_autofix_told(&self, id: &str, told: u32) {
        self.update(id, |job| {
            job.autofix.told = job.autofix.told.saturating_add(told)
        });
    }
}
