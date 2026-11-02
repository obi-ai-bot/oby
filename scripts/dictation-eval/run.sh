#!/bin/bash
# Runs every on-device engine that's installed on every clip, then prints the score table (#1749).
#
#   scripts/dictation-eval/run.sh [eval dir]     (default ~/oby-dictation-eval)
#
# <eval dir>/clips/en-<n>.wav with en-<n>.txt beside it (English only, the owner's decision of 2026-11-01),
# <eval dir>/terms.txt optional. Results go to <eval dir>/results/<engine>/. Nothing leaves the Mac.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="${1:-$HOME/oby-dictation-eval}"
[ -d "$root/clips" ] || { echo "No clips: put en-<n>.wav and .txt in $root/clips (see README.md)" >&2; exit 1; }
terms="$root/terms.txt"; [ -f "$terms" ] || terms=""

bin="$root/.build/transcribe"
if [ ! -x "$bin" ] || [ "$here/transcribe.swift" -nt "$bin" ]; then
  mkdir -p "$root/.build"
  swiftc -O -parse-as-library -o "$bin" "$here/transcribe.swift" 2>/dev/null || swiftc -O -o "$bin" "$here/transcribe.swift"
fi

apple() {  # engine name, locales
  mkdir -p "$root/results/$1"
  for wav in "$root"/clips/*.wav; do
    name="$(basename "$wav" .wav)"
    "$bin" "$2" "$wav" $terms > "$root/results/$1/$name.json" 2>>"$root/results/$1/errors.log" \
      || { rm -f "$root/results/$1/$name.json"; echo "$1 failed on $name (see errors.log)" >&2; }
  done
}

parakeet() {  # NVIDIA Parakeet TDT 0.6B v3 through FluidAudio; the model downloads on the first run
  local build="$root/.build/parakeet"
  if ! swift build -c release --package-path "$here/parakeet" --scratch-path "$build" >"$root/.build/parakeet.log" 2>&1; then
    echo "skipping parakeet: its build failed (see $root/.build/parakeet.log)" >&2
    return
  fi
  mkdir -p "$root/results/parakeet"
  "$build/release/parakeet" "$root/results/parakeet" "$root"/clips/*.wav 2>>"$root/results/parakeet/errors.log" \
    || echo "parakeet failed (see errors.log)" >&2
}

apple apple-en en-US
parakeet

python3 "$here/score.py" "$root"
