# Merging: no silent waits, a repository without GitHub checks, and the stages' real words

The business behavior contract for what happens to a task's pull request after Oby publishes it: Oby's review, the merge by a workflow the owner taught it, and the cards that ask the owner when it can't go on by itself. Sangam's design ([#1564](https://github.com/obi-ai-bot/oby/pull/1564): `apps/electron/src/renderer/src/TaskCard.tsx`, the task's stages; the inbox's cards; `src/main/notify.ts`, the alerts), from [the template](../business-contract-template.md). Issue [#1636](https://github.com/obi-ai-bot/oby/issues/1636), from the core audit of 2026-10-30. Scenarios are numbered `MERGING-n`; the tests that prove them are named after them (`merging_1_…`).

## Feature and status

- **Outcome:** Oby carries a task the whole way, and asks the owner only when something goes wrong (AGENTS.md). Every way a merge can stop ends in something the owner can act on: a card that says why and what to do, its words on the task's Merged stage. A repository without GitHub checks (Oby's own, where Actions is off) merges on the job's own check.
- **Status:** **Implemented** (#1636). Before it, a conflict, a branch behind its base, checks that never finished and a signed-out `gh` were silent waits; a `mac` workflow never merged and its card offered only Dismiss; reviews that ran out raised no inbox item; the task card replaced the Review stage's words with "Waiting for review".
- **Scope:** obid's merge follower (`core/crates/obid/src/merge.rs`, `Jobs::carry` in `jobs.rs`), Oby's review rounds, the pull request's description, and the Electron task card and alerts. **Out of scope:** a rebase round Oby starts by itself for a conflict or a branch behind its base (obid's rule: a pull request that can't merge cleanly gets no rebase round by itself; docs/runner-api.md, "What the runner never does"); the Swift Mac app (frozen).

## What Sangam's design assumes (each one kept)

| The design | What obid promises |
|---|---|
| A task's stages, each with a line under it. | `pipeline.stages[].detail`: obid's own words, shown as they are ("Oby is reviewing", "Pull request open · waiting for your review on GitHub", why it isn't merged). |
| Cards in the inbox, answered in one click. | `merge` items with their options; `failed` items with theirs. |
| An alert when something needs you, titled as what needs you. | `mergeNeedsYou` and `mergeError` on the job; the card's title. |

## Behavior

### Actors and ownership

- **obid** reads the pull request every 3 minutes (`OBID_FOLLOW_SECONDS`), decides whether it may merge, merges with the owner's own `gh`, and raises the cards.
- **The job's own check** runs in the job's box; obid keeps the commit it last passed on, clean (`Job::checked`, never in the API).
- **The owner** answers cards (Merge, Leave it open, Dismiss, One more round) in their own app, or asks Oby to rebase in a message to the task.
- **The Electron app** shows obid's words: stages, cards and alerts.

### Scenarios

- **MERGING-1:** A repository whose workflow waits on the Mac's checks (`checks: mac`) and has no GitHub checks merges by itself once the job's own check passed in its box on exactly the head being merged, with nothing left uncommitted, as long as every other rule holds (MERGING-7).
- **MERGING-2:** Where the job's own check can't stand for the Mac's, the owner is asked with a card titled `Ready to merge: <task>` that offers **Merge** and **Leave it open**: the check said it left the Mac's own tests for the Mac (`Not merged: its check passed, and left the Mac's own tests for this Mac, so it's yours to merge.`; scripts/oby-check.sh in a box says `run them on the Mac before merging`), it passed on another commit (`Not merged: its check didn't pass on exactly this commit, so it's yours to merge.`), or the job has no check (`Not merged: no check ran on its head (this repository has none), so it's yours to merge.`). The owner's Merge merges it (`mergedBy` `owner`); it lifts only the missing check, never another rule.
- **MERGING-3:** A pull request that conflicts with its base (`CONFLICTING`, `DIRTY`, or a conflict GitHub finds only when asked to merge) asks the owner: `Not merged: it conflicts with dev. Ask Oby to rebase it onto dev, or settle it on GitHub.`, with Dismiss. Nothing is sent.
- **MERGING-4:** A branch behind a base whose rules take only up-to-date branches (`BEHIND`) asks the owner: `Not merged: it's behind dev, and GitHub merges only up-to-date pull requests into dev. Ask Oby to rebase it onto dev, or update it on GitHub.` A workflow that rebases first (`rebaseFirst`) and a branch not on its base's latest commit: `Not merged: <repo> rebases onto dev first, and it isn't on its latest commit. Ask Oby to rebase it onto dev.` Oby starts no rebase round by itself.
- **MERGING-5:** GitHub's checks still running an hour after obid pushed the head ask the owner: `Not merged: its checks on GitHub are still running an hour after Oby pushed it. Look at them on GitHub.` A `github` workflow with no checks on the head an hour after the push: `Not merged: GitHub still has no checks on its head an hour after Oby pushed it, and <repo> merges once they pass. Look at it on GitHub.` Within the hour it waits quietly. Checks that finish later merge it by themselves, and the card is settled.
- **MERGING-6:** obid's `gh` signed out, or its login expired: `Not merged: obid's gh isn't signed in to GitHub. Sign in to GitHub again, and Oby carries on.` Once it's signed in again, the merge goes on by itself. A pull request obid couldn't read (`pullRequestError`) shows under the stage that waits on it, Merged or Review.
- **MERGING-7:** Nothing here relaxes a rule: Oby merges by itself only under a workflow the owner agreed to remember; never into `main`, `master` or `trunk`; only Oby's approval (or the workflow's reviewer's) of exactly the head obid pushed; only with a clean secret scan of it. The owner's Merge lifts only MERGING-2's missing check.
- **MERGING-8:** When Oby's reviews run out (`review::REVIEWS`, 3) still asking for changes, the owner gets one inbox item (`failed`, `<job>-review-<review job>`): `Oby's review still asks for changes. Take it from here.`, with **One more round**. One more round hands that review to the job's agent once more, past the limit; Oby reviews what it pushes, and if that review still asks for changes, a new item replaces the old.
- **MERGING-9:** No event turn of Oby's answers that item (or the item of a review that couldn't run): an answer from Oby would loop it.
- **MERGING-10:** A done task's stages show obid's own words: the Review stage's detail as obid says it (never only "Waiting for review"), and the Merged stage's why it isn't merged (`mergeError`, the open card's text).
- **MERGING-11:** The pull request's description says how it merges: under a workflow the owner taught Oby, `Oby merges it by itself, the way its owner taught it: <the workflow's sentence>`; otherwise `Oby merges it only when its owner says so.`
- **MERGING-12:** The alert for a merge that waits on the owner is titled as its card is (`Ready to merge`, `Needs a teammate`, `Not merged`), not always "Not merged".

### Failure and recovery

- A card answered (Leave it open, Dismiss) isn't asked again for that head and reason; a new head, or a new reason, brings a new card, and the old one is settled.
- A read of GitHub that fails waits and reads again, as before: only the states above end in a card.

## Invariants

- obid's API is add-only: no key is renamed or removed. `Job::checked` is kept in the job's file, never in the API.
- Oby pushes nothing while merging, so it force-pushes nothing.
- Text from GitHub stays data: card words are obid's own.

## Tests

| Scenario | Test |
|---|---|
| MERGING-1 | `merge::tests::merging_1_the_jobs_own_check_on_its_head_is_the_macs`, `merge_api::merging_1_the_jobs_own_check_merges_a_repository_without_github_checks` |
| MERGING-2 | `merge::tests::merging_2_where_the_check_doesnt_count_the_owners_merge_does`, `merge_api::merging_2_the_owners_merge_merges_where_the_check_doesnt_count` |
| MERGING-3 | `merge::tests::it_merges_only_cleanly`, `merge::tests::nothing_merges_unless_everything_holds_right_then`, `merge::tests::a_merge_github_refused_says_why`, `merge_api::merging_3_a_conflict_asks_the_owner` |
| MERGING-4 | `merge::tests::it_merges_only_cleanly`, `merge::tests::nothing_merges_unless_everything_holds_right_then` |
| MERGING-5 | `merge::tests::merging_5_checks_still_running_an_hour_on_ask_the_owner` |
| MERGING-6 | `merge_api::merging_6_a_signed_out_gh_asks_the_owner`, `apps/electron/src/renderer/src/test/merging.test.tsx` (`MERGING-6:`) |
| MERGING-7 | `merge::tests::it_never_merges_into_main_or_without_a_workflow`, `merge::tests::only_the_head_obid_pushed_and_scanned`, `merge::tests::merging_1_…`, `merge::tests::merging_2_…` |
| MERGING-8 | `publish_api::merging_8_reviews_that_run_out_ask_the_owner_with_one_more_round` |
| MERGING-9 | `brain::orchestrator::tests::inbox_answers_are_limited_to_the_allow_list` |
| MERGING-10 | `apps/electron/src/renderer/src/test/merging.test.tsx` (`MERGING-10:`), `merge_api::merging_3_a_conflict_asks_the_owner` |
| MERGING-11 | `publish::tests::merging_11_the_description_says_how_it_merges` |
| MERGING-12 | `apps/electron/src/renderer/src/test/merging.test.tsx` (`MERGING-12:`) |

## Test it when it's done (on a real Mac)

1. In this repository (workflow `checks: mac`, Actions off), let a task's check run `scripts/oby-check.sh` on an obid-only change and publish: once Oby's review approves, it merges into `dev` by itself and says "Merged into dev, as you like".
2. A change that touches `Engine/` (the box leaves `swift test` for the Mac): the inbox shows `Ready to merge: <task>` with Merge; Merge merges it.
3. Make a task's pull request conflict with `dev` (merge a conflicting change first): within 3 minutes the task's Merged stage and a card say it conflicts and what to do.
4. `gh auth logout` on the Mac obid runs as: the card says obid's gh isn't signed in; `gh auth login` again, and the merge goes on.
