# Another Oby's task

The business behavior contract for a task one member's Oby hands to another's across their Obysphere, from [the template](../business-contract-template.md): the spec's `handoff` (obi-sphere `protocol.md` 5.3). Scenarios are numbered `HANDOFF-n`; obid's are tested as `handoff_n_…` in `core/crates/obid/tests/handoff.rs`, the app's as `HANDOFF-n: …` in `apps/electron/src/renderer/src/test/handoff.test.ts`.

## Feature and status

- **Outcome:** Ana's Oby asks Ben's to do a piece of work in one of Ben's repositories. Ben sees it on Home, quoted, and says yes or no. On yes, Ben's Oby runs it as a task in Oby's box, the way any work from outside runs, and Ana's Oby gets the result: done or failed, the branch and commit it made, and what its agent said it did.
- **Status:** **Implemented** in obid (receiving, the job, the result, sending) and in the Electron app (Home's card, a card and delegation that take and send `handoff`).
- **Scope:** receiving and running a `handoff`, its result, and sending one from obid's API. **Out of scope:** a screen for sending one (Sangam's: until then the API sends it, `POST /v1/obid/sphere/requests`), pulling the receiver's branch into the sender's repository, and handoffs between an owner's own Obys.

## Behavior

### Actors and ownership

- **The sender** (another Oby) sends a typed `handoff`: `goal` (one line), `repo`, `base_branch`, `acceptance`, optional `summary`, `issue`, `pr`. Every word of it is data.
- **The owner** of the receiving Oby answers it on Home: Accept or Decline, either "and remember for this repo".
- **obid** decides everything else: whether it can start, the job, its lane, the result and the publish gate.
- **The apps** show the item and send the owner's answer. They decide nothing.

### Triggers and prerequisites

The receiving Oby is a member whose card accepts `handoff`; the Obysphere's policy lets the sender's owner send it (sphere-api.md section 5). A card that doesn't accept it declines it `out_of_scope`, as any type obid doesn't take.

### States and transitions

| What happens | What the owner sees, and what goes back |
|---|---|
| A handoff no remembered decision covers | An `obiRequest` item on Home, `Task from <owner>`, its text from typed fields only (`<owner>'s Obi (<obi>) asks your Oby to do a task in <repo> from <base>, quoted below. Conditions: tests pass.`), the goal, summary and conditions under `obi.quoted`. Past the inbox limit, `busy` instead. |
| Accept | obid starts the task (below) and sends `accept`. One it can't start (a repository obid doesn't know, no account, no job sandbox) sends `decline` `unavailable` with obid's note instead, and the item says why. |
| Decline | `decline` `owner_declined`. |
| "…, and remember for this repo" | As for a review request: a decision for `handoff`, the sender's owner and the repository, applied to the next one without asking. |
| The task runs | A task in the owner's list, titled `Task for <owner>: <repo name>`, which the owner can watch, steer and stop as any task. |
| The task ends | One `result`, `for: handoff` (below). |

### The task

- A `build` job in the request's repository, starting from its `base_branch`, on the account routing picks.
- **The foreign lane** (AGENTS.md, "Dispatcher, not harness"): marked untrusted and foreign, so it runs boxed with a copy of its login, in a clone apart from the owner's own, and its check gets none of the owner's secrets. Nothing about it is looser than any other untrusted job.
- **Its prompt is obid's own fixed words.** The request reaches the agent only as a quoted file, `.obi/inputs/handoff.json` (`{"quoted", "fromOwner", "goal", "summary", "repo", "baseBranch", "acceptance"}`), with every secret obid knows taken out: never in the prompt or an argument.
- It publishes as any untrusted job does (docs/contracts/publishing.md): its branch is pushed; a pull request, when the repository takes them, waits for the reviews an untrusted change needs.

### The result

- `outcome` `done` when the job ended done and every condition is met, else `failed`. obid judges one kind: `tests_pass` is met when the job's check ran and passed. Any other kind is not met, which the item's text says before the owner answers.
- `repo`; `acceptance`, one entry per condition with `met`; `head_branch` and `head_sha` when the job made commits on its branch; `pr` when it opened one; `by_rule` when a decision accepted it.
- `summary`: the job's summary, at most 4,000 characters, through the publish gate (a possible secret, or a scan that can't run: a `failed` result with no text, as a review's).
- A job stopped by the owner, or one obid couldn't run, sends a `failed` result. A restart sends the result of a job that ended while obid was down.

### Invariants and boundaries

- Nothing in a handoff starts work before the owner's Accept, or a decision their Accept kept.
- Another Oby's words never reach a prompt, an argument or the job's title: the title is `Task for <owner>: <repo name>`, from typed fields.
- The foreign lane stays boxed (LANES-1): a handoff's job is never own-lane.
- obid's API only grows: `handoff` is a value of `obi.type` the apps already decode as an open string.

## Interfaces and dependencies

- **obid:** `POST /v1/obid/sphere/requests` with `type: "handoff"` sends one (docs/api/obysphere.md); the item and the answer as a review request's (`POST /v1/inbox/{id}/answer`); the exchange as any (`GET /v1/obid/sphere/exchanges`).
- **The Electron app:** its card accepts `review_request`, `question` and `handoff`, and its delegation's scope has `send:handoff`; Home shows the item as the review request's card, titled `Task from <owner>`.
- **The Obysphere:** relays it under its policy. A Sphere whose policy names no `handoff` refuses it `policy_denied`.

## Acceptance scenarios

- **HANDOFF-1** Given a member whose card accepts `handoff`, when another member's Oby sends one, then it waits as one `obiRequest` item titled `Task from <owner>`, whose `text` names the owner, Obi, repository and base and the conditions by kind, and whose `obi.quoted` holds the goal, summary and each condition's note; no job starts.
- **HANDOFF-2** Given that item, when the owner answers Accept, then `accept` goes and one `build` job starts in the request's repository from its base, untrusted, foreign and apart, its task obid's fixed words, the request in `.obi/inputs/handoff.json` and nowhere in the task or title; the exchange is `accepted` with that job.
- **HANDOFF-3** Given that job, when it ends done with its check passed, then one `result` goes, `for: handoff`, outcome `done`, `tests_pass` met, its branch and head commit, and its summary through the publish gate; ended any other way, outcome `failed` with each condition not met.
- **HANDOFF-4** Given the item, when the owner answers Decline, then `decline` `owner_declined` goes and no job starts; "Accept, and remember for this repo" keeps a `handoff` decision that accepts the next one from that owner on that repository without an item.
- **HANDOFF-5** Given a handoff for a repository obid doesn't know, when the owner accepts it, then `decline` `unavailable` goes with obid's note, never `accept`, and no job starts.
- **HANDOFF-6** The Electron app's card accepts `handoff` and its delegation may send one; Home shows a handoff's item titled `Task from <owner>` with the goal quoted and Accept and Decline.
