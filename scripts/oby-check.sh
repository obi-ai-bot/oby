#!/bin/bash
# This repository's check, the one obid runs for its jobs (`repos[].check`, docs/runner-api.md,
# "obid only: configuration"): only the suites what the job changed reaches, then gitleaks. In a
# job's check there's no network but loopback, so what the suites fetch (crates, Swift packages,
# the bundled tools' downloads) is what the agent fetched while it worked, into its own HOME, which
# the check keeps: run this yourself first.
#
#   scripts/oby-check.sh [base]    what changed since the merge base with base (default origin/dev),
#                                  committed or not; with OBY_CHECK_PLAN=1, only says what it'd run
#
#   core/                 cargo fmt --check; clippy -D warnings on the touched crates; obid's lib
#                         tests of each touched module (src/a/b.rs: `a::b::`, all of them for
#                         lib.rs or main.rs), its touched test binaries, tests/<module>.rs or
#                         tests/<module>_api.rs of a touched module, and the binaries that use a
#                         touched tests/support or tests/agents file; every test of another
#                         touched crate
#   apps/mac/, apps/shared/, apps/Oby.xcodeproj, character/
#                         the touched MacTests suites, else a build-for-testing of the Oby scheme
#   apps/ios/, apps/watch/, apps/shared/, apps/Oby.xcodeproj
#                         the touched iOSTests suites on a Simulator, else a build-for-testing
#   Engine/, Runner/, RunnerTests/, mcp/, Package.*
#                         swift test (only the touched RunnerTests suites when only they changed)
#
# Inside a job's sandbox (where Seatbelt can't make another: a no-op sandbox-exec fails), the Apple
# apps aren't built: SwiftPM evaluates every package manifest in a sandbox of its own, which can't
# nest there, and xcodebuild has no switch for it. The Apple check builds and tests them on the Mac,
# and the Mac check before merging (docs/runner-api.md, "Apple check", "Mac check").
set -eo pipefail
cd "$(dirname "$0")/.."

base=$(git merge-base "${1:-origin/dev}" HEAD) || {
    echo "oby-check: can't find where the change starts (${1:-origin/dev})" >&2
    exit 2
}
changed=$({ git diff --name-only "$base"; git ls-files --others --exclude-standard; } | sort -u)
# Xcode's and SwiftPM's builds, kept between runs (in a job, in its HOME).
cache="${OBY_CHECK_CACHE:-$HOME/Library/Caches/oby-check}"

failed=()
step() {
    echo "==> $*"
    [[ -n "${OBY_CHECK_PLAN:-}" ]] || "$@" || failed+=("$*")
}

# The top-level types a Swift test file declares, its suites, each after `$1`.
suites() {
    [[ ! -f "$2" ]] || sed -nE "s#^(@[A-Za-z]+(\([^)]*\))? )*(final )?(struct|class|enum) ([A-Za-z0-9_]+).*#$1\5#p" "$2"
}

# What each part needs, one per line.
rust='' lib='' lib_all='' tests='' crates='' mac='' mac_suites='' ios='' ios_suites='' swift='' swift_suites=''
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
        core/crates/obid/src/lib.rs | core/crates/obid/src/main.rs) lib_all=1 ;;
        core/crates/obid/src/*.rs)
            module=${file#core/crates/obid/src/}
            module=${module%.rs}
            module=${module%/mod}
            lib=$(add "$lib" "${module//\//::}::")
            tests=$(add "$tests" "core/crates/obid/tests/${module%%/*}.rs")
            tests=$(add "$tests" "core/crates/obid/tests/${module%%/*}_api.rs") ;;
        core/crates/*/*)
            crate=${file#core/crates/}
            crates=$(add "$crates" "${crate%%/*}") ;;
    esac
    case "$file" in
        apps/mac/tests/*.swift) mac=1 mac_suites=$(add "$mac_suites" "$(suites MacTests/ "$file")") ;;
        apps/mac/* | apps/shared/* | apps/Oby.xcodeproj/* | character/*) mac=1 ;;
    esac
    case "$file" in
        apps/ios/tests/*.swift) ios=1 ios_suites=$(add "$ios_suites" "$(suites iOSTests/ "$file")") ;;
        apps/ios/* | apps/watch/* | apps/shared/* | apps/Oby.xcodeproj/*) ios=1 ;;
    esac
    case "$file" in
        RunnerTests/*.swift) swift_suites=$(add "$swift_suites" "$(suites '' "$file")") ;;
        Engine/* | Runner/* | RunnerTests/* | mcp/* | Package.swift | Package.resolved) swift=1 ;;
    esac
done <<< "$changed"

# The distinct lines of a list, each after `$2` when given.
each() { printf '%s\n' "$1" | sed '/^$/d' | sort -u | sed "s|^|$2|"; }

if [[ -n "$rust" ]]; then
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
        step "${cargo[@]}" test "${manifest[@]}" -p obid --lib
    elif [[ -n "$lib" ]]; then
        filters=()
        while IFS= read -r filter; do filters+=("$filter"); done < <(each "$lib")
        step "${cargo[@]}" test "${manifest[@]}" -p obid --lib -- "${filters[@]}"
    fi
    binaries=()
    while IFS= read -r test; do
        name=${test##*/}
        [[ ! -f "$test" ]] || binaries+=(--test "${name%.rs}")
    done < <(each "$tests")
    if [[ ${#binaries[@]} -gt 0 ]]; then
        step "${cargo[@]}" test "${manifest[@]}" -p obid "${binaries[@]}"
    fi
    while IFS= read -r crate; do
        [[ "$crate" == obid ]] || step "${cargo[@]}" test "${manifest[@]}" -p "$crate"
    done < <(each "$crates")
fi

# Ad hoc signing with no team or profile, as the Mac checks build (Engine/Checks.swift).
xcode=(xcodebuild -quiet -project apps/Oby.xcodeproj -derivedDataPath "$cache/DerivedData"
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
# SwiftPM runs git on its packages, not this repository, so never with a committing job's own
# object folder; and in its own sandbox only where one can be made (inside a job's, it can't nest).
swiftpm=(env -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES swift test --scratch-path "$cache/swiftpm")
if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null; then
    if [[ -n "$mac" ]]; then apple Oby platform=macOS "$mac_suites"; fi
    if [[ -n "$ios" ]]; then apple ObyPhone "platform=iOS Simulator,name=iPhone 17,OS=latest" "$ios_suites"; fi
else
    swiftpm+=(--disable-sandbox)
    if [[ -n "$mac$ios" ]]; then
        echo "oby-check: the Apple apps aren't built in a job's sandbox (SwiftPM's can't nest in it): the Apple check and the Mac check build them"
    fi
fi
if [[ -n "$swift" ]]; then
    step "${swiftpm[@]}"
elif [[ -n "$swift_suites" ]]; then
    step "${swiftpm[@]}" --filter "$(each "$swift_suites" | paste -sd '|' -)"
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

if [[ ${#failed[@]} -gt 0 ]]; then
    printf 'oby-check: failed: %s\n' "${failed[@]}" >&2
    exit 1
fi
echo "oby-check: passed"
