# Oby's own updates

The business behavior contract for Settings › About's updater rows in the Electron app (Sangam's design, [#1564](https://github.com/obi-ai-bot/oby/pull/1564): "Update automatically", "Check for Updates", "Oby X is available", "Update and Restart", "Oby is up to date"), from [the template](../business-contract-template.md). Issue: [#1577](https://github.com/obi-ai-bot/oby/issues/1577). Scenarios are numbered `UPDATES-n`; the tests that prove them are named after them (`updates_1_…`, `apps/electron/src/renderer/src/test/updates.test.ts`).

## Feature and status

- **Outcome:** The owner keeps Oby current without thinking about it, or, with Update automatically off, decides when; an update that isn't Deskree's, or isn't what was published, never installs.
- **Status:** **Implemented** in the Electron app (`apps/electron`): the decisions in `src/shared/updates.ts`, carried out by `src/main/updates.ts` with electron-updater; About's rows in `SettingsView.tsx` (`Updates`). Publishing a release (below) is the owner's step.
- **Scope:** the Mac app's own build (the Electron app, obid inside it). **Out of scope:** the Swift Mac app's Sparkle feed on `downloads.oby.deskree.com` (it stays, for installed 0.4.5 Macs), the iPhone and Watch (the App Store and TestFlight), Windows and Linux, a release channel or a beta, and Sangam's later screens (design/app: the island offering to restart; waiting for a runner's check before a restart).

## Behavior

### Actors and ownership

- **The owner** turns Update automatically on or off, clicks Check for Updates and Update and Restart.
- **The Electron app's main process** decides and acts (`shared/updates.ts`'s `next`), keeps Update automatically in the app's own folder (`settings.json`'s `updatesAuto`, next to Notifications' `notifications`), and tells About what to show (`update` events; the `updates` call answers `{auto, update}`).
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
| Check for Updates (disabled) | `checking` | Nothing newer → `current`; newer → `downloading` (on) or `available` (off); a failure → `idle`. |
| Check for Updates · Oby is up to date | `current` | Check for Updates stays, so the owner can look again at any time (Wellington, 2026-10-30); with it on, the 6-hour look runs again. |
| Oby X is available, Released Mon D · Update and Restart | `available` | Update and Restart → `downloading` and restarts once it's downloaded and checked; turning Update automatically on downloads it. |
| Oby X is available · Update and Restart | `downloading` | Downloaded and its checksum matches → `ready`. Update and Restart while it downloads restarts once it's ready (the button disables). |
| Oby X is available · Update and Restart | `ready` | Update and Restart → Oby quits and opens the new build. Otherwise it installs when Oby quits. |

The footer "Oby downloads updates in the background and installs them when you quit." shows while Update automatically is on (Sangam's v1 words).

### Failures and recovery

- Offline, the feed or a zip missing, a file off the feed's host, a checksum that doesn't match `latest-mac.yml`, or a signature that doesn't meet the running app's: nothing installs, About shows Check for Updates again, and the reason goes to main's log. With Update automatically on, the next 6-hour look tries again.
- A click while Oby already looks or downloads starts nothing twice.
- Turning Update automatically off once an update is downloaded doesn't take it back: Squirrel.Mac already holds it and installs it when Oby quits.

### Invariants and boundaries

- **Verified before it installs.** electron-updater refuses a zip whose sha512 isn't `latest-mac.yml`'s (it refuses a feed with no checksum at all); Squirrel.Mac installs a bundle only if its code signature meets the running app's designated requirement (Deskree's Developer ID team, `com.deskree.oby.electron`). Neither can be turned off from the app.
- **Never a lower version number, never a pre-release:** `allowDowngrade` and `allowPrerelease` are false. That compares the numbers `latest-mac.yml` states, nothing more: a build is trusted for its Deskree signature and its checksum, so someone who can write to the bucket could serve an older, genuinely signed build under a higher number, and Macs would install it. Signing the feed itself (ed25519, a key the app carries) would close that, and is a follow-up of its own.
- **https only, one host:** the feed is `https://downloads.oby.dev/electron`, set in main's code (`FEED`), never read from the page. Every file `latest-mac.yml` names, resolved against the feed as electron-updater resolves it, must be https on `downloads.oby.dev` (no other host or port, no `http:` or `file:`), or the update is refused before anything downloads (`onFeed`). The bundle's `app-update.yml` names the same feed (a test keeps them equal); electron-updater reads its cache folder's name there.
- **Privacy first:** the look is one anonymous GET of `latest-mac.yml` (and the zip when there's one); no account, identifier or telemetry. With Update automatically off, nothing goes out until the owner clicks.
- Only the window's own page (main's `trusted`) can ask main to check, install or change the setting.

## Interfaces and dependencies

- **Renderer ↔ main (preload `window.app`):** `updates(ask?)` with `'check'`, `'install'` or a boolean (Update automatically) answers `{auto, update}`; `onUpdate(fn)` gets each new `update`. The page is the only caller.
- **electron-updater 6.8.12**, bundled into main (the app ships no `node_modules`), the `generic` provider, whole-zip downloads (no blockmaps). It needs `Contents/Resources/app-update.yml` (`apps/electron/build/app-update.yml`, through electron-builder.yml's `extraResources`) before any download, even with the feed set in code; `package.sh` refuses to notarize an app without it.
- **What `scripts/package.sh` makes** (after notarizing and stapling): `dist/<name>-<version>-<arch>-mac.zip` per architecture (the arm64 one has `arm64` in its name: the updater picks it on Apple silicon, the other elsewhere) and `dist/latest-mac.yml` (version, each zip's sha512 and size, release date). The version is `apps/electron/package.json`'s.

### Publishing a release (the owner's step)

1. `apps/electron/scripts/package.sh` (both architectures: a feed with one leaves the other's Macs without an update).
2. Upload both zips to the `oby-downloads` R2 bucket under `electron/`, then `latest-mac.yml` there last (no Mac is sent to a zip that isn't there yet). `latest-mac.yml` with `Cache-Control: no-cache`, so a new release is seen at the next look.
3. The DMGs where downloads should point.

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

### Test it when it's done (on a real Mac)

1. Install a packaged build of version N from `downloads.oby.dev`; publish N+1 (above). Within a launch, About shows "Oby N+1 is available"; quit and open: About's version is N+1, and obid answers.
2. Turn Update automatically off and relaunch: no request reaches `downloads.oby.dev` until Check for Updates.
3. Publish a `latest-mac.yml` whose sha512 doesn't match its zip: nothing installs, About shows Check for Updates.
4. Publish a zip signed with another identity (ad hoc): Squirrel.Mac refuses it, nothing installs.

Checked in this change: `updates.test.ts` (the nine scenarios), the build bundling electron-updater into main, and an unpackaged run of the app on this Mac showing About's rows on the real bridge with `updatesAuto` kept in `settings.json`. The real-Mac steps above wait for the first release on `downloads.oby.dev`.
