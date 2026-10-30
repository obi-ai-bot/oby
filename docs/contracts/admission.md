# Admission: when a queued task starts

The business behavior contract for when obid starts a queued job ([#1571](https://github.com/obi-ai-bot/oby/issues/1571)), from [the template](../business-contract-template.md). Scenarios are numbered `ADMISSION-n`; the tests that prove them are named after them (`core/crates/obid/tests/admission.rs`, `runner_api.rs`).

## Feature and status

- **Outcome:** The owner dispatches several tasks and they run at once, up to their slots, on any Mac, an 8 GB or 16 GB one included. A task that waits says what it waits for.
- **Status:** **Implemented.** The owner's decision of 2026-10-28 (#1589, "start every job as soon as it has a slot, never waiting for memory"): the Mac's memory never holds a job or a check back. It replaced obid's admission by memory (#1413), which on a 16 GB Mac let only one task run at a time (#1571's report). This contract records that behavior, adds the tests on 8 GB and 16 GB Macs, and lists what #1571 asked that #1589 decided otherwise (below).
- **Scope:** When a queued job starts, and what the owner sees while it waits for a slot. **Out of scope:** the wait for an account with room (#872, #1563), the wait while an account is signed in or removed, and how memory sizes builds (`CARGO_BUILD_JOBS` and friends, docs/runner-api.md, Boxed jobs).

## Behavior

### Actors and ownership

- **obid** decides when a job starts: as soon as fewer than `maxRunning` jobs run (6 unless the owner changes it in `PUT /v1/settings/jobs`, 1 to 20), oldest first. It never reads the Mac's memory or its pressure to decide; checks still run one at a time.
- **The app** shows obid's job as it is: a queued task counts among the running ones on Home (Sangam's attention line, "N running tasks"), its card shows obid's own `step`, and its dot and label say Queued (Sangam's `StatusDot`, `ChatView`). It decides nothing.

### States

| State | When | `status` | `waiting` | `step` |
|---|---|---|---|---|
| Runs | a slot is free and no older job waits | `running` | null | the job's own |
| Waits for a slot | `maxRunning` jobs run | `queued` | `{"kind": "cap", "text": "a slot opens"}` | `Waiting for a free slot: 6 of 6 jobs running` |
| Starts | a running job ends | `running` | null | the job's own |

### Invariants

- Memory never holds a job back: not available memory, not Warn or Critical pressure. A job is never stopped or paused for memory either.
- `status.memory.waiting` is always `[]`, and obid never sends `waiting` `{"kind": "cap", "text": "memory frees up"}` or `step` `Waiting for memory` any more. Both keys stay (the API is add-only); the Swift iPhone app keeps its "waiting for memory" line for a Mac on an older obid.
- Every running agent takes a slot; the rounds of a job that started already never wait for one (docs/runner-api.md, Jobs run side by side).

## Interfaces and dependencies

`GET /v1/jobs`, `GET /v1/jobs/{id}`, `GET /v1/status` (`running`, `maxRunning`, `memory.waiting`) and `PUT /v1/settings/jobs`, as docs/runner-api.md has them. Nothing new.

### What #1571 asked that #1589 decided otherwise

#1571 asked to keep a memory admission and loosen it: a pure admission module over the machine's memory and the jobs' loads, a check's budget charged only during its check stage, Warn pressure admitting work, and Sangam's "N waiting for memory" on Home to show it. #1589 removed the memory wait instead, so there is no memory decision to move, budget or show, and Home's memory line went with it. What is left to decide (a slot is free) is one comparison in `Jobs::schedule`, not a module of its own. Whether Home should count the tasks waiting for a slot apart, and in which words, is Sangam's to design; until he does, they count among the running ones, as his screen does today.

## Acceptance scenarios

- **ADMISSION-1:** On an 8 GB Mac with 1 GiB available, the owner dispatches three tasks: all three run at once, none says it waits, and `status.memory.waiting` is `[]`. (`admission_1_tasks_run_side_by_side_on_an_8_gb_mac`)
- **ADMISSION-2:** On a 16 GB Mac with 3 GiB available, the same: all three run at once. (`admission_2_tasks_run_side_by_side_on_a_16_gb_mac`)
- **ADMISSION-3:** With six tasks running (`maxRunning` 6), a seventh is queued with `waiting` `{"kind": "cap", "text": "a slot opens"}`; once one of the six stops, it starts and its `waiting` is null. (`admission_3_a_seventh_job_waits_for_a_slot_and_says_so`, runner_api.rs)

Test it when it's done, on a real 16 GB Mac: dispatch two tasks from Home while nothing else runs, and see both cards working at once and Home say "2 running tasks".
