# obid-spec

obid's endpoints and background triggers, proven black-box against the real obid binary, as BLOK
workflows and Rust nodes ([docs/contracts/obid-spec.md](../../docs/contracts/obid-spec.md),
OBIDSPEC-1 to 15). Like the Brain's map, but executable: each route in
[docs/api/routes.txt](../../docs/api/routes.txt) has a workflow, each step that asserts names the
contract scenario it proves, and every assertion is shown able to fail on every run. The same
workflows drive the real Electron app and the Mac around it ("The real app and the Mac", below).

```bash
cargo build --manifest-path tools/obid-spec/Cargo.toml --bins
tools/obid-spec/target/debug/obid-spec run --only sphere
tools/obid-spec/target/debug/obid-spec coverage
```

- `obid-spec run [--only <area>] [--workflow <text>] [--jobs <n>] [--obid <path>] [--keep]` builds
  obid once (`cargo build -p obid --bin obid` in `core/`, `CARGO_INCREMENTAL=0`) unless `--obid`
  or `OBID_SPEC_OBID` names one, starts the local Sphere when a workflow needs it, runs the
  workflows in parallel (at most 6), prints a pass/fail table and exits 1 on any failure. A failed
  workflow's folder (its obids' homes and logs, `report.json` with every step's output) is kept and
  named; `--keep` keeps them all.
- `obid-spec run --tag launch` runs only the workflows tagged `launch` (`"tags": ["launch"]`):
  the Obysphere's and the launch flows' (OBIDSPEC-12), a quick run of what launches.
- `obid-spec coverage [--only <area>]` prints, per area, routes with a workflow, triggers with a
  workflow and obid's scenarios with an assertion, and exits 1 when anything is missing.
  `--gate` is the check's: it fails only on a route or trigger without a workflow, an unclassified
  scenario or an unknown one, and reports the scenarios still waiting for an assertion as the
  backlog (OBIDSPEC-11).
- The check runs it (docs/contracts/ci.md, CI-66 and 67): GitHub's merge queue, nightly and on
  demand all of them, a pull request all of them only when it changes
  `tools/obid-spec/` or carries `full-suite`; the Mac's `scripts/oby-check.sh` all of them for a
  change to the suite.
- `obid-spec list`, `obid-spec nodes`.

## Frozen until launch

The owner's decision of 2026-11-18: "wire it in and freeze it". Until after launch, no new
workflow or scenario goes in beyond the launch flows. A workflow, or a step, may still be added
only when it:

- proves a bug fix: the fixed issue's acceptance, or the `expectedRed` that comes off; or
- proves a launch flow (it then carries the `launch` tag).

Anything else waits. `obid-spec coverage`'s backlog of unproven scenarios stays a list, not work.

The lab, below, is the exception the owner made on 2026-11-19: its own area, never in the launch's
set, run only when asked for.

## The lab: new features, proven here first

The owner's decision of 2026-11-19: every feature after wave 3 (other computers, Linux lanes and
Codex as T3 runs it, the browser and the desktop, the Obysphere around them) is built here first,
as Rust nodes and workflows, and proven on real machines before any of it is ported into obid or the
app; once ported, its workflows become its acceptance tests against obid (OBIDSPEC-16).

- Workflows live in `workflows/lab/<feature>/` (the area `lab-<feature>`), with a manual trigger
  and the `lab` tag, never `launch`.
- `obid-spec run --tag lab` (or `--only lab-<feature>`) runs them; any other run leaves them out,
  and `coverage` doesn't count them.
- Nodes are `lab.<feature>.<verb>`, one file per feature in `src/lab/`, owned by the session
  building it: `machine.rs` (the machines), `remote.rs`, `browser.rs`, `lanes.rs`, `sphere.rs`.
- `lab.machine.up` makes an Ubuntu machine of the workflow's own with OrbStack, isolated (no folder
  shared with the Mac) and with the user `oby`; the cleanup deletes every machine the workflow
  made. Without OrbStack a lab step answers `skipped`.

```bash
tools/obid-spec/target/debug/obid-spec run --tag lab
```

## T3 Code's bugs, proven against obid

T3 Code's history is the map of the bugs a dispatcher of the owner's CLIs hits. Each kind that
applies to obid gets workflows here, driving the real obid with its stand-in agents.

- Workflows live in `workflows/t3/<topic>/` (the area `t3-<topic>`), with a manual trigger and the
  `t3` tag, never `launch`.
- `obid-spec run --tag t3` (or `--only t3-<topic>`) runs them; any other run leaves them out, and
  `coverage` doesn't count them.
- `t3/conversations/`: one turn's events in order and once, Stop with words waiting, a provider's
  errors, an agent that dies mid-turn, restarts with words waiting, the event stream reconnected
  mid-turn, a conversation deleted while it runs, the same words sent twice, and two conversations
  at once. Their nodes, `src/t3_conversations.rs`: `t3.follow` (the stream from `after` and/or a
  `Last-Event-ID` header, as `EventSource` reconnects), `t3.trail` (one conversation's events in the
  order they came, so a `check` can assert order and count), `t3.kill-agent` (SIGKILL to the
  stand-in agent itself, never what starts it) and `t3.heard` (what the stand-in heard, from its
  own `turns.jsonl`).
- A step that finds a real obid bug keeps its assertion, marked `expectedRed` with the bug's issue.
- `t3/tasks/`: worktrees and git, a worktree gone from under its task, tasks started at once, branch
  names, edits `git status` hides, a stale lock, a clone moved, a base that moved, binary and large
  diffs, submodules, and a delete that leaves nothing behind. What a crash or the owner's Finder
  does to obid's folders between two of its steps, `t3.git` (git in a worktree or obid's clone) and
  `t3.fs` (remove, move or write a file there) do (`src/t3_tasks.rs`). They only act: what obid
  answers is asserted with the suite's own nodes.

```bash
tools/obid-spec/target/debug/obid-spec run --tag t3
```

## The accounts area's scenarios

The owner's decision of 2026-11-20 ("Yes, both"): every piece of obid's logic is proven through the
suite, area by area, despite the freeze above. The accounts area's scenario workflows sit beside its
route workflows in `workflows/routes/accounts/`, named after the scenario they prove first
(`accounts-7-the-same-login-twice.json`), each with the route it drives most as its trigger. Its own
nodes, `src/coverage_accounts.rs`, only act or read: `accounts.exec` (an executable stand-in: a tool
with another version, one that fails or hangs, a `cursor-agent` with a known model list),
`accounts.link` (a login file that is a link) and `accounts.log` (the scratch obid's own log, so a
`check` proves a login's secret never reached it, or counts the lines that say something).

## How it runs

The host, `obid-spec`, reads the workflows and judges each step. The nodes run in a second binary,
`obid-spec-nodes`, one per workflow, which speaks BLOK's gRPC over its stdin and stdout as
`oby-check-blok-nodes` does: no port nor socket. Each workflow gets its own scratch folder under
`$TMPDIR/obid-spec-<time>-<pid>/`, so every obid it starts has its own home, port and token.

A scratch obid never touches the owner's: its `HOME` is scratch, its `PATH` holds only obid's
stand-in agents and fake `gh` (`core/crates/obid/tests/agents`), `node`, `gitleaks` and the system's
folders, its Keychain is the stand-in `security`, and every process is stopped by PID at the
workflow's end, whatever it ended in.

The local Sphere is obi-sphere's own server at the commit [e2e/sphere.ref](../../e2e/sphere.ref)
pins, as e2e/rig.mjs runs it: one per run, each workflow making its own Obysphere on it. The first
run clones it into `~/Library/Caches/obid-spec/` and installs its packages with `pnpm`;
`OBID_SPEC_SPHERE_CLONE` names a clone of your own. A lying Sphere (`sphere-liar`) can stand in
front of it to serve a member's card re-signed with another owner key, which the real Sphere
never does, so obid's pinning is proven against the real server.

## Workflows

BLOK v2 files, so BLOK Studio can show and edit them: `workflows/routes/<area>/` (one per route,
its trigger `http` naming the method and path as routes.txt lists them), `workflows/triggers/`
(one per background trigger, its trigger `manual`), and `workflows/app/` and `workflows/desktop/`
(the real app and the Mac around it, below; `manual`). A step uses one of the nodes
(`runtime.rust:<node>`, `obid-spec nodes` lists them) with its `inputs`; its output is kept as
`state.<id>`, and a later step's inputs read it as `${state.<id>.<path>}`.

```json
{ "id": "sphere-21-joined-409", "use": "runtime.rust:call",
  "ui": { "proves": ["SPHERE-21"] },
  "inputs": { "obid": "${state.restarted}", "method": "PUT", "path": "/v1/obid/sphere/owner",
              "body": { "owner": "erin" },
              "expect": { "status": 409, "paths": { "error": { "$contains": "already in an Obysphere" } } } } }
```

- `ui.proves`: the scenarios the step's assertion proves; coverage counts them.
- `ui.expectedRed`: a known bug's issue. The step is expected to fail until it's fixed: its failure
  is listed with the issue and doesn't fail the run; its pass does, so the mark comes off.
- `ui.flaky`: a known intermittent bug's issue. Its failure is listed with the issue and doesn't fail
  the run; its pass is a pass.
- `expect`: `status` (one or a list), `body` (a subset), `paths` (dotted paths), `text`, with
  matchers `$exists`, `$ne`, `$type`, `$len`, `$contains`, `$matches`, `$any`, `$none`, `$count`,
  `$oneOf`, `$not` (src/expect.rs).
- `call` takes `as`: `owner` (obid's token, the default), `device` (and the device's
  `X-Oby-Words` over the exact body), `none`, `wrong`, `token`; `confirm` for a Confirm tap signed by
  the device; `within` to wait for state; `pick` to keep an array element for later steps;
  `background` to send a request that blocks (a turn's words) while later steps act, its answer
  read by a `call-wait` step.
- Stand-ins: `obid-start`'s `agent` picks Claude Code's stand-in (`fake-harness-jobs` routes a
  conversation to `fake-harness`, a task to `fake-claude`, a side call to `fake-planner`; a sign-in
  needs `fake-claude` itself, whose `node` obid finds). The task's or the words' text drives it
  (`SAY <words>`, `LINGER <ms>`, `ASK …`, `OBY oby_dispatch {…}`; `COMMIT: <file>`, `SLEEP`, `FAIL`,
  `LIMITED: <s>`, `KEEP PROMPTS`): the stand-ins' own headers list them.
- GitHub: by default the stand-ins' `gh` and no pull requests (a push to the fixture's `file://`
  remote). `obid-start`'s `github` puts obid's recording `gh` (core/crates/obid/tests/fakes/gh)
  first on its `PATH` with the files it names (`routes.json` and what the routes run), and the
  fixture's pushes then get pull requests there. `fixtures/github/` is one such GitHub: one pull
  request on the fixture's bare remote that `pr create` opens, green and clean, which a merge
  squashes onto its base in that remote, as GitHub does (`flags/refuse-merge` in its files: GitHub
  refuses the merge). A file's text may be `{"fixture": "<path under fixtures/>"}`. Never the real
  GitHub.

**Every assertion can fail** (OBIDSPEC-6): the host changes the answer each assertion passed on at
each place it looks, and each change must fail it; one that passes anyway is a vacuous assertion
and fails the step. To see a step go red by hand, copy `workflows/`, make an expectation wrong and
run with `OBID_SPEC_WORKFLOWS=<the copy>`.

## The real app and the Mac

The same workflows drive the real Electron app (`workflows/app/`, OBIDSPEC-13) and the macOS around
it (`workflows/desktop/`, OBIDSPEC-14), as [docs/verification.md](../../docs/verification.md) asks:
a change a person sees is proven by driving the app, from outside, red before green.

- **`app.*`** (Playwright): `app.launch` starts this checkout's built app (the run builds it once
  with `npm run build` in `apps/electron`) through the human drive's launcher
  (`apps/electron/scripts/drive-lib.mjs`, `_electron.launch`), hidden, with its own user-data
  folder, on the workflow's scratch obid (`"obid": "${state.obid}"`, as `OBID_PORT` and `OBID_HOME`)
  or on the app's demo data (`"mock": true`, `?everything&chat`, a port nothing listens on; `query`
  adds more of the demo's words, `browser=<n>` one of the browser's design boards; `storage` sets
  the window's own `oby.*` choices first, the side panel kept open or closed).
  Never the owner's obid on 7777, their Oby.app or their app folder. Then `app.click` (or
  `"hover": true`), `app.type`, `app.press`, `app.read` (count, text, visible, attributes),
  `app.measure` (an element's box and, `against` other targets, `overlap`, `sharedArea`, `gap`),
  `app.screenshot` (into the workflow's `screens/`, and `OBID_SPEC_SHOTS` when set),
  `app.compare` (the window against a design board of `lab-ui/browser/design`, box by box and word
  by word, as `lab.ui.compare` holds the lab's screens: `within`, `sameWords`, `off`, `today`;
  `skip` leaves pieces out, each with its reason in the contract), `app.sidebyside` (the board and
  the window as one PNG) and `app.close`. `app.read` takes `styles` too (a property against an
  app token: `matches`). A target is `{"role", "name"}`, `{"label"}`, `{"text"}` or `{"css"}`, with
  `within`, `hasText`, `visible`, `nth`/`last`. `app.read` and `app.measure` take `expect`,
  `within` (seconds, asked again until it holds) and `hold` (milliseconds: it must still hold
  then, so a toggle that flips back fails). The Rust nodes only orchestrate: each app has one
  helper, `apps/electron/scripts/drive-app.mjs`, one JSON line in and one out per step.
- **`desktop.*`** (agent-desktop): `desktop.snapshot` (a surface's tree; for the menu bar, each
  menu's items by name), `desktop.read` (one menu's items, or an element by role and name),
  `desktop.click` (a menu item by path, `["View", "Toggle Sidebar"]`) and `desktop.press` (a chord,
  `cmd+b`, which agent-desktop delivers by running the menu item that carries it, as macOS does).
  The app is found by its PID, never by name. agent-desktop is installed into the workflow's folder
  (`npm install --prefix`, `agent-desktop@1.3.1`, never globally) unless `OBID_SPEC_AGENT_DESKTOP`
  names one. A menu item's click goes to the app's focused window, as a person's does, so `click`
  and `press` bring the (still invisible) window forward for that moment and give the front back to
  the app that had it.
- **Skipped, never green** (OBIDSPEC-15): off macOS, with agent-desktop absent, or without the
  Accessibility and Screen Recording permissions (granted once by the owner to the terminal that
  runs the suite; no node asks for, grants or changes one), a step answers why and the workflow is
  marked `skip`: neither passed nor failed, its later steps not run. GitHub's Linux runners skip
  every `app.*` and `desktop.*` workflow; the Mac's check runs them.
- **Red first.** `OBID_SPEC_APP=<another checkout>/apps/electron` (built, its `node_modules`
  linked) drives that build instead: the code before a fix, to watch the workflow fail.

```bash
tools/obid-spec/target/debug/obid-spec run --workflow app/ --keep          # screenshots kept
OBID_SPEC_APP=/tmp/before/apps/electron tools/obid-spec/target/debug/obid-spec run --workflow desktop/
OBID_SPEC_AGENT_DESKTOP=/nowhere tools/obid-spec/target/debug/obid-spec run --workflow desktop/   # the skip
```

## Coverage and scope.json

[scope.json](scope.json) says, per area, the route prefixes it covers, the contracts read so far
(each scenario either obid's, to prove here, or another surface's, with why) and the workflow files
that prove each background trigger. A scenario of a contract in scope that is in neither list fails
coverage, as does a step naming a scenario no contract in scope lists: nothing goes unread
silently. A contract not yet in scope.json is counted as unread.

## Adding to it

1. Read the route's `docs/api/<area>.md` and the contract that promises it.
2. Write `workflows/routes/<area>/<method>-<what>.json`: start a scratch obid, its refusals (401,
   403, 404, 409, 422 as the contract says), its happy path, and what it writes or emits
   (`home-file`, `events-wait`), each asserting step named after its scenario.
3. Add the contract to scope.json with every scenario classified.
4. `obid-spec run --workflow <name>`, then `obid-spec coverage --only <area>`.
5. A workflow that finds a real bug: don't fix product code here. File the issue (What / Why /
   How) and mark the step `expectedRed` with it.
