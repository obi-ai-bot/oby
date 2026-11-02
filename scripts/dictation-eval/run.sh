#!/bin/bash
# Runs every on-device engine that's installed on every clip, then prints the score table (#1749).
#
#   scripts/dictation-eval/run.sh [eval dir]     (default ~/oby-dictation-eval)
#
# <eval dir>/clips/<group>-<n>.wav with <group>-<n>.txt beside it (group: pt, en or mix),
# <eval dir>/terms.txt optional. Results go to <eval dir>/results/<engine>/. Nothing leaves the Mac.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="${1:-$HOME/oby-dictation-eval}"
[ -d "$root/clips" ] || { echo "No clips: put <group>-<n>.wav and .txt in $root/clips (see README.md)" >&2; exit 1; }
terms="$root/terms.txt"; [ -f "$terms" ] || terms=""

bin="$root/.build/transcribe"
if [ ! -x "$bin" ] || [ "$here/transcribe.swift" -nt "$bin" ]; then
  mkdir -p "$root/.build"
  swiftc -O -parse-as-library -o "$bin" "$here/transcribe.swift" 2>/dev/null || swiftc -O -o "$bin" "$here/transcribe.swift"
fi

json() { python3 -I -c 'import json,sys; print(json.dumps({"text": sys.argv[1], "seconds": float(sys.argv[2])}))' "$@"; }

apple() {  # engine name, locales
  mkdir -p "$root/results/$1"
  for wav in "$root"/clips/*.wav; do
    name="$(basename "$wav" .wav)"
    "$bin" "$2" "$wav" $terms > "$root/results/$1/$name.json" 2>>"$root/results/$1/errors.log" \
      || { rm -f "$root/results/$1/$name.json"; echo "$1 failed on $name (see errors.log)" >&2; }
  done
}

whisper() {
  local cli; cli="$(command -v whisper-cli || true)"
  local model="${WHISPER_MODEL:-$HOME/oby-dictation-eval/models/ggml-large-v3-turbo.bin}"
  if [ -z "$cli" ] || [ ! -f "$model" ]; then
    echo "skipping whisper: brew install whisper-cpp, and put ggml-large-v3-turbo.bin at $model (or set WHISPER_MODEL)" >&2
    return
  fi
  mkdir -p "$root/results/whisper"
  for wav in "$root"/clips/*.wav; do
    name="$(basename "$wav" .wav)"
    start=$(python3 -I -c 'import time; print(time.time())')
    prompt=""; [ -n "$terms" ] && prompt="$(paste -sd, "$terms")"
    if text="$("$cli" -m "$model" -f "$wav" -l auto -nt -np ${prompt:+--prompt "$prompt"} 2>>"$root/results/whisper/errors.log")"; then
      end=$(python3 -I -c 'import time; print(time.time())')
      json "$(echo "$text" | tr '\n' ' ')" "$(python3 -I -c "print($end - $start)")" > "$root/results/whisper/$name.json"
    else
      echo "whisper failed on $name (see errors.log)" >&2
    fi
  done
}

apple apple-pt pt-BR
apple apple-en en-US
apple apple-best pt-BR,en-US
whisper
# ponytail: Parakeet joins once its Apple tooling (FluidAudio or MLX) runs on this Mac; then one more function like whisper()
echo "skipping parakeet: not wired yet (#1749)" >&2

python3 "$here/score.py" "$root"
