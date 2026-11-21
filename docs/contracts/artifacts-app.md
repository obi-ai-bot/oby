# Artifacts, in the app

The business behavior contract for artifacts in the Electron app, from [the template](../business-contract-template.md): an agent's artifact in its conversation, and beside it, drawn by Sandpack, fully local. Scenarios are numbered `ARTIFACTSAPP-n`. The app's unit tests are named after them (`apps/electron/src/renderer/src/test/artifactsApp.test.ts`, `artifactsPane.test.tsx`, `artifactsFollow.test.tsx`, `artifactsRow.test.tsx`), and so are the checks of the real window, which run on the Mac (`apps/electron/scripts/test-artifacts.mjs`, `npm run test:artifacts`). obid's side, the store and its API, is [artifacts.md](artifacts.md).

## Feature and status

- **Outcome:** An agent makes an artifact in a conversation, such as a page, a document or a picture. The owner sees its card in the first Oby message that tells of it (the turn that published it, or the report of the task that made it), and after the last turn when none does. They click it, or its row under the conversation in the side panel, and see the artifact beside the conversation, drawn as it is. Nothing about it leaves the Mac: no hosted bundler, no CDN, no CodeSandbox server.
- **Status:**
  - **Implemented:** the card, the pane beside the conversation, Preview of a version (ARTIFACTSAPP-1 to 4); Code, following the agent's new versions, and the open artifact sent with the owner's words (ARTIFACTSAPP-5 to 7); the owner's edits, Save and the version menu (ARTIFACTSAPP-8 to 10); Download, Copy and Import (ARTIFACTSAPP-11, 12); the card in its message and its row (ARTIFACTSAPP-15 to 17); and the card's picture (ARTIFACTSAPP-18).
  - **Proposed:** React artifacts, in the order of [the port plan](../plans/artifacts-port.md) (L5).
- **Design:** Sangam's Oby Artifacts canvas (`tools/obid-spec/lab-ui/artifacts/design/project/`), 1280 by 800: board 1a, the card in Oby's message and the row under its conversation (`Conversation.dc.html`); board 1b, "Beside its conversation", the row lit while its artifact is open (`Viewer.dc.html`); and the kind words, `Canvas`, `Page`, `Document`, `Image` and `PDF` (`Main.dc.html`). The words are his.
- **Scope:** the Electron app: `src/renderer/src/artifacts/`, its mount in `Home.tsx` and `ChatView.tsx`, its rows in `Sidebar.tsx`, `src/main/artifactPreview.ts`, `src/main/sandpackRelay.ts`, `src/main/previewRules.ts`, `resources/sandpack-relay/`, `stubs/nodebox/` and `scripts/local-only.mjs`.
- **Out of scope:**
  - obid: the store, its kinds and its API ([artifacts.md](artifacts.md)).
  - Claude Design boards drawn by Oby's runtime (P4), and sharing (P9 to P12).

## Behavior

### Actors and ownership

- **obid** keeps every artifact and version, and serves their files (`GET /v1/artifacts?conversation=`, `GET /v1/artifacts/{id}`, `GET /v1/artifacts/{id}/{version}/{*path}`).
- **The page** lists a conversation's artifacts and draws the card and the pane. It also lists every artifact once, for the side panel's rows. It never reads an artifact's files.
- **Main** reads the version's files from obid with the token it holds. It shows them in a view of its own, laid over the pane's box for the preview, and serves Sandpack's relay on loopback.
- **Sandpack's `static` client** (`@codesandbox/sandpack-client` 2.19.8, Apache-2.0) runs in that view's host page. It shows the files in a frame of the relay's origin, `<id>-preview.localhost`, a new origin each time.

### Triggers and states

- **The card.** A conversation's artifacts are read when it opens, and again whenever its turns or its tasks' states change. Board 1a is the authority.
  - Each artifact's card sits in the first Oby message that tells of it: the turn that published it (`Turn.artifacts`), or the report of the task that made it (the turn obid writes when the task ends). A later message naming a newer version adds no second card, and the card stays, on the latest version.
  - The cards come last in their message. With the message's words just before them, words, cards and time end it together, as board 1a draws it; Copy of Oby's words is on the time's line.
  - An artifact no message tells of (an obid that names none, a task whose report hasn't come) shows after the last turn.
  - Wherever it sits, the card names its task's agent when the window knows that task.
  - Its top is a picture of its latest version, as its preview draws it, taken by main in the preview's own locked view, never in the window's page. Until the picture is ready, and when it can't be taken, the card shows the board's blocks. A new version shows its own picture, the blocks until it comes.
- **The row under its conversation.** In the side panel, each artifact of a conversation has a row after the conversation's tasks, in their group, in the order obid lists them: the Frame, `<b><title></b> · <kind>`, and `<title>, made in this conversation` as its title. A conversation with an artifact and no task gets the group as one with tasks does.
  - The row is lit (`is-on`) while its artifact is open.
  - Its click opens the conversation and the artifact beside it on its latest version, as the card's does; the artifact stays open once the conversation left has gone.
  - The side panel reads every artifact with one `GET /v1/artifacts`, never one per row or conversation, and again at each `artifact` event; on the mock, none. A read that fails keeps the last list.
  - **New**, a dot at the row's end, is the window's own state, never obid's or kept on disk: an artifact made, or given a version, while the window runs gets it, unless it is the one open, until it is opened from its card, its row or by itself. A rename gives none, and after a restart nothing is New. On board 1a the dot follows a reply to a share; sharing is parked, so here it follows a new artifact or version.
- **The pane.** A click on a card opens the pane in the browser panel's place, at its width, on the artifact's latest version.
  - One artifact is open at a time.
  - Close takes the pane and its preview away, and so does leaving the conversation for one that isn't the artifact's.
- **Preview and Code.** The pane's strip has Preview and Code, as LibreChat's panel has them; it opens on Preview.
  - Code lists the version's files as tabs, `index.html` first, then by name, and shows the one picked in CodeMirror 6, read-only, with its language's colours. A file that isn't text (a picture, a font) is named, never read.
  - While Code shows, main's preview view is hidden; back on Preview, it shows again.
  - CodeMirror is the editor Sandpack's own `SandpackCodeEditor` wraps. `sandpack-react` isn't built in: it names CodeSandbox's hosted services, which the build refuses.
- **Following the agent** (`artifact` on `/v1/events`, read again from obid, never taken from the event):
  - a new version of the artifact open moves the pane to it when it was on the latest; on an older version it stays;
  - a rename retitles it;
  - an artifact made in the conversation on view (its version 1) opens by itself when none is open;
  - the conversation's cards are read again.
- **The owner's words.** Words said in a conversation while its artifact is open carry `artifact: {id, version}`, the version on view, once obid announces `artifact-turn-v1` (ARTIFACTS-12). They go signed, as the owner's words do ([signed-words.md](signed-words.md), WORDS-22). An answer to Oby's question carries none, nor do words to another conversation than the artifact's.
- **The owner's edits** (as LibreChat's Code tab, with Save instead of saving by itself):
  - On the latest version, Code is the owner's to edit; on an older one it is read-only. Each file edited is marked, and typing its text back unedits it.
  - Preview shows the edits before they're saved: the page sends them with the preview's ask, and main lays each over its file of the version it read (`edits`, a file of that version only, at most 17 MB in all).
  - **Save as v<n>** (`n` the version it makes) sends them as the owner's edit (`POST /v1/artifacts/{id}/versions`, ARTIFACTS-16), signed by main on that button's own click ([signed-words.md](signed-words.md), WORDS-23). Done, the pane is on the new version; refused (a secret, a limit, another version came first), obid's words show above the tab and the edits stay.
  - A version the agent makes while the owner has unsaved edits never sweeps them away: the pane stays on the version they edit and says the newer one came; their Save then shows only once they open the latest.
- **Download** (Sangam's board 1c, its Export's Download): the pane's Export opens **Project archive (.zip)** (for a design, "Every file, as Claude Design keeps it: opens there unchanged") and **Standalone HTML**. Either saves the version on view as obid exports it (`GET /v1/artifacts/{id}/export/{version}?as=`, ARTIFACTS-9 and 10), byte for byte, where the owner picks in main's own save dialog, named as obid names it, whole or not at all (a new file beside it, renamed over it) and never through a link there; the pane says where it went, or why not. Preview's view steps aside while the menu is open, as main's view lies over the page; Esc, a click outside it or the window changing size closes it, as the app's other menus. **Copy**, in Code, copies the file on view as it reads there, the owner's edit too.
- **Import** (obid's import, ARTIFACTS-11): a project archive (.zip), a page (.html, .htm) or a document (.md, .markdown) dropped on a conversation's thread (not its message box, which keeps its own drops as attachments) is offered in a bar, `Import <name> as an artifact?`. The owner's own click on **Import** sends its bytes as they are, signed by main by that label over those bytes ([signed-words.md](signed-words.md), WORDS-21); main sends them as what they are (`PK\x03\x04`, a zip; text starting with `<`, a page; else a document). Imported, it opens beside the conversation; the owner's first words with it open make it that conversation's (ARTIFACTS-12). Another file is offered nothing; a file over 257 MB, or one obid refuses, says why in the bar.
- **The version menu** (as LibreChat's `ArtifactVersion`): from two versions on, every version, newest first, the latest marked (`v3, the latest`). Another version on view leaves the edits on the one before.
- **The preview.** While the pane shows, the page tells main which artifact, which version and the preview's box, and tells it again whenever the box moves or changes size. Main reads that version once and moves its view with the box.

### What each kind shows

As LibreChat does, the `static` template always opens `index.html`:

- **a page** (`html`), a canvas or a design: its own files. Until P4, a design shows its files, not drawn.
- **a document** (`markdown`): an `index.html` holding it, drawn by `marked`.
- **a picture** (`image`): an `index.html` holding it, with its bytes as they are.
- **anything with no `index.html`**: a list of its files.

### Local only: what may run and what may be reached

- **The relay** is `static-browser-server` 1.0.3's built relay, four files (`resources/sandpack-relay/`), byte for byte the package's.
  - Main serves it on 127.0.0.1, on a port the system picks, and only for a host of `preview.localhost` or `<id>-preview.localhost` with that port. Any other host, path or method is `404`.
  - Main changes two lines of its service worker as it serves it. A request to another origin goes to the network instead of being answered from the artifact's files, and every answer carries the preview's CSP.
- **The preview's CSP:** its own origin only, for anything it fetches, loads, frames or sends a form to (`default-src 'self'`, `connect-src 'self'`, `form-action 'self'`). Inline scripts and styles, `data:` and `blob:` are allowed, as a page needs them. No plugins.
- **The view:**
  - It has its own partition, `oby-artifact-preview`, in memory only. It shares no cookie, storage, cache or service worker with the window's page.
  - It has no preload, no Node in its frames or workers, Chromium's sandbox and context isolation. The page's bridge (`window.app`) and obid aren't there.
  - A second view, never shown and never in the window, takes the cards' pictures (`preview.html?thumb`). It is built the same way, in the same partition, on the same locked session and the same relay, and its session lets go of nothing more.
- **What its session lets go:** its host page (this build's `preview.html` and its assets), the relay on its port, `data:` and `blob:`.
  - Every other request is cancelled before it leaves.
  - It opens no window, downloads nothing, and is granted no permission.
  - Its host page never navigates, and a frame in it navigates only within the relay's hosts.
- **The preview's frame** may run scripts and keep its own origin, which the relay's service worker needs. Nothing else: no popups, no downloads, no navigating the page above it, no camera, microphone or other device.
- **Not shipped:**
  - Sandpack's `runtime` client (a hosted bundler) and its `node` client (Nodebox) aren't part of the build; only the static client's own chunk is.
  - `@codesandbox/nodebox` is under the Sustainable Use License, so it is replaced by an empty package of ours (`stubs/nodebox`) through npm's `overrides`, and nothing of it is downloaded.
  - Sandpack's static client names one hosted address, its default bundler, which is taken out as it's built.
  - The build fails when any chunk holds a hosted bundler's or CDN's address (CodeSandbox, `blazingly.io`, Tailwind's CDN, jsDelivr, unpkg, esm.sh, Skypack) or Nodebox's licence. It also fails when a module would come from the real Nodebox package, or when Sandpack's default address isn't there to take out.

### Failure and recovery

- **obid can't read the version** (the artifact is gone, a version isn't there, obid is down): the pane says why in one line and nothing is drawn. Opening the card again tries again.
- **Sandpack changed under us:** the relay's files differ from the package's, the worker's lines aren't found, or the static default moved. Then the build or the tests fail and nothing is served: a new Sandpack is looked at again before it ships.

### Invariants

- Nothing an artifact holds reaches the network: its preview reaches only its own origin on loopback.
- The preview never reaches the window's page, its bridge, obid or main's token.
- The page gets only a picture's pixels of an artifact for its card, never its files.
- No hosted bundler, no CDN, and no Nodebox code is in the built app.

## Interfaces and dependencies

- **obid:** `GET /v1/artifacts?conversation=`, `GET /v1/artifacts` (no filter, for the side panel), `GET /v1/artifacts/{id}`, `GET /v1/artifacts/{id}/{version}/{*path}` ([docs/api/artifacts.md](../api/artifacts.md)), unchanged.
- **obid's turns:** `Turn.artifacts` (`shared/obid-wire.ts`, `[{id, version}]`, on Oby's turn that published them; none from an older obid) and the report turn (`ended`, `jobs: [<job>]`), [docs/api/conversations.md](../api/conversations.md).
- **obid's events:** `artifact` (`shared/obid-wire.ts`, `{id, version, conversation, renamed, title}`), passed on by main as every event is; `POST /v1/plans`' `artifact` (`artifact-turn-v1`).
- **The page into main:** `artifact-preview` ([ipc.md](ipc.md)), with `{ id, version, bounds, edits? }` or `null`; `artifact-export`, with the id, the version and `zip` or `html`, answering `{ saved }`, `{ cancelled: true }` or `{ error }`. It answers `{ shown: true }`, or `{ error }` saying why nothing shows. `artifact-thumb`, with `{ id, version }`, answering `{ src }` (a PNG `data:` URL) or `{ none: true }`, kept in main's memory only (50 at most, until the window closes).
- **Packages:** `@codesandbox/sandpack-client` 2.19.8, and through it `static-browser-server` 1.0.3 (both Apache-2.0); CodeMirror 6 (`@codemirror/state`, `view`, `language`, `lang-html`, `lang-css`, `lang-javascript`, `lang-markdown`, `lang-json`, `@lezer/highlight`, all MIT, pinned exactly); `marked` (MIT), already the app's.

## Acceptance scenarios

- **ARTIFACTSAPP-1** Given an artifact a task made in a conversation, a page whose `index.html` links its stylesheet, when the owner opens it, then its version draws in Sandpack's preview, stylesheet and all. The frame is served from `<id>-preview.localhost` on main's relay, and every request the preview makes goes to loopback. A document and a picture get an `index.html` around them.
  - Unit tests: `artifactsapp_1_the_relay_serves_sandpacks_own_files_on_loopback_for_a_preview_host`, `artifactsapp_1_the_vendored_relay_is_the_packages_byte_for_byte`, `artifactsapp_1_a_page_shows_its_own_files_a_document_and_a_picture_get_an_index`.
  - Mac: `artifactsapp_1: a page artifact draws in the real window, from loopback only`.
- **ARTIFACTSAPP-2** Given the app's build, then:
  - it holds no hosted bundler's or CDN's address and no Nodebox code;
  - Sandpack's static default is taken out, and the build fails if it isn't there to take out;
  - Nodebox is our empty stub, through `overrides` and the lock, and a module resolved into the real package fails the build.

  Unit tests: `artifactsapp_2_the_guard_names_each_hosted_address_and_nodebox`, `artifactsapp_2_sandpacks_static_default_is_taken_out_and_its_absence_fails_the_build`, `artifactsapp_2_nodebox_is_our_empty_stub_never_the_package`. The build itself runs the guard.
- **ARTIFACTSAPP-3** Given an artifact's preview showing:
  - no fetch, image, script or frame it makes to another origin loads: another site, obid's port, or `127.0.0.1` on the relay's own port;
  - the same origin's files do;
  - neither its frame nor its host page has `process`, `require` or the page's bridge;
  - its view runs sandboxed in its own partition, not the window's, with no preload;
  - the relay answers only preview hosts on its port;
  - the page may ask main only for an artifact, a version and a box.

  Unit tests: `artifactsapp_3_the_relay_answers_only_preview_hosts_on_its_own_port`, `artifactsapp_3_the_worker_sends_other_origins_to_the_network_and_puts_the_csp_on_every_answer`, `artifactsapp_3_the_preview_session_lets_go_only_its_host_page_and_the_relay`, `artifactsapp_3_the_page_asks_for_an_artifact_a_version_and_a_box_nothing_else`. Mac: `artifactsapp_3: the preview reaches nothing off its origin`, `artifactsapp_3: the preview is sandboxed in its own partition with no way into the app`.
- **ARTIFACTSAPP-4** Given a conversation where a task made an artifact:
  - its card shows in the first Oby message that tells of it (the report of the task that made it, or the turn that published it), and after the last turn when none does, saying "Oby artifact · <kind> · v<n> · made by <agent>" (board 1a);
  - a click opens the pane beside the conversation on its latest version, with its title and the same line;
  - main gets the artifact, the version and the preview's box, and nothing else;
  - Close takes the pane and the preview away, and an error from main shows in the pane.

  Unit tests: `artifactsapp_4_the_card_says_what_it_is_and_a_click_opens_it_beside_the_conversation_on_its_latest_version`, `artifactsapp_4_an_error_from_main_shows_in_the_pane`. Mac: `artifactsapp_4: the card in its message opens the pane`.
- **ARTIFACTSAPP-5** Given the pane open on a version of a page (`index.html`, `style.css`, `logo.png`), when the owner picks Code:
  - Preview's view goes;
  - the files show as tabs, `index.html` first, and `index.html`'s text shows in CodeMirror, read-only;
  - another tab shows its file's text, and a picture is named ("logo.png isn’t text: Preview shows it.") and never read;
  - back on Preview, its view comes back.

  Unit test: `artifactsapp_5_code_shows_each_file_of_the_version_index_first_and_preview_stops` (`artifactsFollow.test.tsx`). Mac: `artifactsapp_5: Code shows each file of the version`.
- **ARTIFACTSAPP-6** Given the pane open on an artifact's latest version:
  - when obid says it has a new version, the pane moves there and Preview draws it;
  - a rename retitles it;
  - on an older version, the pane stays, and another artifact's version moves nothing;
  - with none open, an artifact made in the conversation on view opens by itself, and one of another conversation, or with none on view, doesn't.

  Unit tests: `artifactsapp_6_a_new_version_from_the_agent_moves_the_open_pane_to_it`, `artifactsapp_6_an_older_version_on_view_stays_and_another_artifact_moves_nothing`, `artifactsapp_6_an_artifact_the_agent_makes_in_the_conversation_on_view_opens`. Mac: `artifactsapp_6: a new version from the agent moves the open pane to it`.
- **ARTIFACTSAPP-7** Given the pane open on version 1 of a conversation's artifact, when the owner says words in that conversation, then `POST /v1/plans` carries `artifact: {id, version: 1}`. Closed, the words go without it, and so they do to an obid that doesn't announce `artifact-turn-v1`, or to another conversation than the artifact's.

  Unit tests: `artifactsapp_7_words_said_with_the_pane_open_carry_its_artifact_and_version`, `artifactsapp_7_not_to_an_obid_without_it_nor_to_another_conversation`; signed: `words_22_the_artifact_open_signs_with_its_words`.
- **ARTIFACTSAPP-8** Given the pane on a page's latest version (v1), when the owner edits `style.css` in Code:
  - the file is marked, **Save as v2** shows, and typing the text back takes both away;
  - Preview shows the edit unsaved: main is asked for v1 with the edit, and lays it over v1's file (no other file, nothing over 17 MB);
  - Save sends `{base: 1, files: {style.css: …}}`, and the pane moves to v2 with nothing unsaved.

  Unit tests: `artifactsapp_8_an_edit_shows_in_preview_before_it_is_saved_and_save_makes_the_next_version`, `artifactsapp_8_typing_the_text_back_unedits_it`, `artifactsapp_8_main_takes_the_owners_unsaved_edits_to_lay_over_the_version_and_nothing_more`; signed: `words_23_an_artifacts_edit_signs_by_its_own_save_as_label`. Mac: `artifactsapp_8` (typed in the real editor, drawn unsaved, and Save making the owner's version on an obid with the edit route).
- **ARTIFACTSAPP-9** Given an artifact with two versions, then the version menu lists `v2, the latest`, `v1`; picking v1 shows it in Preview and Code, read-only. With one version, there's no menu.

  Unit test: `artifactsapp_9_the_version_menu_opens_any_version_the_latest_marked_an_older_one_read_only`. Mac: `artifactsapp_9`.
- **ARTIFACTSAPP-10** Given the owner's unsaved edit on v1, when the agent makes v2, then the pane stays on v1 with the edit, says `Version 2 came after the one you’re editing.`, and offers no Save. On the latest, a Save obid refuses (`422` for a secret, `409` for a version that came after) shows obid's words, and the edit and Save stay.

  Unit test: `artifactsapp_10_a_version_that_comes_meanwhile_keeps_the_edits_and_obids_refusal_shows_with_them_kept`. Mac: `artifactsapp_10` (an obid without the edit route: its refusal shows, the edit kept).
- **ARTIFACTSAPP-11** Given the pane on v1 of an artifact, when the owner picks Export, then:
  - its Download lists **Project archive (.zip)** and **Standalone HTML**, and Preview's view steps aside while the menu is open;
  - each saves obid's export of v1, byte for byte, where main's dialog answered, named as obid names it (never a path in that name), and the pane says `Saved to <path>`;
  - a cancelled dialog writes nothing and says nothing, and a version obid no longer has says so;
  - a link where the owner picked is refused, and a write that fails leaves the file that was there whole;
  - Esc, a click outside the menu or the window changing size closes it, and Preview comes back;
  - Code's Copy takes the file on view.

  Unit tests: `artifactsapp_11_download_saves_the_versions_zip_or_html_where_the_owner_picks_and_copy_takes_the_file`, `artifactsapp_11_main_saves_obids_export_byte_for_byte_where_the_owner_picked_and_nothing_when_cancelled`, `artifactsapp_11_a_download_replaces_the_file_whole_or_not_at_all_and_never_writes_through_a_link`, `artifactsapp_11_pdf_and_png_stay_off`. Mac: `artifactsapp_11` (the zip and the HTML saved, each compared with obid's export).
- **ARTIFACTSAPP-12** Given a conversation, when the owner drops a page on its thread, then the bar offers it, and nothing is sent until their click on **Import**. Then its bytes go, exactly, signed, and the pane opens on the imported artifact, drawn. A picture dropped is offered nothing, and a refusal shows obid's words in the bar.

  Unit tests: `artifactsapp_12_a_dropped_page_is_imported_on_import_and_opens_beside_the_conversation`, `artifactsapp_12_only_an_archive_a_page_or_a_document_is_offered_and_a_refusal_says_why`, `artifactsapp_12_an_import_is_sent_as_what_its_bytes_are`; signed: `words_21_an_artifacts_import_signs_its_bytes_as_they_are_by_its_own_import`. Mac: `artifactsapp_12` (dropped, imported on the real click, signed, kept by obid as `origin: import`, drawn).
- **ARTIFACTSAPP-15** Given an artifact's card, then it draws the Frame at 16, its title, `Oby artifact · <kind> · v<n> · made by <agent>` and a chevron at 14, both with stroke 1.6 (board 1a). The card and the pane name a kind with the board's word: `html` Page, `markdown` Document, `image` Image, `pdf` PDF, `canvas` and `design` Canvas.

  Unit tests: `artifactsapp_15_the_card_draws_the_frame_its_title_and_what_it_is_with_a_chevron_at_14`, `artifactsapp_15_each_kind_says_the_boards_word` (`artifactsPane.test.tsx`).
- **ARTIFACTSAPP-16** Given a conversation's artifacts:
  - one an Oby turn published shows in that turn's message, after its words and before its time, and nowhere else; a later turn naming its v2 adds no card, and the one card says v2;
  - one a task made shows in the message that reports the task's end, never in or beside the task's card, naming the task's agent;
  - one no message tells of (an obid whose turns name none, a task whose report hasn't come, a turn whose `artifacts` holds nothing readable) shows after the last turn, and a malformed entry makes no card.

  Unit tests: `artifactsapp_16_an_artifact_oby_published_shows_in_the_message_that_published_it`, `artifactsapp_16_a_tasks_artifact_shows_in_the_message_that_reports_its_end_not_under_its_card`, `artifactsapp_16_with_no_message_to_hold_it_the_card_shows_after_the_last_turn` (`artifactsFollow.test.tsx`). Mac: `artifactsapp_16` (the page's card in its task's report, on v2 too, and the site's in the turn that published it).
- **ARTIFACTSAPP-17** Given a conversation with a task and an artifact, and one with only an artifact:
  - each artifact's row comes after the conversation's tasks in its group, titled `<title>, made in this conversation`, with the Frame at 14 and `<b><title></b> · <kind>`; the conversation with only an artifact gets the twist and the group;
  - a click from another conversation opens the row's conversation and the artifact on its latest version, still open once the conversation left has gone, and the row is lit;
  - an artifact event for a new artifact, or a new version of another, puts New on its row, a rename none; opened from its card or its row, the dot goes; a fresh window shows none;
  - the side panel reads the list once, and once more at each artifact event; on the mock, never.

  Unit tests: `artifactsapp_17_the_row_under_its_conversation_says_what_it_is_and_opens_it` (`artifactsRow.test.tsx`), `artifactsapp_17_the_new_dot_shows_until_the_owner_opens_it`, `artifactsapp_17_the_side_panel_reads_the_artifacts_once_not_per_row` (`artifactsFollow.test.tsx`). Mac: `artifactsapp_17` (at 1280 by 800, a version made while the import is open dots the page's row; its click opens v2, lit, the dot gone).
- **ARTIFACTSAPP-18** Given an artifact's card:
  - main sends the picture of its latest version, and the card's top is that picture, filling it; until it comes, when main answers none, or with no bridge (the mock), the board's blocks;
  - a new version asks for its own picture, and a late picture of the one before never shows on it;
  - main answers only `{ id, version }`, a lower-case UUID and a version from 1, and refuses anything else, another key included, with `null`, asking obid nothing;
  - main takes the picture in its preview's second view, never shown: a 720-wide PNG of the page's first screen, sent as a `data:` URL; taking it reaches nothing but the host page and the pane's own relay, and is refused nothing the pane's view isn't.

  Unit tests: `artifactsapp_18_the_card_shows_the_versions_picture_once_main_sends_it`, `artifactsapp_18_the_card_keeps_the_boards_blocks_until_then_and_on_none`, `artifactsapp_18_a_new_version_asks_for_its_own_picture` (`artifactsPane.test.tsx`), `artifactsapp_18_the_channel_refuses_a_bad_ask` (`ipc.test.ts`). Mac: `artifactsapp_18_the_cards_picture_is_the_artifacts_first_screen`, `artifactsapp_18_taking_the_picture_reaches_nothing_new`.
