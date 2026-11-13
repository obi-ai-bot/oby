# CI: what the check guards

The business behavior contract for what the check itself guards, from [the template](../business-contract-template.md). Roadmap milestone R01 (Safety net); CI-9 to 11 are task R01-05, CI-12 and 13 are task R01-06.

## Feature and status

- **Outcome:** obid's API stays add-only (AGENTS.md, "How we work"): the iPhone, the Apple Watch, the Electron app and oby-mcp keep working after obid changes. Removing a route that a client still calls fails the check, as does a client call to a path obid doesn't serve and a gateway allowlist entry with no route, instead of showing up as a `404` on someone's phone.
- **Outcome:** A test that waits on elapsed time fails when the Mac is busy, and a flaky test blocks merges for everyone (#1804, #1656). obid's tests may only lose their `thread::sleep` and `time::sleep` calls, so each one a later change removes stays removed and none comes back.
- **Status:** CI-9 to 11 are **implemented**: one test binary, `core/crates/obid/tests/routes_guard.rs`, with the committed lists `docs/api/routes.txt` and `docs/api/dead-client-calls.txt`. The gateway's dead entries it found were deleted with it; nothing else in obid changed, and no client changed.
- **Status:** CI-12 and CI-13 are **implemented** (task R01-06).
- **Scope:** The paths of obid's routes, of the clients' calls and of the gateway's allowlist. Out of scope: request and response keys, enum values and frozen client decoders (R07); matching a client call by its HTTP method; paths built only by concatenation past a literal prefix; the iPhone and Watch app targets (`apps/ios`, `apps/watch`), the Electron app's tests and mock, `mcp/tests` and `cloud/`; removing the dead client calls themselves (each is a task of its own).
- **Scope:** The count of sleep calls in obid's tests and its limit. Out of scope: converting the sleeps there are today (later changes lower the limit), a clippy `disallowed-methods` rule, any per-file allowance.

## Behavior

### Actors and ownership

- **The check** (`scripts/oby-check.sh`) runs the route guard when a change reaches what it reads (below). No model call is involved, and nothing changes at runtime for obid or any client.
- **The check** counts and judges obid's tests' sleeps in its `rust` group: the block that judges the Rust core's and `jobs.rs`'s line limits, so it runs wherever they run, one group at a time under the installed `oby-check-blok` too.

### Rules

- **The route list.** `docs/api/routes.txt` lists every route obid serves, one `<METHOD> <path>` line per method, the path as axum declares it (`{id}`, `{*path}`), sorted by path, then method. It is rebuilt from every `.route(` under `core/crates/obid/src` whose path is a literal starting `/v1/`, outside test modules (`production_lines`' rule in `scripts/oby-check.sh`) and comment lines; each method router (`get`, `post`, `put`, `patch`, `delete`, or `axum::routing::` one) is a line, and a layer is skipped. A method router the guard doesn't know fails it, naming the file and the path. The routes `gateway.rs` declares itself (`POST /v1/pairing`, on the gateway's own listener) sit under their own heading. A new route commits its line with it; a route that goes shows as a deleted line in review.
- **The clients' calls.** The `/v1/` (or Swift's `v1/`) string literals of `apps/electron/src` (its `.ts` and `.tsx`, not under a `test/` folder, not `*.test.ts(x)`, not `renderer/src/mock.ts`), `apps/shared` and `mcp/server` are paths the clients call. Each is compared, segment by segment and whatever the method, with every route in the list: an interpolated segment matches any, a route's `{x}` matches any one and `{*x}` one or more, and a literal ending in `/` matches when a route starts with it. A query is cut, and an interpolation glued to the end of the text (`/v1/jobs${q(..)}`) is dropped.
- **The known-dead list only shrinks.** `docs/api/dead-client-calls.txt` holds the calls that were already dead when the guard came, `<file> <path>` a line. A dead call not on it fails, and so does a line whose call is gone from its file or served again: delete that line.
- **The gateway's list holds only served routes.** Each `ROUTES` entry in `core/crates/obid/src/gateway.rs` (`*` one segment, a final `**` one or more) must reach a route of the API's with the same method; one that reaches none fails, and is deleted.
- **What sleeps count.** Each occurrence of `thread::sleep` or `time::sleep` (the pattern `(thread|time)::sleep`, written once in `tests_sleeps`) in a `*.rs` file under `core/crates/obid/tests`, however many share a line. No exemptions: `support::within`'s own 100 ms poll counts like any other. A missing folder counts 0.
- **The sleep limit.** `tests_sleep_limit` in `scripts/oby-check.sh`. It may only come down.
- **Shrink-only, as the line limits.** A job's check runs the base's copy of the script on the commit (PUBLISHING-7). `pick_limit` holds a branch that lowered `tests_sleep_limit` since its merge base to its own number, but never to one higher than the running copy's; one that didn't gets the running copy's. When dev's limit changed since the branch started, the messages add "rebase onto dev".
- **Past the sleep limit** fails (CI-12). **Under it** fails too (CI-13), asking for the exact new count. **At it** passes, and says so in one line.

### Failures and recovery

- Each route guard failure names what to fix: the route that went or appeared, the file and path of a dead call, the list line to delete, or the gateway entry. The fix is the route put back, its line committed, the call changed, or the entry or line deleted.
- Past the sleep limit: replace the new sleep with a wait for state (`support::within`, or `events_named` on `follow_events`). Under it: lower `tests_sleep_limit` to the count the check names, in the same change.

### Invariants

- The guard never writes a list: a person commits each change to it, so it shows in review.
- `gateway::ROUTES` stays private: the guard reads gateway.rs's source.
- A test waits for state, never for time: a new sleep fails the check.
- Raising `tests_sleep_limit` is a change to the check, a security path: it merges only after two reviews by different tools or the owner's click (MERGING-13 to 16).

## Interfaces and dependencies

- **Files:** `docs/api/routes.txt`, `docs/api/dead-client-calls.txt`, read by `core/crates/obid/tests/routes_guard.rs`.
- **Check:** `scripts/oby-check.sh` plans the Rust group with `tests/routes_guard.rs` when `core/crates/obid/src/api.rs` or `gateway.rs` changes, another file under `core/crates/obid/src/` with a `.route(` in it, anything under `docs/api/`, or a client source as above (`apps/electron/src` but its tests and mock, `apps/shared/*.swift`, `mcp/server/*.swift`); the groups those paths planned before stay as they were. `scripts/oby-check-test.sh` proves that plan.
- **Cost:** an Electron-only or Swift-only change to client sources now also runs `cargo fmt`, clippy on obid and a build of one obid test binary.
- **Sleep limit:** `scripts/oby-check.sh` (`tests_sleep_limit`, `tests_sleeps`), its `rust` group. Failure name: `obid's tests' sleep limit`. Its cases in `scripts/oby-check-test.sh` run on the commit's own copy of the script (the base's copy a job's check installs may predate the limit).

## Acceptance scenarios

- **CI-9** (implemented). Given a change that removes a route from obid, when the check runs, then it fails and names the route (`` `POST /v1/clear` went ``). Test: `ci_9_a_removed_route_fails` (core/crates/obid/tests/routes_guard.rs), seen red on an inline router and list.
- **CI-10** (implemented). Given a client file that calls a `/v1/` path obid does not serve, when the check runs, then it fails and names the file and path. Test: `ci_10_a_client_path_without_a_route_fails` (core/crates/obid/tests/routes_guard.rs), seen red on an inline client snippet and known-dead list.
- **CI-11** (implemented). Given a `gateway.rs` `ROUTES` entry with no route, when the check runs, then it fails and names the entry. Test: `ci_11_a_dead_gateway_entry_fails` (core/crates/obid/tests/routes_guard.rs), seen red on an inline `ROUTES` snippet.
- **CI-12** (implemented). Given a change that adds a sleep call under `core/crates/obid/tests`, when the check runs, then it fails naming `obid's tests' sleep limit`, says the count and the limit, and says to wait for state with `support::within`; at the limit it passes and says so. Tests: the cases labelled `CI-12` in `scripts/oby-check-test.sh` (at the limit is within it; one sleep past the limit says to wait with support::within; a run with one sleep past the limit fails naming it).
- **CI-13** (implemented). Given a change that removes sleep calls, when the check runs, then it fails, asking to lower `tests_sleep_limit` from the limit to the new count, so the cut stays cut; a commit that lowers the limit is held to it against the base's copy. Tests: the cases labelled `CI-13` in `scripts/oby-check-test.sh` (a removed sleep asks for the limit to come down; a run with one sleep under the limit fails naming it; a commit's lower sleep limit holds against the base's copy).
