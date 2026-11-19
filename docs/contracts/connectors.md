# Connectors: the owner's own, shown where Settings lists them

Every connector this page lists follows [the connector pattern](connector-pattern.md) (`CONNECTOR-n`).

The business behavior contract for Settings › Connectors on obid, from [the template](../business-contract-template.md). Asked for on 2026-11-17 ("The connectors seem not to be there").

## Feature and status

- **Outcome:** The owner opens Settings › Connectors and sees the connectors their agents already have: GitHub and Tetrix, which Oby holds, and the MCP servers and claude.ai connectors of their own Claude Code (Slack, Google Drive, a server of their own), which an own conversation loads as their terminal does (docs/contracts/lanes.md, LANES-12). Oby keeps no second copy of them: as T3 Code does, it shows what the CLI said.
- **Status:** **Implemented** (`core/crates/obid/src/connectors.rs`, `agent::Event::Connectors` in `core/crates/obid/src/agent.rs`, its handling in `brain::harness::one`; `apps/electron/src/renderer/src/Connectors.tsx`).
- **Scope:** what obid keeps of an own Claude Code session's connectors, `GET /v1/connectors`, and the page's rows for them. Outside it: adding, signing in to or removing one (they're the owner's, in Claude Code and on claude.ai; Oby's own registry is R11-06 and A00-92), Codex's and Cursor's own servers, and GitHub's and Tetrix's rows (unchanged).

## Behavior

### Actors and ownership

- **Claude Code** decides which connectors a session has, from the owner's settings and claude.ai account, and says each one's name and status in its `init` line.
- **obid** keeps that list, names and statuses only, and answers it. No model call decides anything here.
- **The Electron app** shows it on Settings › Connectors.

### Rules

- An own Claude Code conversation's session (LANES-12) names its servers in `init`'s `mcp_servers`. obid keeps each one's `name` and `status` as Claude Code wrote them, but its own two, `oby` and `tetrix`, in `home/connectors.json` (0600) with when it saw them, replacing the list it had. Never an address, a command, an argument or an environment: the `init` line has none, and obid adds none.
- A boxed session's list (Oby's box: another Oby's conversation, `--strict-mcp-config`) never replaces it: it holds obid's servers only.
- `GET /v1/connectors` answers `{"claude": {"seenAt": "<RFC 3339>", "servers": [{"name", "status"}]}}`, the servers in Claude Code's order, or `{"claude": null}` before any was seen. `status` is an open string (`connected`, `needs-auth`, `pending`, `failed`, `disabled` today).
- Settings › Connectors on obid lists them after GitHub and Tetrix, each under Connected when its status is `connected`, else under Available. A claude.ai connector shows without its `claude.ai ` prefix and with a line saying it's the owner's claude.ai connector; another server's line says it's their Claude Code's. One that needs a sign-in says so; a claude.ai one gets Connect, which opens claude.ai's connector settings in the browser. One that failed or is connecting says so, with no button.
- Before any was seen, or from an obid without the route (`404`), the page shows GitHub and Tetrix as before, and one line under them saying their Claude Code's connectors show after their first conversation.

### Acceptance scenarios

- **CONNECTORS-1 (`connectors_1_an_own_conversation_keeps_its_claude_codes_connectors`):** an own Claude Code conversation whose session names `oby`, `claude.ai Slack` (connected) and `rea` (failed) answers; `GET /v1/connectors` then lists `claude.ai Slack` and `rea` with their statuses and a `seenAt`, and not `oby`.
- **CONNECTORS-2 (`connectors_2_none_before_any_was_seen`):** a new obid answers `{"claude": null}`.
- **CONNECTORS-3 (`connectors_3_a_boxed_session_never_replaces_them`):** after an own conversation's list, another Oby's conversation (boxed) answers, and the list is unchanged.
- **CONNECTORS-4 (`connectors_4_settings_lists_the_owners_claude_code_connectors`):** with obid's list, Settings › Connectors shows Slack under Connected and Gmail (needs-auth) under Available with Connect, after GitHub and Tetrix.
- **CONNECTORS-5 (`connectors_5_none_seen_says_when_they_show`):** with `{"claude": null}`, or a `404`, the page shows GitHub and Tetrix and the line saying the connectors show after the first conversation.
