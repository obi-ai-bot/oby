#!/bin/zsh
# Stages what the app carries beside Electron into build/helpers, with the Swift app's own build
# phases (apps/mac/app/Tools/bundle-*.sh): obid, universal, at Contents/Helpers/obid with its
# plug-ins at Contents/Resources/obid, and gh and gitleaks at Contents/MacOS, where obid looks for
# them (core/crates/obid/src/app.rs). Each is signed with the team's Developer ID and the hardened
# runtime; electron-builder copies them in and leaves their signatures alone (electron-builder.yml:
# extraFiles, signIgnore), so gitleaks keeps the one entitlement it needs. OBY_CARGO_TARGET_DIR
# reuses a cargo target directory, as for scripts/release.sh. With --if-stale it stages only when
# build/helpers/stamp isn't the last commit of what goes in, or that has changes not committed.
set -euo pipefail

app=${0:A:h:h}
root=${app:h:h}
out=$app/build/helpers
sources=(core plugins apps/mac/app/Tools apps/electron/scripts/helpers.sh)
stamp=$(git -C "$root" log -1 --format=%H -- "${sources[@]}")
if [[ "${1:-}" == --if-stale && "$(cat "$out/stamp" 2> /dev/null)" == "$stamp" \
  && -z "$(git -C "$root" status --porcelain -- "${sources[@]}")" ]]; then
  exit 0
fi
team=${OBY_TEAM_ID:-3UV9W5SV7W}
identity=$(security find-identity -v -p codesigning | grep "Developer ID Application" | grep "($team)" | head -n 1 | awk '{print $2}')
[[ -n "$identity" ]] || { echo "error: no Developer ID Application identity of team $team in the keychain" >&2; exit 1; }

rm -rf "$out"
mkdir -p "$out"
export SRCROOT=$root/apps TARGET_BUILD_DIR=$out CONTENTS_FOLDER_PATH=Contents \
  EXECUTABLE_FOLDER_PATH=Contents/MacOS UNLOCALIZED_RESOURCES_FOLDER_PATH=Contents/Resources \
  EXPANDED_CODE_SIGN_IDENTITY=$identity CONFIGURATION=Release
for tool in obid gh gitleaks; do zsh "$root/apps/mac/app/Tools/bundle-$tool.sh"; done
# The Swift app's launch agents: this app has its own (build/com.deskree.oby.electron.obid.plist).
rm -rf "$out/Contents/Library"
echo "$stamp" > "$out/stamp"
