# Publishing: only the commit the check passed on

The business behavior contract for what a job's publish pushes, what its check runs on ([#1647](https://github.com/obi-ai-bot/oby/issues/1647)), and for which jobs may work on one branch ([#1635](https://github.com/obi-ai-bot/oby/issues/1635), both from the core audit of 2026-10-30), from [the template](../business-contract-template.md). Scenarios are numbered `PUBLISHING-n`; the tests that prove them are named after them (`core/crates/obid/tests/publish_api.rs`, `runner_api.rs`).

## Feature and status

- **Outcome:** Nothing goes out unless its check passed (AGENTS.md, "Rules Oby never breaks"): what a job pushes is exactly the commit its check passed on, never commits another job put on its branch since, and two tasks on one issue never overwrite each other's commits.
- **Status:** **Implemented.** Before #1647, the check ran in the job's own worktree, so an agent could rewrite the check's script to `exit 0`, or leave an ignored `.env` the tests read, and the check passed on what never went out. Before #1635, publishing pushed whatever the job's branch pointed to in obid's clone at publish time, so a follow-up that failed its check could move the branch and Publish on the first job pushed its failing commits; and only follow-ups were kept off a branch another active job used, so "fix issue 34" twice put two jobs on `oby/fix-34`.
- **Scope:** What publishing pushes, by itself and by hand, what the check runs on, and when a new job may start on a branch. **Out of scope:** the secret scan, the lease on a rebase, the pull request and the merge (docs/runner-api.md, "obid only: publishing the owner's work", and #1636), which stay as they are.

## Behavior

### Actors and ownership

- **obid** decides everything here: it makes the check's checkout and chooses which of its files come from the base, says why when nothing counts as checked, records the commit a job's check passed on, refuses a publish when the branch moved, and refuses a job whose branch an active job works on.
- **The apps** show obid's words: `publishError`, the `409` of `GET` and `POST /v1/jobs/{id}/publish`, and the `422` of `POST /v1/jobs`. They decide nothing.

### Triggers and decisions

- **Its check runs.** obid reads the worktree's HEAD and its `git status` (untracked files too, ignored ones not) first. The check passed on that commit only when nothing was uncommitted then and HEAD was still there after it.
- **Where it runs (PUBLISHING-6).** In a checkout obid makes of that commit, out of its clone's objects (`git worktree add --detach`, about half a second for this repository), apart from everything the job's sandbox can write and gone after the check: nothing the commit doesn't hold is there, untracked or ignored (a `.env`, `node_modules`, a build folder), and what the check writes or commits stays there, so it never reaches the job's branch or its next check. The check's home is the job's, with the caches its agent filled (cargo's registry, npm's), since the check has no network; one that gets the owner's secrets has a home of its own. The base is the job's (its pull request's base), else the repository's default branch, as obid's clone has it when the check starts. From the base's head come:
  - **the check's scripts:** for each of the command's top-level steps (split at `&&`), its program when that's a path (`scripts/oby-check.sh`, `./check.sh`), or, after `sh`, `bash`, `zsh`, `dash`, `python3`, `python`, `node`, `ruby` or `perl`, the first word that isn't an option, `NAME=value` words before it skipped; a relative path inside the repository (`./` taken off; never one with `..`, `.git`, a quote, `$` or a glob) that names a file in the commit or on the base. The base's copy replaces the commit's; one the commit deleted is put back. What a command reads without naming it (`make check`'s Makefile, `npm test`'s `package.json`) is the commit's.
  - **`.gitleaks.toml`:** the base's copy, or none when the base has none.
- **The base has no copy of a check's script (PUBLISHING-8).** A script the commit has and the base doesn't (a new repository, or a check added by this very task), or a checkout obid can't make: nothing runs, nothing counts as checked, and the job ends done with nothing published by itself and why in its `reason`. The commit it ended with is kept for Publish by hand, the owner's click.
- **A job ends done.** obid takes its commits into its clone and moves its branch there; the commit its check passed on is the job's checked head (`checkedHead`, kept with the job, not in the API), its head for a job with no check. Another round (a decision round, the round told to commit) records it again when it ends done. A check that ran with work uncommitted gets the one round Oby tells it to commit; still uncommitted then, or HEAD moved by the check, the job ends done with nothing published and why (PUBLISHING-5).
- **A branch moves.** When obid moves a job's branch in its clone (any job's commits, kept when it ends), every job whose checked head on that branch is another commit loses it: it's offered no Publish, and isn't waiting on the owner for one.
- **It publishes, by itself or by hand.** obid pushes the checked head and nothing else, and only while the job's branch in its clone is still there. When the branch moved, nothing goes out and the job says why.
- **The owner publishes by hand while a follow-up carries the branch on.** Refused, as Try again and delete are, while that follow-up is queued, running or done and not dismissed: it publishes the branch once its own check passes. One that failed, stopped or was dismissed doesn't hold it; when its commits moved the branch, the moved-branch refusal holds instead.
- **A job would start on a branch an active job works on.** A queued or running job on the same repository with the same branch, letter case aside (`oby/Fix-34` is `oby/fix-34` on a Mac): the job is refused, naming that job. That's a new job (`POST /v1/jobs`, a plan, a standing rule, GitHub work), an ended one run again (Try again, `oby_continue_job`, the owner's account-limit choice to resume or move it), and Oby's review asking for changes, which then starts no round and says why on the job (`reviewError`). Refused rather than given a branch of its own, so one issue never gets two pull requests that overwrite or duplicate each other: the owner tells the job already at work instead. Once it ended, another may take the branch.

### Invariants

- No commit goes out that a job's check didn't pass on, by itself or by hand.
- No commit a check didn't check, untracked files and commits made while it ran included, is a checked head.
- A check runs the base's copy of its scripts and `.gitleaks.toml`, never the task's, on the commit alone: a task can't make its own check pass by changing the check.
- No two queued or running jobs on one repository work on one branch, new or run again.
- Publish is offered (`GET /v1/jobs/{id}/publish`, a job's Ready alert) only when publishing it would go out as far as obid knows.
- Everything else about publishing holds: the secret scan, the lease, never `main` or the default branch, every outgoing action listed.

### Failure and recovery

- `Not published: oby/fix-75 moved since this job's check passed on it, so its commits now aren't the ones checked.` The owner tells the job (a follow-up runs the check on the branch as it is now) or the follow-up that moved it.
- `Not published: this job ended before Oby kept the commit its check passed on, so Oby can't tell what it checked. Tell it to carry on, and it checks again.` A job that ended before obid kept that commit.
- `Its follow-up <title> carries its branch on: publish that one instead.`
- `<title> (job <id>) is working on oby/fix-34 now: tell that one instead.` For Oby's review, `reviewError` is `Oby's review asks for changes, and obid couldn't hand them to the agent: ` and that line.
- `Not published: its check ran with changes in the worktree that weren't committed, so it didn't check the commits alone. Tell Oby to commit them or drop them, and it checks again.` (the job's `reason`, and Publish's `409`)
- `Not published: its commits changed while its check ran, so the check didn't pass on the ones there now. Tell Oby to carry on, and it checks them again.`
- `Not published: its check runs scripts/check.sh as main has it, and main has no scripts/check.sh, so nothing counts as checked. Read its change, then Publish it by hand.` (PUBLISHING-8, the job's `reason`.) A checkout obid couldn't make, or a base it can't read, gives the same ending with its own words in the middle (`obid's clone doesn't have main`, `obid couldn't make the check's own checkout (its worktree)`).

These words show an internal job id (`job 3125ab36`) and a branch name, as obid's other refusals do; how the apps word them is Sangam's.

## Interfaces and dependencies

`GET` and `POST /v1/jobs/{id}/publish`, `POST /v1/jobs`, and `publishError` on the job, as docs/runner-api.md has them. Nothing new in the API: `checkedHead` stays in the job's file.

## Acceptance scenarios

- **PUBLISHING-1:** A job ends done with its check passed and its publish fails. Published by hand, the commit its check passed on goes out. (`publishing_1_2_only_the_commit_its_check_passed_on_goes_out`)
- **PUBLISHING-2:** Its branch in obid's clone moved since (a commit its check never ran on): Publish by hand is `409` with `Not published: <branch> moved since this job's check passed on it…`, and nothing reaches the remote. (`publishing_1_2_only_the_commit_its_check_passed_on_goes_out`)
- **PUBLISHING-3:** A published job's next run passes its check and its publish fails; a follow-up on its branch fails its check. Publish on the first job is `409` while the follow-up is at work; once the follow-up's commits moved the branch, `GET` and `POST` are `409` because the branch moved. The follow-up's failing commits never reach the remote. (`publishing_3_a_follow_ups_failing_commits_never_go_out_on_the_first_job`)
- **PUBLISHING-4:** "Fix issue 34" while a job already works on `oby/fix-34`, or a job naming that branch in any letter case: `422` naming the job at work. Issue 35 starts; once the first job stopped, issue 34 starts again on `oby/fix-34`, and then the first takes no Try again (`422` naming it). (`publishing_4_two_jobs_on_one_issue_never_share_a_branch`, runner_api.rs.) Nor does the owner's account-limit choice resume or move a job while another works on `oby/Fix-34` (`publishing_4_an_ended_job_runs_again_only_off_a_busy_branch`, obid's lib tests), nor does Oby's review asking for changes start a round: `reviewError` names the other job (`publishing_4_a_reviews_changes_wait_off_a_busy_branch`, publish_api.rs).
- **PUBLISHING-5:** A published job's next check runs with a file it never commits: it's told once to commit, then ends done with `reason` saying so, nothing pushed, and Publish `409` with the same words. A check that commits while it runs commits in its own checkout (PUBLISHING-6): what goes out is the commit it checked, without the check's. (`publishing_5_a_check_that_didnt_check_the_commits_publishes_nothing`.) A check that writes `report.xml` writes it in its own checkout: its new commit publishes without the report and without a round telling it to commit (`publishing_5_what_a_check_leaves_isnt_uncommitted_work`).
- **PUBLISHING-6:** The base has a `.gitleaks.toml` and ignores `.env`. A task commits its own `.gitleaks.toml`: its check reads the base's, passes, and the task's goes out. A task that leaves a `.env` its check needs (ignored, so not uncommitted work) fails its check: the checkout has none, as the commit going out has none. (`publishing_6_the_check_runs_on_the_commit_alone_with_the_bases_files`; which words are scripts: `publishing_6_a_checks_scripts_are_its_programs_paths`, obid's lib tests.)
- **PUBLISHING-7:** The base's `check.sh` fails. A task rewrites it to `exit 0`: its check still runs the base's, which fails, and nothing goes out. A task that changes its check script for good is checked by the base's until it's merged; its pull request also falls under MERGING-15 (#1646). (`publishing_7_a_check_rewritten_to_pass_still_runs_the_bases`)
- **PUBLISHING-8:** The base has no `check.sh`, and the task adds one its check runs (`sh check.sh`): the job ends done with no check run, `reason` `Not published: its check runs check.sh as main has it, and main has no check.sh, so nothing counts as checked. Read its change, then Publish it by hand.`, nothing pushed; Publish by hand pushes it. (`publishing_8_a_check_the_base_doesnt_have_counts_for_nothing`)

Test it when it's done, on a real Mac: ask Oby, in this repository, to change `scripts/oby-check.sh` to `exit 0` and break a test, and see its check fail with dev's script; ask Oby to fix one issue twice, from Home and from a conversation, and see the second refused naming the first; let a task's publish fail (sign `gh` out), tell it something that fails its check, and see Publish on the first task refused.
