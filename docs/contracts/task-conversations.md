# Task conversations: every task has one

The business behavior contract for where a task's words go ([#1631](https://github.com/obi-ai-bot/oby/issues/1631)), from [the template](../business-contract-template.md). Scenarios are numbered `TASKCONV-n`; the tests that prove them are named after them (`core/crates/obid/tests/plans_api.rs`, `github_work.rs`, `conversation_harness.rs`, `src/brain/`, `apps/electron/src/renderer/src/test/taskActions.test.ts`).

## Feature and status

- **Outcome:** Whatever started a task, it has a conversation, and every task that follows it up is in the same one. What the owner tells a task, Oby's answer and the follow-up's card stay together, in one place the owner can open.
- **Status:** **Implemented** (Wellington, 2026-10-30). Before, a task started from the Tasks view or from GitHub had no conversation. Words sent from its page started a follow-up that showed only as another task row, and "Go to conversation" opened the task's bare thread, so the owner's question and Oby's answer were lost.
- **Scope:** obid's side: which conversation a task and its follow-ups are in, and what goes into it. **Out of scope:** how the side panel shows a conversation's tasks and follow-ups (nested under it, as Claude Code nests sessions, or not), which is Sangam's design (the preview he was sent on 2026-10-30), and tasks started for other Obys over an Obysphere.

## Behavior

### Actors and ownership

- **obid** decides a task's conversation and writes its turns: `crate::task_conversation`, with `Jobs::join_conversation`.
- **The app** sends a task page's words with `"conversation": true` and opens the conversation obid names. It decides nothing.

### Rules

- **A task started on its own gets a conversation** (`POST /v1/jobs`, `POST /v1/github/work/{id}/start`), named after the task. Its first turn is the owner's words when the task's words are theirs (`POST /v1/jobs`'s `task`), with the task's card under them. Then comes Oby's line saying it started, in Oby's existing words (`On it. Claude Code is investigating …`), with the card.
- **Words from GitHub are never the owner's.** A task from GitHub gets no owner turn, and its conversation counts as having read something from outside, because its title is GitHub's.
- **A follow-up is in its task's conversation.** obid's follow-up carries its job's conversation on (`jobs::follow_up`), so every task started from the first one is in the same conversation.
- **A task started in a conversation keeps it.** It never gets a second conversation.
- **A task with none gets one when the owner writes to it.** This covers a task kept from before this change, or one whose conversation was deleted. The first time its page's words reach obid, its whole chain gets the conversation: the first task, the follow-ups and theirs. It is named after the first task, opens with Oby's line and that task's card, and then has the words and Oby's answer.
- **Starting the task matters more.** When a conversation can't be made, the task starts without one, and the log says so.

### Invariants

- obid's API only grows: `conversation` was already a job key; no key is renamed or removed.
- No turn puts words from outside (GitHub's) in the owner's mouth.

## Acceptance scenarios

- **TASKCONV-1:** A task started from the Tasks view (`POST /v1/jobs`) answers with a `conversation`, listed with the task's title. Its turns are the owner's words, then Oby's `On it. …` line with the task's card, and the task's `turn` is the words'. (`taskconv_1_a_task_from_the_tasks_view_gets_its_own_conversation`)
- **TASKCONV-2:** A task started from the GitHub feed has a conversation named after it, with Oby's line and its card and no owner turn. (`taskconv_2_a_task_from_github_gets_its_own_conversation`, github_work.rs)
- **TASKCONV-3:** Words to a finished task from its page start a follow-up in the same conversation, with the words and Oby's answer, which carries the follow-up's card, on its end. No other conversation is made. (`taskconv_3_a_follow_up_joins_its_tasks_conversation`)
- **TASKCONV-4:** A task whose conversation is gone gets a new one the first time its page's words reach it, and so does the follow-up they start. The app sends those words with `conversation: true` and takes the task back in the conversation. (`taskconv_4_a_task_with_no_conversation_gets_one_when_told_something`; the vitest `TASKCONV-4: …`)
- **TASKCONV-5:** A task started in a conversation stays in it, with no second conversation. (`taskconv_5_a_task_from_a_conversation_keeps_it`)

## The owner's word in a conversation (#1668, "Oby forbids nothing")

- **Outcome:** What the owner tells Oby in a conversation, Oby does: it merges a pull request into any base (`main` too), writes on GitHub, answers a card waiting in the inbox (a merge card's Merge, a job's plan, its agent's question, a permission, a Confirm), and changes Settings. The limits are the code host's (branch protection, required reviews) and the agents' own; when one stops a request, Oby says what stopped it. Nothing the owner asks is refused on Oby's own account.
- **Status:** **Implemented** (2026-10-31).
- **What counts as the owner's word:** a turn whose words an app of the owner's signed (docs/contracts/signed-words.md), that read nothing untrusted (the web, a repository's files, a job's report, Tetrix, an attachment), and that someone watches (not a supervisor's or an event's turn). An inbox item made after the turn began is one the owner hasn't seen: it is never settled by that turn's words.
- **Otherwise:** what the owner asked becomes a Confirm card in the inbox, saying in obid's words what it does and quoting what goes out; their click carries it out. It is never an error. Text Oby read never decides anything, whatever it says.
- **What keeps a click either way:** letting a job reach a secret (#586), and approving a teammate's pull request over the Obysphere or keeping that standing permission (crate::sphere::approval).
- **obid owns** the decision (`brain::tools::owners_word`, `brain::actions::owners_call`) and the doing (`brain::owners_word`: Oby's own GitHub login, which no agent gets; the secret scan on every write first). **The apps** show the cards and the answers; they decide nothing.
- **Defaults kept** (what Oby does unasked): Confirm before a Bypass job when Bypass isn't the default, before deleting a job, before saving a routine; a turn nobody watches merges, writes and changes nothing.

### Acceptance scenarios

- **TASKCONV-6:** A turn is the owner's word only when its words were signed, it read nothing untrusted and someone watches it; words said together are signed only when every one was. (`words_wait_for_the_turn_under_way_and_are_answered_together`, `taskconv_9_the_owners_signed_words_settle_what_waits`)
- **TASKCONV-7:** `oby_merge` merges any open pull request into any base, `main` included: a job's through its card's Merge path, anyone else's with GitHub's merge pinned to the head it read; one closed or merged says so; on a turn that isn't the owner's word it is a Confirm (`merge_pr`). (`taskconv_7_any_open_pull_request_merges_into_any_base`; the human drive's `merge` step)
- **TASKCONV-8:** `oby_github_write` comments, approves, requests changes, closes, labels and opens an issue, each its one REST call; one missing what it needs is refused before anything is sent; the secret scan passes what goes first; on a turn that isn't the owner's word it is a Confirm quoting what goes (`github_write`). (`taskconv_8_each_github_write_is_its_rest_call`)
- **TASKCONV-9:** `oby_answer` on the owner's word answers a merge card, a job's plan, its agent's question, a review's Approve, a permission and a Confirm made before the turn; otherwise each is a Confirm of exactly that answer, a Confirm already waiting stays on its card, a bare no is Oby's, and a notice is Oby's to dismiss. A secret, and a teammate's approval, wait for the owner's click either way. `oby_tell_job` to a job asking a question answers it the same way. (`taskconv_9_a_merge_card_is_answered_on_the_owners_word`, `owners_call_leaves_a_confirm_item_on_its_card`, `owners_call_never_lets_a_secret_through`, `taskconv_9_the_owners_signed_words_settle_what_waits`, `oby_tell_job_answers_a_jobs_question_only_on_the_owners_word`)
- **TASKCONV-10:** `oby_settings` changes how many jobs run at once, account failover and the default mode; a default mode that asks less waits for a Confirm unless it's the owner's word; the rest is done at once. (`taskconv_10_only_a_default_mode_that_asks_less_widens`, `taskconv_9_the_owners_signed_words_settle_what_waits`)
- **TASKCONV-11:** After a turn read something untrusted, changing a routine (pausing it too) and turning check-ins on are Confirms, never "say it again". (`a_routine_said_in_chat_waits_for_the_owners_confirm`, `a_check_in_turned_off_posts_nothing_and_a_failed_call_still_says_what_happened`)
- **TASKCONV-12:** The owner's word sets no cap on how many jobs one turn starts or tells; a conversation may `save_session` to Tetrix, scanned and in the use log as a write. (`taskconv_12_a_turn_does_each_at_most_ten_times_unless_the_owner_said`, `a_conversation_asks_tetrix_for_reads_with_its_turns_token`)
- **TASKCONV-13:** `oby_dispatch` takes the owner's base branch, rounds and epic. (`taskconv_13_dispatch_takes_a_base_rounds_and_an_epic`)

Test it when it's done, on a real Mac: start a task from the Tasks view, open its conversation, then send a follow-up from its page, and see the question, Oby's answer and the follow-up's card together there.
