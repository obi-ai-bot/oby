// verify: main=oby_factory steps=16 invariants=safe witnesses=neverMergesByItself,neverFails,neverNeedsYou
// verify: main=oby_feeder steps=12 invariants=feederSafe witnesses=neverAllDone
// The factory inside Oby: what happens to one task from the owner's words to dev, and how tasks are fed.
// It models the design, by rule:
// - AGENTS.md, "Non-negotiable": nothing goes out unless the secret scan is clean (scanClean is the only push).
// - AGENTS.md, "Defaults": nothing goes out by itself unless the job's check passed (scanClean needs checked == tip).
// - docs/contracts/merging.md MERGING-13 to 16: a job no app signed merges by itself only after two approving
//   reviews of its exact tip by different tools, or the owner's click (mergeTwo, ownerMerges).
// - MERGING-44 to 53 and the owner's answer on A00-46: the owner's own work merges by itself, once Oby's review of
//   that tip, by another tool than the builder's, passed too (mergeOwn).
// - MERGING-41: a merge into dev goes through GitHub's merge queue and its Check (queuePass, queueFail).
// - A00-67 (proposed): a failure with rounds left starts the next round with no card; out of rounds, the task
//   ends failed with its reason (again, nextRound).
// The limit: a model checks the design, not that obid implements it.
module oby_factory {
  pure val MaxRounds = 3
  pure val Reviewers = Set("claude", "codex")   // Cursor never reviews
  type Review = { tool: str, tip: int, ok: bool }

  var stage: str          // queued, work, needsYou, check, publish, review, queue, merged, failed
  var builder: str
  var signed: bool        // the owner's own app signed the words: own work
  var tip: int           // the branch's tip, counted in commits
  var checked: int        // the tip the check last passed on (-1: none)
  var scanned: int        // the tip the secret scan last passed on
  var pushed: int         // the tip obid last pushed
  var reviews: Set[Review]
  var round: int
  var ownerClicked: bool
  var mergedHead: int

  action init = {
    nondet b = Set("claude", "codex", "cursor").oneOf()
    nondet s = Set(true, false).oneOf()
    all { stage' = "queued", builder' = b, signed' = s, tip' = 0, checked' = -1, scanned' = -1, pushed' = -1,
          reviews' = Set(), round' = 0, ownerClicked' = false, mergedHead' = -1 }
  }

  action keepId = all { builder' = builder, signed' = signed }
  action keepProof = all { checked' = checked, scanned' = scanned, pushed' = pushed, mergedHead' = mergedHead }
  action keepRest = all { reviews' = reviews, ownerClicked' = ownerClicked }

  val again = if (round < MaxRounds) "work" else "failed"
  val nextRound = if (round < MaxRounds) round + 1 else round
  val okTools = reviews.filter(r => r.tip == pushed and r.ok).map(r => r.tool)
  val rejected = reviews.exists(r => r.tip == pushed and not(r.ok))

  action start = all { stage == "queued", stage' = "work", round' = 1, tip' = tip, keepId, keepProof, keepRest }
  action commit = all { stage == "work", stage' = "check", tip' = tip + 1, round' = round, keepId, keepProof, keepRest }
  action ask = all { stage == "work", stage' = "needsYou", tip' = tip, round' = round, keepId, keepProof, keepRest }
  action answer = all { stage == "needsYou", stage' = "work", tip' = tip, round' = round, keepId, keepProof, keepRest }
  action ownerStops = all { stage == "needsYou", stage' = "failed", tip' = tip, round' = round, keepId, keepProof, keepRest }
  action checkPass = all { stage == "check", stage' = "publish", checked' = tip, scanned' = scanned, pushed' = pushed,
                           mergedHead' = mergedHead, tip' = tip, round' = round, keepId, keepRest }
  action checkFail = all { stage == "check", stage' = again, round' = nextRound, tip' = tip, keepId, keepProof, keepRest }
  action scanClean = all { stage == "publish", checked == tip, stage' = "review", scanned' = tip, pushed' = tip,
                           checked' = checked, mergedHead' = mergedHead, tip' = tip, round' = round, keepId, keepRest }
  action scanDirty = all { stage == "publish", stage' = again, round' = nextRound, tip' = tip, keepId, keepProof, keepRest }
  action review = {
    nondet t = Reviewers.oneOf()
    nondet ok = Set(true, false).oneOf()
    all { stage == "review", not(reviews.exists(r => r.tip == pushed and r.tool == t)),
          reviews' = reviews.union(Set({ tool: t, tip: pushed, ok: ok })),
          stage' = if (ok) "review" else again, round' = if (ok) round else nextRound,
          tip' = tip, ownerClicked' = ownerClicked, keepId, keepProof }
  }
  action mergeOwn = all { stage == "review", signed, not(rejected), okTools.exists(t => t != builder),
                          stage' = "queue", tip' = tip, round' = round, keepId, keepProof, keepRest }
  action mergeTwo = all { stage == "review", not(rejected), okTools.size() >= 2,
                          stage' = "queue", tip' = tip, round' = round, keepId, keepProof, keepRest }
  action ownerMerges = all { stage == "review", stage' = "queue", ownerClicked' = true, reviews' = reviews,
                             tip' = tip, round' = round, keepId, keepProof }
  // a commit that lands after the check (the owner's words while the job wraps up, a rebase): back to the check
  action lateCommit = all { stage.in(Set("publish", "review")), stage' = "check", tip' = tip + 1, round' = round,
                            keepId, keepProof, keepRest }
  action queuePass = all { stage == "queue", stage' = "merged", mergedHead' = pushed, checked' = checked,
                           scanned' = scanned, pushed' = pushed, tip' = tip, round' = round, keepId, keepRest }
  action queueFail = all { stage == "queue", stage' = again, round' = nextRound, tip' = tip, keepId, keepProof, keepRest }

  action step = any { start, commit, ask, answer, ownerStops, checkPass, checkFail, scanClean, scanDirty, review, lateCommit,
                      mergeOwn, mergeTwo, ownerMerges, queuePass, queueFail }

  val autoMerged = stage == "merged" and not(ownerClicked)
  // what must always hold
  val scanBeforePush = pushed == scanned
  val mergedIsChecked = autoMerged implies (mergedHead == checked and mergedHead == scanned and mergedHead == tip)
  val mergedIsReviewed = autoMerged implies (
    (signed and reviews.exists(r => r.tip == mergedHead and r.ok and r.tool != builder))
    or reviews.filter(r => r.tip == mergedHead and r.ok).map(r => r.tool).size() >= 2)
  val noRejectedMerge = autoMerged implies not(reviews.exists(r => r.tip == mergedHead and not(r.ok)))
  val roundsBounded = round <= MaxRounds
  val safe = scanBeforePush and mergedIsChecked and mergedIsReviewed and noRejectedMerge and roundsBounded
  // witnesses: each is expected to be violated, so a pass of `safe` is not vacuous
  val neverMergesByItself = not(autoMerged)
  val neverFails = stage != "failed"
  val neverNeedsYou = stage != "needsYou"
}

// Feeding tasks: which task files start, as the factory's queue decides today (factory/bin/job queue) and obid
// would have to: a task starts only when its dependencies merged, no running task shares a file with it, and a
// slot is free.
module oby_feeder {
  pure val Tasks = Set("a", "b", "c")
  pure val MaxRunning = 2
  pure def deps(t: str): Set[str] = if (t == "c") Set("a") else Set()
  pure def files(t: str): Set[str] =
    if (t == "a") Set("jobs.rs") else if (t == "b") Set("jobs.rs", "api.rs") else Set("docs")

  var state: str -> str   // open, running, done, failed

  val running = Tasks.filter(t => state.get(t) == "running")

  action init = state' = Tasks.mapBy(t => "open")
  action startTask = {
    nondet t = Tasks.oneOf()
    all { state.get(t) == "open", deps(t).forall(d => state.get(d) == "done"), running.size() < MaxRunning,
          running.forall(r => files(r).intersect(files(t)) == Set()), state' = state.set(t, "running") }
  }
  action finish = {
    nondet t = Tasks.oneOf()
    nondet ok = Set(true, false).oneOf()
    all { state.get(t) == "running", state' = state.set(t, if (ok) "done" else "failed") }
  }
  action retry = {
    nondet t = Tasks.oneOf()
    all { state.get(t) == "failed", state' = state.set(t, "open") }
  }
  action step = any { startTask, finish, retry }

  val capacity = running.size() <= MaxRunning
  val noSharedFiles = running.forall(r => running.forall(q => r == q or files(r).intersect(files(q)) == Set()))
  val depsFirst = Tasks.forall(t => state.get(t).in(Set("running", "done")) implies deps(t).forall(d => state.get(d) == "done"))
  val feederSafe = capacity and noSharedFiles and depsFirst
  // witness: expected to be violated (all three tasks do get done)
  val neverAllDone = not(Tasks.forall(t => state.get(t) == "done"))
}
