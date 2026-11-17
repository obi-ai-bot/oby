# A passing error is the agent's: obid no longer resends the turn

- **Date:** 2026-11-17
- **Status:** In force
- **Replaced:** RECOVERY-1's passing-error tier: a turn that a 5xx, an overloaded API, a 429 or a dropped connection ended stayed `running` and was sent again after 1, 5 and 15 minutes, 3 tries in a row; with it, `long_jobs::a_transient_error_is_retried`, `long_jobs::retries_run_out`, RECOVERY-7's `long_jobs::recovery_7_a_429_past_its_tries_is_no_limit`, and CHATGPT-14's "a 503 backs off"
- **Changed by:** Nothing
- **Contracts and scenarios:** [recovery.md](../contracts/recovery.md) (RECOVERY-1, RECOVERY-5, RECOVERY-7, RECOVERY-19), [usage.md](../contracts/usage.md) (USAGE-12), [tasks.md](../contracts/tasks.md) (TASKS-25), [chatgpt-sign-in.md](../contracts/chatgpt-sign-in.md) (CHATGPT-14), [admission.md](../contracts/admission.md) (ADMISSION-4)

## The owner's words

> Close the Gap report's findings.

The owner's words of 2026-11-17, as the session that started this work relayed them.

> Drop the "passing error" retry tier and keep the stall and ceiling rules.

The Gap report's words (finding 8, "Compared with T3 Code and OpenBot, Oby is smaller but carries layers the others leave to the CLI"), which the owner's word above adopts. Not a quote of the owner.

## Where it is stated

- [recovery.md](../contracts/recovery.md), "End-of-job rules" and RECOVERY-1, RECOVERY-19
- AGENTS.md, "Dispatcher, not harness": "a turn's own retries ... stay in the CLI"

## Notes

Think of a courier who already knocks ten times before leaving a note. obid was a second courier who, finding the note, came back three more times over twenty minutes. Claude Code retries an overloaded API, a 5xx, a 429 and a dropped connection itself, ten times with growing waits (its `system` `api_retry` lines, which obid still shows as activity: RECOVERY-6), and Codex retries its requests and dropped streams itself. A turn that still ends on one has used up the agent's own retries; resending it from obid was a turn's own retry done twice.

How the two peers the Gap report compared do it:

- **T3 Code** (`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts`, `terminalStatusFromResult` and `isOverloadedResult`): a Claude result with `api_error_status` 529 or 429 marks the turn `failed`, and nothing sends it again. Its provider "retry" record (`CodexAdapterV2.ts`) only shows the CLI's own attempt and limit.
- **OpenBot** (`src/backend/claude-client.ts`): Claude's `system` `api_retry` frames set a "retrying" status that the next reply frame clears; a `success` result with `is_error` is a turn that ended on an API error, and the turn ends there.

What changes for the owner: a turn a passing error still ends after the CLI's own retries ends the job at once, `interrupted` with the tool's words and `Try again`, the branch and session kept. It shows no "Trying again in …" wait, takes none of the job's 10 continuations (RECOVERY-17), and marks no account full: a 429 is still no usage limit (RECOVERY-7). On a ChatGPT login, OpenAI's 503 through obid's relay goes back to Codex as it came, Codex backs off itself, and a turn that still ends on it ends the same way.

What stays, because it is the dispatcher's, not a turn's retry:

- a CLI that ended with no result while there's a session (it crashed or was killed, so it can't retry anything itself): 3 tries in its session after 1, 5 and 15 minutes (RECOVERY-1);
- any other error with a session: one try after a minute (RECOVERY-1);
- the network before a session: a first fetch (obid's own git) and a first launch, 3 tries each (RECOVERY-14);
- the stall rules (quiet, idle, spinning, the 200-turn limit, the loop detector: RECOVERY-3, 4, 12, 16), the 12-hour ceiling and the token budget (RECOVERY-4), and the cap of 10 continuations (RECOVERY-17), all exactly as before;
- a passing error on an account its usage or the tool says is full is still its limit's: it moves or waits by itself (LIMITS-15).

The cost, said plainly: an Anthropic or OpenAI outage that outlasts the CLI's own retries (about ten minutes for Claude Code) now leaves the owner's jobs `interrupted` with `Try again`, where obid used to carry them on by itself after up to 21 more minutes. The owner's Try again picks each up in its session.

The conversation harness (`brain/`) and Oby's review for another Oby (TEAMREVIEW-6, proposed) were outside this change.
