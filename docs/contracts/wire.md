# Wire: a value the app doesn't know shows neutral

The business behavior contract for how the apps read a value obid sends that they don't know yet, from [the template](../business-contract-template.md). Roadmap task R07-01, milestone R07 One wire contract.

## Feature and status

- **Outcome:** An Electron build keeps working when obid sends a status, state, kind or agent it doesn't know: the list it's in still shows, the other items show as before, and that one shows neutral, with nothing to answer. obid can then grow its values (R07-02, R04) without a newer obid taking an older app's screens down.
- **Status:** WIRE-2 to WIRE-5 are **implemented** in the Electron app (R07-01). WIRE-1, the iPhone's, stays **proposed**: the phone apps are frozen ([the owner's answers of 2026-11-13](../decisions/2026-11-13-roadmap-answers.md), "Phone apps: frozen, not deleted"), and it waits on that freeze and on R07-02. The look is the owner's answer [q015](../decisions/2026-11-13-roadmap-answers.md#r07-01-q015), "Gray badge, no text change"; the iPhone release is held by [q016](../decisions/2026-11-13-roadmap-answers.md#r07-01-q016).
- **Scope:** In: every closed string union in `apps/electron/src/shared/obid-wire.ts` that obid fills in, open to any string (`| (string & {})`), and every reader of the job status, the inbox kind, the agent, the stage state and the transcript's kinds and states in the renderer. Out: the iPhone, Apple Watch and Android apps (frozen); obid sending any new value (R07-02, R04); generated wire types (R07-06, R07-07); the pickers' part of an unknown agent, which is PROVIDERS-5 and 6 in [providers.md](providers.md); new words, labels or design for the unknown state; `docs/runner-api.md`, since obid's API doesn't change.

## Behavior

### Actors and ownership

- **obid** owns nothing new: it sends what it sends, and decides nothing here.
- **The Electron app** reads obid's JSON as it comes (the wire types are TypeScript types only: nothing fails at decode time) and shows a value it doesn't know neutral. The values it knows are written once, in `obid-wire.ts` (`JOB_STATUSES`, `AGENT_IDS`, `INBOX_KINDS`, with `isJobStatus`, `isAgent`, `isInboxKind`), and every check for an unknown one imports them.

### The rule

A value this build doesn't know is shown gray, with no text change, no action, never a known state's mark or word, and nothing thrown (q015). Where a word would go, the app shows obid's raw value, as the ask card's label already did, or nothing; an `aria-label` for it is obid's raw value. No string a person reads is added.

### States and transitions

- **An unknown job status (WIRE-3).** The task stays placed where it was, as done (its tab and side panel group), so other tasks don't move, and carries `unknown` on its `Task`. It gets no ask: no "It failed" or "It was interrupted" card, and no inbox item's buttons, so it never needs you. Its status mark is a still gray dot named by obid's status (`.status-dot.is-unknown`), never the check, the pulse, orange or red; its row in a conversation says no status word; its side panel row isn't dimmed as done; its menu offers neither Stop task nor Archive task (View details, Open conversation, Rename and Delete stay); the box under it names no state and offers no follow-up (`Tell <agent>…`, as a running task's); its card shows no result line or Result section, no Stop and no mode menu; the agent's pane gives it a neutral state, and shows no Stop.
- **An unknown inbox kind (WIRE-2).** The item becomes an ask with obid's `headline` (else its `title`), obid's `text`, and no options. Its card draws no option buttons, not even Later, under a gray label (`.tc-ask-kind.is-unknown`) whose text is obid's kind, as before. A task with a known status still needs you for it: obid says it waits on the owner. Items with no job keep today's Home filter, so an unknown kind there stays off Home's stack.
- **An unknown agent (WIRE-4).** `agentOf` (`view.ts`) gives a known agent's name and mark, and for any other id that id as its name and a gray circle with the id's first letter (`lettered` in `brands.tsx`, the owner's answer [q022](../decisions/2026-11-13-roadmap-answers.md#r08-02-q022)). Every lookup whose key comes from obid goes through it: a job, an account, a login found on the Mac, a limit decision and an archived task. Removing an account of an unknown tool names no provider.
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
- **WIRE-4** (implemented, `wire_4_unknown_agent_renders_with_its_own_name`): Given a job whose agent the app doesn't know, when the Electron app shows its card and its row, then they show that agent's id with a gray circle lettered with its first letter, and nothing throws.
- **WIRE-5** (implemented, `wire_5_unknown_stage_and_session_values_render_neutral`): Given a stage, a transcript entry, a tool call or a to-do whose state or kind the app doesn't know, when the Electron app shows the task's page or session, then the stage shows the gray ring named by its value, the entry is left out, and the call and to-do show the gray ring with no failed mark and no Declined.

## How the schema is made (proposed, R07-03)

How obid's wire shapes are written down, as R07-04 to R07-07 and R07-15 follow it. The options compared, the prototypes and their numbers are in [the research note](../research/wire-schema.md).

- **Hand-written JSON Schema, held by tests.** The schema is hand-written JSON Schema, draft 2020-12, the dialect `review.rs` already validates with (`jsonschema` 0.58.6, already obid's). obid's own HTTP tests validate every answer and event they read against it, through a hook in `core/crates/obid/tests/support/mod.rs` under `call` and its variants and `follow_events` (R07-04): a mismatch fails the test and names the path. The schema describes what obid sends, keys present only when set and `null`s included, not what it stores. obid's production code doesn't change for it.
- **Location and layout.** One file per object under `docs/contracts/wire/`, named `<object>.schema.json` in kebab case: `job.schema.json`, `inbox-item.schema.json`, `status.schema.json`, `event.schema.json`, and the others R07-05 adds. Each file's `$id` is `https://oby.dev/wire/<object>.schema.json`, and one file refers to another by that `$id` (`"$ref": "inbox-item.schema.json"`, resolved against the `$id`), its own parts by `$defs`; the tests load every file into one `jsonschema::Registry`, keyed by `$id`, since obid's `jsonschema` resolves no file by itself. `event.schema.json` is the envelope `{name, data}`: a `oneOf` with one branch per event, each fixing `name` with `const` and pointing `data` to that event's schema with `$ref` (the `job` event's `data` to `job.schema.json`).
- **Open and closed enums.** Every enum lists every value obid sends in `enum`. An enum the apps read openly also has `"x-oby-open": true`; one without it is closed (the add-only rule's closed enums, such as the job status `apps/shared/Runner.swift` decodes). obid's tests validate both strictly against the list, so a new value obid sends fails until the schema lists it. The add-only diff test (R07-04) lets an open enum gain a value and fails a closed one that gains one. Before a generator reads the schema, the generation step rewrites each open enum to `anyOf: [{"type": "string", "enum": [...]}, {"type": "string"}]` (for TypeScript, with `tsType` spelling the known values and `(string & {})`, as `obid-wire.ts` does), which the picked generators turn into an open type.
- **Generators.** TypeScript: json-schema-to-typescript (MIT), used by R07-06. Swift: quicktype (Apache-2.0), for R07-07, a choice for later while the phone apps are frozen. Kotlin: quicktype (Apache-2.0), for R07-15, a choice for later while the phone apps are frozen.
- **The fixture corpus.** One JSON file per answer and per event under `docs/contracts/wire/fixtures/`. An answer's file is named after its route as `docs/api/routes.txt` writes it, the path's leading `/` dropped and each `/` made a `.`: `GET.v1.jobs.json`, `GET.v1.jobs.{id}.json`; it matches the schema the route map below gives that route. An event's file is `event.<name>.json` and matches `event.schema.json`'s branch for that name. Tests write the files and a staleness check guards them (built by R07-04). The corpus is apart from `core/crates/obid/tests/fixtures/released/`, which holds stored job files, not wire answers.
- **R01's snapshot.** `docs/api/obid-api.json` doesn't exist and no task plans it; it is not added. R01's API snapshot is `docs/api/routes.txt` (CI-9 to 11), which covers paths only. The wire schema is the one artifact for keys and values, and no second shape snapshot is added. The two are joined by one route map, `docs/contracts/wire/routes.json`, keyed by a `routes.txt` line (`GET /v1/jobs`) with the schema its answer matches; R07-04 adds it beside the schemas.
- **Line cost.** 0 obid production lines: the hook, the corpus writer and the checks are test code, so the way needs no raise of `obid_lines_limit`.
