// The start of Oby's review of one published head, by two callers at once, as the fixed design decides it (task
// A00-63, MERGING-55 in docs/contracts/merging.md). It models, by function:
// - Job::review_due in core/crates/obid/src/job.rs: a head's review is due while none is under way and it lacks
//   the approvals it needs, here two, a head that needs two reviews (due);
// - Jobs::start_review in core/crates/obid/src/jobs.rs: a caller reads a copy of the job and finds the review due,
//   with no lock held over what comes next (see);
// - Jobs::claim_review in core/crates/obid/src/jobs/review_start.rs: under one hold of the jobs lock, the caller
//   checks again that the review is due and, only if it is, keeps the review job and names it on the author's
//   `reviewing` (claim);
// - its two callers that raced: Jobs::reviewed, landing an approval that asks for a second review, and
//   Jobs::settle_reviews on a follow pass (the starters "reviewed" and "settle"; a pass that finds nothing due
//   does nothing), and a review's approval landing (approve).
// oneReviewUnderWay and twoReviewsAtMost are the properties: never two reviews under way at once, and never more
// reviews started than the head needs. Each is expected to hold in every sequence up to 12 steps (quint verify
// --max-steps=12 --invariant=oneReviewUnderWay, and --invariant=twoReviewsAtMost).
// startersNeverOverlap is the first's witness: both starters do see the review due before either claims it, the
// interleaving the race needs, so it is expected to be violated. noSecondReviewStarts is the second's: the second
// review does start, so it is expected to be violated too, and neither pass is vacuous.
// Not modelled: a crash between saving the review job and its author naming it, which can still let a later pass
// start a second review (MERGING-55 accepts it, "the review first").
// The limit: a model checks the design, not that jobs.rs implements it. Nothing here replays a trace against the
// code (Quint Connect is not used).
module review_start {
  pure val STARTERS = Set("reviewed", "settle")
  var phase: str -> str
  var reviewing: bool
  var running: int
  var started: int
  var approvals: int
  val due = not(reviewing) and approvals < 2
  action init = all {
    phase' = STARTERS.mapBy(_ => "idle"), reviewing' = false, running' = 0, started' = 0, approvals' = 0,
  }
  // An idle starter looks: one that finds the review due goes on to claim it; one that doesn't stays idle.
  action see(s: str): bool = all {
    phase.get(s) == "idle",
    phase' = if (due) phase.set(s, "saw") else phase, reviewing' = reviewing, running' = running,
    started' = started, approvals' = approvals,
  }
  action claim(s: str): bool = all {
    phase.get(s) == "saw",
    phase' = phase.set(s, "idle"),
    approvals' = approvals,
    if (due) all { started' = started + 1, running' = running + 1, reviewing' = true }
    else all { started' = started, running' = running, reviewing' = reviewing },
  }
  action approve = all {
    reviewing,
    phase' = phase, reviewing' = false, running' = running - 1, started' = started, approvals' = approvals + 1,
  }
  action starter = { nondet s = STARTERS.oneOf() any { see(s), claim(s) } }
  action step = any { starter, approve }
  val oneReviewUnderWay = running <= 1
  val twoReviewsAtMost = started <= 2
  val startersNeverOverlap = not(STARTERS.forall(s => phase.get(s) == "saw") and due)
  val noSecondReviewStarts = started < 2
}
