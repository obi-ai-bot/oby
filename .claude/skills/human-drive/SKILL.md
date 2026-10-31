---
name: human-drive
description: Drive the real Oby app (Electron, this checkout's build) as its owner does, on their own obid and agent accounts, through the conversation loop (ask, issue to task, steer, pull request, follow-up, merge card, permission and question cards); judge what it showed against Sangam's screens and docs/contracts; propose findings and file the confirmed ones as bugs. Use when asked to "drive Oby", "test Oby like a human", "run the human drive" or to add a scenario to it.
---

# Driving Oby as its owner does

The driver is `apps/electron/scripts/human-drive.mjs`, its scenarios `apps/electron/scripts/human-drive.scenarios.mjs`, the why `docs/human-drive/research.md`, past reports `docs/human-drive/`.

## Run it

1. **Ask the owner** which scratch repository the tasks may push to and merge in, and confirm the real side effects: tasks on their Claude account; an issue, branches, pull requests and a merge in that repository; and a device of the driver's own, `Oby human drive (test, revoked after each run)`, registered with their obid for the run (it can sign words and Confirms while it's registered). The repository is required (`OBY_DRIVE_REPO`, no default); anything under `obi-ai-bot/` is refused. Never sign in to anything: the owner's logins are used as they are.
2. obid must be running on 7777 (the installed Oby starts it). Build the app: `cd apps/electron && npm ci && npm run build`.
3. Run it in the background (a full loop takes 30 to 45 minutes): `OBY_DRIVE_REPO=owner/name npm run drive` (`-- --only open,research` for a few; `-- --list` lists them; `OBY_DRIVE_SHOW=1` shows the window). It prints the run's folder, `~/Library/Application Support/oby-human-drive/runs/<time>`, outside this repository: reports quote the scratch repository, which may be private, so they're never committed as they are.
4. **Check the cleanup.** At the end the driver closes the issue and the pull requests its run made, deletes their `oby/` branches and revokes its device; the report's `## Cleanup` says what it did and anything that failed. Undo by hand only what it lists as failed, and say so. It doesn't stop a task still running or waiting: stop those from the app if the run left one.

## Judge it

Read `report.md`, then **look at every screenshot** and the notes. For each step, compare with:
- Sangam's screen for it (`apps/electron/README.md` maps them; `npm run web` then `?everything` shows his design on the mock) and his words: the design wins.
- The contract it names (`docs/contracts/*.md`): its scenarios, its words ("Pull request open · waiting for your review on GitHub"), its failure behavior.
- `report.json`'s `unsigned` (words main couldn't sign) and `pageErrors`: any is a finding.

Label each finding **broken** (a promise not kept, a hang, an error), **design** (differs from Sangam's screen), **words** (jargon, wrong or unclear text a person reads), or leave it out. A finding names the scenario, the screenshot, what was expected (with the contract or screen) and what was seen. Write them under the report's `## Findings`. A slow agent or a model's phrasing isn't a bug unless a contract or the design promises otherwise.

## File them

Show the owner the list. Only after they confirm, file each confirmed one as `type:bug` on `obi-ai-bot/oby`, in `.github/ISSUE_TEMPLATE/bug.yml`'s shape (Steps, Expected, Actual, version or commit, Area, the obid log lines redacted), titled `fix: …`, and add it to the board "Oby — Roadmap". Link the issues in the report.

## Add a scenario

Append one object to the list in `human-drive.scenarios.mjs`:

```js
{
  id: 'rename-task', title: 'Rename a task in words', contract: 'tasks.md', needs: ['task'],
  run: async (t) => {
    await t.say('Call that task “One line”.');             // typed in the visible box, sent with a real Enter
    await t.see('One line', { label: 'the new name' });    // waits on what the window shows
    await t.shot('renamed');
  },
},
```

`t.say`, `t.reply`, `t.click(name)`, `t.see(text | RegExp)`, `t.shot`, `t.note`, `t.state`, `t.gh(...)` (fixture setup in the scratch repository only: an issue a scenario opens goes in `t.state.issues`, so the run's end closes it) and `t.newestJob()` (evidence from obid's job files, never to drive). Drive only through the window: Playwright's click or Enter is real input, so the owner's words are signed; a `dispatchEvent` would not be. Run the new one with `--only`, and commit the scenario; a report goes into the repository only with the scratch repository's own text taken out.
