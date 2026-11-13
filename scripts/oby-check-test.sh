#!/bin/bash
# What scripts/oby-check.sh's line limit promises (AGENTS.md, "Dispatcher, not harness"), run on a
# repository of its own. scripts/oby-check.sh runs this when it changes.
set -eo pipefail
# Each case plans what it changed: a full check's own OBY_CHECK_ALL or OBY_CHECK_PATHS isn't theirs.
unset OBY_CHECK_ALL OBY_CHECK_PATHS OBY_CHECK_ONLY
# Their runs are the script's own, never the installed oby-check-blok's (which sets this itself);
# the cases of that hand-off, at the end, unset it.
export OBY_CHECK_INNER=1
script="$(cd "$(dirname "$0")" && pwd)/oby-check.sh"
# obid installs the base's authoritative check script before running this test. Keep testing
# its existing rules below, but test the new group interface against the commit that adds it.
committed_script=$(git show HEAD:scripts/oby-check.sh)
limit=$(sed -n 's/^obid_lines_limit=//p' "$script")
repo=$(mktemp -d "${TMPDIR:-/tmp}/oby-check-test.XXXXXX")
trap 'rm -rf "$repo"' EXIT
cd "$repo"
git init --quiet
mkdir scripts
cp "$script" scripts/oby-check.sh
# obid's tests' sleeps (docs/contracts/ci.md): the base holds as many as its copy's limit, none for a
# copy with no limit, so no other case fails on them.
sleepy=core/crates/obid/tests/fixtures/sleeps.rs
sleeps() { # file count
    mkdir -p "$(dirname "$1")"
    for ((n = 0; n < ${2:-0}; n++)); do echo 'std::thread::sleep(d);'; done >"$1"
}
sleeps "$sleepy" "$(sed -n 's/^tests_sleep_limit=//p' "$script")"
git add scripts "$sleepy"
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

# obid's tests may only lose sleeps (docs/contracts/ci.md). The base's copy a job's check installs
# may predate the limit: these run on the commit's own copy, committed here as the base.
printf '%s\n' "$committed_script" >scripts/oby-check.sh
commit_scripts 'the sleep limit'
sleep_limit=$(sed -n 's/^tests_sleep_limit=//p' scripts/oby-check.sh)
sleep_failed="failed: obid's tests' sleep limit"
# What changed is the tests' sleeps, said so: at the base's own count the diff alone has nothing, and
# the limits wouldn't be judged.
export OBY_CHECK_PATHS=$sleepy
sleeps "$sleepy" "$sleep_limit"
expect no "$sleep_failed\|sleep calls, past\|lower tests_sleep_limit" 'CI-12: obid tests at their sleep limit are within it'
expect "obid's tests have $sleep_limit sleep calls, within their limit of $sleep_limit" 'CI-12: at the sleep limit, it says so'
# CI-12: one more sleep fails, saying to wait for state instead.
sleeps "$sleepy" "$((sleep_limit + 1))"
expect "have $((sleep_limit + 1)) sleep calls, past their limit of $sleep_limit.*support::within" 'CI-12: one sleep past the limit says to wait with support::within'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then
    echo 'oby-check-test: CI-12: a run with one sleep past the limit passed' >&2
    exit 1
fi
grep -q "$sleep_failed" <<<"$ran" || {
    echo "oby-check-test: CI-12: a run with one sleep past the limit failed without naming it:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$ran" >&2
    exit 1
}
# CI-13: one sleep fewer asks for the limit to come down, to the new count.
sleeps "$sleepy" "$((sleep_limit - 1))"
expect "lower tests_sleep_limit in this script from $sleep_limit to $((sleep_limit - 1)), so the cut stays cut" 'CI-13: a removed sleep asks for the limit to come down'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then
    echo 'oby-check-test: CI-13: a run with one sleep under the limit passed' >&2
    exit 1
fi
grep -q "$sleep_failed" <<<"$ran" || {
    echo "oby-check-test: CI-13: a run with one sleep under the limit failed without naming it:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$ran" >&2
    exit 1
}
# CI-13: a commit that lowers the limit is held to it, against the base's copy.
since=HEAD~1
set_limit tests_sleep_limit "$((sleep_limit - 1))"
commit_scripts 'a sleep cut'
git show HEAD~1:scripts/oby-check.sh >scripts/oby-check.sh
expect no "$sleep_failed\|sleep calls, past\|lower tests_sleep_limit" "CI-13: a commit's lower sleep limit holds against the base's copy"
sleeps "$sleepy" "$sleep_limit"
expect "have $sleep_limit sleep calls, past their limit of $((sleep_limit - 1))" "CI-13: a commit's lower sleep limit holds against the base's copy"
git reset --quiet --hard HEAD~2
since=
unset OBY_CHECK_PATHS

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
# tests/jobs_setup.rs makes a sandbox of its own too, but only the setup's and the box's code plan
# it: nested, a change to jobs.rs alone is judged, and one to jobs/setup.rs couldn't be.
code core/crates/obid/tests/jobs_setup.rs 1
code "$main" "$((limit - 500))"
said=$(OBY_CHECK_PATHS=core/crates/obid/src/jobs.rs nested)
! grep -qE -- "jobs_setup|^couldn't judge:" <<<"$said" || {
    echo "oby-check-test: nested, a change to jobs.rs alone planned tests/jobs_setup.rs:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$said" >&2
    exit 1
}
said=$(OBY_CHECK_PATHS=core/crates/obid/src/jobs/setup.rs nested)
grep -qxF "couldn't judge: tests/jobs_setup.rs makes a sandbox of its own and can't run in a job's; run the check on the Mac outside a job" <<<"$said" &&
    ! grep -q -- '--test jobs_setup' <<<"$said" || {
    echo "oby-check-test: nested, a change to jobs/setup.rs didn't leave tests/jobs_setup.rs unjudged:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$said" >&2
    exit 1
}
# Not nested, the Mac's own check plans them with tests/jobs_api.rs: a change to jobs.rs runs them.
if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null; then
    code core/crates/obid/tests/jobs_api.rs 1
    said=$(OBY_CHECK_PATHS=core/crates/obid/src/jobs.rs OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
    grep -q -- '--test jobs_setup' <<<"$said" || {
        echo "oby-check-test: not nested, a change to jobs.rs didn't plan tests/jobs_setup.rs:" >&2
        # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
        sed 's/^/| /' <<<"$said" >&2
        exit 1
    }
    said=$(OBY_CHECK_PATHS=core/crates/obid/src/process.rs OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
    grep -q -- '--test jobs_setup' <<<"$said" || {
        echo "oby-check-test: not nested, a change to process.rs didn't plan tests/jobs_setup.rs:" >&2
        # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
        sed 's/^/| /' <<<"$said" >&2
        exit 1
    }
    rm core/crates/obid/tests/jobs_api.rs
fi
rm core/crates/obid/tests/jobs_setup.rs "$main"
# What reads a released obid's files plans tests/released_files.rs (docs/contracts/released-files.md).
code core/crates/obid/tests/released_files.rs 1
for module in job config store; do
    said=$(OBY_CHECK_PATHS=core/crates/obid/src/$module.rs OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
    grep -q -- '--test released_files' <<<"$said" || {
        echo "oby-check-test: a change to $module.rs didn't plan tests/released_files.rs:" >&2
        # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
        sed 's/^/| /' <<<"$said" >&2
        exit 1
    }
done
rm core/crates/obid/tests/released_files.rs
# The routes and the clients that call them plan tests/routes_guard.rs (docs/contracts/ci.md, CI-9 to 11),
# an Electron test doesn't.
code core/crates/obid/tests/routes_guard.rs 1
for path in core/crates/obid/src/api.rs core/crates/obid/src/gateway.rs docs/api/routes.txt \
    apps/electron/src/renderer/src/obid.ts apps/shared/Runner.swift mcp/server/Server.swift; do
    said=$(OBY_CHECK_PATHS=$path OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
    grep -q -- '--test routes_guard' <<<"$said" || {
        echo "oby-check-test: a change to $path didn't plan tests/routes_guard.rs:" >&2
        # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
        sed 's/^/| /' <<<"$said" >&2
        exit 1
    }
done
said=$(OBY_CHECK_PATHS=apps/electron/src/renderer/src/test/x.test.ts OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
! grep -q routes_guard <<<"$said" || {
    echo "oby-check-test: a change to an Electron test planned tests/routes_guard.rs:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$said" >&2
    exit 1
}
rm core/crates/obid/tests/routes_guard.rs

# obid's test binaries run side by side, each to its end: one that fails doesn't stop the others,
# its failure is named and comes last, each runs quietly, and a binary cargo didn't build fails the
# check. Cargo is a stand-in here: it builds each `--test` it's given, unless OBY_FAKE_BUILDS_NONE.
fake=$(mktemp -d "${TMPDIR:-/tmp}/oby-check-fake.XXXXXX")
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
printf '#!/bin/bash\nsleep 2\necho "slow ran from $PWD with $* and $RUST_TEST_THREADS threads"\necho "its output kept in $(ls -d "$TMPDIR"/oby-check.* 2>/dev/null)"\necho "test result: ok. 1 passed"\n' >"$fake/slow-0123abcd"
printf '#!/bin/bash\necho "test result: FAILED. 0 passed; 1 failed"\nexit 101\n' >"$fake/fails-0123abcd"
printf '#!/bin/bash\necho "the obid binary ran as a test"\n' >"$fake/obid"
chmod +x "$fake"/*
code core/crates/obid/tests/slow.rs 3
code core/crates/obid/tests/fails.rs 1
# Its own TMPDIR, as a job's box gives it: the Mac's temporary folder isn't one a box can write.
mkdir "$fake/tmp"
run() { env -u OBY_CHECK_PLAN PATH="$fake:$PATH" TMPDIR="$fake/tmp" OBY_BUILD_JOBS=2 "$@" scripts/oby-check.sh HEAD 2>&1; }
if ran=$(run); then
    echo "oby-check-test: a failing test binary passed the check: $ran" >&2
    exit 1
fi
for said in 'failed: test_binaries' 'tests/fails.rs failed' \
    'slow ran from .*/core/crates/obid with --quiet and 2 threads' 'Running slow' 'Running fails' \
    "its output kept in $fake/tmp/oby-check\."; do
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
for planned in '--test one' '-p obid --lib --bins' '-p obi-protocol' 'oby-check-test.sh' 'apps/electron run --silent typecheck'; do
    grep -q -- "$planned" <<<"$said" || {
        echo "oby-check-test: OBY_CHECK_ALL didn't plan \"$planned\": $said" >&2
        exit 1
    }
done
if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null; then
    for planned in 'build-for-testing' 'swift test'; do
        grep -q -- "$planned" <<<"$said" || {
            echo "oby-check-test: OBY_CHECK_ALL didn't plan \"$planned\": $said" >&2
            exit 1
        }
    done
else
    grep -q "don't run in a job's sandbox" <<<"$said" &&
        ! grep -qE '==> (xcodebuild|swift test)' <<<"$said" || {
        echo "oby-check-test: OBY_CHECK_ALL didn't skip the Apple checks in a job's sandbox: $said" >&2
        exit 1
    }
fi
rm -r core/crates/obi-protocol
rm -r core/crates/obid

# Each filtered plan contains only its group, including side effects outside step.
# Use the candidate's implementation: the base's script may predate OBY_CHECK_ONLY.
printf '%s\n' "$committed_script" >scripts/oby-check.sh
for group in rust electron swift scan; do
    said=$(OBY_CHECK_ONLY="$group" OBY_CHECK_ALL=1 OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
    case "$group" in
        rust) wanted='cargo'; unwanted='apps/electron run\|==> gitleaks\|==> swift\|==> xcodebuild' ;;
        electron) wanted='apps/electron run'; unwanted='==> cargo\|==> gitleaks\|Rust core has\|==> swift\|==> xcodebuild' ;;
        swift) wanted='build-for-testing\|swift test\|Apple apps and swift test'; unwanted='==> cargo\|==> gitleaks\|Rust core has\|apps/electron run' ;;
        scan) wanted='gitleaks'; unwanted='==> cargo\|Rust core has\|apps/electron run\|==> swift\|==> xcodebuild' ;;
    esac
    grep -q "$wanted" <<<"$said" && ! grep -q "$unwanted" <<<"$said" || {
        echo "oby-check-test: incorrect $group filter: $said" >&2
        exit 1
    }
done

# The machine-readable plan names each selected group once, independently of messages.
said=$(OBY_CHECK_ALL=1 OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
for group in rust electron swift scan; do
    [[ $(grep -cxF "oby-check: group $group" <<<"$said") == 1 ]] || {
        echo "oby-check-test: missing or repeated $group plan marker: $said" >&2
        exit 1
    }
done
said=$(OBY_CHECK_PATHS=tools/oby-check-blok/src/lib.rs OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
for command in 'cargo fmt --manifest-path tools/oby-check-blok/Cargo.toml --all -- --check' \
    'cargo clippy --manifest-path tools/oby-check-blok/Cargo.toml --all-targets -- -D warnings' \
    'cargo test --manifest-path tools/oby-check-blok/Cargo.toml' 'oby-check: group rust'; do
    grep -qF "$command" <<<"$said" || { echo "oby-check-test: missing pilot step $command: $said" >&2; exit 1; }
done
said=$(OBY_CHECK_PATHS=README.md OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
! grep -qE '^oby-check: group (rust|electron|swift)$' <<<"$said" || {
    echo "oby-check-test: unchanged groups were planned: $said" >&2; exit 1;
}

# The check is the installed oby-check-blok's (BLOK-1, PUBLISHING-26): a run with none of
# OBY_CHECK_INNER, OBY_CHECK_PLAN and OBY_CHECK_ONLY hands itself to the one on PATH, resolved and
# vetted, from the repository's root, with its base. A stand-in says how it was called; its nodes
# binary beside it. Their runs get a TMPDIR of their own: the stand-ins are in this test's, which
# the script refuses, as it refuses a job's.
blok=$(mktemp -d "${TMPDIR:-/tmp}/oby-check-blok.XXXXXX")
trap 'rm -rf "$repo" "$fake" "$blok"' EXIT
stand_in() { # folder: a stand-in tool and nodes binary there, as an install leaves them
    mkdir -p "$1"
    chmod 755 "$1"
    printf '#!/bin/bash\necho "blok stand-in: $0 $* from $PWD"\nexit 7\n' >"$1/oby-check-blok"
    printf '#!/bin/bash\n' >"$1/oby-check-blok-nodes"
    chmod 755 "$1/oby-check-blok" "$1/oby-check-blok-nodes"
}
stand_in "$blok/bin"
installed=$(cd "$blok/bin" && pwd -P)
# PATH with no oby-check-blok on it, the installed one included.
bare=$(tr ':' '\n' <<<"$PATH" | while IFS= read -r dir; do
    [[ -z "$dir" || -e "$dir/oby-check-blok" ]] || printf '%s:' "$dir"
done)
bare=${bare%:}
handed() { (cd scripts && env -u OBY_CHECK_INNER -u OBY_CHECK_PLAN -u OBY_CHECK_ONLY OBY_CHECK_PATHS=README.md TMPDIR="$repo/.tmp" "$@" 2>&1); }
said=$(handed PATH="$blok/bin:$bare" ./oby-check.sh HEAD) && status=0 || status=$?
[[ $status == 7 && "$said" == "blok stand-in: $installed/oby-check-blok check HEAD from $(pwd)" ]] || {
    echo "oby-check-test: a run didn't hand itself to oby-check-blok, its status kept ($status): $said" >&2
    exit 1
}
said=$(handed PATH="$blok/bin:$bare" ./oby-check.sh) || true
grep -qxF "blok stand-in: $installed/oby-check-blok check origin/dev from $(pwd)" <<<"$said" || {
    echo "oby-check-test: with no base, oby-check-blok wasn't given origin/dev: $said" >&2
    exit 1
}
# Under oby-check-blok (OBY_CHECK_INNER), and planning or running one group, the script runs itself.
for set in OBY_CHECK_INNER=1 OBY_CHECK_PLAN=1 OBY_CHECK_ONLY=scan; do
    said=$(handed PATH="$blok/bin:$bare" "$set" ./oby-check.sh HEAD) || true
    ! grep -q 'blok stand-in' <<<"$said" && grep -q '^==> \|oby-check: gitleaks' <<<"$said" || {
        echo "oby-check-test: with $set, the script didn't run itself: $said" >&2
        exit 1
    }
done
# None installed, or one the checked code could have put there or anyone else could change: nothing
# passes, the script never runs its own check instead, and it says why.
refuses() { # what PATH TMPDIR why
    local said
    if said=$(handed PATH="$2" TMPDIR="$3" ./oby-check.sh HEAD); then
        echo "oby-check-test: with $1, the check passed: $said" >&2
        exit 1
    fi
    grep -q "^oby-check: failed: oby-check-blok (.*$4" <<<"$said" && ! grep -q 'blok stand-in\|^==> \|planted ran' <<<"$said" || {
        echo "oby-check-test: with $1, it didn't fail naming oby-check-blok and \"$4\": $said" >&2
        exit 1
    }
}
stand_in rel
stand_in "$blok/writable"
chmod g+w "$blok/writable/oby-check-blok"
stand_in "$blok/open"
chmod o+w "$blok/open"
stand_in "$blok/nodes"
chmod g+w "$blok/nodes/oby-check-blok-nodes"
stand_in "$blok/no-nodes"
rm "$blok/no-nodes/oby-check-blok-nodes"
mkdir -p "$blok/linked"
chmod 755 "$blok/linked"
ln -s "$(pwd)/rel/oby-check-blok" "$blok/linked/oby-check-blok"
ln -s "$(pwd)/rel/oby-check-blok-nodes" "$blok/linked/oby-check-blok-nodes"
# A link in a folder nobody else may write, to a binary its group may write: judged where it leads.
mkdir -p "$blok/link-out"
chmod 755 "$blok/link-out"
ln -s "$blok/writable/oby-check-blok" "$blok/link-out/oby-check-blok"
stand_in "$blok/nodes-folder"
rm "$blok/nodes-folder/oby-check-blok-nodes"
mkdir -m 755 "$blok/nodes-folder/oby-check-blok-nodes"
# A path with a newline in it, which a comparison of paths would lose at its end.
stand_in "$blok/new"$'\n'"line"
tmp="$repo/.tmp"
refuses 'none installed' "$bare" "$tmp" "isn't installed on PATH"
refuses 'one only on a relative path' "rel:$bare" "$tmp" "isn't installed on PATH"
refuses 'a link into the checked repository' "$blok/linked:$bare" "$tmp" 'inside .* which the checked code can write'
refuses 'one in TMPDIR' "$blok/bin:$bare" "$blok" 'inside .* which the checked code can write'
refuses 'one its group may write' "$blok/writable:$bare" "$tmp" 'others may write'
refuses 'one in a folder others may write' "$blok/open:$bare" "$tmp" 'others may write'
refuses 'a nodes binary its group may write' "$blok/nodes:$bare" "$tmp" 'others may write'
refuses 'a link to one its group may write' "$blok/link-out:$bare" "$tmp" 'others may write'
refuses "a nodes binary that's a folder" "$blok/nodes-folder:$bare" "$tmp" "isn't a regular file"
refuses 'no nodes binary beside it' "$blok/no-nodes:$bare" "$tmp" 'no oby-check-blok-nodes beside'
refuses 'a newline in its path' "$blok/new"$'\n'"line:$bare" "$tmp" 'newline in its path'
refuses 'a TMPDIR ending in a newline' "$blok/bin:$bare" "$tmp"$'\n' 'has a newline in its name'
# On macOS: an access list, on the binary or its folder (macOS's ls marks such a file `@`, not `+`,
# when it has extended attributes, as cargo's installs do), and a folder that belongs to someone
# else, though nobody else may write it: its owner could swap the binary.
if [[ "$OSTYPE" == darwin* ]]; then
    stand_in "$blok/acl-file"
    xattr -w com.example.oby-check-test 1 "$blok/acl-file/oby-check-blok"
    chmod +a 'everyone allow write' "$blok/acl-file/oby-check-blok"
    refuses 'an access list on the binary' "$blok/acl-file:$bare" "$tmp" 'has an access list'
    stand_in "$blok/acl-folder"
    chmod +a 'everyone allow add_file,delete_child' "$blok/acl-folder"
    refuses 'an access list on its folder' "$blok/acl-folder:$bare" "$tmp" 'has an access list'
    # /private/var/at is daemon's; the root-owned file in it passes every other rule.
    if [[ -f /private/var/at/cron.deny ]]; then
        stand_in "$blok/foreign"
        ln -sf /private/var/at/cron.deny "$blok/foreign/oby-check-blok-nodes"
        refuses 'a nodes binary in a folder of someone else' "$blok/foreign:$bare" "$tmp" 'belongs to someone else'
    fi
fi
# Resolved once: what's vetted is what runs. A link on PATH flips between the installed stand-in and
# one planted in the checked repository (its nodes binary the installed one's): each run hands off
# to the installed one or fails, and never runs the planted one.
mkdir -p planted "$blok/flip"
chmod 755 "$blok/flip"
printf '#!/bin/bash\necho "planted ran"\nexit 0\n' >planted/oby-check-blok
chmod 755 planted/oby-check-blok
ln -s "$installed/oby-check-blok-nodes" planted/oby-check-blok-nodes
(while :; do
    ln -sfn "$installed/oby-check-blok" "$blok/flip/oby-check-blok"
    ln -sfn "$(pwd)/planted/oby-check-blok" "$blok/flip/oby-check-blok"
done) &
flipper=$!
ran=0
for _ in $(seq 40); do
    said=$(handed PATH="$blok/flip:$bare" ./oby-check.sh HEAD) || true
    if grep -q 'planted ran' <<<"$said"; then
        kill "$flipper"
        echo "oby-check-test: a link swapped while it was vetted ran the planted binary: $said" >&2
        exit 1
    fi
    ! grep -q 'blok stand-in' <<<"$said" || ran=$((ran + 1))
done
kill "$flipper"
wait "$flipper" 2>/dev/null || true
((ran > 0)) || {
    echo "oby-check-test: the installed stand-in never ran while the link flipped" >&2
    exit 1
}
rm -r rel planted

echo 'oby-check-test: passed'
