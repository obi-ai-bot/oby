#!/bin/sh
# Builds the archive install.sh installs from (docs/install.md): obid-<target>.tar.gz with
#
#   bin/obid          the daemon
#
# and obid-<target>.tar.gz.sha256 beside it. Run on the kind of machine the archive is for.
#
#   sh install/package.sh [--bin FILE] [--out DIR] [--target NAME]
#
# With no --bin it builds obid (cargo build --release). Publishing the archive is a person's step
# (docs/release.md); nothing here uploads.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
bin="" out="$root/build/obid" target=""
while [ $# -gt 0 ]; do
  case "$1" in
  --bin) bin="${2:?--bin needs a file}"; shift 2 ;;
  --out) out="${2:?--out needs a folder}"; shift 2 ;;
  --target) target="${2:?--target needs a name}"; shift 2 ;;
  *) printf 'package: unknown option %s\n' "$1" >&2; exit 1 ;;
  esac
done

if [ -z "$target" ]; then
  case "$(uname -s)" in Linux) kind=linux ;; Darwin) kind=macos ;; *) echo "package: unsupported system" >&2; exit 1 ;; esac
  case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) if [ "$kind" = macos ]; then arch=arm64; else arch=aarch64; fi ;;
  *) echo "package: unsupported machine" >&2; exit 1 ;;
  esac
  target="$kind-$arch"
fi

if [ -z "$bin" ]; then
  cargo build --release --locked --manifest-path "$root/core/Cargo.toml" -p obid
  bin="$root/core/target/release/obid"
fi
[ -x "$bin" ] || { printf 'package: %s isn'"'"'t an executable.\n' "$bin" >&2; exit 1; }

stage="$(mktemp -d "${TMPDIR:-/tmp}/obid-package.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/bin" "$out"
cp "$bin" "$stage/bin/obid"

archive="$out/obid-$target.tar.gz"
tar -czf "$archive" -C "$stage" bin
if command -v sha256sum >/dev/null 2>&1; then sum="$(sha256sum "$archive")"; else sum="$(shasum -a 256 "$archive")"; fi
printf '%s  obid-%s.tar.gz\n' "${sum%% *}" "$target" >"$archive.sha256"
printf '%s\n' "$archive"
