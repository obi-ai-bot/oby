# Failures: each job's trace of its dispatcher steps

The business behavior contract for the trace obid keeps of each job's dispatcher steps, and the part of a failure record it fills, from [the template](../business-contract-template.md). The API is [docs/api/failures.md](../api/failures.md) and `GET /v1/jobs/{id}/trace` in [docs/api/jobs.md](../api/jobs.md).

## Feature and status

- **Outcome:** When a job fails, what obid did with it (when it was admitted, how long each phase took, where it stalled, retried, moved or was picked up again after a restart) can be read back, step by step, with no word of the job's own in it.
- **Status:** **Implemented**, FAILURES-1 to 4.
- **Scope:** obid's dispatcher steps of every job, the trace's file, its cap, its removal on delete, `evidence.log` of a failure record and the route. **Out of scope:** an MCP tool that reads the trace (a tool on obid's agent servers needs its own gates, [agent-mcp.md](agent-mcp.md)); the crash record reading outcome codes (`failures::found` decides the trigger from the job as before); the post-mortem (R09-05), and `reviews`, `worktree` and `source` of the evidence; the 24-hour cleanup pass ([cleanup.md](cleanup.md)), which keeps the job and so its trace.

## Behavior

- **Actors and ownership:** obid alone writes the trace, from the job's state; no agent, client or other Oby writes it. Clients read it only through the route and the failure record: no event, and no words a person sees.
- **The file:** `home/jobs/<id>/trace.ndjson` (0600, in a 0700 folder), one JSON object per line, keys in this order: `{"step":"round","round":1,"start":"<ISO date>","seconds":12,"outcome":"ok"}`. `round` is left out but on a `round` line. `start` is an ISO date, `seconds` a whole number of 0 or more. A line holds nothing else: no reason, summary, path, command, prompt or any agent's or owner's text, redacted or not.
- **Step names** (closed): `admit`, `prepare`, `setup`, `round`, `check`, `publish`, `rebase`, `stall`, `retry`, `account_move`, `park`, `restart_recovery`, `crash`.
- **Outcome codes** (closed), from the job's status once the step ended: `ok` (running or done), `queued`, `failed`, `limit` (failed at one of obid's limits), `interrupted`, `stopped`.
- **What writes each step:** every change saved through `Jobs::keep` compares the job before and after (`jobs/trace.rs`, `ended`), and writes, in this order:
  - the phase it left (`prepare`, `round`, `check` or `publish`), when it was running in that phase and is now in another phase or no longer running;
  - `stall` when its stall count grew, `retry` when its error count grew, `account_move` when it moved to another account, `park` when it was parked, `crash` when its reason became obid's own bug or obid going down with it;
  - `admit` when it went from queued to running.
  A new job writes nothing. `restart_recovery` is written when obid starts again, for each job it was at work on: `interrupted` when it leaves the job for the owner, `queued` when it picks it up. `setup` is written when a repository's setup ran before the agent and wasn't stopped, `ok` or `failed`. `rebase` is **not written yet**: no boundary in obid marks it.
- **Start and seconds:** `admit`, `prepare`, `round`, `check` and `publish` are spans: each starts where the trace's last span line ended (its `start` plus `seconds`), or at the job's `created` when it has none, and lasts until the change that ended it. So an `admit` after a job ended and was continued counts from its last span line, the time it sat ended included. The marks (`stall`, `retry`, `account_move`, `park`, `crash`, `restart_recovery`) start when they're written and last 0 seconds. `setup` carries its own start and duration. Marks and setup don't move where the next span starts. A `round` line's `round` is the round the job was in.
- **The cap:** a trace keeps at most 2,000 lines; before a line is added to a full one, its oldest lines go.
- **Failures and recovery:** a write that fails never fails, delays or changes the job: it is logged once, naming the job and the I/O error. A line that doesn't decode is skipped when the trace is read.
- **The failure record:** a record's `evidence.log` is the job's trace's last 50 lines as stored, read when the record is written. A record written before traces loads with `log` empty.
- **Removal:** deleting a job (`DELETE /v1/jobs/{id}`, or a confirmed `POST /v1/jobs/{id}/delete`) removes `home/jobs/<id>/`, the trace with it. A trace is never written for a job obid no longer keeps.

## Interfaces and dependencies

`core/crates/obid/src/jobs/trace.rs`, called from `Jobs::keep` (`jobs/keep.rs`), `Jobs::open` (`jobs.rs`), `Jobs::setup` (`jobs/setup.rs`) and `Jobs::remove_folders` (`jobs/lanes.rs`); read by `failures.rs` and `routes.rs`. `GET /v1/jobs/{id}/trace` answers `{"lines": [...]}`, add-only.

## Acceptance scenarios

- **FAILURES-1:** A job whose repository has a check runs one round and its check and ends: its trace's steps begin `admit`, `prepare`, `round` (`round` 1), `check`, each line's keys among `step`, `round`, `start`, `seconds` and `outcome`, `seconds` 0 or more and `outcome` one of the six codes; neither the file nor the route's answer holds the job's task or anything its agent printed. A job that left its work phase ends a `round` first, and a queued job that starts running writes `admit` last, with no phase line. (`failures_api::failures_1_a_job_leaves_its_steps_in_order`; Kani: `jobs::trace::proofs::failures_1_a_phase_left_writes_its_own_step`.)
- **FAILURES-2:** After 2,005 lines are written to one job's trace, the file has 2,000, its first the 6th written; dropping the oldest before any line is added always leaves room for it within the cap, drops no more lines than there are, and drops none under the cap. (`jobs::trace::tests::failures_2_the_trace_stays_under_its_cap`; Kani: `jobs::trace::proofs::failures_2_the_trace_never_passes_its_cap`.)
- **FAILURES-3:** A job that fails gets a record whose `evidence.log` isn't empty and is the last lines (at most 50) of its trace file, in order. (`failures_api::failures_3_a_failure_record_carries_the_trace`.)
- **FAILURES-4:** After `DELETE /v1/jobs/{id}` on an ended job, `home/jobs/<id>/` is gone and `GET /v1/jobs/{id}/trace` answers `404`. (`failures_api::failures_4_a_deleted_job_loses_its_trace`.)
