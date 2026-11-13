# Checks

Checks run in two places: GitHub's `Linux` workflow (`.github/workflows/linux.yml`) and the Mac's `scripts/oby-check.sh`. Every pull request says what was run and what it printed. This page is what runs today (docs/contracts/ci.md, CI-1 and CI-2); where CI is going, a new workflow and the merge queue on `dev`, is [docs/ci-architecture.md](ci-architecture.md).

## What GitHub runs

- **A ready pull request** starts the `Linux` workflow by itself on its head, when it is opened, reopened, pushed or marked ready. It runs the secret scan (gitleaks over the pull request's commits, with `.gitleaks.toml`). When the change reaches Rust, it also runs `cargo fmt --check` and `cargo clippy -D warnings`. It runs no tests: no `cargo test`, no `make e2e`, no control-plane tests.
- **What reaches Rust.** A pull request whose files are all under `docs/` (but not `docs/api/`), `factory/`, `.claude/`, `design/`, `character/`, `apps/ios/`, `apps/watch/` or `cloud/`, or other Markdown, runs only the secret scan. `docs/api/`, `core/`, `.github/` and any other file reach Rust. `cloud/control-plane/` and `.github/` reach the control-plane job, which runs only with the tests (nightly, on demand, or `full-ci`).
- **A draft's jobs are skipped**, and a skipped job reports success. Marking a draft ready runs it on the same head.
- **The `full-ci` label** runs everything the change reaches, tests included, from the next push after labelling (a draft with it runs too).
- **A newer push to the same pull request cancels the run it supersedes.**
- **Nightly at 03:17 UTC on `dev`, and on demand** (`workflow_dispatch`, for example `gh workflow run linux.yml --ref dev`), every job runs to the end: fmt, clippy, `cargo test`, the secret scan, the control plane's `pnpm test` and `pnpm check`, and `make e2e` (stub agents, against the oby-sphere commit in `e2e/oby-sphere.ref`).
- **A push to `dev` or `main` starts no run.** The factory merges a pull request as soon as it is ready, so nobody read those runs.
- **Caches.** Only the nightly and on-demand runs save the Rust caches (`Swatinem/rust-cache`'s `save-if`). Every other run only restores them, because a pull request's caches are scoped to `refs/pull/<n>/merge` and no other pull request can read them. The pnpm cache from `actions/setup-node` has no save switch and is left as it is, because it is small.

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

## Signing

The checks need no certificates. `scripts/oby-check.sh` passes these to `xcodebuild`:

```
CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= ENABLE_HARDENED_RUNTIME=NO
```

- `CODE_SIGN_IDENTITY=-` with `CODE_SIGN_STYLE=Manual`, an empty `DEVELOPMENT_TEAM` and an empty `PROVISIONING_PROFILE_SPECIFIER`: the apps are signed **ad hoc**, with no team and no profile, because CI has no certificate, team or profile to sign with.

Notarized builds of the Mac app are made on a Mac by `apps/electron/scripts/package.sh` (apps/electron/README.md), never by a check.

## Releases

Each release tag adds its own `core/crates/obid/tests/fixtures/released/<tag>/` (three jobs and a `config.json`, as the README there describes) in the release's pull request, so `tests/released_files.rs` proves today's obid still reads what that release wrote (docs/contracts/released-files.md).
