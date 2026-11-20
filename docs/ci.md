# Checks

Checks run in two places: GitHub's `CI` workflow (`.github/workflows/ci.yml`) and the Mac's `scripts/oby-check.sh`. Every pull request says what was run and what it printed. This page is what runs today (docs/contracts/ci.md, CI-1 to 6, CI-17 to 20 and CI-60 to 65), the CLI canary (CI-14 to 16) and how a merge lands (CI-7, 8 and 21); the design behind it is [docs/ci-architecture.md](ci-architecture.md).

## Before a push

`scripts/preflight.sh` (docs/contracts/ci.md, CI-69) runs the checks GitHub fails a pull request on that a minute can run: the Brain's repo checks (`tools/brain/test/repo.test.ts`), `obid-spec coverage --gate`, and `cargo fmt --check` and clippy `-D warnings` where Rust changed. It names each miss with what to change, and is required before every push (AGENTS.md, "How we work"). Its first run in a fresh worktree installs the Brain's npm packages and builds with cargo for a few minutes; `scripts/preflight-test.sh` is its own test.

## What GitHub runs

`ci.yml` has one job, `Check`, that says whether a run passed. The other jobs run or are skipped by `if:` (a skipped job reports success); the workflow has no `paths:` filter, so `Check` always reports.

| Job | What it runs |
| --- | --- |
| `plan` | Which lanes the change reaches (below), which Rust tests run ("Which Rust tests run"), and gitleaks with `.gitleaks.toml` over the commits the run brings: a pull request's from its merge base, the merge queue's from the group's base. A nightly or on-demand run on `dev` has no earlier commit and scans its head commit alone. A finding fails `plan`. |
| `lint` | In `core/`: `cargo fmt --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`. |
| `limits` | `OBY_CHECK_ONLY=limits scripts/oby-check.sh <base>` on the checked commit, with full history: it reports obid's production line count and its difference from the base (CI-49), and judges `jobs.rs`'s lines, the tests' sleeps, `AGENTS.md`'s bytes and the flaky list's guard by the script's own counting (docs/contracts/ci.md, CI-35). The base is `merge_group.base_sha` in the queue, `pull_request.base.sha` on a pull request, and the commit itself nightly and on demand. It installs nothing. |
| `rust-build` | One `cargo nextest archive --locked` of `core/` with the targets `plan` chose (`--workspace`, or `-p obid --lib --test <name>…`), uploaded and kept 1 day. |
| `rust-test` (1/n to n/n) | The archive's tests `plan`'s filter keeps, in the shards `plan` chose (1 to 4, `--partition slice:k/n`), with `--profile ci --no-fail-fast --no-tests=warn`, bubblewrap and gitleaks set up. Each uploads its JUnit report, pass or fail, kept 14 days. |
| `e2e` | `make e2e`: stub agents, against the oby-sphere commit in `e2e/sphere.ref`, the deployed Sphere's (the rig and `test:sphere` read the same one). |
| `control-plane` | In `cloud/control-plane/`: `pnpm test` and `pnpm check`. |
| `electron` | In `apps/electron/`: `npm ci`, `npm run typecheck`, `npm test`, `npm run build` (Node 22, with zsh installed: the ChatGPT gate's test runs `scripts/package.sh` with `/bin/zsh`). |
| `tools` | `cargo test` in `tools/oby-check-blok/`, its own Cargo workspace, with the tool installed and gitleaks on `PATH`, since its tests run `scripts/oby-check.sh`. |
| `obid-spec` | In `tools/obid-spec/`, its own Cargo workspace: obid built, the suite's own tests, `obid-spec coverage --gate` (every route and trigger has a workflow, every scenario is classified; the scenarios still waiting for an assertion are reported, not a failure), then `obid-spec run` against this commit's obid, four workflows at a time, on stand-in agents and the oby-sphere commit in `e2e/sphere.ref`, with bubblewrap and gitleaks set up (docs/contracts/obid-spec.md; CI-67). Which workflows, from `plan`: in the merge queue every one when the change reaches the `rust` lane or the suite, so two pull requests green apart can't break `dev` together (about 3 minutes against the 1.5 the `launch` ones took); nightly and on demand every one; on a pull request every one only when it changes `tools/obid-spec/` or carries `full-suite`, else none. A step marked as a known bug (`expectedRed`, `flaky`, each with its issue) doesn't fail it; an expected red that passes does. A failed workflow's scratch folders are uploaded as `obid-spec-failed`, kept 7 days. |
| `brain` | In `tools/brain/`: `npm ci`, `npm run typecheck`, `npm test` (the Brain's map of obid and the app, read from the code) and the extractor's `cargo test` (CI-56). Reported, never a failure: `Check` ignores its result. |
| `proofs` | Kani at its pin: every obid harness compiled (`cargo kani -p obid --tests --only-codegen`, CI-57), then the harnesses the change requires by path proved (`scripts/kani-required.sh` and `scripts/kani-prove.sh`, CI-58), each within 20 minutes, but those on `core/.config/kani-slow.txt`; every one nightly and on demand (CI-59). docs/proofs.md says what they prove. |
| `Check` | Needs every job above. Fails when a guard `plan` named ran no test. |

**Modes.** Which jobs run depends on the event, then on the lanes:

| Run | Jobs |
| --- | --- |
| A draft pull request without `full-ci` or `full-suite` | none: every job is skipped, `Check` too |
| A ready pull request into `dev` | `plan`, `lint` when the `rust` lane is reached, the Rust guards when the change reaches what they read, `Check`: no other test job |
| A pull request into `dev` labelled `full-ci` (a draft too, from the next push after labelling) | `plan`, `Check`, every job its lanes reach, and of the Rust tests the affected set and the guards |
| A pull request labelled `full-suite`, or into `main` | `plan`, `Check`, every job its lanes reach, and every Rust test |
| The merge queue (`merge_group`) | the same as `full-suite`, over the group's whole diff (`merge_group.base_sha..head_sha`) |
| Nightly at 03:17 UTC on `dev`, and on demand (`workflow_dispatch`) | every job: every lane is reached |

**Which label.** A pull request whose change touches Rust under `core/` (a `.rs` file, a `Cargo.toml` or `Cargo.lock`) or `tools/obid-spec/` gets `full-suite` before the push that should run it, and before it's armed, so its own run is its first full one (every Rust test, CI-63, and every obid-spec workflow, CI-67) and a test the affected set would miss fails on the pull request, not in the merge queue with the group behind it. An app-only or docs pull request gets `full-ci`. The ship skill (`.claude/skills/ship/SKILL.md`) says when.

A newer push to the same pull request cancels the run it supersedes; merge queue, nightly and on-demand runs always run to the end. A push to `dev` or `main` starts no run. On demand: `gh workflow run ci.yml --ref dev`.

**Lanes.** `plan` reads `git diff --name-only` of the run's range and prints the lane set to its log. The first row a path matches decides:

| Paths | Lanes |
| --- | --- |
| `.github/**`, `scripts/oby-check*`, `core/Cargo.lock`, `core/Cargo.toml`, `rust-toolchain*` | every lane |
| `docs/api/**` | `rust` |
| `core/**`, `e2e/**`, `Makefile` | `rust`, `e2e` |
| `apps/electron/**` | `electron` |
| `cloud/control-plane/**` | `control-plane` |
| `tools/oby-check-blok/**` | `tools` |
| `tools/brain/**` | none but `brain` |
| `tools/obid-spec/**` | none (`obid-spec` runs by its own rule, in the `obid-spec` row above) |
| `docs/**`, `factory/**`, `.claude/**`, `design/**`, `character/**`, `apps/ios/**`, `apps/watch/**`, `apps/shared/**`, `apps/Oby.xcodeproj/**`, `mcp/**`, `*.md` | none: `plan` and `Check` only |
| any other path | every lane |
| a diff that can't be read (a missing commit, `git diff` failing) | every lane |

`lint` follows `rust`, and so do `limits` and `proofs` in the runs that test (not on a ready pull request into `dev`); `rust-build` and `rust-test` follow `plan`'s choice of Rust tests (below); `e2e`, `electron`, `control-plane` and `tools` follow their own lanes. `brain` follows what the Brain reads, apart from the rows above: `tools/brain/**`, obid's `src/`, `apps/electron/src/`, `apps/shared/`, `docs/api/`, `docs/runner-api.md` and `AGENTS.md` (every lane reaches it too). `plan` also prints the Kani harnesses the change requires (`proofs required:`), `all` when every lane is reached.

**Which Rust tests run.** `plan` hands the changed paths to `scripts/ci-rust-tests.sh` (docs/contracts/ci.md, CI-60 to 65), which prints the archive's targets, a nextest filter and the shards:

| Run | Rust tests |
| --- | --- |
| The merge queue, nightly, on demand, a pull request into `main` or labelled `full-suite`, reaching `rust` | every test of the workspace, 4 shards |
| A pull request into `dev` labelled `full-ci`, reaching `rust` | the affected set and the guards; every test when the map can't decide |
| Any other run but a skipped draft that reaches `rust`, or whose change reaches `core/`, `apps/electron/`, `apps/shared/`, `docs/api/`, `docs/runner-api.md`, `docs/contracts/` or `e2e/` | the guards, 1 shard |
| Anything else | none |

- **The affected set** is the Mac's own map, not a new one: `OBY_CHECK_PLAN=1 OBY_CHECK_PATHS=<the paths> scripts/oby-check.sh` names the obid test binaries and lib modules a worker's check runs (`tests/<module>.rs` and `tests/<module>_api.rs`, the binaries naming a touched folder of `tests/`, `jobs_setup` and `released_files` for their sources). The script widens it by obid's imports: each module's whole lib tests (`sphere::`, not only `sphere::pins::`), and every test binary named after the module (`sphere_questions`, `mcp_sphere`) or importing it (`obid::sphere`). A file under obid that the crate includes (`prompt-budget.txt`, `cli-versions.json`) counts as the modules that name it. A one-file change to `core/crates/obid/src/sphere/` runs 8 binaries of 62 and `sphere::`'s lib tests, in 2 shards.
- **Every test when the map can't decide**: the workspace's manifests and lock, `rust-toolchain*`, `core/.config/`, the checks themselves (`.github/`, `scripts/oby-check*`, `scripts/ci-rust-tests*`), obid's `tests/support/`, its `lib.rs` or `main.rs` beyond a module declared (`pub mod x;`, read from the run's diff), a file in its `src/` that isn't Rust, another crate under `core/crates/` (obid builds on `obi-protocol`), a file of `e2e/` the tests' support runs (`e2e/lib/fixture.mjs`), and a change the map names no test for, or plans all of obid's lib tests or another crate's tests for. `plan`'s log says which.
- **The guards** run in every run that runs Rust tests, and are the only ones on a ready pull request without a label and on an app-only change in the queue. Each keeps two places in agreement, so a change to either side can break it whatever its lane: `routes_guard` (obid's routes, `docs/api/routes.txt`, the gateway's `ROUTES`, the apps' calls), `state_owner` (who writes a job's state), `released_files` (what released obids wrote), `runner_api`'s `caps_` tests (`GET /v1/status` and its API doc), and obid's lib tests `prompt_budget_holds`, `unsafe_1_…`, `keychain_1_…`, `merging_2_…` (merge.rs and `scripts/oby-check.sh`), `agentmcp_7_…` and `one_protocol_version_list` (read from obid's sources), and the secret scan's two over `README.md`, `docs/ARCHITECTURE.md` and the repository's file names. The list is in the script; `Check` fails when one ran no test, and `scripts/ci-rust-tests-test.sh` (run by `plan` when the script or the Mac's map changes, and nightly) fails when one names a binary or function obid lacks. `tests/swift_client.rs` keeps the Swift client to obid too, but builds only on macOS: the Mac's check runs it.
- **Shards**: 4 for every test; for the affected set, one per 20,000 lines of its test binaries beyond the guards, at most 4 (the whole suite's 82,000 lines run in 4); 1 for the guards. A shard the filter leaves empty passes.
- **What it costs**: the merge queue still runs everything, so a test the affected set missed fails there, and the group with it, not on `dev`. Replayed over the 65 Rust pull requests among the last 100 merged into `dev` (to `793e04ae`), the map chose every test for 19 (8 changed the checks, 4 the workspace's manifests, 5 obid's crate root beyond a module declared, 2 the e2e fixture) and the affected set for 46: 7 in 1 shard, 15 in 2, 12 in 3, 12 in 4.

**`Check`'s rule.** It fails when `plan` didn't succeed (so skipped lanes can never pass over a broken plan), when any job failed or was cancelled, or when a guard `plan` named ran no test; a skipped job passes. Its step summary shows the mode, the lanes, which Rust tests ran and why (every test, the affected set or the guards, with the binaries and modules), how many tests ran in how many shards, each job's result, each failed shard with its failing tests (from that shard's JUnit), any guard that ran no test, and under "Flaky" every test that passed only on its retry.

**Shards and retries.** nextest runs each test in its own process. `core/.config/nextest.toml`'s `ci` profile retries a failed test once, writes `target/nextest/ci/junit.xml`, and stops a test after 5 minutes (60-second periods, 5 of them). A test that passes on its retry shows as `flakyFailure` in the JUnit and in `Check`'s "Flaky" list. The same file holds the flaky list (docs/contracts/ci.md, CI-26 to 28): its `[[profile.ci.overrides]]` entries, each naming one exact test and the task that fixes it, get `retries = 2`.

**Caches.** Only the nightly and on-demand runs save the Rust caches (`Swatinem/rust-cache`'s `save-if`), one `shared-key` per job kind: `lint`, `test-build`, `e2e`, `tools`. `rust-test` compiles nothing and has none. Every other run only restores them, because a pull request's caches are scoped to `refs/pull/<n>/merge` and no other pull request can read them. The pnpm and npm caches from `actions/setup-node` have no save switch and are left as they are, because they are small.

**Billed minutes.** Measured on `ci.yml`'s green full run [43453425366](https://github.com/obi-ai-bot/oby/actions/runs/43453425366) (#2175 with `full-ci`, every lane reached because `.github/` changed, about 21 minutes from start to end), each job's duration from `gh run view <id> --json jobs` rounded up to the minute and summed: **66 billed minutes**. `plan` 1, `lint` 3, `rust-build` 8, `rust-test` 12 + 12 + 12 + 10, `e2e` 2, `control-plane` 1, `electron` 2, `tools` 2, `Check` 1. Each shard spends about 2 of its minutes downloading the 2.5 GB archive. A ready pull request into `dev` runs `plan`, `lint` and `Check`: **5 billed minutes**, taken from those three jobs of the same run, since no ready pull request without the label has run `ci.yml` yet. With the affected set (CI-60), a one-file Sphere change labelled `full-ci` took **9 minutes** from `plan` to `Check` ([44379422234](https://github.com/obi-ai-bot/oby/actions/runs/44379422234): `rust-build` 3, two shards of 5 and 6), against a median of **17** over the 26 green `full-ci` pull request runs of 2026-11-18 before it, which ran every test. The guards alone, on an app-only change without a label, took **4 minutes** ([44380421141](https://github.com/obi-ai-bot/oby/actions/runs/44380421141): `rust-build` 2, 21 tests in 1 shard in 1).

## The CLI canary

`.github/workflows/cli-canary.yml` (the `CLI canary` workflow, docs/contracts/ci.md, CI-14 to 16) runs the newest Claude Code, Codex and cursor-agent through obid's real adapters every 6 hours (at minute 41) and on demand, so a release that breaks obid is found before the owner's first job is. It runs on GitHub's `ubuntu-latest`, never on a self-hosted runner, with no login, no real key and no secret. Each CLI comes from its vendor's own package (`docs/cli-canary.md`, "Install commands"): Claude Code and Codex into `/opt/cli-canary`, cursor-agent into a throwaway `HOME` in `$RUNNER_TEMP`. One that fails to install leaves only that CLI `not installed`. Not the home or `/tmp` for the first two: obid's box hides both, and a boxed Codex then can't find its platform package beside its own (`Missing optional dependency @openai/codex-linux-…`).

On demand: `gh workflow run cli-canary.yml --ref dev`. Its one input, `force_fail` (`<cli>/<case>`, such as `codex/start`), makes that case fail with the why `forced`, for the proof runs; it reaches the cases only through an environment variable.

| CLI | Checked for | How |
| --- | --- | --- |
| Claude Code | `start`, `permission_ask`, `stop`, `resume` | An own job whose login shell (a stand-in, `OBID_LOGIN_SHELL`) gives it `ANTHROPIC_BASE_URL` for `core/crates/obid/tests/agents/fake-model-server` and the key `dummy` |
| Codex | the same four, and `rate_limits_updated` | A boxed job on a ChatGPT account signed in to the stand-in OpenAI (`core/crates/obid/tests/support/stand_in_openai.rs`), through obid's relay; the model's `x-codex-*` headers show as the account's `fiveHour` |
| cursor-agent | `signed_out` | adapters.rs's `the_real_cursor_agent_signed_out_says_so`, with the installed binary first on `PATH`: its endpoint is Cursor's own backend, which no fake stands in for |

Each case is a test in `core/crates/obid/tests/cli_canary.rs`. Start ends `done` with the fake's words; permission ask, in `ask`, shows the command as the job's permission, and the owner's signed Allow lets it run; stop leaves the job `stopped` while the model holds its answer; resume is Try again on that stopped job, whose new process sends the first task to the model again.

**The report.** Each run uploads `cli-canary-report.json` as the artifact `cli-canary-report`, kept 14 days:

```json
{"run": 43460000000, "clis": {
  "claude-code": {"version": "2.1.325", "cases": {"start": {"result": "passed", "why": ""}, "…": {}}},
  "codex": {"version": "0.169.0", "cases": {"…": {}}},
  "cursor-agent": {"version": "not installed", "cases": {"signed_out": {"result": "skipped", "why": "not installed"}}}}}
```

A version is the first word of the CLI's `--version` that starts with a digit, kept only when it matches `^[0-9A-Za-z][0-9A-Za-z.+_-]{0,63}$`, else `unknown`. A result is `passed`, `failed` or `skipped` with a short why; when bubblewrap can't run on the runner, every case is `skipped` saying so. The run fails when any case failed.

**Twice in a row.** The `drift` job compares the report with the latest completed run's on the same branch, forced or not. A case that failed in both on the same version opens one issue labelled `cli-drift` (`CLI drift: <cli> <version> <case>`), or, while one is open, comments on it with the pairs it doesn't name yet. A failure then a pass, a `skipped` case in either run, a new version, or no previous report opens nothing. The issue holds only the CLI, the version, the case and the two runs' links.

**Permissions.** The workflow's own `permissions` are `{}`. `canary`, which installs and runs code nobody here reviewed, gets `contents: read` and no secret, and checks out with `persist-credentials: false`. `drift` gets `actions: read` (the previous run's artifact), `contents: read` and `issues: write`, and runs nothing from the CLIs. Every action is pinned by the same SHAs as `ci.yml`.

**By hand**, on a Linux machine with bubblewrap and socat, with the CLIs in a folder of your own outside your home and the temporary folders (the box hides those, as above):

```sh
npm install --prefix /opt/cc @anthropic-ai/claude-code@latest @openai/codex@latest
cd core
CLI_CANARY_CLAUDE=/opt/cc/node_modules/.bin/claude CLI_CANARY_CODEX=/opt/cc/node_modules/.bin/codex \
  cargo test -p obid --test cli_canary -- --include-ignored --test-threads=1
```

`CLI_CANARY_RESULTS=<folder>` writes each case's `<cli>/<case>.json`; a case whose variable is unset is `skipped` (`not installed`) and passes. The report and the rule's own tests (`ci_14_…` to `ci_16_…`) run with obid's other tests, with no CLI.

## What the Mac runs

The Mac's `scripts/oby-check.sh` runs the suites each change reaches, tests included, before it merges (AGENTS.md, "Build and run"). The Apple suites (`xcodebuild`) run only on the Mac.

| Area | On a Mac |
| --- | --- |
| Rust core | In `core/`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked` (workers: only the test binaries they touched; before a merge: all of obid, with `--no-fail-fast`). |
| Plug-ins | In `core/`: `OBI_PLUGIN_HOST_REQUIRE_SANDBOX=1 cargo test --locked -p obi-plugin-api -p obi-plugin-host`. |
| Electron app | In `apps/electron/`: `npm run typecheck`, `npm test`, `npm run build`. |
| iPhone and Watch | `xcodebuild -project apps/Oby.xcodeproj -scheme ObyPhone -destination 'platform=iOS Simulator,name=iPhone 17' test`. |
| E2E | `make e2e` with obi-sphere checked out beside this repository (or `OBI_SPHERE_DIR`). |
| obid-spec | For a change to `tools/obid-spec/`: the suite's `cargo fmt`, `clippy` and `test`, then obid and the suite built, `obid-spec coverage --gate` and `obid-spec run`, every workflow (CI-66). In a job's box only the suite's own steps, since scratch obids and a Sphere on loopback don't fit there. A change to obid alone doesn't plan it here: GitHub's merge queue and the nightly run run all of them. |
| Secrets | `gitleaks git --log-opts=origin/dev..HEAD`. |
| obi-sphere | In `server/`: `pnpm run check`, `pnpm test`, `pnpm run check:validators`, `pnpm run test:client`, and `pnpm run conformance` with `spec/` installed. |

The route guard, obid's `tests/routes_guard.rs` (docs/contracts/ci.md, CI-9 to 11), fails when a route goes, when a client calls a path obid doesn't serve, or when the gateway's `ROUTES` lists one; `scripts/oby-check.sh` plans it for a change to `api.rs`, `gateway.rs`, another obid source with a `.route(`, `docs/api/`, or the clients' sources (`apps/electron/src` but its tests and mock, `apps/shared`). To add a route, commit its line in `docs/api/routes.txt` with it. `docs/api/dead-client-calls.txt`, the calls already dead when the guard came, only shrinks.

The Mac retries a failed obid test once, and only a test on the flaky list in `core/.config/nextest.toml` (docs/contracts/ci.md, CI-26 to 28): when every test a test binary or the lib's tests failed was on the list at the merge base and still is, those tests run once more alone, and the output says `retried flaky: <binary id> <test name>`. Any other failure isn't retried. The list only shrinks: a new entry comes with its fixing task file, `factory/queue/tasks/<KEY>.md`, in the same change, else the check fails naming `the flaky list`.

## How a merge lands

A merge into `dev` goes through GitHub's merge queue and needs its `Check` (docs/contracts/ci.md, CI-7, 8 and 21). `dev`'s ruleset asks for a pull request with 0 approvals and the required status check `Check`, and queues it: squash, build concurrency 1, a minimum group of 2 with a 15-minute wait, a maximum group of 5, every entry must pass, and a status-check timeout of 60 minutes. The queue tests each group on top of `dev` and squash-merges its pull requests in order, so the code that lands is the code that was tested; a group whose `Check` fails merges nothing.

Oby (MERGING-41) and the factory (`job merge`, CI-23) hand an approved pull request to GitHub with `gh pr merge --auto --match-head-commit <head>`, which queues it pinned to the head that was reviewed. A pull request into `main` needs `Check` green, with no queue.

The queue's `limits` job judges `jobs.rs`'s line limit on the group's combined code (CI-35), so two pull requests that each fit can't put `dev` past it together. The script's other rules come along on that code too, but for a raise's payback line and its pull request of its own (CI-39, CI-40), which the pull request's own check judges against its merge base. So a group under the sleep limit fails until a pull request lowers the limit.

The Mac's check still gates each pull request before approval, and the nightly full Mac run checks `dev` as merged: the factory's between its jobs (CI-29), and `scripts/oby-nightly.sh` without it (CI-53 to 55), every night at 03:30 once installed (`scripts/oby-nightly.sh --install`), its results in `~/Library/Logs/Oby/nightly/` on the Mac (`latest.md` the newest). Run by hand or by either nightly, the check runs half the Mac's cores of obid's test binaries at once (CI-51), so a full run fits its limit.

The queue and the required check are GitHub's limits, not Oby's: the owner, as the repository's admin, changes or bypasses them in its settings, and when one holds a merge Oby says what holds it.

## Signing

The checks need no certificates. `scripts/oby-check.sh` passes these to `xcodebuild`:

```
CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= ENABLE_HARDENED_RUNTIME=NO
```

- `CODE_SIGN_IDENTITY=-` with `CODE_SIGN_STYLE=Manual`, an empty `DEVELOPMENT_TEAM` and an empty `PROVISIONING_PROFILE_SPECIFIER`: the apps are signed **ad hoc**, with no team and no profile, because CI has no certificate, team or profile to sign with.

Notarized builds of the Mac app are made on a Mac by `apps/electron/scripts/package.sh` (apps/electron/README.md), never by a check.

## Releases

Each release tag adds its own `core/crates/obid/tests/fixtures/released/<tag>/` (three jobs and a `config.json`, as the README there describes) in the release's pull request, so `tests/released_files.rs` proves today's obid still reads what that release wrote (docs/contracts/released-files.md).

GitHub doesn't cut releases yet: the owner packages the Mac app by hand. R09-14 will, from a `v*` tag on `main`, as [release-in-ci.md](release-in-ci.md) says (what a release needs, the `release` environment and its secrets, and the cadence).
