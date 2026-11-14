# CLI canary: what a fake model on a GitHub runner can check

This is the answer to roadmap task R01-10, for R01-11 (the scheduled canary of the newest agent CLIs). Runs with real logins on the owner's Mac are #1099.

The question: can the newest Claude Code, Codex and cursor-agent each run through obid's adapter cases against a fake model on 127.0.0.1, with no login, on a GitHub runner? And which cases need the owner's Mac with real logins?

## In short

- **Claude Code: yes.** One model turn, a permission ask, a stop and a resume in a new process all ran against the fake model with `ANTHROPIC_BASE_URL` and a dummy key. Its `rate_limit_event` didn't come, so usage and limits stay on the owner's Mac.
- **Codex: yes.** All five cases ran over `codex app-server` with a model provider set only through `-c`. That includes `account/rateLimits/updated`, which Codex built from the fake's response headers. Only `account/rateLimits/read` needs a login.
- **cursor-agent: no.** Its `--endpoint` names Cursor's own backend (Connect RPC services and a key exchange), not a model API. A fake model can't stand in for it. With no login, a runner can check only its install, `--version`, ACP's `initialize`, and that `session/new` says "Authentication required".

## How it was measured

- **When and where.** 2026-11-14, on this Mac (Apple silicon, macOS), inside a throwaway Linux container: Docker's `node:22-bookworm` image as `linux/amd64` (Debian 12, x86_64 under emulation, Node 22.23). That is the OS family and architecture of GitHub's `ubuntu-latest` runner, but not Ubuntu itself, and nothing ran on a GitHub runner. Every "GitHub runner" answer below comes from these no-login Linux runs. The CLIs were not run on macOS itself: there Claude Code and cursor-agent look in the login Keychain, which this spike was not to touch. A macOS runner's answers are not measured.
- **Installed apart.** The CLIs went into a temporary prefix in the scratch folder ("Install commands", below). They ran by that prefix's absolute path, never the `claude` or `codex` already on `PATH`. Everything was deleted afterwards.
- **No login, no real key, no network.** Every measured run:
  - ran in a container started with `--network none`, so 127.0.0.1 was the only place it could reach;
  - had a throwaway `HOME` and config folder (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `CURSOR_CONFIG_DIR`) under the scratch folder;
  - had an environment built from nothing: `PATH`, `HOME`, `TMPDIR`, the config folder, the fake's address, and the key `dummy`.

  No other `ANTHROPIC_*`, `CLAUDE_*`, `CLAUDECODE`, `OPENAI_*` or `CODEX_*` variable was set. No run saw a Keychain, the owner's `~/.claude`, `~/.codex` or `~/.cursor`, or any account. The `#[ignore]` probes were not run.
- **The fake model.** A Python standard-library HTTP server, about 200 lines, on 127.0.0.1:18080. It stayed in the scratch folder and is not committed (R01-11 writes the real one). It logs every request it gets: method, path, model, how it authenticated, and how many messages it carried. It answers:
  - `POST …/messages`: Anthropic's Messages API, streamed (`message_start`, `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop`), or one JSON message when not streamed;
  - `POST …/responses`: OpenAI's Responses API, streamed (`response.created`, `response.output_item.done`, `response.completed` with `usage`), as obid's own `chatgpt_api.rs` stand-in does;
  - `HEAD` with 200, `GET …/models` with an empty list, and 404 for anything else.

  Words in the prompt pick the case. One asks for a shell tool call first, then plain text once the tool's result comes back. One starts the stream, then holds it for 30 to 40 seconds. A server flag adds rate-limit headers to every answer.
- **The drivers.** Small Node scripts, also in the scratch folder. Each starts a CLI the way obid starts it and speaks its protocol: stream-json for Claude Code, JSON-RPC for `codex app-server`, ACP for cursor-agent. They answer every permission ask with allow and log every line.

## Install commands

The newest version of each, into a temporary prefix or a throwaway `HOME`. These are the versions the measurements used, as `--version` printed them; `npm view` gave the same two npm versions that day.

```sh
# Claude Code and Codex: their vendors' npm packages, into a temporary prefix.
npm install --prefix "$TMPDIR/cli-canary" @anthropic-ai/claude-code@latest @openai/codex@latest
"$TMPDIR/cli-canary/node_modules/.bin/claude" --version   # 2.1.325 (Claude Code)
"$TMPDIR/cli-canary/node_modules/.bin/codex" --version    # codex-cli 0.169.0

# cursor-agent: Cursor's installer, which writes under HOME, so a throwaway HOME is its prefix.
mkdir -p "$TMPDIR/cursor-home"
HOME="$TMPDIR/cursor-home" sh -c 'curl -fsS https://cursor.com/install | bash'
"$TMPDIR/cursor-home/.local/bin/cursor-agent" --version   # 2026.11.13-eacade1
```

- Claude Code's npm package brings its native binary as a platform package: `@anthropic-ai/claude-code-linux-x64` here. Leaving out optional dependencies would leave it out too.
- Cursor's installer puts the binary in `$HOME/.local/share/cursor-agent/versions/<version>/` and links `agent` and `cursor-agent` in `$HOME/.local/bin`.

## Claude Code

**One model turn against a fake model on 127.0.0.1 with no login: yes** (measured, 2.1.325, Linux).

How it ran:

- **Environment.** `ANTHROPIC_BASE_URL=http://127.0.0.1:18080` and `ANTHROPIC_API_KEY=dummy`, with `HOME` and `CLAUDE_CONFIG_DIR` throwaway folders.
- **Command.** obid's flags for a job's turn, never `--bare`:

  ```sh
  claude -p --input-format stream-json --output-format stream-json --verbose \
    --include-partial-messages --permission-prompt-tool stdio --permission-mode default \
    --setting-sources "" --strict-mcp-config --max-turns 3
  ```

- **Input.** One line on stdin, `{"type":"user","message":{"role":"user","content":"Say hello."}}`. stdin closes after the `result` line, as obid does.

What came of it:

- **The key.** `system` `init` said `apiKeySource: "ANTHROPIC_API_KEY"`, and the request carried the key as `x-api-key`. `ANTHROPIC_AUTH_TOKEN=dummy` works too: it went as `Authorization: Bearer`, and `init` said `apiKeySource: "none"`. The canary should use `ANTHROPIC_API_KEY`, the one `init` names.
- **What reached the fake.** First `HEAD /api/hello`, a reachability check on the base URL; the fake answered 200, and what a 404 there does wasn't tried. Then one `POST /v1/messages?beta=true`: streamed, model `claude-opus-5-5` (its default), 13 tools, about 52 KB. Nothing else was asked for, and the container had no network.
- **The result.** The usual stream events and an `assistant` message with the fake's words. Then `result` with `subtype: "success"`, `is_error: false`, `terminal_reason: "completed"`, the usage the fake reported, and a `total_cost_usd` worked out from it. Exit 0. About 3 to 4 seconds from start to exit.

The other cases:

- **Permission ask.** The fake answered with a `tool_use` of `Bash` running `touch canary-made-this`.
  - Claude Code sent `control_request` `can_use_tool` (`tool_name: "Bash"`, the input, and `permission_suggestions`).
  - The driver answered `control_response` allow with `updatedInput`. The command ran, and the file was there.
  - The second request carried the `tool_result`, and the turn ended `success`.
  - A read-only command doesn't ask: an earlier try with `echo canary` ran without a `can_use_tool`. The fake must ask for a command that writes.
- **Interrupt.** obid stops a Claude Code turn by ending its process group with SIGKILL (`core/crates/obid/src/conn.rs`). Two runs:
  - **SIGKILL.** Sent one second after `message_start`, while the fake held the stream. The process ended by the signal.
  - **Claude Code's own stream-json interrupt.** A `control_request` with `subtype: "interrupt"`, which obid doesn't send today. It answered `control_response` `success`, then a `user` line "[Request interrupted by user]". The `result` had `subtype: "error_during_execution"`, `is_error: true` and `terminal_reason: "aborted_streaming"`, and it exited 1.
- **Resume in a new process.** A new process with `--resume <session_id>` in the same `CLAUDE_CONFIG_DIR` and working folder. `init` carried the same session id, the request carried the earlier messages (6 messages, with both earlier prompts), and the turn ended `success`. A session ended by SIGKILL mid-stream resumed the same way: its request carried the killed turn's prompt too, since the fake held that answer for 30 seconds, which it does only when that prompt is in the request.
- **Usage and rate-limit events.** No `rate_limit_event`, with `ANTHROPIC_API_KEY` or with `ANTHROPIC_AUTH_TOKEN`. That held even when every answer carried these headers: `anthropic-ratelimit-unified-status`, `-reset` and `-representative-claim`, and `-5h-` and `-7d-` with `status`, `utilization` and `reset`.
  - The `result` line's `usage` and `total_cost_usd` did come.
  - obid reads limits from `rate_limit_event` (`core/crates/obid/src/agent.rs:550`, `limits.rs`) and its usage from Claude Code's `/usage` (`USAGE_FLAGS` in `agent.rs`). Both are the account's own, so they stay on the owner's Mac.
  - Whether a claude.ai plan login is what turns the event on wasn't measured: that needs a real login.

## Codex

**One model turn against a fake model on 127.0.0.1 with no login: yes** (measured, codex-cli 0.169.0, Linux).

How it ran:

- **Environment.** `HOME` and `CODEX_HOME` throwaway folders, and `CANARY_KEY=dummy`.
- **The model provider.** Set only through `-c`, with the keys obid's ChatGPT relay uses (`core/crates/obid/src/chatgpt_relay.rs:125-131`; there the provider is `openai_chatgpt_plan` and its `env_key` is `OBY_MCP_TOKEN`, here the spike's own names):

  ```sh
  codex \
    -c 'model_provider="canary_fake"' \
    -c 'model_providers.canary_fake.name="Canary fake model"' \
    -c 'model_providers.canary_fake.base_url="http://127.0.0.1:18080/v1"' \
    -c 'model_providers.canary_fake.env_key="CANARY_KEY"' \
    -c 'model_providers.canary_fake.wire_api="responses"' \
    -c 'model_providers.canary_fake.requires_openai_auth=false' \
    -c 'model_providers.canary_fake.supports_websockets=false' \
    -c 'cli_auth_credentials_store="ephemeral"' \
    -c 'features.tool_search=false' \
    -c 'model="gpt-canary"' \
    app-server
  ```

- **The requests.** JSON-RPC over stdio, in this order:
  1. `initialize` (`clientInfo` with a `version`), then the `initialized` notification.
  2. `account/read` and `account/rateLimits/read`.
  3. `thread/start` with obid's params for a thread in Oby's box (`codex.rs`, `thread_params`): `{cwd, sandbox: "danger-full-access", approvalPolicy: "untrusted", approvalsReviewer: "user"}`.
  4. `turn/start` with `{threadId, input: [{type: "text", text, text_elements: []}]}`.

What came of it:

- **The account.** `account/read` answered `account: null`, `requiresOpenaiAuth: false`. `account/rateLimits/read` answered error -32600, "codex account authentication required to read rate limits".
- **The thread.** `thread/start` answered with `modelProvider: "canary_fake"`.
- **What reached the fake.** One `POST /v1/responses`, streamed, with `Authorization: Bearer dummy`. Its tools were `exec_command`, `write_stdin`, `request_user_input`, `view_image`, `multi_agent_v1`, `get_goal`, `create_goal`, `update_goal` and `web_search`. Nothing else was asked for, and the container had no network.
- **The turn.** `item/completed` with the `agentMessage`, then `thread/tokenUsage/updated`, `account/rateLimits/updated`, and `turn/completed` with `status: "completed"`. Exit 0 after stdin closed. About 4 seconds.
- **Two warnings, harmless here.**
  - "Model metadata for `gpt-canary` not found", for the made-up model name; a real model name avoids it.
  - "Codex could not find bubblewrap on PATH … will use the bundled bubblewrap", on a Debian with no `bwrap`.

The other cases:

- **Permission ask.** The fake answered with a `function_call` to `exec_command`, `{"cmd": "touch canary-made-this"}`.
  - Codex sent `item/commandExecution/requestApproval` with `command: "/bin/bash -lc 'touch canary-made-this'"`. The driver answered `{"decision": "accept"}`.
  - The command ran (`exitCode: 0`, the file was there), the second request carried the `function_call_output`, and the turn ended `completed`.
  - The fake must call a shell tool by a name the request lists: `exec_command` in 0.169.0. A file-change approval (`item/fileChange/requestApproval`, an `apply_patch` call) wasn't measured.
- **Interrupt.** `turn/interrupt` `{threadId, turnId}`, sent while the fake held the stream, was answered `{}`. Then came `turn/completed` with `status: "interrupted"`, 1.4 seconds after the turn started.
- **Resume in a new process.** A new `codex app-server` with the same `CODEX_HOME` took `thread/resume` with the start run's thread id. It kept the same thread, and its request carried the earlier prompt with the new one; the turn ended `completed`. It also sent `deprecationNotice`: "Full-history hydration is deprecated for paginated threads; use `excludeTurns: true`, then page with `thread/turns/list` and `thread/items/list`." obid's `thread/resume` sends no `excludeTurns` today, so this is drift the canary should catch when it becomes an error.
- **Usage and rate-limit events.** `account/rateLimits/updated` came after every model answer, and `thread/tokenUsage/updated` carried the response's `usage`.
  - With no rate-limit headers, `primary` and `secondary` were null.
  - With `x-codex-primary-used-percent: 42`, `x-codex-primary-window-minutes: 300`, `x-codex-primary-reset-at`, `x-codex-primary-reset-after-seconds`, and the same four `x-codex-secondary-*` headers on the response, the event carried `primary: {usedPercent: 42, windowDurationMins: 300, resetsAt}` and `secondary: {usedPercent: 10, windowDurationMins: 10080, resetsAt}`.
  - The fake sent both reset headers, and which one Codex read wasn't separated.
  - `account/rateLimits/read` needs a login (the error above), so obid's usage read stays on the owner's Mac.

## cursor-agent

**One model turn against a fake model on 127.0.0.1 with no login: no** (measured up to the login, 2026.11.13-eacade1, Linux).

- **Installed.** With Cursor's own installer into a throwaway `HOME` ("Install commands").
- **Its endpoint options**, from `cursor-agent --help`:
  - `-e, --endpoint <url>`, also `CURSOR_API_ENDPOINT` (default `https://api2.cursor.sh`);
  - `--api-key <key>`, also `CURSOR_API_KEY`;
  - `-H, --header`, for extra request headers.
- **The endpoint is Cursor's own backend, not a model API.** It was pointed at the fake with `--api-key dummy`, with no network:
  - `-p` asked `POST /auth/exchange_user_api_key`, then said "The provided API key is invalid".
  - Over ACP (`cursor-agent --sandbox disabled acp`, as obid starts it, `core/crates/obid/src/acp.rs:1078`) it asked `POST /aiserver.v1.DashboardService/GetUserPrivacyMode`, `POST /auth/exchange_user_api_key` and `POST /aiserver.v1.ServerConfigService/GetServerConfig`. Those are Connect RPC calls (`connect-es/1.6.1`). Then `session/new` answered -32603 "Failed to initialize session services".
  - A fake would have to stand in for Cursor's private, undocumented services. That is not a model fake, and it would break whenever Cursor changes them.
- **Bedrock.** `cursor-agent bedrock` configures AWS Bedrock, but its help says it "requires stored credentials" and validates them "with the backend": Cursor's again. Not measured.
- **Offline, with no endpoint**, it says "Failed to reach the Cursor API".
- **What a runner can check with no login.** Measured:
  - the install and `--version`;
  - ACP `initialize` (with `clientInfo` holding a `version`, as obid sends it, `acp.rs:626`): `protocolVersion: 1`, `loadSession: true`, `mcpCapabilities` `http` and `sse`, `authMethods` `cursor_login`. Without a `version` in `clientInfo` it answers -32602 "Invalid params";
  - `session/new` answering -32000 "Authentication required". That is what `the_real_cursor_agent_signed_out_says_so` (`adapters.rs:1309`) checks through obid.

  Everything past `session/new` needs the owner's Mac with a real login.

## Cases, per CLI

"GitHub runner" means a run with a fake model and no login. It is drawn from the no-login Linux container runs above, not from a run on a GitHub runner.

| Case | Claude Code (2.1.325) | Codex (codex-cli 0.169.0) | cursor-agent (2026.11.13-eacade1) |
|---|---|---|---|
| Start (one model turn) | **Yes**, GitHub runner with a fake model. Measured: 2.1.325 on Linux, `claude -p --input-format stream-json --output-format stream-json --verbose …` with `ANTHROPIC_BASE_URL` and `ANTHROPIC_API_KEY=dummy`; `result` `success`. | **Yes**, GitHub runner with a fake model. Measured: 0.169.0 on Linux, `codex -c model_provider=… app-server`, then `initialize`, `thread/start` and `turn/start`; `turn/completed` `completed`. | **No**: owner's Mac with real logins. Measured: 2026.11.13-eacade1 on Linux, `cursor-agent --sandbox disabled acp`; `session/new` answers -32000 "Authentication required", and `--endpoint` reaches Cursor's own services, not a model API. |
| Permission ask | **Yes**, GitHub runner with a fake model. Measured: 2.1.325, the fake's `Bash` `touch` brought `control_request` `can_use_tool`, answered allow. | **Yes**, GitHub runner with a fake model. Measured: 0.169.0, the fake's `exec_command` `touch` brought `item/commandExecution/requestApproval`, answered `accept`. A file-change approval is not measured. | **No**: owner's Mac with real logins. Not measured: a permission ask needs a session, and `session/new` needs a login (measured above). Source: docs/cursor.md (`session/request_permission`, from the signed-in probe). |
| Interrupt | **Yes**, GitHub runner with a fake model. Measured: 2.1.325, SIGKILL mid-stream (how obid stops it, `conn.rs`) ended the process; the stream-json `interrupt` request ended the turn `error_during_execution`. | **Yes**, GitHub runner with a fake model. Measured: 0.169.0, `turn/interrupt` mid-stream gave `turn/completed` `interrupted`. | **No**: owner's Mac with real logins. Not measured: `session/cancel` needs a session, which needs a login. Source: docs/cursor.md ("Stop"), obid's `acp.rs`. |
| Resume in a new process | **Yes**, GitHub runner with a fake model. Measured: 2.1.325, `--resume <session_id>` in a new process carried the earlier messages; a session killed mid-stream resumed too. | **Yes**, GitHub runner with a fake model. Measured: 0.169.0, `thread/resume` in a new `app-server` carried the earlier prompt (with a `deprecationNotice` about full-history hydration). | **No**: owner's Mac with real logins. Not measured: `session/load` needs a session, which needs a login. `initialize` did say `loadSession: true` (measured). Source: docs/cursor.md (`session/load` works in a new process, signed in). |
| Usage and rate-limit events | **No**: owner's Mac with real logins. Measured: 2.1.325 sent no `rate_limit_event` with a dummy key or token, even with `anthropic-ratelimit-unified-*` headers on every answer; only the `result` line's `usage` came. | **Yes** for the event, GitHub runner with a fake model. Measured: 0.169.0, `account/rateLimits/updated` carried `primary` and `secondary` from the fake's `x-codex-primary-*` and `x-codex-secondary-*` headers. **No** for `account/rateLimits/read`, owner's Mac with real logins. Measured: it answers "codex account authentication required". | **No**: owner's Mac with real logins. Not measured: cursor-agent reports no usage, and obid reads it from Cursor's dashboard service with the account's access token. Source: docs/cursor.md, docs/contracts/usage.md (USAGE-6 to 9). |

## What the fake model must send

For R01-11's fake model server, from what worked above:

- **Anthropic.**
  - `HEAD /api/hello` answered 200.
  - `POST /v1/messages` (Claude Code adds `?beta=true`), streamed as server-sent events: `message_start`, `content_block_start`, `content_block_delta` (`text_delta`, or `input_json_delta` for a `tool_use`), `content_block_stop`, `message_delta` with `stop_reason` (`end_turn` or `tool_use`), then `message_stop`.
  - A tool call must be a command that writes, or no permission ask comes.
- **OpenAI Responses.**
  - `POST /v1/responses`, streamed: `response.created`, `response.output_item.done` (a `message` with `output_text`, or a `function_call` with `name`, `arguments` and `call_id`), and `response.completed` with `usage`.
  - The tool name comes from the request's `tools`.
  - `x-codex-primary-*` and `x-codex-secondary-*` headers make `account/rateLimits/updated` carry numbers.
- **A held stream.** For the interrupt case: send the first event, then wait.
- **A log of every request**, so a case can assert that the CLI's model call reached it, and with what (the resume case: the earlier prompt).

## What the canary can reuse

The canary should drive these through obid's adapters, as R01-11 says. Line numbers are on `dev` at 9eb7d431, each a test's `fn` line (its `#[ignore]` is the line above).

- **`make e2e-live`** (`Makefile:46`, `node e2e/run.mjs --live`). The two-Oby review scenario with the real Claude Code adapter, run by a person on a Mac with a signed-in Claude Code. Owner's Mac only.
- **`scripts/cursor-probe.sh offline`.** The real cursor-agent signed out in a throwaway `HOME`: `--version`, `acp --help`, `initialize`. It needs no login, so it can run on a runner as is. Its `signed-in` half needs two real Cursor logins: owner's Mac only.
- **`#[ignore]` real-CLI tests in `core/crates/obid/tests/`.** Listed, not run.
  - `adapters.rs:1309`, `the_real_cursor_agent_signed_out_says_so`. Needs `cursor-agent` on `PATH` and no login. obid starts it in a job's sandbox, and the job fails saying the account is signed out. It fits a runner as is.
  - `claude_bare.rs:533`, `issue_1867_the_real_claude_code_still_takes_a_plan_login`. Needs `claude` on `PATH` signed in with a plan, and spends a real turn. Owner's Mac only. Its flags and environment are the ones a job's turn uses, which a fake-model version can copy.
  - `chatgpt_api.rs:1728`, `chatgpt_real_codex_talks_through_the_relay`, and `chatgpt_api.rs:1789`, `chatgpt_real_codex_runs_through_the_relay`. They need `OBY_REAL_CODEX=<binary>` and no login: the real Codex runs through obid's ChatGPT relay to the test's own OpenAI stand-in. They are the closest existing thing to the Codex canary, and fit a runner as they are.
  - `mcp_sphere.rs:197`, `a_review_request_goes_through_oby_mcp`. Needs `OBY_MCP_BIN` (the `oby-mcp` binary) and no agent CLI.
  - `mcp_sphere.rs:294`, `a_claude_code_session_sends_a_review_request`. Needs `OBY_MCP_BIN` and, with `OBY_MCP_CLAUDE=1`, Claude Code signed in on the Mac. Owner's Mac only.
  - `jobs_api.rs:1732` (`a_command_in_a_jobs_box`) and `:1795` (`reach_from_inside_a_job`) are sandbox helpers, not real-CLI probes.
- **Stand-in agents in `core/crates/obid/tests/agents/`.** What each fakes is what a canary case asserts against the real CLI.
  - `fake-claude`: Claude Code's stream-json for jobs: commits, edits, permission asks, questions, stops.
  - `fake-claude-bare-default`: the lines Claude Code 2.1.302 printed with `--bare` on a plan login (#1867), a recorded transcript.
  - `fake-claude-recording`: records the arguments and environment obid launched Claude Code with, then runs another stand-in.
  - `fake-harness` and `fake-harness-jobs`: Claude Code as a conversation's agent (partial messages, tool calls, the `oby` MCP server), and the switch between it and `fake-claude`.
  - `fake-codex`: `codex app-server` for jobs: `thread/start`, `thread/resume` across processes, command and file-change approvals.
  - `fake-codex-harness` and `fake-codex-on-*`: Codex as a conversation's agent and `codex debug models`; the `on-*` ones keep a feature on (`shell_tool`, `hooks`, `image_generation`) whatever obid sets, which obid must refuse.
  - `fake-cursor` and `fake-cursor-harness`: `cursor-agent acp` for jobs and conversations: `session/new`, `session/load`, `session/request_permission`, and `cursor-agent models`.
  - `fake-login.mjs`: each tool's sign-in, as obid runs it.
  - `fake-planner`: Claude Code as obid's brain (`--json-schema`).
  - Reviewer and other stand-ins (`asking-reviewer`, `prying-reviewer`, `secretspec-agent`, …): obid's own rules, not CLI drift.
- **Recorded transcripts.** `core/crates/obid/tests/fixtures/claude-rate-limit-scoped.json` and `claude-rate-limit-overage.json` hold real `rate_limit_event` lines. A runner can't make Claude Code send them, but obid's parsing of them stays tested. Replaying recorded streams in the stand-ins is #1098.

## Open risks

- **Claude Code's usage and limits need a real account.** No `rate_limit_event` came with a dummy key or token. obid's limit handling for Claude Code (LIMITS, USAGE) can't be checked on a runner, and it stays on the owner's Mac (#1099).
- **cursor-agent has no model endpoint.** Its `--endpoint` is Cursor's private backend. A canary for it is a signed-out check on a runner, plus a real login on the owner's Mac. The other way, a real Cursor API key as a GitHub secret, is a real key on a runner, which this task rules out.
- **Other hosts and telemetry.** Every run here had no network, so whatever else the CLIs try to reach failed quietly, and a turn still finished. With a network a runner would let it out:
  - Claude Code's telemetry, error reports and self-update: obid sets `DISABLE_TELEMETRY`, `DISABLE_ERROR_REPORTING` and `DISABLE_AUTOUPDATER`, each `1`, on every run it starts (`process::CLAUDE_RUN_SWITCHES`, `core/crates/obid/src/process.rs:286-290`), the third turning self-update off per run (CLIVERSIONS-5). The drivers here set none of them;
  - Codex's update check;
  - cursor-agent's backend.

  The canary should run its CLIs with no network but loopback, or set those switches and say so. What each CLI asks for with a network and no login wasn't measured.
- **Self-update.** cursor-agent updates itself by default with no switch (CLIVERSIONS). Codex's update check is `check_for_update_on_startup` in `config.toml`, which a `-c` can set. Claude Code's is the third of `process::CLAUDE_RUN_SWITCHES` (above), which a canary through obid gets with the other two. None of them could update here, with no network. The canary installs the newest each run anyway, but a CLI that updates mid-run would test a version the report doesn't name.
- **Not quite a GitHub runner.** These runs were Debian 12 under x86_64 emulation, not Ubuntu on GitHub's hardware, and Codex used its bundled bubblewrap. Ubuntu 24.04's AppArmor stops unprivileged user namespaces, so Codex's own sandbox, and obid's bubblewrap box on Linux, may not start there (AGENTS.md: on Linux obid runs Codex `danger-full-access`). Claude Code's sandbox wasn't used. A macOS runner would add the Keychain, which these runs never met.
- **Drift already showing.**
  - Codex 0.169.0 warns that full-history `thread/resume` is deprecated.
  - cursor-agent refuses an ACP `initialize` whose `clientInfo` has no `version`.
  - Claude Code's default model in these runs was `claude-opus-5-5`.

  These are the kind of change the canary is for. The fake model should accept any model name.
- **Timing.** Under emulation and a busy Mac, one Claude Code start took nearly two minutes before its first request. Most took 2 to 5 seconds. A canary's time limits need room.

## For #1099

A summary to paste on #1099 as is:

> R01-10 measured the newest CLIs (Claude Code 2.1.325, Codex 0.169.0, cursor-agent 2026.11.13) against a fake model on 127.0.0.1, with no login and no network, on Linux (docs/cli-canary.md). The GitHub canary (R01-11) can cover Claude Code's start, permission ask, interrupt and resume, and all five Codex cases: start, permission ask, interrupt, resume, and `account/rateLimits/updated` from response headers. What stays on the owner's Mac with real logins: Claude Code's `rate_limit_event` and `/usage`, Codex's `account/rateLimits/read`, and every cursor-agent case past `session/new`, since its `--endpoint` is Cursor's own backend, not a model API. On a runner, cursor-agent gets only the signed-out check (`adapters.rs`, `the_real_cursor_agent_signed_out_says_so`).
