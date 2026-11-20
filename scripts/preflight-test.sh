#!/bin/bash
# scripts/preflight.sh's own tests (docs/contracts/ci.md, CI-69): each kind of miss it exists to
# catch fails it with the words that say what to change (red), and a tree with none passes (green).
# They run the real script on a copy of this checkout (its working tree, committed or not), with the
# real tools: node, npm, cargo and python3. Builds and npm packages are this checkout's when it has
# them (its target folders and tools/brain/node_modules are linked in), else made in the copy.
#
#   scripts/preflight-test.sh
#   PREFLIGHT_TEST_ONLY=<text> scripts/preflight-test.sh    only the cases whose name holds <text>
#                                                          (each other case's change is undone)
set -uo pipefail
cd "$(dirname "$0")/.."
src=$PWD

fails=0
ok() { echo "ok: $1"; }
bad() { echo "FAILED: $1" >&2; fails=$((fails + 1)); }

copy=$(mktemp -d "${TMPDIR:-/tmp}/preflight-test.XXXXXX")
trap 'rm -rf "$copy"' EXIT
git ls-files -z --cached --others --exclude-standard | while IFS= read -r -d '' f; do
    [[ -e "$f" || -L "$f" ]] && printf '%s\0' "$f"
done | tar -cf - --null -T - | tar -xf - -C "$copy"
cd "$copy" || exit 1
git init -q && git add -A && git -c user.name=preflight-test -c user.email=preflight-test@localhost commit -qm base || exit 1
links=(core/target tools/obid-spec/target tools/oby-check-blok/target tools/brain/obid-extract/target tools/brain/node_modules)
for l in "${links[@]}"; do
    echo "/$l" >>.git/info/exclude
    [[ -d "$src/$l" ]] && ln -s "$src/$l" "$l"
done

reset() { git checkout -q -- . && git clean -qfd; }
# case, steps, expected exit, then each line the output must have
check() {
    local name=$1 steps=$2 want=$3 out code line
    shift 3
    if [[ -n "${PREFLIGHT_TEST_ONLY:-}" && "$name" != *"$PREFLIGHT_TEST_ONLY"* ]]; then
        reset
        return
    fi
    out=$(PREFLIGHT_ONLY="$steps" scripts/preflight.sh HEAD 2>&1)
    code=$?
    if [[ $code -eq $want ]]; then ok "$name: exit $code"; else bad "$name: exit $code, not $want"; echo "$out" | tail -30 >&2; fi
    for line in "$@"; do
        if grep -qF -- "$line" <<<"$out"; then ok "$name: says '$line'"; else bad "$name: doesn't say '$line'"; echo "$out" | tail -30 >&2; fi
    done
    reset
}

# CI-69 red: a documented route obid doesn't serve (the Brain's retired_8).
sed -i.bak '1a\
Also `GET /v1/preflight-nowhere`.
' docs/runner-api.md && rm docs/runner-api.md.bak
check "CI-69 retired_8" brain 1 \
    "docs/runner-api.md documents GET /v1/preflight-nowhere, but obid doesn't serve it." \
    "fix: serve GET /v1/preflight-nowhere in obid in this change, take it out of docs/runner-api.md, or document it in a section that opens with"

# CI-69 red: a route obid serves, still documented as proposed (RETIRED-10).
sed -i.bak '0,/^### /{/^### /a\
\
**Proposed: obid doesn'"'"'t serve these routes yet.**
}' docs/api/browser.md && rm docs/api/browser.md.bak
check "CI-69 proposed but served" brain 1 \
    "docs/api/browser.md still marks GET /v1/browser as proposed, but obid serves it now." \
    "fix: take the \"**Proposed: obid doesn't serve\" line off its section in docs/api/browser.md"

# CI-69 green: a route documented ahead of obid, in a section marked proposed, is listed, not a failure.
printf '\n### A docs-first section\n\n**Proposed: obid doesn'"'"'t serve these routes yet.**\n\n- `GET /v1/preflight-ahead` lists them.\n' >>docs/api/settings.md
check "CI-69 proposed, not yet served" brain 0 \
    "- listed, not a failure: docs/api/settings.md proposes GET /v1/preflight-ahead, not yet served" \
    "brain     passed"

# CI-69 red: a top-level module of obid in no area of the Brain's map.
printf '//! preflight-test: a module in no area.\n' >core/crates/obid/src/preflight_probe.rs
printf 'pub mod preflight_probe;\n' >>core/crates/obid/src/lib.rs
check "CI-69 module area" brain 1 \
    "obid's module \`preflight_probe\` is in no area of the Brain's map." \
    "fix: add 'preflight_probe' to the modules of the area whose work it does in tools/brain/brain.config.ts"

# CI-69 red: a contract's scenario scope.json doesn't classify.
printf '\n- **SPHERE-999** (proposed). Given preflight-test, then nothing.\n' >>docs/contracts/obysphere.md
check "CI-69 unclassified scenario" coverage 1 \
    "docs/contracts/obysphere.md has SPHERE-999, which tools/obid-spec/scope.json does not classify." \
    "fix: add SPHERE-999 to tools/obid-spec/scope.json under area sphere, contract \"obysphere.md\""

# CI-69 red: a route with no workflow, in an area and in none.
printf 'GET /v1/spheres/preflight-probe\nGET /v1/preflight-orphan\n' >>docs/api/routes.txt
check "CI-69 route without a workflow" coverage 1 \
    "GET /v1/spheres/preflight-probe is in docs/api/routes.txt and no obid-spec workflow calls it." \
    "fix: add a workflow for it under tools/obid-spec/workflows/routes/sphere/" \
    "GET /v1/preflight-orphan is in docs/api/routes.txt and in no area of tools/obid-spec/scope.json."

# CI-69 red: a workflow the change touches fails against this checkout's obid.
wf=tools/obid-spec/workflows/routes/everything-else/get-alerts.json
python3 -c 'import sys; p=sys.argv[1]; s=open(p).read(); open(p, "w").write(s.replace("\"status\": 401", "\"status\": 402", 1))' "$wf"
check "CI-69 workflow red" workflows 1 \
    "preflight: workflows: routes/everything-else/get-alerts.json" \
    "the workflow $wf fails against this checkout's obid." \
    "fix: read its failed step above and the folder it names"

# CI-69 green: a workflow the change touches that passes.
python3 -c 'import sys; p=sys.argv[1]; s=open(p).read(); open(p, "w").write(s.replace("What should reach the owner now", "What reaches the owner now", 1))' "$wf"
check "CI-69 workflow green" workflows 0 "workflows passed"

# CI-69 green: a lab workflow is left to run by hand, never run here.
lab=$(find tools/obid-spec/workflows/lab -name '*.json' | sort | head -1)
printf '\n' >>"$lab"
check "CI-69 workflow lab" workflows 0 \
    "run it by hand: tools/obid-spec/target/debug/obid-spec run --only lab-" \
    "workflows skipped (only lab workflows changed: run them by hand)"

# CI-69 red: Rust cargo fmt would change.
printf '\n/// preflight-test.\npub fn preflight_probe( ) -> u32 {1}\n' >>core/crates/obi-protocol/src/card.rs
check "CI-69 fmt" fmt 1 \
    "not formatted: core/crates/obi-protocol/src/card.rs" \
    "fix: cargo fmt --manifest-path core/Cargo.toml --all, then commit"

# CI-69 red: a clippy warning in the crate that changed.
printf '\n/// preflight-test.\npub fn preflight_probe() -> u32 {\n    let x: u32 = 1;\n    x.clone()\n}\n' >>core/crates/obi-protocol/src/card.rs
check "CI-69 clippy" clippy 1 \
    "clippy: core -p obi-protocol" \
    "using \`clone\` on type \`u32\` which implements the \`Copy\` trait" \
    "--> core/crates/obi-protocol/src/card.rs:" \
    "fix: clear each warning above in core"

# CI-69 green on macOS's /bin/bash, 3.2 (the script's own shebang): a change to only a tool's Rust
# (tools/obid-spec) leaves clippy's crate list empty, which bash 3.2 under `set -u` calls unbound.
# fmt and clippy run on a stand-in cargo that checks nothing, so this case needs no build.
printf '\n// preflight-test.\n' >>tools/obid-spec/src/lib.rs
mkdir -p .stand-in && printf '#!/bin/sh\nexit 0\n' >.stand-in/cargo && chmod +x .stand-in/cargo
PATH="$PWD/.stand-in:$PATH" check "CI-69 bash 3.2, a tool's Rust alone" "fmt clippy" 0 \
    "fmt       passed" "clippy    passed" "preflight: clean"
rm -rf .stand-in
PATH="$PWD/.stand-in:$PATH" check "CI-69 bash 3.2, no step asked for" "none-of-them" 0 "preflight: clean"

# CI-69 green: a clean Rust change, every step.
printf '\n/// preflight-test.\npub fn preflight_probe() -> u32 {\n    1\n}\n' >>core/crates/obi-protocol/src/card.rs
check "CI-69 clean" "" 0 \
    "brain     passed" "coverage  passed" "fmt       passed" "clippy    passed" "preflight: clean"

# CI-69 green: nothing changed, so fmt and clippy have nothing to check.
check "CI-69 nothing changed" "" 0 \
    "workflows skipped (no workflow changed)" \
    "fmt       skipped (no Rust changed)" "clippy    skipped (no Rust changed)" "preflight: clean"

if [[ $fails -gt 0 ]]; then
    echo "preflight-test: $fails failed" >&2
    exit 1
fi
echo "preflight-test: all passed"
