# Working in obid

This file adds to the root [AGENTS.md](../../../AGENTS.md) and [core/AGENTS.md](../../AGENTS.md) and never overrides them. It says where obid's code goes, how its tests wait and when a temporary path ends.

## Ownership

Every kind of obid state has one owning module, listed in [docs/ARCHITECTURE.md, "State ownership"](../../../docs/ARCHITECTURE.md#6-state-ownership). A new field or rule goes in the module that owns its row there. Each module under `src/jobs/` owns one concern and says which in its `//!` header; `jobs.rs` holds what has not been moved out yet.

## Where new code goes

- A new concern gets its own `src/jobs/<concern>.rs`, declared from `jobs.rs`. Its `//!` header says what state it owns, which contract scenarios it serves, and what it never writes.
- One type per file. A type that takes several inputs takes an options struct, as `codex::Options` does, and its constructor only assigns fields: no I/O, no spawning, no deciding.
- The shape to copy is `src/jobs/retry.rs`: a header naming its scenarios, its state and what it never writes, an `impl Jobs` block, and its own tests module.

## `jobs.rs` only shrinks

A change may move code out of `jobs.rs`, never grow it. Its limit is `jobs_rs_limit` in [scripts/oby-check.sh](../../../scripts/oby-check.sh), and how that limit is raised is in the root [AGENTS.md](../../../AGENTS.md), "Dispatcher, not harness".

The first piece to move out is `run_owner_job`'s round loop, its rounds of agent and check, into a module of its own under `src/jobs/`.

## How tests wait

A test waits for state, never for time: it polls with `support::within` and the other helpers in [tests/support/mod.rs](tests/support/mod.rs). The rule on `tests_sleep_limit` is in the root [AGENTS.md](../../../AGENTS.md), "Dispatcher, not harness".

## When a temporary path ends

Every fallback, compatibility path or "until X" branch says in its comment the condition that removes it: a task key, an issue, or a state anyone can observe. It also has a test that fails if the path breaks while it still exists. When the condition holds, the path and its test are removed together, in the same change.
