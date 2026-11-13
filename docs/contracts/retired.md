# Retired: what has no caller leaves

The business behavior contract for roadmap milestone R03, "Delete what has no caller", from [the template](../business-contract-template.md). Each R03 task adds its own section here, with its own RETIRED scenarios.

## Routes obid does not serve

### Feature and status

- **Outcome:** A paired phone can call only routes obid serves, and [runner-api.md](../runner-api.md) promises only those: no speech service, Watch API or Live Activity pushes that obid doesn't have. The routes it stopped serving are listed once, each with the date it went, and a Brain test keeps the allowlist and the doc honest.
- **Why:** The gateway's allowlist let through 7 routes obid never routed (`POST /v1/jobs/*/proposals/*/file`, `POST /v1/jobs/*/start-when`, `PUT /v1/settings/models`, `POST /v1/activities`, `GET` and `POST /v1/watches`, `DELETE /v1/watches/*`), and runner-api.md still documented them, with `POST /v1/speech` and the Watch API. The Swift runner served them; it was deleted on 2026-11-02 (#1653).
- **Status:** **implemented** (RETIRED-7 and RETIRED-8, task R03-03).
- **Scope:** `ROUTES` in `core/crates/obid/src/gateway.rs`, runner-api.md's sections for those routes and its new "Routes obid does not serve" list, and the Brain (`tools/brain`), which reads that list. Out of scope: the clients' calls to these routes (`apps/shared/Runner.swift`, `apps/ios`, `apps/watch`, `mcp/server/Server.swift`; R03-04 and R03-07), and serving any of them again.

### Behavior

- **Actors and ownership:** obid owns its routes and the gateway's allowlist. A phone that calls a route obid doesn't serve gets the gateway's `404` (before, obid's own `404` behind it). Nothing obid serves changes.
- **The gone list:** runner-api.md's section "Routes obid does not serve" lists each route obid stopped serving as a bullet: the route in backticks (`METHOD /path`, one per pair), `(gone <date>)` and why. Older sections that say a route answers `404` stay; their routes are on the list too.
- **The Brain's checks:** every backticked `METHOD /path` inside that section (to the next heading of its level or higher) is a gone route. A mention of a gone route anywhere in the doc never counts as documenting it, so it's never `doc-without-route`; a gone route obid routes is `gone-but-routed`. A segment holding `<…>` (`conversation-<id>`) is a placeholder, as `{x}` is, and a mention whose path matches a route obid serves documents it whatever its prefix (`GET /callback`).
- **Failures and recovery:** None at run time: the Brain is a static read of the repository.

### Invariants

- Every allowlist entry, method and path, matches a route obid serves.
- Every route runner-api.md documents as served exists in obid, and each one obid stopped serving is on the gone list.

### Interfaces and dependencies

- **API:** none added or removed: obid never routed the 7, and the gateway no longer lets them through.
- **Docs:** runner-api.md, whose gone list the Brain finds through `goneSection` in `tools/brain/brain.config.ts`.
- **Tests:** the Brain's tests run by hand (`cd tools/brain && npm test`), not in `scripts/oby-check.sh`. The gateway's own refusals are asserted in `the_gateway_allows_only_the_listed_routes` (`core/crates/obid/src/gateway.rs`), which the check runs.

### Acceptance scenarios

- **RETIRED-7** (implemented). Given obid's router and the gateway's allowlist, when the Brain generates its checks, then every allowlist entry (method and path) matches a route obid serves. Test: `retired_7_every_phone_route_is_served` (`tools/brain/test/repo.test.ts`).
- **RETIRED-8** (implemented). Given docs/runner-api.md, when the Brain generates its checks, then every route documented as served exists in obid, and each route obid stopped serving is listed as gone. Test: `retired_8_every_documented_route_is_served` (`tools/brain/test/repo.test.ts`).
