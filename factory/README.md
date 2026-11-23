# The software factory

`/factory run` in Claude Code, from the repository root, builds the roadmap's tasks one pull request each:
intake → spec → build → gate → prove → review → approve, where only a job whose intake names the `proofs` (Kani)
or `model` (Quint) surface must pass through `prove`, and every other job may still go from gate straight to review.
How to run it and what each piece does: `.claude/skills/factory/SKILL.md`. The roadmap and its 180 tasks: `queue/README.md`.
The builder works by pstack's method, its own review and verification included, and the factory shows each round
and keeps the owner's decision points: `pstack.md`.

```
config.json     concurrency, rounds, the active sprint, routing, protected paths, prices
checks.sh       the gate's checks: scripts/oby-check.sh on the job's worktree
bin/job         the only writer of jobs/<id>/job.json; reads the queue; refuses illegal moves; queues approved PRs
bin/gate        runs checks.sh in a job's worktree → round-N/checks.md; with `remote_gate` set, the Linux groups on
                the gate VM and the macOS-only part on the Mac, falling back to the Mac; with `mac_worker` set too, that
                macOS part on the Mac worker (the second Mac) when it is free, else on this Mac; with `gate_shards`
                set too, obid's test binaries in shards on the VM's slots, the Mac worker and this Mac, all at
                once, with one verdict; `gate --idle` deallocates the idle VM (job queue runs it); `gate --checkout
                <worktree> <commit> <branch> <out-dir>` is the same gate with no job folder (oby-feed's)
bin/gate-ci     the gate from GitHub's CI: waits for GitHub Actions' `Check` (and each `--also <name>`) at the pull
                request's exact head and writes round-N/checks.md from its conclusion and gate's protected paths,
                checks.log on FAIL; it runs no check itself; exit 0 PASS, 1 FAIL, 2 couldn't judge
bin/oby-feed    feeds the task files to the factory's own obid, each as intake, then spec, then build (Oby builds
                against the spec and reviews it), and merges what Oby approved twice, once the models, Kani and
                bin/gate passed on the head; a task intake holds for the owner waits `blocked`; state in
                ~/.oby-factory/feed/
bin/prove       re-runs the Kani harnesses a job's spec.md lists under `## Proofs` on the commit its gate passed, through
                bin/test-run, checks each can fail on its declared broken rule and that the branch adds nothing that
                games them; for a `model` job, runs `quint typecheck`, `run` and `verify` on each Quint model its
                `## Models` lists, each invariant holding and each witness violated → round-N/proofs.md; advisory by
                default (docs/contracts/factory.md, PROVE)
bin/mutants     runs cargo-mutants on the core/crates/ Rust a job's gated commit changes, through bin/test-run on a gate
                VM builder slot, within a time budget → round-N/mutants.md, the missed mutants for reviewer-code and the
                approver; advisory only, it never moves a job (docs/contracts/factory.md, MUTANTS)
bin/route       the job's reviews: pstack's, which the builder writes, and with --lenses the lens reviewers and models
                risk + surfaces give; bin/job adds Codex's when the round starts, for a job that needs two
bin/review-codex  Codex's review of a job's gated head: it copies the head's text files as git stores them
                (ls-tree and cat-file, no .gitattributes applied, no .git, its .codex* and .agents/ withheld), the
                job's spec, intake, issue, diff and log, the exact prompt and the AGENTS.md Codex reads from
                CODEX_HOME by itself into a folder of its own, refuses a head whose changed files aren't all in it,
                scans it all with gitleaks (the main checkout's config, nothing of the head's: no .gitleaksignore,
                .gitleaks.toml or gitleaks:allow), and points Codex at it in its read-only sandbox, the prompt read
                back from the scanned copy, on a private CODEX_HOME (0700, removed after) holding only the scanned
                AGENTS.md and a login copy whose refresh token is emptied, with none of the owner's Codex config,
                rules or MCP servers, no web search or skill list, none of Codex's own apps, plugins, browser,
                computer use, hooks, skill installs or memories, and an environment of PATH, HOME, CODEX_HOME and
                LANG; with codex-review.md →
                round-N/review-codex.md, which it records in job.json under the job's lock once the worktree and the
                copy are unchanged (only the recorded file counts); exit 0 PASS, 1 CHANGES, 2 couldn't judge. Known
                limit, the owner's to accept: the read-only sandbox can still read any file on this Mac (no Codex
                setting limits reads, the same reach as every Claude Code agent here); only the copy is scanned, and
                codex-review.md tells Codex only the copy is in scope
pstack.md       how the factory works by pstack; pstack/ holds the pinned pstack files the agents read
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
remote/         the gate VM's and the Mac worker's side: setup.sh provisions the gate VM over SSH, setup-mac.sh the Mac worker; pins.sh holds the cargo-mutants pin both setup scripts read; tighten.sh is what each runs last on its host, to leave the check's path writable only by its owner; oby-remote-gate runs a commit's Linux check groups in one of four slots on the VM (or `slots`), with --run a builder's command in one of the builder slots, as the no-sudo user oby-builder, and with --mac the macOS groups it is given in one of two slots on the Mac worker
tests/          the factory's own tests, stdlib only, from the repository root: python3 -m unittest discover -s factory/tests;
                the check runs them when bin/, checks.sh, tests/ or config.json change
queue/tasks/    open tasks, one file each (R01-02.md)
queue/done/     finished tasks: a task's own pull request moves its file here
dashboard.html  the board: cd factory && python3 -m http.server, then http://localhost:8000/dashboard.html
jobs/           runtime state, local to this Mac (gitignored); jobs/remote-gate/state.json is the gate VM's uses
```

`bin/oby-feed` runs the same gate before Oby's own factory merges (docs/contracts/factory.md, FEEDGATE): once a
pull request's head is approved twice by two tools and its models and Kani passed, or it waits for the owner on a
security path, it fetches the head into the main checkout, adds a detached worktree of it, and calls `bin/gate`'s
`check_commit()`, imported. So it reads `remote_gate`, `mac_worker`, `gate_shards`, `protected_paths` and
`base_branch` from this `config.json` (and `FACTORY_REMOTE_GATE`, `FACTORY_MAC_WORKER` and `FACTORY_GATE_SHARDS` for
one run), runs the main checkout's `checks.sh`, and keeps each head's `checks.md` and `checks.log` in
`~/.oby-factory/feed/gate/<KEY>/<head>/`, the verdict in the task's state, so a head is gated once. Green arms the
merge. Red holds the head, never merged, and when a part of the check failed sends the build job one fix message
with the failing parts and the log's end, fenced as data, so Oby follows up on the same branch. A gate that couldn't
judge (a part didn't report, or the gate raised) holds the head with a note and no message. A security-path pull
request still waits for the owner, the gate's result added to its note.

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

When the gate's hosts are gone, the orchestrator runs `bin/gate-ci <job-id>` in place of `bin/gate`: GitHub
Actions already runs the required `Check` on every head pushed to a pull request, drafts included. gate-ci refuses
(exit 2, no checks.md) a worktree with uncommitted changes, an `origin/<branch>` or a pull request head other than
the worktree's HEAD; then it reads the head's check runs every 30 seconds until the newest `Check` from the GitHub
Actions app, and the newest run of each `--also <name>` (`--also app-mac`), is completed, at most `--timeout`
seconds (5400 by default). PASS needs every one `success` and no protected path modified or deleted (`bin/gate`'s
own rule, imported); `skipped`, `neutral`, `cancelled` and the rest are FAIL. checks.md carries `GATE`, `COMMIT`,
`HOST: github-actions`, `PR` and `RUNS` (the Actions run ids), so `bin/job advance` and `bin/job merge` read it as
the gate's; on FAIL checks.log holds each failed run's last 300 lines of `gh run view <run> --log-failed`, fenced
as quoted data from GitHub. Exit 0 is PASS, 1 FAIL, and 2 couldn't judge: a refusal above, or a run that didn't
appear or finish in time, or a `gh` that can't run, fails, hangs or prints something that isn't JSON, none of which
writes a checks.md (no `Check` at all usually means CI didn't start for that head: push again); a failed poll is
tried again until the deadline. It never calls az, ssh, checks.sh, cargo or npm.

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
With `gate_shards` (`{"linux": 4, "mac": 2}`, or `FACTORY_GATE_SHARDS` for one run) also set, after the owner ran
`remote/setup.sh` and `remote/setup-mac.sh` again, one gate deals obid's test binaries into shards
(`OBY_CHECK_SHARD`, decided by `scripts/oby-check.sh` alone) and starts every part at once: `linux` calls of
`oby-remote-gate --shard k/<linux>` on the VM, and macOS rust shards on the Mac worker (`--mac --shard`) and this
Mac, without waiting for the VM's verdict. A shard that couldn't judge or whose host is busy or unreachable runs on
this Mac; one that ran nowhere fails the gate. checks.md adds `Sharded: …` and `## Time`. Without the key, nothing
changes.

Builders' heavy runs go to the gate VM too once the owner has run `remote/setup.sh` again (it adds the no-sudo
user `oby-builder` and its `/srv/oby-builder`) and set `builder_slots` in `remote_gate` (`"slots": 4,
"builder_slots": 8` on the 64-core VM; `slots`, by default 4, sizes the gates' slots). `bin/test-run` keeps
the run here, saying `on this Mac: <why>`, when it is a Swift, Xcode or whole-check run, when the worktree's
changes touch macOS-only code by the gate's own rule, or when its `--test` is a macOS-only one; otherwise it
sends the base's SHA and the binary diff to the worktree's current tree as git stores it (nothing ignored; `git
--no-replace-objects diff --no-ext-diff --no-textconv --no-color`, so no external diff, textconv, colour setting or
replace ref changes it; gitleaks, run from the main checkout, first reads the same change as text, whatever
`.gitattributes` say) with `ssh -l oby-builder oby-gate oby-remote-gate --run …`, and the VM runs the same command
line in a builder slot of its own, at nice 15, offline, with its own Cargo folder. Its output and exit code come
back; anything that stops it before it starts there (the VM unreachable, no builder user, every builder slot busy
for 10 minutes) says `fell back to this Mac: <why>` and runs here as before. Its use of the VM is recorded in the
main checkout's `jobs/remote-gate/state.json` like a gate's.
With `builder_slots` in `mac_worker` too (after `remote/setup-mac.sh` again), a run that stays here only because it
needs macOS (macOS-only code, or a macOS-only `--test`) goes to the Mac worker instead, saying `to the Mac worker
<host>: <why>`: `ssh oby-mac-pro oby-worker/bin/oby-remote-gate --mac --run …` runs it in a builder slot of its
own under `~/oby-worker/builders`, never a gate's, with the same output, exit code and fallback to this Mac. A run
that needs this Mac itself (its Keychain, the owner's real home) says so with `bin/test-run --here -- <command>`:
it stays here under the lock and calls no host.

What differs from the reference factory: the queue is the task files here, not GitHub issues (the owner's
decision of 2026-11-12). A task runs only in the active sprint (`sprint` in `config.json`) and after the tasks
its `DEPENDS` line names are in `queue/done`. Intake's questions and the owner's answers stay in the job's
folder (`/factory <answer>`), and nothing reads or writes a GitHub issue or the project board. A pull request
stays a draft until the approver says APPROVE; then GitHub merges it, and the job waits in the stage `queued`
while GitHub's merge queue holds it. The owner can reject a job at any stage but `queued` with `bin/job reject`,
which closes its PR and moves it to Done; the run stops the job's agents first.

Reading a job takes no lock (`bin/job` writes `job.json` whole, by renaming a temporary file), so `job queue`,
the slot count and a long-running importer such as `bin/oby-feed` never hold one. Every write of `job.json` holds
that job's lock: a command that changes a job (`create`, `advance`, `merge`, `answer`, `feedback`, `reject`,
`agents`, `sync`, and `queue` for a queued or merged job) holds it while it reads, decides and saves, and lets it
go when it ends, a refusal included; a save outside the lock is refused. When merging `dev` into a job's branch brings
`queue/tasks/<KEY>.md` back next to its `queue/done/<KEY>.md`, `job advance <job> gate` closes the task again in a
commit of its own, with `dev`'s text of the task. It pushes that, and no tag, only when origin already had the
branch at its head, and only once gitleaks, as `bin/test-run` runs it, passed the commit's diff read as text
(`--no-replace-objects diff --text --no-ext-diff --no-textconv --no-color`, whatever `.gitattributes`, an external
diff, a textconv, a colour setting or a replace ref say): a finding, a scanner that can't run, or a git that can't
write that diff refuses the advance and pushes nothing. An unpushed branch keeps the commit and is refused as
unpushed, for the builder to push. A branch with no `done/` copy, or one whose `tasks/` file differs from `dev`'s,
is refused as before.

Oby's factory (`bin/oby-feed`, building on the factory's own obid) starts each task as the Claude Code factory
does: an intake job first, then a spec job, then the build, told to build against that spec (FEEDSPEC in
`docs/contracts/factory.md`). Intake and spec are read-only `research` jobs following `.claude/agents/intake.md` and
`spec-writer.md` as `dev` has them; their documents reach the next job as fenced message parts. NEEDS-INFO, REJECT
or a step that gave nothing usable leaves the task `blocked`, holding no slot, until its task file changes. Both
factories refuse to spec an intake with no valid `LANE:` line, or an ACCEPT in the `agent` lane (`job advance <job>
spec`, and the feeder before its spec job; NOTHARNESS in `docs/contracts/factory.md`). A build that failed, was
interrupted, is gone or published nothing, on a task with no pull request yet, is built again against the same spec,
its brief giving obid's reason, up to three attempts; then the task fails with that reason, every attempt's job kept
(FEEDLOOP in `docs/contracts/factory.md`).

It has lens reviews and an approver step too. When a head is approved twice by different tools and passes the
security-path, model and Kani checks and the gate, it goes first to its lens reviews: one read-only review job on obid
per lens `bin/route`'s rule gives (its `reviewers()`, imported) for the task intake's `RISK:` line (or, for a task the
feeder has no intake for, the task file's `RISK:` line on `dev`, blank read as `high`) and for the intake's `SURFACES:`
together with every surface the pull request's files touch, so the diff can add a lens but never lose one. Each job
runs with its lens's model and follows `.claude/agents/reviewer-<lens>.md` as `dev` has it. A `VERDICT: CHANGES` holds
the head for the owner, with the lens's findings in `oby-feed --status`; no verdict, or a lens that can't be routed,
holds it too. Only when every lens says `VERDICT: PASS` for that same head does the feeder start an approver job on obid
for that head: a read-only review that follows `.claude/agents/approver.md` as `dev` has it. Only its
`APPROVE` for that same head arms the merge. `ESCALATE` holds the head for the owner, with the approver's reasons
in `oby-feed --status`. No verdict (any other first line, a job that failed or is gone) holds the head too, and a
head that moved while the approver read it needs two fresh approvals and a fresh `APPROVE`. Before it arms on an
`APPROVE`, the feeder checks the approvals, security paths, models, Kani proofs and the gate's verdict again. The
approver still checks the task file's own acceptance list, not the spec's, as `dev` has it, never the pull request's copy;
a task whose file isn't on `dev` is held for the owner. Their contracts are FEEDLENS and FEEDAPPROVE in
`docs/contracts/factory.md`.

A lens's `VERDICT: CHANGES` also sends the build job one fix message, its fix round, in fixed words: the lenses, the
head and their prompts on `dev`, never the findings, so the build reviews its own branch as those lenses would. That
message and the gate's fix message are the feeder's follow-ups, each with its reason (`the code review asked for
changes`, `the gate failed`). Once the last three follow-ups in a row went out for the same reason (`oby-feed
--same-reason N` changes the three), the next failure for it sends nothing: the task waits, its head held, and its note
says once why and what would start it again, as `oby-feed --status` does. A failure for another reason is still sent,
and `oby-feed --again KEY`, the owner's word, sends the one message the stop held back at the feeder's next pass. Its
contract is FEEDSTOP in `docs/contracts/factory.md`.

## From AGENTS.md

From the root [AGENTS.md](../AGENTS.md)'s "Software factory".

`/factory run` (Claude Code, from the repository root) builds the open tasks of the active sprint in `factory/queue/tasks/`, each through intake → spec → build → gate → review → approve, one pull request per task into `dev`. A task runs once the tasks its `DEPENDS` line names are in `factory/queue/done/`; its own pull request moves its file there. State is in `factory/jobs/` (gitignored); `factory/bin/job` is its only writer. The gate's checks are `factory/checks.sh`, which runs `scripts/oby-check.sh` in the job's worktree with one shared Cargo build folder (`~/Library/Caches/oby-factory/core-target`). Dashboard: `cd factory && python3 -m http.server`, then open http://localhost:8000/dashboard.html. How it runs: `.claude/skills/factory/SKILL.md`; what it is and its licence: `factory/README.md`.

- A fresh checkout needs `oby-check-blok` installed once on the Mac ("Build and run") and `gitleaks`; for the Electron app, `cd apps/electron && npm install`. To see a screen on a given port: [apps/electron/AGENTS.md](../apps/electron/AGENTS.md#build-and-run).
- Each job works in its own git worktree, a full checkout at `.worktrees/<job-id>` (gitignored), on a branch `factory/<job-id>`. A tool at the repository root that doesn't read `.gitignore` sees those copies too: exclude `.worktrees/` in its settings.

## Licence

The factory is adapted from ZazenCodes' Software Factory for Claude Code v2
(<https://github.com/zazencodes/zazencodes-season-3/tree/main/src/software-factory-claude-code-v2>), which is
GPLv3: `LICENSE` here is its licence. It covers the files that come from it: `bin/`, `dashboard.html`,
`config.json`, `checks.sh`, `.claude/skills/factory/` and the eight agents in `.claude/agents/`. They are
developer tooling that runs beside Oby and are no part of the app, obid or anything Oby ships; the rest of
this repository stays Apache-2.0. The task files in `queue/` are Oby's own.
