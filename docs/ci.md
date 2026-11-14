# Checks

Checks run in two places: GitHub's `CI` workflow (`.github/workflows/ci.yml`) and the Mac's `scripts/oby-check.sh`. Every pull request says what was run and what it printed. This page is what runs today (docs/contracts/ci.md, CI-1 to 6 and CI-17 to 20) and how a merge lands (CI-7, 8 and 21); the design behind it is [docs/ci-architecture.md](ci-architecture.md).

## What GitHub runs

`ci.yml` has one job, `Check`, that says whether a run passed. The other jobs run or are skipped by `if:` (a skipped job reports success); the workflow has no `paths:` filter, so `Check` always reports.

| Job | What it runs |
| --- | --- |
| `plan` | Which lanes the change reaches (below), and gitleaks with `.gitleaks.toml` over the commits the run brings: a pull request's from its merge base, the merge queue's from the group's base. A nightly or on-demand run on `dev` has no earlier commit and scans its head commit alone. A finding fails `plan`. |
| `lint` | In `core/`: `cargo fmt --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`. |
| `limits` | `OBY_CHECK_ONLY=limits scripts/oby-check.sh <base>` on the checked commit, with full history: obid's line limits, `jobs.rs`'s and the tests' sleeps, `AGENTS.md`'s bytes, and the flaky list's guard, judged by the script's own counting (docs/contracts/ci.md, CI-35). The base is `merge_group.base_sha` in the queue, `pull_request.base.sha` on a pull request, and the commit itself nightly and on demand. It installs nothing. |
| `rust-build` | One `cargo nextest archive --workspace --locked` of `core/`, uploaded and kept 1 day. |
| `rust-test` (1/4 to 4/4) | The archive's tests, a quarter each (`--partition slice:k/4`), with `--profile ci --no-fail-fast`, bubblewrap and gitleaks set up. Each uploads its JUnit report, pass or fail, kept 14 days. |
| `e2e` | `make e2e`: stub agents, against the oby-sphere commit in `e2e/oby-sphere.ref`. |
| `control-plane` | In `cloud/control-plane/`: `pnpm test` and `pnpm check`. |
| `electron` | In `apps/electron/`: `npm ci`, `npm run typecheck`, `npm test`, `npm run build` (Node 22, with zsh installed: the ChatGPT gate's test runs `scripts/package.sh` with `/bin/zsh`). |
| `tools` | `cargo test` in `tools/oby-check-blok/`, its own Cargo workspace, with the tool installed and gitleaks on `PATH`, since its tests run `scripts/oby-check.sh`. |
| `Check` | Needs every job above. |

**Modes.** Which jobs run depends on the event, then on the lanes:

| Run | Jobs |
| --- | --- |
| A draft pull request without `full-ci` | none: every job is skipped, `Check` too |
| A ready pull request into `dev` | `plan`, `lint` when the `rust` lane is reached, `Check`: no test job |
| A pull request labelled `full-ci` (a draft too, from the next push after labelling) | `plan`, `Check`, and every job its lanes reach |
| A pull request into `main` | the same as `full-ci` |
| The merge queue (`merge_group`) | the same, over the group's whole diff (`merge_group.base_sha..head_sha`) |
| Nightly at 03:17 UTC on `dev`, and on demand (`workflow_dispatch`) | every job: every lane is reached |

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
| `docs/**`, `factory/**`, `.claude/**`, `design/**`, `character/**`, `apps/ios/**`, `apps/watch/**`, `apps/shared/**`, `apps/Oby.xcodeproj/**`, `mcp/**`, `*.md` | none: `plan` and `Check` only |
| any other path | every lane |
| a diff that can't be read (a missing commit, `git diff` failing) | every lane |

`lint`, `rust-build` and `rust-test` follow `rust`, and so does `limits` in the runs that test (not on a ready pull request into `dev`); `e2e`, `electron`, `control-plane` and `tools` follow their own lanes.

**`Check`'s rule.** It fails when `plan` didn't succeed (so skipped lanes can never pass over a broken plan), or when any job failed or was cancelled; a skipped job passes. Its step summary shows the mode, the lanes, each job's result, each failed shard with its failing tests (from that shard's JUnit), and under "Flaky" every test that passed only on its retry.

**Shards and retries.** nextest runs each test in its own process. `core/.config/nextest.toml`'s `ci` profile retries a failed test once, writes `target/nextest/ci/junit.xml`, and stops a test after 5 minutes (60-second periods, 5 of them). A test that passes on its retry shows as `flakyFailure` in the JUnit and in `Check`'s "Flaky" list. The same file holds the flaky list (docs/contracts/ci.md, CI-26 to 28): its `[[profile.ci.overrides]]` entries, each naming one exact test and the task that fixes it, get `retries = 2`.

**Caches.** Only the nightly and on-demand runs save the Rust caches (`Swatinem/rust-cache`'s `save-if`), one `shared-key` per job kind: `lint`, `test-build`, `e2e`, `tools`. `rust-test` compiles nothing and has none. Every other run only restores them, because a pull request's caches are scoped to `refs/pull/<n>/merge` and no other pull request can read them. The pnpm and npm caches from `actions/setup-node` have no save switch and are left as they are, because they are small.

**Billed minutes.** Measured on `ci.yml`'s green full run [43453425366](https://github.com/obi-ai-bot/oby/actions/runs/43453425366) (#2175 with `full-ci`, every lane reached because `.github/` changed, about 21 minutes from start to end), each job's duration from `gh run view <id> --json jobs` rounded up to the minute and summed: **66 billed minutes**. `plan` 1, `lint` 3, `rust-build` 8, `rust-test` 12 + 12 + 12 + 10, `e2e` 2, `control-plane` 1, `electron` 2, `tools` 2, `Check` 1. Each shard spends about 2 of its minutes downloading the 2.5 GB archive. A ready pull request into `dev` runs `plan`, `lint` and `Check`: **5 billed minutes**, taken from those three jobs of the same run, since no ready pull request without the label has run `ci.yml` yet.

## What the Mac runs

The Mac's `scripts/oby-check.sh` runs the suites each change reaches, tests included, before it merges (AGENTS.md, "Build and run"). The Apple suites (`xcodebuild`) and `swift test` run only on the Mac.

| Area | On a Mac |
| --- | --- |
| Rust core | In `core/`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked` (workers: only the test binaries they touched; before a merge: all of obid, with `--no-fail-fast`). |
| Plug-ins | In `core/`: `OBI_PLUGIN_HOST_REQUIRE_SANDBOX=1 cargo test --locked -p obi-plugin-api -p obi-plugin-host`. |
| Swift package | `swift test` when `mcp/` or `Package.swift` change (`oby-mcp`). |
| Electron app | In `apps/electron/`: `npm run typecheck`, `npm test`, `npm run build`. |
| iPhone and Watch | `xcodebuild -project apps/Oby.xcodeproj -scheme ObyPhone -destination 'platform=iOS Simulator,name=iPhone 17' test`. |
| E2E | `make e2e` with obi-sphere checked out beside this repository (or `OBI_SPHERE_DIR`). |
| Secrets | `gitleaks git --log-opts=origin/dev..HEAD`. |
| obi-sphere | In `server/`: `pnpm run check`, `pnpm test`, `pnpm run check:validators`, `pnpm run test:client`, and `pnpm run conformance` with `spec/` installed. |

The route guard, obid's `tests/routes_guard.rs` (docs/contracts/ci.md, CI-9 to 11), fails when a route goes, when a client calls a path obid doesn't serve, or when the gateway's `ROUTES` lists one; `scripts/oby-check.sh` plans it for a change to `api.rs`, `gateway.rs`, another obid source with a `.route(`, `docs/api/`, or the clients' sources (`apps/electron/src` but its tests and mock, `apps/shared`, `mcp/server`). To add a route, commit its line in `docs/api/routes.txt` with it. `docs/api/dead-client-calls.txt`, the calls already dead when the guard came, only shrinks.

The Mac retries a failed obid test once, and only a test on the flaky list in `core/.config/nextest.toml` (docs/contracts/ci.md, CI-26 to 28): when every test a test binary or the lib's tests failed was on the list at the merge base and still is, those tests run once more alone, and the output says `retried flaky: <binary id> <test name>`. Any other failure isn't retried. The list only shrinks: a new entry comes with its fixing task file, `factory/queue/tasks/<KEY>.md`, in the same change, else the check fails naming `the flaky list`.

## How a merge lands

A merge into `dev` goes through GitHub's merge queue and needs its `Check` (docs/contracts/ci.md, CI-7, 8 and 21). `dev`'s ruleset asks for a pull request with 0 approvals and the required status check `Check`, and queues it: squash, build concurrency 1, a minimum group of 2 with a 15-minute wait, a maximum group of 5, every entry must pass, and a status-check timeout of 60 minutes. The queue tests each group on top of `dev` and squash-merges its pull requests in order, so the code that lands is the code that was tested; a group whose `Check` fails merges nothing.

Oby (MERGING-41) and the factory (`job merge`, CI-23) hand an approved pull request to GitHub with `gh pr merge --auto --match-head-commit <head>`, which queues it pinned to the head that was reviewed. A pull request into `main` needs `Check` green, with no queue.

The queue's `limits` job judges obid's line limits on the group's combined code (CI-35), so two pull requests that each fit can't put `dev` past a limit together. The script's other rules come along on that code too: a group more than 1,000 lines under the line limit, or under the sleep limit, fails until a pull request lowers the limit.

The Mac's check still gates each pull request before approval, and the nightly full Mac run (CI-29) checks `dev` as merged.

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
