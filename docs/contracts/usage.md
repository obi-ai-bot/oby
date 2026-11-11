# Usage: each account's 5-hour and weekly limits from the first launch

The business behavior contract for the Usage panel by the message box and Settings › Agents' usage rows (Sangam's design, [#1564](https://github.com/obi-ai-bot/oby/pull/1564): `apps/electron/src/renderer/src/Composer.tsx`, `UsagePanel`; `SettingsView.tsx`, `Agents`), from [the template](../business-contract-template.md). Issue [#1626](https://github.com/obi-ai-bot/oby/issues/1626). Scenarios are numbered `USAGE-n`; the tests that prove them are named after them (`usage_1_…`).

## Feature and status

- **Outcome:** From the first launch, before any task has run, the owner sees how much of each signed-in account's 5-hour session and week is used, and when each starts over. Oby routes work by the same figures.
- **Status:** **Implemented.** Cursor's read is the owner's decision of 2026-11-03 (USAGE-6 to 9). obid read Claude Code's own `/usage` and Codex's `account/rateLimits/read` whenever an app looked (#1529). But a Mac whose own `claude` hadn't run for a while had an ended Keychain login, which no read refreshed, so Claude Code showed "No use reported yet" until a task ran. #1626 has the read ready the login first, as a job's run does, and the refresh is now Claude Code's own `/usage`, which asks no model.
- **Scope:** Claude Code, Codex and Cursor accounts on this Mac's obid. **Out of scope:** the iPhone and Android apps, which still say Cursor doesn't report usage in Settings and call its figure a week ([#1885](https://github.com/obi-ai-bot/oby/issues/1885)); Anthropic's OAuth usage endpoint called by obid itself (obid would hold the token: the owner's decision, and not needed).

## What Sangam's design assumes (each one kept)

| The design | What obid promises |
|---|---|
| Each account shows its 5-hour session bar ("Resets 9:59 PM · 0%") and its week ("This week 47% · resets Wednesday"). | `fiveHour`, `fiveHourResets`, `sevenDay` and `sevenDayResets` on each account in `GET /v1/status` and the `accounts` event. |
| Nothing reported shows "No use reported yet". A Cursor account with no figure shows "No usage", which replaces his "Cursor doesn't report usage" (the owner's decision of 2026-11-03). | Both null until a tool reports; a Cursor account's `sevenDay` null while its read gives none. |
| The panel shows the figures when it opens. | Opening the panel asks `GET /v1/status`, which starts the reads that are due. |

## Behavior

### Actors and ownership

- **obid** decides when to read, runs the tools' own reports, keeps the figures in `home/usage.json` and routes by them.
- **The tools** report: Claude Code's `/usage` and its `rate_limit_event` during a run; Codex's `account/rateLimits/read` and `account/rateLimits/updated`. Cursor's CLI reports nothing, so obid asks Cursor's dashboard service itself.
- **The Electron app** shows obid's figures in Sangam's words, and asks for a look when the panel opens.

### Scenarios

- **USAGE-1:** When an app looks (`GET /v1/status`, its `GET /v1/events` stream opening, the Usage panel opening), obid reads each signed-in account not read in the last 10 minutes, one after another in the background, and the figures come in an `accounts` event. The panel shows the session's share and reset time, and "This week N% · resets <day>". While no app looks, nothing is read. A running job's own reports come between reads.
- **USAGE-2:** A Claude Code login in the Keychain (`claudeKeychain`: the default login, or an account with a folder of its own) that has ended or ends within 5 minutes is refreshed before the read by the Mac's own Claude Code, unboxed, with its own `/usage`. That asks no model (Claude Code 2.1.302: `num_turns` 0, `total_cost_usd` 0, no `modelUsage`). Then the boxed read runs with a fresh copy. Every boxed run's refresh before it starts uses the same free `/usage` (docs/contracts/keychain.md, KEYCHAIN-10).
- **USAGE-3:** No read asks a model. A Claude Code whose `/usage` asked one (an older one without its own) isn't asked again until obid restarts.
- **USAGE-4:** A read that failed (offline, a refused login) or answered no figures (an API key's account) is tried again after 10 minutes. The panel keeps what it showed: the last figures, or "No use reported yet".
- **USAGE-5:** A Claude Code account with a `configDir` the configuration names has its Keychain login refreshed like one obid added (USAGE-2): no box ever gets an account's own folder (#655), so nothing waits on it. A login obid doesn't refresh (a login file) within 10 minutes of its end isn't read with. It shows "No use reported yet" (or its last figures) until the person runs `claude` on that account.
- **USAGE-6:** A Cursor account shows the figure obid gives: its percentage in the picker's list, the Usage panel and Settings, as "N% used · resets <its day, or its date past a week>", naming no week since its period may be a month. With no figure it shows "No usage".
- **USAGE-7:** obid takes a Cursor account's figure from `GetCurrentPeriodUsage`: the highest share among `planUsage`'s `includedSpend` over a numeric `limit` above 0, `totalPercentUsed`, `autoPercentUsed` and `apiPercentUsed` (percent fields use 0 to 100). Only present numeric fields count; missing spend isn't zero, and with no share there is no figure (USAGE-8). It's `sevenDay`, for a weekly or a monthly period (no key added). Its reset is `GetPlanInfo`'s `includedUsageResetsAt`, else `billingCycleEnd`; a read takes the place of the last whole. A figure past the included usage reads 100%. Each parsed read logs one INFO line with only these numbers and the winning field, never credentials or other answer content. USAGE-9 remains unchanged.
- **USAGE-8:** A failure is quiet: signed out, a 4xx or 5xx, a timeout, no `planUsage` (usage isn't available for that plan), or an answer that doesn't read gives no number, whatever it read before, and is tried again after 10 minutes. It marks nothing full, blocks no run and shows no error. A `GetPlanInfo` that fails only loses the included usage's reset.
- **USAGE-10:** A 5-hour reading past its reset says nothing beside a week: a plan with no 5-hour window, or one idle for days, must not read 0% with its week at 91%. The app's panel and Settings then show the week as the account's number and bar, as they do for Cursor's figure. A session still going keeps its bar and reset, at 0% too. With no week, an ended session reads 0%. An account that isn't signed in has no bar.
- **USAGE-9:** A Cursor figure holds nothing back, since a Cursor plan may go on past its included usage: at any number the account isn't full or reserved, has room, takes work as with no figure, gets no job's window share, and is no evidence of room for a Resume. It only orders Cursor's own accounts, the emptier first (CONVERSATION-AGENT-15: Cursor stays last on Auto).

### What it costs

- Claude Code: one `claude -p /usage` per account per 10 minutes while an app looks. With an ended login, one more unboxed `claude -p /usage` refreshes it first. Neither asks a model: $0, nothing from the 5-hour or weekly limits.
- Codex: one `account/rateLimits/read` on an app-server with no thread: $0.
- Cursor: two `POST`s per account per 10 minutes while an app looks, the same reads Cursor's own app and CLI make: $0.
- The refresh used to be `claude -p ok` on Haiku: one small model turn per refresh (about one per access token's life, some 8 hours, while the login was in use). It's now `/usage`: $0, and it runs only right before a boxed run that needs it (KEYCHAIN-10).

## Invariants

- obid never reads, sends or keeps a login beyond the documented `claudeKeychain` copy, the Mac's own Claude Code's refresh (AGENTS.md), Cursor's access token to Cursor's own host for its usage (docs/contracts/keychain.md, KEYCHAIN-8 and 9) and Continue with ChatGPT, which it is the OAuth client of (docs/contracts/chatgpt-sign-in.md). The refresh runs the person's own Claude Code by its absolute path, with an empty config folder of obid's and the system's `PATH`, and its output is discarded.
- obid's API is unchanged: no key or route is added or removed, and no value in an enum.
- A read never runs a model and never refreshes from inside a box.

## Tests

| Scenario | Test |
|---|---|
| USAGE-1 | `accounts_api::usage_1_an_apps_look_reads_each_accounts_usage`, `apps/electron/src/renderer/src/test/usage.test.ts` (`USAGE-1:`) |
| USAGE-2 | `accounts_api::usage_2_an_ended_keychain_login_is_refreshed_free_then_read`, `accounts_api::an_added_claude_accounts_login_is_kept_fresh`, `jobs::logins::tests::the_macs_claude_code_refreshes_a_login_near_its_end` |
| USAGE-3, USAGE-4, USAGE-5 | `accounts_api::usage_3_4_5_usage_reads_retry_but_never_after_a_model_ran` |
| USAGE-5 | `accounts_api::usage_5_a_named_folder_login_is_refreshed_like_an_added_one` |
| USAGE-6 | `apps/electron/src/renderer/src/test/usage.test.ts` (`USAGE-6:`), `test/talk.test.ts` (`CONVERSATION-AGENT-12:`), `jobs::tests::usage_6_8_9_a_cursor_accounts_figure_from_its_login_to_what_is_kept` |
| USAGE-7 | `cursor_usage::tests::usage_7_a_monthly_plan_reads_its_total_and_its_cycles_end`, `cursor_usage::tests::usage_7_a_weekly_plan_reads_its_weeks_reset`, `cursor_usage::tests::usage_7_a_limit_above_zero_is_included_spend_over_it`, `cursor_usage::tests::usage_7_the_highest_reported_share_wins_including_auto_only`, `usage::tests::usage_7_a_cursor_read_takes_the_place_of_the_last_whole` |
| USAGE-8 | `cursor_usage::tests::usage_8_no_plan_usage_is_no_number_and_no_second_request`, `cursor_usage::tests::usage_8_a_401_and_a_500_are_no_number_saying_only_the_status`, `cursor_usage::tests::usage_8_garbage_is_no_number`, `cursor_usage::tests::usage_8_a_plan_read_that_fails_keeps_the_figure`, `usage::tests::usage_8_a_cursor_read_with_no_figure_forgets_the_last`, `jobs::tests::usage_6_8_9_a_cursor_accounts_figure_from_its_login_to_what_is_kept` |
| USAGE-7 diagnostics | `cursor_usage::tests::usage_7_one_info_line_has_only_numbers_and_the_winning_field` |
| USAGE-10 | `apps/electron/src/renderer/src/test/usage.test.ts` and `test/usagePanel.test.tsx` (`USAGE-10:`) |
| USAGE-9 | `usage::tests::usage_9_a_cursor_figure_holds_nothing_whatever_it_reads`, `jobs::tests::usage_9_a_cursor_figure_gives_a_running_job_no_window_share`, `jobs::tests::usage_9_the_emptier_cursor_account_comes_first`, `jobs::tests::usage_6_8_9_a_cursor_accounts_figure_from_its_login_to_what_is_kept` |

## Test it when it's done (on a real Mac)

1. With the Mac's own `claude` not run for a day (its Keychain token ended: obid's log said `usage not read: its login ends within 10 minutes`), install this obid and open Oby. Within a minute, the Usage panel shows Claude Code's session bar and "This week N% · resets <day>", matching `claude` → `/usage`.
2. `~/Library/Logs/obid.log` has no `usage not read` line for the default account, and no `didn't refresh its login` line.
3. Claude Code's usage page (claude.ai › Settings › Usage) shows no new usage from the refresh.
4. With Cursor signed in, the picker's list, the Usage panel and Settings › Agents show Cursor's percentage, matching Cursor's own usage page, and when it resets. No password prompt comes with it. `~/Library/Logs/obid.log` holds no token.
