# AUTOFIX: a task's red checks are fixed by that task

The business behavior contract for what happens when the checks on GitHub fail on a pull request the owner's own task pushed, or one a Fix CI task pushed onto (TASKCONV-31), from [the template](../business-contract-template.md). Scenarios are numbered `AUTOFIX-n`; the tests that prove them are named after them (`core/crates/obid/tests/merge_api.rs`, and obid's lib tests and Kani proofs in `core/crates/obid/src/autofix.rs` and `src/github.rs`). The runner API is in [docs/api/merging.md](../api/merging.md), "AUTOFIX", and [docs/api/settings.md](../api/settings.md).

## Feature and status

- **Outcome:** When GitHub's checks fail on a pull request the owner's own task pushed, that same task fixes it, in its own session and conversation, and pushes again, with no card and no question. The owner sees progress lines in the conversation, and hears from Oby only when it gives up ([why](../decisions/2026-11-17-red-ci-fixes-itself.md)).
- **Status:** **Implemented** (AUTOFIX-1 to 20).
- **Scope:** the owner's own work's pull requests (MERGING-44): their red checks, a merge queue that takes them out, what the conversation hears, and the card when it gives up. And the owner's own pull request a Fix CI task pushed onto (TASKCONV-31): its red checks, its conflict with its base, and its green, followed by that task. **Out of scope:** pull requests no task of Oby's pushed (the feed's `CI failed` card, github-work.md), change requests (MERGING-40), conflicts (MERGING-38, unchanged), and work that isn't the owner's own, which keeps MERGING-39.

## Behavior

### Actors and ownership

- **obid** decides when and what (`crate::autofix::decide`, on typed facts only: each check's conclusion, the head, its own counts), reads GitHub, reruns runs, quotes the failure, starts the round, counts, and tells the conversation (`jobs::autofix`, `conversation::tell_autofix`). It never reads a log for instructions.
- **The task's agent** reads the quoted failure, reproduces it, fixes the code, runs the check and commits, in its own session, under the job's mode.
- **The owner** sees the lines, and answers the card only when it gives up: `One more round`, `Merge` or `Leave it open`.
- **The apps** show the conversation's turns and the card as they show any. They decide nothing.

### Rules

- **Which pull requests.** The one the owner's own task opened (`published`), while that task is `done`, its pull request open, on the branch obid made for it, at the head obid pushed last, with its worktree there and nothing else on its branch (MERGING-38's bounds). A head someone else pushed is theirs: no round, the card as before (MERGING-25).
- **No question.** Starting a rerun or a round asks nothing in any mode. Inside a round, the agent's commands follow the job's mode as any round's do. `autoFixChecks: false` turns it off, and MERGING-39 holds for every job.
- **The same task.** A fix round is one more round of the same job (`round` one more, `step` `Fixing its checks on GitHub`), in its own session, even though it ended `done`: never a follow-up, a new task or a second conversation. Its end is told as any end of the job is (TASKCONV-18).
- **What it reads.** What GitHub logged of the failed runs (`gh run view --log-failed`, the last 300 lines of up to 3 runs; the checks' names when none reads), the vault's and obid's own secrets taken out, fenced as data in the job's `.obi/inputs/checks.md`. The round's prompt is fixed words naming the file. A vault that can't be read quotes nothing and starts nothing.
- **What it does, by what GitHub shows** (`crate::autofix::decide`):
  - every failed check stopped for its runner (`CANCELLED`, `TIMED_OUT`, `STARTUP_FAILURE`, `STALE`): `gh run rerun <id> --failed` for each finished run at that head that didn't pass, at most twice per head, with no agent turn;
  - a first failure on a head the job's own check passed on here: the same rerun, once;
  - otherwise a fix round, at most 3 per pull request, every head counted.
- **What stops it.**
  - Its 3 rounds are spent.
  - A round's head fails the same way as the head before it: the same checks' names, and the same last 50 lines of their log with every digit taken out.
  - A round leaves the head as it was.
  - It has been red for 24 hours with nothing green.
  - The failure can't be quoted for its agent, or no round can start.
  - Its job's own ceiling, budget and loop detector stall it as any round.
- **When it gives up.** One line in the conversation, said once, then the card the pull request would have had (MERGING-21), with `One more round` first. That gives 3 more rounds, lifting the stop it gave up at.
- **While it works.** No card: a rerun settling (90 seconds after it went), or a round at work, holds nothing on the owner.
- **The merge queue.** GitHub tests each queued entry on top of the one under it, and its run's branch names its pull request and the commit it was built on (`gh-readonly-queue/<base>/pr-<n>-<sha>`). When the owner's own pull request, handed to the queue, has a run there since obid armed it that ended without passing (`crate::github::dropped`):
  - the run under it failed: it goes back into the queue at the same head, with no change;
  - its own run stopped for its runner: the same;
  - its own run failed with nothing under it failing: a fix round, the queue's run's log quoted, its agent told to bring the branch up to date first.
  - Going back in the queue counts 3 times per head; past that it gives up.
- **The lines** are obid's own words, after the task's title, and carry no one else's, so they mark nothing:
  - `Its checks failed on GitHub. Fixing them: round 1 of 3.`
  - `It failed its checks in GitHub's merge queue. Fixing them: round 1 of 3.`
  - `Its checks stopped on GitHub's side. Running them again there.`
  - `Its checks failed on GitHub, but passed here. Running them again there once before fixing anything.`
  - `Back in GitHub's merge queue: #12, queued ahead of it, failed its checks.`
  - `Its checks pass on GitHub now.`
  - `Its pull request conflicts with main, so GitHub runs none of its checks. Merging main into it.`
  - `Its pull request still conflicts with main, so GitHub runs none of its checks. Over to you.`
  - `One more round, as you asked.`
  - the give-up lines (`… Over to you.`)
  
  They're obid's words until Sangam's screen has them.

- **A Fix CI task's push onto the owner's pull request** (TASKCONV-31; `jobs::onto_pr`). It opened no pull request, so nothing else reads it: the task, done and not another Oby's, follows the owner's pull request it pushed onto as its own, every follow interval, while it is open at the head the task pushed there last.
  - Red there: AUTOFIX's reruns and fix rounds, as above, in the same task (AUTOFIX-14). With `autoFixChecks` off, its card at once.
  - It conflicts with its base (GitHub's `mergeable` `CONFLICTING`, or `mergeStateStatus` `DIRTY`), so GitHub runs none of its checks: obid fetches the base into the task's clone and the same task's next round merges it in (`git merge origin/<base>`), never a rebase, a reset or a force-push: the branch is the owner's, and obid only adds commits on top. One such round per head, 2 in all (AUTOFIX-18). Past them, or when none can start, one line, said once, and a card saying what to do (AUTOFIX-19).
  - Green there: said once per head (AUTOFIX-17).
  - A head someone else pushed is theirs: nothing, and the feed's `CI failed` card is back for it. Closed or merged: nothing more. Nothing merges it: the owner's pull request is theirs to merge.
  - The feed offers no `CI failed` card for the head the task follows (AUTOFIX-14).
  - Its publish stage names the pull request it pushed onto (AUTOFIX-20).

### Failures and recovery

- A rerun GitHub refuses counts as done: the next read fixes instead.
- What it did lives in the job's file (`autofix`, never in the API) and survives a restart.
- A line the conversation couldn't take is told at the job's next event, and a deleted conversation never is.

### Invariants and boundaries

- No word of a log is ever in a prompt.
- Nothing goes out but through the round's publish, after its check and the secret scan.
- Reruns are in the job's `outgoing` (`checksRerun`).
- obid's API only grows: `autoFixChecks` on the jobs settings, `checksRerun` in `outgoing`, and `One more round` among a merge card's options. `autofix` stays out of the job's JSON.

## Interfaces and dependencies

- `GET` and `PUT /v1/settings/jobs`: `autoFixChecks`.
- `POST /v1/inbox/{id}/answer` with `One more round` on a merge card.
- `gh run list`, `gh run rerun --failed`, `gh run view --log-failed`, and `gh run list --event merge_group`.

## Acceptance scenarios

- **AUTOFIX-1:** The owner's own task's pull request reads red at the head it pushed. The same job runs one more round in its own session, its conversation hears `Fixing them: round 1 of 3.`, and no card about red checks is made. (`merge_api::autofix_1_a_red_head_resumes_its_own_task_with_no_card`; `autofix::tests::autofix_1_a_first_code_failure_gets_a_fix_round`)
- **AUTOFIX-2:** Green after it, the conversation hears `Its checks pass on GitHub now.` (`merge_api::autofix_1_a_red_head_resumes_its_own_task_with_no_card`)
- **AUTOFIX-3:** The job was `done`: it is the same job's round 2, with no follow-up and no second conversation. (`merge_api::autofix_1_a_red_head_resumes_its_own_task_with_no_card`)
- **AUTOFIX-4:** With `autoFixChecks: false`, the owner's own work keeps MERGING-39's one round and card, with no `One more round`; the setting reads back and turns on again. (`merge_api::autofix_4_off_keeps_merging_39`)
- **AUTOFIX-5:** The log is quoted fenced as data, an instruction in it stays out of every prompt, and the quote is redacted as MERGING-39's is. (`merge_api::autofix_1_a_red_head_resumes_its_own_task_with_no_card`; the redaction: `merge_api::merging_39_a_secret_in_the_log_never_reaches_the_agent`, the same quoting)
- **AUTOFIX-6:** Checks cancelled on GitHub are rerun there (`gh run rerun 52 --failed`), with no agent turn, `checksRerun` in `outgoing` and its line; green after it, its line. (`merge_api::autofix_6_a_runners_failure_reruns_without_the_agent`; `autofix::tests::autofix_6_a_runners_failure_reruns_twice_then_fixes`; `github::tests::autofix_6_a_runners_failure_is_every_failed_check_stopped`; Kani: `autofix::proofs::autofix_6_reruns_stay_within_their_count`)
- **AUTOFIX-7:** A first failure on a head the job's own check passed on is rerun once, then gets its round. (`merge_api::autofix_7_a_first_failure_where_its_check_passed_reruns_then_fixes`; `autofix::tests::autofix_7_a_head_its_check_passed_on_reruns_once_first`)
- **AUTOFIX-8:** A conflict with the base or a branch behind keeps MERGING-38's rebase round. (`merge_api::merging_3_a_conflict_gets_one_rebase_round_then_asks`, `merge_api::merging_4_a_branch_behind_rebases_once_then_merges`)
- **AUTOFIX-9:** At most 3 rounds. The same failure after a round gives up at once, as does a round that left the head as it was, or a day of red. (`autofix::tests::autofix_9_three_rounds_and_no_loop_on_the_same_failure`, `autofix::tests::autofix_9_the_same_failure_is_its_names_and_its_log_but_digits`; Kani: `autofix::proofs::autofix_9_a_fix_round_only_within_its_bounds`)
- **AUTOFIX-10:** Giving up says one line, once, and the card offers `One more round` first. Its answer runs the same job again. (`merge_api::autofix_9_the_same_failure_after_a_fix_gives_up_once_with_one_more_round`; `autofix::tests::autofix_10_one_more_round_gives_three_more`)
- **AUTOFIX-11:** Its own entry failed in the merge queue (nothing under it failed): its own to fix. Where no round can start, it says so and the card asks, never back into the queue. (`merge_api::autofix_11_its_own_queue_failure_is_its_own`; `github::tests::autofix_11_whose_failure_took_it_out_of_the_queue`)
- **AUTOFIX-12:** The entry under it failed: back into the queue at the same head, 3 times, each said; then it gives up with its card. (`merge_api::autofix_12_another_entrys_failure_puts_it_back_in_the_queue`; `autofix::tests::autofix_12_a_head_goes_back_into_the_queue_three_times`)
- **AUTOFIX-13** (holds already): the feed offers no `CI failed` card for a pull request an obid job opened (`github_work::offers`).
- **AUTOFIX-14:** A Fix CI task that pushed onto the owner's own pull request (TASKCONV-31) follows it like its own: red again at the head it pushed, the same task's next round fixes it, its conversation hears `Fixing them: round 1 of 3.`, and the feed offers no new `CI failed` card for that head. (`github_work::autofix_14_a_fix_ci_task_takes_the_next_red_head_on_the_pull_request_it_pushed_onto`; `onto_pr::tests::autofix_14_a_fix_ci_push_onto_the_owners_pull_request_is_followed`)
- **AUTOFIX-15:** A head someone else pushed gets no round. (`autofix` returns before deciding; MERGING-25's card)
- **AUTOFIX-16:** Work that isn't the owner's own keeps MERGING-39 unchanged. (`merge_api::merging_39_red_checks_get_one_round_with_the_failed_log`, `merge_api::merging_39_a_second_red_head_asks_the_owner`)
- **AUTOFIX-17:** Green at a head the Fix CI task pushed onto the owner's pull request, its conversation hears `Its checks pass on GitHub now.`, once for that head. (`github_work::autofix_14_a_fix_ci_task_takes_the_next_red_head_on_the_pull_request_it_pushed_onto`; `autofix::tests::autofix_17_green_is_said_once_per_head`)
- **AUTOFIX-18:** That pull request conflicts with its base, so GitHub runs none of its checks: its conversation hears `Its pull request conflicts with main, so GitHub runs none of its checks. Merging main into it.`, and the same task's next round merges main in (fetched first), told never to rebase or rewrite the owner's commits. One round per head, 2 in all; never "resolved" while it conflicts (#2486). (`github_work::autofix_18_a_conflicting_pull_request_it_pushed_onto_gets_main_merged_in`; `autofix::tests::autofix_18_a_conflict_gets_one_merge_round_per_head_two_in_all`; `onto_pr::tests::autofix_18_conflicting_is_githubs_conflict_or_dirty`)
- **AUTOFIX-19:** Still conflicting past its merge rounds: one line, said once (`… still conflicts with main, so GitHub runs none of its checks. Over to you.`), and a card saying to merge main into its branch and resolve the conflicts, or tell Oby to. (`github_work::autofix_18_a_conflicting_pull_request_it_pushed_onto_gets_main_merged_in`; `autofix::tests::autofix_19_still_conflicting_is_said_once_per_head`)
- **AUTOFIX-20:** The Fix CI task's publish stage names the pull request it pushed onto: `Secret scan clean · pushed onto pull request #5`, never `no pull request for this remote` (#2486). (`github_work::autofix_18_a_conflicting_pull_request_it_pushed_onto_gets_main_merged_in`; `onto_pr::tests::autofix_20_the_publish_stage_names_the_pull_request_it_pushed_onto`)

Test it when it's done, on a real Mac: a task of yours whose pull request fails a GitHub check its local check doesn't run. Watch its conversation say it's fixing, see the same task push again and the checks go green, with no card on Home at any point.
