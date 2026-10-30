# Task conversations: every task has one

The business behavior contract for where a task's words go ([#1631](https://github.com/obi-ai-bot/oby/issues/1631)), from [the template](../business-contract-template.md). Scenarios are numbered `TASKCONV-n`; the tests that prove them are named after them (`core/crates/obid/tests/plans_api.rs`, `github_work.rs`, `apps/electron/src/renderer/src/test/taskActions.test.ts`).

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

Test it when it's done, on a real Mac: start a task from the Tasks view, open its conversation, then send a follow-up from its page, and see the question, Oby's answer and the follow-up's card together there.
