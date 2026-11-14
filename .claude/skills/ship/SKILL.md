---
name: ship
description: Ship a pull request into dev by hand, outside the factory: the local check, the PR from the template, the review loop, the Mac check on the exact head, the squash-merge through GitHub's queue, and the task file or issue tracked at each step. Use when asked to "ship this PR", "open the PR and get it merged", or to carry a branch from its last commit to dev.
---

# Shipping a pull request into dev

One flow for a pull request shipped by hand, by a person or an agent outside the factory. A factory job is shipped by the factory's own pipeline (`.claude/skills/factory/SKILL.md`) and doesn't use this skill.

This skill changes no rule: it gathers what AGENTS.md already says, in order. Where it and AGENTS.md differ, AGENTS.md wins. Read the rules where they live, don't restate them:
- AGENTS.md, "How we work" (branches, the merge queue, Conventional Commits, reporting checks) and "Rules Oby never breaks › Carrying and merging" (this repository's remembered workflow, the two-reviews rule).
- `docs/ci.md`, "What the Mac runs" and "How a merge lands".
- `docs/contracts/merging.md`, MERGING-13 to 16.
- `.github/PULL_REQUEST_TEMPLATE.md`.

Holds below are defaults: the owner's word lifts any of them ("Defaults: Oby forbids nothing" in AGENTS.md). The merge queue, its required `Check` and the branch rules are GitHub's limits, which the owner changes in the repository's settings.

## 1. Before the PR

1. Branch from `dev` (`git fetch origin dev`, then branch from `origin/dev`).
2. Commit with Conventional Commits, in the style of `git log --oneline`.
3. Run `scripts/oby-check.sh` on the branch, with `oby-check-blok` installed as AGENTS.md "Build and run" says. `OBY_CHECK_PLAN=1 scripts/oby-check.sh` first shows what it will run.
4. Keep the commands you actually ran and what they printed: they go in the PR's "Checks run". A suite that couldn't run is reported as not run, not as passed.

## 2. Open the PR into dev

1. `gh pr create --base dev --head <branch>`, its body from `.github/PULL_REQUEST_TEMPLATE.md`.
2. Answer every section, "Security path" included. A section that doesn't apply says "none" and stays.
3. "Security path" says yes, with the path, when the diff touches one of `merge::SECURITY_PATHS` (MERGING-15, `core/crates/obid/src/merge.rs`). That decides step 5.

## 3. The review loop

For each review that asks for changes:
1. Address every finding, or answer on the PR why not.
2. Commit and push.
3. Run `scripts/oby-check.sh` again on the new head, and update "Checks run" with what it printed.

A review counts only for the head it read: after a push, the review of the new head is the one that counts.

## 4. Before merging

Both hold on the same head (AGENTS.md "Carrying and merging", `docs/ci.md`):
- the review approves the PR's exact head;
- the Mac check passed on that head: `scripts/oby-check.sh` on a Mac at that commit, with its gitleaks pass clean.

Any push after that starts again at step 3.

## 5. Security path or untrusted

When the change touches a path in `merge::SECURITY_PATHS` (MERGING-15) or comes from an untrusted task (MERGING-14), it merges only after two approving reviews of its exact head by different tools, or the owner's click (MERGING-13 to 16).

Stop here and hand the merge to the owner: say on the PR that it touches a security path (or is untrusted), which reviews it has, and that the merge is theirs. Don't queue it yourself unless the owner tells you to.

## 6. Merge

1. Squash into `dev` through GitHub's merge queue, pinned to the approved head, as `docs/ci.md` "How a merge lands" says:
   `gh pr merge <n> --squash --auto --match-head-commit <head>`
2. If the queue removes the PR, read why from its failed `Check` (a conflict with `dev`, a test, a line limit judged on the group's combined code). Fix it, push, and go back to step 3.
3. When GitHub's queue, its required `Check` or a branch rule holds the merge, say plainly what holds it and where the owner changes it.

## 7. Tracking, in the same turn as each step

- **A roadmap task** (AGENTS.md "How we work": the roadmap is the task files): its file moves from `factory/queue/tasks/` to `factory/queue/done/` unchanged, with `git mv`, in the PR's last commit.
- **A GitHub issue** (a bug filed the same day):
  - work starts: `status:in-progress`, and the branch name in a comment;
  - the PR opens: `status:in-review`, and the PR link;
  - it waits on something: `status:blocked`, and what it waits on;
  - it merges: the status labels come off.
- **An epic's checklist item**, only when the PR lands one: tick it, with the PR's link, in the same turn as the merge.
- **The board:** "Oby — Roadmap" is cleared and its issues closed (AGENTS.md "How we work", the line on the task files). Move a row (Now, In review, Done) only where an item still has one.

## Outside this skill

- Merging `dev` into `main`: AGENTS.md "Carrying and merging" says when.
- The release audit (R09).
