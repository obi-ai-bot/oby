# Wire: a value the app doesn't know shows neutral

The business behavior contract for how the apps read a value obid sends that they don't know yet, from [the template](../business-contract-template.md). Roadmap task R07-01, milestone R07 One wire contract.

## Feature and status

- **Outcome:** An Electron build keeps working when obid sends a status, state, kind or agent it doesn't know: the list it's in still shows, the other items show as before, and that one shows neutral, with nothing to answer. obid can then grow its values (R07-02, R04) without a newer obid taking an older app's screens down.
- **Status:** WIRE-2 to WIRE-5 are **implemented** in the Electron app (R07-01). WIRE-1, the iPhone's, stays **proposed**: the phone apps are frozen ([the owner's answers of 2026-11-13](../decisions/2026-11-13-roadmap-answers.md), "Phone apps: frozen, not deleted"), and it waits on that freeze and on R07-02. The look is the owner's answer [q015](../decisions/2026-11-13-roadmap-answers.md#r07-01-q015), "Gray badge, no text change"; the iPhone release is held by [q016](../decisions/2026-11-13-roadmap-answers.md#r07-01-q016).
- **Scope:** In: every closed string union in `apps/electron/src/shared/obid-wire.ts` that obid fills in, open to any string (`| (string & {})`), and every reader of the job status, the inbox kind, the agent, the stage state and the transcript's kinds and states in the renderer. Out: the iPhone, Apple Watch and Android apps (frozen); obid sending any new value (R07-02, R04); generated wire types (R07-06, R07-07); the pickers' part of an unknown agent (R08-02); new words, labels or design for the unknown state; `docs/runner-api.md`, since obid's API doesn't change.

## Behavior

### Actors and ownership

- **obid** owns nothing new: it sends what it sends, and decides nothing here.
- **The Electron app** reads obid's JSON as it comes (the wire types are TypeScript types only: nothing fails at decode time) and shows a value it doesn't know neutral. The values it knows are written once, in `obid-wire.ts` (`JOB_STATUSES`, `AGENT_IDS`, `INBOX_KINDS`, with `isJobStatus`, `isAgent`, `isInboxKind`), and every check for an unknown one imports them.

### The rule

A value this build doesn't know is shown gray, with no text change, no action, never a known state's mark or word, and nothing thrown (q015). Where a word would go, the app shows obid's raw value, as the ask card's label already did, or nothing; an `aria-label` for it is obid's raw value. No string a person reads is added.

### States and transitions

- **An unknown job status (WIRE-3).** The task stays placed where it was, as done (its tab and side panel group), so other tasks don't move, and carries `unknown` on its `Task`. It gets no ask: no "It failed" or "It was interrupted" card, and no inbox item's buttons, so it never needs you. Its status mark is a still gray dot named by obid's status (`.status-dot.is-unknown`), never the check, the pulse, orange or red; its row in a conversation says no status word; its menu offers neither Stop task nor Archive task (View details, Open conversation, Rename and Delete stay); its card shows no result line or Result section, no Stop and no mode menu; the agent's pane gives it a neutral state, and shows no Stop.
- **An unknown inbox kind (WIRE-2).** The item becomes an ask with obid's `headline` (else its `title`), obid's `text`, and no options. Its card draws no option buttons, not even Later, under a gray label (`.tc-ask-kind.is-unknown`) whose text is obid's kind, as before. A task with a known status still needs you for it: obid says it waits on the owner. Items with no job keep today's Home filter, so an unknown kind there stays off Home's stack.
- **An unknown agent (WIRE-4).** `agentOf` (`mock.ts`) gives a known agent's name and mark, and for any other id that id as its name and a plain gray circle (`BRANDS.circle`). Every lookup whose key comes from obid goes through it: a job, an account, a login found on the Mac, a limit decision and an archived task. Removing an account of an unknown tool names no provider.
- **The other values (WIRE-5).** An unknown stage state shows the gray ring still to come, named by obid's value, and an unstyled gray segment on the card's bar. A transcript entry of an unknown kind is left out of the session. An unknown tool call or to-do state shows the gray ring with no meta word, never failed or Declined. An unknown activity group state shows the gray ring too, and a saved rule's unknown decision shows as obid's word, not as Allowed. Unknown triage, inbox state, `priced`, GitHub list, step state, room post `by` and state, room answer and limit state get no new behaviour.

### Failures and recovery

Nothing here fails or retries: the unknown value is shown as it is until a build that knows it reads it.

### Invariants and boundaries

- **Add-only API stays.** obid adds no value to an enum the Swift apps decode as closed: phones already installed still decode them closed (AGENTS.md, "Add-only API"; q016). This contract doesn't change that rule.
- Known values render exactly as before.

## Interfaces and dependencies

`apps/electron/src/shared/obid-wire.ts` (the open unions and the known lists), read by main (`main/notify.ts`) and the renderer. No API, event or persistence changes. WIRE-1 depends on the phone freeze ending and on R07-02.

## Acceptance scenarios

- **WIRE-1** (proposed: waits on the phone freeze and R07-02): Given a job whose status is a value the app doesn't know, when the iPhone reads `GET /v1/jobs`, then the list decodes, the other jobs show as before, and that job shows a neutral state with no action.
- **WIRE-2** (implemented, `wire_2_unknown_inbox_kind_renders` in `apps/electron/src/renderer/src/test/wire.test.tsx`): Given a job with an open inbox item whose kind the app doesn't know, when the Electron app shows its card, then the card shows the item's title, a gray label with its kind and no buttons, and nothing throws; an item of a known kind keeps its buttons and Later.
- **WIRE-3** (implemented, `wire_3_unknown_job_status_shows_a_neutral_mark`): Given a job whose status the app doesn't know, even with an open `failed` or `permission` item, when the Electron app shows it, then it doesn't need you, its mark is a still gray dot named by that status, its row says no status word, and its menu has neither Stop task nor Archive task; a done job beside it shows as before.
- **WIRE-4** (implemented, `wire_4_unknown_agent_renders_with_its_own_name`): Given a job whose agent the app doesn't know, when the Electron app shows its card and its row, then they show that agent's id with a gray circle, and nothing throws.
- **WIRE-5** (implemented, `wire_5_unknown_stage_and_session_values_render_neutral`): Given a stage, a transcript entry, a tool call or a to-do whose state or kind the app doesn't know, when the Electron app shows the task's page or session, then the stage shows the gray ring named by its value, the entry is left out, and the call and to-do show the gray ring with no failed mark and no Declined.
