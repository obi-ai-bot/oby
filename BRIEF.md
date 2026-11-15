# The factory's builder, as an Oby job

You build one factory job inside the Oby job you are running in. Your worktree is
already on the job's branch, `factory/900-a00-66-oby-builds`, cut from `origin/dev`. obid
publishes your commits when you finish: it pushes the branch and opens or updates
the pull request. You never push, never open a pull request and never merge.

## What to read

The factory keeps this job's files on the branch `factory-spec/900-a00-66-oby-builds` of `origin`. Run
`git fetch origin factory-spec/900-a00-66-oby-builds`, then read each file with
`git show FETCH_HEAD:<name>`. Never check that branch out and never merge it.

- `BRIEF.md`: this file.
- `SPEC.md`: what to build. Its acceptance criteria are your test plan.
- `TASK.md`: the roadmap task `A00-66`, as it was written. It is background;
  the spec is what you build.
- `ROUND.md`, from round 2 on: why you are building again. It holds the gate's
  verdict and its failing log, the reviews that asked for changes, or the
  owner's feedback. Address every point in it.

Also read `AGENTS.md` in your worktree and follow it, its "Rules Oby never breaks"
above all.

## Build

- **The simplest change that fully meets the spec.** Match the surrounding
  code's idioms, naming, imports and comment density. Add no fallbacks and no
  extras the spec doesn't ask for.
- **Format only what you changed.** Run the formatters and linters the repo
  configures, on your changed lines only. Never reformat a whole file, and never
  run a `--fix` across the repo.
- **Never raise the line limits.** `obid_lines_limit` and `jobs_rs_limit` in
  `scripts/oby-check.sh` are the owner's to raise. If the change can't fit, end
  BLOCKED and say by how many lines.
- **Never touch a protected file.** Don't modify or delete a file matching
  `protected_paths` in `factory/config.json`. If the spec can't be met without
  changing one, end BLOCKED and say which file and why.
- **Electron setup.** For the Electron app, run `cd apps/electron && npm install`
  first.

## Test

- **Never run the whole check.** The factory's gate runs `factory/checks.sh`
  after you finish. Run only the tests your change touches, plus clippy on the
  crates you changed.
- **Rust:** use your worktree's own build folder: `CARGO_TARGET_DIR=$PWD/core/target`
  in front of every Cargo command run from the worktree root, and
  `--manifest-path core/Cargo.toml`.
- **Electron:** `npm run typecheck`, `npm test` and `npm run build` in
  `apps/electron`, when you touched it.
- **Proofs:** when `SPEC.md` has a Proofs section, follow it: one harness per
  `PROOF-<n>`, its `Broken old` line kept once in the code, and each run's
  `VERIFICATION:-` line reported. Never edit a property to make it pass.
- **Every acceptance criterion gets evidence:** the command you ran and its
  result.

## Commit

- **Conventional Commits,** in the style of `git log --oneline`
  (`feat(obid): …`), naming the scenario ids each commit implements. One commit
  per distinct piece of work. The first commit's subject becomes the pull
  request's title, so make it the task's own title:
  `feat(factory): the build step can run as an Oby job (factory/bin/oby-build)`.
- **Close the task in its own last commit.** If
  `factory/queue/tasks/A00-66.md` is in your worktree, run
  `git mv factory/queue/tasks/A00-66.md factory/queue/done/A00-66.md` and leave
  the file unchanged. Otherwise, write `TASK.md`'s exact contents to
  `factory/queue/done/A00-66.md` and commit that. The factory refuses a build
  that doesn't close its task.
- **Stage explicit paths,** never `git add -A`, and leave `git status --porcelain`
  empty.

## Your last message

It becomes the factory's `build.md`, so its first line must be exactly one of:

- `STATUS: READY`
- `STATUS: BLOCKED: <the reason, in one line>`

After that first line:
- 3 to 5 bullets on what the branch now does and what this round changed.
- Each acceptance criterion with its evidence.
- Anything you couldn't verify, said plainly.
