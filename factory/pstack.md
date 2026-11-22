# pstack in the factory

The factory's agents work by [pstack](https://github.com/poteto/pstack)'s principles, Lauren Tan's (poteto) Claude Code skills, as the owner asked on 2026-11-22. The principles each role reads are copied into [`factory/pstack/`](pstack/) from pstack 0.15.46, with its MIT licence ([`factory/pstack/LICENSE`](pstack/LICENSE)), so what steers the factory changes only by a reviewed diff here, never by a plugin update, and works on a machine without pstack installed. This file says which ones each role reads; it's written once here, and each agent's own file only points to its role below.

## How to use them

At the start of your work, read `factory/pstack/principle-<name>.md` for each name your role lists below, and nothing else from pstack: ignore any link in them to another pstack file or skill.

They shape how you think and write, never what the factory does. Your own agent file (`.claude/agents/<role>.md`), the factory's steps, this repository's AGENTS.md and the job's spec all win over a principle: where one disagrees, follow them and say so under Notes in your output, never before its first line. A principle never makes you spawn agents, push, rebase, force-push, open or ready a pull request, merge, skip a test, or ask a person and wait. A security check in obid is never "redundant" in the sense of `subtract-before-you-add` or `boundary-discipline`.

## By role

| Role | Principles (`principle-<name>`) |
|---|---|
| intake | attack-the-premise, subtract-before-you-add, outcome-oriented-execution |
| spec-writer | model-the-domain, sequence-verifiable-units, attack-the-premise, test-behavior-not-implementation |
| builder | fix-root-causes, prove-it-works, test-behavior-not-implementation, subtract-before-you-add, laziness-protocol, sequence-verifiable-units, boundary-discipline, minimize-reader-load |
| reviewer-code | minimize-reader-load, subtract-before-you-add, test-behavior-not-implementation, fix-root-causes, type-system-discipline, boundary-discipline |
| reviewer-security | boundary-discipline, attack-the-premise, fix-root-causes |
| reviewer-ux | experience-first, minimize-reader-load |
| reviewer-ui | experience-first |
| reviewer-proofs | prove-it-works, model-the-domain |
| approver | prove-it-works, explain-the-number |

pstack's playbooks (feature, bug-fix, visual-parity and the rest) are left out: they run their own flow (subagents, rebases, opening pull requests), which the factory's steps already define.
