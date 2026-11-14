# Where the owner's words stand

The business behavior contract for a durable delivery status on each message the owner sends in a conversation (roadmap R05-02, milestone R05 Durable words), from [the template](../business-contract-template.md). Scenarios are numbered `DELIVERY-n`; the tests that prove them are named after them (`delivery_1_…`): in `core/crates/obid/tests/plans_api.rs`, `core/crates/obid/src/conversation.rs` and `core/crates/obid/src/delivery.rs`.

## Feature and status

- **Outcome:** obid knows, on disk and in its API, whether each message the owner sent in a conversation still waits, is being answered, was answered, failed or was cancelled. Before this, words waiting behind a turn lived only in obid's memory (`Turns.waiting`), and a cancel left them with nothing recorded. This is the record the next tasks build on: recovery after a restart (R05-03), a durable queue in place of `Turns.waiting` (R05-04), cancelling one queued message (R05-08) and showing it (R05-09).
- **Status:** **Implemented** (R05-02). `interrupted` and the `restore` move are defined but no code path uses them yet: they are R05-03's and R05-04's.
- **Scope:** the owner's words sent by `POST /v1/plans`. **Out of scope:** recovery at start, replaying queued words, a drain in place of `Turns.waiting`, cancelling one queued message, words to a job (`POST /v1/jobs/{id}/tell`, R05-06), steering a running turn (R05-05, R05-07), and any app change.

## Behavior

### Actors and ownership

- **obid** owns the status, the moves between statuses, and when each is written. It keeps what a queued message needs to be sent again.
- **The apps** change nothing yet: the `delivery` key on a turn and the `turnDelivery` event are there for later tasks. An app that doesn't know them shows what it showed before.

### States and moves

A delivery is `{status, by?, why?, error?}` on the owner's turn (`core/crates/obid/src/delivery.rs`). `status` is an open string. The values obid writes:

| Status | Means | Terminal |
|---|---|---|
| `queued` | The words are kept and wait for a turn to take them. | no |
| `starting` | A turn took them and is starting. | no |
| `running` | The turn that carries them has begun; `by` is the user's turn id whose turn that is. | no |
| `answered` | Oby's answer for them is kept (Stop, a spent budget and jobs started count: the answer is kept). | yes |
| `failed` | No agent session could answer; `error` says why, every secret obid knows taken out. | yes |
| `interrupted` | The turn was cut by a restart (R05-03; nothing writes it yet). | yes |
| `cancelled` | `POST /v1/plans/cancel` dropped them. | yes |

The only moves, and which code path makes each today:

| Move | From → to | Made by |
|---|---|---|
| start | `queued` → `starting` | `POST /v1/plans`, when `Turns::wait_turn` gives the words to a turn: every message that turn answers |
| confirm(`by`) | `starting` → `running` | right after the turn begins, before the agent runs; `by` is the latest message's turn id |
| restore(`why`) | `starting` → `queued` | nothing yet (R05-03, R05-04) |
| finish | `running` → `answered`, `failed` (with `error`) or `interrupted` | when the turn ends, after Oby's answer is kept and before the request answers: `failed` when no session could answer (`End::BrainFailed`), else `answered` |
| cancel | `queued` or `running` → `cancelled` | `POST /v1/plans/cancel` for the words waiting in that conversation; the turn under way's own words when that cancelled turn ends, before their request answers `none` |

Any other move is refused, logged, and changes nothing. A terminal status never changes. A status obid doesn't know (a newer obid's) is kept as written and never moves.

- The owner's turn is written `queued` in the same file write that adds it (`Conversations::begin`). No other turn gets a delivery: Oby's turns, a job's end, a routine's turn, a task conversation's words. Words sent again with their `id` (`repeated`) add nothing and move nothing.
- Words that wait behind a turn under way stay `queued` until the next turn takes them; that turn takes every message waiting there, and each gets `by` the latest message's turn id. The earlier messages' requests answer `none`, as before.
- Each move after the first is written once (one write per move asked, never per streamed delta) and goes out on a `turnDelivery` event carrying the turn. It never goes out as a `turn` again: the Mac's notifications take back the alert of Oby's answer on every `turn` of the owner's (`notify.ts`, `heard()`).

### What a queued message keeps for a replay

- In the same write as the turn, the conversation's file keeps, by the turn's id, the paths of the files attached to the words and whether an app of the owner's signed them (`replays` on the conversation, `{attachments, signed}`). The jobs attached are the turn's own `jobs`.
- A `replays` entry with no `signed` reads as unsigned (fail closed, TASKCONV-6). An entry goes when its turn is pruned past the 500 kept.
- None of it is served: not in `GET /v1/conversation`, not in a `turn` or a `turnDelivery` event. The key is left out of the file when it's empty.

### Failures and recovery

- **A refused move** (from a status that doesn't allow it): logged with the conversation, the turn, its status and the move asked; that turn is left as it was, and nothing is written or sent for it.
- **An old file:** a turn written before deliveries has no `delivery` key. It reads as before, is served with no `delivery`, and a move asked of it is skipped silently: no write, no event.
- **A failed write:** the conversation goes back to what it was in memory, the failure is logged, and no event goes out. The request answers as it would have.
- **A restart** while words are `queued`, `starting` or `running` leaves them so on disk (R05-03 decides what happens next).

### Invariants

- **Add-only API.** A new key on a turn (`delivery`, left out when there is none) and a new event (`turnDelivery`), which every app ignores until it knows it. `status` is an open string. Nothing renamed or removed.
- **`signed` fails closed.** Missing reads as unsigned.
- **`error` is redacted.** It is the text of Oby's failed answer, passed through every secret obid knows.
- **No repeated `turn`.** Only the first status, `queued`, comes on the turn's own `turn` event.

## Interfaces and dependencies

- `GET /v1/conversation`: `delivery` on the owner's turns sent by `POST /v1/plans` (docs/runner-api.md).
- Events: `turn` carries the first `queued`; `turnDelivery` carries each later change.
- `POST /v1/plans/cancel`: still answers `{"cancelled": true}`; the words it dropped read `cancelled`.
- Persistence: `home/conversations/<id>.json`, the turn's `delivery` and the conversation's `replays`.

## Acceptance scenarios

- **DELIVERY-1** Given no turn under way in a conversation, when the owner's words arrive by `POST /v1/plans`, then their only `turn` event carries `delivery.status` `queued` (on disk before the agent starts), then `turnDelivery` events say `starting`, `running` with `by` their own turn id, and `answered`, the last after Oby's `turn`; `GET /v1/conversation` and the conversation's file read `answered` with `by` their own id. Tests: `delivery_1_words_are_queued_then_running_then_answered` (tests/plans_api.rs); for the replay record, `delivery_1_queued_words_keep_what_a_replay_needs` (conversation.rs): the paths and `signed: true` are in the same file as the `queued` turn, never served, and an entry with no `signed` reads unsigned.
- **DELIVERY-2** Given a turn under way, when the owner sends two more messages, then both stay `queued` while it runs, then both are `running` and `answered` with `by` the latest message's turn id; the earlier one's request answers `none`, the latest's carries the reply. Test: `delivery_2_words_said_meanwhile_are_answered_by_one_turn` (tests/plans_api.rs).
- **DELIVERY-3** Given a turn under way and words waiting behind it, when `POST /v1/plans/cancel` names the conversation, then it answers `{"cancelled": true}`, both requests answer `none`, both turns read `cancelled`, and a later turn there doesn't move them. Test: `delivery_3_cancel_marks_running_and_waiting_words_cancelled` (tests/plans_api.rs).
- **DELIVERY-4** Given no agent session can answer (`End::BrainFailed`), when the turn ends, then its words read `failed` with an `error` that has every secret obid knows taken out. Test: `delivery_4_a_failed_turn_says_why_with_no_secret` (tests/plans_api.rs).
- **DELIVERY-5** Given a conversation file written before deliveries, when obid reads it, then its turns have no `delivery`, are served with none, and a move asked of them changes nothing, sends nothing and writes nothing. Test: `delivery_5_an_old_conversation_file_reads_as_before` (conversation.rs).
- **DELIVERY-6** Given a delivery that is `answered`, `cancelled`, `failed`, `interrupted` or a status obid doesn't know, when any move is asked, then it is refused and the delivery is unchanged; only `queued` may start, cancel moves only `queued` or `running`, each allowed move gives the status and the `by`, `why` or `error` above, and an unknown status round-trips unchanged. Test: `delivery_6_a_terminal_delivery_never_moves` (delivery.rs).
