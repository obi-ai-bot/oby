# Fast replies: from Send to Oby's first words

The owner's ask (2026-10-19): Oby's conversation should answer as fast as Claude Code and the ChatGPT and Codex desktop apps. This page has what the fastest harnesses do, where Oby's seconds went, what changed, and what's left.

## What the fast harnesses do

- **One agent process stays running.** Anthropic recommends Claude Code's streaming input mode, where the agent runs "as a long lived process that takes in user input" and each message goes to the same process ([Agent SDK: streaming input](https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode)). The SDK can even start it before the first prompt (`startup()`, `prewarm()`; [TypeScript SDK](https://code.claude.com/docs/en/agent-sdk/typescript)). Codex's IDE extension runs on `codex app-server`, one process for every thread ([source](https://github.com/openai/codex/tree/main/codex-rs/app-server)). OpenCode's UI talks to a local server: `prompt_async` answers 204 at once and the words come over SSE ([OpenCode server](https://opencode.ai/docs/server/)).
- **They warm what comes next.** When a Codex thread starts or wakes, a background task opens the model's WebSocket and sends a warm-up request, so the first real turn starts sooner ([`startup_prewarm.rs`](https://github.com/openai/codex/blob/main/codex-rs/core/src/session/startup_prewarm.rs)). Aider can ping every 5 minutes to keep the prompt cache warm ([`--cache-keepalive-pings`](https://aider.chat/docs/config/options.html)).
- **Words stream as tokens.** Without partial messages, Claude Code gives a text block only once it's finished. `--include-partial-messages` gives each `text_delta` as it comes, and a tool call's start ([streaming output](https://code.claude.com/docs/en/agent-sdk/streaming-output)). Cline renders each partial block as it arrives and merges renders behind a lock, so the UI never falls behind ([Cline overview](https://github.com/cline/cline/blob/main/.clinerules/cline-overview.md)).
- **The prompt prefix stays the same, so the cache holds it.** The order is the system prompt and tools, then CLAUDE.md, then the conversation. Switching model, connecting an MCP server, or upgrading Claude Code rebuilds the whole prefix. On a Claude subscription the main conversation's cache lasts 1 hour; with an API key it's 5 minutes unless `CLAUDE_CODE_PROMPT_CACHE_TTL=1h` ([how Claude Code uses prompt caching](https://code.claude.com/docs/en/prompt-caching); [why caching matters](https://claude.com/blog/lessons-from-building-claude-code-prompt-caching-is-everything)). Anthropic measured a 10-turn chat with a long system prompt going from about 10 s to 2.5 s to the first token with the cache ([prompt caching](https://claude.com/blog/prompt-caching)).
- **No model call before the answer starts.** None of these harnesses runs a router or planner ahead of the reply.

## Where Oby's seconds went (measured 2026-10-19)

Oby already streamed partial messages, and a conversation never waited for the brain: the harness turn starts at once. The time went before the agent and around it:

- **Repositories were extracted on the critical path.** Every new conversation, and every turn after `dev` moved, extracted each repository (`git archive | tar`, 22 MB and 1,678 files for obi) before "Thinking" showed. That took 3.2–3.5 s just now, and more under heavier load. The owner's own turns on 2026-10-18 and 19 waited 2.4 to 40.5 s between the POST and Claude Code receiving the prompt (from `conversations/<id>.json` against the session transcripts). New conversations were the slow ones: 6.7, 13.8, 13.9 and 40.5 s.
- **A new `claude` started for every turn.** Starting it, connecting Oby's MCP server (`init` waits until every MCP server is connected: a test server that took 4 s per request pushed `init` from 3.6 s to 18.4 s), and reading its session again with `--resume` took 2–7 s per turn. On the same machine, a running `claude` answered its next prompt with its first word in 0.76–0.83 s; a fresh one took 4.25 s.
- **Waiting for the process to exit.** After its `result`, obid waited up to 3 s for Claude Code to exit before sending the reply and answering the POST.

## What changed (this pull request)

1. **A conversation's Claude Code waits for its next turn** (`brain/harness.rs`, `Warm`). After a turn that answered, the process keeps running in the same box with the same login copy and MCP token, and the next turn is written to its stdin. The token is good only while a turn is under way, the scrub runs before every turn, and the login copy is taken out when the process ends (a copy near its end starts a new one, with a new copy). It waits 30 minutes at most, and at most 3 conversations' wait at once. It ends on a stop, a cancel, the budget, a deleted conversation, obid's stop, or a login copy near its end. When one has died, the next turn resumes the session as before.
2. **Rooms never wait for a checkout obid could keep ready.** obid keeps a spare tree of each repository at its default branch (`home/checkouts/<repo>/spare-<sha>`). A new room takes it with a rename, and the next spare is made in the background. A room whose branch moved keeps its tree for the turn, while the new tree is extracted beside it and swapped in with renames.
3. **A stopwatch line for every turn** in obid's log: `turn … ended in its session: answered (its turn 0 ms, repositories 60 ms, its Claude Code was running 97 ms, starting 97 ms, session 109 ms, first words 834 ms, ended 1331 ms)`.

## Before and after

Measured with a throwaway obid (debug build, its own home, events followed over `GET /v1/events`), on the "Personal" Claude account (Sonnet 5.5), with field/fixture-app and obi-ai-bot/obi as repositories. It ran 4 tiny prompts in one new conversation, then 1 in a second new conversation. The Mac's load average was 250–350 throughout. "First words" is POST to the first `delta` event; "answer" is POST to the POST's answer.

| Turn | First words before | after | Answer before | after |
|---|---|---|---|---|
| A follow-up in a conversation | 5.6–6.3 s | **1.1–1.5 s** | 7.0–7.3 s | **1.6–1.9 s** |
| A second new conversation | 8.6 s (3.2 s of it extracting obi) | 2.9–9.4 s (repositories 53–134 ms; starting Claude Code 1.3–7.2 s, depending on load) | 10.0 s | 3.4–10.1 s |
| The first conversation after obid's start, no clone yet | 16.0 s | 6.2–20.4 s (cloning obi both times) | 17.4 s | 6.2–20.4 s |

"After" covers two runs of the same script: one with the warm process and spare trees, and one with the final code. A new conversation now waits only for Claude Code to start. The extraction is gone, but at load 300 the start itself took 1.3 to 7.2 s.

## A new conversation's Claude Code, started ahead

Claude Code connects its MCP servers when it starts, before any prompt: with a test server that took 4 s per request and a prompt sent 25 s after the start, `init` came 41 ms after the prompt. So a Claude Code started ahead saves both its start and the connection, as the Agent SDK's `prewarm()` does.

After each of the owner's turns, obid names the next new conversation's id and readies its room, box, login copy and MCP configuration, then starts its Claude Code with no prompt. It does this for each profile (docs/contracts/profiles.md, PROFILES-46 and 47): the turn's profile first, then the others whose new conversation would talk on Claude Code through one of their own accounts, three profiles at most, one at a time. Until a turn is under way, its token may only connect to Oby's tools and list them. The new conversation's first turn is written to it like a follow-up.

Measured the same way, back to back with #726's code, at load 290–350. The second new conversation in a run got:

| | #726 | Started ahead |
|---|---|---|
| POST until Claude Code had the prompt (`session` in the stopwatch line) | 1.8–2.8 s | **0.14–0.29 s** |
| First words | 3.2–10.0 s | **0.85–4.8 s** (the model's own latency was noisy in this window: follow-ups took 0.8–4.9 s too) |

## What's left

- Show the model's thinking (`thinking_delta`) and a tool's start (`content_block_start`) as they stream, instead of waiting for the whole tool-use block.

## A new Codex conversation's first answer (2026-11-19)

A new conversation on Codex started `codex app-server` for its first turn, then `initialize` and `thread/start`, and only then did Codex start the owner's MCP servers: per thread, in the thread's folder (`mcpServer/startupStatus/updated` names the thread). Its first model call waited for them, 1.6 to 1.8 s on a quiet Mac with the owner's six servers (#2490).

What changed (CONVERSATION-AGENT-51, 52): after each of the owner's turns, the next new conversation whose first turn would talk on Codex with a `codex login` gets its `app-server` started ahead, as a Claude Code is (PROFILES-47): its room, box and login copy readied, `features list` checked, then `initialize` and its thread started with what a first turn's thread is told (`codex::start_ahead`). Its MCP servers come up while it waits. No turn runs, so nothing reaches the model, and Codex writes no thread to its history until one does. The first turn is a `turn/start` on that thread, given only when it was started as the turn would start one now (`harness_codex::key`), as a Codex waiting between turns is. Not on a ChatGPT login: its thread starts with the turn's model, and its run reaches OpenAI only through obid's relay with the turn's own token.

Measured on a scratch obid, the same words in a new conversation each time (`POST /v1/plans` until its answer, and the stopwatch line): see the pull request that made it.

Jobs stay cold (docs/contracts/lanes.md, LANES-25): a job's worktree, git folders, broker token and an own Codex run's profile are fixed when its CLI starts, so nothing can be started for it before it exists.

## Codex follow-ups (2026-11-18)

A Codex conversation started `codex app-server` again for every turn (#2490): its `features list` check, the start, `initialize` and `thread/resume` before the turn's `turn/start`. Measured on a scratch obid on 2026-11-17 at load 10 to 19, its follow-ups took 5.2 to 12.7 s where Claude Code's took 1.3 to 3.3 s.

What changed (CONVERSATION-AGENT-44, 45): a Codex turn that answered keeps its `app-server` running, its thread loaded, as a conversation's Claude Code waits (`Warm`, the same 30 minutes and three conversations at most, ended the same ways). The next turn is a `turn/start` written to it: no start, no `features list`, no `initialize`, no thread read again. That turn's model is its own (`turn/start` names it). It's given the turn only when it was started as the turn would start one now (its `-c` settings, its lane, its account and what its thread is told: `harness_codex::key`), its login copy isn't near its end, and it wrote nothing while it waited but its housekeeping (`codex::between_turns`: a notification that isn't an item's or a turn's). Else it ends, and the turn starts Codex and picks the thread up as before. An own Codex (LANES-13) is asked for its settings again before each turn, as before each thread. That is also what happens after it died. T3 Code does the same: one `app-server` per session, each turn a `turn/start` on its thread, and a failed one retired so the next turn starts a fresh one (its `CodexAdapterV2`).

Not measured on a real Codex: the follow-up's time is now the model's, as Claude Code's is.

## The own lane's first turn (2026-11-16)

It came back slow: a new conversation's "Hello" took 34 s again, its model call only 3–4 s of it (`brain-turns.jsonl`: `wall_ms` 34260, `duration_ms` 3696), and wrote 520,051 tokens to the cache. The stopwatch put all of it in `session` (POST until Claude Code's `init`): 30.5 s every time.

- **Why.** Since LANES-12 (#2093) the owner's conversation loads their own MCP servers. Given any `--mcp-config` server, Claude Code 2.1.325 holds a headless session's first turn until every MCP server answers or its 30 s timeout passes (its `explicit_mcp_config` hold). One of the owner's servers is an `ssh` command that never answers, so every first turn waited 30 s. Without `--mcp-config` the same launch reached `init` in 3.6 s; with a server there, stdio or HTTP, working or not, 30.4–30.9 s. The prestarted Claude Code (above) hid this only when it had been running 30 s, and only for the conversation it was readied for.
- **Why so many tokens.** `--tools` had no `ToolSearch`, so Claude Code put every tool of the owner's servers in the prompt: 707 tools, about 500k tokens, where their terminal defers them.
- **What changed** (CONVERSATION-AGENT-24): an own session's environment sets `CLAUDE_CODE_MCP_STARTUP_WAIT_MS=0`, which lifts the hold; `oby-mcp.json` marks Oby's and Tetrix's servers `alwaysLoad`, so they're still connected before the first turn; `--tools` adds `ToolSearch`. All three are Claude Code's own switches: obid copies none of its work.

Measured on the owner's Mac with a scratch obid (its own home, port 7881) on their real `~/.claude`, Claude Opus 5.5, "Hello" five times in one new conversation, then once in another. First on `dev` before #2433, then on `dev` with it (#2433 loads every settings source and runs the owner's hooks):

| Turn | `wall_ms` before | after | `duration_ms` before | after |
|---|---|---|---|---|
| Before #2433: a new conversation, cold | 34174 | **4689** | 2996 | 3182 |
| Before #2433: the same conversation, turns 2–5 | 1627–3154 | 2166–2539 | 1622–3148 | 2164–2537 |
| Before #2433: a second new conversation (started ahead) | 13484 | **1525** | 2061 | 1513 |
| With #2433: a new conversation, cold | 36178 | **10027** | 3186 | 3669 |
| With #2433: the same conversation, turns 2–5 | 1816–2613 | 2426–6661 (turn 3 called a tool) | 1806–2607 | 2421–6647 |
| With #2433: a second new conversation | 37669 | **6130** | 4133 | 3695 |

The cold turn's `session` mark went from about 30,500 ms to 1,648–7,263 ms, Claude Code's own start at a load average near 30 (replays of the same launch reached `init` 2.6–3.1 s after the prompt; once its hooks started only 7.3 s in). It wrote 42,078–53,753 tokens to the cache instead of about 520,000. A first turn still calls Oby's tools (`oby_jobs`, 3.5 s), and `ToolSearch` finds one of the owner's tools (3.3 s).

Two more, found while measuring that (CONVERSATION-AGENT-25, LANES-24):

- **With #2433 the Claude Code started ahead was never used.** The owner's `SessionStart` hooks run since #2433, and Claude Code writes their `hook_started` and `hook_response` lines as it starts, before any prompt. A waiting Claude Code that wrote anything wasn't given the turn, so every new conversation started cold. obid now reads and drops a waiting one's hook lines (`Conn::quiet_but`) and still refuses one that wrote anything else. Measured on a scratch obid, `dev` before and after, a calm Mac (load 4 to 14): each new conversation after the first had its Claude Code started ahead taken (`its Claude Code was running`, `session` 88–90 ms) where before it started anew (`session` 483–534 ms; 3.8–7.3 s at a load near 30, above).
- **An own Claude Code job waited the same 30 s** whenever Oby is signed in to Tetrix: obid then gives it an `--mcp-config` (Tetrix's server), and the job loads the owner's servers. Its environment now sets the same `CLAUDE_CODE_MCP_STARTUP_WAIT_MS=0` (obid's run environment lets that one `CLAUDE_CODE_*` name in) and its servers are `alwaysLoad`. A real own job's launch, captured on a scratch obid and replayed with a server like Tetrix's in its `--mcp-config`: `init` at 31.4 s as `dev` ran it, 1.7 s after, the server connected both times.
