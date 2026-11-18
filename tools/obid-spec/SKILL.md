---
name: obid-spec
description: Prove an obid endpoint or background trigger black-box with obid-spec's BLOK workflows, against the real obid binary and a local Sphere.
---

Build both binaries first: `cargo build --manifest-path tools/obid-spec/Cargo.toml --bins`.

- `tools/obid-spec/target/debug/obid-spec run --only <area>` runs an area's workflows (or
  `--workflow <text>` for one); it builds obid once and prints a pass/fail table, exit 0 only when
  no step failed. Expected reds (a known bug's issue) are listed and don't fail the run.
- `tools/obid-spec/target/debug/obid-spec coverage [--only <area>]` prints routes, triggers and
  obid's scenarios proven, exit 1 when anything is missing; `--gate`, the check's, fails only on
  what must hold now and reports the unproven scenarios as the backlog.

The check runs it: `scripts/oby-check.sh` for a change to a route or to the suite, and GitHub's
`obid-spec` job in the merge queue and nightly. A step that turns red there is a regression or a
new bug: fix the product, or file the bug and mark the step `ui.expectedRed`; loosen a step only when the step itself was wrong.

When you change a route or a trigger, add or change its workflow under
`tools/obid-spec/workflows/` (README.md, "Adding to it"), name each asserting step after the
scenario it proves (`ui.proves`), and classify the contract's scenarios in `scope.json`.

A workflow that finds a bug: don't fix product code from the workflow's change. File the issue
(What / Why / How, nothing personal) and mark the step `ui.expectedRed` with its number.

It starts scratch obids on stand-in agents only, on this Mac, and stops each by PID. Never point it
at the owner's obid or their logins. Keep `--jobs` at 6 or less: the Mac is shared.
