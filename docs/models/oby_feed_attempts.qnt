// verify: main=oby_feed_attempts steps=12 invariants=attemptsBounded,endsWithReason,noEndedJobLeft,failsOnlyOutOfAttempts,noSecondPullRequest,blockedOrStoppedNeverRebuilt witnesses=attemptsNeverRunOut,neverFailsOutOfAttempts,neverMergedAfterARebuild,neverEndsForGoodEarly,neverFailsWithAPullRequest,neverResentARebuild,refusedHeadNeverWaits
//
// factory/bin/oby-feed's build attempts as built (docs/contracts/factory.md, FEEDLOOP-1, 2, 5 and 6), one task. It
// models, by function:
// - after_attempt(): the one rule for a `running` task's job, `afterAttempt` below, word for word: follow while it is
//   queued or running; end for a STATUS: BLOCKED summary or a stopped job; review for a done job that published a
//   pull request; end once the task has a pull request; again while attempts are left; else end;
// - follow()'s `running` branch and its 404 check, with attempt_ended(): `again` saves a build against the saved spec
//   and sends it at once (post_step(), below), waits on the job that ended while the disk is under --min-free-gb, and ends
//   the task `failed` when no spec is saved; `end` fails it with its note (the last reason when the attempts ran
//   out, else the note it had before FEEDLOOP);
// - post_step()'s `builds`: the attempt count, and its answers to a rebuild's POST: obid makes the job (one attempt
//   more), gives no answer or a 5xx (the request stays saved, `owed`, the task `running` on the job that ended), or a
//   4xx (block(): the task is `blocked`, its request dropped);
// - follow()'s resend: a saved request goes first at the next pass, before anything else is looked at;
// - follow()'s follow-up loop: a follow-up obid starts from the feeder's fix message takes a reviewing or waiting
//   task back to `running` on the new job;
// - follow()'s `reviewing`, `merging` and `waiting` branches, abstracted: the reviews, holds, gate, lenses and
//   approver are one nondeterministic outcome of the pass (armed, or held waiting), a security path always waits,
//   GitHub's queue merges the armed head or refuses it, and a refused or conflicting head stays `waiting` (the
//   proposed FEEDLOOP-3 and 4 aren't built), until a follow-up's new head, a merge by hand or a close.
// The world: the job ends done, failed, interrupted, stopped or gone (a 404), with or without a STATUS: BLOCKED
// summary and a published pull request; obid starts a follow-up of a task with an open pull request; GitHub merges,
// refuses or closes; the disk fills and frees. Whether the spec was saved is chosen once (an older feeder's task may
// have none).
// Invariants:
// - attemptsBounded (FEEDLOOP-5): at most MaxAttempts build jobs for one task.
// - endsWithReason (FEEDLOOP-5): a failed task carries a note, and one whose attempts ran out fails at attempt
//   MaxAttempts with the last attempt's ending (`note == job`, the model's stand-in for attempt_reason()).
// - noEndedJobLeft (FEEDLOOP-1, 2): right after a pass that judged a `running` task, its job is running, unless the
//   disk is under the minimum or obid hasn't answered the rebuild yet (`owed`, resent at the next pass): a job that
//   ended was started again, sent to review or ended the task. A pass that only followed a follow-up (`switched`)
//   judges it at the next one, as follow()'s loop returns once it switched.
// - failsOnlyOutOfAttempts (FEEDLOOP-2, 5): a task fails on a job that failed, was interrupted, is gone or published
//   nothing, with no pull request, not blocked and its spec saved, only at attempt MaxAttempts.
// - noSecondPullRequest (FEEDLOOP-6): no build starts again once the task has a pull request (ghost withPrRebuilds).
// - blockedOrStoppedNeverRebuilt (FEEDLOOP-6): no build starts again after a STATUS: BLOCKED or stopped job (ghost
//   endedRebuilds).
// Witnesses, each expected VIOLATED so the invariants aren't vacuous: attemptsNeverRunOut (attempt 3 is reached),
// neverFailsOutOfAttempts (a task fails having run out), neverMergedAfterARebuild (a rebuilt task merges),
// neverEndsForGoodEarly (a blocked, stopped or pull-request end is reached), neverFailsWithAPullRequest (a follow-up
// on an open pull request ends badly and fails the task), neverResentARebuild (obid makes a rebuild's job from a
// resent request), refusedHeadNeverWaits (a refused head waits, as the feeder does until FEEDLOOP-3 and 4).
// docs/models/oby_feed_loops.qnt's loops_fixed is the target design, FEEDLOOP-3 and 4 included; this is the code.
// The limit: a model checks the design, not that the script implements it; nor the brief's words or reason text.

module oby_feed_attempts {
  pure val MaxAttempts = 3
  pure val Ends = Set("done", "failed", "interrupted", "stopped", "gone")

  var st: str             // running, reviewing, merging, waiting, failed, blocked, merged
  var job: str            // the task's current job: running, done, failed, interrupted, stopped or gone
  var blocked: bool       // its summary's first line starts STATUS: BLOCKED
  var published: bool     // it published a pull request
  var followUp: bool      // obid started a follow-up the feeder hasn't followed yet
  var pr: str             // the task's pull request: none, open, merged, closed
  var refusedHead: bool   // GitHub's queue refused the armed head (or it conflicts)
  var security: bool      // the pull request touches a security path
  var specSaved: bool     // feed/<KEY>.spec.md exists
  var room: bool          // the disk is at or over --min-free-gb
  var attempts: int       // len(builds)
  var note: str           // a failed task's note
  var endKind: str        // why it failed: none, spent, nospec, blocked, stopped, pr, closed
  var withPrRebuilds: int // ghost: builds started again while the task had a pull request
  var endedRebuilds: int  // ghost: builds started again after a blocked or stopped job
  var switched: bool      // this pass only followed a follow-up, as follow()'s loop returns at once
  var owed: bool          // a rebuild's request is saved and obid hasn't made its job (item["request"])
  var resent: bool        // ghost: obid made a rebuild's job from a request resent at a later pass
  var afterPass: bool

  pure def afterAttempt(status: str, blk: bool, pub: bool, hasPr: bool, attempt: int): str =
    if (status == "queued" or status == "running") "follow"
    else if (blk or status == "stopped") "end"
    else if (status == "done" and pub) "review"
    else if (hasPr) "end"
    else if (attempt < MaxAttempts) "again"
    else "end"

  action init = {
    nondet saved = Set(true, false).oneOf()
    all {
      st' = "running", job' = "running", blocked' = false, published' = false, followUp' = false, pr' = "none",
      refusedHead' = false, security' = false, specSaved' = saved, room' = true, attempts' = 1, note' = "",
      endKind' = "none", withPrRebuilds' = 0, endedRebuilds' = 0, switched' = false, owed' = false,
      resent' = false, afterPass' = false
    }
  }

  action keepFeeder = all { st' = st, attempts' = attempts, note' = note, endKind' = endKind,
                            withPrRebuilds' = withPrRebuilds, endedRebuilds' = endedRebuilds, specSaved' = specSaved,
                            switched' = switched, owed' = owed, resent' = resent }

  // ---- the world ----
  action jobEnds = {
    nondet out = Ends.oneOf()
    nondet blk = Set(true, false).oneOf()
    nondet pub = Set(true, false).oneOf()
    nondet sec = Set(true, false).oneOf()
    all {
      job == "running", job' = out, blocked' = (out != "gone" and blk), published' = (out == "done" and pub),
      security' = if (out == "done" and pub and pr == "none") sec else security,
      followUp' = followUp, pr' = pr, refusedHead' = refusedHead, room' = room, keepFeeder, afterPass' = false
    }
  }
  action followUpStarts = all {
    pr == "open", job != "running", st.in(Set("reviewing", "waiting")), not(followUp),
    job' = "running", blocked' = false, published' = false, followUp' = true,
    pr' = pr, refusedHead' = refusedHead, security' = security, room' = room, keepFeeder, afterPass' = false
  }
  action github = {
    nondet happens = Set("merges", "refuses", "closes").oneOf()
    all {
      pr == "open", st.in(Set("reviewing", "merging", "waiting")),
      if (happens == "merges") all { st == "waiting" or (st == "merging" and not(refusedHead)), pr' = "merged",
                                     refusedHead' = refusedHead }
      else if (happens == "refuses") all { st == "merging", not(refusedHead), pr' = pr, refusedHead' = true }
      else all { pr' = "closed", refusedHead' = refusedHead },
      job' = job, blocked' = blocked, published' = published, followUp' = followUp, security' = security,
      room' = room, keepFeeder, afterPass' = false
    }
  }
  action diskChanges = all {
    room' = not(room), job' = job, blocked' = blocked, published' = published, followUp' = followUp, pr' = pr,
    refusedHead' = refusedHead, security' = security, keepFeeder, afterPass' = false
  }
  action world = any { jobEnds, followUpStarts, github, diskChanges }

  // ---- the feeder's pass ----
  action endFor(why: str, kind: str): bool = all {
    st' = "failed", note' = why, endKind' = kind, attempts' = attempts, job' = job, blocked' = blocked,
    published' = published, pr' = pr, refusedHead' = refusedHead, withPrRebuilds' = withPrRebuilds,
    endedRebuilds' = endedRebuilds, owed' = owed, resent' = resent
  }
  action moveTo(target: str, nextPr: str, refused: bool): bool = all {
    st' = target, pr' = nextPr, refusedHead' = refused, note' = note, endKind' = endKind, attempts' = attempts,
    job' = job, blocked' = blocked, published' = published, withPrRebuilds' = withPrRebuilds,
    endedRebuilds' = endedRebuilds, owed' = owed, resent' = resent
  }
  action stay = moveTo(st, pr, refusedHead)

  // post_step() sending a rebuild's saved request: obid makes the job, gives no answer or a 5xx (still owed), or a 4xx
  action postBuild(answer: str, moreWithPr: int, moreEnded: int, fromResend: bool): bool = all {
    pr' = pr, refusedHead' = refusedHead, withPrRebuilds' = withPrRebuilds + moreWithPr,
    endedRebuilds' = endedRebuilds + moreEnded,
    if (answer == "made") all {
      st' = "running", job' = "running", blocked' = false, published' = false, attempts' = attempts + 1,
      note' = note, endKind' = endKind, owed' = false, resent' = (resent or fromResend)
    }
    else if (answer == "lost") all {
      st' = st, job' = job, blocked' = blocked, published' = published, attempts' = attempts, note' = note,
      endKind' = endKind, owed' = true, resent' = resent
    }
    else all {
      st' = "blocked", job' = job, blocked' = blocked, published' = published, attempts' = attempts,
      note' = "obid didn't start the build", endKind' = endKind, owed' = false, resent' = resent
    }
  }

  // attempt_ended() on `again`: a new build against the saved spec, its request saved and sent at once, or a wait
  // for disk, or no spec to build from
  action again(answer: str): bool = if (not(specSaved)) endFor("its spec isn't saved", "nospec")
    else if (not(room)) stay
    else postBuild(answer, if (pr != "none") 1 else 0, if (blocked or job == "stopped") 1 else 0, false)

  action running(answer: str): bool = {
    val verdict = afterAttempt(job, blocked, published, pr != "none", attempts)
    if (verdict == "follow") stay
    else if (verdict == "review") moveTo("reviewing", if (pr == "none") "open" else pr, false)
    else if (verdict == "again") again(answer)
    else if (blocked) endFor("STATUS: BLOCKED", "blocked")
    else if (job == "stopped") endFor("stopped", "stopped")
    else if (pr != "none") endFor(job, "pr")
    else endFor(job, "spent")
  }

  action pass = {
    nondet passes = Set(true, false).oneOf()  // the reviews, holds, gate, lenses and approver, for one head
    nondet answer = Set("made", "lost", "refused").oneOf()  // obid's answer to a build's POST, if one is sent
    all {
      afterPass' = true, room' = room, security' = security, specSaved' = specSaved,
      followUp' = (followUp and not(st.in(Set("reviewing", "waiting")))),
      switched' = (followUp and st.in(Set("reviewing", "waiting"))),
      if (owed) postBuild(answer, 0, 0, true)
      else if (st == "failed" or st == "blocked" or st == "merged") stay
      else if (followUp and st.in(Set("reviewing", "waiting"))) moveTo("running", pr, refusedHead)
      else if (st == "running") running(answer)
      else if (pr == "closed") endFor("its pull request was closed", "closed")
      else if (pr == "merged") moveTo(if (st == "reviewing") "merging" else "merged", pr, refusedHead)
      else if (st == "reviewing") moveTo(if (security or not(passes)) "waiting" else "merging", pr, refusedHead)
      else if (st == "merging" and refusedHead) moveTo("waiting", pr, true)
      else stay
    }
  }

  action step = any { world, pass }

  val attemptsBounded = attempts <= MaxAttempts
  val endsWithReason = (st == "failed" implies note != "")
    and ((st == "failed" and endKind == "spent") implies (attempts == MaxAttempts and note == job))
  val noEndedJobLeft = (afterPass and st == "running" and not(switched))
    implies (job == "running" or not(room) or owed)
  val failsOnlyOutOfAttempts = (st == "failed" and pr == "none" and not(blocked) and job != "stopped" and specSaved)
    implies attempts == MaxAttempts
  val noSecondPullRequest = withPrRebuilds == 0
  val blockedOrStoppedNeverRebuilt = endedRebuilds == 0

  val attemptsNeverRunOut = attempts < MaxAttempts
  val neverFailsOutOfAttempts = not(st == "failed" and endKind == "spent")
  val neverMergedAfterARebuild = not(st == "merged" and attempts > 1)
  val neverEndsForGoodEarly = not(st == "failed" and endKind.in(Set("blocked", "stopped", "pr")))
  val neverFailsWithAPullRequest = not(st == "failed" and endKind == "pr")
  val neverResentARebuild = not(attempts > 1 and resent)
  val refusedHeadNeverWaits = not(st == "waiting" and refusedHead)
}
