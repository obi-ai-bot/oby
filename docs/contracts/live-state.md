# What obid has, as the app keeps it

The business behavior contract for the one rule by which obid's jobs reach the Electron app's state, from [the template](../business-contract-template.md).

## Feature and status

- **Outcome:** The owner sees the same tasks and cards as before. What changes is underneath: whether a job from obid replaces the one the app holds is decided in one place, a pure function tested on its own, instead of by hand at each event and each answer in `live.ts`. A late answer can't undo a newer event, and a deleted task can't come back.
- **Status:** **Implemented** for jobs only (roadmap R06-06): `apps/electron/src/shared/live-state.ts`, used by `apps/electron/src/renderer/src/live.ts`. The other kinds (inbox, turns, conversations, accounts, GitHub's work, the room and peer threads) stay in `live.ts` until R06-07; running the rule in main is R06-08. Related: #1707 (an answer applied over the state as it is when it comes) and #1709 (revisions and deleted jobs).
- **Scope:** The `job` and `jobGone` events, every job a call to obid answers with, and the job half of the reconnect's sync. **Out of scope:** every other event kind, a job's session (its transcript), the jobs the app saw when it opened, any screen, and obid and its API ([runner-api.md](../runner-api.md) is unchanged).

## Behavior

### Actors and ownership

- **obid** owns a job's `revision` (`core/crates/obid/src/job.rs`, `Job::revision`): one more each time it sends the job changed, kept across restarts, none from an older obid ([runner-api.md](../runner-api.md), "revision"). It decides when a job is deleted, and says so with a `job` event with `deleted: true`.
- **The app** only applies what obid sent. It decides nothing about a job but which of two copies it keeps.

### The rule

The state is `ObidState`: the jobs by id, and `gone`, the ids of the jobs obid said it deleted. `apply(state, event)` takes a `job` or `jobGone` event; `answer(state, job)` takes a job a call returned, by the same rule.

- A job whose id is in `gone` leaves the state unchanged, from an event or an answer.
- Otherwise, when the state holds that job with a `revision` higher than the incoming job's, the held copy stays. On a tie the incoming one is taken.
- Otherwise the incoming job is taken, including one with no `revision` (an older obid, after a downgrade): it is taken as it comes, never held off by a revision from before.
- `jobGone` removes the job and adds its id to `gone`.

### Triggers

- An event from obid's stream, applied as it comes.
- obid's answer to a call on a job (stop, mode, rename, archive, a message, an answer to its question, starting GitHub's work, fetching one job), and the jobs a plan's answer carries, applied over the state as it is when the answer comes, never over a state read before the call (#1707, #1709).
- The reconnect's sync, below.

### Failures and recovery

- An answer and an event that crossed: the higher revision wins, whichever came last.
- A deleted job's answer that crossed its `deleted` event: it stays out.
- **After a reconnect** (the sync), obid's list of jobs is the truth, since a revision can go back (a restart after a save that didn't land, a downgrade and upgrade again). For each listed job:
  - whose copy in the app didn't change while the list was read, obid's listed copy is taken whatever its revision, and over an older kept copy with the same id;
  - whose copy changed while the list was read (an answer came meanwhile), the newer of the two wins by the rule;
  - deleted ones stay out.

  The older jobs a conversation fetched, past obid's latest 100, are kept. The decisions on deleted jobs and revisions in this merge are the rule's; the sync only says which copy counts as held. The events that came while the list was read are then applied over it.
- A deleted job's session (its transcript, if the app had it) is let go by the code that handles the `jobGone` event, never by the rule.

### Invariants and boundaries

- The rule is pure: no I/O, no module state, nothing of the screens or of Electron; it imports only obid's wire types. It never changes the state it's given, and the same events from the same state give the same result.
- It's written once: nothing else in the app compares job revisions or adds to `gone`.
- A rename shows its new title before obid answers, and puts the old one back if obid refuses: that's not an answer and doesn't go through the rule. It keeps the held revision, so it can't break it.

## Interfaces and dependencies

Reads the `job` event (`deleted: true` parsed as `jobGone` by `parseEvent` in `apps/electron/src/shared/obid-wire.ts`) and the jobs obid's routes answer with, as [runner-api.md](../runner-api.md) describes them. No new route, key or value in obid's API.

## Acceptance scenarios

Tests: [`apps/electron/src/renderer/src/test/liveState.test.ts`](../../apps/electron/src/renderer/src/test/liveState.test.ts) calls the rule directly. Through `live.ts`, [`taskActions.test.ts`](../../apps/electron/src/renderer/src/test/taskActions.test.ts) covers the answers that cross events ("obid's answers over the events that came meanwhile", #1707), a job with no revision, the sync after a restart that took a revision back ("back after obid restarted with a lower revision: obid's list wins, but a newer answer that came while it was read stays") and after a deletion while the stream was away ("back after the stream was away: a task deleted meanwhile (on the iPhone) leaves, the older ones a conversation fetched stay"), and a deleted task not fetched back.

- **LIVE-1** (implemented): Given the state holds job J at revision 5, when a `job` event or an answer brings J at revision 4, then the state keeps revision 5.
- **LIVE-2** (implemented): Given a `job` event with `deleted: true` for J, when any later answer or event brings J, then J stays out of the state.
- **LIVE-3** (implemented): Given J with no revision (an older obid), when it comes, then it is taken as it comes, over a held revision too.
- **LIVE-4** (implemented): Given the same list of events, when `apply` runs twice from the same state, then both results are equal and the state they started from is unchanged.
