# Home: the apps never show a job, conversation or inbox state a restart takes back

The business behavior contract for how a change to a job, a conversation or an inbox item reaches obid's memory and the apps, from [the template](../business-contract-template.md). The API is `docs/runner-api.md` (`GET /v1/jobs/{id}`, `POST /v1/jobs`, `POST /v1/plans`, `GET /v1/inbox`, `POST /v1/obid/decisions`).

## Feature and status

- **Outcome:** What Home and a job's page show of a job is what obid kept on disk, and so is what a conversation and the inbox show. A job never shows a state, a revision or a whole job that a restart of obid then takes back; a conversation never shows a change, and the inbox never shows a new item, that a restart takes back. A save that fails is an error the caller sees, not only a log line.
- **Status:** HOME-1 and HOME-2 **Implemented** (roadmap R02-01). HOME-3 and HOME-4, the same rule for conversations and the inbox's new items, **Implemented** (roadmap R02-02).
- **Scope:** Every change obid makes to a job and tells the apps of with a `job` event (`Jobs::update`, `insert`, `hear_at_work`, `message_as` and `dismiss`); every change obid makes to a conversation and tells the apps of with a `conversation` or `turn` event; every `inbox` event; and every new inbox item obid makes on its own (a job's permission, GitHub's work, obid's own cards and confirms, the brain's offers, a teammate approval to remove on GitHub). Outside it: a typed transition layer for jobs (R04), append-only turns (R05), the saves obid makes at startup (which send no event), trying a failed save again, and what "Still open" below names.

## Behavior

### Actors and ownership

- **obid** owns the rule: memory and events follow the disk, never lead it. A change is made on a copy of the job, under the lock it read the job under; the copy is saved to `home/jobs/<id>.json`, and only then does it replace the job in memory and go to the apps as a `job` event. What follows the change (a worker woken, the room told, the words noted in the transcript, the cleanup pass woken) follows the save too.
- **The apps** show nothing new. A request that changed a job and couldn't be saved answers `500` with why, instead of `2xx`; so does `POST /v1/plans` for words a conversation couldn't keep, and `POST /v1/obid/decisions` for an item the inbox couldn't keep.

### Rules

- Two changes to one job never lose each other: each is read, made and saved under the same lock, one after the other.
- A save that fails changes nothing: the job in memory keeps its content and its `revision`, no `job` event is sent, and nothing that would follow the change happens. obid logs `can't keep job <id>: <reason>`.
- The one exception fails closed: a mark that only restricts a job (`foreign`, `reached`, `untrusted`, `unsigned`, `memoryFromOthers`, `ranUnproxied`, `withheld`, `checkWithheldWhy`, and `vetted` turned off) stays on the job in memory though its save failed, and the job's next save keeps it. A newer `checkWithheldWhy` reason replaces the older one, and one cleared by a save that failed keeps its older reason. A job is never treated as less marked than obid found it to be, so its check never gets the owner's declared secrets and its pull request never merges on one review because a save failed.
- A job `POST /v1/jobs` makes from words no app of the owner's signed is `unsigned`, and carries its request key, from its first save: it never exists, in memory or on disk, without them.
- A new job whose save fails doesn't exist: not in memory, not in `home/jobs`, not in `GET /v1/jobs`, and no event says it was made.
- A conversation's change goes into memory, and out as a `conversation` or `turn` event, only once it's saved in `home/conversations`. One whose save failed leaves the conversation as it was. When an account is removed and a conversation that chose it can't be saved without it, that conversation keeps its choice, in memory as on disk, and no event says otherwise; obid logs `conversation <id>: its choice stays: <reason>`. The marks that only restrict a conversation (what it read that wasn't the owner's) stay in memory though their save failed, as a job's do.
- An `inbox` event goes out only for an item obid saved in `home/inbox`, or again for an item in the inbox whose sort changed (kept in `home/triage.json`, not in the item). obid logs `can't keep inbox item <id>: <reason>` for one it couldn't.
- A new inbox item whose save failed doesn't exist: not in memory, not in `GET /v1/inbox`, and no event says it was made. A later pass may make it again.
- obid keeps, for each job, why its last save failed, until a later save of it succeeds or it's deleted. A route whose change didn't land answers `500` `The job couldn't be kept: <reason>`, never `404` "There's no such job.", `409` or `2xx`; an inbox answer that carries a change out answers the same.

### Failures and recovery

- A failed save isn't tried again by obid. The same request, once the disk takes it, lands and is sent once.
- Still open, each left to a later task: a change to an inbox item that exists already (answered, done, put off, claimed, reopened, settled) whose save failed stays in obid's memory, and in `GET /v1/inbox`, with no event, until a restart takes it back. Items made together with an exchange (another Obi's review request or question, a teammate approval ask, the sharing card) still enter memory before their save. Exchanges send their `exchange` event whatever their save did.
- A worker whose change couldn't be kept doesn't loop on it: a retry counted only in the job's file ends the job's run with "the job couldn't be kept" instead of trying for ever.

## Interfaces and dependencies

- `GET /v1/jobs/{id}` and every job route: `500` with `The job couldn't be kept: <reason>`, no new keys.
- `POST /v1/jobs`: `500` with the same message when the new job couldn't be saved; a refusal is still `422`.
- `GET /v1/events`: a `job`, `conversation`, `turn` or `inbox` event only for a change obid saved.
- `POST /v1/plans`: `500` when the conversation couldn't keep the words.
- `POST /v1/obid/decisions`: `500` with `The item couldn't be kept: <reason>` when the `confirm` item couldn't be saved, instead of `202`.

## Acceptance scenarios

- **HOME-1.** Given a job and a `home/jobs` obid can't write, when `PATCH /v1/jobs/{id}` renames it, then the answer is `500` with "couldn't be kept", no `job` event for it comes within 2 seconds, and `GET /v1/jobs/{id}` shows its old `revision` and `title`. Once the folder is writable again, the same rename answers `200` with one `job` event. Test: `home_1_a_failed_save_sends_no_event` (core/crates/obid/tests/jobs_api.rs); in memory, with a dismissal and a new job too: `a_failed_save_leaves_insert_update_and_dismiss_as_they_were` (core/crates/obid/src/jobs.rs); two threads changing one job lose nothing: `two_changes_to_one_job_both_land` (core/crates/obid/src/jobs.rs).
- **HOME-2.** Given a `home/jobs` obid can't write, when `POST /v1/jobs` makes a job, then the answer is `500` with "couldn't be kept", no `job` event comes, `GET /v1/jobs` lists the same jobs as before, and no new `*.json` is in `home/jobs`. Test: `home_2_a_failed_save_makes_no_job` (core/crates/obid/tests/jobs_api.rs).
- **HOME-3.** Given a conversation and a `home/conversations` obid can't write, when `POST /v1/plans` brings the owner's words to it, then the answer is `500`, no `conversation` or `turn` event comes within 2 seconds, and `GET /v1/conversations` is unchanged; once the folder is writable again, the same words answer `200`. Test: `home_3_a_failed_conversation_save_sends_no_event` (core/crates/obid/tests/conversations_api.rs). A removed account a conversation chose, when that conversation can't be saved: no `conversation` event, and it keeps the choice; once it can, one event, and the choice is gone. Test: `home_3_a_forgotten_account_that_cant_be_kept_sends_no_event` (core/crates/obid/src/conversation.rs).
- **HOME-4.** Given a joined obid and a `home/inbox` obid can't write, when `POST /v1/obid/decisions` asks for a teammate approval, then the answer is `500` with "couldn't be kept", no `inbox` event comes within 2 seconds, and `GET /v1/inbox` lists no `confirm` item. Once the folder is writable again, the same request answers `202`, one `inbox` event carries the item, and `GET /v1/inbox` lists it; with the folder unwritable again, putting it off sends no `inbox` event for it. Test: `home_4_a_failed_inbox_save_sends_no_event` (core/crates/obid/tests/sphere_api.rs).
