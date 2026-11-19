# Projects: a named set of folders a conversation belongs to

The business behavior contract for projects, from [the template](../business-contract-template.md). Tasks A00-70 and A00-73 (obid), and A00-71 (the Electron app). Scenarios are numbered `PROJECTS-n`; the tests that prove them are named after them (`projects_<n>_…` in `core/crates/obid/tests/projects_api.rs`, `core/crates/obid/tests/gateway_api.rs`, `core/crates/obid/src/projects.rs` and `core/crates/obid/src/job.rs`; the app's, `projects_16_…` to `projects_31_…`, in `apps/electron/src/renderer/src/test/projects.test.tsx`). Its API is [docs/api/projects.md](../api/projects.md).

## Feature and status

- **Outcome:** The owner names a set of folders and GitHub repositories once (a project, such as a product made of several repositories) and puts conversations in it. A conversation in a project works in the project's folders, so each one says which product it is for, and a change to the project's folders reaches every conversation in it.
- **Status:** **Implemented** in obid (A00-70): `home/projects.json`, `GET`/`POST /v1/projects`, `PATCH`/`DELETE /v1/projects/{id}`, a conversation's `project`, `project` on `PATCH /v1/conversations/{id}` and `POST /v1/plans`, the start's reconcile, and `projects-v1` in `capabilities`. A00-73 added a job's `project`, `GET /v1/jobs?project=<id>` and `project-jobs-v1` in `capabilities`. No app shows a job's project yet. **Implemented** in the Electron app (A00-71, screens 01 to 03 of `docs/design/oby-home/`): the side panel's Projects side lists conversations under their project, then No project, with New project at its foot (`Sidebar.tsx`); a project chip with its menu in Home's box (`Composer.tsx`) and in a conversation's title bar (`Home.tsx`), and the New project sheet (`ProjectSheet.tsx`), on the projects store (`projects.ts`) and `moveToProject` (`live.ts`); main signs words with `project` only as WORDS-18 says ([signed-words.md](signed-words.md)). The same sheet edits a project and deletes one (PROJECTS-25 to 31), from the title bar's chip and a project's heading in the side panel; a conversation in a project shows no folder in its box.
- **Scope:** a project's store and routes, a conversation's membership, and what happens to a conversation when its project changes or goes.
- **Out of scope:**
  - Words of Sangam's for editing or deleting a project: his screens draw neither, so the app reuses his New project sheet (screen 02) with "Edit project", "Save" and "Delete project…", and obid's refusal texts are placeholders, shown as they are.
  - Anything on screen of a project's jobs: the factory beside the conversation and in full (Sangam's screens 05 and 06).
  - The factory's stages or settings: the stages are a job's `pipeline`, as it is.
  - A project deciding where a job runs, and moving a job between projects: a job's `project` is set by obid alone, as it starts.
  - Moving or copying projects between profiles. Projects are kept per profile ([profiles.md](profiles.md), A00-79): each profile's in `projects.json` under its root, the default profile's still `home/projects.json`, and a request names its profile with `Oby-Profile`. Conversations are still one per Mac, so a conversation of any profile may be in another profile's project, and the start's reconcile reads every profile's projects.
  - `repos` deciding where a job runs, any call to GitHub, or a check that a repository exists: `repos` are kept and answered only.
  - Folders reaching jobs or the brain's briefing, still proposed in [folders.md](folders.md).
  - A `project` event, a cap on projects or repositories, unique names, and any order but the order made.

## Behavior

### Actors and ownership

- **obid** owns every decision: what a project is, which folders a conversation answers, what a delete does to its conversations, and the reconcile at start. It keeps projects in `home/projects.json` (0600) and each conversation's membership in its own file.
- **The owner's Mac app** shows and sends only. It shows obid's projects (`GET /v1/projects`, read once the status lists `projects-v1`, again as a chip's menu opens and after a create) as the side panel's groups and the chips' menus, each conversation under the project its `project` names (none, or one not listed: No project). It sends `POST /v1/projects` (the New project sheet: exactly `name`, `folders` picked in main's own dialog, and `repos` from `GET /v1/repos`), `PATCH /v1/projects/{id}` (the same sheet, editing: only the keys that changed, nothing when none did), `DELETE /v1/projects/{id}` (after the sheet asks), `project` with words that start a conversation from Home (`POST /v1/plans` with `new: true`), and `PATCH /v1/conversations/{id} {"project": …}` from a conversation's title bar. It keeps nothing of its own but which groups are folded (`oby.projectsFolded`); with an obid without `projects-v1` it shows none of this and sends no `project`.
- **A paired device, the phone relay, and other Obys** reach no project route, and may only take a conversation out of its project.

### Triggers and prerequisites

- A project is `{id, name, folders, repos}`:
  - `id`, a lower-case UUID obid makes;
  - `name`, trimmed, one line of 1 to 100 characters, as a conversation's title is; two projects may share one;
  - `folders`, canonical paths, each checked as [folders.md](folders.md) checks a conversation's (absolute, there, a folder, none of the places Oby never works in), kept once each, at most the owner's `limits.folders`;
  - `repos`, `owner/name` strings, each kept once (any case), never looked up on GitHub.
- A conversation joins a project by `PATCH /v1/conversations/{id}` `{"project": <id>}`, or starts in one by `POST /v1/plans` with `"new": true` and `"project": <id>`.

### States and transitions

- **Not in a project.** `project` is null, and the conversation answers its own folders, as [folders.md](folders.md) says.
- **In a project.** `project` is the project's id, and `folders` answers the project's folders. Its own list is kept underneath, untouched, and its own folders aren't set while it's in one: `folders` on `PATCH`, and `folders` with words to it, are refused.
- **Taken out** (`{"project": null}`). It answers its own list again, as it was before it joined.
- **Its project's folders change.** It answers the new folders, its revision one higher.
- **Its project is deleted.** It leaves the project, and the folders it was working in (the project's) become its own list: the folders it answers don't change.
- The rule for whose folders it answers after each of these is one function, `next` in `core/crates/obid/src/projects.rs`, proven by Kani (PROJECTS-5, PROJECTS-8).
- Every conversation that changes is written, its revision raised, and sent as a `conversation` event (EVENTS-7).
- **A job's project.** A job's `project` is set once, as it starts (`Job::start_in`), from the project its conversation is in then, read from that conversation's file; null when the conversation is in none, or there is none. It never changes after: not when the job joins a conversation later, nor when its conversation leaves or changes project, nor when the project is deleted, nor on a restart (PROJECTS-13).
- **A project's jobs.** `GET /v1/jobs?project=<id>` answers the jobs whose `project` is that id, never a job in no project (`in_project` in `core/crates/obid/src/projects.rs`, proven by Kani), queued and running first, then newest (PROJECTS-14).

### Failures and recovery

| What happens | What obid does | What the owner sees |
|---|---|---|
| A refused value (a folder, a name, a repository) | `422` with the reason, nothing kept | The reason |
| An unknown project id on a project route | `404` | — |
| A project of another profile than the one the request names | `404` on a project route, `422` on a conversation or plan, nothing changed | The reason |
| `Oby-Profile` names no profile on this Mac, is empty or not text, or is sent twice | `400`, nothing read or written, never the default profile ([profiles.md](profiles.md)) | — |
| An unknown project id on a conversation or plan | `422`, nothing changed, no conversation made | The reason |
| `projects.json` can't be written | `500`, the change undone in memory | The reason |
| A conversation can't be written on a delete | `500`; that conversation is put back, and the project stays | The project still listed; `DELETE` again finishes it |
| obid stops partway through a delete | At start, every conversation whose project is gone leaves it, keeping its folders; the project, if still kept, is listed with no conversation | The project still listed, if it was; `DELETE` again removes it |
| obid stops after a project's folders changed, before a conversation was written | At start, each conversation in a kept project gets that project's folders now | Its current folders |
| `projects.json` doesn't read | Logged and left as it is; every project route answers `500` naming the file; nothing writes over it; the start's reconcile changes no conversation | The reason |

### Invariants and boundaries

- No conversation points at a project that isn't kept, once obid has started (the reconcile) and after every route: a delete takes conversations out before it removes the project.
- A delete never changes the folders a conversation answers.
- A conversation's own list changes only by the owner's own `folders` (while in no project) or by its project's delete.
- One lock holds a project's membership: every project route, and putting a conversation in a project, run under the projects' lock, taken before the conversations' and never while holding it.
- Only the owner's Mac app creates, changes or deletes a project, or puts a conversation in one: the routes aren't on the gateway's list, and through the gateway or the relay `project` may only be null.
- The API stays add-only: new routes, a new key on the conversation, a new capability; no closed enum gains a value.
- A job's `project` is set once as it starts, from its conversation's project then, and never changes.

## Interfaces and dependencies

- `GET /v1/projects`, `POST /v1/projects`, `PATCH /v1/projects/{id}`, `DELETE /v1/projects/{id}` ([docs/api/projects.md](../api/projects.md)).
- A conversation's `project` (an id or null, always sent) in `GET /v1/conversations`, search and the `conversation` event; `folders` is the project's while `project` is set ([docs/api/conversations.md](../api/conversations.md)).
- `project` on `PATCH /v1/conversations/{id}` and on `POST /v1/plans`.
- `projects-v1` in `capabilities` ([capabilities.md](capabilities.md)): an app sends `project` only to an obid that lists it, since `POST /v1/plans` and `PATCH` refuse unknown keys.
- A job's `project` (an id or null, always sent) in `GET /v1/jobs`, `GET /v1/jobs/{id}`, a plan's `jobs` and the `job` event ([docs/api/jobs.md](../api/jobs.md)), kept in `home/jobs/<id>.json`.
- `GET /v1/jobs?project=<id>`: that project's jobs, queued and running first ([docs/api/jobs.md](../api/jobs.md)).
- `project-jobs-v1` in `capabilities` ([capabilities.md](capabilities.md)): an older obid ignores `project=` and answers every job, so an app trusts the filter only from an obid that lists it.
- `home/projects.json` (0600), `{"projects": [...]}`, and `project` and `projectFolders` in `home/conversations/<id>.json`.
- The design is checked by the Quint model `docs/models/projects.qnt` (delete, crash and restart).

## Acceptance scenarios

The app's (A00-71) are PROJECTS-16 to 31, each a test of the same name in `apps/electron/src/renderer/src/test/projects.test.tsx`; obid's are PROJECTS-1 to 15.

- **PROJECTS-1, create and list (implemented).** `POST /v1/projects {"name": " Oby ", "folders": [<a picked folder>], "repos": ["obi-ai-bot/oby"]}` answers `201` with a UUID `id`, `name` `"Oby"`, the canonical folder and the repository; `GET /v1/projects` lists it; after obid restarts on the same home it is still listed, and `home/projects.json` is 0600. Test: `projects_1_a_project_is_made_listed_and_kept`.
- **PROJECTS-2, refused input (implemented).** A relative path, a missing path, `~/.ssh`, obid's home, more than `limits.folders` folders, a repository `"a/b/c"` or `"../x"`, an empty name, a 101-character name, a name with a line break: each `422`; an unknown key `400`; `GET /v1/projects` unchanged after each. The same folder twice is kept once. Test: `projects_2_refused_input_keeps_nothing`.
- **PROJECTS-3, change a project (implemented).** `PATCH` with only `name` changes only the name; with `folders` it replaces them, and a conversation in the project answers the new folders, its `revision` one higher; `{}` is `422`, an unknown id `404`, a refused folder `422` with nothing changed. Test: `projects_3_a_change_reaches_its_conversations`.
- **PROJECTS-4, put a conversation in a project (implemented).** A conversation with own folders `[A]`, `PATCH {"project": <id>}` (project folders `[B, C]`) answers `"project": <id>` and `"folders": [B, C]`, as `GET /v1/conversations` does; one with no project answers `"project": null`; an unknown id is `422` with nothing changed. Tests: `projects_4_a_conversation_put_in_a_project_answers_its_folders`, `projects_4_assign_answers_the_project`.
- **PROJECTS-5, take it out again (implemented).** After PROJECTS-4, `PATCH {"project": null}` answers `"project": null` and `"folders": [A]`. Tests: `projects_5_taken_out_it_answers_its_own_folders_again`, `projects_5_take_off_answers_the_own_list`, and the Kani harness `projects_5_taking_a_project_off_keeps_the_own_list`.
- **PROJECTS-6, no own folders while in a project (implemented).** For a conversation in a project, `PATCH {"folders": [D]}` is `422`, and words with `folders: [D]` to it are `422`; its folders and turns are unchanged. `PATCH {"project": null, "folders": [D]}` is `200` and answers `"folders": [D]`. Test: `projects_6_no_own_folders_while_in_a_project`.
- **PROJECTS-7, start a conversation in a project (implemented).** `POST /v1/plans {"said": "…", "new": true, "project": <id>}` starts a conversation answering that project and its folders; `project` without `new` is `422`, and an unknown id `422`, with no conversation made; the same words sent again with the same `id` are `repeated`. Test: `projects_7_a_conversation_starts_in_a_project`.
- **PROJECTS-8, delete a project (implemented).** With two conversations in a project with folders `[B, C]`, `DELETE` answers `204`; both conversations remain with `"project": null`, `"folders": [B, C]` and their revision raised; the project is gone from the list; a second `DELETE` is `404`. Tests: `projects_8_deleting_a_project_keeps_its_conversations_folders`, `projects_8_project_gone_keeps_the_folders`, the Kani harness `projects_8_a_gone_project_leaves_its_folders`, and the model's `deleteKeepsConversations`.
- **PROJECTS-9, restart partway (implemented).** Written into a stopped obid's home: (a) a conversation naming a project `projects.json` doesn't hold, its `projectFolders` `[B]`, answers `"project": null` and `"folders": [B]` after start; (b) one whose `projectFolders` differ from its kept project's answers the project's current folders; (c) a project kept with no conversation after a cut-short delete is listed, and `DELETE` removes it. Test: `projects_9_a_restart_partway_leaves_no_conversation_in_a_gone_project`, and the model's `oneProjectOrNone`.
- **PROJECTS-10, Mac only (implemented).** Through the gateway with a paired device's token, and through the relay: the four project routes are `404` "That route isn't available through the gateway."; a conversation `PATCH {"project": <id>}` is `422`; `PATCH {"project": null}` is `200`; `POST /v1/plans` with `project: <id>` is `422`. Tests: `projects_10_a_paired_phone_only_takes_a_conversation_out`, `projects_10_a_phone_on_the_relay_only_takes_a_conversation_out` in `core/crates/obid/tests/gateway_api.rs`.
- **PROJECTS-11, capability (implemented).** `GET /v1/status` `capabilities` holds `projects-v1` and is still sorted; CAPS-1 still passes. Test: `projects_11_status_lists_projects_v1`.
- **PROJECTS-12, an unreadable file (implemented).** With `home/projects.json` holding `not json` and a conversation naming a project: after start that conversation is unchanged on disk, every project route is `500`, and `projects.json` still holds `not json`. Tests: `projects_12_an_unreadable_file_changes_nothing`, `projects_12_an_unreadable_file_is_left_as_it_is`.
- **PROJECTS-13, a job's project (implemented).** Given a project P and a conversation started in it (`POST /v1/plans` with `"new": true, "project": P`) whose turn dispatches a job, that job answers `"project": P` in the plan's `jobs`, `GET /v1/jobs`, `GET /v1/jobs/{id}` and its `job` event. A job dispatched from a conversation in no project answers `"project": null`, and so does a job from `POST /v1/jobs` with no `conversation`. After that conversation is taken out of P, P is deleted and obid restarts on the same home, the first job still answers `"project": P`. Tests: `projects_13_a_job_keeps_the_project_it_started_in` in `core/crates/obid/tests/projects_api.rs`, `projects_13_a_job_takes_its_conversations_project_as_it_starts` and `projects_13_a_job_kept_before_projects_has_none` in `core/crates/obid/src/job.rs`.
- **PROJECTS-14, a project's jobs (implemented).** Given jobs in P (one queued or running, older than two finished ones), one in Q and one in no project, `GET /v1/jobs?project=P` answers exactly P's three, the active one first, then newest first, and `?project=P&limit=1` answers only the active one; `?project=<an unknown id>` and `?project=` answer `200 []`; `GET /v1/jobs` without `project` answers every job, newest first, as before; `?project=P&limit=x` answers `422` `limit is a number.`. Tests: `projects_14_get_jobs_by_project` in `core/crates/obid/tests/projects_api.rs`, `projects_14_a_projects_jobs_come_running_first` and `projects_14_in_project_only_its_own` in `core/crates/obid/src/projects.rs`, and the Kani harness `projects_14_a_job_is_in_only_its_own_project`.
- **PROJECTS-15, capability (implemented).** `GET /v1/status` `capabilities` holds `project-jobs-v1` and is still sorted; CAPS-1 and CAPS-2 still pass. Test: `projects_15_status_lists_project_jobs_v1`.
- **PROJECTS-16, no projects (implemented).** With `projects-v1` and `GET /v1/projects` answering `[]`: the Projects side lists the conversations as before, with no "No project" heading; "New project" is the last row; Home's chip reads "No project". Test: `projects_16_no_projects`.
- **PROJECTS-17, one project (implemented).** One project "Oby app", one conversation in it and one with `project: null`: the panel shows the "Oby app" heading (a folder mark) with its row, then "No project" with the other, then "New project"; each row keeps its second line and its marks. Test: `projects_17_one_project`.
- **PROJECTS-18, several, folding and search (implemented).** Oby app and Tetrix, conversations in each (one pinned), one in none and one naming an unlisted id: the groups in obid's order, then No project holding the unlisted one, the pinned row first in its group. Folding Tetrix hides its rows and writes `oby.projectsFolded`; shown again, Tetrix is still folded. A search matching only a row in Tetrix shows Tetrix alone, open. Test: `projects_18_several_folding_and_search`.
- **PROJECTS-19, the menu (implemented).** Home's chip lists "Oby app · 2 repos" and "Tetrix · 3 repos" in obid's order, "No project" (checked), then "New project…". After choosing Tetrix the chip reads Tetrix, the box drops the folders it had picked and offers no "Select working folder…", and sending sends `POST /v1/plans` with `new: true`, `project: <Tetrix id>` and no `folders`. Test: `projects_19_the_menu`.
- **PROJECTS-20, the sheet (implemented).** "New project" opens the sheet with its words, "Create project" disabled until a name. "Add a folder…" (main's dialog) shows a row "On this Mac"; "Add from GitHub…" lists `GET /v1/repos`, and the one chosen shows a row "GitHub" (none listed: it says "No repositories to add", and the sheet stays as it was); `×` removes a row. "Create project" sends exactly `{name, folders, repos}`; on `201` the sheet closes and the project is a heading in the panel and in the chip's menu. Opened from Home's chip, the new project is the chosen one. "Cancel" sends nothing. Tests: `projects_20_the_sheet`, `projects_20_add_from_github_with_none_listed_says_so`.
- **PROJECTS-21, a refused create (implemented).** `POST /v1/projects` answers `422 {"error": "…"}`: obid's sentence shows in the sheet, which stays open with its name and rows, and the projects listed are unchanged. Test: `projects_21_a_refused_create`.
- **PROJECTS-22, the title bar's chip (implemented).** A conversation in Oby app is open: its title bar shows the chip "Oby app". Choosing Tetrix sends `PATCH /v1/conversations/{id} {"project": <Tetrix id>}`, and "No project" `{"project": null}`; the chip shows what obid answered, and on a `422` obid's words show and the chip stays as it was. Its box offers no "Select working folder…" (and shows no folder: PROJECTS-25). A helper's conversation shows no chip. Test: `projects_22_the_title_bar_chip`.
- **PROJECTS-23, an obid without projects (implemented).** The status has no `projects-v1`: no heading, no "New project" row, no chip in the box or the title bar; `GET /v1/projects` is never called; Home's words go without `project`. Test: `projects_23_an_obid_without_projects`.
- **PROJECTS-24, the mock (implemented).** On the mock (`?everything&chat`) the panel shows Oby app, Tetrix and No project with conversations under each, the chip's menu shows "2 repos" and "3 repos", a conversation in Oby app has the chip beside its title, and "New project" opens the sheet. Test: `projects_24_the_mock`.
- **PROJECTS-25, no folder in a project's box (implemented).** A conversation in Oby app is open: its title bar's chip names the project, and its box shows no folder tag (no tag row at all, with nothing attached) and no project chip; words to it go with no `folders`. A conversation in no project shows its own folders in its box, each with its ×, and offers "Select working folder…", the one place they are picked. Home's box keeps its project chip (PROJECTS-19): Home has no title bar to show it. Test: `projects_25_no_folder_chip_in_a_projects_box`.
- **PROJECTS-26, edit opens filled in (implemented).** A project's heading in the side panel has a ⋮ on hover (and its right-click), none on No project, whose menu is "Edit project…", then "Delete project…". "Edit project…" opens the New project sheet titled "Edit project", its name, its folders ("On this Mac") and its repositories ("GitHub") filled in, its actions "Delete project…", "Cancel" and "Save". "Save" with nothing changed, and "Cancel", send nothing. The title bar's chip menu, for a conversation in a project, has "Edit project…" before "New project…", opening the same sheet on that project; for one in no project it has none. Test: `projects_26_edit_opens_filled_in`.
- **PROJECTS-27, add a repository (implemented).** From the title bar's chip, "Edit project…", "Add from GitHub…" and a repository chosen: "Save" sends `PATCH /v1/projects/{id}` with only `repos`, the old ones then the new; the sheet closes, and the chip's menu shows the project's new count. Test: `projects_27_add_a_repo`.
- **PROJECTS-28, remove a repository (implemented).** A repository's × in the sheet takes its row out; "Save" sends `PATCH` with only `repos`, without it; the chip's menu shows one fewer. Test: `projects_28_remove_a_repo`.
- **PROJECTS-29, add and remove a folder (implemented).** "Add a folder…" (main's own dialog) adds a row "On this Mac", and the old folder's × takes it out; with the name changed too (trimmed), "Save" sends `PATCH` with `name` and `folders` and no `repos`. The heading and the title bar's chip take the new name; its conversations take the new folders from obid's `conversation` events (PROJECTS-3), and their boxes still show none. Test: `projects_29_add_and_remove_a_folder`.
- **PROJECTS-30, a refused save (implemented).** `PATCH /v1/projects/{id}` answers `422 {"error": "…"}`: obid's sentence shows in the sheet, which stays open with its name and rows as they were edited and "Save" enabled; the projects listed are unchanged. Test: `projects_30_a_refused_save`.
- **PROJECTS-31, delete asks first (implemented).** "Delete project…" in the sheet asks "Delete <name>?", "Its conversations stay, working in its folders as their own.", with "Cancel" (back to the sheet) and "Delete project"; from the heading's menu it opens at that ask ("Cancel" closes it). "Delete project" sends `DELETE /v1/projects/{id}`; on `204` the sheet closes and the heading goes; its conversations move under No project as obid's `conversation` events take them out (PROJECTS-8). A refusal shows in the ask as PROJECTS-30's does. Test: `projects_31_delete_asks_first`.

### Test it when it's done (a real Mac)

1. Make a project with a folder ("New project", "Add a folder…", "Create project"): it is a heading in the side panel.
2. Choose it with Home's chip and start a conversation from Home: the conversation is listed under the project, and its agents work in the project's folder.
3. Open it and move it to another project, then to No project, from the chip in its title bar: its row moves each time.
4. Open a conversation in a project: its box shows no folder; one in no project shows its own.
5. From the project's heading ⋮, "Edit project…": add a repository and a folder, remove one, "Save"; open it again and it shows what was saved.
6. "Delete project…" from the heading's ⋮: its conversations are under No project, still working in its folders.
