# Retired: what left because nothing called it

The business behavior contract for what milestone R03 ("Delete what has no caller") removes, from [the template](../business-contract-template.md). Each section is one removal: what left, what stayed, and the tests that hold it.

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
