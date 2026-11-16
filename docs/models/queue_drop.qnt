// verify: main=queue_drop steps=12 invariants=oneDropRound,noRoundOffOwnBranch,droppedHeadNeverArmed,stopSaysWhy witnesses=noDropRound,noDropOffOwnBranch,noNewHeadArmedAfterRound,noStop
// One job whose pull request obid hands to a base with a merge queue, and what happens when the queue drops its head
// (task A00-98; docs/contracts/merging.md, MERGING-55 to 59). It models, by function:
// - Jobs::by_github in core/crates/obid/src/jobs/by_github.rs, with crate::merge::arm in core/crates/obid/src/merge.rs:
//   obid arms GitHub's auto-merge on a head that isn't one the queue dropped (arm), and never on a head in
//   Job::queue_drops;
// - GitHub's merge queue: it merges the head it holds (queueMerges), or drops it, leaving obid's `auto_merge` set until
//   obid reads it (queueDrops);
// - Jobs::dropped in jobs/by_github.rs, with crate::merge::queue_state in merge.rs: the follower's read finds the armed
//   head open with neither a request nor a queue entry, clears `auto_merge` and notes the drop (noteDrop);
// - Jobs::merges in jobs/by_github.rs, Jobs::drop_round and may_round in core/crates/obid/src/jobs/after_pr.rs: the
//   drop's stop first tries the one round, which may_round admits only on obid's own branch for the job's one drop
//   (mayRound), and may not start for another reason; else Jobs::stop_drop marks it stopped, and own work's
//   conversation is told (tell_queue_stopped in core/crates/obid/src/conversation.rs) while other work gets its card
//   (decide);
// - the round's agent turn, Jobs::onto_base and the publish in core/crates/obid/src/jobs/owner_run.rs: it ends with a
//   new head, or the head as it was (roundEnds);
// - a later pass of Jobs::merges at a dropped head (crate::merge::dropped): no second round, so it stops
//   (passAtDroppedHead).
// oneDropRound, noRoundOffOwnBranch, droppedHeadNeverArmed and stopSaysWhy are the properties, each expected to hold
// in every sequence up to 12 steps (quint verify --max-steps=12 --invariant=<name>). noDropRound, noDropOffOwnBranch,
// noNewHeadArmedAfterRound and noStop are their witnesses, each expected to be violated, so a pass of its invariant is
// not vacuous.
// The limit: a model checks the design, not that the code implements it. Nothing here replays a trace against the
// code (Quint Connect is not used). obid never arms a branch it didn't make (crate::merge::ready's own-branch hold), so
// a drop off its own branch can't happen in the code; the model lets it, to show the round's guard alone keeps it from
// a round.
module queue_drop {
  var head: int
  var armed: bool
  var held: bool
  var merged: bool
  var droppedHeads: Set[int]
  var drops: int
  var rounds: int
  var pending: bool
  var running: bool
  var stopped: bool
  var told: bool
  var carded: bool
  var ownBranch: bool
  var own: bool
  val mayRound = ownBranch and drops == 1 and rounds == 0
  action init = {
    nondet b = Set(true, false).oneOf()
    nondet o = Set(true, false).oneOf()
    all {
      head' = 1, armed' = false, held' = false, merged' = false, droppedHeads' = Set(), drops' = 0, rounds' = 0,
      pending' = false, running' = false, stopped' = false, told' = false, carded' = false, ownBranch' = b, own' = o,
    }
  }
  action arm = all {
    not(merged), not(running), not(stopped), not(armed), not(droppedHeads.contains(head)),
    armed' = true, held' = true,
    head' = head, merged' = merged, droppedHeads' = droppedHeads, drops' = drops, rounds' = rounds, pending' = pending,
    running' = running, stopped' = stopped, told' = told, carded' = carded, ownBranch' = ownBranch, own' = own,
  }
  action queueMerges = all {
    armed, held,
    merged' = true,
    head' = head, armed' = armed, held' = held, droppedHeads' = droppedHeads, drops' = drops, rounds' = rounds,
    pending' = pending, running' = running, stopped' = stopped, told' = told, carded' = carded, ownBranch' = ownBranch,
    own' = own,
  }
  action queueDrops = all {
    armed, held,
    held' = false,
    head' = head, armed' = armed, merged' = merged, droppedHeads' = droppedHeads, drops' = drops, rounds' = rounds,
    pending' = pending, running' = running, stopped' = stopped, told' = told, carded' = carded, ownBranch' = ownBranch,
    own' = own,
  }
  action noteDrop = all {
    armed, not(held), not(merged),
    armed' = false, droppedHeads' = droppedHeads.union(Set(head)), drops' = drops + 1, pending' = true,
    head' = head, held' = held, merged' = merged, rounds' = rounds, running' = running, stopped' = stopped,
    told' = told, carded' = carded, ownBranch' = ownBranch, own' = own,
  }
  // The stop that says why: own work's conversation is told, other work gets a card.
  action stops = all {
    stopped' = true, told' = own, carded' = not(own),
  }
  action decide = {
    nondet starts = Set(true, false).oneOf()
    all {
      pending,
      pending' = false,
      if (mayRound and starts)
        all { rounds' = rounds + 1, running' = true, stopped' = stopped, told' = told, carded' = carded }
      else
        all { rounds' = rounds, running' = running, stops },
      head' = head, armed' = armed, held' = held, merged' = merged, droppedHeads' = droppedHeads, drops' = drops,
      ownBranch' = ownBranch, own' = own,
    }
  }
  action roundEnds = {
    nondet moved = Set(true, false).oneOf()
    all {
      running,
      running' = false, head' = if (moved) head + 1 else head,
      armed' = armed, held' = held, merged' = merged, droppedHeads' = droppedHeads, drops' = drops, rounds' = rounds,
      pending' = pending, stopped' = stopped, told' = told, carded' = carded, ownBranch' = ownBranch, own' = own,
    }
  }
  // A round that left the head as it was gets no second round: it stops as decide does.
  action passAtDroppedHead = all {
    not(running), not(pending), not(stopped), not(merged), droppedHeads.contains(head), rounds == 1,
    stops,
    head' = head, armed' = armed, held' = held, merged' = merged, droppedHeads' = droppedHeads, drops' = drops,
    rounds' = rounds, pending' = pending, running' = running, ownBranch' = ownBranch, own' = own,
  }
  action stutter = all {
    head' = head, armed' = armed, held' = held, merged' = merged, droppedHeads' = droppedHeads, drops' = drops,
    rounds' = rounds, pending' = pending, running' = running, stopped' = stopped, told' = told, carded' = carded,
    ownBranch' = ownBranch, own' = own,
  }
  action step = any { arm, queueMerges, queueDrops, noteDrop, decide, roundEnds, passAtDroppedHead, stutter }
  val oneDropRound = rounds <= 1
  val noRoundOffOwnBranch = not(ownBranch) implies rounds == 0
  val droppedHeadNeverArmed = armed implies not(droppedHeads.contains(head))
  val stopSaysWhy = and {
    stopped implies ((own and told and not(carded)) or (not(own) and carded and not(told))),
    not(stopped) implies (not(told) and not(carded)),
  }
  val noDropRound = rounds == 0
  val noDropOffOwnBranch = not(not(ownBranch) and drops > 0)
  val noNewHeadArmedAfterRound = not(armed and rounds == 1)
  val noStop = not(stopped)
}
