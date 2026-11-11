# Chat first: the app opens on your Oby's conversation

## Feature and status

- **Outcome:** The owner opens Oby and is in a chat with their own Oby, the message box ready. Their Oby talks to other Obys for them; they don't have to learn tasks, Home or kinds of work first.
- **Status:** Implemented in the Electron app, `apps/electron`. The owner's decision of 2026-11-02: "100% Obysphere-centric, just a chat", "I interact with my Oby on my conversation". On that day's call Dima and Sangam couldn't tell a task from a conversation. It overrides Sangam's Home as the first screen for now, on the owner's word; his pieces and words stay wherever they fit, and his Home stays in the code to compare. The concept: the canvas "Oby: the Sphere is the chat", https://claude.ai/artifact/StYU4XCqZAnQ3t9z9yBDsd.
- **Scope:** Where the app opens, what the side panel lists, and where what Home held went. Outside it: a task's live view, Obys asking Obys (their own work), and the canvas's other screens.

## Behavior

- **Actors and ownership:** Only the app changes. obid keeps every conversation, task and ask as it did; this feature adds no API. What its inbox says decides the dots; the app only reads it.
- **Triggers:** The app opening, once setting up is done and obid's list of conversations is in.
- **States:**
  - **Your conversation:** where you come to rest, its box focused. It's the one that moved last by obid's `lastActivity` when the app opens, then whichever you're in. Esc and the back arrow have nothing to leave it for.
  - **The empty chat:** no conversation open: Oby's character, its line, "What do you want me to work on?" with no name and whatever is running, what Oby is asking you, and the box (Home's stage, without the chips). Words sent there start a conversation, which opens. It's a resting place too, reached from Oby's row or New conversation.
  - **A page over it:** a task, a kind of work, Settings, an Obysphere, a teammate's Oby. Back returns to where you were resting.
- **The side panel** (the canvas's Chats and Sphere boards, the owner's decision of 2026-11-03; no profiles yet, so no circle, no name under Chats, and no Connectors or Automations until their pages exist):
  - **The icons:** Chats; a line and each Obysphere's tile, with no line when there's none; Settings at the foot. They only open: the panel's toggle closes it. Chats from a page (a thread, Settings) also brings your conversation back. A tile opens its column (Knowledge, Invite member, Members), and its room when it has one.
  - **Chats:** Oby's row, which opens the empty chat, New conversation, then every conversation in one scrolling list, pinned ones first. Search on the panel's top row covers all of it: titles, and the words of those loaded.
- **Where each thing went:**
  - **A task:** a card in its conversation, as before, with its detail view (the card, then Open the task's page), an alert's click, and a task's Go to conversation all as they were. Its own list, left, is hidden, not deleted: `?home` or `oby.home` on obid, `?everything&chat` on the demo.
  - **Archived:** at the foot of Chats once something's in it, with Archive completed's choice still in force (the choice's menu went with the list: `?home`). A done task with no conversation to hold its card (one from before every task had one, or whose conversation was deleted) is there too.
  - **Review PR and Issues (GitHub's To do):** went with the list, behind `?home`. What obid offers from them is on the stack, below. An item no one's started lights nothing: there's nowhere left to click.
- **What waits on you** shows, and is answered in one click, in the same place whatever it is: the stack ("Oby is asking"), over any conversation and on the empty chat. Over any other page it waits for you to come back, with the dots saying so. The orange dot always leads somewhere: on a row, and on the Chats icon while any row has one. The row you're on stays in view however you came back to it; Chats, with the chats showing, brings the first row that waits into view, however far down.

  | What waits | Its card | Its dot |
  |---|---|---|
  | A task's permission, plan, question or merge (`permission`, `plan`, `agentQuestion`, `question`, `merge`) | The stack, and its card in its conversation | Its conversation's row |
  | A task that failed or was cut off | The same | Its conversation's row |
  | A task's, with no conversation | The stack | Oby's row |
  | Oby's question back, its answer to your words | Over that conversation's box | Its conversation's row |
  | A conversation's own Confirm; an offer to start work | The stack, with Open conversation, and in its conversation | Its conversation's row |
  | A Confirm with no conversation, the offer to share (PEER-16) among them; a GitHub offer; an Oby failure | The stack | Oby's row |
  | An account's limit | The stack, and in its tasks' conversations | Each of those rows, and Oby's |
  | A question only you can answer (`obiRequest`, ASK-18) | In "Questions from other Obys", where you tell Oby what to say; its Confirm then goes as a conversation's | That row |

  Later takes a card off the stack until its time, then it's back, by itself and across a restart (a limit's only within the window's life: obid keeps no Later for it). A Confirm or an offer goes quiet meanwhile; a task's dot stays on its row, as it still needs you, its card there saying Later; one with no row is quiet until its time. Settled elsewhere meanwhile, its dot goes. The first load brings the job of everything open, however old: past the latest 100 obid lists, its ask would show nowhere.
- **Known gap, as before this change:** a teammate's review request (`obiRequest`, "Review request from …") has no card in the Electron app. It notifies, and a click lands on your conversation with nothing to answer. obid's quoted words must show whole before it's answered, which the stack's clamped text doesn't, so it needs a card of its own.
- **Known ceilings:** a task still running with no conversation shows nowhere, with no Stop, until it ends; deleting "Questions from other Obys" while a question waits leaves that question with no row.
- **Failures and recovery:** With obid away the splash and the connection line stand as before. A conversation deleted while it's open, or one a page was opened from, goes to your latest conversation, or the empty chat if none is left. A task deleted while its page is open goes back to where you were resting.
- **Invariants:** No conversation is created by opening the app. Where it opens is decided once, as it opens: a conversation that arrives later never moves you. An alert for what's on screen goes once the window is in front, so one for an ask with no task goes when you're on a conversation too (with `?home` as well, where a conversation has no stack: a known ceiling of the comparison). `?everything` shows Sangam's design as it was. Sangam's Home stays reachable: `?home`, or `oby.home` = `1` in the window's local storage. `?everything&chat` shows this on his demo data, Obysphere and teammates included.

## Interfaces and dependencies

None in obid. A member of a live Obysphere shows in the panel and opens their thread (docs/contracts/peer-thread.md).

Proposed words, Sangam's to change: "Chats" (the panel's title, from the canvas), "Back" (the arrow's label). The rest are his or the canvas's: Oby, New conversation, No conversations yet, Tasks, "Ernesto's Oby", "Accounts and how much of each is used". The canvas's "Bring your team in" row isn't here: it asks for a field to paste an invite, which the app doesn't have yet.

## Acceptance scenarios

Tests: `apps/electron/src/renderer/src/test/chatFirst.test.tsx`, `test/leftPanel.test.tsx` (from obid's answers to the rendered panel and stack) and, driving the built app with real input on a scratch obid, `apps/electron/scripts/test-chat-first.mjs` (`npm run test:chat-first`).

- **CHATFIRST-1:** With conversations, the app opens on the one that moved last, its box focused, with no Home behind it. An old conversation with a card still open doesn't count as having just moved.
- **CHATFIRST-2:** With none, it opens on the empty chat: the character and the box, no chips. Words sent there open their conversation. A conversation that arrives meanwhile, from the iPhone say, doesn't move you off it.
- **CHATFIRST-3:** The side panel, open the first time where it fits, lists Oby first, then New conversation. It shows no usage: that's in the message box, its gauge and its picker's list (the owner's decision of 2026-11-03).
- **CHATFIRST-4:** New conversation is one click from the side panel, and lands on the empty chat with its box focused. So does Oby's row.
- **CHATFIRST-5:** A tile opens the Obysphere's column, and its room only chat first; a teammate's Oby opens from its row, and Chats brings you back to your conversation.
- **CHATFIRST-6:** A task opens from its card in the conversation, and back returns to that conversation.
- **CHATFIRST-7:** Retired with the Tasks icon (2026-11-03). Review PR, Issues and Tasks open from Home's chips with `?home`.
- **CHATFIRST-8:** What Oby is asking shows over your conversation and is answered there in one click. Taller than the window it scrolls, so its choices stay reachable. An alert already out for one with no task goes.
- **CHATFIRST-9:** From a page opened from the side panel, back returns to the conversation you were last in, or to the empty chat if that's where you were, your words still in its box; the panel goes back to the chats. Esc in your conversation stays there.
- **CHATFIRST-10:** First run still works and lands on the empty chat, its Oby in place. Its window is setting up's narrow one, so that first landing keeps the side panel closed; the next launch opens it.
- **CHATFIRST-12:** Deleting the conversation you're in goes to your latest, and when it was the last one, to the empty chat.
- **CHATFIRST-11:** `?home` shows Sangam's Home with its chips, opening on no conversation; `?everything` too.
- **CHATFIRST-13 (`rail_1_foot_order`):** The icons are Chats, a line, each Obysphere's tile, then Settings at the foot: no Home, Tasks, Connectors, Automations or profile circle.
- **CHATFIRST-14 (`rail_3_no_line_without_a_sphere`):** With no Obysphere there's no line.
- **CHATFIRST-15 (`tasks_list_1_hidden_on_the_left`):** No Tasks icon or list, whatever tasks there are; `?home` and `?everything&chat` keep them.
- **CHATFIRST-16 (`chats_1_whole_list_scrolls`, `tasks_list_4_no_show_more`):** Sixty conversations are sixty rows, with no Show more, in a list that scrolls; the row you're on and the first that waits come into view.
- **CHATFIRST-17 (`chats_2_no_sphere_under_it`):** No Obysphere or member under the chats.
- **CHATFIRST-18:** Pinned ones come first.
- **CHATFIRST-19:** With none, the column says "No conversations yet" and Oby's row "All quiet", or "Needs you" with something waiting. Oby's row opens the empty chat.
- **CHATFIRST-20:** Archived is at the foot of Chats once something's in it, opens as a page, and a task in it opens.
- **CHATFIRST-21 (`dots_1_row_when_its_task_needs_you`, `dots_2_chats_icon`):** A task that needs you lights its conversation's row, however far down, and the Chats icon. One running marks its row as working and lights nothing.
- **CHATFIRST-22:** A conversation asking by itself, Oby's question back included, lights its row and the icon. What has no row lights Oby's. GitHub's work no one's started lights nothing. The icon is never lit with no row lit.
- **CHATFIRST-23 (`chats_4_asks_still_show`):** The stack shows on the empty chat and over a conversation.
- **CHATFIRST-24 (`hello_1_no_name_invented`):** The empty chat's line is "What do you want me to work on?", with no name, whatever is running or waiting, its box "Message Oby…"; `?home` keeps Sangam's line and prompts.
- **CHATFIRST-25:** A job's permission, agent's question, plan, merge or question lights its conversation's row and the icon, and is on the stack.
- **CHATFIRST-26:** So does one on a job past the latest 100 obid lists, on the first load and when its item comes as an event.
- **CHATFIRST-27:** The offer to share, a Confirm with no job or conversation, is on the stack and lights the icon and Oby's row.
- **CHATFIRST-28:** A conversation's own Confirm or offer lights its row, and its card goes there in a click. Put off, it's quiet until its time.
- **CHATFIRST-29:** A question only you can answer lights "Questions from other Obys" (one sorted as noise doesn't). A limit lights the conversation of each task it stopped.
- **CHATFIRST-30:** A task with no conversation, or whose conversation was deleted, that needs you is on the stack and lights Oby's row. Done, it's in Archived.
- **CHATFIRST-31:** Later takes a task's card off the stack, keeps its row lit, and brings the card back when its time comes, with no event and across a restart. Settled elsewhere meanwhile, its dot goes. With no row, nothing is lit until its time.
- **CHATFIRST-32:** Copying from a conversation or a room pastes plain text, as in Claude Code: no "1." or "2." before each message (`chatCopy.test.tsx`). A list someone wrote inside a message keeps its numbers.
- **CHATFIRST-33 (`stop.test.tsx`):** While Oby answers in a conversation and the message box is empty, its send button is Stop: a click ends the turn under way in that conversation (`POST /v1/conversation/stop`, naming it), and Oby's answer is what it said until then. With words in the box it's Send, and they wait their turn. No turn under way, a task's page, another Oby's thread: no Stop.
