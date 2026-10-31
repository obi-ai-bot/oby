# Publishing: only the commit the check passed on

The business behavior contract for what a job's publish pushes, and for which jobs may work on one branch ([#1635](https://github.com/obi-ai-bot/oby/issues/1635), from the core audit of 2026-10-30), from [the template](../business-contract-template.md). Scenarios are numbered `PUBLISHING-n`; the tests that prove them are named after them (`core/crates/obid/tests/publish_api.rs`, `runner_api.rs`).

## Feature and status

- **Outcome:** Nothing goes out unless its check passed (AGENTS.md, "Rules Oby never breaks"): what a job pushes is exactly the commit its check passed on, never commits another job put on its branch since, and two tasks on one issue never overwrite each other's commits.
- **Status:** **Implemented.** Before #1635, publishing pushed whatever the job's branch pointed to in obid's clone at publish time, so a follow-up that failed its check could move the branch and Publish on the first job pushed its failing commits; and only follow-ups were kept off a branch another active job used, so "fix issue 34" twice put two jobs on `oby/fix-34`.
- **Scope:** What publishing pushes, by itself and by hand, and when a new job may start on a branch. **Out of scope:** the secret scan, the lease on a rebase, the pull request and the merge (docs/runner-api.md, "obid only: publishing the owner's work", and #1636), which stay as they are.

## Behavior

### Actors and ownership

- **obid** decides everything here: it records the commit a job's check passed on, refuses a publish when the branch moved, and refuses a job whose branch an active job works on.
- **The apps** show obid's words: `publishError`, the `409` of `GET` and `POST /v1/jobs/{id}/publish`, and the `422` of `POST /v1/jobs`. They decide nothing.

### Triggers and decisions

- **A job ends done.** Its check passed (or it has none), obid takes its commits into its clone and moves its branch there; that commit is the job's checked head (`checkedHead`, kept with the job, not in the API). Another round (a decision round, the round told to commit) records it again when it ends done.
- **It publishes, by itself or by hand.** obid pushes the checked head and nothing else, and only while the job's branch in its clone is still there. When the branch moved, nothing goes out and the job says why.
- **The owner publishes by hand while a follow-up carries the branch on.** Refused, as Try again and delete are: the follow-up publishes the branch once its own check passes.
- **A new job would take a branch an active job works on.** A queued or running job on the same repository with the same branch: the new one is refused, naming that job. Refused rather than given a branch of its own, so one issue never gets two pull requests that overwrite or duplicate each other: the owner tells the job already at work instead. Once it ended, a new job may take the branch.

### Invariants

- No commit goes out that a job's check didn't pass on, by itself or by hand.
- No two queued or running jobs on one repository work on one branch.
- Everything else about publishing holds: the secret scan, the lease, never `main` or the default branch, every outgoing action listed.

### Failure and recovery

- `Not published: oby/fix-75 moved since this job's check passed on it, so its commits now aren't the ones checked.` The owner tells the job (a follow-up runs the check on the branch as it is now) or the follow-up that moved it.
- `Not published: obid has no record of the commit oby/fix-75's check passed on.` A job that ended before obid kept that commit: telling it runs the check again.
- `Its follow-up <title> carries its branch on: publish that one instead.`
- `<title> (job <id>) is working on oby/fix-34 now: tell that one instead.`

## Interfaces and dependencies

`GET` and `POST /v1/jobs/{id}/publish`, `POST /v1/jobs`, and `publishError` on the job, as docs/runner-api.md has them. Nothing new in the API: `checkedHead` stays in the job's file.

## Acceptance scenarios

- **PUBLISHING-1:** A job ends done with its check passed and its publish fails. Published by hand, the commit its check passed on goes out. (`publishing_1_2_only_the_commit_its_check_passed_on_goes_out`)
- **PUBLISHING-2:** Its branch in obid's clone moved since (a commit its check never ran on): Publish by hand is `409` with `Not published: <branch> moved since this job's check passed on it…`, and nothing reaches the remote. (`publishing_1_2_only_the_commit_its_check_passed_on_goes_out`)
- **PUBLISHING-3:** A published job's next run passes its check and its publish fails; a follow-up on its branch fails its check. Publish on the first job is `409` while the follow-up carries the branch on; once the follow-up is deleted, it's `409` because the branch moved. The follow-up's failing commits never reach the remote. (`publishing_3_a_follow_ups_failing_commits_never_go_out_on_the_first_job`)
- **PUBLISHING-4:** "Fix issue 34" while a job already works on `oby/fix-34`, or a job naming that branch: `422` naming the job at work. Issue 35 starts; once the first job stopped, issue 34 starts again on `oby/fix-34`. (`publishing_4_two_jobs_on_one_issue_never_share_a_branch`, runner_api.rs)

Test it when it's done, on a real Mac: ask Oby to fix one issue twice, from Home and from a conversation, and see the second refused naming the first; let a task's publish fail (sign `gh` out), tell it something that fails its check, and see Publish on the first task refused.
