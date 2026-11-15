# Memory: not asking twice, and the `memory` event

The business behavior contract for what Oby does after the owner answers "Can I remember that?", from [the template](../business-contract-template.md): it remembers a Not this so it doesn't ask again, and it tells the apps each memory change. Scenarios are numbered `MEMORY-n`, and the tests that prove them are named after them (`memory_n_…` in `core/crates/obid/tests/memory_api.rs` and `core/crates/obid/src/memory.rs`). The memory API itself is [docs/api/memory.md](../api/memory.md).

## Feature and status

- **Outcome:** A suggestion the owner declined isn't asked again, and every memory change reaches the Mac and a paired phone as an event, ready for Sangam's screen.
- **Status:** **implemented** in obid (MEMORY-1 to MEMORY-5, task R12-10). No app shows the declined list or reads the `memory` event yet: that waits for Sangam's design.
- **Scope:** the declined list, its match rule and clearing it, the `memory` event, and the memory routes on the gateway. **Out of scope:** any screen; keeping `declined.jsonl` out of exports (R12-12); `POST /v1/memory/find` on the gateway; an event when the declined list is cleared.

## Behavior

### Actors and ownership

- **obid** keeps the declined list, decides what matches it, and sends the event.
- **The owner** says Not this, Remember, or deletes a memory, from the Mac or a paired phone.
- **The apps** show nothing new until Sangam's screen exists. An app that doesn't know the `memory` event ignores it.

### The declined list

- On `POST /v1/memory/suggestions/{id}/dismiss`, obid takes the suggestion out, then adds one line to `home/memory/declined.jsonl`: `{"text", "scope", "repo"}`, where `text` is the suggestion's match key (below), `scope` `user` or `project` and `repo` the repository or null. The file is 0600 in the 0700 `home/memory` folder, and holds the newest 200: writing one more past that drops the oldest. A line that doesn't read is skipped.
- A decline that can't be written is logged, and the dismiss still answers `204`: the suggestion is gone, and Oby may ask it again.
- `DELETE /v1/memory/declined` empties the list and answers `204`; `500` with obid's message when it can't.

### The match rule

- A text's match key is its NFKD form with its combining marks dropped, lower case, straight apostrophes and one space between words: the normalizer `widens` reads through, without its refusals. "RÉBASE first, then run the TESTS." and "Rebase first, then run the tests." have one key. A text with hidden characters or mixed alphabets still has a key, and matches only a text with the same key.
- Find, and Oby's own "Can I remember that?" about how a repository merges, skip a text whose key equals a declined entry's of the same scope and repository, or a saved memory's of the same repository. Declining a text for one repository doesn't silence it for the owner or another repository.

### The owner's yes is never filtered

The owner's Remember on a suggestion, their "Merge, and remember" and their "Ask a teammate, and remember" on a merge card save as they did, even a text they declined before.

### The `memory` event

- After a change is saved, and never on a refusal or a failed write, obid sends one `memory` event per memory or suggestion it changed: `{"change", "id"}`. `change` is `added` (a memory saved; a Remember on a suggestion adds `"suggestion": <its id>`), `removed` (a memory deleted), `used` (a memory followed once more), `suggested` (a suggestion find kept) or `declined` (a suggestion the owner said Not this to). `id` is the memory's or the suggestion's.
- The event carries no title, text or `from`: an app reads `GET /v1/memory` for the rest. It reaches paired phones through the gateway like any event but the Mac's own.

### The gateway

A paired phone may call `GET /v1/memory`, `DELETE /v1/memory/{id}`, `POST /v1/memory/suggestions/{id}/remember` and `POST /v1/memory/suggestions/{id}/dismiss` through the gateway. `DELETE /v1/memory/{id}` includes `DELETE /v1/memory/declined`, so a paired phone may clear the declined list too. `POST /v1/memory/find` and every `PUT` on these paths stay on the Mac.

### Invariants and boundaries

- Declined texts are the owner's own words: the file stays 0600, and no event or answer carries them.
- obid's API only grows: one new route, one new event name and new gateway entries.

## Acceptance scenarios

- **MEMORY-1** (implemented): Given a dismissed suggestion, when find returns the same text with other accents or case, then no new suggestion is made. Tests: `memory_1_a_declined_text_is_not_asked_again` (memory_api.rs), and `memory_1_the_owners_yes_saves_a_declined_workflow` (memory.rs) for Oby's own workflow question and the owner's yes.
- **MEMORY-2** (implemented): Given 200 declines kept, when the next is written, then the oldest is dropped and the file holds 200, at 0600. Tests: `memory_2_the_declined_list_keeps_the_newest_200` (memory_api.rs); the Kani proof `memory_2_the_declined_list_holds_at_most_200` (memory.rs).
- **MEMORY-3** (implemented): Given a declined list, when `DELETE /v1/memory/declined` arrives, then it is empty and the text can be suggested again. Test: `memory_3_clearing_the_declined_list_lets_a_text_be_asked_again` (memory_api.rs).
- **MEMORY-4** (implemented): Given an SSE client, when a memory is saved, removed or used or a suggestion is dismissed, then one `memory` event arrives after the save. Tests: `memory_4_each_memory_change_is_one_event_after_its_save` (memory_api.rs), `memory_4_a_use_is_one_event_after_its_count` (memory.rs).
- **MEMORY-5** (implemented): Given a paired phone, when it calls `GET /v1/memory` through the gateway, then it is allowed, and `PUT` on the same path is not. Tests: `memory_5_the_gateway_allows_memory_reads` (memory_api.rs), `memory_5_a_phone_reads_memory_but_never_finds` (memory.rs), `the_gateway_allows_only_the_listed_routes` (gateway.rs).
