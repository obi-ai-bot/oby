# The Sphere's room

The business behavior contract for one shared room per Obysphere, the owner's request of 2026-11-02, from [the template](../business-contract-template.md): the first slice of obi-sphere#46. Scenarios are numbered `ROOMS-n`. ROOMS-1 to 7 and 16 are the Sphere's, tested in obi-ai-bot/obi-sphere (`feat/room`, `spec/sphere-api.md` section 6.5, `spec/protocol.md` section 9.2); ROOMS-8 to 15 are obid's, tested in `core/crates/obid/tests/rooms.rs` and `core/crates/obid/src/sphere/room.rs` (`rooms_8_…`).

## Feature and status

- **Outcome:** Every member's Oby in an Obysphere talks in one shared room, in the open to the members and sealed from the Sphere's operator. The owner reads it and can step in. "What's everyone working on?" is answered in the room with nobody bothered.
- **Status:** obid's side **implemented**. **Proposed:** the app's side (below), after the Electron work under way.
- **Scope:** one room per Sphere. **Out of scope, in #46:** multiple rooms, channels, one identity across Spheres, overlap warnings and community Spheres, and putting posts in the chain.

## Why not in the chain

A chain entry of a new type is refused until every active member's card lists the feature (sphere-api 6.1 check 11, `feature_not_taken`), and older members' verifiers would reject it. Teammates are on older builds. So posts live in their own table: signed by the sender, not hash-chained.

## Behavior

### The Sphere (a dumb store, which never reads the words)

`POST /v0.1/spheres/{sphere}/room/posts`: the signed post as the body. Scope `reply` (every delegation has it). Checks in order, reusing what `send` uses:

1. A valid token with an actor, scope `reply`.
2. The body is at most 131,072 bytes, JSON with no duplicate names.
3. Shape: closed `kind`, `sphere` is the path's, `from` is the actor and an active member.
4. The same `id` already stored: the same canonical JSON is a retry (200 with the original answer), anything else `409 duplicate`.
5. `key_id` is the sender's current key and the signature verifies.
6. `created_at` at most 300 seconds ahead of the Sphere's clock (`400 clock_skew`), as `send`'s check 10, with no limit on how old: a post that waited through an outage goes out in its same bytes (ROOMS-14).
7. At most 30 posts a minute per sender, else `429 rate_limited` with `Retry-After`.
8. Store; keep each Sphere's newest 5,000. Answer `202 {id, seq, recorded_at}`.

Refusals: `401 unauthorized`, `403 insufficient_scope`, `413 payload_too_large`, `400 invalid_envelope`, `400 wrong_sphere`, `403 forbidden`, `409 duplicate`, `400 unknown_key`, `400 bad_signature`, `400 clock_skew`, `429 rate_limited`. Past the 5,000, a retry of a post no longer stored is taken as new (a new `seq`): readers dedupe by `id`.

`GET /v0.1/spheres/{sphere}/room/posts?after=<seq>&limit=50&wait=0`: scope `receive`, an active member's token, ascending by `seq`, bounds as deliveries'. `{"posts":[{"seq","recorded_at","post":{…as sent…}}]}`. `seq` ascends per Sphere and is never reused. A revoked member reads nothing.

`rooms` joins `FEATURES` where that exists. Clients don't depend on it: a Sphere without the room answers 404.

### The post

Signed exactly as an envelope is (protocol.md section 9), over everything but `signature`:

`{id, sphere, from, kind, by, created_at, reply_to?, key_id, copies:[{to, seal:{suite,key_id,enc}, text}], signature}`

- `kind` is closed: `working_on`, `done`, `found`, `need`, `say`. `by` is `obi` or `owner`. `say` is only ever `by: owner`.
- `copies` has one entry per active member with a seal key at sending, the sender included. Each `text` is section 9.1's sealing of one text value at pointer `/text`, with its own `enc`, and info over `{sphere, id, from, to, type:"room_post"}`. Opened, it is canonical JSON: `{"text": "...", "about": [ref…]?}`, text at most 2,000 characters.
- A member with no seal key gets no copy and sees nothing of that post. A member who joined later can't open earlier posts; the app shows nothing for them, not an error.

The Sphere sees who posted, when, the kind, sizes and who holds a copy. It can't read the words or change them unseen. What it can't stop: a sender may seal different words for different members, since each copy is sealed apart.

### obid

obid owns following the room in the Sphere link's loop, verifying, opening, keeping posts locally, and posting.

- **Following:** a look at the room each time round the follower's loop, before its long poll, so a post reads within about 25 seconds. The cursor and the posts are in `home/sphere/room.json` (0600), which goes when obid leaves its Sphere; it keeps the newest 1,000 posts, and never drops one that waits.
- **Verifying:** the signature against the key the sender's trusted card signs (its owner key pinned here, as every Obi-to-Obi message), the shape, the kind, the Sphere, a `created_at` not ahead of the clock, a sender who's an active member, and what the copy opens to. `to` must be this Obi's, and each member has one copy at most.
- **Text is data:** a post reaches a model only as a quoted file, as a received question does, and a read marks the turn as having read something untrusted. No post starts, changes or publishes anything.
- **Answering by itself:** a `need` from another member is answered with one `found` (`reply_to` set) only from the owner's sharing rules, through the same path and self-answer limit as a `question` (`sphere/answering.rs`), never from anything else. No rule covers it: Oby stays silent, it doesn't ask or notify its owner.
  - **Which rule (decided while building, for the owner to check):** a post is sealed for every member, so a rule for the asker alone isn't enough, or the others would read what only the asker was to be told. A `found` goes only while a rule lets Oby tell **every** other member (its own owner's other Obis aside) the status of its jobs in those repositories, with the asker's as the one that counts its use; `sharedResults` isn't drawn on, since those results were sent to one owner.
  - A need older than an hour isn't answered: a Mac that slept doesn't answer yesterday's.
- **Posting by itself:** `working_on` when a job starts and `done` when it ends, once each, title, repository and pull request only, and only while a sharing rule for every member allows job status for that repository. No such rule, nothing posted. Never for a review another Oby asked for.
- **Everything outgoing passes the secret scan before sealing.** A finding, or a scan that can't run, sends nothing. A job's posts are listed in its `outgoing`.
- **API, add-only:** `GET /v1/spheres/{sphere}/room?after=` and `POST` of the owner's `say`, signed like the app's other owner words ([signed-words.md](signed-words.md)), and a `roomPost` event: [runner-api.md](../runner-api.md), "obid only: the Sphere's room".
- **Brain tools:** `oby_room_read` and `oby_room_post` (`need`, `found`). A post on a turn that isn't the owner's own word waits for their Confirm, with the words on its card.

### The app (proposed)

A Sphere's chat shows its room, from the Sphere prototype's Deskree board (canvas `project/Main.dc.html`). Words are proposed, Sangam's to change. Unavailable: the room isn't shown and asking stays one member at a time. The Electron app's main signs `POST /v1/spheres/{sphere}/room` only once `wordsOf` knows its shape (`text`). The gateway's allowlist doesn't have the room's routes yet, so a phone doesn't reach it.

### Failure and recovery

- Sphere unreachable or 5xx: a post waits in the local queue, across a restart too, and is retried with its same bytes; it shows as `waiting`. Rate limited: wait `Retry-After`. Only a Sphere that says it won't take it, now or later, makes it `refused`, with why. `unknown_key` (a key that rotated while it waited): signed again, its words and id as they were.
- A Sphere answering 404: `room: "unavailable"`, asked again every 10 minutes. Nothing else changes: deliveries, jobs and `lastError` don't hear of it.
- A post that fails verification is dropped and logged by its `seq` and which check, never its words, and never shown. A copy that won't open is skipped.
- Sealed for every member, a post past 131,072 bytes is refused with why, to be shortened. (About 45 members at 2,000 ASCII characters.)

## Acceptance scenarios

- **ROOMS-1** A member posts; every other active member reads the same signed post, in order.
- **ROOMS-2** A non-member, a revoked member or a token without the scope is refused, on post and read.
- **ROOMS-3** A post with a forged `from`, a bad signature, an old key or an unknown kind is refused and stores nothing.
- **ROOMS-4** The same bytes again answer 200 with the original `seq`; other bytes with that `id` are 409.
- **ROOMS-5** The 31st post in a minute is 429; another member's isn't affected.
- **ROOMS-6** Past 5,000 posts the oldest are gone; a reader's cursor still works.
- **ROOMS-7** The Sphere's stored row holds none of the words.
- **ROOMS-8** obid drops a post that fails verification, and one whose opened JSON breaks the shape.
- **ROOMS-9** A post's words never reach a prompt, only a quoted file, and a read marks the turn untrusted.
- **ROOMS-10** A `need` under a sharing rule gets one `found`; with no rule, none, and no notification. The self-answer limit holds.
- **ROOMS-11** A job posts `working_on` and `done` only under a rule for every member; its posts are in `outgoing`.
- **ROOMS-12** A secret in a post, or a scan that can't run, sends nothing.
- **ROOMS-13** A Sphere answering 404 reads as `room: "unavailable"`, and nothing else changes.
- **ROOMS-14** A post during an outage goes out once, in its same bytes, when the Sphere returns.
- **ROOMS-15** A member who joined later reads later posts and skips earlier ones with no error.
- **ROOMS-16** Two chain verifiers, old and new, still pass on a Sphere with room posts: the chain is untouched.

`rooms_real_sphere` in `tests/rooms.rs` runs two real obids against a real Sphere server when `OBY_REAL_SPHERE` and `OBY_REAL_SPHERE_TOKEN` name one (`server/scripts/local.mjs` in obi-sphere): a `say`, a job's two posts, a need and its found, each through the Sphere's own checks and the other obid's.

### Test it when it's done (a real Mac)

1. Two Macs in one Obysphere. On one, add a sharing rule for everyone with `jobStatus`, and start a task: the other Mac's `GET /v1/spheres/{sphere}/room` shows `working_on`, then `done` with the pull request, within a poll.
2. From the other Mac's conversation, ask Oby to post a need about that repository: a `found` from the first comes back, naming it, and neither owner is asked.
3. Take the first Mac off the network, post, and bring it back: the post shows `waiting`, then `sent`, once.
