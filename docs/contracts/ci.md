# CI: what this repository's check holds

The business behavior contract for `scripts/oby-check.sh`'s own rules, from [the template](../business-contract-template.md). Roadmap milestone R01 (Safety net); each task adds its own scenarios here.

## Feature and status

- **Outcome:** A test that waits on elapsed time fails when the Mac is busy, and a flaky test blocks merges for everyone (#1804, #1656). obid's tests may only lose their `thread::sleep` and `time::sleep` calls, so each one a later change removes stays removed and none comes back.
- **Status:** CI-12 and CI-13 are **implemented** (task R01-06).
- **Scope:** The count of sleep calls in obid's tests and its limit. Out of scope: converting the sleeps there are today (later changes lower the limit), a clippy `disallowed-methods` rule, any per-file allowance.

## Behavior

### Actors and ownership

- **The check** (`scripts/oby-check.sh`) counts and judges, in its `rust` group: the block that judges the Rust core's and `jobs.rs`'s line limits, so it runs wherever they run, one group at a time under the installed `oby-check-blok` too. No model call and nothing at runtime is involved.

### Rules

- **What counts.** Each occurrence of `thread::sleep` or `time::sleep` (the pattern `(thread|time)::sleep`, written once in `tests_sleeps`) in a `*.rs` file under `core/crates/obid/tests`, however many share a line. No exemptions: `support::within`'s own 100 ms poll counts like any other. A missing folder counts 0.
- **The limit.** `tests_sleep_limit` in `scripts/oby-check.sh`. It may only come down.
- **Shrink-only, as the line limits.** A job's check runs the base's copy of the script on the commit (PUBLISHING-7). `pick_limit` holds a branch that lowered `tests_sleep_limit` since its merge base to its own number, but never to one higher than the running copy's; one that didn't gets the running copy's. When dev's limit changed since the branch started, the messages add "rebase onto dev".
- **Past the limit** fails (CI-12). **Under the limit** fails too (CI-13), asking for the exact new count. **At the limit** passes, and says so in one line.

### Failures and recovery

- Past the limit: replace the new sleep with a wait for state (`support::within`, or `events_named` on `follow_events`).
- Under the limit: lower `tests_sleep_limit` to the count the check names, in the same change.

### Invariants

- A test waits for state, never for time: a new sleep fails the check.
- Raising `tests_sleep_limit` is a change to the check, a security path: it merges only after two reviews by different tools or the owner's click (MERGING-13 to 16).

## Interfaces and dependencies

- **Check:** `scripts/oby-check.sh` (`tests_sleep_limit`, `tests_sleeps`), its `rust` group. Failure name: `obid's tests' sleep limit`.
- **Tests:** `scripts/oby-check-test.sh`, on the commit's own copy of the script (the base's copy a job's check installs may predate the limit).

## Acceptance scenarios

- **CI-12** (implemented). Given a change that adds a sleep call under `core/crates/obid/tests`, when the check runs, then it fails naming `obid's tests' sleep limit`, says the count and the limit, and says to wait for state with `support::within`; at the limit it passes and says so. Tests: the cases labelled `CI-12` in `scripts/oby-check-test.sh` (at the limit is within it; one sleep past the limit says to wait with support::within; a run with one sleep past the limit fails naming it).
- **CI-13** (implemented). Given a change that removes sleep calls, when the check runs, then it fails, asking to lower `tests_sleep_limit` from the limit to the new count, so the cut stays cut; a commit that lowers the limit is held to it against the base's copy. Tests: the cases labelled `CI-13` in `scripts/oby-check-test.sh` (a removed sleep asks for the limit to come down; a run with one sleep under the limit fails naming it; a commit's lower sleep limit holds against the base's copy).
