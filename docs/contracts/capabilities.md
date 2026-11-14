# Capabilities: which optional features obid serves, by name

The business behavior contract for `capabilities` on `GET /v1/status`, from [the template](../business-contract-template.md). Roadmap task R06-01, milestone R06, Events that clients only apply.

## Feature and status

- **Outcome:** A client learns which optional features obid serves by stable name, instead of guessing from obid's version or from a `404`. A new optional feature ships its name in the same pull request.
- **Status:** **Implemented** by R06-01 (`CAPABILITIES` and `Status.capabilities` in `core/crates/obid/src/api.rs`; docs/runner-api.md, `GET /v1/status`, "Capabilities"). R06-02 and R06-03 add their names. R06-05 gates the Electron app on it.
- **Scope:** `capabilities` on `GET /v1/status`, a static sorted list of names with a version suffix, its first name `folders-v1`; the Capabilities table in docs/runner-api.md; the test that keeps them equal. Outside it: request headers such as `Oby-Client` or `Oby-Capabilities`, and obid answering differently per client; deleting #1653's dead routes and the Bonjour path (R01 and R03); a client gating an action on a name (R06-05 for the Electron app); the iPhone and Apple Watch apps, which are frozen; names for other features (R06-02, R06-03); removing or deprecating any existing key.

## Behavior

### Actors and ownership

- **obid** owns the list and the names. The list is static, in the code: no runtime state, setting or request changes it, and every caller gets the same answer.
- **The apps** only read it. Nothing changes for clients in this task.

### The rules

- **A name means one behaviour forever.** A changed behaviour gets a new name, such as `folders-v2`; the old name is never reused for it.
- **An obid without a name answers as an older obid does.** A name missing from the list means that obid doesn't serve the feature, whatever its version.
- **A client hides the action whose name is missing.** An obid older than R06-01 sends no `capabilities` key: a client then falls back to that feature's own key (for example `folders`) or treats the feature as absent.
- **A new optional feature ships its name, and its row in docs/runner-api.md's Capabilities table, in the same pull request.** CAPS-2's test fails while either one is missing.
- **The list is sorted, with no duplicate.** It is a plain JSON list of strings, never an enum a client decodes as closed, and the key is always sent, even when the list is empty.
- **The existing booleans stay** (`folders`, `receipts` and the rest), with their names, types and meanings, for the released apps that decode them.

### Failures and recovery

- None of its own: the list is a constant, so it can't fail to be read. A status that fails, fails as before.

### Invariants and boundaries

- `GET /v1/status` stays add-only: `capabilities` is a new key, and nothing is renamed or removed.
- The names say what obid serves, nothing about the owner, the Mac or its accounts.

## Interfaces and dependencies

- `GET /v1/status`: `"capabilities": ["folders-v1"]`, add-only (docs/runner-api.md, `GET /v1/status`, with its Capabilities table: Name, What it means, Since).
- `folders-v1`: obid takes working folders (`folders` on `POST /v1/plans`, on conversations and on `PATCH /v1/conversations/{id}`), the behaviour `folders: true` announces, since #1572.
- Older clients ignore the key: the iPhone's `RunnerStatus` (Swift's `Decodable`) and the Electron app's `Status` ignore unknown keys.

## Acceptance scenarios

- **CAPS-1 (implemented):** Given any obid, when a client reads `GET /v1/status`, then `capabilities` is a list of strings, sorted, with no duplicate, and `folders-v1` is in it exactly when `folders` is `true`. Test: `caps_1_status_lists_capabilities` in `core/crates/obid/tests/runner_api.rs`.
- **CAPS-2 (implemented):** Given the Capabilities table in docs/runner-api.md, when the test reads it and the list obid serves, then each served name has a row and each row's name is served. Test: `caps_2_every_capability_is_documented` in `core/crates/obid/tests/runner_api.rs` (seen failing on an undocumented name, R06-01).
- **CAPS-3 (implemented):** Given an older client that decodes the status without `capabilities`, when it reads `GET /v1/status`, then it decodes as before: the key is added, nothing renamed or removed. Test: `caps_3_old_status_decoders_still_read` in `core/crates/obid/tests/runner_api.rs`; `core/crates/obid/tests/swift_client.rs` covers the Swift side.
