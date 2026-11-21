# Checks

Checks run in two places: GitHub's `CI` workflow (`.github/workflows/ci.yml`) and the Mac's `scripts/oby-check.sh`. Every pull request says what was run and what it printed. This page is what runs today (docs/contracts/ci.md, CI-70 to 76) and how a merge lands (CI-7, 8 and 21).

## Before a push

`scripts/preflight.sh` (docs/contracts/ci.md, CI-69) runs the checks that a minute can run: the Brain's repo checks (`tools/brain/test/repo.test.ts`), `obid-spec coverage --gate`, `obid-spec run` of each obid-spec workflow the change touches (but the lab's, run by hand), and `cargo fmt --check` and clippy `-D warnings` where Rust changed. It names each miss with what to change, and is required before every push (AGENTS.md, "How we work"). Its first run in a fresh worktree installs the Brain's npm packages and builds with cargo for a few minutes; `scripts/preflight-test.sh` is its own test.

## What GitHub runs

`ci.yml` is the software factory's gate, lean on purpose (the owner's decision of 2026-11-21, [docs/decisions/2026-11-21-lean-ci.md](decisions/2026-11-21-lean-ci.md)). The same jobs run on every push to a pull request, a draft included, in the merge queue (`merge_group`), and on `dev` after each merge. A newer push to the same pull request, or to `dev`, cancels the run it supersedes; a merge-queue run runs to the end. Every job runs on GitHub's `ubuntu-latest` with `contents: read`, and checks out without keeping credentials.

| Job | What it runs |
| --- | --- |
| `changes` | What the change reaches, from the paths it changed (below), then gitleaks with `.gitleaks.toml` and `.gitleaksignore` over the commits the run brings: a pull request's since its merge base, the queue's and `dev`'s since the run's base, the head commit alone when there is no earlier commit. Older history isn't rescanned. A finding fails `changes`. |
| `rust` | For a change that reaches all of Rust: `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, obid's limits (`OBY_CHECK_ONLY=limits scripts/oby-check.sh <base>`, CI-35: `jobs.rs`'s lines, the tests' sleeps, `AGENTS.md`'s bytes, the flaky list's guard, and the Rust core's line count reported), then every test of the workspace compiled once into one `cargo nextest archive`, kept 1 day. For a change that reaches only what the guards read: the guards alone (below). |
| `rust-test` (1/4 to 4/4) | The archive's tests in 4 shards (`--partition slice:k/4 --profile ci --no-fail-fast`), with bubblewrap and gitleaks set up. Compiles nothing. |
| `electron` | In `apps/electron/`: `npm ci`, `npm run typecheck`, `npm test`, `npm run build` (Node 22, with zsh installed: the ChatGPT gate's test runs `scripts/package.sh` with `/bin/zsh`). |
| `Check` | Always runs. Passes only when `changes` succeeded and every other job passed or was skipped. |

`.github/actions/setup/action.yml` installs gitleaks at the version `ci.yml` pins (`GITLEAKS_VERSION`, checked against `GITLEAKS_SHA256`), and, when asked, bubblewrap and slirp4netns for obid's job sandbox; the workflow's next step proves bubblewrap works on the runner. Rust is pinned to the 1.97 channel (`RUSTUP_TOOLCHAIN`), and every action to a commit SHA.

**What a change reaches.** `changes` reads `git diff --name-only` of the run's range (a pull request's from its merge base) and prints `rust=<full|guards|none> electron=<true|false>`. The first row a path matches decides what that path reaches, and the paths' results add up:

| Paths | Reaches |
| --- | --- |
| `.github/**`, `scripts/oby-check*`, `core/Cargo.toml`, `core/Cargo.lock`, `rust-toolchain*` | all of Rust, the Electron app |
| `core/crates/obid/src/browser/**` | all of Rust, the Electron app (its relay test reads the CDP methods obid's browser sends) |
| `core/**`, `e2e/**`, `docs/api/**`, `Makefile` | all of Rust |
| `apps/electron/**` | the Electron app, the guards |
| `apps/shared/**`, `docs/runner-api.md`, `docs/contracts/**` | the guards |
| `docs/**`, `factory/**`, `tools/**`, `cloud/**`, `.claude/**`, `design/**`, `character/**`, `apps/ios/**`, `apps/watch/**`, `apps/Oby.xcodeproj/**`, `mcp/**`, `*.md` | nothing: `changes` and `Check` only |
| any other path | everything |
| a diff that can't be read | everything |

A job is skipped with `if:`, never with a workflow-level `paths:`, so `Check` reports on every run.

**Which label.** None: no label, schedule or `workflow_dispatch` changes what runs. A pull request runs what its paths reach, whatever it carries.

**Which Rust tests run.** Every test of the workspace, in 4 shards, for a change that reaches all of Rust; the guards alone for one that reaches only what they read; none otherwise.

- **The guards** keep two places in agreement, so a change to either side can break one: `routes_guard` (obid's routes, `docs/api/routes.txt`, the gateway's `ROUTES`, the apps' calls), `state_owner` (who writes a job's state), `released_files` (what released obids wrote), `runner_api`'s `caps_` tests (`GET /v1/status` and its doc), and obid's lib tests `prompt_budget_holds`, `unsafe_1_…`, `keychain_1_…`, `merging_2_…` (merge.rs and `scripts/oby-check.sh`), `agentmcp_7_…`, `one_protocol_version_list`, and the secret scan's two over this repository's docs and file names. The list is `GUARDS` in `ci.yml`. They run with `--no-tests=fail`: a run whose filter matches no test fails. `tests/swift_client.rs` keeps the Swift client to obid too, but builds only on macOS: the Mac's check runs it.
- **Shards and retries.** nextest runs each test in its own process. `core/.config/nextest.toml`'s `ci` profile retries a failed test once and stops a test after 5 minutes (60-second periods, 5 of them). The same file holds the flaky list (docs/contracts/ci.md, CI-26 to 28): its `[[profile.ci.overrides]]` entries, each naming one exact test and the task that fixes it, get `retries = 2`. Retries come only from that file, never from the command line.

**`Check`'s rule.** A skipped job passes: its part of the repository didn't change. A failed or cancelled job fails it, and so does anything short of success in `changes`, which holds the secret scan. Its log lists each job's result.

**Caches.** Only a run on `dev` saves the Rust cache (`Swatinem/rust-cache`'s `save-if`), one per kind of `rust` run (`full`, `guards`); a pull request and the merge queue restore `dev`'s, never another pull request's. `rust-test` compiles nothing and has none. The npm cache from `actions/setup-node` has no save switch and is left as it is.

**What GitHub doesn't run.** The end-to-end suite (`make e2e`), the control plane's tests, `tools/oby-check-blok`'s tests, obid-spec, the Brain's tests and Kani's harnesses run on the Mac or by hand ("What the Mac runs", docs/proofs.md), not on GitHub. There is no nightly or on-demand run on GitHub.

## The CLI canary

GitHub runs no CLI canary. The cases stay in `core/crates/obid/tests/cli_canary.rs` (docs/cli-canary.md) and run by hand, on a Linux machine with bubblewrap and socat, with the CLIs in a folder of your own outside your home and the temporary folders (obid's box hides those, and a boxed Codex installed there can't find its platform package beside its own):

```sh
npm install --prefix /opt/cc @anthropic-ai/claude-code@latest @openai/codex@latest
cd core
CLI_CANARY_CLAUDE=/opt/cc/node_modules/.bin/claude CLI_CANARY_CODEX=/opt/cc/node_modules/.bin/codex \
  CLI_CANARY_RESULTS=/tmp/canary cargo test -p obid --test cli_canary -- --include-ignored --test-threads=1
```

| CLI | Checked for | How |
| --- | --- | --- |
| Claude Code | `start`, `permission_ask`, `stop`, `resume` | An own job whose login shell (a stand-in, `OBID_LOGIN_SHELL`) gives it `ANTHROPIC_BASE_URL` for `core/crates/obid/tests/agents/fake-model-server` and the key `dummy` |
| Codex | the same four, and `rate_limits_updated` | A boxed job on a ChatGPT account signed in to the stand-in OpenAI (`core/crates/obid/tests/support/stand_in_openai.rs`), through obid's relay; the model's `x-codex-*` headers show as the account's `fiveHour` |
| cursor-agent | `signed_out` | adapters.rs's `the_real_cursor_agent_signed_out_says_so`, with the installed binary first on `PATH`: its endpoint is Cursor's own backend, which no fake stands in for |

A case whose variable is unset is `skipped` (`not installed`) and passes. `CLI_CANARY_RESULTS=<folder>` writes each case's `<cli>/<case>.json`; `node scripts/cli-canary.mjs report --results <folder> --run <id> --out <file>` turns that folder (with each CLI's `--version` output in `<cli>.version`) into one report, and `node scripts/cli-canary.mjs drift --current <file> --previous <file>` compares two reports and opens or comments on one `cli-drift` issue with `gh` when a case failed in both on the same version. The report's and the rule's own tests run with obid's other tests, with no CLI.

## What the Mac runs

The Mac's `scripts/oby-check.sh` runs the suites each change reaches, tests included, before it merges (AGENTS.md, "Build and run"). The Apple suites (`xcodebuild`) run only on the Mac.

| Area | On a Mac |
| --- | --- |
| Rust core | In `core/`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked` (workers: only the test binaries they touched; before a merge: all of obid, with `--no-fail-fast`). |
| Plug-ins | In `core/`: `OBI_PLUGIN_HOST_REQUIRE_SANDBOX=1 cargo test --locked -p obi-plugin-api -p obi-plugin-host`. |
| Electron app | In `apps/electron/`: `npm run typecheck`, `npm test`, `npm run build`. |
| iPhone and Watch | `xcodebuild -project apps/Oby.xcodeproj -scheme ObyPhone -destination 'platform=iOS Simulator,name=iPhone 17' test`. |
| E2E | `make e2e` with obi-sphere checked out beside this repository (or `OBI_SPHERE_DIR`). |
| obid-spec | For a change to `tools/obid-spec/`: the suite's `cargo fmt`, `clippy` and `test`, then obid and the suite built, `obid-spec coverage --gate` and `obid-spec run`, every workflow (CI-66). In a job's box only the suite's own steps, since scratch obids and a Sphere on loopback don't fit there. A change to obid alone doesn't plan it here; `scripts/preflight.sh` runs each workflow a change touches (CI-69). |
| Secrets | `gitleaks git --log-opts=origin/dev..HEAD`. |
| obi-sphere | In `server/`: `pnpm run check`, `pnpm test`, `pnpm run check:validators`, `pnpm run test:client`, and `pnpm run conformance` with `spec/` installed. |

The route guard, obid's `tests/routes_guard.rs` (docs/contracts/ci.md, CI-9 to 11), fails when a route goes, when a client calls a path obid doesn't serve, or when the gateway's `ROUTES` lists one; `scripts/oby-check.sh` plans it for a change to `api.rs`, `gateway.rs`, another obid source with a `.route(`, `docs/api/`, or the clients' sources (`apps/electron/src` but its tests and mock, `apps/shared`). To add a route, commit its line in `docs/api/routes.txt` with it. `docs/api/dead-client-calls.txt`, the calls already dead when the guard came, only shrinks.

The Mac retries a failed obid test once, and only a test on the flaky list in `core/.config/nextest.toml` (docs/contracts/ci.md, CI-26 to 28): when every test a test binary or the lib's tests failed was on the list at the merge base and still is, those tests run once more alone, and the output says `retried flaky: <binary id> <test name>`. Any other failure isn't retried. The list only shrinks: a new entry comes with its fixing task file, `factory/queue/tasks/<KEY>.md`, in the same change, else the check fails naming `the flaky list`.

## How a merge lands

A merge into `dev` goes through GitHub's merge queue and needs its `Check` (docs/contracts/ci.md, CI-7, 8 and 21). `dev`'s ruleset asks for a pull request with 0 approvals and the required status check `Check`, and queues it: squash, at most 2 groups built at once, a minimum group of 1 with a 15-minute wait, a maximum group of 5, every entry must pass (`ALLGREEN`), and a status-check timeout of 120 minutes. The queue tests each group on top of `dev` and squash-merges its pull requests in order, so the code that lands is the code that was tested; a group whose `Check` fails merges nothing.

Oby (MERGING-41) and the factory (`job merge`, CI-23) hand an approved pull request to GitHub with `gh pr merge --auto --match-head-commit <head>`, which queues it pinned to the head that was reviewed. A pull request into `main` needs `Check` green, with no queue.

When the group reaches all of Rust, the queue's `rust` job runs the limits on the group's combined code (CI-35), so two pull requests that each fit can't put `dev` past a limit together. Every rule of the limits block holds there, but for a raise's payback line and its pull request of its own (CI-39, CI-40), which the pull request's own check judges against its merge base. So a group under the sleep limit fails until a pull request lowers the limit.

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
