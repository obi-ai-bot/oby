// verify: main=oby_feed_fix_stop steps=12 invariants=sameReasonRunAtMostLimit,newReasonNeverStopped,stopHoldsUntilAgain witnesses=stopNeverReached,newReasonNeverTriedAfterStop,againNeverUsed
//
// factory/bin/oby-feed, one task: the fix rounds and their stop (docs/contracts/factory.md, FEEDSTOP), in
// read_lenses()'s case (c) with fix_round(), gated()'s red head, post_fix() counting a follow-up in `fixRun`, stops()
// and stop_fix() saving `stopped`, send_owed_fix() and send_fix() sending what is owed, and take_agains() reading the
// owner's `oby-feed --again KEY`. The task's follow-ups keep failing, each failure with a reason of its own, `lens` (a
// lens asked for changes) or `gate` (the gate failed), chosen by the world. For each failure the feeder decides once:
// it owes the build job a fix message, or, when the last Limit follow-ups in a row went out for that same reason, it
// sends nothing and the task is stopped. Two messages can be owed at once, as in the code: the gate's (`gate.owed`)
// and the fix round's (`fixOwed`, a lens's, or the one the owner's again gave back). obid hears each owed message (a
// 2xx: Oby starts a follow-up, which fails again later), refuses it (a 4xx: dropped, not counted), or doesn't answer
// (it stays owed). The owner's again gives back the message the stop didn't send as `fixOwed`, and starts the run
// again; while another `fixOwed` is owed it waits, as take_agains() leaves the `.again` file for a later pass.
// Invariants. Each reads the ghosts `streak` (the follow-ups obid heard in a row for one reason, since the last other
// reason or the owner's again), `declined` and `stopGhost`, kept apart from the feeder's own `fixRun`, so a feeder that
// counts or compares wrong breaks one:
// - sameReasonRunAtMostLimit (FEEDSTOP-2): the follow-ups sent in a row for one reason never exceed Limit.
// - newReasonNeverStopped (FEEDSTOP-3): a failure the feeder declined to answer always has the reason of a run already
//   at Limit.
// - stopHoldsUntilAgain (FEEDSTOP-4): once the feeder declined a reason, a later failure for it gets no follow-up
//   until the owner's again, or a follow-up obid heard for another reason, ended the stop.
// Witnesses, each expected VIOLATED so the model shows the case can happen: stopNeverReached (a stop happens),
// newReasonNeverTriedAfterStop (after a stop, a follow-up for another reason is sent and heard), againNeverUsed (the
// owner's again leads to a follow-up obid heard).
// The limit: a model checks the design, not that the script implements it; nor the words of a message, only that one
// is owed, kept or dropped. Limit is 2, not the feeder's default 3, so that depth 12 reaches the stop (8 steps) and a
// new reason or the owner's again after it (11). A message obid didn't answer is the same follow-up when it is sent
// again under its key, so it is one owed message here until obid hears or refuses it; a new head dropping it unsent is
// a refusal here. A lens's CHANGES replaces a `fixOwed` still owed, as fix_round() does.

module oby_feed_fix_stop {
  pure val Reasons = Set("lens", "gate")
  pure val Limit = 2

  var failing: str          // a failure the feeder hasn't decided on yet: none, lens or gate
  var gateOwed: bool        // the gate's fix message is owed to obid (`gate.owed`)
  var fixOwed: str          // the reason of the fix round's message owed to obid (`fixOwed`), or none
  var owedFromAgain: bool   // `fixOwed` is the one the owner's again gave back
  var runReason: str        // `fixRun.reason`: none while the task has no run
  var runCount: int         // `fixRun.count`
  var stopped: str          // `stopped.reason`: none while the task isn't stopped
  var streakReason: str     // ghost: the reason of the follow-ups obid heard in a row
  var streak: int           // ghost: how many, since the last other reason or the owner's again
  var stopGhost: str        // ghost: the reason the feeder last declined, until again or another reason's follow-up
  var declined: Set[(str, str, int)]  // ghost: (the failure's reason, streakReason, streak) for each failure declined
  var answeredWhileStopped: bool      // ghost: a failure for stopGhost's reason was owed a message
  var everStopped: bool     // ghost: the feeder declined a failure
  var newAfterStop: bool    // ghost: obid heard a follow-up for another reason while a stop held
  var heardAfterAgain: bool // ghost: obid heard the message the owner's again gave back

  action init = all {
    failing' = "none", gateOwed' = false, fixOwed' = "none", owedFromAgain' = false, runReason' = "none", runCount' = 0,
    stopped' = "none", streakReason' = "none", streak' = 0, stopGhost' = "none", declined' = Set(),
    answeredWhileStopped' = false, everStopped' = false, newAfterStop' = false, heardAfterAgain' = false
  }

  action keepFeeder = all { gateOwed' = gateOwed, fixOwed' = fixOwed, owedFromAgain' = owedFromAgain,
                            runReason' = runReason, runCount' = runCount, stopped' = stopped }
  action keepGhosts = all { streakReason' = streakReason, streak' = streak, stopGhost' = stopGhost, declined' = declined,
                            answeredWhileStopped' = answeredWhileStopped, everStopped' = everStopped,
                            newAfterStop' = newAfterStop, heardAfterAgain' = heardAfterAgain }

  // ---- the world: the task's newest head fails, for one reason or the other ----
  action headFails = {
    nondet r = Reasons.oneOf()
    all { failing == "none", not(gateOwed), fixOwed == "none", failing' = r, keepFeeder, keepGhosts }
  }

  // ---- the feeder: fix_round() for a lens, gated() for the gate ----
  action decide = all {
    failing != "none",
    failing' = "none",
    runReason' = runReason, runCount' = runCount,
    streakReason' = streakReason, streak' = streak, newAfterStop' = newAfterStop, heardAfterAgain' = heardAfterAgain,
    // stops(): the last Limit follow-ups in a row went out for this reason
    if (runReason == failing and runCount >= Limit)
      all { gateOwed' = gateOwed, fixOwed' = fixOwed, owedFromAgain' = owedFromAgain, stopped' = failing,
            stopGhost' = failing, declined' = declined.union(Set((failing, streakReason, streak))), everStopped' = true,
            answeredWhileStopped' = answeredWhileStopped }
    else
      all { gateOwed' = (gateOwed or failing == "gate"),
            fixOwed' = if (failing == "lens") "lens" else fixOwed,
            owedFromAgain' = (owedFromAgain and failing != "lens"),
            stopped' = stopped, stopGhost' = stopGhost, declined' = declined, everStopped' = everStopped,
            answeredWhileStopped' = (answeredWhileStopped or stopGhost == failing) }
  }

  // ---- obid: a 2xx to one owed message (post_fix() counts it), or a 4xx ----
  action heard(reason: str, fromAgain: bool): bool = all {
    failing' = failing,
    runReason' = reason,
    runCount' = if (runReason == reason) runCount + 1 else 1,
    stopped' = if (runReason == reason) stopped else "none",
    streakReason' = reason,
    streak' = if (streakReason == reason) streak + 1 else 1,
    stopGhost' = if (stopGhost == reason) stopGhost else "none",
    declined' = declined, answeredWhileStopped' = answeredWhileStopped, everStopped' = everStopped,
    newAfterStop' = (newAfterStop or (stopGhost != "none" and stopGhost != reason)),
    heardAfterAgain' = (heardAfterAgain or fromAgain)
  }
  action hearGate = all { gateOwed, gateOwed' = false, fixOwed' = fixOwed, owedFromAgain' = owedFromAgain,
                          heard("gate", false) }
  action hearFix = all { fixOwed != "none", gateOwed' = gateOwed, fixOwed' = "none", owedFromAgain' = false,
                         heard(fixOwed, owedFromAgain) }
  action refuseGate = all {
    gateOwed,
    failing' = failing, gateOwed' = false, fixOwed' = fixOwed, owedFromAgain' = owedFromAgain, runReason' = runReason,
    runCount' = runCount, stopped' = stopped, keepGhosts
  }
  action refuseFix = all {
    fixOwed != "none",
    failing' = failing, gateOwed' = gateOwed, fixOwed' = "none", owedFromAgain' = false, runReason' = runReason,
    runCount' = runCount, stopped' = stopped, keepGhosts
  }

  // ---- the owner: `oby-feed --again KEY`, read by take_agains() at a pass with no `fixOwed` ----
  action again = all {
    stopped != "none", fixOwed == "none",
    failing' = failing, gateOwed' = gateOwed, fixOwed' = stopped, owedFromAgain' = true, runReason' = "none",
    runCount' = 0, stopped' = "none", streakReason' = "none", streak' = 0, stopGhost' = "none", declined' = declined,
    answeredWhileStopped' = answeredWhileStopped, everStopped' = everStopped, newAfterStop' = newAfterStop,
    heardAfterAgain' = heardAfterAgain
  }

  action step = any { headFails, decide, hearGate, hearFix, refuseGate, refuseFix, again }

  val sameReasonRunAtMostLimit = streak <= Limit
  val newReasonNeverStopped = declined.forall(d => d._1 == d._2 and d._3 >= Limit)
  val stopHoldsUntilAgain = not(answeredWhileStopped)
  val stopNeverReached = not(everStopped)
  val newReasonNeverTriedAfterStop = not(newAfterStop)
  val againNeverUsed = not(heardAfterAgain)
}
