# A task's decision waits over the message box, the way T3 Code shows its own

- **Date:** 2026-11-17
- **Status:** In force
- **Replaced:** LIVETASK-9 and LIVETASK-16's decision under the card's head, answered there (Sangam's screen 04), and the decision at the end of a task's own thread; a task decision's Later, which put it off for an hour in obid and hid its buttons, its row in Waiting for you and its place everywhere
- **Changed by:** Nothing
- **Contracts and scenarios:** [live-task.md](../contracts/live-task.md) (LIVETASK-9, LIVETASK-16, LIVETASK-24 to LIVETASK-27), [home.md](../contracts/home.md) (WAITING-1, WAITING-4), [chat-first.md](../contracts/chat-first.md) (CHATFIRST-8, CHATFIRST-23)

## The owner's words

> Those blocks are horrible: they should disappear, and the card stay lean at the top once the response is done.

The owner's words of 2026-11-17, as the session that started this work relayed them. Not an exact quote.

> Once you click Later there is no way to get back to the decision. I want decisions shown the way T3 Code shows its approvals and questions.

The same.

## Where it is stated

- [live-task.md](../contracts/live-task.md), "States" and LIVETASK-9, LIVETASK-16, LIVETASK-24 to LIVETASK-27
- [home.md](../contracts/home.md), WAITING-1 and WAITING-4
- [chat-first.md](../contracts/chat-first.md), CHATFIRST-8 and CHATFIRST-23

## Notes

Think of a desk with a single in-tray at the front. Before, every task card in a conversation grew its own sticky note (its decision, two or three lines of why, and its buttons) wherever the card happened to sit in the thread; "Later" threw the note in a drawer for an hour, and the drawer had no handle. Now the note sits in the one tray at the front of the desk, over the message box, one line long; "Later" only takes it off the tray, and the task's own row, its conversation's mark, its own thread and Waiting for you all still point at it until it's answered.

### How T3 Code does it (`apps/web/src`, read 2026-11-17)

- **Where.** A pending user-input question is a drawer attached to the top of the composer (`ChatComposer.tsx`, `ComposerPendingUserInputPanel.tsx`): one header line, clickable to fold it to "header | question", a "2/3" count when there are several, the options under it, the text area below it for a typed answer. An approval is a compact row at the end of the timeline (`TimelinePendingApprovalRow.tsx`): the command in a code block, then one line "Waiting for approval · Decline · Approve" with Cancel and "Always allow this session" behind More. An approval from a **child thread** (our nested task) is a card stacked over the parent's composer, "Approval needed from {title}" (`ChildThreadApprovals.tsx`).
- **Reachable.** There is no Later. What's pending is the server's (`runtimeRequests` with status `pending`), so leaving the thread and coming back shows it again; a question is dismissible only when its provider says so. Folding the drawer is component state.
- **Answered.** One work-log line stays: "Approval requested" with its status, or "Answered questions" with the answer.
- **Side panel.** `resolveThreadStatusPill` puts "Pending Approval" (amber) and "Awaiting Input" over Working, Completed and the rest; a closed folder shows an attention dot.
- **Composer.** It stays on: typing while an approval waits declines it and steers the run.

### What Oby showed before this change

| Kind | Where | Words and buttons | Later |
|---|---|---|---|
| A task's permission | Under the card's head in the conversation (`ThreadTask`, `msg-task-ask`), and again at the end of its own thread (`lt-thread-ask`) | "Permission", the headline, obid's text, the command as a code block; Allow once, Always allow, Deny, Later | Renderer state (`answered`, an hour) and obid's `later` (`POST /v1/inbox/{id}/answer` `{"later": +1h}`): the buttons went for an hour, and the item left Waiting for you (its `state` became `later`) |
| A task's merge card | The same two places | "Ready to merge" / "Not merged", the headline, every hold on its own line ("No check ran on its head…", "Its base is main…"); Merge, Dismiss, Later | The same |
| The agent's question | The same | "Question", the question, its options, Tell it something (a field), Later | The same |
| Oby's escalation (a stopped or stalled task) | The same | Its text whole, the "Tell it what to do" box, Done, Later | The same |
| A failure, a review out of rounds | The same | obid's words, its options, Later | The same |
| A repository's check (`set_check` Confirm) | The same, or in Oby's answer when it has no task | "Check", the question, the command; Use it, Edit, No check / Keep it, Later | The same |
| An account's limit | At the end of the conversation, after every message (`msg-ask`) | "Out of usage", its options, Later | Renderer only (`limitLater`, an hour, Home's stack too) |
| A Confirm Oby's answer made, an offer to start, a setup step, a question to another Oby | Inside Oby's message that made it | Their own cards | Confirm and offer: obid's `later` |
| Every task's decision, again | Home's ask stack, floating over whatever conversation is open (`asks-over`, CHATFIRST-8): the task's title, "1/1", Open task, then the whole block | As on the card | obid's `later` and the renderer's, as above |
| Oby's own question back | Already over the message box (`chat-ask`) | The question, its picks, "Or answer in your own words below" | None |

### What it is now

- **The card is lean.** A task's card in the conversation is its head only: title, "Needs you · Claude Code · oby-scratch", the stages bar with the stage it waits on orange. Nothing grows under it.
- **One drawer over the message box.** The decision a task waits on is a drawer attached to the top of the message box, where T3 Code puts its child thread's approvals and its questions: in the conversation, every decision of the tasks it started and its account limits; in a task's own thread, that task's. One line: its kind, its headline (in the conversation, "from" the task's title), a "1 of 3" count with the next one's arrow when there are several, Details when there is more to read, then its buttons and Later. obid's text and a check's command sit behind Details; a permission's command shows on one line under it, as T3 Code shows the command it approves. An escalation's "Tell it what to do" box and Oby's ask to share (its repositories to tick) show open, since they are what is answered.
- **Later only hides it from that spot.** It is the drawer's own state: no obid call, no `answered` record. The task still needs the owner: its card says "Needs you", its nested row and its conversation keep the orange mark, opening the task (or the conversation again) shows the drawer again, and Waiting for you lists it, whatever an earlier build or another device put off in obid.
- **Answered, one line stays.** In the task's own thread a line "Permission · Allow once" (its kind and what was chosen) stays where the decision was asked, until obid asks something else; the conversation keeps nothing but the card.
- **No second copy over the conversation.** The ask stack chat first floats over an open conversation keeps only what has no task (a Confirm, GitHub's offer, another Oby's question); a task's decision is in its own conversation's drawer, its row's orange mark and Waiting for you. With no conversation open, Home's stack still holds every decision, as before.
- **Unchanged.** Home's stack and Waiting for you keep their own cards and Later for what has no task (another Oby's question, a Confirm of its own, GitHub's offers); a Confirm, an offer, a setup step and a question to another Oby stay in the Oby message that made them; Oby's own question back stays over the message box. Sangam's later screen for the drawer, if he draws one, wins over this.

### The plan, as built

1. `AskBlock` (TaskCard.tsx) gains a `bar` shape: the kind and headline on one line, the rest behind Details, Later calling the drawer's own `onLater`.
2. `DecisionBar.tsx`: the drawer, holding the pending decisions it's given, one shown at a time with "n of N", and the keys put off in this spot (component state, gone when the view closes).
3. `ChatView.tsx`: `ThreadTask` loses its decision block; `Home.tsx` passes `tasks={false}` to the stack over a conversation (`AskStack.tsx`); the conversation's drawer takes its tasks' decisions and its limits, a task-from-elsewhere's decision too, in place of the thread's `ask` entry and the limits after the thread. `AgentThread.tsx`: the drawer over its box, the one-line record where the decision was.
4. `live.ts`: `waitingOf` keeps a task's decision put off in obid and the renderer's old Later; nothing else in obid's API or the app's other answers changes.
5. live-task.md and home.md say what is now true, with LIVETASK-24 to 27 and WAITING-4 and tests named after them; `npm run test:live-task` walks a real permission through the drawer.
