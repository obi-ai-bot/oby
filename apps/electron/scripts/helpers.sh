#!/bin/zsh
# Stages what the app carries beside Electron into build/helpers, with the Swift app's own build
# phases (apps/mac/app/Tools/bundle-*.sh): obid, universal, at Contents/Helpers/obid with its
# plug-ins at Contents/Resources/obid, and gh and gitleaks at Contents/MacOS, where obid looks for
# them (core/crates/obid/src/app.rs); beside them oby-picture-text, built here from
# native/picture-text, where main looks for it (main/pictureText.ts). Each is signed with the team's Developer ID and the hardened
# runtime; electron-builder copies them in and leaves their signatures alone (electron-builder.yml:
# extraFiles, signIgnore), so gitleaks keeps the one entitlement it needs. OBY_CARGO_TARGET_DIR
# reuses a cargo target directory, as for scripts/release.sh. With --if-stale it stages only when
# build/helpers/stamp isn't the last commit of what goes in, or that has changes not committed.
# For a build that stays on this Mac (scripts/run-dev.sh): OBY_HELPERS_IDENTITY signs with another
# identity (its SHA-1, or - for ad hoc), and OBY_OBID_TARGETS builds obid for those targets only.
# obid is told the commit it's built from (OBY_BUILD_COMMIT, else HEAD), for Settings › About.
set -euo pipefail

app=${0:A:h:h}
root=${app:h:h}
out=$app/build/helpers
sources=(core plugins apps/mac/app/Tools apps/electron/native/picture-text apps/electron/scripts/helpers.sh)
export OBY_BUILD_COMMIT=${OBY_BUILD_COMMIT:-$(git -C "$root" rev-parse HEAD)}
# what's staged, and how: a release's staging is never taken for a local one's, nor the other way
stamp="$(git -C "$root" log -1 --format=%H -- "${sources[@]}") ${OBY_OBID_TARGETS:-all} ${OBY_HELPERS_IDENTITY:-team}"
if [[ "${1:-}" == --if-stale && "$(cat "$out/stamp" 2> /dev/null)" == "$stamp" \
  && -z "$(git -C "$root" status --porcelain -- "${sources[@]}")" ]]; then
  exit 0
fi
team=${OBY_TEAM_ID:-3UV9W5SV7W}
identity=${OBY_HELPERS_IDENTITY:-$(security find-identity -v -p codesigning | grep "Developer ID Application" | grep "($team)" | head -n 1 | awk '{print $2}')}
[[ -n "$identity" ]] || { echo "error: no Developer ID Application identity of team $team in the keychain" >&2; exit 1; }

rm -rf "$out"
mkdir -p "$out"
export SRCROOT=$root/apps TARGET_BUILD_DIR=$out CONTENTS_FOLDER_PATH=Contents \
  EXECUTABLE_FOLDER_PATH=Contents/MacOS UNLOCALIZED_RESOURCES_FOLDER_PATH=Contents/Resources \
  EXPANDED_CODE_SIGN_IDENTITY=$identity CONFIGURATION=Release
for tool in obid gh gitleaks; do zsh "$root/apps/mac/app/Tools/bundle-$tool.sh"; done
# oby-picture-text: the text macOS's Vision reads in an Obysphere picture, for the secret scan before
# it leaves (main/pictureText.ts, SPHEREAPP-18). Universal (macOS 26, as the app), signed as gh is.
# One macOS kills isn't one: main would send no picture. Never ship it.
picture_text=$out/Contents/MacOS/oby-picture-text
work=$(mktemp -d)
for arch in arm64 x86_64; do
  xcrun swiftc -O -target $arch-apple-macos26.0 -o "$work/$arch" "$app/native/picture-text/main.swift"
done
lipo -create -output "$picture_text" "$work/arm64" "$work/x86_64"
rm -rf "$work"
timestamp=--timestamp=none
[[ "$identity" != "-" ]] && timestamp=--timestamp
codesign --force --options runtime "$timestamp" --sign "$identity" "$picture_text"
code=0; "$picture_text" > /dev/null 2>&1 || code=$?
(( code == 64 )) || { echo "error: the bundled oby-picture-text doesn't run (exit $code)" >&2; exit 1; }
# The Swift app's launch agents: this app has its own (build/com.deskree.oby.electron.obid.plist).
rm -rf "$out/Contents/Library"
echo "$stamp" > "$out/stamp"
