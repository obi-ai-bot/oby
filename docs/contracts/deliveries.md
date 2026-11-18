# Where the owner's words stand

The business behavior contract for a durable delivery status on each message the owner sends in a conversation (roadmap R05-02, milestone R05 Durable words), from [the template](../business-contract-template.md). Scenarios are numbered `DELIVERY-n`; the tests that prove them are named after them (`delivery_1_…`): in `core/crates/obid/tests/plans_api.rs`, `core/crates/obid/src/conversation.rs` and `core/crates/obid/src/delivery.rs`. Recovery at start (R05-03) is `Conversations::recover` and `brain::recover`.

## Feature and status

- **Outcome:** obid knows, on disk and in its API, whether each message the owner sent in a conversation still waits, is being answered, was answered, failed or was cancelled. Before this, words waiting behind a turn lived only in obid's memory (`Turns.waiting`), and a cancel left them with nothing recorded. This is the record the next tasks build on: recovery after a restart (R05-03), a durable queue in place of `Turns.waiting` (R05-04), cancelling one queued message (R05-08) and showing it (R05-09).
- **Status:** **Implemented** (R05-02, R05-03: recovery at start). The `restore` move is defined but no code path uses it yet: it is R05-04's.
- **Scope:** the owner's words sent by `POST /v1/plans`, and what becomes of them when obid stops and starts again. **Out of scope:** a drain in place of `Turns.waiting`, cancelling one queued message, words to a job (`POST /v1/jobs/{id}/tell`, R05-06), steering a running turn (R05-05, R05-07), and any app change.

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
| `interrupted` | A stop cut the turn that carried them (DELIVERY-7): obid went down or was stopped while they were `starting` or `running`. | yes |
| `cancelled` | `POST /v1/plans/cancel` dropped them. | yes |

The only moves, and which code path makes each today:

| Move | From → to | Made by |
|---|---|---|
| start | `queued` → `starting` | `POST /v1/plans`, when `Turns::wait_turn` gives the words to a turn: every message that turn answers |
| confirm(`by`) | `starting` → `running` | right after the turn begins, before the agent runs; `by` is the latest message's turn id |
| restore(`why`) | `starting` → `queued` | nothing yet (R05-04) |
| finish | `running` → `answered`, `failed` (with `error`) or `interrupted` | when the turn ends, after Oby's answer is kept and before the request answers: `failed` when no session could answer (`End::BrainFailed`), else `answered`; `interrupted` at start, for a turn a stop cut |
| cut | `starting` → `interrupted` | at start, for words a stop cut as their turn began (DELIVERY-7) |
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
- **A restart** while words are `starting` or `running` (DELIVERY-7, DELIVERY-9): as obid starts, before it answers anything, they read `interrupted`, and their conversation gets one Oby turn in obid's words, written with them in one write and sent as a `turn` event, then a `turnDelivery` for each: `Oby restarted before it finished answering that. Send it again to get an answer.` after a clean stop (an update, a quit: obid's `stopped-cleanly` mark), else `Oby went down before it finished answering that. Send it again to get an answer.` (obid's proposed words, until Sangam's). No agent turn runs them again by itself: the cut turn may have started jobs or merged. The same words sent again with their `id` are a repeat, as before, and add nothing; the owner's next words get their turn as usual.
- **Words that only waited** (`queued` at start, DELIVERY-8): one turn per conversation answers them all, oldest first, as one turn answers words said meanwhile, through the same path (`brain::answer`), each with the files and `signed` its `replays` entry kept, the turn the owner's own only when every one was signed. Nobody's request waits for that answer: it arrives as Oby's `turn`.
- **No delivery** (DELIVERY-10): a conversation whose turns have no `delivery` key, or none left `starting`, `running` or `queued`, gets nothing at start.

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
- **DELIVERY-6** Given a delivery that is `answered`, `cancelled`, `failed`, `interrupted` or a status obid doesn't know, when any move is asked, then it is refused and the delivery is unchanged; only `queued` may start, cancel moves only `queued` or `running`, only `starting` or `running` may be cut to `interrupted`, each allowed move gives the status and the `by`, `why` or `error` above, and an unknown status round-trips unchanged. Test: `delivery_6_a_terminal_delivery_never_moves` (delivery.rs).
- **DELIVERY-7** Given a turn under way, when obid is killed and starts again, then that turn's words read `interrupted`, the conversation has one Oby turn after them saying Oby went down before it finished answering, no agent turn runs the words again, the same words sent again with their `id` answer `repeated` and add nothing, and the owner's next words get their answer. Test: `delivery_7_a_turn_cut_by_a_kill_reads_interrupted_and_is_not_sent_again` (tests/plans_api.rs).
- **DELIVERY-8** Given two messages waiting behind a turn when obid is killed, when obid starts again, then one new turn answers them together (both `answered`, `by` the later's id), and the cut turn's words aren't in it. Test: `delivery_8_words_that_waited_are_answered_after_start` (tests/plans_api.rs).
- **DELIVERY-9** Given obid stopped cleanly (SIGTERM: an update or a quit) with a turn under way, when it starts again, then the cut turn's line says Oby restarted, not that it went down, and the rest is as DELIVERY-7. Test: `delivery_9_a_clean_restart_says_restarted` (tests/plans_api.rs).
- **DELIVERY-10** Given conversations whose words were answered, or whose turns have no `delivery` key, when obid starts, then nothing is added to them. Test: `delivery_10_old_conversations_get_nothing_at_start` (tests/plans_api.rs).
