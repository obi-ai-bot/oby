## Check workflow

This repository's check (`"check": "scripts/oby-check.sh"` in obid's `config.json`) is a BLOK workflow
run by `tools/oby-check-blok`, a standalone Apache-2.0 Cargo workspace outside obid (BLOK-1 to 3,
[publishing](contracts/publishing.md), "Check workflow" and PUBLISHING-26). obid runs the script as
it always did, in the check's box with the base's copy (PUBLISHING-6, 7); the script, run with none
of `OBY_CHECK_INNER`, `OBY_CHECK_PLAN` and `OBY_CHECK_ONLY`, execs the first `oby-check-blok` on the
check's `PATH`, by its resolved path: `oby-check-blok check [base]` (default `origin/dev`), from the
repository's root. Before it does, it is resolved through its links once, and that path is the one vetted and run: it and the nodes binary beside it (resolved too) must be regular files of the check's user or root that no other user may write, with no access list, in a folder of the check's user or root that no other user may write, with no access list either, and outside the checked checkout and the check's `TMPDIR`, which the checked code can write; a path, or one of those folders, with a newline in it is refused. What it doesn't check: the folders above the binary's folder (a home whose `~/.cargo` others may write could swap its `bin`), and a folder the owner deliberately put first on `PATH`, which the check trusts as theirs. Otherwise, or with none installed or one only on a
relative path, the check fails (`oby-check: failed: oby-check-blok (<why>)`, exit 1) and the script
runs nothing of its own.

Install it on each Mac that runs the check, from `dev`, and again whenever the tool changes:

```
cargo install --locked --path tools/oby-check-blok
```

That puts `oby-check-blok` and `oby-check-blok-nodes` side by side in `~/.cargo/bin`, which obid's
check finds there even when the login shell's `PATH` misses it (one of obid's usual folders; read-only
in its box). A pull request
that changes the tool is checked by the installed one until it's merged and installed again.

`check` runs only the built-in workflow (`tools/oby-check-blok/workflows/oby-check.json`), never
`OBY_CHECK_WORKFLOW` or the pipelines editor's saved copy, and writes no journal. Planning stays in
`scripts/oby-check.sh`: `OBY_CHECK_PLAN=1` with `OBY_CHECK_ONLY=rust|electron|swift|factory|scan`
selects its groups, and a plan says `oby-check: group <group>` once per group with work, the host's
only selection signal. Then five typed nodes, `rust-affected-tests`, `swift-tests`, `ts-checks`,
`factory-tests` and `secret-scan`, run the planned groups one at a time (to limit memory), each
through the script with `OBY_CHECK_ONLY` and `OBY_CHECK_INNER=1`; `OBY_CHECK_PATHS` and
`OBY_CHECK_ALL` reach them. The `factory` group runs `python3 -m unittest discover -s factory/tests`,
with no GitHub login, when a change touches `factory/bin/`, `factory/checks.sh`, `factory/tests/` or
`factory/config.json`, and in a full run (`OBY_CHECK_ALL`); it plans no Rust group. Each
node's output streams to stderr as it runs; a node's `couldn't judge:` line comes as `oby-check:
<group>: couldn't judge: …`. The run ends `oby-check: passed` (exit 0), `oby-check: failed: <nodes>`
on stderr (exit 1), or, with nothing failed and something not judged, a `couldn't judge: <why>` line
(exit 2). A failed node wins over one not judged, then passed; a node that couldn't start or answer
is not judged; nothing passes unless `secret-scan` passed.

`OBY_CHECK_ONLY=limits` is GitHub's: its `limits` job in `ci.yml` runs
`OBY_CHECK_ONLY=limits scripts/oby-check.sh <base>`, which judges only obid's line limits (the Rust
core's and `jobs.rs`'s lines, the tests' sleeps and the flaky list's guard) on the checked commit,
whatever changed, with no `oby-check-blok` (CI-35, [ci](contracts/ci.md)). It isn't a group
`oby-check-blok` plans or runs: its `rust` group judges the same limits when a change reaches them.

Each step's limit is its `maxDuration` in the workflow file, enforced by the node itself (CI-33,
[ci](contracts/ci.md)): past it, the node sends SIGTERM to the group's script and every process
descended from it (found by parent id, in the check's own process group), relays what the script says
for up to 30 seconds, sends SIGKILL to what's left, and answers `couldn't judge` with `ran past its
maxDuration of <N>s`, followed by the test binaries the script named as still running (`: tests/<name>.rs, …
were still running`). The host's own timeout is the limit plus 60 seconds. A full run (`check` with
`OBY_CHECK_ALL`) gives the Rust group (`rust-affected-tests`) the full-run limit, 60 minutes
(`FULL_RUN_RUST_LIMIT`, CI-34); every other step, and the Rust group of a gate's check or obid's,
keeps the workflow's (`rust-affected-tests` 45 minutes).

The host speaks BLOK's gRPC (the pinned BLOK Rust SDK's `NodeRuntime`) to `oby-check-blok-nodes`,
found beside it, over that child's stdin and stdout: no port and no socket, so it runs in a box
with no network at all, another Oby's job's included. It waits up to 60 seconds for their first
answer. The host fixes `OBY_CHECK_DIR`, `OBY_CHECK_BASE` and a random `OBY_CHECK_TOKEN` when it
starts them; every node request's inputs must carry that token, and a request naming another
checkout or base is refused without running a script.

The same tool keeps its other runs, which decide nothing about publishing or merging:
`oby-check-blok <checkout> [base]` runs a shadow of the check (with `OBY_CHECK_WORKFLOW`, else the
pipelines editor's `~/.oby-flow/workflows/oby-check.json`, else the built-in file) and prints and
appends a JSON record to `~/.oby-flow/pipelines/runs.jsonl` (commit, base, workflow `oby-check`,
nodes, folded verdict, start time; no raw output). Agents call the stdio MCP server
`oby-check-blok mcp` through its sole tool, `oby_check_affected` (optional string `base`, default
`origin/dev`), or run `oby-check-blok --affected [base]` from the repository root, instead of the
whole script in their box: both plan only what changed, force offline Cargo/npm dependency use, and
keep the script's nested-sandbox skips. Their JSON has `verdict` (`passed`, `failed`, `unjudged`),
`failing` test names, `reason` and per-node results; the CLI prints one JSON line to stdout and one
summary to stderr, exiting 0/1/2, and MCP returns that JSON as text. It's a tool for third-party
agents, not an obid agent loop, and adds no publishing, approval, merge or secret capability; its
instructions are `tools/oby-check-blok/SKILL.md`. Changes under `tools/oby-check-blok/` select its
fmt, clippy and tests in the Rust group, with its own manifest and a target directory under the
check cache. Another project adopts it as the tool's README says ("Adopt it").

### Mac check

On obid (2026-10-27), the Mac app's separate Mac checks are gone: `GET /v1/checks/next`, `GET /v1/checks/{id}/bundle`, `POST /v1/checks/{id}/result` and the agents' `oby-check apple` (`/v1/agent/apple-check`) answer `404`. A learned workflow's `checks: "mac"` no longer merges by itself: the job's own check runs boxed and skips the suites only the Mac runs unboxed (MacTests, `swift test`; scripts/oby-check.sh), and nothing else runs them on the pushed head, so once everything else holds the owner gets a card (`Not merged: the checks this workflow waits for on this Mac don't run by themselves any more, so it's yours to merge.`). A job with no check at all never merges by itself under such a workflow either (`Not merged: no check ran on its head (this repository has none), so it's yours to merge.`). A job's `macCheck`, `appleCheck` and `appleRan` stay in its JSON (null on new jobs) for the apps that decode them.

### Xcode Cloud

Xcode Cloud builds and tests every pull request into `dev` (the "Pull Requests" workflow), as GitHub check runs like `Test - iOS` (#122). GitHub Actions stays off. Each time the runner follows a published pull request (every 3 minutes, with the same login), it reads the pull request's head (`headRefOid`), then that commit's check runs (`repos/<repo>/commits/<sha>/check-runs`, every page). Only Xcode Cloud's runs on that commit count, and of each task only the newest run (a rerun replaces the one before it); Oby's own check runs aren't Xcode Cloud's. Each failed task's failure annotations (failing tests, compiler errors) are read too, and come first.

- `xcodeCloud`: null until the pull request has Xcode Cloud checks, otherwise `{head, state, details, conflict, checkedAt, rounds, handedOff, problem, stops}`. `state` is `pending`, `passed` (every task succeeded) or `failed` (one finished otherwise). `details` is each task's conclusion and what it reported, 16,000 characters at most. `conflict`: Xcode Cloud said “Your source code changes conflict with changes made on the pull request target branch”. `rounds` counts the fix rounds Oby handed to the author, on any head. `handedOff`: this head's failure reached the author. `problem`: why Oby couldn't hand it on, or null, and `stops` how many times that happened on this head (each brings its own inbox item). Once the pull request has Cloud checks, a new head is `pending` until its own appear.
- `xcodeCloudError`: null, or `Couldn't refresh Xcode Cloud. Will try again shortly.` The last result stays, and the merge waits until a read succeeds.
- On obid, `xcodeCloud` and `xcodeCloudError` are always null and `cloudNeedsYou` false: obid reads no Xcode Cloud checks and starts no round for them (the Swift runner's, deleted on 2026-11-02).
- The pipeline adds a `cloud` stage, **Xcode Cloud**, before Merged: `Pending` (`current`), `Passed` (`done`) or `Failed` (`failed`, `reason` the problem or what failed). Only a result on the pull request's head completes it.
- **The merge waits** for Xcode Cloud to pass on the commit the Mac checked, whenever the pull request has its checks: the Merged stage says `Mac check passed · waiting for Xcode Cloud`, with no merge error or inbox item, and the merge goes on when the follower sees it pass. Right before merging, the runner reads that commit's checks again, as the GitHub app; a failed read (`xcodeCloudError`, until the follower reads it again) or a result that isn't `passed` keeps it waiting, with no merge error and none of the merge's tries used. A pull request without Xcode Cloud checks merges as before.

