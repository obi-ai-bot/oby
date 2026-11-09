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
planned() { OBY_CHECK_PLAN=1 scripts/oby-check.sh "${since:-HEAD}" 2>&1; }
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
    echo "oby-check-test: a run past the limit failed without naming it:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$ran" >&2
    exit 1
}

code "$main" "$limit"
{ echo '#[cfg(test)]'; echo; echo 'mod tests {'; seq 50; echo '}'; } >>"$main"
expect no "$past" "a file's tests don't count"
code "$main" "$limit"
{ echo '#[cfg(test)]'; echo 'pub(crate) mod tests {'; seq 50; echo '}'; } >>"$main"
expect no "$past" "a file's public test module doesn't count"

# cfg(test) helper counted: a `#[cfg(test)]` on one helper doesn't hide the production code after it.
code "$main" "$((limit - 2))"
{ echo '#[cfg(test)]'; echo 'fn helper() {}'; echo 'fn production() {}'; echo '#[cfg(test)]'; echo 'mod tests {}'; } >>"$main"
expect "$past" 'a cfg(test) helper and the code after it count'
code "$main" "$((limit - 2))"
{ echo '#[cfg(test)]'; echo 'mod helpers;'; echo 'fn production() {}'; echo '#[cfg(test)]'; echo 'mod tests {}'; } >>"$main"
expect "$past" 'an out-of-line cfg(test) module and the code after it count'
# Code after a mid-file test module counts: only the module, to its `}` in column 0, doesn't.
code "$main" "$((limit - 1))"
{ echo '#[cfg(test)]'; echo 'mod tests {'; echo '    fn t() {}'; echo '}'; echo 'fn production() {}'; echo 'fn more() {}'; } >>"$main"
expect "$past" 'code after a mid-file test module counts'
{ echo '#[cfg(test)]'; echo 'mod t {}'; seq "$((limit + 1))"; } >"$main"
expect "$past" 'a one-line test module at the top hides nothing after it'
rm "$main"

code core/crates/obid/src/sphere/inbox.rs "$((limit + 1))"
expect "$past" "the Sphere's inbox counts"
rm core/crates/obid/src/sphere/inbox.rs
code core/crates/obid/src/sphere/link/inbox_actions.rs "$((limit + 1))"
expect "$past" "the Sphere's inbox actions count"
rm core/crates/obid/src/sphere/link/inbox_actions.rs

code core/crates/obid/src/sphere/link.rs "$((limit + 1))"
expect no "$past" "the Sphere doesn't count"
rm core/crates/obid/src/sphere/link.rs

code core/crates/obi-protocol/src/lib.rs "$((limit + 1))"
expect no "$past" "the Sphere's protocol doesn't count"
rm core/crates/obi-protocol/src/lib.rs

code core/crates/new/src/lib.rs "$((limit + 1))"
expect "$past" 'a new crate counts'
rm core/crates/new/src/lib.rs

# Far under fails: a cut of more than 1,000 lines lowers the limit in the same change.
code "$main" "$((limit - 1001))"
expect "lower obid_lines_limit in this script from $limit to $(((limit - 1001 + 499) / 500 * 500))" 'a cut asks for the limit to come down, to a number'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then
    echo 'oby-check-test: a run far under the limit passed' >&2
    exit 1
fi
grep -q "failed: the Rust core's line limit" <<<"$ran" || {
    echo "oby-check-test: a run far under the limit failed without naming it: $ran" >&2
    exit 1
}
code "$main" "$((limit - 1000))"
expect no 'lower obid_lines_limit' 'a small cut leaves the limit'

# On a job's base, which changed nothing, obid names the paths the job changed (PUBLISHING-18):
# those are what changed, not the diff (main.rs, which would plan every lib test).
export OBY_CHECK_PATHS=core/crates/obid/src/facts.rs
expect 'lib -- facts::$' 'OBY_CHECK_PATHS says what changed'
unset OBY_CHECK_PATHS

# A job's check runs the base's copy on the commit (PUBLISHING-7), against where its branch started:
# here the working copy is the base's, HEAD the commit, and `since` where the branch started.
commit_scripts() { git add scripts && git -c user.name=check -c user.email=check@example.invalid -c commit.gpgsign=false commit --quiet --allow-empty -m "$1"; }
set_limit() { # name value, in the working copy (its mode kept)
    sed "s/^$1=.*/$1=$2/" scripts/oby-check.sh >scripts/edited
    cat scripts/edited >scripts/oby-check.sh
    rm scripts/edited
}
since=HEAD~1
# A cut that lowers the limit passes the base's copy: the branch's lower one holds.
set_limit obid_lines_limit "$((limit - 1500))"
commit_scripts 'a cut'
git show HEAD~1:scripts/oby-check.sh >scripts/oby-check.sh
code "$main" "$((limit - 1500))"
expect no 'lower obid_lines_limit' "a commit's lower limit holds against the base's copy"
expect no "$past" "a commit's lower limit holds against the base's copy"
code "$main" "$((limit - 1499))"
expect "$past" "a commit's lower limit holds against the base's copy"
git reset --quiet --hard HEAD~1
# A commit's higher limit isn't taken.
set_limit obid_lines_limit "$((limit + 1000))"
commit_scripts 'a raise'
git show HEAD~1:scripts/oby-check.sh >scripts/oby-check.sh
code "$main" "$((limit + 1))"
expect "$past" "a commit's higher limit isn't taken"
git reset --quiet --hard HEAD~1
# Dev's limit raised since the branch started, which didn't change it: dev's holds.
commit_scripts 'other work'
set_limit obid_lines_limit "$((limit + 1000))"
code "$main" "$((limit + 500))"
expect no "$past" "dev's raise holds for a branch that started before it"
git reset --quiet --hard HEAD~1
# Dev's limit lowered since: past it, the check says to rebase.
commit_scripts 'other work'
set_limit obid_lines_limit "$((limit - 1000))"
code "$main" "$limit"
expect "$past" "dev's cut holds for a branch that started before it"
expect 'rebase onto dev' "a branch that started before dev's limit changed is told to rebase"
git reset --quiet --hard HEAD~1
code "$main" "$((limit + 1))"
since=HEAD expect no 'rebase onto dev' "a branch whose limit didn't move isn't told to rebase"
# A commit with no scripts/oby-check.sh fails, saying so.
git rm --quiet scripts/oby-check.sh
git -c user.name=check -c user.email=check@example.invalid -c commit.gpgsign=false commit --quiet -m 'no check'
mkdir -p scripts
git show HEAD~1:scripts/oby-check.sh >scripts/oby-check.sh
chmod +x scripts/oby-check.sh
code "$main" 1
expect 'this commit has no scripts/oby-check.sh' 'a commit that deletes the check'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD~1 2>&1); then
    echo 'oby-check-test: a run on a commit with no check passed' >&2
    exit 1
fi
grep -q 'failed: scripts/oby-check.sh is gone' <<<"$ran" || {
    echo "oby-check-test: a run on a commit with no check failed without naming it: $ran" >&2
    exit 1
}
git reset --quiet --hard HEAD~1
since=
rm "$main"

# jobs.rs may only shrink: past its own limit fails, though the whole is within its own.
jobs=core/crates/obid/src/jobs.rs
jobs_limit=$(sed -n 's/^jobs_rs_limit=//p' scripts/oby-check.sh)
code "$jobs" "$jobs_limit"
{ echo '#[cfg(test)]'; echo 'mod tests {'; seq 50; echo '}'; } >>"$jobs"
expect no 'jobs.rs has .* past' 'jobs.rs at its limit is within it'
code "$jobs" "$((jobs_limit + 1))"
expect 'jobs.rs has .* past its limit' 'jobs.rs one line past its limit'
expect no "the Rust core has .* $past" 'jobs.rs past its limit while the whole is within its own'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then
    echo 'oby-check-test: a run with jobs.rs past its limit passed' >&2
    exit 1
fi
grep -q "failed: jobs.rs's line limit" <<<"$ran" || {
    echo "oby-check-test: a run with jobs.rs past its limit failed without naming it: $ran" >&2
    exit 1
}
# A commit that shrinks jobs.rs and lowers its limit is held to that, against the base's copy.
since=HEAD~1
set_limit jobs_rs_limit "$((jobs_limit - 100))"
commit_scripts 'jobs.rs shrinks'
git show HEAD~1:scripts/oby-check.sh >scripts/oby-check.sh
code "$jobs" "$((jobs_limit - 100))"
expect no 'jobs.rs has .* past' "a commit's lower jobs.rs limit holds against the base's copy"
code "$jobs" "$((jobs_limit - 99))"
expect 'jobs.rs has .* past its limit' "a commit's lower jobs.rs limit holds against the base's copy"
git reset --quiet --hard HEAD~1
since=
rm "$jobs"

# Nested in a job's sandbox, the tests that make a sandbox of their own aren't run, and a change
# they'd test ends the check couldn't judge (PUBLISHING-22).
# Here the check runs in a sandbox of this test's where its own sandbox-exec fails, as in a job's,
# unless this test already runs in one.
nested_run() {
    if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null; then
        /usr/bin/sandbox-exec -p \
            '(version 1)(allow default)(deny process-exec (literal "/usr/bin/sandbox-exec"))' \
            scripts/oby-check.sh HEAD
    else
        scripts/oby-check.sh HEAD
    fi
}
nested() { OBY_CHECK_PLAN=1 nested_run 2>&1; }
code core/crates/obid/src/sandbox.rs 1
code core/crates/obid/src/foo.rs 1
code core/crates/obid/tests/sandbox.rs 1
code core/crates/obid/tests/plugins_api.rs 1
code core/crates/obid/tests/foo.rs 1
# At its line limit, so only the box's tests can leave it unjudged (far under fails the check too).
code "$main" "$((limit - 500))"
# Not nested, they run.
if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null; then
    expect ' --test sandbox' 'not nested, tests/sandbox.rs runs'
fi
said=$(nested)
grep -q -- '--test foo' <<<"$said" || {
    echo "oby-check-test: nested, it didn't plan an ordinary test binary:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$said" >&2
    exit 1
}
! grep -qE -- '--test (sandbox|plugins_api)' <<<"$said" || {
    echo "oby-check-test: nested, it planned a sandbox test binary:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$said" >&2
    exit 1
}
# The line obid reads (PUBLISHING-22), a line of its own.
judge="couldn't judge: tests/sandbox.rs makes a sandbox of its own and can't run in a job's; run the check on the Mac outside a job"
grep -qxF "$judge" <<<"$said" || {
    echo "oby-check-test: nested, it didn't say it couldn't judge tests/sandbox.rs:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$said" >&2
    exit 1
}
# Skipped, they end the check unjudged, never passed: a change to the box never merges untested.
if ran=$(OBY_CHECK_PLAN= nested_run 2>&1); then
    echo 'oby-check-test: nested, a change to the box passed with tests/sandbox.rs not run' >&2
    exit 1
fi
grep -q "failed: tests/sandbox.rs, which can't run in a job's sandbox" <<<"$ran" || {
    echo "oby-check-test: nested, a change to the box failed without naming tests/sandbox.rs:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$ran" >&2
    exit 1
}
# Here its cargo steps fail too (the fixture's files aren't Rust): that's the change's to fix, so it
# doesn't say it couldn't judge, or a change that doesn't build could be published by hand.
grep -q "failed: .*cargo" <<<"$ran" && ! grep -q "^couldn't judge:" <<<"$ran" || {
    echo "oby-check-test: nested, it said it couldn't judge a change that also failed elsewhere:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$ran" >&2
    exit 1
}
rm core/crates/obid/src/foo.rs core/crates/obid/tests/foo.rs "$main"
said=$(nested)
! grep -q '==> test_binaries *$' <<<"$said" || {
    echo "oby-check-test: nested, it planned a test step with no binary left:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$said" >&2
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
# every crate, this test, the iPhone build, swift test and the Electron app.
commit() { git add -A >/dev/null && git -c user.name=check -c user.email=check@example.invalid -c commit.gpgsign=false commit --quiet --allow-empty -m "$1"; }
code core/crates/obid/tests/one.rs 1
code core/crates/obi-protocol/src/lib.rs 1
commit all
said=$(OBY_CHECK_ALL=1 OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
for planned in '--test one' '-p obid --lib --bins' '-p obi-protocol' 'oby-check-test.sh' 'build-for-testing' 'swift test' 'apps/electron run --silent typecheck'; do
    grep -q -- "$planned" <<<"$said" || {
        echo "oby-check-test: OBY_CHECK_ALL didn't plan \"$planned\": $said" >&2
        exit 1
    }
done
rm -r core/crates/obi-protocol
rm -r core/crates/obid

echo 'oby-check-test: passed'
