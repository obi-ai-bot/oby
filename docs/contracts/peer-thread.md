# A member's thread

The business behavior contract for one thread per member of your Obysphere, from [the template](../business-contract-template.md): a view over every exchange with that member's Oby, in the shape of the room ([rooms.md](rooms.md)). Scenarios are numbered `PEER-n`, and the tests that prove them are named after them (`peer_n_…` in `core/crates/obid/tests/peer_flow.rs`, `core/crates/obid/tests/sphere_questions.rs`, `core/crates/obid/src/sphere/exchange.rs`, `core/crates/obid/src/sphere/sharing.rs`, `core/crates/obid/src/sphere/link/asking.rs` and `apps/electron/src/renderer/src/test/`). The drives on the plan's Sphere tab are numbered `PEER-n` too and are a different list: human-drive steps, not these.

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
| A Council's request not confirmed by the Sphere | The request's post, `state: waiting`, `seq` and `recordedAt` null, and `error` in obid's words. Only a Council's: a question whose send got no answer isn't kept, so it has no post |
| The Sphere refused a question | No exchange is kept, so no post (docs/runner-api.md, "obid only: Obi-to-Obi exchanges"). The refusal answers whoever asked |

### Asking from it

The owner's words go straight to that member's Oby as a typed `question`, as `oby_ask`'s do: through the same secret scan, sealed for a card with a seal key, signed, and open for a day. It differs from asking in a conversation in what's left out: no model of the owner's runs, and no conversation is in it. So they speak as themselves, and the thread marks it as theirs (`by: owner`, kept with the exchange: a question's body has no `by` in the protocol, so the other Oby isn't told). Everything comes back in the thread, and only there.

It's the owner's own act, signed, so it needs no Confirm, and none of what a turn's trust decides applies. The signature protects that mark: it doesn't stop what holds obid's token sending a question, which `POST /v1/obid/sphere/requests` still does, unsigned and marked `obi`. It goes by the exact owner id of the thread, and a request to anyone else is `404`, never a guess at who was meant: nothing here reads a name or an Obi id. With no Confirm, the app is the only place the owner sees who: **SPH-4 must show the owner id beside a name that differs from it, as `whose_oby` does.** When an owner has more than one Oby, it goes to one that takes questions, one whose words go sealed first.

obid reads the directory again as the question goes, and a Sphere's operator could serve that Oby by then as someone else's, under an owner key pinned at first sight and a seal key of its own. So it goes only while the Oby still says it acts for that owner id, and otherwise nothing goes. `oby_ask` holds to the same, on the owner's word and on their Confirm.

| What stops it | What the owner reads (the route's answer) |
|---|---|
| Unsigned | `403`, nothing goes |
| A possible secret, or a scan that can't run | What the scan said, without the words: nothing was sent |
| Not a member, or before joining | `404`; `409` |
| The card doesn't take questions | "Dima's Oby doesn't take questions." |
| The Oby now says it acts for someone else, or dropped its seal key | "I can't ask Dima's Oby: nothing was sent." `409` |
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

## Being asked to share, and what your Oby tells

The other side of a thread: for a member's Oby to answer by itself, its owner must have said it may. A membership from before the starter rule (docs/contracts/ask-an-oby.md, "The default") has no rule, so every question waits on its owner and the asker reads "It's waiting on Dana". The words below are proposals: this design leads for now, by the owner's decision, and Sangam can change them.

- **Asked once, where the owner already is.** The first question that waits on an owner who has no rule, and was never asked, brings one Confirm card on Home (and on the phones, and as a Mac notification, like any Confirm): "Can I tell people in your Obysphere how your tasks are going?", then what it tells, that it's every repository, and what it never tells, all of it in the three lines Home's card shows. obid marks it asked when it makes the card, so no other question, member or restart asks again. Nothing else asks: not obid's start, not a join.
- **The card holds obid's own words only.** Neither who asked nor their words are in it: nothing a member chose shapes what the owner's device signs. The question itself is on its own card, as ever.
- **Yes.** One rule, `{"for": "*", "repos": "*", "tells": ["jobStatus"]}`, answering who asks and opening no room, since the card promises nothing about the room. Every question waiting on the owner then goes by it, as if it had come now: so the asker who read "It's waiting on Dana" gets their answer, and a member isn't stuck behind a question they already asked (`busy`). One the material can't answer comes back to the owner. "Questions from other Obys" says it was answered: "I answered this question from Dana's Oby by myself:", quoted.
- **No (Cancel).** Nothing changes, and it doesn't ask again. Questions keep waiting on the owner.
- **Listed.** The Obysphere's settings list "What your Oby tells": each rule's what, to whom, in which repositories and how many times it answered, with Remove. With nothing shared, "Nothing yet" and a button that shares, so no and Remove aren't one-way.
- **Removed stays removed.** Removing any rule also marks the owner asked: no start, question or join brings the rule or the card back. Only their own click does. A `sharing.json` from before this that a person emptied by hand through the API can't be told from an upgrade's, so it gets the one card; a card only asks. A file that doesn't read shares nothing and stays as it is: no card, which would write over it.
- **How the click is signed.** It's a Confirm (docs/runner-api.md, "Confirm takes the owner's tap"): the device signs `oby-confirm\n<item id>\n<title>\n<signedAt>`, within two minutes, once, and obid refuses `Confirm` without it (`403`, the item open, no rule). In the Electron app, main signs it, and for this action only after a real click it saw itself (`mayConfirm`, `takeInput`: `docs/contracts/signed-words.md`), so a script in the page can't either; a phone signs on the owner's tap. Settings' button is one such click: it opens the card and confirms it. **No agent, other Oby or job can grant it:** agents can't reach these routes, another Oby sends only typed actions and a question's words are data, and inside obid nothing confirms without a device.

**How this and "only the repositories I choose" fit (SPH-8): one question, not two.** There is one latch (`offered`) and one card, and SPH-8 changes what the card asks instead of adding a second question: it lists the repositories to tell about, and a Confirm signs a title that names them. SPH-8 also stops the first join from making a rule, so a new member and an old one reach the same card at their first question. Until it lands the card says "in every repository", plainly.

## Interfaces and dependencies

- `GET /v1/obid/sphere/peers/{owner}?limit=50`, `POST /v1/obid/sphere/peers/{owner}/ask` and the `peerThread` event: [runner-api.md](../runner-api.md), "obid only: a member's thread". On the gateway's allowlist, the read and the event only: the room's `POST` isn't on it either, and no phone asks from a thread until SPH-13 and SPH-14 add the route with their signing.
- `GET /v1/obid/sharing`, `POST /v1/obid/sharing/offer`, `DELETE /v1/obid/sharing/rules/{id}` and the `share_status` Confirm: [runner-api.md](../runner-api.md), "obid only: sharing rules".
- `Exchange::posts` builds a post, and `QUIET` and `waiting_on_them` are the same rule `oby_ask`'s report goes by (docs/contracts/ask-an-oby.md), imported rather than copied.

## Acceptance scenarios

- **PEER-1** Every exchange with a member, in either direction and of any type, reads as one list in the room's shape, oldest first, with what each message said, opened if it came sealed, and where the exchange stands; a member with none reads empty; `limit` keeps the newest; each change sends a `peerThread` event with that exchange's posts. After a restart with the Sphere away the thread still reads, and says it doesn't know who is a member.
- **PEER-2** A member's words and card name show in the thread as sent; reading it runs no model, starts nothing and marks no conversation, and the owner's next turn hears none of it.
- **PEER-3** A thread holds that member's exchanges only, by the exact owner id: not another member's, not a look-alike card's, not an earlier membership's, also with the same Sphere id on another server. Someone with no membership and no exchange is `404`, and before joining it's `409`.
- **PEER-4** The owner's signed words go as a typed `question` with only those words, sealed for a card with a seal key, and read in the thread `by: owner` from the first event on, while what the other Oby says stays its own. Unsigned, another shape, a signature for another owner or used before, anyone but an exact member, someone revoked, a card that takes no questions, blank or over-long words, or a Sphere that serves the Oby as someone else's between two reads: nothing goes. Of several Obys it goes to one that takes questions and seals; one that dropped its seal key is last and refuses, sending nothing in the clear. A directory that can't be read says so, also for someone who joined since the last read.
- **PEER-5** A possible secret, or a scan that can't run, sends nothing, keeps nothing and doesn't repeat it. A Sphere whose rules refuse says an admin can change them, and keeps no exchange.
- **PEER-6** No model runs for it, and no conversation hears what comes back, its words or that it happened: none starts, moves or is marked, nothing starts, the inbox stays empty, the owner's next turn hears none of it, also while a question from a conversation is told there.
- **PEER-16** A member from before is asked once, by the first question that waits on them: not by a second, not again after a restart, not again after a no. The card holds none of a member's words, and a file that doesn't read gets no card.
- **PEER-17** Nothing is shared silently: not by the question, its words, the card, the API token alone or Settings opening the card. A Confirm with the device's signature makes one status rule that answers who asks and opens no room, and in the Electron app main signs it only on a click it saw itself. The questions that waited go by it: answered by the Oby and off the owner's list, or back with the owner when the material can't answer. Settings lists the rule in plain words.
- **PEER-18** A rule the owner removed stays removed, across a restart and later questions, and nothing asks for it again; only their own click brings it back.

### Test it when it's done

The thread itself has no real-Mac step yet: SPH-4 brings the first, on the box in the thread.

On a real Mac that is already in an Obysphere from before the starter rule (or with `home/sharing.json` holding `{"rules": []}`), with a teammate's Oby:

1. The teammate asks how your work is going: they read "It's waiting on" you, and Home shows the one card.
2. They ask something else: no second card. Confirm: they get an answer from your Oby, and your list is clear.
3. The Obysphere's settings list what your Oby tells. Remove it: the next question waits on you, and nothing asks again.
4. Share from the settings: the next question is answered by itself.
