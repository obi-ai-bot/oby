# The agents' CLIs

The text of the root [AGENTS.md](../AGENTS.md)'s "Proven on 2026-10-09 (keep these working)"; its rules for obid are in [core/AGENTS.md](../core/AGENTS.md#proven-on-2026-10-09-keep-these-working).

## Proven on 2026-10-09 (keep these working)

- **Claude Code headless:** `claude -p --input-format stream-json --output-format stream-json --verbose`, never `--bare`. Permission prompts with `--permission-prompt-tool stdio` arrive as `control_request` `can_use_tool`; answer with `control_response` allow (`updatedInput`) or deny (`message`). Usage arrives as `rate_limit_event` with `five_hour` and `seven_day` utilization.
- **Spawning `claude`:** remove `CLAUDECODE`, `CLAUDE_CODE_*` and `ANTHROPIC_*` from the environment first.
- **PATH:** read the login shell with `getpwuid(getuid()).pw_shell` (it can differ from `$SHELL`) and run it with `-l -c /usr/bin/env`.
- **Codex:** `codex app-server` speaks JSON-RPC over stdio: `initialize`, then `account/read`, `account/rateLimits/read`, `thread/start`, `turn/start`, `turn/interrupt`.
- **Separate accounts:** `CLAUDE_CONFIG_DIR` per Claude account, `CODEX_HOME` per Codex account.
- **The brain:** `claude -p --model haiku --tools "" --strict-mcp-config --setting-sources "" --no-session-persistence --output-format json --system-prompt … --json-schema …`, the prompt on stdin, in its own directory; the plan is `.structured_output` (it can be missing even on success). 3–4 s. obid validates every plan (`Plan::problems`). Since #65 the prompt carries a briefing (the conversation, the jobs' titles and stages, the inbox, usage), with what agents wrote quoted in `work.md` there: then it runs with `--tools Read --allowedTools Read --max-turns 6` (a StructuredOutput call that misses the schema costs a turn; a run that ends `error_max_turns` is retried once, told to read less, #535), boxed like a job (`core/crates/obid/src/sandbox.rs`, bubblewrap on Linux). The cost of that read against a real `claude` isn't measured yet.
- **Codex jobs:** `thread/start` with `sandbox: "workspace-write"`, `approvalPolicy: "on-request"`, `approvalsReviewer: "user"` and `config.sandbox_workspace_write` (`writable_roots`: the clone's git directory, `network_access: false`), so commits stay sandboxed. Approvals arrive as `item/commandExecution/requestApproval` and `item/fileChange/requestApproval`; answer `{"decision": "accept" | "decline"}`.
- **Reading agents' output:** split on `\n` only. A reader that also breaks at U+2028 (Swift Foundation's `lines` does) breaks inside JSON strings, and a broken `result` line hangs the job.
- **The character in the window:** Oby follows the pointer anywhere on screen only while its window is shown, and main stops watching it when the window is hidden, minimized or closed ([apps/electron/AGENTS.md](../apps/electron/AGENTS.md#the-character-in-the-window)).
