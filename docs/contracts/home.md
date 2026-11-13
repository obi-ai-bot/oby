# Home: the apps never show a job state a restart takes back

The business behavior contract for how a change to a job reaches obid's memory and the apps, from [the template](../business-contract-template.md). The API is `docs/runner-api.md` (`GET /v1/jobs/{id}`, `POST /v1/jobs`).

## Feature and status

- **Outcome:** What Home and a job's page show of a job is what obid kept on disk. A job never shows a state, a revision or a whole job that a restart of obid then takes back. A save that fails is an error the caller sees, not only a log line.
- **Status:** HOME-1 and HOME-2 **Implemented** (roadmap R02-01). HOME-3 and HOME-4, the same rule for conversations and the inbox, are left to R02-02.
- **Scope:** Every change obid makes to a job and tells the apps of with a `job` event (`Jobs::update`, `insert`, `hear_at_work`, `message_as` and `dismiss`). Outside it: conversations and the inbox's own saves (R02-02), a typed transition layer for jobs (R04), the saves obid makes of jobs at startup (which send no event), and trying a failed save again.

## Behavior

### Actors and ownership

- **obid** owns the rule: memory and events follow the disk, never lead it. A change is made on a copy of the job, under the lock it read the job under; the copy is saved to `home/jobs/<id>.json`, and only then does it replace the job in memory and go to the apps as a `job` event. What follows the change (a worker woken, the room told, the words noted in the transcript, the cleanup pass woken) follows the save too.
- **The apps** show nothing new. A request that changed a job and couldn't be saved answers `500` with why, instead of `2xx`.

### Rules

- Two changes to one job never lose each other: each is read, made and saved under the same lock, one after the other.
- A save that fails changes nothing: the job in memory keeps its content and its `revision`, no `job` event is sent, and nothing that would follow the change happens. obid logs `can't keep job <id>: <reason>`.
- The one exception fails closed: a mark that only restricts a job (`foreign`, `reached`, `untrusted`, `unsigned`, `memoryFromOthers`, `ranUnproxied`, and `vetted` turned off) stays on the job in memory though its save failed, and the job's next save keeps it. A job is never treated as less marked than obid found it to be, so its check never gets the owner's declared secrets and its pull request never merges on one review because a save failed.
- A job `POST /v1/jobs` makes from words no app of the owner's signed is `unsigned`, and carries its request key, from its first save: it never exists, in memory or on disk, without them.
- A new job whose save fails doesn't exist: not in memory, not in `home/jobs`, not in `GET /v1/jobs`, and no event says it was made.
- obid keeps, for each job, why its last save failed, until a later save of it succeeds or it's deleted. A route whose change didn't land answers `500` `The job couldn't be kept: <reason>`, never `404` "There's no such job.", `409` or `2xx`; an inbox answer that carries a change out answers the same.

### Failures and recovery

- A failed save isn't tried again by obid. The same request, once the disk takes it, lands and is sent once.
- A worker whose change couldn't be kept doesn't loop on it: a retry counted only in the job's file ends the job's run with "the job couldn't be kept" instead of trying for ever.

## Interfaces and dependencies

- `GET /v1/jobs/{id}` and every job route: `500` with `The job couldn't be kept: <reason>`, no new keys.
- `POST /v1/jobs`: `500` with the same message when the new job couldn't be saved; a refusal is still `422`.
- `GET /v1/events`: a `job` event only for a change obid saved.

## Acceptance scenarios

- **HOME-1.** Given a job and a `home/jobs` obid can't write, when `PATCH /v1/jobs/{id}` renames it, then the answer is `500` with "couldn't be kept", no `job` event for it comes within 2 seconds, and `GET /v1/jobs/{id}` shows its old `revision` and `title`. Once the folder is writable again, the same rename answers `200` with one `job` event. Test: `home_1_a_failed_save_sends_no_event` (core/crates/obid/tests/jobs_api.rs); in memory, with a dismissal and a new job too: `a_failed_save_leaves_insert_update_and_dismiss_as_they_were` (core/crates/obid/src/jobs.rs); two threads changing one job lose nothing: `two_changes_to_one_job_both_land` (core/crates/obid/src/jobs.rs).
- **HOME-2.** Given a `home/jobs` obid can't write, when `POST /v1/jobs` makes a job, then the answer is `500` with "couldn't be kept", no `job` event comes, `GET /v1/jobs` lists the same jobs as before, and no new `*.json` is in `home/jobs`. Test: `home_2_a_failed_save_makes_no_job` (core/crates/obid/tests/jobs_api.rs).
