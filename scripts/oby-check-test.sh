#!/bin/bash
# What scripts/oby-check.sh's line limit promises (AGENTS.md, "Dispatcher, not harness"), run on a
# repository of its own. scripts/oby-check.sh runs this when it changes.
set -eo pipefail
script="$(cd "$(dirname "$0")" && pwd)/oby-check.sh"
limit=$(sed -n 's/^obid_lines_limit=//p' "$script")
repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT
cd "$repo"
git init --quiet
mkdir scripts
cp "$script" scripts/oby-check.sh
git add scripts
git -c user.name=check -c user.email=check@example.invalid -c commit.gpgsign=false commit --quiet -m base

code() { # file lines
    mkdir -p "$(dirname "$1")"
    seq "$2" >"$1"
}
planned() { OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1; }
expect() { # words said, or "no" and the words not said; what it's about
    local said
    said=$(planned)
    if [[ "$1" == no ]]; then
        ! grep -q "$2" <<<"$said" || { echo "oby-check-test: said \"$2\": $3" >&2; exit 1; }
    else
        grep -q "$1" <<<"$said" || { echo "oby-check-test: didn't say \"$1\": $2" >&2; exit 1; }
    fi
}
past='past its limit'
main=core/crates/obid/src/main.rs

code "$main" "$limit"
expect no "$past" 'at the limit is within it'

code "$main" "$((limit + 1))"
expect "$past" 'one line past the limit'
# A real run fails on it, not only says so.
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then
    echo 'oby-check-test: a run past the limit passed' >&2
    exit 1
fi
grep -q "failed: the Rust core's line limit" <<<"$ran" || {
    echo "oby-check-test: a run past the limit failed without naming it: $ran" >&2
    exit 1
}

code "$main" "$limit"
{ echo '#[cfg(test)]'; seq 50; } >>"$main"
expect no "$past" "a file's tests don't count"
rm "$main"

code core/crates/obid/src/sphere/link.rs "$((limit + 1))"
expect no "$past" "the Sphere doesn't count"
rm core/crates/obid/src/sphere/link.rs

code core/crates/obi-protocol/src/lib.rs "$((limit + 1))"
expect no "$past" "the Sphere's protocol doesn't count"
rm core/crates/obi-protocol/src/lib.rs

code core/crates/new/src/lib.rs "$((limit + 1))"
expect "$past" 'a new crate counts'
rm core/crates/new/src/lib.rs

code "$main" "$((limit - 1001))"
expect 'lower obid_lines_limit' 'a cut asks for the limit to come down'
code "$main" "$((limit - 1000))"
expect no 'lower obid_lines_limit' 'a small cut leaves the limit'

# Nested in a job's sandbox, the tests that make a sandbox of their own aren't run (HEALTH-13).
# Here the check runs in a sandbox of this test's where its own sandbox-exec fails, as in a job's,
# unless this test already runs in one.
nested() {
    if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null; then
        OBY_CHECK_PLAN=1 /usr/bin/sandbox-exec -p \
            '(version 1)(allow default)(deny process-exec (literal "/usr/bin/sandbox-exec"))' \
            scripts/oby-check.sh HEAD 2>&1
    else
        planned
    fi
}
code core/crates/obid/src/sandbox.rs 1
code core/crates/obid/src/foo.rs 1
code core/crates/obid/tests/sandbox.rs 1
code core/crates/obid/tests/plugins_api.rs 1
code core/crates/obid/tests/foo.rs 1
# Not nested, they run.
if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null; then
    expect ' --test sandbox' 'not nested, tests/sandbox.rs runs'
fi
said=$(nested)
grep -q -- '--test foo' <<<"$said" || {
    echo "oby-check-test: nested, it didn't plan an ordinary test binary: $said" >&2
    exit 1
}
! grep -qE -- '--test (sandbox|plugins_api)' <<<"$said" || {
    echo "oby-check-test: nested, it planned a sandbox test binary: $said" >&2
    exit 1
}
grep -q 'tests/sandbox.rs makes a sandbox of its own' <<<"$said" || {
    echo "oby-check-test: nested, it didn't say tests/sandbox.rs wasn't run: $said" >&2
    exit 1
}
rm core/crates/obid/src/foo.rs core/crates/obid/tests/foo.rs
said=$(nested)
! grep -q '==> test_binaries *$' <<<"$said" || {
    echo "oby-check-test: nested, it planned a test step with no binary left: $said" >&2
    exit 1
}

# obid's test binaries run side by side, each to its end: one that fails doesn't stop the others,
# its failure is named and comes last, each runs quietly, and a binary cargo didn't build fails the
# check. Cargo is a stand-in here: it builds each `--test` it's given, unless OBY_FAKE_BUILDS_NONE.
fake=$(mktemp -d)
trap 'rm -rf "$repo" "$fake"' EXIT
cat >"$fake/cargo" <<STANDIN
#!/bin/bash
[[ " \$* " == *" --no-run "* ]] || exit 0
[[ -z "\$OBY_FAKE_BUILDS_NONE" ]] || exit 0
while [[ \$# -gt 0 ]]; do
    if [[ "\$1" == --test ]]; then
        printf '{"reason":"compiler-artifact","target":{"kind":["test"],"name":"%s","src_path":"%s"},"executable":"%s"}\n' \
            "\$2" "\$PWD/core/crates/obid/tests/\$2.rs" "$fake/\$2-0123abcd"
        shift
    fi
    shift
done
printf '{"reason":"compiler-artifact","target":{"kind":["bin"],"name":"obid","src_path":"x"},"executable":"$fake/obid"}\n'
STANDIN
printf '#!/bin/bash\nsleep 2\necho "slow ran from $PWD with $* and $RUST_TEST_THREADS threads"\necho "test result: ok. 1 passed"\n' >"$fake/slow-0123abcd"
printf '#!/bin/bash\necho "test result: FAILED. 0 passed; 1 failed"\nexit 101\n' >"$fake/fails-0123abcd"
printf '#!/bin/bash\necho "the obid binary ran as a test"\n' >"$fake/obid"
chmod +x "$fake"/*
code core/crates/obid/tests/slow.rs 3
code core/crates/obid/tests/fails.rs 1
run() { env -u OBY_CHECK_PLAN PATH="$fake:$PATH" OBY_BUILD_JOBS=2 "$@" scripts/oby-check.sh HEAD 2>&1; }
if ran=$(run); then
    echo "oby-check-test: a failing test binary passed the check: $ran" >&2
    exit 1
fi
for said in 'failed: test_binaries' 'tests/fails.rs failed' \
    'slow ran from .*/core/crates/obid with --quiet and 2 threads' 'Running slow' 'Running fails'; do
    grep -q "$said" <<<"$ran" || {
        echo "oby-check-test: the test binaries' run didn't say \"$said\": $ran" >&2
        exit 1
    }
done
[[ $(grep -n 'Running slow' <<<"$ran" | cut -d: -f1) -lt $(grep -n 'Running fails' <<<"$ran" | cut -d: -f1) ]] || {
    echo "oby-check-test: a failing binary's output didn't come last: $ran" >&2
    exit 1
}
! grep -q 'the obid binary ran' <<<"$ran" || {
    echo "oby-check-test: it ran a binary that isn't a test: $ran" >&2
    exit 1
}
! grep -q 'tests/slow.rs failed' <<<"$ran" || {
    echo "oby-check-test: it named a binary that passed: $ran" >&2
    exit 1
}
rm core/crates/obid/tests/fails.rs
if ran=$(run OBY_FAKE_BUILDS_NONE=1); then
    echo "oby-check-test: a check whose test binaries weren't built passed: $ran" >&2
    exit 1
fi
grep -q 'found 0 of the [0-9]* test binaries cargo built' <<<"$ran" || {
    echo "oby-check-test: it didn't say the test binaries weren't built: $ran" >&2
    exit 1
}
rm core/crates/obid/tests/slow.rs

# OBY_CHECK_ALL=1 plans everything, whatever changed: every test binary, the lib and bin tests,
# every crate, and this test.
commit() { git add -A >/dev/null && git -c user.name=check -c user.email=check@example.invalid -c commit.gpgsign=false commit --quiet --allow-empty -m "$1"; }
code core/crates/obid/tests/one.rs 1
code core/crates/obi-protocol/src/lib.rs 1
commit all
said=$(OBY_CHECK_ALL=1 OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
for planned in '--test one' '-p obid --lib --bins' '-p obi-protocol' 'oby-check-test.sh'; do
    grep -q -- "$planned" <<<"$said" || {
        echo "oby-check-test: OBY_CHECK_ALL didn't plan \"$planned\": $said" >&2
        exit 1
    }
done
rm -r core/crates/obi-protocol
rm -r core/crates/obid

echo 'oby-check-test: passed'
