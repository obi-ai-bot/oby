# The message box: chips, waiting words, drop, @, /, earlier words and quotes

The business behavior contract for what the message box takes besides typing, done the way T3 Code's composer does it, from [the template](../business-contract-template.md). Scenarios are numbered `COMPOSER-n`; the tests that prove them are named after them (`composer_1_…`).

## Feature and status

- **Outcome:** The owner sets the mode the tasks their words start will run in right in the box, before they start, without changing Settings, and picks the model and effort on the one account menu (one Auto); sees the words they sent while Oby was still answering, and can take them back or change them; sees Stop being honoured; and brings files in by dropping them or naming them with `@`.
- **Status:** Implemented (COMPOSER-1 and 3 to 24; COMPOSER-2 retired by COMPOSER-17). The `/` list is honest about what's missing: obid lists no agent's own commands yet (COMPOSER-16).
- **Scope:** The Electron app's message box on Home and in a conversation with Oby, and obid's `defaults` on `POST /v1/plans`, `turn` on `POST /v1/plans/cancel`, and `GET /v1/conversations/{id}/files`. **Out of scope:** a task's own box and a conversation with other people's Obys (no chips, no waiting row); running an agent's slash commands; the iPhone and Apple Watch apps (frozen).

## Behavior

### Actors and ownership

- **The owner** sets the mode chip, picks an account's model and effort, sends words, presses Stop, Edit or Cancel, drops files, and types `@` or `/`.
- **The Electron app** keeps the mode chip per box (with the box's draft, `chips.ts`: across a relaunch, a week after they were last set, on this Mac only), sends them with the words, shows the waiting words obid reports, and attaches dropped files as + › Add files does.
- **obid** checks the chips, makes them the jobs' the words start, keeps one answer at a time in a conversation (docs/api/conversations.md), drops one waiting message on its own when asked, and lists a conversation's files. Oby's brain still picks the tool where the words name one (the dispatcher-not-harness decision, [why](../decisions/2026-11-05-dispatcher-not-harness.md)).

### The chips (COMPOSER-1 to 7)

- One chip sits before the talk pill in a box whose words go to Oby (Home's, a conversation's), once obid lists `composer-v1` (always on the mock): **Mode** (Default, then Ask, Accept edits, Plan, Bypass, each with its line). Default is muted.
- It is a default for this box's words alone. It stays after a send, for the next words. Nothing writes it to Settings (`PUT /v1/settings/permissions`).
- Sending carries the mode, when one is set, as `defaults` (`{mode}`); nothing when the chip is left alone; an answer to Oby's question carries none. obid still takes `agent`, `model` and `effort` in `defaults` (below, for any client: the API only grows); this app sends none of them.
- obid makes them the jobs the words start (`oby_dispatch`, `oby_offer`): the chip's mode is the job's, over the brain's; Bypass from the chip on words an app of the owner's signed is their own word, so the job starts with no Confirm; on unsigned words it's treated as the brain's Bypass (the owner's default, or not taken). The chip's tool is the job's where the brain left the tool `auto`; its model and effort hold only on that tool, and a job that moves to another tool takes that tool's account's.
- Words answered together carry the latest words' chips.

### One Auto (COMPOSER-17)

- The box has one Auto: the talk pill's (Auto · Whichever has room, then each account with its model and usage, a `>` to its models, then Add account). There is no model chip.
- An account's `>` list holds its models, then, for a tool whose effort isn't in its model's name (Claude Code, Codex; not Cursor), a line, **Effort**, and that model's efforts as pills, four to a row, the one the account runs on checked.
- Picking a model or an effort keeps it on the account (`PATCH /v1/accounts/{id}`, as Settings › Agents does) and talks through that account, as picking a model always has (CONVERSATION-AGENT-19).
- A tool, model or effort an earlier build's model chip kept for a box is read as nothing: nothing on screen would show it, so it never goes with the words.

### Earlier words (COMPOSER-20 to 22)

- In an empty box, ↑ fills it with the owner's previous words in this conversation (or to this task), then, pressed again, the ones before, as T3 Code's composer does. ↓ goes forward, and past the newest it leaves the box empty again. The same words sent twice in a row come back once; words that were only files are skipped.
- Once the box holds words the owner typed, or changed what ↑ brought back, the arrows move the caret as ever; with ⌥, ⌘ or ⌃ they're never the history's. While the `@` list is open, its arrows are the list's.
- The app reads them from the conversation it shows; obid adds nothing.

### Quoting a reply (COMPOSER-23, 24)

- Selecting words in a reply, Oby's in a conversation or an agent's in its session, shows a small **Reply about this** button by the selection's end once the pointer lets go (T3 Code's Cite). Words of the owner's own, a task's card or a selection across two replies show none; Esc or a click elsewhere puts it away.
- A click puts the words in the message box as a Markdown quote (`> ` on each line) on lines of their own above the caret, an empty line after it, the caret there and the box focused; with no caret in the box, at its end.

### The usage panel (COMPOSER-18)

- The usage gauge's panel is as wide as the account menu (380, less where the window is narrower), its right edge on the gauge's and 4 px over it, as the account menu sits on its pill; it opens from its bottom right.

### Words that wait (COMPOSER-8 to 11)

- Words sent while a turn is under way in a conversation wait for it (one answer at a time). Every waiting message, as obid reports it (`delivery.status` `queued` behind words a turn has taken), shows as a row over the box (its words, "Waiting", Edit and Cancel), not in the conversation; a turn taking them puts them in the conversation.
- The app follows each move as it happens, from obid's `turnDelivery` event (docs/api/events.md): the turn again, kept as the newer by its `revision`, in main's copy and the window's alike. It's no new turn and says nothing about a turn under way (COMPOSER-19).
- **Cancel** sends `POST /v1/plans/cancel` with the conversation and the words' `turn`: obid drops those words alone (their request answers `none`, they read `cancelled`), and the turn under way and other waiting words go on; the words left are answered as Oby's turn in the conversation. A cancelled message shows nowhere in the conversation.
- **Edit** cancels them the same way, then puts their words back in the box, before anything typed there. When a turn took them first (`dropped` empty), nothing changes: they're in the conversation.

### Stop (COMPOSER-12)

- Stop pressed reads "Stopping…" (dimmed, turning, not pressable) until the turn really ends, or for at most 10 seconds, after which Stop can be pressed again.

### Drop, @ and / (COMPOSER-13 to 16)

- Files dropped on the box are attached as + › Add files attaches them: a tag each, a photo with its preview; the box's outline lights while files are over it. A folder dropped isn't attached; dropped alone, the box says to use + › Select working folder…. The room's box (words only) takes no drop.
- `@` at the start of a word lists, after the Obys it could be, up to 20 files of the conversation's folders (its own, or its project's) whose path holds what's typed after it, asked 120 ms after the last keystroke, the latest answer winning. Picking one puts `@` and its path (after its folder's name when the conversation has several) in the words.
- obid answers `GET /v1/conversations/{id}/files?q=` with `{"files": [{"folder", "path"}]}`: git's files in a repository (tracked, and untracked but not ignored), else a walk that skips hidden folders, `node_modules`, `target`, `dist`, `build` and `__pycache__` and follows no link, at most 5000 names a folder; names starting with `q` first, then names holding it, then paths holding it, shorter first.
- `/` as the first thing in the box lists the chosen tool's own commands (the one the conversation talks through) where obid can list them. It lists none yet, so it says what's missing: `<Tool>'s own commands can't be listed here yet: Oby doesn't read them from its sessions, and words to Oby don't run them. Say what you want instead.` Esc puts it away until the `/` goes.

### Failures and recovery

- An obid without `composer-v1`: no chips, no waiting row; `@` lists Obys only; the box is as before.
- Chips obid can't use (a mode outside the four, a tool it doesn't know, a model or effort without a tool, a model id it can't pass on, an effort not the tool's, another key) are `422` (`400` for another key) and nothing of the words is kept; the box keeps them.
- A Cancel or Edit obid refuses says why, and the row stays.
- `@` files that can't be read answer an empty list.

### Invariants and boundaries

- The mode chip never changes Settings, and `defaults` never picks the tool over words that name one. An account's model and effort change only from the account menu, as Settings › Agents changes them.
- Bypass from a chip starts a job without a Confirm only on words an app of the owner's signed (docs/contracts/signed-words.md: `defaults` is signed with the body, never one of the words).
- The files list reads only names, only in the folders obid keeps for that conversation, never a path the request names; another profile's conversation is `404`.

## Interfaces and dependencies

- `GET /v1/status`: `composer-v1` in `capabilities` (docs/api/capabilities.md).
- `POST /v1/plans`: `defaults` (docs/api/conversations.md).
- `POST /v1/plans/cancel`: `turn`, and `dropped` in its answer (docs/api/conversations.md).
- `GET /v1/conversations/{id}/files?q=` (docs/api/conversations.md).
- A job's `effort_named` is kept in its file, never in the API.

## Acceptance scenarios

- **COMPOSER-1:** Given a box whose words go to Oby, when the owner picks a mode on the Mode chip, then the chip shows it, it's kept for that box (another box keeps its own), and nothing is sent to Settings. Test: `composer_1_the_mode_chip_is_the_boxs_own` (`apps/electron/src/renderer/src/test/composer.test.tsx`).
- **COMPOSER-2:** Retired by COMPOSER-17: there is no model chip.
- **COMPOSER-3:** Given a mode set and an obid with `composer-v1`, when the owner sends words, then `POST /v1/plans` carries it as `defaults` and the chip stays; with none set, or an obid without `composer-v1`, no `defaults`. Tests: `composer_3_the_chips_go_with_the_words`, `composer_3_chips_are_kept_per_box`, `composer_3_only_the_mode_goes` (`chips.test.ts`).
- **COMPOSER-4:** The chip's mode is the job's; Bypass only on signed words. Test: `composer_4_the_chips_mode_is_the_jobs` (`core/crates/obid/src/brain/defaults.rs`).
- **COMPOSER-5:** The chip's tool where the brain left it auto; its model and effort only on it, the effort holding over the account's until the job changes tool. Tests: `composer_5_the_chips_model_and_effort_hold_on_their_tool`, `composer_5_a_named_effort_holds_over_the_accounts` (`src/jobs/tuning.rs`).
- **COMPOSER-6:** Unusable chips are refused. Tests: `composer_6_defaults_are_checked`, `composer_6_unusable_defaults_are_refused` (`tests/plans_api.rs`).
- **COMPOSER-7:** Words answered together take the latest words' chips. Test: `composer_7_words_answered_together_take_the_latest_chips` (`src/brain/turn.rs`).
- **COMPOSER-8:** Words queued behind a turn under way show over the box, not in the conversation. Test: `composer_8_waiting_words_show_over_the_box`.
- **COMPOSER-9:** Cancel drops the one message. Tests: `composer_9_cancel_drops_the_waiting_words`, `composer_9_one_waiting_message_is_cancelled_alone` (`tests/plans_api.rs`).
- **COMPOSER-10:** Edit puts the words back in the box once obid dropped them, and changes nothing when a turn took them. Test: `composer_10_edit_puts_the_words_back`.
- **COMPOSER-11:** obid drops waiting words alone, never the turn under way. Test: `composer_11_cancelled_waiting_words_go_alone` (`src/brain/turn.rs`).
- **COMPOSER-12:** Stop reads "Stopping…" until the turn ends. Test: `composer_12_stop_says_stopping_until_the_turn_ends`.
- **COMPOSER-13:** Dropped files are attached; a folder alone is explained. Tests: `composer_13_dropped_files_are_attached`, `composer_13_folders_are_told_apart` (`composerInput.test.ts`).
- **COMPOSER-14:** `@` lists the conversation's files after the Obys and puts the picked path in the words. Test: `composer_14_at_lists_the_conversations_files`.
- **COMPOSER-15:** obid lists a folder's files as above. Tests: `composer_15_files_of_a_folder_match_what_is_typed`, `composer_15_a_repository_offers_what_git_does` (`src/conversation_files.rs`).
- **COMPOSER-16:** `/` at the start says what's missing for the chosen tool. Test: `composer_16_slash_says_what_is_missing`.
- **COMPOSER-17:** Given the box, then it has one Auto (the talk pill's) and no model chip; an account's `>` list has its efforts under its models (none for Cursor), its own checked, and picking one keeps it on the account; a model an earlier build's chip kept is nothing. Tests: `composer_17_one_auto_the_accounts`, `composer_17_a_model_an_earlier_build_kept_is_nothing` (`chips.test.ts`), `COMPOSER-17: an account's efforts are under its models…`, `COMPOSER-17: Cursor's effort is in its model's name…` (`talkMenu.test.tsx`).
- **COMPOSER-18:** Given the usage gauge pressed, then its panel is the account menu's width, its right edge on the gauge's, 4 px over it. Test: `composer_18_the_usage_panel_is_the_account_menus_width_on_its_gauge`.
- **COMPOSER-19:** Given words sent while Oby answers, when obid sends their `turn` (queued) and the first words' `turnDelivery` moves (starting, running), then the later words show in the waiting row, not the thread, and move into the thread when their own `turnDelivery` says a turn took them; an older copy come late moves nothing back. Tests: `composer_19_waiting_words_follow_obids_delivery_events`, `composer_19_main_keeps_the_delivery_too` (`composerLive.test.ts`).

- **COMPOSER-20:** Given a conversation where the owner said three things, when they press ↑ in its empty box, then the box holds their latest words, ↑ again the ones before, and at the oldest ↑ changes nothing. Tests: `composer_20_up_in_an_empty_box_brings_back_earlier_words`, `composer_20_history_is_the_owners_words_once_each` (`recall.test.tsx`).
- **COMPOSER-21:** Given words ↑ brought back, when the owner presses ↓, then the next newer ones show, and past the newest the box is empty; in an empty box ↓ does nothing. Test: `composer_21_down_goes_forward_then_back_to_an_empty_box`.
- **COMPOSER-22:** Given the owner's own words in the box (typed, or recalled and changed), or ↑ with a modifier, then the arrows aren't the history's; with the `@` list open, they're the list's. Tests: `composer_22_the_owners_own_words_keep_their_arrows`, `composer_22_an_open_at_list_keeps_its_arrows`.
- **COMPOSER-23:** Given words selected in Oby's reply, then **Reply about this** shows; words of the owner's own, or across two replies, show none. Test: `composer_23_words_picked_in_a_reply_show_reply_about_this` (`replySelection.test.tsx`).
- **COMPOSER-24:** Given Reply about this pressed, then the selected words go into the box as a Markdown quote above the caret, an empty line after it, on a line of their own under words already there, and the button and the selection go. Tests: `composer_24_the_words_go_in_as_a_quote_above_the_caret` (`recall.test.tsx`), `composer_24_a_click_quotes_the_words_into_the_box` (`replySelection.test.tsx`).

### Test it when it's done (real Mac)

1. In a conversation, set Mode to Plan, and on the account chip open a Claude Code account's `>` and pick a model, then High under Effort; ask for a small change: the task starts in Plan on that model and effort; Settings › Agents shows that account's model and effort; Settings' mode is unchanged. The box shows one Auto.
2. While Oby answers, send two more messages: both show over the box as Waiting; Cancel one and Edit the other: the first is gone, the second's words are back in the box, and Oby's answer carries on.
3. Press Stop while Oby answers: "Stopping…" until the answer ends.
4. Drop two files on the box: two tags. Type `@` and part of a file's name in a conversation with a folder: the file is listed; pick it.
5. Type `/`: the list says what's missing.
6. Press the usage gauge: its panel is as wide as the account chip's menu, over the gauge, not across the whole box.
7. In a conversation where you've said a few things, press ↑ in the empty box: your last words come back; ↑ again, the ones before; ↓ until the box is empty. Type something, then ↑: the caret moves.
8. Select a few words of Oby's reply: Reply about this shows by them; click it: they're in the box as a `>` quote, the caret under it.
