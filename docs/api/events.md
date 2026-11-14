### `GET /v1/events`

Server-Sent Events, so the app stays live while it's open:

```
event: job
data: { …Job }
id: 3f2a9c0e8d7b4f61a5c2e9d0b1f4a6c8-41

event: accounts
data: [ …Account ]

event: clear
data: {"clearedAt": "2026-10-10T08:12:00Z"}

event: cleanup
data: {"path": "/srv/nvme/oby/jobs", "bytes": 8912896, "lastRun": null}

event: inbox
data: { …InboxItem }

event: check
data: { …Mac check }

event: turn
data: { …Turn }

event: conversation
data: { …Conversation }

event: step
data: { …Step }

event: thinking
data: { …Thinking }

event: reply
data: { …Reply }

event: stalled
data: { …Job }

event: recap
data: {"text": "Two jobs are done. Pick a base branch needs you.", "since": "2026-10-17T08:10:00Z", "at": "2026-10-17T09:02:00Z"}
```

- Numbered events (obid only, R06-02, `events-replay-v1`, [docs/contracts/events.md](contracts/events.md)): every event but `transcript`, `delta`, `transcriptStale` and `resync` carries an SSE `id:` line, `<epoch>-<seq>`, and its `event` and `data` stay as they are. The epoch is random, made each time obid starts and never saved, so a restart changes it; `seq` goes up by one with each numbered event. obid keeps the last 2,000 numbered events in memory, and no more than 8 MB of their data. Apps that don't know the `id:` line ignore it.
- Resuming (obid only): an app that reconnects names the last id it had, with `?after=<epoch>-<seq>` or the `Last-Event-ID` header. The query wins, and an empty value of either counts as not sent. When that id is still in obid's memory, obid sends the numbered events after it, in order, then goes live, none twice. When it isn't (another epoch, after a restart; older than what obid keeps; ahead of the latest; not an id at all), the stream's first event is one `resync`, `{"epoch": "<this run's epoch>"}`, with no id, then live events: the app fetches again (status, jobs, inbox, conversations), as after every reconnect before. An app that names no id gets live events only, as before, and never a `resync`. Phones resume with `?after=`: the gateway passes the query on and not `Last-Event-ID`, and a phone's replay leaves out the Mac's own events as its live stream does.
- An app never infers a gap from a jump in the numbers: the gateway leaves the Mac's own events out of a phone's stream, and `delta` has no number. Only `resync` means "fetch again".
- An app that falls more than 1,024 events behind on events other than `transcript` has its stream ended there, instead of skipping those events and staying open, so the app reconnects and resumes with `after` (above), or fetches again when it gets `resync` or names no id. `transcript` events have a queue of their own (below): falling more than 1,024 of them behind never ends the stream, so a busy job's lines can't make a phone reconnect over and over.
- A `job` event comes with every change: a new step, a status, the summary, `commits`, `published`, `publishError`, `pipeline`, `pullRequest`, `pullRequestError`, `dismissed`, `macCheck`, `mergeError`, a move to another account. The app replaces the job with that `id`.
- `stalled` (obid only) comes once when a job stalls (`limit`, above), with the job as its `job` event carries it, `stalled` and the handoff note included.
- `transcript` (obid only) comes with each transcript entry that comes or changes: `{"job": "<id>", "entry": {…}}`, and with each line the agent writes: `{"job": "<id>", "n": 9, "append": "…"}` (add it to entry `n`'s `text`), the entry as `GET /v1/jobs/{id}/transcript` has it. Update by `n`: a call comes again under its `n` when it ends. These events have a queue of their own in obid, so a busy job's calls never push another event (a job's end, say) out of a slow app's backlog; an app that falls more than 1,024 of them behind gets one `transcriptStale` event (obid only), `{"missed": 976}`, the number of `transcript` events it missed, of any job, in their place, and the stream goes on: it fetches each transcript it shows again with `after` (`GET /v1/jobs/{id}/transcript`, below). Apps that don't know `transcriptStale` ignore it, as any event they don't know.
- `accounts` comes when accounts are added, renamed or removed and a sign-in ends (obid only: accounts), and when sign-ins, usage, reset times, running counts, `fullUntil`, a lifted reserve (`reserveLiftedUntil`) or Tetrix (`tetrix`, `tetrixJobs`, `tetrixUsed`) change.
- `cleanup` (obid only) comes once obid has measured the job worktrees after it started answering, with what `status.cleanup` says from then on (above).
- `clear` comes when Clear done is pressed on any app (`POST /v1/clear`): hide the settled jobs that finished by `clearedAt`.
- `turnDelivery` (obid only, docs/contracts/deliveries.md) comes when the `delivery` of one of the owner's turns changes after it was added `queued`, as that turn: replace the turn with that `id`. It's no new turn, says nothing about a turn under way, and no alert comes with it. An app that doesn't know it ignores it.
- `turnAsks` (obid only) comes when a question's first card goes on a turn already there, as that turn, with its `asks` (obid only: questions from other Obis, "A question's card"): replace the turn with that `id`. It's no new turn and says nothing about one under way, and no alert comes with it. An app that doesn't know it ignores it.
- `peerThread` (obid only) comes with every change to an exchange in this Sphere, as `{"owner": "<id>", "posts": […]}`, that exchange's posts as `GET /v1/obid/sphere/peers/{owner}` shows them (obid only: a member's thread): replace each post with that `id`. An app that doesn't know it ignores it.
- `roomPost` (obid only) comes with every post of the Sphere's room that obid keeps or whose state changes, as `GET /v1/spheres/{sphere}/room` shows it (obid only: the Sphere's room): replace the post with that `id`. An app that doesn't know it ignores it.
- `inbox` comes with every inbox item that arrives or changes: answered, done or put off, on any app, or settled by itself. Replace the item with that `id`; one whose `state` is `done` leaves the inbox.
  obid sends the item as it is when the event goes out, one event at a time, so the events for one item never go back to an older state and the last one is the item as `GET /v1/inbox` lists it (the same goes for `exchange` events, obid only: Obi-to-Obi exchanges, below). Two changes close together can come as two events that both carry the newer state: an item answered as soon as it arrives may never be seen `open`. On obid the event comes only for an item obid kept on disk (docs/contracts/home.md, HOME-4): a change whose save failed sends none, and a new item whose save failed isn't made, in `GET /v1/inbox` or anywhere.
- `check` comes when a Mac check or an Apple check (below) is queued, taken by a Mac or ends. A Mac that runs checks asks for one (`GET /v1/checks/next`) when one is `queued`; other apps can ignore it, since the job's own event carries `macCheck` and `appleCheck` too.
- `turn` comes with every turn of the conversation that's added, from any app, and again when one changes (its `jobs`): the turn as `GET /v1/conversation` lists it. Replace the turn with that `id`, or add it in `at` order. Older runners don't send it.
- `step`, `thinking` and `reply` (`thinking` and `reply` obid only) follow a turn of the conversation as it happens: `POST /v1/plans`, above. On obid a turn in an agent session also sends `delta`, its words as they're written, and `step` events with `tool` and `detail` (obid only: the conversation harness, below).
- `recap` (obid only) comes when the owner is back after more than 30 minutes away and the recap is written (`POST /v1/plans`, "What happened while you were away"): `text` is one to three sentences, `since` when the owner was last seen, `at` when they came back. It isn't a turn; the next answer starts with it. Older runners don't send it.
- `artifact` (obid only) comes when an artifact is made or gets a new version: `{"id", "version", "title", "kind", "conversation", "job"}`; and `{"id": "<id>", "deleted": true}` when it's deleted ([obid only: artifacts](#obid-only-artifacts)).
- `conversation` (obid only) comes whenever one of obid's conversations starts, gets a turn, is renamed, pinned or deleted: obid only: conversations, below. The Swift runner doesn't send it.
- No model turn runs on a job's, a check's or an inbox item's event: what happens when a job ends is obid's own rule (docs/contracts/recovery.md, "End-of-job rules").
- A `: ping` comment every 15 seconds keeps the connection open. Reconnect after any drop. Fetch what you show (`GET /v1/status`, `GET /v1/jobs`, `GET /v1/inbox`; a runner older than the inbox answers `404` there, which means an empty inbox, and any other failure means try again) only once the events' response has arrived, and apply the events that came meanwhile over it: the runner follows a connection before it answers, so nothing that changes in between is lost.
- `?alerts=1` (obid only, add-only; docs/contracts/alerts.md, ALERTS-3): the Mac app's main process follows the stream for its alerts (`apps/electron/src/main/notify.ts`). Counted only on loopback, never from a paired device through the gateway. While none follows, a look of the pushes (`POST /v1/notifications`, below) that finds something new waiting on the owner (what the relay's gate lets through, a thing and what it needs, each once per obid run) opens the Mac app hidden: `/usr/bin/open -g -j -a <the .app obid runs from> --args --background`, once for that look. What comes while one follows is no news. Within 60 seconds of one dropping (or of obid's start) it may be reconnecting: what's new then is held, and obid looks again when the 60 seconds are up, opening the app then if none follows. Only on macOS, and only for an obid inside the app's bundle (`Contents/Helpers/obid`); a failing `open` is logged and nothing else changes. Launched with `--background`, the app opens no window (its banner shows the alert); its window opens only while it's the active app, so the reopen `open` brings, to that launch or to an app already running with its window closed, opens nothing, and a Dock click or ⌘-Tab does.

