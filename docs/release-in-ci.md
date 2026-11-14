# Releasing the Mac app from a tag in CI

This is the answer to roadmap task R09-13, for R09-14 (the release workflow, `.github/workflows/release.yml`, on `v*` tags of `main`). It lists what a release needs today, records the owner's decisions on key custody, CI signing and the dev-to-main cadence, names what the owner sets up in GitHub, and leaves R09-14 the open points it needs answered.

Nothing here ran on a runner. The signing test the task asked for needs the owner's certificate and notary key, which the factory never creates, reads or stores, so R09-14's test tag proves the method below instead (q044).

## What a release needs today

A release is `apps/electron/scripts/package.sh` (zsh), run by hand on one Mac. It runs `apps/electron/scripts/helpers.sh`, which runs `apps/mac/app/Tools/bundle-{obid,gh,gitleaks}.sh`.

Secrets and logins:

- **The Developer ID Application identity in the login keychain.** `package.sh` line 50 and `helpers.sh` line 22 look it up with `security find-identity -v -p codesigning`, filtered to the team `OBY_TEAM_ID` names (`package.sh` line 40, `helpers.sh` line 21; a default is set there). electron-builder signs with `OBY_SIGN_IDENTITY`, passed as `CSC_NAME` (`package.sh` lines 43–44; `electron-builder.yml` names none). The bundle scripts sign with it as `EXPANDED_CODE_SIGN_IDENTITY` (`helpers.sh` line 29).
- **The `oby-notary` keychain profile.** `package.sh` line 39 reads `OBY_NOTARY_PROFILE` (default `oby-notary`), and line 55 runs `xcrun notarytool submit … --keychain-profile "$profile" --wait`. The profile holds the notary credentials in the login keychain.
- **A `gh` login that can read this repository, for the ChatGPT gate.** `package.sh` lines 27–37 read `src/renderer/src/chatgptGate.ts`. When it names a report, line 35 confirms the comment exists with `gh api repos/obi-ai-bot/oby/issues/comments/<id>`. Left empty, the gate needs nothing.
- **The feed key, once R09-01 lands.** R09-01 (`factory/queue/tasks/R09-01.md`, not yet built) makes `package.sh` sign `latest-mac.yml` with an ed25519 private key read from the file `OBY_FEED_KEY_FILE` names.
- **Write access to the `oby-downloads` R2 bucket.** Uploading is the owner's step, by hand (`package.sh` lines 15–16; `docs/contracts/updates.md`, "Publishing a release"): both zips to the bucket under `electron/` first, then `latest-mac.yml` last with `Cache-Control: no-cache`. The updater reads them at `https://downloads.oby.dev/electron`.

Machine dependencies:

- **Xcode's tools.** `codesign` (`package.sh` lines 74, 76 and 108; every bundle script's last step), `xcrun notarytool` (line 55), `xcrun stapler staple` and `stapler validate` (lines 57–58), `hdiutil create` and `hdiutil verify` (lines 106 and 110), `spctl` (lines 111–112), and `lipo` (`package.sh` line 70; `bundle-obid.sh` line 43, `bundle-gh.sh` line 42, `bundle-gitleaks.sh` line 45). Also `ditto` (lines 96, 103 and 119).
- **cargo with both Darwin targets.** `apps/mac/app/Tools/bundle-obid.sh` line 18 runs `cargo build --release -p obid`, for `aarch64-apple-darwin` and `x86_64-apple-darwin` (line 24). A release build stops without both (lines 38–41: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`). It runs cargo with `PATH` set to `~/.cargo/bin` and Homebrew's folders only (line 17). `OBY_CARGO_TARGET_DIR` keeps a target folder between releases (`helpers.sh` line 8).
- **node and node-gyp for both architectures.** `package.sh` line 47 reads the version with `node`, line 64 runs `npm run build` (electron-vite), line 69 runs `npx node-gyp rebuild --arch="$arch"` against Electron's headers from `electronjs.org` for each architecture, and line 72 runs `npx electron-builder --mac dir`. `npm install` in `apps/electron` comes first.
- **gh 2.102.0, pinned.** `apps/mac/app/Tools/bundle-gh.sh` line 11 pins it, and line 28 downloads it from GitHub's releases with `curl`, checked against the SHA-256 on lines 13–14.
- **gitleaks 8.30.1, pinned.** `apps/mac/app/Tools/bundle-gitleaks.sh` line 13 pins it, and line 31 downloads it the same way, checked against lines 15–16. It is signed with one entitlement (lines 61–72). `package.sh` line 78 checks the app's copy says `8.30.1`, and lines 82–92 scan the app with it before anything is sent to Apple.
- **A download cache.** Both download scripts cache under `~/Library/Caches/com.deskree.oby/` (`bundle-gh.sh` line 15, `bundle-gitleaks.sh` line 17). A fresh runner starts with it empty, so it downloads every time.
- **Small tools.** `openssl` and `stat -f` for `latest-mac.yml`'s hashes and sizes (`package.sh` lines 131–132), `git` for `helpers.sh`'s stamp (line 16), `curl`, `shasum`, `unzip` and `tar` in the bundle scripts.

What it makes: `dist/<name>-<version>-<arch>.dmg`, `dist/<name>-<version>-<arch>-mac.zip` for each architecture, and `dist/latest-mac.yml`.

### Stale references

- `package.sh` lines 2–3 and 18 cite `docs/release.md`, and lines 2 and 83 and `helpers.sh` line 8 cite `scripts/release.sh`. Neither file exists any more: both were the deleted Swift app's.
- `package.sh` line 2 says it runs "on this Mac (no CI)". That stays true until R09-14.
- The task names `.github/workflows/linux.yml`, which doesn't exist. The repository's only workflow is `.github/workflows/ci.yml`, and every job in it runs on `ubuntu-latest`.
- The task says AGENTS.md calls GitHub Actions something that doesn't run for this repository. AGENTS.md no longer says so, and Actions does run: `docs/ci.md`, "What GitHub runs", and GitHub's merge queue needs its `Check` for every merge into `dev`.
- The task says `main` is 322 commits behind. On 2026-11-14, `origin/dev` has 448 commits that `origin/main` lacks, and `origin/main` has 1 commit that `dev` lacks: the merge commit `a72f3905` ("Merge pull request #1706 from obi-ai-bot/dev", 2026-11-01). Until that commit is in `dev`, `main` can't be fast-forwarded to `dev`.

R09-14 fixes these lines; this task changes none of them.

## Decided by the owner

The owner answered these on 2026-11-13, so this document records them and asks nothing about them again.

- **Custody (q041):** the release's signing credentials live in GitHub environment secrets with a required reviewer, not only on the owner's Mac. [R09-13 (q041)](decisions/2026-11-13-roadmap-answers.md#r09-13-q041)
- **Cadence (q042):** `dev` goes to `main` only when the owner calls for it. There is no schedule. [R09-13 (q042)](decisions/2026-11-13-roadmap-answers.md#r09-13-q042)
- **CI signs (q043):** the owner lets a CI runner use the Apple Developer ID certificate and adds the secrets under the names this document gives. The owner asked for it the way OpenBot does: a tag on `main` cuts the release in CI, and a publish job puts the build on GitHub Releases. [R09-13,R09-14 (q043)](decisions/2026-11-13-roadmap-answers.md#r09-13r09-14-q043)
- **Tags (q044):** the builder creates only the test tag, and the owner creates the real release tag. [R09-14 (q044)](decisions/2026-11-13-roadmap-answers.md#r09-14-q044)

## What the owner sets up

**One GitHub environment, `release`** (the repository's Settings › Environments):

- Its required reviewer is the owner, so a run waits for the owner's approval before any job that uses the environment starts, and before it can read a secret.
- Its deployment rule allows only tags matching `v*` ("Selected branches and tags", a tag rule `v*`), so no branch run can use it.

**Every secret is an environment secret of `release`, never a repository or organization secret.** A pull request's run and a merge-queue run (`ci.yml`) never name the `release` environment, so neither can read one, and a run that names it waits for the owner and is refused outside a `v*` tag.

| Secret | Its value |
| --- | --- |
| `OBY_DEVELOPER_ID_P12` | The Developer ID Application certificate with its private key, exported as a `.p12` and base64-encoded. |
| `OBY_DEVELOPER_ID_P12_PASSWORD` | That `.p12`'s password. |
| `OBY_NOTARY_KEY_P8` | The App Store Connect API key's `.p8` file, the text as downloaded. |
| `OBY_NOTARY_KEY_ID` | That API key's key id. |
| `OBY_NOTARY_ISSUER_ID` | That API key's issuer id. |
| `OBY_FEED_KEY` | R09-01's ed25519 private key for `latest-mac.yml`. The workflow writes it to a file in the runner's temporary folder and passes that file's path as `OBY_FEED_KEY_FILE`. |
| `OBY_R2_ACCESS_KEY_ID` | The access key id of an R2 API token that can write only the `oby-downloads` bucket. |
| `OBY_R2_SECRET_ACCESS_KEY` | That token's secret access key. |
| `OBY_R2_ENDPOINT` | The S3 endpoint of the account that holds `oby-downloads`. |

The team id stays where it is today: `OBY_TEAM_ID`'s default in `package.sh` and `helpers.sh`. It isn't a secret.

**The ChatGPT gate needs no new secret.** The comment it confirms is on this repository, so the job's own `GITHUB_TOKEN`, with `permissions: contents: read` and `issues: read`, passed to `gh` as `GH_TOKEN`, answers `gh api`. No personal token is stored.

This document holds names only: no secret's value, no key or certificate, no account id, email or person's name. The owner creates each secret in GitHub's settings; nobody else sees a value.

## How a macOS runner signs and notarizes

**Not yet run in this repository.** This is the documented method for a GitHub-hosted macOS runner. R09-14 builds it into `release.yml`, and its test tag (q044) is the first run that proves it.

1. **A temporary keychain.** Generate a random password in the run (for example `openssl rand -base64 32`, kept in a shell variable and never printed). `security create-keychain -p "$pw" "$RUNNER_TEMP/release.keychain-db"`, then `security set-keychain-settings -lut 21600` on it and `security unlock-keychain -p "$pw"` on it.
2. **Import the `.p12`, for `codesign` only.** Decode `OBY_DEVELOPER_ID_P12` with `base64 --decode` into `$RUNNER_TEMP`, then `security import <file>.p12 -k "$RUNNER_TEMP/release.keychain-db" -P "$OBY_DEVELOPER_ID_P12_PASSWORD" -T /usr/bin/codesign`, so only `codesign` may use the key. `security set-key-partition-list -S apple-tool:,apple: -s -k "$pw"` on the keychain lets it sign with no prompt. Delete the decoded `.p12` right after.
3. **Add it to the search list.** `security list-keychains -d user -s "$RUNNER_TEMP/release.keychain-db" <the keychains already listed>`, so `security find-identity -v -p codesigning` (`package.sh` line 50, `helpers.sh` line 22) and electron-builder's `CSC_NAME` find the identity there, as they find the login keychain's on the owner's Mac.
4. **Notarize with the API key.** Write `OBY_NOTARY_KEY_P8` to a file in `$RUNNER_TEMP`, readable only by the runner's user, and run `xcrun notarytool submit … --key <that file> --key-id "$OBY_NOTARY_KEY_ID" --issuer "$OBY_NOTARY_ISSUER_ID" --wait` in place of `--keychain-profile`. `stapler staple`, `stapler validate` and `spctl` need no credentials. R09-01's feed key is written the same way, to the file `OBY_FEED_KEY_FILE` names.
5. **Clean up in an `always()` step.** `security delete-keychain "$RUNNER_TEMP/release.keychain-db"`, and delete the `.p12`, `.p8` and feed key files, whether the run passed, failed or was cancelled.
6. **Echo no secret.** Pass secrets through `env:` and never on a command line that `set -x` or an error message could print; never `echo` one. GitHub masks a secret's value in the log, but a value it has transformed (a decoded `.p12`, a key file's text) isn't masked, so nothing transformed is printed either.

**The one change `package.sh` needs** (R09-14's work, not done here): `notarize()` (line 55) takes the notary credentials from the environment in CI (the API key's file, `OBY_NOTARY_KEY_ID` and `OBY_NOTARY_ISSUER_ID`, as `--key`, `--key-id` and `--issuer`), and keeps `--keychain-profile "$profile"` locally, when those aren't set. Signing itself needs no change: steps 1–3 put the identity where `package.sh` and `helpers.sh` already look.

R09-14 also checks on the test tag what the runner image carries: Xcode, Node, rustup with `x86_64-apple-darwin` added, and node-gyp cross-building `window-style` for the other architecture.

## Cadence

The owner's choice (q042) is no schedule: a release happens when the owner calls for one. Then:

1. **A release candidate is a named `dev` commit.** The owner names it (its hash), and `dev` keeps taking merges after it.
2. **The real-Mac items are done on a build of that commit.** The "Test it when it's done" items of every feature merged into `dev` since the last release, with the user confirming the real-Mac ones, and a human-drive run (`docs/human-drive/`), all on a build of the candidate.
3. **The full checks pass on it on the owner's Mac.** `scripts/oby-check.sh` in full on the candidate.
4. **A pull request into `main` moves `main` to it**, with GitHub's `Check` green.
5. **The owner pushes the `v*` tag on `main`.** `release.yml` runs, and the `release` environment waits for the owner's approval before any secret is read.
6. **A failing item blocks only that candidate.** It becomes a bug issue the same day, as AGENTS.md "How we work" says; `dev` keeps moving, and the next candidate is a later `dev` commit with the fix.

How each step keeps the rule in AGENTS.md "Carrying and merging" (`dev` merges into `main` only when a feature's "Test it when it's done" list passes with the user confirming the real-Mac items, the full checks pass on `dev` on the user's Mac, and the pull request into `main` has `Check` green):

- Steps 1 and 2 keep the real-Mac rule's intent and make it exact: the list passes on the very commit that goes to `main`, not on whatever `dev` holds when the merge happens, and every feature since the last release is covered, not only the newest.
- Step 3 is the rule's "full checks pass on `dev` on the user's Mac", on the candidate.
- Step 4 is the rule's "a pull request into `main` needs `Check` green", unchanged.
- Step 5 adds what the rule didn't need while releases were by hand: the release is cut only from `main`, only by the owner's tag, and its secrets are read only after the owner's approval.
- Step 6 keeps the rule's intent that nothing untested reaches `main`, without freezing `dev` while a candidate is tested.

**The fast-forward.** `main` holds the merge commit `a72f3905`, which `dev` lacks, so a pull request from `dev` into `main` can't fast-forward today. Once that commit is brought into `dev` (a merge of `main` into `dev`, the owner's call; not done here), later releases can move `main` to the candidate by fast-forward and `main` holds exactly the commit that was tested.

**Proposed wording for AGENTS.md "How we work"** (a proposal for the follow-up task, not applied here):

> `dev` goes into `main` only when the owner calls for a release, on no schedule: the candidate is a named `dev` commit; the "Test it when it's done" items of the features merged since the last release (the user confirms the real-Mac ones) and a human-drive run pass on a build of it, the full checks pass on it on the owner's Mac, and a pull request moves `main` to it with `Check` green. The owner's `v*` tag on `main` then cuts the release in CI, behind the `release` environment's approval (docs/release-in-ci.md). A failing item blocks only that candidate, and `dev` keeps moving.

## Open points for R09-14

- Does the release publish to GitHub Releases (q043's note, OpenBot's way), or keep today's feed on `https://downloads.oby.dev/electron`, which `docs/contracts/updates.md` UPDATES-5 and UPDATES-9 pin?
- Is the release tag cut on `main` once it moves (q043, R09-14), or is `dev`'s head tagged first as the candidate, as the task proposed and this document's cadence names?
- R09-14 moves `apps/mac/app/Tools/bundle-*.sh` to `apps/electron/scripts/`, as its task already says: does anything else still read them from `apps/mac`?
