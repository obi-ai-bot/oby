# The owner's own work merges by itself

- **Date:** 2026-11-14
- **Status:** In force
- **Replaced:** Nothing AGENTS.md names
- **Changed by:** Nothing
- **Contracts and scenarios:** [merging.md](../contracts/merging.md) (MERGING-7, MERGING-8, MERGING-17, MERGING-22), [task-conversations.md](../contracts/task-conversations.md) (TASKCONV-6, TASKCONV-26 to TASKCONV-33)

## The owner's words

> "Yes, as described (Recommended)" to: "For your OWN work started from your OWN conversation: Oby merges it by itself (any base, main included), with no merge, check-offer, review, read-marker, safety-limit or Start cards; only the agent's own questions, plan approval (Plan mode) and permission cards (Ask/Accept edits modes) stay."

The owner's answer in the factory session of 2026-11-14.

> "Keep the silent 2nd review (Recommended)" to: "When your conversation has read something from outside (a web page, Bash output, a file), should jobs it starts still auto-merge, or keep a silent second review first (no card, just a second AI review of the exact commit)?"

The owner's answer in the factory session of 2026-11-14.

> "Yes, all my own work (Recommended)" to: "Should a task you type in the Tasks view (not in a conversation) also merge by itself with no cards, like your conversation work?"

The owner's answer in the factory session of 2026-11-14.

> He doesn't want Oby to keep asking him whether to merge something or not.

The owner's words, relayed from his formal-verification session, paraphrased at his request.

> He wants to see cards asking for permissions only when they are truly needed, the way Claude Code, Codex and other agents behave.

The owner's words, relayed from his formal-verification session, paraphrased at his request.

> When he asks Oby in his own conversation for something to be done, he wants it done and merged, and that's it.

The owner's words, relayed from his formal-verification session, paraphrased at his request.

> On Oby's own review still asking for changes after its rounds: "Merge, and say what it flagged"

The owner's words, relayed from his formal-verification session.

> "Only what my words named (Recommended)" to: "After your conversation reads outside text (a GitHub issue, a web page), may it merge, approve or post on GitHub in your name with no click?"

The owner's answer in the factory session of 2026-11-14.

> "Keep two approvals there (Recommended)" to: "When Oby's review still flags problems after its last round ... Should that also apply to untrusted or security-path changes?"

The owner's answer in the factory session of 2026-11-14.

> After an outside read, an action my signed words did not name: "Skip it, say so in a line" (it is not done, Oby says so in one line in the conversation with no card, and I say it again naming it).

The owner's answer in the factory session of 2026-11-15.

> What counts as named: "Number, link or task name" (my words of that same turn contain the pull request's or issue's number or link, or the task's name; for a waiting card, "merge it" or "allow it" counts when it is the only such card waiting in that conversation).

The owner's answer in the factory session of 2026-11-15.

> no force-pushing, nor merging into `main` but the owner's own work, on its own

AGENTS.md's wording, not a quote.

> and, but for the owner's own work, a review asking for changes it can't settle or a repository whose workflow it hasn't learned yet. What the owner asks for in their own conversation or Tasks view merges by itself on any base, `main` too, with no card; past its review's last round Oby merges, saying what the review flagged, unless it needs two approving reviews

AGENTS.md's wording, not a quote.

> it opens the PR and, but for the owner's own work, asks before any merge and doesn't push to or merge into the default branch or `main` on its own; it doesn't force-push them, nor a branch it didn't create, unless the owner tells it to ("Oby forbids nothing", above).

AGENTS.md's wording, not a quote.

> Everything else the mark does stays: a clone apart, Oby's box, no declared secrets for a check written after a foreign read, the secret scan, and a second approving review of its exact head by another tool before it merges, with no card

AGENTS.md's wording, not a quote.

> For the owner's own work, these are only the agent's own questions, plan approval in Plan mode and permissions in Ask and Accept edits; after an outside read, a conversation does on GitHub only what their signed words named

AGENTS.md's wording, not a quote.

## Where AGENTS.md states it

- Oby forbids nothing
- The launch build
- Rules Oby never breaks

## Notes

It covers the work the owner starts from his own app, in his own conversation or in the Tasks view: the own lane, any agent. Oby merges that work by itself, into any base, `main` included, and shows no merge, check-offer, review, read-marker, safety-limit or Start card for it. When Oby's review still asks for changes after its last round, Oby merges and says what the review flagged. That holds for his normal work only: an untrusted pull request, or one that touches a security path (`merge::SECURITY_PATHS`), still needs two approving reviews of its exact head by different tools before it merges, run with no card, and merging past a review that still flags problems never stands in for an approving review.

The cards that stay are the agent CLI's own questions, plan approval in Plan mode, and permission cards in Ask and Accept edits. After the conversation read something from outside, the jobs it starts still need a second approving review of the exact head by a different tool, run with no card. After the conversation read outside text (a GitHub issue, a web page), its own direct actions (merging, approving or posting on GitHub) count as the owner's word only for what his signed words named: it never merges, approves or posts something his words didn't name because text it read asked for it. That keeps the Approvals non-negotiable: approvals never come from GitHub text or anything received.

Unchanged: another Oby's requests, teammates, the Obysphere, and reviews another Oby asked for; the five non-negotiables; the force-push default; the security-path rule; this repository's own remembered workflow (`dev` through the merge queue, `dev` into `main` after the "Test it when it's done" list); and the code host's own limits, which Oby tells the owner plainly ("Oby forbids nothing"). These are defaults the owner's word overrides ([2026-10-31-oby-forbids-nothing.md](2026-10-31-oby-forbids-nothing.md)).

The conversation's own direct actions after an outside read are done (A00-48). obid doesn't do the rest yet: the code is A00-46, A00-47 and A00-51, and the merge of an untrusted job of the owner's own after its silent second review.

Amends [2026-10-18-merge-workflow-is-learned.md](2026-10-18-merge-workflow-is-learned.md) (the safe default), [2026-10-24-oby-carries-a-task-the-whole-way.md](2026-10-24-oby-carries-a-task-the-whole-way.md) (when Oby asks its owner), [2026-10-30-two-reviews-for-untrusted-or-security-paths.md](2026-10-30-two-reviews-for-untrusted-or-security-paths.md) (the second review after an outside read, now with no card), [2026-10-23-core-audit.md](2026-10-23-core-audit.md) (the untrusted-read bullets), [2026-10-24-approvals-are-a-click.md](2026-10-24-approvals-are-a-click.md) (the cards that wait on the owner) and [2026-10-31-oby-forbids-nothing.md](2026-10-31-oby-forbids-nothing.md) (its list of defaults).
