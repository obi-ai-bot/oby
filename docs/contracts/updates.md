# Oby's own updates

The business behavior contract for Settings › About's updater rows in the Electron app (Sangam's design, [#1564](https://github.com/obi-ai-bot/oby/pull/1564): "Update automatically", "Check for Updates", "Oby X is available", "Update and Restart", "Oby is up to date"), from [the template](../business-contract-template.md). Issue: [#1577](https://github.com/obi-ai-bot/oby/issues/1577). Scenarios are numbered `UPDATES-n`; the tests that prove them are named after them (`updates_1_…`, `apps/electron/src/renderer/src/test/updates.test.ts`).

## Feature and status

- **Outcome:** The owner keeps Oby current without thinking about it, or, with Update automatically off, decides when; an update that isn't Deskree's, or isn't what was published, never installs.
- **Status:** **Implemented** in the Electron app (`apps/electron`): the decisions in `src/shared/updates.ts`, carried out by `src/main/updates.ts` with electron-updater; About's rows in `SettingsView.tsx` (`Updates`). Publishing a release (below) is a `v*` tag on `main`, cut in CI ([release.md](release.md)).
- **Scope:** the Mac app's own build (the Electron app, obid inside it). **Out of scope:** the Swift Mac app's Sparkle feed on `downloads.oby.deskree.com` (the app was deleted from this repository on 2026-11-02; the feed stays, for installed 0.4.5 Macs), the iPhone and Watch (the App Store and TestFlight), Windows and Linux, a release channel or a beta, and Sangam's later screens (design/app: the island offering to restart; waiting for a runner's check before a restart).

## Behavior

### Actors and ownership

- **The owner** turns Update automatically on or off, clicks Check for Updates and Update and Restart.
- **The Electron app's main process** decides and acts (`shared/updates.ts`'s `next`), keeps Update automatically and the `issued` of the last feed it accepted in the app's own folder (`settings.json`'s `updatesAuto` and `updatesIssued`, next to Notifications' `notifications` and the page's local Settings rows under `local`; `src/main/settings.ts` owns the file, [live-state.md](live-state.md)), and tells About what to show (`update` events; the `updates` call answers `{auto, update}`).
- **obid** owns nothing here: it can't replace the app it runs inside, and the setting is this Mac's app's. obid is inside the app bundle (`Contents/Helpers/obid`), so an update brings its obid too.
- **downloads.oby.dev** (`https://downloads.oby.dev/electron`) serves `latest-mac.yml` and the zips `scripts/package.sh` makes.

### Triggers and prerequisites

- The app is packaged (a development run never looks: About goes back to Check for Updates).
- With Update automatically on (the default): Oby looks at launch and every 6 hours (Sangam's design/app).
- With it off: Oby never looks by itself. Only the owner's Check for Updates reaches the network.

### States and transitions

| About shows | State | What changes it |
|---|---|---|
| Check for Updates | `idle` | Launch; a failure. Check for Updates → `checking`. |
| Check for Updates (disabled) | `checking` | Nothing newer → `current`; newer → `downloading` (on) or `available` (off); a failure, or 90 s with no answer → `idle`. |
| Check for Updates · Oby is up to date | `current` | Check for Updates stays, so the owner can look again at any time (Wellington, 2026-10-30); with it on, the 6-hour look runs again. |
| Oby X is available, Released Mon D · Update and Restart | `available` | Update and Restart → `downloading` and restarts once it's downloaded and checked; turning Update automatically on downloads it. |
| Oby X is available · Update and Restart | `downloading` | Downloaded and its checksum matches → `ready`. Update and Restart while it downloads restarts once it's ready (the button disables). 120 s with no progress → `idle`, the download cancelled. |
| Oby X is available · Update and Restart | `ready` | Update and Restart → Oby quits and opens the new build; a second click in the same session does nothing, and if Oby is still running 300 s later → `idle`. Otherwise it installs when Oby quits. |

The footer "Oby downloads updates in the background and installs them when you quit." shows while Update automatically is on (Sangam's v1 words).

### Failures and recovery

- Offline, the feed or a zip missing, a file off the feed's host, a checksum that doesn't match `latest-mac.yml`, or a signature that doesn't meet the running app's: nothing installs, About shows Check for Updates again, and the reason goes to main's log. With Update automatically on, the next 6-hour look tries again.
- A feed Deskree didn't sign is refused the same way, before anything downloads, and main's log line is `update: check_failed refused the feed: <why>`: no signature, or one that isn't the last line; a bad signature; bytes main read that don't parse to what electron-updater parsed ("not the feed that was checked"; a CDN serving two copies between the two reads makes one look fail, and the next tries again); an unreadable `issued`, `version` or `minimumVersion`; an `issued` before the last one this Mac accepted ("a replay"); a `version` under its own `minimumVersion`; or `latest-mac.yml` that can't be read again ("couldn't read it (<status>)"). About shows nothing new for it.
- A missing or unreadable `updatesIssued` counts as none: any signed feed is accepted, and its `issued` kept. One that can't be kept is logged (`update: couldn't keep updatesIssued`) and the update goes on.
- A click while Oby already looks or downloads starts nothing twice.
- Every busy phase has a deadline (`DEADLINES` in `shared/updates.ts`), and main holds one timer for the phase in force: a look with no answer fails after 90 s (`check_failed`); a download fails after 120 s with no progress event, counted from the last one or from its start, and is cancelled (`download_failed`); an install fails if Oby is still running 300 s after Update and Restart (`install_failed`). About then shows Check for Updates again.
- Every failure has its code, `check_failed`, `download_failed` or `install_failed`, and main's log line names it first, then the reason (the updater's own message, or the deadline). An updater error during a download is `download_failed`, on a downloaded update `install_failed`, and anywhere else `check_failed`. An answer that comes after its deadline fired is taken as any other, and the state it leads to gets its own deadline.
- Update and Restart runs the install once per session: main keeps a latch, set when it asks to restart and freed only when the state leaves `ready` (an `install_failed`), so a later update can still be installed by a click.
- Turning Update automatically off once an update is downloaded doesn't take it back: Squirrel.Mac already holds it and installs it when Oby quits.

### Invariants and boundaries

- **Verified before it installs.** electron-updater refuses a zip whose sha512 isn't `latest-mac.yml`'s (it refuses a feed with no checksum at all); Squirrel.Mac installs a bundle only if its code signature meets the running app's designated requirement (Deskree's Developer ID team, `com.deskree.oby.electron`). Neither can be turned off from the app.
- **Signed by Deskree.** `latest-mac.yml`'s last line is `signature: '<base64>'`: an ed25519 signature, by Deskree's feed key, over the file's bytes without that line. No other line starts with `signature:`. The app carries the public key (`FEED_KEY` in `src/shared/updates.ts`, the owner's key, copied byte for byte); before an update counts as found, main reads `latest-mac.yml` again, checks those bytes, and requires them to parse (with the same `js-yaml` electron-updater uses) to exactly what electron-updater parsed (`verifyFeed`). It refuses a feed with no signature or a bad one, an `issued` before the last one this Mac accepted (the same `issued` is accepted, so a look at an unchanged feed still works), and a `version` under the feed's own `minimumVersion`, compared number by number. The private key is never in the repository, the bucket or a log: `scripts/sign-feed.mjs` reads it from the file `OBY_FEED_KEY_FILE` names, outside the repository, and signs only when it matches `FEED_KEY`.
- **Never a lower version number, never a pre-release:** `allowDowngrade` and `allowPrerelease` are false, so a Mac never installs a version under the one it runs. Someone who can write to the bucket can't sign a feed, so they can't serve an older build under a higher number, nor replay an older signed feed.
- **https only, one host:** the feed is `https://downloads.oby.dev/electron`, set in main's code (`FEED`), never read from the page. Every file `latest-mac.yml` names, resolved against the feed as electron-updater resolves it, must be https on `downloads.oby.dev` (no other host or port, no `http:` or `file:`), or the update is refused before anything downloads (`onFeed`). The bundle's `app-update.yml` names the same feed (a test keeps them equal); electron-updater reads its cache folder's name there.
- **Privacy first:** the look is an anonymous GET of `latest-mac.yml`, a second one by main when it names another version than this one (to check its signature), and the zip when there's one; no account, identifier or telemetry. With Update automatically off, nothing goes out until the owner clicks.
- Only the window's own page (main's `trusted`) can ask main to check, install or change the setting.

## Interfaces and dependencies

- **Renderer ↔ main (preload `window.app`):** `updates(ask?)` with `'check'`, `'install'` or a boolean (Update automatically) answers `{auto, update}`; `onUpdate(fn)` gets each new `update`. The page is the only caller. An `idle` update may carry `error` (`check_failed`, `download_failed`, `install_failed`), which About doesn't show.
- **electron-updater 6.8.12**, bundled into main (the app ships no `node_modules`), the `generic` provider, whole-zip downloads (no blockmaps). It needs `Contents/Resources/app-update.yml` (`apps/electron/build/app-update.yml`, through electron-builder.yml's `extraResources`) before any download, even with the feed set in code; `package.sh` refuses to notarize an app without it.
- **What `scripts/package.sh` makes** (after notarizing and stapling): `dist/<name>-<version>-<arch>-mac.zip` per architecture (the arm64 one has `arm64` in its name: the updater picks it on Apple silicon, the other elsewhere) and `dist/latest-mac.yml` (version, each zip's sha512 and size, release date, `issued` (the UTC second it was written, `YYYY-MM-DDTHH:MM:SSZ`), `minimumVersion` (`minimum_version` in `package.sh`), and last `signature`). The version is `apps/electron/package.json`'s; `package.sh` stops before building when it's under `minimum_version`, or when the feed key is missing or isn't the one the app carries.

### Publishing a release

The owner pushes the tag `v<apps/electron/package.json's version>` on `main` and approves the run (a tag `v<version>-<suffix>` is a test run, published under `electron-test/<tag>/`, where no Mac looks); `.github/workflows/release.yml` does the rest ([release.md](release.md)):

1. `apps/electron/scripts/package.sh arm64 x64`, with the feed key's file as `OBY_FEED_KEY_FILE` (both architectures: a feed with one leaves the other's Macs without an update).
2. `apps/electron/scripts/upload-release.sh`: both zips and the DMGs to the `oby-downloads` R2 bucket under `electron/`, then `latest-mac.yml` there last (no Mac is sent to a zip that isn't there yet), with `Cache-Control: no-cache`, so a new release is seen at the next look.
3. To withdraw a bad release: publish the last good code under a higher version, with `minimum_version` in `package.sh` raised to that version, never by a downgrade (`allowDowngrade` stays false).

## Acceptance scenarios

- **UPDATES-1** With Update automatically on, Oby looks at launch and every 6 hours; a newer build downloads by itself and installs when Oby quits.
- **UPDATES-2** With it off, Oby never looks by itself; Check for Updates looks; a newer build shows "Oby X is available", and Update and Restart downloads it and restarts into it once it's checked.
- **UPDATES-3** Nothing newer: "Oby is up to date".
- **UPDATES-4** A failure (offline, a bad checksum, a signature that doesn't match) installs nothing and offers Check for Updates again; with Update automatically on, the next 6-hour look tries again.
- **UPDATES-5** The feed is `https://downloads.oby.dev/electron`, and the bundle's `app-update.yml` names the same one; never a lower version number or a pre-release.
- **UPDATES-6** Update and Restart on a downloaded update restarts into it now; clicked while it downloads, it restarts once it's ready.
- **UPDATES-7** Turning Update automatically on looks now, or downloads what was found; off, Oby stops looking. It's kept across launches.
- **UPDATES-8** A click or a look while Oby already looks or downloads starts nothing twice.
- **UPDATES-9** A `latest-mac.yml` naming a file anywhere but https on `downloads.oby.dev` is refused: nothing downloads, About offers Check for Updates again.
- **UPDATES-10** An update never moves the app. Squirrel.Mac renames an installed bundle after the update's executable (`renamedTargetIfNeededWithTargetURL`) and relaunches from where ShipIt was, so the executable in every build stays `Oby Electron`: `/Applications/Oby Electron.app` keeps its folder and Update and Restart reopens it, its obid and login item stay valid, and the menu says Oby. New installs are `Oby.app`, and stay there. Finder and Spotlight go by the file name, so updated installs may still show the old name there.
- **UPDATES-11** (implemented) Given a `latest-mac.yml` with no signature, a signature that doesn't verify against the key the app carries, or bytes that aren't what electron-updater parsed, when Oby looks, then nothing downloads and About offers Check for Updates again.
- **UPDATES-12** (implemented) Given this Mac accepted a feed with `issued` T, when a feed with `issued` strictly before T arrives, even correctly signed, then it is refused as a replay; one issued at T is accepted.
- **UPDATES-13** (implemented) Given a signed feed whose `version` is under its own `minimumVersion`, when Oby looks, then it is refused.
- **UPDATES-14** (implemented) Given a correctly signed feed no older than the last, when it is accepted, then its `issued` is kept in `settings.json` (`updatesIssued`, every other key kept) and survives a relaunch.
- **UPDATES-15** (implemented) Given Oby is `checking`, when 90 s pass with no answer, then About shows Check for Updates and main's log says `check_failed`; an answer that comes later is taken, and its state gets its own deadline.
- **UPDATES-16** (implemented) Given Oby is `downloading`, when 120 s pass with no progress event, then the download is cancelled and ends with `download_failed`; each progress event gives it 120 s again.
- **UPDATES-17** (implemented) Given Update and Restart was clicked on a `ready` update, when it is clicked again in the same session, then nothing more happens; if Oby is still running 300 s later it ends with `install_failed`, and the next update installs on one click again.

### Test it when it's done (on a real Mac)

1. Install a packaged build of version N from `downloads.oby.dev`; publish N+1 (above). Within a launch, About shows "Oby N+1 is available"; quit and open: About's version is N+1, and obid answers.
2. Turn Update automatically off and relaunch: no request reaches `downloads.oby.dev` until Check for Updates.
3. Publish a `latest-mac.yml` whose sha512 doesn't match its zip: nothing installs, About shows Check for Updates.
4. Publish a zip signed with another identity (ad hoc): Squirrel.Mac refuses it, nothing installs.
5. On a Mac with `/Applications/Oby Electron.app` running obid, click Update and Restart for the renamed build: Oby reopens by itself from the same folder, its menu says Oby, obid answers and its pid is restarted once idle, the Obysphere and paired phones are still there, and macOS asks for no Keychain password. After a restart of the Mac, obid starts.
6. Install the renamed build from its DMG on a clean Mac, then update it: it stays at `/Applications/Oby.app` and reopens.
7. Publish a `latest-mac.yml` signed with another key: nothing installs, About shows Check for Updates.

Checked in this change: `updates.test.ts` (the scenarios, UPDATES-11 to 14 with throwaway keys and `scripts/sign-feed.mjs`), the build bundling electron-updater and js-yaml into main, and an unpackaged run of the app on this Mac showing About's rows on the real bridge with `updatesAuto` kept in `settings.json`. The real-Mac steps above wait for the first release on `downloads.oby.dev`.
