# The message box: chips, waiting words, drop, @, /, earlier words and quotes

The business behavior contract for what the message box takes besides typing, done the way T3 Code's composer does it, from [the template](../business-contract-template.md). Scenarios are numbered `COMPOSER-n`; the tests that prove them are named after them (`composer_1_…`).

## Feature and status

- **Outcome:** The owner sets the mode the tasks their words start will run in right in the box, before they start, without changing Settings, and picks the model and effort on the one account menu (one Auto); sees the words they sent while Oby was still answering, and can take them back or change them; sees Stop being honoured; brings files in by dropping them or naming them with `@`; and starts one of their own agent skills with `/`, as in their own Claude Code or Codex.
- **Status:** Implemented (every scenario unless it says it's proposed or retired; COMPOSER-2 retired by COMPOSER-17). Where obid gives no skills, the `/` list is honest about what's missing (COMPOSER-16).
- **Scope:** The Electron app's message box on Home, in a conversation with Oby and, for `/`'s skills alone, in a task's box; obid's `defaults` on `POST /v1/plans`, `turn` on `POST /v1/plans/cancel`, `GET /v1/conversations/{id}/files`, `GET /v1/skills`, `skill` on `POST /v1/plans` and `POST /v1/jobs/{id}/messages`, and `POST /v1/conversations/{id}/restart`. **Out of scope:** a conversation with other people's Obys (no chips, no waiting row, no skills); a task's box's chips and waiting row; running an agent's built-in slash commands; the iPhone and Apple Watch apps (frozen).

## Behavior

### Actors and ownership

- **The owner** sets the mode chip, picks an account's model and effort, sends words, presses Stop, Edit or Cancel, drops files, types `@` or `/`, picks a skill, and restarts a conversation's agent from ⌘K.
- **The Electron app** keeps the mode chip per box (with the box's draft, `chips.ts`: across a relaunch, a week after they were last set, on this Mac only), sends them with the words, shows the waiting words obid reports, attaches dropped files as + › Add files does, lists the skills obid gives for the box, and sends the one picked beside the words.
- **obid** checks the chips, makes them the jobs' the words start, keeps one answer at a time in a conversation (docs/api/conversations.md), drops one waiting message on its own when asked, lists a conversation's files, lists the skills of the agent the words would reach and hands the one picked to that agent in the form it understands (docs/contracts/skills.md, docs/api/skills.md). Oby's brain still picks the tool where the words name one (the dispatcher-not-harness decision, [why](../decisions/2026-11-05-dispatcher-not-harness.md)).

### The chips (COMPOSER-1 to 7)

- One chip sits before the talk pill in a box whose words go to Oby (Home's, a conversation's), once obid lists `composer-v1` (always on the mock): **Mode** (Default, then Ask, Accept edits, Plan, Bypass, each with its line). Default is muted.
- It is a default for this box's words alone. It stays after a send, for the next words. Nothing writes it to Settings (`PUT /v1/settings/permissions`).
- Sending carries the mode, when one is set, as `defaults` (`{mode}`); nothing when the chip is left alone; an answer to Oby's question carries none. obid still takes `agent`, `model` and `effort` in `defaults` (below, for any client: the API only grows); this app sends none of them.
- obid makes them the jobs the words start (`oby_dispatch`, `oby_offer`): the chip's mode is the job's, over the brain's; Bypass from the chip on words an app of the owner's signed is their own word, so the job starts with no Confirm; on unsigned words it's treated as the brain's Bypass (the owner's default, or not taken). The chip's tool is the job's where the brain left the tool `auto`; its model and effort hold only on that tool, and a job that moves to another tool takes that tool's account's.
- Words answered together carry the latest words' chips.

### One Auto (COMPOSER-17)

- The box has one Auto: the talk pill's (Auto · Whichever has room, then each account with its model and usage, a `>` to its models, then Add account). There is no model chip.
- An account's `>` list holds its models, then, for a tool whose effort isn't in its model's name (Claude Code, Codex; not Cursor), a line, **Effort**, and that model's efforts as pills, four to a row, the one the account runs on checked.
- Picking a model or an effort keeps it on the account (`PATCH /v1/accounts/{id}`, as Settings › Accounts does) and talks through that account, as picking a model always has (CONVERSATION-AGENT-19).
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
- `/` as the first thing in the box lists the skills of the agent the words would reach (COMPOSER-25, below). With none to list it says obid's reason when obid gives one, else what's missing: `<Tool>'s own commands can't be listed here yet: Oby doesn't read them from its sessions, and words to Oby don't run them. Say what you want instead.` (the tool the conversation talks through). Esc puts it away until the `/` goes.

### Skills (COMPOSER-25 to 28)

- Once obid lists `skills-v1` (always on the mock), `/` as the first thing in a box lists the skills obid gives for it: a conversation's box asks `GET /v1/skills?conversation=<id>`, Home's box `GET /v1/skills`, a task's box `GET /v1/skills?job=<id>`. A task page's box whose words go on the end of its conversation asks for that conversation's.
- It asks each time the list opens. It lists only the skills the owner may start (`userInvocable`), each by its name and its description on one line. What's typed after the `/` narrows them, case aside: names starting with it, then names holding it, then descriptions holding it.
- ↑ and ↓ move, Enter or Tab picks, Esc closes, as in the `@` list. A typed `/` that matches nothing says so.
- Picking one takes the `/` and what follows out of the box and shows the skill as a tag at the start of the box's tags: `/` and its name, with an ×. One at a time: another picked takes its place. Backspace with the cursor at the very start of the words, or in an empty box, takes it out.
- Sending sends `skill` beside the words, on `POST /v1/plans` or, for a task's box, `POST /v1/jobs/{id}/messages`. The words never get `/<name>` put in them. A skill picked is something to send on its own: with no words, send is lit, Enter sends it, and the words go empty (`said: ""`, `text: ""`). The tag goes with that send: unlike the mode chip, it is for those words only.
- Words where the owner typed `/name` by hand carry no `skill`: they reach the agent as words.
- In a conversation, ⌘K's Actions hold **Restart agent**. It sends `POST /v1/conversations/{id}/restart` and says `Agent restarted: your next message starts it fresh.`, or `Nothing to restart: your next message starts the agent fresh anyway.` when no agent waited.

### Failures and recovery

- An obid without `composer-v1`: no chips, no waiting row; `@` lists Obys only; the box is as before.
- An obid without `skills-v1`: the app asks for no skills, `/` is COMPOSER-16's line alone (none in a task's box), no `skill` is ever sent, and ⌘K has no Restart agent.
- A skills list obid doesn't give (a refusal, no answer) is an empty one.
- Chips obid can't use (a mode outside the four, a tool it doesn't know, a model or effort without a tool, a model id it can't pass on, an effort not the tool's, another key) are `422` (`400` for another key) and nothing of the words is kept; the box keeps them.
- A Cancel or Edit obid refuses says why, and the row stays.
- `@` files that can't be read answer an empty list.

### Invariants and boundaries

- The mode chip never changes Settings, and `defaults` never picks the tool over words that name one. An account's model and effort change only from the account menu, as Settings › Accounts changes them.
- Bypass from a chip starts a job without a Confirm only on words an app of the owner's signed (docs/contracts/signed-words.md: `defaults` is signed with the body, never one of the words).
- The files list reads only names, only in the folders obid keeps for that conversation, never a path the request names; another profile's conversation is `404`.

## Interfaces and dependencies

- `GET /v1/status`: `composer-v1` in `capabilities` (docs/api/capabilities.md).
- `POST /v1/plans`: `defaults` (docs/api/conversations.md).
- `POST /v1/plans/cancel`: `turn`, and `dropped` in its answer (docs/api/conversations.md).
- `GET /v1/conversations/{id}/files?q=` (docs/api/conversations.md).
- `GET /v1/status`: `skills-v1`; `GET /v1/skills`, `skill` on `POST /v1/plans` and `POST /v1/jobs/{id}/messages`, and `POST /v1/conversations/{id}/restart` (docs/api/skills.md). Main signs `skill` with the body, never as one of the words (docs/contracts/signed-words.md).
- A job's `effort_named` is kept in its file, never in the API.

## Acceptance scenarios

- **COMPOSER-1:** Given a box whose words go to Oby, when the owner picks a mode on the Mode chip, then the chip shows it, it's kept for that box (another box keeps its own), and nothing is sent to Settings. Its menu, like every menu (`Menu.tsx`: `menuPlace`, `menuShift`), opens wholly inside the window, 8px from its edges, moved back in sideways where lined up with its chip it would run past the right edge (a 900px window). Tests: `composer_1_the_mode_chip_is_the_boxs_own` (`apps/electron/src/renderer/src/test/composer.test.tsx`), `menuLayout.test.ts` (the box's and the side panel's menus at 1280px and 900px).
- **COMPOSER-2:** Retired by COMPOSER-17: there is no model chip.
- **COMPOSER-3:** Given a mode set and an obid with `composer-v1`, when the owner sends words, then `POST /v1/plans` carries it as `defaults` and the chip stays; with none set, or an obid without `composer-v1`, no `defaults`. Tests: `composer_3_the_chips_go_with_the_words`, `composer_3_chips_are_kept_per_box`, `composer_3_only_the_mode_goes` (`chips.test.ts`).
- **COMPOSER-4:** The chip's mode is the job's; Bypass only on signed words. Where Bypass is the owner's default (Settings › Permissions), a Bypass chip on unsigned words is that default: the task starts in Bypass with no Confirm, as the same words with no chip do. Tests: `composer_4_the_chips_mode_is_the_jobs` (`core/crates/obid/src/brain/defaults.rs`), `plans_api::composer_4_a_bypass_chip_where_bypass_is_the_default_asks_nothing`.
- **COMPOSER-5:** The chip's tool where the brain left it auto; its model and effort only on it, the effort holding over the account's until the job changes tool. Tests: `composer_5_the_chips_model_and_effort_hold_on_their_tool`, `composer_5_a_named_effort_holds_over_the_accounts` (`src/jobs/tuning.rs`).
- **COMPOSER-6:** Unusable chips are refused. Tests: `composer_6_defaults_are_checked`, `composer_6_unusable_defaults_are_refused` (`tests/plans_api.rs`).
- **COMPOSER-7:** Words answered together take the latest words' chips. Test: `composer_7_words_answered_together_take_the_latest_chips` (`src/brain/turn.rs`).
- **COMPOSER-8:** Words queued behind a turn under way show over the box, not in the conversation. Test: `composer_8_waiting_words_show_over_the_box`.
- **COMPOSER-9:** Cancel drops the one message. Tests: `composer_9_cancel_drops_the_waiting_words`, `composer_9_one_waiting_message_is_cancelled_alone` (`tests/plans_api.rs`).
- **COMPOSER-10:** Edit puts the words back in the box once obid dropped them, their files back as tags (sent again from where obid keeps them; one this app didn't send says so and never goes without it), and changes nothing when a turn took them. Tests: `composer_10_edit_puts_the_words_back`, `t3_queue_edit_brings_back_the_files_of_the_waiting_words_too`, `t3_queue_edit_files_go_again_from_where_obid_keeps_them_and_never_without_them`.
- **COMPOSER-11:** obid drops waiting words alone, never the turn under way. Test: `composer_11_cancelled_waiting_words_go_alone` (`src/brain/turn.rs`).
- **COMPOSER-12:** Stop reads "Stopping…" until the turn ends. Test: `composer_12_stop_says_stopping_until_the_turn_ends`.
- **COMPOSER-13:** Dropped files are attached; a folder alone is explained. Tests: `composer_13_dropped_files_are_attached`, `composer_13_folders_are_told_apart` (`composerInput.test.ts`).
- **COMPOSER-14:** `@` lists the conversation's files after the Obys and puts the picked path in the words. Test: `composer_14_at_lists_the_conversations_files`.
- **COMPOSER-15:** obid lists a folder's files as above. Tests: `composer_15_files_of_a_folder_match_what_is_typed`, `composer_15_a_repository_offers_what_git_does` (`src/conversation_files.rs`).
- **COMPOSER-16:** `/` at the start, where obid gives no skills, says obid's reason or what's missing for the chosen tool. Tests: `composer_16_slash_says_what_is_missing` (`composer.test.tsx`, `composerInput.test.ts`), `composer_25_with_none_and_no_why_it_says_what_is_missing` (`composerSkills.test.tsx`).
- **COMPOSER-17:** Given the box, then it has one Auto (the talk pill's) and no model chip; an account's `>` list has its efforts under its models (none for Cursor), its own checked, and picking one keeps it on the account; a model an earlier build's chip kept is nothing. Tests: `composer_17_one_auto_the_accounts`, `composer_17_a_model_an_earlier_build_kept_is_nothing` (`chips.test.ts`), `COMPOSER-17: an account's efforts are under its models…`, `COMPOSER-17: Cursor's effort is in its model's name…` (`talkMenu.test.tsx`).
- **COMPOSER-18:** Given the usage gauge pressed, then its panel is the account menu's width, its right edge on the gauge's, 4 px over it. Test: `composer_18_the_usage_panel_is_the_account_menus_width_on_its_gauge`.
- **COMPOSER-19:** Given words sent while Oby answers, when obid sends their `turn` (queued) and the first words' `turnDelivery` moves (starting, running), then the later words show in the waiting row, not the thread, and move into the thread when their own `turnDelivery` says a turn took them; an older copy come late moves nothing back. Tests: `composer_19_waiting_words_follow_obids_delivery_events`, `composer_19_main_keeps_the_delivery_too` (`composerLive.test.ts`).

- **COMPOSER-20:** Given a conversation where the owner said three things, when they press ↑ in its empty box, then the box holds their latest words, ↑ again the ones before, and at the oldest ↑ changes nothing. Tests: `composer_20_up_in_an_empty_box_brings_back_earlier_words`, `composer_20_history_is_the_owners_words_once_each` (`recall.test.tsx`).
- **COMPOSER-21:** Given words ↑ brought back, when the owner presses ↓, then the next newer ones show, and past the newest the box is empty; in an empty box ↓ does nothing. Test: `composer_21_down_goes_forward_then_back_to_an_empty_box`.
- **COMPOSER-22:** Given the owner's own words in the box (typed, or recalled and changed), or ↑ with a modifier, then the arrows aren't the history's; with the `@` list open, they're the list's. Tests: `composer_22_the_owners_own_words_keep_their_arrows`, `composer_22_an_open_at_list_keeps_its_arrows`.
- **COMPOSER-23:** Given words selected in Oby's reply, then **Reply about this** shows; words of the owner's own, or across two replies, show none. Test: `composer_23_words_picked_in_a_reply_show_reply_about_this` (`replySelection.test.tsx`).
- **COMPOSER-24:** Given Reply about this pressed, then the selected words go into the box as a Markdown quote above the caret, an empty line after it, on a line of their own under words already there, and the button and the selection go. Tests: `composer_24_the_words_go_in_as_a_quote_above_the_caret` (`recall.test.tsx`), `composer_24_a_click_quotes_the_words_into_the_box` (`replySelection.test.tsx`).
- **COMPOSER-25:** Given an obid with `skills-v1`, when the owner types `/` first in a conversation's, Home's or a task's box, then the list holds the `userInvocable` skills of `GET /v1/skills` with that box's query, asked each time it opens, narrowed by what's typed (names first), with ↑ ↓, Enter, Tab and Esc; with none, obid's `why`, else COMPOSER-16's line; an obid without it is asked nothing. It shows as Claude's own slash menu does (the owner's reference, 2026-11-23): a compact popover over the box's left edge, a third of its width, as tall as fits above the box and at most about 13 rows, scrolling inside with the last row cut; each row its name alone; the lit row kept in view as ↑ ↓ move, its description in a tip beside it and no other's; after a lone `/`, "Type to filter" past it. Esc closes it, the words and the focus left in the box; a click anywhere outside it (the conversation, the side panel, elsewhere in the box) closes it picking nothing, and the click is still the outside's own. A box no one sees (Home's, under a page) holds no list open, and Esc there is that page's: a list open as the page came over it stays closed, its words kept, until its `/` or `@` is typed again; a box with none open opens its list as ever once it shows. Given 40 skills, when the owner types `/` and presses ↓ 20 times, then the list starts inside the window, the 21st row is lit and in view, and only its description shows. Tests: `composer_25_the_list_is_bounded_and_scrolls_with_the_lit_row_in_view`, `composer_25_only_the_lit_rows_description_shows_beside_it`, `composer_25_esc_closes_it_leaving_the_words_and_the_focus_in_the_box`, `composer_25_a_click_outside_closes_it_picking_nothing`, `composer_25_a_hidden_box_holds_no_list_and_leaves_esc_to_the_page`, `composer_25_an_empty_box_hidden_and_shown_again_still_opens_its_list`, `composer_25_esc_closes_it_with_the_focus_elsewhere_and_gives_the_box_its_focus_back` (`slashMenu.test.tsx`, on the real message box), `composer_25_slash_lists_the_skills_obid_gives_for_the_box`, `composer_25_each_box_asks_for_its_own_skills`, `composer_25_with_none_to_list_the_panel_says_why`, `composer_25_with_none_and_no_why_it_says_what_is_missing`, `composer_25_keys_move_pick_and_close_the_list`, `composer_25_an_obid_without_skills_v1_lists_none` (`apps/electron/src/renderer/src/test/composerSkills.test.tsx`).
- **COMPOSER-26:** Given a skill picked, then the `/` words leave the box and the skill is the first tag, `/` and its name with an ×; another picked replaces it; Backspace at the very start of the words, or in an empty box, or its ×, takes it out. Tests: `composer_26_a_picked_skill_is_a_chip_before_the_words`, `composer_26_backspace_at_the_start_takes_the_chip_out`, `composer_26_its_x_takes_it_out` (`composerSkills.test.tsx`), `composer_26_backspace_at_the_start_of_the_real_editor_is_the_chips` (`skillChip.test.tsx`).
- **COMPOSER-27:** Given a skill's tag, when the owner sends, then `skill` goes beside the words on `POST /v1/plans` or `POST /v1/jobs/{id}/messages`, the words without `/<name>`, and the tag goes; with no words, the tag alone is sent, with empty words; words with `/name` typed by hand carry none; an obid without `skills-v1` never gets one; main signs it with the body. Tests: `composer_27_the_picked_skill_goes_beside_the_words`, `composer_27_a_skill_alone_is_sent`, `composer_27_a_skill_alone_is_sent_to_obid`, `composer_27_words_typed_as_slash_name_carry_no_skill`, `composer_27_obid_gets_the_skill_beside_the_words`, `composer_27_an_obid_without_skills_v1_never_gets_skill` (`composerSkills.test.tsx`).
- **COMPOSER-28:** Given a conversation open and an obid with `skills-v1`, when the owner picks ⌘K › Restart agent, then `POST /v1/conversations/{id}/restart` is sent and a line says the agent restarted, or that there was nothing to restart; with no conversation open or no `skills-v1`, it isn't offered. Tests: `composer_28_cmd_k_restarts_the_conversations_agent`, `composer_28_nothing_waiting_says_so`, `composer_28_only_in_a_conversation_on_an_obid_with_skills_v1` (`restartAgent.test.tsx`).

### Test it when it's done (real Mac)

1. In a conversation, set Mode to Plan, and on the account chip open a Claude Code account's `>` and pick a model, then High under Effort; ask for a small change: the task starts in Plan on that model and effort; Settings › Accounts shows that account's model and effort; Settings' mode is unchanged. The box shows one Auto.
2. While Oby answers, send two more messages: both show over the box as Waiting; Cancel one and Edit the other: the first is gone, the second's words are back in the box, and Oby's answer carries on.
3. Press Stop while Oby answers: "Stopping…" until the answer ends.
4. Drop two files on the box: two tags. Type `@` and part of a file's name in a conversation with a folder: the file is listed; pick it.
5. In a conversation talking through Claude Code with a skill of your own installed, type `/` and part of its name: it's listed with its description; pick it with Enter: it's a tag at the start of the box; type what it's for and send: the agent runs that skill on those words, and the tag is gone. Pick one again and press Backspace in the empty box: the tag goes. In a conversation in Oby's box, `/` says why skills aren't offered there. Install a skill, then ⌘K › Restart agent: the line says the agent restarted, and `/` lists the new skill.
6. Press the usage gauge: its panel is as wide as the account chip's menu, over the gauge, not across the whole box.
7. In a conversation where you've said a few things, press ↑ in the empty box: your last words come back; ↑ again, the ones before; ↓ until the box is empty. Type something, then ↑: the caret moves.
8. Select a few words of Oby's reply: Reply about this shows by them; click it: they're in the box as a `>` quote, the caret under it.
