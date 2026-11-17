//! Lane choice (docs/contracts/lanes.md, LANES-1, LANES-18, LANES-19): which job's agent runs
//! unboxed on the owner's own login. Any job: any kind and mode string up to 6 bytes (every kind
//! the decision reads, `build`, `fix`, `test` and `review`, every mode that moves it, `bypass`, and
//! any other string, which decides as an unknown one does), with or without another Oby, a review,
//! a mark, an untrusted read or a parked plan, on every tool. The job is forgotten, not dropped:
//! dropping its hundred fields is what CBMC would spend its time on, and proves nothing.
//! Proves: core/crates/obid/src/jobs/lanes.rs
//! Proves: core/crates/obid/src/jobs/permission_asks.rs
//! Proves: core/crates/obid/src/jobs/codex_plan.rs
//! Proves: core/crates/obid/src/job.rs

use super::any_str;
use crate::agent::Tool;
use crate::job::{Job, ObiRef};
use crate::jobs::{COMMITTING, Jobs};
use crate::permissions::{Mode, Parked};

fn any_job() -> Job {
    let mut job = Job {
        kind: any_str::<6>(),
        mode: kani::any::<bool>().then(any_str::<6>),
        untrusted: kani::any(),
        review_of: kani::any::<bool>().then(String::new),
        foreign: kani::any::<bool>().then(String::new),
        ..Job::default()
    };
    if kani::any() {
        job.obi = Some(ObiRef {
            correlation_id: String::new(),
            from: String::new(),
            from_owner: String::new(),
            request_id: String::new(),
        });
    }
    if kani::any() {
        job.parked = Some(Parked {
            tool: String::new(),
            input: serde_json::Value::Null,
            answered: kani::any(),
            allowed: kani::any(),
            ended_turn: kani::any(),
        });
    }
    job
}

fn any_tool() -> Tool {
    match kani::any::<u8>() % 3 {
        0 => Tool::Claude,
        1 => Tool::Codex,
        _ => Tool::Cursor,
    }
}

/// Another Oby's job, an untrusted one, a review and any Cursor run are always boxed; an own
/// Codex run is Bypass on macOS, never a build, nor the rest of a run its plan card resumed.
#[kani::proof]
#[kani::unwind(10)]
fn foreign_untrusted_or_review_work_never_runs_unboxed() {
    let (job, tool) = (any_job(), any_tool());
    if Jobs::own_lane(&job, tool) {
        assert!(job.obi.is_none() && !job.untrusted && job.review_of.is_none());
        assert!(tool != Tool::Cursor);
        if tool == Tool::Codex {
            assert!(cfg!(target_os = "macos"));
            assert_eq!(Jobs::mode_of(&job), Mode::Bypass);
            assert!(!COMMITTING.contains(&job.kind.as_str()));
            assert!(
                !job.parked
                    .as_ref()
                    .is_some_and(|p| p.ended_turn && p.answered)
            );
        }
    }
    std::mem::forget(job);
}

/// Every Claude Code job of the owner's own runs in the own lane (the owner's decision of
/// 2026-11-09): with no other Oby, no untrusted read and no review, only their own review of
/// someone else's code stays boxed.
#[kani::proof]
#[kani::unwind(10)]
fn the_owners_own_claude_work_runs_in_its_own_lane() {
    let job = any_job();
    let marked = job.obi.is_some() || job.untrusted || job.review_of.is_some();
    // The owner's review (kind `review`) of code someone else's words reached.
    let their_review_of_others = job.kind == "review" && job.foreign.is_some();
    assert_eq!(
        Jobs::own_lane(&job, Tool::Claude),
        !marked && !their_review_of_others
    );
    std::mem::forget(job);
}
