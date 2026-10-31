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

- **Every job with an `issue` gets it quoted**, however it started (a conversation, `POST /v1/jobs`, a plan, the GitHub feed): its title, description, labels and latest 30 comments, at most 64 KiB, each author as `@login (ASSOCIATION)`, in `.obi/inputs/github-<name>-<number>.md`, fenced as data, every secret obid knows taken out. Read afresh each time the job runs (a first run, Try again, the run after an answer). The feed's tap reads it too, only so a tap GitHub can't answer starts nothing (`502`); it keeps no copy.
- **It's data, never instructions.** The agent's first prompt names the file and says so. Nothing of the issue goes in a prompt, a task, a title or the brain's briefing.
- **An issue obid can't read is said plainly.** When the owner asked for the work by the issue alone ("Do <issue URL>", "Fix #42": at most 3 words besides links, `#N` and `owner/name`), the task fails before its agent starts: `obid couldn't read issue #<n> of <repo> on GitHub (<why>), so its agent never saw it and nothing was started. Sign gh in to an account that can read it, then ask again.` It never ends Done as if the agent had seen it. When the owner's words carry the work, the agent works from them: its prompt says obid couldn't read the issue (never GitHub's error text), and its report ends `obid couldn't read issue #<n> of <repo> on GitHub (<why>): its agent worked from the task alone.`
- **Whose words the issue holds is read each time it's quoted, right after.** An issue only the owner wrote, with comments only the owner wrote, leaves the job the owner's. A word anyone else wrote there (a comment, an edit, a rename), there the first time or added before a later run, marks the job someone else's from then on, and so does anyone but the owner in what obid quoted, a comment deleted before the authorship read included.: a read GitHub refuses (a rate limit) marks it too, never asked again, since a comment deleted in between would clear a job it reached. Its agent never saw an issue obid couldn't read, but who wrote it is read as it starts all the same: its check gets none of the owner's secrets, and its pull request merges after two reviews (MERGING-13). Organization members count as anyone else here, for now (crate::secretspec, rule 5; the owner's decision of 2026-10-24 that their words are the owner's isn't kept for this mark yet, [#1675](https://github.com/obi-ai-bot/oby/issues/1675)).
- **Who wrote it is a read GitHub answers**: at most 500,000 nodes.

### Invariants

- obid's API only grows: no key is renamed or removed, no new enum value.
- The agent never gets a GitHub login.

## Acceptance scenarios

- **ISSUETASK-1:** A job on issue 140 has `.obi/inputs/github-fixture-app-140.md` with the issue's title, description and comments, their authors, in fences; its prompt names the file and holds none of the issue's words. A feed tap on an assigned issue keeps no attachment and its job gets the same file. (`issuetask_1_a_task_on_an_issue_gets_its_text_quoted`, `the_feed_lists_offers_new_work_once_and_starts_it_on_a_tap`, github_work.rs)
- **ISSUETASK-2:** "Do #404", an issue GitHub can't give, fails, its summary saying obid couldn't read it, its agent never saw it, and to sign gh in to an account that can read it; no agent ran. A task whose words carry the work runs, its prompt saying the issue wasn't read and its report ending so. (`issuetask_2_an_issue_obid_cant_read_is_said_plainly`, github_work.rs; `issuetask_2_a_request_is_the_issue_alone_or_carries_the_work`, obid's lib tests)
- **ISSUETASK-3:** obid's reads of who wrote an issue or a pull request ask GitHub for at most 500,000 nodes. (`issuetask_3_who_wrote_it_is_a_read_github_answers`, obid's lib tests)
- **ISSUETASK-4:** A job on an issue only the owner wrote ends unmarked; after a stranger comments, Try again ends it marked someone else's, naming the issue; and a job whose quote holds a stranger's comment that GitHub's authorship read no longer lists ends marked too. (`issuetask_4_whose_words_are_read_each_time_the_issue_is`, github_work.rs; `issuetask_4_the_quote_is_the_owners_only_when_all_of_it_is`, obid's lib tests)

Test it when it's done, on a real Mac: open an issue in a repository you can push to, say "Do <its URL>" in Home's box, and see the task open a pull request that does what the issue asks, with no foreign mark on the task.
