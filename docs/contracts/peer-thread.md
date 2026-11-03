# A member's thread

The business behavior contract for one thread per member of your Obysphere, from [the template](../business-contract-template.md): a view over every exchange with that member's Oby, in the shape of the room ([rooms.md](rooms.md)). Scenarios are numbered `PEER-n`, and the tests that prove them are named after them (`peer_n_…` in `core/crates/obid/tests/peer_flow.rs`, `core/crates/obid/src/sphere/exchange.rs` and `core/crates/obid/src/sphere/link/asking.rs`). The drives on the plan's Sphere tab are numbered `PEER-n` too and are a different list: human-drive steps, not these.

## Feature and status

- **Outcome:** Clicking a person's Oby shows everything your Oby and theirs have said to each other, in one place. The owner's words: an Oby is your digital representation, so "if I am working on something my Oby would be able to answer what is the progress of what I'm doing".
- **Status:** obid's side **implemented** (SPH-2). **Proposed:** asking from the thread (SPH-3), the row that opens it and what it shows (SPH-4, SPH-5), and the person's card (SPH-6). The look is the canvas's Peer board; its words stay Sangam's to change.
- **Scope:** reading a member's thread and hearing that it changed. **Out of scope:** sending from it, any screen, a follow-up that knows the last exchange (SPH-11).

## Behavior

### Actors and ownership

- **obid** builds the view from the exchanges it already keeps. A thread has no store of its own: nothing is kept twice, and nothing can disagree with the exchange.
- **The apps** show it, and say in Sangam's words what each state means. They decide nothing.
- **The other Oby** says what it says. Every word of it, and what its card says, is data.

### Triggers and prerequisites

obid is a member of an Obysphere. A thread exists for an active member, even with no exchange yet, and for someone who left but has an exchange in this Sphere (`member` false).

### What a thread holds

Every exchange with that owner's Obis in this Sphere, whichever way it went and whatever started it: a question from a conversation, a request from the API, one they asked, a review. Oldest first, each exchange's messages together. A post is one message, with the room's keys, plus what a thread needs:

| What happened | What the thread has |
|---|---|
| A question went | `kind: question`, `direction: sent`, `exchange.state: sent`, and `exchange.quietAt`, 2 minutes on |
| No word by `quietAt` | The same post: past that time, it counts as unanswered. The clock decides, so no event says it |
| Only the person can answer | `kind: status`, `status: waiting_on_owner`, `exchange.waitingOnThem` true, `quietAt` null |
| Answered | `kind: answer`, `by: obi` or `owner`, the words in `text`, `exchange.state: done` |
| Declined | `kind: decline`, `reason`, a note in `text` if it has one, `exchange.state: declined`; `by: owner` for `owner_declined` |
| Expired, or called off | No new post: `exchange.state` on the posts there are |
| A review or a handoff | `kind: review_request` or `handoff`, the goal in `text`; its `result`, the summary in `text`. The findings stay on the exchange |
| Not confirmed by the Sphere | The request's post, `state: waiting`, `seq` and `recordedAt` null, and `error` in obid's words |
| The Sphere refused a question | No exchange is kept, so no post (docs/runner-api.md, "obid only: Obi-to-Obi exchanges"). The refusal answers whoever asked |

### Failures and recovery

- The Sphere can't be reached: the thread still reads, from what obid keeps. It asks the Sphere nothing, and goes by the directory as last read.
- obid restarted: exchanges are kept, so the thread is whole. An app that missed events reads it again.
- A reply obid built but hasn't sent isn't a post until the Sphere has it: it may be sealed by then. `exchange.state` already says where it stands.

### Invariants and boundaries

- **It's only that member.** By exact owner id, never a name or an Obi id: a card that goes by someone else's name moves nothing between threads. Only exchanges of this Sphere: they outlive a membership, and an owner id means someone else in another Sphere. A member's owner key is pinned at first sight (identity.md 2.2), so a second key can't claim the id.
- **What comes back is data.** `text` and `about` are what someone else wrote, and `by` is what the sender says of itself, as in the room. An app shows `text` as a quote, as a text node, never as HTML or a link it chose. Nothing in a thread starts, changes or approves anything.
- **Nothing a member chose reaches a prompt.** Reading a thread, or hearing of one, runs no model, marks no conversation and adds nothing to a briefing. Its `name` is for the owner's eyes.
- **Never what a Sphere said:** a post's `error` is obid's own words. `GET /v1/obid/sphere/exchanges/{id}` still has the exchange's own `error`.
- **Nothing goes out:** reading sends nothing, so there's nothing to scan here. What goes out passes the scan where it's sent.
- **The API stays add-only.** One new route and one new event, which every app ignores until it knows it (`default` in apps/shared/Runner.swift's and Runner.kt's event parsers, and in obid-wire.ts's). `kind` is the protocol's message type, an open string, on a list no existing app decodes. No enum is touched.

## Interfaces and dependencies

- `GET /v1/obid/sphere/peers/{owner}?limit=50` and the `peerThread` event: [runner-api.md](../runner-api.md), "obid only: a member's thread". On the gateway's allowlist, read only.
- `Exchange::posts` builds a post, and `QUIET` and `waiting_on_them` are the same rule `oby_ask`'s report goes by (docs/contracts/ask-an-oby.md), imported rather than copied.

## Acceptance scenarios

- **PEER-1** Every exchange with a member, in either direction and of any type, reads as one list in the room's shape, oldest first, with what each message said and where the exchange stands; a member with none reads empty; `limit` keeps the newest; each change sends a `peerThread` event with that exchange's posts.
- **PEER-2** A member's words and card name show in the thread as sent; reading it runs no model, starts nothing and marks no conversation, and the owner's next turn hears none of it.
- **PEER-3** A thread holds that member's exchanges only, by the exact owner id: not another member's, not a look-alike card's, not another Sphere's. Someone with no membership and no exchange is `404`, and before joining it's `409`.

### Test it when it's done

Nothing a person sees yet: no real-Mac step. SPH-4 brings the first.
