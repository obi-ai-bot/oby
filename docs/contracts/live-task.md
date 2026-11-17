# A task at work, live

The business behavior contract for a dispatched agent's work in the Electron app: its live card in the conversation, its row nested under that conversation in the side panel, and its own thread, a conversation of its own that either opens, from [the template](../business-contract-template.md).

## Feature and status

- **Outcome:** The owner sees what a dispatched agent is doing the way Claude Code shows it, and can follow it from the beginning and talk to it. In their words: "I just saw a chat that dispatches, but what happens with the agent that were dispatched? Where are the thinking, the information on it?" and "Showing the conversation of that agent would be better".
- **Status:** **Implemented** in the Electron app. obid already recorded and streamed all of this (`GET /v1/jobs/{id}/transcript`, the `transcript` event, `docs/runner-api.md`). One obid change, with nothing new in its API: a job's Claude Code now starts with `CLAUDE_CODE_ENABLE_TODO_TOOLS=1`, because headless 2.1.302 otherwise has no to-do tool at all, so no Claude Code job could ever make a list. The design is Wellington's canvas "Oby: the Sphere is the chat", boards "Oby at work" and "The agent's own conversation" (https://claude.ai/artifact/StYU4XCqZAnQ3t9z9yBDsd), and screens 03 and 04 of `docs/design/oby-home/`. The nested rows, the card's head and the task's own thread follow page 2 of the canvas "Oby Factory On and Off" (https://claude.ai/artifact/WVGbVtqZBEsVWH6p3Nxvxm), screens 03 and 04; where that page draws the card opened in place, the owner's word of 2026-11-16 holds: a task opens as a session of its own, never inline. Where screen 04 draws the decision under the card's head, the owner's word of 2026-11-17 holds: the card stays its head alone and the decision waits in a drawer over the message box, as T3 Code shows its approvals and questions ([why](../decisions/2026-11-17-decisions-like-t3-code.md)). **Proposed, Sangam's to change:** the drawer's words "Details", "1 of 3", "Next decision" and the answered line ("Permission · Allow once"). **Proposed, Sangam's to change:** "View details" in the card's menu (a right-click: the opened card has no title bar), the words "N done before these" and "N more to do" on a long list, "Show N earlier", "Thinking", "Thought", "Declined" and the note when a call's details weren't kept.
- **Scope:** The task's card in a conversation (`ThreadTask`, `TaskHead` in `ChatView.tsx`), the drawer of decisions over a message box (`DecisionBar.tsx`, `AskBlock`'s `bar` in `TaskCard.tsx`), its row nested under the conversation in the side panel (`Sidebar.tsx`, `nested.ts`), the task's own thread (`TaskThread.tsx` around `AgentThread.tsx`), where typed words go (`replyTarget.ts`, modelled in `docs/models/reply_target.qnt`), and reading transcript events (`obid-wire.ts`, `live.ts`). **Out of scope:** a helper agent's own conversation (it shows as one line); Home's stage.

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

| The task | Its card in the conversation | Its row under the conversation | Its own thread |
|---|---|---|---|
| Queued or running | "Running · tool · repository"; what it's on now as a live line ("Work · Running cargo test"); its stages as a bar | Its agent, **its title** · what it's on, a working dot | Everything from the start, to-dos on top, "Tell Claude Code…" with Stop in it |
| Past the agent's work (check, publish) | The stage it's in, in the live line | As above | As above |
| Needs the owner | Its head alone: "Needs you · Claude Code · oby-scratch", the stage it's on orange; the decision waits in the drawer over the conversation's message box, "from" its title | **Its title** · what it asks ("asks to run a command"), an orange dot; the conversation's line says it too ("Codex asks to run a command") | The decision in the drawer over its box, then "Tell Claude Code…" with send; answered, one line where it was asked |
| Ended | One line: "Done", "Nothing done", "Stopped" or "Failed", and its commits, branch, pull request and time | **Its title** · Done (a check) or Failed (a red dot) | Everything, no Stop, "Ask for a follow-up…" |

A card never opens in place: a click on it, or on its row, opens the task's own thread over the conversation, and back returns to the conversation. A decision never shows in the thread or under a card: it waits in the drawer over the message box until it's answered, Later taking it off that drawer only (LIVETASK-24 to 27). The message box at the foot of a conversation with Oby talks to Oby; the box in a task's own thread talks to that task's agent.

### Failures and recovery

- An event that didn't reach the app, or a lost connection: the session is fetched again at each of the job's events, from the last entry known with none missing before it, so a hole fills by itself.
- A fetch answered late never undoes an event: an ended call stays ended and words stay as far as they got.
- Telling the agent is refused: obid's own words show, as for any tell (`say`).
- The job is deleted while its thread is open: its card, its row and its thread go with it, back to the conversation.

### Invariants and boundaries

- Only a job something shows is kept in memory, let go when the last thing showing it goes, and an agent's every line redraws only the lines that changed. A long session shows its last 300 lines, with earlier ones on asking.
- Everything shown is the transcript's own, masked by obid. A call's output and a file's contents come only when its line is opened (LIVETASK-11).
- `Job`, the phones' API and its enums are untouched.

## Interfaces and dependencies

Reads `GET /v1/jobs/{id}/transcript`, `GET /v1/jobs/{id}/transcript/{n}` when a line opens, a job's `reason`, and the `transcript` event; sends `POST /v1/jobs/{id}/messages` and `POST /v1/jobs/{id}/stop`. No new route, key or value in obid's API.

## Acceptance scenarios

Tests: `apps/electron/src/renderer/src/test/liveTask.test.tsx`, `decisions.test.tsx` and `taskActions.test.ts`, and through the built app with real input `npm run test:live-task` (`scripts/test-live-task.mjs`); the real-Mac steps are below.

- **LIVETASK-1:** A running task's card says what it's on now in one line (its stage and step) and shows its stages as a bar; it holds no list. Its own thread shows the agent's list on top with the current to-do lit, and its steps. A long list shows the part around the current one.
- **LIVETASK-2:** A running task whose agent made no list: its card and thread show no to-do; its card says what obid says it's on, and nothing when obid says nothing.
- **LIVETASK-3:** The `transcript` event is read. A followed job's entries update by `n` without a fetch, its lines add to its words, and a missed entry is fetched. A job nobody shows keeps nothing; a deleted job's session goes.
- **LIVETASK-4:** An ended task's card is one line that says how it ended, with its branch and pull request: Done, Stopped and Failed each read as what happened.
- **LIVETASK-5:** A click on the card opens the task's own thread (LIVETASK-22), never the card in place: under its head as the card shows it, what the agent was told, its words with their time, its steps, its thinking, its to-dos on top once, and its box ("Tell Claude Code…") with Stop in it while it runs.
- **LIVETASK-6:** Thinking shows only what its tool gave: "Thinking" while it goes on, "Thought for 8s" with no words, a tool's own words when it gave any, never a made-up time.
- **LIVETASK-7:** The thread's box sends `POST /v1/jobs/{id}/messages`; told after it finished, the follow-up it started opens as its own thread.
- **LIVETASK-8:** Retired by LIVETASK-5 (2026-11-15): the agent's conversation opens in place under its card, so there's no side for it to lie over.
- **LIVETASK-9:** A task that needs the owner says so on its card ("Needs you", its orange mark), with no running sheen and no decision under it; its own thread shows the decision in the drawer over its box (LIVETASK-24), and its box has no Stop.
- **LIVETASK-10:** A step reads as what it's doing while it runs ("Reading a.ts") and as what it did once it ended ("Read a.ts"); a call refused says "Declined", apart from one that failed.
- **LIVETASK-11:** A tool line opens to what it ran and gave back, from `GET /v1/jobs/{id}/transcript/{n}`: a command and its output, an edit's diff, a read's lines, a search's matches.
- **LIVETASK-12:** A stopped task says why on one line, in obid's `reason`, before the agent's last words.
- **LIVETASK-13:** One to-do list, the newest: each version replaces the one before, in Full session too.
- **LIVETASK-14:** A job's Claude Code has its to-do tools, and they never ask the owner or show as a step or as the job's `step`; a deleted to-do keeps the numbers of the rest (obid: the stand-in `fake-claude` refuses a job without them, so every test in `jobs_api` proves it; `permissions` and `transcript`'s tests).
- **LIVETASK-15:** An opened tool line stays that call's as the session changes. A card, and its decision, go with its job when the job is deleted. A session nothing shows any more is let go, and a long one doesn't redraw unchanged lines.

- **LIVETASK-16:** No card shows a decision, whether it waits on the owner, works or has ended, and no card opens in place.
- **LIVETASK-17:** Words typed in a task's own thread reach that job and nothing else, and words typed in the message box of a conversation with Oby reach Oby and no job, whatever card needs the owner (`replyTarget` in `replyTarget.ts`; the model `docs/models/reply_target.qnt`).
- **LIVETASK-18:** The task's page opens from the card's menu (a right-click), View details, and back returns to the conversation.
- **LIVETASK-19:** Under each conversation in the side panel, the tasks it started, each a row of its own, oldest first, archived ones left out: its agent's mark, **its title** · what it's on (its step, "Queued"), what it asks ("asks to run a command", "has a question") or how it ended ("Done", "Failed"), and its state's mark at the right (working, needs you, done, failed); hovered, its title and the account it runs on. A conversation with no task has no twist (`nested.ts`; `nested.test.tsx`).
- **LIVETASK-20:** A conversation's rows show for the open conversation, the one holding the open task and one whose task needs the owner; otherwise folded. Its twist folds and shows them, and the owner's choice is kept (`oby.kidsShown`) over the default.
- **LIVETASK-21:** A conversation's second line says what waits on the owner first ("Codex asks to run a command"), else how many of its agents work ("2 agents working"), else its own line; its mark is orange or working accordingly.
- **LIVETASK-22:** A nested row, or a card, opens that task's own thread over the conversation that started it, the trail reading "conversation › task"; its row is lit while it shows, and back returns to the conversation (`TaskThread.tsx`).
- **LIVETASK-23:** A conversation with tasks hovers, lights and lays out exactly like one without: the same row box (edges, radius, padding), no class or offset of its own on it. Every conversation row keeps a fixed gutter on its left, so titles line up; the twist sits in that gutter, inside the row's box, with no box of its own (only its colour answers the pointer), and looks the same idle, hovered, open and needing you, folded or not. The nested rows keep their indent and tree line and hover alike (`Sidebar.tsx`, `app.css`; `nested.test.tsx`).
- **LIVETASK-24:** What a conversation's tasks wait on the owner for, and an account's limit stopping its work, wait in a drawer on top of its message box (`DecisionBar.tsx`); a task's own thread has its own decision there, over its box. One shows at a time, on one line: its kind ("Permission", "Ready to merge"), its headline and, in a conversation, " · " and the task's title; "1 of 3" with the next one's arrow when there are several; Details when there is more (obid's text, a merge's holds, a check's command), which opens it; a permission's command on one line under it; then its options and Later. An escalation's "Tell it what to do" box and a Confirm's repositories to tick show as they are, since they are what's answered. Given a conversation whose task asks to run `npm test`, when it shows, then the drawer is after the thread and before the box, its line "Permission Run npm test? · Run the tests", its holds not shown until Details. Tests: `LIVETASK-24: a conversation's pending decision waits over its message box…` and `LIVETASK-24: several pending decisions show one at a time…` (`liveTask.test.tsx`).
- **LIVETASK-25:** The card stays lean: a task's card in the conversation is its head alone (its title, "Needs you · Claude Code · oby-scratch", its stages with the one it waits on orange), whatever it waits on, and nothing grows under it while the decision waits or once it's answered. Given a merge decision ("Merge oby-scratch #4 into main?", "No check ran on its head…", "Its base is main…"), when the conversation shows, then its card holds none of those words or buttons. Test: `LIVETASK-25: the card stays lean…` (`liveTask.test.tsx`).
- **LIVETASK-26:** A postponed decision is still reachable. Later takes it off the drawer it's in, and only while that view is open: nothing is sent to obid, the task still needs the owner (its card says "Needs you", its nested row and its conversation keep the orange mark and their line, "is ready to merge"), Waiting for you lists it (WAITING-4), and opening the task or its conversation again shows it in the drawer again. A decision put off in obid (`later`, from another device or an earlier build's Later) still waits in the drawer with its options. Given two waiting decisions, when the owner clicks Later on the first, then the second shows alone, no call reaches obid, and opening the view again shows "1 of 2" with the first. Tests: `LIVETASK-26: Later takes it off the drawer only…` and `LIVETASK-26: a decision put off in obid…` (`decisions.test.tsx`).
- **LIVETASK-27:** Answered, a decision leaves one line in the task's own thread where it was asked: its kind and the answer ("Permission · Allow"), until obid asks something else; the conversation keeps nothing but the card. A decision put off leaves no line: it still waits. Test: `LIVETASK-27: answered in its own thread, one line stays…` (`liveTask.test.tsx`).

## Test it when it's done (real Mac)

Dispatch a short task on Claude Code and on Codex. The card shows its steps as they happen, and its to-dos with the current one lit if the agent made a list. A click opens its own thread over the conversation. A task that needs you stays its card's head ("Needs you"), and its decision waits over the message box on one line; Later takes it off, and the row, Waiting for you and opening the task bring it back. Open a tool line to see its output. Tell it something from the card's box, and Oby something from the message box at the foot. Right-click the card and View details opens the task's page. When it ends, the card folds to one line with its branch or pull request.
