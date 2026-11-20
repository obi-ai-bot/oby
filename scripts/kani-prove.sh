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
# on to name it, where without a ceiling the runner died with no log at all. The harnesses on
# core/.config/kani-slow.txt, when proved, go after the others, one at a time, each with
# KANI_SLOW_MEMORY_GB (by default 13): jobstate_7 aborted ("CBMC failed with status 6") under 6 GB
# beside another harness on three nights in a row.
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
    if name not in slow:
        print("fast", name)
    elif slow_mode == "prove":
        print("slow", name)
    else:
        print(f"kani-prove: {name} is on .config/kani-slow.txt: proved nightly, not here", file=sys.stderr)
PY
)
if [[ -z "$names" ]]; then
    echo "kani-prove: what this change requires is proved nightly only (.config/kani-slow.txt)."
    exit 0
fi
fast=$(awk '$1 == "fast" {print $2}' <<< "$names")
slow=$(awk '$1 == "slow" {print $2}' <<< "$names")

# prove <jobs> <memory GB> <names>: one cargo kani over those harnesses, in a subshell so its memory
# ceiling stays its own (a lowered ulimit can't be raised again in the same shell).
prove() {
    local jobs=$1 memory=$2 list=$3 args=()
    while IFS= read -r name; do args+=(--harness "$name"); done <<< "$list"
    echo "kani-prove: proving $(wc -l <<< "$list" | tr -d ' ') harnesses, $jobs at a time, each within ${KANI_HARNESS_TIMEOUT:-20m} and $memory GB"
    printf 'kani-prove:   %s\n' $list
    (
        if [[ "$(uname)" == Linux ]]; then
            ulimit -v $(( memory * 1024 * 1024 ))
        fi
        exec env -u RUSTUP_TOOLCHAIN cargo kani -p obid --tests --output-format terse --exact "${args[@]}" \
            --jobs "$jobs" -Z unstable-options --harness-timeout "${KANI_HARNESS_TIMEOUT:-20m}"
    )
}

status=0
if [[ -n "$fast" ]]; then
    prove "${KANI_JOBS:-2}" "${KANI_MEMORY_GB:-6}" "$fast" || status=1
fi
if [[ -n "$slow" ]]; then
    prove 1 "${KANI_SLOW_MEMORY_GB:-13}" "$slow" || status=1
fi
exit "$status"
