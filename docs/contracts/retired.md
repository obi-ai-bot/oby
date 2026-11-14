# Retired: what left because nothing called it

The business behavior contract for what milestone R03 ("Delete what has no caller") removes, from [the template](../business-contract-template.md). Each section is one removal: what left, what stayed, and the tests that hold it.

## The telemetry flag

### Feature and status

- **Outcome:** obid has no setting that suggests telemetry exists, and Oby still sends nothing. A `config.json` an older obid or install.sh wrote with `"telemetry"` in it keeps working: the key is ignored and named, as `council` is.
- **Why:** obid parsed and wrote a `telemetry` key, and nothing read it. `git grep -n '\.telemetry'` over `core/`, `apps/`, `mcp/`, `install/` and `cloud/control-plane/` found only its own copy in `Config::parse` and tests' asserts.
- **Status:** **implemented** (RETIRED-1 and RETIRED-2), by R03-01.
- **Scope:** The `telemetry` field of obid's configuration (`core/crates/obid/src/config.rs`), the first configuration obid writes (`default_config`, `core/crates/obid/src/app.rs`) and the one install.sh writes. Out of scope: an opt-in sender of failure reports (later work, R09); the guard in `core/crates/obid/src/memory.rs` that refuses a memory turning telemetry on, and its sentence; AGENTS.md's "No telemetry unless the user turns it on", still true; the agents' own telemetry switches (`DISABLE_TELEMETRY`).

### Behavior

- **Actors and ownership:** obid owns its configuration keys. A top-level key it doesn't know is ignored, listed in `Config.ignored` and named in its log at start: `<path>: telemetry isn't a key this obid knows: ignored`. Nothing changes for the apps.
- **Failures and recovery:** None new: an older key never stops obid. Every install that has `"telemetry": false` logs that one warning at each start, as for any retired key.

### Invariants

- Oby sends no telemetry.
- The released files in `core/crates/obid/tests/fixtures/released/` are never edited: those that carry `"telemetry": false` are what that release wrote ([released-files.md](released-files.md)).

### Interfaces and dependencies

- **Persistence:** `config.json`, which obid and install.sh write without the key. No API changes.

### Acceptance scenarios

- **RETIRED-1** (implemented). Given a `config.json` with `"telemetry": true` that an older obid wrote, when obid starts, then it starts, `Config.ignored` names `telemetry`, and its log says `telemetry isn't a key this obid knows: ignored`. Tests: `retired_1_an_old_telemetry_key_is_ignored_and_named` (core/crates/obid/src/config.rs) and `optional_parts_never_stop_obid` (core/crates/obid/tests/status.rs). `released_3_released_config_starts` (core/crates/obid/tests/released_files.rs) covers the released files that carry the key.
- **RETIRED-2** (implemented). Given no `config.json`, when obid writes its first one (`default_config`, with and without Codex) or install.sh writes one, then it has no `telemetry` key. Test: `retired_2_a_new_configuration_has_no_telemetry_key` (core/crates/obid/tests/install.rs).

## The control plane

### Feature and status

- **Outcome:** The deployed Worker in `cloud/control-plane` serves only the iPhone push relay and its health check. A public webhook endpoint, a sealed store with no reader, the tunnel sweep and the sign-in flow leave the code and, once the owner deploys, the internet.
- **Status:** **Implemented** (RETIRED-3 to RETIRED-6), by R03-02. Deploying the Worker and running migration 0009 on an instance's D1 are the owner's steps (the order is below).
- **Scope:**
  - **Left:** sign-in with Apple and Google (`GET /v1/auth`, `/v1/auth/{apple,google}/{start,callback}`), installations (`/v1/installations`, `/v1/installations/self`), one Cloudflare Tunnel per installation (`/v1/installations/self/endpoint`, its token), hooks (`/v1/installations/self/hooks`, `/h/{id}`), kept deliveries (`/v1/installations/self/queue-key`, `/v1/installations/self/hooks/queue`), the signed calls they used, the sweep's Cloudflare calls, and their tables (`users`, `sign_ins`, `installations`, `endpoints`, `nonces`, `hooks`, `hook_queue`). Git history keeps them.
  - **Stayed, unchanged:** `GET /v1/health`, `POST /v1/push/devices`, `DELETE /v1/push/devices/{pushId}` and `POST /v1/push`, with their limits, messages and statuses ([remote-access.md](../remote-access.md#pushes)), and the push relay's tables (`rate_limits`, `push_devices`, `push_device_count` and its triggers, `apns_provider_token`). The cron still runs every minute.
  - **Outside it:** where obid sends its pushes (Deskree's instance by default), the iPhone's tunnel-hostname code (`apps/shared/RemoteRoute.swift`) and Android's hostname test, and Sangam's mock Settings › Remote access.

### Behavior

- **Actors and ownership:** obid sends only `POST /v1/push`; the iPhone sends only `POST /v1/push/devices` and `DELETE /v1/push/devices/{pushId}`. Neither changes. The Worker decides nothing new: any other method or path is no route.
- **States and transitions:** A request to a removed path answers 404 `{"error": "No such route."}`, the same as any unknown path, with no special 405 or 410, and reads or writes nothing in D1. The sweep, every minute, deletes every rate-limit window a day old and each push relay window once it ends, and nothing else; it calls no one.
- **Invariants and boundaries:** Deploy order: the code without the removed routes is deployed first, and only then does the owner run migration 0009 remotely, so no running Worker reads a dropped table. Any endpoint left in D1 is a live Cloudflare Tunnel and DNS record: the owner deletes those before deploying, since nothing can reach them after. Migration 0009 never touches the push relay's tables.

### Acceptance scenarios

- **RETIRED-3.** Given the Worker, when `POST /v1/auth/apple/start`, `POST /v1/auth/google/callback`, `GET /v1/auth`, `POST /v1/installations`, `GET /v1/installations/self`, `POST /v1/installations/self/hooks`, or `POST` or `GET /h/{id}` arrives, then it answers 404 "No such route." and writes nothing to D1. Test: `retired_3_dead_routes_answer_404` (cloud/control-plane/test/retired.test.ts).
- **RETIRED-4.** Given a phone registered at `POST /v1/push/devices`, when obid posts a sealed push signed with that phone's `pushKey` to `/v1/push`, then APNs gets the alert as before and the relay's limits still hold. Test: `retired_4_a_sealed_push_still_reaches_apns` (cloud/control-plane/test/push.test.ts, the "a push" cases).
- **RETIRED-5.** Given the minute cron, when the sweep runs, then it deletes only ended rate-limit windows and makes no Cloudflare API call. Test: `retired_5_the_sweep_asks_cloudflare_nothing` (cloud/control-plane/test/retired.test.ts).
- **RETIRED-6.** Given a D1 database migrated through 0008 with rows in every table, when 0009 runs, then the installation, sign-in, endpoint and hook tables are gone and every `push_devices`, `push_device_count`, `apns_provider_token` and `rate_limits` row is unchanged. Test: `retired_6_migration_keeps_push_rows` (cloud/control-plane/test/retired.test.ts).

## Routes obid does not serve

### Feature and status

- **Outcome:** A paired phone can call only routes obid serves, and [runner-api.md](../runner-api.md) promises only those: no speech service, Watch API or Live Activity pushes that obid doesn't have. The routes it stopped serving are listed once, each with the date it went, and a Brain test keeps the allowlist and the doc honest.
- **Why:** The gateway's allowlist let through 7 routes obid never routed (`POST /v1/jobs/*/proposals/*/file`, `POST /v1/jobs/*/start-when`, `PUT /v1/settings/models`, `POST /v1/activities`, `GET` and `POST /v1/watches`, `DELETE /v1/watches/*`), and runner-api.md still documented them, with `POST /v1/speech` and the Watch API. The Swift runner served them; it was deleted on 2026-11-02 (#1653).
- **Status:** **implemented** (RETIRED-7 to RETIRED-9, tasks R03-03 and R03-04).
- **Scope:** `ROUTES` in `core/crates/obid/src/gateway.rs`, runner-api.md's sections for those routes and its new "Routes obid does not serve" list, and the Brain (`tools/brain`), which reads that list. The Swift client's calls (`apps/shared` and `apps/ios`, not their tests) are in. Out of scope: Android's `Runner.kt` (R03-08), and serving any of these routes again. `mcp/server/Server.swift`'s call left with oby-mcp (R03-05, below). The Watch app, and its calls with it, was removed on 2026-11-13 (docs/decisions/2026-11-13-roadmap-answers.md).

### Behavior

- **Actors and ownership:** obid owns its routes and the gateway's allowlist. A phone that calls a route obid doesn't serve gets the gateway's `404` (before, obid's own `404` behind it). Nothing obid serves changes.
- **The gone list:** runner-api.md's section "Routes obid does not serve" lists each route obid stopped serving as a bullet: the route in backticks (`METHOD /path`, one per pair), `(gone <date>)` and why. Older sections that say a route answers `404` stay; their routes are on the list too.
- **The Brain's checks:** every backticked `METHOD /path` inside that section (to the next heading of its level or higher) is a gone route. A mention of a gone route anywhere in the doc never counts as documenting it, so it's never `doc-without-route`; a gone route obid routes is `gone-but-routed`. A segment holding `<…>` (`conversation-<id>`) is a placeholder, as `{x}` is, and a mention whose path matches a route obid serves documents it whatever its prefix (`GET /callback`). The Brain also reads every `call("METHOD", "v1/…")` and `send("METHOD", "v1/…")` of the Swift client (`swiftClient` in `brain.config.ts`), each `\(…)` a placeholder segment, and one obid doesn't route is `call-without-route`.
- **Failures and recovery:** None at run time: the Brain is a static read of the repository.

### Invariants

- Every allowlist entry, method and path, matches a route obid serves.
- Every route runner-api.md documents as served exists in obid, and each one obid stopped serving is on the gone list.
- Every call the Swift client makes reaches a route obid serves, but the named exceptions in RETIRED-9's test.

### Interfaces and dependencies

- **API:** none added or removed: obid never routed the 7, and the gateway no longer lets them through.
- **Docs:** runner-api.md, whose gone list the Brain finds through `goneSection` in `tools/brain/brain.config.ts`.
- **Tests:** the Brain's tests run by hand (`cd tools/brain && npm test`), not in `scripts/oby-check.sh`. The gateway's own refusals are asserted in `the_gateway_allows_only_the_listed_routes` (`core/crates/obid/src/gateway.rs`), which the check runs.

### Acceptance scenarios

- **RETIRED-7** (implemented). Given obid's router and the gateway's allowlist, when the Brain generates its checks, then every allowlist entry (method and path) matches a route obid serves. Test: `retired_7_every_phone_route_is_served` (`tools/brain/test/repo.test.ts`).
- **RETIRED-8** (implemented). Given docs/runner-api.md, when the Brain generates its checks, then every route documented as served exists in obid, and each route obid stopped serving is listed as gone. Test: `retired_8_every_documented_route_is_served` (`tools/brain/test/repo.test.ts`).
- **RETIRED-9** (implemented). Given `apps/shared` and `apps/ios` (not their tests), when the Brain scans their `call`/`send` calls, then every method and path they call is a route obid serves, except the one named exception in the test (`POST /v1/jobs/{id}/proposals/{index}/file`, the task page's). Test: `retired_9_the_swift_client_calls_only_served_routes` (`tools/brain/test/repo.test.ts`).

## oby-mcp

### Feature and status

- **Outcome:** The repository carries no MCP server that reads a deleted app's settings and calls a route obid doesn't serve. obid's own MCP server, which conversations and agents get, is unchanged.
- **Why:** oby-mcp (`mcp/`, built by the root `Package.swift`) found obid through the deleted Swift Mac app's `runnerURL` default and called `POST /v1/jobs/{id}/start-when`, which obid doesn't serve. Nothing under `apps/`, `install/`, `scripts/` or `.github/` started or shipped it. The owner's answer: "Delete it now" (2026-11-13, [trim.md](../trim.md)).
- **Status:** **implemented**, by R03-05.
- **Scope:**
  - **Left:** `mcp/` (the `oby-mcp` server and its tests), the root `Package.swift`, `docs/mcp.md`, and the check's `swift test` step. Git history keeps them.
  - **Stayed, unchanged:** obid's `/v1/agent/mcp/{job}` and the conversation's `oby` MCP server, and the check's Apple apps' build and suites (the `swift` group).
  - **Outside it:** an `oby` CLI or an `oby mcp` command (R12).

### Behavior

- **Actors and ownership:** Nothing changes in obid or in the apps. A person who added oby-mcp to Claude Code by hand loses it.

### Acceptance scenarios

- None: a deletion adds no behaviour.
