# Files and photos

The business behavior contract for the message box's **Add files or photos** and **Attach tasks** (Sangam's + panel, [#1564](https://github.com/obi-ai-bot/oby/pull/1564): `Composer.tsx`, `MessageEditor.tsx`'s pasted files, the tags under a message in `ChatView.tsx`), from [the template](../business-contract-template.md). Scenarios are numbered `ATTACH-n`; the tests that prove them are named after them (`attach_1_…`): obid's in `core/crates/obid/tests/attachments_api.rs`, the app's in `apps/electron/src/renderer/src/test/attachments.test.ts`.

## Feature and status

- **Outcome:** What the owner picks or pastes into the message box (a screenshot, a log, a PDF) and the tasks they attach reach Oby with their words, and a task Oby starts from those words gets the files. A message sent again (the connection dropped, obid restarted) is taken once.
- **Status:** **Implemented** (#1576). obid already took `attachments`, `jobs` and `id` on `POST /v1/plans` (#891, #1173, #493) and copied attachments into the conversation's room and a job's `.obi/inputs`; this adds the upload (`POST /v1/attachments`), the limits, the files' names on the owner's turn, files or tasks with no words, and the Electron app sending all of it.
- **Scope:** the Electron app on this Mac and obid. **Out of scope:** uploading from the iPhone or the Watch (the gateway doesn't list the upload), showing a photo's preview after the app restarts (obid keeps the file, the turn keeps only its name), other Obys (`obys`, brought in by name in the words).

## Behavior

### Actors and ownership

- **The owner** picks files with the Mac's file browser (+ › Add files or photos), pastes one (a screenshot), or attaches tasks (+ › Attach tasks), then sends, with words or without.
- **The Electron app** uploads each file's bytes when the owner sends (never a path: it reads only what the owner picked or pasted), then sends the words with the paths obid answered, the attached tasks' ids and a fresh `id`. Until every file is up, the box keeps the words and tags; one refused (over 25 MB, which the app checks before reading the file, or empty) leaves them there, says obid's words, and removes the files uploaded before it. Words obid refuses take their files away too. A second pasted screenshot (every one is `image.png`) is numbered (`image 2.png`), never dropped as a duplicate. It shows each file as a tag under the words (the name picked, a photo's preview while it has one) and each task as its card.
- **obid** keeps the files under its home, decides what's accepted, copies them where an agent may read them (secrets taken out), marks what read them untrusted, and makes a resent message a repeat.

### Triggers and prerequisites

- The upload and the words come from the owner's Mac app over loopback with the runner's token. A phone can't upload (the gateway's list doesn't have `POST /v1/attachments`).

### What obid keeps and refuses

- `POST /v1/attachments?name=<the file's name>`, the file's bytes as the body: kept as `home/attachments/app/<uuid>/<name>` (0600 in 0700 folders), the name cleaned to letters, digits, `.`, `_` and `-` (as the room's are). Answers `201 {"path", "name"}`.
- Limits: a file over 25 MB is `413` (`<name> is larger than 25 MB, so Oby can't take it.`); an empty one `422` (`<name> is empty.`); more than 20 files on one message `422` (`A message attaches at most 20 files.`); at most 20 tasks (`A message attaches at most 20 jobs.`). obid keeps the newest 200 uploads; older ones go (the rooms and tasks keep their own copies).
- `DELETE /v1/attachments?path=<path>` removes an upload whose words never went: only one of `POST /v1/attachments`'s (`422` otherwise), never one a conversation or a task that hasn't ended holds (`409`).
- A conversation keeps the paths of the files attached to its words. When it's deleted (or forgotten past obid's 500 conversations), those originals go with it, unredacted as they are, except one a task that hasn't ended still holds, which the uploads' bound takes later.
- A path in `attachments` that isn't a file in obid's attachments folder once `..` and every link are resolved (one the app didn't upload, `/etc/hosts`, a link planted there that leads out) is `422` naming the file, and nothing of the request happens.

### Decisions and invariants

- **An attachment is untrusted.** Words that carry one mark their conversation as having read something untrusted (`files the owner attached`), so the tasks it starts are `untrusted` (in a clone apart, the owner's mode, every other protection as before), and its later words keep the owner's declared secrets from a task's check.
- **Quoted files only.** The conversation's agent gets copies in its room (`attachments/<turn>/<name>`) and the prompt names them between the turn's fences as data; a task's agent gets them in `.obi/inputs`, named between fences in its prompt too. Never the file's text in a prompt. A text copy has every secret obid knows taken out; a binary file holding one, a file over 25 MB, or one gone from obid's folder before its turn or task started (its conversation deleted, or past the uploads' bound) is left out and named as left out.
- **Retry-safe.** The words' `id` is the owner's turn's id. The same words sent again with it, where they went, add nothing: no second turn, no second answer, no second copy of a file or a task, and the answer is `{"kind": "none", "repeated": true}`. The app sends again, with the same request, only when obid didn't answer at all (status 0), up to 3 times a second apart; every answer of obid's is final.
- **Files or tasks are a message.** Words may be empty when a file or a task goes with them; a conversation they start is named after the first file. Files with no words to a finished task start its follow-up with them (its words `Look at the files I attached.`). Files go only with a follow-up or a new task: to a task at work they're refused (`Files go with a follow-up of a finished task or a new task: tell this one in words.`), and the app removes their uploads. Proposed (not built): obid keeps them for that task's next follow-up instead, and says so.
- **Add-only.** New routes (`POST` and `DELETE /v1/attachments`); on the owner's turn a new `attachments` key (the files' names) and a job's `attachmentsGone`, each left out when empty. Nothing renamed or removed.

### Failure and recovery

- An upload refused or failed: the words aren't sent, the box keeps them and the files, the files already uploaded are removed, and obid's words show as the app's error. The owner sends again.
- obid not answering while the words go: sent again with the same `id`; if it still doesn't answer, the app says so and nothing is lost on obid's side (the words, if they got there, are in the conversation).

## Acceptance scenarios

- **ATTACH-1** A file picked or pasted is kept in obid's attachments folder: `POST /v1/attachments` answers `201` with a path under `home/attachments/app`, the name cleaned, the bytes as sent, mode 0600.
- **ATTACH-2** Past the limits: an empty file `422`, one over 25 MB `413`, 21 files on one message `422`, each in obid's words.
- **ATTACH-3** A path outside what the owner picked (`/etc/hosts`, a link in the folder that leads out) is `422`, and no conversation starts.
- **ATTACH-4** Uploaded files reach the conversation: the room has them (secrets taken out), the prompt names them as quoted data, and the owner's turn carries their names (`attachments`); Oby's turn carries none.
- **ATTACH-5** Files alone are a message: empty words with a file are taken, the new conversation is named after it, the turn's text is empty and its `attachments` names the file. Empty words with nothing attached stay `422`.
- **ATTACH-6** A task started from words with files gets them in `.obi/inputs`, and is `untrusted`.
- **ATTACH-7** Attached tasks go on the owner's turn (`jobs`), with words or without.
- **ATTACH-8** Words sent again with their `id` add nothing twice: `repeated: true`, one owner's turn and one answer, one copy of the files in the room, one conversation.
- **ATTACH-9** Files are uploaded only on the Mac: the gateway doesn't allow `POST /v1/attachments`.
- **ATTACH-10** The app uploads each file, then sends the words with the paths, the tasks and a UUID `id`; files alone are sent; a send obid didn't answer goes again, the same request with the same `id`; an answer of obid's is final; the tag shows the name picked and a photo's preview.
- **ATTACH-11** A deleted conversation takes its files with it: the upload's folder is gone; one a task at work holds stays.
- **ATTACH-12** An upload whose words never went can go (`DELETE /v1/attachments`, `204`); a path that isn't an upload is `422`; one a conversation holds is `409` and stays.
- **ATTACH-13** Files with no words to a finished task start its follow-up, with them in its `.obi/inputs`; to a task at work they're `422` in obid's words.
- **ATTACH-14** A file gone before its turn or task started is named as left out (`(left out: <name>, it is no longer on this Mac)`); a task's prompt names its files between fences.
- **ATTACH-15** One refused upload (empty, or over 25 MB, which isn't read or sent) keeps the box, says obid's words, and removes the files uploaded before it; words obid refuses take their uploads away.
- **ATTACH-16** A second pasted screenshot is numbered (`image 2.png`), not dropped.
