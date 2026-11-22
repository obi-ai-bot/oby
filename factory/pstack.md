# pstack in the factory

The factory's agents work by [pstack](https://github.com/poteto/pstack)'s principles and playbooks, Lauren Tan's (poteto) Claude Code skills, as the owner asked on 2026-11-22. This file says which ones each role reads. It's written once here; each agent's own file only points to its role below.

## Where pstack is

Installed as a Claude Code plugin, its files are under `~/.claude/plugins/cache/*/pstack/<version>/skills/`. Use the newest `<version>` there:

```bash
P=$(ls -d ~/.claude/plugins/cache/*/pstack/*/skills 2>/dev/null | sort -V | tail -1)
```

A principle is `$P/principle-<name>/SKILL.md`; a playbook is `$P/poteto-mode/playbooks/<name>.md`. Read only the files your role lists, at the start of your work, before anything else. If `$P` is empty (pstack isn't installed on this machine), say so in your output file's first lines and go on without it: never block on it.

Never read or follow `$P/poteto-mode/SKILL.md`, `worker.md` or anything that starts its own run, spawns agents, opens pull requests or changes the factory's flow: the factory's own steps and this repository's AGENTS.md decide those. Where a principle and AGENTS.md or the job's spec disagree, AGENTS.md and the spec win; name the disagreement in your output.

## By role

| Role | Principles (`principle-<name>`) | Playbook |
|---|---|---|
| intake | attack-the-premise, subtract-before-you-add, outcome-oriented-execution | — |
| spec-writer | model-the-domain, sequence-verifiable-units, attack-the-premise, test-behavior-not-implementation | — |
| builder | fix-root-causes, prove-it-works, test-behavior-not-implementation, subtract-before-you-add, laziness-protocol, sequence-verifiable-units, boundary-discipline, minimize-reader-load | `feature` for a feat task, `bug-fix` for a fix, `visual-parity` when the task holds work to a board |
| reviewer-code | minimize-reader-load, subtract-before-you-add, test-behavior-not-implementation, fix-root-causes, type-system-discipline, boundary-discipline | — |
| reviewer-security | boundary-discipline, attack-the-premise, fix-root-causes | — |
| reviewer-ux | experience-first, minimize-reader-load | `visual-parity` |
| reviewer-ui | experience-first | `visual-parity` |
| reviewer-proofs | prove-it-works, model-the-domain | — |
| approver | prove-it-works, explain-the-number, outcome-oriented-execution | — |
