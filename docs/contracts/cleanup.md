# Cleanup: a finished job's builds don't fill the Mac

The business behavior contract for what obid frees by itself on the Mac's disk, from [the template](../business-contract-template.md). The API is `docs/runner-api.md` (`cleanup`, `buildFreed`).

## Feature and status

- **Outcome:** The owner's report of 2026-11-10: the Mac's disk fell to 350 MB because obid kept every finished job's builds (44 jobs, 75 GB: a worktree's `core/target` about 9.4 GB; in the job's HOME a check's `check-target` 4.9 GB, `oby-check-sim-target` 1.2 GB, `.cargo` 0.4 GB). A job's builds go when it ends, and a disk low on space gets room back by itself.
- **Status:** **Implemented** (CLEANUP-1, CLEANUP-2). The words in `why` are proposed, and Sangam's to change.
- **Scope:** Build outputs and caches: what a build makes again. Outside it: removing a worktree or its HOME, which stays the 24-hour rule of the cleanup pass (`cleanup` in runner-api.md, unchanged), a job's branch and commits, its login copies, and the clones in `home/clones`.

## Behavior

### Actors and ownership

- **obid** decides and frees, in its cleanup pass. No model turn and no click.
- **The apps** may show `status.cleanup.freed` and a job's `buildFreed`; they send nothing.

### Rules

- A job **at rest**: it ended (done, failed, stopped or interrupted) and no worker or publish holds it.
- What a job's **builds** are, in its worktree and in its HOME (`home/tmp/<job id>`): a folder with a `CACHEDIR.TAG` (cargo's target folders have one), Xcode's `DerivedData`, cargo's and npm's caches (`.cargo`, `.npm`) and a `node_modules`'s `.cache`. In the worktree, only folders git ignores: never the job's commits or uncommitted files. In the HOME, only its own folders: its login copies (`.claude`, `.claude.json`, `.codex`) and everything else stay. A Python virtual environment, a `node_modules`'s packages, `.obi/` and anything reached through a link stay.
- The worktree and the HOME stay: a follow-up, a fix round or Try again builds again.
- A check runs without the network in the job's HOME. After its `.cargo` or `.npm` went, a check that runs with no agent run before it (Publish by hand) has no cached packages to build with; a job that runs again fetches them with its agent.

## Acceptance scenarios

- **CLEANUP-1.** A job at rest frees its builds on the next cleanup pass (which its end wakes), its pull request open or not, needing the owner or not; its `buildFreed` says what that freed, and `status.cleanup.freed` says it, with `why` "Jobs ended.". A job still running keeps its builds. A pass looks once per end: a job that runs again and ends frees them again. Test: `cleanup_1_a_job_that_ends_frees_its_builds` (core/crates/obid/src/jobs.rs).
- **CLEANUP-2.** obid looks at its disk's free space every 5 minutes. Under 20 GB free, the cleanup pass runs at once, and frees the builds of every job at rest (a check run since its end included), oldest end first, until it freed what 30 GB free lacks; a queued or running job keeps them. `status.cleanup.freed` says how much it freed and why ("The disk had 12 GB free."). From 20 GB free up, nothing more is freed than CLEANUP-1 frees. Test: `cleanup_2_a_low_disk_frees_every_idle_jobs_builds_oldest_first` (core/crates/obid/src/jobs.rs).
