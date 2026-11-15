# Release: a tag on main cuts the Mac release in CI

The business behavior contract for publishing the Mac app, from [the template](../business-contract-template.md). Roadmap task R09-14 (milestone R09, Ship and debug safely), after R09-13's answer ([docs/release-in-ci.md](../release-in-ci.md)).

## Feature and status

- **Outcome:** A `v*` tag on `main` builds both architectures, signs, notarizes and staples them, writes and signs `latest-mac.yml`, and uploads the zips first and the feed last. No release step depends on one developer's Mac or on the deleted Swift app (`apps/mac` is gone; its bundle scripts are `apps/electron/scripts/bundle-{obid,gh,gitleaks}.sh`).
- **Status:** RELEASE-1 to 4 are **implemented** in `.github/workflows/release.yml` and the scripts it runs, each tested on its own (`apps/electron/src/renderer/src/test/release.test.ts`). The first run on a real tag, and a Mac on the previous version updating to it, are the owner's ([q044](../decisions/2026-11-13-roadmap-answers.md#r09-14-q044): the builder makes only the test tag).
- **Scope:** The Mac app's zips, DMGs and `latest-mac.yml` on `https://downloads.oby.dev/electron` ([updates.md](updates.md)). **Out of scope:** a staged rollout; Linux archives of obid (R09-16); GitHub Releases as a second place to download from.

## Behavior

### Actors and ownership

- **The owner** moves `main` to a tested `dev` commit, pushes the `v*` tag on it, and approves the run in the `release` environment (its required reviewer).
- **`release.yml`** runs `on-main` (no secret), then `mac` in the `release` environment. obid owns nothing here; nothing changes for the apps.
- **`apps/electron/scripts/`**: `on-main.sh` (the guard), `package.sh` (build, sign, scan with `scan-app.sh`, notarize, staple, sign the feed with `sign-feed.mjs`), `upload-release.sh` (the upload).

### Rules

- **Only from `main`.** The tag's commit must be reachable from `origin/main`; otherwise `on-main` fails and `mac`, which needs it, never starts.
- **Secrets only after the owner's approval.** Every secret is a `release` environment secret, read only by `mac`, each through `env:` into a file in `RUNNER_TEMP` or a keychain of the run's, all deleted in an `always()` step.
- **Credentials from the environment in CI, the keychain on a Mac.** `package.sh` notarizes with the App Store Connect API key when `OBY_NOTARY_KEY_FILE` is set, else with the keychain profile `oby-notary`.
- **Scanned before it leaves.** Each architecture's app is scanned with gitleaks before it is notarized; a finding, or a gitleaks that can't run, stops `package.sh`, so nothing is notarized or uploaded.
- **Zips first, the feed last.** `upload-release.sh` uploads every zip `latest-mac.yml` names, and the DMGs, to `oby-downloads/electron/`, then `latest-mac.yml` with `Cache-Control: no-cache`. A feed with no signature, or naming a zip that isn't there, uploads nothing.
- **A test tag never reaches a Mac.** The tag decides where the upload goes, against `latest-mac.yml`'s `version` (`apps/electron/package.json`'s): `v<version>` is the release, under `electron/`; `v<version>-<suffix>` (such as `v0.3.0-test.1`) is a test run, under `electron-test/<tag>/`, which no Mac's `app-update.yml` names; any other tag uploads nothing. So the builder's test tag (q044) runs every step, signing, notarizing and uploading included, without touching the feed Macs read.

### Failures and recovery

- A failed run publishes nothing after its failing step; the zips it uploaded before the feed are harmless, since no Mac is sent to them. Fix, then tag a higher version.
- A bad release is withdrawn as [updates.md](updates.md) says: the last good code under a higher version, with `minimum_version` raised.

## Interfaces and dependencies

- **Secrets:** `OBY_DEVELOPER_ID_P12`, `OBY_DEVELOPER_ID_P12_PASSWORD`, `OBY_NOTARY_KEY_P8`, `OBY_NOTARY_KEY_ID`, `OBY_NOTARY_ISSUER_ID`, `OBY_FEED_KEY`, `OBY_R2_ACCESS_KEY_ID`, `OBY_R2_SECRET_ACCESS_KEY`, `OBY_R2_ENDPOINT` ([release-in-ci.md](../release-in-ci.md), "What the owner sets up"). The ChatGPT gate's `gh api` uses the job's own token (`issues: read`).
- **Runner:** GitHub's `macos-latest`: Xcode's tools, Node 22, rustup with both Darwin targets, `aws`.

## Acceptance scenarios

- **RELEASE-1** (implemented). Given a `v*` tag on main, when the workflow runs, then it publishes both zips and then a signed `latest-mac.yml`, and a Mac on the previous version sees the update. Test: `release_1_a_tag_publishes_zips_then_the_feed` (the upload's order and refusals, and the workflow packaging before it uploads); the run on a real tag and the Mac that updates are the owner's.
- **RELEASE-2** (implemented). Given a tag not on main, when the workflow runs, then it publishes nothing. Test: `release_2_a_tag_off_main_publishes_nothing` (`on-main.sh` on a repository of its own, and `mac` needing `on-main`).
- **RELEASE-3** (implemented). Given gitleaks finds something in the app, when the workflow runs, then nothing is notarized or uploaded. Test: `release_3_a_secret_in_the_app_stops_the_release` (`scan-app.sh` with a finding and with a gitleaks that can't run, and `package.sh` scanning before it notarizes).
- **RELEASE-4** (implemented). Given a test tag `v<version>-<suffix>` on main, when the workflow runs, then it uploads only under `electron-test/<tag>/`, nothing under `electron/`; a tag that is neither that nor `v<version>` uploads nothing. Test: `release_4_a_test_tag_publishes_only_to_the_test_path` (`upload-release.sh` with a test tag, a wrong version, an empty suffix, a slash and no `v`).
