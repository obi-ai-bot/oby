# A task's changes, its worktree, and Undo this round

The business behavior contract for seeing everything a task changed, opening its worktree in the owner's own Terminal or editor, and taking its last round back, from [the template](../business-contract-template.md). Scenarios are numbered `CHANGES-n`; the tests that prove them are named after them: obid's in `core/crates/obid/tests/task_changes.rs` and `core/crates/obid/src/jobs/changes/rounds/undo.rs`, the app's in `apps/electron/src/renderer/src/test/taskChanges.test.tsx` and `apps/electron/src/renderer/src/test/worktreeOpen.test.ts`.

## Feature and status

- **Outcome:** The owner sees what a task did to their code as T3 Code's diff panel shows a thread's: every file it changed, then the diff, for the whole task or one round at a time, with whitespace left out on a click. From the same task they open its worktree in their own Terminal or editor, and take its last round back when it went the wrong way.
- **Status:** **Implemented.** The whole task's files and diff (`GET /v1/jobs/{id}/changes`) are [tasks.md](tasks.md)'s TASKS-23, 24, 29 and 30; this contract adds rounds, whitespace, the panel, the worktree and Undo. Sangam has no screen for these yet: the panel follows T3 Code's and the app's own side panel, and the words (**Changes**, **All rounds**, **Round n**, **Ignore whitespace**, **Open in Terminal**, **Open in <editor>**, **Undo this round…**, **Undo**, **Not now**) are new, all his to change.
- **Scope:** obid's record of a job's rounds, its per-round and whitespace diff, Undo this round; the Electron app's Changes panel, its task menu's Open in Terminal, Open in <editor> and Undo this round…. **Out of scope:** a terminal or editor inside Oby (T3 Code's own terminal), checkpoints of work left uncommitted, undoing a round of a merged pull request, split view, and the iPhone.

## Behavior

### Actors and ownership

- **obid** keeps each job's rounds (`Job::checkpoints`, in its file, never in the API's job), lists a task's, reads their diffs in the job's worktree inside its sandbox, and decides and does an Undo: the worktree, the clone's branch and, when the round went out, the force-push.
- **Main** (`src/main/worktree.ts`, `src/main/device.ts`) opens a worktree in a program of the owner's and asks the owner, in its own dialog, before an Undo is signed.
- **The page** shows the panel and the menu, and asks; it never names a path or a program, and never signs.

### Rounds

- A **round** is one turn of an owner's job's agent that moved its worktree's HEAD: obid reads HEAD as the turn starts and as it ends, and keeps `{from, to, at}` when they differ. A turn that commits nothing is no round. Oby's own commits outside a turn (its rebase onto a moved base, its commit of work left uncommitted) are none either.
- A **task's rounds** are a job's own and those of the jobs it follows up on its branch (`followUpOf`, started from that job's branch), oldest first, numbered from 1. A round whose end isn't on the job's HEAD any more (a rebase rewrote it, the owner undid it) isn't listed.

### The Changes panel

- A task's card shows **Changes** once it has a worktree; a click opens the panel beside the conversation, as T3 Code's diff panel opens beside a thread. Its head names the task and the totals (`+N −M`), with **All rounds** or one **Round n** to pick, and **Ignore whitespace**.
- The panel lists the files first (path, added, deleted or changed, `+N −M`), then each file's unified diff, as git wrote it; a click on a file goes to its diff. A diff obid cut (`truncated`) says so.
- Picking a round, or Ignore whitespace, asks obid again (`?round=n`, `?whitespace=ignore`). While the task runs the panel reads again as its job changes.

### Open in Terminal, Open in <editor>

- The task's menu (⋯) offers **Open in Terminal** and **Open in <editor>** while its worktree is on disk. Main opens obid's worktree of that job, `<obid's home>/worktrees/<id>`, only when the job is one obid has and that folder is a real folder inside obid's worktrees (no link out); the page sends only the job's id.
- Terminal is the Mac's Terminal (`open -a Terminal <folder>`). The editor is the first of Cursor, VS Code and Zed that is installed in `/Applications` or the owner's own Applications folder (`open -a <app> <folder>`, as T3 Code's "Open in" picker offers them), else Finder. Never a program an agent, a job, or the page names.

### Undo this round

- The task's menu offers **Undo this round…** when obid has a sentence for it (`GET /v1/jobs/{id}/undo` answers 200). A click shows main's own dialog with obid's sentence, **Undo** and **Not now**; only Undo there signs it (`oby-undo`), as Publish by hand is signed (PUBLISHING-14).
- obid undoes only the job's last round, and only when: the job isn't running or queued; its pull request isn't merged; its worktree's HEAD is still that round's end; its branch in obid's clone is at that end or before its start (a later follow-up didn't carry it on); and what Oby pushed last to that branch is that round's end or comes before its start.
- It puts the worktree back at the round's start (`git reset --hard`, which drops work left uncommitted: the sentence says so), and the clone's branch too when it held the round. When the round went out, it first force-pushes the job's own `oby/` branch back to that start, leased on the round's end, so a push that fails changes nothing; the sentence says so, and no new commit leaves. A pushed round on a branch Oby didn't make isn't force-pushed: obid says to revert it there.
- The job counts fewer commits (and fewer pushed), its `outgoing` lists the push, the task lists one round fewer, and its summary opens with what was undone and where its branch is, which the next follow-up's agent hears with its report.

### Decisions and invariants

- **Nothing is undone on the page's say.** The signature is main's, after the owner's click in main's own dialog on obid's sentence read by main; obid checks it as it checks Publish's, for `oby-undo` only, once, within two minutes.
- **Only Oby's own branch is force-pushed, and only back.** The commit pushed is one already on the remote branch's history, so the secret scan has nothing new to read.
- **Add-only API.** `changes` gains `round` and `rounds`; `GET` and `POST /v1/jobs/{id}/undo` are new. No key or value changes.

### Failure and recovery

- No worktree (a queued job, one cleaned up): no files, no rounds, no Changes, no Open in, and Undo says why.
- The push fails (the remote moved, no network): obid answers `409` with git's words, and nothing local changed.
- The sentence changed between the menu and the click (the task ran again): main refuses and the menu reads it again.

## Interfaces and dependencies

`GET /v1/jobs/{id}/changes?path=&round=&whitespace=ignore`, `GET /v1/jobs/{id}/undo` (`{action, from, to, branch, push}` or `409`), `POST /v1/jobs/{id}/undo` (`{device, action, signedAt, signature}`, signed over `oby-undo\n<id>\n<action>\n<signedAt>`): [docs/api/jobs.md](../api/jobs.md). The app's IPC: `obid-undo` and `open-worktree` (main).

## Acceptance scenarios

- **CHANGES-1** A task of two rounds (a job that committed `hello.txt`, then the owner's words to it, its follow-up on the same branch, that committed `second.txt`) lists both rounds on the follow-up's `changes`, numbered 1 and 2, each with its job, `from`, `to` and `at`; the first job lists only its own. (`task_changes::changes_1_2_a_two_round_task_lists_its_rounds_and_each_rounds_diff`)
- **CHANGES-2** `?round=1` answers only `hello.txt` and its diff, `?round=2` only `second.txt`, a round the task doesn't have no files and no diff. (same test)
- **CHANGES-3** `?whitespace=ignore` leaves a re-indented file out of `files` and the diff, and keeps the task's other files; any other value is `422`. It holds on whatever git the Mac has: an older one (2.43) still lists a file only whitespace changed under `--ignore-all-space`, where 2.54 and later leave it out, with no count (2.43 prints no `--numstat` line for it), so obid leaves out a plain modification (`M`) that keeps its mode and is counted `0 0` or not at all; a file whose mode alone changed, an empty new file, a binary, an addition or a deletion stays listed. (`task_changes::changes_3_ignoring_whitespace_leaves_out_a_reindented_file`; `jobs::changes::tests::changes_3_whitespace_alone_is_left_out_on_any_git`, with each git's own output) (`task_changes::changes_3_ignoring_whitespace_leaves_out_a_reindented_file`)
- **CHANGES-4** The task card's **Changes** opens the panel: the files with their counts, then the diff; no worktree shows no Changes. (`taskChanges.test.tsx`, `CHANGES-4`)
- **CHANGES-5** Picking **Round 2** asks obid for `?round=2` and shows its files only; **Ignore whitespace** asks with `whitespace=ignore`; a click on a file goes to its diff. (`taskChanges.test.tsx`, `CHANGES-5`)
- **CHANGES-6** **Open in Terminal** runs `open -a Terminal` on `<obid's home>/worktrees/<id>`; **Open in <editor>** runs `open -a` with the first installed of Cursor, VS Code and Zed, else Finder's `open`. (`worktreeOpen.test.ts`, `CHANGES-6`)
- **CHANGES-7** Main opens nothing for an id that isn't one (`../x`, a path), a job obid doesn't have, a worktree that isn't a folder, or one that's a link out of obid's worktrees. (`worktreeOpen.test.ts`, `CHANGES-7`)
- **CHANGES-8** `GET /v1/jobs/{id}/undo` on the follow-up answers obid's sentence naming its branch, the commit it goes back to, how many commits leave and the force-push, with `from`, `to`, `branch` and `push`. (`task_changes::changes_8_10_11_undo_this_round_force_pushes_the_tasks_own_branch_back`)
- **CHANGES-9** A job with no round has nothing to undo (`409`, `Nothing this task did is left to undo.`); no such job is `404`. (`task_changes::changes_9_a_job_with_no_round_has_nothing_to_undo`)
- **CHANGES-10** The signed Undo force-pushes the task's `oby/` branch back to the first round's end, moves the clone's branch and the worktree there, counts no commit on the follow-up, lists the push in its `outgoing` and one round on the task; there's then nothing more to undo on it. (the CHANGES-8 test)
- **CHANGES-11** An Undo from an unregistered device, signed by another key, signed as `oby-publish`, for other words, or more than two minutes old is `403`, and nothing moves; a body that isn't an approval is `422`. (the CHANGES-8 test)
- **CHANGES-12** An Undo puts what was undone at the head of the job's summary (`The owner undid one of its rounds: its branch is back at <sha>, …`, one line however many rounds, the agent's report under it), so words to the task after it start a follow-up whose agent hears it with the report. (`jobs::changes::rounds::undo::tests::changes_12_a_follow_up_after_an_undo_hears_of_it`)
- **CHANGES-13** A round that went out to a branch the owner named, not an `oby/` one, isn't force-pushed back: `409` saying why, and the remote branch stays. (`task_changes::changes_13_a_round_pushed_to_a_branch_oby_didnt_make_isnt_force_pushed`)
- **CHANGES-14** The menu offers **Undo this round…** only while obid has a sentence for it; the click asks main with that sentence, and main signs only on **Undo** in its own dialog, after an input it saw, for the sentence it read from obid itself. (`taskChanges.test.tsx`, `CHANGES-14`; `device.test.ts`, `CHANGES-14`)
- **CHANGES-15** A file whose name isn't ASCII (`café.txt`) is named in the diff's header as `files` names it (`diff --git a/café.txt b/café.txt`), not in git's octal escapes, so a click on it in the list reaches its diff: every git call of obid's runs with `core.quotePath=false`. (`task_changes::changes_15_a_name_outside_ascii_is_named_in_the_diff_as_in_the_list`)

### Test it when it's done (a real Mac)

1. Give Claude Code a task in a scratch repository, then tell it a second thing once it's done: **Changes** on its card shows both files, **Round 1** and **Round 2** each its own.
2. **Open in Terminal** and **Open in Cursor** (or your editor) open the task's worktree.
3. **Undo this round…**: the dialog says it force-pushes the `oby/` branch; after **Undo**, GitHub's branch and the panel are back at the first round.
