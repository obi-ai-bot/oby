# Keychain: no password prompts from Oby

The business behavior contract for what obid asks of the macOS Keychain, from [the template](../business-contract-template.md). Issue [#1679](https://github.com/obi-ai-bot/oby/issues/1679). Scenarios are numbered `KEYCHAIN-n`; the tests that prove them are named after them (`keychain_1_…`).

## Feature and status

- **Outcome:** Starting or upgrading Oby never makes macOS ask the owner's Mac password, and obid answers while it starts. The owner's words of 2026-10-31: "WE NEED TO REMOVE THIS FOREVER, having to put the password 4 times is stupid."
- **Status:** **Implemented.** Until #1679, an obid starting in the Mac app's runner home with no `vault/secrets.json` copied the Swift runner's vault items (service `com.deskree.oby.runner.vault`) out of the Keychain. The Swift app made those items, so macOS asked the owner's password for each one, and obid answered no request until all were answered (4 prompts on the owner's Mac).
- **Scope:** obid on macOS: its vault, Tetrix's login, the agent logins it copies (`claudeKeychain`, `cursorKeychain`), Cursor's access token read for its usage, and Claude Code's runs staying on that login when `claude -p` changes (#1867). **Out of scope:** the Electron app's own device key (docs/contracts/signed-words.md) and the Swift apps.

## Behavior

### Actors and ownership

- **obid** keeps Oby's own secrets (the vault: Tetrix's login, the owner's connections) in `home/vault/secrets.json` only (0600), and reads Claude Code's and Cursor's own Keychain logins with `/usr/bin/security`.
- **Claude Code and cursor-agent** write their Keychain logins with `/usr/bin/security`, so macOS lets that program read them back without asking.
- **The apps** show what obid says: Tetrix's `error` in Settings, an account's `label`, a task's failure.

### Scenarios

- **KEYCHAIN-1:** obid never reads or writes the Keychain for its own secrets: no Security framework in obid, no Keychain store for the vault, no copy out of the Keychain. A configuration's `{"store": "keychain"}` is read as the file, and obid's log says to change it. Once obid answers, it lists the names of the Swift runner's items (`security dump-keychain`, attributes only: no value, no prompt, 10 seconds at most) and, for each one the file doesn't hold, says in its log which and how to add it back: Tetrix's by connecting Tetrix again, any other with `obid vault --config <path> add NAME` (`oby.fcm.serviceAccount`, or what `obid vault add` put there).
- **KEYCHAIN-2:** A Mac upgrading from a vault in the Keychain starts with no prompt. A saved Tetrix connection whose login isn't in the file is no longer connected, and says once, in Settings: "Oby's Tetrix login isn't in its vault any more: connect Tetrix again." When obid then finds `oby.tetrix.access` among the Swift runner's Keychain items, it says why: "Connect Tetrix again: Oby no longer reads its logins from your Keychain, so the one kept there didn't carry over." The old Keychain items stay, unread.
- **KEYCHAIN-3:** obid asks for a Claude Code or Cursor Keychain login's value at most once per change of the item: it reads the item's modification date first (no value, no prompt), keeps what macOS gave while that date stays the same (so an "Allow" rather than "Always Allow" isn't asked again at every run that starts on it), and reads again only once the tool rewrote the item (a refresh, a sign-in). A value macOS doesn't give (Deny, or no answer within 60 seconds) isn't asked for again until the item changes: that account, with no login file, isn't signed in; its label says "Sign in again, then Always Allow", and a run on it (a follow-up too) fails saying "Claude Code keeps its login in your Keychain, and macOS asked whether Oby may read it: that was denied or not answered, so Oby won't ask again until that login changes. To be asked again, run claude, then /login, and choose Always Allow when macOS asks." (Cursor: "run cursor-agent login"). obid looks at the item's date again at most every 30 seconds, so the sign-in clears it without a restart. A locked login Keychain, or no one at the screen (`security`'s exit 36), is never kept: the run says "Your login Keychain is locked, so Oby couldn't read the agent's login there: unlock your Mac (sign in at its screen), then try again.", and the next read asks again. An account with a login file (`.credentials.json`) uses it first and is never marked refused.

### No keeper: a boxed run refreshes its login before it starts (OBY-29)

obid kept a thread, the keeper, that looked every minute at each Keychain login a boxed run had used in the last hour, had the Mac's own Claude Code refresh it near its end and wrote every running copy again. It's gone: the own lane runs on the account's own login, which Claude Code refreshes itself (docs/contracts/lanes.md), and what is still boxed (another Oby's or an `untrusted` job, Oby's own review, the brain's planner, a boxed conversation, the usage read) refreshes right before it launches.

- **KEYCHAIN-10 (implemented):** Before a boxed Claude Code run starts on a Keychain login (`claudeKeychain`: the default login, or an account with a folder of its own), a token that has ended, ends within 5 minutes or doesn't say when has the Mac's own Claude Code refresh it first, as before: unboxed, by its absolute path, the system's `PATH` first, an empty config folder of obid's, its own `/usage` (no model call); one refresh at a time per item, at most one per 4 minutes. Then the run's copy is written, with only what calls the API and never a refresh token. A refresh that fails, or a token that has ended, stops the run saying the Mac's Claude login needs signing in again. Nothing writes a running copy again. A boxed job whose copy ends mid-run is refused its login (a 401 or 403): it gets one more try in its session (RECOVERY-1's path, after the same minute), whose launch refreshes the login and writes a new copy; refused again, it ends as a login does. A conversation's turn already starts a new Claude Code with a new copy when the waiting one's copy is near its end, and a failed turn's second launch is readied the same way. Oby's own review, the planner and the usage read are single runs that start with at least 5 minutes left (the usage read, 10). A run in the own lane is never refreshed by obid.

### Claude Code's bare mode (#1867)

Claude Code's docs say `--bare` will become the default for `claude -p`, and bare mode never reads OAuth or the Keychain. Every Oby run on Claude Code is `claude -p` on the owner's plan login. obid passes no flag that holds today's behaviour, because the docs name none (as of Claude Code 2.1.302).

- **KEYCHAIN-4:** obid passes no `--bare` to any launch of Claude Code, and no `CLAUDE_CODE_SIMPLE` (what `--bare` sets) or `ANTHROPIC_*` variable reaches one: a job's turn, a review another Oby asked for, a conversation's turn and the one started ahead for the next conversation, the brain's side calls, the usage read, the refresh of a login, a sign-in, `claude auth status`, and an answer to another Oby's question. Oby's own review of a job is a job's turn, on the same flags.
- **KEYCHAIN-5 (what's true today, the gap in #1867, not what it should end as):** On a bare default, a job on a plan login stops after one launch: `interrupted`, its `summary` and `reason` Claude Code's own "Not logged in · Please run /login", `nextStep` "Try again", no `resumesAt`, so nothing retries it. The account stays signed in. The owner is told to run a `/login` that wouldn't help, and nothing says Claude Code changed.
- **KEYCHAIN-6 (what's true today, the gap in #1867, not what it should end as):** On a bare default, a conversation's turn launches twice, as any session that fails does, and answers "I couldn't answer: its agent session failed twice." Claude Code's words reach only obid's log. The account stays signed in, and the owner isn't told why or what to do.
- **KEYCHAIN-7:** An early warning against the real Claude Code, run by hand after each Claude Code update on a Mac signed in with a plan: one small turn on haiku with a job's flags, which fails saying plainly that the plan login was refused (`cargo test -p obid --test claude_bare issue_1867_the_real_claude -- --ignored --nocapture`).

### Cursor's usage (the owner's decision of 2026-11-03)

Cursor's CLI reports no usage, so obid asks what Cursor's own app and CLI ask (docs/contracts/usage.md, USAGE-6 to 9). These endpoints are undocumented and can change; a failure just means no number.

- **KEYCHAIN-8:** What it reads: a Cursor account's login, as a run's copy reads it: its login file whole (a regular file: an account's own folder, or Cursor's own `~/.cursor/auth.json`), else, for Cursor's own login with `cursorKeychain` on, its `cursor-access-token` and `cursor-api-key` Keychain items, by the same read as a run's copy (KEYCHAIN-3), so it brings no new prompt and no value is asked for twice while its item is unchanged. An account with a folder of its own and no file in it never takes that login. It's all off the request path and off obid's async runtime, so a prompt nobody answers holds up nothing else. What it uses and sends is only the `accessToken`: never the API key, and never the refresh token (from the Keychain, `cursor-refresh-token` isn't asked for). An access token that has ended, or a login with none, sends nothing.
- **KEYCHAIN-9:** Where it sends it: only to `https://api2.cursor.sh`, that exact host, which no configuration can change, as `Authorization: Bearer` on `POST /aiserver.v1.DashboardService/GetCurrentPeriodUsage` and `POST /aiserver.v1.DashboardService/GetPlanInfo` (`Content-Type: application/json`, `Connect-Protocol-Version: 1`, body `{}`), with redirects off, no proxy and 10 seconds for each. The token is learned as a secret obid knows, so logs scrub it; it's never logged, stored or put in any answer. Only obid does this: never in a box, never for an agent. Of an answer only a number and a time are taken, and none of its text reaches an agent or the owner; a failure says only its status.

### What the owner can still see

- No prompt in the usual case: Claude Code and cursor-agent write their items with `/usr/bin/security`, and obid reads them with it.
- A prompt only when an item was made by another program (an older Claude Code, a tool that writes the Keychain itself) or the login Keychain is locked. Then macOS asks once per change of the item: Always Allow ends it for good; Allow asks again when the tool next rewrites its login (a refresh, about every 8 hours for Claude Code); Deny or no answer ends it until the login changes (KEYCHAIN-3). A value is read when a boxed run starts, and again after the refresh before it, off the request path; Cursor's is also read when an app asks `GET /v1/status`, at most once a minute, so that one answer can wait for the owner's choice, once, and by its usage read, in the background.
- While obid starts it asks only whether an item exists, which reads no secret and never prompts for an item, each answer within 5 seconds.

## Invariants

- No vault secret is ever read from or written to the Keychain.
- No Keychain value is asked for twice while its item is unchanged.
- obid's API is add-only: no key or route is removed; `tetrix.error` gains one text.

## Tests

| Scenario | Test |
|---|---|
| KEYCHAIN-1 | `vault::tests::keychain_1_the_vault_never_touches_the_keychain`, `vault::tests::where_the_vault_is` |
| KEYCHAIN-1 (the items left behind) | `jobs::logins::tests::the_swift_runners_vault_items_are_named_from_their_attributes` |
| KEYCHAIN-2 | `tetrix::tests::keychain_2_a_login_left_in_the_keychain_asks_to_connect_again` |
| KEYCHAIN-3 | `jobs::logins::tests::keychain_3_a_login_is_asked_for_once_per_change`, `jobs::logins::tests::keychain_3_two_reads_at_once_ask_once`, `accounts_api::keychain_3_a_cursor_login_macos_refused_says_why`, `accounts_api::keychain_3_a_claude_login_macos_refused_fails_the_run_saying_why` |
| KEYCHAIN-8 | `jobs::logins::tests::keychain_8_a_usage_read_takes_only_a_live_access_token`, `accounts_api::keychain_8_a_usage_read_asks_the_keychain_nothing_new`, `jobs::tests::keychain_8_a_slow_keychain_read_leaves_the_runtime_free` |
| KEYCHAIN-9 | `cursor_usage::tests::keychain_9_the_token_goes_to_cursors_host_as_cursors_own_app_sends_it`, `cursor_usage::tests::keychain_9_a_redirect_is_refused_and_the_token_stays`, `cursor_usage::tests::keychain_9_a_proxy_in_obids_environment_is_asked_nothing`, `cursor_usage::tests::keychain_9_a_host_that_doesnt_answer_times_out_saying_nothing_of_it`, `cursor_usage::tests::keychain_9_none_of_an_answers_text_comes_through`, `jobs::tests::usage_6_8_9_a_cursor_accounts_figure_from_its_login_to_what_is_kept` (the token in no log line or file) |
| KEYCHAIN-10 | `jobs::tests::keychain_10_a_boxed_runs_ended_copy_gets_one_more_try`, `jobs::logins::tests::the_token_before_a_refresh_stays_redacted`, `jobs::logins::tests::one_accounts_refresh_holds_up_no_other`, `jobs::logins::tests::keychain_3_a_login_is_asked_for_once_per_change` |
| KEYCHAIN-4 | `claude_bare::issue_1867_a_job_never_runs_bare`, `claude_bare::issue_1867_a_review_never_runs_bare`, `claude_bare::issue_1867_a_conversation_turn_never_runs_bare`, `claude_bare::issue_1867_a_conversation_started_ahead_never_runs_bare`, `claude_bare::issue_1867_a_side_call_of_the_brain_never_runs_bare`, `claude_bare::issue_1867_the_usage_read_never_runs_bare`, `claude_bare::issue_1867_the_refresh_never_runs_bare`, `claude_bare::issue_1867_a_sign_in_and_the_login_check_never_run_bare`, `claude_bare::issue_1867_an_answer_to_another_oby_never_runs_bare` |
| KEYCHAIN-5 | `claude_bare::issue_1867_a_bare_default_stops_a_job_in_claude_codes_words` |
| KEYCHAIN-6 | `claude_bare::issue_1867_a_bare_default_fails_a_turn_without_saying_why` |
| KEYCHAIN-7 | `claude_bare::issue_1867_the_real_claude_code_still_takes_a_plan_login` (ignored: it spends a real turn) |

## Test it when it's done (on a real Mac)

1. On a Mac with the Electron app at 0.1.0 and its vault items in the Keychain, move `~/Library/Application Support/Oby/Runner/vault/secrets.json` aside, install the signed release with this obid, and open Oby. No password prompt; Home shows within seconds.
2. Settings › Connectors: Tetrix asks to be connected again, with the text above. Connect it; jobs use it.
3. `~/Library/Logs/obid.log` names each item still in the Keychain that the file lacks (`… is still in the Keychain, where obid no longer reads its secrets …`), with how to add it back.
