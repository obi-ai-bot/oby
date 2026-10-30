# Settings › Agents: the default agent, and each account's model and effort

The business behavior contract for Settings › Agents' **New tasks go to** and each agent card's **Model** menu (with its effort) and **Effort on complex tasks** (Sangam's design, [#1564](https://github.com/obi-ai-bot/oby/pull/1564): `apps/electron/src/renderer/src/SettingsView.tsx`, `Agents`), from [the template](../business-contract-template.md). Issues [#1573](https://github.com/obi-ai-bot/oby/issues/1573) and [#1574](https://github.com/obi-ai-bot/oby/issues/1574). Scenarios are numbered `AGENTS-n`; the tests that prove them are named after them (`agents_1_…`).

## Feature and status

- **Outcome:** The owner picks which of their agents (an account: "Work · Claude Code") new tasks go to, and which model and how much effort each account's tasks run with, once, in Settings; every task Oby starts afterwards follows it, on every device, and survives a restart.
- **Status:** **Implemented.** obid keeps both in `home/settings.json` (`defaultAccount`, `tuning`), says them in `GET /v1/status` and on each account, and uses them when it routes and starts a job. The Electron app shows and sets obid's once its status says `defaultAccount` (even `null`); before that (an older obid) and on the mock it keeps them on this Mac, as Sangam's v1 did (`agent.default`, `models.mine`).
- **Scope:** new jobs on this Mac's obid, from a conversation's plan or the jobs API. **Out of scope:** the brain's own model (Haiku, unchanged), a conversation's harness model (`config.json` `conversation`), each account's own live model list (the choices are one list per tool, below), the tool-wide `PUT /v1/settings/models` of the Swift runner.

## What Sangam's design assumes (each one kept)

| The design | What obid promises |
|---|---|
| "New tasks go to" lists the signed-in accounts and stores one account. | `defaultAccount` is an account id; obid routes to it while it can take work. |
| With nothing chosen, the first signed-in account shows as chosen. | Unset, routing's own order (Claude Code, Codex, Cursor; each tool's accounts in order), which starts at that same account while it has room. |
| Each account has its own model and effort ("Personal" and "Work" on Claude Code each their own). | `model`, `effort` and `complexEffort` are per account. |
| The menus start on what the tool runs (`status.models`) and offer the choices of `ModelOptions`. | `status.models` stays; `GET /v1/settings/models/options` answers the choices in that shape. |
| Cursor has a model and no effort. | An effort for a Cursor account is refused. |

## Behavior

### Actors and ownership

- **The owner** chooses in Settings › Agents, on the Mac (the iPhone has no such screen yet; it can read both from status and accounts).
- **obid** keeps the choices, validates them, routes new jobs and passes the model and effort to the tool. Every decision below is obid's.
- **The Electron app** shows obid's choices and sends the owner's; it moves a choice it kept itself to obid once (AGENTS-7).

### The account new tasks go to (#1573)

- **AGENTS-1:** With `defaultAccount` set and that account signed in with room, a job with no account named (`auto`) runs on it, whatever its tool, and a conversation's plan whose account is `auto` goes to it unless the owner's words name an agent (Claude, Codex or Cursor). A job that names another tool goes to that tool (to the default when it's that tool's); one that names an account goes to that account.
- **AGENTS-2:** Signed out, out of room (full, at 95% of a window, or in its weekly reserve), being signed in or removed, or gone: it's passed over and routing's own order decides, as if unset. For a plan, `defaultAgent` (#1568) is tried next, then the planner's own choice.
- **AGENTS-3:** `defaultAgent` (#1568, a tool) keeps working as before for whoever sets it; `defaultAccount`, when it can take the work, comes first. The brain's briefing marks the default account ("new tasks go here unless the user names another agent").
- **AGENTS-4:** `PUT /v1/settings/jobs` takes `"defaultAccount": "<id>"` (an account obid has, else `422`) or `null` (unset), alone or with the other keys; `GET /v1/settings/jobs` and `GET /v1/status` say it (`null` when unset). It's kept across restarts. Removing the account unsets it.

### Each account's model and effort (#1574)

- **AGENTS-5:** `PATCH /v1/accounts/{id}` takes `model`, `effort` and `complexEffort`, any of them, each a value or `null` (the tool's own). A model is letters, digits, `-`, `.` and `_`; an effort is one the tool takes (Claude Code `low` `medium` `high` `xhigh` `max`; Codex `low` `medium` `high` `xhigh`; Cursor none). Anything else is `422` and nothing changes. The Account JSON carries each one set, and leaves out the rest. Kept across restarts; removed with the account.
- **AGENTS-6:** A job made with no model of its own takes its account's; its effort is the account's `complexEffort` when the plan called the task complex (and the account has one), else its `effort`. Neither set, nothing is passed and the tool runs its own, as before. Claude Code gets `--model` and `--effort`; Codex the turn's `model` and `effort`; Cursor its model through its session's options. The job's `model` and `effort` show what it runs on. A job moved to another tool drops both.
- **AGENTS-8:** `GET /v1/settings/models/options` lists the choices per tool (`models` with each model's `efforts`, `efforts`, `complexEfforts`); `GET /v1/settings/models` answers the tools' own, as `status.models`.

### The Electron app

- **AGENTS-7:** Once obid's status says `defaultAccount`, a choice this Mac kept before (`agent.default`, `models.mine` in `localStorage`) is sent to obid once and then forgotten here, so nothing the owner chose is lost: a default only when obid has none yet (one set on another device wins); a choice for an account that's gone is dropped; one obid refused stays and is tried again at the next connection. Before that it keeps them here, as before.
- A choice obid refuses shows its words where Sangam's screen shows a problem (the card's, or under Default agent), and the menu stays on obid's value.

## Invariants

- obid's API stays add-only: `defaultAgent` and `PUT /v1/settings/default-agent` are unchanged; `defaultAccount`, the account's three keys and the two `GET`s are new keys and routes.
- A default never overrides an account or a tool the owner or the plan named, and never puts work on an account that's signed out or without room.
- Nothing here reaches an agent but the model and effort arguments.

## Tests

| Scenario | Test |
|---|---|
| AGENTS-1, AGENTS-2, AGENTS-6 | `jobs::tests::agents_1_2_6_new_tasks_go_to_the_default_account_on_its_model` |
| AGENTS-3 | `brain::actions::tests::explicit_agent_names_override_the_default`, `jobs::tests::a_job_waiting_for_room_follows_the_reserves` (#1568), `runner_api::default_agent_is_validated_and_kept`, `brain::briefing::tests::agents_3_the_briefing_marks_the_default_account` |
| AGENTS-4 | `runner_api::agents_4_the_default_account_is_validated_kept_and_cleared` |
| AGENTS-5, AGENTS-6 | `runner_api::agents_5_6_jobs_run_on_their_accounts_model_and_effort`, `adapters::agents_6_codex_runs_on_its_accounts_model_and_effort`, `settings::tests::agents_6_a_complex_task_takes_its_own_effort` |
| AGENTS-7 | `apps/electron/src/renderer/src/test/agents.test.ts` |
| AGENTS-8 | `runner_api::agents_8_the_choices_are_listed` |

## Test it when it's done (on a real Mac)

1. Settings › Agents with two Claude Code accounts: pick the second under New tasks go to; ask Oby for a small fix. The task runs on the second account.
2. Fill the second (or sign it out): the next task runs on the first.
3. Set the second account's model to Sonnet 5.5 at High, Effort on complex tasks Max; start a task: its page shows Sonnet; ask for something large: `ps` shows `--effort max`.
4. With a 0.4.x app's choices kept locally, update: the same choices show after the first connection, and quitting and reopening keeps them.
