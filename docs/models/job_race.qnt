// A job's race between Stop, Try again and a worker's end, as dev's rules decide it (task A00-40; the #1649
// lost-retry race). It models, by function:
// - Jobs::work() in core/crates/obid/src/jobs.rs, its `_ if stopped` arm: a stopped worker writes no status
//   (workerDone and workerFails write nothing for a stopped worker);
// - Command::NextRound in core/crates/obid/src/job/lifecycle.rs, Try again: a stopped or failed job is queued
//   again in its next round (tryAgain);
// - Jobs::end_done's check in jobs.rs that the job is active and still in the worker's round, TASKS-22
//   (docs/contracts/tasks.md) and JOBSTATE-18 (docs/contracts/job-state.md): a worker of an earlier round
//   writes no done (workerDone's writes).
// noLostRetry is the property: a job Try again queued is never ended, done or failed, by an older worker before
// its new round starts. It is expected to hold in every sequence up to 12 steps (quint verify --max-steps=12
// --invariant=noLostRetry).
// noRetriedJobEverFinishes is its witness: a retried job does reach done, so it is expected to be violated, and
// a pass of noLostRetry is not vacuous.
// The limit: a model checks the design, not that jobs.rs implements it. Nothing here replays a trace against the
// code (Quint Connect is not used).
module job_race_dev {
  type Worker = { round: int, stopped: bool }
  var status: str
  var round: int
  var workers: Set[Worker]
  var retryPending: bool
  action init = all { status' = "queued", round' = 1, workers' = Set(), retryPending' = false }
  action start = all { status == "queued", workers == Set(), status' = "running", round' = round, workers' = Set({ round: round, stopped: false }), retryPending' = false }
  action stop = all { status == "running", status' = "stopped", round' = round, workers' = workers.map(w => { ...w, stopped: true }), retryPending' = retryPending }
  action tryAgain = all { status.in(Set("stopped", "failed")), status' = "queued", round' = round + 1, workers' = workers, retryPending' = true }
  action workerDone = {
    nondet w = workers.union(Set({ round: -1, stopped: true })).oneOf()
    val writes = not(w.stopped) and status.in(Set("queued", "running")) and round == w.round
    all { workers.contains(w), workers' = workers.exclude(Set(w)), status' = if (writes) "done" else status, round' = round, retryPending' = retryPending }
  }
  action workerFails = {
    nondet w = workers.union(Set({ round: -1, stopped: true })).oneOf()
    all { workers.contains(w), workers' = workers.exclude(Set(w)), status' = if (not(w.stopped)) "failed" else status, round' = round, retryPending' = retryPending }
  }
  action step = any { start, stop, tryAgain, workerDone, workerFails }
  val noLostRetry = not(status.in(Set("done", "failed")) and retryPending)
  val noRetriedJobEverFinishes = not(status == "done" and round >= 2)
}
