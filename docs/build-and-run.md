# Build and run

The text of the root [AGENTS.md](../AGENTS.md)'s "Build and run" that the root points to here.

- The Electron app is built, run in the window or a browser, and typechecked with `npm` in `apps/electron`, as [apps/electron/AGENTS.md](../apps/electron/AGENTS.md#build-and-run) says.
- The iPhone app: open `apps/Oby.xcodeproj` in Xcode 27 and run the `ObyPhone` scheme, or `xcodebuild -project apps/Oby.xcodeproj -scheme ObyPhone -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO build`.
- Tests: `iOSTests` (Swift Testing, hosted by the iPhone app): `xcodebuild -project apps/Oby.xcodeproj -scheme ObyPhone -destination 'platform=iOS Simulator,name=iPhone 17' test`. `tests/swift_client.rs` in obid builds their client from `apps/shared` and reads a running obid with it, so an API change that breaks them fails there.
- What changed, checked: `scripts/oby-check.sh` is this repository's check in obid (`"check": "scripts/oby-check.sh"` on `obi-ai-bot/oby` in its `config.json`, docs/runner-api.md, `repos`), so Oby's jobs here run it in their sandbox. It hands the run to the BLOK workflow ([why](decisions/2026-11-11-check-hands-off-to-blok.md)): the installed `oby-check-blok` (`cargo install --locked --path tools/oby-check-blok`, again whenever the tool changes; never built from the commit it checks) runs only the suites a change reaches (obid's touched lib modules and test binaries with fmt and clippy, the touched iOSTests suites, Electron's typecheck, tests and build), each a node of its built-in workflow, then gitleaks; without the tool, the check fails. `OBY_CHECK_PLAN=1` prints what it would run (docs/runner-api.md, "Check workflow").
- obid keeps its clones, job worktrees and data in its home, on a Mac `~/Library/Application Support/Oby/Runner`.
- The character's source for the Swift apps is `character/`. The iPhone target's build phase runs `character/package/build.sh` and bundles the clean copy (no playground) into the app. The Electron app's is `design/oby`.
- The Electron app isn't sandboxed (it runs the user's CLIs) and ships with Developer ID, not the App Store; its packaging is in [apps/electron/AGENTS.md](../apps/electron/AGENTS.md#packaging).
- `cloud/control-plane/` is Oby's control plane on Cloudflare (a Worker with D1, TypeScript, #285): `pnpm install`, `pnpm test` (vitest inside Miniflare), `pnpm check`. Deploying is the user's step (docs/remote-access.md); the Worker's secrets are never in git.
- One repository for every platform: the Mac's app is `apps/electron/`; the iPhone app (`apps/ios/app/`) is the target of `apps/Oby.xcodeproj`, with `apps/shared/`; the Apple Watch app was removed, git history its archive ([why](decisions/2026-11-13-apple-watch-app-removed.md)); `apps/android/` (phones and Wear OS, one Gradle project) and `Desktop/` (Windows and Linux) come later. Every app shows the same character from `character/`.

## Today's dev on this Mac

`scripts/run-dev.sh` puts Oby and its engine, built from one commit, on this Mac in place of what's installed, so the app you use every day is today's `dev` and not the last release. Run it from any checkout of this repository:

```bash
scripts/run-dev.sh            # today's origin/dev
scripts/run-dev.sh <ref>      # a branch, tag or commit instead
```

What it does, in order (its header says the same, step by step):

1. Fetches `origin` and checks the commit out, clean and detached, in `~/.oby-dev/src` (a worktree of this repository; `OBY_DEV_SRC` moves it). Cargo builds into `~/.oby-dev/target` (`OBY_CARGO_TARGET_DIR`), so the next run builds only what changed; `CARGO_BUILD_JOBS` is honoured.
2. Builds obid (release, for this Mac's architecture), with gh and gitleaks, through the app's own `apps/electron/scripts/helpers.sh`, and tells obid the commit (`OBY_BUILD_COMMIT`).
3. Builds the app, told the commit too, and packages it with `apps/electron/electron-builder.yml`: `Oby.app`, bundle ID `com.deskree.oby.electron`, obid at `Contents/Helpers/obid` with its launch agent's plist, as every install has it. macOS sees "Oby", not "Electron", so its notifications are allowed. It's signed with the team's Developer ID when this Mac has it, else this Mac's Apple Development identity, else ad hoc (`OBY_SIGN_IDENTITY` names another). It isn't notarized or scanned for secrets: it never leaves this Mac (`apps/electron/scripts/package.sh` makes releases).
4. Quits Oby and installs the new one as `/Applications/Oby.app`. An `Oby Electron.app` from the first releases is quit, its launch agent stopped and its folder removed, so the Mac has one Oby, named Oby. (A release's own updates still keep that old folder: docs/contracts/updates.md, UPDATES-10.)
5. Restarts obid on the new build: the app's launch agent (`com.deskree.oby.electron.obid`) when launchd has it; an obid you started by hand on 7777 is stopped, so Oby registers and starts its own (macOS may ask you to turn Oby on in System Settings › General › Login Items). install.sh's obid (`com.deskree.obid`), if there is one, is left running and not updated: Oby uses it, and the script says so. Then it opens Oby.
6. Reads obid's `GET /v1/status` with its token (from obid's home, never an argument) and fails unless obid answers on its port built from the same commit. It prints that commit.

Settings › About shows it as the version's build, `0.2.23 (150bdab)`, in the place Sangam's About keeps for one ("0.9.2 (412)"); `0.2.23 (150bdab, engine dce386c)` when obid was built from another commit. The app's commit is baked in when it's built (`apps/electron/buildInfo.ts`: `OBY_BUILD_COMMIT`, else git's `HEAD`), obid's is `runner.commit` in `GET /v1/status` (docs/api/configuration.md).

Oby keeps this Mac's device and owner keys as 0600 files in its folder, with no Keychain, so a build signed by another identity doesn't ask for your password (docs/contracts/keychain.md, KEYCHAIN-12). Only the first start after an older Oby, which sealed them in the Keychain item "oby-app Safe Storage", reads that item once to move them, and macOS asks only if the item doesn't trust the new build's signature (KEYCHAIN-13). If launchd then refuses the engine (exit 78, a registration macOS kept from the other signature), Oby registers it again once by itself (docs/contracts/engine.md); `~/Library/Logs/obid.log` says what happened. Going back to a release: install its DMG over it, or let Oby update itself (About › Check for Updates).

### A scratch install, beside the real one

```bash
scripts/run-dev.sh <ref> --prefix <folder> --home <folder>/runner/obid-home --port 7901
scripts/run-dev.sh --stop --prefix <folder> --home <folder>/runner/obid-home
```

With `--prefix`, `--home` and `--port` (anything but 7777, never `/Applications`), it installs `<prefix>/Oby.app` and starts an obid of its own from that app, on that port with that home (its `config.json`, `obid.log` and `obid.pid` beside the home), and leaves the installed Oby, its engine and its launch agent alone. That build has its port and home baked in (`OBY_SCRATCH_PORT`, `OBY_SCRATCH_HOME`: `apps/electron/src/shared/build.ts`), never read at run time: it registers no launch agent or login item, takes no `oby://` links, doesn't update itself, and keeps its own app folder (`app-data` beside the home) and Keychain item. `--stop` quits it and its obid; its files stay until you remove them.
