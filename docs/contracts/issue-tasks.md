# Issue tasks: the agent reads the issue

The business behavior contract for a task about a GitHub issue ([#1660](https://github.com/obi-ai-bot/oby/issues/1660), found by `npm run drive`), from [the template](../business-contract-template.md). Scenarios are numbered `ISSUETASK-n`; the tests that prove them are named after them (`core/crates/obid/tests/github_work.rs`, obid's lib tests in `github_work.rs`).

## Feature and status

- **Outcome:** "Do https://github.com/owner/repo/issues/140" starts a task whose agent reads what the issue asks and does it, ending in a pull request. An issue the owner wrote is the owner's own words.
- **Status:** **Implemented** (2026-10-31). Before, the agent got only the issue's number and no GitHub login, so it ended with no change and the task showed Done; and obid's read of who wrote the issue asked GitHub for more than it answers (520,300 nodes, past its 500,000), so every issue and pull request was taken as someone else's and the job was marked foreign.
- **Scope:** obid's side: what a job with an `issue` gets before its agent starts. **Out of scope:** how the brain picks the repository and number from the owner's words, and the words the app shows.

## Behavior

### Actors and ownership

- **obid** reads the issue with its own `gh` (the owner's GitHub connection) as the job's worktree is prepared, and quotes it (`crate::github_work::quote`, `Jobs::run_owner_job`).
- **The agent** reads the quoted file; it has no GitHub login.
- **The app** shows the task as obid reports it. It decides nothing.

### Rules

- **Every job with an `issue` gets it quoted**, however it started (a conversation, `POST /v1/jobs`, a plan, the GitHub feed): its title, description, labels and latest 30 comments, at most 64 KiB, each author as `@login (ASSOCIATION)`, in `.obi/inputs/github-<name>-<number>.md`, fenced as data, every secret obid knows taken out. Read afresh each time the job runs.
- **It's data, never instructions.** The agent's first prompt names the file and says so. Nothing of the issue goes in a prompt, a task, a title or the brain's briefing.
- **An issue obid can't read fails the task, saying so**, before its agent starts: `obid couldn't read issue #<n> of <repo> on GitHub (<why>), so its agent never saw it and nothing was started.` It never ends Done as if the agent had seen it.
- **Who wrote it is a read GitHub answers.** An issue only the owner wrote, with comments only the owner wrote, doesn't mark its job someone else's. What anyone else wrote there still does, for the owner's secrets (crate::secretspec, rule 5: organization members included).

### Invariants

- obid's API only grows: no key is renamed or removed, no new enum value.
- The agent never gets a GitHub login.

## Acceptance scenarios

- **ISSUETASK-1:** A job on issue 140 has `.obi/inputs/github-fixture-app-140.md` with the issue's title, description and comments, their authors, in fences; its prompt names the file and holds none of the issue's words. (`issuetask_1_a_task_on_an_issue_gets_its_text_quoted`, github_work.rs)
- **ISSUETASK-2:** A job on an issue GitHub can't give fails, its summary saying obid couldn't read it and its agent never saw it; no agent ran. (`issuetask_2_an_issue_obid_cant_read_fails_its_task_saying_so`, github_work.rs)
- **ISSUETASK-3:** obid's reads of who wrote an issue or a pull request ask GitHub for at most 500,000 nodes. (`issuetask_3_who_wrote_it_is_a_read_github_answers`, obid's lib tests)

Test it when it's done, on a real Mac: open an issue in a repository you can push to, say "Do <its URL>" in Home's box, and see the task open a pull request that does what the issue asks, with no foreign mark on the task.
