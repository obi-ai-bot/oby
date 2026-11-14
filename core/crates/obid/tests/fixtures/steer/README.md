# Steering a turn: recorded transcripts

What Claude Code and Codex did when more words were written to them while a turn ran (task R05-01, measured on 2026-11-14). The answer is in `docs/conversation-harness.md`, section 8. No test loads these files yet: teaching the fake harnesses to replay them is R05-05's and R05-07's.

## Format

One JSON object per line: `{"ms": <milliseconds since the process started>, "dir": "in" | "out", "line": <the line as JSON>}`, in the order the lines were written or read. `"in"` is what the driver wrote on the CLI's stdin, `"out"` what the CLI wrote on its stdout. Stderr isn't recorded. Runs of streamed partial-message lines (Claude's `stream_event` `content_block_delta`, Codex's `item/agentMessage/delta`) are thinned to the first and last of each run; every other line is kept.

## How they were made

A driver script (not committed) ran each CLI outside obid, as obid drives it, in a fresh scratch git repository in a temporary folder, on the Mac's existing login, and answered every permission request with allow: Claude's `control_request` `can_use_tool` with a `control_response` allow carrying `updatedInput`, Codex's `requestApproval` with `{"decision":"accept"}`. None came: Claude Code allowed `sleep 20` by itself (`permission_decision` `source: "config"` in the tool result), and Codex ran it inside its `workspace-write` sandbox.

- The long turn's prompt: ``Run the shell command `sleep 20`. When it finishes, run `sleep 20` again as a separate command (not chained with the first). Then reply with the single word DONE.``
- The extra words: `Added while you work: when you are done, also say the word PINEAPPLE.` "PINEAPPLE" in the answer shows the words reached the model.

### Claude Code

`claude --version`: `2.1.317 (Claude Code)`. Run from the scratch repository with `CLAUDECODE`, `CLAUDE_CODE_*` and `ANTHROPIC_*` removed from the environment. The driver read on past the first `result` until 30 seconds passed with no line, then closed stdin and read until the process exited.

Left out of every Claude case, as obid-only or as the owner's own settings: `--append-system-prompt` (Oby's orientation), `--mcp-config` (Oby's tools) and `--restricted` (it removes Bash, which the long turn needs). `--setting-sources ""` and `--strict-mcp-config` (the boxed lane's) stand in for the own lane's `--setting-sources user`, so none of the owner's settings, hooks or MCP servers loaded. `--model haiku` keeps the probes small.

| File | Flags after `claude` | When the extra words were written |
|---|---|---|
| `claude-conversation-mid-turn.jsonl` | `-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --permission-prompt-tool stdio --permission-mode default --setting-sources "" --strict-mcp-config --exclude-dynamic-system-prompt-sections --tools Read,Grep,Glob,LS,NotebookRead,WebSearch,WebFetch,Write,Edit,MultiEdit,NotebookEdit,Task,TodoWrite,Bash,BashOutput,KillShell,Skill,ListMcpResourcesTool,ReadMcpResourceTool,ReadMcpResourceDirTool --max-turns 50 --model haiku` (a conversation's, `brain/harness.rs`, with the own lane's tool list) | 5 seconds after the first Bash `tool_use`: after its `task_started`, before its `tool_result` |
| `claude-conversation-replay.jsonl` | the same, plus `--replay-user-messages` | 5 seconds after the first Bash `tool_use`, before its `task_started` and `tool_result` |
| `claude-job-mid-turn.jsonl` | `-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --permission-prompt-tool stdio --permission-mode default --exclude-dynamic-system-prompt-sections --max-turns 200 --setting-sources "" --strict-mcp-config --model haiku` (a job's, `jobs.rs`) | 5 seconds after the first Bash `tool_use`, before its `task_started` and `tool_result` |
| `claude-conversation-no-tool-boundary.jsonl` | the conversation's, as in the first row | An extra case, not in the task's list: the prompt is `Without using any tools, write the whole numbers from 1 to 200, separated by spaces.`, and the words were written at its first `text_delta`, when no tool boundary was left in the turn |

### Codex

`codex --version`: `codex-cli 0.163.0`. `codex app-server`, with `CODEX_HOME` a temporary folder holding a copy of the Mac's `auth.json` with `tokens.refresh_token` set to `""` and an empty `config.toml` (so none of the owner's MCP servers, plugins or settings loaded; as obid's boxed Codex runs get their login). The JSON-RPC as `codex.rs` speaks it: `initialize`, the `initialized` notification, `thread/start` (or `thread/resume`) with `{"cwd": <the scratch repository>, "sandbox": "workspace-write", "approvalPolicy": "on-request", "approvalsReviewer": "user"}`, then `turn/start` with `{"threadId", "input": [{"type": "text", "text": <prompt>, "text_elements": []}], "effort": "low"}`. The model was the account's default.

| File | What it does | When the extra words were written |
|---|---|---|
| `codex-steer-mid-turn.jsonl` | `turn/steer` with the thread's id and the running turn's id as `expectedTurnId` | 5 seconds after the first `commandExecution` item started, while `sleep 20` ran |
| `codex-steer-wrong-turn.jsonl` | `turn/steer` with a made-up `expectedTurnId` (`turn-3`, a fresh UUID before scrubbing) | the same moment |
| `codex-steer-after-completed.jsonl` | `turn/steer` with the finished turn's id | right after `turn/completed` |
| `codex-resume-after-steer.jsonl` | A new `codex app-server` process on the same `CODEX_HOME`: `thread/resume` of `codex-steer-mid-turn.jsonl`'s thread (`thread-1`), then `turn/start` with `What words, if any, did I add while your previous turn was running? Quote them exactly.` | no extra words: it asks for the earlier ones |

### Cursor

None. `cursor-agent` isn't installed on the Mac that recorded these, so no Cursor case ran.

## Scrubbing

Every id was replaced with a placeholder, one mapping across all eight files, so equal ids stay equal and different ids stay different (`codex-resume-after-steer.jsonl`'s `thread-1` is `codex-steer-mid-turn.jsonl`'s). Ids inside other strings (an error message, a session file's path) were replaced too.

| Placeholder | Stands for |
|---|---|
| `session-N` | Claude's `session_id`, Codex's `sessionId` |
| `thread-N`, `turn-N`, `item-N` | Codex's thread, turn and item ids (`exec-…` and `msg_…` items included) |
| `message-N` | Claude's `msg_…` message ids |
| `tool-N` | Claude's `toolu_…` tool use ids |
| `request-N` | Claude's `request_id` (an API request's id) |
| `bash-N`, `run-N` | Claude's background task's `task_id` and `run_id` |
| `installation-N` | Codex's `installationId` |
| `uuid-N` | every other UUID (Claude's `uuid` fields) |
| `host-1` | the Mac's host name (Codex's `serverName`) |
| `/scratch/tmp/…` | the Mac's temporary folders (`$TMPDIR` and `/tmp`), where the scratch repositories and Codex's home were |
| `/scratch/user/…` | the home folder |

Rate-limit numbers, timestamps and the JSON-RPC request numbers are as recorded.
