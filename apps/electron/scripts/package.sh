#!/bin/zsh
# Packages the Mac app, one DMG per architecture, on this Mac (no CI), as scripts/release.sh does
# the Swift app's (docs/release.md):
#   1. obid, gh and gitleaks, staged and signed by helpers.sh when what goes in changed since;
#   2. main, preload and the renderer (electron-vite);
#   3. per architecture: window-style built for it, the app signed with Developer ID and the
#      hardened runtime (electron-builder), checked, scanned for secrets before it leaves this Mac,
#      notarized and stapled; then dist/<name>-<version>-<arch>.dmg (the app and an
#      Applications link), signed, notarized and stapled too, and checked; and
#      dist/<name>-<version>-<arch>-mac.zip, the stapled app for the updater (main/updates.ts);
#   4. dist/latest-mac.yml: the version and each zip's sha512, which the updater checks.
# Uploading is the owner's step: the zips, then latest-mac.yml, to https://downloads.oby.dev/electron
# (docs/contracts/updates.md, "Publishing a release").
# Usage: scripts/package.sh [arm64] [x64] (both by default). Notarization uses the keychain
# profile oby-notary (or OBY_NOTARY_PROFILE): docs/release.md, "Store the notarization credentials".
set -euo pipefail

app_dir=${0:A:h:h}
root=${app_dir:h:h}
cd "$app_dir"
profile=${OBY_NOTARY_PROFILE:-oby-notary}
team=${OBY_TEAM_ID:-3UV9W5SV7W}
# the identity electron-builder signs with (CSC_NAME: electron-builder.yml names none): another
# team's Developer ID sets both
export OBY_SIGN_IDENTITY=${OBY_SIGN_IDENTITY:-"Deskree Technologies Inc. (3UV9W5SV7W)"}
export CSC_NAME=$OBY_SIGN_IDENTITY
archs=("$@")
(( ${#archs} )) || archs=(arm64 x64)
version=$(node -p "require('./package.json').version")
name=$(sed -n 's/^productName: //p' electron-builder.yml)
electron=$(node -p "require('electron/package.json').version")
identity=$(security find-identity -v -p codesigning | grep "Developer ID Application" | grep "($team)" | head -n 1 | awk '{print $2}')
[[ -n "$identity" ]] || { echo "error: no Developer ID Application identity of team $team in the keychain" >&2; exit 1; }
die() { echo "error: $*" >&2; exit 1; }
say() { echo "==> $*"; }

notarize() {
  local item=$1 submit=$2 log=dist/notary-${1:t}.log
  xcrun notarytool submit "$submit" --keychain-profile "$profile" --wait > "$log" 2>&1 || { cat "$log"; die "notarization failed: $log"; }
  grep -q 'status: Accepted' "$log" || { cat "$log"; die "Apple didn't accept ${item:t}: $log"; }
  xcrun stapler staple "$item" > /dev/null || die "no ticket to staple on ${item:t}"
  xcrun stapler validate "$item" > /dev/null || die "the ticket on ${item:t} doesn't validate"
}

zsh scripts/helpers.sh --if-stale
gitleaks=build/helpers/Contents/MacOS/gitleaks
say "Building main, preload and the renderer"
npm run build --silent

zips=()
for arch in $archs; do
  say "Oby $version for $arch"
  npx node-gyp rebuild --silent --directory native/window-style --arch="$arch" --target="$electron" --dist-url=https://electronjs.org/headers
  [[ "$(lipo -archs native/window-style/build/Release/window_style.node)" == "${arch/x64/x86_64}" ]] || die "window-style isn't built for $arch"
  rm -rf dist/mac*(N)
  npx electron-builder --mac dir "--$arch" --publish never
  app=(dist/mac*/"$name".app)
  codesign --verify --deep --strict "$app" || die "$name.app's signature doesn't verify"
  for helper in Helpers/obid MacOS/gh MacOS/gitleaks; do
    [[ "$(codesign -dv "$app/Contents/$helper" 2>&1)" == *"TeamIdentifier=$team"* ]] || die "$helper isn't signed by team $team"
  done
  [[ "$("$app/Contents/MacOS/gitleaks" version 2>&1)" == 8.30.1 ]] || die "the app's gitleaks doesn't run"
  # without it the updater never downloads (electron-updater reads its cache folder's name there)
  cmp -s build/app-update.yml "$app/Contents/Resources/app-update.yml" || die "$name.app has no app-update.yml: it could never update"

  # The app's files scanned before they leave this Mac, for Apple's notary service too. gitleaks
  # reads text: a secret inside a Mach-O binary goes unseen, as in scripts/release.sh.
  say "Secret scan"
  scan=0
  "$gitleaks" dir "$app" --config "$root/.gitleaks.toml" --no-banner --redact --exit-code=99 \
    --report-format=json --report-path=dist/gitleaks-$arch.json > dist/gitleaks-$arch.log 2>&1 || scan=$?
  case $scan in
    0) ;;
    99) die "possible secret in $name.app: dist/gitleaks-$arch.json says where (redacted)" ;;
    *) tail -n 20 dist/gitleaks-$arch.log; die "gitleaks couldn't scan $name.app: nothing is notarized unscanned" ;;
  esac

  # The app carries its own ticket, so a copy taken out of the DMG opens offline too.
  say "Notarizing the app"
  ditto -c -k --keepParent "$app" dist/app-$arch.zip
  notarize "$app" dist/app-$arch.zip
  rm dist/app-$arch.zip

  say "The DMG"
  dmg=dist/${name// /-}-$version-$arch.dmg
  staging=$(mktemp -d)
  ditto "$app" "$staging/$name.app"
  ln -s /Applications "$staging/Applications"
  rm -f "$dmg"
  hdiutil create -quiet -fs HFS+ -volname "$name" -srcfolder "$staging" -format UDZO "$dmg"
  rm -rf "$staging"
  codesign --sign "$identity" --timestamp "$dmg"
  notarize "$dmg" "$dmg"
  hdiutil verify -quiet "$dmg" || die "$dmg doesn't verify"
  spctl -a -t open --context context:primary-signature "$dmg" || die "Gatekeeper refuses $dmg"
  spctl -a -t exec "$app" || die "Gatekeeper refuses $name.app"
  say "$dmg"

  # The updater's copy: the stapled app, zipped as Squirrel.Mac unzips it (ditto). Its name has
  # the architecture in it: the updater picks the arm64 zip on Apple silicon, the other elsewhere.
  zip=dist/${name// /-}-$version-$arch-mac.zip
  rm -f "$zip"
  ditto -c -k --sequesterRsrc --keepParent "$app" "$zip"
  zips+=("$zip")
done

# What the updater reads: this version, each zip with its sha512 (base64, as electron-updater
# checks it) and size. Upload it after the zips, so no Mac is sent to a zip that isn’t there yet.
say "dist/latest-mac.yml"
{
  echo "version: $version"
  echo "files:"
  for zip in $zips; do
    echo "  - url: ${zip:t}"
    echo "    sha512: $(openssl dgst -sha512 -binary "$zip" | base64)"
    echo "    size: $(stat -f %z "$zip")"
  done
  echo "path: ${zips[1]:t}"
  echo "sha512: $(openssl dgst -sha512 -binary "${zips[1]}" | base64)"
  echo "releaseDate: '$(date -u +%Y-%m-%dT%H:%M:%S.000Z)'"
} > dist/latest-mac.yml
# window-style back to this Mac's architecture, for npm run dev
npm run build:native --silent > /dev/null
