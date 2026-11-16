// verify: main=oby_feed_approver steps=12 invariants=armedOnlyApproverApproved,escalatedNeverArmed,brokenApproverNeverArms,staleVerdictNeverArms witnesses=neverArmed,neverEscalated,approverNeverBroke,headNeverMovedUnderApprover
//
// factory/bin/oby-feed, one task: follow() with its approver step (ask_approver, read_approver), from
// oby_feed_fixed's world and pass in docs/models/oby_feed.qnt, copied here because check_models() sends one file alone.
// A head approved twice by different tools, off security paths, goes to `approving`: the feeder starts an approver job
// and arms the merge only on its verdict. The approver job starts when obid gets to it and reads the pull request's
// tip as it is then (its startCommit), then ends done with approve, escalate or malformed, or failed, or is gone.
// Anyone may push a new head while the pull request is open.
// Invariants (FEEDAPPROVE-2 to 5):
// - armedOnlyApproverApproved: an armed tip has two approvals by different tools, touches no security path, and the
//   feeder read a well-formed APPROVE for it from a done approver whose startCommit was that tip.
// - escalatedNeverArmed: a tip the approver escalated is never armed.
// - brokenApproverNeverArms: a tip whose approver failed, was gone or answered malformed is never armed.
// - staleVerdictNeverArms: the armed tip is the one the feeder asked the approver about and the one it read.
// Witnesses, each expected VIOLATED so the model shows the case can happen: neverArmed (a tip is armed),
// neverEscalated (an approver escalates), approverNeverBroke (an approver breaks), headNeverMovedUnderApprover (a head
// is pushed while the approver reads the old one).
// The limit: a model checks the design, not that the script implements it; nor what the approver's words say, only
// that the feeder acts on its first line alone.

module oby_feed_approver {
  pure val Tools = Set("claude", "codex")
  pure val MaxRounds = 2
  type Review = { tool: str, tip: int, ok: bool }
  pure def okTools(rs: Set[Review], t: int): Set[str] = rs.filter(r => r.tip == t and r.ok).map(r => r.tool)
  pure def asked(rs: Set[Review], t: int): bool = rs.exists(r => r.tip == t and not(r.ok))

  var st: str            // open, running, reviewing, approving, merging, waiting, failed, merged
  var job: str           // none, running, done, failed: Oby's newest build job of the task
  var pr: str            // none, open, merged
  var tip: int           // the pull request's head, counted
  var reviews: Set[Review]
  var armed: bool        // GitHub's auto-merge, pinned to armedTip
  var armedTip: int
  var rounds: int
  var security: bool     // the change touches a security path
  var rejectedTip: int   // the head the feeder holds (`refused`)
  var askedTip: int      // `approverHead`: the tip the feeder asked its approver about
  var approver: str      // none, asked, running, approve, escalate, malformed, failed, gone
  var readTip: int       // the approver job's startCommit; -1 before it starts
  var approvedTips: Set[int]   // ghost: tips the feeder read a well-formed APPROVE for, from a done approver that read them
  var escalatedTips: Set[int]  // ghost: tips the feeder read ESCALATE for
  var brokenTips: Set[int]     // ghost: tips whose approver failed, was gone or answered malformed

  val canMerge = okTools(reviews, tip).size() >= 2 and not(asked(reviews, tip)) and not(security)

  action init = {
    nondet sec = Set(true, false).oneOf()
    all { st' = "open", job' = "none", pr' = "none", tip' = 0, reviews' = Set(), armed' = false, armedTip' = -1,
          rounds' = 0, security' = sec, rejectedTip' = -1, askedTip' = -1, approver' = "none", readTip' = -1,
          approvedTips' = Set(), escalatedTips' = Set(), brokenTips' = Set() }
  }
  action keepWorld = all { job' = job, pr' = pr, tip' = tip, reviews' = reviews, rounds' = rounds, security' = security }
  action keepApprover = all { approver' = approver, readTip' = readTip }
  action keepGhosts = all { approvedTips' = approvedTips, escalatedTips' = escalatedTips, brokenTips' = brokenTips }
  action keepFeeder = all { st' = st, armed' = armed, armedTip' = armedTip, rejectedTip' = rejectedTip, askedTip' = askedTip,
                            keepGhosts }
  // the feeder moves the task to `s` and changes nothing else
  action moveTo(s: str): bool = all { st' = s, armed' = armed, armedTip' = armedTip, rejectedTip' = rejectedTip,
                                   askedTip' = askedTip, keepApprover, keepGhosts }

  // ---- the world: Oby, its reviewers, the approver job, GitHub, the owner, anyone who pushes ----
  action jobEnds = {
    nondet ok = Set(true, false).oneOf()
    all { job == "running", job' = if (ok) "done" else "failed",
          pr' = if (ok and pr == "none") "open" else pr, tip' = if (ok) tip + 1 else tip,
          reviews' = reviews, rounds' = rounds, security' = security, keepFeeder, keepApprover }
  }
  action review = {
    nondet t = Tools.oneOf()
    nondet ok = Set(true, false).oneOf()
    all { job == "done", pr == "open", not(reviews.exists(r => r.tip == tip and r.tool == t)),
          reviews' = reviews.union(Set({ tool: t, tip: tip, ok: ok })),
          job' = job, pr' = pr, tip' = tip, rounds' = rounds, security' = security, keepFeeder, keepApprover }
  }
  action followUp = all { job == "done", pr == "open", not(armed), asked(reviews, tip), rounds < MaxRounds,
                          job' = "running", rounds' = rounds + 1, pr' = pr, tip' = tip, reviews' = reviews,
                          security' = security, keepFeeder, keepApprover }
  action push = all { pr == "open", tip' = tip + 1, job' = job, pr' = pr, reviews' = reviews, rounds' = rounds,
                      security' = security, keepFeeder, keepApprover }
  action queueMerges = all { armed, pr == "open", tip == armedTip, pr' = "merged", armed' = false,
                             job' = job, tip' = tip, reviews' = reviews, rounds' = rounds, security' = security,
                             st' = st, armedTip' = armedTip, rejectedTip' = rejectedTip, askedTip' = askedTip,
                             keepGhosts, keepApprover }
  action queueRefuses = all { armed, pr == "open", armed' = false, pr' = pr,
                              job' = job, tip' = tip, reviews' = reviews, rounds' = rounds, security' = security,
                              st' = st, armedTip' = armedTip, rejectedTip' = rejectedTip, askedTip' = askedTip,
                              keepGhosts, keepApprover }
  action ownerMerges = all { pr == "open", job != "running", pr' = "merged", armed' = false,
                             job' = job, tip' = tip, reviews' = reviews, rounds' = rounds, security' = security,
                             st' = st, armedTip' = armedTip, rejectedTip' = rejectedTip, askedTip' = askedTip,
                             keepGhosts, keepApprover }
  // the approver job: obid starts it and it reads the tip as it is then; it ends, or obid loses it
  action approverStarts = all { approver == "asked", approver' = "running", readTip' = tip, keepWorld, keepFeeder }
  action approverEnds = {
    nondet end = Set("approve", "escalate", "malformed", "failed").oneOf()
    all { approver == "running", approver' = end, readTip' = readTip, keepWorld, keepFeeder }
  }
  action approverGone = all { approver != "none", approver != "gone", approver' = "gone", readTip' = readTip,
                              keepWorld, keepFeeder }
  action world = any { jobEnds, review, followUp, push, queueMerges, queueRefuses, ownerMerges,
                       approverStarts, approverEnds, approverGone }

  // ---- the feeder's pass ----
  // a head may_arm() passes: the approver is asked about it, in place of arming it
  action ask = all { st' = "approving", armed' = armed, armedTip' = armedTip, rejectedTip' = rejectedTip,
                     askedTip' = tip, approver' = "asked", readTip' = -1, keepGhosts }
  action settle = {
    if (job == "running") moveTo("running")
    else if (pr == "merged") moveTo("merged")
    else if (pr == "open" and canMerge and tip != rejectedTip) ask
    else if (pr == "open" and (asked(reviews, tip) or okTools(reviews, tip).size() >= 2)) moveTo("waiting")
    else moveTo(st)
  }
  // read_approver: a moved head waits unrefused; APPROVE arms the asked tip; ESCALATE and no verdict hold it
  action readVerdict = {
    if (job == "running") moveTo("running")
    else if (pr == "merged") moveTo("merged")
    else if (approver == "asked" or approver == "running") moveTo(st)
    else if (tip != askedTip or (readTip >= 0 and readTip != askedTip)) moveTo("waiting")
    else if (approver == "approve")
      all { st' = "merging", armed' = true, armedTip' = askedTip, rejectedTip' = rejectedTip, askedTip' = askedTip,
            keepApprover, approvedTips' = approvedTips.union(Set(readTip)), escalatedTips' = escalatedTips,
            brokenTips' = brokenTips }
    else if (approver == "escalate")
      all { st' = "waiting", armed' = armed, armedTip' = armedTip, rejectedTip' = askedTip, askedTip' = askedTip,
            keepApprover, approvedTips' = approvedTips, escalatedTips' = escalatedTips.union(Set(askedTip)),
            brokenTips' = brokenTips }
    else  // malformed, failed or gone
      all { st' = "waiting", armed' = armed, armedTip' = armedTip, rejectedTip' = askedTip, askedTip' = askedTip,
            keepApprover, approvedTips' = approvedTips, escalatedTips' = escalatedTips,
            brokenTips' = brokenTips.union(Set(askedTip)) }
  }
  action pass = all {
    keepWorld,
    if (st == "open") moveTo("running")
    else if (st == "running")
      moveTo(if (job == "failed") "failed" else if (job == "done") (if (pr == "open") "reviewing" else "failed") else st)
    else if (st == "reviewing" or st == "waiting") settle
    else if (st == "approving") readVerdict
    else if (st == "merging") {
      if (pr == "merged") moveTo("merged")
      else if (pr == "open" and not(armed))
        all { st' = "waiting", armed' = armed, armedTip' = armedTip, rejectedTip' = armedTip, askedTip' = askedTip,
              keepApprover, keepGhosts }
      else moveTo(st)
    }
    else moveTo(st)
  }
  action start = all { st == "open", job == "none", job' = "running", pr' = pr, tip' = tip, reviews' = reviews,
                       rounds' = 1, security' = security, st' = "running", armed' = armed, armedTip' = armedTip,
                       rejectedTip' = rejectedTip, askedTip' = askedTip, keepApprover, keepGhosts }
  action step = any { world, start, all { st != "open", pass } }

  val armedOnlyApproverApproved = armed implies
    (okTools(reviews, armedTip).size() >= 2 and not(security) and approvedTips.contains(armedTip))
  val escalatedNeverArmed = armed implies not(escalatedTips.contains(armedTip))
  val brokenApproverNeverArms = armed implies not(brokenTips.contains(armedTip))
  val staleVerdictNeverArms = armed implies (armedTip == askedTip and armedTip == readTip)
  val neverArmed = not(armed)
  val neverEscalated = escalatedTips == Set()
  val approverNeverBroke = brokenTips == Set()
  val headNeverMovedUnderApprover = not(st == "approving" and tip != askedTip)
}
