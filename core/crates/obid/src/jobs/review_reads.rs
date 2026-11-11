//! What a review runs without asking: Oby's own (PERMISSIONS-14) and one the owner dispatched
//! (MERGING-36).

use super::*;

impl Jobs {
    /// Whether review `job` may run `call` without asking, as this machine boxes it: Oby's own
    /// ([`review_may_run`]), or one the owner dispatched ([`hand_review_may_run`]). What it would
    /// still ask keeps its card.
    pub(super) fn review_runs(&self, job: &Job, call: &Call, bounds: &permissions::Bounds) -> bool {
        let boxed = self
            .job_account(job)
            .is_ok_and(|(tool, _)| !Self::own_lane(job, tool));
        match job.review_of.as_deref() {
            Some(of) => review_may_run(
                job,
                self.get(of).as_ref(),
                self.confines_egress(),
                boxed,
                call,
                bounds,
            ),
            None => hand_review_may_run(job, boxed && self.confines_egress(), call, bounds),
        }
    }
}

/// Whether a review the owner dispatched (MERGING-36) may run `call` without asking, never on
/// someone else's code ([`Job::someone_elses`]: a pull request with commits obid didn't record, or
/// one someone else wrote in). In Oby's box with its egress through obid's proxy (`proxied`), what
/// Oby's own review runs ([`reads_in_box`]); anywhere else (Claude Code's own sandbox, with no
/// proxy), only what provably only reads ([`permissions::only_reads`]): a test or a script there
/// would run with the network.
fn hand_review_may_run(
    review: &Job,
    proxied: bool,
    call: &Call,
    bounds: &permissions::Bounds,
) -> bool {
    review.read_only()
        && review.review_of.is_none()
        && review.someone_elses().is_none()
        && if proxied {
            reads_in_box(call, bounds)
        } else {
            call.tool == "Bash"
                && permissions::reach(call, bounds).is_none()
                && call
                    .command
                    .as_deref()
                    .is_some_and(|command| permissions::only_reads(command, bounds))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(review: &Job, proxied: bool, command: &str) -> bool {
        hand_review_may_run(
            review,
            proxied,
            &Call::new("Bash", &json!({ "command": command })),
            &permissions::Bounds::of(Path::new("/tmp/obid-hand-review")),
        )
    }

    fn hand_review() -> Job {
        Job::new(
            "r".into(),
            "o/r".into(),
            "review".into(),
            "Review it".into(),
            "claude".into(),
            "a".into(),
        )
    }

    /// MERGING-36: outside a box that holds its egress (Claude Code's own sandbox), a hand review
    /// runs unasked only what provably only reads: a script, a test or the network still asks,
    /// though PERMISSIONS-14's test (nothing obid reads as writing) would let it through.
    #[test]
    fn merging_36_a_hand_review_runs_only_reads_outside_a_proxied_box() {
        let review = hand_review();
        for read in [
            "grep -n greet src/greet.js",
            "git diff origin/dev...HEAD",
            "cat src/greet.js",
            "sed -n '1,20p' src/greet.js",
        ] {
            assert!(runs(&review, false, read), "{read}");
            assert!(runs(&review, true, read), "{read}");
        }
        // A sed or awk script obid can't prove only prints (macOS sed keeps `/` inside `[…]`).
        for asks in [
            "python3 -c 'print(1)'",
            "node -e 'require(\"fs\").writeFileSync(\"x\", \"\")'",
            "sed -n '1w copy.js' src/greet.js",
            "sed -n 's/[/]/g;s/w .//1' src/greet.js",
            "sed -n 's/greet/hi/p' src/greet.js",
            "awk 'BEGIN{system(\"touch x\")}'",
            "awk 'NR < 3' src/greet.js",
            "npm test",
            "make",
            "curl https://example.com",
        ] {
            assert!(!runs(&review, false, asks), "{asks}");
        }
        // In Oby's box, with egress through obid's proxy, a local command runs (PERMISSIONS-14).
        assert!(runs(&review, true, "npm test"));
        assert!(!runs(&review, true, "curl https://example.com"));
    }

    /// MERGING-36: a hand review of someone else's code asks for every read, in either lane.
    #[test]
    fn merging_36_a_hand_review_of_someone_elses_code_asks() {
        let mut review = hand_review();
        review.foreign = Some("pull request #9's head has commits obid didn't record".into());
        for proxied in [false, true] {
            assert!(!runs(&review, proxied, "cat src/greet.js"));
        }
    }
}
