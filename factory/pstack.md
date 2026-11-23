# pstack in the factory

The factory's agents work by [pstack](https://github.com/poteto/pstack), Lauren Tan's (poteto) Claude Code skills. The owner, on 2026-11-22: "The idea is to follow Pstack and make it the truth way and the factory is just a way for me as a human to understand what is going on and be part of the process." So the builder builds by pstack's own method, its review and verification included, and the factory's stages, files and dashboard show each round and keep the owner's decision points.

Every pstack file an agent reads is copied into [`factory/pstack/`](pstack/) from pstack 0.15.46, unchanged, with its MIT licence ([`factory/pstack/LICENSE`](pstack/LICENSE)), so what steers the factory changes only by a reviewed diff here, never by a plugin update, and works on a machine without pstack installed. Nothing is read from the plugin's own folder.

## The pinned files

| File in `factory/pstack/` | pstack 0.15.46's |
|---|---|
| `principle-<name>.md` | `skills/principle-<name>/SKILL.md`, for the 16 names below |
| `poteto-mode.md`, `worker.md` | `skills/poteto-mode/SKILL.md`, `skills/poteto-mode/worker.md` |
| `playbook-<title>.md` | `skills/poteto-mode/playbooks/<title>.md`, for feature, bug-fix, visual-parity, opening-a-pr and shipping |
| `<name>.md` | `skills/<name>/SKILL.md`, for interrogate, arena, architect, in-your-own-words, tdd, no-comments, unslop and technical-writing |
| `comment-sicko.md` | `agents/comment-sicko.md` |
| `claude-code.md` | `docs/harness/claude-code.md`, the map from pstack's Cursor names to Claude Code's |

The principles: attack-the-premise, boundary-discipline, experience-first, explain-the-number, fix-root-causes, laziness-protocol, minimize-reader-load, model-the-domain, outcome-oriented-execution, prove-it-works, redesign-from-first-principles, separate-before-serializing-shared-state, sequence-verifiable-units, subtract-before-you-add, test-behavior-not-implementation, type-system-discipline.

A link or name in a pinned file maps to this folder: a skill `<name>` (`skills/<name>/SKILL.md`, `../<name>/SKILL.md`, "the **name** skill", `/name`) is `factory/pstack/<name>.md`, a playbook `playbooks/<title>.md` is `factory/pstack/playbook-<title>.md`, `worker.md` and the Claude Code adapter (`docs/harness/claude-code.md`) are their files here, and the agent `comment-sicko` is a `pstack-worker` whose brief is `factory/pstack/comment-sicko.md`. A skill, principle or tool that isn't here (pstack's `why`, `swarm`, `figure-it-out` and the rest, `cursor-team-kit`'s `/deslop`, `control-ui` and `control-cli`, Cursor's cloud agents and Bugbot) is skipped with one line under Notes in `build.md`, `Skipped: <step>. <why>.`, as the adapter skips a Cursor-only step; the factory's own tools stand in where they can (`factory/bin/test-run`, `agent-browser` and the `verify-oby` skill for the window).

## The builder: pstack's method

The builder works by `poteto-mode.md` and the playbook for its task's kind, named in `build.md`'s Summary: `playbook-bug-fix.md` for a task whose title is a `fix`, `playbook-visual-parity.md` for one whose spec asks the window to match an existing screen or Sangam's board pixel for pixel, and `playbook-feature.md` for every other. It reads `claude-code.md` first, then `poteto-mode.md`, the playbook, `worker.md`, and every file here a step it runs names. It runs the steps as written, their review and verification included:

- **Its own review.** Every round, contested or not, it runs `interrogate.md` on the branch's diff from `origin/<base>`, and then one verifier from `playbook-shipping.md` step 1: a fresh agent that didn't write the code drives the change on the matching surface, parent against head, and says `PASS`, `PASS+NOTES` or `FAIL`. Shipping's steps 2 to 6 (landing) are the factory's: the gate, the approver and `factory/bin/job merge`. The builder fixes what they find, each fix verified, and runs both again on the head it pushes last.
- **Its record.** It writes their outcome to `round-<N>/review-pstack.md` in the job folder, in the format `.claude/agents/builder.md` gives, for that pushed head: this is the round's review, which the factory and the dashboard read as they read a reviewer's.
- **Subagents.** Where a step spawns agents (architect, arena, interrogate, the verifier, no-comments), the builder spawns each as `subagent_type: "pstack-worker"` (`.claude/agents/pstack-worker.md`: the builder's file and shell tools, no Agent tool, no MCP server), never another type (`pstack:poteto-agent` reads the plugin's folder), with the model and effort `~/.claude/rules/pstack-models.md` gives the role, mapped by `claude-code.md`, and no `isolation`. A pinned file's `generalPurpose`, `poteto-agent` or `comment-sicko` is a `pstack-worker` too. Each kickoff's first line is `Read <main checkout>/factory/pstack/worker.md before anything else.`, and its second, with the two paths filled in: `This kickoff overrides worker.md: push nothing, open, edit or ready no pull request, use no MCP tool, send nothing outside this Mac, and write only under <worktree> or <scratch>.` Then the job's header line, this file's path, and the goal. `<worktree>` is the job's worktree, or for a read-only role the detached worktree of the job's head the builder makes for it under `<job folder>/scratch/` (`git worktree add --detach`), `<scratch>`; the builder removes those before it finishes. When the session refuses the builder an Agent call, the step can't run: the builder writes `STATUS: BLOCKED` and says so.
- **Commits.** Small, ordered commits, each a verified unit (`principle-sequence-verifiable-units.md`); the failing test lands before its fix where `tdd.md` says one is cheap. To order them it may rebase its own job branch, `factory/<job-id>`, and push it with `git push --force-with-lease origin factory/<job-id>`: the owner's word (2026-11-22). It never force-pushes, or pushes at all, any other branch, `dev` and `main` above all. The gate judges whatever head the branch has then, and every review names that head.
- **The pull request.** Its body follows `playbook-opening-a-pr.md`'s headings after the factory's first line, and it stays a draft: the factory marks it ready only on the approver's APPROVE, where `worker.md` and the playbook say to open it ready.

## What stays the factory's

pstack shapes how an agent works; it never moves a job past the factory's rules. Where a pinned file and the factory disagree, the factory wins and the agent says so under Notes, never before its file's first line:

- AGENTS.md's five non-negotiables, the secret scan first among them.
- This brief overrides `factory/pstack/poteto-mode.md`'s Autonomy for the builder: use no MCP tool, and take no external action (no team chat, ticket update, eval, message or post), whatever a pinned file says; the only things that leave this Mac are your pushes of `factory/<job-id>` and its draft pull request.
- A change to a security path, RISK high or the `security` surface needs two approving reviews of its exact head by different tools: pstack's (`review-pstack.md`) and Codex's (`review-codex.md`, written by `factory/bin/review-codex`), which `factory/bin/job` adds to the round's reviews itself. Every factory task comes from `queue/tasks` on `dev`, so none is untrusted.
- Each job's own stages: intake, the spec, the gate on the pull request's exact head, `prove`, the approver, and the owner's decision points (`needs-info`, `needs-human`, ESCALATE). Nothing merges into `main`, and only `factory/bin/job merge` merges at all.
- The factory's reviewer agents (`reviewer-code`, `-ux`, `-ui`, `-security`, `-proofs`) are routed only when the owner asks, with `factory/bin/route <job-id> --lenses`.
- A security check in obid is never "redundant" in the sense of `principle-subtract-before-you-add.md` or `principle-boundary-discipline.md`.

## The other roles: principles

Intake, the spec writer, the approver and a lens reviewer when routed read, at the start of their work, `factory/pstack/principle-<name>.md` for each name their role lists below, and nothing else from pstack: they ignore any link in them to another pstack file or skill. A principle shapes how they think and write, never what the factory does: it never makes them spawn agents, push, open or ready a pull request, merge, skip a test, or ask a person and wait.

| Role | Principles (`principle-<name>`) |
|---|---|
| intake | attack-the-premise, subtract-before-you-add, outcome-oriented-execution |
| spec-writer | model-the-domain, sequence-verifiable-units, attack-the-premise, test-behavior-not-implementation |
| reviewer-code | minimize-reader-load, subtract-before-you-add, test-behavior-not-implementation, fix-root-causes, type-system-discipline, boundary-discipline |
| reviewer-security | boundary-discipline, attack-the-premise, fix-root-causes |
| reviewer-ux | experience-first, minimize-reader-load |
| reviewer-ui | experience-first |
| reviewer-proofs | prove-it-works, model-the-domain |
| approver | prove-it-works, explain-the-number |
