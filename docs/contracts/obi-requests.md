# Obi-to-Obi requests: calling off what obid asked

The business behavior contract for the requests the owner's Oby sends another Oby (a teammate's review, a question) once the work they were about ends, from [the template](../business-contract-template.md). The API is [docs/api/obysphere.md](../api/obysphere.md), "obid only: Obi-to-Obi exchanges". Roadmap: R10-01.

## Feature and status

- **Outcome:** When the owner's work ends, the teammate's Oby stops reviewing or approving it. No teammate review or GitHub approval of a stale commit runs for up to the request's 2 hours after the job stopped or the pull request moved on.
- **Status:** **Implemented**: every scenario unless it says it's proposed.
- **Scope:** obid sends the protocol's `cancel` (spec `protocol.md` section 5.17, an empty body, `in_reply_to` the request) for its own open `review_request` and `question` exchanges. Outside it: any change to the `cancel` action, its schema or the Sphere server; an app button for calling a request off (no Sangam screen yet); `handoff` and `introduction`, which obid doesn't send on its own; what the receiving Oby does with a `cancel`, which is unchanged (it stops the review job and dismisses an approval it posted).

## Behavior

### Actors and ownership

- **obid** owns which open outgoing exchange belongs to which job's pull request, head or conversation turn, when a call-off is due, and sending the `cancel` once through the exchange's outbox. Rules decide; no model call does (`sphere/calloff.rs`).
- **The apps** show nothing new: the peer thread and the conversation's ask card already read an exchange `cancelled` as `Called off` (ask-an-oby.md, ASK-20).
- **The owner** stops a job or a turn, merges or closes a pull request, or calls a request off by hand through the API.

### Triggers and prerequisites

An outgoing exchange is **open** while it is `sent` or `accepted` and nothing that ends it (a `decline`, `result`, `answer` or `cancel`) was sent, received or is waiting to be sent. A review request is about a job's pull request when its `repo` (any letter case) and `pr` are those the job published to. obid calls an open one off when:

- the owner stopped the job (`POST /v1/jobs/{id}/stop`, Stop all), for every request about its pull request asked before it stopped (OBIREQ-1);
- GitHub read the pull request merged or closed after the request was asked (OBIREQ-3);
- the pull request's head is no longer the request's `head_sha`: obid pushed another commit, or GitHub showed another head, after the request was asked (OBIREQ-2);
- obid asks a teammate's Oby about a new head of the pull request: every open approval request at another head goes first, so one request per pull request stays open (OBIREQ-2);
- the owner stopped a conversation turn under way (`POST /v1/conversation/stop`): every question that conversation asked since the turn began (OBIREQ-4);
- the owner calls it off by hand, `POST /v1/obid/sphere/exchanges/{id}/cancel` (OBIREQ-5).

What obid read or pushed before a request was asked never calls it off, and neither does GitHub showing, after the request, a head obid had pushed by the time it asked (the request's own, or one it replaced): GitHub's read lags behind obid's push, so a stale read can't take a request about the head obid just pushed.

### States and transitions

- An open request called off reads `cancelled` at once, with its `cancel` in the outbox, and an `exchange` event says so; once the Sphere takes the `cancel`, it is the exchange's last sent message.
- A question a conversation asked ends its card there as any end does (ask-an-oby.md, ASK-20): once the turn is over, the card goes to `cancelled` in place, told once, on a relayed turn whose words, for an app with no card, are `I called off this question to <name>'s Oby.`
- A request already ended stays as it is: nothing is sent (OBIREQ-5).
- A `decline`, `result` or `answer` from the other Oby that comes before the `cancel` went ends the exchange as it says, and the `cancel` waiting for it is dropped.

### Failures and recovery

- The Sphere can't be reached: the `cancel` waits in the exchange's outbox, which is kept with the exchange, and goes once when the Sphere is back, also after obid restarts (OBIREQ-6).
- obid was down when the work ended: at start it looks at every job once, and again whenever it fell behind on the jobs' events.
- obid isn't joined, or has no Obi key: nothing can be signed; the manual route answers `409`.

### Invariants and boundaries

- Only the Oby that asked calls a request off (protocol.md section 4, rule 1): an incoming exchange is never called off here (`409`).
- A request is called off once: its `cancel` is built and signed once, and sent as any reply is, the same bytes on a retry.
- The manual call-off takes obid's token alone, not an app's signature: it only withdraws the owner's own request and shares nothing (as removing a rule does, peer-thread.md). The owner may ask for a signature later.
- No new key or enum value but the route: `cancelled` is a state the exchange already had.

## Interfaces and dependencies

- **`POST /v1/obid/sphere/exchanges/{id}/cancel`** (obid only, loopback, obid's token; not on the gateway): `200` with the exchange, `state` `cancelled`; `404` for an exchange obid doesn't have; `409` for one another Oby sent, one that already ended, or with no Obi key to sign with. [docs/api/obysphere.md](../api/obysphere.md).
- The `exchange` event, as for every change of an exchange.
- Jobs' `job` events, read only.

## Acceptance scenarios

- **OBIREQ-1:** Given a job that asked a teammate's Oby for a review, when the owner stops the job, then obid sends one `cancel` for that exchange and the exchange reads `cancelled`. Tests: `obireq_1_a_stopped_job_calls_off_its_review` (core/crates/obid/tests/sphere_api.rs), `obireq_1_a_stopped_job_calls_off_what_it_asked_before` (sphere/calloff.rs).
- **OBIREQ-2:** Given an open teammate approval request at head A, when the pull request's head moves to B and obid asks again, then the request at A is called off before or with the new one, and only one request per pull request stays open. Tests: `obireq_2_a_new_head_calls_off_the_old_request` (sphere/link/calling_off.rs), `obireq_2_a_head_that_moved_since_calls_off`, `obireq_2_a_lagging_read_of_the_old_head_doesnt_call_off`, `obireq_2_only_requests_at_another_head_are_replaced` (sphere/calloff.rs).
- **OBIREQ-3:** Given an open outgoing request, when its pull request merges or closes, then obid sends `cancel` once. Tests: `obireq_3_a_merged_pull_request_calls_off` (sphere_api.rs), `obireq_3_a_merged_or_closed_pull_request_calls_off` (sphere/calloff.rs).
- **OBIREQ-4:** Given a question a conversation turn asked, when the owner stops that turn, then the question is called off and the conversation card reads `Called off` (its state `cancelled`). Tests: `obireq_4_a_stopped_turn_calls_off_its_question` (core/crates/obid/tests/ask_an_oby.rs: the card goes asking, then `cancelled`), `obireq_4_a_stopped_turn_calls_off_its_questions` (sphere/link/calling_off.rs), `obireq_4_a_turn_calls_off_only_its_own_questions` (sphere/calloff.rs), `obireq_4_a_called_off_question_ends_its_card` (brain/ask.rs).
- **OBIREQ-5:** Given an outgoing exchange that already ended, when the call-off route or a job stop runs, then nothing is sent and the route answers `409`. Tests: `obireq_5_an_ended_exchange_sends_nothing` (sphere_api.rs), `obireq_5_only_an_open_outgoing_request_is_called_off` (sphere/calloff.rs); obid-spec's `POST /v1/obid/sphere/exchanges/{id}/cancel` (tools/obid-spec/workflows/routes/sphere/post-exchange-cancel.json: `404` unknown, `409` incoming and ended with one `cancel` sent, `200` and `cancelled` on both sides).
- **OBIREQ-6:** Given the Sphere cannot be reached, when a call-off is due, then the `cancel` waits in the outbox and goes once, also across an obid restart. Test: `obireq_6_a_call_off_waits_in_the_outbox` (sphere_api.rs).

**Test it when it's done (real Mac):** on the rig, stop a job that waits on a teammate's review; the teammate's exchange reads `cancelled` and its review job stops.
