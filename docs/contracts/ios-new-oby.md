# The iPhone, in line with the new desktop app

The business behavior contract for bringing the iPhone app (`apps/ios/app`) in line with the Electron app's chat first, its picker for who Oby talks through, its live tasks and recovery, from [the template](../business-contract-template.md).

## Feature and status

- **Outcome:** The owner opens Oby on the iPhone and is in the same chat as on the desktop: their latest conversation with Oby, who it talks through a tap away, a task's work live on its card and its agent's own conversation one tap further. What they learned on one app holds on the other.
- **Status:** **Implemented** on the iPhone, the owner's request of 2026-11-03. The behavior is the desktop's: [chat-first.md](chat-first.md), [conversation-agent.md](conversation-agent.md), [live-task.md](live-task.md), [recovery.md](recovery.md). The look is Sangam's on a phone ([docs/ios-design.md](../ios-design.md)), as the iPhone does things, not the desktop's layout. Real pairing with an owner's iPhone isn't proven: only through a local Obysphere, on a Simulator.
- **Scope:** Where the app opens, the side panel, the message box's picker, a task's card in a conversation, the agent's own conversation, and one addition to obid's gateway. **Outside it:** another Oby's answer quoted in a conversation (#1871), the Sphere's room (#1868), five-hour limits resuming by themselves and Cursor's real usage: not merged, and none of them is built against. The Apple Watch keeps its own screens.

## Behavior

### Actors and ownership

- **obid** keeps every conversation, task, choice and session, decides which account a turn runs on and whether a job is tried again. Nothing here moves a decision to the phone.
- **The iPhone** shows what obid sent and sends the owner's words and choices. It reads the same keys the desktop reads: `lastActivity`, `talkAccount`, `talkedOn`, `movedOff`, `talkNext`, `fullUntil`, a job's `step`, `status`, `reason`, `nothingDone`, and its transcript.

### Where it rests

- **Your conversation:** the one that moved last by obid's `lastActivity` when the list comes in, then whichever you're in. Deciding once, as it opens: a conversation that arrives later never moves you.
- **The empty chat:** no conversation: Oby, its line and the box, with what Oby is asking you where it was. Words sent there start a conversation, which opens, in the same page.
- **Before the list comes:** a thread's bones and the box; with the Mac out of reach, the empty chat. Typing there keeps you there. Words sent start a conversation of their own, waiting as words always did if the Mac is out of reach. A list that fails is this connection's and is tried again: it never settles on a thread it can't name.
- **A page over it:** a kind of work, a task, an agent's conversation, Archived, the Inbox, Settings. Back returns to where you rest. Opening a conversation, from the panel, a task or an alert, makes it where you rest.

**Where the iPhone differs from the desktop, on purpose:** opening the app doesn't bring up the keyboard, which would cover half of the conversation; New conversation does. New conversation is also one tap at the top right of a conversation, as a phone's chats have it, besides the panel's row. What Oby is asking stays out of the thread except for an offer made in that conversation, which is in its thread where it was made, once, and a failure or a review, which stay under its card as they did.

### The side panel

Its strip has **Oby** (its title **Chats**) and **Tasks**, with your picture for Settings at its foot.

- **Chats:** Oby first, with its latest words, when, and an orange dot while something needs you; New conversation; the latest five conversations, the one you're in always among them, pinned first, then Show more; Search on the title's row, by obid's own search; then the Obysphere with its number of members and each teammate's Oby. No usage: that's in the message box. A teammate's row opens nothing yet, as on the desktop (SPHERE-7).
- **Tasks:** New task; Review PR, Issues and Tasks (Home's chips) with their dots; the tasks by kind or status; the ⋮ with Group by and Clear done; Archived. The icon's orange dot shows while anything needs you, GitHub's work waiting on you included.

### Who Oby talks through

In the message box, by the gauge, on the empty chat and in a conversation, not on a task's box. Closed: the chosen account's mark and an arrow, or the word Auto and an arrow, with a dot past 80% (yellow) and past 90% or held full (red). Open: Auto ("Whichever has room"), then every connected account with its mark, who it is and its percentage, the fuller of its week and its 5-hour session ("Full", "No usage" for Cursor), the chosen one checked.

- In a conversation a choice is that conversation's (`PATCH /v1/conversations/{id}`); on the empty chat it's the setting new conversations follow (`PUT /v1/settings/jobs`), the same as Settings › Agents' "New conversations talk through". It names the choice at once; refused, it goes back and says why under the box.
- **No Add account:** the gateway keeps adding and signing in accounts on the Mac, on purpose, and Settings › Agents already says so.
- A conversation still being named has no picker yet: its choice has nothing to go to.

### A task's card and its agent's conversation

- **At work:** "Running · tool · account · repository · 12m"; its to-dos with the current one lit, at most eight around it, "N done before these" and "N more to do"; its last three steps, under the current to-do or alone; a stage past the agent's work, or what obid says it's on, as a live line. No list means no to-dos: nothing is made up.
- **Needs you:** "Needs you", what for, the current to-do orange, a card edge in the same colour.
- **Ended:** one card of a few lines: "Done", "Nothing done", "Stopped" or "Failed", who had it; why, in obid's `reason`; its commits, pull request and time.
- **A tap** opens the agent's own conversation, a page on a phone: the task's name and who has it in its bar, Stop while it runs, the task's page, its to-dos on top once, everything from the start (the last 300 lines, "Show N earlier"), thinking only as its tool gave it, tool lines that open to their detail, and the decision over the box. "Tell Claude Code…", and on an ended one "Ask for a follow-up…", whose follow-up the page then follows. A job deleted while it's open closes the page.

### Recovery

A job being tried again stays `running`: its card says so with obid's `step` ("Trying again in 5 minutes (2 of 3)") even with steps above it, because that line isn't one of its own steps. It raises no failed card, no alert and no "Needs you" until the tries are used, when it ends as any other job does. The phone counts nothing: it shows `status` and `step`.

### Failures and recovery

- **Mac out of reach:** the reach line stays over every screen, and words wait as they did. Back in reach, every session on screen reads what it missed, from the last entry known with none missing before it, and at each of its job's events, so a lost one fills by itself.
- **Connecting:** nothing slow holds the events back: clearing old alerts and reading the Sphere go apart, since one that didn't answer on a Simulator left the whole app deaf to obid until it was opened again.
- **A late answer** never undoes an event: an ended call stays ended, and words stay as far as they got.
- **The conversation you're in is deleted:** your latest, or the empty chat if none is left.
- **The empty chat is one page:** New conversation on it builds no second, since two would put Oby's one WebPage in two WebViews, and WebKit stops the app.
- **An older obid** with no `talkAccount`, no Sphere or no list of conversations: Auto, no Sphere's rows, the one thread.

### Invariants and boundaries

- **Add-only API.** The iPhone sends what the desktop sends and decodes keys it doesn't know without failing. No enum the Swift apps decode changed.
- **Sealed end to end.** Everything goes through the owner's Obysphere as before, and every route is on the gateway's allowlist. Two reads were added, for the panel's Obysphere rows: `GET /v1/obid/sphere` and `GET /v1/obid/sphere/members`. The membership carries its state, the Sphere's id and URL, this Oby's id and owner, its public keys and signed card, what it accepts, whether it seals, the delegation's scope and expiry, the token's expiry but never the token, when it last polled and its last error. The members carry each Oby's id and owner, what it accepts, whether it's sealed and whether its owner's key was compared. No secret, and nothing a paired device, which is the owner's, shouldn't know. Joining, leaving, the keys, the card and the pins stay on the machine.
- **Only a session something shows is kept,** and let go when the last thing showing it goes. A call's output and a file's contents come only when its line opens: the phone no longer reads one call's detail to learn whether obid serves it.
- **English only.** Words are the desktop's. **Proposed, Sangam's to change:** "Chats", "Thinking", "Show N earlier", "N done before these", "N more to do", "Declined", "Open the task's page", "New conversations talk through" and the picker's words, all already proposed on the desktop. New on the iPhone: nothing a person reads; the pencil's label is the desktop's "New conversation".
- **Sangam's Home isn't on the iPhone any more:** its chips, Running and Recent went to the panel. His first-run and pairing screens are unchanged.

## Interfaces and dependencies

- **Gateway:** the two reads above; `docs/runner-api.md`, "obid only: Obi-to-Obi exchanges".
- **Reads:** `GET /v1/status`, `GET /v1/conversations`, `GET /v1/jobs/{id}/transcript`, `GET /v1/jobs/{id}/transcript/{n}`, the `transcript`, `conversation`, `job` and `accounts` events.
- **Sends:** `PATCH /v1/conversations/{id}`, `PUT /v1/settings/jobs`, `POST /v1/jobs/{id}/messages`, `POST /v1/jobs/{id}/stop`.
- **Unresolved:** obid knows a Sphere only by its id: the name in its invite link stays with the app that joined, so the phone shows the id. Room is left for #1871: an answer quoted under a turn is one more row in a conversation's thread, and nothing here assumes a turn is only Oby's or yours.

## Acceptance scenarios

Tests: `apps/ios/tests/NewObyTests.swift` (`IOSNEW-n`), and for the gateway `core/crates/obid/src/gateway.rs`, `iosnew_4_a_phone_reads_the_sphere_and_changes_nothing`.

- **IOSNEW-1:** With conversations, the app rests on the one that moved last; with none, on the empty chat; with no list from the runner, on its one thread. A conversation that arrives later doesn't move it.
- **IOSNEW-2:** New conversation lands on the empty chat, creating nothing; words sent there, before obid's list is in too, start the conversation, which obid's name then adopts in the same page. Typing before the list comes keeps you on the box. Deleting the one you're in goes to your latest, and when it was the last, to the empty chat.
- **IOSNEW-3:** The panel's chats are pinned first, then latest, five of them and Show more, the one you're in always among them; a search shows all it found.
- **IOSNEW-4:** The Obysphere and its teammates' Obys come through the gateway, read only; a Sphere obid isn't in, or an obid that doesn't serve them, shows none. Your own other Mac isn't a teammate, and two Obys of one owner are two rows.
- **IOSNEW-5:** Each account's percentage is the fuller of its week and 5-hour session, past its reset counting 0; "Full" when held full, "No usage" for Cursor; the dot yellow past 80%, red past 90% or held full.
- **IOSNEW-6:** The picker names the chosen account; moved off it or on Auto, where it last talked, else what obid says is next, each only while it can run. A conversation's own choice beats the setting; `auto` is Auto whatever the setting says.
- **IOSNEW-7:** The list has every connected account, tool by tool, by its tool's name and by its own once its tool has more than one, with who it is.
- **IOSNEW-8:** A choice in a conversation goes to that conversation and names it at once; on the empty chat it goes to the setting. A choice obid refuses goes back and says why.
- **IOSNEW-9:** A running task's card has its newest to-do list, a long one around the current to-do, its last three steps; with no list, none is shown.
- **IOSNEW-10:** A thought says only what its tool gave. A step reads as what it's doing while it runs and what it did once ended; one refused says "Declined" and doesn't read as a failure.
- **IOSNEW-11:** An ended task's card says Done, Nothing done, Stopped or Failed, and why in obid's words on one line.
- **IOSNEW-12:** A session reads from the last entry known with none missing before it; a late answer doesn't undo an ended call or shorten words; each job's events go to its own session; a session nothing shows is let go, and one held twice stays until both let go.
- **IOSNEW-13:** A job being tried again is running: no "Needs you", no failed card, no outcome, and its card shows obid's step. Past its tries, `interrupted` needs you with the real reason.
- **IOSNEW-14:** Keys and values the iPhone doesn't know don't fail its decoding, and the ones it now reads are read.
- **IOSNEW-15:** What Oby asks shows in the stack over the conversation once, but an offer made in the conversation on screen, which is in its thread; a failure stays with its card.

### Test it when it's done (a real iPhone)

Pair through your Obysphere. Oby opens on your latest conversation. Open the panel: Oby, your conversations, your Obysphere and teammates. Tap the picker by the box, choose another account, ask something: the answer comes from it. Ask for a task: its card shows its steps as they happen; a tap opens its agent's conversation; open a tool line; tell it something. Turn the Wi-Fi off on the Mac during a task until Claude Code gives up: the card says it's trying again, with no alert.
