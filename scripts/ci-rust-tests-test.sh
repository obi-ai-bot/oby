#!/bin/bash
# scripts/ci-rust-tests.sh's own tests (docs/contracts/ci.md, CI-60 to 65), on this checkout's
# tree and history: ci.yml's plan job runs them when the script, this file or the Mac's map
# (scripts/oby-check.sh) changes, and nightly. Each case says which scenario it holds.
set -eo pipefail
cd "$(dirname "$0")/.."

fails=0
pick() { # the value of one key in the script's output
    sed -n "s/^$1=//p" <<<"$2"
}
ok() { echo "ok: $1"; }
bad() { echo "FAILED: $1" >&2; fails=$((fails + 1)); }
has() { # case, haystack, needle
    if [[ " $2 " == *" $3 "* ]]; then ok "$1"; else bad "$1: no '$3' in: $2"; fi
}
lacks() {
    if [[ " $2 " != *" $3 "* ]]; then ok "$1"; else bad "$1: '$3' in: $2"; fi
}
is() {
    if [[ "$2" == "$3" ]]; then ok "$1"; else bad "$1: '$2', not '$3'"; fi
}
run() { # mode, paths (one a line), [range]
    printf '%s\n' "$2" | CI_RUST_RANGE="${3:-}" bash scripts/ci-rust-tests.sh "$1"
}

# CI-60: a one-file Sphere change runs the Sphere's binaries, the module's lib tests and the guards.
out=$(run affected core/crates/obid/src/sphere/pins.rs)
targets=$(pick rust-targets "$out")
filter=$(pick rust-filter "$out")
is "CI-60 sphere: affected" "$(pick rust-tests "$out")" affected
for name in sphere_api sphere_questions mcp_sphere routes_guard state_owner released_files runner_api; do
    has "CI-60 sphere: builds $name" "$targets" "$name"
done
lacks "CI-60 sphere: not chatgpt_api" "$targets" chatgpt_api
lacks "CI-60 sphere: not the workspace" "$targets" --workspace
[[ "$filter" == *"test(/^sphere::/)"* ]] && ok "CI-60 sphere: its lib tests" || bad "CI-60 sphere: no lib filter: $filter"
[[ "$filter" != *"binary_id(obid::runner_api) |"* ]] && ok "CI-60 sphere: runner_api for its guard only" ||
    bad "CI-60 sphere: runner_api runs whole: $filter"
is "CI-60 sphere: fewer than 4 shards" "$(($(pick rust-shard-count "$out") < 4))" 1

# CI-61: where the map can't decide, every test.
for path in core/Cargo.lock core/Cargo.toml core/crates/obid/Cargo.toml core/crates/obid/tests/support/mod.rs \
    core/crates/obi-protocol/src/lib.rs e2e/lib/fixture.mjs .github/workflows/ci.yml scripts/ci-rust-tests.sh; do
    out=$(run affected "$path")
    is "CI-61 $path: full" "$(pick rust-tests "$out")" full
    is "CI-61 $path: the workspace" "$(pick rust-targets "$out")" --workspace
done
out=$(run affected core/crates/obid/src/lib.rs)
is "CI-61 lib.rs with no range: full" "$(pick rust-tests "$out")" full
# 263e0a47 only declared a module in lib.rs (`pub mod autofix;`); de770667 also changed what obid
# starts. This repository's history keeps both.
if git cat-file -e '263e0a47^{commit}' 2>/dev/null && git cat-file -e 'de770667^{commit}' 2>/dev/null; then
    out=$(run affected "$(git diff-tree --no-commit-id --name-only -r 263e0a47)" 263e0a47^..263e0a47)
    is "CI-61 lib.rs declaring a module: affected" "$(pick rust-tests "$out")" affected
    out=$(run affected "$(git diff-tree --no-commit-id --name-only -r de770667)" de770667^..de770667)
    is "CI-61 lib.rs changing more: full" "$(pick rust-tests "$out")" full
else
    bad "CI-61 lib.rs: commits 263e0a47 and de770667 aren't in this clone (fetch its whole history)"
fi

# CI-62: an app-only change and a docs/api change run the guards alone.
for path in apps/electron/src/renderer/src/obid.ts docs/api/routes.txt; do
    out=$(run affected "$path")
    targets=$(pick rust-targets "$out")
    has "CI-62 $path: routes_guard" "$targets" routes_guard
    lacks "CI-62 $path: no other binary" "$targets" sphere_api
    is "CI-62 $path: one shard" "$(pick rust-shards "$out")" "[1]"
done
out=$(run guards core/crates/obid/src/sphere/pins.rs)
is "CI-62 guards mode: guards" "$(pick rust-tests "$out")" guards
lacks "CI-62 guards mode: no affected binary" "$(pick rust-targets "$out")" sphere_api

# CI-63: full runs every test in 4 shards.
out=$(run full core/crates/obid/src/sphere/pins.rs)
is "CI-63 full: the workspace" "$(pick rust-targets "$out")" --workspace
is "CI-63 full: 4 shards" "$(pick rust-shards "$out")" "[1,2,3,4]"
is "CI-63 full: no filter" "$(pick rust-filter "$out")" ""

# CI-65: every guard names a test binary that exists and a test it can find, so a guard renamed
# here or in obid fails before a run does (Check fails a run where one ran no test).
guards=$(pick rust-guards "$(run guards x)")
while IFS=$'\t' read -r name suite pattern; do
    binary=${suite#obid::}
    if [[ "$suite" == obid::* ]]; then
        [[ -f "core/crates/obid/tests/$binary.rs" ]] && ok "CI-65 $name: tests/$binary.rs" || bad "CI-65 $name: no tests/$binary.rs"
        where="core/crates/obid/tests/$binary.rs"
    else
        where=core/crates/obid/src
    fi
    if [[ -n "$pattern" ]]; then
        # `::name$` is one function, whole; `^prefix` is every function that starts so.
        fn=${pattern#::}
        fn=${fn#^}
        if [[ "$fn" == *'$' ]]; then fn="${fn%$}[(<]"; fi
        if grep -rqE "fn $fn" "$where"; then ok "CI-65 $name: fn $fn"; else bad "CI-65 $name: no fn $fn in $where"; fi
    fi
done < <(python3 -c 'import json, sys
for g in json.loads(sys.argv[1]):
    print("\t".join(g))' "$guards")

# The usage.
if bash scripts/ci-rust-tests.sh nonsense </dev/null >/dev/null 2>&1; then bad "usage: a bad mode passed"; else ok "usage: a bad mode fails"; fi

if ((fails > 0)); then
    echo "ci-rust-tests-test: $fails failed" >&2
    exit 1
fi
echo "ci-rust-tests-test: passed"
