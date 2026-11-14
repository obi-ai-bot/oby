# The factory's proofs: an advisory `prove` stage after the gate

The business behavior contract for re-running a factory job's Kani proofs on the commit its gate passed, from [the template](../business-contract-template.md). Scenarios are numbered `PROVE-n`. The factory itself: [factory/README.md](../../factory/README.md); where the proofs run: [remote-gate.md](remote-gate.md), REMOTE-48.

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
