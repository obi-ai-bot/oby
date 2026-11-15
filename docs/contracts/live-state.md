# What obid has, as the app keeps it

The business behavior contract for the one rule by which what obid has (its jobs, conversations, turns, inbox items, status and GitHub's work) reaches the Electron app's state, from [the template](../business-contract-template.md).

## Feature and status

- **Outcome:** The owner sees the same tasks, conversations, cards and Home as before. What changes is underneath: whether an object from obid replaces the one the app holds is decided in one place, a pure function tested on its own, instead of by hand at each event and each answer in `live.ts`. A late answer can't undo a newer event, a deleted task or conversation can't come back, and an older answer can't reopen an item the iPhone settled.
- **Status:** **Implemented** (roadmap R06-06 for jobs, R06-07 for the other kinds): `apps/electron/src/shared/live-state.ts`, used by `apps/electron/src/renderer/src/live.ts`. Since R06-08 the rule runs in main: `apps/electron/src/main/live.ts`, main's one follower of obid's events, keeps the state the window and the Mac's alerts read (LIVE-10, LIVE-11). Related: #1707 (an answer applied over the state as it is when it comes) and #1709 (revisions and deleted jobs).
- **Scope:** The `job`, `jobGone`, `conversation`, `conversationGone`, `turn`, `turnAsks`, `inbox`, `accounts` and `github` events; every job a call to obid answers with, the turns `GET /v1/conversation` answers with, and the status `GET /v1/status` answers with; and the reconnect's sync of jobs, conversations, the inbox, the status and GitHub's work; and main's follower of the events (`main/live.ts`), with its snapshot, live and load channels to the page (below, "Main's follower"). **Out of scope:** the `step`, `delta`, `roomPost` and `peerThread` events, a job's session (its transcript), the limit decisions (`limits`, which have their own revision and refresh), the repositories (`repos`), the jobs the app saw when it opened, the fields the cards and task actions write before obid answers (R06-11), any screen, and obid and its API ([runner-api.md](../runner-api.md) is unchanged).

## Behavior

### Actors and ownership

- **obid** owns each object's `revision`: a job's (`core/crates/obid/src/job.rs`, `Job::revision`), a conversation's, a turn's and an inbox item's ([events.md](events.md), EVENTS-7 to 10): one more each time it sends the object changed, kept across restarts, none from an older obid ([runner-api.md](../runner-api.md), "revision"). It decides when a job or a conversation is deleted, and says so with a `job` or `conversation` event with `deleted: true`.
- **The app** only applies what obid sent. It decides nothing about an object but which of two copies it keeps.

### The rule

The state is `ObidState`:

- `jobs`, by id, and `gone`, the ids of the jobs obid said it deleted;
- `status` and `github` (GitHub's work), each `null` until read;
- `inbox`, the open and put-off items, sorted by `created`;
- `conversations`, and `turns`, each conversation's sorted by `at`;
- `goneConversations`, the ids of the conversations obid's `conversationGone` named;
- `settledInbox`, the `revision` of the copy that settled an item, by id, only for a settled item that had a revision.

`apply(state, event)` takes each kind in Scope. One comparison decides between two copies of the same object, for every kind: when the state holds it with a `revision` higher than the incoming copy's, the held copy stays; on a tie the incoming one is taken; an incoming copy with no `revision` (an older obid, after a downgrade) is always taken, never held off by a revision from before.

- **`job`:** one whose id is in `gone` leaves the state unchanged; otherwise the comparison by id.
- **`jobGone`:** removes the job and adds its id to `gone`.
- **`conversation`:** one whose id is in `goneConversations` leaves the state unchanged; otherwise the comparison by id.
- **`conversationGone`:** removes the conversation and its turns, and adds its id to `goneConversations`.
- **`turn` and `turnAsks`** (the same, as far as what obid has goes): one with no `conversation`, or of a conversation in `goneConversations`, leaves the state unchanged; otherwise the comparison by turn id within that conversation's turns.
- **`inbox`:** a copy whose revision is lower than the item's `settledInbox` mark is refused; otherwise a held copy with a higher revision stays; otherwise an item with `state` `done` leaves `inbox` and, if it has a `revision`, sets its mark to it, and any other item replaces the held one and clears its mark. An item with no `revision` is taken as it comes, over a mark too.
- **`accounts`:** sets the status's accounts when there is a status; with none, nothing changes.
- **`github`:** replaces GitHub's work.

The answers go through the same rule: `answer(state, job)` takes a job a call returned; `answerTurns(state, conversation, turns)` takes the turns `GET /v1/conversation` returned, each under the conversation asked for (a turn without its own `conversation` too); `answerStatus(state, status)` takes a status as it comes, since it has no revision. Every other answer (a conversation edited, an inbox item answered or confirmed, a job fetched) is taken as its event.

### Triggers

- An event from obid's stream, applied as it comes.
- obid's answer to a call (stop, mode, rename, archive, a message, an answer to a job's question, starting GitHub's work, fetching one job, a conversation's turns, an edited conversation, an inbox item's answer or Confirm, the status), and the jobs a plan's answer carries, applied over the state as it is when the answer comes, never over a state read before the call (#1707, #1709).
- The reconnect's sync, below.

### Failures and recovery

- An answer and an event that crossed: the higher revision wins, whichever came last.
- A deleted job's or conversation's answer that crossed its `deleted` event: it stays out, its turns too.
- **After a reconnect** (the sync), `synced(held, before, read)` takes what obid listed (`read`: its latest 100 jobs, every conversation and whether the list was `complete`, the inbox, the status and GitHub's work) over what's held, `before` being what was held when the read started. obid's lists are the truth, since a revision can go back (a restart after a save that didn't land, a downgrade and upgrade again). A held copy changed while the lists were read when it isn't the one `before` held, or `before` held none.
  - **Jobs:** a listed job whose copy didn't change while read is taken whatever its revision, and over an older kept copy with the same id; one whose copy changed (an answer came meanwhile) goes through the rule; deleted ones stay out. The older jobs a conversation fetched, at or past the 100th's `created`, are kept; any other held job was deleted while the stream was away and goes.
  - **Conversations:** a listed one in `goneConversations` stays out; one whose copy changed while read goes through the rule; otherwise obid's listed copy is taken whatever its revision. A held conversation missing from the list is dropped only when the list is complete: one cut short drops nothing. The sync doesn't change the turns.
  - **Inbox:** each listed item goes through the `inbox` decision, but a held copy that didn't change while read doesn't hold the listed one off. Settled marks already in `before` are dropped, as obid's list is the truth; marks set while the list was read stay and apply. A held item missing from the list leaves.
  - **The status and GitHub's work** are taken as read (when GitHub's work can't be read, the sync passes the last one with why, as obid keeps its last lists).

  The sync only says which copy counts as held: the decisions on deleted objects and revisions are the rule's. The events that came while the lists were read are then applied over it.
- A deleted job's session (its transcript, if the app had it) is let go by the code that handles the `jobGone` event, never by the rule.

### Invariants and boundaries

- The rule is pure: no I/O, no module state, nothing of the screens or of Electron; it imports only obid's wire types. It never changes the state it's given, and the same events from the same state give the same result.
- It's written once: nothing else in the app compares the revision of a job, a conversation, a turn or an inbox item, or writes `gone`, `goneConversations` or `settledInbox`. A limit decision's revision is another thing, and stays `main/live.ts`'s.
- What a turn shows while it's under way (what it's on, Oby's words so far and as paced, a new conversation's id in the app) is `live.ts`'s, worked out from the state the rule left; a turn the rule kept out changes none of it. A send's own fields before obid answers (the words still waiting for their answer) are `pending.ts`'s (below, "Words to Oby as a pending overlay"); the cards' and task actions' are still `live.ts`'s until R06-11.
- A rename shows its new title before obid answers, and puts the old one back if obid refuses: that's not an answer and doesn't go through the rule. It keeps the held revision, so it can't break it.
- The owner's Delete hides a conversation and its turns at once, but that's not a deletion obid said: it adds nothing to `goneConversations`, so one obid refuses to delete can come back. Only obid's `conversationGone` keeps it out.

### Main's follower

- **Main** (`apps/electron/src/main/live.ts`) follows `GET /v1/events?alerts=1` from launch until Oby quits, window open or not; nothing else in the app opens obid's events. Once the stream answers it reads the status, the latest 100 jobs, the inbox, every conversation, GitHub's work and the limit decisions, merges them with `synced`, sends the page a snapshot, then applies and passes on, in order, the events that came while it read. Each later event is parsed once: the kinds the rule takes go through `apply`, then on to the page; a job's session, a turn's steps and words, the room's posts and a thread's change are passed on as they come and never kept. It reads the job of an inbox item it doesn't hold, and after each sync the turns of the conversations the page loaded (100) and of the 20 newest (2).
- **Down and back:** a stream that can't open, ends or fails, or a sync that fails, sets `down` with the page's words of before (a 401: "obid refused this Mac’s token (api-token)."; an ended stream: "obid closed the event stream."; obid's JSON `error`, else "obid answered <n>."; main's own reason). Main opens it again after 1 s while no status was read and obid starts (`why: 'waiting'`), else after `min(10 s, 1 s·2^retry)`, `retry` back to 0 after a good sync.
- **The limit decisions** are main's, refreshed 300 ms after a `limitDecision`, `job` or `accounts` event, when the page asks, and every 15 s while live; an older answer than a newer request is dropped, and a change goes to the page.
- **The page** asks `obid-snapshot` when it opens (main's state, the limit decisions, the connection, and `seq`, the number of the last message main sent), after listening on `obid-live`, where main numbers each message (a snapshot after a sync, an event, a load's turns, the limit decisions, a connection change); the preload hands on the snapshot, then only the messages past its number, once each, in order (`shared/live-wire.ts`). It asks `obid-load` for a conversation's turns (`GET /v1/conversation?id=…&limit=100`, through `answerTurns`, then each job they name that main doesn't hold and that isn't deleted, through `answer` and on as a `job` event), a job, or the limit decisions again; the answer is `true`, or why not. The page's own call answers are still applied in the page, through the same rule; obid's events bring them to main.
- **The alerts** (`main/notify.ts`) read the inbox, the limit decisions and the conversations' titles from main's state, and take main's messages in the order main applied them.

## Interfaces and dependencies

Reads the `job`, `conversation`, `turn`, `turnAsks`, `inbox`, `accounts` and `github` events (`deleted: true` parsed as `jobGone` or `conversationGone` by `parseEvent` in `apps/electron/src/shared/obid-wire.ts`) and the objects obid's routes answer with, as [runner-api.md](../runner-api.md) describes them, with the `revision` of [events.md](events.md) (EVENTS-7 to 10). No new route, key or value in obid's API.

## Acceptance scenarios

Tests: [`apps/electron/src/renderer/src/test/liveState.test.ts`](../../apps/electron/src/renderer/src/test/liveState.test.ts) calls the rule directly. Through `live.ts`, [`taskActions.test.ts`](../../apps/electron/src/renderer/src/test/taskActions.test.ts) covers the answers that cross events ("obid's answers over the events that came meanwhile", #1707), a job with no revision, and the turns that cross a conversation's fetch. The reconnect's sync tests live in [`mainLive.test.ts`](../../apps/electron/src/renderer/src/test/mainLive.test.ts), against `main/live.ts`: after a restart that took a revision back ("back after obid restarted with a lower revision: obid's list wins, but a newer answer that came while it was read stays") and after a deletion while the stream was away ("back after the stream was away: a task deleted meanwhile (on the iPhone) leaves, the older ones a conversation fetched stay"); a deleted task not fetched back is in LIVE-10's.

- **LIVE-1** (implemented): Given the state holds job J at revision 5, when a `job` event or an answer brings J at revision 4, then the state keeps revision 5.
- **LIVE-2** (implemented): Given a `job` event with `deleted: true` for J, when any later answer or event brings J, then J stays out of the state.
- **LIVE-3** (implemented): Given J with no revision (an older obid), when it comes, then it is taken as it comes, over a held revision too.
- **LIVE-4** (implemented): Given the same list of events, of every kind the rule takes, when `apply` runs twice from the same state, then both results are equal and the state they started from is unchanged.
- **LIVE-5** (implemented): Given the state holds turn T at revision 3 from a `turn` event, when `GET /v1/conversation` answers with T at revision 2, then the state keeps revision 3 and its text; T at revision 4 replaces it.
- **LIVE-6** (implemented): Given inbox item I held after an `inbox` event at revision 4 (the iPhone's answer, settled, put off with its answer, or still open), when an older answer of the Mac's at revision 3 arrives as an `inbox` event, then I stays as the revision-4 copy left it.
- **LIVE-7** (implemented): Given a `conversationGone` for C, when a `conversation` event for C (with or without a revision) or a reconnect's complete list holding C comes, then C stays out, and so do its turns, from a later `turn` event or answer too.
- **LIVE-8** (implemented): Given an answer during the reconnect's read brought a newer conversation and inbox item, obid's lists hold those at a lower revision and another conversation at a lower revision than its unchanged held copy (a restart), and the events that came meanwhile bring a newer turn and a `conversationGone`, when `synced` and then those events run, then the state holds exactly obid's newest copy of each object.
- **LIVE-9** (implemented): Given a conversation, a turn and an inbox item with no revision, when they come over held copies with revisions or over an empty state, then each is taken as it comes, in the order they came.
- **LIVE-10** (implemented): Given the renderer asks main to load a conversation's turns, when obid answers, then the turns enter the state through `apply` and every open window gets the events. `obid-load` with `{ conversation: 'c1' }` reads `GET /v1/conversation?id=c1&limit=100`; a held turn at a higher revision stays; the window gets the turns, then a `job` event for each job they name that main didn't hold (a deleted one isn't read); the call answers `true`. A job and the limit decisions load the same way, and what obid refuses answers its words. `mainLive.test.ts`: `live_10_a_conversation_the_page_loads_goes_into_mains_state_and_the_window_gets_it`, `live_10: a job, and the limit decisions, read for the page; what obid refuses says why`.
- **LIVE-11** (implemented): Given obid stops, when main's stream ends, then the window shows 'down' with obid's reason as today, and main reconnects with the same back-off. The window gets a connection change to `down` with "obid closed the event stream.", or "obid refused this Mac’s token (api-token)." for a 401, or obid's own `error`; main opens the next stream after 1 s, then 2 s, then 4 s while syncs fail, and every 1 s while no status was read and obid starts. `mainLive.test.ts`: `live_11_when_obids_stream_ends_the_window_shows_down_and_main_reconnects_with_the_same_back_off`, `live_11: obid refusing the token, and a refusal in obid’s own words`, `live_11: while obid starts, before any status was read, main tries every second`.

## Words to Oby as a pending overlay (R06-10)

The business behavior contract for the owner's words to Oby on their way, from [the template](../business-contract-template.md). Roadmap task R06-10 (`factory/queue/done/R06-10.md`). Scenarios LIVE-15 to LIVE-18; their tests are named after them in [`apps/electron/src/renderer/src/test/pending.test.ts`](../../apps/electron/src/renderer/src/test/pending.test.ts).

### Feature and status

- **Outcome:** The owner's words show at once, "Thinking" under them, and stay until Oby answers, as before. Underneath, the app no longer guesses which new conversation is its own by the words' text: words of the same text sent from another device meanwhile start their own conversation on screen too.
- **Status:** **Implemented** in the Electron app: `apps/electron/src/renderer/src/pending.ts`, used by `live.ts`.
- **Scope:** the words `ask` sends (`POST /v1/plans`), from the send until obid's state answers for them. **Out of scope:** a visible "sending" state (it needs Sangam's design), the cards' and task actions' fields before obid answers (R06-11), main's signing, and obid and its API.

### Actors and ownership

- **obid** owns the turn id and what it answers for it: the user's turn takes the id the app gave the words, and the same words sent again with it answer `{"kind": "none", "repeated": true}` while that turn is kept (`Conversations::begin`, [conversations.md](../api/conversations.md)).
- **The page** keeps one pending entry per send, keyed by that turn id (`pending.ts`, a pure module): the conversation as shown (obid's id, or the app's own `new:<n>` for words starting one), obid's conversation once known, the words as sent, their files as the screen shows them, and whether the plan's answer said an answer is coming. What the screen shows is obid's reduced state plus these entries: the same message box, words, "Thinking" and cards as before.

### When an entry leaves

- obid's state holds Oby's answer to it: a turn of Oby's answering words (not a job's report or a routine's run) after the entry's own user turn, in that conversation, whether it came as an event or in a conversation's turns.
- The plan's answer says no answer is coming: `kind: "none"` without `repeated: true`.
- The send failed: obid refused it, anything else threw, or obid answered none of its tries.
- obid's `conversationGone` named its conversation.

Words that started a conversation never leave before obid's own conversation for them is listed, so the page opened for them stays open; meanwhile their row shows under `new:<n>`, with the words and their files until obid's turn shows them.

### Adoption by id only

- The conversation shown under `new:<n>` is the one the words' own user turn is in (the turn whose id is the entry's), or the one the plan's answer to that very request names. Either maps obid's id onto `new:<n>`; nothing else does, and the words' text is never compared.

### Failures and recovery

- REST answers clear or fail an entry and never write obid's state: the plan's answer adds no conversation, its jobs go through the rule, and a new conversation's turns come through `GET /v1/conversation`.
- A send obid didn't answer (status 0) is sent again only as the very same request, the same body and id, 3 tries 1 s apart (`sent`). Nothing else sends words again: not a reconnect, a snapshot, a conversation's turns nor a connection change. A connection `down` ends what only events said was under way, but the entries stay until their own end.

### Acceptance scenarios

- **LIVE-15** (implemented): Given words sent with no conversation, when obid's events bring conversation `cA` and a user turn in it with the sent id and another text, then the `new:1` row is `cA`'s with its turns, `realId('new:1')` is `cA`, no second row shows for `cA`, and once Oby's answer is in the state the entry is gone. `LIVE-15: obid's turn with the sent id adopts its conversation, whatever its text`.
- **LIVE-16** (implemented): Given words sent with no conversation and the plan's answer held, when obid's events bring first `cB` with a user turn of the same text and another id, then `cA` with this app's id, then `cB` shows as its own row under its own id and `new:1` shows `cA`'s turns. `LIVE-16: another device's words of the same text start their own conversation, never this app's`.
- **LIVE-17** (implemented): Given `POST /v1/plans` answers status 0, then `{"kind": "none", "repeated": true}`, when the words are sent, then exactly two calls go with deep-equal bodies and the same id, the entry stays across the retry, `c1` holds one user turn of that id once it arrives, and no third call is made. `LIVE-17: a send obid didn't answer goes again as the very same request, and its words are taken once`.
- **LIVE-18** (implemented): Given a send whose plan answer is held, when the connection goes down and a new snapshot comes, then no further call is made, the entry stays and its `new:1` row still shows the words; it leaves once its turn and Oby's answer arrive, or once the held answer comes back `422`. `LIVE-18: it leaves once its turn and Oby's answer arrive`, `LIVE-18: it leaves once the held answer is let go as a 422`.

## Answers to cards and task actions as pending entries (R06-11)

The business behavior contract for the owner's answers to cards and actions on a task while obid hasn't answered for them, from [the template](../business-contract-template.md). Roadmap task R06-11 (`factory/queue/done/R06-11.md`). Scenarios LIVE-19 to LIVE-21; their tests are named after them in [`apps/electron/src/renderer/src/test/pending.test.ts`](../../apps/electron/src/renderer/src/test/pending.test.ts).

### Feature and status

- **Outcome:** A card the owner answered, or a task they stopped, shows as before until obid's state confirms it, and then shows exactly what obid says. The task card is obid's state plus one overlay.
- **Status:** **Implemented** in the Electron app: `apps/electron/src/renderer/src/pending.ts` (`Action`), used by `live.ts` (`answerJob`, `answerOffer`, `answerQuestion`, a limit card's answer, `stopJob`, `archiveJob`, `deleteJob`, `renameJob`); the cards and tasks read obid's state plus these in `liveView.ts` (`askOf`, `taskOf`, `asksOf`, `chatOf`, `limitAsks`).
- **Scope:** from the click until obid's state answers for it. **Out of scope:** a spinner or any other new look (it needs Sangam's design); main's signing of Confirms and limit answers (`device.ts`, kept as it is); obid and its API.

### Actors and ownership

- **obid** owns the jobs, the inbox items and the limit decisions, and their revisions.
- **The page** keeps one entry per click, keyed by what it is (an answer, a reply to another Oby's question or review request, Stop, Archive, Delete, Rename) and the id of what it acts on: an inbox item's, else the job's, or a limit card's. The entry holds obid's copy as it was clicked, so its revision, and, for an answer, the option and the card as it was asked; for a rename, the new name.
- **What shows:** while an answer's entry is there, the task card shows the pick on the card as it was asked (Later: put off), and Home's stack and the conversation leave its item out; while a rename's is, the task shows the new name. A reply, Stop, Archive and Delete change nothing shown: a reply's card stays on Home's stack until obid's newer copy settles it, since the card keeps the owner's words and says a refusal over them (QuestionCard.tsx). Tests: `an answer to another Oby's question keeps its card while on its way, and leaves with obid's newer copy`, `an answer to another Oby's question obid refuses: its entry goes, the card stays, and the refusal is the caller's`.

### When an entry leaves

- obid's state holds what it acts on at a newer revision, or no longer holds it (an item settled, a job deleted). Where obid sent no revision, any other copy is newer.
- The call failed: obid refused it, or anything else threw. Only that click's entry leaves, never a later one's.
- A Delete obid asks to confirm first, and a limit card's answer once its call is over (the decisions are read again either way).

### Failures and recovery

- A call's answer goes through the shared rule (`take`) and is never written into the state: an answer older than an event that came meanwhile changes nothing.

### Acceptance scenarios

- **LIVE-19** (implemented): Given the owner clicks Allow on a job's permission card, when obid's `inbox` event at a newer revision arrives, then the card shows obid's state and the entry goes. Until then the task shows Allow on the permission card and isn't waiting on the owner; the same revision again changes nothing. `LIVE-19: Allow on a job's permission card shows answered until obid's newer inbox event, then obid's state`.
- **LIVE-20** (implemented): Given the owner clicks Stop, when the call fails, then the card shows the job as obid has it and the reason, as before. `LIVE-20: a Stop that fails shows the job as obid has it, and the reason`.
- **LIVE-21** (implemented): Given the owner answers a card and obid's event arrives before the call's answer, when the answer arrives, then nothing changes on the card. `LIVE-21: obid's event before the call's answer: when the answer arrives, nothing changes on the card`.

## Settings rows obid has no setting for yet (R06-13)

The business behavior contract for Settings rows whose choice obid doesn't keep yet, from [the template](../business-contract-template.md). Roadmap task R06-13 (`factory/queue/done/R06-13.md`); it uses R06-04's channels ([ipc.md](ipc.md)). Scenarios LIVE-25 and LIVE-26; the tests that prove them are named after them (`live_25_…`, `live_26_…`, `apps/electron/src/renderer/src/test/localSettings.test.ts` and `.tsx`).

### Feature and status

- **Outcome:** A Settings choice that obid doesn't keep yet, such as Open at login, survives a restart of the app, in one file main owns, instead of looking saved and coming back on its default.
- **Status:** **Implemented** in the Electron app (`apps/electron`): the file in `src/main/settings.ts`, the page's half in `useSetting` (`src/renderer/src/SettingsView.tsx`), the bridge as `window.app.settings` (`src/preload/index.ts`).
- **Scope:** every row `useSetting` holds, the demo rows `?everything` shows included, but `accounts`. **Out of scope:** making a row do anything (Open at login's tie to the Mac's login item is [Open at login (A00-90)](#open-at-login-a00-90)), moving a row into obid, any change to how a row looks, reads or defaults, the choices `useKeptSetting` keeps in the page's `localStorage` (`models.mine`, `agent.default`, `talk.default`, outside this channel), and obid and its API ([runner-api.md](../runner-api.md) is unchanged).

### Actors and ownership

- **The owner** turns a row in Settings.
- **The page** shows each row: the value set in this launch, else the kept one when it has the type of the row's default (the same `typeof`, an array only for an array default, `null` only for a `null` default), else the default. It reads what main kept once, as it starts, and sends each change to main. Until main's answer comes the row shows its default, and a change made before then isn't replaced by it.
- **Main** owns `settings.json` in the app's own folder (`src/main/settings.ts`): its `local` section holds the rows, an object of row key to JSON value, beside main's own keys (`notifications`, `updatesAuto`, `loginApplied`). Every write reads the file, sets its keys and writes it whole, owner-only (0600) and never through a link (`writeOwn`), so a row keeps main's keys and main's keys keep `local`.
- **obid** owns nothing here.

### The channels

- `local-settings` (no arguments): the `local` section, only the keys a row may have.
- `local-setting` (`rowKey`, `rowValue`): `local[key] = value`, answered `true`. A key is a string matching `^[A-Za-z][A-Za-z0-9._-]{0,63}$`; a value is plain JSON of at most 65,536 characters, kept as it reads back.
- Both are refused (`null`) to anything but the window's own page; `local-setting` with a key or value the decoders can't read gets `null` and writes nothing ([ipc.md](ipc.md)).
- In a browser (`npm run web`) there is no bridge: the rows stay in memory while the page is open, as before.

### Failures and recovery

- **The file can't be read** (missing, not JSON) or its `local` isn't a plain object: `local-settings` answers `{}` and every row shows its default.
- **A kept value of the wrong type:** the row shows its default, and nothing throws. Turning the row keeps the new value over it.
- **A write fails** (the folder is gone, the disk is full): `local-setting` answers `false` and main logs it; the row keeps its new value for this launch.
- **Main's answer is refused or fails:** nothing is read, and the rows show their defaults.

### Invariants and boundaries

- The page's channels never read or write a key of main's own: only `local`'s entries.
- `accounts` is never kept nor read from `local`: on obid it's obid's own list (`replaceAccounts`), and a kept list would show stale accounts.
- No row's look, words or default changes.

### The local rows

What each row holds, where it shows and what stands in its place on obid. None has a task that moves it into obid yet.

| Key | Page | Who sees it | On obid today | Task |
|---|---|---|---|---|
| `login` | General, Open at login | everyone | The same switch. Its value is kept, and the Mac's login item for the app follows it (a packaged Oby in Applications), shown as macOS has it ([Open at login](#open-at-login-a00-90)). | A00-90 |
| `voice.speaks` | General, Voice | `?everything`, a browser | Nothing: the Mac has no voice yet. | no task yet |
| `mac.mode` | General, Permissions on this Mac (the mock) | `?everything`, a browser | obid's `/v1/settings/permissions` (`MacPermissions`). | no task yet |
| `mac.rules` | General, Permissions on this Mac (the mock's rules) | `?everything`, a browser | obid's saved rules, on the same page. | no task yet |
| `remote.on`, `remote.forgot` | Remote access (the mock) | a browser | Pairing (Pair a device, Paired devices) through the owner's Obysphere. | no task yet |
| `share.mac`, `share.when`, `share.who`, `share.chosen`, `share.jobs`, `share.size`, `share.work`, `share.guests`, `share.grants` | Sharing | `?everything` | Nothing: Sharing is hidden at launch, and obid's side of it went with the strip to a dispatcher. | no task yet |
| `claude.model`, `claude.effort`, `claude.complexEffort`, `codex.model`, `codex.effort`, `codex.complexEffort`, `cursor.model` | Models, and the mock's models on Agents | `?everything`, a browser | Settings › Agents, each account's model and efforts kept by obid. | no task yet |
| `vault` | Vault | `?everything` | Nothing: Vault is hidden for launch. | no task yet |
| `updates.auto` | About (the mock) | a browser | Main's own `updatesAuto` in the same file ([updates.md](updates.md)). | no task yet |
| `connected` | Connectors (the mock) | a browser | obid's GitHub and Tetrix. | no task yet |

Kept outside this channel, in the page's `localStorage` (`useKeptSetting`): `models.mine`, `agent.default` and `talk.default`.

### Acceptance scenarios

- **LIVE-25** Given the owner changes a local Settings row, when the app restarts, then the row shows the changed value. Main: `local-setting('login', false)` from the window's own page answers `true`, and after main's modules load again `local-settings` answers `{ login: false }`, from a `settings.json` of `{"local":{"login":false}}` with mode 0600. The page: Open at login shows on until main's answer comes, then off; turned before the answer comes, it keeps the owner's turn and sends it to main. `accounts` is never sent nor read; a refused sender gets `null` and nothing is written; a key of main's own (`notifications`) reaches only `local.notifications`; `__proto__`, `''`, a number, a 65-character key and a value over 65,536 JSON characters are refused with the file unchanged; alerts, Update automatically and the rows share the file. `live_25_a_local_row_set_through_main_is_there_after_a_restart` (both files), and the other `live_25: …` cases.
- **LIVE-26** Given a value of the wrong type in `settings.json`, when the row reads it, then it shows its default. With `local` holding `{ login: "yes", "share.chosen": 3 }`, the rows show `true` and `[]`, and nothing throws; a file that isn't JSON, and a `local` that is a string or an array, answer `{}`. `live_26_a_kept_value_of_the_wrong_type_shows_the_rows_default` (both files).

#### Test it when it's done (on a real Mac)

1. `npm run dev`: turn Open at login off in Settings › General, quit Oby and open it again: it shows off. Notifications and Update automatically keep their own choices.

## Open at login (A00-90)

The business behavior contract for Settings › General's Open at login, from [the template](../business-contract-template.md). Roadmap task A00-90 (`factory/queue/done/A00-90.md`); it builds on R06-13's `local.login` row (above) and adds a channel ([ipc.md](ipc.md)). Scenarios LIVE-27 to LIVE-29; the tests that prove them are named after them (`live_27_…`, `live_28_…`, `live_29_…`, `apps/electron/src/renderer/src/test/localSettings.test.ts` for main and `.tsx` for the page).

### Feature and status

- **Outcome:** Open at login opens Oby when the owner logs in to the Mac, and turning it off stops it. The switch shows what macOS has, so a change made in System Settings › Login Items shows too.
- **Status:** **Implemented** in the Electron app: main's half in `src/main/settings.ts` (`login-item`, `loginDefault`), called from `src/main/index.ts`; the bridge as `window.app.loginItem` (`src/preload/index.ts`); the page's half in `General()` (`src/renderer/src/SettingsView.tsx`).
- **Scope:** the app's own macOS login item, for a packaged Oby in Applications, and what the switch shows. **Out of scope:** starting the window hidden at login (`wasOpenedAtLogin`, `openAsHidden`), or any change to the `--background` start, `createWindow()`'s conditions or the ALERTS-3 activation rule: a login start opens the window like any other start; obid's launch agent (its registration, `startObid`, `register`, `status`, the approval prompt and `open-login-items`), which starts obid at login as before; any change to the row's label, look, place, footer or default, and any new words, including a message when macOS wants the owner's approval; moving `login` out of `local` or into obid, and obid and its API; registering from `npm run dev`, tests, a copy on the disk image or a translocated copy, or on Windows or Linux; applying the default again at later starts, or "repairing" macOS's state to match the saved row; watching System Settings live while General is open.

### Actors and ownership

- **The owner** turns the switch in Settings › General, or removes or adds Oby in System Settings › General › Login Items.
- **Main** owns the app's own login item (`app.setLoginItemSettings({ openAtLogin })`, `app.getLoginItemSettings()`, Electron's `mainAppService`), apart from obid's `agentService` entry (`src/main/obid.ts`), which nothing here reads or changes. It keeps `loginApplied: true` in `settings.json`, one of its own keys, once the default was applied or the owner turned the switch.
- **The page** shows macOS's answer when it has one, else the kept `local.login` (LIVE-25 and 26), and keeps each turn through `local-setting` as before.
- **obid** owns nothing here.

### Triggers

- **Main starts** (`loginDefault()`, in `app.whenReady()` right after `localSettings`): on a packaged copy in Applications whose `settings.json` has no `loginApplied: true`, it registers the app's login item unless `local.login` is `false` (missing, or of another type, counts as the default, on), then writes `loginApplied: true`. With `loginApplied` already `true` it does nothing, so removing Oby in System Settings sticks.
- **Settings › General opens:** the page asks `login-item` with no value and shows the answer.
- **The owner turns the switch:** the page keeps the row (`local-setting('login', on)`), shows `on` at once, asks `login-item(on)` and shows its answer.

### The channel

- `login-item` (`orUndefined(boolean)`), refused (`null`) to anything but the window's own page ([ipc.md](ipc.md)).
- With no value it answers macOS's state, `getLoginItemSettings().openAtLogin`. With a boolean it calls `setLoginItemSettings({ openAtLogin })`, writes `loginApplied: true` and answers macOS's state as read after the call. It never writes `local`.
- Only a copy for which `app.isPackaged && app.isInApplicationsFolder()` holds (asked in that order) touches a login item: `npm run dev`, tests and a copy outside Applications (a disk image, a translocated copy) get `null`, with no login-item call and nothing written, and the switch shows the kept row as before. In a browser there is no bridge and the row behaves as R06-13 says.

### Failures and recovery

- **macOS refuses a turn** (`setLoginItemSettings` throws): main logs it and answers macOS's state as read afterwards; the switch shows that.
- **macOS answers otherwise** (an item that needs the owner's approval answers off): the switch shows macOS's answer, with no new words.
- **The write of `loginApplied` fails:** main logs it and still answers.
- **The default fails at start:** main logs it, writes nothing, and tries again at the next start.
- **Main's answer is `null` or fails:** the switch shows the kept row.

### Invariants and boundaries

- Every call for the app's item has no `type` but `mainAppService` and no `serviceName`; obid's `agentService` entry is never touched here, and `src/main/obid.ts` is unchanged.
- The default is applied once per install and never over the owner's own turn.
- No row's look, words or default changes.

### Acceptance scenarios

- **LIVE-27** Given a packaged Oby in Applications with its login item on and obid's entry enabled, when the owner turns Open at login off, then the app's item goes and obid's stays; turned on, it comes back. Main: `login-item(false)` from the window's own page makes one `setLoginItemSettings({ openAtLogin: false })`, with no `agentService` type and no `serviceName`, answers `false` and leaves obid's entry enabled with no call for it; `login-item(true)` answers `true`. In development, and from a copy outside Applications, `login-item()` and `login-item(false)` answer `null`, call nothing and write nothing. The page: turning the switch keeps `login` and calls `loginItem(false)`, and the switch shows off. `live_27_turning_open_at_login_off_removes_the_apps_login_item_and_leaves_obids_entry`, `live_27: in development nothing is registered` (main), `live_27: turning the switch keeps the row and asks main to apply it` (page).
- **LIVE-28** Given a fresh install, when Oby starts, then it registers the app's login item once. `loginDefault()` makes one `setLoginItemSettings({ openAtLogin: true })` and writes `loginApplied: true`; after a restart, with the item removed, it makes no call. A saved `false` registers nothing and still writes `loginApplied: true`; a saved value of another type counts as on; after the owner's turn the next start makes no call; in development or outside Applications it calls and writes nothing; a set that throws writes nothing and is tried again at the next start. `live_28_a_fresh_install_registers_the_apps_login_item_once_at_start`, `live_28: a saved off is never registered`, `live_28: a saved value of the wrong type counts as on`, `live_28: an owner's turn is never overridden`, `live_28: nothing at start in development`, `live_28: a default macOS doesn't take is tried again at the next start`.
- **LIVE-29** Given macOS's state differs from the kept row, when Settings › General opens, then the switch shows macOS's state. Main: with `local.login` `true` and the item off, `login-item()` answers `false` and sets nothing; a throwing set answers macOS's state and logs once, and a failed write is logged too. The page: kept `{ login: true }` with `loginItem()` answering `false` shows off, kept `{ login: false }` with `true` shows on; `loginItem(true)` answering `false` leaves the switch off; a failed or `null` answer shows the kept row. `live_29_the_switch_shows_macos_state_when_it_differs_from_the_saved_value` (both files), `live_29: a change macOS doesn't take goes back to what macOS says` (both files), `live_29: no answer from main shows the kept row` (page).

#### Test it when it's done (on a real Mac)

With a packaged Oby copied into /Applications on a Mac where it never ran:

1. Open it once. Oby is listed in System Settings › General › Login Items › Open at Login, and Settings › General shows Open at login on. obid's own entry is listed as before.
2. Turn the switch off. Oby leaves that list, obid's entry stays, and after logging out and in, the window doesn't open.
3. Turn it back on and log out and in. The window opens.
4. Remove Oby in System Settings, then reopen Settings › General. The switch shows off. Quit and open Oby: it stays out of the list.

## The live path without the mock (R06-12)

The view types the screens draw (`Job`, `Message`, `Ask`, ...), their constants (`AGENTS`, `TELL`, `STEER`, `ASK_LABEL`) and pure helpers (`agentOf`, `hasAttachments`, `ago`) live in `apps/electron/src/renderer/src/view.ts`, which imports nothing from `mock.ts` or `lean.ts`; `mock.ts` holds only the demo data and the functions that read it. Roadmap task R06-12 (`factory/queue/done/R06-12.md`).

- **LIVE-22** (implemented) Given the source of `live.ts`, when the test reads its imports, then none is `./mock`. `live_22_live_ts_imports_nothing_from_the_mock`, and `live_22: view.ts imports nothing from the mock or lean.ts`, in `apps/electron/src/renderer/src/test/view.test.ts`.
