// verify: main=worktree_free steps=12 invariants=neverLosesWork witnesses=neverFrees
// The cleanup pass freeing a job's worktree, racing the job running again (task A00-69; docs/contracts/cleanup.md,
// CLEANUP-3 and CLEANUP-4). It models, by function:
// - Jobs::clean_worktrees in core/crates/obid/src/jobs.rs, whose worktree half is Jobs::free_worktrees in
//   core/crates/obid/src/jobs/free.rs: the pass looks at a settled job, takes the repository's `preparing` lock
//   (Jobs::preparing in jobs.rs), asks the guard (Jobs::worktree_holds_work) and, still under the lock, reads the
//   job again and removes its worktree only when may_free says so (passLooks, then passRemoves);
// - settled_in in jobs/free.rs: done, its pull request not open (free_at_once: merged, closed or none), and the
//   guard found no work (settled, passLooks);
// - may_free in jobs/free.rs, mirrored by mayFree;
// - Command::NextRound in core/crates/obid/src/job/lifecycle.rs, Try again: a done job is queued again (tryAgain);
// - worktree preparation in Jobs::run_owner_job (and Jobs::run_obi_review) in jobs.rs, under the same `preparing`
//   lock: a queued job runs, and a worktree that is none or freed is made again from its branch with no work in it
//   (prepare).
// neverLosesWork is the property: no state has a freed worktree that still held work that exists nowhere else. It
// is expected to hold in every sequence up to 12 steps (quint verify --max-steps=12 --invariant=neverLosesWork).
// neverFrees is its witness: a worktree is freed, so it is expected to be violated, and a pass of neverLosesWork is
// not vacuous.
// The limit: a model checks the design, not that free.rs implements it. Nothing here replays a trace against the
// code (Quint Connect is not used). Like the code, the pass doesn't run the guard again at removal; the owner
// editing the worktree by hand between the guard and the removal isn't modelled.
module worktree_free {
  var status: str
  var worktree: str
  var work: str
  var pr: str
  var passHolds: bool
  def mayFree(settled: bool, running: bool, unpublished: bool, uncommitted: bool): bool =
    settled and not(running) and not(unpublished) and not(uncommitted)
  val settled = status == "done" and pr != "open"
  val running = status.in(Set("queued", "running"))
  action init = all { status' = "queued", worktree' = "none", work' = "none", pr' = "none", passHolds' = false }
  action prepare = all {
    status == "queued",
    not(passHolds),
    status' = "running",
    worktree' = "present",
    work' = if (worktree == "present") work else "none",
    pr' = pr,
    passHolds' = passHolds,
  }
  action agentWorks = {
    nondet left = Set("uncommitted", "unpublished").oneOf()
    all { status == "running", worktree == "present", work' = left, status' = status, worktree' = worktree, pr' = pr, passHolds' = passHolds }
  }
  action jobEnds = all { status == "running", status' = "done", worktree' = worktree, work' = work, pr' = pr, passHolds' = passHolds }
  action publish = all {
    status == "done",
    work == "unpublished",
    pr != "merged",
    work' = "none",
    pr' = "open",
    status' = status,
    worktree' = worktree,
    passHolds' = passHolds,
  }
  action mergeOrClose = all { pr == "open", pr' = "merged", status' = status, worktree' = worktree, work' = work, passHolds' = passHolds }
  action tryAgain = all { status == "done", status' = "queued", worktree' = worktree, work' = work, pr' = pr, passHolds' = passHolds }
  action passLooks = all {
    not(passHolds),
    worktree == "present",
    settled,
    work == "none",
    passHolds' = true,
    status' = status,
    worktree' = worktree,
    work' = work,
    pr' = pr,
  }
  // The guard answered at the look (work was none): the removal passes its answer, as the code does, and reads the
  // job again.
  action passRemoves = all {
    passHolds,
    worktree' = if (mayFree(settled, running, false, false)) "freed" else worktree,
    passHolds' = false,
    status' = status,
    work' = work,
    pr' = pr,
  }
  action stutter = all { status' = status, worktree' = worktree, work' = work, pr' = pr, passHolds' = passHolds }
  action step = any { prepare, agentWorks, jobEnds, publish, mergeOrClose, tryAgain, passLooks, passRemoves, stutter }
  val neverLosesWork = not(worktree == "freed" and work != "none")
  val neverFrees = worktree != "freed"
}
