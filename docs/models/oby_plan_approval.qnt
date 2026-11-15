// verify: main=plan_today steps=8 invariants=noEditBeforeYes witnesses=planReachesOwner
// verify: main=plan_fixed steps=10 invariants=noEditBeforeYes,planReachesOwner,yesStartsWork witnesses=neverApproved,neverEdited,neverCarded,neverEndsAsBefore,neverCardedAgain
//
// Plan mode across tools (docs/contracts/permissions.md, Plan: "Plans first, and works once you approve the plan",
// Sangam's words; PERMISSIONS-5, PERMISSIONS-17 to 21; task A00-85). It models, by function:
// - Jobs::rule_on in core/crates/obid/src/jobs.rs: Claude Code's ExitPlanMode and Cursor's create_plan in Plan
//   become the owner's plan card (permissions.rs `Permission::is_plan`) (finishPlan);
// - codex::Run::answer in core/crates/obid/src/codex.rs: in Plan, every file change Codex asks for is declined
//   (tryEdit);
// - waits_on_plan and Jobs::ends_on_plan in core/crates/obid/src/jobs/codex_plan.rs, and Jobs::wait_for_answer
//   there: a Codex turn in Plan waits on the same plan card only when the job is the owner's (neither read-only
//   nor unwatched: `owners`) and the turn changed no file and said something (`clean`); any other Codex turn ends
//   `done` as before (PERMISSIONS-21) (plan_fixed's finishPlan);
// - Jobs::answer_parked in core/crates/obid/src/jobs.rs: a yes resumes the same job out of Plan (approved_mode is
//   never Plan), a no sends it back to planning, and its next plan is a new card (answer).
// `edits` counts the file changes the agent asked for that obid let through. A command that writes without asking
// (fake-codex's SNEAK) isn't one of them: it is what makes a turn not `clean`, so that turn ends as before.
// Left out: the owner changing the job's mode while the card waits (the owner's own later choice), and Claude
// Code's and Cursor's read-only jobs, whose plan rule_on declines.
// noEditBeforeYes: no file change goes through before the owner's yes to the plan.
// planReachesOwner: a finished, unanswered plan that the code cards (any Claude Code or Cursor plan; a Codex plan
//   of the owner's from a clean turn) is waiting on the owner's plan card, the first and every one after a no.
// yesStartsWork: after the owner's yes, the job goes on to do the work (it is running or did it).
// plan_today: Codex's path before task A00-85, kept as the before-picture. Expected: planReachesOwner is
//   VIOLATED (witness there).
// plan_fixed: Codex's path with A00-85. Its witnesses are expected to be VIOLATED, each showing that what an
//   invariant guards is reachable: neverCarded (a plan reaches the card), neverApproved (a yes), neverEdited (work
//   after a yes), neverEndsAsBefore (a Codex plan that isn't carded ends `done`, so planReachesOwner's scope is what
//   keeps it from failing), neverCardedAgain (a plan made after a no reaches a card of its own).
// The limit: a model checks the design, not that obid implements it.

module plan_common {
  pure val Tools = Set("claude", "codex", "cursor")
}

module plan_today {
  import plan_common.*
  var tool: str
  var st: str          // planning, card, working, done
  var planned: bool    // the agent finished its plan
  var answered: str    // none, yes, no
  var edits: int
  action init = { nondet t = Tools.oneOf() all { tool' = t, st' = "planning", planned' = false, answered' = "none", edits' = 0 } }
  // the agent tries an edit while planning: declined (all tools)
  action tryEdit = all { st == "planning", edits' = edits, st' = st, tool' = tool, planned' = planned, answered' = answered }
  action finishPlan = all { st == "planning", planned' = true, tool' = tool, answered' = answered, edits' = edits,
                            st' = if (tool == "codex") "done" else "card" }
  action answer = {
    nondet a = Set("yes", "no").oneOf()
    all { st == "card", answered' = a, st' = if (a == "yes") "working" else "planning", planned' = a == "yes",
          tool' = tool, edits' = edits }
  }
  action work = all { st == "working", edits' = edits + 1, st' = "done", tool' = tool, planned' = planned, answered' = answered }
  action step = any { tryEdit, finishPlan, answer, work }
  val noEditBeforeYes = edits > 0 implies answered == "yes"
  val planReachesOwner = (planned and answered == "none") implies st == "card"
}

module plan_fixed {
  import plan_common.*
  var tool: str
  var owners: bool     // the job is the owner's: neither read-only nor unwatched
  var st: str          // planning, card, working, done
  var planned: bool    // the agent finished its current plan
  var clean: bool      // the turn that finished it changed no file and said something
  var answered: str    // the owner's answer to the current plan: none, yes
  var sentBack: bool   // the owner sent one of the job's plans back
  var edits: int
  action init = {
    nondet t = Tools.oneOf()
    nondet o = Set(true, false).oneOf()
    all { tool' = t, owners' = o, st' = "planning", planned' = false, clean' = false, answered' = "none",
          sentBack' = false, edits' = 0 }
  }
  // the agent asks for a file change while planning: declined (all tools)
  action tryEdit = all { st == "planning", edits' = edits, st' = st, tool' = tool, owners' = owners, planned' = planned,
                         clean' = clean, answered' = answered, sentBack' = sentBack }
  // Claude's ExitPlanMode and Cursor's create_plan are carded; Codex's last message is, as waits_on_plan decides
  action finishPlan = {
    nondet c = Set(true, false).oneOf()
    all { st == "planning", planned' = true, clean' = c, tool' = tool, owners' = owners, answered' = answered,
          sentBack' = sentBack, edits' = edits,
          st' = if (tool != "codex" or (owners and c)) "card" else "done" }
  }
  // a no leaves the plan behind: the next one the agent finishes is a new, unanswered one
  action answer = {
    nondet a = Set("yes", "no").oneOf()
    all { st == "card", answered' = if (a == "yes") "yes" else "none", sentBack' = (sentBack or a == "no"),
          st' = if (a == "yes") "working" else "planning", planned' = a == "yes",
          tool' = tool, owners' = owners, clean' = clean, edits' = edits }
  }
  action work = all { st == "working", edits' = edits + 1, st' = "done", tool' = tool, owners' = owners,
                      planned' = planned, clean' = clean, answered' = answered, sentBack' = sentBack }
  action step = any { tryEdit, finishPlan, answer, work }
  val carded = tool != "codex" or (owners and clean)
  val noEditBeforeYes = edits > 0 implies answered == "yes"
  val planReachesOwner = (planned and answered == "none" and carded) implies st == "card"
  val yesStartsWork = answered == "yes" implies st.in(Set("working", "done"))
  val neverApproved = answered != "yes"
  val neverEdited = edits == 0
  val neverCarded = st != "card"
  val neverEndsAsBefore = not(st == "done" and planned and answered == "none")
  val neverCardedAgain = not(st == "card" and sentBack)
}
