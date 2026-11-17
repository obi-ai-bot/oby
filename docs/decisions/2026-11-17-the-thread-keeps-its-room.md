# The thread keeps its room: only its own asks, a panel that gives way, titles in a few words

- **Date:** 2026-11-17
- **Status:** In force
- **Replaced:** CHATFIRST-8 and CHATFIRST-23's ask stack floating over an open conversation with every other conversation's decisions (`asks-over`); the side panel pushing the chat past the window's right edge in a narrow window; a new conversation titled with its whole first line, cut at 48 characters
- **Changed by:** Nothing
- **Contracts and scenarios:** [chat-first.md](../contracts/chat-first.md) (CHATFIRST-8, CHATFIRST-23, CHATFIRST-35, CHATFIRST-36), [conversations.md](../api/conversations.md) (`title`)

## The owner's words

> A conversation shows only its own tasks' decisions over its message box. The app keeps the chat column usable at narrow widths. A new conversation gets a short title.

The task the session that started this work gave, from a human drive of dev against T3 Code. Not an exact quote.

## Where it is stated

- [chat-first.md](../contracts/chat-first.md), CHATFIRST-8, CHATFIRST-23, CHATFIRST-35, CHATFIRST-36
- [conversations.md](../api/conversations.md), the conversation's `title`

## Notes

Think of a desk again. The drawer at the front (the decision bar over the message box, [2026-11-17-decisions-like-t3-code.md](2026-11-17-decisions-like-t3-code.md)) holds what this conversation needs from you. Until now a second tray also hung over the desk, holding everyone else's mail, and it sat on top of the conversation's first page: a merge card from another conversation hid the owner's own first message in every conversation, even maximized. That tray is gone from over a conversation. Other conversations' decisions light their own rows and wait in Waiting for you; what belongs to no conversation (another Oby's question, a Confirm of its own) is on the empty chat's stack and in Waiting for you. What this conversation asks with no task (a failure, a GitHub offer made in it) joins its own drawer, unless its thread already shows it (a Confirm, an offer).

### How T3 Code does it (`~/Projects/Personal/t3code`, read 2026-11-17)

- **Approvals.** Only inside their own thread; a child thread's approval over its parent's composer (`ChildThreadApprovals.tsx`). Never another thread's.
- **Width.** `threadSidebarWidth.ts`: the thread keeps `THREAD_MAIN_CONTENT_MIN_WIDTH`, 40rem (640px); the sidebar's widest is the window less that, its narrowest 13rem. Under 768px (`SIDEBAR_MOBILE_QUERY`) the sidebar isn't docked: it opens as a sheet over the thread, a click outside closing it. Its window's minimum is 400 × 360.
- **Titles.** A seed from the first message (`deriveThreadTitleSeed`, cut short), then a model call (`generateThreadTitle`, run through the user's own CLI with tools off) writes a short title.

### What Oby does now

- **Over a conversation, only its own.** `Home.tsx` no longer renders `AskStack` over an open conversation; `ChatView.tsx` adds the conversation's own non-task asks to its drawer. A Mac alert for something with no task or conversation is withdrawn only while the empty chat shows, since nothing else shows it (`notify.ts`, `onPage`).
- **A panel that gives way.** `panelFit.ts` follows T3 Code's numbers: docked while the chat keeps 640px, narrowing to 252px; narrower than that, the panel floats over the chat (a dimmed strip beside it closes it, as does a row's click), and floating is never remembered as open. The window's own minimum stays 420px. Measured on dev at 700px with the panel open: the view ran 82px past the right edge (the message box) and the ask card 62px; now 0.
- **Titles in a few words, by rule.** obid's `short_title` (conversation.rs) drops openers, keeps the first clause, at most six words, nothing dangling; `titled` still cuts at 48 characters. It is not a model call. Which lane would own one: a title written by a model is the agent's work, the conversation's own CLI naming it through one of Oby's tools (a rename it may call on its first answer), never a model call of obid's, which "Dispatcher, not harness" in AGENTS.md rules out ("Rules decide what happens to a job, not a model call of obid's"). That tool is left for its own change, if the rule's titles aren't enough.

### Proof

Screenshots before and after, at 700px and in a wide window, are in [docs/thread-keeps-its-room/2026-11-17](../thread-keeps-its-room/2026-11-17), taken on the renderer's mock (`npm run web`, `?everything&chat`).
