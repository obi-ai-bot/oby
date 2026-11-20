#!/bin/bash
# The checks GitHub fails a pull request on that are cheap to run first (docs/contracts/ci.md,
# CI-69): run this before every push. It prints, for each miss, what to change in plain words.
#
#   scripts/preflight.sh [base]    what changed since the merge base with base (default origin/dev),
#                                  committed or not, untracked files included
#
#   brain       tools/brain's repo checks (test/repo.test.ts): every documented route is served
#               (retired_8), every top-level module of obid is in an area, and the rest of that file
#   coverage    `obid-spec coverage --gate`: every route has a workflow, every scenario of a contract
#               in tools/obid-spec/scope.json is classified
#   workflows   `obid-spec run --workflow <file>` for each workflow under tools/obid-spec/workflows the
#               change touches, against this checkout's obid (a lab workflow is left to run by hand)
#   fmt         `cargo fmt --check` in each Cargo workspace whose Rust changed
#   clippy      `clippy --all-targets --locked -D warnings` on the crates whose Rust changed
#
# PREFLIGHT_ONLY="<step> …" runs only those steps. Exit 0 when every step passed or had nothing to
# check, 1 when a step failed or couldn't run (a step that couldn't run is never counted as passed).
#
# What needs a build, and how long: the first run in a fresh worktree installs tools/brain's npm
# packages and builds three small things with cargo (the Brain's extractor, obid-spec's binary and
# whatever clippy checks), a few minutes; after that the builds are cargo's no-ops and the whole
# script takes about a minute. clippy compiles the crates it checks: a change to obid compiles obid.
set -uo pipefail
cd "$(dirname "$0")/.."
root=$PWD
base_ref=${1:-origin/dev}

tmp=$(mktemp "${TMPDIR:-/tmp}/preflight.XXXXXX")
trap 'rm -f "$tmp"' EXIT
declare -a summary=()
failed=0
say() { printf '%s\n' "$*"; }
fix() { printf '  fix: %s\n' "$*"; }
record() { # step, verdict, why
    summary+=("$(printf '%-9s %s%s' "$1" "$2" "${3:+ ($3)}")")
    if [[ "$2" == failed || "$2" == "not run" ]]; then failed=1; fi
}
wanted() { [[ -z "${PREFLIGHT_ONLY:-}" || " $PREFLIGHT_ONLY " == *" $1 "* ]]; }
need() { # step, tool: say so and record it when the tool is missing
    if command -v "$2" >/dev/null 2>&1; then return 0; fi
    say "preflight: $1 can't run: $2 isn't installed"
    record "$1" "not run" "$2 isn't installed"
    return 1
}

# What changed: the merge base's diff to the working tree, and untracked files.
if mb=$(git merge-base "$base_ref" HEAD 2>/dev/null); then
    changed=$( { git diff --name-only "$mb"; git ls-files --others --exclude-standard; } | sort -u)
    say "preflight: checking what changed since $base_ref ($(printf '%s' "$changed" | grep -c . || true) files)"
else
    changed=""
    everything=1
    say "preflight: no merge base with $base_ref (fetch it?): checking every Rust workspace"
fi
touched() { [[ -n "${everything:-}" ]] || grep -qE "$1" <<<"$changed"; }

# --- brain ----------------------------------------------------------------------------------
brain_step() {
    need brain node && need brain npm && need brain cargo || return
    if [[ ! -d tools/brain/node_modules ]]; then
        say "preflight: brain: installing tools/brain's npm packages (once per worktree)"
        if ! (cd tools/brain && npm_config_update_notifier=false npm ci --no-audit --no-fund --loglevel=error); then
            record brain "not run" "npm ci failed in tools/brain"
            return
        fi
    fi
    say "preflight: brain: the Brain's repo checks (the first run builds its extractor with cargo)"
    local out
    out=$(cd tools/brain && NO_COLOR=1 FORCE_COLOR=0 npx --no-install vitest run test/repo.test.ts 2>&1)
    local code=$?
    # Routes a doc proposes ahead of obid (RETIRED-10): listed, never a failure.
    (cd tools/brain && NO_COLOR=1 npx --no-install tsx -e "import { generateBrain } from './generator/index.ts'; import { config } from './brain.config.ts'; for (const c of generateBrain(config).checks) if (c.kind === 'proposed-route') console.log(c.message);" 2>/dev/null) |
        sed 's/^/- listed, not a failure: /'
    if [[ $code -eq 0 ]]; then
        record brain passed
        return
    fi
    printf '%s\n' "$out" | grep -E '^ *(FAIL|×|✗)|AssertionError|Error:' | head -20
    # Each message the Brain's checks print names what to change: say how, one line each.
    printf '%s\n' "$out" | sed -nE 's/^[[:space:]]*[+-]?[[:space:]]*"(.*)",?$/\1/p' | sort -u | while IFS= read -r m; do
        case "$m" in
            *" documents "*" as proposed, but obid routes it")
                file=${m%% documents *}; route=${m#* documents }; route=${route% as proposed*}
                say "- $file still marks $route as proposed, but obid serves it now."
                fix "take the \"**Proposed: obid doesn't serve\" line off its section in $file, or move $route out of that section" ;;
            *" documents "*", which obid doesn't route")
                file=${m%% documents *}; route=${m#* documents }; route=${route%, which obid*}
                say "- $file documents $route, but obid doesn't serve it."
                fix "serve $route in obid in this change, take it out of $file, or document it in a section that opens with \"**Proposed: obid doesn't serve these routes yet.**\" until obid serves it (RETIRED-10)" ;;
            *" lists "*" as gone, but obid routes it")
                file=${m%% lists *}; route=${m#* lists }; route=${route% as gone*}
                say "- $file lists $route under \"Routes obid does not serve\", but obid serves it."
                fix "take $route out of that list in $file" ;;
            "Module "*" isn't in any area of brain.config.ts")
                mod=${m#Module }; mod=${mod%% *}
                say "- obid's module \`$mod\` is in no area of the Brain's map."
                fix "add '$mod' to the modules of the area whose work it does in tools/brain/brain.config.ts (areas: $(sed -nE "s/^ *\{ id: '([a-z-]+)'.*/\1/p" tools/brain/brain.config.ts | paste -sd, - | sed 's/,/, /g'))" ;;
            "brain.config.ts puts "*" in "*", but obid has no such module")
                mod=${m#brain.config.ts puts }; mod=${mod%% in *}
                say "- tools/brain/brain.config.ts names \`$mod\`, a module obid no longer has."
                fix "take '$mod' out of its area's modules in tools/brain/brain.config.ts" ;;
            *" calls "*", which obid doesn't route")
                say "- $m."
                fix "serve that route in obid, or change the call to a route obid serves" ;;
            "The phone allowlist lets "*)
                say "- $m."
                fix "take that entry out of ROUTES in core/crates/obid/src/gateway.rs, or serve the route" ;;
            "The app keeps "*)
                say "- $m."
                fix "add that key to \`local\` in tools/brain/brain.config.ts with owner 'app', or 'obid' and its issue" ;;
            \"*)
                say "- $m."
                fix "point that rule at code and a test that exist, in \`rules\` in tools/brain/brain.config.ts" ;;
            *" appears in the open-source code")
                say "- $m."
                fix "take that host out of the file" ;;
        esac
    done
    say "  (the full output: cd tools/brain && npx vitest run test/repo.test.ts)"
    record brain failed
}

# --- coverage -------------------------------------------------------------------------------
coverage_step() {
    need coverage cargo && need coverage python3 || return
    say "preflight: coverage: building obid-spec (cargo; seconds once built) and running its gate"
    if ! cargo build --quiet --manifest-path tools/obid-spec/Cargo.toml --bin obid-spec; then
        record coverage "not run" "obid-spec didn't build"
        return
    fi
    local out
    out=$(OBID_SPEC_ROOT="$root" tools/obid-spec/target/debug/obid-spec coverage --gate 2>&1)
    if [[ $? -eq 0 ]]; then
        record coverage passed
        return
    fi
    # The gate's lines, each with what to change in scope.json or the workflows.
    printf '%s\n' "$out" | python3 -c '
import json, sys
scope = json.load(open("tools/obid-spec/scope.json"))
areas = scope["areas"]
def area_of_contract(f):
    return next((a for a, v in areas.items() if f in v.get("contracts", {})), None)
def area_of_route(r):
    path = r.split(" ", 1)[-1]
    best = None
    for a, v in areas.items():
        for p in v.get("routes", []):
            if path == p or path.startswith(p + "/") or path.startswith(p + "?"):
                if best is None or len(p) > len(best[1]):
                    best = (a, p)
    return best[0] if best else None
shown = False
known = ("unclassified: ", "no workflow", "in no area of scope.json: ", "a workflow for a route obid", "proves a scenario no contract")
for raw in sys.stdin:
    line = raw.strip()
    if raw.startswith("  ") and not line.startswith(known) and not line.startswith(("obid'"'"'s scenarios with no assertion yet", "contracts not yet in scope.json")):
        shown = True
        print("- the gate says: " + line)
        continue
    def say(what, how):
        global shown
        shown = True
        print("- " + what)
        print("  fix: " + how)
    if line.startswith("unclassified: "):
        rest = line[len("unclassified: "):]
        f, _, sid = rest.partition(": ")
        a = area_of_contract(f)
        if sid.endswith(" is in scope.json but not in the contract"):
            sid = sid.split(" ")[0]
            say(f"{sid} is in tools/obid-spec/scope.json but docs/contracts/{f} no longer lists it.",
                f"take {sid} out of areas.{a}.contracts.\"{f}\" in tools/obid-spec/scope.json")
        else:
            say(f"docs/contracts/{f} has {sid}, which tools/obid-spec/scope.json does not classify.",
                f"add {sid} to tools/obid-spec/scope.json under area {a}, contract \"{f}\": in \"obid\" when obid itself does it "
                f"(a workflow step must then prove it), else in \"elsewhere\" as \"{sid}\": \"<what tests it instead>\"")
    elif line.startswith("no workflow for the trigger: "):
        t = line.split(": ", 1)[1]
        say(f"the background trigger {t} has no workflow.",
            f"add one under tools/obid-spec/workflows/triggers/ and name its file in triggers.\"{t}\".workflows in tools/obid-spec/scope.json")
    elif line.startswith("no workflow: "):
        r = line.split(": ", 1)[1]
        a = area_of_route(r)
        say(f"{r} is in docs/api/routes.txt and no obid-spec workflow calls it.",
            f"add a workflow for it under tools/obid-spec/workflows/routes/{a}/ (tools/obid-spec/README.md)")
    elif line.startswith("in no area of scope.json: "):
        r = line.split(": ", 1)[1]
        say(f"{r} is in docs/api/routes.txt and in no area of tools/obid-spec/scope.json.",
            "add its path prefix to the routes of the area it belongs to in tools/obid-spec/scope.json, and a workflow for it")
    elif line.startswith("a workflow for a route obid doesn'"'"'t serve: "):
        w = line.split(": ", 1)[1]
        say(f"the workflow {w} calls a route docs/api/routes.txt no longer has.",
            "delete that workflow, or point it at the route that replaced it")
    elif line.startswith("proves a scenario no contract in scope lists: "):
        w = line.split(": ", 1)[1]
        say(f"a workflow step proves {w}, which no contract in tools/obid-spec/scope.json lists.",
            "fix the id in that step'"'"'s ui.proves, or add its contract to scope.json")
if not shown:
    print("- the gate failed and named nothing; its output is below")
'
    say "  (the full output: tools/obid-spec/target/debug/obid-spec coverage --gate)"
    record coverage failed
}

# --- workflows ------------------------------------------------------------------------------
# Each obid-spec workflow the change touches, run against this checkout's obid, so a red one shows
# before CI's run does. A lab workflow makes machines: it's run by hand, never here.
workflows_step() {
    local files f rel area only out any="" bad="" lab=()
    files=$(grep -E '^tools/obid-spec/workflows/.*\.json$' <<<"$changed" || true)
    while IFS= read -r f; do
        [[ -n "$f" && -f "$f" ]] || continue
        rel=${f#tools/obid-spec/workflows/}
        case "$rel" in
            lab/*) area=${rel#lab/}; lab+=("lab-${area%%/*}"); continue ;;
            t3/*) area=${rel#t3/}; only=(--only "t3-${area%%/*}") ;;
            *) only=() ;;
        esac
        if [[ -z "$any" ]]; then
            need workflows cargo || return
            say "preflight: workflows: building obid-spec and obid (cargo; minutes in a fresh worktree)"
            if ! cargo build --quiet --manifest-path tools/obid-spec/Cargo.toml --bins; then
                record workflows "not run" "obid-spec didn't build"
                return
            fi
            any=1
        fi
        say "preflight: workflows: $rel"
        if ! OBID_SPEC_ROOT="$root" tools/obid-spec/target/debug/obid-spec run ${only[@]+"${only[@]}"} --workflow "$rel" >"$tmp" 2>&1; then
            grep -vE '^ *$' "$tmp" | tail -25 | sed 's/^/  | /'
            say "- the workflow $f fails against this checkout's obid."
            fix "read its failed step above and the folder it names; fix the workflow or obid, or mark the step ui.expectedRed with its issue when it's a known bug (tools/obid-spec/README.md)"
            bad=1
        fi
    done <<<"$files"
    for area in ${lab[@]+"${lab[@]}"}; do
        say "preflight: workflows: a lab workflow changed; it makes machines, so run it by hand: tools/obid-spec/target/debug/obid-spec run --only $area"
    done
    if [[ -n "$bad" ]]; then record workflows failed
    elif [[ -n "$any" ]]; then record workflows passed
    elif [[ ${#lab[@]} -gt 0 ]]; then record workflows skipped "only lab workflows changed: run them by hand"
    else record workflows skipped "no workflow changed"; fi
}

# --- fmt and clippy ---------------------------------------------------------------------------
# Each Cargo workspace and what of it changed: core's crates by folder, the tools whole.
workspaces=(core tools/obid-spec tools/oby-check-blok tools/brain/obid-extract)
rust_touched() { touched "^$1/.*(\.rs|Cargo\.toml|Cargo\.lock|build\.rs)$|^$1/rust-toolchain"; }

fmt_step() {
    need fmt cargo || return
    local ws any="" bad=""
    for ws in "${workspaces[@]}"; do
        touched "^$ws/.*\.rs$" || continue
        any=1
        if ! cargo fmt --manifest-path "$ws/Cargo.toml" --all -- --check >"$tmp" 2>&1; then
            grep -E '^Diff in ' "$tmp" | sed -E "s#^Diff in ($root/)?#- not formatted: #; s#:([0-9]+):?\$# (line \1)#" | sort -u | head -20
            fix "cargo fmt --manifest-path $ws/Cargo.toml --all, then commit"
            bad=1
        fi
    done
    if [[ -n "$bad" ]]; then record fmt failed
    elif [[ -n "$any" ]]; then record fmt passed
    else record fmt skipped "no Rust changed"; fi
}

clippy_step() {
    need clippy cargo || return
    local ws any="" bad="" args
    for ws in "${workspaces[@]}"; do
        rust_touched "$ws" || continue
        any=1
        args=()
        if [[ "$ws" == core ]] && [[ -z "${everything:-}" ]] &&
            ! touched '^core/(Cargo\.(toml|lock)|rust-toolchain|\.config/)'; then
            local dir name
            for dir in core/crates/*/; do
                touched "^${dir}" || continue
                name=$(sed -nE 's/^name *= *"(.*)"/\1/p' "$dir/Cargo.toml" | head -1)
                [[ -n "$name" ]] && args+=(-p "$name")
            done
        fi
        [[ "$ws" == core && ${#args[@]} -eq 0 ]] && args=(--workspace)
        say "preflight: clippy: $ws ${args[*]:-} (it compiles what it checks: minutes in a fresh worktree)"
        if ! cargo clippy --quiet --manifest-path "$ws/Cargo.toml" ${args[@]+"${args[@]}"} --all-targets --locked -- -D warnings >"$tmp" 2>&1; then
            grep -E '^(error|warning)|^ *--> ' "$tmp" | grep -v '^error: could not compile' | sed -E "s#--> #--> $ws/#" | head -40
            fix "clear each warning above in $ws (CI's lint job runs clippy with -D warnings)"
            bad=1
        fi
    done
    if [[ -n "$bad" ]]; then record clippy failed
    elif [[ -n "$any" ]]; then record clippy passed
    else record clippy skipped "no Rust changed"; fi
}

for step in brain coverage workflows fmt clippy; do
    if wanted "$step"; then "${step}_step"; fi
done

say ""
say "preflight:"
for line in ${summary[@]+"${summary[@]}"}; do say "  $line"; done
if [[ $failed -ne 0 ]]; then
    say "preflight: fix what's above before you push (GitHub fails the pull request on it)"
    exit 1
fi
say "preflight: clean"
