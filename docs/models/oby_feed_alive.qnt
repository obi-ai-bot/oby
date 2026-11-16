// verify: main=alive_merged steps=8 invariants=slotsBounded witnesses=readyNeverOrphaned
// verify: main=alive_fixed steps=12 invariants=slotsBounded,readyNeverOrphaned,waitingFollowed witnesses=neverStarted,neverObidDown
//
// The feeder process itself (factory/bin/oby-feed's main loop), unattended: obid may be down for a while (a restart,
// an update), new task files land on dev at any time, and a task may sit `waiting` on the owner (a security path).
// readyNeverOrphaned: while a task is ready to start or waiting to be followed, a feeder is still running to do it.
// waitingFollowed: a waiting task's merge is recorded at the next pass with obid up.
// slotsBounded: never more tasks under way than --max.
// alive_merged: main() as on origin/dev: a pass with nothing under way and nothing started ends the process
//   ("nothing under way and nothing ready: stopping"), and obid unreachable raises URLError out of api(), which
//   nothing catches. Expected: readyNeverOrphaned is VIOLATED (a witness there).
// alive_fixed: the loop runs until it is stopped; a pass that can't reach obid (or GitHub) is skipped and tried at the
//   next one. Expected: all three hold.
// The limit: a model checks the design, not that the script implements it.

module alive_common {
  pure val Max = 2
}

module alive_merged {
  import alive_common.*
  var alive: bool
  var obidUp: bool
  var ready: int       // task files ready to start
  var running: int
  var waitingMerged: bool   // a waiting task whose pull request the owner merged, not yet recorded
  var waiting: bool
  action init = all { alive' = true, obidUp' = true, ready' = 0, running' = 0, waiting' = false, waitingMerged' = false }
  action newTask = all { ready < 3, ready' = ready + 1, alive' = alive, obidUp' = obidUp, running' = running,
                         waiting' = waiting, waitingMerged' = waitingMerged }
  action obidFlips = all { obidUp' = not(obidUp), alive' = alive, ready' = ready, running' = running,
                           waiting' = waiting, waitingMerged' = waitingMerged }
  action taskEnds = {
    nondet w = Set(true, false).oneOf()     // merged, or waits on the owner
    all { running > 0, running' = running - 1, waiting' = (waiting or w), alive' = alive, obidUp' = obidUp, ready' = ready,
          waitingMerged' = waitingMerged }
  }
  action ownerMerges = all { waiting, not(waitingMerged), waitingMerged' = true, waiting' = waiting, alive' = alive,
                             obidUp' = obidUp, ready' = ready, running' = running }
  action pass = {
    val start = if (Max - running < ready) Max - running else ready
    all { alive,
          if (not(obidUp)) all { alive' = false, ready' = ready, running' = running, waiting' = waiting, waitingMerged' = waitingMerged }
          else all { running' = running + start, ready' = ready - start,
                     waiting' = (waiting and not(waitingMerged)), waitingMerged' = false,
                     alive' = (running + start > 0) },
          obidUp' = obidUp }
  }
  action step = any { newTask, obidFlips, taskEnds, ownerMerges, pass }
  val slotsBounded = running <= Max
  val readyNeverOrphaned = (ready > 0 or waiting) implies alive
}

module alive_fixed {
  import alive_common.*
  var alive: bool
  var obidUp: bool
  var ready: int
  var running: int
  var waitingMerged: bool
  var waiting: bool
  var started: int
  var obidWentDown: bool
  var afterUpPass: bool
  action init = all { alive' = true, obidUp' = true, ready' = 0, running' = 0, waiting' = false, waitingMerged' = false,
                      started' = 0, obidWentDown' = false, afterUpPass' = false }
  action newTask = all { ready < 3, ready' = ready + 1, alive' = alive, obidUp' = obidUp, running' = running,
                         waiting' = waiting, waitingMerged' = waitingMerged, started' = started, obidWentDown' = obidWentDown,
                         afterUpPass' = false }
  action obidFlips = all { obidUp' = not(obidUp), obidWentDown' = (obidWentDown or obidUp), alive' = alive, ready' = ready,
                           running' = running, waiting' = waiting, waitingMerged' = waitingMerged, started' = started,
                           afterUpPass' = false }
  action taskEnds = {
    nondet w = Set(true, false).oneOf()
    all { running > 0, running' = running - 1, waiting' = (waiting or w), alive' = alive, obidUp' = obidUp, ready' = ready,
          waitingMerged' = waitingMerged, started' = started, obidWentDown' = obidWentDown, afterUpPass' = false }
  }
  action ownerMerges = all { waiting, not(waitingMerged), waitingMerged' = true, waiting' = waiting, alive' = alive,
                             obidUp' = obidUp, ready' = ready, running' = running, started' = started,
                             obidWentDown' = obidWentDown, afterUpPass' = false }
  action pass = {
    val start = if (Max - running < ready) Max - running else ready
    all { alive, alive' = true, obidUp' = obidUp, obidWentDown' = obidWentDown,
          if (not(obidUp)) all { ready' = ready, running' = running, waiting' = waiting, waitingMerged' = waitingMerged,
                                 started' = started, afterUpPass' = false }
          else all { running' = running + start, ready' = ready - start, started' = started + start,
                     waiting' = (waiting and not(waitingMerged)), waitingMerged' = false, afterUpPass' = true } }
  }
  action step = any { newTask, obidFlips, taskEnds, ownerMerges, pass }
  val slotsBounded = running <= Max
  val readyNeverOrphaned = (ready > 0 or waiting) implies alive
  val waitingFollowed = afterUpPass implies not(waitingMerged)
  val neverStarted = started == 0
  val neverObidDown = not(obidWentDown and started > 0)
}
