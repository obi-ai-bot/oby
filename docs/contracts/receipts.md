# Receipts: a request sent twice gets the first answer

The business behavior contract for obid's receipt store, from [the template](../business-contract-template.md). Roadmap task R02-03 (milestone R02, Commands that are safe to repeat).

## Feature and status

- **Outcome:** One place in obid remembers, for 24 hours and across restarts, which request key got which answer. A request that creates work, sent again with the same key (a phone that lost the answer, an app that retried), gets the first answer again instead of a second job. Every endpoint family that creates work uses this store instead of a rule of its own.
- **Status:** The store is **built** (`core/crates/obid/src/receipts.rs`), and **no route uses it yet**: R02-04 to R02-06 wire the routes, and `docs/runner-api.md` gets the request key then. RECEIPTS-1 to 4 stay **proposed** until a route reaches people with them.
- **Scope:** The store: its files, its lock per key, its 24-hour window and its sweep. Outside it: a request-key header or body field, any route, any change to `docs/runner-api.md`, how `POST /v1/plans` dedupes its turn id (`brain/mod.rs`, `begun.repeated`, unchanged), an API to list receipts, and a setting for the window.

## Behavior

### Actors and ownership

- **obid** owns the receipts: it writes, reads, judges and sweeps them. No model call decides anything here.
- **The apps** see nothing new, and send nothing new, until a route takes a request key.

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

### Failures and recovery

- **A file that doesn't read** (unreadable, or not a receipt): get logs it and counts it as none, so the request runs as new; the sweep logs it, leaves it and goes on.
- **A file the sweep can't remove** is logged and left; the next sweep tries again, and get never returns it once it is 24 hours old.
- **A failed sweep** (the folder can't be read) is logged and never stops obid.
- **`home/receipts/` can't be made** when obid starts: this is logged and obid starts anyway, with no receipt store.
- **A restart** keeps every receipt: they are files, and a new store on the same home finds them.

### Invariants

- Only the body's hash is kept: the request body is never written anywhere.
- The answer is kept redacted.
- A key never becomes a path.
- A receipt 24 hours old or older is never returned.

## Interfaces and dependencies

- **Persistence:** `home/receipts/*.json`, obid's alone.
- **API:** none yet. `api::Shared` carries the store as `receipts` (none when its folder couldn't be made) for the routes R02-04 to R02-06 wire.

## Acceptance scenarios

- **RECEIPTS-1** (proposed). Given a receipt for key K, when the same route asks with K and the same body within 24 hours, then the store returns the first status and answer. A second lock on K waits while the first is held. Test: `receipts_1_a_repeat_gets_the_first_answer` (core/crates/obid/src/receipts.rs).
- **RECEIPTS-2** (proposed). Given a receipt for key K, when a request with K has a different body, then the store says conflict; so it does for the same body on another route. Test: `receipts_2_another_body_is_a_conflict` (core/crates/obid/src/receipts.rs).
- **RECEIPTS-3** (proposed). Given a receipt written before obid restarted, when obid starts, then the receipt is still found. Test: `receipts_3_a_restart_keeps_receipts` (core/crates/obid/src/receipts.rs).
- **RECEIPTS-4** (proposed). Given a receipt older than 24 hours, when obid sweeps, then the file is gone and K is free. Test: `receipts_4_a_day_old_receipt_goes` (core/crates/obid/src/receipts.rs).
