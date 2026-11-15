// verify: main=plan_today steps=8 invariants=noEditBeforeYes witnesses=planReachesOwner
// verify: main=plan_fixed steps=10 invariants=noEditBeforeYes,planReachesOwner,yesStartsWork witnesses=neverApproved,neverEdited,neverCarded
//
// Plan mode across tools (docs/contracts/permissions.md, Plan: "Plans first, and works once you approve the plan",
// Sangam's words; PERMISSIONS-5, PERMISSIONS-17 to 21; task A00-85). It models, by function:
// - Jobs::rule_on in core/crates/obid/src/jobs.rs: Claude Code's ExitPlanMode and Cursor's create_plan in Plan
//   become the owner's plan card (permissions.rs `Permission::is_plan`) (finishPlan);
// - codex::Run::answer in core/crates/obid/src/codex.rs: in Plan, every file change Codex asks for is declined
//   (tryEdit);
// - waits_on_plan and Jobs::wait_for_answer in core/crates/obid/src/jobs/codex_plan.rs: a Codex turn in Plan that
//   ended with its plan and changed no file waits on the same plan card (plan_fixed's finishPlan);
// - Jobs::answer_parked in core/crates/obid/src/jobs.rs: a yes resumes the same job out of Plan, a no sends it back
//   to planning (answer).
// noEditBeforeYes: no file changes before the owner's yes to the plan.
// planReachesOwner: a Plan job whose agent finished planning is waiting on the owner's plan card, or the owner
//   answered it: it never ends `done` with the plan unanswered.
// yesStartsWork: after the owner's yes, the job goes on to do the work (it is running or did it).
// plan_today: Codex's path before task A00-85, kept as the before-picture. Expected: planReachesOwner is
//   VIOLATED (witness there).
// plan_fixed: every tool's finished plan becomes the same plan card; a yes resumes the same job out of Plan.
//   Its witnesses neverApproved, neverEdited and neverCarded are expected to be VIOLATED, so a pass of each
//   invariant is not vacuous.
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
  var st: str
  var planned: bool
  var answered: str
  var edits: int
  action init = { nondet t = Tools.oneOf() all { tool' = t, st' = "planning", planned' = false, answered' = "none", edits' = 0 } }
  action tryEdit = all { st == "planning", edits' = edits, st' = st, tool' = tool, planned' = planned, answered' = answered }
  // Codex's last message in Plan is its plan: obid holds the job on the same plan card Claude's ExitPlanMode makes
  action finishPlan = all { st == "planning", planned' = true, tool' = tool, answered' = answered, edits' = edits, st' = "card" }
  action answer = {
    nondet a = Set("yes", "no").oneOf()
    all { st == "card", answered' = a, st' = if (a == "yes") "working" else "planning", planned' = a == "yes",
          tool' = tool, edits' = edits }
  }
  action work = all { st == "working", edits' = edits + 1, st' = "done", tool' = tool, planned' = planned, answered' = answered }
  action step = any { tryEdit, finishPlan, answer, work }
  val noEditBeforeYes = edits > 0 implies answered == "yes"
  val planReachesOwner = (planned and answered == "none") implies st == "card"
  val yesStartsWork = answered == "yes" implies st.in(Set("working", "done"))
  val neverApproved = answered != "yes"
  val neverEdited = edits == 0
  val neverCarded = st != "card"
}
