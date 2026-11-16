// verify: main=oby_feed_lenses steps=12 invariants=approverOnlyAfterEveryLensPassed,changesNeverReachApprover,brokenLensNeverReachesApprover,staleLensVerdictNeverReachesApprover witnesses=approverNeverAsked,noLensAskedForChanges,noLensBroke,headNeverMovedUnderLenses
//
// factory/bin/oby-feed, one task: follow() with its lens step (ask_lenses, post_lenses, read_lenses and check_again's
// call of ask_approver), from oby_feed_approver's world in docs/models/oby_feed_approver.qnt, copied here because
// check_models() sends one file alone. A head approved twice by different tools, off security paths, goes to `lenses`:
// the feeder starts one review job per routed lens (`code` always, `security` and `proofs` as route.reviewers() gives
// them, chosen once at init) and asks the approver only once every one of them passed that head. Each lens job starts
// when obid gets to it and reads the pull request's tip as it is then (its startCommit), then ends pass, changes,
// malformed or failed, or is gone. Anyone may push a new head while the pull request is open, or force-push it back to
// a head it had before. read_lenses() sees the pull request's head through `gh pr view` (`look`) and reads the lens
// jobs after it, so the head may move between that look and its decision. `refused` holds one head; the heads a lens
// asked changes for or gave no verdict on are held for good (`held`). A reviewer may turn its approval into a change
// request while the lenses run; check_again() runs may_arm() again before it asks the approver, so the task then waits.
// Invariants (FEEDLENS-3 to 6). Each reads what the lenses said when the approver was asked (the ghosts `passed` and
// `askedWith`), not only the feeder's own holds, so a feeder that sent a `changes`, malformed, failed or gone lens on
// breaks one:
// - approverOnlyAfterEveryLensPassed: every tip the approver was asked about has a pass from every routed lens that
//   read that tip.
// - changesNeverReachApprover: no lens had said changes when the approver was asked, nor is a tip the feeder held for
//   changes ever asked about.
// - brokenLensNeverReachesApprover: no lens had failed, was gone or answered malformed when the approver was asked,
//   nor is a tip the feeder held for that ever asked about.
// - staleLensVerdictNeverReachesApprover: in `approving`, the asked tip is the one the lenses were asked about, every
//   routed lens read it, and it is the head the feeder's look saw (FEEDLENS-6's both cases: a lens that read another
//   head, and every lens passing OLD while the pull request is at NEW).
// Witnesses, each expected VIOLATED so the model shows the case can happen: approverNeverAsked (the approver is
// asked), noLensAskedForChanges (a lens asks for changes), noLensBroke (a lens breaks), headNeverMovedUnderLenses (a
// head is pushed while the lenses read the old one).
// The limit: a model checks the design, not that the script implements it; nor what a lens's words say, only that the
// feeder acts on its first line alone. It ends at `approving`: the approver and the arming after it are
// oby_feed_approver.qnt's. The gate, the models and Kani are folded into `canMergeAt`'s security flag, and a tip
// pushed back gets fresh lens jobs here, where obid's receipts would give back the old ones (read_lenses() then sees
// they read another head). Depth 12 reaches a head pushed between the feeder's look and its decision (staleLens fails
// if the approver is asked about the pull request's tip in place of `lensTip`), and, from the task found under review,
// every lens passing OLD while NEW is approved twice and seen (staleLens fails without the `seenTip != lensTip` test);
// but not a held head coming back after another head was refused: factory/tests/test_oby_feed.py covers that
// (FEEDLENS-4).

module oby_feed_lenses {
  pure val Tools = Set("claude", "codex")
  pure val Lenses = Set("code", "security", "proofs")
  pure val MaxRounds = 2
  pure val MaxTip = 4    // pushes reach at most this head: enough for a head pushed away and back within 12 steps
  type Review = { tool: str, tip: int, ok: bool }
  pure def okTools(rs: Set[Review], t: int): Set[str] = rs.filter(r => r.tip == t and r.ok).map(r => r.tool)
  pure def asked(rs: Set[Review], t: int): bool = rs.exists(r => r.tip == t and not(r.ok))

  var st: str            // open, running, reviewing, lenses, approving, waiting, failed, merged
  var job: str           // none, running, done, failed: Oby's newest build job of the task
  var pr: str            // none, open, merged
  var tip: int           // the pull request's head: a commit, numbered in the order it was first pushed
  var top: int           // the newest head ever pushed
  var reviews: Set[Review]
  var rounds: int
  var security: bool     // the change touches a security path
  var routed: Set[str]   // the lenses route.reviewers() gives the task: `code` and any of the others
  var rejectedTip: int   // the head the feeder holds (`refused`): only the last one held
  var held: Set[int]     // the heads a lens asked changes for or gave no verdict on, held for good (`held`)
  var lensTip: int       // `lensHead`: the tip the feeder asked its lenses about
  var lensStatus: str -> str  // each lens job: none, asked, running, pass, changes, malformed, failed, gone
  var lensRead: str -> int    // each lens job's startCommit; -1 before it starts
  var seenTip: int       // the head `gh pr view` gave read_lenses() this pass; -1 before it looked
  var askedTip: int      // `approverHead`: the tip the feeder asked its approver about
  var approverTips: Set[int]      // ghost: tips the approver was asked about
  var passed: Set[(str, int)]     // ghost: (lens, the tip it read) for each routed lens that said pass, when the
                                  // approver was asked
  var askedWith: Set[(str, int, str)]  // ghost: (lens, the tip it read, what it said) for each routed lens, when the
                                       // approver was asked
  var askedSeen: int              // ghost: the head the feeder's look saw when it asked the approver
  var changedTips: Set[int]       // ghost: tips a lens asked changes for
  var brokenTips: Set[int]        // ghost: tips whose lens failed, was gone or answered malformed

  def canMergeAt(t: int): bool = okTools(reviews, t).size() >= 2 and not(asked(reviews, t)) and not(security)

  // a task from its start, or one the feeder finds published and under review (a feeder started again): the second
  // brings a head moved under passing lenses, and approved twice itself, within the depth
  action init = {
    nondet sec = Set(true, false).oneOf()
    nondet lenses = Lenses.powerset().filter(s => s.contains("code")).oneOf()
    nondet fresh = Set(true, false).oneOf()
    all { st' = if (fresh) "open" else "reviewing", job' = if (fresh) "none" else "done",
          pr' = if (fresh) "none" else "open", tip' = if (fresh) 0 else 1, top' = if (fresh) 0 else 1, reviews' = Set(),
          rounds' = if (fresh) 0 else 1, security' = sec,
          routed' = lenses, rejectedTip' = -1, held' = Set(), lensTip' = -1, lensStatus' = Lenses.mapBy(l => "none"),
          lensRead' = Lenses.mapBy(l => -1), seenTip' = -1, askedTip' = -1, approverTips' = Set(), passed' = Set(),
          askedWith' = Set(), askedSeen' = -1, changedTips' = Set(), brokenTips' = Set() }
  }
  action keepWorld = all { job' = job, pr' = pr, tip' = tip, top' = top, reviews' = reviews, rounds' = rounds,
                           security' = security, routed' = routed }
  action keepLensJobs = all { lensStatus' = lensStatus, lensRead' = lensRead }
  action keepGhosts = all { approverTips' = approverTips, passed' = passed, askedWith' = askedWith,
                            askedSeen' = askedSeen, changedTips' = changedTips, brokenTips' = brokenTips }
  action keepFeeder = all { st' = st, rejectedTip' = rejectedTip, held' = held, lensTip' = lensTip, seenTip' = seenTip,
                            askedTip' = askedTip, keepGhosts }
  // the feeder moves the task to `s` and changes nothing else; the pass's look at the pull request ends with it
  action moveTo(s: str): bool = all { st' = s, rejectedTip' = rejectedTip, held' = held, lensTip' = lensTip,
                                   seenTip' = -1, askedTip' = askedTip, keepLensJobs, keepGhosts }

  // ---- the world: Oby, its reviewers, the lens jobs, GitHub, the owner, anyone who pushes ----
  action jobEnds = {
    nondet ok = Set(true, false).oneOf()
    all { job == "running", job' = if (ok) "done" else "failed",
          pr' = if (ok and pr == "none") "open" else pr, tip' = if (ok) top + 1 else tip, top' = if (ok) top + 1 else top,
          reviews' = reviews, rounds' = rounds, security' = security, routed' = routed, keepFeeder, keepLensJobs }
  }
  action review = {
    nondet t = Tools.oneOf()
    nondet ok = Set(true, false).oneOf()
    all { job == "done", pr == "open", not(reviews.exists(r => r.tip == tip and r.tool == t)),
          reviews' = reviews.union(Set({ tool: t, tip: tip, ok: ok })),
          job' = job, pr' = pr, tip' = tip, top' = top, rounds' = rounds, security' = security, routed' = routed,
          keepFeeder, keepLensJobs }
  }
  // a reviewer turns its approval of the tip into a change request
  action reviewAsksChanges = {
    nondet t = Tools.oneOf()
    all { reviews.contains({ tool: t, tip: tip, ok: true }),
          reviews' = reviews.exclude(Set({ tool: t, tip: tip, ok: true })).union(Set({ tool: t, tip: tip, ok: false })),
          job' = job, pr' = pr, tip' = tip, top' = top, rounds' = rounds, security' = security, routed' = routed,
          keepFeeder, keepLensJobs }
  }
  action followUp = all { job == "done", pr == "open", asked(reviews, tip), rounds < MaxRounds,
                          job' = "running", rounds' = rounds + 1, pr' = pr, tip' = tip, top' = top, reviews' = reviews,
                          security' = security, routed' = routed, keepFeeder, keepLensJobs }
  // a new head, or a force-push back to one the pull request had before
  action push = {
    nondet t = 0.to(MaxTip).oneOf()
    all { pr == "open", t != tip, t <= top + 1, tip' = t, top' = if (t > top) t else top, job' = job, pr' = pr,
          reviews' = reviews, rounds' = rounds, security' = security, routed' = routed, keepFeeder, keepLensJobs }
  }
  action ownerMerges = all { pr == "open", job != "running", pr' = "merged",
                             job' = job, tip' = tip, top' = top, reviews' = reviews, rounds' = rounds,
                             security' = security, routed' = routed, keepFeeder, keepLensJobs }
  // a lens job: obid starts it and it reads the tip as it is then; it ends, or obid loses it
  action lensStarts = {
    nondet l = routed.oneOf()
    all { lensStatus.get(l) == "asked", lensStatus' = lensStatus.set(l, "running"), lensRead' = lensRead.set(l, tip),
          keepWorld, keepFeeder }
  }
  action lensEnds = {
    nondet l = routed.oneOf()
    nondet end = Set("pass", "changes", "malformed", "failed").oneOf()
    all { lensStatus.get(l) == "running", lensStatus' = lensStatus.set(l, end), lensRead' = lensRead,
          keepWorld, keepFeeder }
  }
  action lensGone = {
    nondet l = routed.oneOf()
    all { lensStatus.get(l) != "none", lensStatus.get(l) != "gone", lensStatus' = lensStatus.set(l, "gone"),
          lensRead' = lensRead, keepWorld, keepFeeder }
  }
  action world = any { jobEnds, review, reviewAsksChanges, followUp, push, ownerMerges, lensStarts, lensEnds, lensGone }

  // ---- the feeder's pass ----
  // ask_lenses: a head may_arm() and the gate pass gets one lens job per routed lens, in place of its approver
  action askLenses = all { st' = "lenses", rejectedTip' = rejectedTip, held' = held, lensTip' = tip, seenTip' = -1,
                           askedTip' = askedTip, lensStatus' = Lenses.mapBy(l => if (routed.contains(l)) "asked" else "none"),
                           lensRead' = Lenses.mapBy(l => -1), keepGhosts }
  action settle = {
    if (job == "running") moveTo("running")
    else if (pr == "merged") moveTo("merged")
    else if (pr == "open" and canMergeAt(tip) and tip != rejectedTip and not(held.contains(tip))) askLenses
    else if (pr == "open" and (asked(reviews, tip) or okTools(reviews, tip).size() >= 2)) moveTo("waiting")
    else moveTo(st)
  }
  // `gh pr view`: read_lenses() sees the pull request's head, then reads the lens jobs
  action look = all { st' = st, rejectedTip' = rejectedTip, held' = held, lensTip' = lensTip, seenTip' = tip,
                      askedTip' = askedTip, keepLensJobs, keepGhosts }
  // a lens with no verdict, or asking for changes: lensTip is held for good
  action holdLensTip(changes: bool): bool =
    all { st' = "waiting", rejectedTip' = lensTip, held' = held.union(Set(lensTip)), lensTip' = lensTip, seenTip' = -1,
          askedTip' = askedTip, keepLensJobs, approverTips' = approverTips, passed' = passed, askedWith' = askedWith,
          askedSeen' = askedSeen, changedTips' = if (changes) changedTips.union(Set(lensTip)) else changedTips,
          brokenTips' = if (changes) brokenTips else brokenTips.union(Set(lensTip)) }
  // read_lenses, in its order: still running, a moved head, no verdict, changes, then every lens passing
  action readLenses = {
    if (job == "running") moveTo("running")
    else if (pr == "merged") moveTo("merged")
    else if (seenTip < 0) look
    else if (routed.exists(l => lensStatus.get(l) == "asked" or lensStatus.get(l) == "running")) moveTo(st)
    // as read_another(): a gone job reads no commit (obid's 404), so only another non-null read is a moved head
    else if (seenTip != lensTip or routed.exists(l => lensStatus.get(l) != "gone" and lensRead.get(l) >= 0
                                                    and lensRead.get(l) != lensTip)) moveTo("waiting")
    else if (routed.exists(l => Set("malformed", "failed", "gone").contains(lensStatus.get(l)))) holdLensTip(false)
    else if (routed.exists(l => lensStatus.get(l) == "changes")) holdLensTip(true)
    // check_again(): may_arm() again on the head the feeder saw, before it asks the approver
    else if (canMergeAt(seenTip))
      all { st' = "approving", rejectedTip' = rejectedTip, held' = held, lensTip' = lensTip, seenTip' = -1,
            askedTip' = lensTip, keepLensJobs, approverTips' = approverTips.union(Set(lensTip)),
            passed' = passed.union(routed.filter(l => lensStatus.get(l) == "pass").map(l => (l, lensRead.get(l)))),
            askedWith' = askedWith.union(routed.map(l => (l, lensRead.get(l), lensStatus.get(l)))), askedSeen' = seenTip,
            changedTips' = changedTips, brokenTips' = brokenTips }
    else moveTo("waiting")
  }
  action pass = all {
    keepWorld,
    if (st == "open") moveTo("running")
    else if (st == "running")
      moveTo(if (job == "failed") "failed" else if (job == "done") (if (pr == "open") "reviewing" else "failed") else st)
    else if (st == "reviewing" or st == "waiting") settle
    else if (st == "lenses") readLenses
    else moveTo(st)  // approving: the approver is oby_feed_approver.qnt's
  }
  action start = all { st == "open", job == "none", job' = "running", pr' = pr, tip' = tip, top' = top, reviews' = reviews,
                       rounds' = 1, security' = security, routed' = routed, st' = "running", rejectedTip' = rejectedTip,
                       held' = held, lensTip' = lensTip, seenTip' = seenTip, askedTip' = askedTip, keepLensJobs,
                       keepGhosts }
  action step = any { world, start, all { st != "open", pass } }

  val approverOnlyAfterEveryLensPassed = approverTips.forall(t => routed.forall(l => passed.contains((l, t))))
  val changesNeverReachApprover = approverTips.intersect(changedTips) == Set() and askedWith.forall(a => a._3 != "changes")
  val brokenLensNeverReachesApprover = approverTips.intersect(brokenTips) == Set() and
    askedWith.forall(a => not(Set("malformed", "failed", "gone").contains(a._3)))
  val staleLensVerdictNeverReachesApprover = st == "approving" implies
    (askedTip == lensTip and askedSeen == lensTip and routed.forall(l => lensRead.get(l) == askedTip))
  val approverNeverAsked = approverTips == Set()
  val noLensAskedForChanges = changedTips == Set()
  val noLensBroke = brokenTips == Set()
  val headNeverMovedUnderLenses = not(st == "lenses" and tip != lensTip)
}
