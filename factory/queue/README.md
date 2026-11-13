# The roadmap to a stable Oby

The factory's queue (the owner's decision of 2026-11-12: task files here, not GitHub issues).
183 tasks in 13 milestones and 10 sprints, from the comparison
with OpenBot of 2026-11-12. Each task is one pull request into `dev`, written as Oby's feature
contract: Sangam's screen, outcome, scope, what obid owns and what the apps show, numbered
acceptance scenarios, files, steps, tests and done-when checks.

- `tasks/<KEY>.md` is an open task. Its first lines are `NAME: value` fields the factory reads
  (`SPRINT`, `ORDER`, `DEPENDS`, `FIXES`); the task's text follows the first blank line.
- `done/<KEY>.md` is a finished task: the task's own pull request moves its file there.
- `/factory run` starts the tasks of the active sprint (`sprint` in `factory/config.json`)
  whose `DEPENDS` are all done, in `ORDER`. Raise `sprint` when a sprint is finished.
- A scenario id such as `RECOVERY-69` is a proposal until the owner accepts the contract.
- AI agents wrote the tasks from the code on `dev`, the contracts and the open issues. They ran
  no code and no tests. No task tells an agent to copy OpenBot's code (PolyForm Noncommercial).

## Milestones

| Milestone | Goal | Tasks |
| --- | --- | --- |
| **R01 Safety net** | GitHub Actions tests every change once, in batches: a ready pull request gets a cheap lint run, and dev merges only through GitHub's merge queue, whose batch runs the whole Linux suite under one required Check; the Mac check gates each pull request's head and runs in full on dev nightly; flaky tests sit on one shrink-only list (docs/ci-architecture.md). Each agent CLI's version is known, recorded on jobs and failures, and checked against a tested and known-bad table, and a scheduled canary finds CLI drift. Released job files and obid's routes cannot be broken silently. | 14 |
| **R02 Commands that are safe to repeat** | A double click, a relay retry, a retried brain tool call or a restart never makes a second job or follow-up: each request that creates work carries a key, and a repeat gets the first answer. obid sends an event only for what it has on disk, refuses a home a newer obid wrote, and never drops an unreadable file or setting silently. Every log line is redacted where it is written. | 14 |
| **R03 Delete what has no caller** | Code, routes, flags and docs that nothing calls leave the repository: the telemetry flag, the control plane's sign-in, tunnel and hook routes, the phone allowlist entries and runner-api.md sections for routes obid does not serve, and the iPhone's calls to them. The clients that do not ship (apps/web, the Watch target, oby-mcp) are deleted or kept by the owner's recorded word, and Android is frozen out of the parity work. Each deletion says what it searched to prove that nothing calls it. | 8 |
| **R04 One owner for job state** | A job's status and the side fields that go with it change only through one pure transition function, and a stale worker can no longer end a run that Stop, Try again or a move took from it. The branch of a task chain is stored once and read by one function, so Publish, follow-ups, delete and the follower agree. Every step leaves obid working, shrinks jobs.rs, and is pinned by tests named after numbered scenarios. | 15 |
| **R05 Durable words** | Every message the owner sends, to a conversation or to a job, has a delivery status in obid's own files (queued, starting, running, answered, cancelled, failed, interrupted), so a restart, a kill or a failed launch never loses words without a notice. One event-driven drain per conversation replaces the two in-memory locks. The owner can steer a running Codex (and, if the spike proves it, Claude Code) turn through the API, with 'queue' kept as the default because Sangam's design shows one answer at a time. | 9 |
| **R06 Events that clients only apply** | obid numbers its events, replays what a client missed and says which optional features it has, and conversations, turns and inbox items carry a revision as jobs do. The Mac app follows obid once, in main, and applies obid's events through one pure reducer, with a small pending overlay for what the owner just sent; the window and the Mac's alerts read the same state. Sangam's screens look and behave as before; the iPhone and Watch keep working unchanged and can adopt the replay. | 14 |
| **R07 One wire contract** | One machine-readable schema under docs/contracts/wire/ describes what obid sends, obid's own tests hold every answer and event to it on Linux, and the Electron and iPhone wire types are generated from it with every enum open. Each client recovers its connection to obid by one written state machine with fixed timing, and every refusal and user-facing sentence carries a stable code that clients branch on instead of text or id prefixes. | 17 |
| **R08 One interface for every agent CLI** | Claude Code, Codex and Cursor sit behind one provider descriptor table (data) and one AgentDriver trait (behaviour), so obid runs a turn, reads usage, lists models and copies a login without a `match tool` in jobs.rs, the conversation harness or jobs/logins.rs. Account limits live in one hold table, every session replacement uses one handoff builder, and the clients keep working when obid names a tool they do not know. A fourth agent CLI then needs a table row, a driver value (or an ACP flavor) and its own tests, not a cross-cutting rewrite. | 16 |
| **R09 Ship and debug safely** | An update installs only from a feed Deskree signed, never older than the last one this Mac accepted, and obid restarts for it only when obid says no work would be lost. A failed job leaves a bounded, content-free trace that an agent can turn into a post-mortem, and the owner can save a redacted diagnostics file to attach to an issue by hand. A crash loop or a broken start is counted and leads to a recovery page, sleep no longer leaves quiet jobs, and a release is cut from a tag by CI with notes, an upgrade audit and signed Linux archives. | 16 |
| **R10 Finish the Obysphere client** | A teammate's review request and question work end to end in the Electron app: the owner can accept or decline a review request on Home, a new Obysphere's admin can let members ask questions, and outgoing requests stop when their work ends and are never sent twice. Look-alike and changed member identities are shown before anything goes. The Obi-to-Obi protocol, the spec and the Sphere server (obi-ai-bot/obi-sphere) do not change: all work is in obid's sphere client, obi-protocol's client side and the apps. | 12 |
| **R11 Connections: MCP, sign-in and the vault** | One MCP core in obid serves every agent server (the conversation's tools, the job broker, Tetrix), and its gates are data in one table. The owner's MCP servers live in one registry behind the existing broker, signed in through one OAuth module, and reach every own run of Claude Code, Codex and Cursor without a token in any run. The owner puts a secret into the vault from the app, never through a conversation, and sees vault health in GET /v1/status. | 15 |
| **R12 Developer workflows** | A script, a CI step or a git hook drives Oby through one `oby` command with fixed exit codes; routines start from GitHub feed events and from a payload, exactly once, and end quietly when there is nothing to report. The owner can take a job over in Terminal and hand it back safely, memory stops asking twice and reaches every client, and the owner can move Oby to a new Mac or run a headless obid that is a Sphere member. | 17 |
| **R13 How we work** | AGENTS.md states each rule in force once, splits a few non-negotiables from defaults the owner's word changes, and links to a decision log that holds the history. obid, Electron and iOS each have local instructions, size limits are paid back by named cuts, and obid's fixed prompt text and AGENTS.md cannot grow without a visible edit. The 425 gap-map issues have an owner-approved verdict each. | 16 |

## Sprints

### Sprint 1: Safety net · Commands that are safe to repeat · Delete what has no caller

23 tasks, 25 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R01-01 | chore(ci): linux.yml stops runs nobody reads: a nightly full run on dev instead of every push | process | S | low |  |
| R02-01 | fix(obid): send a job event only after the job file is saved | obid | S | medium |  |
| R01-02 | chore(ci): one ci.yml: lanes, a nextest archive in 4 shards, and one Check for pull requests, the merge queue and the nightly run | process | M | medium | R01-01 |
| R02-02 | fix(obid): conversations and inbox items send an event only after their save | obid | S | low | R02-01 |
| R01-03 | chore(process): dev merges through GitHub's merge queue, gated by Check; main requires Check | process | S | medium | R01-02, R01-12 |
| R02-03 | feat(obid): a receipt store for requests that create work | obid | S | low |  |
| R01-04 | test(obid): load a job file and config.json from each released version | tests | S | low |  |
| R02-04 | feat(obid): POST /v1/jobs takes a request key | obid | S | low | R02-03 |
| R01-05 | test(obid): fail the check when a route goes, or a client calls one obid does not serve | tests | S | low |  |
| R02-05 | feat(obid): messages and inbox answers take a request key | obid | S | medium | R02-03, R02-01 |
| R01-06 | test(obid): a shrink-only sleep count, and wait-for-state in the flaky tests | tests | S | low |  |
| R02-06 | feat(obid): Try again and brain-made follow-ups carry a command id | obid | S | medium | R02-03 |
| R01-07 | feat(obid): read and record each agent CLI's version | obid | S | low |  |
| R02-07 | feat(electron): send a request key with every create, message and answer | electron | S | low | R02-04, R02-05 |
| R01-08 | feat(obid): no CLI self-update under a running job | obid | S | low | R01-07 |
| R02-08 | feat(ios, mcp): the iPhone and oby-mcp send a request key | ios | S | low | R02-04, R02-05 |
| R03-01 | chore(obid): drop the telemetry flag that nothing reads | obid | S | low |  |
| R03-02 | chore(cloud): delete the control plane's sign-in, installation, tunnel and hook routes; keep the push relay | cloud | S | medium |  |
| R03-03 | fix(obid,docs): the phone allowlist and runner-api.md name only the routes obid serves | obid | S | low |  |
| R03-04 | chore(ios): the iPhone stops calling routes obid does not serve | ios | S | low |  |
| R01-12 | chore(factory): pull requests stay drafts until approved, and an approved one is queued, not synced, gated again and merged | process | S | medium | R01-02 |
| R01-13 | test(ci): one shrink-only list of flaky tests, retried in CI and once on the Mac, each with a task to fix it | tests | S | low | R01-02 |
| R01-14 | chore(factory): a nightly full Mac check of dev, run by the factory when no gate waits | process | S | low |  |

### Sprint 2: Commands that are safe to repeat · How we work · Safety net

20 tasks, 30 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R01-09 | feat(obid): a tested and known-bad CLI version table | obid | S | low | R01-07 |
| R02-09 | fix(obid): Try again on a task that never started runs it, not a follow-up | obid | S | medium | R02-06 |
| R01-10 | spike: how a CLI canary runs each new Claude Code, Codex and cursor-agent | tests | S | low |  |
| R02-10 | fix(obid): a worker ends only the round it started | obid | S | medium |  |
| R01-11 | test(ci): a scheduled canary of the newest agent CLIs against a fake model | tests | M | low | R01-10, R01-02 |
| R02-11 | feat(obid): a home format file, and refuse a home a newer obid wrote | obid | S | medium |  |
| R02-12 | feat(obid): quarantine files that do not read, and report them | obid | S | low |  |
| R02-13 | feat(obid): keep unknown keys on save, and load settings.json field by field | obid | S | medium | R02-12 |
| R02-14 | fix(obid): redact every log line in the writer | obid | S | low |  |
| R03-05 | chore(mcp): retire oby-mcp, on the owner's yes | obid | S | low |  |
| R03-06 | chore(web): delete apps/web and its stale docs, on the owner's yes | process | S | low |  |
| R03-07 | chore(ios): drop the Apple Watch target from the iPhone build, on the owner's yes | ios | M | medium | R03-04 |
| R13-01 | docs: decide that routines stay or go, and bring docs/trim.md up to date | docs | S | low |  |
| R13-02 | docs: a decision log in docs/decisions/, one file per owner decision | docs | M | low |  |
| R13-08 | test(obid): a byte budget for obid's own prompt text | obid | S | low |  |
| R13-09 | docs(github): a pull request template with surfaces and failure modes | process | S | low |  |
| R04-01 | docs: a 'State ownership' table for obid and owner comments on each module | docs | S | low |  |
| R04-02 | test(obid): characterization tests that pin today's job lifecycle | tests | M | low | R04-01 |
| R05-01 | spike(obid): what Claude Code stream-json and Codex turn/steer accept during a turn | obid | S | low |  |
| R05-02 | feat(obid): a durable delivery status on each owner's turn in a conversation | obid | M | medium |  |

### Sprint 3: Events that clients only apply · One wire contract · One owner for job state

20 tasks, 38 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R13-03 | docs: rewrite AGENTS.md as non-negotiables, defaults and links, keeping every rule in force | process | M | medium | R13-02 |
| R04-03 | refactor(obid): one pure lifecycle function, starting with next_round and the end-of-run decision | obid | M | medium | R04-02 |
| R04-09 | refactor(obid): one function decides whether a follow-up carries a job's branch | obid | S | medium | R04-02 |
| R04-10 | feat(obid): store the task chain's branch once and let follow-ups read it | obid | M | medium | R04-09 |
| R05-03 | feat(obid): recover conversation deliveries when obid starts | obid | S | low | R05-02 |
| R05-04 | refactor(obid): one event-driven drain per conversation in place of Turns.owners and Sessions.busy | obid | M | medium | R05-02, R05-03 |
| R06-01 | feat(obid): a `capabilities` list in GET /v1/status | obid | S | low |  |
| R06-02 | feat(obid): number events and replay what a client missed (`?after=`, Last-Event-ID, `resync`) | obid | M | medium | R06-01, R02-01 |
| R06-03 | feat(obid): a `revision` on conversations, turns and inbox items | obid | M | medium | R06-01, R02-01 |
| R06-04 | refactor(electron): register every IPC handler through `handle(channel, decode, fn)` | electron | S | low |  |
| R06-05 | feat(electron): gate optional actions on obid's capabilities | electron | S | low | R06-01 |
| R06-06 | refactor(electron): a pure reducer in src/shared, beside live.ts, for jobs first | electron | S | low |  |
| R06-13 | fix(electron): Settings rows obid has no value for are kept in main's settings file | electron | S | low | R06-04 |
| R08-01 | refactor(obid): one provider descriptor table, data only | obid | M | medium |  |
| R08-07 | refactor(obid): one limit hold table keyed by account and scope | obid | M | medium |  |
| R08-11 | fix(obid): one queue per account for sign-in, refresh, sign-out and removal | obid | S | low |  |
| R07-01 | fix(ios,electron): every wire enum decodes an unknown value | ios | S | low |  |
| R07-03 | spike: how obid's wire schema is made and how client types are generated | obid | S | low | R01-03 |
| R07-04 | test(obid): wire schema for Job, InboxItem, Status and the event envelope, with a fixture corpus | tests | M | medium | R07-03 |
| R07-08 | test(relay): phones read a session that obid's envelope tests record | tests | S | low |  |

### Sprint 4: How we work · Durable words · One interface for every agent CLI

20 tasks, 36 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R13-04 | chore(check): a byte budget for AGENTS.md with a ratchet | process | S | low | R13-03 |
| R13-05 | docs(obid): core/crates/obid/AGENTS.md: where new code goes and when a temporary path ends | process | S | low | R13-03 |
| R13-06 | docs(apps): apps/electron/AGENTS.md and apps/ios/AGENTS.md | process | S | low | R13-03 |
| R13-07 | chore(check): a limit raise names its payback issue and comes in its own pull request | process | S | low | R13-03 |
| R13-10 | docs(github): a Lane field on feature issues and an owner-only `approved` label | process | S | low | R13-03 |
| R13-11 | docs(skills): a triage skill for one issue: valid, duplicate, already done, whose lane | process | S | low | R13-10 |
| R13-12 | chore(process): triage the 425 gap-map issues into a verdict list for the owner | process | M | medium | R13-11 |
| R13-13 | docs(skills): a ship skill: PR into dev, review loop, checks green, issue and board updated | process | S | low | R13-09, R13-03 |
| R04-04 | fix(obid): a run token so a stale worker cannot end a run Stop, Try again or a move took over | obid | M | medium | R04-03 |
| R05-05 | feat(obid): steer a running conversation turn, with fallback to the queue | obid | M | medium | R05-01, R05-02, R05-04 |
| R05-06 | fix(obid): the owner's words to a job stay queued until its agent turn runs | obid | M | medium | R05-02, R04-03 |
| R05-08 | feat(obid): take back one waiting message | obid | S | low | R05-04 |
| R06-07 | refactor(electron): conversations, turns and inbox items through the reducer, by revision | electron | M | medium | R06-06, R06-03 |
| R06-14 | feat(ios): resume the event stream with `after` and refetch only on `resync` | ios | S | low | R06-02 |
| R08-02 | fix(clients): show an agent id the app does not know instead of breaking or calling it Claude Code | electron | M | low | R08-01 |
| R08-03 | refactor(obid): AgentDriver trait for a job's turn, with each driver's own error mapping | obid | M | medium | R08-01 |
| R08-08 | chore(obid): delete the retired limit-decision store, answer its API from the hold table | obid | S | medium | R08-07 |
| R07-02 | feat(obid): old clients keep the six released job statuses | obid | S | medium | R07-01, R06-01 |
| R07-05 | test(obid): wire schema for Conversation, Turn, Account and every remaining event | tests | M | low | R07-04 |
| R07-09 | feat(obid,ios): stable reasons on refused phone connections and the device's current expiry | obid | S | low |  |

### Sprint 5: One wire contract · How we work · One interface for every agent CLI

16 tasks, 38 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R13-14 | docs(skills): pin a plan or epic to its commit and start with a drift check | process | S | low | R13-13 |
| R13-15 | docs(api): split docs/runner-api.md into docs/api/<area>.md with an index | docs | M | medium | R13-03 |
| R13-16 | docs(contracts): contracts state current behaviour; history moves to the decision log | docs | S | low | R13-02, R13-03 |
| R04-05 | fix(obid): keep the owner's words until the agent is launched | obid | S | medium | R04-04 |
| R04-06 | refactor(obid): Stop, Try again and Another round as lifecycle commands; a never-started job reruns | obid | M | medium | R04-04 |
| R05-07 | feat(obid): steer a running Claude Code or Codex job, with fallback to its next turn | obid | M | medium | R05-01, R05-05, R05-06 |
| R06-08 | refactor(electron): follow obid's events once, in main, and feed the window and the alerts from it | electron | M | medium | R06-04, R06-07 |
| R06-10 | refactor(electron): words to Oby as a pending overlay keyed by the turn id | electron | M | medium | R06-07 |
| R08-04 | refactor(obid): the conversation harness and Sphere answering run turns through AgentDriver | obid | M | medium | R08-03 |
| R08-05 | refactor(obid): usage reads, model lists and identity behind AgentDriver | obid | M | medium | R08-03 |
| R08-06 | refactor(obid): split acp.rs into a generic ACP client and a Cursor flavor | obid | M | low | R08-03 |
| R07-06 | refactor(electron): generate obid-wire types from the schema | electron | M | low | R07-05, R07-01 |
| R07-07 | refactor(ios): generate the Swift wire types from the schema | ios | M | medium | R07-05, R07-01, R03-07 |
| R07-10 | feat(obid): stable codes on obid's sentences and an explicit inbox kind | obid | S | low | R07-04 |
| R07-11 | feat(ios): one recovery state machine for the phone relay | ios | M | medium | R07-09 |
| R07-16 | feat(obid,android): seal Android pushes as the iPhone's are (skip if Android is frozen) | android | S | low | R03-06 |

### Sprint 6: One interface for every agent CLI · One wire contract · Ship and debug safely

20 tasks, 38 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R03-08 | docs(process): freeze Android out of the launch and the parity work, on the owner's yes | process | S | low | R07-06 |
| R04-07 | refactor(obid): the worker's and the restart's status writes through lifecycle | obid | M | high | R04-06 |
| R05-09 | feat(electron): show a message's delivery status in the conversation | electron | S | low | R05-02, R05-03, R05-08, R06-10 |
| R06-09 | feat(electron): resume obid's stream with `after` and refetch only on `resync` | electron | S | low | R06-02, R06-08 |
| R06-11 | refactor(electron): answers to cards and task actions as pending entries | electron | S | medium | R06-10 |
| R06-12 | refactor(electron): the mock as a fake obid behind `window.app.obid` | electron | M | low | R06-08 |
| R08-09 | feat(obid): pace usage reads per account with backoff, and forget them on a login change | obid | S | low | R08-05 |
| R08-10 | feat(obid): after a refusal with no reset, read the account's usage before guessing an hour | obid | S | low | R08-07, R08-09 |
| R08-12 | refactor(obid): describe each tool's sign-in and login layout as data in its row | obid | M | medium | R08-01, R08-05, R08-11 |
| R08-14 | refactor(obid): one handoff builder for every session replacement | obid | M | medium | R08-04 |
| R08-16 | feat(obid): move a saved model the CLI no longer lists to the newest of its family | obid | S | low | R08-05 |
| R07-12 | refactor(electron): the main process follows obid's events by the same recovery rule | electron | S | low | R07-11 |
| R07-13 | fix(ios): the phone's outbox sends in order and never drops words silently | ios | S | low | R07-11, R02-05 |
| R07-14 | feat(ios): an encrypted read-only snapshot at iPhone start | ios | M | low | R07-11, R07-09 |
| R07-15 | refactor(android): generated Kotlin types, the recorded relay session and the recovery rule (skip if Android is frozen) | android | M | medium | R07-05, R07-08, R07-11, R03-06 |
| R07-17 | feat(obid,ios): the iPhone attaches files over the sealed relay with a hash check | ios | M | medium | R07-11, R06-01 |
| R09-01 | feat(electron): sign latest-mac.yml and refuse unsigned, replayed or below-floor feeds | electron | S | low |  |
| R09-02 | feat(electron): a deadline and an error code for every busy update phase, and one install per session | electron | S | low |  |
| R09-03 | feat(obid): status.restart says when a restart loses nothing, and the app restarts obid only then | obid | M | medium |  |
| R09-04 | feat(obid): a bounded per-job trace of dispatcher steps, with outcome codes and no content | obid | M | low | R04-01 |

### Sprint 7: Finish the Obysphere client · Ship and debug safely · One owner for job state

20 tasks, 38 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R04-08 | refactor(obid): make Job.status private to the job module | obid | M | medium | R04-07 |
| R04-13 | feat(obid): a per-job transition log | obid | M | low | R04-07 |
| R04-15 | spike: should jobs, inbox and conversations move to one SQLite file? | obid | S | low | R04-07, R04-10, R02-11 |
| R08-13 | refactor(obid): one login_copy and one strip in place of the per-tool login-copy families | obid | M | high | R08-12 |
| R08-15 | feat(obid): a conversation's new session reads its handoff as a room file | obid | S | low | R08-14 |
| R09-05 | feat(obid): Fix it starts a research job on the owner's agent that writes the post-mortem | obid | M | low | R09-04 |
| R09-06 | feat(obid): GET /v1/diagnostics, a redacted, scanned, loopback-only bundle | obid | M | low | R09-04 |
| R09-09 | feat(electron): the app's run marker, renderer and child crash records, a 30 s quit deadline, and launchd back-off | electron | S | low |  |
| R09-11 | feat(obid): stay awake for any pending work, and catch up at once after a sleep | obid | M | medium |  |
| R09-12 | feat(install): protect a Linux obid from its agents' memory use | obid | S | low | R01-01 |
| R09-13 | spike(process): release from a tag in CI, key custody, and the dev-to-main cadence | process | S | low | R01-01 |
| R09-14 | chore(process): cut the Mac release from a tag on main in CI, with no Swift leftovers | process | M | medium | R09-01, R09-13, R01-01 |
| R10-01 | feat(obid): call off outgoing Obi-to-Obi requests when their work ends | sphere-client | S | low |  |
| R10-02 | feat(obid): an operation id makes an owner's Obi-to-Obi send safe to repeat | sphere-client | S | low | R10-01, R02-03 |
| R10-03 | feat(obid): a teammate's review request alerts with Accept and Decline | obid | S | medium |  |
| R10-04 | feat(electron): accept or decline a teammate's review request on Home and in their thread | electron | M | medium | R10-03 |
| R10-05 | spike: how a client reads and writes an Obysphere's policy today | sphere-client | S | low |  |
| R10-06 | feat(electron): the app's main process reads and writes the Obysphere's policy with the admin's own token | electron | S | medium | R10-05 |
| R10-07 | feat(electron): who may ask whom in Settings › Obysphere, with one click to let members ask | electron | M | low | R10-06 |
| R10-08 | feat(obid): an owner id that looks like another member's waits for a Confirm that names both | sphere-client | S | medium |  |

### Sprint 8: Connections: MCP, sign-in and the vault · Ship and debug safely · Finish the Obysphere client

18 tasks, 38 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R04-11 | feat(obid): serve the owner actions a job's state allows | obid | M | low | R04-08, R04-10 |
| R09-07 | feat(electron): Help › Export Diagnostics… and Report a Problem… | electron | S | low | R09-06 |
| R09-08 | feat(obid): a run marker, start attempts per version, and a runtime stall probe | obid | M | low | R09-06 |
| R09-15 | chore(process): changelog.d notes per PR and an upgrade-safety audit before each tag | process | S | low | R09-14, R01-01, R01-04, R01-05 |
| R09-16 | feat(process): signed Linux obid archives in the release, and a timer that updates at an idle point | process | M | medium | R09-03, R09-12, R09-14, R01-01 |
| R10-09 | feat(obid): a member's changed owner key becomes one Confirm on Home | sphere-client | S | medium |  |
| R10-10 | feat(electron): compare a member's key and forget remembered decisions in Settings › Obysphere | electron | M | low | R10-08, R10-09, R10-04 |
| R10-11 | refactor(obid): one inbox owner, so the Sphere link holds only Obi-to-Obi traffic | obid | M | medium | R10-09 |
| R10-12 | test(obid): the phone relay keeps its slots and writes across drops, rotations and reconnects | obid | M | medium |  |
| R11-01 | spike(design): which screens add an MCP server, show vault health and take a secret | process | S | low |  |
| R11-02 | refactor(obid): one MCP JSON-RPC core for the three agent servers | obid | S | low |  |
| R11-03 | refactor(obid): agent tools as a table with their gates as data | obid | M | medium | R11-02 |
| R11-04 | refactor(obid): one OAuth client module, Tetrix its first caller | obid | M | medium |  |
| R11-06 | feat(obid): the owner's MCP server registry: GET/POST/DELETE /v1/connections | obid | M | medium |  |
| R11-12 | feat(obid): put a secret in the vault over loopback: GET/PUT/DELETE /v1/obid/vault | obid | S | medium |  |
| R11-13 | feat(electron): Settings › Vault on obid's vault routes | electron | S | low | R11-12, R11-01 |
| R11-15 | feat(obid): vault health in GET /v1/status | obid | S | low | R11-12 |
| R12-01 | feat(cli): an `oby` command over the loopback API with fixed exit codes | obid | M | low | R02-03, R06-01 |

### Sprint 9: Developer workflows · Connections: MCP, sign-in and the vault · One owner for job state

14 tasks, 32 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R04-12 | feat(electron): draw the task card's buttons from obid's actions | electron | S | low | R04-11 |
| R04-14 | refactor(obid): move the job lifecycle rules into an obid-rules crate with no I/O | obid | M | medium | R04-11 |
| R09-10 | feat(electron): a start-recovery page after three failed starts of one version | electron | M | medium | R09-01, R09-08, R09-09 |
| R11-05 | refactor(obid): ChatGPT sign-in on the shared OAuth module | obid | M | medium | R11-04 |
| R11-07 | feat(obid): the broker hands registry servers to every owner run, resolved per request | obid | M | medium | R11-03, R11-06 |
| R11-14 | feat(obid): a `secret` ask for missing secrets, answered on its own route | obid | M | medium | R11-12, R11-03 |
| R12-02 | feat(cli): `oby inbox` and `oby answer`, with the same limits as oby-mcp | obid | S | medium | R12-01, R06-01 |
| R12-03 | feat(obid): a routine run can carry a payload, quoted as data | obid | S | medium | R12-01, R13-01, R02-01 |
| R12-04 | feat(obid): a routine run ends quiet on `NOTHING TO REPORT` | obid | S | low | R04-03, R13-01 |
| R12-05 | feat(obid): start routines from GitHub feed events, once each | obid | M | medium | R13-01, R02-01, R04-03 |
| R12-06 | feat(electron): wire Sangam's Home Routines group to /v1/routines | electron | S | low | R13-01, R06-07, R07-06 |
| R12-07 | feat(obid): take over a job, with one hold guard at the lowest point | obid | M | medium | R04-06 |
| R12-10 | feat(obid): memory stops asking twice and goes out on the event stream | obid | M | low | R06-02, R02-01 |
| R12-12 | feat(obid): export what Oby learned, with every secret cleared | obid | M | medium | R09-13, R11-15 |

### Sprint 10: Developer workflows · Connections: MCP, sign-in and the vault

12 tasks, 22 points (a small task is 1, a medium task is 3).

| Key | Task | Area | Size | Risk | Depends on |
| --- | --- | --- | --- | --- | --- |
| R11-08 | refactor(obid): Tetrix becomes one registry row | obid | M | medium | R11-04, R11-07 |
| R11-09 | feat(obid): sign in to a registry server with OAuth | obid | M | medium | R11-04, R11-06, R11-07 |
| R11-10 | feat(obid): a registry change restarts warm sessions with the new list | obid | S | low | R11-07 |
| R11-11 | feat(obid): a move says which MCP servers the new run loses | obid | S | low | R11-07 |
| R12-08 | feat(obid): Hand back reads the worktree again and checks the owner's commit | obid | S | low | R12-07, R08-14 |
| R12-09 | feat(electron): Take over and Hand back on a task | electron | M | medium | R12-07, R12-08, R06-01 |
| R12-11 | feat(obid): a conversation stages a workflow proposal; only the owner's yes saves it | obid | S | low | R12-10, R05-02 |
| R12-13 | feat(obid): import as stage, preview and apply, at a fresh obid's level | obid | M | medium | R12-12, R11-06 |
| R12-14 | feat(obid): keep rebuildable and secret folders out of Time Machine, and stamp the home | obid | S | low | R01-02, R06-01 |
| R12-15 | spike: a headless obid's Sphere enrollment, renewal and phone pairing | obid | S | medium | R10-02 |
| R12-16 | feat(obid): `obid sphere enroll` joins a Sphere from a one-time bundle on stdin | obid | M | medium | R12-15, R09-16, R10-05 |
| R12-17 | spike: a signed inbound webhook over an outbound socket | cloud | S | medium | R12-05, R12-03, R03-02 |
