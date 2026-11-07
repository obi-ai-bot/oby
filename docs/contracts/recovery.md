# Recovery: a job carries itself on before anyone is told

The business behavior contract for what obid does by itself when a job's run is cut, from [the template](../business-contract-template.md). The API is `docs/runner-api.md` (`limit`, `stalled`, "After any restart", "obid's own bugs").

## Feature and status

- **Outcome:** The owner's direction: "Follow the exact way Claude Code follows." A job whose run was cut carries itself on and tells a person last; every automatic loop has a limit, and past it the job ends plainly, with the real reason.
- **Status:** **Implemented.** The words on the step lines are proposed, and Sangam's to change.
- **Scope:** The owner's own jobs on Claude Code, Codex and Cursor; RECOVERY-7's reading is for a conversation's turns too ([conversation-agent.md](conversation-agent.md), CONVERSATION-AGENT-15). Outside it: usage limits ([account-limits.md](account-limits.md), #1563), the Confirms, follow-ups on a new session or branch, a nudge for a looping agent, publish retries, `CLAUDE_CODE_RETRY_WATCHDOG`, and a review another Oby asked for (its answer goes back to that Oby).

## Behavior

### Actors and ownership

- **obid** decides, counts and continues. It spends no model turn deciding, while a job is being tried again or once its tries are spent ("End-of-job rules"). The job's own agent hears a marked message in its own session.
- **The apps** only show `status` and `step`: no new API key, status or enum value. The counts stay in the job's file, never in the API.

### End-of-job rules

Every way a job's run ends has one rule, and obid applies it itself: no model turn reads the end and decides (the event orchestrator that did, and whose turns mostly left a second card with only Done, is retired, and so is the conversation supervisor that took stalled jobs on: rules-delete-supervisor, the owner's decision of 2026-11-06). Past its tries a job shows its own card, with its real reason, and nothing else. **One gap, open:** when Oby's own review of a done job's pull request ends without a verdict and no other reviewer takes it (the review job ends done with no verdict, ends done with commits of its own, or is stopped), obid only sets the author's `reviewError` (`Oby's review didn't finish: …`) and raises no card or item. The orchestrator's `review_error` event was the only thing that told the owner, so until obid raises one itself that pull request waits with nobody told; the owner sees it only on the task's Review stage.

| Kind | Trigger | Tries | Waits | Past them: obid's rule, and what the owner sees |
|---|---|---|---|---|
| A passing error | A turn ends on the transient allowlist (`API Error: 5xx` or `429`, overloaded, a dropped connection or stream), or the CLI ends with no result while there's a session | 3 | 1, 5, 15 minutes (never longer than `stallAfter`) | `interrupted`: its card says the real reason and offers `Try again` |
| obid or the Mac went down | obid starts with a job at work | 3 | None | `interrupted`: `obid or the Mac went down 4 times…` and `Try again` |
| obid's own bug | A panic in the job's task before publishing began | 3, shared with the restarts | None | `failed`, a `crash` record and one `failed` item |
| Gone quiet | No progress for `stallAfter` | 5, shared with the turn limit | None | Stalled (below) |
| The 200-turn limit (Claude Code only) | `error_max_turns` | 5, shared with quiet | None | Stalled (below) |
| A loop, the 12-hour ceiling, the token budget | The loop detector, `jobCeilingHours`, `jobTokenBudget` | None | None | Stalled (below); past a ceiling or budget only the owner's own words carry it on |
| A session too long for its model | The tool says so (RECOVERY-7) | 1 new session, until the owner's own words | None | `interrupted` with the tool's words and `Try again`, which starts a new session again |
| A check that keeps failing | A `fix`, `build` or `test` job with a check and no `rounds` named fails it the same way two rounds running with no passing run between (its runs' `signature`: the failing step, its failing tests and their messages, else its whole output; temporary paths, UUIDs, runs of hex, timings and numbers taken out, but not a count of what's left) (RECOVERY-9) | Until then, at most 8 rounds | None | `interrupted`, `Stopped at a limit: \`<check>\` still fails after <n> round(s). …` and `One more round`; a `rounds` named is counted to its end instead |
| A login error, a session that's gone, an error off the allowlist | The tool's own fields, else whole words on stderr (RECOVERY-7) | None | None | Ends as the tool said (`failed` or `interrupted`), with its words |
| A usage limit | The account's usage or the tool says it's full | Its own ([account-limits.md](account-limits.md)) | Until the reset, five-hour limits only (LIMITS-5) | Any other limit: the limit card, asked once (#1563) |
| The owner's Stop | Stop | None | None | `stopped`: never continued by itself |
| Done with commits | The agent's turn ends well | None | None | Published, reviewed and merged by [publishing.md](publishing.md) and [merging.md](merging.md); their own items when they can't. Except the open gap above: a review that ends with no verdict and no reviewer left only sets `reviewError`, with no card, and the pull request waits silently |

**Stalled** is `interrupted` with `stalled` set and the agent's handoff note. Every stalled job stays stalled, a conversation started it or not: its own card offering `Try again`, and its conversation told how it ended. No turn takes it on by itself, so the owner sees more stall cards until the stall causes are fixed.

### What the agent hears

`[From Oby, by itself: not the owner's words] Your run was cut: <what>.` Then, for an error, that error quoted as data on one line (masked, at most 200 characters). Then: check what really ran before repeating anything, and carry on where it left off. `End of Oby's note.` closes it, so what follows isn't read as Oby's.

What follows is what the cut may have lost. After a passing error or a quiet job: `What you were last sent, which may not have reached you (check before acting on it again):` and all of it, with any words that came meanwhile, so the owner's words, a failed check's output, a rebase order or the very first prompt are never replaced by "carry on". At the turn limit nothing follows: 200 turns in, it was heard. After a restart with a session: the same, kept in the job's file until its turn ends well, since the owner's words it holds are nowhere else; with nothing kept, the prompt, or the owner's words, as it always did.

The error is what the CLI's `result` says or, with none, the run's exit and what that run printed on stderr. It's never the agent's own last words, which can quote anyone, nor an earlier run's stderr (`agent.log` keeps them all).

A new session, when the last grew too long, hears `[From Oby, by itself: not the owner's words] Your run was cut: your session grew too long for your model, so this is a new one with none of its history. What was done so far is on this branch: read its log and diff first…`, then all its first message said (its task, the repository's rules, the owner's memories, teammates' pull requests, Tetrix's context) and what it was last sent.

### How a turn's end is read

From the tool's own fields, in this order, and only then from its own words, for older CLIs and Cursor. The agent's words are never read for any of it, so a stranger's comment that says "you've hit your usage limit" holds nothing.

| Tool | Its words | Passing | Too long | Session gone |
|---|---|---|---|---|
| Claude Code | `errors`, else the `result` of an error with subtype `success` | `api_error_status` 5xx, or 429 unless the account is full | `terminal_reason` `prompt_too_long`, `blocking_limit` or `rapid_refill_breaker` | `errors`' `No conversation found` |
| Codex | `turn.error.message` | `codexErrorInfo` `serverOverloaded`, `internalServerError`, or a connection or stream kind short of a 4xx; `rateLimitExceeded` unless full | `contextWindowExceeded` | Its answer to `thread/resume` was an error |
| Cursor | What ACP answered, or its `stopReason` | Words only | Words only | Its answer to `session/load` was an error, or it can't load |

A status beats the words about a status: `API Error: 400 … overloaded` isn't passing. Words that name an account's usage limit beat a 429, which a plan's limit answers too. Codex's `usageLimitExceeded` is a limit whatever its words. A 429's own words say "rate limit", and that's no usage limit: past its tries it ends as any passing error does, and neither a job nor a conversation marks its account full over one. Only a turn the tool's fields can't place (no status, no reason) has its words read for a limit.

### What the person sees

- The job stays `running` (after a restart or a bug: `queued`, then `running`). No Failed or Needs you alert, no card and no inbox item goes out until the tries are spent.
- Its `step`: `Trying again in 5 minutes (2 of 3)`, counting down by the minute. The real error is added only from the third try: `Trying again in 15 minutes (3 of 3): API Error: 529`. Gone quiet or at the turn limit: `Trying again now (2 of 5)`. After a restart: `Oby restarted while this ran: continuing it (1 of 3)`, or with no count after a clean stop.
- Claude Code's own retrying shows the same way with its own numbers (`Trying again in 8 seconds (1 of 10)`), then `Working` once it's heard from again.
- Stop works during every wait, and Stop is final: a job the owner stopped never continues by itself. Words for the job end a wait: they go with its next try.

### Failures and recovery

- A login error, or a session that's gone, isn't tried again, nor is an error that isn't on the allowlist. With no result from the CLI, that run's stderr decides, in whole words: a login (`not logged in`, `/login`, `authentication`, `401`, …) or `No conversation found` ends the job as today.
- A usage limit goes to [account-limits.md](account-limits.md) as today, whatever cut the turn: a passing error on an account its usage or the tool says is full is that limit's. It takes none of these tries: a five-hour limit counts its own (LIMITS-9).
- A try goes to its own session or doesn't run: a tool that can't load sessions (a Cursor without `loadSession`) leaves the job `interrupted` with why, and Try again starts a new session. A Codex thread that refuses to resume (cut before its first prompt) ends with Codex's own words, not proven on a real Codex.
- With no session to continue: a passing error runs the same prompt again, as it did. A CLI that ended with no result, a restart, a panic or a stall ends as today.
- A panic once publishing began (its claim, its phase, its step, or a branch already pushed) ends as today: nothing can run twice mid-publish.
- A restart while checking or publishing picks up at that phase with no agent turn ("After any restart"): it isn't counted, since no agent is told anything.
- #915's guard stands first: a job picked up twice in 30 minutes after obid went down stays `interrupted`, its item in the inbox. Past RECOVERY-2's tries it's the same: one `failed` item and a `crash` record, the owner's.
- A job `queued` for a slot when obid went down wasn't cut: it isn't counted, or told it was. A clean stop (a quit, an update) isn't counted: a job restarted 4 times by obid's own updates keeps running, and its `step` reads `Oby restarted while this ran: continuing it` with no count. Only #915's guard (for crashes) and the job's ceiling bound it.

### Invariants

- Retried tries count together. They live on the job, survive restarts, and start again only on the owner's own words (signed, their Confirm, or `oby_tell_job` in a turn of a conversation they're watching, which needs their message): never on a good turn or the legacy API's. So no retry multiplies another. A job's tree reaches at most 11 tries per job (3 passing errors, 3 restarts or bugs, 5 quiet or turn-limit); no turn adds any.
- A retry never resets the 12-hour ceiling or the token budget, and never touches the loop detector: those are the guards against burning accounts, and they stall at once. A waiting job's time counts toward its ceiling, saved as it waits.
- Claude Code reports tokens only on its `result` line, so a run cut before that counts nothing until a resumed run reports: up to 11 cut runs a job can pass the budget unseen. The 12-hour ceiling is the guard there.
- A retry never answers a permission, merges or changes a setting, and keeps the job's untrusted mark, mode and box.
- A job waiting to try again takes no slot meanwhile, and takes it back at once when its wait ends (ADMISSION-4).

## Claude Code, beside Oby

Claude Code's side is what the 2.1.302 binary, the app-managed 2.1.309 binary and Claude.app 4.4212.0 showed when read in 2026-11; only `api_retry` was also run (below).

| Situation | Claude Code | Oby | Same? |
|---|---|---|---|
| API error, overload, dropped connection | 10 retries with backoff inside the CLI | The same: the CLI retries first, and obid sees only what ends the turn | Same |
| Retries spent, on a lost connection | Desktop app: Try again at 1, 5, 15 minutes, 3 tries, reset only by the person, network errors only, probing the API first | 1, 5, 15 minutes, 3 tries, reset only by the owner | Differs: Oby also takes 5xx, overload and 429 (its allowlist), and doesn't probe |
| The process dies mid-turn | Desktop app: respawns with backoff, and doesn't continue the turn by itself (inferred from its filter) | Continued, as a passing error | **Differs on purpose:** the owner asked for less waiting on a person |
| What the person sees | `Retrying in … attempt n/m`; the real error held back until attempt 3, unless the network is down, SSL fails or a rate limit applies | The same line; held back until the third try, always | Differs in the exceptions |
| Sleep, app quit, host restart | A continue message in the same session, marked with its origin; 3 continues, 6 starts, 24 hours on a remote host | A marked message in the same session, 3 per job, no age limit | Differs in the age limit and start count |
| The Mac sleeps | Interrupts the hung turn on waking and tells it | While any job's worker runs, obid keeps the Mac from idle sleep (`caffeinate -i`, `keepAwake`, RECOVERY-10); a closed lid still sleeps, obid isn't restarted by it, and what the sleep broke arrives as a passing error or a quiet job | **Differs:** no sleep detection; idle sleep held off instead |
| A crash loop | Backs off and parks at 5 in a row or 15 in all (inferred) | Picked up twice in 30 minutes, then left (#915) | Differs in the numbers |
| Workflow agent stalls | 90 seconds without progress: aborted and retried up to 5 times, then `stalled on all N attempts` | **15 minutes** without progress: stopped and continued up to 5 times, then stalled | **Differs on purpose:** 90 seconds fits an agent with no long commands, and a job runs them (a build, a test suite). The desktop app's own session timeout is about 16 minutes |
| Subagent fails | Aborted, marked failed; the parent model decides, with no automatic restart | Past the tries, the job stays stalled with its own card | Same shape |
| Turn limit | `max_turns` ends the turn; no continuation | Continued, counted with quiet | **Differs on purpose**, by the owner's direction; Claude Code only, since Codex and Cursor have no such switch |
| Its own retrying seen as progress | `system/api_retry` in stream-json | Any line is progress; `api_retry` also shows on the step | Same |
| Usage limit | Desktop app resumes after the reset, five-hour only, skipping unattended sessions | Resumes after the reset, five-hour only, unattended jobs too; any other limit is the owner's choice ([account-limits.md](account-limits.md), LIMITS-5 to 13) | **Differs on unattended jobs**, the owner's decision of 2026-11-02 |
| Runaway cost | No job ceiling or budget found | 12-hour ceiling, token budget, loop detector, all as they were | Oby only. The loop detector doesn't see Cursor, whose adapter reports no calls |
| Past every limit | A plain failed state (`api_error`, a failed agent, "keeps crashing") | A plain failed state (`interrupted` or `failed`, with the real reason) | Same |

**Proven against the installed CLI:** `claude` 2.1.302 against a local stand-in answering 529, with a made-up key and no real account, printed `{"type":"system","subtype":"api_retry","attempt":1,"max_retries":4,"retry_delay_ms":552,"error_status":529,"error":"overloaded",…}`. Everything else in Claude Code's column comes from reading it, not running it.

The same way, its `result` lines, which RECOVERY-7 reads: a 529 and a 500, `is_error` true, subtype `success`, `terminal_reason` `api_error`, `api_error_status`, `result` `API Error: 529 Overloaded…`; a 429, the same with `API Error: Request rejected (429) · This request would exceed your rate limit`, words the old reading took for no status at all; a 401, `Invalid API key · Fix external API key`; a 400 saying the prompt is too long, `terminal_reason` `prompt_too_long`, `Prompt is too long · the request is ~250000 tokens…`; `--resume` of a session that isn't there, subtype `error_during_execution`, no `result` and no `terminal_reason`, `errors` `["No conversation found with session ID: …"]`. Codex's field names and sentence come from its 0.162.1 schema and binary, not from a run.

On a real account, on a scratch obid and haiku (2026-11-03): a job stopped mid-run, its session grown past what Claude Code's own compaction could mend, then Try again: `Started a new session: its last was too long for its model`, and the new session's first message held obid's note, the task and the owner's memories line (RECOVERY-8). A conversation whose session the API refused (a block it doesn't know, planted in the session's file) answered from a new session with the earlier turns (CONVERSATION-AGENT-15), and the next turn went on there.

## Interfaces and dependencies

- **API:** add-only, and nothing was added: `step` is the existing string, and no status or enum changes. `docs/runner-api.md` says what changed.
- **Persistence:** `recovery` in the job's file (`errors`, `stalls`, `restarts`, what cut it, what its turn under way was sent), never in the API.
- **Clients:** `apps/electron` needs no change: `askOf` shows a card only for `failed` and `interrupted`, `taskOf` shows `queued` and `running` as running with their step, and `notify.ts` alerts only on an end.
- **Open:** the tries are borrowed from Claude Code and not measured. Nothing here has run on a real CLI but what's said above. How well a new session does from the branch alone wasn't measured beyond that one job, which had no commits. Cursor's words for a session too long aren't known, so it isn't told apart.

## Acceptance scenarios

- **RECOVERY-1** A turn cut by a passing error, or a CLI that ends with no result, stays `running` in its same session and worker, and continues with the marked message quoting the error, after 1, 5, then 15 minutes, 3 tries, then ends as today with the real reason. A login or an unknown error isn't retried, and a usage limit goes to #1563. `long_jobs::recovery_1_a_passing_error_continues_the_same_session`, `long_jobs::recovery_1_three_tries_then_the_real_reason`, `long_jobs::recovery_1_a_good_turn_gives_no_more_tries`, `long_jobs::recovery_1_a_login_or_a_limit_is_not_retried`, `long_jobs::recovery_1_the_words_a_turn_died_on_go_with_its_next_try`, `long_jobs::recovery_1_the_owners_words_end_a_wait_and_give_more_tries`, `long_jobs::recovery_1_an_earlier_runs_stderr_is_no_reason`, `long_jobs::recovery_1_the_agents_last_words_are_not_the_error`, `long_jobs::recovery_1_a_retry_keeps_the_ceiling_and_the_budget`, `long_jobs::recovery_1_a_retry_keeps_the_mode_and_the_mark`, `jobs::tests::recovery_1_only_whole_words_say_trying_again_wont_mend`, `adapters::recovery_1_codex_and_cursor_continue_after_a_cut`, `adapters::recovery_1_a_cursor_that_cant_load_its_session_starts_no_new_one`.
- **RECOVERY-2** A run cut by obid or the Mac going down continues in its same session when obid starts again, told what happened, at most 3 per job; the fourth stays `interrupted`. A stop obid was asked for (a quit, an update: `home/stopped-cleanly`) continues it the same way, told it was stopped, and costs no try. A panic before publishing began does the same; one after it began, or with no session, ends as today, and a job the owner stopped stays stopped. `long_jobs::recovery_2_a_run_cut_by_a_restart_continues_at_most_three_times`, `long_jobs::recovery_2_a_clean_restart_costs_no_try`, `jobs::tests::recovery_2_a_panic_before_publishing_continues_its_session`, `jobs::tests::recovery_2_a_real_panic_is_scheduled_again`, `long_jobs::recovery_2_the_run_after_a_panic_hears_of_it`, `long_jobs::recovery_2_the_words_a_run_went_down_with_go_with_the_next`; #915's guard: `failures_api::a_job_obid_keeps_going_down_with_stays_interrupted`.
- **RECOVERY-3** A job gone quiet is stopped and continued in its session, with no model turn, up to 5 tries, and only then stalls: its own card offering `Try again`, whether a conversation started it or not, and no turn takes it on (`supervised` stays `[]`). `long_jobs::recovery_3_a_quiet_job_continues_in_its_session`, `long_jobs::recovery_3_five_tries_then_it_stalls`, `adapters::recovery_3_codex_and_cursor_continue_when_quiet`.
- **RECOVERY-4** The 200-turn limit continues the same session, counted in RECOVERY-3's 5. The ceiling, the budget and the loop detector stall at once, as they did. `long_jobs::recovery_4_the_turn_limit_continues_and_counts_with_quiet`; the guards: `long_jobs::a_looping_job_stalls`, `long_jobs::the_token_budget_stalls_and_only_the_owners_confirm_carries_on`, `long_jobs::the_ceiling_counts_every_run`.
- **RECOVERY-5** While it's tried again, the job shows as running and nothing says it failed; its step says it's trying again and when, the real error only from the third try; Stop ends a wait. `long_jobs::recovery_5_a_retry_shows_as_running_with_the_error_held_back`; on the desktop card, under the steps it already has: `liveTask.test.tsx`, "RECOVERY-5".
- **RECOVERY-6** Claude Code's own `system/api_retry` counts as activity, so its retrying trips no quiet window, and shows the same way. `long_jobs::recovery_6_claude_codes_own_retrying_is_activity`.

- **RECOVERY-7** How a turn ended is read from the tool's own fields, its words only as the fallback, and never the agent's words: a 429 is a passing error, a status beats the words, and words of the agent's that name a limit hold nothing. `agent::tests::recovery_7_how_a_turn_ended_is_read_from_the_tools_fields`, `codex::tests::recovery_7_codex_error_info_says_how_a_turn_ended`, `agent::tests::recovery_7_a_rate_limit_is_no_usage_limit`, `long_jobs::recovery_7_a_429_is_read_from_its_status`, `long_jobs::recovery_7_a_429_past_its_tries_is_no_limit`, `conversation_harness::recovery_7_a_conversations_429_holds_no_account_full`, `long_jobs::recovery_7_the_agents_words_are_no_limit`, `adapters::recovery_7_codex_says_how_its_turn_ended`, `adapters::recovery_7_codex_says_its_limit_in_its_field`, `conversation_codex::recovery_7_the_agents_own_words_never_say_a_limit`.
- **RECOVERY-8** A session too long for its model goes on in a new one, told so with all its first message said, no one asked, and isn't told to Try again in that session. Once until the owner's own words; too long there too, it ends `interrupted` with the tool's words. `long_jobs::recovery_8_a_session_too_long_goes_on_in_a_new_one`, `long_jobs::recovery_8_a_new_session_hears_all_it_was_first_told`, `long_jobs::recovery_8_too_long_again_tells_the_owner`, `adapters::recovery_8_codex_out_of_room_goes_on_in_a_new_thread`.
- **RECOVERY-9** A committing job with a check and no `rounds` named reads `rounds` 8 and stops at a limit, with `One more round`, as soon as its check fails the same way two rounds running, by each failed run's `signature`, with no passing run between; a failure that changes every round goes on to the 8th, and One more round after such a stop runs exactly one more when it repeats. A `rounds` named keeps counting to its end. `jobs_api::recovery_9_the_same_failure_twice_stops`, `jobs_api::recovery_9_a_changing_failure_goes_on_to_the_ceiling`, `facts::tests::recovery_9_signature_reads_the_failure_not_its_noise`, `facts::tests::recovery_9_signature_reads_failing_tests_and_their_messages`, `facts::tests::recovery_9_a_pass_between_two_failures_is_no_repeat`.
- **RECOVERY-10** While any job's worker runs, an idle one waiting on the check queue or a retry's timer too, obid holds `/usr/bin/caffeinate -i -w <obid's pid>`, so idle sleep doesn't freeze agents, checks or retry clocks; it ends that once no worker is left, and with obid. Idle sleep only: a closed lid still sleeps. `keepAwake: false` turns it off. macOS only. `long_jobs::recovery_10_a_running_job_keeps_the_mac_awake`, `config::tests::recovery_10_keep_awake_is_on_unless_set_false`.

**Test it when it's done (real Mac):** turn the Wi-Fi off during a task until Claude Code gives up, then on: the task says it's trying again and goes on, with no alert. Quit obid during a task and start it: the task continues, and its agent says it checked what ran.
