//! A job's lifecycle (docs/contracts/job-state.md): what a command does to a job, its next status
//! and the side fields that go with it decided together by [`transition`] and written by
//! [`Change::apply`]. Both read only their arguments, never the clock, the disk, a lock or the log:
//! `Jobs::apply` (jobs.rs) runs them on the job it keeps.

use serde_json::Value;

use super::{Job, Phase, Status, TRY_AGAIN};

/// Why a job couldn't be made: a person can read it (`422`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Refused(pub String);

/// How the note a new session hears first begins (`handoff`, jobs/logins.rs): its handoff is
/// written then.
pub(crate) const TAKING_OVER: &str = "You are taking over this task from ";

/// Whether `job` waits to hand its work to a new session whose handoff isn't written.
pub(crate) fn hands_over(job: &Job) -> bool {
    job.heard.iter().any(|w| w.starts_with(TAKING_OVER))
}

/// The kind of a run's end that wasn't done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndKind {
    /// A dead end: nothing ran, or no session to pick up.
    Failed,
    /// It needs the owner.
    NeedsYou,
    /// Its account is full: it moved or waits.
    AccountLimit,
    /// Its agent asked the owner, and its run ended on its way to another account.
    Asked,
}

/// A run that didn't end done, as [`Command::RunEnded`] hears it: its kind and what travels beside
/// it.
#[derive(Debug, Clone)]
pub struct Ended {
    pub kind: EndKind,
    /// Why it stopped.
    pub reason: String,
    /// The one next step (`ONE_MORE_ROUND`, `TRY_AGAIN`), or none.
    pub next: Option<&'static str>,
    /// When obid tries it again by itself.
    pub resumes: Option<i64>,
    /// Another Obi's review result, written only on another Obi's job.
    pub review: Option<Value>,
    /// When it ended.
    pub at: i64,
}

/// What happens to a job.
#[derive(Debug, Clone)]
pub enum Command {
    /// One more round of an ended job's session (Try again, Oby's review asking for changes, One
    /// more round): queued for its work again, the round counted, how it last ended gone
    /// (JOBSTATE-7).
    NextRound,
    /// Its run ended, not done (JOBSTATE-3).
    RunEnded(Ended),
}

/// A field a new round clears ([`ROUND_CLEARS`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Finished,
    Dismissed,
    Archived,
    Limit,
    Reason,
    StoppedBy,
    NextStep,
    ResumesAt,
    Unjudged,
    AccountLimitBlocked,
    AccountLimitEpisode,
    AccountLimitNonce,
    AccountLimitReset,
    AccountLimitChoice,
    Stalled,
    Waiting,
}

/// What a new round clears: how the job last ended, and what it waited for (JOBSTATE-7).
const ROUND_CLEARS: [Field; 16] = [
    Field::Finished,
    Field::Dismissed,
    Field::Archived,
    Field::Limit,
    Field::Reason,
    Field::StoppedBy,
    Field::NextStep,
    Field::ResumesAt,
    Field::Unjudged,
    Field::AccountLimitBlocked,
    Field::AccountLimitEpisode,
    Field::AccountLimitNonce,
    Field::AccountLimitReset,
    Field::AccountLimitChoice,
    Field::Stalled,
    Field::Waiting,
];

impl Field {
    /// Empties this field of `job`.
    fn clear(self, job: &mut Job) {
        match self {
            Field::Finished => job.finished = None,
            Field::Dismissed => job.dismissed = None,
            Field::Archived => job.archived = None,
            Field::Limit => job.limit = None,
            Field::Reason => job.reason = None,
            Field::StoppedBy => job.stopped_by = None,
            Field::NextStep => job.next_step = None,
            Field::ResumesAt => job.resumes_at = None,
            Field::Unjudged => job.unjudged = None,
            Field::AccountLimitBlocked => job.account_limit_blocked = false,
            Field::AccountLimitEpisode => job.account_limit_episode = None,
            Field::AccountLimitNonce => job.account_limit_nonce = None,
            Field::AccountLimitReset => job.account_limit_reset = None,
            Field::AccountLimitChoice => job.account_limit_choice = None,
            Field::Stalled => job.stalled = None,
            Field::Waiting => job.waiting = None,
        }
    }
}

/// What [`transition`] decided for a job, which [`Change::apply`] writes.
#[derive(Debug)]
pub enum Change {
    /// The job stays as it was.
    Keep,
    /// Queued for its work in `round`, `rounds` its count, every field of [`ROUND_CLEARS`] empty.
    Round { round: u32, rounds: Option<u32> },
    /// Its run ended, not done: `status` is interrupted or failed.
    End {
        status: Status,
        /// Its summary from now on, or none to keep the agent's.
        summary: Option<String>,
        reason: String,
        next_step: Option<String>,
        resumes_at: Option<i64>,
        finished: i64,
        /// Its review result, for another Obi's job only.
        review_result: Option<Option<Value>>,
    },
}

/// What `cmd` does to `job`. Neither command refuses, in any status.
pub fn transition(job: &Job, cmd: &Command) -> Result<Change, Refused> {
    Ok(match cmd {
        Command::NextRound => Change::Round {
            round: job.current_round().saturating_add(1),
            rounds: job
                .rounds
                .map(|r| if r > 1 { r.saturating_add(1) } else { r }),
        },
        Command::RunEnded(end) => ended(job, end),
    })
}

/// How `job` ends after a run that didn't end done (`end`): interrupted or failed, and why.
fn ended(job: &Job, end: &Ended) -> Change {
    let (reason, next, resumes) = match end.kind {
        // It moved or waits (LIMITS-15), or Stop came as it ended (LIMITS-6).
        EndKind::AccountLimit => return Change::Keep,
        EndKind::Failed | EndKind::NeedsYou => (end.reason.clone(), end.next, end.resumes),
        // Only a stop or a shutdown ends a run that moved on, and one that asked waits: both are
        // handled before its end comes here.
        EndKind::Asked => (
            "It stopped while moving to another account.".to_owned(),
            Some(TRY_AGAIN),
            None,
        ),
    };
    // Needing the owner, with a session to pick up, at the cap or with its handoff to write
    // (LIMITS-17): the agent's summary stays (`summaries` has every round's), and why it stopped
    // is said apart. Else failed, its summary why; with a session, a failure keeps Try again
    // (RECOVERY-14).
    let owners_failed = end.kind == EndKind::Failed && job.obi.is_none();
    let needs_you = end.kind == EndKind::AccountLimit
        || hands_over(job)
        || (end.kind == EndKind::NeedsYou && (job.session.is_some() || job.recovery.spent()))
        || (owners_failed && job.session.is_some());
    let (status, summary, next_step, resumes_at) = if needs_you {
        let blank = job.summary.trim().is_empty();
        let summary = blank.then(|| reason.clone());
        (
            Status::Interrupted,
            summary,
            next.map(str::to_owned),
            resumes,
        )
    } else {
        (Status::Failed, Some(reason.clone()), None, None)
    };
    Change::End {
        status,
        summary,
        reason,
        next_step,
        resumes_at,
        finished: end.at,
        review_result: job.obi.is_some().then(|| end.review.clone()),
    }
}

impl Change {
    /// Writes into `job` what [`transition`] decided: the one place a new round or a run's end
    /// sets a job's status or clears its fields.
    pub fn apply(&self, job: &mut Job) {
        match self {
            Change::Keep => {}
            Change::Round { round, rounds } => {
                job.status = Status::Queued;
                job.phase = Some(Phase::Work);
                job.round = Some(*round);
                job.rounds = *rounds;
                for field in ROUND_CLEARS {
                    field.clear(job);
                }
            }
            Change::End {
                status,
                summary,
                reason,
                next_step,
                resumes_at,
                finished,
                review_result,
            } => {
                job.status = *status;
                if let Some(summary) = summary {
                    job.summary = summary.clone();
                }
                job.next_step = next_step.clone();
                job.resumes_at = *resumes_at;
                job.reason = Some(reason.clone());
                job.asked = false;
                job.publish_step = None;
                job.step = String::new();
                job.finished = Some(*finished);
                if let Some(review) = review_result {
                    job.review_result = review.clone();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{Stalled, Unjudged, Waiting};

    /// `job` with every field of [`ROUND_CLEARS`] filled.
    pub(super) fn fill(job: &mut Job) {
        for field in ROUND_CLEARS {
            match field {
                Field::Finished => job.finished = Some(10),
                Field::Dismissed => job.dismissed = Some(11),
                Field::Archived => job.archived = Some(12),
                Field::Limit => job.limit = Some("The turn limit.".into()),
                Field::Reason => job.reason = Some("It broke.".into()),
                Field::StoppedBy => job.stopped_by = Some("you".into()),
                Field::NextStep => job.next_step = Some(TRY_AGAIN.into()),
                Field::ResumesAt => job.resumes_at = Some(13),
                Field::Unjudged => {
                    job.unjudged = Some(Unjudged {
                        tries: 1,
                        next: Some(14),
                        why: "No runner.".into(),
                    })
                }
                Field::AccountLimitBlocked => job.account_limit_blocked = true,
                Field::AccountLimitEpisode => job.account_limit_episode = Some("e".into()),
                Field::AccountLimitNonce => job.account_limit_nonce = Some("n".into()),
                Field::AccountLimitReset => job.account_limit_reset = Some(15),
                Field::AccountLimitChoice => {
                    job.account_limit_choice = Some(crate::account_limits::Choice::Pending)
                }
                Field::Stalled => {
                    job.stalled = Some(Stalled {
                        why: "quiet".into(),
                        text: "Quiet.".into(),
                        at: 16,
                        round: 4,
                        branch: "oby/fix".into(),
                        commit: None,
                        commits: 1,
                        session: true,
                        note: "Note.".into(),
                        note_by: "obid".into(),
                    })
                }
                Field::Waiting => {
                    job.waiting = Some(Waiting {
                        kind: "room".into(),
                        text: "Waiting.".into(),
                        reserved: None,
                        tool: None,
                    })
                }
            }
        }
    }

    /// Whether `job`'s `field` is empty.
    pub(super) fn is_clear(field: Field, job: &Job) -> bool {
        match field {
            Field::Finished => job.finished.is_none(),
            Field::Dismissed => job.dismissed.is_none(),
            Field::Archived => job.archived.is_none(),
            Field::Limit => job.limit.is_none(),
            Field::Reason => job.reason.is_none(),
            Field::StoppedBy => job.stopped_by.is_none(),
            Field::NextStep => job.next_step.is_none(),
            Field::ResumesAt => job.resumes_at.is_none(),
            Field::Unjudged => job.unjudged.is_none(),
            Field::AccountLimitBlocked => !job.account_limit_blocked,
            Field::AccountLimitEpisode => job.account_limit_episode.is_none(),
            Field::AccountLimitNonce => job.account_limit_nonce.is_none(),
            Field::AccountLimitReset => job.account_limit_reset.is_none(),
            Field::AccountLimitChoice => job.account_limit_choice.is_none(),
            Field::Stalled => job.stalled.is_none(),
            Field::Waiting => job.waiting.is_none(),
        }
    }

    /// JOBSTATE-7 (docs/contracts/job-state.md): a new round empties every field of its list, and
    /// every other field of `Job` is either one it sets or kept as it was. A field added to `Job`
    /// doesn't compile here until it's sorted, and one sorted as cleared but not in the list fails.
    #[test]
    fn jobstate_7_a_new_round_clears_every_listed_field() {
        let mut before = Job::new(
            "j".into(),
            "o/r".into(),
            "fix".into(),
            "Fix it".into(),
            "claude".into(),
            "a".into(),
        );
        fill(&mut before);
        before.status = Status::Failed;
        before.phase = Some(Phase::Check);
        before.round = Some(4);
        before.rounds = Some(2);
        before.session = Some("s".into());
        before.branch = "oby/fix".into();
        before.summary = "Did half of it.".into();
        before.heard = vec!["Go on.".into()];
        before.commits = Some(2);
        before.step = "Working".into();
        before.asked = true;
        before.recovery.tries = 3;
        let mut after = before.clone();
        transition(&after, &Command::NextRound)
            .unwrap()
            .apply(&mut after);
        for field in ROUND_CLEARS {
            assert!(is_clear(field, &after), "{field:?}");
        }
        assert_eq!(
            (after.status, after.phase, after.round, after.rounds),
            (Status::Queued, Some(Phase::Work), Some(5), Some(3))
        );
        let empty = Job::default();
        macro_rules! sorted {
            (
                set: [$($set:ident),* $(,)?],
                cleared: [$($cleared:ident: $field:ident),* $(,)?],
                kept: [$($kept:ident),* $(,)?] $(,)?
            ) => {
                let Job { $($set: _,)* $($cleared,)* $($kept,)* } = after.clone();
                assert_eq!([$(Field::$field),*].len(), ROUND_CLEARS.len());
                $(
                    assert!(
                        ROUND_CLEARS.contains(&Field::$field),
                        "{} is cleared but not in ROUND_CLEARS",
                        stringify!($cleared)
                    );
                    assert_eq!($cleared, empty.$cleared, "{}", stringify!($cleared));
                )*
                $(assert_eq!($kept, before.$kept, "{} is kept", stringify!($kept));)*
            };
        }
        sorted! {
            set: [status, phase, round, rounds],
            cleared: [
                finished: Finished,
                dismissed: Dismissed,
                archived: Archived,
                limit: Limit,
                reason: Reason,
                stopped_by: StoppedBy,
                next_step: NextStep,
                resumes_at: ResumesAt,
                unjudged: Unjudged,
                account_limit_blocked: AccountLimitBlocked,
                account_limit_episode: AccountLimitEpisode,
                account_limit_nonce: AccountLimitNonce,
                account_limit_reset: AccountLimitReset,
                account_limit_choice: AccountLimitChoice,
                stalled: Stalled,
                waiting: Waiting,
            ],
            kept: [
                id, request, agent, account, repo, branch, base, pr, kind, task, title, step,
                summary, session, created, started, commits, published, pushed, publish_error,
                publish_step, outgoing, pull_request, pull_request_error, review_error, reviewing,
                review, approvals, review_of, security_paths, no_reviewer, review_failed,
                summaries, issue, agent_version, check, check_asked, checked, model, effort,
                model_from_account, complex, issue_unread, notes, files, attachments,
                attachments_gone, start_commit, checked_head, mode, cut, after_pr_rebase,
                checks_rounds, changes_rounds, crash_resumes, recovery, worktree, obi,
                conversation, turn, review_result, approvable, obi_request, permission,
                question_call, parked, mac_check, apple_check, apple_ran, apple_token, answered,
                answered_count, tetrix, teammates, connection_uses, heard, owners, said, api_said,
                replies, offers, asked, brief, artifacts, artifact_problems, follow_up_of,
                unfollowed, activity, context, doing, owner_spoke, earlier, made_branch,
                start_from, supervised, revision, diffs, checks, scan, stage_times, estimate,
                usage, peak_rss, agent_peak_rss, check_peak_rss, build_freed, usage_seen,
                window_share, ran_secs, budget_from, changes, moves, named, routine, unattended,
                untrusted, own_lane, ran_unproxied, unsigned, client_request_id,
                memory_from_others, foreign, foreign_by_api, withheld, check_withheld,
                check_withheld_was, reached, made, apart, clone_root, vetted, own_conversation,
                merge_error, told, merge_needs_you, merged_by, sending, auto_merge,
                auto_merge_refused, extra,
            ],
        }
    }

    /// JOBSTATE-7: at `u32::MAX` a new round keeps the round and its count there, never wrapping.
    #[test]
    fn jobstate_7_a_new_round_at_the_maximum_stays_there() {
        let job = Job {
            round: Some(u32::MAX),
            rounds: Some(u32::MAX),
            ..Job::default()
        };
        let mut after = job.clone();
        transition(&job, &Command::NextRound)
            .unwrap()
            .apply(&mut after);
        assert_eq!(after.round, Some(u32::MAX));
        assert_eq!(after.rounds, Some(u32::MAX));
        assert_eq!(after.status, Status::Queued);
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proofs of the lifecycle (R04-03's trial): `kani::any()` stands for every value of
    //! the enums and numbers below; every other field comes from one fixed job.

    use super::tests::{fill, is_clear};
    use super::*;
    use crate::job::ObiRef;

    /// Any status, from a number.
    fn any_status() -> Status {
        match kani::any::<u8>() {
            0 => Status::Queued,
            1 => Status::Running,
            2 => Status::Done,
            3 => Status::Failed,
            4 => Status::Stopped,
            _ => Status::Interrupted,
        }
    }

    /// The fixed job, every field of [`ROUND_CLEARS`] filled, in any status, round, count of
    /// rounds and tries, with or without a session and as the owner's or another Obi's.
    fn any_job() -> Job {
        let mut job = Job::default();
        fill(&mut job);
        job.status = any_status();
        job.phase = Some(Phase::Check);
        job.round = kani::any();
        job.rounds = kani::any();
        job.recovery.tries = kani::any();
        job.summary = "Half.".into();
        if kani::any() {
            job.session = Some("s".into());
        }
        if kani::any() {
            job.obi = Some(ObiRef {
                correlation_id: "c".into(),
                from: "f".into(),
                from_owner: "o".into(),
                request_id: "r".into(),
            });
        }
        job
    }

    /// An end of `kind` with a next step or none, any resume and any time.
    fn any_end(kind: EndKind) -> Command {
        Command::RunEnded(Ended {
            kind,
            reason: "It broke.".into(),
            next: if kani::any() { Some(TRY_AGAIN) } else { None },
            resumes: kani::any(),
            review: None,
            at: kani::any(),
        })
    }

    /// `check` on any command, from a number: another round, or an end of any kind. Each command is
    /// made where `check` runs on it, so the solver never merges two commands into one whose
    /// review (a `serde_json::Value`, which it can't unwind) it no longer knows is empty. For the
    /// same reason the proofs forget the jobs, commands and changes they made rather than drop
    /// them: dropping isn't the lifecycle's code.
    fn any_command(check: impl Fn(Command)) {
        match kani::any::<u8>() {
            0 => check(Command::NextRound),
            1 => check(any_end(EndKind::Failed)),
            2 => check(any_end(EndKind::NeedsYou)),
            3 => check(any_end(EndKind::AccountLimit)),
            _ => check(any_end(EndKind::Asked)),
        }
    }

    /// `transition` answers every command in every status, and never panics or overflows.
    #[kani::proof]
    fn transition_answers_every_command_in_every_status() {
        let job = any_job();
        any_command(|cmd| {
            let answer: Result<Change, Refused> = transition(&job, &cmd);
            std::mem::forget((answer, cmd));
        });
        std::mem::forget(job);
    }

    /// JOBSTATE-7 for every status and command: a change that leaves the job queued for another
    /// round leaves every field of [`ROUND_CLEARS`] empty.
    #[kani::proof]
    fn jobstate_7_holds_in_every_status() {
        any_command(|cmd| {
            let mut job = any_job();
            if let Ok(change) = transition(&job, &cmd) {
                change.apply(&mut job);
                if job.status == Status::Queued && job.phase == Some(Phase::Work) {
                    for field in ROUND_CLEARS {
                        assert!(is_clear(field, &job));
                    }
                }
                std::mem::forget(change);
            }
            std::mem::forget((job, cmd));
        });
    }
}
