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

echo 'oby-check-test: passed'
