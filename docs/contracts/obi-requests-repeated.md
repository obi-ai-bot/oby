# Obi-to-Obi requests: a send that's safe to repeat

The business behavior contract for sending another Oby a request again after its answer was lost, from [the template](../business-contract-template.md). It continues the OBIREQ scenarios of [obi-requests.md](obi-requests.md) (OBIREQ-1 to 6, R10-01) and uses R02's receipt store as [receipts.md](receipts.md) states it. The API is [docs/api/obysphere.md](../api/obysphere.md), "obid only: Obi-to-Obi exchanges". Roadmap: R10-02.

## Feature and status

- **Outcome:** A retry over the phone relay, or an app's or MCP client's retry, never sends a teammate's Oby the same question or review request twice, so it never counts twice against their limit of 5 open requests per owner (`busy`).
- **Status:** **Implemented** (OBIREQ-7 to 10).
- **Scope:** `POST /v1/obid/sphere/requests` and `POST /v1/obid/sphere/peers/{owner}/ask`. Outside it: other routes (R02's own), any protocol change (the envelope's `id` stays a new UUID per request), and oby-mcp, which isn't in this repository any more (`mcp/` is gone): a client that wants it sends its tool call's id as `operationId`.

## Behavior

### Actors and ownership

- **obid** owns the receipt and the decision that a repeat is the same send, through R02's store: no store of its own.
- **Clients** that retry send the same key: an optional `operationId` in the body, or R02's `Idempotency-Key` header. One that sends none gets what it got before.

### Rules

- **The key.** `operationId` (a string) or `Idempotency-Key`, read by R02's rule: 1 to 128 characters; both sent and different, a key that isn't a string, or more than one header is `422`, and nothing is sent.
- **What a repeat must match.** The route (the peer ask's names its `{owner}`) and the body without `operationId`. So a key in the body one time and in the header the next is the same send.
- **Before anything else.** The receipt is looked up before the secret scan, the directory read, the signature check and the send, under the key's lock, so two copies arriving together send once.
- **Repeat (OBIREQ-7, 9).** `202` with the first exchange as it is now (the peer ask: its question's post as it is now), or as it was answered when obid no longer has it. Nothing is scanned, signed or sent again.
- **Conflict (OBIREQ-8).** The key's receipt is for another route or body: `409` with R02's words, and nothing is sent.
- **A signed peer ask (OBIREQ-9).** A peer ask's signature holds once (PEER-33). Its receipt is looked up before the signature is checked, so a retry of a question that went gets its first post, never `403` for a used signature. A receipt is written only for a question whose signature held, and a repeat shows only what obid's token already reads (`GET /v1/obid/sphere/exchanges`).
- **What keeps no receipt.** A refusal or a failed send (`4xx`, `5xx`) keeps none: nothing was asked, so the same key may go again.
- **How long.** R02's store keeps a receipt 24 hours, longer than a review request's 2 hours and as long as a question's day. A request asked for longer than a day can be sent again with its key after that day.
- **No key (OBIREQ-10).** Nothing changes: each call is a new request, as before.

### Invariants

- One store, R02's, and one way of reading the key (`routes::request_key`, `routes::keyed`).
- No protocol change: a repeat sends no envelope at all.

## Interfaces and dependencies

- `POST /v1/obid/sphere/requests` and `POST /v1/obid/sphere/peers/{owner}/ask` take an optional `operationId` and the `Idempotency-Key` header. Add-only: no other key or value changes.

## Acceptance scenarios

- **OBIREQ-7:** Given a request sent with operationId X, when the same body comes again with X, then obid answers the first exchange and the fake Sphere sees one envelope. Test: `obireq_7_a_repeat_sends_nothing` (core/crates/obid/tests/sphere_api.rs).
- **OBIREQ-8:** Given operationId X used for one body, when a different body comes with X, then obid answers `409` and sends nothing. Test: `obireq_8_one_id_two_bodies_is_409` (sphere_api.rs).
- **OBIREQ-9:** Given a signed peer ask with operationId X that went, when the same signed body is retried, then the first exchange is answered and the used signature is not refused as reused. Test: `obireq_9_a_signed_retry_gets_the_first_exchange` (core/crates/obid/tests/peer_flow.rs).
- **OBIREQ-10:** Given a client that sends no operationId, when it posts twice, then two exchanges go, as today. Test: `obireq_10_no_id_behaves_as_before` (sphere_api.rs).
