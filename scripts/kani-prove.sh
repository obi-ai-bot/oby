#!/usr/bin/env bash
# Proves obid's Kani harnesses that a change requires (docs/proofs.md, CI-58 and 59), from core/,
# after `cargo kani -p obid --tests --only-codegen` (CI-57) wrote every harness's name.
#
#   scripts/kani-prove.sh "<filters>"   the harnesses whose names hold any filter (Kani's own
#                                        rule), or every one for `all`; nothing for none
#
# KANI_SLOW=prove also proves the harnesses on core/.config/kani-slow.txt, which are otherwise left
# out and named; KANI_HARNESS_TIMEOUT (by default 20m) is each harness's limit, past which it fails.
# KANI_JOBS (by default 2) runs that many at once: CBMC takes several GB a harness, and four at once
# ran GitHub's 16 GB runner out of memory. On Linux each process gets at most KANI_MEMORY_GB (by
# default 6) of memory: a harness past it fails with CBMC's out-of-memory message and the job goes
# on to name it, where without a ceiling the runner died with no log at all.
set -euo pipefail
cd "$(dirname "$0")/../core"
filters=${1:-}
if [[ -z "$filters" ]]; then
    echo "kani-prove: no harness proves what this change touched: compiled only."
    exit 0
fi
names=$(python3 - "$filters" "${KANI_SLOW:-}" <<'PY'
import glob, json, sys
filters, slow_mode = sys.argv[1].split(), sys.argv[2]
import os
# Every harness is in obid's lib, so its newest metadata (the codegen just run) names them all;
# older files can name harnesses since removed.
found = []
for path in glob.glob("target/kani/*/debug/**/obid-*.kani-metadata.json", recursive=True):
    harnesses = json.load(open(path)).get("proof_harnesses", [])
    if harnesses:
        found.append((os.path.getmtime(path), {h["pretty_name"] for h in harnesses}))
names = max(found, key=lambda f: f[0])[1] if found else set()
if not names:
    sys.exit("kani-prove: no harness found in target/kani: run cargo kani --only-codegen first")
slow = set()
with open(".config/kani-slow.txt") as listed:
    for line in listed:
        name = line.split("#", 1)[0].strip()
        if name:
            slow.add(name)
for name in slow - names:
    sys.exit(f"kani-prove: .config/kani-slow.txt names {name}, which isn't a harness")
wanted = sorted(n for n in names if "all" in filters or any(f in n for f in filters))
for name in wanted:
    if name in slow and slow_mode != "prove":
        print(f"kani-prove: {name} is on .config/kani-slow.txt: proved nightly, not here", file=sys.stderr)
    else:
        print(name)
PY
)
if [[ -z "$names" ]]; then
    echo "kani-prove: what this change requires is proved nightly only (.config/kani-slow.txt)."
    exit 0
fi
args=()
while IFS= read -r name; do args+=(--harness "$name"); done <<< "$names"
if [[ "$(uname)" == Linux ]]; then
    ulimit -v $(( ${KANI_MEMORY_GB:-6} * 1024 * 1024 ))
fi
echo "kani-prove: proving $(wc -l <<< "$names" | tr -d ' ') harnesses, ${KANI_JOBS:-2} at a time, each within ${KANI_HARNESS_TIMEOUT:-20m} and ${KANI_MEMORY_GB:-6} GB"
printf 'kani-prove:   %s\n' $names
exec env -u RUSTUP_TOOLCHAIN cargo kani -p obid --tests --output-format terse --exact "${args[@]}" \
    --jobs "${KANI_JOBS:-2}" -Z unstable-options --harness-timeout "${KANI_HARNESS_TIMEOUT:-20m}"
