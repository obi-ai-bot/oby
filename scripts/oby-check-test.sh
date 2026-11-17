#!/bin/bash
# What scripts/oby-check.sh promises (docs/contracts/ci.md): obid's line count reported, its limits,
# its plans and its groups, run on a repository of its own. scripts/oby-check.sh runs this when it
# changes.
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
# AGENTS.md (CI-36 to 38): the base's is as big as its copy's byte limit, or the commit's copy's when
# the base's has none, or 1 byte, so no other case fails on it.
agents_md() { # bytes: the fixture's AGENTS.md, that size
    head -c "$1" /dev/zero | tr '\0' x >AGENTS.md
}
agents_limit=$(sed -n 's/^agents_md_bytes_limit=//p' "$script")
[[ -n "$agents_limit" ]] || agents_limit=$(sed -n 's/^agents_md_bytes_limit=//p' <<<"$committed_script")
agents_md "${agents_limit:-1}"
# Where the cases switch to the commit's copy: AGENTS.md at its limit, for the next commit.
agents_md_at_limit() {
    local at
    at=$(sed -n 's/^agents_md_bytes_limit=//p' scripts/oby-check.sh)
    agents_md "${at:-1}"
    git add AGENTS.md
}
# AGENTS.md's rules (CI-42 to 44): the manifest's one entry is in the fixture's AGENTS.md.
mkdir docs
printf 'The rules.\n\n### L1\n- `AGENTS.md`: x\n' >docs/agents-rules-manifest.md
git add scripts "$sleepy" AGENTS.md docs/agents-rules-manifest.md
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
main=core/crates/obid/src/main.rs
jobs=core/crates/obid/src/jobs.rs
jobs_limit=$(sed -n 's/^jobs_rs_limit=//p' scripts/oby-check.sh)
# What the Rust core's production lines outside the Sphere count (AGENTS.md, "Dispatcher, not
# harness"): the plan says exactly how many, from this copy of the script or the base's.
counted() { # lines, what it's about
    expect "the Rust core has $1 production lines outside the Sphere" "$2"
}
count=1000

code "$main" "$count"
counted "$count" "a file's lines count"
{ echo '#[cfg(test)]'; echo; echo 'mod tests {'; seq 50; echo '}'; } >>"$main"
counted "$count" "a file's tests don't count"
code "$main" "$count"
{ echo '#[cfg(test)]'; echo 'pub(crate) mod tests {'; seq 50; echo '}'; } >>"$main"
counted "$count" "a file's public test module doesn't count"

# cfg(test) helper counted: a `#[cfg(test)]` on one helper doesn't hide the production code after it.
code "$main" "$((count - 2))"
{ echo '#[cfg(test)]'; echo 'fn helper() {}'; echo 'fn production() {}'; echo '#[cfg(test)]'; echo 'mod tests {}'; } >>"$main"
counted "$((count + 1))" 'a cfg(test) helper and the code after it count'
code "$main" "$((count - 2))"
{ echo '#[cfg(test)]'; echo 'mod helpers;'; echo 'fn production() {}'; echo '#[cfg(test)]'; echo 'mod tests {}'; } >>"$main"
counted "$((count + 1))" 'an out-of-line cfg(test) module and the code after it count'
# Code after a mid-file test module counts: only the module, to its `}` in column 0, doesn't.
code "$main" "$((count - 1))"
{ echo '#[cfg(test)]'; echo 'mod tests {'; echo '    fn t() {}'; echo '}'; echo 'fn production() {}'; echo 'fn more() {}'; } >>"$main"
counted "$((count + 1))" 'code after a mid-file test module counts'
{ echo '#[cfg(test)]'; echo 'mod t {}'; seq "$((count + 1))"; } >"$main"
counted "$((count + 1))" 'a one-line test module at the top hides nothing after it'
rm "$main"

code core/crates/obid/src/sphere/inbox.rs "$((count + 1))"
counted "$((count + 1))" "the Sphere's inbox counts"
rm core/crates/obid/src/sphere/inbox.rs
code core/crates/obid/src/sphere/link/inbox_actions.rs "$((count + 1))"
counted "$((count + 1))" "the Sphere's inbox actions count"
rm core/crates/obid/src/sphere/link/inbox_actions.rs

code core/crates/obid/src/sphere/link.rs "$((count + 1))"
counted 0 "the Sphere doesn't count"
rm core/crates/obid/src/sphere/link.rs

code core/crates/obi-protocol/src/lib.rs "$((count + 1))"
counted 0 "the Sphere's protocol doesn't count"
rm core/crates/obi-protocol/src/lib.rs

code core/crates/new/src/lib.rs "$((count + 1))"
counted "$((count + 1))" 'a new crate counts'
rm core/crates/new/src/lib.rs

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
# A commit's higher limit isn't taken.
set_limit jobs_rs_limit "$((jobs_limit + 1000))"
commit_scripts 'a raise'
git show HEAD~1:scripts/oby-check.sh >scripts/oby-check.sh
code "$jobs" "$((jobs_limit + 1))"
expect 'jobs.rs has .* past its limit' "a commit's higher limit isn't taken"
git reset --quiet --hard HEAD~1
# Dev's limit raised since the branch started, which didn't change it: dev's holds.
commit_scripts 'other work'
set_limit jobs_rs_limit "$((jobs_limit + 1000))"
code "$jobs" "$((jobs_limit + 500))"
expect no 'jobs.rs has .* past' "dev's raise holds for a branch that started before it"
git reset --quiet --hard HEAD~1
# Dev's limit lowered since: past it, the check says to rebase.
commit_scripts 'other work'
set_limit jobs_rs_limit "$((jobs_limit - 1000))"
code "$jobs" "$jobs_limit"
expect 'jobs.rs has .* past its limit' "dev's cut holds for a branch that started before it"
expect 'rebase onto dev' "a branch that started before dev's limit changed is told to rebase"
git reset --quiet --hard HEAD~1
code "$jobs" "$((jobs_limit + 1))"
since=HEAD expect 'jobs.rs has .* past its limit' "a branch whose limit didn't move, past it"
since=HEAD expect no 'rebase onto dev' "a branch whose limit didn't move isn't told to rebase"
rm "$jobs"
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

# jobs.rs may only shrink: past its own limit fails.
code "$jobs" "$jobs_limit"
{ echo '#[cfg(test)]'; echo 'mod tests {'; seq 50; echo '}'; } >>"$jobs"
expect no 'jobs.rs has .* past' 'jobs.rs at its limit is within it'
code "$jobs" "$((jobs_limit + 1))"
expect 'jobs.rs has .* past its limit' 'jobs.rs one line past its limit'
counted "$((jobs_limit + 1))" "jobs.rs's lines count in the Rust core's"
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
agents_md_at_limit
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

wrong() { # what went wrong, said with the run's output
    echo "oby-check-test: $1:" >&2
    # Quoted, so no line of it starts as the check's own words would (PUBLISHING-22's sign).
    sed 's/^/| /' <<<"$ran" >&2
    exit 1
}
# AGENTS.md may only shrink (docs/contracts/ci.md, CI-36 to 38), on the commit's own copy of the
# script, committed here as the base with AGENTS.md at its limit. What changed is AGENTS.md, said so.
printf '%s\n' "$committed_script" >scripts/oby-check.sh
agents_md_at_limit
commit_scripts 'the byte limit'
agents_limit=$(sed -n 's/^agents_md_bytes_limit=//p' scripts/oby-check.sh)
agents_limit=${agents_limit:-1}
agents_failed="failed: AGENTS.md's byte limit"
export OBY_CHECK_PATHS=AGENTS.md
expect "AGENTS.md has $agents_limit bytes, within its limit of $agents_limit" 'CI-36: at the byte limit, it says so'
expect no 'bytes, past its limit\|lower agents_md_bytes_limit' 'CI-36: AGENTS.md at its byte limit is within it'
agents_md "$((agents_limit + 1))"
expect "AGENTS.md has $((agents_limit + 1)) bytes, past its limit of $agents_limit" 'CI-36: one byte past the byte limit'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then wrong 'CI-36: a run with AGENTS.md one byte past its limit passed'; fi
grep -qF "$agents_failed" <<<"$ran" || wrong 'CI-36: a run with AGENTS.md one byte past its limit failed without naming it'
# CI-37: up to 512 bytes under is within it; more asks for the limit to come down, to the size.
agents_md "$((agents_limit - 512))"
expect "AGENTS.md has $((agents_limit - 512)) bytes, within its limit of $agents_limit" 'CI-37: 512 bytes under the byte limit is within it'
expect no 'lower agents_md_bytes_limit' 'CI-37: a cut of 512 bytes leaves the limit'
agents_md "$((agents_limit - 513))"
expect "lower agents_md_bytes_limit in this script from $agents_limit to $((agents_limit - 513)), so the cut stays cut" 'CI-37: a cut of 513 bytes asks for the limit to come down, to the size'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then wrong 'CI-37: a run with AGENTS.md 513 bytes under its limit passed'; fi
grep -qF "$agents_failed" <<<"$ran" || wrong 'CI-37: a run with AGENTS.md 513 bytes under its limit failed without naming it'
# CI-37: a commit that lowers the limit is held to it, against the base's copy.
since=HEAD~1
set_limit agents_md_bytes_limit "$((agents_limit - 600))"
commit_scripts 'an AGENTS.md cut'
git show HEAD~1:scripts/oby-check.sh >scripts/oby-check.sh
agents_md "$((agents_limit - 600))"
expect no 'bytes, past its limit\|lower agents_md_bytes_limit' "CI-37: a commit's lower byte limit holds against the base's copy"
agents_md "$((agents_limit - 599))"
expect "AGENTS.md has $((agents_limit - 599)) bytes, past its limit of $((agents_limit - 600))" "CI-37: a commit's lower byte limit holds against the base's copy"
git reset --quiet --hard HEAD~1
since=
# CI-38: a change to AGENTS.md alone runs the limits, and no cargo step.
said=$(planned)
grep -qx 'oby-check: group rust' <<<"$said" && grep -q "AGENTS.md has $agents_limit bytes" <<<"$said" &&
    ! grep -q '^==> cargo' <<<"$said" || { ran=$said; wrong 'CI-38: a change to AGENTS.md alone did not plan the limits alone'; }
# CI-38: a commit with no AGENTS.md fails, saying so, never counting it as 0 bytes.
git rm --quiet AGENTS.md
commit_scripts 'no AGENTS.md'
expect 'this commit has no AGENTS.md' 'CI-38: a commit that deletes AGENTS.md'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then wrong 'CI-38: a run on a commit with no AGENTS.md passed'; fi
grep -qF 'failed: AGENTS.md is gone' <<<"$ran" && ! grep -q 'AGENTS.md has 0 bytes' <<<"$ran" ||
    wrong 'CI-38: a run on a commit with no AGENTS.md failed without naming it'
git reset --quiet --hard HEAD~2
unset OBY_CHECK_PATHS

# A raise of jobs_rs_limit names its payback and comes alone (docs/contracts/ci.md, CI-39 to 41), on
# the commit's own copy of the script, committed here as the base within every limit and with none of
# its payback lines, with a stand-in for this test, which the check runs when the script changes. Each
# case is a commit on it.
grep -v '^# payback: ' <<<"$committed_script" >scripts/oby-check.sh
agents_md_at_limit
raise_jobs=$(sed -n 's/^jobs_rs_limit=//p' scripts/oby-check.sh)
printf '#!/bin/bash\n' >scripts/oby-check-test.sh
commit_scripts 'the raise rules'
since=HEAD~1
payback() { # name line [line]: the line directly above the name's limit in the working copy, the third between them
    awk -v n="$1" -v l="$2" -v b="${3:-}" '$0 ~ "^" n "=" { print l; if (b != "") print b } { print }' scripts/oby-check.sh >scripts/edited
    cat scripts/edited >scripts/oby-check.sh
    rm scripts/edited
}
raise() { # name value [payback line [line between]]: committed, with what else is staged
    set_limit "$1" "$2"
    [[ -z "${3:-}" ]] || payback "$1" "$3" "${4:-}"
    commit_scripts "a raise of $1"
}
raise_run() { ran=$(env -u OBY_CHECK_PLAN OBY_CHECK_ONLY=rust scripts/oby-check.sh "$since" 2>&1); }
raise_fails() { # what it's about, the failure it names
    if raise_run; then wrong "$1 passed"; fi
    grep -qxF "oby-check: failed: $2" <<<"$ran" || wrong "$1 didn't fail naming \"$2\""
}
raise_passes() { # what it's about
    raise_run || wrong "$1 failed"
    grep -qx 'oby-check: passed' <<<"$ran" && ! grep -q "payback line\|in a pull request of its own" <<<"$ran" ||
        wrong "$1 didn't pass"
}
raise jobs_rs_limit "$((raise_jobs + 10))"
expect "raises jobs_rs_limit from $raise_jobs to $((raise_jobs + 10)) with no payback line" 'CI-39: a raise of jobs_rs_limit with no payback line says so'
raise_fails 'CI-39: a raise of jobs_rs_limit with no payback line' "jobs_rs_limit's payback line"
grep -qF "directly above \"jobs_rs_limit=$((raise_jobs + 10))\"" <<<"$ran" && ! grep -q 'in a pull request of its own' <<<"$ran" ||
    wrong "CI-39: a raise alone with no payback line didn't say where the line goes, or was told to come alone"
git reset --quiet --hard HEAD~1
raise jobs_rs_limit "$((raise_jobs + 10))" '# payback: #2131'
raise_passes 'CI-39: a raise alone with "# payback: #2131" directly above it'
[[ $(grep -A1 '^oby-check: jobs.rs has' <<<"$ran" | sed -n 2p) == "oby-check: jobs_rs_limit's payback: #2131" ]] ||
    wrong "CI-41: a payback line above jobs_rs_limit wasn't printed after its message"
git reset --quiet --hard HEAD~1
raise jobs_rs_limit "$((raise_jobs + 10))" '# payback: R05-02 until oby_shell goes'
raise_passes 'CI-39: a raise alone with a task key as its payback'
git reset --quiet --hard HEAD~1
raise jobs_rs_limit "$((raise_jobs + 10))" '# payback: R05-02' '# the limit'
raise_fails 'CI-39: a raise whose payback line is one line higher' "jobs_rs_limit's payback line"
git reset --quiet --hard HEAD~1
raise jobs_rs_limit "$((raise_jobs + 10))" '# payback: soon'
raise_fails 'CI-39: a raise whose payback line names no issue or task' "jobs_rs_limit's payback line"
git reset --quiet --hard HEAD~1
raise jobs_rs_limit "$((raise_jobs - 10))"
raise_passes 'CI-39: a cut with no payback line'
git reset --quiet --hard HEAD~1
echo '# another rule' >>scripts/oby-check.sh
code README.md 1
git add README.md
commit_scripts 'the limits unchanged'
raise_passes 'CI-39: an unchanged limit, with another file'
git reset --quiet --hard HEAD~1
code README.md 1
git add README.md
raise jobs_rs_limit "$((raise_jobs + 10))" '# payback: #2131'
expect 'raises jobs_rs_limit .* and changes README.md too' 'CI-40: a raise with another file names it'
raise_fails 'CI-40: a raise with another file' "jobs_rs_limit's raise in a pull request of its own"
! grep -q 'failed: .*payback line' <<<"$ran" || wrong 'CI-40: a raise with its payback line was told to add one'
git reset --quiet --hard HEAD~1
for n in 1 2 3 4 5 6; do code "docs/n$n.md" 1; done
git add docs
raise jobs_rs_limit "$((raise_jobs + 10))" '# payback: R05-02'
expect 'changes docs/n1.md, docs/n2.md, docs/n3.md, docs/n4.md, docs/n5.md and 1 more too' 'CI-40: a raise with six other files names five and counts the rest'
git reset --quiet --hard HEAD~1
# Uncommitted: the base's copy installed over the commit's, a tracked file changed, an untracked one.
raise jobs_rs_limit "$((raise_jobs + 10))" '# payback: #2131'
git show HEAD~1:scripts/oby-check.sh >scripts/oby-check.sh
echo '# changed' >>scripts/oby-check-test.sh
code README.md 1
raise_passes 'CI-40: a raise alone next to uncommitted and untracked files'
git reset --quiet --hard HEAD~1
rm README.md
# CI-41: GitHub's `limits` job judges neither rule on combined code, and prints each payback line.
code README.md 1
git add README.md
payback tests_sleep_limit "# payback: R05-02 a sleep's cut"
raise jobs_rs_limit "$((raise_jobs + 10))"
raise_fails 'CI-41: the rust group, by contrast, on a raise with no payback line and another file' "jobs_rs_limit's raise in a pull request of its own"
ran=$(env -u OBY_CHECK_PLAN -u OBY_CHECK_PATHS OBY_CHECK_ONLY=limits scripts/oby-check.sh "$since" 2>&1) ||
    wrong 'CI-41: under OBY_CHECK_ONLY=limits, a raise with no payback line and another file failed'
grep -qx 'oby-check: passed' <<<"$ran" && ! grep -q "payback line\|in a pull request of its own" <<<"$ran" ||
    wrong 'CI-41: under OBY_CHECK_ONLY=limits, the raise rules were judged'
grep -qxF "oby-check: tests_sleep_limit's payback: R05-02 a sleep's cut" <<<"$ran" ||
    wrong "CI-41: under OBY_CHECK_ONLY=limits, a payback line wasn't printed"
git reset --quiet --hard HEAD~2
since=

# AGENTS.md's rules are where the manifest says (docs/contracts/ci.md, CI-42 to 44), on the commit's
# own copy of the script, committed here as the base: a root that links docs/rules.md, the rule there,
# and in core/AGENTS.md one wrapped over two lines and one whose link goes elsewhere.
printf '%s\n' "$committed_script" >scripts/oby-check.sh
agents_limit=$(sed -n 's/^agents_md_bytes_limit=//p' scripts/oby-check.sh)
link='[rules](docs/rules.md)'
{ printf '%s\n' "$link"; head -c "$((${agents_limit:-100} - ${#link} - 1))" /dev/zero | tr '\0' x; } >AGENTS.md
mkdir -p core
printf '# Rules\n\n- One rule, wrapped\n  over two lines.\n- A rule with [a link](../docs/x.md) in it.\n' >core/AGENTS.md
printf '# More rules\n\n- A linked rule.\n' >docs/rules.md
printf '%s\n' 'The rules.' '' '### L1' '- `AGENTS.md`: x' '### L2' '- `core/AGENTS.md`: One rule, wrapped over two lines.' \
    '### L3' '- `core/AGENTS.md`: A rule with [a link](docs/x.md) in it.' '### L4' '- `docs/rules.md`: A linked rule.' \
    >docs/agents-rules-manifest.md
git add AGENTS.md core/AGENTS.md docs
commit_scripts 'the rules'
rules_ok="AGENTS.md's rules: each of the 4 in docs/agents-rules-manifest.md is where it says"
rules_failed="failed: AGENTS.md's rules"
export OBY_CHECK_PATHS=docs/agents-rules-manifest.md
expect "$rules_ok" 'CI-42: every sentence where it says, one wrapped over two lines and one whose link differs'
expect no 'no longer holds' 'CI-42: every sentence where it says, none named missing'
# CI-42: a sentence taken out of its home is named, and fails a real run.
printf '# Rules\n\n- One rule, wrapped\n  over two lines.\n' >core/AGENTS.md
expect "AGENTS.md's rules: core/AGENTS.md no longer holds \"A rule with \[a link\](docs/x.md) in it.\"" 'CI-42: a sentence taken out of its home is named'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then wrong 'CI-42: a run with a sentence out of its home passed'; fi
grep -qF "$rules_failed" <<<"$ran" || wrong 'CI-42: a run with a sentence out of its home failed without naming it'
git checkout --quiet -- core/AGENTS.md
# CI-43: a home the root doesn't link fails naming it; once the root links it, it passes.
printf '# Other rules\n\n- An unlinked rule.\n' >docs/other.md
printf '%s\n' '### L5' '- `docs/other.md`: An unlinked rule.' >>docs/agents-rules-manifest.md
expect "AGENTS.md's rules: docs/other.md, a home in docs/agents-rules-manifest.md, isn't AGENTS.md, core/AGENTS.md or a file the root AGENTS.md links" 'CI-43: a home the root does not link'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then wrong 'CI-43: a run with a home the root does not link passed'; fi
grep -qF "$rules_failed" <<<"$ran" || wrong 'CI-43: a run with a home the root does not link failed without naming it'
printf '%s\n' '[other](./docs/other.md#rules)' >>AGENTS.md
expect "AGENTS.md's rules: each of the 5 in docs/agents-rules-manifest.md is where it says" 'CI-43: the same home passes once the root links it'
git checkout --quiet -- AGENTS.md docs/agents-rules-manifest.md
rm docs/other.md
# CI-43: a commit with no manifest fails, saying so.
git rm --quiet docs/agents-rules-manifest.md
commit_scripts 'no manifest'
expect 'this commit has no docs/agents-rules-manifest.md, which lists AGENTS.md.s rules: put it back' 'CI-43: a commit that deletes the manifest'
if ran=$(env -u OBY_CHECK_PLAN scripts/oby-check.sh HEAD 2>&1); then wrong 'CI-43: a run on a commit with no manifest passed'; fi
grep -qF 'failed: the rules manifest is gone' <<<"$ran" || wrong 'CI-43: a run on a commit with no manifest failed without naming it'
git reset --quiet --hard HEAD~1
unset OBY_CHECK_PATHS
# CI-44: what changed plans the rules: a doc the manifest names does, with no cargo step; one it
# doesn't name doesn't; AGENTS.md alone still plans what CI-38 says.
echo '- Another line.' >>docs/rules.md
said=$(planned)
grep -qx 'oby-check: group rust' <<<"$said" && grep -qF "$rules_ok" <<<"$said" && ! grep -q '^==> cargo' <<<"$said" ||
    { ran=$said; wrong 'CI-44: a change to only a doc the manifest names did not plan the rules alone'; }
git checkout --quiet -- docs/rules.md
printf 'Not a rule.\n' >docs/unnamed.md
expect no "AGENTS.md's rules" "CI-44: a change to only a doc the manifest doesn't name"
rm docs/unnamed.md
sed 's/^x/y/' AGENTS.md >AGENTS.new && cat AGENTS.new >AGENTS.md && rm AGENTS.new
said=$(planned)
grep -qx 'oby-check: group rust' <<<"$said" && grep -q "AGENTS.md has ${agents_limit:-100} bytes" <<<"$said" &&
    grep -qF "$rules_ok" <<<"$said" && ! grep -q '^==> cargo' <<<"$said" ||
    { ran=$said; wrong 'CI-44: a change to AGENTS.md alone did not plan what CI-38 says'; }
git checkout --quiet -- AGENTS.md
git reset --quiet --hard HEAD~1

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
# obid's own code changed too, whose lines are only reported.
code "$main" 1
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
code "$main" 1
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
    apps/electron/src/renderer/src/obid.ts apps/shared/Runner.swift; do
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
# CI-56: what the Brain reads plans its tests, in the electron group; a doc it doesn't read doesn't;
# in a job's box only a change to the Brain itself runs them.
for path in tools/brain/brain.config.ts core/crates/obid/src/jobs/lanes.rs apps/electron/src/renderer/src/Sidebar.tsx \
    apps/shared/Runner.swift docs/api/jobs.md docs/runner-api.md AGENTS.md; do
    said=$(OBY_CHECK_PATHS=$path OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
    grep -q -- '==> npm --prefix tools/brain run --silent test' <<<"$said" && grep -qxF 'oby-check: group electron' <<<"$said" || {
        echo "oby-check-test: CI-56: a change to $path didn't plan the Brain's tests in the electron group:" >&2
        sed 's/^/| /' <<<"$said" >&2
        exit 1
    }
done
said=$(OBY_CHECK_PATHS=docs/decisions/x.md OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
! grep -q 'tools/brain' <<<"$said" || {
    echo "oby-check-test: CI-56: a change to a doc the Brain doesn't read planned its tests:" >&2
    sed 's/^/| /' <<<"$said" >&2
    exit 1
}
if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null; then
    said=$(OBY_CHECK_PATHS=core/crates/obid/src/jobs/lanes.rs nested)
    grep -q "the Brain's map isn't checked in a job's box" <<<"$said" && ! grep -q -- '--prefix tools/brain' <<<"$said" || {
        echo "oby-check-test: CI-56: nested, a change to obid planned the Brain's tests:" >&2
        sed 's/^/| /' <<<"$said" >&2
        exit 1
    }
    said=$(OBY_CHECK_PATHS=tools/brain/brain.config.ts nested)
    grep -q -- '==> npm --prefix tools/brain run --silent test' <<<"$said" || {
        echo "oby-check-test: CI-56: nested, a change to the Brain didn't plan its tests:" >&2
        sed 's/^/| /' <<<"$said" >&2
        exit 1
    }
fi

# obid's test binaries run side by side, each to its end: one that fails doesn't stop the others,
# its failure is named and comes last, each runs quietly, and a binary cargo didn't build fails the
# check. Cargo is a stand-in here: it builds each `--test` it's given, unless OBY_FAKE_BUILDS_NONE,
# and runs the lib's tests as OBY_FAKE_LIB, when set.
fake=$(mktemp -d "${TMPDIR:-/tmp}/oby-check-fake.XXXXXX")
trap 'rm -rf "$repo" "$fake"' EXIT
cat >"$fake/cargo" <<STANDIN
#!/bin/bash
[[ " \$* " != *" --lib "* || -z "\$OBY_FAKE_LIB" ]] || exec "\$OBY_FAKE_LIB" "\$@"
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
# CI-52: each binary's seconds, and a last line with how long they all took and the longest.
for said in 'Running slow ([0-9]*s)' "obid's test binaries took [0-9]*s, the longest tests/slow.rs at [0-9]*s"; do
    grep -q "$said" <<<"$ran" || {
        echo "oby-check-test: the test binaries' run didn't say \"$said\": $ran" >&2
        exit 1
    }
done
# CI-51: run by hand (no OBY_BUILD_JOBS), obid's test binaries run half the Mac's cores at once, at
# least 2, whatever the load: 10 cores run 5, 6 run 3, 2 run 2.
printf '#!/bin/bash\ncase "$*" in *hw.ncpu*) echo "$OBY_FAKE_CORES" ;; *vm.loadavg*) echo "{ 12.50 1.00 1.00 }" ;; *) exit 1 ;; esac\n' >"$fake/sysctl"
chmod +x "$fake/sysctl"
for cores_at_once in '10 5' '6 3' '2 2'; do
    read -r cores at_once <<<"$cores_at_once"
    ran=$(env -u OBY_CHECK_PLAN -u OBY_BUILD_JOBS PATH="$fake:$PATH" TMPDIR="$fake/tmp" OBY_FAKE_CORES="$cores" scripts/oby-check.sh HEAD 2>&1) || true
    grep -q "oby-check: [0-9]* test binaries, $at_once at once with 2 threads each ($cores CPUs, a load average of 12.50)" <<<"$ran" || {
        echo "oby-check-test: on $cores cores the check didn't run $at_once binaries at once: $ran" >&2
        exit 1
    }
done
rm "$fake/sysctl"
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
# CI-33: stopped with SIGTERM to it and every process descended from it (found by parent id, as
# oby-check-blok's node stops a group past its limit), it names the binary still running, exits 143
# and leaves nothing behind.
printf '#!/bin/bash\necho $$ >"%s/hangs.pid"\nexec sleep 300\n' "$fake" >"$fake/hangs-0123abcd"
chmod +x "$fake/hangs-0123abcd"
code core/crates/obid/tests/hangs.rs 1
descendants() { # a process and every one under it, parents first
    local child
    echo "$1"
    for child in $(ps -A -o pid= -o ppid= | awk -v p="$1" '$2 == p { print $1 }'); do descendants "$child"; done
}
gone() { # a pid: it ended within 5 seconds
    local n
    for ((n = 0; n < 50; n++)); do
        kill -0 "$1" 2>/dev/null || return 0
        sleep 0.1
    done
    return 1
}
env -u OBY_CHECK_PLAN PATH="$fake:$PATH" TMPDIR="$fake/tmp" OBY_BUILD_JOBS=2 scripts/oby-check.sh HEAD >"$fake/hangs.said" 2>&1 &
checking=$!
for ((n = 0; n < 300; n++)); do
    [[ -f "$fake/hangs.pid" ]] && compgen -G "$fake/tmp/oby-check.*/hangs-0123abcd.out" >/dev/null && break
    sleep 0.1
done
[[ -f "$fake/hangs.pid" ]] || {
    kill -KILL $(descendants "$checking") 2>/dev/null || true
    echo "oby-check-test: CI-33: the hanging test binary never started: $(cat "$fake/hangs.said")" >&2
    exit 1
}
kill -TERM $(descendants "$checking") 2>/dev/null || true
stopped=0
wait "$checking" || stopped=$?
ran=$(cat "$fake/hangs.said")
[[ $stopped == 143 ]] || {
    echo "oby-check-test: CI-33: a stopped check exited $stopped, not 143: $ran" >&2
    exit 1
}
grep -q 'oby-check: tests/hangs.rs was still running' <<<"$ran" || {
    echo "oby-check-test: CI-33: a stopped check didn't name the binary still running: $ran" >&2
    exit 1
}
gone "$(cat "$fake/hangs.pid")" || {
    kill -KILL "$(cat "$fake/hangs.pid")" 2>/dev/null || true
    echo "oby-check-test: CI-33: the hanging test binary outlived the stopped check" >&2
    exit 1
}
rm core/crates/obid/tests/hangs.rs

# OBY_CHECK_ALL=1 plans everything, whatever changed: every test binary, the lib and bin tests,
# every crate, this test, the iPhone build and the Electron app.
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
    grep -q -- 'build-for-testing' <<<"$said" || {
        echo "oby-check-test: OBY_CHECK_ALL didn't plan \"build-for-testing\": $said" >&2
        exit 1
    }
else
    grep -q "don't run in a job's sandbox" <<<"$said" &&
        ! grep -qE '==> (xcodebuild|swift test)' <<<"$said" || {
        echo "oby-check-test: OBY_CHECK_ALL didn't skip the Apple checks in a job's sandbox: $said" >&2
        exit 1
    }
fi
rm -r core/crates/obi-protocol
rm -r core/crates/obid

# The flaky list (docs/contracts/ci.md, CI-26 to 28), on the commit's own copy of the script (the
# base's copy a job's check installs may predate it). The base lists a test of tests/flaky.rs and one
# of the lib's, and has one task file. Within its limits, so only what a case does can fail its run.
printf '%s\n' "$committed_script" >scripts/oby-check.sh
agents_md_at_limit
code "$main" 1
sleeps "$sleepy" "$(sed -n 's/^tests_sleep_limit=//p' scripts/oby-check.sh)"
listed=core/.config/nextest.toml
entry() { # key, binary id, the filter's test(…)
    printf '\n# %s: what it waits on\n[[profile.ci.overrides]]\nfilter = '"'"'binary_id(%s) & test(%s)'"'"'\nretries = 2\n' "$@"
}
mkdir -p core/.config
{ echo '[profile.ci]'; echo 'retries = 1'; entry R01-70 obid::flaky =listed_test; entry R01-71 obid =codex::tests::listed_test; } >"$listed"
code factory/queue/tasks/R01-80.md 1
commit 'the flaky list'
# A libtest-shaped run: the tests OBY_FAKE_FAILS names fail, on the first run only with OBY_FAKE_ONCE;
# with OBY_FAKE_BARE it fails naming none. Each run's arguments are a line of OBY_FAKE_CALLS.
cat >"$fake/libtest" <<'STANDIN'
#!/bin/bash
echo "$*" >>"$OBY_FAKE_CALLS"
printf '\nrunning 2 tests\n'
[[ -z "$OBY_FAKE_BARE" ]] || { echo 'it crashed'; exit 101; }
if [[ -z "$OBY_FAKE_FAILS" || (-n "$OBY_FAKE_ONCE" && $(wc -l <"$OBY_FAKE_CALLS") -gt 1) ]]; then
    printf '\ntest result: ok. 2 passed; 0 failed; 0 ignored\n'
    exit 0
fi
printf '\nfailures:\n\n'
for test in $OBY_FAKE_FAILS; do printf -- '---- %s stdout ----\nit failed\n\n' "$test"; done
printf '\nfailures:\n'
for test in $OBY_FAKE_FAILS; do printf '    %s\n' "$test"; done
printf '\ntest result: FAILED. 1 passed; %s failed; 0 ignored\n' "$(wc -w <<<"$OBY_FAKE_FAILS" | tr -d ' ')"
exit 101
STANDIN
chmod +x "$fake/libtest"
cp "$fake/libtest" "$fake/flaky-0123abcd"
calls="$fake/calls"
flaky_run() { rm -f "$calls"; run OBY_CHECK_ONLY=rust OBY_FAKE_CALLS="$calls" "$@"; }
runs() { wc -l <"$calls" | tr -d ' '; }
code core/crates/obid/tests/flaky.rs 3
ran=$(flaky_run OBY_FAKE_FAILS=listed_test OBY_FAKE_ONCE=1) || wrong 'CI-26: a listed test that passed its retry failed the check'
grep -qxF 'retried flaky: obid::flaky listed_test' <<<"$ran" && [[ $(runs) == 2 ]] &&
    [[ $(sed -n 2p "$calls") == '--quiet --exact listed_test' ]] && ! grep -q 'tests/flaky.rs failed' <<<"$ran" ||
    wrong "CI-26: a listed test that failed once wasn't retried alone, saying so (its runs: $(cat "$calls"))"
if ran=$(flaky_run OBY_FAKE_FAILS=listed_test); then wrong 'CI-26: a listed test that failed its retry too passed the check'; fi
[[ $(runs) == 2 ]] && grep -q 'tests/flaky.rs failed' <<<"$ran" && ! grep -q 'retried flaky' <<<"$ran" ||
    wrong 'CI-26: a listed test that failed its retry too was not a failed binary'
if ran=$(flaky_run OBY_FAKE_FAILS=other_test OBY_FAKE_ONCE=1); then wrong 'CI-27: an unlisted test that failed passed the check'; fi
[[ $(runs) == 1 ]] && grep -q 'failed: test_binaries' <<<"$ran" && ! grep -q 'retried flaky' <<<"$ran" ||
    wrong 'CI-27: an unlisted test that failed was retried'
if ran=$(flaky_run OBY_FAKE_FAILS='listed_test other_test' OBY_FAKE_ONCE=1); then
    wrong 'CI-27: a listed and an unlisted test that failed passed the check'
fi
[[ $(runs) == 1 ]] || wrong 'CI-27: a listed and an unlisted test that failed in one binary were retried'
if ran=$(flaky_run OBY_FAKE_BARE=1 OBY_FAKE_ONCE=1); then wrong 'CI-27: a failure that named no test passed the check'; fi
[[ $(runs) == 1 ]] || wrong 'CI-27: a failure with no summary was retried'
# Listed in the checked tree only, with its task: the change can't pass its own gate with it.
entry R01-81 obid::flaky =new_test >>"$listed"
code factory/queue/tasks/R01-81.md 1
if ran=$(flaky_run OBY_FAKE_FAILS=new_test OBY_FAKE_ONCE=1); then wrong 'CI-27: a test listed only in the checked tree passed the check'; fi
[[ $(runs) == 1 ]] && grep -q 'failed: test_binaries' <<<"$ran" ||
    wrong 'CI-27: a test listed only in the checked tree, not at the base, was retried'
git checkout --quiet -- "$listed"
rm factory/queue/tasks/R01-81.md core/crates/obid/tests/flaky.rs
# The lib's tests: through cargo, with every target run to its end.
code core/crates/obid/src/codex.rs 1
ran=$(flaky_run OBY_FAKE_LIB="$fake/libtest" OBY_FAKE_FAILS=codex::tests::listed_test OBY_FAKE_ONCE=1) ||
    wrong 'CI-26: a listed lib test that passed its retry failed the check'
grep -qxF 'retried flaky: obid codex::tests::listed_test' <<<"$ran" && [[ $(runs) == 2 ]] &&
    [[ $(sed -n 1p "$calls") == *' --no-fail-fast -p obid --lib -- codex::' ]] &&
    [[ $(sed -n 2p "$calls") == *' --no-fail-fast -p obid --lib -- --exact codex::tests::listed_test' ]] ||
    wrong "CI-26: a listed lib test that failed once wasn't retried alone, saying so (its runs: $(cat "$calls"))"
if ran=$(flaky_run OBY_FAKE_LIB="$fake/libtest" OBY_FAKE_FAILS=codex::tests::other_test OBY_FAKE_ONCE=1); then
    wrong 'CI-27: an unlisted lib test that failed passed the check'
fi
[[ $(runs) == 1 ]] && ! grep -q 'retried flaky' <<<"$ran" || wrong 'CI-27: an unlisted lib test that failed was retried'
rm core/crates/obid/src/codex.rs
# CI-28: the list only shrinks. A new entry passes only with the task file the same change adds.
guard() { # what it's about, then `passes` or the words its failure says
    if ran=$(run OBY_CHECK_ONLY=rust); then
        [[ "$2" == passes ]] && grep -q '^oby-check: the flaky list has ' <<<"$ran" || wrong "$1 passed the check"
    else
        [[ "$2" != passes ]] && grep -q 'failed: the flaky list' <<<"$ran" && grep -qF "$2" <<<"$ran" ||
            wrong "$1 didn't fail naming the flaky list and \"$2\""
    fi
}
entry R01-90 obid::merge_api =brand_new >>"$listed"
guard 'CI-28: a new entry with no task file' 'new entry obid::merge_api brand_new names R01-90'
code factory/queue/tasks/R01-90.md 1
guard 'CI-28: a new entry whose task file the same change adds' passes
git checkout --quiet -- "$listed"
entry R01-80 obid::merge_api =brand_new >>"$listed"
guard 'CI-28: a new entry naming a task file the base already had' 'new entry obid::merge_api brand_new names R01-80'
git checkout --quiet -- "$listed"
entry R01-90 obid::merge_api merging_39_ >>"$listed"
guard 'CI-28: a prefix filter' "isn't a flaky list entry"
{ echo '[profile.ci]'; echo 'retries = 1'; entry R01-70 obid::flaky =listed_test; } >"$listed"
guard 'CI-28: a removed entry' passes
git checkout --quiet -- "$listed"
rm factory/queue/tasks/R01-90.md "$fake/flaky-0123abcd"

# Each filtered plan contains only its group, including side effects outside step.
# Use the candidate's implementation: the base's script may predate OBY_CHECK_ONLY.
printf '%s\n' "$committed_script" >scripts/oby-check.sh
agents_md_at_limit
for group in rust electron swift factory scan; do
    said=$(OBY_CHECK_ONLY="$group" OBY_CHECK_ALL=1 OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
    case "$group" in
        rust) wanted='cargo'; unwanted='apps/electron run\|==> gitleaks\|==> swift\|==> xcodebuild\|unittest' ;;
        electron) wanted='apps/electron run'; unwanted='==> cargo\|==> gitleaks\|Rust core has\|==> swift\|==> xcodebuild\|unittest' ;;
        swift) wanted="build-for-testing\|Apple apps don't run in a job's sandbox"; unwanted='==> cargo\|==> gitleaks\|Rust core has\|apps/electron run\|unittest' ;;
        factory) wanted='python3 -m unittest discover -s factory/tests'; unwanted='==> cargo\|==> gitleaks\|Rust core has\|apps/electron run\|==> swift\|==> xcodebuild' ;;
        scan) wanted='gitleaks'; unwanted='==> cargo\|Rust core has\|apps/electron run\|==> swift\|==> xcodebuild\|unittest' ;;
    esac
    grep -q "$wanted" <<<"$said" && ! grep -q "$unwanted" <<<"$said" || {
        echo "oby-check-test: incorrect $group filter: $said" >&2
        exit 1
    }
done

# The machine-readable plan names each selected group once, independently of messages.
said=$(OBY_CHECK_ALL=1 OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
for group in rust electron swift factory scan; do
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
! grep -qE '^oby-check: group (rust|electron|swift|factory)$' <<<"$said" || {
    echo "oby-check-test: unchanged groups were planned: $said" >&2; exit 1;
}
# The factory's own scripts plan its tests, and no Rust group (factory/checks.sh runs that plan
# outside the Cargo lock).
for path in factory/bin/test-run factory/checks.sh factory/tests/test_x.py factory/config.json; do
    said=$(OBY_CHECK_PATHS=$path OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
    grep -qx 'oby-check: group factory' <<<"$said" &&
        grep -q '^==> .*python3 -m unittest discover -s factory/tests$' <<<"$said" || {
        echo "oby-check-test: $path didn't plan the factory's tests: $said" >&2; exit 1;
    }
done
said=$(OBY_CHECK_PATHS=factory/bin/test-run OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)
! grep -qx 'oby-check: group rust' <<<"$said" || {
    echo "oby-check-test: a change to factory/bin/test-run planned the Rust group: $said" >&2; exit 1;
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

# The gate's shards (docs/contracts/remote-gate.md, REMOTE-58 to 60), on the commit's own copy of the
# script (the base's copy a job's check installs may predate OBY_CHECK_SHARD), committed here as the
# base within its limits. Seven test files, ranked by lines: a 7, b 6, c 5, d 4, e 3 and f 3 (a tie,
# by path), g 1.
printf '%s\n' "$committed_script" >scripts/oby-check.sh
agents_md_at_limit
for file in a:7 b:6 c:5 d:4 e:3 f:3 g:1; do code "core/crates/obid/tests/${file%:*}.rs" "${file#*:}"; done
commit 'the shards'
shard_paths=$(printf 'core/crates/obid/tests/%s.rs\n' a b c d e f g)
shard_plan() { # shard, then OBY_CHECK_PATHS: the rust group's plan, the shard unset when empty
    env OBY_CHECK_PATHS="${2:-$shard_paths}" OBY_CHECK_ONLY=rust ${1:+OBY_CHECK_SHARD=$1} OBY_CHECK_PLAN=1 \
        scripts/oby-check.sh HEAD 2>&1
}
binaries_in() { # a plan: the --test names of its test_binaries step, one a line, sorted
    sed -n 's/^==> test_binaries //p' <<<"$1" | tr ' ' '\n' | grep -v '^--test$' | LC_ALL=C sort || true
}
unsharded=$(binaries_in "$(shard_plan '')")
[[ "$unsharded" == "$(printf '%s\n' a b c d e f g)" ]] || { ran=$unsharded; wrong 'REMOTE-58: the unsharded plan did not plan the seven binaries'; }
# Dealt round-robin over the ranking: n = 2 and n = 3, each binary in exactly one shard, the shards
# together the unsharded set.
for want in '2 a,c,e,g b,d,f' '3 a,d,g b,e c,f'; do
    n=${want%% *}
    dealt=''
    k=0
    for members in ${want#* }; do
        k=$((k + 1))
        said=$(shard_plan "$k/$n")
        got=$(binaries_in "$said")
        [[ "$got" == "$(tr ',' '\n' <<<"$members")" ]] || { ran=$said; wrong "REMOTE-58: shard $k/$n planned $(tr '\n' ' ' <<<"$got")not $members"; }
        dealt+="$got"$'\n'
        # Only shard 1 plans fmt, clippy, the lib tests and the limits.
        if ((k == 1)); then
            grep -q '^==> cargo --quiet fmt ' <<<"$said" && grep -q '^==> cargo --quiet clippy ' <<<"$said" &&
                grep -q 'production lines outside the Sphere' <<<"$said" || { ran=$said; wrong "REMOTE-58: shard 1/$n did not plan fmt, clippy and the limits"; }
        else
            ! grep -q '^==> cargo \|Sphere\|AGENTS.md has' <<<"$said" || { ran=$said; wrong "REMOTE-58: shard $k/$n planned fmt, clippy or the limits"; }
        fi
    done
    [[ "$(LC_ALL=C sort <<<"${dealt%$'\n'}")" == "$unsharded" ]] || { ran=$dealt; wrong "REMOTE-58: the $n shards together are not the unsharded set, each binary once"; }
done
# The lib tests are shard 1's.
lib_paths=$(printf '%s\ncore/crates/obid/src/facts.rs\n' "$shard_paths")
grep -q -- '--lib -- facts::$' <<<"$(shard_plan 1/2 "$lib_paths")" || { ran=$(shard_plan 1/2 "$lib_paths"); wrong 'REMOTE-58: shard 1/2 did not plan the lib tests'; }
! grep -q -- '--lib' <<<"$(shard_plan 2/2 "$lib_paths")" || { ran=$(shard_plan 2/2 "$lib_paths"); wrong 'REMOTE-58: shard 2/2 planned the lib tests'; }
# A binary's shard is the same whatever else is planned.
[[ "$(binaries_in "$(shard_plan 2/2 "$(printf 'core/crates/obid/tests/%s.rs\n' b f)")")" == "$(printf '%s\n' b f)" ]] &&
    [[ -z "$(binaries_in "$(shard_plan 1/2 "$(printf 'core/crates/obid/tests/%s.rs\n' b f)")")" ]] ||
    wrong 'REMOTE-58: a smaller planned subset moved a binary to another shard'
# 1/1 plans exactly what the unset variable plans, the whole check's groups too.
[[ "$(OBY_CHECK_PATHS=$lib_paths OBY_CHECK_SHARD=1/1 OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)" == \
    "$(OBY_CHECK_PATHS=$lib_paths OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD 2>&1)" ]] ||
    wrong 'REMOTE-58: OBY_CHECK_SHARD=1/1 planned other than the unset variable'
# Shard 2/2 with none of its binaries planned has no rust group, and its run passes.
said=$(shard_plan 2/2 core/crates/obid/tests/a.rs)
! grep -qx 'oby-check: group rust' <<<"$said" || { ran=$said; wrong 'REMOTE-58: a shard with nothing to run planned the rust group'; }
ran=$(run OBY_CHECK_ONLY=rust OBY_CHECK_SHARD=2/2 OBY_CHECK_PATHS=core/crates/obid/tests/a.rs) && grep -qx 'oby-check: passed' <<<"$ran" ||
    wrong 'REMOTE-58: a shard with nothing to run did not pass'
# In a job's box, a binary that can't run there is left unjudged only in its own shard: sandbox.rs, 5
# lines, ties c.rs and ranks fourth of eight, so it is shard 2/2's.
code core/crates/obid/tests/sandbox.rs 5
judge="couldn't judge: tests/sandbox.rs makes a sandbox of its own and can't run in a job's; run the check on the Mac outside a job"
said=$(OBY_CHECK_PATHS=core/crates/obid/tests/sandbox.rs OBY_CHECK_ONLY=rust OBY_CHECK_SHARD=2/2 nested)
grep -qxF "$judge" <<<"$said" && grep -qx 'oby-check: group rust' <<<"$said" || { ran=$said; wrong 'REMOTE-58: nested, the shard holding tests/sandbox.rs did not leave it unjudged'; }
said=$(OBY_CHECK_PATHS=core/crates/obid/tests/sandbox.rs OBY_CHECK_ONLY=rust OBY_CHECK_SHARD=1/2 nested)
! grep -q 'sandbox' <<<"$said" || { ran=$said; wrong 'REMOTE-58: nested, a shard without tests/sandbox.rs counted it'; }
rm core/crates/obid/tests/sandbox.rs

# REMOTE-59: a failing binary turns its own shard red, and only its own. Each binary is a stand-in that
# fails when OBY_FAKE_RED names it.
cat >"$fake/red" <<'STANDIN'
#!/bin/bash
name=${0##*/}
name=${name%-*}
if [[ "$OBY_FAKE_RED" == "$name" ]]; then
    printf 'test result: FAILED. 0 passed; 1 failed\n'
    exit 101
fi
printf 'test result: ok. 1 passed\n'
STANDIN
chmod +x "$fake/red"
for name in a b c d e f g; do cp "$fake/red" "$fake/$name-0123abcd"; done
shard_run() { # shard, then settings: the rust group's run of the seven binaries
    local shard=$1
    shift
    run OBY_CHECK_ONLY=rust OBY_CHECK_SHARD="$shard" OBY_CHECK_PATHS="$shard_paths" "$@"
}
for want in '2 a,c,e,g b,d,f' '3 a,d,g b,e c,f'; do
    n=${want%% *}
    k=0
    for members in ${want#* }; do
        k=$((k + 1))
        for red in ${members//,/ }; do
            ran=$(shard_run "$k/$n" OBY_FAKE_RED="$red") && wrong "REMOTE-59: shard $k/$n passed with tests/$red.rs failing"
            grep -qxF "oby-check: tests/$red.rs failed" <<<"$ran" || wrong "REMOTE-59: shard $k/$n did not name tests/$red.rs"
            for ((other = 1; other <= n; other++)); do
                ((other == k)) && continue
                ran=$(shard_run "$other/$n" OBY_FAKE_RED="$red") || wrong "REMOTE-59: shard $other/$n failed with tests/$red.rs, shard $k/$n's, failing"
            done
        done
        ran=$(shard_run "$k/$n") && grep -qx 'oby-check: passed' <<<"$ran" || wrong "REMOTE-59: shard $k/$n failed with no binary failing"
    done
done
# A listed flaky test (the flaky list's obid::flaky listed_test) that passes its retry passes in its
# shard: flaky.rs, 8 lines, ranks first, so it is shard 1/2's.
cp "$fake/libtest" "$fake/flaky-0123abcd"
code core/crates/obid/tests/flaky.rs 8
rm -f "$calls"
ran=$(run OBY_CHECK_ONLY=rust OBY_CHECK_SHARD=1/2 OBY_CHECK_PATHS=core/crates/obid/tests/flaky.rs OBY_FAKE_CALLS="$calls" \
    OBY_FAKE_FAILS=listed_test OBY_FAKE_ONCE=1) || wrong 'REMOTE-59: a listed flaky test that passed its retry failed its shard'
grep -qxF 'retried flaky: obid::flaky listed_test' <<<"$ran" && [[ $(runs) == 2 ]] ||
    wrong 'REMOTE-59: a listed flaky test that failed once was not retried in its shard'
rm -f "$calls"
ran=$(run OBY_CHECK_ONLY=rust OBY_CHECK_SHARD=2/2 OBY_CHECK_PATHS=core/crates/obid/tests/flaky.rs OBY_FAKE_CALLS="$calls" \
    OBY_FAKE_FAILS=listed_test) && [[ ! -f "$calls" ]] || wrong 'REMOTE-59: the shard without tests/flaky.rs ran it'
rm core/crates/obid/tests/flaky.rs "$fake/flaky-0123abcd"

# REMOTE-60: an invalid shard ends 2 with nothing run, before oby-check-blok's hand-off too.
for shard in 0/2 3/2 1/65 01/2 a/b 1 1/ /2 '1/2 ' 2/1; do
    for how in inner handed; do
        if [[ $how == inner ]]; then
            said=$(OBY_CHECK_SHARD=$shard OBY_CHECK_ONLY=rust scripts/oby-check.sh HEAD 2>&1) && status=0 || status=$?
        else
            said=$(handed PATH="$blok/bin:$bare" OBY_CHECK_SHARD="$shard" ./oby-check.sh HEAD) && status=0 || status=$?
        fi
        [[ $status == 2 && "$said" == 'oby-check: invalid OBY_CHECK_SHARD' ]] ||
            { ran=$said; wrong "REMOTE-60: OBY_CHECK_SHARD=$shard ($how) exited $status"; }
    done
done
for shard in 1/1 64/64 1/64; do
    OBY_CHECK_SHARD=$shard OBY_CHECK_PLAN=1 scripts/oby-check.sh HEAD >/dev/null 2>&1 || wrong "REMOTE-60: OBY_CHECK_SHARD=$shard was refused"
done
git rm --quiet core/crates/obid/tests/[a-g].rs
for name in a b c d e f g; do rm "$fake/$name-0123abcd"; done
commit 'no shards'

# CI-35: OBY_CHECK_ONLY=limits, GitHub's `limits` job, judges the limits alone on the commit, whatever
# changed since its base: committed here, measured against HEAD, the diff is empty. On the commit's own
# copy of the script (the base's copy a job's check installs may predate the mode).
printf '%s\n' "$committed_script" >scripts/oby-check.sh
agents_md_at_limit
jobs_limit=$(sed -n 's/^jobs_rs_limit=//p' scripts/oby-check.sh)
sleep_limit=$(sed -n 's/^tests_sleep_limit=//p' scripts/oby-check.sh)
limits_commit() { # message: what the cases wrote, committed
    git add scripts core AGENTS.md >/dev/null && git -c user.name=check -c user.email=check@example.invalid -c commit.gpgsign=false commit --quiet --allow-empty -m "$1"
}
limits() { # base, then settings: the mode's run, outside oby-check-blok and with none on PATH
    local base=$1
    shift
    env -u OBY_CHECK_PLAN -u OBY_CHECK_PATHS -u OBY_CHECK_INNER PATH="$bare" OBY_CHECK_ONLY=limits "$@" \
        scripts/oby-check.sh "$base" 2>&1
}
limits_fails() { # what it's about, the failure it names, then limits' arguments; its scenario, CI-35 unless set
    local about=$1 named=$2 at=${scenario:-CI-35}
    shift 2
    if ran=$(limits "$@"); then wrong "$at: $about passed"; fi
    grep -qF "oby-check: failed: $named" <<<"$ran" || wrong "$at: $about didn't fail naming \"$named\""
    ! grep -q "^==> \|oby-check-blok can't judge this" <<<"$ran" || wrong "$at: $about ran a step or asked for oby-check-blok"
}
limits_passes() { # what it's about, then limits' arguments; its scenario, CI-35 unless set
    local about=$1 at=${scenario:-CI-35}
    shift
    ran=$(limits "$@") || wrong "$at: $about failed"
    grep -qx 'oby-check: passed' <<<"$ran" && ! grep -q '^==> ' <<<"$ran" || wrong "$at: $about didn't pass on the limits alone"
}
# The Rust core past the line limit it once had (102,450) passes, its lines reported (CI-49).
code "$main" 102451
sleeps "$sleepy" "$sleep_limit"
limits_commit 'past the old line limit'
[[ -z "$(git status --porcelain -- core scripts AGENTS.md)" ]] || wrong 'CI-35: the commit left something uncommitted'
past_report='oby-check: the Rust core has 102451 production lines outside the Sphere, +0 since its base'
limits_passes 'the Rust core past its old line limit, nothing changed since the base' HEAD
grep -qxF "$past_report" <<<"$ran" || wrong "CI-35: the Rust core past its old line limit wasn't reported"
# The rust group, by contrast, judges nothing here: nothing changed since the base.
ran=$(env -u OBY_CHECK_PLAN OBY_CHECK_ONLY=rust scripts/oby-check.sh HEAD 2>&1) && ! grep -q 'Rust core has' <<<"$ran" ||
    wrong 'CI-35: the rust group judged the limits with nothing changed: the case proves nothing'
# Under oby-check-blok (OBY_CHECK_INNER) or not, the same: the mode never needs the tool.
limits_passes 'the Rust core past its old line limit, under oby-check-blok' HEAD OBY_CHECK_INNER=1
grep -qxF "$past_report" <<<"$ran" || wrong "CI-35: the Rust core past its old line limit, under oby-check-blok, wasn't reported"
code "$jobs" "$((jobs_limit + 1))"
limits_commit 'jobs.rs past'
limits_fails "one line past jobs.rs's limit" "jobs.rs's line limit" HEAD
rm "$jobs"
sleeps "$sleepy" "$((sleep_limit + 1))"
limits_commit 'one sleep past'
limits_fails "one sleep past the tests' limit" "obid's tests' sleep limit" HEAD
sleeps "$sleepy" "$sleep_limit"
agents_limit=$(sed -n 's/^agents_md_bytes_limit=//p' scripts/oby-check.sh)
agents_md "$((${agents_limit:-1} + 1))"
limits_commit 'one byte past'
scenario=CI-38 limits_fails 'AGENTS.md one byte past its limit, nothing changed since the base' "AGENTS.md's byte limit" HEAD
agents_md "${agents_limit:-1}"
limits_commit 'back within'
limits_passes 'back within every limit' HEAD
# A commit that lowered jobs_rs_limit since an earlier base, run by that base's copy with its higher
# limit: from that base, the commit is held to its own lower limit; from itself, it isn't.
set_limit jobs_rs_limit "$((jobs_limit - 100))"
code "$jobs" "$((jobs_limit - 50))"
limits_commit 'a cut that lowers the limit'
git show HEAD~1:scripts/oby-check.sh >scripts/oby-check.sh
limits_fails "a commit's lower limit, from an earlier base" "jobs.rs's line limit" HEAD~1
grep -q "jobs.rs has $((jobs_limit - 50)) production lines, past its limit of $((jobs_limit - 100))" <<<"$ran" ||
    wrong "CI-35: from an earlier base, the commit's lower limit didn't hold"
limits_passes "the running copy's limit, from the commit itself" HEAD
git checkout --quiet -- scripts/oby-check.sh
# An unknown value is still refused.
said=$(env -u OBY_CHECK_PLAN OBY_CHECK_ONLY=nolimits scripts/oby-check.sh HEAD 2>&1) && status=0 || status=$?
[[ $status == 2 ]] && grep -qx 'oby-check: invalid OBY_CHECK_ONLY' <<<"$said" ||
    { ran=$said; wrong "CI-35: an unknown OBY_CHECK_ONLY exited $status"; }

# CI-49: the limits report the Rust core's production lines and the change's difference from its
# base, and never fail on them, on the commit's own copy of the script, committed here as the base as
# CI-35's are: a commit from 102,000 lines past the line limit it once had, then one that cuts 2,000.
printf '%s\n' "$committed_script" >scripts/oby-check.sh
agents_md_at_limit
rm "$jobs"
code "$main" 102000
limits_commit 'the Rust core at 102,000 lines'
code "$main" 102451
limits_commit 'past the old line limit'
scenario=CI-49 limits_passes 'a commit taking the Rust core from 102,000 to 102,451 lines, from its parent' HEAD~1
grep -qxF 'oby-check: the Rust core has 102451 production lines outside the Sphere, +451 since its base' <<<"$ran" ||
    wrong "CI-49: a commit taking the Rust core past 102,450 lines didn't report its count and difference"
git reset --quiet --hard HEAD~1
code "$main" 100000
limits_commit 'a 2,000-line cut'
scenario=CI-49 limits_passes 'a commit cutting the Rust core from 102,000 to 100,000 lines, from its parent' HEAD~1
grep -qF -- '-2000 since its base' <<<"$ran" && ! grep -q lower <<<"$ran" ||
    wrong "CI-49: a 2,000-line cut didn't report its difference, or was told to lower something"
scenario=CI-49 limits_passes 'the same commit, from itself' HEAD
grep -qxF 'oby-check: the Rust core has 100000 production lines outside the Sphere, +0 since its base' <<<"$ran" ||
    wrong "CI-49: from the commit itself, the difference wasn't +0"
# The rust group's plan reports a change to obid's code the same way, never as past a limit.
code "$main" 102451
expect 'oby-check: the Rust core has 102451 production lines outside the Sphere, +2451 since its base' \
    "CI-49: the rust group's plan didn't report the Rust core's lines"
expect no 'the Rust core has .* past its limit' "CI-49: the rust group's plan held the Rust core to a limit"
git checkout --quiet -- "$main"

# CI-50: a base whose core/crates git can't read, none in its tree or an object that's gone (as in a
# partial clone): the count, its difference unknown, no error of git's or another tool's, and never a
# failure.
unknown="oby-check: the Rust core has 100000 production lines outside the Sphere, the difference from its base unknown (its core/crates can't be read)"
git rm -r --quiet core/crates
limits_commit 'no core/crates'
git checkout --quiet HEAD~1 -- core/crates
limits_commit 'core/crates back'
scenario=CI-50 limits_passes 'a base with no core/crates' HEAD~1
grep -qxF "$unknown" <<<"$ran" && ! grep -q 'fatal:' <<<"$ran" && ! grep -qv '^oby-check: ' <<<"$ran" ||
    wrong "CI-50: a base with no core/crates didn't say its difference is unknown, or said git's error or another tool's"
echo '// only the base holds this' >core/crates/obid/src/gone.rs
limits_commit 'a file only the base holds'
rm core/crates/obid/src/gone.rs
limits_commit 'without it'
gone=$(git rev-parse HEAD~1:core/crates/obid/src/gone.rs)
rm ".git/objects/${gone:0:2}/${gone:2}"
scenario=CI-50 limits_passes "a base whose core/crates holds an object git can't read" HEAD~1
grep -qxF "$unknown" <<<"$ran" && ! grep -q 'fatal:' <<<"$ran" && ! grep -qv '^oby-check: ' <<<"$ran" ||
    wrong "CI-50: a base with an object git can't read didn't say its difference is unknown, or said git's error or another tool's"

echo 'oby-check-test: passed'
