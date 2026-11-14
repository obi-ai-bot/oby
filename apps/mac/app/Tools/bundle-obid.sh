#!/bin/zsh
# Bundles obid, Oby's engine, with Oby (the owner's decision of 2026-10-18): Contents/Helpers/obid,
# signed with the app's identity and the hardened runtime, and clears an older build's folders from
# Contents/Resources/obid (core/crates/obid/src/app.rs). Run by the Electron app's packaging
# (apps/electron/scripts/helpers.sh).
set -euo pipefail

root="$SRCROOT/.."
contents="$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH"
helpers="$contents/Helpers"
resources="$contents/Resources/obid"
mkdir -p "$helpers" "$resources"

# Xcode's build variables would point cargo at the wrong SDK, so it builds in a clean environment.
# OBY_CARGO_TARGET_DIR is a target directory kept between releases.
target_dir="${OBY_CARGO_TARGET_DIR:-$root/core/target}"
build=(env -i HOME="$HOME" PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
  cargo build --quiet --release -p obid --manifest-path "$root/core/Cargo.toml" --target-dir "$target_dir")

# Both architectures when this Mac's Rust has both, in one cargo run so they compile side by side;
# a release needs them.
targets=()
slices=()
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  libdir=$(env -i HOME="$HOME" PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin" \
    rustc --print target-libdir --target "$target" 2>/dev/null || true)
  if [[ -n "$libdir" ]] && ls "$libdir"/libstd-* > /dev/null 2>&1; then
    targets+=(--target "$target")
    slices+=("$target_dir/$target/release/obid")
  fi
done
if (( ${#slices} == 0 )); then
  "${build[@]}"
  slices=("$target_dir/release/obid")
else
  "${build[@]}" "${targets[@]}"
fi
if [[ "${CONFIGURATION:-Debug}" == Release ]] && (( ${#slices} < 2 )); then
  echo "error: a release of Oby needs obid for arm64 and x86_64: rustup target add aarch64-apple-darwin x86_64-apple-darwin" >&2
  exit 1
fi
rm -f "$helpers/obid"
lipo -create -output "$helpers/obid" "${slices[@]}"
chmod 755 "$helpers/obid"

# obid has no plug-ins any more (the strip to a dispatcher): an older build's folder goes.
rm -rf "$resources/plugins" "$resources/web"

# Signed like the app: the hardened runtime for notarization, and a secure timestamp on release
# builds (it needs the network).
identity="${EXPANDED_CODE_SIGN_IDENTITY:--}"
timestamp=--timestamp=none
if [[ "$identity" != "-" && "${CONFIGURATION:-Debug}" == Release ]]; then timestamp=--timestamp; fi
codesign --force --options runtime "$timestamp" --sign "$identity" "$helpers/obid"
