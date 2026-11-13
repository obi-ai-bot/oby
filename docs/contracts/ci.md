# CI: what GitHub runs, and the check guards obid's API

The business behavior contract for what GitHub's Linux workflow runs and what the check itself guards, from [the template](../business-contract-template.md). Roadmap milestone R01 (Safety net); CI-1 and CI-2 are task R01-01, CI-9 to 11 are task R01-05.

## Feature and status

- **Outcome:** GitHub spends minutes only on runs someone reads: a ready pull request gets the secret scan, fmt and clippy, and `dev` is tested in full once a night and on demand, never on every push (docs/ci.md, "What GitHub runs"). obid's API stays add-only (AGENTS.md, "How we work"): the iPhone, the Apple Watch, the Electron app and oby-mcp keep working after obid changes. Removing a route that a client still calls fails the check, as does a client call to a path obid doesn't serve and a gateway allowlist entry with no route, instead of showing up as a `404` on someone's phone.
- **Status:** CI-1 is **implemented** and CI-2 is **proposed** until a nightly or on-demand run on `dev` proves it (below): `.github/workflows/linux.yml` as #2166 and #2167 left it, with no change here. CI-9 to 11 are **implemented**: one test binary, `core/crates/obid/tests/routes_guard.rs`, with the committed lists `docs/api/routes.txt` and `docs/api/dead-client-calls.txt`. The gateway's dead entries it found were deleted with it; nothing else in obid changed, and no client changed.
- **Scope:** For CI-1 and CI-2, the triggers, jobs and cache saving of `.github/workflows/linux.yml`. Out of scope there: the new workflow, its lanes and shards (R01-02), and the merge queue and branch protection (R01-03). For CI-9 to 11, the paths of obid's routes, of the clients' calls and of the gateway's allowlist. Out of scope: request and response keys, enum values and frozen client decoders (R07); matching a client call by its HTTP method; paths built only by concatenation past a literal prefix; the iPhone and Watch app targets (`apps/ios`, `apps/watch`), the Electron app's tests and mock, `mcp/tests` and `cloud/`; removing the dead client calls themselves (each is a task of its own).

## Behavior

### Actors and ownership

- **The check** (`scripts/oby-check.sh`) runs the route guard when a change reaches what it reads (below). No model call is involved, and nothing changes at runtime for obid or any client.

### Rules

- **The route list.** `docs/api/routes.txt` lists every route obid serves, one `<METHOD> <path>` line per method, the path as axum declares it (`{id}`, `{*path}`), sorted by path, then method. It is rebuilt from every `.route(` under `core/crates/obid/src` whose path is a literal starting `/v1/`, outside test modules (`production_lines`' rule in `scripts/oby-check.sh`) and comment lines; each method router (`get`, `post`, `put`, `patch`, `delete`, or `axum::routing::` one) is a line, and a layer is skipped. A method router the guard doesn't know fails it, naming the file and the path. The routes `gateway.rs` declares itself (`POST /v1/pairing`, on the gateway's own listener) sit under their own heading. A new route commits its line with it; a route that goes shows as a deleted line in review.
- **The clients' calls.** The `/v1/` (or Swift's `v1/`) string literals of `apps/electron/src` (its `.ts` and `.tsx`, not under a `test/` folder, not `*.test.ts(x)`, not `renderer/src/mock.ts`), `apps/shared` and `mcp/server` are paths the clients call. Each is compared, segment by segment and whatever the method, with every route in the list: an interpolated segment matches any, a route's `{x}` matches any one and `{*x}` one or more, and a literal ending in `/` matches when a route starts with it. A query is cut, and an interpolation glued to the end of the text (`/v1/jobs${q(..)}`) is dropped.
- **The known-dead list only shrinks.** `docs/api/dead-client-calls.txt` holds the calls that were already dead when the guard came, `<file> <path>` a line. A dead call not on it fails, and so does a line whose call is gone from its file or served again: delete that line.
- **The gateway's list holds only served routes.** Each `ROUTES` entry in `core/crates/obid/src/gateway.rs` (`*` one segment, a final `**` one or more) must reach a route of the API's with the same method; one that reaches none fails, and is deleted.

### Failures and recovery

- Each failure names what to fix: the route that went or appeared, the file and path of a dead call, the list line to delete, or the gateway entry. The fix is the route put back, its line committed, the call changed, or the entry or line deleted.

### Invariants

- The guard never writes a list: a person commits each change to it, so it shows in review.
- `gateway::ROUTES` stays private: the guard reads gateway.rs's source.

## Interfaces and dependencies

- **Workflow:** `.github/workflows/linux.yml` (GitHub's `Linux` workflow) is what CI-1 and CI-2 describe; GitHub's own runs prove them, since no test runner in this repository runs a workflow's triggers.
- **Files:** `docs/api/routes.txt`, `docs/api/dead-client-calls.txt`, read by `core/crates/obid/tests/routes_guard.rs`.
- **Check:** `scripts/oby-check.sh` plans the Rust group with `tests/routes_guard.rs` when `core/crates/obid/src/api.rs` or `gateway.rs` changes, another file under `core/crates/obid/src/` with a `.route(` in it, anything under `docs/api/`, or a client source as above (`apps/electron/src` but its tests and mock, `apps/shared/*.swift`, `mcp/server/*.swift`); the groups those paths planned before stay as they were. `scripts/oby-check-test.sh` proves that plan.
- **Cost:** an Electron-only or Swift-only change to client sources now also runs `cargo fmt`, clippy on obid and a build of one obid test binary.

## Acceptance scenarios

- **CI-1** (implemented). Given a ready pull request into `dev`, when it is opened, reopened, pushed or marked ready, then the Linux workflow starts by itself on that head and runs the secret scan, plus `cargo fmt --check` and clippy when the change reaches Rust. Given a draft without `full-ci`, its jobs are skipped. Proof: run [43444624617](https://github.com/obi-ai-bot/oby/actions/runs/43444624617) (#2170, a ready pull request reaching Rust: Changes, Secret scan and Rust (core) with fmt and clippy passed, `cargo test` skipped, E2E and Control plane skipped), run [43445078023](https://github.com/obi-ai-bot/oby/actions/runs/43445078023) (#2171, `factory/` only: the secret scan alone), and run [43428763381](https://github.com/obi-ai-bot/oby/actions/runs/43428763381) (a draft: every job skipped).
- **CI-2** (proposed). Given a push to `dev`, when it lands, then no Linux workflow run starts. Given the nightly schedule or a `workflow_dispatch` on `dev`, every job runs to the end, and only that run saves the Rust caches. Proof: the pushes to `dev` after #2166 (`9991deb0` to `80d9d398`) started no run (`gh run list --workflow linux.yml --branch dev --event push`: the latest is [43430439449](https://github.com/obi-ai-bot/oby/actions/runs/43430439449), on `f3b0120f`, #2165, before #2166). Missing: a nightly or `workflow_dispatch` run on `dev`; none has run yet.
- **CI-9** (implemented). Given a change that removes a route from obid, when the check runs, then it fails and names the route (`` `POST /v1/clear` went ``). Test: `ci_9_a_removed_route_fails` (core/crates/obid/tests/routes_guard.rs), seen red on an inline router and list.
- **CI-10** (implemented). Given a client file that calls a `/v1/` path obid does not serve, when the check runs, then it fails and names the file and path. Test: `ci_10_a_client_path_without_a_route_fails` (core/crates/obid/tests/routes_guard.rs), seen red on an inline client snippet and known-dead list.
- **CI-11** (implemented). Given a `gateway.rs` `ROUTES` entry with no route, when the check runs, then it fails and names the entry. Test: `ci_11_a_dead_gateway_entry_fails` (core/crates/obid/tests/routes_guard.rs), seen red on an inline `ROUTES` snippet.
