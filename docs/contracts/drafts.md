# Drafts: what was typed and not sent stays, with what went with it

The business behavior contract for a message box's unsent draft, from [the template](../business-contract-template.md). Scenarios are numbered `DRAFTS-n`; the tests that prove them are named after them (`drafts_4_…` to `drafts_8_…` in `apps/electron/src/renderer/src/test/drafts.test.ts` and `draftTags.test.tsx`; DRAFTS-1 to 3 by the older tests there, by what they say).

## Feature and status

- **Outcome:** what the owner typed in a message box and didn't send is back when the box shows again, after a relaunch too, and so is everything that was going with it: the files and photos added, the tasks, Mac and repository tagged, and on Home the project and working folders picked. Before 2026-11-17 a draft kept its words only.
- **Status:** **Implemented.** The words were already (the Mac and iPhone apps keep theirs in UserDefaults, `apps/shared/ChatThread.swift`); the rest is new, the T3 Code way (`composerDraftStore.ts`: a draft's images kept with it, a file's bytes when they fit).
- **Scope:** the Electron app's boxes: Home's, a conversation's, a task's, an Oby's page's, a peer thread's. **Out of scope:** the Sphere's room (words only, no tags), and the phones.

## Behavior

### Actors and ownership

- **The Electron app** owns drafts: they're the owner's own words, kept in the window's `localStorage` under `oby.drafts`, on this Mac only, never sent to obid or anywhere else.
- **obid** has no part in them.

### Scenarios

- **DRAFTS-1:** each box keeps its own words until they're sent or cleared (white space alone is nothing), across a relaunch, the cursor at their end when put back.
- **DRAFTS-2:** a draft goes a week after it was last typed in, so what was pasted, a secret among it, doesn't stay for good; so does one whose conversation is deleted.
- **DRAFTS-3:** a value that doesn't read is nothing kept.
- **DRAFTS-4:** with the words go the box's tags: the tasks attached, the Mac it runs on and the repository; they come back with them when the box shows again, after a relaunch too, and another box shows its own.
- **DRAFTS-5:** Home's box keeps its project and its working folders with its words (a conversation's folders and project are the conversation's own, kept with it).
- **DRAFTS-6:** a file's bytes up to 1 MB, and 3 MB for all of a box's files told, are kept with its draft, so it comes back after a relaunch; a bigger one comes back while the app runs, and after a relaunch its tag doesn't come back.
- **DRAFTS-7:** a box with tags and no words is still kept.
- **DRAFTS-8:** sending lets the whole draft go, tags and files too, as does a tag taken out or the conversation deleted; a tag that doesn't read is nothing kept.

## Invariants

- A draft never leaves the Mac: no request to obid or anywhere carries it but the send itself.
- Every draft goes a week after it was last touched, files with it.

## Tests

| Scenario | Test |
|---|---|
| DRAFTS-1 to 3 | `drafts.test.ts` (`drafts`, `a box's keeper`) |
| DRAFTS-4 | `drafts.test.ts drafts_4_tasks_a_mac_and_a_repository_tagged_come_back_after_a_relaunch`, `draftTags.test.tsx drafts_4_a_boxs_tags_and_file_come_back_with_its_words` |
| DRAFTS-5 | `drafts.test.ts drafts_5_homes_project_and_folders_are_kept_with_its_words` |
| DRAFTS-6 | `drafts.test.ts drafts_6_a_small_files_bytes_come_back_after_a_relaunch_a_big_one_only_this_run` |
| DRAFTS-7 | `drafts.test.ts drafts_7_a_box_with_tags_and_no_words_is_still_kept` |
| DRAFTS-8 | `drafts.test.ts drafts_8_sending_or_deleting_lets_the_tags_go_with_the_words`, `drafts_8_a_foreign_tag_is_nothing_kept`, `draftTags.test.tsx drafts_8_a_tag_taken_out_is_taken_out_of_the_draft` |

## Test it when it's done (on a real Mac)

1. Type in a conversation, add a photo and a task tag, open another conversation and come back: all there.
2. Quit Oby and open it again: the words, the photo and the tag are back.
3. Send it: the box is empty, and stays empty after a relaunch.
