# A task's card and its nested row open the task's own thread again

- **Date:** 2026-11-23
- **Status:** In force
- **Replaced:** In part, [2026-11-17-tasks-live-in-the-conversation.md](2026-11-17-tasks-live-in-the-conversation.md): its card and its nested row no longer open the conversation at the card
- **Changed by:** Nothing
- **Contracts and scenarios:** [live-task.md](../contracts/live-task.md) (LIVETASK-22, -24, -28, -29, -31), [chat-first.md](../contracts/chat-first.md) ("A task")

## The owner's words

On 2026-11-23, in the installed build, clicking a task's card or its nested row only lit the card: "it only shows an orange line around the task but nothing happens", and "I had that working before: when I clicked on the task card it used to open, select in the sidebar the task that was running, and I could see the messages from the agent."

## Where it is stated

- [live-task.md](../contracts/live-task.md), LIVETASK-22, -24, -28, -29 and -31
- [chat-first.md](../contracts/chat-first.md), "A task"

## Notes

Think of a group chat where you asked a colleague to do something: their status badge sits in the chat, and tapping it opens your one-to-one with them, where you read everything they did and talk to them; back takes you to the group.

### What it is now

- **Its card and its nested row open its own thread.** Over the conversation that started it (back returns there): its request, its card, every line it did as it happens (nothing folded behind "Show N more"), and its box telling its agent, with Stop while it works and its own decision over the box. Its nested row is lit while that thread shows; from the keyboard, the card's title is the button that opens it; deleted while it shows, the thread goes, back to the conversation. This is the task's own thread a task in no conversation already had (`ChatView` with `kind="task"`), so no view came back: `TaskThread.tsx` and `AgentThread.tsx`, deleted by #2477, stay deleted.
- **Unchanged.** An alert's click, Waiting for you's Open and Answer, Home's stack of asks and a task row's menu still open the conversation at its card, lit. What it did still shows under its card in the conversation, its last 4 lines while it works; its ⋯ menu is as it was.
- **What broke it.** #2477 (`14a613686`, 2026-11-18) made every way to a task open the conversation at its card, as the owner's word of 2026-11-17 asked; the card's click and the nested row went with it.
