# Account limits: move by itself past a limit, or wait for the reset

The business behavior contract for what happens when an account reaches its usage limit ([#1563](https://github.com/obi-ai-bot/oby/issues/1563) as reversed by ACC-6, the owner's decision of 2026-11-04), from [the template](../business-contract-template.md). The API is `docs/runner-api.md`, "obid: account-limit decisions". Scenarios are numbered `LIMITS-n`; the tests that prove them are named after them (`limits_1_…`, in obid's `account_limit_flow` and `jobs::account_limits::tests`).

## Feature and status

- **Outcome:** A job that reaches its account's limit moves by itself, with no question, to another signed-in account with room (any tool), and continues. With nowhere to go it waits for the reset and resumes by itself, with no question. A five-hour wait still says when (LIMITS-5). Reviews, `claudeOwnSandbox` accounts and API-key accounts stay on today's ask (below).
- **Status:** **Implemented** (#1563's signed card for the left-alone cases; ACC-6 of 2026-11-04 for automatic moves; the owner's decision of 2026-11-02 for LIMITS-5 to 14's five-hour wait). The words on the job's line and the conversation line are proposed, and Sangam's to change.
- **Scope:** Jobs on Claude Code, Codex and Cursor accounts on this Mac's obid. A conversation turn isn't in it: it moves to another account already (`accountFailover`).

## The owner's decision of 2026-11-04 (ACC-6)

Wellington, the owner, decided on 2026-11-04: "Codex is out of tokens, why do we have Oby still working on a task using it? Shouldn't we change to another that still has room?" A job that reaches its account's limit moves by itself, with no question, to another signed-in account with room, on any tool, and continues. That reverses #1563's ask for ordinary jobs.

## The owner's decision of 2026-11-02

Wellington, the owner, decided on 2026-11-02 that jobs at a five-hour limit resume by themselves, the way Claude's desktop app does, replacing #1563's ask for that limit. He was told the catch and accepted it: Claude's app skips unattended sessions and most Oby jobs are unattended, so a fresh window gets spent with nobody watching. ACC-6 keeps that wait when no other account has room.

What's borrowed, from reading Claude.app 4.4212.0: armed by default, 90 seconds after the reset, five-hour limit only, 3 attempts. What differs: Oby doesn't skip unattended jobs (the decision above), and needs no app page open; its stagger is a tick of its clock (15 seconds) where the app's is 12 seconds plus 2 to 8.

## Behavior

### Actors and ownership

- **obid** stops the jobs, picks a destination with room (same tool first, most room, then any other in Auto's order), moves or waits out a reset by itself, and only asks for the left-alone cases (reviews, `claudeOwnSandbox`, API-key accounts).
- **The apps** show a waiting job as `queued` with its `waiting` and `step`, and show a left-alone decision when there is one. They decide nothing about ordinary moves.

### How a destination is picked (ACC-6)

- Only a connected login that can run, with verified room (both windows under 95%, week out of its reserve, a recent reading), or a ChatGPT login that isn't held and has background consent when the job is background (CHATGPT-20).
- Same tool first, the account with the most room (least used of its holding window; a Cursor figure holds nothing back and only orders Cursor's own accounts, USAGE-9), then any other tool in Auto's order (Claude Code, Codex, Cursor last).
- A move keeps the worktree, branch and commits. Another tool starts a new session from the job's handoff note (what's done and left), reusing `carry_over`. Before it starts, the job's model (and effort) become that account's (AGENTS-6).
- One line in the job's conversation: `{Tool} hit its limit, so this moved to {Tool}.` (proposed words). The job's `moves` lists it.

### Left alone (stay as #1563's ask)

- **Reviews** (`read_only`, or another Obi's): they pick their account themselves and can't change accounts; a review in a group still leaves the task its move (LIMITS-3).
- **`claudeOwnSandbox` accounts:** a move would leave the job's login and box unsafe (the account's own folder and Keychain).
- **API-key accounts** (pay per token, `priced` `api`, or a Codex login that's only an `OPENAI_API_KEY`): they report no subscription windows, so "room" can't be verified and a limit there isn't the same kind of hold.

### Scenarios

- **LIMITS-1** An approved Resume on an account that has reported no usage since its reset (Cursor and API-key accounts never do; a read can fail) runs again 30 minutes after the reset, or after the question when no reset is known, instead of waiting for a reading that never comes, unless what obid last read of the account still holds it (its week at the reserve, a window not yet reset); before then it waits and says so. If it hits the limit again, ACC-6 moves or waits again; a left-alone case asks again. `account_limit_flow::limits_1_a_resume_on_an_account_that_reports_no_usage_runs_again_on_time`, `account_limit_flow::limits_1_a_resume_with_no_usage_waits_its_30_minutes_and_a_known_hold`.
- **LIMITS-2** An approved Resume that only the account's weekly reserve holds (past 80% of its week, not lifted) says so in its jobs' reason, and waits until the week resets or the owner lifts the reserve. `account_limit_flow::limits_2_a_resume_held_by_the_weekly_reserve_says_so`.
- **LIMITS-3** A review stopped by the same limit as a task doesn't take Move away from the task: Move is offered while any job in the group can move; chosen, the tasks move and the review alone comes back as a fresh decision with Resume and Stop, on its own account. `account_limit_flow::limits_3_a_review_in_the_group_leaves_the_task_its_move`.
- **LIMITS-4** Options that change after the owner looked (a usage read landing, an account connected or gone) don't refuse their answer: the revision they sign follows the jobs shown, and only a choice no longer offered is refused (`That option is no longer available; refresh and choose again.`). `account_limit_flow::limits_4_an_answer_stands_when_only_other_options_changed`.

- **LIMITS-5** A job a five-hour limit stops (a reset its tool said, no more than five hours off, with its week under 95% and out of its reserve; a job that starts on an account so held too, and Oby's review of published work) first tries ACC-6's move. With nowhere to go it waits, `queued` and with no slot, no card and no alert, until 90 seconds after that reset. Its line says when (`Resuming at 3:40 PM`, proposed words). Then it goes on in its same session, worktree, branch and account, told by a marked note what cut it, with all the cut turn was sent. `account_limit_flow::limits_5_a_five_hour_limit_resumes_by_itself_in_its_session`, `account_limit_flow::limits_5_a_job_that_starts_on_a_full_account_waits_too`, `account_limit_flow::limits_5_a_waiting_job_takes_no_slot`, `publish_api::limits_5_a_review_waits_for_the_reset_with_its_job`, `usage::tests::limits_5_only_a_five_hour_limit_is_waited_out`, `jobs::account_limits::tests::limits_5_the_line_says_the_time_in_the_macs_zone`.
- **LIMITS-6** Stop works during the wait, and is final: nothing resumes it, a Stop that lands just as the run ends included. While it waits its account is busy, as with any queued job: signing in again or removing the account answers `409` until it's stopped, and a job moves only once stopped. `account_limit_flow::limits_6_stop_ends_the_wait`, `account_limit_flow::limits_6_stop_frees_the_waiting_jobs_account`, `jobs::account_limits::tests::limits_6_a_stop_as_the_run_ends_isnt_undone`.
- **LIMITS-7** The wait survives an obid restart: the same time on its line, and no restart counted against the job (RECOVERY-2), since nothing was running. A Mac that slept through the time resumes it on waking. `account_limit_flow::limits_7_the_wait_survives_a_restart`.
- **LIMITS-8** Jobs whose time came resume one at a time, a tick of obid's clock apart (15 seconds, `RESET_TICK`), then wait for a slot like any queued job. `account_limit_flow::limits_8_jobs_resume_one_at_a_time`.
- **LIMITS-9** A job waits out at most 3 five-hour limits on the same account. The next limit tries ACC-6's move; with nowhere to go it waits for the reset again, with no question (ACC-6). The count lives on the job, survives restarts, and starts again only on the owner's own words, as RECOVERY's do. A signed Resume or Move on a left-alone case isn't such words. `account_limit_flow::limits_9_three_tries_then_the_owner_is_asked` (left-alone), `account_limit_flow::limits_9_a_signed_resume_gives_no_new_tries`, `account_limit_flow::limits_9_only_the_owners_own_words_start_the_count_again`, `account_limit_flow::limits_9_past_three_tries_moves_or_waits_with_no_question`.
- **LIMITS-10** A weekly limit, a limit with no known reset (obid's own guess at one isn't known), a reset further off than five hours, or a week in its reserve: ACC-6 moves when another account has room; with none it waits for the reset (or an hour when none is known) and resumes by itself, with no question. A left-alone case still asks. `account_limit_flow::limits_10_any_other_limit_moves_or_waits_with_no_question`, `account_limit_flow::limits_10_a_job_that_starts_on_an_account_a_week_holds_waits_with_no_question`, `limits::tests::limits_10_the_tools_own_words_say_a_limit`, `account_limit_flow::owner_moves_a_limit_stopped_job_to_a_connected_account_of_another_tool` (left-alone / signed path).
- **LIMITS-11** At its time, an account a later five-hour reset holds gets a new wait, with no try counted. A week that filled or reached its reserve meanwhile tries ACC-6; with nowhere to go it waits for that reset. An account that's gone tries ACC-6; with nowhere, a left-alone ask. `account_limit_flow::limits_11_an_account_still_held_at_the_reset`.
- **LIMITS-12** Resuming changes nothing else: it's the same round, counted no further, ending as that round would (`Nothing committed, so nothing was published.` unless the owner asked for the round); the token budget, the 12-hour ceiling (the wait isn't counted toward it), the untrusted mark, the mode and the box hold as for any run, and a limit isn't a passing error (RECOVERY-1's tries aren't touched). The owner's words during the wait don't end it, since the account is still full: they go with the resume. `account_limit_flow::limits_12_resuming_keeps_everything_else`, `account_limit_flow::limits_12_a_resumed_run_ends_as_its_round_would`.
- **LIMITS-13** A wait an older installed release left a pending decision stays one until ACC-6 can move it or the owner answers; one older still (`interrupted` with a `resumesAt`) becomes a wait or a left-alone decision when obid starts. `account_limit_flow::limits_13_an_older_wait_stays_the_owners`, `resets::old_automatic_retries_become_one_pending_decision_after_restart`.
- **LIMITS-14** A wait that no longer leads anywhere spends no fresh window. A job whose pull request obid reads as merged or closed during the wait stops: let go (TASKS-13) or, with commits that never went out, kept in sight with why (`Its pull request was merged or closed while it waited, so Oby didn't run it again.`, proposed words). Words anyone sent it still go, so it runs. A review of a head its job has since pushed past stops at its time, and the newer head's review comes in its place, once an account has room. `account_limit_flow::limits_14_a_wait_on_an_ended_pull_request_doesnt_run`, `publish_api::limits_14_a_waiting_review_of_an_old_head_spends_no_turn`.

- **LIMITS-15 (ACC-6)** A task whose account hits its limit, with another signed-in account of the same tool that has the most room, moves there by itself: same worktree, branch and commits, the new account's model, no card, and its conversation gets one line naming the tools. `account_limit_flow::limits_15_a_limit_moves_by_itself_to_the_same_tools_account_with_most_room`.
- **LIMITS-16 (ACC-6)** With no room on the same tool and room on another, it moves across tools: a new session from the handoff note (`carry_over`), the destination account's model, and the conversation line. `account_limit_flow::limits_16_a_limit_moves_by_itself_to_another_tool`.
- **LIMITS-17 (ACC-6)** With no account with room, it waits for the reset and resumes by itself, with no question and no `accountLimit` card. `account_limit_flow::limits_17_with_no_room_it_waits_for_the_reset_with_no_question`.
- **LIMITS-18 (ACC-6)** A job already waiting on a limit (or left pending from an older ask) moves by itself when an account with room appears, same pick as LIMITS-15/16. `account_limit_flow::limits_18_a_waiting_job_moves_when_room_appears`.
- **LIMITS-19 (ACC-6)** A review, a job on a `claudeOwnSandbox` account, or one on an API-key account still asks as #1563; they are not auto-moved. `account_limit_flow::limits_19_reviews_own_sandbox_and_api_key_accounts_still_ask`.
- **LIMITS-20 (ACC-6)** A ChatGPT login is offered or chosen as a destination for background work only with its background consent (CHATGPT-20); without it the next account is tried, or the job waits. `chatgpt_api::chatgpt_25_a_move_passes_over_it_until_consent`.

### Invariants

- A job resumes from a limit by itself under LIMITS-5 (and LIMITS-17's wait), at most 3 five-hour waits on the same account before the next limit tries a move again (LIMITS-9). A job on a ChatGPT login never waits out a reset by itself: OpenAI's limit names no time (docs/contracts/chatgpt-sign-in.md, `chatgpt_api::chatgpt_13_a_job_never_waits_for_a_reset_by_itself`). Ordinary jobs move by themselves under ACC-6 with no signed answer; left-alone cases still need one. `account_limit_blocked` marks exactly those waiting on the owner.
- The worktree and branch are kept, whatever is chosen.

### Open

- A job the owner just started on a full account they named waits with no Move card, and that uses 1 of its 3 tries before it has run. A week past 80% (its reserve) waits rather than asks when ACC-6 finds no room. Both are as built, for the owner to overrule.
- Run on a real CLI: `claude` 2.1.302 only, with a local API answering 429 with the limit's headers, then the real login (the pull request says what ran). No real account has hit a real limit under this code. Codex's shape is checked against an answer `codex app-server` 0.163 gave; a Codex or a Cursor run hasn't been waited out for real.

**Test it when it's done (real Mac):** a task on Codex at its limit moves by itself to a Claude Code account with room, its conversation saying so; with every account full it says `Resuming at …` (or waits for the week) with no card. A review on a full account still asks. A task on an account at its five-hour limit with nowhere else goes on by itself 90 seconds after the reset; Stop during the wait ends it.
