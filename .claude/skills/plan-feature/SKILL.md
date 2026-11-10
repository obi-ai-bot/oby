---
name: plan-feature
description: Turn one roadmap issue (a feature, a bug or a chore) into a Ready business contract plus a list of small, dispatchable tasks, checked against the code on origin/dev, for the owner to approve before any agent codes. Use when asked to "plan #<n>", "make #<n> ready", "draft the contract for #<n>", or to plan the next item from Now or Next.
---

# Planning one issue, just in time

One issue at a time, never the whole roadmap: plans written ahead of the code go stale, and a wrong step in a plan hurts an agent more than no plan. You write a draft; the owner approves it; only then is anything dispatched.

What "Ready" and "Done" mean, and the contract's sections, are defined once elsewhere. Read them, don't restate them:
- AGENTS.md, "How we work" (Ready, Done, the board) and "Business behavior contracts and code design".
- `.github/ISSUE_TEMPLATE/feature.yml`: the fields, in order. A feature's draft fills every required one; a bug's or a chore's, see step 3.
- `docs/business-contract-template.md`: what each section must say.

## 1. Read before writing

1. **Plan from `origin/dev`, never the checkout.** `git fetch origin dev`, then `git rev-list --count dev..origin/dev`: a local `dev` is often hundreds of commits behind. Unpack a copy into a scratch folder (`git archive origin/dev | tar -x -C <scratch>`, then `git init -q && git add -A && git commit -qm base` there so the check's plan mode works) and read, grep and count lines in that. That includes AGENTS.md: the copy in your context may be the stale one.
2. The issue, its comments and every issue, PR or decision it links: `gh issue view <n> -R obi-ai-bot/oby --json title,body,labels,comments` (plain `--comments` can print nothing). Its board row: `gh project item-list 2 --owner obi-ai-bot --limit 1000 --format json`, filtered on the number (Status, Area, Priority). Something not in Now is planned ahead of its turn: say so.
3. **Open PRs on the same files:** `gh pr list -R obi-ai-bot/oby --state open --json number,title,files`. A task waits for an open PR only when they change the same function or lines; touching the same file is a risk to list, not a wait.
4. The contract of each area it touches, `docs/contracts/<area>.md`, and the matching section of `docs/runner-api.md`.
5. **Verify every claim in the issue.** Issues quote lines, numbers and diagnoses from the day they were filed. Re-read each cited place, and check each stated fact (a limit, a timer, who reads what) against the code. Before choosing where a fix goes, read who else reads or writes the same file or state (an offset taken into a log, a cache, a timer). Corrections go under "Already true on dev".
6. Find the real entry points, callers and existing tests. When a Tetrix MCP is connected, ask it what a file touches (`code_impact`) first and confirm with `git grep`; without it, `git grep` alone.
7. **Is it already done, or half done?** `git log origin/dev --oneline -S '<symbol>'`, merged PRs that mention the issue, tests whose names match. Fully done: say so with the commit and stop.

## 2. Decide what you can't

An owner decision is anything that changes what a person sees (a word, a default, a card), a promise in a contract or AGENTS.md, a merge or publish rule (does this change what a job's check proves, and so whether it merges by itself, MERGING-2?), a line limit or a group (`OBY_CHECK_ONLY`, which `tools/oby-check-blok` also lists) in `scripts/oby-check.sh`, or a fix that differs from what the issue asks. Everything else (a size, a file name, which contract file) is your call: make it and state it in the draft.

Put the owner's questions first, under **Decisions needed**, each with your recommended answer and why. Never write a guess as if it were decided. A plan with an open decision is not Ready, and the tasks behind it wait.

## 3. Write the contract

- **A feature:** `feature.yml`'s fields, in order.
- **A bug:** `bug.yml`'s facts (steps, expected, actual, since), then of `feature.yml`'s fields only those the fix changes (always Scenarios, Files and Test it when it's done; Status, Failures and Invariants when it changes or clarifies a promised behavior: AGENTS.md, "A bug fix updates the affected contract").
- **A chore** (tooling, the check, docs): scenarios and files only, unless it changes behavior a person or an agent relies on.

Rules for the fields:

- **Sangam's screen:** the path in `apps/electron` and its `?everything` link, or "None: engine work in obid". Never invent words a person sees: quote his screen, or list the sentence under Decisions needed.
- **Status:** "Proposed", with current behavior and proposed behavior kept apart.
- **Area:** the contract that already owns the behavior; one issue may touch several, each numbered on its own. When none owns it, propose a new `docs/contracts/<area>.md` and say so.
- **Scenarios:** `<AREA>-<n>`, continuing from the highest number in that area's contract on `origin/dev`. Each is Given / when / then, observable at a boundary (an API answer, a file on disk, an event, a refused write; a lib test is fine where that is the only boundary), and names the test it becomes:
  - obid: `core/crates/obid/tests/<module>_api.rs` or the module's own `#[cfg(test)]`, `fn <area>_<n>_<what_it_shows>`;
  - the Electron app: a vitest next to the source;
  - the check and other scripts: a case in `scripts/oby-check-test.sh` (inline cases, so the scenario's key goes in a comment on it), or the script's existing self-test. What the check decides is PUBLISHING's area.
  Cover the normal path, each choice, each failure, a restart and the same request twice. Say what makes each one fail today; a "doesn't over-trigger" case belongs in the scenario whose positive case it guards.
- **Invariants:** name each AGENTS.md rule the change must keep. obid's API is add-only (new keys and values; no rename, removal or new value in an enum the Swift apps decode as closed).
- **Files it will touch:** real paths on `origin/dev`, or new ones marked new. Flag any on a security path (`SECURITY_PATHS` in `core/crates/obid/src/merge.rs`) or the check's own files (`scripts/oby-check.sh`): those PRs merge only after two reviews by different tools or the owner's click (MERGING-15).
- **Line limits:** `scripts/oby-check.sh` caps obid's production lines and `jobs.rs` (`obid_lines_limit`, `jobs_rs_limit`). Measure the room left for the files you plan to grow (run the check on the scratch copy: it prints both counts), and put code where it fits; needing a raise is a decision. Open PRs share that room, so each task that grows a capped file says how many lines it adds and tells its agent to re-measure before starting and stop if the room is gone.
- **Test it when it's done:** steps a person runs on a real Mac, or "None: covered by the tests above".

## 4. Cut it into tasks

Each task is one worker PR into `dev`, sized for one agent run that a check can judge:

- About one to two hours of a developer's work, at most about 8 files. Bigger: split it.
- Names the scenarios it makes pass and **the commands that prove them**. The check picks suites by path: see what it would run for the task's files with `OBY_CHECK_PLAN=1 OBY_CHECK_PATHS="$(printf '%s\n' <paths>)" scripts/oby-check.sh`, using the script as `origin/dev` has it (the rules are at its top; `OBY_CHECK_ONLY` names its groups). When it misses the test that proves the task (an integration binary named after another module, a lib test under another module's filter), name the extra `cargo test` command. When a task changes the check itself, the check can't judge it: a job's check runs the base's copy of the script, so its cases go in `scripts/oby-check-test.sh` against the committed script, and the commands to run by hand are named.
- **Inside or outside the box.** Some suites end "couldn't judge" inside an Oby job (Apple builds, `swift test`, nested sandboxes such as `tests/sandbox.rs`). Mark the task "runs on the owner's Mac" when its proof is one of them.
- **Owns its files.** Two tasks that can run at once never touch the same file, contracts and `docs/runner-api.md` included, and each task changes its docs in its own PR (AGENTS.md). Tasks that share a file run in sequence; prefer fewer tasks in sequence over parallel ones that collide. Say which can run in parallel.
- Carries its context as pointers, not pasted code: `path:line` on `origin/dev` of where to start, the existing function or pattern to reuse, the test to copy from, and what not to touch.
- Depends on: other tasks by key, decisions by number, open PRs by number.

More than 6 tasks, more than two of obid, the Electron app and the phones, or several fixes that don't depend on each other: propose splitting the issue under Decisions needed, and say how.

## 5. Output

Write the draft to the path you were given (default: `~/.oby-flow/plans/<issue number>.md`, outside the repository, so no `git add .` or secret scan picks it up). Layout:

```
# Plan: #<n> <title>
Planned from origin/dev at <short sha> (local dev <N> behind), <date>. Planner: <tool>, <model id>.
Board: <Status>, <Area>, <Priority>.
## Decisions needed            (or "None")
## Already true on dev         (what exists, with paths; what the issue got wrong)
## Contract                    (the fields from step 3, as headings, in order)
## Tasks                       (key, scenarios, files, proof commands, inside/outside the box, context, depends on, parallel with)
## Risks                       (what could make an agent fail: environment, flaky tests, box limits, open PRs)
```

Then stop. Don't edit the issue, the board or any code, and don't dispatch: the owner reads the draft, settles the decisions, and says what goes where. When they approve, the contract goes into the issue and the issue moves on the board (AGENTS.md, "Update the issue at every step").

## When to stop

- The issue already holds: report the evidence and stop.
- The issue contradicts AGENTS.md or Sangam's screen: write that under Decisions needed and plan nothing past it.
- You can't find where the behavior lives after reading the code: say what you searched, and stop rather than invent files.
