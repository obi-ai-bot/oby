# obid-spec: every endpoint and trigger proven from outside

The business behavior contract for `tools/obid-spec`, from [the template](../business-contract-template.md). Scenarios are numbered `OBIDSPEC-n`; the tests that prove them are named after them (`obidspec_<n>_…` in `tools/obid-spec/src/` and `tools/obid-spec/tests/suite.rs`).

## Feature and status

- **Outcome:** Each route obid serves and each background trigger it runs is proven against the real obid binary, black-box, by a BLOK workflow whose steps name the contract scenarios they prove, so "implemented" in a contract means a workflow saw it happen.
- **Status:** OBIDSPEC-1 to 9 are **implemented** in `tools/obid-spec`. Its workflows cover the Obysphere area (every `/v1/obid/sphere…` and `/v1/spheres/…` route, the Sphere long-poll, deliveries and directory read); the other areas follow, area by area, until `obid-spec coverage` passes whole. It isn't wired into `scripts/oby-check.sh` or CI yet: that is a security path, its own pull request, on the owner's click.
- **Scope:** obid's HTTP API and its background triggers, from outside, on stand-in agents and a local Sphere. Out of scope: the Electron app's own behavior (its tests), the Obysphere server's own behavior (obi-sphere's), real agent CLIs and logins, and anything on the network past this Mac.

## Behavior

### Actors and ownership

- **The suite's host**, `obid-spec`, reads the workflows (`tools/obid-spec/workflows/`), builds obid once, starts the run's local Sphere, runs the workflows and judges each step.
- **The nodes**, `obid-spec-nodes`, one child per workflow, speak BLOK's gRPC to the host over their stdin and stdout, as `oby-check-blok-nodes` does, and do what each step says: start or stop a scratch obid, call it, wait for an event, read a file in its home, sign as the owner's device or owner key, run the local Sphere.
- **scope.json** says, area by area, which routes and contracts the suite covers, which scenarios are obid's (to prove here) and which another surface's, with why, and which workflow files prove each background trigger.

### Triggers and prerequisites

- `obid-spec run [--only <area>] [--workflow <text>] [--jobs <n>] [--obid <path>] [--keep]`, from anywhere in the checkout. `cargo`, `git`, `node`, `pnpm` and `gitleaks` on `PATH`; the first run clones obi-sphere at the commit `e2e/rig-sphere.ref` pins into `~/Library/Caches/obid-spec/` and installs its packages (`OBID_SPEC_SPHERE_CLONE` names another clone).
- `obid-spec coverage [--only <area>]`, `obid-spec list`, `obid-spec nodes`.

### States and transitions

| A step | What the host does |
|---|---|
| Its inputs name `${state.<step>…}` | Takes those values from the earlier steps' outputs, as BLOK keeps `ctx.state[id]`; one that names nothing yet fails the step. |
| It passes | Keeps its output as `state.<id>`. |
| It fails | Fails the workflow; the rest of its steps are skipped. |
| It has `ui.expectedRed` (a known bug's issue) | Its failure is reported as expected red and the workflow goes on; its pass fails the run, so the fix is noticed and the mark removed. |
| The workflow ends, however | A `cleanup` call stops every process its nodes started, by PID; the nodes child is stopped. |

### Failures and recovery

- obid that doesn't say it listens within 60 seconds, a Sphere that doesn't answer its health within 120 seconds, a step past 300 seconds: the step fails, with what obid or the Sphere logged.
- A failed workflow's folder (its obids' homes, logs and `config.json`, the run's `report.json` with every step's output) is kept and named; a passed one's is removed.

### Invariants and boundaries

- **Nothing of the owner's.** A scratch obid has its own home, port and token under the run's folder, `HOME` a scratch folder, `PATH` only obid's stand-in agents and fake `gh` (`core/crates/obid/tests/agents`), `node`, `gitleaks` and the system's folders, a stand-in `security` for the Keychain and an empty `GH_CONFIG_DIR`. Never the owner's obid on 127.0.0.1:7777, `/Applications/Oby.app`, their logins, their CLIs or their GitHub; the HTTP client reaches only 127.0.0.1.
- **The Mac is shared.** obid is built once per run with `CARGO_INCREMENTAL=0`; at most 6 workflows at once; every process stopped by its PID and its process group, and a node stops only a process the suite started.
- **Black-box.** The suite reads none of obid's crates: what the owner's app signs (an owner key's card and delegation, a device's words and Confirm) is written again here from the Electron app's `shared/sphere.ts`, `main/words.ts` and `main/device.ts`, so a signature only obid could read fails.
- **Workflows are BLOK v2.** BLOK Studio can show and edit them; a step BLOK can express that this host can't run (`branch`, `switch`, `forEach`, `loop`, `wait`, `tryCatch`, `parallel`, `subworkflow`) is refused before the run, not run differently.

## Interfaces and dependencies

- obid's HTTP API (`docs/api/`), its route list `docs/api/routes.txt`, and the contracts under `docs/contracts/`.
- The local Sphere: obi-sphere's `server/scripts/local.mjs` at `e2e/rig-sphere.ref` (Dmytro's server, read only).
- `blok-rs` at the revision oby-check-blok pins, for the node registry and its gRPC service.

## Acceptance scenarios

- **OBIDSPEC-1** Each workflow is a BLOK v2 file; one under `workflows/routes/<area>/` has an `http` trigger naming a route's method and path as `docs/api/routes.txt` lists it, one under `workflows/triggers/` a `manual` trigger. Every shipped workflow parses, and every route it names is one obid serves (`obidspec_1_…`).
- **OBIDSPEC-2** The nodes are written once and every workflow uses them: `obid-start`, `obid-stop`, `call`, `check`, `events-follow`, `events-wait`, `home-file`, `device-pair`, `device-sign`, `owner-key`, `sign-card`, `delegation`, `sphere-make`, `sphere-invite`, `sphere-policy`, `sphere-admin`, `sphere-owner`, `sphere-join`, `sphere-seal`, `sphere-liar`, `sphere-lie`, `next-second`; a step that uses anything else, or a BLOK step this host can't run, is refused before the run (`obidspec_2_…`).
- **OBIDSPEC-3** `obid-spec coverage` prints, per area, routes with a workflow, triggers with a workflow and obid's scenarios with an assertion naming them (`ui.proves`), and exits 1 when any is missing; `--only <area>` judges that area alone (`obidspec_3_…`).
- **OBIDSPEC-4** Nothing goes unread silently: a scenario of a contract in scope.json that is in neither its `obid` nor its `elsewhere` list, a scenario scope.json names that the contract doesn't list, and a step naming a scenario no contract in scope lists each fail coverage (`obidspec_4_…`).
- **OBIDSPEC-5** `obid-spec run` runs the workflows in parallel, each on its own nodes child and scratch folder, steps in order with each one's output kept for the next; it prints one line per workflow (its steps passed, its assertions passed, its expected reds) and the expected reds with their issues, and exits 0 only when no step failed (`obidspec_5_…`).
- **OBIDSPEC-6** Every assertion is proven able to fail, on every run: the answer it passed on is changed at each place it looks (the status, each leaf of the expected body, each path, the text), and each change must fail it. One that passes anyway is a vacuous assertion and fails the step; a step that names a scenario but asserts nothing fails too (`obidspec_6_…`).
- **OBIDSPEC-7** A step marked `ui.expectedRed` with a bug's issue is expected to fail: its failure doesn't fail the run and is listed with the issue; its pass fails the run (`obidspec_7_…`).
- **OBIDSPEC-8** A scratch obid gets only what's said under "Nothing of the owner's": its own config, home, token and port, a `PATH` of its stand-ins and the system's folders, and no variable of the person's environment; `obid-stop` and the end of a workflow stop it by PID, and a PID the suite didn't start is never signalled (`obidspec_8_…`).
- **OBIDSPEC-9** The lying Sphere passes every call through as it is, but serves the named owner's cards in the directory re-signed with a new owner key, the key the real Sphere never changes, so a changed owner key (SPHERE-8 to 10) is proven against the real server (`obidspec_9_…`).
