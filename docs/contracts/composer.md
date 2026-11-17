# The message box: chips, waiting words, drop, @ and /

The business behavior contract for what the message box takes besides typing, done the way T3 Code's composer does it, from [the template](../business-contract-template.md). Scenarios are numbered `COMPOSER-n`; the tests that prove them are named after them (`composer_1_…`).

## Feature and status

- **Outcome:** The owner sets how the tasks their words start will run (the mode, and the tool, model and effort) right in the box, before they start, without changing Settings or an account; sees the words they sent while Oby was still answering, and can take them back or change them; sees Stop being honoured; brings files in by dropping them or naming them with `@`; and runs their own Claude Code's commands and skills by typing `/`, as in their terminal.
- **Status:** Implemented (COMPOSER-1 to 21). `/` lists and runs Claude Code's commands; Codex's and Cursor's aren't listed yet, and the list says so (COMPOSER-16).
- **Scope:** The Electron app's message box on Home and in a conversation with Oby, and obid's `defaults` on `POST /v1/plans`, `turn` on `POST /v1/plans/cancel`, and `GET /v1/conversations/{id}/files`, `GET /v1/commands`, and a command's turn in the conversation harness. **Out of scope:** a task's own box and a conversation with other people's Obys (no chips, no waiting row, no commands); Codex's and Cursor's commands and skills; the iPhone and Apple Watch apps (frozen).

## Behavior

### Actors and ownership

- **The owner** sets the chips, sends words, presses Stop, Edit or Cancel, drops files, and types `@` or `/`.
- **The Electron app** keeps the chips per box (with the box's draft, `chips.ts`: across a relaunch, a week after they were last set, on this Mac only), sends them with the words, shows the waiting words obid reports, and attaches dropped files as + › Add files does.
- **obid** checks the chips, makes them the jobs' the words start, keeps one answer at a time in a conversation (docs/api/conversations.md), drops one waiting message on its own when asked, lists a conversation's files, keeps the commands the owner's own Claude Code names, and hands a command to the conversation's session as its prompt. Oby's brain still picks the tool where the words name one (the dispatcher-not-harness decision, [why](../decisions/2026-11-05-dispatcher-not-harness.md)).
- **Claude Code** owns the commands and skills: what they are, what they do, and running them. obid never reads a command's file, expands one or decides what one means.

### The chips (COMPOSER-1 to 7)

- Two chips sit before the talk pill in a box whose words go to Oby (Home's, a conversation's), once obid lists `composer-v1` (always on the mock): **Mode** (Default, then Ask, Accept edits, Plan, Bypass, each with its line) and **Model** (Auto, then each tool the owner has an account on with its models; beside a model, its efforts, first "Account's effort"). Default and Auto are muted.
- They are defaults for this box's words alone. They stay after a send, for the next words. Nothing writes them to Settings (`PUT /v1/settings/permissions`) or to an account (`PATCH /v1/accounts/{id}`).
- Sending carries what's set as `defaults` (`{mode?, agent?, model?, effort?}`); a model or effort only with its tool; nothing when both chips are left alone; an answer to Oby's question carries none.
- obid makes them the jobs the words start (`oby_dispatch`, `oby_offer`): the chip's mode is the job's, over the brain's; Bypass from the chip on words an app of the owner's signed is their own word, so the job starts with no Confirm; on unsigned words it's treated as the brain's Bypass (the owner's default, or not taken). The chip's tool is the job's where the brain left the tool `auto`; its model and effort hold only on that tool, and a job that moves to another tool takes that tool's account's.
- Words answered together carry the latest words' chips.

### Words that wait (COMPOSER-8 to 11)

- Words sent while a turn is under way in a conversation wait for it (one answer at a time). Every waiting message, as obid reports it (`delivery.status` `queued` behind words a turn has taken), shows as a row over the box (its words, "Waiting", Edit and Cancel), not in the conversation; a turn taking them puts them in the conversation.
- **Cancel** sends `POST /v1/plans/cancel` with the conversation and the words' `turn`: obid drops those words alone (their request answers `none`, they read `cancelled`), and the turn under way and other waiting words go on; the words left are answered as Oby's turn in the conversation. A cancelled message shows nowhere in the conversation.
- **Edit** cancels them the same way, then puts their words back in the box, before anything typed there. When a turn took them first (`dropped` empty), nothing changes: they're in the conversation.

### Stop (COMPOSER-12)

- Stop pressed reads "Stopping…" (dimmed, turning, not pressable) until the turn really ends, or for at most 10 seconds, after which Stop can be pressed again.

### Drop, @ and / (COMPOSER-13 to 16)

- Files dropped on the box are attached as + › Add files attaches them: a tag each, a photo with its preview; the box's outline lights while files are over it. A folder dropped isn't attached; dropped alone, the box says to use + › Select working folder…. The room's box (words only) takes no drop.
- `@` at the start of a word lists, after the Obys it could be, up to 20 files of the conversation's folders (its own, or its project's) whose path holds what's typed after it, asked 120 ms after the last keystroke, the latest answer winning. Picking one puts `@` and its path (after its folder's name when the conversation has several) in the words.
- obid answers `GET /v1/conversations/{id}/files?q=` with `{"files": [{"folder", "path"}]}`: git's files in a repository (tracked, and untracked but not ignored), else a walk that skips hidden folders, `node_modules`, `target`, `dist`, `build` and `__pycache__` and follows no link, at most 5000 names a folder; names starting with `q` first, then names holding it, then paths holding it, shorter first.
### / commands (COMPOSER-16 to 21)

A command runs where the owner's words to Oby go: the conversation's own Claude Code session, which is the owner's own Claude Code on macOS (docs/contracts/lanes.md, LANES-12). Running one is the agent's (Claude Code's own), and which session gets the words is the dispatcher's ([why](../decisions/2026-11-05-dispatcher-not-harness.md)): obid neither starts a task for a command nor reads what a command does; it hands the owner's words to the session unchanged, so Claude Code runs them as it does in their terminal. What's listed is what that same session can run (its room is its working folder, so a repository's own `.claude/commands` and skills aren't listed: a task in that repository has them).

- `/` as the first thing in the box (Home's, a conversation's, once obid lists `composer-v1`) lists the commands of the conversation's tool (the one it talks through; Home's: Claude Code) as obid last saw them (`GET /v1/commands`), each `/name`, a skill marked "Skill". What's typed after the `/`, up to the first space, narrows the list as T3 Code's does: names that are it, then names starting with it, then names with it after a `-`, `_` or `:`, then names holding it, by name within each, at most 50; with nothing typed, every one in Claude Code's order.
- ↑ and ↓ move the lit row, Enter or Tab picks it, a click picks one, Esc puts the list away until the `/` goes. Picking puts `/name ` in the box in place of what was typed: the owner adds what the command takes and sends, or sends it as it is.
- With nothing to list it says what's missing: for Claude Code with none seen yet, `Claude Code’s commands show here once Oby has talked through it on this Mac.`; for Codex or Cursor, `<Tool>’s own commands can’t be listed here yet. Say what you want instead.`; with nothing matching, `No command matches /<typed>.`
- obid keeps the commands an own Claude Code conversation session names in its `init` line: `slash_commands`, but those `terminal_slash_commands` names (they run only in a terminal) and names starting with `_`, each a skill when `skills` names it too, in Claude Code's order, in `home/commands.json` (0600), replaced whole by the next own session's. A boxed session's list (another Oby's conversation, off macOS) and a job's never replace it: neither is where the owner's words go.
- Words run as a command when all of this holds: an app of the owner's signed them; they're one message alone (not answered together with other words, not an answer to Oby's question); nothing is attached to them (no file, no task); their first word is `/name` with `name` one obid keeps; and the turn runs on an own Claude Code session. Then that turn's prompt is the words alone, unchanged: nothing goes before them (no earlier turns, task list, job reports, attached names or repositories' note), so Claude Code takes them as the command and what follows it as its arguments. What the session answers, or the command prints, is Oby's answer in the conversation, as any turn's.
- Anything else starting with `/` is words: a path (`/Users/...`), a name obid doesn't keep, words no app of the owner's signed, words with files or tasks, words answered together, a boxed session's turn, Codex's and Cursor's. Where such words would still start the prompt with a kept name (nothing to say before them), obid puts `These words aren't a command:` on a line before them, so Claude Code reads them as words; an unknown `/name` or a path is words to Claude Code already, and goes as it is.
- The job reports a command's turn didn't tell go with the next turn of the owner's: a command's turn hears none. obid counts any words naming a kept command, with nothing attached, as a command's turn here, so a report may be told twice, never lost.

What Codex and Cursor expose, for when they're listed: Codex's app-server (codex-cli 0.163) answers `skills/list` (each skill's `name`, `description`, `path`, `scope`, `enabled`) and takes a skill as a turn's input (`{"type": "skill", "name", "path"}`); its own `/` commands are its terminal's. Cursor's agent speaks ACP, whose `session/update` `available_commands_update` lists the commands a session takes (not yet checked against cursor-agent 2026.11.13).

### Failures and recovery

- An obid without `composer-v1`: no chips, no waiting row, no `/` list; `@` lists Obys only; the box is as before. One without `GET /v1/commands` (`404`) lists no command, and says what's missing.
- `home/commands.json` that can't be read answers `{"claude": null}`; one that can't be written leaves the last list, logged, never the turn's failure.
- A command Claude Code can't run (one gone since obid saw the list) is Claude Code's to answer, in its own words, as Oby's answer.
- Chips obid can't use (a mode outside the four, a tool it doesn't know, a model or effort without a tool, a model id it can't pass on, an effort not the tool's, another key) are `422` (`400` for another key) and nothing of the words is kept; the box keeps them.
- A Cancel or Edit obid refuses says why, and the row stays.
- `@` files that can't be read answer an empty list.

### Invariants and boundaries

- The chips never change Settings or an account, and never pick the tool over words that name one.
- Bypass from a chip starts a job without a Confirm only on words an app of the owner's signed (docs/contracts/signed-words.md: `defaults` is signed with the body, never one of the words).
- The files list reads only names, only in the folders obid keeps for that conversation, never a path the request names; another profile's conversation is `404`.
- A command runs only on the owner's own signed words, in their own Claude Code session: never on words another Oby, a job, a script or anything read sent, and never in a boxed session. obid keeps only commands' names, never what a command or skill says or does.

## Interfaces and dependencies

- `GET /v1/status`: `composer-v1` in `capabilities` (docs/api/capabilities.md).
- `POST /v1/plans`: `defaults` (docs/api/conversations.md).
- `POST /v1/plans/cancel`: `turn`, and `dropped` in its answer (docs/api/conversations.md).
- `GET /v1/conversations/{id}/files?q=` (docs/api/conversations.md).
- `GET /v1/commands` (docs/api/commands.md), from Claude Code's stream-json `system`/`init` line (`slash_commands`, `terminal_slash_commands`, `skills`; claude 2.1.317 and later).
- A job's `effort_named` is kept in its file, never in the API.

## Acceptance scenarios

- **COMPOSER-1:** Given a box whose words go to Oby, when the owner picks a mode on the Mode chip, then the chip shows it, it's kept for that box (another box keeps its own), and nothing is sent to Settings. Test: `composer_1_the_mode_chip_is_the_boxs_own` (`apps/electron/src/renderer/src/test/composer.test.tsx`).
- **COMPOSER-2:** Given the Model chip, when the owner picks a tool's model and an effort, then the chip shows the model and effort, and nothing is sent to the account. Test: `composer_2_the_model_chip_sets_tool_model_and_effort`.
- **COMPOSER-3:** Given chips set and an obid with `composer-v1`, when the owner sends words, then `POST /v1/plans` carries them as `defaults` and the chips stay; with none set, or an obid without `composer-v1`, no `defaults`. Tests: `composer_3_the_chips_go_with_the_words`, `composer_3_chips_are_kept_per_box` (`chips.test.ts`).
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
- **COMPOSER-16:** `/` at the start lists the conversation tool's commands as obid kept them, narrowed by what's typed, skills marked; with none, it says what's missing. Tests: `composer_16_slash_lists_the_agents_commands`, `composer_16_slash_says_what_is_missing` (`composer.test.tsx`), `composer_16_commands_rank_as_t3s_do` (`composerInput.test.ts`).
- **COMPOSER-17:** Picking a command (Enter, Tab or a click) puts `/name ` in the box in place of what was typed. Test: `composer_17_picking_a_command_puts_it_in_the_box`.
- **COMPOSER-18:** obid keeps an own Claude Code conversation session's commands from its `init` line, terminal-only ones and `_` names left out, skills marked, and answers them on `GET /v1/commands`; a boxed session's never replace them. Tests: `composer_18_commands_are_read_from_init`, `composer_18_the_next_list_replaces_the_last` (`src/commands.rs`), `composer_18_an_own_conversation_keeps_its_commands`, `composer_18_a_boxed_sessions_commands_are_not_kept` (`tests/conversation_harness.rs`).
- **COMPOSER-19:** The owner's signed `/name` words, alone, with nothing attached, go to an own Claude Code session as the turn's whole prompt. Tests: `composer_19_a_command_is_the_whole_prompt` (`tests/conversation_harness.rs`), `composer_19_words_alone_run_as_a_command` (`src/commands.rs`).
- **COMPOSER-20:** Other words starting with `/` are words, quoted as before. Tests: `composer_20_other_slash_words_are_words` (`src/commands.rs`), `composer_20_unsigned_or_attached_words_are_words` (`tests/conversation_harness.rs`).
- **COMPOSER-21:** A command's turn hears no job report; the next turn does. Test: `composer_21_a_command_turn_hears_no_reports` (`tests/conversation_harness.rs`).

### Test it when it's done (real Mac)

1. In a conversation, set Mode to Plan and Model to a Claude Code model with High; ask for a small change: the task starts in Plan on that model and effort; Settings and the account are unchanged.
2. While Oby answers, send two more messages: both show over the box as Waiting; Cancel one and Edit the other: the first is gone, the second's words are back in the box, and Oby's answer carries on.
3. Press Stop while Oby answers: "Stopping…" until the answer ends.
4. Drop two files on the box: two tags. Type `@` and part of a file's name in a conversation with a folder: the file is listed; pick it.
5. Type `/` in a conversation with Oby that has answered once: the owner's own commands and skills are listed; type `/sim` and pick `/simplify`; send it: Claude Code's skill runs and its answer is Oby's.
6. Type `/Users/` and a question: it's answered as words.
