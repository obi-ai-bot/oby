# A task lives in its conversation: the card is progress, the details are messages

- **Date:** 2026-11-17
- **Status:** In force
- **Replaced:** A task's own thread over its conversation (LIVETASK-22 as of 2026-11-16, `TaskThread.tsx` around `AgentThread.tsx`), opened from its card, its nested row, Waiting for you and its alert; the task's page from the card's menu (LIVETASK-18, CHATFIRST-6, "View details"); an alert's click opening the task's page
- **Changed by:** [2026-11-23-a-task-opens-its-own-thread.md](2026-11-23-a-task-opens-its-own-thread.md): its card and its nested row open its own thread again
- **Contracts and scenarios:** [live-task.md](../contracts/live-task.md) (LIVETASK-1, -2, -5, -7, -9, -12, -15 to -18, -22, -26 to -31), [home.md](../contracts/home.md) (WAITING-2), [alerts.md](../contracts/alerts.md) (ALERTS-18, ALERTS-20), [chat-first.md](../contracts/chat-first.md) (CHATFIRST-6, CHATFIRST-34)

## The owner's words

The plan, as the session that started this work put it to the owner on 2026-11-17, and the owner's answer, "Yes exactly":

1. The old full-screen task view goes. Nothing opens it: not a notification, not Waiting for you, not a card or a nested row.
2. Every route leads to the conversation that started the task, scrolled to that task's card.
3. The task card is only progress: its head (title, agent, repository, state) and the stages bar.
4. The details live in the conversation: what the agent did, what changed, the test run, the result, and the decision bar (Merge / Leave it open / Later). They appear there as messages, the way T3 Code's thread is the session's work itself.

He wants it as simple as T3 Code.

## Where it is stated

- [live-task.md](../contracts/live-task.md), "States", LIVETASK-28 to 31 and the scenarios they changed
- [alerts.md](../contracts/alerts.md), ALERTS-20
- [home.md](../contracts/home.md), WAITING-2

## Notes

Think of a group chat where you asked a colleague to do something. You don't get sent to a separate room to see how it went: their updates land in the same chat, under your request, and when you come back from a notification the chat opens where they answered. The card is the colleague's status badge, nothing more.

### What it is now

- **One place.** A task's card sits in the conversation that started it, and what it did sits right under the card, outside it, as part of the conversation (`TaskLog.tsx`): its newest to-do list and its last 4 lines while it works (earlier ones behind "Show N more", as T3 Code folds its work log), and once it ends its last words whole (what changed, the test run, the commit, the result), long words cut behind "Show more".
- **Every route lands there.** An alert's click, Waiting for you's Open and Answer, a nested row, Home's stack and a task row's menu all open that conversation, scroll to the card and light it a moment (`Home.tsx`, `openAt`; `ChatView.tsx`, `flash`). A task with no conversation (from GitHub, another Oby) opens its own thread, which is the same view: the card, what it did under it, its box talking to its agent. That is where such a task is already listed and opened today, so it needs no new place.
- **Steering and Stop.** The card's ⋯ (or a right-click) offers "Tell Claude Code…" (ended: "Ask for a follow-up…"), Stop task and Delete task. "Tell…" puts a chip "To Claude Code · <task>" over the conversation's own box, which then tells that job alone (LIVETASK-17), with Stop in it while the task works; sent, or the chip's ×, and the box is Oby's again. This is T3 Code's one composer, pointed at a child for one message.
- **Decisions.** Unchanged from [2026-11-17-decisions-like-t3-code.md](2026-11-17-decisions-like-t3-code.md): one drawer over the box. Opening the conversation at a task brings back what Later put off there; an answered decision leaves its one line under the card.
- **Unchanged.** Sangam's earlier Home (`?home`) keeps its task page (`TaskView.tsx`), its own design; the dev control center's `oby:open` still opens it. Nothing changes in obid or its API.
