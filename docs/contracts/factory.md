# The factory's proofs: an advisory `prove` stage after the gate

The business behavior contract for re-running a factory job's Kani proofs on the commit its gate passed, from [the template](../business-contract-template.md). Scenarios are numbered `PROVE-n`, and those of the advisory mutants step below `MUTANTS-n`. The factory itself: [factory/README.md](../../factory/README.md); where the proofs run: [remote-gate.md](remote-gate.md), REMOTE-48.

## Feature and status

- **Outcome:** a job whose task proves properties with Kani has those proofs re-run by the factory itself, on the exact commit its gate passed, on the gate VM, before its reviewers read it. The factory, not the builder's own word, says whether each harness ends `VERIFICATION:- SUCCESSFUL`, whether it would catch the bug it was written for (its broken rule turns it red), and whether the branch added anything that makes a proof pass without proving anything. The owner asked for it advisory first: a failed proof is shown to the reviewers and the owner, and the job goes on.
- **Status:** PROVE-1 to PROVE-12 are **implemented**: `factory/bin/prove` and the `prove` stage in `factory/bin/job`, tested by `factory/tests/test_prove.py` (task A00-37). Two steps are the owner's, because `factory/config.json` is protected: adding `"proofs": {"mode": "advisory", "timeout_s": 600}` (the code uses those defaults while the key is absent) and `"proofs": {"proofs": "opus"}` under `surface_reviewers`. Intake sets the `proofs` surface, the spec writer writes `## Proofs`, the `reviewer-proofs` lens reviews it, and the factory skill sends a `proofs` job through `prove` after the gate (task A00-38).
- **Scope:** the `## Proofs` format in a job's `spec.md`; the `prove` stage and its transitions; `factory/bin/prove`'s checks, verdicts, mode, settings and `round-N/proofs.md`; the dashboard showing a job in `prove`. **Out of scope:** `factory/config.json` (the owner's edit); `factory/bin/route`, `.claude/agents/*`, the factory skill and `AGENTS.md`'s pipeline line (A00-38); proofs run by `scripts/oby-check.sh`, `factory/checks.sh`, `factory/bin/gate` or `factory/bin/nightly`; blocking mode in use (it is accepted and tested, and nothing defaults to it); Quint or any tool but Kani; writing obid's harnesses (R04-03, R04-04); changes to `factory/bin/test-run` or `factory/bin/gate`; proof chips or a proofs column on the dashboard; catching `kani::any_where` or other narrowing of inputs beyond the constructs below, which is the proofs reviewer lens's judgement.

## Behavior

### The `## Proofs` format

A `## Proofs` section in a job's `spec.md` holds one or more entries, each one `### PROOF-<n>` heading and its key/value bullets:

```
### PROOF-<n>
- Property: <one sentence>
- Scenario: <AREA-n>
- Crate: `<crate>`
- Harness: `<harness fn name>`
- Assumes: none | `<expr>` `<expr>` ...
- Unwind: none | <positive integer>
- Broken file: `<path from the repository root>`
- Broken old: `<one existing line of that file>`
- Broken new: `<the line that replaces it>`
```

- Keys are matched case-insensitively, appear in any order, and all nine are required. Unknown keys are ignored.
- A backticked value is the text between the first and last backtick on its line.
- Crate matches `[A-Za-z0-9_-]+`; Harness matches `[A-Za-z_][A-Za-z0-9_]*`; Scenario is a scenario id such as `JOBSTATE-7`.
- Assumes is `none`, or one or more backticked expressions, each the exact argument of an allowed `kani::assume(...)` (compared with all whitespace removed).
- Unwind is `none`, or the one bound an allowed `kani::unwind(<n>)` may name.
- Broken old must match exactly one line of Broken file, compared with leading and trailing whitespace stripped on both sides. Broken new replaces that line, keeping its indentation, and may be empty.

### Actors and ownership

- **The spec writer** writes the `## Proofs` section. When the job leaves `spec`, `factory/bin/job` records the sha256 of that section (`proofs_hash` in job.json, `none` without one); a job.json without `proofs_hash` counts as `none`.
- **The builder** writes the harnesses, in `#[cfg(test)] mod proofs { #![cfg(kani)] … }` or a crate's `tests/` folder, and changes nothing in the section.
- **`factory/bin/prove <job-id>`** judges them and writes `round-N/proofs.md`. It reads job.json (never writing it or taking its lock), `spec.md`, `round-N/checks.md` and the `config.json` beside it, and loads `proofs_hash` from `factory/bin/job` beside it rather than copying it.
- **`factory/bin/job`** moves the job into and out of `prove` only on what `proofs.md` says.
- **The owner** decides the mode and timeout in `config.json`, and reads `proofs.md` with the reviews.

### States and transitions

- `gate` → `prove` needs what `gate` → `review` needs (checks.md for the worktree's HEAD, `GATE: PASS`) and is open to every job.
- `gate` → `review` is refused for a job whose intake surfaces include `proofs`, naming `factory/bin/prove <job-id>`; for every other job it is unchanged.
- Leaving `prove` needs `round-N/proofs.md` whose `COMMIT:` is round-N/checks.md's and the worktree's HEAD; otherwise it says to run `factory/bin/prove` again.
  - `prove` → `review` on `PROOFS: PASS` or `ADVISORY-FAIL`, or `SKIPPED` for a job without the `proofs` surface. Every reviewer starts pending, as after the gate.
  - `prove` → `build` on `PROOFS: FAIL` or `TIMEOUT`, starting a new round, or `prove` → `needs-human` instead on the last round, with decision.md saying `Proofs failed in round <n>, the last allowed round. See round-<n>/proofs.md.`
- `job sync` records the round's `PROOFS:` value in job.json's `proofs` (null without a proofs.md) and prints `proofs <value or ->` after the gate's; a new round starts with `proofs` null.
- The dashboard shows a job in `prove` in the Build column, as a live stage.

### What prove checks, in order

It stops at the first step that decides the verdict:

1. A job without `proofs` in its intake surfaces is `SKIPPED`: nothing else runs, and no scratch copy is made.
2. The section's hash differs from `proofs_hash`: FAIL, `properties changed after spec`.
3. The section is missing, has no entry, or an entry misses a key or has an invalid value: FAIL, naming the PROOF and the key.
4. The added lines of the branch's `*.rs` files (`git diff origin/<base>...<commit>`) hold any of these: FAIL, naming each `file:line`.
   - `kani::stub`, `stub_verified` included, or `kani::should_panic`;
   - a `should_panic` attribute among the attribute lines directly around a `#[kani::proof]`;
   - `cfg(` with the word `kani`, outside a crate's `tests/` folder and outside a module declared `#[cfg(test)]` (on its `mod` line or the attribute lines directly above it). Braces are counted without regard to strings or comments;
   - a `kani::assume(<arg>)` whose argument, up to its balanced closing parenthesis on the same line, isn't in any entry's Assumes, or that doesn't close on its line;
   - a `kani::unwind(<n>)` whose `n` isn't any entry's Unwind.
5. The harnesses, in one scratch worktree of the commit (`git worktree add --detach`), never the job's own worktree:
   - Each runs as `<prove's folder>/test-run -- cargo kani -p <crate> --tests --harness <harness>` from the scratch copy's `core/`, with `CARGO_TARGET_DIR=<scratch>/core/target`, so test-run sends it to a gate VM builder slot (REMOTE-48). Its output is appended to `round-N/proofs.log`.
   - Its result is `SUCCESSFUL` only on exit 0 with a `VERIFICATION:- SUCCESSFUL` line and no `VERIFICATION:- FAILED` line; else `TIMEOUT` when `timeout_s` passed; else `FAILED` on a `VERIFICATION:- FAILED` line; else `NO VERDICT (exit <code>)`. A run test-run kept on this Mac is judged the same way: without Kani there it has no verdict.
   - Only a `SUCCESSFUL` harness has its broken rule run: the file must be inside the scratch copy (a relative path, no `..`, its real path inside the copy), else `OUTSIDE THE REPOSITORY`; Broken old must match exactly one line, else `DID NOT APPLY`. The edit is applied, the same command runs, and the file is restored with `git checkout`. `RED` (a `VERIFICATION:- FAILED` line) is what is expected; `GREEN` is FAIL `vacuous`; `TIMEOUT` is TIMEOUT; anything else is FAIL.

### Verdict, mode and settings

- FAIL outranks TIMEOUT, which outranks PASS.
- The settings are `config.json`'s `proofs`: `mode`, `advisory` (the default) or `blocking`, and `timeout_s`, a positive integer (by default 600) for each run. Any other value stops prove with exit 2 naming the field, and no proofs.md.
- In `advisory` mode FAIL and TIMEOUT are written `PROOFS: ADVISORY-FAIL`, with `RESULT: FAIL` or `RESULT: TIMEOUT` keeping which, so the job goes on to review; prove never writes `PROOFS: FAIL` or `TIMEOUT` then. In `blocking` mode they stay as they are.
- Exit 0 on PASS, ADVISORY-FAIL and SKIPPED; 1 on FAIL and TIMEOUT; 2 when it can't judge: a usage or settings error, a job with no round, or a checks.md that is missing or not `GATE: PASS`.

### `round-N/proofs.md`

```
PROOFS: PASS | FAIL | TIMEOUT | ADVISORY-FAIL | SKIPPED
COMMIT: <checks.md's COMMIT>
MODE: advisory | blocking
RESULT: FAIL | TIMEOUT          (advisory failures only)

## Why                          (every reason; not on PASS or SKIPPED)

## PROOF-<n> <harness>          (one per entry run)
- Command: the exact command
- Result: SUCCESSFUL | FAILED | TIMEOUT | NO VERDICT (exit <code>)
- Verification seconds: Kani's `Verification Time: <x>s`, or -
- VM seconds: the wall-clock seconds of the test-run call, its build included
- Broken rule: RED | GREEN | TIMEOUT | NO VERDICT (…) | DID NOT APPLY … | OUTSIDE THE REPOSITORY … | not run: <why>
```

prove prints the first line, with the job id and round, on stdout.

### Failures and recovery

- A run past `timeout_s` gets SIGTERM by its test-run's PID, then SIGKILL 30 seconds later; test-run stops its own run's session, on the VM too.
- SIGTERM or SIGINT to prove sends SIGTERM to the test-run it started, by that PID only, removes the scratch copy (`git worktree remove --force`, then `git worktree prune`) and exits 128 + signal, leaving no proofs.md. The scratch copy is removed the same way however prove ends.
- prove deletes the round's earlier proofs.md before it judges, so a stopped run never leaves one behind for the stage to read.

### Invariants and boundaries

- The proofs are of the gated commit only: the scratch copy is that commit, and test-run sends nothing uncommitted from the job's worktree, which prove never modifies.
- A broken rule edits only a file inside the scratch copy, and the copy is clean again before the next rule.
- No proof result is a pass without Kani's own `VERIFICATION:- SUCCESSFUL`: no verification line, a test-run that fell back to a Mac without Kani, or a non-zero exit is FAIL.

## Interfaces and dependencies

- `factory/bin/test-run`, unchanged: it sends a command starting `cargo kani` to a gate VM builder slot (REMOTE-48), and stops its run on SIGTERM.
- Kani's output: a `VERIFICATION:- SUCCESSFUL` or `VERIFICATION:- FAILED` line per harness and a `Verification Time: <x>s` line.
- job.json gains `proofs_hash` (set when leaving `spec`) and `proofs` (set by sync); the dashboard reads the stage `prove`.

## Acceptance scenarios

- **PROVE-1** (implemented). Given a job without the `proofs` surface whose gate passed, when prove runs, then it exits 0 and writes proofs.md starting `PROOFS: SKIPPED` and `COMMIT: <sha>`, calls no test-run and registers no extra worktree. Tests: `test_prove_1_a_job_without_the_surface_is_skipped`.
- **PROVE-2** (implemented). Given a `proofs` job whose `## Proofs` changed after its hash was recorded, or a job.json with no `proofs_hash` and a spec with the section, then blocking mode writes `PROOFS: FAIL` with `properties changed after spec` and exits 1, and advisory mode (or no `proofs` key) writes `PROOFS: ADVISORY-FAIL` and `RESULT: FAIL` with the same reason and exits 0. Tests: `test_prove_2_changed_properties_fail_in_blocking_mode`, `test_prove_2_changed_properties_are_advisory_by_default`, `test_prove_2_a_job_with_no_recorded_hash_counts_as_none`.
- **PROVE-3** (implemented). Given a branch adding `kani::stub`, `#[should_panic]` on a `#[kani::proof]` fn, `#[cfg(kani)]` on a top-level item of a `src/` file, an unlisted `kani::assume` or an unlisted `#[kani::unwind(n)]`, then each is FAIL naming its `file:line`, with no test-run called; a listed assume or unwind, `#![cfg(kani)]` inside `#[cfg(test)] mod proofs {`, a file in a crate's `tests/` and `cfg(kani)` in a `Cargo.toml` pass the scan. Tests: `test_prove_3_each_forbidden_construct_is_caught`, `test_prove_3_listed_and_allowed_constructs_pass_the_scan`.
- **PROVE-4** (implemented). Given a listed harness that ends SUCCESSFUL and a broken rule that ends FAILED, then `PROOFS: PASS` and exit 0, its section showing the command `cargo kani -p <crate> --tests --harness <name>`, `Result: SUCCESSFUL`, the verification and VM seconds and `Broken rule: RED`; the job's worktree and its `git status` are unchanged, and no scratch copy is left. Tests: `test_prove_4_a_green_harness_with_a_red_broken_rule_passes`.
- **PROVE-5** (implemented). Given a harness that stays SUCCESSFUL with its broken rule, then FAIL (blocking) or ADVISORY-FAIL (advisory), with `vacuous` under `## Why` and `Broken rule: GREEN`. Tests: `test_prove_5_a_harness_green_on_its_broken_rule_is_vacuous`.
- **PROVE-6** (implemented). Given `timeout_s: 1` and a run that doesn't end, then blocking writes `PROOFS: TIMEOUT` and exits 1, and advisory writes `ADVISORY-FAIL` with `RESULT: TIMEOUT`; the run's process is gone and the scratch copy removed. Given SIGTERM to prove mid-run, then the run is stopped, the scratch copy removed, no proofs.md written and prove exits 143. Tests: `test_prove_6_a_run_past_its_timeout_is_timeout`, `test_prove_6_a_stop_ends_the_run_and_removes_the_scratch_copy`.
- **PROVE-7** (implemented). Given a run that prints no `VERIFICATION:-` line, one that fell back to this Mac with `no such command: kani`, or one that prints SUCCESSFUL but exits non-zero, then each is FAIL, never PASS. Tests: `test_prove_7_a_run_with_no_verdict_is_not_a_pass`.
- **PROVE-8** (implemented). Given a broken rule whose old line matches zero or two lines, or whose file is `../x.rs` or an absolute path, then each is FAIL, and nothing outside the scratch copy is modified. Tests: `test_prove_8_a_broken_rule_that_cant_apply_fails`.
- **PROVE-9** (implemented). Given a `proofs` job, then gate → review is refused naming `factory/bin/prove`; gate → prove succeeds; prove → review succeeds on PASS and ADVISORY-FAIL; prove → build on FAIL starts round 2; on the last round prove → build is refused and prove → needs-human writes decision.md; and leaving `prove` is refused with no proofs.md, or one whose COMMIT isn't checks.md's or HEAD. Tests: `test_prove_9_a_proofs_job_goes_through_prove`, `test_prove_9_an_advisory_fail_goes_on_to_review`, `test_prove_9_a_fail_goes_back_to_build_or_to_a_human_on_the_last_round`, `test_prove_9_proofs_md_must_be_for_the_gated_commit`.
- **PROVE-10** (implemented). Given a job without the surface, then gate → review succeeds as before, and gate → prove, prove (SKIPPED), prove → review succeeds too. Tests: `test_prove_10_a_job_without_the_surface_goes_gate_to_review_as_before`, `test_prove_10_a_job_without_the_surface_may_go_through_prove`.
- **PROVE-11** (implemented). Given a job leaving `spec`, then job.json records `proofs_hash`, the sha256 of the section or `none`; `job sync` stores the round's PROOFS value in `proofs` and prints `proofs <value>`; a new round resets `proofs` to null. Tests: `test_prove_11_leaving_spec_records_the_proofs_hash`, `test_prove_11_sync_records_the_rounds_proofs`, `test_prove_9_a_fail_goes_back_to_build_or_to_a_human_on_the_last_round`.
- **PROVE-12** (implemented). Given `"proofs": {"mode": "strict"}`, `"timeout_s": 0` or a timeout that isn't an integer, then prove exits 2 naming the field and writes no proofs.md; `"mode": "blocking"` is accepted; a job with no round, or no passing checks.md, exits 2 too. Tests: `test_prove_12_invalid_settings_stop_before_anything`, `test_prove_12_blocking_mode_is_accepted`, `test_prove_12_no_passing_gate_or_round_stops_before_anything`.

## Mutants: an advisory step after the gate

### Feature and status

- **Outcome:** every job whose branch changes Rust under `core/crates/` has its tests' strength measured on the commit its gate passed: cargo-mutants changes that code one small way at a time (`>` to `>=`, a body to a default value) and reruns the package's tests (for obid, the tests the check's own plan selects for the change), and a change no test notices is a missed mutant, a test the job could add. The code reviewer and the approver read the result. It is advisory only: it never moves a job, and blocking on it is the owner's word.
- **Status:** MUTANTS-1 to MUTANTS-16 are **implemented**: `factory/bin/mutants`, tested by `factory/tests/test_mutants.py`, the `cargo mutants` route in `factory/bin/test-run` and cargo-mutants' install in `factory/remote/setup.sh` (tasks A00-41, A00-43, A00-44 and A00-54). Two steps are the orchestrator's: adding `"mutants": {"budget_s": 1200}` to the protected `factory/config.json` (the code uses that default while the key is absent), and running the new `setup.sh` on the gate VM.
- **Scope:** `factory/bin/mutants`, its settings and `round-N/mutants.md`; test-run's route for `cargo mutants`; setup.sh's cargo-mutants pin and install; the factory skill, `reviewer-code` and the approver reading the file. **Out of scope:** any blocking mode, job stage or transition (`factory/bin/job`, `gate`, `route` and the dashboard are unchanged); `factory/config.json`; a nightly or whole-crate run; `.rs` files outside `core/crates/` (`tools/`); excluding `target_os = "macos"` code; fixing the gaps a run finds.

### Actors and ownership

- **`factory/bin/mutants <job-id>`** runs the step and writes `round-N/mutants.md` and `round-N/mutants.log`. It reads job.json (never writing it or taking its lock), `round-N/checks.md` and the `config.json` beside it, and never modifies the job's worktree. It loads `factory/bin/job` beside it for the config and the job's files, and takes `end` and `KILL_AFTER` from `factory/bin/prove` rather than copying them.
- **The factory skill** starts it in the background in the same step that spawns the round's reviewers: after a passing gate, or for a `proofs` job after prove sends it to review. Nothing waits for it, and the skill stops a run still going, by its PID, when the job leaves the round. Its result never moves the job.
- **`reviewer-code`** reads mutants.md if it is there when it looks, and otherwise notes "mutants still running"; it may raise a missed mutant in shipping code the branch changed as a finding: a test to add. **The approver** quotes the `MUTANTS:` line in decision.md, or writes `MUTANTS: not finished`, and never withholds APPROVE on it alone.
- **The orchestrator** adds the budget to `config.json` on the owner's word and runs `setup.sh` on the VM.

### What mutants runs

1. With `<mb>` = `git merge-base origin/<base> <commit>` in the job's worktree, the `.rs` files `git diff --name-only --no-renames --diff-filter=d <mb> <commit>` lists name the touched crates: each `core/crates/<dir>/`, its package the first `name = "<x>"` line of its `Cargo.toml` at the commit. With none, the step is `SKIPPED`: no test-run, no scratch copy.
2. Otherwise, each call below runs in a scratch worktree of the commit of its own (`git worktree add --detach`), never the job's own, in which it writes `core/mutants.diff` (untracked and not ignored, so test-run sends it): `git diff --relative=core --no-renames --no-color --no-ext-diff <mb> <commit> -- '*.rs'`, paths relative to `core/`, the Cargo workspace root cargo-mutants reads them against.
3. A call runs from the scratch copy's `core/`, with `CARGO_TARGET_DIR=<scratch>/core/target`. The touched packages but `obid` get one: `<mutants' folder>/test-run -- cargo mutants --in-place --in-diff mutants.diff -p <pkg> [-p <pkg> …] --no-times --caught --unviable --colors never`, the packages sorted, each with its full test set; a branch that touches no obid code gets only that one, and reads no plan. test-run sends it to a gate VM builder slot (MUTANTS-10). `--in-place`: without it cargo-mutants copies only the Cargo workspace (`core/`), and obid's tests read the repository's files above it (`scripts/`, `README.md`, `docs/`, `e2e/`, `git ls-files` at its root), so the unmutated baseline fails. The files are mutated where the call runs: on the gate VM, the builder slot's own worktree, which every run starts clean (REMOTE-34); on a fall-back to this Mac, the scratch worktree, which mutants removes however it ends. The job's own worktree is never touched. In place, cargo-mutants runs one build at a time by itself, and it refuses any `-j` (or `CARGO_MUTANTS_JOBS`) beside `--in-place`, `-j 1` included, so the command has none: the slot gives every build one `CARGO_TARGET_DIR`, and parallel cargo-mutants builds there would test each other's binaries; each build still uses the slot's share of the cores.
4. A touched `obid` gets a call of its own, after the other packages', narrowed to the tests the commit's own check plans for the change. In its scratch copy, before `core/mutants.diff` is written (so the plan sees only the commit's diff), mutants runs `scripts/oby-check.sh <mb>` from the scratch root with its own environment, less `OBY_CHECK_INNER`, `OBY_CHECK_PATHS` and `OBY_CHECK_ALL`, plus `OBY_CHECK_PLAN=1` and `OBY_CHECK_ONLY=rust`, for at most `PLAN_TIMEOUT` (120 seconds, in the code). Its stdout and stderr go to mutants.log under `==== OBY_CHECK_PLAN=1 OBY_CHECK_ONLY=rust scripts/oby-check.sh <mb> (exit <code>) ====`. Of its stdout, split on whitespace, a line starting `==> lib_tests ` with no `--` word means all of obid's lib tests, one with a `--` gives the lib filters (the words after the first `--`), and a line starting `==> test_binaries ` gives the test binaries (the word after each `--test`).
5. obid's call is `cargo mutants --in-place --in-diff mutants.diff -p obid --no-times --caught --unviable --colors never`, then `-C --lib` when the plan has lib filters, one `-C --test=<name>` per test binary in the plan's order, and `-- -- <filter> [<filter> …]` only when it has lib filters and no binaries. `-C` reaches every cargo call of the run, so both each mutant's build (`cargo test --no-run`) and its tests are narrowed, and a `--test=<obid binary>` would break another package's build: hence obid's call of its own. With both filters and binaries the filters are left out, so all of obid's lib tests run beside the binaries (`cargo test` applies a filter to every binary it runs): wider than the plan, never narrower.
6. `mutants.budget_s` covers the whole step: each call's limit is the time left until `budget_s` after the first call started, and obid's call doesn't start when the other packages' ran past it. obid's call must also print its baseline line (`^\S+\s+Unmutated baseline`) within half of `mutants.budget_s` (`BASELINE_SHARE = 0.5`, in the code: `max(1, int(budget_s * 0.5))` seconds of its start, and never later than the step's budget), or it is stopped as a call past its budget is. That limit is obid's call's only.
7. Each output line `^(MISSED|caught|unviable|TIMEOUT)\s+(\S+?):(\d+):(\d+): (.+)$` is one mutant: its outcome, its path relative to `core/`, its line, column and change. A result line matches `^\d+ mutants? tested` or holds `No mutants to filter`.
8. A mutant whose position, in its file at the commit, sits in code under `#[cfg(kani)]` or `#[cfg(test)]` is excluded, counted only in `EXCLUDED:`: a file whose first non-comment line is `#![cfg(kani)]` or `#![cfg(test)]`; inside the braces of an item whose attribute lines, directly above it or on its own line, hold one of those once whitespace is removed; or inside a block whose first line is `#![cfg(kani)]` or `#![cfg(test)]`. Braces are counted without regard to strings or comments.
9. A missed mutant's function is the name after the last ` in ` of its change, else the name after `replace ` and before ` -> ` or ` with `, by its last `::` segment with any `<…>` removed. It is Kani-covered when that name is called (`<name>(` or `<name>::<…>(`) in the body of a `#[kani::proof]` fn in any `.rs` file of the same crate at the commit. It still counts as missed, and is listed under its own heading.

### Verdicts and settings

Each call, and each obid skip, is one part, with its packages and its value:

- The result line present, the call within its budget and its exit 0, 2 or 3: `<c> caught, <m> missed, <t> timeout, <u> unviable`, counted after the exclusions.
- Past the budget: `TIMEOUT after <budget_s>s: <counts>`, counted from what was printed before the stop; for obid's call, only once its baseline line has shown.
- Anything else is `NO RESULT (exit <code>)`, never a count: a test-run that couldn't start, no result line (a fall-back to a Mac without cargo-mutants, `no such command: mutants`), a failed baseline (exit 4) or any other exit.
- obid is `SKIPPED`, with no test-run call for it, when the plan can't be read (`the check's plan couldn't be read (<exit N | ran past 120s | couldn't start: <error> | no "planned, nothing run" line>), so obid's tests can't be narrowed`: it couldn't start, ran past `PLAN_TIMEOUT`, exited other than 0 or printed no `oby-check: planned, nothing run` line); when it runs all of obid's lib tests (`the check's plan runs all of obid's lib tests (--lib --bins) for this change, so obid's tests can't be narrowed`); when it runs none of obid's tests (`the check's plan runs none of obid's tests for this change`); when the other packages' call spent the budget (`the budget was spent before obid's run`); and when its call's baseline line didn't show in time (`its narrowed baseline didn't finish within <n>s (half of mutants.budget_s), so no obid mutant was tested`), never `TIMEOUT … 0 caught, 0 missed`.

The first line combines the parts: `MUTANTS: SKIPPED` when every part is SKIPPED; else `MUTANTS: NO RESULT (exit <code>)`, the first part with no result's code, when any part has none; else `MUTANTS: TIMEOUT after <budget_s>s: <counts>` when any part is TIMEOUT; else `MUTANTS: <counts>`. The counts are summed over the parts that ran. `WHY:` is that of the part that set a SKIPPED or NO RESULT first line: the first SKIPPED part or the first NO RESULT part.

- The setting is `config.json`'s `mutants.budget_s`, a positive integer (by default 1200) for the whole step. A `mutants` that isn't an object, or a `budget_s` that isn't a positive integer, stops mutants with exit 2 naming the field, and no mutants.md.
- Exit 0 whenever it wrote mutants.md, whatever it says; 2 on a usage or settings error, a job that doesn't exist or has no round, or a checks.md that is missing or not `GATE: PASS`; 128 + signal when stopped.

### `round-N/mutants.md`

```
MUTANTS: <counts> | SKIPPED | TIMEOUT after <n>s: <counts> | NO RESULT (exit <code>)
COMMIT: <checks.md's COMMIT>
BUDGET: <budget_s>s
VM SECONDS: <wall-clock seconds of the test-run calls, summed, one decimal> | -
COMMAND: `<the exact test-run command>`[; `<the next call's>`] | -
EXCLUDED: <n> under #[cfg(kani)] or #[cfg(test)], summed over the calls
WHY: <why>                                (SKIPPED and NO RESULT only)

## Missed                                 (counts and TIMEOUT only)
- core/<path>:<line>: <the change> (no test notices)
(or "None.")

## Missed in functions a Kani proof calls (counts and TIMEOUT only)
- core/<path>:<line>: <the change> (no test notices; <function> is called by a #[kani::proof] harness)
(or "None.")

## Crates                                 (whenever a crate under core/crates/ is touched)
- <pkg> [<pkg> …]: <counts> | TIMEOUT after <n>s: <counts>
- <pkg> [<pkg> …]: SKIPPED: <why> | NO RESULT (exit <code>): <why>
```

`## Missed` and its Kani heading list what every call that ran found. `## Crates` has one line per part, in the order they ran. Each call's whole output is appended to `round-N/mutants.log` under a `==== <command> (<its part's value>, <s>s) ====` line, and mutants prints `MUTANTS: <value> (<job-id> round <N>)` on stdout.

### Failures and recovery

- mutants deletes the round's earlier mutants.md and any leftover `mutants.md.tmp` before anything else and starts mutants.log empty, so a stopped run never leaves an old one behind.
- The result is written to `mutants.md.tmp` and renamed into place, so a reader sees no mutants.md or the whole of it.
- A call past its budget, or obid's past its baseline limit, gets prove's `end()`: SIGTERM by its test-run's PID, then SIGKILL 30 seconds later; test-run stops its own run's session, on the VM too.
- SIGTERM or SIGINT to mutants ends the test-run it started, by that PID only, or the check's plan, by its process group, removes the scratch copy (`git worktree remove --force`, then `git worktree prune`) and exits 128 + signal, with no mutants.md. The scratch copy is removed the same way however mutants ends.
- On exit 2, the skill tells the user what it said and goes on without it.

### Invariants and limits

- Advisory: no verdict, count or timeout moves a job, writes job.json or takes its lock.
- An unfinished or missing run holds nothing: no reviewer, approver or merge waits for it.
- The run is of the gated commit only: the scratch copy is that commit, and test-run sends nothing uncommitted from the job's worktree.
- No run is a clean result without cargo-mutants' own result line: a fall-back to a Mac without it, a failed baseline or a missing line is NO RESULT.
- **Limits.** The Kani-covered rule matches names, not resolved paths: it misses a function a harness reaches only through another call, and can over-match a same-named function elsewhere in the crate. The run covers obid's Linux build, so a missed mutant in code only macOS compiles is noise. The run is budget-bound: a TIMEOUT counts only what was tested before it. obid's mutants run only the tests the check's plan selects, so one only a test outside that selection would catch counts as missed; and when the plan has both lib filters and test binaries, all of obid's lib tests run.

### Acceptance scenarios

- **MUTANTS-1** (implemented). Given a gated job whose branch changes only a `.md` file, only a `.rs` file outside `core/crates/`, or only deletes a `.rs` file, then mutants exits 0, mutants.md starts `MUTANTS: SKIPPED` and `COMMIT: <head>` with a `WHY:` line, no test-run is called and no worktree is registered. Tests: `test_mutants_1_a_branch_with_no_rust_change_under_core_crates_is_skipped`.
- **MUTANTS-2** (implemented). Given a branch changing two crates and a non-`.rs` file, then one call runs `test-run -- cargo mutants --in-place --in-diff mutants.diff -p <a> -p <b> --no-times --caught --unviable --colors never`, with no `-j` and no `CARGO_MUTANTS_JOBS`, from `<scratch>/core` with `CARGO_TARGET_DIR=<scratch>/core/target`, its diff's `+++ b/` paths start `crates/` and hold no non-`.rs` file; no scratch copy is left and the job's worktree is unchanged. Tests: `test_mutants_2_one_call_on_the_touched_crates_with_a_diff_relative_to_core`.
- **MUTANTS-3** (implemented). Given two caught, two missed, one timeout and one unviable mutant and a result line, exit 2, then mutants.md starts `MUTANTS: 2 caught, 2 missed, 1 timeout, 1 unviable` and `COMMIT:`, `BUDGET: 1200s`, `VM SECONDS:`, `COMMAND:` and `EXCLUDED: 0 …`, lists each missed mutant under `## Missed`, the log holds the output and mutants prints the first line and exits 0. Tests: `test_mutants_3_mutants_md_has_its_shape`.
- **MUTANTS-4** (implemented). Given mutants inside a `#[cfg(kani)] mod`, a `#[cfg(test)] mod { #![cfg(kani)] … }` and a file starting `#![cfg(kani)]`, and one in plain code, then only the plain one is listed and counted, and `EXCLUDED: 3`. Tests: `test_mutants_4_mutants_under_cfg_kani_or_cfg_test_are_excluded`.
- **MUTANTS-5** (implemented). Given a missed `replace > with >= in big` where a `#[kani::proof]` fn of the crate calls `big(`, then it is listed under `## Missed in functions a Kani proof calls` and still counted as missed; a missed mutant in a function no harness calls stays under `## Missed`. Tests: `test_mutants_5_a_missed_mutant_a_kani_proof_calls_is_listed_apart`.
- **MUTANTS-6** (implemented). Given `budget_s: 1` and a run that printed one caught and one missed mutant and doesn't end, then `MUTANTS: TIMEOUT after 1s: 1 caught, 1 missed, 0 timeout, 0 unviable`, the missed one listed, the run's process gone, no scratch copy left, the job's worktree unchanged and exit 0. Given SIGTERM to mutants mid-run, then exit 143, no mutants.md, the run's process gone, no scratch copy left and the job's worktree unchanged. Tests: `test_mutants_6_a_run_past_its_budget_is_timeout_with_what_it_tested`, `test_mutants_6_a_stop_ends_the_run_and_removes_the_scratch_copy`.
- **MUTANTS-7** (implemented). Given a run that fell back to this Mac with `no such command: mutants` (exit 101), one that prints mutants but no result line, or a failed baseline (exit 4), then each writes `MUTANTS: NO RESULT (exit <code>)` with a `WHY:`, never a count, and exits 0. Tests: `test_mutants_7_a_run_with_no_result_is_never_a_count`.
- **MUTANTS-8** (implemented). Given `"budget_s": 0`, `"budget_s": "20"` or `"mutants": 5`, a job with round 0, a missing job or a usage error, or a checks.md that is `GATE: FAIL` or missing, then mutants exits 2 naming what is wrong, with no mutants.md and no test-run call; with no `mutants` key the budget is 1200s. Tests: `test_mutants_8_invalid_settings_stop_before_anything`, `test_mutants_8_no_passing_gate_or_round_stops_before_anything`, `test_mutants_8_the_budget_is_1200s_by_default`.
- **MUTANTS-9** (implemented). Given missed, TIMEOUT, NO RESULT and SKIPPED runs, then job.json is byte for byte unchanged and no `.lock` file appears in the job's folder. Tests: `test_mutants_9_it_never_writes_job_json_or_takes_its_lock`.
- **MUTANTS-10** (implemented). Given a worktree touching macOS-only code, then `test-run -- cargo mutants …` goes to a gate VM builder slot, never the Mac worker, and `sh -c "cargo mutants"` stays on this Mac, as for `cargo kani` (REMOTE-48). Tests: `test_mutants_10_a_mutants_run_with_macos_only_changes_goes_to_the_vm`, `test_mutants_10_a_mutants_run_after_another_command_stays_on_this_mac`, `test_mutants_10_a_mutants_run_goes_to_the_vm_not_the_mac_worker`.
- **MUTANTS-11** (implemented). Given setup.sh's host script, then it holds `MUTANTS=27.1.0`; root installs `cargo install --locked cargo-mutants --version 27.1.0` into the builder's `.cargo` after its toolchain and before the tightening, unless `cargo install --list` holds `cargo-mutants v27.1.0:`; no `oby-builder` call installs it; one builder call checks `cargo mutants --version` is `cargo-mutants 27.1.0` with a Cargo home of its own, and setup stops before `is set up` when it isn't; `--pins` prints no cargo-mutants line. Tests: `test_mutants_11_setup_installs_cargo_mutants_for_the_builder_as_root`, `test_mutants_11_setup_stops_when_cargo_mutants_fails_for_the_builder`, `test_setup_pins_are_ci_yml_s`.
- **MUTANTS-12** (implemented). Given a run that writes mutants.md, then a reader at any moment sees either no mutants.md or the whole of it, and a `mutants.md.tmp` left from a stopped run is gone after the next run. Tests: `test_mutants_12_mutants_md_appears_whole_by_a_rename`.
- **MUTANTS-13** (implemented). Given a branch touching obid and a plan with `--lib -- merge::` and no binaries, one with those filters and `--test a --test b`, or one with binaries only, then exactly one test-run call runs `cargo mutants --in-place --in-diff mutants.diff -p obid --no-times --caught --unviable --colors never` followed by `-C --lib -- -- merge::`, by `-C --lib -C --test=a -C --test=b` with no `--`, or by `-C --test=<name>` with no `-C --lib`; the check ran from the scratch root with argv `[<mb>]`, `OBY_CHECK_PLAN=1` and `OBY_CHECK_ONLY=rust` and no other `OBY_CHECK_*`, before `core/mutants.diff` existed, and its output is in mutants.log. Tests: `test_mutants_13_obid_s_call_is_narrowed_to_the_check_s_plan`.
- **MUTANTS-14** (implemented). Given a plan that exits 1, one with no `planned, nothing run` line, a `--lib --bins` plan or one with no obid lines, then mutants exits 0, mutants.md starts `MUTANTS: SKIPPED` with the reason's `WHY:` and `## Crates` holds `- obid: SKIPPED: <why>`, no test-run is called and no worktree is left registered. Given SIGTERM to mutants while the plan runs, then exit 143, no mutants.md, the plan's process gone and no scratch copy left. Tests: `test_mutants_14_obid_is_skipped_when_the_plan_can_t_narrow_it`, `test_mutants_14_a_stop_while_the_plan_runs_leaves_no_plan_or_scratch_copy`.
- **MUTANTS-15** (implemented). Given `budget_s: 2` and an obid call that prints no baseline line and doesn't end, then mutants.md starts `MUTANTS: SKIPPED`, its WHY holds `narrowed baseline didn't finish within 1s`, the first line has no `TIMEOUT` and the call's process is gone. Given one that prints `ok       Unmutated baseline` and one caught mutant and doesn't end, then `MUTANTS: TIMEOUT after 2s: 1 caught, 0 missed, 0 timeout, 0 unviable`. Tests: `test_mutants_15_a_narrowed_baseline_that_doesn_t_show_in_time_is_skipped`, `test_mutants_15_past_the_baseline_a_call_past_the_budget_is_timeout`.
- **MUTANTS-16** (implemented). Given a branch touching `demo` and `obid` with a narrowed plan, then `… -p demo …` runs first, as today, and obid's narrowed call after; the first line sums both calls' counts, `COMMAND:` holds both joined by `; ` and `## Crates` has `demo`'s line then `obid`'s. With obid's plan skipped, only demo's call runs, the first line is its counts with no `WHY:`, and `## Crates` holds `- obid: SKIPPED: <why>`. With demo's call exiting 4, the first line is `MUTANTS: NO RESULT (exit 4)` though obid's has counts. With demo's call past the budget, obid's doesn't start and is `SKIPPED: the budget was spent before obid's run`. Tests: `test_mutants_16_other_crates_run_first_and_obid_after_with_one_first_line`, `test_mutants_16_a_skipped_obid_leaves_the_other_crates_result`, `test_mutants_16_no_result_in_any_part_sets_the_first_line`, `test_mutants_16_obid_doesn_t_start_once_the_other_crates_spent_the_budget`.
