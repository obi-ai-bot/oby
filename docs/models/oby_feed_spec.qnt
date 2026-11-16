// verify: main=oby_feed_spec steps=12 invariants=noBuildWithoutAcceptedSpec,heldIntakeStartsNothing,failClosedStartsNoBuild,oneJobPerStep,blockedHasNoLiveBuild witnesses=neverBuilt,neverHeldForOwner,neverFailedClosed,noAnswerEverLost,noPartEverRefused
//
// factory/bin/oby-feed, one task's start path (docs/contracts/factory.md, FEEDSPEC): start() and started_item() for
// its intake, then follow()'s step functions, read_step(), ask_step(), post_step(), send_parts(), undelivered() and
// send_stops(), with ready_tasks()'s asks_again() for a blocked task asked again. What follows the build's start
// (reviews, gate, approver, merge) is docs/models/oby_feed_approver.qnt's.
// Each step (intake, spec, build) is a request saved before its POST and resent unchanged until obid answers it: obid
// makes a job under a request's key only once (its receipts), so a lost answer or a 5xx gives the same job back. A
// refusal (4xx) comes only for a key obid made nothing under. As in the script, the first POST of a step goes in the
// same pass as what starts it (start() for the intake, read_step()'s ask_step() for the spec and the build); a resend
// is a pass of its own. Once a step's job is recorded, its parts are sent; a part may land, go unanswered (sent again)
// or not land (refused, or answered by a follow-up), which stops the job before the task is blocked. An intake job ends
// with ACCEPT, NEEDS-INFO, REJECT, an answer that isn't one, failed (any status but done: an account limit or a
// passing error too) or gone; a spec job with numbered criteria, none, failed or gone. A blocked task whose text
// changed on the base starts a new attempt from its intake.
// Invariants (FEEDSPEC-3 to 7):
// - noBuildWithoutAcceptedSpec: obid made a build job only once the saved intake is ACCEPT and a spec with numbered
//   criteria is saved, in this attempt.
// - heldIntakeStartsNothing: an attempt whose intake answered NEEDS-INFO or REJECT has no spec or build job.
// - failClosedStartsNoBuild: an attempt whose intake gave no decision, or whose spec job failed, went or had no
//   numbered criteria, has no build job.
// - oneJobPerStep: obid made at most one intake, one spec and one build job in this attempt, lost answers and resent
//   requests included.
// - blockedHasNoLiveBuild: a blocked task has no build job queued or running.
// Witnesses, each expected VIOLATED so the model shows the case can happen: neverBuilt (a build is made),
// neverHeldForOwner (a task waits on NEEDS-INFO), neverFailedClosed (a task is blocked for a malformed or failed step),
// noAnswerEverLost (a POST's answer, a job's or a part's, is lost), noPartEverRefused (the one way a task with a build
// gets blocked: the build's part refused, the build stopped, the task blocked; 9 steps).
// The limit: a model checks the design, not that the script implements it, nor what the agents' words say. A
// document's parts are one message here, and the 40-part and 2,000-character bounds are the tests' (FEEDSPEC-8); a
// step blocked as too long starts nothing. Re-asks are bounded at MaxAttempts to keep the state space small, where
// asks_again() has no bound. Left out: an older feeder's saved `kind: build` start, recorded `running` with no intake
// or spec when obid has its key (Requirement 16; never sent again otherwise), which this feeder never makes. Depth 12
// reaches a build made after lost answers and a build's refused part stopped and blocked, but not two re-asks in a row:
// factory/tests/test_oby_feed.py covers those (FEEDSPEC-4).

module oby_feed_spec {
  pure val MaxAttempts = 3

  type T = {
    st: str,            // ready, intake, speccing, running, blocked: the feeder's state of the task
    req: str,           // none, intake, spec, build: the step request saved and not yet answered
    keyMade: bool,      // obid made a job under the saved request's key
    intakeJob: str,     // none, running, accept, needsInfo, reject, malformed, failed, gone, stopped
    specJob: str,       // none, running, criteria, noCriteria, failed, gone, stopped
    buildJob: str,      // none, live, stopped: queued or running is live
    intakeMade: int,    // jobs obid made for each step, in this attempt
    specMade: int,
    buildMade: int,
    partsOwed: bool,    // the recorded step job's parts aren't all answered
    stopOwed: bool,     // a part didn't land: its job's stop isn't answered yet
    decision: str,      // none, ACCEPT, NEEDS-INFO, REJECT: the saved intake's
    specSaved: bool,    // a spec with numbered criteria is saved
    failedClosed: bool, // the intake gave no decision, or the spec job failed, went or had no criteria
    why: str,           // none, owner, failed, refused, part: why the task is blocked
    attempt: int,
    changed: bool,      // the task text changed on the base since the task was blocked
    lostEver: bool,     // ghost: a POST's answer was lost
    refusedEver: bool   // ghost: a part was refused or answered with a follow-up
  }
  var s: T

  pure val fresh: T = { st: "ready", req: "none", keyMade: false, intakeJob: "none", specJob: "none", buildJob: "none",
                        intakeMade: 0, specMade: 0, buildMade: 0, partsOwed: false, stopOwed: false, decision: "none",
                        specSaved: false, failedClosed: false, why: "none", attempt: 0, changed: false,
                        lostEver: false, refusedEver: false }
  pure def stepState(step: str): str = if (step == "intake") "intake" else if (step == "spec") "speccing" else "running"
  pure def ended(job: str): bool = job != "none" and job != "running" and job != "stopped"
  // obid makes the job under the request's key, unless it already has one
  pure def made(x: T): T =
    if (x.keyMade) x
    else if (x.req == "intake") { ...x, keyMade: true, intakeMade: x.intakeMade + 1, intakeJob: "running" }
    else if (x.req == "spec") { ...x, keyMade: true, specMade: x.specMade + 1, specJob: "running" }
    else { ...x, keyMade: true, buildMade: x.buildMade + 1, buildJob: "live" }
  pure def blocked(x: T, why: str): T = { ...x, st: "blocked", req: "none", keyMade: false, partsOwed: false,
                                          stopOwed: false, why: why }

  action init = s' = fresh

  // ---- the world: obid's step jobs, the owner changing the task text ----
  action intakeEnds = {
    nondet end = Set("accept", "needsInfo", "reject", "malformed", "failed", "gone").oneOf()
    all { s.intakeJob == "running", s' = { ...s, intakeJob: end } }
  }
  action specEnds = {
    nondet end = Set("criteria", "noCriteria", "failed", "gone").oneOf()
    all { s.specJob == "running", s' = { ...s, specJob: end } }
  }
  action ownerEditsTask = all { s.st == "blocked", not(s.changed), s' = { ...s, changed: true } }
  action world = any { intakeEnds, specEnds, ownerEditsTask }

  // ---- the feeder's pass ----
  // post_step(), the saved request `x.req` sent: obid answers with its job, the answer is lost (no answer or a 5xx:
  // the request stays saved), or obid refuses a key it made nothing under
  action post(x: T): bool = {
    nondet answer = Set("job", "lost", "refused").oneOf()
    if (answer == "job") s' = { ...made(x), st: stepState(x.req), req: "none", keyMade: false, partsOwed: true }
    else if (answer == "lost") s' = { ...made(x), lostEver: true }
    else all { not(x.keyMade), s' = blocked(x, "refused") }
  }
  // start(): a ready task's intake request, saved and sent
  action startIntake = all { s.st == "ready", s.req == "none", post({ ...s, req: "intake" }) }
  // a later pass sends a request whose answer was lost again, unchanged
  action resend = all { s.req != "none", not(s.partsOwed), not(s.stopOwed), post(s) }
  // send_parts(): they land, one goes unanswered (sent again), or one doesn't land and its job is to be stopped
  action sendParts = {
    nondet answer = Set("landed", "lost", "refused").oneOf()
    all {
      s.partsOwed, not(s.stopOwed),
      if (answer == "landed") s' = { ...s, partsOwed: false }
      else if (answer == "lost") s' = { ...s, lostEver: true }
      else s' = { ...s, partsOwed: false, stopOwed: true, refusedEver: true }
    }
  }
  // send_stops(): once obid answers the stop, the job is stopped and the task blocked
  action sendStop = {
    nondet answered = Set(true, false).oneOf()
    all {
      s.stopOwed,
      if (not(answered)) s' = { ...s, lostEver: true }
      else if (s.st == "intake") s' = blocked({ ...s, intakeJob: "stopped" }, "part")
      else if (s.st == "speccing") s' = blocked({ ...s, specJob: "stopped" }, "part")
      else s' = blocked({ ...s, buildJob: "stopped" }, "part")
    }
  }
  // read_step(): an ended step job, its parts all landed and no request pending; ask_step() sends the next request
  action readIntake = all {
    s.st == "intake", s.req == "none", not(s.partsOwed), not(s.stopOwed), ended(s.intakeJob),
    if (s.intakeJob == "accept") post({ ...s, decision: "ACCEPT", req: "spec" })
    else if (s.intakeJob == "needsInfo") s' = blocked({ ...s, decision: "NEEDS-INFO" }, "owner")
    else if (s.intakeJob == "reject") s' = blocked({ ...s, decision: "REJECT" }, "owner")
    else s' = blocked({ ...s, failedClosed: true }, "failed")
  }
  action readSpec = all {
    s.st == "speccing", s.req == "none", not(s.partsOwed), not(s.stopOwed), ended(s.specJob),
    if (s.specJob == "criteria") post({ ...s, specSaved: true, req: "build" })
    else s' = blocked({ ...s, failedClosed: true }, "failed")
  }
  // asks_again(): a blocked task whose text changed starts a new attempt from its intake
  action askAgain = all {
    s.st == "blocked", s.changed, s.attempt < MaxAttempts,
    s' = { ...fresh, attempt: s.attempt + 1, lostEver: s.lostEver, refusedEver: s.refusedEver }
  }
  action feeder = any { startIntake, resend, sendParts, sendStop, readIntake, readSpec, askAgain }

  action step = any { world, feeder }

  val noBuildWithoutAcceptedSpec = s.buildMade > 0 implies (s.decision == "ACCEPT" and s.specSaved)
  val heldIntakeStartsNothing = (s.decision == "NEEDS-INFO" or s.decision == "REJECT") implies (s.specMade == 0 and s.buildMade == 0)
  val failClosedStartsNoBuild = s.failedClosed implies s.buildMade == 0
  val oneJobPerStep = s.intakeMade <= 1 and s.specMade <= 1 and s.buildMade <= 1
  val blockedHasNoLiveBuild = s.st == "blocked" implies s.buildJob != "live"
  val neverBuilt = s.buildMade == 0
  val neverHeldForOwner = not(s.st == "blocked" and s.decision == "NEEDS-INFO")
  val neverFailedClosed = not(s.st == "blocked" and s.failedClosed)
  val noAnswerEverLost = not(s.lostEver)
  // FEEDSPEC-7's path: a build was made, then the task blocked (its part refused, the build stopped)
  val noPartEverRefused = not(s.st == "blocked" and s.buildMade > 0)
}
