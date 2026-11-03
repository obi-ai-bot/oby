# A task at work, live

The business behavior contract for a dispatched agent's work in the Electron app: its live card in the conversation and its own conversation beside the chat, from [the template](../business-contract-template.md).

## Feature and status

- **Outcome:** The owner sees what a dispatched agent is doing the way Claude Code shows it, and can follow it from the beginning and talk to it. In their words: "I just saw a chat that dispatches, but what happens with the agent that were dispatched? Where are the thinking, the information on it?" and "Showing the conversation of that agent would be better".
- **Status:** **Implemented** in the Electron app. obid already recorded and streamed all of this (`GET /v1/jobs/{id}/transcript`, the `transcript` event, `docs/runner-api.md`). One obid change, with nothing new in its API: a job's Claude Code now starts with `CLAUDE_CODE_ENABLE_TODO_TOOLS=1`, because headless 2.1.302 otherwise has no to-do tool at all, so no Claude Code job could ever make a list. The design is Wellington's canvas "Oby: the Sphere is the chat", boards "Oby at work" and "The agent's own conversation" (https://claude.ai/artifact/StYU4XCqZAnQ3t9z9yBDsd). **Proposed, Sangam's to change:** the button "Open the task's page" in the conversation's bar (the card used to open that page), the words "N done before these" and "N more to do" on a long list, "Show N earlier", "Thinking", "Thought", "Declined" and the note when a call's details weren't kept.
- **Scope:** The task's card in a conversation (`ThreadTask`, `WorkNow`), the agent's conversation (`AgentPane.tsx`), and reading transcript events (`obid-wire.ts`, `live.ts`). **Out of scope:** a helper agent's own conversation (it shows as one line); a side panel or Home change.

## Behavior

### Actors and ownership

- **obid** owns the transcript: what's recorded, what's masked, each entry's number and state. It decides nothing new here.
- **The app** shows what obid sent and invents nothing. It sends the owner's words and Stop as it does everywhere else.

### What each tool gives

| | To-dos | Thinking |
|---|---|---|
| Claude Code | Only if it made a list (`TodoWrite`, `TaskCreate`) | "Thought for 8s", no words: nothing to open |
| Codex | Only if it made a plan (`turn/plan/updated`) | A summary at most, maybe none |
| Cursor | Only if it made a list (`plan`, `cursor/update_todos`) | Its words, after the thought ends |

### States

| The task | Its card | Its conversation |
|---|---|---|
| Queued or running | "Running · tool · account · repository · 12m"; to-dos with the current one lit; the last three things it did, under the current to-do or alone if it made no list | Everything from the start, to-dos on top, Stop, "Tell Claude Code…" |
| Past the agent's work (check, publish) | The stage it's in, as a live line | As above |
| Needs the owner | "Needs you", what for, the current to-do orange | The decision, over the message box |
| Ended | One line: "Done", "Nothing done", "Stopped" or "Failed", and its commits, branch, pull request and time | Everything, no Stop, "Ask for a follow-up…" |

### Failures and recovery

- An event that didn't reach the app, or a lost connection: the session is fetched again at each of the job's events, from the last entry known with none missing before it, so a hole fills by itself.
- A fetch answered late never undoes an event: an ended call stays ended and words stay as far as they got.
- Telling the agent is refused: obid's own words show, as for any tell (`say`).
- The job is deleted while its conversation is open: the conversation closes.

### Invariants and boundaries

- Only a job something shows is kept in memory, let go when the last thing showing it goes, and an agent's every line redraws only the lines that changed. A long session shows its last 300 lines, with earlier ones on asking.
- Everything shown is the transcript's own, masked by obid. A call's output and a file's contents come only when its line is opened (LIVETASK-11).
- `Job`, the phones' API and its enums are untouched.

## Interfaces and dependencies

Reads `GET /v1/jobs/{id}/transcript`, `GET /v1/jobs/{id}/transcript/{n}` when a line opens, a job's `reason`, and the `transcript` event; sends `POST /v1/jobs/{id}/messages` and `POST /v1/jobs/{id}/stop`. No new route, key or value in obid's API.

## Acceptance scenarios

Tests: `apps/electron/src/renderer/src/test/liveTask.test.tsx` and `taskActions.test.ts`, and through the built app with real input `npm run test:live-task` (`scripts/test-live-task.mjs`); the real-Mac steps are below.

- **LIVETASK-1:** A running task whose agent made a list: its card shows the list with the current to-do lit, and its last three steps under it. A long list shows the part around the current one.
- **LIVETASK-2:** A running task whose agent made no list: its card and conversation show no to-do, only its steps; with no steps yet, what obid says it's on.
- **LIVETASK-3:** The `transcript` event is read. A followed job's entries update by `n` without a fetch, its lines add to its words, and a missed entry is fetched. A job nobody shows keeps nothing; a deleted job's session goes.
- **LIVETASK-4:** An ended task's card is one line that says how it ended, with its branch and pull request: Done, Stopped and Failed each read as what happened.
- **LIVETASK-5:** A click on the card opens the agent's conversation beside the chat: what it was told, its words with their time, its steps, its thinking, its to-dos on top once, Stop while it runs, close, the task's page and the box.
- **LIVETASK-6:** Thinking shows only what its tool gave: "Thinking" while it goes on, "Thought for 8s" with no words, a tool's own words when it gave any, never a made-up time.
- **LIVETASK-7:** The box sends `POST /v1/jobs/{id}/messages`; told after it finished, the follow-up it started is what the conversation then follows.
- **LIVETASK-8:** In a narrow window the conversation lies over the chat; otherwise beside it.
- **LIVETASK-9:** A task that needs the owner says so and what for on its card, shows no running sheen, and carries the decision in its conversation.
- **LIVETASK-10:** A step reads as what it's doing while it runs ("Reading a.ts") and as what it did once it ended ("Read a.ts"); a call refused says "Declined", apart from one that failed.
- **LIVETASK-11:** A tool line opens to what it ran and gave back, from `GET /v1/jobs/{id}/transcript/{n}`: a command and its output, an edit's diff, a read's lines, a search's matches.
- **LIVETASK-12:** A stopped task says why on one line, in obid's `reason`, before the agent's last words.
- **LIVETASK-13:** One to-do list, the newest: each version replaces the one before, in Full session too.
- **LIVETASK-14:** A job's Claude Code has its to-do tools, and they never ask the owner or show as a step or as the job's `step`; a deleted to-do keeps the numbers of the rest (obid: the stand-in `fake-claude` refuses a job without them, so every test in `jobs_api` proves it; `permissions` and `transcript`'s tests).
- **LIVETASK-15:** An opened tool line stays that call's as the session changes. The conversation closes when its job is deleted. A session nothing shows any more is let go, and a long one doesn't redraw unchanged lines.

## Test it when it's done (real Mac)

Dispatch a short task on Claude Code and on Codex. The card shows its steps as they happen, and its to-dos with the current one lit if the agent made a list. A click opens its conversation beside the chat; a narrow window shows it over the chat; Esc closes it. Open a tool line to see its output. Tell it something from the box. When it ends, the card folds to one line with its branch or pull request.
