# Working folders

The business behavior contract for the message box's **Select working folder…** (Sangam's design, [#1564](https://github.com/obi-ai-bot/oby/pull/1564)), from [the template](../business-contract-template.md). Scenarios are numbered `FOLDERS-n`; the tests that prove them are named after them (`folders_1_…`).

## Feature and status

- **Outcome:** The owner points Oby at any folder on this Mac (a repository they cloned, a project with no git, a new empty folder) and asks for work there, as they would with Claude Code in a terminal, without first listing a repository in `config.json` or putting it on GitHub. The work comes back into that folder: as a branch in its own repository, or as changed files.
- **Status:** **The boundary is implemented** (#1572, first pull request): `status.folders`, `folders` on `POST /v1/plans`, on conversations and on `PATCH /v1/conversations/{id}`, every path's validation, the gateway and relay refusals, and the Electron app sending them and showing obid's (FOLDERS-6's keeping and replacing, FOLDERS-7's refusals, FOLDERS-9, FOLDERS-10's canonical path, FOLDERS-11, FOLDERS-22; runner-api.md, "obid only: working folders"). **Proposed, not built yet:** everything from the brain's briefing on (the room's read-only folders, one task per folder, the three kinds, landing, checks, trust, the job's `folder` and `landed`, the failures table): until it lands, words in a conversation with folders start jobs in repositories as before. Decided by Wellington on 2026-10-29, answering Sangam's questions of 2026-10-28 (Slack). Sangam's design (PR #1564: `apps/electron/src/renderer/src/folders.ts`, `Composer.tsx`, `Home.tsx`, `live.ts`, `main/index.ts`'s `pick-folders`) is the spec for what the user sees.
- **Scope:** folders picked on this Mac, sent with the owner's words from the Electron app; obid validating, keeping and working in them; how the result lands back. **Out of scope:** folders on another Mac (Runner share is hidden for launch), picking a folder from the iPhone, a folder's uncommitted changes (v1, below), syncing a result back into a folder continuously, choosing a branch.

## What Sangam's design assumes (each one kept)

| The design | What obid promises |
|---|---|
| The Mac's own picker hands over absolute paths (`dialog.showOpenDialog`, `openDirectory`, `multiSelections`, `createDirectory`). | `folders` are absolute paths, as picked. obid canonicalizes them; it never asks the app for anything else. |
| Several folders per message, and per conversation. | `folders` is a list; a conversation keeps a list. |
| A conversation keeps its folders for every later message (its tags stay in the box). | obid keeps them on the conversation; a later message needn't send them again. The app's tags × a folder → `PATCH`. |
| Home's folders go to the conversation the words start (`Home.tsx`: `setFolders(id, att.folders)`). | `POST /v1/plans` with `new: true` and `folders` keeps them on the new conversation. |
| Any folder, a new empty one too (`createDirectory`); the app checks nothing about git. | obid accepts any folder that passes the boundary checks below, git or not, empty or not. |
| No branch, worktree or repository name is ever shown. | obid's job JSON has `folder {path, name}` for the app to show; `repo` and `branch` stay for the Swift apps and are never needed to understand the result. |
| The recent list (8, newest first) is the app's (`oby.recentFolders`). | obid keeps no recent list and offers none. |

## Behavior

### Actors and ownership

- **The owner** picks folders in the Electron app on this Mac and sends words with them.
- **The Electron app** sends the paths, shows each as a tag (its name, the path as the tooltip), shows the conversation's folders from obid once `status.folders` is true (dropping its own `oby.folders` copy, keeping the recent list), and shows a task's `folder` and `landed`. The first time it sees `status.folders`, the folders it kept for a conversation obid has none for are sent with `PATCH`, so nothing the owner picked before is lost. Words that start a conversation carry Home's folders; a conversation's own are changed with `PATCH` and aren't sent again with later words (a folder gone since would refuse them).
- **obid** validates every path, keeps a conversation's folders, tells the brain about them, starts one task per folder, works in a worktree apart, runs the folder's check and the secret scan, and lands the result. Every decision below is obid's.
- **The iPhone** (and the Watch) never adds a folder: it sends words to a conversation that has folders, and those are used.

### Triggers and prerequisites

- `GET /v1/status` answers `"folders": true`. Until then the app sends no `folders` (today's obid answers `400` to an unknown key: `Request` is `deny_unknown_fields`).
- The words come from the owner's own Mac app over loopback with the runner's token (never through the LAN gateway or the Sphere), and every folder passes validation.

### Validation at the boundary (every path, every time)

On `POST /v1/plans` and `PATCH /v1/conversations/{id}`, and again at every task's start, each path is:

1. absolute, and canonicalized (`realpath`: links resolved, `..` gone); the canonical path is what's kept;
2. there, and a directory;
3. none of: `/`; the person's home folder itself; obid's home or any folder holding it; `~/Library` or anything inside it; `~/.claude`, `~/.codex`, `~/.cursor` or anything inside them; any account's `configDir` or a folder holding one; `/System`, `/Library`, `/Applications`, `/usr`, `/bin`, `/sbin`, `/etc`, `/var`, `/private`, `/opt`, `/Volumes` itself, `/cores`, `/dev` and anything inside them (a folder on another volume, `/Volumes/X/…`, is fine);
4. compared case-insensitively on macOS (APFS and HFS+ ignore case by default), so `~/LIBRARY` is `~/Library`.

A path that fails is refused with `422` naming the folder by its name and why, in Sangam's words to come: `"Library" is where your Mac keeps its own settings, so Oby doesn't work there.` Nothing of the request happens (no turn, no conversation). The same path twice is kept once.

### What a conversation keeps

- `folders` on the conversation: canonical paths, in the order picked, at most 20. `POST /v1/plans` adds a request's `folders` to the conversation's (never removes); `PATCH /v1/conversations/{id}` `{"folders": [...]}` replaces the list (an empty list clears it).
- The brain's briefing names the conversation's folders by name and path. Its room gets each one read-only, as `room/repos` holds repository checkouts today (`Spec.frozen`), so it can read them.
- Every task the brain starts in a conversation with folders works in them: **one task per folder, in parallel**, each its own job with its own worktree, check and result. Words that concern one folder ("only in the API") start one task; the brain decides which, as it picks repositories today.

### Which kind of folder, decided at each task's start

| The folder | Where the job works | How the result lands (`landed`) |
|---|---|---|
| **A git repository whose `origin` is GitHub and gh can push to it** | As today's repositories: obid's clone of GitHub (`home/clones/<owner>/<name>`), `repo` `owner/name`, based on the repository's default branch. The folder itself isn't read for the job. | As today: publish rules, the pull request, Oby's review, the learned workflow. `landed` stays null; the pull request is the result. The folder is never touched. |
| **Any other git repository** (no remote, another host, or a GitHub repository the owner can't push to) | obid's own clone of the folder's repository (`home/clones/local/<slug>`, made with `git clone --local --no-checkout` from the folder, fetched again at each start), based on the folder's `HEAD` commit. Never a `git worktree` in the folder's own `.git`. | A branch `oby/<task>` in the folder's own repository, pointing at the job's last commit: `{"how": "branch", "branch": "oby/fix-login"}`. Its `HEAD`, index, working files and every other branch stay as they were. |
| **A plain folder** (no `.git`), an empty one too, or a repository with no commit yet | A snapshot of the folder in obid's own git (`home/folders/<slug>/snapshot.git`, `GIT_WORK_TREE` the folder; the folder is never `git init`ed and gets no `.git`), one commit per task start; the job's worktree is made from it. | The job's changes are applied into the folder's files, merged three ways against the snapshot it started from: `{"how": "files", "count": 7}` (files written, added or removed). |

The job's `repo` is `owner/name` for GitHub, else `local/<slug>` (the folder's name, lower-cased to `[a-z0-9._-]`, `-2`, `-3` … when taken by another folder), which passes `owner/name` everywhere it's checked. `<task>` is the job's own branch name today (`oby/<kind>-<id>`), made unique in the folder's repository; an existing branch is never moved or overwritten.

### States and transitions

A folder task is an ordinary job: `queued`, `running`, `done`, `failed`, `stopped`, with today's `step` and pipeline. Only these differ:

- **Before the agent starts:** the path is validated again, the folder's kind is decided, and the clone or snapshot is made (`step` `Getting <name> ready`).
- **After it's done with commits:** its check runs (below), then the secret scan of everything that would land, then it lands. `step` `Bringing the changes into <name>`. `landed` is set once it has.
- **A clash on apply (plain folder):** a file the job changed was also changed in the folder since the snapshot, and the three-way merge can't settle it. Nothing of that file is written; files that merged cleanly are written. The job stays `done` with `landed` null, and its `question` asks, as an inbox item (`question`, which every app decodes) shown in Home's AskStack: `"index.ts changed in api while Oby worked on it. Keep yours, take Oby's, or keep both?"` with the options `Keep yours`, `Take Oby's`, `Keep both` (Oby's beside it as `index.oby.ts`). One question per job, listing every clashing file. The answer finishes the landing.
- **Uncommitted changes in a git folder** aren't part of v1: the job starts from the folder's `HEAD` commit, and its report says so (`api had changes that aren't committed; Oby worked from your last commit.`). A plain folder has no such case: its snapshot is what's on disk.
- **The owner removes a folder from the conversation** while its task runs: the task goes on and lands; later words don't start tasks there.

### Checks per folder

- A GitHub folder: the repository's `check` as today (`config.json` `repos`).
- A local folder: the check the owner gave it (said once in words, remembered as the folder's learned workflow after "Can I remember that?"; proposed: kept in `home/folders.json` with the slug), else none. A job with no check lands without one, as a repository without a check publishes today; its report says `No check ran: api has none yet.`
- A local folder's check never gets declared secrets (`secretspec.toml`): obid can't tell whose commits a local repository holds.

### Trust

- **A folder the owner picked counts as listed** (proposed): as one of `config.json`'s `repos`. Picking it is the owner's own act, so a job in it isn't marked `untrusted` for being there, and runs in the owner's mode.
- **Its contents are still other people's words.** The brain reading the folder in its room taints the turn, as reading `room/repos` does today; jobs that turn starts start marked `untrusted` (the owner's mode still applies, as since 2026-10-27). The agent gets the folder's files only in its worktree, never in a prompt.
- **A local repository counts as someone else's code for secrets:** no declared secrets for its checks (above).

### Failures and recovery

| What happens | What obid does | What the owner sees |
|---|---|---|
| The folder is gone or was moved when a task starts | The task fails before anything runs; the folder stays on the conversation. | `failed`: `api isn't at ~/Projects/api any more. Pick it again where it is now.` Sangam designs how the tag looks (asked). |
| The folder is gone or moved when the result lands | Nothing is written; the result stays (branch in obid's clone, or the job's commits). | `done`, `landed` null, a `question`: `api isn't at ~/Projects/api any more. Pick where it is now, or discard Oby's changes?` |
| Unreadable (permissions, macOS privacy: Desktop, Documents, removable volumes without access) | The task fails; nothing is copied. | `failed`: `Oby can't read api: macOS hasn't let it in. Allow it in System Settings › Privacy & Security › Files and Folders.` |
| The path was swapped for a link, or now resolves elsewhere | Validated again at start and before landing: a canonical path that differs from the kept one, or one now refused, stops it. | `failed` or the landing question, as for a moved folder. obid never follows a link out of the folder when snapshotting or applying (`O_NOFOLLOW`, every file opened relative to the folder's own descriptor). |
| A clash on apply | As above: clean files written, a question for the rest. | The AskStack question. |
| The folder's repository moved its `HEAD`, or has an `oby/<task>` branch already | The branch is made with the next free name; nothing existing changes. | `landed.branch` is the name used. |
| obid or the Mac restarts mid-task | As any job: picked up again; a folder task re-validates its folder first. A landing cut short is done again from the start: writing a branch is all or nothing (`update-ref` with the old value null), and applying files writes each through a temporary file and a rename, recording what's done in the job so a second run skips it. | Nothing new; at worst the landing question again. |
| The same words sent twice (same `id`) | Nothing new, as today: no second set of tasks. | — |
| A folder's snapshot would be too big | Refused before the agent starts (proposed: over 2 GB or 200,000 files, after `.gitignore`). | `failed`: `api is too big for Oby to copy (3.1 GB). Pick a smaller folder.` |
| The secret scan finds something, or can't run | Nothing lands. | `done`, `landed` null, and why, as publishing says today. |

### Invariants and boundaries

- The agent always works in a worktree apart, inside Oby's box: never in the folder itself. The box, the check, the secret scan and rollback hold as for any job.
- obid never changes the folder's repository but by adding a branch: never its `HEAD`, index, working files, config, hooks or other refs. A plain folder is never `git init`ed and gets no file of obid's.
- No hook of the folder's repository ever runs: obid writes the branch with its own git (`core.hooksPath=/dev/null`, `core.fsmonitor=false`, `protocol.ext.allow=never`, every call), copying objects with `fetch` run against the folder's repository rather than `push` into it, so no `receive-pack` hook (`pre-receive`, `update`, `post-receive`, `reference-transaction`) runs there.
- Nothing lands before the job's check passed (when it has one) and the secret scan of what would land is clean.
- Only the owner's own Mac app adds folders. A paired device, another Oby, a job, the brain or text anywhere never does: through the gateway or the Sphere, `folders` on `POST /v1/plans` is refused (`422`), and `PATCH` may only remove.
- Disk: a git folder's clone shares its objects with the folder's repository on the same volume (`--local` hard links), so it costs about one checkout; on another volume it's a full copy. A plain folder's snapshot is a full copy, compressed. Both go with obid's usual tidying (`status.cleanup`) once no task or conversation uses them.

## Interfaces and dependencies

All add-only. No new value in any enum the Swift apps decode as closed: job `status`, `waiting.kind`, inbox `kind` and the pipeline's stage states stay as they are.

- `GET /v1/status`: `"folders": true` once obid takes them. Older obids don't send the key.
- `POST /v1/plans`: `"folders": ["/Users/me/Projects/api", "/Users/me/Notes"]`, optional. With `new: true` they go on the new conversation; otherwise they're added to the conversation's.
- `GET /v1/conversations` and the `conversation` event: each conversation has `"folders": [...]` (canonical paths), `[]` when none.
- `PATCH /v1/conversations/{id}`: `{"folders": [...]}` alongside today's `title` and `pinned`.
- Job JSON: `"folder": {"path": "/Users/me/Projects/api", "name": "api"}` or null; `"landed": {"how": "branch", "branch": "oby/build-ab12cd34"}`, `{"how": "files", "count": 7}` or null. `repo` may be `local/<slug>`.
- Persistence: the conversation's folders with the conversation; `home/folders.json` for each folder's slug and remembered check; `home/clones/local/<slug>` and `home/folders/<slug>/snapshot.git`.
- macOS: reading a folder in Desktop, Documents, Downloads, iCloud Drive or a removable volume needs obid to have been let in (Files and Folders); obid can't ask for it itself.

**Still Sangam's to design** (asked): the task card's meta line for a folder task and the words for its result (`landed`: "On branch oby/… in api" or "7 files changed in api"); how a missing or refused folder's tag looks in the box; the clash question's look in the AskStack. obid's words above are placeholders until he does.

**Open for the owner:** whether a local folder's check is learned only in words or also from a file in the folder; the snapshot's size limit.

## Acceptance scenarios

- **FOLDERS-1, the normal path (plain folder).** Given `~/Notes` with no `.git`, the owner picks it on Home and sends "tidy up the headings". A new conversation keeps `~/Notes`; one task runs; once done, its changed files are in `~/Notes`, `landed` `{"how": "files", "count": n}`, and `~/Notes` has no `.git`.
- **FOLDERS-2, a local git folder.** Given `~/Projects/api`, a git repository with no remote on branch `main`, a task there ends with commits: `~/Projects/api` has a new branch `oby/<task>` at the job's last commit; its `HEAD` is still `main`, its working files and index are unchanged, `git worktree list` shows only its own.
- **FOLDERS-3, a GitHub folder.** Given a folder whose `origin` is a GitHub repository gh can push to, the task publishes a pull request as today's repository jobs do, `repo` is `owner/name`, and the folder is unchanged.
- **FOLDERS-4, a GitHub folder the owner can't push to** is treated as a local git folder (FOLDERS-2), and its check gets no declared secrets.
- **FOLDERS-5, several folders.** With `~/Projects/api` and `~/Projects/web` on a conversation, "rename the User type everywhere" starts two tasks at once, one per folder, each with its own `folder` and `landed`.
- **FOLDERS-6, kept for later messages.** After FOLDERS-5, a second message sent with no `folders` works in both again; after `PATCH {"folders": ["…/web"]}`, the next one works only in `web`.
- **FOLDERS-7, from the iPhone.** Words sent from the paired iPhone to that conversation use its folders; a request from the iPhone carrying `folders` is refused `422`, and its `PATCH` can remove a folder but not add one.
- **FOLDERS-8, a new empty folder.** A folder made in the picker and picked empty: the task runs and its files land in it (`how: files`).
- **FOLDERS-9, refused paths.** `/`, `~`, `~/Library/Mobile Documents`, `~/.claude`, obid's home, a folder holding obid's home, an account's `configDir`, `/usr/local`, a file, a relative path and a missing path are each refused `422` with the folder's name and why, and nothing is kept; `~/LIBRARY` is refused as `~/Library`.
- **FOLDERS-10, a link.** `~/work` is a link to `~/Projects/api`: `~/Projects/api` is kept. A folder swapped for a link to `~/Library` after it was kept fails at the next start, and nothing is read through it.
- **FOLDERS-11, gate.** With an obid whose `status` has no `folders`, the app sends none and keeps them itself, as #1564 does; with `"folders": true`, it sends them and shows obid's.
- **FOLDERS-12, uncommitted changes.** A git folder with uncommitted edits: the task starts from `HEAD`, its report says the changes weren't included, and the edits are still in the folder, untouched, after it lands.
- **FOLDERS-13, folder gone at start.** The folder was deleted after it was kept: the next task fails with `<name> isn't at <path> any more…`, nothing runs, and the folder stays on the conversation.
- **FOLDERS-14, folder moved before landing.** The folder is moved while its task runs: nothing is written, the job is `done` with `landed` null and asks where it is now, or to discard.
- **FOLDERS-15, unreadable.** A folder obid isn't let into (macOS privacy): the task fails saying how to allow it; nothing is copied.
- **FOLDERS-16, a clash.** The owner edits `notes.md` in a plain folder while Oby changes it too: the other files land, `notes.md` doesn't, and the AskStack asks Keep yours / Take Oby's / Keep both; each answer leaves the file as it says, and `landed` is set after.
- **FOLDERS-17, a taken branch name.** The folder's repository already has `oby/<task>`: the result lands as `oby/<task>-2`, and the existing branch is unchanged.
- **FOLDERS-18, hooks never run.** A git folder with `pre-receive`, `post-receive`, `reference-transaction` and `post-checkout` hooks that write a marker file: after a task lands there, no marker exists.
- **FOLDERS-19, the scan.** A task whose changes hold a secret lands nothing, and says why.
- **FOLDERS-20, restart mid-landing.** obid stops halfway through applying files: on restart the landing finishes once, with no file written twice and none half-written.
- **FOLDERS-21, trust.** A task in a picked folder that the brain hadn't read starts in the owner's mode, not `untrusted`; after the brain reads a file in that folder, tasks the turn starts are `untrusted`.
- **FOLDERS-22, the same words twice.** Words with folders sent again with the same `id` start no second set of tasks.
- **FOLDERS-23, too big.** A folder over the snapshot limit fails before the agent starts, saying its size.

Real-Mac "Test it when it's done": pick a new empty folder, a cloned repository and a GitHub repository in one message from Home; see three tasks; confirm the first two folders' results with Finder and `git branch`, and the third's pull request; quit and reopen the app mid-task (FOLDERS-20).
