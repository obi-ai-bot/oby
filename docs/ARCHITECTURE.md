# Oby's architecture

How Oby is put together with the engine running as the Rust daemon `obid`: where every piece lives, which lines nothing crosses, and which module of `obid` owns each kind of its state (section 6). The contracts it relies on are:

- [docs/runner-api.md](runner-api.md): the API between `obid` and every shell, including its "obid only" section.
- The Obi-to-Obi protocol v0.1: `spec/` in [obi-ai-bot/obi-sphere](https://github.com/obi-ai-bot/obi-sphere).

Decided and not reopened here: the product is Oby (apps, bundle ids, `window.oby`); Apache-2.0; the engine moves from Swift to `obid` behind the existing runner API; the UI shells stay native and thin; the Sphere server is TypeScript on Cloudflare Workers with D1; Tetrix is an optional plug-in; Deskree's own pipeline rules become configuration. The GitHub org, the repository names and the daemon keep the `obi` spelling.

## 1. Layout

```
obi/
├── core/            Rust: the daemon obid and its crates (section 3)
├── apps/
│   ├── web/         TypeScript universal client, served by obid
│   ├── mac/         Swift: window, menu bar, approvals, character; a thin client of obid
│   ├── ios/         Swift thin client
│   ├── watch/       Swift thin client
│   └── android/     Kotlin thin client, later
├── character/       The three.js mascot, unchanged
├── mcp/             The Oby MCP server
├── e2e/             The end-to-end scenario: SCENARIO.md, the harness, the stub agent, fixtures
├── docs/            Documentation
└── Makefile         make e2e, make e2e-live
```

Where today's folders go. Moves happen with `git mv`, so history follows:

| Today | Target | Note |
|---|---|---|
| `Engine/`, `Runner/`, `RunnerTests/` | deleted | The specification `core/` was ported from; deleted with the Swift Mac app by the owner's decision of 2026-11-02 (git history has them). `Package.swift` stays, for `oby-mcp`. |
| `Mac/`, `MacTests/` | deleted | The Swift Mac app, deleted by the owner's decision of 2026-11-02: the Mac's app is `apps/electron/`. |
| `iOS/`, `iOSTests/`, `LiveActivity/` | `apps/ios/` | |
| `Watch/` | `apps/watch/` | |
| `Shared/` | `apps/shared/` | Swift code the Apple shells share. |
| `Oby.xcodeproj`, `Config/`, `Icon/` | under `apps/` | The restructure decides the exact paths; the Apple apps must keep building. |
| `Android/` | `apps/android/` | |
| `Character/` | `character/` | Differs only by case: on macOS move it through a temporary name (`git mv Character character.tmp && git mv character.tmp character`). |
| `MCP/`, `MCPTests/` | `mcp/` | `MCP` to `mcp` differs only by case too: same two-step move. |
| `Desktop/` | stays | Open: `apps/web` may replace it. |
| `cloud/control-plane/` | stays | Open: whether it merges into the Sphere server. |
| `spikes/`, `scripts/`, `docs/` | stay | |

## 2. The boundary: obid and the shells

```
 apps/electron  apps/ios  apps/watch  apps/web  mcp/
     │          │           │           │        │
     └──────────┴─── HTTP + SSE: docs/runner-api.md ───┘
                              │
                            obid  ── Obi-to-Obi client ──►  Sphere (obi-sphere/server)
                              │
             agents (Claude Code, Codex, Cursor) in sandboxed worktrees
                              │
                       plug-ins (out of process)
```

- **The runner API is the only way in.** Every shell talks to `obid` over HTTP and the `GET /v1/events` stream, as the apps always have. No shell links the engine, imports its types or reads its database; `obid` serves the same API, so the Mac, iPhone and Watch apps run against it with no change to their code. Whether a later version adds an in-process binding (UniFFI) is an open decision; today it is HTTP only.
- **Change the API in the same pull request as the code** (AGENTS.md). New endpoints that only `obid` serves go in the "obid only" section of runner-api.md.
- **Loopback and a token.** `obid` listens on 127.0.0.1 only, checks `Host`, and answers only requests with its API token; remote devices reach it through the gateway's allowlist, as today. The web app (`apps/web`) is no exception: the page sends the token on every API call, from the browser's session storage for its own origin. The one thing obid opens without the token is reading the web app's own files under `/app/`, which hold no data, with a page session: a random value obid keeps in memory, opened by signing in with the token and ended by signing out or a restart. It exists because a browser can't send a header when it opens a page and #492 keeps the page closed without the token; it admits nothing under `/v1`, since anything on the machine can replay a cookie and the headers a browser adds (runner-api.md, "obid only: the web app"). On macOS it listens on `127.0.0.1:7777` unless its configuration says otherwise: the address the Mac app already uses for a runner on the same Mac (runner-api.md, "Reaching it" and "obid only: configuration"). The Oby app's own Swift runner (its login item) listens there too, so one Mac runs one of the two: when the address is taken, `obid` doesn't start and says why in one line; it never moves to another port by itself. Its API token is only ever in `home/api-token`, never in its environment or arguments (5.3).
- **The Swift clients on `obid`.** `core/crates/obid/tests/swift_client.rs` builds the iPhone's and the Watch's own client code (`apps/shared/Runner.swift` and what it needs) and drives it against a running `obid`, so an API change those apps can't read fails there.

## 3. Inside core/

`core/` is one Cargo workspace whose root `core/Cargo.toml` lists `members = ["crates/*"]`, so a new crate joins it without editing the root, and declares the shared dependency versions under `[workspace.dependencies]` (serde, serde_json, tokio, thiserror, tracing, async-trait). Crates use `workspace = true` for those.

| Crate | What | Owner |
|---|---|---|
| `core/crates/obid/` | The daemon: configuration, the runner API and event stream, jobs, worktrees and sandbox, agent adapters (Claude Code stream-json first, then Codex app-server, then ACP), the planner, decision memory, the inbox, the publish gate (secret scan), the merge (by the owner's learned workflow, or their tap), the vault, accounts and usage. | Rust core |
| `core/crates/obi-protocol/` | Obi-to-Obi protocol v0.1: types, canonical JSON, Ed25519 signing and checks, the agent card, the token exchange, the Sphere client. Tested against obi-sphere's conformance suite (`--impl`). | Rust core |

How the engine's parts are split into modules inside `obid` is the Rust core's choice. What `obid` does is fixed by runner-api.md and by the Swift engine it is ported from, minus what this document removes.

## 4. Trust boundaries

What crosses into Oby, and what it may become:

| Comes from | Arrives as | May become |
|---|---|---|
| The owner (a shell, the MCP server) | Runner API calls with the API token | Anything the API offers: goals, answers, settings, publishing by hand with a device signature. |
| Another Oby | A signed, typed action through the Sphere, checked against the spec (schema, signature, expiry, policy) | Only what the receiver's decision rule allows (spec `protocol.md` section 11): an inbox item, work a remembered decision covers, or, for a `question`, an `answer` the Oby writes by itself from what one of its owner's sharing rules allows (a model run on the owner's account, limited per sender), and then only the typed reply to that action. Never a merge, a push, a published change, a changed setting or a changed decision. |
| GitHub, Slack, web pages, plug-ins, other people | Data | Quoted to agents as files (`.obi/inputs/`, `.obi/context/`), shown to the owner. Never part of a prompt's instructions. |
| Connected apps (through `obid`'s broker) | MCP tool results the agent asked for, with the vault's secrets taken out, each block fenced as quoted data (#575) | Untrusted data inside the agent's context, labelled: every block of a broker result is text between `<<<quoted-<nonce>` and `quoted-<nonce>>>>`, with a random nonce per job that is taken out of the text first, so nothing an app answers can close its fence; the job's prompt says once what the fence means. Non-text blocks (resource links, embedded resources) become fenced JSON text, images and audio stay as they are, and `structuredContent` is dropped (and `outputSchema` with it), since MCP asks a tool that returns it to send the same in a text block, which is fenced. Even so, it can steer the agent within what the agent may do, never past `obid`'s rules: the broker forwards only read-only tools, and agents can't publish. |
| `agent-mcp` plug-ins' servers | MCP tool results the agent asked for, straight from the plug-in's server | Untrusted data inside the agent's context, not marked as data: `obid` isn't between the agent and these servers. It can steer the agent within what the agent may do, never past `obid`'s rules. |
| Agents (their output, their questions) | Data and typed requests (permission prompts, `QUESTION:` lines) | What `obid`'s fixed rules and the owner's saved rules allow. In the default `sandbox` mode, `obid` answers by itself, and records on the job, whatever stays inside the job's OS sandbox (edits in its worktree, commands, tests, reads); what leaves it or sticks (the network beyond the model's API, secrets or the vault, a read outside the worktree, standing rules) and the agent's own questions wait in the inbox (runner-api.md, "obid only: permissions answered without the owner"). |

The rules every component keeps:

- **Typed actions in, data never instructions.** Other Obys can only send the eight typed actions; there is no free-text type, and a field the schema doesn't list makes a message invalid. Every human-written field (`x-obi-text: data` in the spec) reaches an agent only as a quoted file and the owner only as text. The same holds for text from GitHub, Slack, plug-ins and fixtures.
- **The publish gate.** Nothing leaves `obid` (a push, a pull request, its description, a check run, an Obi-to-Obi reply) until the secret scan has passed on everything in it, with the vault's secrets and their encoded forms. If the scanner can't run, nothing goes out. A job whose checks passed and whose scan is clean publishes by itself, one whose conversation read something Oby didn't write and one a standing rule started included (the owner's decision of 2026-10-24, replacing 2026-10-23's publish by hand): what such a job read only keeps it out of Bypass, and merging waits for a review.
- **Oby merges only its own pull requests, only as its owner said** (AGENTS.md, "How work merges is learned, not fixed", and the owner's decision of 2026-10-24): a pull request a job of the owner's opened, once the review the repository's workflow names approved its head and the checks it names passed on that head, the way that workflow says, or on the owner's own tap where Oby hasn't learned one. By itself, never into `main`, `master` or `trunk` and never anyone else's pull request; on the owner's own Merge, into `main`, `master` or `trunk` and into another base than the learned workflow's (#1658), and anyone else's pull request on their word (proposed: Limits 2/3); and never for another Obi (docs/runner-api.md, "obid only: merging"). Deskree's own promotion rules (Xcode Cloud, `dev` to `main`, the three-review escalation) become configuration that changes what Oby checks and reports, never something that merges.
- **Agents can't publish.** They run in sandboxed worktrees with no code-host login, no vault secret, and a clone whose push URL is invalid. Only the publish gate pushes, and only the owner's own work.
- **Secrets stay in the vault.** No agent, plug-in or other Oby receives a vault secret, in any form. No vault secret and no token is ever in `obid`'s environment or arguments either, because other processes of the same user can read those (5.3, "What the sandbox can't hide").
- **Oby sends no telemetry.**

## 5. Plug-ins

Removed on 2026-10-27 (the strip to a dispatcher): obid has no plug-in host, no `obi-plugin-api` or `obi-plugin-host` crate and no `plugins/` folder. Tetrix, which was the first plug-in, is obid's own (`core/crates/obid/src/tetrix.rs`): its allowed tools are `tetrix::TOOLS`, and what it knows about a job is quoted in `.obi/context/tetrix.md` as before. `bundledPlugins` in a configuration is ignored with a warning.

### 5.3. Job box

On macOS a job box hides obid's home, the person's homes, account folders,
`/private/tmp`, `/Users/Shared` and the user's Darwin temporary folder (`TMPDIR`).
The job's own worktree, git directories, temporary folder, private login copy and
explicit tool reads remain accessible. Unboxed Claude login refresh uses a private
`obid-refresh-*` scratch folder under obid's home, removed after refresh.
The own lane's vendor sandbox is separate from these box rules.

## 6. State ownership

Which module of `obid` writes each kind of its state, where that state is kept and who reads it, so a new field or rule goes into the module that owns its row. Module names are under `core/crates/obid/src/`. "Stored in" is a file in obid's home (`home/…`), or memory, which a restart loses. "Target after R04" is what the R04 task files in `factory/queue/` change (`factory/queue/tasks/`, then `factory/queue/done/` once built), and "same" where they change nothing; `job/lifecycle.rs` and `job/chain.rs` are targets, not files yet.

| State | Owning module (writes it) | Stored in | Read by | Target after R04 |
|---|---|---|---|---|
| Job lifecycle: `status` and its side fields (`phase`, `step`, `reason`, `next_step`, `asked`) | `jobs.rs`, `jobs/account_limits.rs` and `jobs/retry.rs`, and through `next_round` also `jobs/moves.rs` and `jobs/after_pr.rs` (every write in the list below), saved through `Jobs::keep` (`jobs/keep.rs`) by `store.rs` | `home/jobs/<id>.json` | the runner API (`routes.rs`, `api.rs`), `publish.rs`, `merge.rs`, `routines.rs`, the brain (`brain/briefing.rs`) and every `jobs/` module | `job/lifecycle.rs` (target, R04-03 to R04-08): the one writer of status and its side fields, with `Job.status` private to the job module (R04-08) |
| Task chain and branch: `follow_up_of`, `branch`, `made_branch`, `start_from` | `jobs.rs`: `create` and `follow_up` decide them; the comparisons in the list below read the chain | `home/jobs/<id>.json` | `jobs.rs`, `jobs/account_limits.rs`, `jobs/after_pr.rs`, `publish.rs`, `review.rs`, `task_conversation.rs` | `job/chain.rs` (target, R04-09 and R04-10): the chain's branch decided in one function and stored once |
| Recovery counters: `Job.recovery` (`errors`, `stalls`, `tries`, `restarts`, `limit_resumes`, `renewed`, `scoped_holds`, `cut`) | `jobs.rs` (`open`, `crashed`, `run_owner_job`, the stall checks), `jobs/retry.rs` and `jobs/account_limits.rs` | `home/jobs/<id>.json` | `jobs.rs`, `jobs/retry.rs`, `jobs/account_limits.rs` | same |
| Account-limit wait: `resumes_at`, `waiting`, the `account_limit_*` fields, and the owner's decisions | the job's fields: `jobs/account_limits.rs`, `jobs.rs`, and `jobs/moves.rs` (a move ends the wait); the decisions: `account_limits.rs` (`Decisions`), each saved before it is acted on | the job's: `home/jobs/<id>.json`; the decisions: `home/limit-decisions.json` | `jobs.rs`, `jobs/account_limits.rs`, the runner API (`routes.rs`), and `job.rs`, which mirrors the decision into the job's JSON | same |
| Trust mark: `untrusted`, `unsigned`, `foreign`, `foreign_by_api`, `reached` (where it is written and kept, not its rules) | `jobs.rs`: `create` (a follow-up inherits its job's) `Jobs::mark` (through `Job::note_mark`, `job.rs`) and `mark_unsigned`; `carry_marks` (`jobs/keep.rs`) keeps a mark when a save fails | `home/jobs/<id>.json` | `merge.rs`, `review.rs`, `jobs/lanes.rs` and `jobs.rs` | same |
| Conversation turns | `conversation.rs` (`Conversations`), called by the brain (`brain/mod.rs`, `brain/ask.rs`, `brain/recap.rs`), `routines.rs`, `task_conversation.rs` and the runner API (`routes.rs`) | `home/conversations/<id>.json` | the runner API, the brain (`brain/briefing.rs`) | same |
| Inbox items | `sphere/inbox.rs` (`Inbox::save`, and the items it builds), called by the Sphere link (`sphere/link.rs` and its `sphere/link/` modules), which also holds them in memory; `jobs.rs` and `review.rs` hand it items through the link's `add_items` (`sphere/link.rs`) | one file per item in `home/inbox/` | the runner API (`sphere/routes.rs`), the brain (`brain/briefing.rs`), `sphere/link/inbox_actions.rs` | same |
| Vault | `vault.rs` (its own store) | `home/vault/secrets.json` (its use log, `home/vault/uses.jsonl`, is `connections.rs`'s) | `secrets.rs` (redaction), `scan.rs`, `connections.rs`, `tetrix.rs`, `chatgpt.rs` | same |
| Transcripts | `transcript.rs` (`Transcripts`), fed by `jobs.rs` from the agent adapters' events | `home/jobs/<id>/transcript.jsonl` and `home/jobs/<id>/transcript-detail.jsonl` | the runner API (`GET /v1/jobs/{id}/transcript`, `routes.rs`), `activity.rs`, `jobs/moves.rs` | same, plus each job's transition log, `home/jobs/<id>/events.jsonl`, appended by `Jobs::apply` (target, R04-13) |
| In-memory `State` sets in `jobs.rs`: `workers`, `publishing`, `followed`, `handed`, `idle`, `returning` (its other in-memory fields, such as `asks` and `held`, are left out here) | `workers` and `publishing`: `jobs.rs` (`schedule`, `spawn_worker`, `publish_job`, `crashed`); `followed`: `jobs.rs`'s pull-request follower; `handed`: `jobs/by_github.rs`; `idle` and `returning`: `jobs/slots.rs` | memory | `jobs.rs` and its `jobs/` modules | same |

Storage is unchanged in R04: one JSON file per job, per conversation and per inbox item. The move to one SQLite file is planned (R04-15, the owner's "Yes, plan the move" in `docs/decisions/2026-11-13-roadmap-answers.md`), not built in R04.

Where the job lifecycle is written today (test modules left out):

- `.status = Status::` in `jobs.rs`: `open` (a restart), `crashed`, `stop`, `answer_parked`, `answer_kept_question`, `work` (its end), `run_obi_review`, `run_owner_job`, `end_done` and `next_round`; in `jobs/account_limits.rs`: `wait_for_reset`; in `jobs/retry.rs`: `queued`.
- `next_round` (`jobs.rs`) queues a job for another round, and is called from `jobs.rs` (`message_as`, `review_round_again`, `reviewed`), `jobs/account_limits.rs` (`Failover::apply`, `settle`, `resume_limit_waits`, `apply_limit_job`), `jobs/after_pr.rs` (`after_pr`) and `jobs/moves.rs` (`move_to`).
- `store.save(` of a job: `Jobs::keep` (`jobs/keep.rs`), the path every `Jobs::update` takes; `open` in `jobs.rs` (a restart); `apply_limit_job` and `reask_limit_job` in `jobs/account_limits.rs`. The vault's `add` (`vault.rs`) saves to the vault's own store, not a job.
- The `.status = Status::` lines in `routines.rs`, `jobs/research.rs` and `jobs/unjudged.rs` are in their test modules only.

Where `follow_up_of` is compared with a branch today (test modules left out):

- `jobs.rs`: `create` (a follow-up's `start_from`, and a branch of its own unless it carries one on), `follow_up` (its `carries`: whether the follow-up keeps its job's branch), `message_as`, `review_round_again`, `reviewed`, `review_left_to_owner`, `chain_tip`, `publish_action_of`, `undeletable_in` and `done_with`, with the helpers `follows_up` and `same_branch`; and the function `carries`, which says whether a job still carries its pull request.
- `jobs/account_limits.rs`: `apply_limit_job`.
- `jobs/after_pr.rs`: `may_round` (through `same_branch`).
