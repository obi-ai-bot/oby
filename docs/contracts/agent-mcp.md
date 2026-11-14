# Agent MCP: the one protocol shape of obid's MCP servers for agents

The business behavior contract for the MCP protocol plumbing that obid's servers for agents share, from [the template](../business-contract-template.md). Roadmap task R11-02, milestone R11 Connections: MCP, sign-in and the vault.

## Feature and status

- **Outcome:** Every MCP server obid answers agents on speaks the protocol the same way: the versions it accepts, its `initialize` answer, `ping`, notifications, the error for a method it doesn't serve, and the scrub that takes every secret obid knows out of a result. It is written once, so the servers can't drift apart and a vault secret can't reach an agent through one of them.
- **Status:** **Implemented** (`core/crates/obid/src/mcp.rs`, used by `Connections::handle` in `core/crates/obid/src/connections.rs`, `brain::tools::{handshake, handle}` in `core/crates/obid/src/brain/tools.rs`, and `Tetrix::handle` and `Tetrix::converse` in `core/crates/obid/src/tetrix.rs`). AGENTMCP-1 to 3 are implemented.
- **Scope:** Three servers, at four handlers:
  - **The job's broker:** `POST /v1/agent/mcp/{job}`, with the job's broker token.
  - **The conversation's tools:** `POST /v1/agent/mcp/conversation-<id>`, with the turn's token, or the waiting session's between turns.
  - **Tetrix's server:** `POST /v1/agent/tetrix/{job}` with the job's broker token, and `POST /v1/agent/tetrix/conversation-<id>` with the turn's token.

  Outside it: each server's tool lists, tool gates and what a tool call does (R11-03), its routes, tokens and HTTP statuses, and the MCP calls obid itself makes as a client (to a connected server, and to Tetrix), which only read the version list from here.

## Behavior

### Actors and ownership

- **obid** owns the protocol shape of every MCP server it answers agents on. No model call decides anything here.
- **Agents** (Claude Code, Codex, Cursor, the conversation's session) send JSON-RPC 2.0 messages to these servers.
- **The apps** show and send nothing new.

### Rules

- **Versions.** obid speaks MCP `2025-06-18`, `2025-03-26` and `2024-11-05`, newest first, in one list (`mcp::VERSIONS`). No other file holds a second list.
- **A message** is read for its `method`, its `id` and its `params` (`{}` when there are none). A message with no `method`, or with no `id` or a null one (a notification), gets no answer: the route answers HTTP 202 with no body.
- **`initialize`** answers `protocolVersion`, `capabilities` `{"tools": {"listChanged": false}}`, `serverInfo` `{"name", "version"}` and `instructions`. `protocolVersion` is the one the agent asked for when obid speaks it, else obid's newest. `version` is obid's API version. Each server keeps its own name and instructions: `oby` for the broker and for the conversation's tools, `tetrix` for Tetrix's server.
- **`ping`** answers `{}`.
- **A method the server doesn't serve** is the JSON-RPC error `-32601`, the same shape on every server: `{"jsonrpc": "2.0", "id": <id>, "error": {"code": -32601, "message": "<server> doesn't answer <method>."}}`, with the method cut to its first 64 characters. `<server>` is `Oby's broker`, `Oby's tools server` or `Oby's Tetrix server`.
- **No turn.** On the two conversation endpoints, a `tools/call` while no turn of the conversation is under way (its Claude Code waiting between turns, with the waiting session's token) is the JSON-RPC error `-32600` `No turn of Oby's is under way: tools are called only in one.` Any other method the server doesn't serve gets `-32601` as above.
- **The scrub.** Every result any of the three servers answers goes through one scrub: every string and every object key, at any depth, has every secret obid knows replaced by its marker. A vault secret's marker is `[redacted: vault <name>]`.

### Failures and recovery

- **A body that isn't JSON**, or holds no method or no id, gets no answer (HTTP 202), as a notification does.
- **A method name longer than 64 characters** is cut in the error: the agent gets the error, never a longer echo.

### Invariants

- A vault secret never reaches an agent in a result of these servers, as a value or as a key (AGENTS.md, "Non-negotiable").
- The version list, the `initialize` answer, `ping`, the notification rule, the unknown-method error and the scrub are written once, in `core/crates/obid/src/mcp.rs`.
- What each server offers and refuses is its own: this contract changes no tool list, tool gate, token, route or HTTP status.

## Interfaces and dependencies

- **API:** the three servers' routes, unchanged (`docs/runner-api.md`). The texts of the `-32601` errors are for agents and quoted by no client.
- **Persistence:** none.
- **Clients:** none change.

## Acceptance scenarios

- **AGENTMCP-1** (implemented). Given any of obid's agent MCP servers (a conversation's tools, a job's broker, Tetrix's for a job and for a conversation), when an agent sends `initialize` with a `protocolVersion` obid knows (`2025-03-26`), then it answers that version; with one it doesn't know (`1999-01-01`), then it answers obid's newest (`2025-06-18`). Test: `agentmcp_1_every_server_answers_the_same_versions` (core/crates/obid/tests/tetrix_api.rs); one list only: `mcp::tests::one_protocol_version_list`.
- **AGENTMCP-2** (implemented). Given any of the three servers, when an agent sends a method obid doesn't serve, then it answers JSON-RPC error `-32601` with `<server> doesn't answer <the method's first 64 characters>.`, the same shape on all three. Test: `agentmcp_2_an_unknown_method_is_the_same_error_everywhere` (core/crates/obid/tests/tetrix_api.rs).
- **AGENTMCP-3** (implemented). Given a vault secret in a result, when any of the three servers answers, then the secret is replaced by its `[redacted: vault <name>]` marker in strings and keys. Test: `mcp::tests::agentmcp_3_results_are_scrubbed_once` (core/crates/obid/src/mcp.rs).
