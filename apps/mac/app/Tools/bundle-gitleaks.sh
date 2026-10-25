#!/bin/zsh
# Bundles gitleaks with Oby (#1026): Contents/MacOS/gitleaks, universal, signed with the app's
# identity and the hardened runtime, so every Mac running Oby has obid's whole secret scan with
# nothing to install. obid uses it before any other (core/crates/obid/src/scan.rs, gitleaks_bin),
# and publishes nothing without one ("If the scanner can't run, nothing is published"). The
# release is pinned to the version obid's canary was measured with, and checked against its
# SHA-256 (from the release's gitleaks_<version>_checksums.txt) on every build; the download is
# cached under ~/Library/Caches, never in the repository. Moving to another version means running
# scan.rs's gitleaks tests against it first. Run by the "Bundle gitleaks" build phase of the Oby
# target (apps/Oby.xcodeproj), as bundle-gh.sh is.
set -euo pipefail

VERSION=8.30.1
typeset -A SHA256
SHA256[arm64]=b40ab0ae55c505963e365f271a8d3846efbc170aa17f2607f13df610a9aeb6a5
SHA256[x64]=dfe101a4db2255fc85120ac7f3d25e4342c3c20cf749f2c20a18081af1952709
cache="${GITLEAKS_BUNDLE_CACHE:-$HOME/Library/Caches/com.deskree.oby/gitleaks}/$VERSION"

# The release's archive for one architecture: downloaded once into the cache, but the cache is
# the user's to write, so every build copies it into a private folder and checks that copy against
# its pinned SHA-256 before gitleaks is taken out of it (a swapped file is never signed into the
# app).
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fetch() {
  local arch=$1 name="gitleaks_${VERSION}_darwin_$1"
  local archive="$cache/$name.tar.gz" copy="$work/$name.tar.gz"
  mkdir -p "$cache"
  if [[ -f "$archive" ]]; then cp "$archive" "$copy"; fi
  if ! echo "${SHA256[$arch]}  $copy" | shasum -a 256 -c - > /dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$copy" "https://github.com/gitleaks/gitleaks/releases/download/v$VERSION/$name.tar.gz"
    echo "${SHA256[$arch]}  $copy" | shasum -a 256 -c - > /dev/null
    cp "$copy" "$archive.part" && mv "$archive.part" "$archive"
  fi
  mkdir "$work/$arch"
  tar -xzf "$copy" -C "$work/$arch" gitleaks LICENSE
}

fetch arm64
fetch x64

dest="$TARGET_BUILD_DIR/$EXECUTABLE_FOLDER_PATH/gitleaks"
mkdir -p "$(dirname "$dest")"
rm -f "$dest"
lipo -create -output "$dest" "$work/arm64/gitleaks" "$work/x64/gitleaks"

# gitleaks is MIT: its license goes with it.
licenses="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/Licenses"
mkdir -p "$licenses"
cp "$work/arm64/LICENSE" "$licenses/gitleaks.txt"

# Signed like the app: the hardened runtime for notarization, and a secure timestamp on release
# builds (it needs the network). Xcode seals the app, with this inside, after this phase.
# ponytail: the release is built with go1.24.11, whose runtime macOS kills under the hardened
# runtime unless it may map unsigned executable memory (measured 2026-10-25: exit 137 without it,
# with an Apple Development identity too; gh 2.102.0 needs nothing). Drop the entitlement once a
# release built with a newer Go runs without it (the check below says so).
identity="${EXPANDED_CODE_SIGN_IDENTITY:--}"
timestamp=--timestamp=none
if [[ "$identity" != "-" && "${CONFIGURATION:-Debug}" == Release ]]; then timestamp=--timestamp; fi
cat > "$work/gitleaks.entitlements" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.cs.allow-unsigned-executable-memory</key>
  <true/>
</dict>
</plist>
PLIST
codesign --force --options runtime "$timestamp" --entitlements "$work/gitleaks.entitlements" \
  --sign "$identity" "$dest"

# A gitleaks macOS kills isn't one: obid would publish nothing. Never ship it.
said=$("$dest" version 2>&1) || { echo "error: the bundled gitleaks doesn't run: $said" >&2; exit 1; }
[[ "$said" == "$VERSION" ]] || { echo "error: the bundled gitleaks says $said, not $VERSION" >&2; exit 1; }
