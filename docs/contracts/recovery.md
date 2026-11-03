# Recovery: a job carries itself on before anyone is told

The business behavior contract for what obid does by itself when a job's run is cut, from [the template](../business-contract-template.md). The API is `docs/runner-api.md` (`limit`, `stalled`, "After any restart", "obid's own bugs").

## Feature and status

- **Outcome:** The owner's direction: "Follow the exact way Claude Code follows." A job whose run was cut carries itself on and tells a person last; every automatic loop has a limit, and past it the job ends plainly, with the real reason.
- **Status:** **Implemented.** The words on the step lines are proposed, and Sangam's to change.
- **Scope:** The owner's own jobs on Claude Code, Codex and Cursor. Outside it: usage limits ([account-limits.md](account-limits.md), #1563), the Confirms, follow-ups on a new session or branch, a nudge for a looping agent, publish retries, `CLAUDE_CODE_RETRY_WATCHDOG`, and a review another Oby asked for (its answer goes back to that Oby).

## Behavior

### Actors and ownership

- **obid** decides, counts and continues. It spends no model turn deciding: no supervisor or orchestrator turn runs while a job is being tried again. The job's own agent hears a marked message in its own session.
- **The apps** only show `status` and `step`: no new API key, status or enum value. The counts stay in the job's file, never in the API.

### Triggers and tries

| Kind | Trigger | Tries | Waits | Past them |
|---|---|---|---|---|
| A passing error | A turn ends on the transient allowlist (`API Error: 5xx` or `429`, overloaded, a dropped connection or stream), or the CLI ends with no result while there's a session | 3 | 1, 5, 15 minutes (never longer than `stallAfter`) | The job's end today: `interrupted`, `Try again`, the real reason |
| obid or the Mac went down, or obid's own bug | obid starts with a job at work; or a panic in the job's task before publishing began | 3 | None | `interrupted` with `obid or the Mac went down 4 times…` and `Try again`; after a panic, today's `failed` and its item |
| Gone quiet | No progress for `stallAfter` | 5, shared with the turn limit | None | Today's stalled path, where the parent decides |
| The 200-turn limit (Claude Code only) | `error_max_turns` | 5, shared with quiet | None | Today's stalled path |

### What the agent hears

`[From Oby, by itself: not the owner's words] Your run was cut: <what>.` Then, for an error, that error quoted as data on one line (masked, at most 200 characters). Then: check what really ran before repeating anything, and carry on where it left off. `End of Oby's note.` closes it, so what follows isn't read as Oby's.

What follows is what the cut may have lost. After a passing error or a quiet job: `What you were last sent, which may not have reached you (check before acting on it again):` and all of it, with any words that came meanwhile, so the owner's words, a failed check's output, a rebase order or the very first prompt are never replaced by "carry on". At the turn limit nothing follows: 200 turns in, it was heard. After a restart with a session: the same, kept in the job's file until its turn ends well, since the owner's words it holds are nowhere else; with nothing kept, the prompt, or the owner's words, as it always did.

The error is what the CLI's `result` says or, with none, the run's exit and what that run printed on stderr. It's never the agent's own last words, which can quote anyone, nor an earlier run's stderr (`agent.log` keeps them all).

### What the person sees

- The job stays `running` (after a restart or a bug: `queued`, then `running`). No Failed or Needs you alert, no card and no inbox item goes out until the tries are spent.
- Its `step`: `Trying again in 5 minutes (2 of 3)`, counting down by the minute. The real error is added only from the third try: `Trying again in 15 minutes (3 of 3): API Error: 529`. Gone quiet or at the turn limit: `Trying again now (2 of 5)`. After a restart: `Oby restarted while this ran: continuing it (1 of 3)`.
- Claude Code's own retrying shows the same way with its own numbers (`Trying again in 8 seconds (1 of 10)`), then `Working` once it's heard from again.
- Stop works during every wait, and Stop is final: a job the owner stopped never continues by itself. Words for the job end a wait: they go with its next try.

### Failures and recovery

- A login error, or a session that's gone, isn't tried again, nor is an error that isn't on the allowlist. With no result from the CLI, that run's stderr decides, in whole words: a login (`not logged in`, `/login`, `authentication`, `401`, …) or `No conversation found` ends the job as today.
- A usage limit goes to #1563 as today, whatever cut the turn: a passing error on an account its usage or the tool says is full is that limit's.
- A try goes to its own session or doesn't run: a tool that can't load sessions (a Cursor without `loadSession`) leaves the job `interrupted` with why, and Try again starts a new session. A Codex thread that refuses to resume (cut before its first prompt) ends with Codex's own words, not proven on a real Codex.
- With no session to continue: a passing error runs the same prompt again, as it did. A CLI that ended with no result, a restart, a panic or a stall ends as today.
- A panic once publishing began (its claim, its phase, its step, or a branch already pushed) ends as today: nothing can run twice mid-publish.
- A restart while checking or publishing picks up at that phase with no agent turn ("After any restart"): it isn't counted, since no agent is told anything.
- #915's guard stands first: a job picked up twice in 30 minutes after obid went down stays `interrupted`, its item in the inbox. Past RECOVERY-2's tries it's the same: one `failed` item and a `crash` record, and the orchestrator leaves it to the owner.
- A job `queued` for a slot when obid went down wasn't cut: it isn't counted, or told it was. 3 per job covers clean stops too (a quit, an update), which had no limit before: a job restarted 4 times by obid's own updates stops with a card.

### Invariants

- Retried tries count together. They live on the job, survive restarts, and start again only on the owner's own words (signed, their Confirm, or `oby_tell_job` in a turn of a conversation they're watching, which needs their message): never on a good turn, an unwatched supervisor's or orchestrator's words, or the legacy API's. So no retry multiplies another. A job's tree may still reach 11 tries per job, plus the supervisor's 3 steps.
- A retry never resets the 12-hour ceiling or the token budget, and never touches the loop detector: those are the guards against burning accounts, and they stall at once. A waiting job's time counts toward its ceiling, saved as it waits.
- Claude Code reports tokens only on its `result` line, so a run cut before that counts nothing until a resumed run reports: up to 11 cut runs a job can pass the budget unseen. The 12-hour ceiling is the guard there.
- A retry never answers a permission, merges or changes a setting, and keeps the job's untrusted mark, mode and box.
- A job waiting holds its slot.

## Claude Code, beside Oby

Claude Code's side is what the 2.1.302 binary, the app-managed 2.1.309 binary and Claude.app 4.4212.0 showed when read in 2026-11; only `api_retry` was also run (below).

| Situation | Claude Code | Oby | Same? |
|---|---|---|---|
| API error, overload, dropped connection | 10 retries with backoff inside the CLI | The same: the CLI retries first, and obid sees only what ends the turn | Same |
| Retries spent, on a lost connection | Desktop app: Try again at 1, 5, 15 minutes, 3 tries, reset only by the person, network errors only, probing the API first | 1, 5, 15 minutes, 3 tries, reset only by the owner | Differs: Oby also takes 5xx, overload and 429 (its allowlist), and doesn't probe |
| The process dies mid-turn | Desktop app: respawns with backoff, and doesn't continue the turn by itself (inferred from its filter) | Continued, as a passing error | **Differs on purpose:** the owner asked for less waiting on a person |
| What the person sees | `Retrying in … attempt n/m`; the real error held back until attempt 3, unless the network is down, SSL fails or a rate limit applies | The same line; held back until the third try, always | Differs in the exceptions |
| Sleep, app quit, host restart | A continue message in the same session, marked with its origin; 3 continues, 6 starts, 24 hours on a remote host | A marked message in the same session, 3 per job, no age limit | Differs in the age limit and start count |
| The Mac sleeps | Interrupts the hung turn on waking and tells it | obid isn't restarted by a sleep: what the sleep broke arrives as a passing error or a quiet job | **Differs:** no sleep detection |
| A crash loop | Backs off and parks at 5 in a row or 15 in all (inferred) | Picked up twice in 30 minutes, then left (#915) | Differs in the numbers |
| Workflow agent stalls | 90 seconds without progress: aborted and retried up to 5 times, then `stalled on all N attempts` | **15 minutes** without progress: stopped and continued up to 5 times, then stalled | **Differs on purpose:** 90 seconds fits an agent with no long commands, and a job runs them (a build, a test suite). The desktop app's own session timeout is about 16 minutes |
| Subagent fails | Aborted, marked failed; the parent model decides, with no automatic restart | Past the tries, the job's conversation supervises it; a job with none goes to the orchestrator | Same shape |
| Turn limit | `max_turns` ends the turn; no continuation | Continued, counted with quiet | **Differs on purpose**, by the owner's direction; Claude Code only, since Codex and Cursor have no such switch |
| Its own retrying seen as progress | `system/api_retry` in stream-json | Any line is progress; `api_retry` also shows on the step | Same |
| Usage limit | Desktop app resumes after the reset, five-hour only | Untouched: the owner's choice (#1563) | **Differs**, the owner's decision of 2026-10-28 |
| Runaway cost | No job ceiling or budget found | 12-hour ceiling, token budget, loop detector, all as they were | Oby only. The loop detector doesn't see Cursor, whose adapter reports no calls |
| Past every limit | A plain failed state (`api_error`, a failed agent, "keeps crashing") | A plain failed state (`interrupted` or `failed`, with the real reason) | Same |

**Proven against the installed CLI:** `claude` 2.1.302 against a local stand-in answering 529, with a made-up key and no real account, printed `{"type":"system","subtype":"api_retry","attempt":1,"max_retries":4,"retry_delay_ms":552,"error_status":529,"error":"overloaded",…}`. Everything else in Claude Code's column comes from reading it, not running it.

## Interfaces and dependencies

- **API:** add-only, and nothing was added: `step` is the existing string, and no status or enum changes. `docs/runner-api.md` says what changed.
- **Persistence:** `recovery` in the job's file (`errors`, `stalls`, `restarts`, what cut it, what its turn under way was sent), never in the API.
- **Clients:** `apps/electron` needs no change: `askOf` shows a card only for `failed` and `interrupted`, `taskOf` shows `queued` and `running` as running with their step, and `notify.ts` alerts only on an end.
- **Open:** the tries are borrowed from Claude Code and not measured. Nothing here has run on a real CLI but `api_retry`, above.

## Acceptance scenarios

- **RECOVERY-1** A turn cut by a passing error, or a CLI that ends with no result, stays `running` in its same session and worker, and continues with the marked message quoting the error, after 1, 5, then 15 minutes, 3 tries, then ends as today with the real reason. A login or an unknown error isn't retried, and a usage limit goes to #1563. `long_jobs::recovery_1_a_passing_error_continues_the_same_session`, `long_jobs::recovery_1_three_tries_then_the_real_reason`, `long_jobs::recovery_1_a_good_turn_gives_no_more_tries`, `long_jobs::recovery_1_a_login_or_a_limit_is_not_retried`, `long_jobs::recovery_1_the_words_a_turn_died_on_go_with_its_next_try`, `long_jobs::recovery_1_the_owners_words_end_a_wait_and_give_more_tries`, `long_jobs::recovery_1_an_earlier_runs_stderr_is_no_reason`, `long_jobs::recovery_1_the_agents_last_words_are_not_the_error`, `long_jobs::recovery_1_a_retry_keeps_the_ceiling_and_the_budget`, `long_jobs::recovery_1_a_retry_keeps_the_mode_and_the_mark`, `jobs::tests::recovery_1_only_whole_words_say_trying_again_wont_mend`, `adapters::recovery_1_codex_and_cursor_continue_after_a_cut`, `adapters::recovery_1_a_cursor_that_cant_load_its_session_starts_no_new_one`.
- **RECOVERY-2** A run cut by obid or the Mac going down continues in its same session when obid starts again, told what happened, at most 3 per job; the fourth stays `interrupted`. A panic before publishing began does the same; one after it began, or with no session, ends as today, and a job the owner stopped stays stopped. `long_jobs::recovery_2_a_run_cut_by_a_restart_continues_at_most_three_times`, `jobs::tests::recovery_2_a_panic_before_publishing_continues_its_session`, `jobs::tests::recovery_2_a_real_panic_is_scheduled_again`, `long_jobs::recovery_2_the_run_after_a_panic_hears_of_it`, `long_jobs::recovery_2_the_words_a_run_went_down_with_go_with_the_next`; #915's guard: `failures_api::a_job_obid_keeps_going_down_with_stays_interrupted`.
- **RECOVERY-3** A job gone quiet is stopped and continued in its session, with no supervisor or orchestrator turn, up to 5 tries, and only then takes today's stalled path. `long_jobs::recovery_3_a_quiet_job_continues_in_its_session`, `long_jobs::recovery_3_five_tries_then_it_stalls`, `adapters::recovery_3_codex_and_cursor_continue_when_quiet`.
- **RECOVERY-4** The 200-turn limit continues the same session, counted in RECOVERY-3's 5. The ceiling, the budget and the loop detector stall at once, as they did. `long_jobs::recovery_4_the_turn_limit_continues_and_counts_with_quiet`; the guards: `long_jobs::a_looping_job_stalls`, `long_jobs::the_token_budget_stalls_and_only_the_owners_confirm_carries_on`, `long_jobs::the_ceiling_counts_every_run`.
- **RECOVERY-5** While it's tried again, the job shows as running and nothing says it failed; its step says it's trying again and when, the real error only from the third try; Stop ends a wait. `long_jobs::recovery_5_a_retry_shows_as_running_with_the_error_held_back`.
- **RECOVERY-6** Claude Code's own `system/api_retry` counts as activity, so its retrying trips no quiet window, and shows the same way. `long_jobs::recovery_6_claude_codes_own_retrying_is_activity`.

**Test it when it's done (real Mac):** turn the Wi-Fi off during a task until Claude Code gives up, then on: the task says it's trying again and goes on, with no alert. Quit obid during a task and start it: the task continues, and its agent says it checked what ran.
