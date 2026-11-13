# The software factory

`/factory run` in Claude Code, from the repository root, builds the roadmap's tasks one pull request each:
intake → spec → build → gate → review → approve. How to run it and what each piece does:
`.claude/skills/factory/SKILL.md`. The roadmap and its 180 tasks: `queue/README.md`.

```
config.json     concurrency, rounds, the active sprint, routing, protected paths, prices
checks.sh       the gate's checks: scripts/oby-check.sh on the job's worktree
bin/job         the only writer of jobs/<id>/job.json; reads the queue; refuses illegal moves; merges
bin/gate        runs checks.sh in a job's worktree → round-N/checks.md
bin/route       risk + surfaces → the job's reviewers and models
queue/tasks/    open tasks, one file each (R01-02.md)
queue/done/     finished tasks: a task's own pull request moves its file here
dashboard.html  the board: cd factory && python3 -m http.server, then http://localhost:8000/dashboard.html
jobs/           runtime state, local to this Mac (gitignored)
```

What differs from the reference factory: the queue is the task files here, not GitHub issues (the owner's
decision of 2026-11-12). A task runs only in the active sprint (`sprint` in `config.json`) and after the tasks
its `DEPENDS` line names are in `queue/done`. Intake's questions and the owner's answers stay in the job's
folder (`/factory <answer>`), and nothing reads or writes a GitHub issue or the project board.

## Licence

The factory is adapted from ZazenCodes' Software Factory for Claude Code v2
(<https://github.com/zazencodes/zazencodes-season-3/tree/main/src/software-factory-claude-code-v2>), which is
GPLv3: `LICENSE` here is its licence. It covers the files that come from it: `bin/`, `dashboard.html`,
`config.json`, `checks.sh`, `.claude/skills/factory/` and the eight agents in `.claude/agents/`. They are
developer tooling that runs beside Oby and are no part of the app, obid or anything Oby ships; the rest of
this repository stays Apache-2.0. The task files in `queue/` are Oby's own.
