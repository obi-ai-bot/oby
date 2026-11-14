# Decisions

The owner's decisions that AGENTS.md dates, and those taken out of a contract in `docs/contracts/` (the template's "Current behavior, not history"), one file each, with the owner's words, what each replaced, the contracts it touches and whether it still holds. AGENTS.md or the contract states the rule; the decision file holds the history.

The decision files quote AGENTS.md, and name its sections, as it stood before the rewrite of R13-03 (`git show 3ed5099c:AGENTS.md`). AGENTS.md now states each rule once, without its dates, and links its decision file.

## Naming

A decision file is named `YYYY-MM-DD-<slug>.md`: the date AGENTS.md gives the decision, then a slug of lowercase ASCII letters, digits and hyphens. A date that carries several separate decisions has several files; clauses that are one decision share a file. Two files have other names: this README, and `not-decisions.md`, which lists the dates in AGENTS.md that mark something other than a decision (a finding, a contract note, a consequence of a decision, an issue's date).

## Format

Every decision file has these parts, in this order:

1. `# <title>`
2. `- **Date:** YYYY-MM-DD`
3. `- **Status:** In force`, or `- **Status:** Superseded by` and a link to the file that replaced or retired it. A decision is superseded only where AGENTS.md says a later one replaces or retired it.
4. `- **Replaced:**` and a link to the decision it replaced; or, where AGENTS.md gives that rule no date, the rule in AGENTS.md's words (with its issue number when AGENTS.md gives one); or `Nothing AGENTS.md names`.
5. `- **Changed by:**` and links to later decisions that AGENTS.md says amend it without replacing it (its "since ..." clauses and the like), or `Nothing`.
6. `- **Contracts and scenarios:**` and links to the contracts in `docs/contracts/`, with the scenario ids AGENTS.md ties to the decision, or `None named in AGENTS.md`.
7. `## The owner's words`: one or more single-line blockquotes, each copied exactly from AGENTS.md, each followed by a line that says either "The owner's words, as AGENTS.md quotes them." or "AGENTS.md's wording, not a quote."
8. `## Where AGENTS.md states it`: the AGENTS.md section or sections.
9. Optionally, `## Notes`: the earlier decision a file amends, and anything else AGENTS.md says about it.

A rule AGENTS.md replaced but gives no date (the earlier Touch ID, Face ID and password approvals, #1563's ask for jobs, #130's login rule) has no file of its own: the file that replaced it names it under **Replaced**.

`2026-11-13-roadmap-answers.md` is different: a batch of the owner's answers to the roadmap tasks' questions, kept as it was written and listed below under its date.

## Decisions from a contract

A decision taken out of a contract is filed the same way: named `YYYY-MM-DD-<slug>.md`, with the date the contract gave it, and with the nine parts above, the contract standing where they name AGENTS.md, but for three:

- **Contracts and scenarios** names the contract and the scenario ids the decision came from.
- Under `## The owner's words`, each blockquote is followed by "The owner's words, as <contract> quoted them." or "<contract>'s wording, not a quote."
- Part 8 is `## Where it is stated`: the contract and its scenarios, and the AGENTS.md section that states the rule too, if AGENTS.md states it.

The contract links the file from the rule as `([why](../decisions/<file>.md))` and keeps the decision's code (`LANE-6a`, `ASK-LESS`) as the rule's label. Other history taken out of a contract (what a fix replaced, a finding and its date, when a scenario was implemented) goes in the pull request's body, not here.

## Index

Every date that appeared in AGENTS.md at `3ed5099c`, with its files, and the files added since.

| Date | Files |
| --- | --- |
| 2026-10-09 | [not-decisions.md](not-decisions.md) |
| 2026-10-10 | [2026-10-10-pr-opens-by-itself.md](2026-10-10-pr-opens-by-itself.md)<br>[not-decisions.md](not-decisions.md) |
| 2026-10-17 | [2026-10-17-claude-keychain-login-copy.md](2026-10-17-claude-keychain-login-copy.md) |
| 2026-10-18 | [2026-10-18-claude-keychain-on-by-itself.md](2026-10-18-claude-keychain-on-by-itself.md)<br>[2026-10-18-merge-workflow-is-learned.md](2026-10-18-merge-workflow-is-learned.md)<br>[2026-10-18-sangams-design-leads.md](2026-10-18-sangams-design-leads.md)<br>[2026-10-18-tetrix-connected-once.md](2026-10-18-tetrix-connected-once.md) |
| 2026-10-19 | [2026-10-19-claude-code-parity-first.md](2026-10-19-claude-code-parity-first.md) |
| 2026-10-21 | [2026-10-21-guest-jobs-on-a-teammates-mac.md](2026-10-21-guest-jobs-on-a-teammates-mac.md) |
| 2026-10-23 | [2026-10-23-core-audit.md](2026-10-23-core-audit.md)<br>[2026-10-23-cursor-keychain-login.md](2026-10-23-cursor-keychain-login.md)<br>[2026-10-23-publish-by-hand.md](2026-10-23-publish-by-hand.md)<br>[2026-10-23-the-launch-build.md](2026-10-23-the-launch-build.md) |
| 2026-10-24 | [2026-10-24-approvals-are-a-click.md](2026-10-24-approvals-are-a-click.md)<br>[2026-10-24-conversation-account-failover.md](2026-10-24-conversation-account-failover.md)<br>[2026-10-24-every-job-publishes-by-itself.md](2026-10-24-every-job-publishes-by-itself.md)<br>[2026-10-24-honest-end-for-hand-published-prs.md](2026-10-24-honest-end-for-hand-published-prs.md)<br>[2026-10-24-obid-added-accounts-refreshed.md](2026-10-24-obid-added-accounts-refreshed.md)<br>[2026-10-24-oby-carries-a-task-the-whole-way.md](2026-10-24-oby-carries-a-task-the-whole-way.md)<br>[2026-10-24-org-members-words.md](2026-10-24-org-members-words.md) |
| 2026-10-25 | [2026-10-25-slack-cut.md](2026-10-25-slack-cut.md) |
| 2026-10-26 | [2026-10-26-electron-is-the-desktop-app.md](2026-10-26-electron-is-the-desktop-app.md)<br>[2026-10-26-own-lane-for-claude-code-jobs.md](2026-10-26-own-lane-for-claude-code-jobs.md)<br>[2026-10-26-plans-live-in-oby-plan.md](2026-10-26-plans-live-in-oby-plan.md)<br>[2026-10-26-work-here-is-oby-only.md](2026-10-26-work-here-is-oby-only.md) |
| 2026-10-27 | [2026-10-27-bypass-after-an-untrusted-read.md](2026-10-27-bypass-after-an-untrusted-read.md)<br>[2026-10-27-own-lane-race-accepted.md](2026-10-27-own-lane-race-accepted.md)<br>[2026-10-27-strip-to-a-dispatcher.md](2026-10-27-strip-to-a-dispatcher.md) |
| 2026-10-28 | [2026-10-28-the-social-contract.md](2026-10-28-the-social-contract.md) |
| 2026-10-29 | [2026-10-29-planning-in-layers.md](2026-10-29-planning-in-layers.md)<br>[2026-10-29-sangam-owns-ui-ux-dx.md](2026-10-29-sangam-owns-ui-ux-dx.md) |
| 2026-10-30 | [2026-10-30-signed-words-risk-accepted.md](2026-10-30-signed-words-risk-accepted.md)<br>[2026-10-30-the-check-is-the-bases.md](2026-10-30-the-check-is-the-bases.md)<br>[2026-10-30-two-reviews-for-untrusted-or-security-paths.md](2026-10-30-two-reviews-for-untrusted-or-security-paths.md)<br>[2026-10-30-vault-file-on-a-mac.md](2026-10-30-vault-file-on-a-mac.md) |
| 2026-10-31 | [2026-10-31-oby-forbids-nothing.md](2026-10-31-oby-forbids-nothing.md)<br>[not-decisions.md](not-decisions.md) |
| 2026-11-01 | [2026-11-01-remote-access-in-electron.md](2026-11-01-remote-access-in-electron.md) |
| 2026-11-02 | [2026-11-02-a-job-waits-for-its-own-reset.md](2026-11-02-a-job-waits-for-its-own-reset.md)<br>[2026-11-02-chatgpt-sign-in.md](2026-11-02-chatgpt-sign-in.md)<br>[2026-11-02-swift-mac-app-deleted.md](2026-11-02-swift-mac-app-deleted.md)<br>[not-decisions.md](not-decisions.md) |
| 2026-11-03 | [2026-11-03-cursor-usage-from-api2.md](2026-11-03-cursor-usage-from-api2.md) |
| 2026-11-05 | [2026-11-05-dispatcher-not-harness.md](2026-11-05-dispatcher-not-harness.md) |
| 2026-11-06 | [2026-11-06-split-it.md](2026-11-06-split-it.md)<br>[2026-11-06-tool-api-hosts-dont-mark-a-job.md](2026-11-06-tool-api-hosts-dont-mark-a-job.md) |
| 2026-11-08 | [2026-11-08-answers-on-merging-30-and-31.md](2026-11-08-answers-on-merging-30-and-31.md)<br>[2026-11-08-couldnt-judge.md](2026-11-08-couldnt-judge.md)<br>[2026-11-08-own-reads-dont-mark.md](2026-11-08-own-reads-dont-mark.md)<br>[2026-11-08-reviews-only.md](2026-11-08-reviews-only.md)<br>[2026-11-08-test-the-task-names.md](2026-11-08-test-the-task-names.md) |
| 2026-11-09 | [2026-11-09-lane-12.md](2026-11-09-lane-12.md)<br>[2026-11-09-own-lane-by-default.md](2026-11-09-own-lane-by-default.md) |
| 2026-11-10 | [2026-11-10-a-job-at-a-limit-moves.md](2026-11-10-a-job-at-a-limit-moves.md)<br>[2026-11-10-agents-below-checks.md](2026-11-10-agents-below-checks.md)<br>[2026-11-10-api-words-stay-quoted.md](2026-11-10-api-words-stay-quoted.md)<br>[2026-11-10-check-stops-when-stalled.md](2026-11-10-check-stops-when-stalled.md)<br>[2026-11-10-hand-review-reads.md](2026-11-10-hand-review-reads.md)<br>[2026-11-10-kept-merge.md](2026-11-10-kept-merge.md)<br>[2026-11-10-lane-10a.md](2026-11-10-lane-10a.md)<br>[2026-11-10-lane-6a.md](2026-11-10-lane-6a.md)<br>[2026-11-10-review-reads-current-head.md](2026-11-10-review-reads-current-head.md)<br>[2026-11-10-second-review-waits.md](2026-11-10-second-review-waits.md)<br>[2026-11-10-wait-when-no-room.md](2026-11-10-wait-when-no-room.md) |
| 2026-11-11 | [2026-11-11-afterpr-1.md](2026-11-11-afterpr-1.md)<br>[2026-11-11-afterpr-3.md](2026-11-11-afterpr-3.md)<br>[2026-11-11-ask-less.md](2026-11-11-ask-less.md)<br>[2026-11-11-check-hands-off-to-blok.md](2026-11-11-check-hands-off-to-blok.md)<br>[2026-11-11-codex-1-bypass-only.md](2026-11-11-codex-1-bypass-only.md)<br>[2026-11-11-lane-6b.md](2026-11-11-lane-6b.md)<br>[2026-11-11-lanes-4a.md](2026-11-11-lanes-4a.md)<br>[2026-11-11-lanes-ask.md](2026-11-11-lanes-ask.md)<br>[2026-11-11-managed-1.md](2026-11-11-managed-1.md)<br>[2026-11-11-unsafe-1.md](2026-11-11-unsafe-1.md) |
| 2026-11-12 | [2026-11-12-codex-2.md](2026-11-12-codex-2.md)<br>[2026-11-12-roadmap-is-the-task-files.md](2026-11-12-roadmap-is-the-task-files.md) |
| 2026-11-13 | [2026-11-13-apple-watch-app-removed.md](2026-11-13-apple-watch-app-removed.md)<br>[2026-11-13-drop-the-mac-merge-gate.md](2026-11-13-drop-the-mac-merge-gate.md)<br>[2026-11-13-roadmap-answers.md](2026-11-13-roadmap-answers.md) (a batch of answers) |
