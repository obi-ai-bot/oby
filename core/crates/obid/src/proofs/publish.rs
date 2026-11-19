//! The force-push default (PUBLISHING-13): without the owner's signed force, obid replaces what
//! is on a branch only when it made that branch, never the default branch or anyone else's.
//! Proves: core/crates/obid/src/publish.rs
//! Proves: core/crates/obid/src/git.rs

use super::any_str;
use crate::job::Job;
use crate::publish::{ByHand, may_force};

/// For any target branch and job branch (any bytes, up to 4: `main` and every other name that
/// short; past that, `is_branch_name`'s string searches cost CBMC more than CI's run) and any
/// `made_branch`: without the owner's force, obid force-pushes only an `oby/` branch, or the
/// job's own branch when obid made it, so never a `main` it didn't make.
#[kani::proof]
#[kani::unwind(8)]
fn obid_force_pushes_only_branches_it_made() {
    let to = any_str::<4>();
    let job = Job {
        branch: any_str::<4>(),
        made_branch: kani::any(),
        ..Job::default()
    };
    if may_force(&to, &job, &ByHand::default()) {
        assert!(to.starts_with("oby/") || (to == job.branch && job.made_branch));
        assert!(to != "main" || job.made_branch);
    }
    // Forgotten, not dropped: dropping a job's hundred fields is CBMC's time, and proves nothing.
    std::mem::forget(job);
}
