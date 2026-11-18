# obid-spec

obid's endpoints and background triggers, proven black-box against the real obid binary, as BLOK
workflows and Rust nodes ([docs/contracts/obid-spec.md](../../docs/contracts/obid-spec.md),
OBIDSPEC-1 to 9). Like the Brain's map, but executable: each route in
[docs/api/routes.txt](../../docs/api/routes.txt) has a workflow, each step that asserts names the
contract scenario it proves, and every assertion is shown able to fail on every run.

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
- `obid-spec coverage [--only <area>]` prints, per area, routes with a workflow, triggers with a
  workflow and obid's scenarios with an assertion, and exits 1 when anything is missing.
- `obid-spec list`, `obid-spec nodes`.

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
its trigger `http` naming the method and path as routes.txt lists them) and `workflows/triggers/`
(one per background trigger, its trigger `manual`). A step uses one of the nodes
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
- `expect`: `status` (one or a list), `body` (a subset), `paths` (dotted paths), `text`, with
  matchers `$exists`, `$ne`, `$type`, `$len`, `$contains`, `$matches`, `$any`, `$none`, `$count`,
  `$oneOf`, `$not` (src/expect.rs).
- `call` takes `as`: `owner` (obid's token, the default), `device` (and the device's
  `X-Oby-Words` over the exact body), `none`, `wrong`, `token`; `confirm` for a Confirm tap signed by
  the device; `within` to wait for state; `pick` to keep an array element for later steps.

**Every assertion can fail** (OBIDSPEC-6): the host changes the answer each assertion passed on at
each place it looks, and each change must fail it; one that passes anyway is a vacuous assertion
and fails the step. To see a step go red by hand, copy `workflows/`, make an expectation wrong and
run with `OBID_SPEC_WORKFLOWS=<the copy>`.

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
