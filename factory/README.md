# The software factory

`/factory run` in Claude Code, from the repository root, builds the roadmap's tasks one pull request each:
intake → spec → build → gate → review → approve. How to run it and what each piece does:
`.claude/skills/factory/SKILL.md`. The roadmap and its 180 tasks: `queue/README.md`.

```
config.json     concurrency, rounds, the active sprint, routing, protected paths, prices
checks.sh       the gate's checks: scripts/oby-check.sh on the job's worktree
bin/job         the only writer of jobs/<id>/job.json; reads the queue; refuses illegal moves; queues approved PRs
bin/gate        runs checks.sh in a job's worktree → round-N/checks.md; with `remote_gate` set, the Linux groups on
                the gate VM and the macOS-only part on the Mac, falling back to the Mac; with `mac_worker` set too, that
                macOS part on the Mac worker (the second Mac) when it is free, else on this Mac; `gate --idle`
                deallocates the idle VM (job queue runs it)
bin/route       risk + surfaces → the job's reviewers and models
bin/test-run    a builder's or approver's own test, clippy or build run at nice 15, half the cores; only runs that can
                build Rust or Xcode code take turns on its own lock, other runs start at once; it refuses a run whose
                CARGO_TARGET_DIR is the shared Cargo folder: they build in <worktree>/core/target; with
                `builder_slots` set, a heavy run that doesn't need macOS goes to a builder slot on the gate VM
bin/check-lock  the gate's lock holder for a check that builds Rust: one check at a time on the shared Cargo folder;
                inside the lock it gives the worktree's tracked core/ and tools/ files the current time, so the
                worktree's own code is built; it stops the check's leftovers and passes its exit code through
bin/child_session.py  runs a command in a session of its own and stops its leftovers; shared by check-lock, test-run
                and nightly
bin/nightly     the nightly full Mac check: checks.sh with OBY_CHECK_ALL=1 on origin/dev, when no gate waits →
                jobs/nightly/<stamp>.md, and a fix task on a failure
remote/         the gate VM's and the Mac worker's side: setup.sh provisions the gate VM over SSH, setup-mac.sh the Mac worker; tighten.sh is what each runs last on its host, to leave the check's path writable only by its owner; oby-remote-gate runs a commit's Linux check groups in one of four slots on the VM (or `slots`), with --run a builder's command in one of the builder slots, as the no-sudo user oby-builder, and with --mac the macOS groups it is given in one of two slots on the Mac worker
tests/          the factory's own tests, stdlib only, from the repository root: python3 -m unittest discover -s factory/tests;
                the check runs them when bin/, checks.sh, tests/ or config.json change
queue/tasks/    open tasks, one file each (R01-02.md)
queue/done/     finished tasks: a task's own pull request moves its file here
dashboard.html  the board: cd factory && python3 -m http.server, then http://localhost:8000/dashboard.html
jobs/           runtime state, local to this Mac (gitignored); jobs/remote-gate/state.json is the gate VM's uses
```

`jobs/nightly/` holds the nightly check's results, one `<stamp>.md` (`GATE: PASS|FAIL`, the commit, when it
started, the log's last 80 lines) and its `<stamp>.log` per run, named by the UTC start time; the dashboard
shows the newest.

Builders', code reviewers' and approvers' own test, clippy and build runs go through `bin/test-run`, so they don't
slow a gate past the check's caps, and build in the worktree's own folder:
`CARGO_TARGET_DIR=<worktree>/core/target bin/test-run -- cargo test -p obid --test <x>`. test-run refuses a run
whose `CARGO_TARGET_DIR` is the shared Cargo folder (`~/Library/Caches/oby-factory/core-target`), where another
worktree's build of a workspace crate can run in place of this one's: only the gate's check builds there, under
`bin/check-lock`. Only runs that can build Rust or Xcode code (a command naming `cargo`, `cargo-nextest`, `nextest`, `rustc`,
`xcodebuild`, `oby-check.sh`, `oby-check-blok`, or `swift` with `build` or `test`) take turns, one at a time on
`~/Library/Caches/oby-factory/test-run.lock`, never the gate's lock, so a gate never waits on them; the holder
line records its pid, when it took the lock, its folder and its command, and a run still waiting says who holds
it every 5 minutes. Other runs (`python3 -m unittest`, `npm test`, `npx vitest`) start at once. Every run is at
`nice -n 15`, so a gate's check gets the CPU first when both want it, and not `taskpolicy -b`, which also
throttles disk I/O and keeps the run on the efficiency cores even when no gate runs; with `CARGO_BUILD_JOBS`,
`RUST_TEST_THREADS` and `NEXTEST_TEST_THREADS` at half the cores. Commands that read none of those (`npm test`,
`npm run typecheck`, `swift test`) are held only by the priority (and `swift test` by the lock), and an explicit
`-j` or `--test-threads` overrides them.

Gates run on the Mac until the owner adds `remote_gate` to `config.json` (`docs/factory-remote-gate.md`, "The
owner's steps"). With it, `bin/gate` reads the check's plan, starts the gate VM `oby-gate` when it is stopped
(`az vm start`), runs the Linux groups there with `ssh oby-gate oby-remote-gate <commit> origin/dev`, and runs
swift, and rust when the diff touches a macOS-only path, on the Mac. It runs the whole check on the Mac when the
VM can't be reached in 3 minutes, couldn't judge, or the commit isn't on origin; before that, when SSH doesn't
answer, it moves the VM's SSH rule to this Mac's current public IP (`/32`) if it changed and tries once more.
`FACTORY_REMOTE_GATE` (the same JSON) turns it on for one run. Each use of the VM is recorded in
`jobs/remote-gate/state.json`, and `bin/job queue` runs `bin/gate --idle`, which deallocates the VM once no use is
open and the last one ended 30 minutes ago, whatever stage any job is in.
With `mac_worker` (`{"ssh_host": "oby-mac-pro"}`, or `FACTORY_MAC_WORKER` for one run) also set, the Mac's part
runs on that second Mac, set up by `remote/setup-mac.sh`, once the VM's part passed: in one call, `ssh
oby-mac-pro oby-worker/bin/oby-remote-gate --mac <commit> origin/dev <group>...`. When it doesn't answer within
60 seconds, both its slots are busy, it couldn't judge or was cut off, that part runs on this Mac as before.

Builders' heavy runs go to the gate VM too once the owner has run `remote/setup.sh` again (it adds the no-sudo
user `oby-builder` and its `/srv/oby-builder`) and set `builder_slots` in `remote_gate` (`"slots": 4,
"builder_slots": 8` on the 64-core VM; `slots`, by default 4, sizes the gates' slots). `bin/test-run` keeps
the run here, saying `on this Mac: <why>`, when it is a Swift, Xcode or whole-check run, when the worktree's
changes touch macOS-only code by the gate's own rule, or when its `--test` is a macOS-only one; otherwise it
sends the base's SHA and the binary diff to the worktree's current tree (nothing ignored, scanned by gitleaks
first) with `ssh -l oby-builder oby-gate oby-remote-gate --run …`, and the VM runs the same command line in a
builder slot of its own, at nice 15, offline, with its own Cargo folder. Its output and exit code come back;
anything that stops it before it starts there (the VM unreachable, no builder user, every builder slot busy
for 10 minutes) says `fell back to this Mac: <why>` and runs here as before. Its use of the VM is recorded in the
main checkout's `jobs/remote-gate/state.json` like a gate's.

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
