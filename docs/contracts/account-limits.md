# Account limits: one owner decision for the jobs an account's limit stopped

The business behavior contract for what happens when an account reaches its usage limit ([#1563](https://github.com/obi-ai-bot/oby/issues/1563), the owner's decision of 2026-10-28), from [the template](../business-contract-template.md). The API is `docs/runner-api.md`, "obid: account-limit decisions". Scenarios are numbered `LIMITS-n`; the tests that prove them are named after them (`limits_1_…`, in obid's `account_limit_flow`).

## Feature and status

- **Outcome:** When an account hits its limit, the owner decides once for the jobs it stopped: move them to another connected account with room, resume them when room returns, or keep them stopped. Whatever they choose ends: a Resume runs again, a Move moves what can move.
- **Status:** **Implemented** (#1563; #1645 for LIMITS-1 to 4).
- **Scope:** Claude Code, Codex and Cursor accounts on this Mac's obid. The owner's signed choice covers the exact jobs shown and the destination they picked.

## Behavior

### Actors and ownership

- **obid** stops the jobs, groups them per account and limit episode, offers the options, checks the owner's signed answer and carries it out.
- **The apps** show the decision and sign the owner's click; they decide nothing.

### Scenarios

- **LIMITS-1** An approved Resume on an account that has reported no usage since its reset (Cursor and API-key accounts never do; a read can fail) runs again 30 minutes after the reset, or after the question when no reset is known, instead of waiting for a reading that never comes. If it hits the limit again, the owner is asked again. `account_limit_flow::limits_1_a_resume_on_an_account_that_reports_no_usage_runs_again_on_time`.
- **LIMITS-2** An approved Resume that only the account's weekly reserve holds (past 80% of its week, not lifted) says so in its jobs' reason, and waits until the week resets or the owner lifts the reserve. `account_limit_flow::limits_2_a_resume_held_by_the_weekly_reserve_says_so`.
- **LIMITS-3** A review stopped by the same limit as a task doesn't take Move away from the task: Move is offered while any job in the group can move; chosen, the tasks move and the review alone comes back as a fresh decision with Resume and Stop, on its own account. `account_limit_flow::limits_3_a_review_in_the_group_leaves_the_task_its_move`.
- **LIMITS-4** Options that change after the owner looked (a usage read landing, an account connected or gone) don't refuse their answer: the revision they sign follows the jobs shown, and only a choice no longer offered is refused (`That option is no longer available; refresh and choose again.`). `account_limit_flow::limits_4_an_answer_stands_when_only_other_options_changed`.

### Invariants

- No job moves or resumes from a limit without the owner's signed answer; the answer covers exactly the jobs shown.
- The worktree and branch are kept, whatever is chosen.

**Test it when it's done (real Mac):** a task and Oby's review on one Claude account at its limit offer Move to a Codex account with room; Move moves the task, and the review asks again on its own.
