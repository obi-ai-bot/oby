# Task actions and the task page

The business behavior contract for a task's menu and its page in the Electron app, on obid's data ([#1575](https://github.com/obi-ai-bot/oby/issues/1575)), from [the template](../business-contract-template.md). Scenarios are numbered `TASKS-n`; the tests that prove them are named after them (`apps/electron/src/renderer/src/test/taskActions.test.ts`, `jobPage.test.ts`).

## Feature and status

- **Outcome:** The owner renames, archives and deletes a task, and reads its page (Result, What went out, Timeline, Usage, Details, Full session), and what they see is what obid has: an action holds after the next update, and the page shows the job's own times, activity, usage and session instead of the mock's.
- **Status:** **Implemented** in the Electron app (this change). obid's routes and job keys already existed (`docs/runner-api.md`), but for one added for Delete's ask-first (`POST /v1/jobs/{id}/delete`).
- **Scope:** Sangam's TaskMenu (Rename, Stop, Archive, Delete), the task page (`TaskCard.tsx` with `alone`, `TaskView.tsx`, `SessionView.tsx`) and the result line's pull request link. **Out of scope:** the page's Mode menu (#1570 wires it to `POST /v1/jobs/{id}/mode`; this change only shows the job's mode), a tool call's detail in the session (`GET /v1/jobs/{id}/transcript/{n}`: the lines don't open yet on obid), check rows under Check and Publish (obid's job has no per-check rows), Unarchive (Sangam's Archive has no control for it).

## Behavior

### Actors and ownership

- **obid** owns every decision: whether a job may be renamed (any time), archived (ended and needing nothing) or deleted (ended, not being published, its pull request not open, no follow-up carrying its branch), and the words of a refusal. It owns the job's stage times, estimate, activity, usage, window share, outgoing actions and transcript.
- **The app** sends the owner's action and shows obid's answer. It never decides whether an action is allowed, and it shows obid's refusal in obid's own words (the live problem line, `live.ts` `say`).

### The menu

| Action | Sends | On success | On refusal |
|---|---|---|---|
| Rename (menu, side panel, title bar) | `PATCH /v1/jobs/{id}` `{"title"}`, trimmed; nothing when it's the name it has | The new title shows at once, and obid's job replaces it | The old title comes back; obid's words show (`422`: not one line of 1 to 60 characters) |
| Stop | `POST /v1/jobs/{id}/stop` (as before) | obid's job | obid's words |
| Archive task (done tasks) | `POST /v1/jobs/{id}/archive` `{"archived": true}` | The job's `archived` is set: off Home, under Archive | Stays where it was; obid's words (`409`: `Dismiss it first, or publish it.`) |
| Delete task | `POST /v1/jobs/{id}/delete` | Nothing unpublished: gone from Home and Archive at once, its unsent words too. Commits that aren't published: obid's Confirm on the task's card (and Home's stack), naming them (`Delete Vault tests (its worktree and local branch go; 2 commits that aren't published are lost)`); only the owner's Confirm there deletes it | Stays; obid's words (`409`: running, publishing, pull request open, a follow-up on its branch). Proposed (not built): obid asks first instead, naming what's in the way, and the owner's Confirm deletes it anyway (a running task stopped first; an open pull request stays on GitHub) |

A delete of a job obid no longer has (`404`) counts as gone. Asking first only when something would be lost is the owner's decision of 2026-10-30 ("Yes, ask first"); otherwise Delete stays one click, as in Sangam's design. The Confirm is obid's own `delete_job` item, the one `oby_delete_job` raises, shown on Sangam's existing Confirm card: no new dialog. Its Confirm is signed by this Mac's device key (main), and obid refuses it once the job changed (another round, a commit). The job's `job` event (`deleted: true` for a delete) keeps every other open app in step.

### The page

What obid has goes into Sangam's `Page` (`mock.ts`) and stages, in `jobPage.ts`; a part with no data isn't drawn, as his page already does:

- **Stages:** each one's time: `seconds` when it's over ("22s", "3m 19s"), counted from `started` less `waited` while it's current ("9m 38s so far"), the middle of its `estimate` while it's to come ("≈ 30s"). Under Work, the activity when there's any, and the agent's latest words (`context`) while it's current.
- **Timeline:** from `estimate` and the stage times. Running: "10m in · done in about 2–10m", the track's done, from and to, paused while the task needs the owner, and `basis` under it; past the range, "25m in · jobs like this: 12–20m". Done: "Took 25m · estimated 10–18m" (the range first given). Failed or stopped: "Ran 25m · jobs like this: 12–20m". With no estimate, the section stays "Stages".
- **Activity:** `activity.counts` as Sangam counts them ("31 read · 6 edits · 2 test runs"), kinds with none left out; `activity.groups` with their time, the running one's last file and diff, and its tools.
- **Usage:** only when a plan covers it (`usage.priced` `plan`), the tool gave a cost and the account's 5-hour and weekly windows are known, since Sangam's line says "covered by your plan": every token count summed ("3.0M"), the cost, each window's use and the job's share from `windowShare` ("11%", "<1%" under one point or when no report rose).
- **What went out:** each `outgoing` action in words, with its clock time: "Pushed oby/fix-75 to deskree-inc/oby", "Opened pull request oby#375, ready for review", "Merged oby#375 into dev". A check run (its id is its own URL) is named by the job's pull request; a comment by the pull request its URL is on. Any other action shows as obid names it, with its id (`issueLabels https://github.com/…/issues/12`).
- **Mode:** the job's `mode` as Sangam names it (Ask, Accept edits, Plan, Bypass; Sandbox for an older job).
- **Pull request link:** the short name ("oby#375"), opening `published` on GitHub in the browser (only `https://github.com/…`).
- **Full session:** `GET /v1/jobs/{id}/transcript` when it opens, page after page (`?after=` obid's `next`, at most 1,000 entries each) until `next` is null; then `?after=` the highest `n` it has at each of the job's events, updated by `n`. Each entry as Sangam's session line: a round's start, the agent's words, its thinking, the owner's words, its to-do list, and each tool call (verb and target from obid's summary, its time, a failed command's exit code, an edit's lines added and removed).

### Failures and recovery

- obid unreachable or refusing: the action's task stays as it was (a rename goes back), and the reason shows in the problem line for a few seconds.
- A session fetch that fails shows its reason; the next job event tries again. Two events while one fetch is out make one more fetch, never two at once.
- The app restarting: everything comes from obid again (`GET /v1/jobs`); nothing of this is kept by the app.

### Invariants

- obid's API only grows: one route added, `POST /v1/jobs/{id}/delete` (docs/runner-api.md); `DELETE /v1/jobs/{id}` is unchanged, and the Swift apps are untouched.
- The mock (`npm run web`, `?everything`, `?dev=`) keeps working in place and calls nothing.
- What came from GitHub stays data: only an `https://github.com/` link opens.

## Acceptance scenarios

- **TASKS-1** Rename sends `PATCH /v1/jobs/{id}` with the trimmed title, shows it at once and keeps obid's; the same name again sends nothing; refused, the old title comes back and obid's words show.
- **TASKS-2** Archive sends `POST /v1/jobs/{id}/archive`; the archived job is off Home and in Archive; a `409` leaves it on Home with obid's words.
- **TASKS-3** Delete sends `POST /v1/jobs/{id}/delete`; a `409` leaves the task with obid's words; deleted, or `404`, it's gone.
- **TASKS-4** The result line's pull request shows its short name and opens its GitHub page.
- **TASKS-5** The stages' times and the timeline come from obid's stage times and estimate, running, paused, done and failed.
- **TASKS-6** Activity counts and groups show under Work, with the agent's latest words.
- **TASKS-7** Usage shows only when a plan covers it, with the job's share of each window.
- **TASKS-8** What went out lists every outgoing action in words; a check run by the job's pull request; any other action as obid names it, with its id.
- **TASKS-9** Full session shows obid's transcript in order, as Sangam's lines: every page until `next` is null, then only what came since (`?after=` the highest `n`), merged by `n`, with events during a fetch making one more fetch.
- **TASKS-10** On the mock, the menu works in place and calls nothing.

- **TASKS-11** Delete on a task with commits that aren't published deletes nothing yet: obid's Confirm, naming the commits lost, comes onto the task's card, and only the owner's Confirm there deletes it (`tasks_11_delete_asks_first_when_commits_arent_published` in obid's `conversation_harness`, TASKS-11 in `taskActions.test.ts`).
- **TASKS-12** A task that ended with nothing done reads that way, never as plain Done ([#1665](https://github.com/obi-ai-bot/oby/issues/1665)). obid decides it once, from its own facts: a `build`, `fix` or `test` job that ended `done` with no commits, nothing published and nothing pushed has `nothingDone: true` (`Job::nothing_done`; a review or research job, one that committed, pushed or published, or one still running doesn't). Its conversation's turn says `<title> ended with nothing done. Nothing committed, so nothing was published.`, then the agent's first line; its card in the conversation reads `Nothing done · <agent> · <account>`, its result line (Home's card, its page) `No changes`, and its notification is titled `Nothing done`. Its `status` stays `done` (the Swift apps decode it as closed). Tests: `tasks_12_a_job_that_committed_nothing_ended_with_nothing_done` (obid's `job.rs`), `job_report`'s TASKS-12 case (`conversation.rs`), `tasks_12_nothing_committed_is_done_with_nothing_done_and_needs_nothing` (`jobs_api`), TASKS-12 in `nothingDone.test.tsx`.
- **TASKS-13** Home keeps only what still needs the owner ([#1666](https://github.com/obi-ai-bot/oby/issues/1666)'s drive). obid settles, with no answer, every open item about one of its own jobs (a question, permission, plan, the agent's question, a merge card, a limit, its crash or review failure) once the job is gone, or ended let go or with its pull request merged or closed; and Oby's own escalation (only Done) on an ended job that has another open item, where obid offers that decision now (MERGING-8's). It checks when it starts and on every job and inbox change, and the apps hear each settled item. A queued or running job's items stay, as does everything not about one of its jobs: a Confirm, an offer, GitHub's, another Obi's, a teammate approval to remove on GitHub. Test: `tasks_13_items_about_an_ended_job_settle_and_what_needs_the_owner_stays` (obid's `sphere/link.rs`).
- **TASKS-14** Cards and replies use Sangam's words ([#1666](https://github.com/obi-ai-bot/oby/issues/1666)): a task by its title, never an id; never "job" or "room". Oby's escalation is titled `<task title> needs your decision`, its text what Oby found and recommends, in words for the owner, and the Electron app shows that text under the title on obid's own Question card. Every brain prompt that writes to the owner (the conversation's, the recap's, the check-in's, the escalation's) takes one rule for it (`owners_words!`, `brain/mod.rs`), and obid's own answers to its tools say "task". Tests: `tasks_14_an_event_turn_escalates_with_its_diagnosis_and_calls_only_its_tools` (`orchestrator_api`), TASKS-14 in `asks.test.ts`.

**On a real Mac (Test it when it's done):** rename a task from its menu and from the side panel, then quit and reopen: the name stays. Archive a done task: it's under Archive after a reopen. Delete a task whose pull request is open: obid's words show and it stays. Delete a task whose commits weren't published: its card asks Confirm, naming the commits; Cancel keeps it, Confirm deletes it. Open a done task's page: its pull request opens in the browser; Timeline, Usage and What went out show the job's own numbers. Open Full session on a running task: lines come in as the agent works.
