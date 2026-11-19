# AGENTS-OPENBOT: Agents do general work the way OpenBot's agents do

- **Date:** 2026-11-16
- **Status:** In force
- **Replaced:** For Agents (obid's helpers) only, three things. AGENTS.md's "Dispatcher, not harness" puts a browser, Computer Use and an agent's own memory on "the agent's" side, which stays in the CLI. The helpers contract's own conversation runs on Oby's own-lane sandbox, writing only its room. Oby's memory is written only on the owner's Remember. Projects, coding jobs and conversations keep all three as they are.
- **Changed by:** Nothing
- **Contracts and scenarios:** [helpers.md](../contracts/helpers.md); [lanes.md](../contracts/lanes.md) for an agent's level; the scenarios of tasks B00-60 to B00-70 (proposed: AGENTLEVEL, AGENTROUTINE, AGENTMCP, AGENTMEM, AGENTEDIT, AGENTMSG, AGENTMAKE, AGENTTASK, AGENTBROWSER, AGENTCU)

## The owner's words

> Follow exactly what OpenBot does!

The owner's words, 2026-11-16, answering whether an agent that reads mail needs a click for every send, and again whether a browser and Computer Use are offered by default.

> Let's follow what OpenBot does!

The owner's words, 2026-11-16, answering who writes an agent's memory.

> I think so and I guess on OpenBot you can choose different tools for different agents right? If that's the case we can follow that but yeah definitely those tools comes for every Agent.

The owner's words, 2026-11-16, answering whether a general agent keeps Oby's tools (start coding jobs, change Settings, post to the Obysphere).

> The agents part is more fore general tasks and the other part is focused on coding! Makes sense?

The owner's words, 2026-11-16.

## Where it is stated

Not yet in AGENTS.md: task B00-60 states it under "Dispatcher, not harness", within AGENTS.md's byte limit. Until then, this file and the tasks B00-60 to B00-70 hold it, and docs/contracts/helpers.md takes it as each task lands.

## What OpenBot does, as decided

Read from OpenBot at commit `6b2ccd1f2` (PolyForm Noncommercial: Oby copies the behaviour, never the code).

- **Permission level.** Each agent has one: Workspace only, Ask first or Full access. A new agent starts at Full access, which never asks (`packages/contracts/src/ipc-agents.ts:40`, `packages/contracts/src/ipc-agent-admin.ts:47-81`). Only the person widens an agent's level; an agent can only narrow it. An agent that reads mail acts on it without a click at Full access.
- **Memory.** The agent writes its own memories freely (`remember`, `forget`). A write is kept only when its turn completes. Essential memories, up to 8 KiB, go into every turn, marked as untrusted data, and the rest are searched. The person views, edits, clears and pins them, and a pinned memory is locked against the agent.
- **Browser and Computer Use.** A persistent browser inside the app that agents open and drive, and that the person watches, takes over and signs in to; secrets go in through a card, never through the model. Computer Use is on for every agent unless switched off for one ("absent means on", `agentComputerUseEnabled`), through a bundled `cua-driver`. A screen capture always asks.
- **Tools.** Every agent gets all of the app's own tools. Every MCP server the person adds goes to every agent, unless that server is limited to a list of agents (`DEFAULT_MCP_SERVER_ACCESS = { agents: null }`, `packages/contracts/src/ipc-mcp-access.ts`).

## How Oby keeps its non-negotiables under it

- **Vault secrets never reach an agent.** OpenBot keeps its secrets encrypted by the operating system. Oby's vault is a 0600 file in obid's home. So an agent's Full access is the whole Mac except obid's home and the other accounts' login folders, held by the kernel sandbox, never by a prompt. If the vault moves into the Keychain, that exception can narrow.
- **Keys never become files an agent reads.** OpenBot writes an API key the agent asked for to a file its commands read. Oby keeps the key or OAuth token in the vault, and obid makes the call for the agent, as OpenBot itself does for MCP OAuth.
- **The secret scan.** Everything obid sends for an agent (a brokered MCP call, an `oby_*` tool's request) passes the secret scan first. What an agent's own CLI sends by itself at Full access goes out as today's own-lane runs do, with no egress proxy.
- **Approvals come only from the owner's apps.** An agent's level is widened only by the owner's signed click in the Electron app. The agent may narrow its own.
- **Coding stays as it is.** An agent at Full access acts on its own work without a Confirm card. Jobs it starts in a project are coding jobs and follow the project's rules: an untrusted job's pull request still needs two reviews or the owner's click to merge.

## Notes

The factory's intake refuses a task it classifies as the agent's lane (NOTHARNESS-1 to 3). For the tasks above, the browser, Computer Use and an agent's memory are the dispatcher's, by this decision.
