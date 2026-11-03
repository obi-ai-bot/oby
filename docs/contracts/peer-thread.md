# A member's thread

The business behavior contract for one thread per member of your Obysphere, from [the template](../business-contract-template.md): a view over every exchange with that member's Oby, in the shape of the room ([rooms.md](rooms.md)). Scenarios are numbered `PEER-n`, and the tests that prove them are named after them (`peer_n_…` in `core/crates/obid/tests/peer_flow.rs`, `core/crates/obid/src/sphere/exchange.rs` and `core/crates/obid/src/sphere/link/asking.rs`). The drives on the plan's Sphere tab are numbered `PEER-n` too and are a different list: human-drive steps, not these.

## Feature and status

- **Outcome:** Clicking a person's Oby shows everything your Oby and theirs have said to each other, in one place. The owner's words: an Oby is your digital representation, so "if I am working on something my Oby would be able to answer what is the progress of what I'm doing".
- **Status:** obid's side **implemented** (SPH-2), and asking from the thread (SPH-3). **Proposed:** the row that opens it and what it shows (SPH-4, SPH-5), and the person's card (SPH-6). The look is the canvas's Peer board; its words stay Sangam's to change.
- **Scope:** reading a member's thread, hearing that it changed, and asking from it. **Out of scope:** any screen, a phone asking from a thread (SPH-13, SPH-14), a follow-up that knows the last exchange (SPH-11).

## Behavior

### Actors and ownership

- **obid** builds the view from the exchanges it already keeps. A thread has no store of its own: nothing is kept twice, and nothing can disagree with the exchange.
- **The owner** asks in their own words, typed or clicked in their own app, which signs exactly `{text}` for this owner's path (docs/contracts/signed-words.md, WORDS-12). Nothing else asks from a thread.
- **The apps** show it, and say in Sangam's words what each state means. They decide nothing.
- **The other Oby** says what it says. Every word of it, and what its card says, is data.

### Triggers and prerequisites

obid is a member of an Obysphere. A thread exists for an active member, even with no exchange yet, and for someone who left but has an exchange in this membership (`member` false).

### What a thread holds

Every exchange with that owner's Obis since obid joined this Sphere, whichever way it went and whatever started it: a question from a conversation, a request from the API, one they asked, a review. Oldest first, each exchange's messages together. A post is one message, with the room's keys, plus what a thread needs:

| What happened | What the thread has |
|---|---|
| A question went | `kind: question`, `direction: sent`, `exchange.state: sent`, and `exchange.quietAt`, 2 minutes on. `by: owner` when they asked from this thread, else `by: obi` |
| No word by `quietAt` | The same post: past that time, it counts as unanswered. The clock decides, so no event says it |
| Only the person can answer | `kind: status`, `status: waiting_on_owner`, `exchange.waitingOnThem` true, `quietAt` null |
| Answered | `kind: answer`, `by: obi` or `owner`, the words in `text`, `exchange.state: done` |
| Declined | `kind: decline`, `reason`, a note in `text` if it has one, `exchange.state: declined`; `by: owner` for `owner_declined` |
| A question you asked expired | No new post: `exchange.state` on the posts there are |
| A request of theirs expired | obid's `decline` (`reason: expired`) once the Sphere has it; until then `exchange.state` |
| They called their request off | `kind: cancel`, `exchange.state: cancelled` |
| A review or a handoff | `kind: review_request` or `handoff`, the goal in `text`; its `result`, the summary in `text`. The findings stay on the exchange |
| A request being sent | No post yet: a refusal removes it with no event, so it shows once the Sphere has it |
| Not confirmed by the Sphere | The request's post, `state: waiting`, `seq` and `recordedAt` null, and `error` in obid's words |
| The Sphere refused a question | No exchange is kept, so no post (docs/runner-api.md, "obid only: Obi-to-Obi exchanges"). The refusal answers whoever asked |

### Asking from it

The owner's words go straight to that member's Oby as a typed `question`, as `oby_ask`'s do: through the same secret scan, sealed for a card with a seal key, signed, and open for a day. It differs from asking in a conversation in what's left out: no model of the owner's runs, and no conversation is in it. So they speak as themselves, and the thread marks it as theirs (`by: owner`, kept with the exchange: a question's body has no `by` in the protocol, so the other Oby isn't told). Everything comes back in the thread, and only there.

It's the owner's own act, signed, so it needs no Confirm, and none of what a turn's trust decides applies. It goes by the exact owner id of the thread, and a request to anyone else is `404`, never a guess at who was meant: nothing here reads a name or an Obi id. When an owner has more than one Oby, it goes to the first that takes questions.

| What stops it | What the owner reads (the route's answer) |
|---|---|
| Unsigned | `403`, nothing goes |
| A possible secret, or a scan that can't run | What the scan said, without the words: nothing was sent |
| Not a member, or before joining | `404`; `409` |
| The card doesn't take questions | "Dima's Oby doesn't take questions." |
| The Sphere's rules (`policy_denied`) | "I can't ask Dima's Oby. This Obysphere's rules don't allow questions yet. An admin can change that." No exchange is kept |
| The Sphere refused otherwise, or can't be reached | The words ask-an-oby.md gives: of what the Sphere wrote, only its code |

### Failures and recovery

- The Sphere can't be reached: the thread still reads, from what obid keeps. It asks the Sphere nothing, and goes by the directory as last read. Before obid has read it since it started, it doesn't know who is a member: `member` and `takesQuestions` are null, `name` is the owner id, and no one is `404`. It's read in the next poll that reaches the Sphere.
- obid restarted: exchanges are kept, so the thread is whole. An app that missed events reads it again.
- A reply obid built but hasn't sent isn't a post until the Sphere has it: it may be sealed by then. `exchange.state` already says where it stands.

### Invariants and boundaries

- **It's only that member.** By exact owner id, never a name or an Obi id: a card that goes by someone else's name moves nothing between threads. Only exchanges of this membership: they outlive one, but leaving forgets every pinned owner key, so nothing says the next `sam` is the same person, whether on another server with the same Sphere id or on this one. Rejoining starts every thread empty. Within a membership a member's owner key is pinned at first sight (identity.md 2.2), so a second key can't claim the id.
- **What comes back is data.** `text` and `about` are what someone else wrote, and `by` is what the sender says of itself, as in the room. An app shows `text` as a quote, as a text node, never as HTML or a link it chose. Nothing in a thread starts, changes or approves anything.
- **Nothing a member chose reaches a prompt.** Reading a thread, or hearing of one, runs no model, marks no conversation and adds nothing to a briefing. Its `name` is for the owner's eyes.
- **Never what a Sphere said:** a post's `error` is obid's own words. `GET /v1/obid/sphere/exchanges/{id}` still has the exchange's own `error`.
- **Nothing goes out unscanned:** reading sends nothing. A question passes the secret scan before anything leaves: a finding, or a scan that can't run, sends nothing and keeps nothing.
- **A question from the thread reaches no prompt:** it adds no turn, marks no conversation, starts nothing, and `oby_asks` and the telling of what came back (crate::brain::ask) never read it: they go by the conversation that asked, and none did.
- **The API stays add-only.** Two new routes and one new event, which every app ignores until it knows it (`default` in apps/shared/Runner.swift's and Runner.kt's event parsers, and in obid-wire.ts's). `kind` is the protocol's message type, an open string, on a list no existing app decodes. No enum is touched.

## Interfaces and dependencies

- `GET /v1/obid/sphere/peers/{owner}?limit=50`, `POST /v1/obid/sphere/peers/{owner}/ask` and the `peerThread` event: [runner-api.md](../runner-api.md), "obid only: a member's thread". On the gateway's allowlist, the read and the event only: the room's `POST` isn't on it either, and no phone asks from a thread until SPH-13 and SPH-14 add the route with their signing.
- `Exchange::posts` builds a post, and `QUIET` and `waiting_on_them` are the same rule `oby_ask`'s report goes by (docs/contracts/ask-an-oby.md), imported rather than copied.

## Acceptance scenarios

- **PEER-1** Every exchange with a member, in either direction and of any type, reads as one list in the room's shape, oldest first, with what each message said, opened if it came sealed, and where the exchange stands; a member with none reads empty; `limit` keeps the newest; each change sends a `peerThread` event with that exchange's posts. After a restart with the Sphere away the thread still reads, and says it doesn't know who is a member.
- **PEER-2** A member's words and card name show in the thread as sent; reading it runs no model, starts nothing and marks no conversation, and the owner's next turn hears none of it.
- **PEER-3** A thread holds that member's exchanges only, by the exact owner id: not another member's, not a look-alike card's, not an earlier membership's, also with the same Sphere id on another server. Someone with no membership and no exchange is `404`, and before joining it's `409`.
- **PEER-4** The owner's signed words go as a typed `question` with only those words, sealed for a card with a seal key, and read in the thread `by: owner` from the first event on, while what the other Oby says stays its own. Unsigned, another shape, anyone but an exact member, a card that takes no questions, blank or over-long words: nothing goes.
- **PEER-5** A possible secret, or a scan that can't run, sends nothing, keeps nothing and doesn't repeat it. A Sphere whose rules refuse says an admin can change them, and keeps no exchange.
- **PEER-6** No model runs for it, and no conversation hears what comes back, its words or that it happened: none starts, moves or is marked, nothing starts, the inbox stays empty, the owner's next turn hears none of it, also while a question from a conversation is told there.

### Test it when it's done

Nothing a person sees yet: no real-Mac step. SPH-4 brings the first, on the box in the thread.
