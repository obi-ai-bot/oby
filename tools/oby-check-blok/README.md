# oby-check-blok

The check (`scripts/oby-check.sh`) as a BLOK workflow, in shadow mode (#2046): the script still decides
publishing and merging; this tool only records what the workflow would have said, in
`~/.oby-flow/pipelines/runs.jsonl`, for the pipelines viewer.

- `oby-check-blok <checkout> [base]`: one shadow run, appended to `runs.jsonl`.
- `oby-check-blok --affected [base]` and `oby-check-blok mcp`: the agents' affected check (SKILL.md).

## The workflow file

What a run does is [`workflows/oby-check.json`](workflows/oby-check.json), a BLOK v2 workflow, so BLOK
Studio can show and edit it. A run reads, in order: `OBY_CHECK_WORKFLOW` (a path), else
`~/.oby-flow/workflows/oby-check.json` (where the pipelines editor saves), else the built-in file. Each run
records which one in `workflow_file`.

The script plans which groups a change reaches. Then each step whose node's group is planned runs, in the
file's order, honoring BLOK's step settings:

| Setting | What it does here |
|---|---|
| `use` | `runtime.rust:rust-affected-tests`, `runtime.rust:ts-checks` or `runtime.rust:secret-scan`, each once |
| `active: false` | skips the step; its group counts as not judged |
| `stop: true` | halts the workflow after the step; later planned steps count as not judged |
| `retry.maxAttempts` | tries again a node that couldn't judge (a transport error, a timeout), never a failure |
| `maxDuration` | `"45m"`, `"30s"` or milliseconds: past it, the node couldn't judge |

A planned group with no step, skipped or stopped is never a pass: the run folds to `unjudged`. A step this
host can't run (`branch`, `switch`, `forEach`, `tryCatch`, `wait`, `loop`, `subworkflow`, a node that isn't
one of the three) makes the whole run `unjudged`, with the reason, rather than running something else.
