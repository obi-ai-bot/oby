#!/bin/bash
# Whether a release tag's commit is on main (docs/contracts/release.md, RELEASE-2): release.yml's
# first job runs it, and no job after it runs when it fails, so a tag off main publishes nothing.
# Usage: scripts/on-main.sh <tag> [main's ref, origin/main by default], in a clone that has both.
set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }
tag=${1:?usage: on-main.sh <tag> [main]}
main=${2:-origin/main}
commit=$(git rev-parse --verify --quiet "refs/tags/$tag^{commit}") || die "there's no tag $tag: nothing is published"
git rev-parse --verify --quiet "$main^{commit}" > /dev/null || die "there's no $main to check $tag against: nothing is published"
git merge-base --is-ancestor "$commit" "$main" || die "$tag ($commit) isn't on main: nothing is published"
echo "$tag ($commit) is on main"
