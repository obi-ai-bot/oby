# `gh pr list … --head <branch>`: once a pull request was opened (`flags/opened`), the open one of
# that branch, at the commit obid's clone has the branch at (what obid pushed); before, none.
[ -f flags/opened ] || exit 0
prev=
for a in "$@"; do [ "$prev" = --head ] && branch="$a"; prev="$a"; done
oid=$(git -C ../home/clones/obi-e2e/fixture-app rev-parse "refs/heads/$branch")
printf '[{"url":"https://github.com/obi-e2e/fixture-app/pull/7","isCrossRepository":false,"headRefOid":"%s","baseRefName":"main"}]\n' "$oid"
