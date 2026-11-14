# Providers: one row of facts per agent CLI

The business behavior contract for obid's provider table, and for how the Electron app shows an agent it doesn't know, from [the template](../business-contract-template.md). Roadmap tasks R08-01 and R08-02, milestone R08 One interface for every agent CLI.

## Feature and status

- **Outcome:** Each agent CLI's plain facts (its id, name, default binary, default model, efforts, fixed fallback models, API hosts and vendor domains) are one row in one table, so a fact that differs per tool is read from its row and can't be missed in one of three copies. Nothing a person or an app sees changes.
- **Status:** **Implemented** (`core/crates/obid/src/providers.rs`). PROVIDERS-1 to 3 are implemented. PROVIDERS-5 and 6 are implemented in the Electron app (R08-02).
- **Scope:** The table `crate::providers::PROVIDERS`, `Tool::row`, and the lookups that read it: `Tool::id`, `Tool::name`, `Tool::from_id`, `egress::tools_api`, `egress::tools_own`, `settings::default_model`, `settings::efforts`, `settings::models` and `accounts::default_bin`. Outside it: a tool's tested CLI versions, complex efforts, its home environment variable, how its effort is passed, whether its usage is advisory and a routing order number (each comes with the task that reads it), the per-tool behaviour in `model_lists.rs`, `brain/talk.rs`, `jobs.rs` and its modules, `usage.rs`, `api.rs` and `config.rs` (running a turn, reading usage, signing in, lanes, routing), `config.json`'s `agents` shape, and any new provider, endpoint, key or value.
- **Scope, the Electron app (PROVIDERS-5 and 6):** how it reads an agent id it doesn't know in Settings (Agents, and Accounts), the message box's account picker and the agent marks (`agentOf` in `mock.ts`, `lettered` and `BrandMark` in `brands.tsx`, `talk.ts`, `TalkPill.tsx`, `SettingsView.tsx`). Outside it: the iPhone and Android apps, frozen ([the owner's answers of 2026-11-13](../decisions/2026-11-13-roadmap-answers.md), "Phone apps: frozen, not deleted"); a real name, mark or model list for any new provider; offering an unknown tool in Add an account or first run; removing such an account, which stays as WIRE-4 left it (docs/contracts/wire.md).

## Behavior

### Actors and ownership

- **obid** owns the table and its lookups. No model call reads or decides anything here.
- **The apps** see no change from the table: every id, key and value they read is the same.
- **The Electron app** shows an unknown agent's account and jobs with a grey lettered mark and the id as its name, lists it disabled in pickers, and sends nothing for it.

### Rules

- **One row per tool.** `PROVIDERS` holds one `Descriptor` per tool, Claude Code, Codex, Cursor, each with `tool`, `id`, `name`, `bin`, `default_model`, `efforts`, `models`, `api_hosts` and `own_domains`.
- **Looked up by place.** `Tool::row` is the row at the tool's place in `Tool::ALL`; `Tool::from_id` is the tool of the row whose `id` matches, else none.
- **API hosts.** A host is one of a tool's API hosts only when it equals one in its row's `api_hosts` exactly: Claude Code's `api.anthropic.com`, `statsig.anthropic.com`, `console.anthropic.com`; Codex's `api.openai.com`, `auth.openai.com`; Cursor's `api2.cursor.sh`, `api3.cursor.sh`, `api4.cursor.sh`.
- **Vendor domains.** A host on port 443 is one of a tool's own services when it is one of its row's `own_domains` or a subdomain of one: Claude Code's `anthropic.com`, `claude.ai`, `claude.com`; Codex's `openai.com`, `chatgpt.com`; Cursor's `cursor.sh`, `cursor.com`.
- **Efforts.** Claude Code's are `low`, `medium`, `high`, `xhigh`, `max`; Codex's `low`, `medium`, `high`, `xhigh`; Cursor has none (AGENTS-5, AGENTS-8).

### An agent the Electron app doesn't know

- **Its name and mark.** `agentOf` gives an id that isn't `claude`, `codex` or `cursor` that id as its name, and a plain grey circle (`#6e6e74`) with the id's first character, upper-cased, in white on it (the owner's answer [q022](../decisions/2026-11-13-roadmap-answers.md#r08-02-q022)); an empty id gets the circle alone. The mark keeps its grey and its letter where marks are drawn in the text's colour. Every place that shows an agent from obid reads it there.
- **Where it shows.** Its accounts come after the tools the app knows, in obid's order, one group per id: in the message box's account picker (signed in only), on Settings › Agents (a card each, with its mark, its id as the tool, `who`, and its usage or what obid says), and on Settings › Accounts (a group titled with the id).
- **Listed disabled.** Its row in the message box's picker is a disabled choice with its usage and no model: it opens no model menu and chooses nothing. "New tasks go to" and "New conversations talk through" list its signed-in accounts as disabled options. Its Settings card has no Model, Effort, sign-in or background control, and its Accounts row no Sign In or Cancel; Remove stays on both.
- **Nothing sent.** The app sends no request for it from these controls: no model or effort change, no choice of who a conversation talks through or new tasks go to, and no sign-in. A saved default or talk-through account that is one of its accounts shows as the select's (disabled) value.

### Failures and recovery

- Does not apply: the table is compiled into obid and read with no failure path.

### Invariants

- Wire ids and JSON keys are unchanged: `claude`, `codex`, `cursor` and every key the API answers stay as they are (add-only API).
- The API host lists are exactly the ones above and match exactly: they decide which reaches leave a job unmarked (docs/contracts/merging.md, MERGING-30).
- Routing order is row order: `Tool::ALL` lists the tools in the table's order.
- A tool's tested CLI versions live in `core/crates/obid/cli-versions.json` (docs/contracts/cli-versions.md, CLIVERSIONS-7), not in the row.
- No stored choice (`agent.default`, `talk.default`, `models.mine`, or obid's `defaultAccount`, `talkAccount` and an account's model) is dropped or reset because its tool is unknown to the app.

## Interfaces and dependencies

- **API, events, persistence, clients:** none changes.

## Acceptance scenarios

- **PROVIDERS-1** (implemented). Given the provider table, when obid reads it, then every tool id that jobs, accounts and `config.json` use has exactly one row, and `Tool::ALL` lists them in the table's order, Claude Code, Codex, Cursor. Test: `providers::tests::providers_1_each_id_has_exactly_one_row` (core/crates/obid/src/providers.rs).
- **PROVIDERS-2** (implemented). Given a job on any tool, when its agent reaches a host, then the hosts that count as its tool's own API (`egress::tools_api`) are exactly the lists above, read from its row, and a host outside them (`claude.ai`, `chatgpt.com`, `cursor.com`, a lookalike) isn't one. Test: `egress::tests::providers_2_tool_api_hosts_are_unchanged` (core/crates/obid/src/egress.rs).
- **PROVIDERS-3** (implemented). Given `GET /v1/settings/models/options`, when it answers, then each tool's `efforts` and `complexEfforts`, and every listed model's `efforts`, are the lists above, read from the rows. Test: `providers_3_effort_choices_are_unchanged` (core/crates/obid/tests/runner_api.rs).
- **PROVIDERS-5** (implemented). Given obid lists an account and a job whose agent is `gemini`, when the Electron app shows them, then its job's card and row, the message box's picker and Settings › Agents show the name `gemini` with a grey circle lettered `G`, never `Claude Code`, and the account's card under its own name. Test: `providers_5_unknown_agent_shows_its_id_and_a_lettered_mark` (apps/electron/src/renderer/src/test/unknownAgent.test.tsx).
- **PROVIDERS-6** (implemented). Given an account whose agent the Electron app doesn't know, when the owner opens the message box's picker or Settings › Agents, then it is listed disabled (a disabled picker row with no model menu, and disabled options in "New tasks go to" and "New conversations talk through"), its card has no Model row and no Sign In, and the app sends no request for it; a saved default, talk-through account or model of it is shown and left as it was. Test: `providers_6_unknown_agent_is_listed_disabled_and_nothing_is_sent` (apps/electron/src/renderer/src/test/unknownAgent.test.tsx).
