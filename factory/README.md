# The software factory

`/factory run` in Claude Code, from the repository root, builds the roadmap's tasks one pull request each:
intake → spec → build → gate → review → approve. How to run it and what each piece does:
`.claude/skills/factory/SKILL.md`. The roadmap and its 180 tasks: `queue/README.md`.

```
config.json     concurrency, rounds, the active sprint, routing, protected paths, prices
checks.sh       the gate's checks: scripts/oby-check.sh on the job's worktree
bin/job         the only writer of jobs/<id>/job.json; reads the queue; refuses illegal moves; queues approved PRs
bin/gate        runs checks.sh in a job's worktree → round-N/checks.md
bin/route       risk + surfaces → the job's reviewers and models
bin/test-run    a builder's own test, clippy or build run: one at a time on its own lock, at nice 15, half the cores
bin/check-lock  the gate's lock holder for a check that builds Rust: one check at a time on the shared Cargo folder;
                it stops the check's leftovers and passes its exit code through
bin/child_session.py  runs a command in a session of its own and stops its leftovers; shared by check-lock, test-run
                and nightly
bin/nightly     the nightly full Mac check: checks.sh with OBY_CHECK_ALL=1 on origin/dev, when no gate waits →
                jobs/nightly/<stamp>.md, and a fix task on a failure
remote/         the gate VM's side: setup.sh provisions the gate VM over SSH; oby-remote-gate runs a commit's Linux check groups in one of four slots there
tests/          the factory's own tests, stdlib only, from the repository root: python3 -m unittest discover -s factory/tests;
                the check runs them when bin/, checks.sh, tests/ or config.json change
queue/tasks/    open tasks, one file each (R01-02.md)
queue/done/     finished tasks: a task's own pull request moves its file here
dashboard.html  the board: cd factory && python3 -m http.server, then http://localhost:8000/dashboard.html
jobs/           runtime state, local to this Mac (gitignored)
```

`jobs/nightly/` holds the nightly check's results, one `<stamp>.md` (`GATE: PASS|FAIL`, the commit, when it
started, the log's last 80 lines) and its `<stamp>.log` per run, named by the UTC start time; the dashboard
shows the newest.

Builders' own test, clippy and build runs go through `bin/test-run`, so they don't slow a gate past the check's
caps: one at a time on `~/Library/Caches/oby-factory/test-run.lock`, never the gate's lock, so a gate never
waits on them; at `nice -n 15`, so a gate's check gets the CPU first when both want it, and not
`taskpolicy -b`, which also throttles disk I/O and keeps the run on the efficiency cores even when no gate runs;
with `CARGO_BUILD_JOBS`, `RUST_TEST_THREADS` and `NEXTEST_TEST_THREADS` at half the cores. Commands that read
none of those (`npm test`, `npm run typecheck`, `swift test`) are held only by the lock and the priority, and an
explicit `-j` or `--test-threads` overrides them.

Gates still run on the Mac: `bin/gate` doesn't use `remote/` yet, which a second task wires in
(`docs/factory-remote-gate.md`, "Still to build"). The gate VM `oby-gate` is only started by hand
(`az vm start`), and deallocated by hand when its work is done.

What differs from the reference factory: the queue is the task files here, not GitHub issues (the owner's
decision of 2026-11-12). A task runs only in the active sprint (`sprint` in `config.json`) and after the tasks
its `DEPENDS` line names are in `queue/done`. Intake's questions and the owner's answers stay in the job's
folder (`/factory <answer>`), and nothing reads or writes a GitHub issue or the project board. A pull request
stays a draft until the approver says APPROVE; then GitHub merges it, and the job waits in the stage `queued`
while GitHub's merge queue holds it.

## Licence

The factory is adapted from ZazenCodes' Software Factory for Claude Code v2
(<https://github.com/zazencodes/zazencodes-season-3/tree/main/src/software-factory-claude-code-v2>), which is
GPLv3: `LICENSE` here is its licence. It covers the files that come from it: `bin/`, `dashboard.html`,
`config.json`, `checks.sh`, `.claude/skills/factory/` and the eight agents in `.claude/agents/`. They are
developer tooling that runs beside Oby and are no part of the app, obid or anything Oby ships; the rest of
this repository stays Apache-2.0. The task files in `queue/` are Oby's own.
