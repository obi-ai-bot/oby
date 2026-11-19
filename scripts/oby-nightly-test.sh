#!/bin/bash
# scripts/oby-nightly.sh's own tests (docs/contracts/ci.md, CI-53 to 55), on a throwaway repository
# whose origin's dev has a stand-in scripts/oby-check.sh, and a stand-in npm. Nothing real is checked.
set -eo pipefail
nightly="$(cd "$(dirname "$0")" && pwd)/oby-nightly.sh"
work=$(cd "$(mktemp -d "${TMPDIR:-/tmp}/oby-nightly-test.XXXXXX")" && pwd)
trap 'rm -rf "$work"' EXIT
fail() { echo "oby-nightly-test: $*" >&2; exit 1; }

mkdir -p "$work/bin"
printf '#!/bin/bash\necho "npm $*"\n' >"$work/bin/npm"
chmod +x "$work/bin/npm"
git init --quiet --bare "$work/origin.git"
git init --quiet "$work/repo"
cd "$work/repo"
git config user.email t@example.com
git config user.name t
mkdir scripts
# The stand-in check says what it was given, then does what CHECK_DOES says (pass, fail or hang).
cat >scripts/oby-check.sh <<'STANDIN'
#!/bin/bash
echo "check ran in $PWD with OBY_CHECK_ALL=$OBY_CHECK_ALL CARGO_TARGET_DIR=$CARGO_TARGET_DIR base $1"
case "$(cat "$CHECK_DOES" 2>/dev/null)" in
fail) echo "oby-check: failed: rust-affected-tests"; exit 1 ;;
hang) echo $$ >"$CHECK_DOES.pid"; exec sleep 300 ;;
esac
echo "oby-check: passed"
STANDIN
chmod +x scripts/oby-check.sh
git add -A
git commit --quiet -m one
git branch -M dev
git remote add origin "$work/origin.git"
git push --quiet origin dev
commit=$(git rev-parse HEAD)
git switch --quiet -c elsewhere
results="$work/results"
export OBY_NIGHTLY_REPO="$work/repo" OBY_NIGHTLY_DIR="$results" OBY_NIGHTLY_QUIET=1 CHECK_DOES="$work/does" PATH="$work/bin:$PATH"
run() { "$nightly" "$@" 2>&1; }
md() { find "$results" -maxdepth 1 -name '????-??-??T????Z.md' | sort; }

# CI-53: with no result, a run checks origin/dev as merged with every suite, writes its result and
# keeps its worktree on that commit; the checkout it ran from is untouched.
said=$(run) || fail "a passing run didn't exit 0: $said"
grep -q "^oby-nightly: PASS (${commit:0:7}, exit 0 in [0-9]*s): $results/.*\.md$" <<<"$said" || fail "a pass wasn't said: $said"
[[ $(md | wc -l) -eq 1 ]] || fail "a run didn't write one result: $(md)"
result=$(md)
grep -qx 'RESULT: PASS' "$result" && grep -qx "COMMIT: $commit" "$result" && grep -q '^SECONDS: [0-9]*$' "$result" &&
    grep -q '^LOAD: .* as it started, .* as it ended$' "$result" || fail "the result lacks a line: $(cat "$result")"
grep -q "check ran in $results/worktree with OBY_CHECK_ALL=1 CARGO_TARGET_DIR=$results/target base $commit" "$result" ||
    fail "the check didn't run on origin/dev with every suite in its own worktree and target: $(cat "$result")"
grep -q 'npm --prefix apps/electron ci' "$result" || fail "the Electron app's modules weren't installed: $(cat "$result")"
[[ "$(readlink "$results/latest.md")" == "${result##*/}" ]] || fail "latest.md doesn't link the newest result"
[[ ! -e "$results/.lock" ]] || fail "a run left its lock"
[[ "$(git -C "$results/worktree" rev-parse HEAD)" == "$commit" ]] || fail "a run didn't keep its worktree on its commit"
# The next night moves the kept worktree to dev's new commit and cleans it.
echo left >"$results/worktree/left-by-the-check"
git switch --quiet dev
echo two >two
git add two
git commit --quiet -m two
git push --quiet origin dev
commit=$(git rev-parse HEAD)
git switch --quiet elsewhere
said=$(run --now) || fail "a second run didn't pass: $said"
[[ "$(git -C "$results/worktree" rev-parse HEAD)" == "$commit" && -f "$results/worktree/two" ]] || fail "the kept worktree didn't move to dev's new commit"
[[ ! -e "$results/worktree/left-by-the-check" ]] || fail "the kept worktree wasn't cleaned"
rm -f "$results"/*.md
said=$(run) || fail "a run with no result didn't pass: $said"
[[ "$(git branch --show-current)" == elsewhere && -z "$(git status --porcelain)" ]] || fail "the checkout it ran from changed"

# CI-54: a result under 20 hours old makes it not due; --now runs anyway; one 21 hours old is due.
said=$(run) || fail "not due didn't exit 0: $said"
grep -q '^oby-nightly: not due (the newest result, .*, is under 20 hours old)$' <<<"$said" || fail "a fresh result didn't make it not due: $said"
echo fail >"$work/does"
if said=$(run --now); then fail "a failing run exited 0: $said"; else code=$?; fi
((code == 1)) || fail "a failing run exited $code, not 1: $said"
grep -q '^oby-nightly: FAIL' <<<"$said" || fail "a fail wasn't said: $said"
failed=$(readlink "$results/latest.md")
grep -qx 'RESULT: FAIL' "$results/$failed" && grep -q 'oby-check: failed: rust-affected-tests' "$results/$failed" ||
    fail "a failure's result lacks its verdict or its log's tail: $(cat "$results/$failed")"
rm -f "$results"/*.md
old=$(date -u -v-21H +%Y-%m-%dT%H%MZ 2>/dev/null || date -u -d '21 hours ago' +%Y-%m-%dT%H%MZ)
echo 'RESULT: PASS' >"$results/$old.md"
rm -f "$work/does"
said=$(run) || fail "a run due after 21 hours didn't pass: $said"
grep -q '^oby-nightly: PASS' <<<"$said" || fail "a result 21 hours old didn't make it due: $said"

# CI-55: a lock a live run holds makes it not due; one whose run is gone is taken over.
rm -f "$results"/*.md
mkdir "$results/.lock"
sleep 300 &
holder=$!
echo "$holder" >"$results/.lock/pid"
said=$(run) || fail "a held lock didn't exit 0: $said"
grep -q '^oby-nightly: not due (another run holds ' <<<"$said" || fail "a held lock didn't make it not due: $said"
kill "$holder"
wait "$holder" 2>/dev/null || true
said=$(run) || fail "a stale lock wasn't taken over: $said"
grep -q '^oby-nightly: PASS' <<<"$said" || fail "a stale lock wasn't taken over: $said"

# CI-55: a stopped run writes no result and leaves no lock, so it's due again.
rm -f "$results"/*.md
echo hang >"$work/does"
"$nightly" >"$work/stopped.said" 2>&1 &
running=$!
for ((n = 0; n < 100; n++)); do [[ -f "$work/does.pid" ]] && break; sleep 0.1; done
[[ -f "$work/does.pid" ]] || fail "the hanging check never started: $(cat "$work/stopped.said")"
kill -TERM "$running" "$(cat "$work/does.pid")"
wait "$running" 2>/dev/null || true
[[ -z "$(md)" ]] || fail "a stopped run wrote a result: $(md)"
[[ ! -e "$results/.lock" ]] || fail "a stopped run left its lock"

# CI-53: when origin/dev can't be checked out, it exits 2, says why, and writes no result.
rm -f "$work/does"
if said=$(OBY_NIGHTLY_BASE=nowhere run); then fail "a run with no branch to check out passed: $said"; else code=$?; fi
((code == 2)) || fail "a run with no branch to check out exited $code, not 2: $said"
grep -q "^oby-nightly: couldn't check out origin/nowhere" <<<"$said" || fail "it didn't say why: $said"
[[ -z "$(md)" ]] || fail "a run that couldn't check out wrote a result"

echo "oby-nightly-test: passed"
