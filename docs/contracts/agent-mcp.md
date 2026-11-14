# Agent MCP: the one protocol shape of obid's MCP servers for agents, and their tools' gates

The business behavior contract for the MCP protocol plumbing that obid's servers for agents share, and for the table of the tools they serve with each tool's gates, from [the template](../business-contract-template.md). Roadmap tasks R11-02 and R11-03, milestone R11 Connections: MCP, sign-in and the vault.

## Feature and status

- **Outcome:** Every MCP server obid answers agents on speaks the protocol the same way: the versions it accepts, its `initialize` answer, `ping`, notifications, the error for a method it doesn't serve, and the scrub that takes every secret obid knows out of a result. It is written once, so the servers can't drift apart and a vault secret can't reach an agent through one of them. Every tool an agent can call on them is one entry of one table, its gates written out on it, so a reader sees who may call what and a new tool can't skip a gate by leaving it out.
- **Status:** **Implemented** (`core/crates/obid/src/mcp.rs`, used by `Connections::handle` in `core/crates/obid/src/connections.rs`, `brain::tools::{handshake, handle}` in `core/crates/obid/src/brain/tools.rs`, and `Tetrix::handle` and `Tetrix::converse` in `core/crates/obid/src/tetrix.rs`). The tool table is `mcp::every`, built from `brain::tools::table`, `connections::table` and `tetrix::table`. AGENTMCP-1 to 7 are implemented.
- **Scope:** Three servers, at four handlers:
  - **The job's broker:** `POST /v1/agent/mcp/{job}`, with the job's broker token.
  - **The conversation's tools:** `POST /v1/agent/mcp/conversation-<id>`, with the turn's token, or the waiting session's between turns.
  - **Tetrix's server:** `POST /v1/agent/tetrix/{job}` with the job's broker token, and `POST /v1/agent/tetrix/conversation-<id>` with the turn's token.

  Inside it too: each server's tools and the gates obid holds each one to ("Tools and their gates").

  Outside it: what a tool's handler does once its gates passed (the contracts of its own area: conversations, jobs, merging, routines), the routes, tokens and HTTP statuses, and the MCP calls obid itself makes as a client (to a connected server, and to Tetrix), which only read the version list from here.

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

### Tools and their gates

Every tool an agent can call on the three servers is one entry (`mcp::Tool`): its name, its server and four gates, each written out on every entry (no default, no entry built from another). A conversation's and the broker's entries also carry the tool's description, input schema and handler; Tetrix's tools are described by Tetrix's own listing and forwarded.

- **`confirm_unless_owners_word`:** on a turn that isn't the owner's own word (their words this turn signed by an app of theirs, nothing untrusted read, someone watching), the turn asks for the owner's OK at most 10 times; past that the call answers `Oby asks for the owner's OK at most 10 times in one turn: …`. The Confirm card itself is the handler's, with its own title and text.
- **`owner_only`:** the tool is given only to the owner's own work. The broker lists and answers `oby_ask_owner` only for a job whose agent asks the owner through Oby (the owner's own job, not a review, a Codex or Cursor job); Tetrix's tools are given only to the owner's own jobs (`Jobs::tetrix_for`, and the route refuses another Oby's job).
- **`marks_on_result`:** what the tool's answer marks where it's read: nothing; always, with a reason; as `<name>'s answer`; or by the job named in the call's `id` (`the title of job <id>, which <why>` when that job isn't the owner's words throughout, `<name> on a job obid doesn't have` when obid has no such job, else nothing). A conversation is marked (`foreign` in its file); a job's MCP reads are judged by `secretspec::tool_is_known_safe`, which agrees with the table: every broker and Tetrix entry marks exactly when that function doesn't call it safe.
- **`read_only`:** the tool only reads: a conversation's turn that calls any other has acted (another agent given the same words would call it again); the broker lists `oby_ask_owner` with `"annotations": {"readOnlyHint": true}`; and Tetrix's use record says `access` `write` for the one that doesn't only read (`save_session`), else `read`.

| Tool | Server | confirm_unless_owners_word | owner_only | marks_on_result | read_only |
|---|---|---|---|---|---|
| `oby_jobs` | conversation | no | no | nothing (the handler marks by the jobs it lists) | yes |
| `oby_job` | conversation | no | no | "a job's report, pull request and checks" | yes |
| `oby_dispatch` | conversation | no | no | nothing | no |
| `oby_offer` | conversation | no | no | nothing | no |
| `oby_tell_job` | conversation | no | no | by the job named | no |
| `oby_stop_job` | conversation | no | no | by the job named | no |
| `oby_continue_job` | conversation | yes | no | by the job named | no |
| `oby_archive_job` | conversation | no | no | by the job named | no |
| `oby_delete_job` | conversation | yes | no | by the job named | no |
| `oby_change_job` | conversation | yes | no | by the job named | no |
| `oby_tasks` | conversation | no | no | nothing | yes |
| `oby_inbox` | conversation | no | no | "oby_inbox's answer" | yes |
| `oby_github` | conversation | no | no | "oby_github's answer" | yes |
| `oby_answer` | conversation | yes | no | "oby_answer's answer" | no |
| `oby_merge` | conversation | yes | no | nothing | no |
| `oby_github_write` | conversation | yes | no | nothing | no |
| `oby_settings` | conversation | yes | no | nothing | no |
| `oby_routines` | conversation | no | no | "oby_routines's answer" | yes |
| `oby_routine` | conversation | yes | no | nothing | no |
| `oby_change_routine` | conversation | yes | no | "oby_change_routine's answer" | no |
| `oby_check_in` | conversation | yes | no | nothing | no |
| `oby_room_read` | conversation | no | no | "oby_room_read's answer" | yes |
| `oby_room_post` | conversation | yes | no | nothing | no |
| `oby_conversations` | conversation | no | no | "oby_conversations's answer" | yes |
| `oby_delete_conversations` | conversation | yes | no | "oby_delete_conversations's answer" | no |
| `oby_members` | conversation | no | no | "oby_members's answer" | yes |
| `oby_ask` | conversation | yes | no | nothing | no |
| `oby_asks` | conversation | no | no | nothing (what came back is marked where it's read) | yes |
| `oby_artifact` | conversation | no | no | nothing | no |
| `oby_read` | conversation | no | no | nothing (the handler marks a read of the repositories) | yes |
| `oby_search` | conversation | no | no | nothing (the same) | yes |
| `oby_list` | conversation | no | no | nothing (the same) | yes |
| `oby_ask_owner` | broker | no | yes | nothing | yes |
| `graph_summary` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `search_entities` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `get_entity` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `operational_context` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `impact_radius` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `service_topology` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `code_impact` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `query_graph` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `session_digest` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `session_context` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `repo_sessions` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `list_sessions` | Tetrix | no | yes | "Tetrix's answers" | yes |
| `save_session` | Tetrix | no | yes | "Tetrix's answers" | no |

Tetrix's entries are built from `tetrix::TOOLS`, which stays the allowlist: a tool Tetrix lists past it is never listed or forwarded.

**The conversation's dispatcher.** A `tools/call` to the conversation's tools runs these steps in this order, after the call is heard (`Session::heard`):

1. A stopped turn refuses: `The turn was stopped: nothing more happens for it.`
2. A tool that isn't `read_only` marks the turn as having acted.
3. `marks_on_result` marks the conversation with its reason.
4. `confirm_unless_owners_word`, on a turn that isn't the owner's own word, counts one ask for their OK and refuses past 10.
5. The entry's handler runs.

A name with no entry fails closed: it acts, it marks the conversation with `<name>'s answer`, it isn't capped, and it answers the error `Oby has no tool <name>.`

**The broker and Tetrix** read their entries where they check them: the broker lists and answers a tool only when its `owner_only` gate passes for the job, and answers anything else `There's no tool named <name>: connected apps went with the strip to a dispatcher.`; Tetrix's server lists, forwards and records only its entries, refusing any other with `Oby doesn't let agents call Tetrix's <name>.`, and on a conversation's turn it marks the conversation, with the entry's reason, only once a call passed that refusal and the secret scan.

**Gates that depend on the call stay in the handlers.** What a call's arguments decide is the handler's, with its own conditions and texts: the per-job marks of `oby_jobs`; the turn's untrusted read (`Session::taint`) of `oby_job`, `oby_conversations`, `oby_inbox`, `oby_github` and `oby_tasks`; the repositories' mark of `oby_read`, `oby_search` and `oby_list`; `oby_settings`' check of a mode that asks less; the refusal of another Oby's job; and every Confirm card.

### Failures and recovery

- **A body that isn't JSON**, or holds no method or no id, gets no answer (HTTP 202), as a notification does.
- **A method name longer than 64 characters** is cut in the error: the agent gets the error, never a longer echo.

### Invariants

- A vault secret never reaches an agent in a result of these servers, as a value or as a key (AGENTS.md, "Non-negotiable").
- The version list, the `initialize` answer, `ping`, the notification rule, the unknown-method error and the scrub are written once, in `core/crates/obid/src/mcp.rs`.
- Adding a tool is adding an entry that names every gate; the table is the only list of each server's tools and gates.
- A tool's result is scrubbed once, in `mcp::ok`, whichever server answers it.
- Tetrix's allowlist is `tetrix::TOOLS`; no tool past it reaches Tetrix or an agent's list.

## Interfaces and dependencies

- **API:** the three servers' routes, unchanged ([docs/api/](../api/)). The texts of the `-32601` errors are for agents and quoted by no client.
- **Persistence:** none.
- **Clients:** none change.

## Acceptance scenarios

- **AGENTMCP-1** (implemented). Given any of obid's agent MCP servers (a conversation's tools, a job's broker, Tetrix's for a job and for a conversation), when an agent sends `initialize` with a `protocolVersion` obid knows (`2025-03-26`), then it answers that version; with one it doesn't know (`1999-01-01`), then it answers obid's newest (`2025-06-18`). Test: `agentmcp_1_every_server_answers_the_same_versions` (core/crates/obid/tests/tetrix_api.rs); one list only: `mcp::tests::one_protocol_version_list`.
- **AGENTMCP-2** (implemented). Given any of the three servers, when an agent sends a method obid doesn't serve, then it answers JSON-RPC error `-32601` with `<server> doesn't answer <the method's first 64 characters>.`, the same shape on all three. Test: `agentmcp_2_an_unknown_method_is_the_same_error_everywhere` (core/crates/obid/tests/tetrix_api.rs).
- **AGENTMCP-3** (implemented). Given a vault secret in a result, when any of the three servers answers, then the secret is replaced by its `[redacted: vault <name>]` marker in strings and keys. Test: `mcp::tests::agentmcp_3_results_are_scrubbed_once` (core/crates/obid/src/mcp.rs).
- **AGENTMCP-4** (implemented). Given a turn that isn't the owner's signed word, when its agent calls a tool whose entry says `confirm_unless_owners_word` (`oby_delete_conversations` on that conversation's id), then the tool doesn't act: its handler leaves a `confirm` inbox item (`Delete the conversation …`) and the conversation stays; the 11th such call in the turn answers `Oby asks for the owner's OK at most 10 times …`, and a tool without the gate (`oby_routines`) called 11 times is never capped. Test: `agentmcp_4_a_confirm_tool_waits_without_the_owners_word` (core/crates/obid/tests/conversation_harness.rs).
- **AGENTMCP-5** (implemented). Given the owner's signed turn in a new conversation, when its agent calls a tool whose entry marks its answer (`oby_routines`), then the conversation is marked with its reason (`oby_routines's answer`); a tool whose entry marks nothing (`oby_tasks`) leaves it clean; a name with no entry (`oby_no_such_tool`) marks it as `oby_no_such_tool's answer` and answers `Oby has no tool oby_no_such_tool.` A job's MCP reads are marked where they're read, by `secretspec::tool_is_known_safe`, which AGENTMCP-7 holds the broker's and Tetrix's entries to. Test: `agentmcp_5_only_marking_tools_mark` (core/crates/obid/tests/conversation_harness.rs).
- **AGENTMCP-6** (implemented). Given a job of the owner's whose agent is given Tetrix's tools and lists them but calls none, when it ends, then the job carries no mark (MERGING-30 kept by the table), and every tool it was listed is one of Tetrix's entries. Test: `agentmcp_6_given_tetrix_tools_no_call_no_mark` (core/crates/obid/tests/tetrix_api.rs).
- **AGENTMCP-7** (implemented). Given the table, when a test lists every entry, then the conversation's are its 32 tools in `tools/list`'s order, the broker's is `oby_ask_owner` and Tetrix's are `tetrix::TOOLS` in order, each server's names are unique, every broker and Tetrix entry marks exactly when `secretspec::tool_is_known_safe` doesn't call it safe, and the source has no `Default` for the entry or its gates and no entry built with `..`. Test: `mcp::tests::agentmcp_7_every_entry_names_every_gate` (core/crates/obid/src/mcp.rs).
