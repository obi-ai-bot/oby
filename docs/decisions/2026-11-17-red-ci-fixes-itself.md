# AUTOFIX: a task's red checks are fixed by that task, with no card

- **Date:** 2026-11-17
- **Status:** In force
- **Replaced:** For the owner's own work only: [AFTERPR-1](2026-11-11-afterpr-1.md)'s "a red GitHub check gets one round per job … A second red head gets the card" (MERGING-39), and MERGING-21's card for red checks.
- **Changed by:** Nothing
- **Contracts and scenarios:** [autofix.md](../contracts/autofix.md) (AUTOFIX-1 to 16); [merging.md](../contracts/merging.md) (MERGING-21, 39, 41); [task-conversations.md](../contracts/task-conversations.md) (TASKCONV-29, 31); [github-work.md](../contracts/github-work.md)

## The owner's words

> for those cards that keep asking stuff if a CI fails Claude does not tell us anything, the conversation that is working on it have a watcher and this watcher checks the failure and fix it and then try again without fucking asking specially on the bypass permission… so there is no more CI failure stupid ass cards.

The owner's words of 2026-11-17, as the session that asked for this design relayed them.

## Where it is stated

- [autofix.md](../contracts/autofix.md)
- [merging.md](../contracts/merging.md), MERGING-39, which holds only for work that isn't the owner's own, and MERGING-21's `One more round`

## Notes

**As built** (the design below is how it was planned on 2026-11-17, before a line of code):
- "The same failure" is the failed checks' names *and* a digest of their log's last 50 lines with every digit taken out. Names alone would have given up after one round in a repository whose one required check is `Check`.
- AUTOFIX-14 (a Fix CI task watching the owner's pull request it pushed onto) is still proposed. Until it's built, that task's next red head gets the feed's `CI failed` card, as before.
- The owner's two open questions weren't answered before "build it", so the design's defaults stand: a rerun first where the job's own check passed, and the `CI failed` card for pull requests Oby didn't push.
- The merge queue is read from its runs alone (`gh run list --event merge_group`): GitHub's `pr view` has no field that says a pull request is in the queue. A pull request taken out by hand, with no failed run, stays as it was.


### What happens today, from a red check to a card

Two different cards come from two different paths. The one the owner meets depends on who opened the pull request.

**A pull request a task of Oby's opened** (its job's `published`):

1. obid follows every published pull request every 3 minutes (`follow_pull_requests`, `core/crates/obid/src/jobs.rs:4514`; `OBID_FOLLOW_SECONDS`, `jobs.rs:5162`). Each read is `gh pr view` with `statusCheckRollup` (`github.rs:469`). Any check whose conclusion is `FAILURE`, `ERROR`, `TIMED_OUT`, `CANCELLED`, `ACTION_REQUIRED`, `STARTUP_FAILURE` or `STALE` counts as red (`github.rs:848`). A runner that died and a broken test look the same.
2. `carry` (`jobs.rs:4710`) asks `merge::owner_must_settle` (`merge.rs:1532`), which gives `RED_CHECKS` (`merge.rs:1543`).
3. `red_round` (`jobs/after_pr.rs:52`) quotes the failed runs' logs as data (`github::failed_logs`, `github.rs:780`: at most 3 runs, their last 300 lines, secrets taken out, fenced into `.obi/inputs/checks.md`). It then queues **one** round of the same job, in its same session (`after_pr`, `after_pr.rs:103`; prompt `CHECKS_PROMPT`, `after_pr.rs:20`). The job's commits go out to the same pull request.
4. **That is the only round.** `may_round` allows it only while `checks_rounds == 0` (`after_pr.rs:199`), one per job, ever. It also allows it only on an `oby/` branch, at the head obid pushed last, with the job `done`, its worktree still there, and no follow-up on its branch (`after_pr.rs:190`).
5. When the checks are still red after that round, or the round can't start, `carry` makes MERGING-21's card (`stop_card`, `merge.rs:2364`, called at `jobs.rs:4871`): "Ready to merge: <task>", with Merge and Leave it open. `offer_merge_card` (`jobs/merge_by_owner.rs:11`) sets `mergeNeedsYou`, so the task shows in Waiting for you and on Home.
6. The round never reruns anything. Every red, flaky or infrastructure, costs the one agent round, and the next red head gets the card.

**A pull request the owner opened elsewhere** (`author:@me` on GitHub, for example from a Claude Code desktop session):

1. The GitHub feed reads every 15 minutes or so. A pull request in the `mine` list whose head is red is offered once per head (`offered_for`, `github_work.rs:702`; `offers`, `github_work.rs:742`). The offer is skipped when the pull request is a draft or a fork's, when obid doesn't work in its repository, or when one of obid's jobs opened it (`opened_by`, `github_work.rs:778`). That last filter already keeps this card off Oby's own pull requests.
2. The card is "CI failed: <repo> #N" with Fix CI and Dismiss (`sphere/inbox.rs:969` and `1004`).
3. Fix CI starts a **new, separate** `fix` job (`begin`, `github_work.rs:1295`; its goal at `github_work.rs:1396`). That job runs in a conversation of its own (TASKCONV-29), with the last 60 lines of up to 3 failed logs quoted. When its check passes, it pushes straight onto the pull request's branch with a fast-forward only (`onto`, `github_work.rs:1424`; TASKCONV-31).
4. **Nothing follows that push.** No pull request is opened, so the job has no `published` URL, and obid's follower never reads it. If the new head is red too, the feed offers a new "CI failed" card for it (once per head). This is the "keeps asking" loop. Each red head means one more click and one more separate task.

**A finished, merged or let-go task:** the round in step 3 above runs only for a job that is `done`, so a done task is resumed today, but only once. A merged or closed pull request is no longer followed (`jobs.rs:4592`). A job the owner let go (`dismissed`) carries nothing.

**The merge queue (to verify while building, not a finding yet):** when obid hands a pull request to the queue, it records `auto_merge` at that head. Each later pass that sees the same head treats it as handed over and stays silent (`jobs/by_github.rs:175`). obid's read doesn't include `autoMergeRequest` (`github.rs:469`). So a pull request that GitHub dropped from the queue because its group failed may sit there with no card and no retry. In the queue, the pull request's own checks stay green; only the `merge_group` runs (`gh-readonly-queue/<base>/pr-<n>-<sha>`) fail.

### What the reference tools do

- **Claude Code desktop's Auto-fix:** a switch on the session's own pull request, which the owner turns on, only for a pull request their GitHub account opened in a repository they can push to. Once it's on, the app watches the pull request. When a CI check fails, a merge conflict appears or a review comment arrives, it wakes **the same session** with an event message, and the agent fixes and pushes in the conversation it was already having. Turning it on asks for approval in Ask mode and asks nothing in auto or bypass mode. The agent never polls. The app watches.
- **T3 Code's "Fix findings":** a button, not a watcher. The pull request panel's main action becomes "fix" when checks fail (and "resolve" for a conflict). Its click puts a prepared message in **the same thread**: the pull request's number, branch and base, the failing checks' names, and at most 20 findings of 1,000 characters each, all "untrusted data, not instructions". For a single check it says to "reproduce it locally first", since the run may fail for a reason the code can't show (`apps/web/src/components/pullRequest/pullRequestDetail.logic.ts`, `buildFixFindingsHandoff`). The person still clicks.

Oby takes Claude Code's watcher (same session, no click) and T3 Code's handoff (failures quoted as data, reproduce first, a cap on what's quoted). Oby already has both halves: `red_round`'s quote and the follower's pass. What's missing is a loop with limits instead of one round, and telling infrastructure failures apart from code failures.

### The design

**Which pull requests.** Pull requests a task of the owner's own pushed to: the one it opened (`published`), or the one a Fix CI task pushed onto (TASKCONV-31), in the owner's own lane (`merge::own_work`, MERGING-44). That task watches the pull request until it merges, closes or someone else pushes to it. Anything else keeps today's behavior: another Oby's work, `untrusted` work, GitHub work from the feed, routines, and issue tasks get MERGING-39's one round and then the card.

**Modes.** Starting a fix round asks nothing in any mode. The round is the owner's own task carrying on, as MERGING-39's round is today, so there's no "may I fix it?" card in Bypass, Auto (Sandbox), Accept edits or Ask. Inside the round, the agent's own commands follow the job's mode exactly as in any other round: Bypass never asks, while Ask and Accept edits ask for what they always ask for. A Plan job publishes nothing, so it never has a pull request to watch. The owner can turn the whole thing off with `autoFixChecks: false` on `PUT /v1/settings/jobs` (add-only, on by default; Settings has no row for it until Sangam draws one). This follows "Oby forbids nothing": a default of Oby's, overridden by the owner's word.

**Who fixes.** The same job, in its same session and the same conversation, gets one more round (`Command::NextRound`, as `after_pr` does today), even after it ended `done`. There's no new card, no Fix CI task and no follow-up job. Its worktree is kept while it watches. If cleanup took the worktree, obid makes it again at the head obid pushed. If the agent's CLI lost the session, a new session hears the task and the earlier rounds' summaries, as a job that moved accounts does (LIMITS-15). The conversation gets one progress line per step, in obid's words until Sangam's screen has them:
`Checks failed on GitHub (test). Fixing them: round 1 of 3.` · `Rerunning test on GitHub: it was cancelled, not failed.` · `Fixed and pushed a1b2c3d. Waiting for GitHub's checks.` · `Checks pass on GitHub.`
The task's card shows it running again with the step `Fixing its checks on GitHub`, as today.

**What it reads.** obid reads `failed_logs` as today, plus each failed check's name and conclusion, typed. It writes them fenced, as data, into `.obi/inputs/checks.md`, with the vault's and obid's own secrets taken out (MERGING-33's redaction), and nothing is written if the vault can't be read. The prompt is fixed words naming the file. No word of a log ever goes in a prompt (AGENTS.md, "From outside"). Following T3 Code, the prompt says to reproduce the failure locally first. The round's commits go out the way every publish does: through the job's check and the secret scan. A finding, or a scan that can't run, sends nothing, and that ends the watch with a line (below).

**Which failures, and what each gets.** obid decides from typed facts only (each check's conclusion, the head, its own counters), never from the log's words:

| What GitHub shows | What obid does | Agent turn? |
|---|---|---|
| A check `CANCELLED` (not by a newer push of the same branch), `TIMED_OUT` or `STARTUP_FAILURE` | `gh run rerun <id> --failed`, once per head | No |
| `FAILURE` on a head the job's own check passed on locally, the first time on that head | The same rerun, once: the code passed here, so a runner or a flaky test is likelier than a bug | No |
| `FAILURE` again after that rerun, or on a head with no local check | A fix round | Yes |
| A conflict with the base, or `BEHIND` where the base requires up-to-date branches | The rebase round (MERGING-38: obid rebases; on a conflict, the agent's round) | Only on a conflict |
| The merge queue dropped it because its group failed | Attribution (below), then a re-queue or a fix round | Only when the failure is its own |

**Limits.**
- At most **3 fix rounds** per pull request, every head counted. That's the same 3 as `review::REVIEWS`, and with the reruns first, 3 rounds is a lot of tries.
- At most 2 reruns per head and 3 re-queues per head. Reruns cost GitHub minutes, not tokens.
- No loop on the same failure: a fix round whose next head fails with the **same failed checks** (same names, same conclusions) as the head before it ends the watch at once, the way RECOVERY-9 stops a job whose check fails the same way twice.
- The job's own 12-hour ceiling and token budget still apply, fix rounds included, and so do the loop detector and RECOVERY-17's 10 continuations.
- The watch ends 24 hours after the first red with nothing green.
- A job at its account's limit waits or moves as any job does (LIMITS-5 to 16), with no card.

**When it gives up.** It writes one line in the conversation, in obid's words: `Its checks on GitHub still fail after 3 fixes (test). Over to you.`, or the matching reason: the same failure twice, the 24 hours, a scan finding, someone else's push. That is the **only** time something reaches Waiting for you: one item on the task offering `One more round` (three more), `Merge` (the owner's word lifts the hold, MERGING-27) and `Leave it open`. Words to the task in its conversation also start a round, as follow-ups do today.

**The merge queue.** GitHub builds one `merge_group` run per entry, each on top of the entries before it. The run's branch names its pull request and the commit it was built on: `gh-readonly-queue/<base>/pr-<n>-<sha>`. When obid's pull request leaves the queue unmerged (`autoMergeRequest` gone while obid holds `auto_merge`), obid lists the group's runs (`gh run list --event merge_group`, since it armed) and walks the stack from the base up.
- **The failure is its own** when its own entry's run failed and the entry it was built on passed, or it was built on the base itself. That gets a fix round, quoting its own `merge_group` run's log, rebased first on the base as it is now.
- **Someone else's** when an entry under it failed first. obid re-arms auto-merge pinned to the same head with no code change (MERGING-41) and writes the line `Back in the merge queue: #2478, ahead of it, failed its checks.` This is at most 3 times per head, then the give-up line.

This ends the silent stop described above. Reading `autoMergeRequest` is a new field of obid's own read, not of its API.

**Cards.**
- No "CI failed" card, and no MERGING-21 card about red checks, for a pull request a task of the owner's own is watching. The feed's offer filter widens from "an obid job opened it" (`github_work.rs:778`) to "an obid job pushed it and follows it".
- A pull request Oby didn't push to keeps the "CI failed" card, in the lean shape the github-card session is drawing. Once the owner taps Fix CI on it, the Fix CI task that pushes onto it watches it from then on, so it's one click per pull request, ever, not one per red head.

**Which lane owns what** ("Dispatcher, not harness"):
- **obid decides:** when a round starts, what kind (rerun, rebase, fix, re-queue), the counters, the limits, the give-up, what the conversation is told, what's quoted and the scan on what leaves. All of these are rules on typed facts, with no model call of obid's.
- **The agent's:** reading the quoted failure, reproducing it, fixing the code, running the check, committing. The same CLI session, under the job's mode.
- **Between owners:** the logs are other people's workflows' output: quoted, fenced and redacted, never a prompt.

### Contract scenarios (proposed; each a test of the same name)

- **AUTOFIX-1:** An own-work task's pull request turns red at the head obid pushed. The same job, in its same session, runs a fix round. The conversation gets `Checks failed on GitHub (<names>). Fixing them: round 1 of 3.`, with no inbox item, card or alert. Test: `merge_api::autofix_1_a_red_head_resumes_its_own_task`.
- **AUTOFIX-2:** The round's commits go to the same pull request through the check and the scan. The next green read writes `Checks pass on GitHub.` and the merge goes on as own work does (MERGING-45 to 47). Test: `merge_api::autofix_2_a_fixed_head_merges_by_itself`.
- **AUTOFIX-3:** A task already `done` is resumed the same way, as one more round of that job: no follow-up job and no second conversation. Test: `merge_api::autofix_3_a_done_task_takes_its_round_in_place`.
- **AUTOFIX-4:** No mode asks to start a round. In Ask, the round's own commands ask as any round's do. With `autoFixChecks: false`, today's MERGING-39 and card hold. Test: `merge_api::autofix_4_no_mode_asks_to_start_a_fix`.
- **AUTOFIX-5:** The failed logs are quoted, fenced and redacted in `.obi/inputs/checks.md`. A secret planted in a log never reaches the agent, the prompt holds no word of the log, and an unreadable vault starts nothing and gives up with its line. Test: `merge_api::autofix_5_logs_are_data_and_secrets_never_reach_the_agent`.
- **AUTOFIX-6:** A check `CANCELLED`, `TIMED_OUT` or `STARTUP_FAILURE` gets `gh run rerun <id> --failed` once per head, with no agent turn and a line saying so. A check cancelled by a newer push is ignored. Test: `merge_api::autofix_6_an_infra_failure_reruns_without_the_agent`.
- **AUTOFIX-7:** The first `FAILURE` on a head the job's check passed on locally gets one rerun first. Red again on that head, it gets a fix round. Test: `merge_api::autofix_7_a_first_failure_reruns_then_fixes`.
- **AUTOFIX-8:** A conflict with the base or a branch behind gets MERGING-38's rebase round, counted in the same limits. Test: `merge_api::autofix_8_a_conflict_rebases_in_place`.
- **AUTOFIX-9:** At most 3 fix rounds per pull request. The same failed checks on two heads in a row stop it at once. So does 24 hours with nothing green. Test: `merge_api::autofix_9_three_rounds_and_no_loop_on_the_same_failure`.
- **AUTOFIX-10:** Giving up writes one line in the conversation and one Waiting for you item (`One more round`, `Merge`, `Leave it open`). `One more round` gives three more. Test: `merge_api::autofix_10_giving_up_asks_once`.
- **AUTOFIX-11:** Merge queue, its own failure: its own `merge_group` entry failed and the entry under it passed. It gets a fix round on its own run's log, rebased first. Test: `merge_api::autofix_11_its_own_queue_failure_is_fixed`.
- **AUTOFIX-12:** Merge queue, another's failure: an entry under it failed first. It is re-queued at the same head with no code change, at most 3 times. A pull request dropped from the queue is never left silent. Test: `merge_api::autofix_12_another_entrys_failure_requeues_it`.
- **AUTOFIX-13:** The feed offers no "CI failed" card for a pull request an own-work task pushed and follows. One Oby didn't push to keeps the card. Test: `github_work::autofix_13_no_ci_card_for_a_watched_pull_request`.
- **AUTOFIX-14:** A Fix CI task that pushed onto the owner's pull request (TASKCONV-31) watches it like its own. Its next red head gets a round, not a new card. Test: `github_work::autofix_14_fix_ci_watches_the_branch_it_pushed_onto`.
- **AUTOFIX-15:** Someone else's push, the owner's Stop or let-go, or the pull request merged or closed ends the watch, with one line for a push and none for the rest. The counters survive a restart. Test: `merge_api::autofix_15_the_watch_ends_on_anothers_push_and_survives_a_restart`.
- **AUTOFIX-16:** Work that isn't the owner's own keeps MERGING-39 (one round, then the card). Test: `merge_api::merging_39_red_checks_get_one_round_with_the_failed_log`, unchanged.

The pure decision (what a red read leads to: rerun, rebase, fix, re-queue or give up) is one function in `jobs/autofix.rs` with its own unit tests and a Kani proof that no sequence of reads passes the round caps.

**Live proof**, on a scratch obid with its own port and home, never the owner's (7777) or his Oby.app, with Codex in Bypass for the tasks. The repository is the private scratch repository the earlier Fix CI proofs used.
1. A task whose change passes the local check but fails a GitHub workflow step that the local check doesn't run. Expect: the line, one Codex round in the same conversation, a push, green, merged. No card at any point, shown by `GET /v1/inbox` before and after.
2. A workflow that fails only on its first attempt (`GITHUB_RUN_ATTEMPT` = 1). Expect: a rerun, green, no agent turn.
3. A failure the agent can't fix (a step that always fails). Expect: 3 rounds, or the same-failure stop, then exactly one line and one Waiting for you item.

The merge queue needs an organization's repository (GitHub offers it there, not on a personal private one), so AUTOFIX-11 and 12 are proven on a stand-in `gh` replaying recorded `merge_group` runs. A live run waits for an organization scratch repository, which the owner names. The pull request will say so, not claim otherwise.

### Open questions for the owner

1. **Pull requests Oby didn't open.** Most of this repository's red pull requests today were opened by Claude Code desktop sessions, not by Oby's tasks. Those keep a "CI failed" card under this design, until one Fix CI click adopts each one (AUTOFIX-14). Should they get no card at all, leaving them to Claude Code desktop's own Auto-fix?
2. **Rerun first.** It costs one GitHub run (about 10 to 20 minutes here) before a real code failure gets its fix. It's cheaper than a model round for flakes, but slower for real bugs. The alternative is to fix at once and rerun only `CANCELLED`, `TIMED_OUT` and `STARTUP_FAILURE`.
