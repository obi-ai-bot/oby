# oby-check-blok

The check (`scripts/oby-check.sh`) as a BLOK workflow. Installed, it is the check: the script hands its
run to `oby-check-blok check` (BLOK-1, docs/contracts/publishing.md PUBLISHING-26). Its shadow runs
(#2046) still record what an edited workflow would have said, in `~/.oby-flow/pipelines/runs.jsonl`, for
the pipelines viewer.

- `oby-check-blok <checkout> [base]`: one shadow run, appended to `runs.jsonl`.
- `oby-check-blok --affected [base]` and `oby-check-blok mcp`: the agents' affected check (SKILL.md).
- `oby-check-blok validate <workflow.json>`: the rules a run applies, for an editor's draft (`{"valid": …}`).
- `oby-check-blok check [base]`: the real check, below.

The nodes run in a second binary, `oby-check-blok-nodes`, found beside `oby-check-blok`, which speaks
BLOK's gRPC to it over the child's stdin and stdout: no port or socket, so a run works in a box with no
network at all, loopback and Unix sockets included (another Oby's job's check, BLOK-3).

## Check mode

`oby-check-blok check [base]`, run from the repository root, is a drop-in for the script's whole run,
for the script to exec an installed copy (`cargo install --locked --path tools/oby-check-blok`, BLOK-1):

- It runs only the built-in workflow, never `OBY_CHECK_WORKFLOW` or the editor's saved copy (BLOK-2), and
  writes nothing to `runs.jsonl`.
- `OBY_CHECK_PATHS` and `OBY_CHECK_ALL` reach the script's planning and nodes (PUBLISHING-18's base run);
  the shadow and affected runs still plan only what changed.
- Each node's script output streams to stderr as it runs (obid stops a check silent for 15 minutes,
  PUBLISHING-16). A node's `couldn't judge:` line is relayed as `oby-check: <group>: couldn't judge: …`, so
  obid reads only the check's own.
- It ends as the script does: `oby-check: passed` and exit 0; else `oby-check: failed: <nodes>` on stderr
  and exit 1, or, when nothing failed but something couldn't be judged, a `couldn't judge: <why>` line
  first and exit 2. Nothing passes unless the `secret-scan` node passed (F-M2-35): a plan with no work, or
  none for the scan, is unjudged.
- Every script it runs gets `OBY_CHECK_INNER=1`, so the script never execs this tool again under it.
- It waits up to 60 seconds for the nodes binary to start and answer.

## Adopt it

Another project makes its check this workflow in four steps; obid needs no change.

1. Install the tool on each Mac that runs the check, from this repository, and again whenever it
   changes: `cargo install --locked --path tools/oby-check-blok`. Both binaries land side by side in
   `~/.cargo/bin`, which must be on the login shell's `PATH`.
2. Name the check `scripts/oby-check.sh` (the nodes run that path from the repository's root) and
   make it plan by groups: with `OBY_CHECK_PLAN=1` and `OBY_CHECK_ONLY=<group>` it runs nothing and
   prints `oby-check: group <group>` once when that group has work; with `OBY_CHECK_ONLY` alone it
   runs that group and exits 0 when it passed, else non-zero, and a line starting `couldn't judge: `
   when it couldn't judge. The groups are `rust`, `electron`, `swift` and `scan` today (the nodes
   `rust-affected-tests`, `ts-checks`, `swift-tests`, `secret-scan`), whatever each runs in that
   project. Nothing passes unless `scan` planned work and passed.
3. Put the same hand-off at the script's top, after its `cd` to the repository's root: this
   repository's `scripts/oby-check.sh`, from `resolved()` to its `exec`. It runs the first
   `oby-check-blok` on `PATH` only once it and the nodes binary beside it, each resolved through its
   links, are regular files of the check's user that nobody else may write, in folders nobody else
   may write, and outside the checked repository and `TMPDIR`; it then runs the resolved one. A
   binary the checked code planted never judges it, and with none that passes, the check fails
   rather than running anything else. It doesn't vet the folders above the binary's, nor a `PATH`
   whose owner put a folder of their own before `~/.cargo/bin`.
4. Point obid at it: `"check": "scripts/oby-check.sh"` on the repository in obid's `config.json`.

## The workflow file

What a run does is [`workflows/oby-check.json`](workflows/oby-check.json), a BLOK v2 workflow, so BLOK
Studio can show and edit it. A run reads, in order: `OBY_CHECK_WORKFLOW` (a path), else
`~/.oby-flow/workflows/oby-check.json` (where the pipelines editor saves), else the built-in file. Each run
records which one in `workflow_file`.

The script plans which groups a change reaches. Then each step whose node's group is planned runs, in the
file's order, honoring BLOK's step settings:

| Setting | What it does here |
|---|---|
| `use` | `runtime.rust:rust-affected-tests`, `runtime.rust:swift-tests`, `runtime.rust:ts-checks` or `runtime.rust:secret-scan`, each once |
| `active: false` | skips the step; its group counts as not judged |
| `stop: true` | halts the workflow after the step; later planned steps count as not judged |
| `retry.maxAttempts` | tries again a node that couldn't judge (a transport error, a timeout), never a failure |
| `maxDuration` | `"45m"`, `"30s"` or milliseconds: past it, the node couldn't judge |

A planned group with no step, skipped or stopped is never a pass: the run folds to `unjudged`. A step this
host can't run (`branch`, `switch`, `forEach`, `tryCatch`, `wait`, `loop`, `subworkflow`, a node that isn't
one of the four) makes the whole run `unjudged`, with the reason, rather than running something else.
