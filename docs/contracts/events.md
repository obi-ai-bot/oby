# Events: numbered, and replayed to a client that missed them

The business behavior contract for numbered events on `GET /v1/events`, from [the template](../business-contract-template.md). Roadmap task R06-02, milestone R06, Events that clients only apply. Fixes #1000's first half.

## Feature and status

- **Outcome:** A client that drops its connection gets the events it missed instead of fetching everything again, and one that was away too long is told so with one `resync` event. Phones over the gateway get the same through a query parameter.
- **Status:** **Implemented** by R06-02 (`Events::send` and `Events::follow_from` in `core/crates/obid/src/events.rs`; `events` in `core/crates/obid/src/routes.rs`; docs/runner-api.md, `GET /v1/events`), announced as `events-replay-v1` in `capabilities` ([capabilities.md](capabilities.md)). The clients that send `after` are R06-09 (the Electron app) and R06-14 (the iPhone, frozen): until then no client sends it.
- **Scope:** The epoch, the numbering, the ring, the `id:` line, `after` and `Last-Event-ID`, `resync`, and the gateway's handling of a phone's replay. Outside it: numbering, keeping or replaying `transcript` and `delta` events, and catching up a half-written answer (#1000's mid-turn part); forwarding `Last-Event-ID` through the gateway (phones use `?after=`); any client change; keeping the epoch, the numbers or the ring on disk, or making the ring's size a setting.

## Behavior

### Actors and ownership

- **obid** owns the numbering, the ring, its size, the epoch, and when a client must fetch again (`resync`).
- **Clients** only send the last id they had, as `after` or `Last-Event-ID`, and apply what comes. A client that sends neither gets the stream as before.

### The rules

- **The epoch.** obid makes a random epoch (32 hex characters, no `-`) each time it starts. It never goes into a file, a setting or the log, so a restarted obid has a new one.
- **Numbering.** Every event on the main queue is numbered, the first `1` and each next one more: every event but `transcript` (its own queue, with `transcriptStale`) and `delta`. `transcriptStale` and `resync`, which obid makes for one follower, have no number. A numbered event's frame carries `id: <epoch>-<seq>`; its `event` and `data` lines stay as they were (the `accounts` data is a JSON array, so no key is added to any data).
- **The ring.** obid keeps the latest numbered events in memory, at most 2,000 of them, and with their serialized `data` adding up to at most 8 MB. Past either cap the oldest go. Numbering, keeping and sending happen under one lock, and a follower copies the ring and subscribes under the same lock, so a replay never skips or repeats an event.
- **Resuming.** `GET /v1/events` reads `after` from the query, else the `Last-Event-ID` header. A non-empty query wins over the header; an empty value of either counts as not sent. `<epoch>-<n>` resumes when the epoch is this run's and `last - kept <= n <= last`, where `last` is the latest number given (0 before any) and `kept` how many events the ring holds: the stream is the kept events after `n`, in order, then live events, none twice.
- **`resync`.** Any other id gets, as the stream's first event, one `resync` with data `{"epoch": "<this run's epoch>"}` and no id, then live events: another epoch (a restart means `resync`), a number older than the ring, a number ahead of `last`, and an id that doesn't parse. `resync` is never kept or sent to anyone else. A client that drops again before any live event resumes from its old id and gets one more `resync`.
- **No id, no replay.** A client that names no id gets live events only, with their `id:` lines, and never a `resync`.
- **Never a gap from the numbers.** A client must never infer a gap from a jump in `seq`: the gateway drops the Mac's own frames from a phone's stream, and `delta` has no number. Only `resync` means "fetch again".

### Failures and recovery

- **Falling behind.** A follower more than 1,024 events behind on the live main queue still has its stream ended there; it can now come back with `after`. One more than 1,024 `transcript` events behind still gets one `transcriptStale` and goes on.
- **A restart.** The ring and the numbers are in memory only: after a restart every id a client holds is from another epoch and gets `resync`.

### Invariants and boundaries

- A replay hands a follower only what the same route would have sent it live. Through the gateway, replayed frames pass `Scrubber::frame` like live ones: the Mac's own events (`tetrix`, `hooks`, `watch`, `roomPost`) are left out, every `data` is scrubbed, and the `id:` line passes as it is. A paired phone gets nothing it couldn't already see.
- The API stays add-only: the `id:` line, the `after` query, the `Last-Event-ID` header and the `resync` event are new; no event is renamed and no data changes. Old parsers (the Electron app's `sseReader`, the Swift apps' `EventParser`) read only `event:` and `data:` lines and never send `after`, so they never get `resync`.
- The `WIRE-4` comments on the tests in `events.rs` predate [wire.md](wire.md), whose WIRE numbers mean something else; EVENTS doesn't reuse WIRE numbers.

## Interfaces and dependencies

- `GET /v1/events`: the SSE `id:` line on numbered events; the query `after=<epoch>-<seq>` and the header `Last-Event-ID`; the event `resync`, `{"epoch": string}`, sent only to a client that asked to resume (docs/runner-api.md, `GET /v1/events`).
- `GET /v1/status`: `events-replay-v1` in `capabilities` (R06-01, [capabilities.md](capabilities.md)).
- The gateway: no change. A phone's `?after=` reaches obid with the path and query; `Last-Event-ID` isn't forwarded.

## Acceptance scenarios

- **EVENTS-1 (implemented):** Given obid sends a `job` event, when a client follows `GET /v1/events`, then the frame has an `id: <epoch>-<seq>` line, the seq is one more than the last numbered event's, and the data is unchanged; `transcript` and `delta` get no id and aren't kept. Test: `events_1_numbered_events_carry_an_id` in `core/crates/obid/src/events.rs`.
- **EVENTS-2 (implemented):** Given a client saw id E-10 and obid then sent E-11 to E-15, when it reconnects with `?after=E-10`, then it gets E-11 to E-15 in order, then live events, and none twice. Test: `events_2_reconnect_replays_what_was_missed` in `core/crates/obid/tests/runner_api.rs` (seen failing with the replay disabled, R06-02).
- **EVENTS-3 (implemented):** Given a client's last id is from an earlier epoch or older than the ring, when it reconnects with that id, then the first event is one `resync` with the current epoch, and live events follow. Tests: `events_3_unknown_epoch_or_too_old_gets_resync` in `core/crates/obid/tests/runner_api.rs` (another epoch, an id that doesn't parse) and `events_3_older_than_the_ring_gets_resync` in `core/crates/obid/src/events.rs` (the 2,000-event and 8 MB caps, an id older than the ring, one ahead of it).
- **EVENTS-4 (implemented):** Given a client reconnects with `Last-Event-ID: E-10` and no query, when obid answers, then it replays as for `?after=E-10`; with both, the query wins. Test: `events_4_last_event_id_header_resumes` in `core/crates/obid/tests/runner_api.rs`.
- **EVENTS-5 (implemented):** Given an old client that sends no id (or an empty one), when it follows `GET /v1/events`, then it gets no `resync` and no replay, only live events as today, with `id:` lines it ignores. Test: `events_5_no_id_no_replay` in `core/crates/obid/tests/runner_api.rs`.
- **EVENTS-6 (implemented):** Given a phone follows through the gateway with `?after=`, when Mac-only events (`tetrix`, `hooks`, `watch`, `roomPost`) were among those missed, then the phone's replay leaves them out and is otherwise complete, in order and with ids. Test: `events_6_gateway_replay_leaves_out_mac_only_events` in `core/crates/obid/tests/gateway_api.rs`.
