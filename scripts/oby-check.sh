#!/bin/bash
# This repository's check, the one obid runs for its jobs (`repos[].check`, docs/runner-api.md,
# "obid only: configuration"): only the suites what the job changed reaches, then gitleaks. In a
# job's check there's no network but loopback, so what the suites fetch (crates, Swift packages,
# the bundled tools' downloads) is what the agent fetched while it worked, into its own HOME, which
# the check keeps: run this yourself first.
#
#   scripts/oby-check.sh [base]    what changed since the merge base with base (default origin/dev),
#                                  committed or not; with OBY_CHECK_PLAN=1, only says what it'd run;
#                                  with OBY_CHECK_ALL=1, runs everything, whatever changed;
#                                  with OBY_CHECK_ONLY=rust|electron|swift|factory|scan, runs only that group;
#                                  with OBY_CHECK_SHARD=<k>/<n> (1 <= k <= n <= 64, no leading zero), the
#                                  rust group runs only shard k's planned obid test binaries, and the rest
#                                  of the rust group only when k is 1: the tree's tests/*.rs ranked by lines,
#                                  most first (ties by path), are dealt round-robin over the n shards (the
#                                  gate's, docs/contracts/remote-gate.md, REMOTE-58 to 60); any other value
#                                  ends 2 with nothing run;
#                                  with OBY_CHECK_ONLY=limits, only the limits below, obid's line
#                                  count reported and AGENTS.md's bytes judged, whatever changed
#                                  (GitHub's `limits` job, docs/contracts/ci.md, CI-35, CI-49);
#                                  with OBY_CHECK_PATHS (one a line), runs what those paths reach:
#                                  obid sets it when it runs this on a job's base, which changed
#                                  nothing, to see whether the base fails as the job does
#                                  (docs/contracts/publishing.md, PUBLISHING-18); with neither
#                                  OBY_CHECK_PLAN nor OBY_CHECK_ONLY, the installed
#                                  `oby-check-blok check [base]` runs it, group by group (below)
#
#   core/                 the limits below (obid's lines reported); cargo fmt --check; clippy -D warnings on the touched
#                         crates; obid's lib tests of each touched module (src/a/b.rs: `a::b::`; all of them for
#                         lib.rs, main.rs, build.rs, a file under src/ that isn't Rust, its
#                         Cargo.toml, or the workspace's Cargo.toml or Cargo.lock), its touched
#                         test binaries, tests/<module>.rs or tests/<module>_api.rs of a touched
#                         module, and the binaries that name a touched folder under tests/
#                         (support, agents, plugins); tests/jobs_setup.rs for src/jobs/setup.rs,
#                         sandbox.rs, egress.rs, process.rs or secretspec.rs, and, outside a job's
#                         box, wherever tests/jobs_api.rs runs; tests/released_files.rs for
#                         src/job.rs, config.rs or store.rs; every test of another touched crate;
#                         an obid run whose failed tests are all on the flaky list
#                         (core/.config/nextest.toml, which may only shrink) runs them once more
#   docs/api/, obid's src/ files with a `.route(`, apps/electron/src (not its tests or mock), apps/shared
#                         the Rust group above with tests/routes_guard.rs (docs/contracts/ci.md)
#   apps/ios/, apps/watch/, apps/shared/, apps/Oby.xcodeproj, character/
#                         the touched iOSTests suites on a Simulator, else a build-for-testing
#   apps/electron/        npm run typecheck, npm test and npm run build there (its node_modules from npm install)
#   scripts/oby-check*    scripts/oby-check-test.sh and the limits below
#   scripts/oby-nightly*  scripts/oby-nightly-test.sh, the nightly's own tests (docs/contracts/ci.md, CI-53 to 55)
#   AGENTS.md             the limits below, AGENTS.md's bytes among them (the root's only; CI-36 to 38)
#   AGENTS.md, core/AGENTS.md, core/CLAUDE.md, docs/agents-rules-manifest.md and the files it names
#                         AGENTS.md's rules: each line the manifest lists is in its home, with no cargo
#                         step for core/AGENTS.md or core/CLAUDE.md (CI-42 to 44)
#   factory/bin/, factory/checks.sh, factory/tests/, factory/config.json
#                         python3 -m unittest discover -s factory/tests, with no GitHub login
#
# Inside a job's sandbox (where Seatbelt can't make another: a no-op sandbox-exec fails), the Apple
# apps aren't built: SwiftPM evaluates every package manifest in a sandbox of its own, which can't
# nest there, and xcodebuild has no switch for it, and a cold build in the job's HOME doesn't fit the check's 45 minutes. Nothing runs them by itself
# since obid's Mac checks went (docs/runner-api.md, "Mac check"): a `mac` workflow's merge waits
# for the owner, who runs them on the Mac.
# obid's tests that make a sandbox of their own (tests/sandbox.rs, tests/jobs_setup.rs, plug-ins' if
# they return) can't run there either: a change they'd test, with nothing else failing, ends the
# check there with `couldn't judge: …`, so it never merges untested (the owner's review of #1944)
# and obid leaves it to the owner's Publish by hand (docs/contracts/publishing.md, PUBLISHING-22);
# it passes only outside a job's sandbox.
#
# Builds and tests run as many jobs at once as obid says the Mac's memory leaves them (OBY_BUILD_JOBS,
# CARGO_BUILD_JOBS, RUST_TEST_THREADS: one per 1.5 GiB of the room memory leaves, at most half the
# cores), for Cargo, Rust's tests and xcodebuild (-jobs). Run by hand, outside obid, every one gets
# 4, on purpose: obid runs one check at a time, and a Mac has other work. obid's test binaries are
# the exception (test_binaries, docs/contracts/ci.md, CI-51): they mostly wait on stand-ins, so by
# hand as many run at once as half the Mac's cores, 2 threads each, and a full run fits its limit.
set -eo pipefail
# The Mac's cores and its last minute's load average, as "<cores> <load>"; a load that can't be read
# is 0.
cpus_and_load() {
    local cores load
    cores=$(sysctl -n hw.ncpu 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)
    load=$({ sysctl -n vm.loadavg 2>/dev/null || true; } | awk '{ print $2 }')
    [[ -n "$load" ]] || load=$(awk '{ print $1 }' /proc/loadavg 2>/dev/null || true)
    echo "$cores ${load:-0}"
}
jobs="${OBY_BUILD_JOBS:-4}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$jobs}" RUST_TEST_THREADS="${RUST_TEST_THREADS:-$jobs}"
cd "$(dirname "$0")/.."

# The check is the installed oby-check-blok's (BLOK-1, docs/contracts/publishing.md, PUBLISHING-26):
# its built-in workflow (BLOK-2) runs this script's groups, each with OBY_CHECK_INNER and
# OBY_CHECK_ONLY set, so it never comes back here. OBY_CHECK_PLAN and OBY_CHECK_ONLY stay this
# script's own. The first one on PATH is resolved through its links once, and that path is the one
# vetted and run: it and the nodes binary beside it (resolved too) must be regular files of this
# user's or root's that nobody else may write, with no access list, in a folder of this user's or
# root's that nobody else may write, with none either, and outside the checked repository and
# TMPDIR, which a job's code can write. A path or one of those folders with a newline in it is
# refused. So a binary planted earlier on PATH, or a link into what the job wrote, never judges.
# Else nothing passes, as without gitleaks: this script never runs its own check instead.
# ponytail: the folders above the binary's aren't vetted (a home whose ~/.cargo others may write
# could swap its bin), nor a folder the owner put first on PATH themselves; on Linux an access list
# is seen only as GNU ls's `+`.
resolved() { # a path through its links, never one with a newline (a trailing one would be lost)
    local out
    out=$({ /bin/realpath "$1" 2>/dev/null || /usr/bin/realpath "$1" 2>/dev/null ||
        /usr/bin/perl -MCwd -e '$p = Cwd::abs_path($ARGV[0]); defined $p && -e $p or exit 1; print "$p\n"' "$1"; } && echo .) || return 1
    out=${out%.}
    out=${out%$'\n'}
    [[ "$out" == /* && "$out" != *$'\n'* ]] || return 1
    printf '%s' "$out"
}
shut() { # a file or folder: why someone else may change it, or nothing
    local listing mode links uid
    if [[ "$OSTYPE" == darwin* ]]; then
        listing=$(/bin/ls -lden "$1" 2>/dev/null) || true
    else
        listing=$(/bin/ls -ldn "$1" 2>/dev/null) || true
    fi
    read -r mode links uid _ <<<"$listing"
    [[ "$mode" == [-d]* ]] || { echo "its mode can't be read: $1"; return; }
    [[ "$uid" == "$EUID" || "$uid" == 0 ]] || { echo "$1 belongs to someone else"; return; }
    [[ "${mode:5:1}${mode:8:1}" != *w* ]] || { echo "others may write $1"; return; }
    # macOS's `ls -e` lists an access list's entries under the first line; GNU's ls marks it `+`.
    [[ "$listing" != *$'\n'* && "${mode:10:1}" != + ]] || { echo "$1 has an access list"; return; }
}
refused() { # a resolved binary: why it isn't one the Mac installed, or nothing
    local folder at why unsafe
    [[ -f "$1" ]] || { echo "$1 isn't a regular file"; return; }
    folder=${1%/*}
    for at in "$1" "${folder:-/}"; do
        why=$(shut "$at")
        [[ -z "$why" ]] || { echo "$why"; return; }
    done
    for unsafe in "$PWD" "${TMPDIR:-}"; do
        [[ -n "$unsafe" ]] || continue
        [[ "$unsafe" != *$'\n'* ]] || { echo "a folder the checked code can write has a newline in its name"; return; }
        # One that isn't there holds nothing.
        unsafe=$(resolved "$unsafe") || continue
        [[ "$1" != "${unsafe%/}"/* ]] || { echo "$1 is inside $unsafe, which the checked code can write"; return; }
    done
}
# OBY_CHECK_SHARD (above), refused before anything runs, oby-check-blok's hand-off included.
shard_k='' shard_n=''
if [[ -n "${OBY_CHECK_SHARD:-}" ]]; then
    if [[ "$OBY_CHECK_SHARD" =~ ^([1-9][0-9]?)/([1-9][0-9]?)$ ]] &&
        ((BASH_REMATCH[1] <= BASH_REMATCH[2] && BASH_REMATCH[2] <= 64)); then
        shard_k=${BASH_REMATCH[1]} shard_n=${BASH_REMATCH[2]}
    else
        echo 'oby-check: invalid OBY_CHECK_SHARD' >&2
        exit 2
    fi
fi
if [[ -z "${OBY_CHECK_INNER:-}${OBY_CHECK_PLAN:-}${OBY_CHECK_ONLY:-}" ]]; then
    found=$(command -v oby-check-blok || true)
    if [[ "$found" != /* ]]; then
        why="it isn't installed on PATH (cargo install --locked --path tools/oby-check-blok)"
    elif ! blok=$(resolved "$found"); then
        why="the one on PATH doesn't resolve, or has a newline in its path"
    else
        why=$(refused "$blok")
        if [[ -z "$why" ]]; then
            if nodes=$(resolved "${blok%/*}/oby-check-blok-nodes"); then
                why=$(refused "$nodes")
            else
                why="no oby-check-blok-nodes beside $blok, or one with a newline in its path"
            fi
        fi
    fi
    if [[ -n "$why" ]]; then
        echo "oby-check: oby-check-blok can't judge this: nothing passes unchecked" >&2
        echo "oby-check: failed: oby-check-blok ($why)" >&2
        exit 1
    fi
    # The path resolved once and vetted, never PATH's again: the nodes binary it starts beside
    # itself is the one vetted here.
    exec "$blok" check "${1:-origin/dev}"
fi

base=$(git merge-base "${1:-origin/dev}" HEAD) || {
    echo "oby-check: can't find where the change starts (${1:-origin/dev})" >&2
    exit 2
}
if [[ -n "${OBY_CHECK_PATHS:-}" ]]; then
    changed=$OBY_CHECK_PATHS
else
    changed=$({ git diff --name-only "$base"; git ls-files --others --exclude-standard; } | sort -u)
fi
# Xcode's and SwiftPM's builds, kept between runs (in a job, in its HOME).
cache="${OBY_CHECK_CACHE:-$HOME/Library/Caches/oby-check}"

# Raising jobs_rs_limit is the owner's word: this is the check, so a job's own change to it is
# checked by the base's copy and merges as a change to the check does (docs/contracts/merging.md). A
# raise of it comes in a pull request that changes only this script, with a `# payback: #<issue>` or
# `# payback: <task key>` line directly above it (docs/contracts/ci.md, CI-39 to 41).
# jobs.rs alone may only shrink: past this it fails (the owner's decision of 2026-11-06, "1.3 Both").
jobs_rs_limit=6305
# obid's tests may only lose sleeps: a test waits for state with support::within (docs/contracts/ci.md, CI-12, CI-13).
tests_sleep_limit=203
# AGENTS.md may only shrink: a cut lowers this, and raising it is the owner's word (docs/contracts/ci.md, CI-36 to 38).
# payback: A00-55 root summaries of the login, vault and Tetrix rules (owner, 2026-11-14)
agents_md_bytes_limit=29203
# A job's check runs the base's copy of this script on the commit (PUBLISHING-7). A branch that
# changed a limit since it started from its merge base (a cut lowers it) is held to its own number,
# but never to one higher than the running copy's; one that didn't gets the running copy's, an
# owner's raise on dev included. When dev's limit moved since the branch started, its messages say
# to rebase.
limit_in() { # a limit in the copy of this script at a commit
    git show "$1:scripts/oby-check.sh" 2>/dev/null | sed -n "s/^$2=\([0-9]\{1,9\}\)\$/\1/p" | head -n 1 || true
}
limit_moved=()
pick_limit() { # name: sets it from the commit's copy where the branch changed it
    local running=${!1} started committed
    started=$(limit_in "$base" "$1")
    committed=$(limit_in HEAD "$1")
    if [[ -n "$committed" && "$committed" != "$started" ]] && ((committed < running)); then
        printf -v "$1" %s "$committed"
    fi
    if [[ -n "$started" && "$started" != "$running" && "$committed" != "$running" ]]; then
        limit_moved+=("$1")
    fi
}
pick_limit jobs_rs_limit
pick_limit tests_sleep_limit
pick_limit agents_md_bytes_limit
# Said after a limit's failure: dev's limit changed since this branch started.
moved() { [[ " ${limit_moved[*]} " != *" $1 "* ]] || echo " (dev's limit changed since this branch started: rebase onto dev)"; }
above_limit() { # the line directly above a limit in the commit's copy of this script
    git show "HEAD:scripts/oby-check.sh" 2>/dev/null | awk -v name="$1" '$0 ~ "^" name "=[0-9]+$" { print above; exit } { above = $0 }' || true
}
# Said after a limit's own message: its payback, when the line above it is one (CI-41).
paid() {
    local above
    above=$(above_limit "$1")
    [[ "$above" != '# payback: '* ]] || echo "oby-check: $1's payback: ${above#'# payback: '}"
}
payback_form='^# payback: (#[0-9]+|[A-Z][A-Z0-9]*-[0-9]+)( .*)?$'
# A file counts all but its test modules: from a `#[cfg(test)]` on a `mod name {` to the next `}` in
# column 0 (rustfmt's). A `#[cfg(test)]` on a helper or an out-of-line `mod name;` counts.
production_lines='
    FNR == 1 { n += held; held = 0; skip = 0 }
    skip { if (/^}/) skip = 0; next }
    held && /^[[:space:]]*$/ { held++; next }
    held && /^(pub(\([^)]*\))? )?mod [A-Za-z0-9_]+ *\{/ { held = 0; skip = !/\}[[:space:]]*$/; next }
    held { n += held; held = 0 }
    /^#\[cfg\(test\)\]/ { held = 1; next }
    { n++ }
    END { print n + held }'
# The Sphere's and its protocol's don't count, but the Sphere's inbox does (the owner's decision of
# 2026-11-06). A new crate counts.
obid_lines() {
    find core/crates -path '*/src/*' -name '*.rs' -not -path 'core/crates/obi-protocol/*' \
        \( -not -path 'core/crates/obid/src/sphere/*' -o -path core/crates/obid/src/sphere/inbox.rs \
        -o -path core/crates/obid/src/sphere/link/inbox_actions.rs \) \
        -exec awk "$production_lines" {} + |
        awk '{ total += $1 } END { print total + 0 }'
}
# obid_lines at the merge base, by the same rule, on the base's core/crates taken out of git; it fails
# when git can't give them (none in the base's tree, or an object it can't read).
base_obid_lines() {
    local dir status=0
    # In TMPDIR, named: macOS's `mktemp -d` alone ignores TMPDIR, and a job's box can't write the
    # Mac's own temporary folder.
    dir=$(mktemp -d "${TMPDIR:-/tmp}/oby-check.XXXXXX") || return 1
    if git archive "$base" core/crates 2>/dev/null | tar -x -C "$dir" 2>/dev/null; then
        (cd "$dir" && obid_lines) || status=1
    else
        status=1
    fi
    rm -rf "$dir"
    return "$status"
}
jobs_rs_lines() {
    if [[ -f core/crates/obid/src/jobs.rs ]]; then awk "$production_lines" core/crates/obid/src/jobs.rs; else echo 0; fi
}
# Each `thread::sleep` and `time::sleep` in obid's tests, however many share a line.
tests_sleeps() {
    if [[ -d core/crates/obid/tests ]]; then
        { grep -rEo '(thread|time)::sleep' --include='*.rs' core/crates/obid/tests || true; } | wc -l | tr -d ' '
    else
        echo 0
    fi
}
# The flaky list (docs/contracts/ci.md, CI-26 to 28), from a copy of core/.config/nextest.toml on
# stdin: an entry a line, its binary id, its test's full name and the task that fixes it, tab-separated,
# or `malformed` and its first two lines for an override not in the list's shape. No file, no entry.
flaky_list() {
    local lines=() line i id test q="'"
    # Each entry is these four lines: its task, its header, one exact test and two retries.
    local comment='^# ([A-Z][A-Z0-9]*-[0-9]+): [^ ].*$'
    local filter="filter = ${q}binary_id("
    local named="^(obid(::[a-z0-9_]+)?)[)] & test[(]=([A-Za-z0-9_:]+)[)]${q}\$"
    # Every override is an entry: a table or key naming overrides that isn't one is malformed.
    local override="^[[:space:]]*([[].*overrides|[^=#${q}]*overrides[^=]*=)"
    while IFS= read -r line || [[ -n "$line" ]]; do lines+=("$line"); done
    for ((i = 0; i < ${#lines[@]}; i++)); do
        line=${lines[i]}
        [[ "$line" =~ $override ]] || continue
        if [[ "$line" == '[[profile.ci.overrides]]' && "${lines[i + 1]}" == "$filter"* &&
            "${lines[i + 1]#"$filter"}" =~ $named ]]; then
            id=${BASH_REMATCH[1]} test=${BASH_REMATCH[3]}
            if [[ "${lines[i + 2]}" == 'retries = 2' ]] && ((i > 0)) && [[ "${lines[i - 1]}" =~ $comment ]]; then
                printf '%s\t%s\t%s\n' "$id" "$test" "${BASH_REMATCH[1]}"
                continue
            fi
        fi
        printf 'malformed\t%s %s\n' "$line" "${lines[i + 1]}"
    done
}
flaky_at() { # base or tree: the flaky list there
    if [[ "$1" == tree ]]; then
        [[ ! -f core/.config/nextest.toml ]] || flaky_list <core/.config/nextest.toml
    else
        { git show "$1:core/.config/nextest.toml" 2>/dev/null || true; } | flaky_list
    fi
}
# The failed tests a libtest run's output (stdin) names, a line each, only when every target it
# started ended (a `test result:` for each `running N tests`) and the names are as many as its
# summaries count failed. Else it fails: a crash, a build error or a garbled summary names nothing.
failed_tests() {
    awk '
        /^running [0-9]+ tests?$/ { ran++ }
        /^test result: / { ended++ }
        /^test result: FAILED\. / { n = $0; sub(/ failed.*/, "", n); sub(/.* /, "", n); want += n; summaries++ }
        names && /^    [^ ]+$/ { print substr($0, 5); got++; next }
        { names = /^failures:$/ }
        END { exit !(summaries && ran == ended && got == want) }'
}
# A failed run of binary `$1` (`obid` for the lib, `obid::<name>` for tests/<name>.rs), its output on
# stdin: the tests to run once more, a line each, when every one it failed is on the flaky list at the
# base and in the checked tree (CI-26). Else it fails, and nothing is retried (CI-27).
flaky_retry() {
    local names listed test
    names=$(failed_tests) || return 1
    listed=$(LC_ALL=C comm -12 <(flaky_at "$base" | cut -f1,2 | LC_ALL=C sort -u) \
        <(flaky_at tree | cut -f1,2 | LC_ALL=C sort -u))
    while IFS= read -r test; do
        grep -qxF "$1	$test" <<<"$listed" || return 1
    done <<<"$names"
    printf '%s\n' "$names"
}

failed=() unjudged=()
case "${OBY_CHECK_ONLY:-}" in
    ''|rust|electron|swift|factory|scan|limits) ;;
    *) echo 'oby-check: invalid OBY_CHECK_ONLY' >&2; exit 2 ;;
esac
group=rust
selected() { [[ -z "${OBY_CHECK_ONLY:-}" || "$OBY_CHECK_ONLY" == "$group" ]]; }
# Inside a job's sandbox, where Seatbelt can't make another.
nested=''
/usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null || nested=1
step() {
    selected || return 0
    plan_group
    echo "==> $*"
    [[ -n "${OBY_CHECK_PLAN:-}" ]] || "$@" || failed+=("$*")
}

planned_groups=' '
plan_group() {
    if [[ -n "${OBY_CHECK_PLAN:-}" && "$planned_groups" != *" $group "* ]]; then
        echo "oby-check: group $group"
        planned_groups+="$group "
    fi
}

# obid's test binaries (`--test name` each), built once, then run `jobs` at a time, the longest
# first, every one to its end. `cargo test` runs them one after another and stops at the first that
# fails, and most of their time is spent waiting on an agent's stand-in, not working. Each binary
# runs with OBY_TEST_THREADS test threads (2 by default) and `--quiet`, half as many binaries at once
# as obid's `jobs` (at least one), so threads in all stay within obid's memory budget, or by hand half
# the Mac's cores (at least 2: CI-51); its output comes
# whole: the ones that passed first, the ones that failed last, so a failure stays in the check's
# tail, each with the seconds it ran, and a last line says how long they all took and which took
# longest (CI-52). Stopped (the check's 45 minutes), it names the binaries still running.
# ponytail: the longest first by its source's lines; keep each binary's last time if that's off.
test_binaries() {
    local dir list status=0 exe name want got flaky again test at_once began
    # In TMPDIR, named: macOS's `mktemp -d` alone ignores TMPDIR, and a job's box can't write the
    # Mac's own temporary folder.
    dir=$(mktemp -d "${TMPDIR:-/tmp}/oby-check.XXXXXX")
    if ! "${cargo[@]}" test "${manifest[@]}" -p obid "$@" --no-run --message-format=json-render-diagnostics >"$dir/built"; then
        rm -rf "$dir"
        return 1
    fi
    list="$dir/list"
    grep '"kind":\["test"\]' "$dir/built" |
        sed -n 's/.*"src_path":"\([^"]*\)".*"executable":"\([^"]*\)".*/\1	\2/p' |
        while IFS=$'\t' read -r src exe; do printf '%s\t%s\n' "$(wc -l <"$src")" "$exe"; done |
        sort -rn | cut -f2 >"$list"
    want=$(($# / 2))
    got=$(($(wc -l <"$list")))
    if ((got != want)); then
        echo "oby-check: found $got of the $want test binaries cargo built" >&2
        rm -rf "$dir"
        return 1
    fi
    # Stopped: say which binaries hadn't ended, so a hang is named, and leave nothing behind.
    stopped_binaries() {
        while IFS= read -r exe; do
            # Started (its .out exists) and not ended: a binary still queued in xargs isn't named.
            name=${exe##*/}
            [[ -f "$dir/$name.out" && ! -f "$dir/$name.status" ]] && echo "oby-check: tests/${name%-*}.rs was still running" >&2
        done <"$list"
        rm -rf "$dir"
        exit 143
    }
    trap stopped_binaries TERM
    # Where cargo runs them: the crate's folder. A binary's seconds are written before its status,
    # which says it ended.
    if [[ -n "${OBY_BUILD_JOBS:-}" ]]; then
        at_once=$(((jobs + 1) / 2))
    else
        at_once=$(cpus_and_load | awk '{ n = int($1 / 2); print (n < 2 ? 2 : n) }')
    fi
    began=$SECONDS
    echo "oby-check: $got test binaries, $at_once at once with ${OBY_TEST_THREADS:-2} threads each ($(cpus_and_load | awk '{ print $1 " CPUs, a load average of " $2 }'))"
    (cd core/crates/obid && tr '\n' '\0' <"$list" | RUST_TEST_THREADS="${OBY_TEST_THREADS:-2}" \
        xargs -0 -n 1 -P "$at_once" sh -c 's=$(date +%s); "$2" --quiet >"$1/${2##*/}.out" 2>&1; e=$?; echo $(($(date +%s) - s)) >"$1/${2##*/}.seconds"; echo $e >"$1/${2##*/}.status"' _ "$dir")
    trap - TERM
    # Then, one at a time, a binary whose failed tests are all on the flaky list runs them once more,
    # and passes if they do (CI-26, CI-27).
    while IFS= read -r exe; do
        name=${exe##*/}
        [[ "$(cat "$dir/$name.status" 2>/dev/null)" == 101 ]] || continue
        flaky=$(flaky_retry "obid::${name%-*}" <"$dir/$name.out") || continue
        again=()
        while IFS= read -r test; do again+=("$test"); done <<<"$flaky"
        if (cd core/crates/obid && RUST_TEST_THREADS="${OBY_TEST_THREADS:-2}" "$exe" --quiet --exact "${again[@]}") \
            </dev/null >"$dir/$name.again" 2>&1; then
            echo 0 >"$dir/$name.status"
            printf '%s\n' "$flaky" >"$dir/$name.flaky"
        fi
    done <"$list"
    for pass in ok failed; do
        while IFS= read -r exe; do
            name=${exe##*/}
            if [[ "$(cat "$dir/$name.status" 2>/dev/null)" == 0 ]]; then
                [[ $pass == ok ]] || continue
            else
                [[ $pass == failed ]] || continue
                status=1
            fi
            echo "     Running ${name%-*} ($(cat "$dir/$name.seconds" 2>/dev/null)s)"
            cat "$dir/$name.out"
            if [[ -f "$dir/$name.again" ]]; then
                echo "     Running ${name%-*} again, its listed flaky tests only"
                cat "$dir/$name.again"
            fi
            [[ ! -f "$dir/$name.flaky" ]] || sed "s/^/retried flaky: obid::${name%-*} /" "$dir/$name.flaky"
            [[ $pass == ok ]] || echo "oby-check: tests/${name%-*}.rs failed" >&2
        done <"$list"
    done
    while IFS= read -r exe; do
        name=${exe##*/}
        printf '%s\t%s\n' "$(cat "$dir/$name.seconds" 2>/dev/null || echo 0)" "${name%-*}"
    done <"$list" | sort -rn |
        awk -F '\t' -v all=$((SECONDS - began)) 'NR == 1 { print "oby-check: obid'"'"'s test binaries took " all "s, the longest tests/" $2 ".rs at " $1 "s" }'
    rm -rf "$dir"
    return "$status"
}

# obid's lib tests: cargo's command, with `--no-fail-fast`, so every target runs to its end and a
# retry never stands in for one that didn't run. If the tests it failed are all on the flaky list, the
# same targets run them once more, in place of its filters, and it passes if they do (CI-26, CI-27).
lib_tests() {
    local out flaky cargo_args=() again=() arg test
    out=$(mktemp "${TMPDIR:-/tmp}/oby-check.XXXXXX")
    if "$@" | tee "$out"; then
        rm -f "$out"
        return 0
    fi
    flaky=$(flaky_retry obid <"$out") || { rm -f "$out"; return 1; }
    rm -f "$out"
    for arg in "$@"; do
        [[ "$arg" != -- ]] || break
        cargo_args+=("$arg")
    done
    while IFS= read -r test; do again+=("$test"); done <<<"$flaky"
    echo "oby-check: obid's lib tests again, their listed flaky tests only"
    "${cargo_args[@]}" -- --exact "${again[@]}" || return 1
    sed 's/^/retried flaky: obid /' <<<"$flaky"
}

# The top-level types a Swift test file declares, its suites, each after `$1`.
suites() {
    [[ ! -f "$2" ]] || sed -nE "s#^(@[A-Za-z]+(\([^)]*\))? )*(final )?(struct|class|enum) ([A-Za-z0-9_]+).*#$1\5#p" "$2"
}

# What each part needs, one per line.
rust='' blok='' electron='' lib='' lib_all='' tests='' crates='' ios='' ios_suites='' check_script='' nightly_script='' factory='' agents_md='' agents_rules=''
add() { printf '%s\n%s' "$1" "$2"; }
while IFS= read -r file; do
    case "$file" in
        core/AGENTS.md | core/CLAUDE.md) ;;
        core/*) rust=1 ;;
        tools/oby-check-blok/*) blok=1 ;;
    esac
    case "$file" in
        core/crates/obid/tests/support/*)
            tests=$(add "$tests" "$(grep -l '^mod support;' core/crates/obid/tests/*.rs)") ;;
        core/crates/obid/tests/agents/*)
            tests=$(add "$tests" "$(grep -l "tests/agents/${file##*/}" core/crates/obid/tests/*.rs)") ;;
        core/crates/obid/tests/*.rs) tests=$(add "$tests" "$file") ;;
        core/crates/obid/tests/*/*)
            folder=${file#core/crates/obid/tests/}
            tests=$(add "$tests" "$(grep -l "tests/${folder%%/*}" core/crates/obid/tests/*.rs)") ;;
        core/crates/obid/src/lib.rs | core/crates/obid/src/main.rs) lib_all=1 ;;
        core/crates/obid/src/*.rs)
            module=${file#core/crates/obid/src/}
            module=${module%.rs}
            module=${module%/mod}
            lib=$(add "$lib" "${module//\//::}::")
            tests=$(add "$tests" "core/crates/obid/tests/${module%%/*}.rs")
            tests=$(add "$tests" "core/crates/obid/tests/${module%%/*}_api.rs") ;;
        # What every module may read (a prompt, a schema), how obid builds, and its dependencies.
        core/crates/obid/src/* | core/crates/obid/Cargo.toml | core/crates/obid/build.rs | core/Cargo.toml | core/Cargo.lock)
            lib_all=1 ;;
        core/crates/*/*)
            crate=${file#core/crates/}
            crates=$(add "$crates" "${crate%%/*}") ;;
    esac
    case "$file" in
        # A repository's setup in the box (PUBLISHING-23, 24): only its own and the box's code plan
        # it, so a change to jobs.rs alone isn't left unjudged in a job's sandbox (below).
        core/crates/obid/src/jobs/setup.rs | core/crates/obid/src/sandbox.rs | core/crates/obid/src/egress.rs | \
            core/crates/obid/src/process.rs | core/crates/obid/src/secretspec.rs)
            tests=$(add "$tests" core/crates/obid/tests/jobs_setup.rs) ;;
    esac
    case "$file" in
        # What reads a released obid's job files and config.json (docs/contracts/released-files.md).
        core/crates/obid/src/job.rs | core/crates/obid/src/config.rs | core/crates/obid/src/store.rs)
            tests=$(add "$tests" core/crates/obid/tests/released_files.rs) ;;
    esac
    # The routes obid serves and the clients that call them (docs/contracts/ci.md, CI-9 to 11).
    routes=''
    case "$file" in
        core/crates/obid/src/api.rs | core/crates/obid/src/gateway.rs | docs/api/* | apps/shared/*.swift)
            routes=1 ;;
        core/crates/obid/src/*) [[ ! -f "$file" ]] || ! grep -qF '.route(' "$file" || routes=1 ;;
        */test/* | *.test.ts | *.test.tsx | apps/electron/src/renderer/src/mock.ts) ;;
        apps/electron/src/*.ts | apps/electron/src/*.tsx) routes=1 ;;
    esac
    if [[ -n "$routes" ]]; then rust=1 tests=$(add "$tests" core/crates/obid/tests/routes_guard.rs); fi
    case "$file" in
        apps/ios/tests/*.swift) ios=1 ios_suites=$(add "$ios_suites" "$(suites iOSTests/ "$file")") ;;
        apps/ios/* | apps/watch/* | apps/shared/* | apps/Oby.xcodeproj/* | character/*) ios=1 ;;
    esac
    case "$file" in
        apps/electron/*) electron=1 ;;
        scripts/oby-check*) check_script=1 ;;
        scripts/oby-nightly*) nightly_script=1 ;;
        AGENTS.md) agents_md=1 agents_rules=1 ;;
        core/AGENTS.md | core/CLAUDE.md | docs/agents-rules-manifest.md) agents_rules=1 ;;
        factory/bin/* | factory/checks.sh | factory/tests/* | factory/config.json) factory=1 ;;
    esac
done <<< "$changed"
# A file the tree's rules manifest names as an entry's home plans AGENTS.md's rules too (CI-44).
rules_manifest=docs/agents-rules-manifest.md
if [[ -z "$agents_rules" && -f "$rules_manifest" ]]; then
    rules_homes=$(sed -n 's/^- `\([^`][^`]*\)`: ..*$/\1/p' "$rules_manifest" | sort -u)
    while IFS= read -r file; do
        [[ -z "$file" ]] || ! grep -qxF -- "$file" <<<"$rules_homes" || agents_rules=1
    done <<<"$changed"
fi
# Not nested (the Mac's own check of a pull request), what plans tests/jobs_api.rs plans the setup's
# tests too: jobs.rs calls the setup, so a change to it is judged before it merges (#1944). In a
# job's box they stay unplanned, and the job isn't sunk.
if [[ -z "$nested" && "$tests" == *core/crates/obid/tests/jobs_api.rs* ]]; then
    tests=$(add "$tests" core/crates/obid/tests/jobs_setup.rs)
fi

# OBY_CHECK_ALL=1: everything, as if every part had changed (a full check of a branch): obid, the
# Electron app and the Apple apps' build (no suites named: build-for-testing). The secret scan still
# covers what changed against the base, which on the base itself is nothing.
if [[ -n "${OBY_CHECK_ALL:-}" ]]; then
    rust=1 blok=1 lib_all=1 check_script=1 nightly_script=1 electron=1 ios=1 factory=1 agents_rules=1
    tests=$(ls core/crates/obid/tests/*.rs 2>/dev/null || true)
    crates=$(ls core/crates 2>/dev/null || true)
fi

# The distinct lines of a list, each after `$2` when given.
each() { printf '%s\n' "$1" | sed '/^$/d' | sort -u | sed "s|^|$2|"; }

# With OBY_CHECK_SHARD, shard k's share of obid's test binaries: the tree's tests/*.rs ranked by lines,
# most first, ties by path, the one at 0-based position i shard (i mod n) + 1's. The rest of the rust
# group is shard 1's.
first_shard() { [[ -z "$shard_n" || "$shard_k" == 1 ]]; }
shard_tests=''
if [[ -n "$shard_n" ]]; then
    shard_tests=$(for test in core/crates/obid/tests/*.rs; do
        [[ ! -f "$test" ]] || printf '%s\t%s\n' "$(wc -l <"$test" | tr -d ' ')" "$test"
    done | LC_ALL=C sort -t $'\t' -k1,1nr -k2,2 |
        awk -F '\t' -v k="$shard_k" -v n="$shard_n" '(NR - 1) % n == k - 1 { print $2 }')
fi

group=rust
if [[ -n "$check_script" ]] && first_shard; then step bash scripts/oby-check-test.sh; fi
if [[ -n "$nightly_script" ]] && first_shard; then step bash scripts/oby-nightly-test.sh; fi

# OBY_CHECK_ONLY=limits runs this block alone, whatever changed (CI-35); a change to AGENTS.md alone
# runs it too, with no cargo step (CI-38).
if { selected && first_shard && [[ -n "$rust$check_script$agents_md" ]]; } || [[ "${OBY_CHECK_ONLY:-}" == limits ]]; then
    plan_group
    if ! git cat-file -e HEAD:scripts/oby-check.sh 2>/dev/null; then
        echo "oby-check: this commit has no scripts/oby-check.sh, this repository's check and its line limits: put it back" >&2
        failed+=("scripts/oby-check.sh is gone")
    fi
    # Dispatcher, not harness (AGENTS.md): the Rust core's lines and the change's difference from its
    # base are reported, never judged (docs/contracts/ci.md, CI-49, CI-50).
    lines=$(obid_lines)
    if base_lines=$(base_obid_lines); then
        echo "oby-check: the Rust core has $lines production lines outside the Sphere, $(printf '%+d' "$((lines - base_lines))") since its base"
    else
        echo "oby-check: the Rust core has $lines production lines outside the Sphere, the difference from its base unknown (its core/crates can't be read)"
    fi
    jobs_lines=$(jobs_rs_lines)
    if ((jobs_lines > jobs_rs_limit)); then
        echo "oby-check: jobs.rs has $jobs_lines production lines, past its limit of $jobs_rs_limit: it may only shrink: put new code in a module of its own, and lower jobs_rs_limit in this script when jobs.rs shrinks$(moved jobs_rs_limit)" >&2
        failed+=("jobs.rs's line limit")
    else
        echo "oby-check: jobs.rs has $jobs_lines production lines, within its limit of $jobs_rs_limit"
    fi
    paid jobs_rs_limit
    sleeps=$(tests_sleeps)
    if ((sleeps > tests_sleep_limit)); then
        echo "oby-check: obid's tests have $sleeps sleep calls, past their limit of $tests_sleep_limit (docs/contracts/ci.md, CI-12): a test waits for state with support::within instead of sleeping$(moved tests_sleep_limit)" >&2
        failed+=("obid's tests' sleep limit")
    elif ((sleeps < tests_sleep_limit)); then
        echo "oby-check: obid's tests have $sleeps sleep calls, under their limit of $tests_sleep_limit (docs/contracts/ci.md, CI-13): lower tests_sleep_limit in this script from $tests_sleep_limit to $sleeps, so the cut stays cut$(moved tests_sleep_limit)" >&2
        failed+=("obid's tests' sleep limit")
    else
        echo "oby-check: obid's tests have $sleeps sleep calls, within their limit of $tests_sleep_limit"
    fi
    paid tests_sleep_limit
    # The root AGENTS.md's bytes, committed or not (CI-36 to 38).
    if [[ ! -f AGENTS.md ]]; then
        echo "oby-check: this commit has no AGENTS.md, whose size agents_md_bytes_limit holds: put it back" >&2
        failed+=("AGENTS.md is gone")
    else
        bytes=$(wc -c <AGENTS.md | tr -d ' ')
        if ((bytes > agents_md_bytes_limit)); then
            echo "oby-check: AGENTS.md has $bytes bytes, past its limit of $agents_md_bytes_limit: cut it, or ask the owner to raise agents_md_bytes_limit in this script$(moved agents_md_bytes_limit)" >&2
            failed+=("AGENTS.md's byte limit")
        elif ((bytes + 512 < agents_md_bytes_limit)); then
            echo "oby-check: AGENTS.md has $bytes bytes, more than 512 under its limit: lower agents_md_bytes_limit in this script from $agents_md_bytes_limit to $bytes, so the cut stays cut$(moved agents_md_bytes_limit)" >&2
            failed+=("AGENTS.md's byte limit")
        else
            echo "oby-check: AGENTS.md has $bytes bytes, within its limit of $agents_md_bytes_limit"
        fi
    fi
    paid agents_md_bytes_limit
    # A raise of jobs_rs_limit since the branch started names its payback (CI-39) and comes alone, in
    # a committed change to this script only (CI-40). Not on the combined code GitHub's `limits` job
    # checks (CI-35), where a raise shares its diff with the pull requests queued beside it.
    if [[ "${OBY_CHECK_ONLY:-}" != limits ]]; then
        raise_others=$(git diff --name-only "$base" HEAD | grep -vxF scripts/oby-check.sh || true)
        for raised in jobs_rs_limit; do
            raised_from=$(limit_in "$base" "$raised")
            raised_to=$(limit_in HEAD "$raised")
            [[ -n "$raised_from" && -n "$raised_to" ]] && ((raised_to > raised_from)) || continue
            if [[ ! "$(above_limit "$raised")" =~ $payback_form ]]; then
                echo "oby-check: this change raises $raised from $raised_from to $raised_to with no payback line: add \"# payback: #<issue>\" or \"# payback: <task key>\", naming what cut pays it back, as the line directly above \"$raised=$raised_to\" in this script (docs/contracts/ci.md, CI-39)" >&2
                failed+=("$raised's payback line")
            fi
            if [[ -n "$raise_others" ]]; then
                echo "oby-check: this change raises $raised from $raised_from to $raised_to and changes $(awk 'NR <= 5 { printf "%s%s", (NR > 1 ? ", " : ""), $0 } END { if (NR > 5) printf " and %d more", NR - 5 }' <<<"$raise_others") too: move the raise into a pull request that changes only scripts/oby-check.sh (docs/contracts/ci.md, CI-40)" >&2
                failed+=("$raised's raise in a pull request of its own")
            fi
        done
    fi
    # The flaky list may only shrink (CI-28): an entry the base's list hasn't comes with the task that
    # fixes it, a file this change adds.
    flaky_base=$(flaky_at "$base" | cut -f1,2)
    flaky_tree=$(flaky_at tree)
    flaky_bad=''
    while IFS=$'\t' read -r id test key; do
        if [[ "$id" == malformed ]]; then
            echo "oby-check: core/.config/nextest.toml has an override that isn't a flaky list entry: $test: write it as the list's entries are (docs/contracts/ci.md, CI-28), or take it out" >&2
            flaky_bad=1
        elif [[ -n "$id" ]] && ! grep -qxF "$id	$test" <<<"$flaky_base"; then
            if [[ ! -f "factory/queue/tasks/$key.md" ]] || git cat-file -e "$base:factory/queue/tasks/$key.md" 2>/dev/null; then
                echo "oby-check: the flaky list's new entry $id $test names $key, a task this change doesn't add: add factory/queue/tasks/$key.md, the task that fixes it, in the same change, or take the entry out of core/.config/nextest.toml" >&2
                flaky_bad=1
            fi
        fi
    done <<<"$flaky_tree"
    if [[ -n "$flaky_bad" ]]; then
        failed+=("the flaky list")
    else
        echo "oby-check: the flaky list has $(grep -c . <<<"$flaky_tree" || true) tests, each new one with the task this change adds to fix it"
    fi
fi

# AGENTS.md's rules (docs/contracts/ci.md, CI-42 to 44): each line the manifest lists is in the file
# its entry names, AGENTS.md, core/AGENTS.md or one the root AGENTS.md links, both compared with
# each link's target and each run of whitespace taken out. In the checked tree, committed or not.
if selected && first_shard && [[ -n "$agents_rules" ]]; then
    plan_group
    if [[ ! -f "$rules_manifest" ]]; then
        echo "oby-check: this commit has no $rules_manifest, which lists AGENTS.md's rules: put it back" >&2
        failed+=("the rules manifest is gone")
    else
        # The root's links: no URL or bare #anchor, each without its #anchor and leading ./.
        rules_linked=$({ grep -o '\]([^)]*)' AGENTS.md 2>/dev/null || true; } |
            sed -e 's/^](//' -e 's/)$//' -e 's/#.*//' -e 's|^\./||' | grep -v -e '^[A-Za-z][A-Za-z0-9+.-]*:' -e '^$' || true)
        rules_linked=$(printf '%s\n' AGENTS.md core/AGENTS.md "$rules_linked")
        # Of those, the ones awk may read: a folder or a missing file holds nothing.
        rules_files=$(while IFS= read -r file; do [[ ! -f "$file" ]] || printf '%s\n' "$file"; done <<<"$rules_linked")
        # A line each: `home` and a home the root doesn't link, `missing`, its home and its sentence, or
        # `count` and how many entries there are. A home not linked isn't read.
        rules_said=$(OBY_RULES_LINKED="$rules_linked" OBY_RULES_FILES="$rules_files" awk '
            function norm(s) { gsub(/[[:space:]]+/, " ", s); gsub(/\]\([^)]*\)/, "]", s); sub(/^ /, "", s); sub(/ $/, "", s); return s }
            BEGIN {
                n = split(ENVIRON["OBY_RULES_LINKED"], l, "\n"); for (i = 1; i <= n; i++) ok[l[i]] = 1
                n = split(ENVIRON["OBY_RULES_FILES"], l, "\n"); for (i = 1; i <= n; i++) readable[l[i]] = 1
            }
            /^- `[^`]+`: ./ {
                count++
                rest = substr($0, 4)
                home = substr(rest, 1, index(rest, "`") - 1)
                sentence = substr(rest, length(home) + 4)
                if (!(home in ok)) { if (!(home in told)) print "home\t" home; told[home] = 1; next }
                if (!(home in text)) {
                    t = ""
                    if (home in readable) {
                        while ((getline line < home) > 0) t = t "\n" line
                        close(home)
                    }
                    text[home] = norm(t)
                }
                if (!index(text[home], norm(sentence))) print "missing\t" home "\t" sentence
            }
            END { print "count\t" count + 0 }' "$rules_manifest")
        rules_bad=''
        while IFS=$'\t' read -r what home sentence; do
            case "$what" in
                home)
                    echo "oby-check: AGENTS.md's rules: $home, a home in $rules_manifest, isn't AGENTS.md, core/AGENTS.md or a file the root AGENTS.md links: link it from the root AGENTS.md, or move the rule" >&2
                    rules_bad=1 ;;
                missing)
                    echo "oby-check: AGENTS.md's rules: $home no longer holds \"${sentence:0:60}\": put it back, or change its entry in $rules_manifest in the same change" >&2
                    rules_bad=1 ;;
                count) rules_count=$home ;;
            esac
        done <<<"$rules_said"
        if [[ -n "$rules_bad" ]]; then
            failed+=("AGENTS.md's rules")
        else
            echo "oby-check: AGENTS.md's rules: each of the $rules_count in $rules_manifest is where it says"
        fi
    fi
fi

if selected && first_shard && [[ -n "$blok" ]]; then
    step env CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$cache/oby-check-blok" cargo fmt --manifest-path tools/oby-check-blok/Cargo.toml --all -- --check
    step env CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$cache/oby-check-blok" cargo clippy --manifest-path tools/oby-check-blok/Cargo.toml --all-targets -- -D warnings
    step env CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$cache/oby-check-blok" cargo test --manifest-path tools/oby-check-blok/Cargo.toml
fi

if selected && [[ -n "$rust" ]]; then
    # A job's check has no network (above): use the crates the agent already fetched into HOME.
    # Without this, cargo still tries to refresh index.crates.io and fails DNS before the cache.
    export CARGO_NET_OFFLINE=true
    cargo=(cargo --quiet)
    manifest=(--manifest-path core/Cargo.toml)
    if first_shard; then step "${cargo[@]}" fmt "${manifest[@]}" --all -- --check; fi
    packages=()
    if [[ -n "$lib$lib_all$tests" ]]; then crates=$(add "$crates" obid); fi
    while IFS= read -r crate; do packages+=(-p "$crate"); done < <(each "$crates")
    if [[ ${#packages[@]} -gt 0 ]] && first_shard; then
        step "${cargo[@]}" clippy "${manifest[@]}" "${packages[@]}" --all-targets -- -D warnings
    fi
    if [[ -n "$lib_all" ]] && first_shard; then
        step lib_tests "${cargo[@]}" test "${manifest[@]}" --no-fail-fast -p obid --lib --bins
    elif [[ -n "$lib" ]] && first_shard; then
        filters=()
        while IFS= read -r filter; do filters+=("$filter"); done < <(each "$lib")
        step lib_tests "${cargo[@]}" test "${manifest[@]}" --no-fail-fast -p obid --lib -- "${filters[@]}"
    fi
    binaries=()
    while IFS= read -r test; do
        name=${test##*/}
        name=${name%.rs}
        [[ -f "$test" ]] || continue
        # Another shard's binary is that shard's to run, or to leave unjudged.
        [[ -z "$shard_n" ]] || grep -qxF -- "$test" <<<"$shard_tests" || continue
        # They make a sandbox of their own, which can't nest in a job's on a Mac. On Linux, where
        # nested is always set (no sandbox-exec), they still run: they test bubblewrap there.
        if [[ -n "$nested" && "$OSTYPE" == darwin* && "$name" =~ ^(sandbox|plugins_api|jobs_setup)$ ]]; then
            plan_group
            unjudged+=("tests/$name.rs")
            failed+=("tests/$name.rs, which can't run in a job's sandbox")
            continue
        fi
        binaries+=(--test "$name")
    done < <(each "$tests")
    if [[ ${#binaries[@]} -gt 0 ]]; then
        step test_binaries "${binaries[@]}"
    fi
    while IFS= read -r crate; do
        [[ "$crate" == obid ]] || ! first_shard || step "${cargo[@]}" test "${manifest[@]}" -p "$crate"
    done < <(each "$crates")
fi

# Ad hoc signing with no team or profile.
group=swift
xcode=(xcodebuild -quiet -jobs "$jobs" -project apps/Oby.xcodeproj -derivedDataPath "$cache/DerivedData"
    -clonedSourcePackagesDirPath "$cache/SourcePackages"
    CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER=
    ENABLE_HARDENED_RUNTIME=NO)
# A scheme's touched suites (`$3`), else a build of its tests.
apple() {
    local only=()
    while IFS= read -r suite; do only+=("$suite"); done < <(each "$3" -only-testing:)
    if [[ ${#only[@]} -gt 0 ]]; then
        step "${xcode[@]}" -scheme "$1" -destination "$2" test "${only[@]}"
    else
        step "${xcode[@]}" -scheme "$1" -destination "$2" build-for-testing
    fi
}
if ! selected; then
    :
elif [[ -z "$nested" ]]; then
    if [[ -n "$ios" ]]; then apple ObyPhone "platform=iOS Simulator,name=iPhone 17,OS=latest" "$ios_suites"; fi
elif [[ -n "$ios" ]]; then
    plan_group
    echo "oby-check: the Apple apps don't run in a job's sandbox (Seatbelt can't nest in it): run them on the Mac before merging"
fi

group=electron
if selected && [[ -n "$electron" ]]; then
    [[ -d apps/electron/node_modules ]] || echo "oby-check: apps/electron has no node_modules: run npm install there first" >&2
    step npm --prefix apps/electron run --silent typecheck
    step npm --prefix apps/electron run --silent test
    step npm --prefix apps/electron run --silent build
fi

# The factory's own tests (factory/tests), in a job's box too: they need no network, no lock and no
# build, and nothing outside TMPDIR and the checkout. The owner's GitHub login stays out of them (their
# gh is a stand-in), and Python writes no __pycache__ into the checkout.
group=factory
if selected && [[ -n "$factory" ]]; then
    step env -u GH_TOKEN -u GITHUB_TOKEN -u GH_ENTERPRISE_TOKEN -u GITHUB_ENTERPRISE_TOKEN \
        GH_CONFIG_DIR=/nonexistent/oby-check-no-gh-login PYTHONDONTWRITEBYTECODE=1 \
        python3 -m unittest discover -s factory/tests
fi

# What changed, committed or not, scanned for secrets with this repository's rules.
group=scan
leaks=(--config .gitleaks.toml --no-banner --redact)
uncommitted_leaks() {
    git diff HEAD | gitleaks stdin "${leaks[@]}"
    while IFS= read -r file; do
        [[ -z "$file" ]] || gitleaks dir "$file" "${leaks[@]}"
    done < <(git ls-files --others --exclude-standard)
}
if ! selected; then
    :
elif command -v gitleaks >/dev/null; then
    step gitleaks git . "--log-opts=$base..HEAD" "${leaks[@]}"
    step uncommitted_leaks
else
    echo "oby-check: gitleaks isn't installed (brew install gitleaks): nothing passes unscanned" >&2
    plan_group
    failed+=(gitleaks)
fi

# obid reads this line: the job ends unjudged, for the owner's Publish by hand (PUBLISHING-22). Only
# when the suites that couldn't run are all that failed: fmt, clippy, a build or gitleaks failing
# too is the change's to fix, never one the owner could publish by hand.
if [[ ${#unjudged[@]} -gt 0 && ${#failed[@]} -eq ${#unjudged[@]} ]]; then
    printf "couldn't judge: %s makes a sandbox of its own and can't run in a job's; run the check on the Mac outside a job\n" "${unjudged[@]}"
fi
if [[ -n "${OBY_CHECK_PLAN:-}" ]]; then
    echo "oby-check: planned, nothing run"
elif [[ ${#failed[@]} -gt 0 ]]; then
    printf 'oby-check: failed: %s\n' "${failed[@]}" >&2
    exit 1
else
    echo "oby-check: passed"
fi
