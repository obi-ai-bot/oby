// verify: main=loops_merged steps=12 invariants=attemptsBounded,neverMergedAfterAFailure witnesses=nothingOwed
// verify: main=loops_fixed steps=16 invariants=attemptsBounded,nothingOwed,endsWithReason witnesses=neverMergedAfterAFailure,neverRebuilt
//
// The factory's outer loop, one task: factory/bin/oby-feed starts an Oby job, follows it to a pull request, arms
// GitHub's merge queue, and must keep the task moving with nobody watching. What can happen to it: the job ends
// failed or interrupted (a crash, obid restarting, rounds run out), the queue refuses the armed head (its Check fails
// on the combined code), or dev moves and the branch conflicts.
// nothingOwed: right after a feeder pass, the task is merged, is being worked on (a job running, a head in the queue,
//   reviews pending), or has ended for good with its reason: never parked where only a person can move it while
//   attempts are left.
// attemptsBounded: the feeder starts at most MaxAttempts jobs for one task (no endless loop).
// endsWithReason: a task that stopped for good carries the reason of its last attempt.
// loops_merged: oby-feed as on origin/dev (A00-80 merged): a job that ends anything but done-with-a-pull-request
//   makes the task `failed` for good; a refused or conflicting head waits for someone to push a new one.
//   Expected: nothingOwed is VIOLATED (a witness there), and neverMergedAfterAFailure HOLDS: one failed job ends the task.
// loops_fixed: a failed or interrupted job starts the task again with the last reason in its brief, and a refused
//   or conflicting head starts a follow-up on the same branch ("rebase onto dev, make the queue's Check pass"), each
//   while attempts are left; out of attempts it ends `failed` with the reason. Expected: all three hold.
// The limit: a model checks the design, not that the script implements it.

module loops_common {
  pure val MaxAttempts = 3
}

module loops_merged {
  import loops_common.*
  var st: str          // running, reviewing, merging, waiting, failed, merged
  var job: str         // running, done, failed, interrupted
  var pr: str          // none, open, merged
  var conflict: bool   // the branch conflicts with dev, or the queue's Check fails on the combined code
  var attempts: int
  var reason: str
  var failures: int    // jobs that ended failed or interrupted, for the witness
  var afterPass: bool

  action init = all { st' = "running", job' = "running", pr' = "none", conflict' = false, attempts' = 1,
                      reason' = "", failures' = 0, afterPass' = false }
  action keepFeed = all { st' = st, attempts' = attempts, reason' = reason }

  // ---- the world ----
  action jobEnds = {
    nondet out = Set("done", "failed", "interrupted").oneOf()
    all { job == "running", job' = out, pr' = if (out == "done" and pr == "none") "open" else pr,
          conflict' = false, failures' = if (out == "done") failures else failures + 1, keepFeed, afterPass' = false }
  }
  action devMoves = all { pr == "open", not(conflict), conflict' = true, job' = job, pr' = pr, failures' = failures,
                          keepFeed, afterPass' = false }
  // the queue merges a head that doesn't conflict
  action queueMerges = all { st == "merging", pr == "open", not(conflict), pr' = "merged", job' = job,
                             conflict' = conflict, failures' = failures, keepFeed, afterPass' = false }
  action world = any { jobEnds, devMoves, queueMerges }

  // ---- the feeder's pass as merged ----
  action pass = all {
    job' = job, pr' = pr, conflict' = conflict, failures' = failures, afterPass' = true, attempts' = attempts,
    if (st == "running" and job == "running") all { st' = st, reason' = reason }
    else if (st == "running" and job != "done") all { st' = "failed", reason' = job }
    else if (st == "running") all { st' = "reviewing", reason' = reason }
    else if (st == "reviewing") all { st' = if (conflict) "waiting" else "merging", reason' = reason }   // reviews pass
    else if (st == "merging" and pr == "merged") all { st' = "merged", reason' = reason }
    else if (st == "merging" and conflict) all { st' = "waiting", reason' = "the queue refused the head" }
    else if (st == "waiting" and pr == "merged") all { st' = "merged", reason' = reason }
    else all { st' = st, reason' = reason }
  }
  action step = any { world, pass }

  val inMotion = (st == "running" and job == "running") or (st == "merging" and not(conflict)) or st == "reviewing"
  val owed = not(st == "merged" or inMotion or (st == "failed" and attempts >= MaxAttempts))
  val nothingOwed = afterPass implies not(owed)
  val attemptsBounded = attempts <= MaxAttempts
  val neverMergedAfterAFailure = not(st == "merged" and failures > 0)
}

module loops_fixed {
  import loops_common.*
  var st: str
  var job: str
  var pr: str
  var conflict: bool
  var attempts: int
  var reason: str
  var failures: int
  var rebuilt: bool    // a follow-up was started to rebase or fix a refused head, for the witness
  var afterPass: bool

  action init = all { st' = "running", job' = "running", pr' = "none", conflict' = false, attempts' = 1,
                      reason' = "", failures' = 0, rebuilt' = false, afterPass' = false }

  action jobEnds = {
    nondet out = Set("done", "failed", "interrupted").oneOf()
    all { job == "running", job' = out, pr' = if (out == "done" and pr == "none") "open" else pr,
          conflict' = false, failures' = if (out == "done") failures else failures + 1,
          st' = st, attempts' = attempts, reason' = reason, rebuilt' = rebuilt, afterPass' = false }
  }
  action devMoves = all { pr == "open", not(conflict), conflict' = true, job' = job, pr' = pr, failures' = failures,
                          st' = st, attempts' = attempts, reason' = reason, rebuilt' = rebuilt, afterPass' = false }
  action queueMerges = all { st == "merging", pr == "open", not(conflict), pr' = "merged", job' = job,
                             conflict' = conflict, failures' = failures, st' = st, attempts' = attempts, reason' = reason,
                             rebuilt' = rebuilt, afterPass' = false }
  action world = any { jobEnds, devMoves, queueMerges }

  // again: a new job for the task (a fresh build when no pull request, else a follow-up on its branch)
  val left = attempts < MaxAttempts
  action pass = all {
    failures' = failures, afterPass' = true, pr' = pr,
    if (st == "running" and job == "running")
      all { st' = st, job' = job, conflict' = conflict, attempts' = attempts, reason' = reason, rebuilt' = rebuilt }
    else if (st == "running" and job != "done")
      if (left) all { st' = "running", job' = "running", conflict' = conflict, attempts' = attempts + 1, reason' = job, rebuilt' = rebuilt }
      else all { st' = "failed", job' = job, conflict' = conflict, attempts' = attempts, reason' = job, rebuilt' = rebuilt }
    else if (st == "running")
      all { st' = "reviewing", job' = job, conflict' = conflict, attempts' = attempts, reason' = reason, rebuilt' = rebuilt }
    else if (st == "merging" and pr == "merged")
      all { st' = "merged", job' = job, conflict' = conflict, attempts' = attempts, reason' = reason, rebuilt' = rebuilt }
    else if ((st == "reviewing" or st == "merging" or st == "waiting") and conflict)
      if (left) all { st' = "running", job' = "running", conflict' = conflict, attempts' = attempts + 1,
                      reason' = "the queue refused the head", rebuilt' = true }
      else all { st' = "failed", job' = job, conflict' = conflict, attempts' = attempts, reason' = "the queue refused the head", rebuilt' = rebuilt }
    else if (st == "reviewing")
      all { st' = "merging", job' = job, conflict' = conflict, attempts' = attempts, reason' = reason, rebuilt' = rebuilt }
    else all { st' = st, job' = job, conflict' = conflict, attempts' = attempts, reason' = reason, rebuilt' = rebuilt }
  }
  action step = any { world, pass }

  val inMotion = (st == "running" and job == "running") or (st == "merging" and not(conflict)) or st == "reviewing"
  val owed = not(st == "merged" or inMotion or (st == "failed" and attempts >= MaxAttempts))
  val nothingOwed = afterPass implies not(owed)
  val attemptsBounded = attempts <= MaxAttempts
  val endsWithReason = st == "failed" implies reason != ""
  val neverMergedAfterAFailure = not(st == "merged" and failures > 0)
  val neverRebuilt = not(rebuilt and st == "merged")
}
