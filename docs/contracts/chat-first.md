# Chat first: the app opens on your Oby's conversation

## Feature and status

- **Outcome:** The owner opens Oby and is in a chat with their own Oby, the message box ready. Their Oby talks to other Obys for them; they don't have to learn tasks, Home or kinds of work first.
- **Status:** Implemented in the Electron app, `apps/electron`. The owner's decision of 2026-11-02: "100% Obysphere-centric, just a chat", "I interact with my Oby on my conversation". On that day's call Dima and Sangam couldn't tell a task from a conversation. It overrides Sangam's Home as the first screen for now, on the owner's word; his pieces and words stay wherever they fit, and his Home stays in the code to compare. The concept: the canvas "Oby: the Sphere is the chat", https://claude.ai/artifact/StYU4XCqZAnQ3t9z9yBDsd.
- **Scope:** Where the app opens, what the side panel lists, and where what Home held went. Outside it: a task's live view, Obys asking Obys (their own work), and the canvas's other screens.

## Behavior

- **Actors and ownership:** Only the app changes. obid keeps every conversation, task and ask as it did; this feature adds no API.
- **Triggers:** The app opening, once setting up is done and obid's list of conversations is in.
- **States:**
  - **Your conversation:** where you come to rest, its box focused. It's the one that moved last by obid's `lastActivity` when the app opens, then whichever you're in. Esc and the back arrow have nothing to leave it for.
  - **The empty chat:** no conversation open: Oby's character, its line and the box (Home's stage, without the chips). Words sent there start a conversation, which opens. It's a resting place too.
  - **A page over it:** a task, a kind of work, Settings, an Obysphere, a teammate's Oby. Back returns to where you were resting.
- **Where Home's things went:**
  - Review PR, Issues and Tasks (Home's chips): the side panel's **Tasks** icon, first under its title, with their dots. The icon has the orange dot while something in them needs you, GitHub's work waiting on you included. Tasks also keeps Sangam's list by status, New task, Archived and its ⋮.
  - A task: its card in the conversation that started it, as before, or the Tasks icon.
  - What Oby is asking you (Home's stack): over your conversation, answered there in one click; on the empty chat, where it was. The rule that every request waiting on the owner shows and is answered in one click holds.
  - Conversations (the old icon): under Oby in the side panel, the latest five, then Show more; pinned ones first; Search on the panel's top row.
- **Failures and recovery:** With obid away the splash and the connection line stand as before. A conversation deleted while it's open, or one a page was opened from, goes to your latest conversation, or the empty chat if none is left. A task deleted while its page is open goes back to where you were resting.
- **Invariants:** No conversation is created by opening the app. Where it opens is decided once, as it opens: a conversation that arrives later never moves you. An alert for what's on screen goes once the window is in front, so one for an ask with no task goes when you're on a conversation too (with `?home` as well, where a conversation has no stack: a known ceiling of the comparison). `?everything` shows Sangam's design as it was. Sangam's Home stays reachable: `?home`, or `oby.home` = `1` in the window's local storage. `?everything&chat` shows this on his demo data, Obysphere and teammates included.

## Interfaces and dependencies

None in obid. A member of a live Obysphere shows in the panel but opens nothing yet (`canChat`, SPHERE-7): obid has no conversation with another Oby for it to open. A teammate's chat is reachable where the app has one to show.

Proposed words, Sangam's to change: "Chats" (the panel's title, from the canvas), "Back" (the arrow's label). The rest are his or the canvas's: Oby, New conversation, No conversations yet, Tasks, "Ernesto's Oby", "Accounts and how much of each is used". The canvas's "Bring your team in" row isn't here: it asks for a field to paste an invite, which the app doesn't have yet.

## Acceptance scenarios

Tests: `apps/electron/src/renderer/src/test/chatFirst.test.tsx` and, driving the built app with real input on a scratch obid, `apps/electron/scripts/test-chat-first.mjs` (`npm run test:chat-first`).

- **CHATFIRST-1:** With conversations, the app opens on the one that moved last, its box focused, with no Home behind it. An old conversation with a card still open doesn't count as having just moved.
- **CHATFIRST-2:** With none, it opens on the empty chat: the character and the box, no chips. Words sent there open their conversation. A conversation that arrives meanwhile, from the iPhone say, doesn't move you off it.
- **CHATFIRST-3:** The side panel, open the first time where it fits, lists Oby first, then each Obysphere with its teammates' Obys under it, then one usage row per tool with a connected account at its foot. A row shows the account with the most room; with no usage reported it shows "–", never a made-up number; a click opens the accounts.
- **CHATFIRST-4:** New conversation is one click from the side panel, and lands on the empty chat with its box focused.
- **CHATFIRST-5:** A teammate's Oby opens from its row; you come back to your conversation from it.
- **CHATFIRST-6:** A task opens from its card in the conversation, and back returns to that conversation.
- **CHATFIRST-7:** Review PR, Issues and Tasks are under the side panel's Tasks icon, and each opens its cards as Home's chip did. The icon's orange dot shows a review or an issue waiting on you.
- **CHATFIRST-8:** What Oby is asking shows over your conversation and is answered there in one click. Taller than the window it scrolls, so its choices stay reachable. An alert already out for one with no task goes.
- **CHATFIRST-9:** From a page opened from the side panel, back returns to the conversation you were last in, or to the empty chat if that's where you were, your words still in its box; the panel goes back to the chats. Esc in your conversation stays there.
- **CHATFIRST-10:** First run still works and lands on the empty chat, its Oby in place. Its window is setting up's narrow one, so that first landing keeps the side panel closed; the next launch opens it.
- **CHATFIRST-12:** Deleting the conversation you're in goes to your latest, and when it was the last one, to the empty chat.
- **CHATFIRST-11:** `?home` shows Sangam's Home with its chips, opening on no conversation; `?everything` too.
