# Released files: a job file and config.json from each release still load

The business behavior contract for reading what a released obid wrote, from [the template](../business-contract-template.md). Roadmap task R01-04 (milestone R01, Safety net).

## Feature and status

- **Outcome:** Someone who updates Oby keeps every job they had and the configuration they had. `Store::load_all` (`core/crates/obid/src/store.rs`) skips a job file that doesn't read, with one warning in the log, and the job is gone from `GET /v1/jobs` with no other sign: a field renamed or made required in `Job` would do that to every job an earlier release saved. These scenarios fail first.
- **Status:** RELEASED-1 to 3 are **implemented**: tests that start today's obid on the files of each covered release. Nothing in obid changed.
- **Scope:** Tags v0.4.1, v0.4.2, v0.4.3, v0.4.4 and v0.4.5, each with three job files (done, failed, waiting), a `config.json` and the accounts' `usage.json` that keeps the waiting job waiting, in `core/crates/obid/tests/fixtures/released/<tag>/`. Out of scope: the home's schema version and an inbox item for a job that doesn't read (#923); tags before v0.4.1; other stored files (conversations, routines, the inbox, accounts' state, permissions) as things to prove; any API change.

## Behavior

### Actors and ownership

- **obid** reads the job files in `home/jobs/` and its `config.json` when it starts. No model call is involved.
- **The apps** see the jobs through `GET /v1/jobs`, and the repositories and accounts through `GET /v1/repos` and `GET /v1/status`.

### Rules

- **Every job stays.** A job file an earlier release wrote is listed by `GET /v1/jobs` after an update, with the status it was saved in.
- **A done job keeps what it made.** Its `branch`, its pull request's number (`pr`) and URL (`published`) are those it was saved with.
- **The configuration still starts obid.** A released `config.json` starts today's obid with every repository it configured and every account of every agent it configured. A top-level key a later obid retired is ignored with a warning, never a reason not to start.
- **Each release brings its files.** Each release tag adds its own folder in the release's pull request (docs/ci.md, "Releases").

### Failures and recovery

- A job file that doesn't read is the failure these scenarios exist for: the test names the tag and the job file, and obid's log says why (`doesn't read: missing field …`). The fix is in obid, never in the released file.

### Invariants

- The released files are never edited to make a test pass: they are what that release wrote.
- They hold made-up values only: no token, login, person's path or real repository.

## Interfaces and dependencies

- **Persistence:** `home/jobs/<id>.json`, `home/usage.json` and `config.json`, as each tag's `job.rs`, `usage.rs` and `config.rs` define them.
- **API:** `GET /v1/jobs`, `GET /v1/repos`, `GET /v1/status` (`accounts`), unchanged.
- **Check:** `scripts/oby-check.sh` runs `tests/released_files.rs` when `src/job.rs`, `src/config.rs` or `src/store.rs` change, or the fixtures do.

## Acceptance scenarios

- **RELEASED-1** (implemented). Given each covered release's three job files in `home/jobs/`, when today's obid starts on them and has looked at its waiting jobs again, then `GET /v1/jobs` lists every one by id with the status its file had (`done`, `failed`, `queued`); a missing one fails naming the tag and the file. Test: `released_1_every_released_job_is_listed` (core/crates/obid/tests/released_files.rs).
- **RELEASED-2** (implemented). Given each covered release's done job, when today's obid lists it, then its `branch`, `pr` and `published` equal its file's. Test: `released_2_branch_and_pr_are_kept` (core/crates/obid/tests/released_files.rs).
- **RELEASED-3** (implemented). Given each covered release's `config.json` with only its machine-specific keys made the test's, when today's obid starts on it, then it starts, `GET /v1/repos` lists each repository it configured and `GET /v1/status` lists each configured agent's accounts by id. Test: `released_3_released_config_starts` (core/crates/obid/tests/released_files.rs).
