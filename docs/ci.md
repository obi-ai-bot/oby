# Checks

GitHub Actions doesn't run for obi-ai-bot's repositories (there's no Actions budget), so their workflows were removed on 2026-10-23. Every check runs on a Mac: workers run the checks for what they changed, the coordinator runs the full suites before a merge (the merge ritual), and every pull request says what was run and what it printed.

## What runs

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


## Signing

The checks need no certificates. `scripts/oby-check.sh` passes these to `xcodebuild`:

```
CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= ENABLE_HARDENED_RUNTIME=NO
```

- `CODE_SIGN_IDENTITY=-` with `CODE_SIGN_STYLE=Manual`, an empty `DEVELOPMENT_TEAM` and an empty `PROVISIONING_PROFILE_SPECIFIER`: the apps are signed **ad hoc**, with no team and no profile, because CI has no certificate, team or profile to sign with.

Notarized builds of the Mac app are made on a Mac by `apps/electron/scripts/package.sh` (apps/electron/README.md), never by a check.

## Releases

Each release tag adds its own `core/crates/obid/tests/fixtures/released/<tag>/` (three jobs and a `config.json`, as the README there describes) in the release's pull request, so `tests/released_files.rs` proves today's obid still reads what that release wrote (docs/contracts/released-files.md).
