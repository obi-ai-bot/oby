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
#                                  with OBY_CHECK_PATHS (one a line), runs what those paths reach:
#                                  obid sets it when it runs this on a job's base, which changed
#                                  nothing, to see whether the base fails as the job does
#                                  (docs/contracts/publishing.md, PUBLISHING-18)
#
#   core/                 the line limit below; cargo fmt --check; clippy -D warnings on the touched
#                         crates; obid's lib tests of each touched module (src/a/b.rs: `a::b::`; all of them for
#                         lib.rs, main.rs, build.rs, a file under src/ that isn't Rust, its
#                         Cargo.toml, or the workspace's Cargo.toml or Cargo.lock), its touched
#                         test binaries, tests/<module>.rs or tests/<module>_api.rs of a touched
#                         module, and the binaries that name a touched folder under tests/
#                         (support, agents, plugins); tests/jobs_setup.rs for src/jobs/setup.rs,
#                         sandbox.rs, egress.rs, process.rs or secretspec.rs, and, outside a job's
#                         box, wherever tests/jobs_api.rs runs; every test of another touched crate
#   apps/ios/, apps/watch/, apps/shared/, apps/Oby.xcodeproj, character/
#                         the touched iOSTests suites on a Simulator, else a build-for-testing
#   mcp/, Package.swift   swift test
#   apps/electron/        npm run typecheck, npm test and npm run build there (its node_modules from npm install)
#   scripts/oby-check*    scripts/oby-check-test.sh and the line limit below
#
# Inside a job's sandbox (where Seatbelt can't make another: a no-op sandbox-exec fails), the Apple
# apps aren't built and swift test doesn't run: SwiftPM evaluates every package manifest in a
# sandbox of its own, which can't nest there, and xcodebuild has no switch for it, and a cold
# SwiftPM build in the job's HOME doesn't fit the check's 45 minutes. Nothing runs them by itself
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
# cores), for Cargo, Rust's tests, swift test (-j) and xcodebuild (-jobs). Run by hand, outside obid,
# every one gets 4, on purpose: obid runs one check at a time, and a Mac has other work.
set -eo pipefail
jobs="${OBY_BUILD_JOBS:-4}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$jobs}" RUST_TEST_THREADS="${RUST_TEST_THREADS:-$jobs}"
cd "$(dirname "$0")/.."

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

# Dispatcher, not harness (AGENTS.md): how many production lines the Rust core may have outside the
# Sphere. A cut lowers it. Raising it is the owner's word: this is the check, so a job's own change
# to it is checked by the base's copy and merges as a change to the check does (docs/contracts/merging.md).
obid_lines_limit=95000
# jobs.rs alone may only shrink: past this it fails (the owner's decision of 2026-11-06, "1.3 Both").
jobs_rs_limit=15315
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
pick_limit obid_lines_limit
pick_limit jobs_rs_limit
# Said after a limit's failure: dev's limit changed since this branch started.
moved() { [[ " ${limit_moved[*]} " != *" $1 "* ]] || echo " (dev's limit changed since this branch started: rebase onto dev)"; }
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
jobs_rs_lines() {
    if [[ -f core/crates/obid/src/jobs.rs ]]; then awk "$production_lines" core/crates/obid/src/jobs.rs; else echo 0; fi
}

failed=() unjudged=()
# Inside a job's sandbox, where Seatbelt can't make another.
nested=''
/usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null || nested=1
step() {
    echo "==> $*"
    [[ -n "${OBY_CHECK_PLAN:-}" ]] || "$@" || failed+=("$*")
}

# obid's test binaries (`--test name` each), built once, then run `jobs` at a time, the longest
# first, every one to its end. `cargo test` runs them one after another and stops at the first that
# fails, and most of their time is spent waiting on an agent's stand-in, not working. Each binary
# runs with OBY_TEST_THREADS test threads (2 by default) and `--quiet`, half as many binaries at once
# as `jobs` (at least one), so threads in all stay within obid's memory budget; its output comes
# whole: the ones that passed first, the ones that failed last, so a failure stays in the check's
# tail. Stopped (the check's 45 minutes), it names the binaries still running.
# ponytail: the longest first by its source's lines; keep each binary's last time if that's off.
test_binaries() {
    local dir list status=0 exe name want got
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
    # Where cargo runs them: the crate's folder.
    (cd core/crates/obid && tr '\n' '\0' <"$list" | RUST_TEST_THREADS="${OBY_TEST_THREADS:-2}" \
        xargs -0 -n 1 -P "$(((jobs + 1) / 2))" sh -c '"$2" --quiet >"$1/${2##*/}.out" 2>&1; echo $? >"$1/${2##*/}.status"' _ "$dir")
    trap - TERM
    for pass in ok failed; do
        while IFS= read -r exe; do
            name=${exe##*/}
            if [[ "$(cat "$dir/$name.status" 2>/dev/null)" == 0 ]]; then
                [[ $pass == ok ]] || continue
            else
                [[ $pass == failed ]] || continue
                status=1
            fi
            echo "     Running ${name%-*}"
            cat "$dir/$name.out"
            [[ $pass == ok ]] || echo "oby-check: tests/${name%-*}.rs failed" >&2
        done <"$list"
    done
    rm -rf "$dir"
    return "$status"
}

# The top-level types a Swift test file declares, its suites, each after `$1`.
suites() {
    [[ ! -f "$2" ]] || sed -nE "s#^(@[A-Za-z]+(\([^)]*\))? )*(final )?(struct|class|enum) ([A-Za-z0-9_]+).*#$1\5#p" "$2"
}

# What each part needs, one per line.
rust='' electron='' lib='' lib_all='' tests='' crates='' ios='' ios_suites='' swift='' check_script=''
add() { printf '%s\n%s' "$1" "$2"; }
while IFS= read -r file; do
    case "$file" in
        core/*) rust=1 ;;
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
        apps/ios/tests/*.swift) ios=1 ios_suites=$(add "$ios_suites" "$(suites iOSTests/ "$file")") ;;
        apps/ios/* | apps/watch/* | apps/shared/* | apps/Oby.xcodeproj/* | character/*) ios=1 ;;
    esac
    case "$file" in
        mcp/* | Package.swift) swift=1 ;;
    esac
    case "$file" in
        apps/electron/*) electron=1 ;;
        scripts/oby-check*) check_script=1 ;;
    esac
done <<< "$changed"
# Not nested (the Mac's own check of a pull request), what plans tests/jobs_api.rs plans the setup's
# tests too: jobs.rs calls the setup, so a change to it is judged before it merges (#1944). In a
# job's box they stay unplanned, and the job isn't sunk.
if [[ -z "$nested" && "$tests" == *core/crates/obid/tests/jobs_api.rs* ]]; then
    tests=$(add "$tests" core/crates/obid/tests/jobs_setup.rs)
fi

# OBY_CHECK_ALL=1: everything, as if every part had changed (a full check of a branch): obid, the
# Electron app, swift test and the Apple apps' build (no suites named: build-for-testing). The secret
# scan still covers what changed against the base, which on the base itself is nothing.
if [[ -n "${OBY_CHECK_ALL:-}" ]]; then
    rust=1 lib_all=1 check_script=1 electron=1 swift=1 ios=1
    tests=$(ls core/crates/obid/tests/*.rs 2>/dev/null || true)
    crates=$(ls core/crates 2>/dev/null || true)
fi

# The distinct lines of a list, each after `$2` when given.
each() { printf '%s\n' "$1" | sed '/^$/d' | sort -u | sed "s|^|$2|"; }

if [[ -n "$check_script" ]]; then step bash scripts/oby-check-test.sh; fi

if [[ -n "$rust$check_script" ]]; then
    if ! git cat-file -e HEAD:scripts/oby-check.sh 2>/dev/null; then
        echo "oby-check: this commit has no scripts/oby-check.sh, this repository's check and its line limits: put it back" >&2
        failed+=("scripts/oby-check.sh is gone")
    fi
    lines=$(obid_lines)
    if ((lines > obid_lines_limit)); then
        echo "oby-check: the Rust core has $lines production lines outside the Sphere, past its limit of $obid_lines_limit (AGENTS.md, \"Dispatcher, not harness\"): cut as much elsewhere, or ask the owner to raise obid_lines_limit in this script$(moved obid_lines_limit)" >&2
        failed+=("the Rust core's line limit")
    elif ((lines + 1000 < obid_lines_limit)); then
        echo "oby-check: the Rust core has $lines production lines outside the Sphere, more than 1,000 under its limit: lower obid_lines_limit in this script from $obid_lines_limit to $(((lines + 499) / 500 * 500)), so the cut stays cut$(moved obid_lines_limit)" >&2
        failed+=("the Rust core's line limit")
    else
        echo "oby-check: the Rust core has $lines production lines outside the Sphere, within its limit of $obid_lines_limit"
    fi
    jobs_lines=$(jobs_rs_lines)
    if ((jobs_lines > jobs_rs_limit)); then
        echo "oby-check: jobs.rs has $jobs_lines production lines, past its limit of $jobs_rs_limit: it may only shrink: put new code in a module of its own, and lower jobs_rs_limit in this script when jobs.rs shrinks$(moved jobs_rs_limit)" >&2
        failed+=("jobs.rs's line limit")
    else
        echo "oby-check: jobs.rs has $jobs_lines production lines, within its limit of $jobs_rs_limit"
    fi
fi

if [[ -n "$rust" ]]; then
    # A job's check has no network (above): use the crates the agent already fetched into HOME.
    # Without this, cargo still tries to refresh index.crates.io and fails DNS before the cache.
    export CARGO_NET_OFFLINE=true
    cargo=(cargo --quiet)
    manifest=(--manifest-path core/Cargo.toml)
    step "${cargo[@]}" fmt "${manifest[@]}" --all -- --check
    packages=()
    if [[ -n "$lib$lib_all$tests" ]]; then crates=$(add "$crates" obid); fi
    while IFS= read -r crate; do packages+=(-p "$crate"); done < <(each "$crates")
    if [[ ${#packages[@]} -gt 0 ]]; then
        step "${cargo[@]}" clippy "${manifest[@]}" "${packages[@]}" --all-targets -- -D warnings
    fi
    if [[ -n "$lib_all" ]]; then
        step "${cargo[@]}" test "${manifest[@]}" -p obid --lib --bins
    elif [[ -n "$lib" ]]; then
        filters=()
        while IFS= read -r filter; do filters+=("$filter"); done < <(each "$lib")
        step "${cargo[@]}" test "${manifest[@]}" -p obid --lib -- "${filters[@]}"
    fi
    binaries=()
    while IFS= read -r test; do
        name=${test##*/}
        name=${name%.rs}
        [[ -f "$test" ]] || continue
        # They make a sandbox of their own, which can't nest in a job's on a Mac. On Linux, where
        # nested is always set (no sandbox-exec), they still run: they test bubblewrap there.
        if [[ -n "$nested" && "$OSTYPE" == darwin* && "$name" =~ ^(sandbox|plugins_api|jobs_setup)$ ]]; then
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
        [[ "$crate" == obid ]] || step "${cargo[@]}" test "${manifest[@]}" -p "$crate"
    done < <(each "$crates")
fi

# Ad hoc signing with no team or profile.
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
if [[ -z "$nested" ]]; then
    if [[ -n "$ios" ]]; then apple ObyPhone "platform=iOS Simulator,name=iPhone 17,OS=latest" "$ios_suites"; fi
    if [[ -n "$swift" ]]; then step swift test -j "$jobs" --scratch-path "$cache/swiftpm"; fi
elif [[ -n "$ios$swift" ]]; then
    echo "oby-check: the Apple apps and swift test don't run in a job's sandbox (SwiftPM's own can't nest in it): run them on the Mac before merging"
fi

if [[ -n "$electron" ]]; then
    [[ -d apps/electron/node_modules ]] || echo "oby-check: apps/electron has no node_modules: run npm install there first" >&2
    step npm --prefix apps/electron run --silent typecheck
    step npm --prefix apps/electron run --silent test
    step npm --prefix apps/electron run --silent build
fi

# What changed, committed or not, scanned for secrets with this repository's rules.
leaks=(--config .gitleaks.toml --no-banner --redact)
uncommitted_leaks() {
    git diff HEAD | gitleaks stdin "${leaks[@]}"
    while IFS= read -r file; do
        [[ -z "$file" ]] || gitleaks dir "$file" "${leaks[@]}"
    done < <(git ls-files --others --exclude-standard)
}
if command -v gitleaks >/dev/null; then
    step gitleaks git . "--log-opts=$base..HEAD" "${leaks[@]}"
    step uncommitted_leaks
else
    echo "oby-check: gitleaks isn't installed (brew install gitleaks): nothing passes unscanned" >&2
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
