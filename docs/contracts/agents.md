# Settings › Agents: the default agent, and each account's model and effort

The business behavior contract for Settings › Agents' **New tasks go to** and each agent card's **Model** menu (with its effort) and **Effort on complex tasks** (Sangam's design, [#1564](https://github.com/obi-ai-bot/oby/pull/1564): `apps/electron/src/renderer/src/SettingsView.tsx`, `Agents`), from [the template](../business-contract-template.md). Issues [#1573](https://github.com/obi-ai-bot/oby/issues/1573) and [#1574](https://github.com/obi-ai-bot/oby/issues/1574). Scenarios are numbered `AGENTS-n`; the tests that prove them are named after them (`agents_1_…`).

## Feature and status

- **Outcome:** The owner picks which of their agents (an account: "Work · Claude Code") new tasks go to, and which model and how much effort each account's tasks run with, once, in Settings; every task Oby starts afterwards follows it, on every device, and survives a restart.
- **Status:** **Implemented.** obid keeps both in `home/settings.json` (`defaultAccount`, `tuning`), says them in `GET /v1/status` and on each account, and uses them when it routes and starts a job. The Electron app shows and sets obid's once its status says `defaultAccount` (even `null`); before that (an older obid) and on the mock it keeps them on this Mac, as Sangam's v1 did (`agent.default`, `models.mine`).
- **Scope:** new jobs on this Mac's obid, from a conversation's plan or the jobs API. Conversation model selection follows [conversation-agent.md](conversation-agent.md), CONVERSATION-AGENT-19 to 21. **Out of scope:** the brain's quick calls (Haiku, unchanged), each account's own live model list (the choices are one list per tool, below), the tool-wide `PUT /v1/settings/models` of the Swift runner.

## What Sangam's design assumes (each one kept)

| The design | What obid promises |
|---|---|
| "New tasks go to" lists the signed-in accounts and stores one account. | `defaultAccount` is an account id; obid routes to it while it can take work. |
| With nothing chosen, the first signed-in account shows as chosen. | Unset, routing's own order (Claude Code, Codex, Cursor; each tool's accounts in order), which starts at that same account while it has room. |
| Each account has its own model and effort ("Personal" and "Work" on Claude Code each their own). | `model`, `effort` and `complexEffort` are per account. |
| The menus start on what the tool runs (`status.models`) and offer the choices of `ModelOptions`. | `status.models` stays; `GET /v1/settings/models/options` answers the choices in that shape. |
| Cursor has a model and no effort. | An effort for a Cursor account is refused; Cursor's effort and Fast are part of its model id (AGENTS-11). |

## Behavior

### Actors and ownership

- **The owner** chooses in Settings › Agents, on the Mac (the iPhone has no such screen yet; it can read both from status and accounts).
- **obid** keeps the choices, validates them, routes new jobs and passes the model and effort to the tool. Every decision below is obid's.
- **The Electron app** shows obid's choices and sends the owner's; it moves a choice it kept itself to obid once (AGENTS-7).

### The account new tasks go to (#1573)

- **AGENTS-1:** With `defaultAccount` set and that account signed in with room, a job with no account named (`auto`) runs on it, whatever its tool, and a conversation's plan whose account is `auto` goes to it unless the owner's words name an agent (Claude, Codex or Cursor). A job that names another tool goes to that tool (to the default when it's that tool's); one that names an account goes to that account.
- **AGENTS-2:** Signed out, out of room (full, at 95% of a window, or in its weekly reserve), being signed in or removed, or gone: it's passed over and routing's own order decides, as if unset. For a plan, `defaultAgent` (#1568) is tried next, then the planner's own choice.
- **AGENTS-3:** `defaultAgent` (#1568, a tool) keeps working as before for whoever sets it; `defaultAccount`, when it can take the work, comes first. The brain's briefing marks the default account ("new tasks go here unless the user names another agent") only while routing would put a task on it, so a plan never pins a full or signed-out account.
- **AGENTS-4:** `PUT /v1/settings/jobs` takes `"defaultAccount": "<id>"` (an account obid has, else `422`) or `null` (unset), alone or with the other keys; `GET /v1/settings/jobs` and `GET /v1/status` say it (`null` when unset). It's kept across restarts. Removing the account unsets it.

### Each account's model and effort (#1574)

- **AGENTS-5:** `PATCH /v1/accounts/{id}` takes `model`, `effort` and `complexEffort`, any of them, each a value or `null` (the tool's own). A model is one the tool offers (`GET /v1/settings/models/options`: never another tool's, and never an effort's name such as `high`, #1652); an effort is one the tool takes (Claude Code `low` `medium` `high` `xhigh` `max`; Codex `low` `medium` `high` `xhigh`; Cursor none). Anything else, or a refused `name` or `payer` in the same body, is `422` and nothing changes. The Account JSON carries each one set, and leaves out the rest. Kept across restarts; removed with the account.
- **AGENTS-6:** A job made with no model of its own takes its account's; its effort is the account's `complexEffort` when the plan called the task complex (a conversation's `oby_dispatch` and `oby_offer` take `complexity`; the job's `complexity` says `complex`), else its `effort`. With no account model, or one no longer offered, the job runs the tool default from `status.models` (CONVERSATION-AGENT-21); "no longer offered" is judged only against a list obid knows: Claude Code's fixed list, or a Codex or Cursor account's live list once obid has read it since it started. Until then (just after a restart) the saved model is taken as offered, and one that is really gone fails with the tool's own error instead of quietly running the default; an unset effort still passes nothing. Claude Code gets `--model` and `--effort`; Codex the turn's `model` and `effort`; Cursor its model through its session's options. The job's `model` and `effort` record overrides; a null model means the tool default. Whenever a job changes account (it starts on another after waiting for room, the owner moves it, the owner's account-limit choice moves it) they're the new account's again; a model the job named itself stays on the same tool. A follow-up keeps its job's complexity and the model it named, not its account's. Each account in status says `modelsRead` once obid knows its list, and the app says `Previous model unavailable · using default` only then: before, the saved model runs and the row shows it.
- **AGENTS-9:** The owner's own ceilings are settings (the owner's decision of 2026-10-31): `limits` on `GET` and `PUT /v1/settings/jobs`, `rounds` (10), `maxRunning` (20), `folders` (20), `routines` (20), `accounts` (32), each shown as its value now, each set to a whole number from 1 or back to its default with null; kept across restarts; a lower `maxRunning` ceiling lowers `maxRunning`. Caps that only protect the Mac (lengths, sizes, connections) stay fixed.
- **AGENTS-8:** `GET /v1/settings/models/options` lists the choices per tool (`models` with each model's `efforts`, `efforts`, `complexEfforts`, and Cursor's `groups`, AGENTS-11); `GET /v1/settings/models` answers the tools' own, as `status.models`.
- **AGENTS-11:** Cursor lists one id per model, effort and Fast (`grok-4.7-low`, `grok-4.7-low-fast`, …), and doesn't say which settings each model takes, so obid groups the ids it listed by model (`groups` on Cursor's options: each model's name, its default id and its variants, each one id with its effort and Fast; docs/runner-api.md). A bare id (`gpt-5.3-codex`) is the model at its own default effort; `thinking` is part of the model; `auto`, and a model with one id, stand alone; an id named like an effort (`gpt-5.1-codex-max` beside `gpt-5.1-codex-max-high`) stays its own model. Every listed id is in exactly one group, so every combination the owner can pick is one Cursor listed, and the composed id is what's kept (`model`, AGENTS-5) and passed to Cursor. A saved model keeps working unchanged: it's still an id.

### The Electron app

- **AGENTS-7:** Once obid's status says `defaultAccount`, a choice this Mac kept before (`agent.default`, `models.mine` in `localStorage`) is sent to obid once and then forgotten here, so nothing the owner chose is lost: a default only when obid has none yet (one set on another device wins); a choice for an account that's gone is dropped; one obid refused stays and is tried again at the next connection. Before that it keeps them here, as before.
- A choice obid refuses is reported as the app reports any failure (`live.ts`, `say`), and the menu stays on obid's value. Sangam's screen has no words of its own for it yet.
- An account with nothing chosen shows and runs the tool's model from `status.models` (CONVERSATION-AGENT-21). The menu has no separate "Default" entry.
- **AGENTS-11 (the app):** With Cursor's `groups`, a Cursor card's Model menu lists each model once, and its Effort (in the same menu, as Claude Code's and Codex's) the chosen model's efforts, its bare id as Default; a Fast switch shows only where the chosen effort has both. Another model keeps the effort and Fast where it has them, else starts on its default; another effort keeps Fast where that effort has it. A saved id Cursor no longer lists shows as itself, with no effort or Fast. An older obid with no `groups`: the flat list, as before. The message box's model menu still lists Cursor's ids flat.
- **AGENTS-10:** The Model menu stays inside the window, at its smallest size too: under its button where it fits, else on the side with more room, never taller than that room. A list longer than the room (Cursor's) scrolls under the Model label, the chosen model scrolled into view; Effort stays below the list and opens its submenu beside it, inside the window.

## Invariants

- obid's API stays add-only: `defaultAgent` and `PUT /v1/settings/default-agent` are unchanged; `defaultAccount`, the account's three keys and the two `GET`s are new keys and routes.
- A default never overrides an account or a tool the owner or the plan named, and never puts work on an account that's signed out or without room.
- Nothing here reaches an agent but the model and effort arguments.

## Tests

| Scenario | Test |
|---|---|
| AGENTS-1, AGENTS-2, AGENTS-3, AGENTS-6 | `jobs::tests::agents_1_2_6_new_tasks_go_to_the_default_account_on_its_model` (the default account before the default agent; a move; a job that waited for room) |
| AGENTS-6 | `jobs::tests::agents_6_a_restart_keeps_the_saved_model` (no live list read yet: the saved model stands), `conversation_agent::conversation_agent_22_account_lists_come_from_the_cli_and_reach_status` (`modelsRead`), Electron `talk.test.ts` (AGENTS-6: unavailable only once read) |
| AGENTS-3 | `brain::actions::tests::explicit_agent_names_override_the_default`, `jobs::tests::a_job_waiting_for_room_follows_the_reserves` (#1568), `runner_api::default_agent_is_validated_and_kept`, `brain::briefing::tests::agents_3_the_briefing_marks_the_default_account` |
| AGENTS-4 | `runner_api::agents_4_the_default_account_is_validated_kept_and_cleared` |
| AGENTS-5, AGENTS-6 | `runner_api::agents_5_6_jobs_run_on_their_accounts_model_and_effort`, `runner_api::agents_4_the_default_account_is_validated_kept_and_cleared` (removed with the account), `adapters::agents_6_codex_runs_on_its_accounts_model_and_effort`, `conversation_harness::agents_6_a_complex_task_from_a_conversation_runs_at_its_complex_effort`, `account_limit_flow::owner_moves_a_limit_stopped_job_to_a_connected_account_of_another_tool`, `settings::tests::agents_6_a_complex_task_takes_its_own_effort` |
| AGENTS-7 | `apps/electron/src/renderer/src/test/agents.test.ts` |
| AGENTS-8 | `runner_api::agents_8_the_choices_are_listed` |
| AGENTS-9 | `runner_api::agents_9_the_owners_limits_are_settings_with_defaults` |
| AGENTS-10 | `apps/electron/src/renderer/src/test/modelMenu.test.tsx` |
| AGENTS-11 | `model_lists::tests::agents_11_cursor_ids_group_by_model_with_their_efforts_and_fast`, `model_lists::tests::agents_11_every_id_cursor_lists_is_in_one_group` (cursor-agent 2026.10.01's own 276 ids), `runner_api::agents_8_the_choices_are_listed`, `apps/electron/src/renderer/src/test/cursorModel.test.tsx` |

## Test it when it's done (on a real Mac)

1. Settings › Agents with two Claude Code accounts: pick the second under New tasks go to; ask Oby for a small fix. The task runs on the second account.
2. Fill the second (or sign it out): the next task runs on the first.
3. Set the second account's model to Sonnet 5.5 at High, Effort on complex tasks Max; start a task: its page shows Sonnet; ask for something large: `ps` shows `--effort max`.
4. With a 0.4.x app's choices kept locally, update: the same choices show after the first connection, and quitting and reopening keeps them.
5. With a Cursor account, make the window its smallest and open its Model menu, near the window's bottom and then scrolled near its top: the whole menu shows, Model label included, its list scrolls to every model, the chosen one in view. On a Codex account, Effort below the list still opens its efforts beside it.
6. With a Cursor account, open its Model menu: each model once (Grok 4.7, not eight Grok 4.7 rows). Pick Grok 4.7, Effort High, turn Fast on; start a task on that account: it runs on `grok-4.7-high-fast` (obid sets it as the ACP session's `model` option, docs/cursor.md) and answers as Grok. Pick a model with no Fast (Gemini 3.1 Pro): the Fast row goes.
