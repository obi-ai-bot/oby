# Receipts: a request sent twice gets the first answer

The business behavior contract for obid's receipt store, from [the template](../business-contract-template.md). Roadmap tasks R02-03 (the store), R02-04 (`POST /v1/jobs`) and R02-06 (brain tool calls and obid's own Try again), milestone R02, Commands that are safe to repeat.

## Feature and status

- **Outcome:** One place in obid remembers, for 24 hours and across restarts, which request key got which answer. A request that creates work, sent again with the same key (a phone that lost the answer, an app that retried), gets the first answer again instead of a second job. Every endpoint family that creates work uses this store instead of a rule of its own.
- **Status:** The store is **built** (`core/crates/obid/src/receipts.rs`), **`POST /v1/jobs` uses it** (R02-04, `dispatch` in `core/crates/obid/src/routes.rs`; `docs/runner-api.md`, "A request key"), and so do a conversation's `oby_dispatch` and `oby_tell_job` and obid's own Try again (R02-06, **implemented**: `once` in `core/crates/obid/src/brain/tools.rs`, `Jobs::try_again` in `core/crates/obid/src/jobs/retry.rs`, both through `receipts::once`). The other routes that create work do not yet: messages, the inbox, stop and move (R02-05). RECEIPTS-5 to 7 and 12 to 14 are **implemented**; RECEIPTS-1 to 4 stay **proposed** until a route reaches people with them.
- **Scope:** The store: its files, its lock per key, its 24-hour window and its sweep; and `POST /v1/jobs`'s request key (`job.clientRequestId` or `Idempotency-Key`), the Job's `clientRequestId`, `receipts` on `GET /v1/status`, and the gateway forwarding `Idempotency-Key`; a conversation's `oby_dispatch` and `oby_tell_job` repeated in one turn, and obid's own Try again (the brain's `retry_job`, a `failed` item's Try again, a Confirm of `retry_job`) once per job revision (R02-06). Outside it: the other routes (R02-05), the `limit` item's Try again, `oby_continue_job`, the owner's own Try again sent as words, Oby's review's Try again and One more round, past-safety Confirms, the other brain tools, any client sending a key (R02-07, R02-08), keys kept apart per caller, how `POST /v1/plans` dedupes its turn id (`brain/mod.rs`, `begun.repeated`, unchanged), an API to list receipts, and a setting for the window.

## Behavior

### Actors and ownership

- **obid** owns the receipts: it writes, reads, judges and sweeps them. No model call decides anything here. It also makes the keys of its own: one for each `oby_dispatch` or `oby_tell_job` call of a conversation's turn, and one for each Try again of its own; the brain's CLI is a client of obid's MCP server, and its keys are obid's, never the agent's.
- **The apps** send nothing new yet: `POST /v1/jobs` takes a key, and the Electron app (R02-07), the iPhone and oby-mcp (R02-08) send one later. Any caller of that route may: the API token or a paired device through the gateway, all the owner's own, so keys aren't kept apart per caller.

### Rules

- **The key.** A key is 1 to 128 characters. Any other key is refused with an error, and nothing is read or written for it.
- **The file.** A receipt is one file, `home/receipts/<hex sha256 of the key>.json` (0600, written whole and renamed into place, in a 0700 folder). The raw key is never part of a path: a key with `/` or `..` is as safe as any other.
- **What it holds.** `key`, `route` (such as `POST /v1/jobs`), `bodyHash` (the hex sha256 of the body's canonical JSON: its object keys sorted at every depth, so the same body with its keys in another order has the same hash), `status` (the HTTP status), `answer`, `job` (the job it started, or none) and `created` (seconds since the epoch).
- **Put.** The answer is written as its JSON text with every secret obid knows taken out (`secrets::known().redact`), and returned as that text.
- **Get.** For a key, a route and a body:
  - **None:** there is no receipt, or its file doesn't read, or it is 24 hours old or older (`now - created >= 86400`), whether the sweep has reached it yet or not.
  - **Repeat:** the route and the body's hash both match; the stored status, answer and job come back.
  - **Conflict:** the receipt is under 24 hours old but its route or body hash differs. One key names one request.
- **The lock.** The caller takes a lock per key (an async call that returns a guard) around its get, its work and its put: a second request with the same key waits until the first guard is dropped, then finds the first one's receipt. Requests with different keys don't wait for each other. A key's lock nobody holds is dropped from memory at the latest when the sweep runs.
- **Sweep.** When obid starts and once an hour after that, every receipt 24 hours old or older is removed; the sweep says how many.

#### `POST /v1/jobs` (R02-04)

- **Reading the key.** Once the body is JSON with `job` an object, obid reads the key from `job.clientRequestId` and from the `Idempotency-Key` header. `"clientRequestId": null` is none. It answers `422` and makes nothing when `clientRequestId` isn't a string, either key is empty or longer than 128 characters, the header isn't visible ASCII or comes more than once, or both keys are sent and differ. This runs before the job is described and before its signed words are checked.
- **No key.** Nothing changes: no receipt is read or written, and every answer and refusal is as before.
- **The lock.** With a key and a store, obid takes the key's lock and holds it across the lookup, making the job, marking it unsigned and writing the receipt, under the route `POST /v1/jobs`. The body it judges is the parsed body with `job.clientRequestId` set to none, so a key in the header one time and in the body the next is still the same request.
- **New.** No receipt: the job is made as before and answers `201`, carrying `clientRequestId`; then the receipt is written with `201`, that answer and the job's id.
- **Repeat (RECEIPTS-5).** `200` with the job as it is now, or the receipt's answer when obid no longer has the job. Nothing is made: the repeat's `X-Oby-Words` aren't checked or used up, and the first job's signed or unsigned mark and mode stay as they were.
- **Conflict (RECEIPTS-6).** `409` with an `{"error": …}` line, and nothing is made.
- **At once (RECEIPTS-7).** Two requests with one key wait for each other on its lock: one job, a `201` and a `200` naming it.
- **Refused requests keep no receipt.** A `400`, `410` or `422` (PUBLISHING-4's busy branch and `runsOn` included) writes none, so the corrected request can use the key.
- **The Job's key.** The job's file and its JSON carry `clientRequestId`, add-only, set just after the job is made (under the lock, so a repeat never sees a job without it); the job's first `job` event leaves it out.

#### Brain tool calls (R02-06)

- **The key.** obid keys a call of `oby_dispatch` or `oby_tell_job` only in a turn with a turn id (a turn of the owner's words, not an event's) and only with a store. The key is `tool:` and the hex sha256 (`receipts::body_hash`) of `{"turn": <turn id>, "tool": <name>, "arguments": <the call's arguments>}`; the route is the tool's name, and the body that same object. The agent's `tool_use` id and the JSON-RPC id aren't part of it: a CLI that repeats itself sends new ones.
- **No turn id, or no store.** The call runs exactly as before.
- **The lock.** obid holds the key's lock from the lookup until the receipt is written, so two copies arriving together run one after the other.
- **Repeat (RECEIPTS-12, 14).** The tool answers the stored answer as a success. Nothing is made, relayed, confirmed or put on the turn's cards, and nothing counts against the turn's ten. A conflict or a store that fails counts as no receipt.
- **New.** The call runs as before. An `oby_tell_job` that answers gets a receipt with its answer and the follow-up it made, if it made one; an `oby_dispatch` that started a job gets one with its answer and the job's id. A refused call, and an `oby_dispatch` that only left a Bypass Confirm, keep none.
- **The job's key.** A job `oby_dispatch` started under a key, and a follow-up `oby_tell_job` made under one, carry it as `clientRequestId`, set under the key's lock before the receipt is written. A job that only heard the words (a running job, or the follow-up already on it) isn't marked.
- **Another turn.** The same arguments in another turn make another key: the call runs as new.

#### obid's own Try again (R02-06)

- **Who.** The brain's `retry_job`, a `failed` item's Try again and a Confirm of `retry_job` go through `Jobs::try_again`, each with the revision of the job it read when the Try again was decided: the brain's plan with the job it was given, the inbox's when the answer is carried out.
- **The key.** `try-again:<job id>:<revision>`, the route `Try again`, the body `{"job": <id>, "revision": <revision>}`. obid holds the key's lock from the lookup until the receipt is written.
- **Repeat (RECEIPTS-13).** The job as it is now (none when it's gone): no round runs, no words are sent, and the job is left as it is (the owner's Try again gives nothing back again).
- **New.** Try again's words go to the job as before (`Jobs::message`); only when that went through does the owner's Try again give the 10 back (RECOVERY-17) and is the receipt written, with the job's id. A refused Try again keeps none.
- **No store.** Exactly as before.
- Every other refusal, `409` and recovery of these three paths is unchanged.

### Failures and recovery

- **A file that doesn't read** (unreadable, or not a receipt): get logs it and counts it as none, so the request runs as new; the sweep logs it, leaves it and goes on.
- **A file the sweep can't remove** is logged and left; the next sweep tries again, and get never returns it once it is 24 hours old.
- **A failed sweep** (the folder can't be read) is logged and never stops obid.
- **`home/receipts/` can't be made** when obid starts: this is logged and obid starts anyway, with no receipt store.
- **A restart** keeps every receipt: they are files, and a new store on the same home finds them.
- **No store** (`home/receipts/` couldn't be made): a keyed `POST /v1/jobs` runs as one without a key, with no lookup and no receipt, and the job still carries its key; `receipts` on `GET /v1/status` is `false`.
- **A receipt that can't be written** after the job is made: logged, and the answer is still `201`, since the job exists and failing the request would invite the duplicate this prevents. So for a brain call and a Try again: logged, and the first answer stands.

### Invariants

- Only the body's hash is kept: the request body is never written anywhere.
- The answer is kept redacted.
- A key never becomes a path.
- A receipt 24 hours old or older is never returned.

## Interfaces and dependencies

- **Persistence:** `home/receipts/*.json`, obid's alone.
- **API:** `api::Shared` carries the store as `receipts` (none when its folder couldn't be made). `POST /v1/jobs` takes `job.clientRequestId` or `Idempotency-Key` and answers `200` on a repeat and `409` on another body; the Job gains `clientRequestId`; `GET /v1/status` gains `receipts`; the gateway forwards `Idempotency-Key` (`docs/runner-api.md`). A conversation's `oby_dispatch` and `oby_tell_job`, and obid's own Try again, use it with keys of obid's own (R02-06), the first two carried on the job they made as `clientRequestId`; `Link` gets the store as it gets the relay (`use_receipts`). The other routes that create work (R02-05) don't use it yet.

## Acceptance scenarios

- **RECEIPTS-1** (proposed). Given a receipt for key K, when the same route asks with K and the same body within 24 hours, then the store returns the first status and answer. A second lock on K waits while the first is held. Test: `receipts_1_a_repeat_gets_the_first_answer` (core/crates/obid/src/receipts.rs).
- **RECEIPTS-2** (proposed). Given a receipt for key K, when a request with K has a different body, then the store says conflict; so it does for the same body on another route. Test: `receipts_2_another_body_is_a_conflict` (core/crates/obid/src/receipts.rs).
- **RECEIPTS-3** (proposed). Given a receipt written before obid restarted, when obid starts, then the receipt is still found. Test: `receipts_3_a_restart_keeps_receipts` (core/crates/obid/src/receipts.rs).
- **RECEIPTS-4** (proposed). Given a receipt older than 24 hours, when obid sweeps, then the file is gone and K is free. Test: `receipts_4_a_day_old_receipt_goes` (core/crates/obid/src/receipts.rs).
- **RECEIPTS-5** (implemented). Given `POST /v1/jobs` answered `201` with key K, when the same body comes again with K, then obid answers `200` with the same job and makes none. Test: `receipts_5_a_repeated_job_request_makes_one_job` (core/crates/obid/tests/jobs_api.rs); through the gateway, `receipts_5_a_phones_repeated_job_request_makes_one_job` (core/crates/obid/tests/gateway_api.rs).
- **RECEIPTS-6** (implemented). Given `POST /v1/jobs` answered with key K, when another body comes with K, then obid answers `409` and makes nothing. Test: `receipts_6_another_body_under_a_key_is_409` (core/crates/obid/tests/jobs_api.rs).
- **RECEIPTS-7** (implemented). Given two `POST /v1/jobs` with key K at the same moment, then exactly one job is made and both answers name it. Test: `receipts_7_two_requests_at_once_make_one_job` (core/crates/obid/tests/jobs_api.rs).
- **RECEIPTS-12** (implemented). Given a conversation turn whose `oby_tell_job` to an ended job started a follow-up, when the same call comes again in that turn, then no second follow-up starts and the tool answers with the first. Test: `receipts_12_a_repeated_tell_job_makes_one_follow_up` (core/crates/obid/tests/conversation_harness.rs).
- **RECEIPTS-13** (implemented). Given an ended job at revision R, when Try again for revision R arrives twice, then the job runs one more round. Test: `receipts_13_try_again_twice_is_one_round` (core/crates/obid/src/jobs/retry.rs); with no store, `receipts_13_with_no_receipts_try_again_is_as_before`.
- **RECEIPTS-14** (implemented). Given `oby_dispatch` called twice with the same call in one turn, then one job starts. Test: `receipts_14_a_repeated_dispatch_makes_one_job` (core/crates/obid/tests/conversation_harness.rs).
