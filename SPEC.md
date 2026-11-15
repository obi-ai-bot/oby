# Spec: the build step can run as an Oby job (A00-66)

## What
Add two files exactly as given below, and one short section to `factory/README.md`.

1. `factory/bin/oby-build`: executable (`chmod +x`, committed with mode 100755), contents exactly as in "File: factory/bin/oby-build" below.
2. `factory/oby-builder.md`: contents exactly as in "File: factory/oby-builder.md" below.
3. `factory/README.md`: add a section `## Builds through Oby` right before the section that holds the licence text (or at the end if there is none), 4 to 8 lines, saying: the build step can run as a job of a factory-only obid (`obid serve --config <file>` with its own `home` and `listen`, never the This Mac obid), started by `factory/bin/oby-build <job-id> <round> [account]`; the job's files reach the agent on the branch `factory-spec/<job-id>` because an Oby job reads only its worktree; obid publishes the pull request and the factory still gates, reviews and merges it; obid's own reviews of that pull request run too and can't be switched off; boxed tools (Codex on a ChatGPT login, Cursor) can't build Rust until obi-ai-bot/oby#2337 is fixed.

## Acceptance criteria
- AC1: `bash -n factory/bin/oby-build` exits 0, and `shellcheck -S warning factory/bin/oby-build` prints nothing (if shellcheck is installed; say so if not).
- AC2: `git diff --stat origin/dev` shows only those three files plus `factory/queue/done/A00-66.md`.
- AC3: The two new files are byte-identical to the blocks below (check with `diff`).
- AC4: The task is closed as BRIEF.md says.

## Out of scope
No change to `.claude/`, `factory/bin/job`, or any other file.

## File: factory/bin/oby-build

````bash
#!/usr/bin/env bash
# factory/bin/oby-build <job-id> <round> [account]
#
# The factory's build step, run as an Oby job on the factory's own obid (not the This Mac one).
# The job's files go to the agent on a branch of their own, `factory-spec/<job-id>`, because an
# Oby job reads only its own worktree. Round 1 starts a `build` job on the job's branch; later
# rounds send a message to the last Oby job, which starts its follow-up on the same branch.
# obid publishes (push, pull request); this script then brings the factory's worktree to the
# pushed head and writes build.md from the agent's last message. It never merges anything.
#
# Settings: OBY_FACTORY_HOME (default ~/.oby-factory: config.json, home/api-token),
#           OBY_FACTORY_URL (default http://127.0.0.1:7811).
set -euo pipefail

[ $# -ge 2 ] || { echo "usage: factory/bin/oby-build <job-id> <round> [account]" >&2; exit 2; }
ID=$1 ROUND=$2 ACCOUNT=${3:-deskree-team}
ROOT=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
J=$ROOT/factory/jobs/$ID
OBY=${OBY_FACTORY_HOME:-$HOME/.oby-factory}
URL=${OBY_FACTORY_URL:-http://127.0.0.1:7811}
TOKEN=$(cat "$OBY/home/api-token")
STATE=$J/oby.json

api() { curl -sS -m "${WAIT:-60}" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' "$@"; }
die() { echo "oby-build $ID: $*" >&2; exit 1; }

[ -f "$J/job.json" ] || die "no job folder $J"
KEY=$(jq -r .issue "$J/job.json")
BRANCH=$(jq -r .branch "$J/job.json")
WT=$(jq -r .worktree "$J/job.json")
BASE=$(jq -r '.base // empty' "$J/job.json"); BASE=${BASE:-$(jq -r '.base // "dev"' "$ROOT/factory/config.json")}
TITLE=$(head -1 "$J/issue.md" | sed -E 's/^# Task [A-Z0-9-]+: //')
SPEC_REF=factory-spec/$ID

# 1. The job's files, as one parentless commit on factory-spec/<job-id>.
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
sed -e "s|{{BRANCH}}|$BRANCH|g" -e "s|{{BASE}}|$BASE|g" -e "s|{{SPEC_REF}}|$SPEC_REF|g" \
    -e "s|{{KEY}}|$KEY|g" -e "s|{{TITLE}}|${TITLE//|/\\|}|g" "$ROOT/factory/oby-builder.md" > "$tmp/BRIEF.md"
cp "$J/spec.md" "$tmp/SPEC.md"
task=$ROOT/factory/queue/tasks/$KEY.md; [ -f "$task" ] || task=$ROOT/factory/queue/done/$KEY.md
cp "$task" "$tmp/TASK.md"
if [ "$ROUND" -gt 1 ]; then
  prev=$J/round-$((ROUND - 1))
  {
    echo "# Round $ROUND: why you build again"
    for f in "$J"/feedback-*.md; do [ -f "$f" ] && { echo; echo "## The owner's feedback ($(basename "$f"))"; cat "$f"; }; done
    if [ -f "$prev/checks.md" ] && ! head -1 "$prev/checks.md" | grep -q 'GATE: PASS'; then
      echo; echo "## The gate failed"; cat "$prev/checks.md"
      [ -f "$prev/checks.log" ] && { echo; echo '```'; tail -n 200 "$prev/checks.log"; echo '```'; }
    fi
    for f in "$prev"/review-*.md "$prev"/queue-failure.md; do
      [ -f "$f" ] || continue
      head -1 "$f" | grep -q 'VERDICT: PASS' && continue
      echo; echo "## $(basename "$f" .md)"; cat "$f"
    done
  } > "$tmp/ROUND.md"
fi
entries=""
for f in "$tmp"/*.md; do
  entries+="100644 blob $(git -C "$ROOT" hash-object -w "$f")	$(basename "$f")"$'\n'
done
tree=$(printf '%s' "$entries" | git -C "$ROOT" mktree)
commit=$(git -C "$ROOT" commit-tree "$tree" -m "factory: $ID round $ROUND (the build's files, not code)")
git -C "$ROOT" push -q -f origin "$commit:refs/heads/$SPEC_REF"

# 2. Start the build (round 1) or its follow-up (later rounds).
ASK="You are the factory's builder for task $KEY, round $ROUND. Run: git fetch origin $SPEC_REF && git show FETCH_HEAD:BRIEF.md . Follow BRIEF.md exactly: it says what to read (SPEC.md, TASK.md, and ROUND.md from round 2 on), how to build, test and commit, and how your last message must start (STATUS: READY or STATUS: BLOCKED). Never push and never open a pull request: Oby publishes."
if [ "$ROUND" -eq 1 ] || [ ! -f "$STATE" ]; then
  body=$(jq -n --arg repo deskree-inc/oby --arg acct "$ACCOUNT" --arg br "$BRANCH" --arg base "$BASE" \
    --arg title "$KEY ${TITLE:0:50}" --arg task "$ASK" --arg key "factory-$ID-r$ROUND" \
    '{job: {repo: $repo, kind: "build", account: $acct, branch: $br, base: $base, title: $title[0:60], rounds: 1, task: $task, clientRequestId: $key}}')
  out=$(api -X POST "$URL/v1/jobs" -d "$body")
else
  last=$(jq -r .last "$STATE")
  out=$(api -X POST "$URL/v1/jobs/$last/messages" -d "$(jq -n --arg t "$ASK" '{text: $t}')")
fi
oid=$(echo "$out" | jq -r '.id // .job.id // empty')
[ -n "$oid" ] || die "obid refused the job: $(echo "$out" | jq -r '.error // .' | head -c 400)"
jq -n --arg last "$oid" --argjson round "$ROUND" --arg acct "$ACCOUNT" \
  --slurpfile old <( [ -f "$STATE" ] && cat "$STATE" || echo '{}' ) \
  '($old[0] // {}) + {last: $last, account: $acct, round: $round} | .jobs += [{round: $round, id: $last}]' > "$STATE.new"
mv "$STATE.new" "$STATE"
echo "oby-build $ID: round $ROUND is Oby job $oid on $ACCOUNT"

# 3. Wait for it, bring the factory's worktree to what it pushed, and write build.md.
res=$(WAIT=43200 api "$URL/v1/jobs/$oid/wait" | tr -d '\n' | sed 's/^ *//')
status=$(echo "$res" | jq -r .status)
summary=$(echo "$res" | jq -r '.summary // ""')
pr=$(echo "$res" | jq -r '.published // .pr // "" | tostring')
perr=$(echo "$res" | jq -r '.publishError // "" | tostring')
agent=$(echo "$res" | jq -r '.agent // ""')
first=$(printf '%s\n' "$summary" | sed -n '/^STATUS:/{p;q;}')
if [ "$status" = "done" ] && [ -z "$perr" ] && git -C "$WT" fetch -q origin "$BRANCH" 2>/dev/null; then
  git -C "$WT" reset -q --hard FETCH_HEAD
  [ -n "$first" ] || first="STATUS: BLOCKED: the agent's last message had no STATUS line"
else
  first="STATUS: BLOCKED: Oby job $oid ended $status${perr:+, publishing failed: $perr}"
fi
{
  echo "$first"
  echo
  echo "# Build: $TITLE"
  echo
  echo "Built by Oby job $oid ($agent, account $ACCOUNT), round $ROUND. Pull request: ${pr:-none}."
  echo
  printf '%s\n' "$summary" | awk '!f && /^STATUS:/ {f=1; next} {print}'
  if [ -f "$J/build.md" ]; then echo; echo "## Earlier rounds"; sed -n '/^# Build/,$p' "$J/build.md" | sed '1d'; fi
} > "$J/build.md.new"
mv "$J/build.md.new" "$J/build.md"
echo "oby-build $ID: $first"
````

## File: factory/oby-builder.md

````markdown
# The factory's builder, as an Oby job

You build one factory job inside the Oby job you are running in. Your worktree is
already on the job's branch, `{{BRANCH}}`, cut from `origin/{{BASE}}`. obid
publishes your commits when you finish: it pushes the branch and opens or updates
the pull request. You never push, never open a pull request and never merge.

## What to read

The factory keeps this job's files on the branch `{{SPEC_REF}}` of `origin`. Run
`git fetch origin {{SPEC_REF}}`, then read each file with
`git show FETCH_HEAD:<name>`. Never check that branch out and never merge it.

- `BRIEF.md`: this file.
- `SPEC.md`: what to build. Its acceptance criteria are your test plan.
- `TASK.md`: the roadmap task `{{KEY}}`, as it was written. It is background;
  the spec is what you build.
- `ROUND.md`, from round 2 on: why you are building again. It holds the gate's
  verdict and its failing log, the reviews that asked for changes, or the
  owner's feedback. Address every point in it.

Also read `AGENTS.md` in your worktree and follow it, its "Rules Oby never breaks"
above all.

## Build

- **The simplest change that fully meets the spec.** Match the surrounding
  code's idioms, naming, imports and comment density. Add no fallbacks and no
  extras the spec doesn't ask for.
- **Format only what you changed.** Run the formatters and linters the repo
  configures, on your changed lines only. Never reformat a whole file, and never
  run a `--fix` across the repo.
- **Never raise the line limits.** `obid_lines_limit` and `jobs_rs_limit` in
  `scripts/oby-check.sh` are the owner's to raise. If the change can't fit, end
  BLOCKED and say by how many lines.
- **Never touch a protected file.** Don't modify or delete a file matching
  `protected_paths` in `factory/config.json`. If the spec can't be met without
  changing one, end BLOCKED and say which file and why.
- **Electron setup.** For the Electron app, run `cd apps/electron && npm install`
  first.

## Test

- **Never run the whole check.** The factory's gate runs `factory/checks.sh`
  after you finish. Run only the tests your change touches, plus clippy on the
  crates you changed.
- **Rust:** use your worktree's own build folder: `CARGO_TARGET_DIR=$PWD/core/target`
  in front of every Cargo command run from the worktree root, and
  `--manifest-path core/Cargo.toml`.
- **Electron:** `npm run typecheck`, `npm test` and `npm run build` in
  `apps/electron`, when you touched it.
- **Proofs:** when `SPEC.md` has a Proofs section, follow it: one harness per
  `PROOF-<n>`, its `Broken old` line kept once in the code, and each run's
  `VERIFICATION:-` line reported. Never edit a property to make it pass.
- **Every acceptance criterion gets evidence:** the command you ran and its
  result.

## Commit

- **Conventional Commits,** in the style of `git log --oneline`
  (`feat(obid): …`), naming the scenario ids each commit implements. One commit
  per distinct piece of work. The first commit's subject becomes the pull
  request's title, so make it the task's own title:
  `{{TITLE}}`.
- **Close the task in its own last commit.** If
  `factory/queue/tasks/{{KEY}}.md` is in your worktree, run
  `git mv factory/queue/tasks/{{KEY}}.md factory/queue/done/{{KEY}}.md` and leave
  the file unchanged. Otherwise, write `TASK.md`'s exact contents to
  `factory/queue/done/{{KEY}}.md` and commit that. The factory refuses a build
  that doesn't close its task.
- **Stage explicit paths,** never `git add -A`, and leave `git status --porcelain`
  empty.

## Your last message

It becomes the factory's `build.md`, so its first line must be exactly one of:

- `STATUS: READY`
- `STATUS: BLOCKED: <the reason, in one line>`

After that first line:
- 3 to 5 bullets on what the branch now does and what this round changed.
- Each acceptance criterion with its evidence.
- Anything you couldn't verify, said plainly.
````
