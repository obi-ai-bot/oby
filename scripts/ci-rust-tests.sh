#!/bin/bash
# Which of the Rust core's tests a GitHub run runs (docs/ci.md, "Which Rust tests run";
# docs/contracts/ci.md, CI-60 to 65). ci.yml's plan job gives it the run's changed paths on stdin,
# one a line, and writes what it prints to its outputs:
#
#   scripts/ci-rust-tests.sh full       every test of the workspace, in 4 shards
#   scripts/ci-rust-tests.sh affected   the guards below and the tests the change can break; every
#                                       test when it can't tell which
#   scripts/ci-rust-tests.sh guards     the guards alone
#
# It prints `key=value` lines:
#   rust-tests        full, affected or guards: what runs (affected becomes full when it can't tell)
#   rust-why          why, in one sentence
#   rust-targets      what `cargo nextest archive` builds: `--workspace`, or `-p obid --lib --test <name>...`
#   rust-filter       the nextest filterset (`-E`) over that archive; empty runs all of it
#   rust-shards       the shard numbers, a JSON list: [1] up to [1,2,3,4]
#   rust-shard-count  how many shards
#   rust-guards       the guards, a JSON list of [name, JUnit suite, test-name regex]: Check fails
#                     when one ran no test (renamed or gone: change it here too)
#   rust-selected     what runs, in words, for Check's summary
#
# The affected set is the Mac's own map: scripts/oby-check.sh's dry run (OBY_CHECK_PLAN,
# OBY_CHECK_PATHS) names the obid test binaries and lib modules a worker's check of these paths
# runs. It is widened by obid's imports: for each top-level module the map names, the lib tests of
# the whole module, and every test binary named after it (the module's name a word of the
# binary's) or importing it (`obid::<module>`). Where the map can't decide, every test runs: the
# workspace's manifests and lock, the checks themselves, obid's crate root (but a module only
# declared there), a file in obid's src/ that isn't Rust, the tests' shared support and the e2e
# harness's files it runs, another crate obid builds on, and a change the map names no test for or
# plans all of obid's lib tests or another crate's tests for. CI_RUST_RANGE, the run's `git diff`
# range, is how the crate root's change is read; without it, a change there is every test.
#
# The guards always run: each keeps two places in agreement, so a change to either side can break
# it, whichever lane the change is in (an app-only change and the route guard, 2026-11-18).
set -eo pipefail
cd "$(dirname "$0")/.."

want=${1:-}
case "$want" in
    full | affected | guards) ;;
    *) echo "usage: scripts/ci-rust-tests.sh full|affected|guards < changed-paths" >&2; exit 2 ;;
esac
changed=$(sed '/^$/d' | sort -u)
obid=core/crates/obid

# The guards, `<name> <binary> <test-name regex>`: a test binary of obid's whole (no regex), or the
# lib's or a binary's tests whose names match. Each keeps two places in agreement:
#   routes_guard    obid's routes, docs/api/routes.txt, the gateway's ROUTES and the apps' calls (CI-9 to 11)
#   state_owner     who writes a job's state, counted in obid's sources, and its constants (JOBSTATE-19)
#   released_files  what released obids wrote and what today's obid reads (docs/contracts/released-files.md)
#   caps            GET /v1/status's capabilities and the one API doc that lists them (CAPS-1 to 3)
#   prompt_budget_holds  the prompts obid adds to a session and prompt-budget.txt
#   unsafe_1        obid's one unsafe function, read over every crate's sources and manifests (UNSAFE-1)
#   keychain_1      the vault and obid's manifest: no Keychain (KEYCHAIN-1)
#   merging_2       merge.rs's Mac-only paths and what scripts/oby-check.sh leaves to the Mac (MERGING-2)
#   agentmcp_7, one_protocol_version_list  the MCP tool list and protocol versions, read from obid's sources
#   clean_docs, clean_names  obid's secret scan against this repository's docs and its file names
guards='
routes_guard routes_guard
state_owner state_owner
released_files released_files
caps runner_api ^caps_
prompt_budget_holds obid ::prompt_budget_holds$
unsafe_1 obid ::unsafe_1_only_may_write_is_unsafe$
keychain_1 obid ::keychain_1_the_vault_never_touches_the_keychain$
merging_2 obid ::merging_2_what_only_the_mac_tests_is_the_scripts$
agentmcp_7 obid ::agentmcp_7_every_entry_names_every_gate$
one_protocol_version_list obid ::one_protocol_version_list$
clean_docs obid ::clean_docs_and_the_fixture_result_stay_clean$
clean_names obid ::this_repositorys_names_are_clean_and_a_token_among_them_is_found$
'

suite() { if [[ "$1" == obid ]]; then echo obid; else echo "obid::$1"; fi; }
guard_json='' guard_filter='' guard_binaries=()
while read -r name binary regex; do
    [[ -n "$name" ]] || continue
    guard_json+="${guard_json:+,}[\"$name\",\"$(suite "$binary")\",\"$regex\"]"
    if [[ -z "$regex" ]]; then
        guard_binaries+=("$binary")
        guard_filter+="${guard_filter:+ | }binary_id($(suite "$binary"))"
    else
        guard_filter+="${guard_filter:+ | }(binary_id($(suite "$binary")) & test(/$regex/))"
    fi
done <<<"$guards"

say() {
    echo "rust-tests=$1"
    echo "rust-why=$2"
    echo "rust-targets=$3"
    echo "rust-filter=$4"
    echo "rust-shards=[$(seq 1 "$5" | paste -sd , -)]"
    echo "rust-shard-count=$5"
    echo "rust-guards=[$guard_json]"
    echo "rust-selected=$6"
}
full() {
    say full "$1" --workspace '' 4 "every test of the workspace, the guards among them"
    exit 0
}

[[ "$want" != full ]] || full "this run tests everything"

binaries=()
modules=()
why="the change reaches what the guards read"
if [[ "$want" == affected ]]; then
    why="the guards and the tests the change can break"
    mapped='' # the paths handed to the Mac's map
    while IFS= read -r f; do
        case "$f" in
            core/Cargo.toml | core/Cargo.lock | core/crates/*/Cargo.toml | core/crates/*/build.rs | rust-toolchain* | core/.config/*)
                full "$f changes how the workspace builds or runs its tests" ;;
            .github/* | scripts/oby-check* | scripts/ci-rust-tests*)
                full "$f changes the checks themselves" ;;
            "$obid"/tests/support/*)
                full "$f is the tests' shared support" ;;
            "$obid"/src/lib.rs | "$obid"/src/main.rs)
                # A module declared or taken out (`pub mod x;`) runs nothing by itself: x's own file
                # is in the change. Anything else there, or no range to read it in, is every test.
                if [[ -z "${CI_RUST_RANGE:-}" ]] || ! diff=$(git diff --unified=0 "$CI_RUST_RANGE" -- "$f" 2>/dev/null); then
                    full "$f is obid's crate root"
                fi
                if grep -E '^[-+]' <<<"$diff" | grep -vE '^(\+\+\+|---) ' |
                    grep -vqE '^[-+][[:space:]]*((pub(\([a-z]+\))? )?mod [A-Za-z0-9_]+;)?[[:space:]]*(//.*)?$'; then
                    full "$f is obid's crate root, and the change is more than a module declared"
                fi ;;
            "$obid"/src/*.rs | "$obid"/tests/*)
                mapped+="$f"$'\n' ;;
            "$obid"/src/*)
                full "$f is in obid's src/ and isn't Rust: any module may include it" ;;
            "$obid"/*.md | core/*.md) ;;
            "$obid"/*)
                # A file the crate includes (prompt-budget.txt, cli-versions.json): the modules that
                # name it, as if they had changed.
                found=$(grep -rlF "${f##*/}" --include='*.rs' "$obid/src" || true)
                [[ -n "$found" ]] || full "$f is in obid, and no module of its names it"
                mapped+="$found"$'\n' ;;
            core/crates/*)
                full "$f is in a crate obid builds on" ;;
            e2e/*)
                # The e2e harness's files obid's tests run (its fixture, its stub agent).
                folder=${f#e2e/}
                folder=e2e/${folder%%/*}
                if grep -qF "$folder" "$obid"/tests/support/*.rs; then
                    full "$f is in $folder, which the tests' shared support runs"
                fi
                mapped+=$(grep -lF "$folder" "$obid"/tests/*.rs || true)$'\n' ;;
        esac
    done <<<"$changed"

    if [[ -n "$(sed '/^$/d' <<<"$mapped")" ]]; then
        plan=$(OBY_CHECK_PLAN=1 OBY_CHECK_PATHS="$mapped" bash scripts/oby-check.sh HEAD 2>/dev/null) ||
            full "the Mac's map (scripts/oby-check.sh) couldn't plan this change"
        while IFS= read -r line; do
            case "$line" in
                "==> test_binaries "*)
                    for word in ${line#"==> test_binaries "}; do
                        [[ "$word" == --test ]] || binaries+=("$word")
                    done ;;
                "==> lib_tests "*" --bins"*)
                    full "the Mac's map runs all of obid's lib tests for this change" ;;
                "==> lib_tests "*" -- "*)
                    for filter in ${line#*" -- "}; do modules+=("${filter%%::*}"); done ;;
                "==> cargo "*" test "*"-p "*)
                    full "the Mac's map runs another crate's tests for this change" ;;
            esac
        done <<<"$plan"
        [[ ${#binaries[@]} -gt 0 || ${#modules[@]} -gt 0 ]] ||
            full "the Mac's map names no test for $(sed '/^$/d' <<<"$mapped" | head -n 3 | paste -sd ' ' -)"
        # Widened by obid's imports: the binaries named after each module the map names, or that
        # import it.
        for module in $(printf '%s\n' "${modules[@]}" | sort -u); do
            for test in "$obid"/tests/*.rs; do
                name=${test##*/}
                name=${name%.rs}
                if [[ "_${name}_" == *"_${module}_"* ]] || grep -qE "obid::$module([^A-Za-z0-9_]|$)" "$test"; then
                    binaries+=("$name")
                fi
            done
        done
    fi
fi

targets="-p obid --lib"
filter=$guard_filter
lines=0
extra=()
for name in $(printf '%s\n' "${guard_binaries[@]}" runner_api "${binaries[@]}" | sed '/^$/d' | sort -u); do
    [[ "$name" =~ ^[a-z0-9_]+$ && -f "$obid/tests/$name.rs" ]] ||
        full "the map named tests/$name.rs, which isn't one of obid's test binaries"
    targets+=" --test $name"
    [[ " ${guard_binaries[*]} " != *" $name "* ]] || continue
    # runner_api is built for its guard and runs whole only when the change reaches it.
    [[ "$name" != runner_api || " ${binaries[*]} " == *" runner_api "* ]] || continue
    extra+=("$name")
    filter+=" | binary_id(obid::$name)"
    lines=$((lines + $(wc -l <"$obid/tests/$name.rs")))
done
read -r -a modules <<<"$(printf '%s\n' "${modules[@]}" | sed '/^$/d' | sort -u | tr '\n' ' ')"
for module in "${modules[@]}"; do
    [[ "$module" =~ ^[a-z0-9_]+$ ]] || full "the map named $module, which isn't one of obid's modules"
    filter+=" | (binary_id(obid) & test(/^$module::/))"
done

# A shard for each 20,000 lines of the binaries it runs beyond the guards, at most 4: the full
# suite's 82,000 lines run in 4.
shards=$(((lines + 19999) / 20000))
((shards >= 1)) || shards=1
((shards <= 4)) || shards=4

selected="the guards"
if [[ "$want" == affected ]]; then
    selected+="; test binaries: ${extra[*]:-none}; obid's lib tests of: ${modules[*]:-none}"
fi
say "$want" "$why" "$targets" "$filter" "$shards" "$selected"
