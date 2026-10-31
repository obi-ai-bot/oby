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

- **Its check runs.** obid reads the worktree's HEAD and its `git status` (untracked files too, ignored ones not) first. The check passed on that commit only when nothing was uncommitted then and the check left HEAD there: what a check writes after it starts (an artifact, a commit) never counts as checked.
- **A job ends done.** obid takes its commits into its clone and moves its branch there; the commit its check passed on is the job's checked head (`checkedHead`, kept with the job, not in the API), its head for a job with no check. Another round (a decision round, the round told to commit) records it again when it ends done. A check that ran with work uncommitted gets the one round Oby tells it to commit; still uncommitted then, or HEAD moved by the check, the job ends done with nothing published and why (PUBLISHING-5).
- **A branch moves.** When obid moves a job's branch in its clone (any job's commits, kept when it ends), every job whose checked head on that branch is another commit loses it: it's offered no Publish, and isn't waiting on the owner for one.
- **It publishes, by itself or by hand.** obid pushes the checked head and nothing else, and only while the job's branch in its clone is still there. When the branch moved, nothing goes out and the job says why.
- **The owner publishes by hand while a follow-up carries the branch on.** Refused, as Try again and delete are, while that follow-up is queued, running or done and not dismissed: it publishes the branch once its own check passes. One that failed, stopped or was dismissed doesn't hold it; when its commits moved the branch, the moved-branch refusal holds instead.
- **A job would start on a branch an active job works on.** A queued or running job on the same repository with the same branch, letter case aside (`oby/Fix-34` is `oby/fix-34` on a Mac): the job is refused, naming that job. That's a new job (`POST /v1/jobs`, a plan, a standing rule, GitHub work), an ended one run again (Try again, `oby_continue_job`, the owner's account-limit choice to resume or move it), and Oby's review asking for changes, which then starts no round and says why on the job (`reviewError`). Refused rather than given a branch of its own, so one issue never gets two pull requests that overwrite or duplicate each other: the owner tells the job already at work instead. Once it ended, another may take the branch.

### Invariants

- No commit goes out that a job's check didn't pass on, by itself or by hand.
- No commit a check didn't check, untracked files and commits made while it ran included, is a checked head.
- No two queued or running jobs on one repository work on one branch, new or run again.
- Publish is offered (`GET /v1/jobs/{id}/publish`, a job's Ready alert) only when publishing it would go out as far as obid knows.
- Everything else about publishing holds: the secret scan, the lease, never `main` or the default branch, every outgoing action listed.

### Failure and recovery

- `Not published: oby/fix-75 moved since this job's check passed on it, so its commits now aren't the ones checked.` The owner tells the job (a follow-up runs the check on the branch as it is now) or the follow-up that moved it.
- `Not published: obid has no record of the commit oby/fix-75's check passed on.` A job that ended before obid kept that commit: telling it runs the check again.
- `Its follow-up <title> carries its branch on: publish that one instead.`
- `<title> (job <id>) is working on oby/fix-34 now: tell that one instead.` For Oby's review, `reviewError` is `Oby's review asks for changes, and obid couldn't hand them to the agent: ` and that line.
- `Not published: its check ran with changes in the worktree that weren't committed, so it didn't check the commits alone. Tell Oby to commit them or drop them, and it checks again.` (the job's `reason`, and Publish's `409`)
- `Not published: its commits changed while its check ran, so the check didn't pass on the ones there now. Tell Oby to carry on, and it checks them again.`

These words show an internal job id (`job 3125ab36`) and a branch name, as obid's other refusals do; how the apps word them is Sangam's.

## Interfaces and dependencies

`GET` and `POST /v1/jobs/{id}/publish`, `POST /v1/jobs`, and `publishError` on the job, as docs/runner-api.md has them. Nothing new in the API: `checkedHead` stays in the job's file.

## Acceptance scenarios

- **PUBLISHING-1:** A job ends done with its check passed and its publish fails. Published by hand, the commit its check passed on goes out. (`publishing_1_2_only_the_commit_its_check_passed_on_goes_out`)
- **PUBLISHING-2:** Its branch in obid's clone moved since (a commit its check never ran on): Publish by hand is `409` with `Not published: <branch> moved since this job's check passed on it…`, and nothing reaches the remote. (`publishing_1_2_only_the_commit_its_check_passed_on_goes_out`)
- **PUBLISHING-3:** A published job's next run passes its check and its publish fails; a follow-up on its branch fails its check. Publish on the first job is `409` while the follow-up is at work; once the follow-up's commits moved the branch, `GET` and `POST` are `409` because the branch moved. The follow-up's failing commits never reach the remote. (`publishing_3_a_follow_ups_failing_commits_never_go_out_on_the_first_job`)
- **PUBLISHING-4:** "Fix issue 34" while a job already works on `oby/fix-34`, or a job naming that branch in any letter case: `422` naming the job at work. Issue 35 starts; once the first job stopped, issue 34 starts again on `oby/fix-34`, and then the first takes no Try again (`422` naming it). (`publishing_4_two_jobs_on_one_issue_never_share_a_branch`, runner_api.rs.) Nor does the owner's account-limit choice resume or move a job while another works on `oby/Fix-34` (`publishing_4_an_ended_job_runs_again_only_off_a_busy_branch`, obid's lib tests), nor does Oby's review asking for changes start a round: `reviewError` names the other job (`publishing_4_a_reviews_changes_wait_off_a_busy_branch`, publish_api.rs).
- **PUBLISHING-5:** A published job's next check runs with a file it never commits: it's told once to commit, then ends done with `reason` saying so, nothing pushed, and Publish `409` with the same words. A check that commits while it runs: the same, with its own words. (`publishing_5_a_check_that_didnt_check_the_commits_publishes_nothing`)

Test it when it's done, on a real Mac: ask Oby to fix one issue twice, from Home and from a conversation, and see the second refused naming the first; let a task's publish fail (sign `gh` out), tell it something that fails its check, and see Publish on the first task refused.
