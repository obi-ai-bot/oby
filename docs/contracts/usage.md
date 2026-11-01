# Usage: each account's 5-hour and weekly limits from the first launch

The business behavior contract for the Usage panel by the message box and Settings › Agents' usage rows (Sangam's design, [#1564](https://github.com/obi-ai-bot/oby/pull/1564): `apps/electron/src/renderer/src/Composer.tsx`, `UsagePanel`; `SettingsView.tsx`, `Agents`), from [the template](../business-contract-template.md). Issue [#1626](https://github.com/obi-ai-bot/oby/issues/1626). Scenarios are numbered `USAGE-n`; the tests that prove them are named after them (`usage_1_…`).

## Feature and status

- **Outcome:** From the first launch, before any task has run, the owner sees how much of each signed-in account's 5-hour session and week is used, and when each starts over. Oby routes work by the same figures.
- **Status:** **Implemented.** obid read Claude Code's own `/usage` and Codex's `account/rateLimits/read` whenever an app looked (#1529). But a Mac whose own `claude` hadn't run for a while had an ended Keychain login, which no read refreshed, so Claude Code showed "No use reported yet" until a task ran. #1626 has the read ready the login first, as a job's run does, and the refresh is now Claude Code's own `/usage`, which asks no model.
- **Scope:** Claude Code and Codex accounts on this Mac's obid. **Out of scope:** Cursor, whose CLI reports no usage (its dashboard would need its session login: the owner's decision); Anthropic's OAuth usage endpoint called by obid itself (obid would hold the token: the owner's decision, and not needed).

## What Sangam's design assumes (each one kept)

| The design | What obid promises |
|---|---|
| Each account shows its 5-hour session bar ("Resets 9:59 PM · 0%") and its week ("This week 47% · resets Wednesday"). | `fiveHour`, `fiveHourResets`, `sevenDay` and `sevenDayResets` on each account in `GET /v1/status` and the `accounts` event. |
| Nothing reported shows "No use reported yet"; Cursor shows "Cursor doesn't report usage". | Both null until a tool reports, and never for Cursor. |
| The panel shows the figures when it opens. | Opening the panel asks `GET /v1/status`, which starts the reads that are due. |

## Behavior

### Actors and ownership

- **obid** decides when to read, runs the tools' own reports, keeps the figures in `home/usage.json` and routes by them.
- **The tools** report: Claude Code's `/usage` and its `rate_limit_event` during a run; Codex's `account/rateLimits/read` and `account/rateLimits/updated`.
- **The Electron app** shows obid's figures in Sangam's words, and asks for a look when the panel opens.

### Scenarios

- **USAGE-1:** When an app looks (`GET /v1/status`, its `GET /v1/events` stream opening, the Usage panel opening), obid reads each signed-in Claude Code and Codex account not read in the last 10 minutes, one after another in the background, and the figures come in an `accounts` event. The panel shows the session's share and reset time, and "This week N% · resets <day>". While no app looks, nothing is read. A running job's own reports come between reads.
- **USAGE-2:** A Claude Code login in the Keychain (`claudeKeychain`: the default login, or an account obid added) that has ended or ends within 5 minutes is refreshed before the read by the Mac's own Claude Code, unboxed, with its own `/usage`. That asks no model (Claude Code 2.1.302: `num_turns` 0, `total_cost_usd` 0, no `modelUsage`). Then the boxed read runs with a fresh copy. The keeper keeps that login fresh for the next hour, as after any boxed run, with the same free `/usage`. Jobs' refreshes use it too.
- **USAGE-3:** No read asks a model. A Claude Code whose `/usage` asked one (an older one without its own) isn't asked again until obid restarts.
- **USAGE-4:** A read that failed (offline, a refused login) or answered no figures (an API key's account) is tried again after 10 minutes. The panel keeps what it showed: the last figures, or "No use reported yet".
- **USAGE-5:** A login obid doesn't refresh (a login file, or an account's own folder while the Swift engine may run on it, #655) within 10 minutes of its end isn't read with. It shows "No use reported yet" (or its last figures) until the person runs `claude` on that account.
- **USAGE-6:** Cursor accounts show "Cursor doesn't report usage", whatever obid says of them.

### What it costs

- Claude Code: one `claude -p /usage` per account per 10 minutes while an app looks. With an ended login, one more unboxed `claude -p /usage` refreshes it first. Neither asks a model: $0, nothing from the 5-hour or weekly limits.
- Codex: one `account/rateLimits/read` on an app-server with no thread: $0.
- The keeper's refresh used to be `claude -p ok` on Haiku: one small model turn per refresh (about one per access token's life, some 8 hours, while the login was in use). It's now `/usage`: $0.

## Invariants

- obid never reads, sends or keeps a login beyond the documented `claudeKeychain` copy and the Mac's own Claude Code's refresh (AGENTS.md). The refresh runs the person's own Claude Code by its absolute path, with an empty config folder of obid's and the system's `PATH`, and its output is discarded.
- obid's API is unchanged: no key or route is added or removed.
- A read never runs a model and never refreshes from inside a box.

## Tests

| Scenario | Test |
|---|---|
| USAGE-1 | `accounts_api::usage_1_an_apps_look_reads_each_accounts_usage`, `apps/electron/src/renderer/src/test/usage.test.ts` (`USAGE-1:`) |
| USAGE-2 | `accounts_api::usage_2_an_ended_keychain_login_is_refreshed_free_then_read`, `accounts_api::an_added_claude_accounts_login_is_kept_fresh`, `jobs::logins::tests::the_macs_claude_code_refreshes_a_login_near_its_end` |
| USAGE-3, USAGE-4, USAGE-5 | `accounts_api::usage_3_4_5_usage_reads_retry_but_never_after_a_model_ran` |
| USAGE-6 | `apps/electron/src/renderer/src/test/usage.test.ts` (`USAGE-6:`) |

## Test it when it's done (on a real Mac)

1. With the Mac's own `claude` not run for a day (its Keychain token ended: obid's log said `usage not read: its login ends within 10 minutes`), install this obid and open Oby. Within a minute, the Usage panel shows Claude Code's session bar and "This week N% · resets <day>", matching `claude` → `/usage`.
2. `~/Library/Logs/obid.log` has no `usage not read` line for the default account, and no `didn't refresh its login` line.
3. Claude Code's usage page (claude.ai › Settings › Usage) shows no new usage from the refresh.
