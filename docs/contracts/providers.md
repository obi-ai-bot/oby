# Providers: one row of facts per agent CLI

The business behavior contract for obid's provider table, from [the template](../business-contract-template.md). Roadmap task R08-01, milestone R08 One interface for every agent CLI.

## Feature and status

- **Outcome:** Each agent CLI's plain facts (its id, name, default binary, default model, efforts, fixed fallback models, API hosts and vendor domains) are one row in one table, so a fact that differs per tool is read from its row and can't be missed in one of three copies. Nothing a person or an app sees changes.
- **Status:** **Implemented** (`core/crates/obid/src/providers.rs`). PROVIDERS-1 to 3 are implemented.
- **Scope:** The table `crate::providers::PROVIDERS`, `Tool::row`, and the lookups that read it: `Tool::id`, `Tool::name`, `Tool::from_id`, `egress::tools_api`, `egress::tools_own`, `settings::default_model`, `settings::efforts`, `settings::models` and `accounts::default_bin`. Outside it: a tool's tested CLI versions, complex efforts, its home environment variable, how its effort is passed, whether its usage is advisory and a routing order number (each comes with the task that reads it), the per-tool behaviour in `model_lists.rs`, `brain/talk.rs`, `jobs.rs` and its modules, `usage.rs`, `api.rs` and `config.rs` (running a turn, reading usage, signing in, lanes, routing), `config.json`'s `agents` shape, and any new provider, endpoint, key or value.

## Behavior

### Actors and ownership

- **obid** owns the table and its lookups. No model call reads or decides anything here.
- **The apps** see no change: every id, key and value they read is the same.

### Rules

- **One row per tool.** `PROVIDERS` holds one `Descriptor` per tool, Claude Code, Codex, Cursor, each with `tool`, `id`, `name`, `bin`, `default_model`, `efforts`, `models`, `api_hosts` and `own_domains`.
- **Looked up by place.** `Tool::row` is the row at the tool's place in `Tool::ALL`; `Tool::from_id` is the tool of the row whose `id` matches, else none.
- **API hosts.** A host is one of a tool's API hosts only when it equals one in its row's `api_hosts` exactly: Claude Code's `api.anthropic.com`, `statsig.anthropic.com`, `console.anthropic.com`; Codex's `api.openai.com`, `auth.openai.com`; Cursor's `api2.cursor.sh`, `api3.cursor.sh`, `api4.cursor.sh`.
- **Vendor domains.** A host on port 443 is one of a tool's own services when it is one of its row's `own_domains` or a subdomain of one: Claude Code's `anthropic.com`, `claude.ai`, `claude.com`; Codex's `openai.com`, `chatgpt.com`; Cursor's `cursor.sh`, `cursor.com`.
- **Efforts.** Claude Code's are `low`, `medium`, `high`, `xhigh`, `max`; Codex's `low`, `medium`, `high`, `xhigh`; Cursor has none (AGENTS-5, AGENTS-8).

### Failures and recovery

- Does not apply: the table is compiled into obid and read with no failure path.

### Invariants

- Wire ids and JSON keys are unchanged: `claude`, `codex`, `cursor` and every key the API answers stay as they are (add-only API).
- The API host lists are exactly the ones above and match exactly: they decide which reaches leave a job unmarked (docs/contracts/merging.md, MERGING-30).
- Routing order is row order: `Tool::ALL` lists the tools in the table's order.
- A tool's tested CLI versions live in `core/crates/obid/cli-versions.json` (docs/contracts/cli-versions.md, CLIVERSIONS-7), not in the row.

## Interfaces and dependencies

- **API, events, persistence, clients:** none changes.

## Acceptance scenarios

- **PROVIDERS-1** (implemented). Given the provider table, when obid reads it, then every tool id that jobs, accounts and `config.json` use has exactly one row, and `Tool::ALL` lists them in the table's order, Claude Code, Codex, Cursor. Test: `providers::tests::providers_1_each_id_has_exactly_one_row` (core/crates/obid/src/providers.rs).
- **PROVIDERS-2** (implemented). Given a job on any tool, when its agent reaches a host, then the hosts that count as its tool's own API (`egress::tools_api`) are exactly the lists above, read from its row, and a host outside them (`claude.ai`, `chatgpt.com`, `cursor.com`, a lookalike) isn't one. Test: `egress::tests::providers_2_tool_api_hosts_are_unchanged` (core/crates/obid/src/egress.rs).
- **PROVIDERS-3** (implemented). Given `GET /v1/settings/models/options`, when it answers, then each tool's `efforts` and `complexEfforts`, and every listed model's `efforts`, are the lists above, read from the rows. Test: `providers_3_effort_choices_are_unchanged` (core/crates/obid/tests/runner_api.rs).
