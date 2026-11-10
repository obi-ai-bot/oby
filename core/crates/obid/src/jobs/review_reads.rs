//! What a review runs without asking: Oby's own (PERMISSIONS-14) and one the owner dispatched
//! (MERGING-36).

use super::*;

impl Jobs {
    /// Whether review `job` may run `call` without asking, as this machine boxes it: Oby's own
    /// ([`review_may_run`]), or one the owner dispatched, whose reads run as Oby's own do
    /// (MERGING-36): in Claude Code's own sandbox or a box that holds its egress, never on someone
    /// else's code. What it would still ask keeps its card.
    pub(super) fn review_runs(&self, job: &Job, call: &Call, bounds: &permissions::Bounds) -> bool {
        let own = self
            .job_account(job)
            .ok()
            .map(|(tool, _)| Self::own_lane(job, tool));
        match job.review_of.as_deref() {
            Some(of) => review_may_run(
                job,
                self.get(of).as_ref(),
                self.confines_egress(),
                own == Some(false),
                call,
                bounds,
            ),
            None => {
                job.read_only()
                    && job.someone_elses().is_none()
                    && (own == Some(true) || self.confines_egress())
                    && reads_in_box(call, bounds)
            }
        }
    }
}

/// Whether Oby's own `review` may run `call` with nobody watching (PERMISSIONS-14, the owner's
/// answer of 2026-11-08: local commands inside its box, with no network), fail-closed: only a Bash
/// command [`reads_in_box`]; only where the box holds its egress to obid's proxy (`confined`) and
/// obid boxes the run itself (`boxed`: not Claude Code's own sandbox); and only in a review of the
/// owner's own work (`author`, read afresh for each call: from the owner's own words, no one
/// else's, no second review needed, nothing reached past the registries or withheld), so a
/// stranger's code never runs beside a reviewer's login. Every other command is refused as
/// [`unwatched_review`] says.
pub(super) fn review_may_run(
    review: &Job,
    author: Option<&Job>,
    confined: bool,
    boxed: bool,
    call: &Call,
    bounds: &permissions::Bounds,
) -> bool {
    review.review_of.is_some()
        && confined
        && boxed
        && review.someone_elses().is_none()
        && author.is_some_and(|author| {
            // Someone else's, not from the owner's words, past the registries, withheld (all
            // `own_check_refuses_secrets`), unsigned, from others' memory or a security path
            // (`two_reviews`).
            author.own_check_refuses_secrets().is_none()
                && author.check_withheld.is_none()
                && crate::merge::two_reviews(author, review.security_paths.as_deref()).is_none()
        })
        && reads_in_box(call, bounds)
}

/// A Bash command that reaches nothing past its box ([`permissions::reach`]) and writes nothing as
/// obid reads it ([`permissions::writes`]).
fn reads_in_box(call: &Call, bounds: &permissions::Bounds) -> bool {
    call.tool == "Bash"
        && permissions::reach(call, bounds).is_none()
        && call
            .command
            .as_deref()
            .is_some_and(|command| !permissions::writes(command))
}
