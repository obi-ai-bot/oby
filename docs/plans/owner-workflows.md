# The owner's rules and workflows as data, not code

A plan, not built (2026-11-16), checked against `origin/dev` at `6d3ca790`. It answers the owner's ask of 2026-11-16 and proposes six tasks, [B00-27](../../factory/queue/tasks/B00-27.md) to [B00-32](../../factory/queue/tasks/B00-32.md), for the owner to approve. No code was run.

> "Oby is not ours, those are our own rules, the VM, the Macbooks, the project we are working on ARE ALL SOMETHING THAT BELONG TO US AND THEY CAN'T BE WRITTEN IN STONE ON THE OBY APP THEY NEED TO BE MODULAR, DYNAMIC, EASY FOR PEOPLE TO CREATE THEIR OWN WORKFLOWS LIKE T3CODE AND OPENBOT."

Think of Oby as a workshop anyone can move into. Today a few of our own house rules are painted on its walls: which of our files need two reviews, which of our folders only a Mac can test, how we write commit messages, which branch we merge into. Whoever moves in next inherits our paint. This plan takes those rules off the walls and puts them on cards: one card the repository carries (a file in it), one the owner keeps (in obid), and the workshop's few safety rules, which stay painted because they protect everyone who works there.

## What

### What is written in stone today

Each line was read on `origin/dev` at `6d3ca790`.

| Rule | Where | Whose it really is |
|---|---|---|
| 26 paths whose change needs two approving reviews | `core/crates/obid/src/merge.rs:1305`, `SECURITY_PATHS` | 22 are general: what steers agents (`AGENTS.md`, `CLAUDE.md`, `.claude/` and the like), CI files, the secret scan's settings, `CODEOWNERS`. 4 are this repository's own docs: `docs/agents-rules-manifest.md`, `docs/build-and-run.md`, `docs/agent-clis.md`, `docs/contracts/publishing.md`. |
| 9 folders only the Mac tests | `merge.rs:978`, `MAC_ONLY` | All nine are this repository's. Three of them (`Engine/`, `Runner/`, `RunnerTests/`) are in neither this repository nor its check script any more: the Swift Mac app was deleted. The file-type half of the rule (`.swift`, `.xcodeproj`, `.xcworkspace`, `Package.resolved`, `merge.rs:991-999`) is true for anyone. |
| Two approvals by different tools for an untrusted or security-path change | `merge.rs:1393`, `two_reviews`; `merge.rs:1242` | A default the owner's word overrides (AGENTS.md, "Defaults: Oby forbids nothing"), with nowhere to say it. |
| Three review rounds, then the owner is asked | `core/crates/obid/src/review.rs:77`, `REVIEWS` | A default, same as above. |
| `main`, `master` and `trunk` aren't merged into by itself | `merge.rs:66`, `protected` | A good default. A repository whose protected branch is `release` or `production` can't say so. |
| "Merged into dev" on a job with no base yet | `core/crates/obid/src/job.rs:1921` | This repository's branch, shown for every repository. |
| Conventional Commits, told to every agent | `core/crates/obid/src/jobs.rs:369`, `jobs.rs:14468` | This repository's style. |
| `cargo test --manifest-path core/Cargo.toml`, the example in every agent's prompt | `jobs.rs:14486` | This repository's layout. |
| A job's stages: Prepare, Work, Check, Publish, Review, Merged | `job.rs:162`, `Phase`; `job.rs:1903-1924` | The dispatcher's own lifecycle. It stays (§1), but nobody can add a step to it. |

Outside obid, in the factory's own tooling (GPLv3, not the product), the pipeline and the machines are code or config too:

| What | Where |
|---|---|
| The pipeline's order: intake, spec, build, lenses, approver | `factory/bin/oby-feed:122-123` (`ACTIVE`, `STEPS`) and the functions that follow them |
| The protected paths the factory checks | `factory/bin/oby-feed:178-183`: obid's `SECURITY_PATHS`, read by a regular expression over the Rust source, plus `factory/config.json`'s `protected_paths` and `scripts/oby-check` |
| The gate VM and the Mac worker | `factory/config.json` (`remote_gate`, `mac_worker`, `gate_shards`). obid's source names no machine. |

### What is already the owner's data

- **The repository's entry in obid's configuration** (`config.json`'s `repos[]`, `core/crates/obid/src/config.rs:199`): `check`, `setup`, `checkKeep`, `defaultBranch`, `pullRequests`. `POST /v1/repos/check` keeps the owner's check there after their signed Confirm (docs/api/github.md).
- **The learned merge workflow** per repository (`core/crates/obid/src/memory.rs:131-185`): the base, the merge method, rebase first, who approves, whose checks. Saved only on the owner's yes.
- **Settings**: `home/settings.json` (models and effort per tool, limits, `accountFailover`), `home/permissions.json` (the mode), routines (`core/crates/obid/src/routines.rs`).
- **Planned, not built**: B00-06's `factory.json` per project (who runs each factory step, on which account and model) and B00-12's switches per project (factory off, ask before anything merges).

So the pieces of a configurable Oby exist. Two things are missing: a place for the owner's word on the rules in the first table, and a way for anyone to define the steps their work goes through.

## Why

- **The owner asked** (2026-11-16, quoted above). AGENTS.md already agrees: what Oby holds back on its own "are its defaults, each overridden by the owner's word". But the code gives that word nowhere to go except a change to obid's Rust.
- **It already leaks into other people's repositories.** A repository with an `Engine/` folder, common in game projects, has every change there treated as Mac-only, so its own check can't stand for the Mac's and the merge waits for the owner (`merge.rs:972`, `MAC_LEFT`; `merge.rs:1169`). A repository with a `docs/build-and-run.md` needs two approving reviews for a docs edit. Every agent is told to write Conventional Commits and shown our `core/Cargo.toml`.
- **The social contract.** "A movement, not a product": a stranger can read obid, run it and change it, and shouldn't have to change it to stop it following our house rules. "Vendor agnostic": "two approvals by different tools" names no vendor; which tools, and how many, is the owner's.
- **The factory reads Rust with a regular expression** (`oby-feed:178-183`). If `SECURITY_PATHS` changes shape, the match finds nothing, and the factory's protected list quietly shrinks to three entries. One file both of them read ends that.

### How the two products the owner named do it

Paths at T3 Code `304cf149c8` and OpenBot `6b2ccd1f2`. OpenBot is PolyForm Noncommercial: this copies what it does, never its code.

| They do | Where | What Oby takes from it |
|---|---|---|
| T3 Code: one optional checked-in file, `t3.json`, at the repository's root. Its scripts run when a worktree is created (`runOnWorktreeCreate`) or when a thread settles (`runOnSettle`). A missing or invalid file counts as no file, with a warning. | `packages/contracts/src/t3ProjectFile.ts:8,25-117`; `apps/server/src/project/T3ProjectFileLoader.ts` | A repository file, `oby.json`, that Oby only reads. |
| T3 Code: a person **imports** the file's scripts into their own project settings ("Import from t3.json"); the file alone runs nothing. | `apps/web/src/components/settings/ProjectActionsSettings.tsx:139`; the schema's own description, `t3ProjectFile.ts:74` | A command a repository offers runs only once the owner adopts it. |
| T3 Code: three layers, global, the person's project settings and `t3.json`, and each setting shows where its value came from. Commit style, review instructions and branch naming are per project. | `packages/shared/src/projectSettings.ts:21`; `packages/contracts/src/settings.ts:1171-1192`; `apps/web/src/components/settings/SettingInheritance.tsx:106-115` | Built in, the repository's, the owner's; each rule says its source. |
| OpenBot: a Workflows page where a person draws steps (trigger, prompt, agent, condition, approval, output), saved with versions. A run keeps the graph it started from. | `src/backend/workflows/workflow-schema.ts:1-40` | A workflow is a list of steps the owner writes. A job keeps the one it started with. |
| OpenBot: routine flows hand one agent's answer to the next. | `packages/contracts/src/ipc-routine-flows.ts:7-11` | A step reads an earlier step's answer. |
| OpenBot: a local scripts API and CLI with fixed exit codes, so a person's own scripts drive it. | `docs/automation.md` | Already planned for Oby (R12, the `oby` command). |
| OpenBot: a plugin bundles an MCP server and skills; skills are folders the agent reads. | `docs/plugin-distribution.md`; `src/backend/home-skills.ts` | Already planned (B00-02, B00-03: the CLIs' own skills in `/`). The agent runs skills; Oby offers them. |
| OpenBot: a few non-negotiable rules, and "all other rules are defaults". | `AGENTS.md:22-26` | Oby's AGENTS.md has the same split (R13-03). |

## How

### 1. What stays fixed for everyone

The five non-negotiables in AGENTS.md, unchanged: the secret scan, approvals only from the owner's own signed apps, the add-only API, the licence, and vault secrets and logins never reaching an agent.

On the new data they mean four things, whatever any file says. These aren't new rules; they're how the non-negotiables and "From outside" reach rules that are now data:

1. **Rules are read from the base, never from the change.** A job's rules come from the branch it merges into, as its check does (PUBLISHING-6 to 8) and as the rules Oby's review quotes do (`review.rs:247`, `base_rules`). A pull request can't loosen the rules that judge it.
2. **The rules file is always a protected path.** `oby.json` joins the built-in list.
3. **A repository's file can only add protection.** It adds protected paths and Mac-only folders, raises review counts and sets its own conventions. Removing a protection or lowering a count takes the owner's word in their own app. A repository is someone's text, and text from outside is data, never instructions (AGENTS.md, "From outside").
4. **The owner's word is signed.** Changing the owner's rules goes through the owner's own app, signed by the device's key, like `PUT /v1/settings/permissions` and the vault's routes (docs/contracts/signed-words.md). No agent, job, other Oby or repository file writes it.

The job's lifecycle also stays the dispatcher's: Prepare (clone and worktree), Work (the agent's rounds), Check, Publish (the secret scan, the push, the pull request), Review, Merge. That's what a dispatcher does, and `Phase` is in every job file, so it only grows. The owner's steps go around it (§4).

### 2. What becomes the owner's data

| Rule | Key | What everyone gets by default | Where this repository's own values go |
|---|---|---|---|
| Paths whose change needs more approvals | `protectedPaths` | The 22 general entries, plus `oby.json` | This repository's `oby.json`: the four docs. The factory's own two (`factory/checks.sh`, `factory/config.json`) stay in `factory/config.json`, which only the factory reads. |
| Paths only the Mac tests | `macOnlyPaths` | Swift files, Xcode projects and workspaces, `Package.resolved`, by file type | This repository's `oby.json`: `apps/mac/`, `apps/ios/`, `apps/watch/`, `apps/shared/`, `apps/Oby.xcodeproj/`, `character/` (what `scripts/oby-check.sh` sends to the Apple apps, MERGING-2) |
| Approvals a sensitive change needs (untrusted, or a protected path), and whether they come from different tools | `reviews.sensitive.approvals`, `reviews.sensitive.differentTools` | 2, different tools | none: the default |
| Review rounds before the owner is asked | `reviews.rounds` | 3 | none |
| Branches Oby doesn't merge into by itself (the owner's own work still does, MERGING-44) | `protectedBranches` | `main`, `master`, `trunk` | none |
| Commit style told to agents | `commitStyle` | none: the agent follows the repository's own `AGENTS.md` and `CLAUDE.md`, which obid already quotes to it (`review.rs:261`, `rules_line`) | `"conventional"` |
| Commands at points of a job's life | `hooks.afterWorktree`, `hooks.beforePublish` | none (`afterWorktree` is today's `setup`) | offered in `oby.json`, adopted by the owner |
| The steps work goes through | `workflows` | none: today's lifecycle | the factory's pipeline, as one workflow (§4) |

Two of these change what other repositories get today, on purpose: they stop being Mac-only for `Engine/` and the rest, and stop needing two reviews for our four docs (B00-27); their agents stop being told Conventional Commits unless their repository or owner says so (B00-29). This repository keeps all of it through its `oby.json`.

The learned merge workflow (`memory.rs`) and B00-06's `factory.json` stay as they are. The first says where and how a repository's work merges, the second who runs each factory step. The rules say what must be true before a merge, and a workflow says which steps come first.

### 3. Where it lives

Three layers, as T3 Code has them. Each rule's answer says which layer its value came from.

1. **Built in**, in obid: §2's defaults, in one module (`rules.rs`) with one function every caller asks, `rules_for(repo, base)`. No other list stays in obid.
2. **The repository's file**, `oby.json` at its root, read from the base and optional. No file is no file; an invalid one is reported on the job and ignored, as T3 Code does. It's for what a team shares: protections, conventions and offers.
3. **The owner's word**, on the repository's entry in obid's configuration, beside its `check` and `setup` (`config.rs:199`; per profile once A00-79 moves repositories there). It's read and changed through `GET` and `PUT /v1/repos/rules`, signed. It can add, remove or change anything, the built-in defaults included: Oby forbids nothing.

How they combine: the built-in values, plus the repository's additions (protections only grow: the union of paths, the larger count), then the owner's changes on top. Commands and workflows from a repository are offers: they don't run until the owner adopts them. An adopted one is kept with the commit it came from, and when the repository changes it, the change is offered again.

This repository's `oby.json` would start like this (B00-27 writes the real one):

```json
{
  "schema": "oby.repo/1",
  "protectedPaths": [
    "docs/agents-rules-manifest.md", "docs/build-and-run.md",
    "docs/agent-clis.md", "docs/contracts/publishing.md"
  ],
  "macOnlyPaths": ["apps/mac/", "apps/ios/", "apps/watch/", "apps/shared/", "apps/Oby.xcodeproj/", "character/"],
  "commitStyle": "conventional"
}
```

**Machines are the owner's too**, and never in a repository file. A workflow step names a role (`linux-gate`, `mac`); the owner's side says which machine plays it. For now that's `factory/config.json` (`remote_gate`, `mac_worker`), the owner's own file, not the app. obid gets such a map only when it runs steps on another machine, which is later (§6).

### 4. Steps and hooks: the owner's workflow

**Hooks** are the small part: a command at a fixed point in a job's life, like T3 Code's scripts.

- `afterWorktree` is today's `setup` (PUBLISHING-23), unchanged, now one entry in a list.
- `beforePublish` runs after the check passes, before the secret scan and the push. It fails as a check fails: nothing goes out, and the agent hears why and gets a round to fix it.
- Both run in the job's box under the check's rules. Each shows as a stage of the job, and the stage list is open strings on the wire (`apps/electron/src/shared/obid-wire.ts:22`), so a new stage breaks no client.
- "After a merge" is a routine on a merge event, which R12 already plans, not a hook.

**A workflow** is the bigger part: an ordered list of steps around the build, written by the owner or offered by a repository.

- Each step has an `id`, a `kind`, and what that kind needs:
  - `agent`: a prompt file from the repository at the base (`.claude/agents/intake.md` is one today) and what it reads: the task, or an earlier step's answer. Its answer is kept for the steps after it.
  - `build`: the job itself, today's lifecycle. One per workflow.
  - `check`: a command, on a machine role.
  - `review`: a review job with its prompt file (a lens), ending `VERDICT: APPROVE` or `VERDICT: CHANGES`. CHANGES sends the build a fix round.
  - `approve`: an agent's last word, APPROVE or ESCALATE.
  - `owner`: waits for the owner's signed click. B00-12's "ask before anything merges" is this step.
- `when` says whether a step runs, from header lines an earlier step wrote (`RISK: high`, `SURFACES: ui`), never from a model call of obid's: rules decide what happens to a job (AGENTS.md, "Dispatcher, not harness").
- Who runs each step (tool, account, model, effort) stays in B00-06's `factory.json`, keyed by the step's `id`.
- A job keeps the workflow it started with, as OpenBot's runs do, so an edit mid-run changes nothing under it.

**The Claude Code factory's pipeline becomes one workflow** in this repository's `oby.json`, with the prompts already here:

```json
"workflows": {
  "factory": [
    { "id": "intake",   "kind": "agent",   "prompt": ".claude/agents/intake.md" },
    { "id": "spec",     "kind": "agent",   "prompt": ".claude/agents/spec-writer.md", "when": { "DECISION": "ACCEPT" } },
    { "id": "build",    "kind": "build" },
    { "id": "gate",     "kind": "check",   "runsOn": "linux-gate", "run": "factory/checks.sh" },
    { "id": "prove",    "kind": "check",   "runsOn": "linux-gate", "when": { "SURFACES": ["proofs", "model"] } },
    { "id": "code",     "kind": "review",  "prompt": ".claude/agents/reviewer-code.md" },
    { "id": "ux",       "kind": "review",  "prompt": ".claude/agents/reviewer-ux.md", "when": { "SURFACES": ["ui"] } },
    { "id": "ui",       "kind": "review",  "prompt": ".claude/agents/reviewer-ui.md", "when": { "SURFACES": ["ui"] } },
    { "id": "security", "kind": "review",  "prompt": ".claude/agents/reviewer-security.md", "when": { "SURFACES": ["security"] } },
    { "id": "proofs",   "kind": "review",  "prompt": ".claude/agents/reviewer-proofs.md", "when": { "SURFACES": ["proofs", "model"] } },
    { "id": "approve",  "kind": "approve", "prompt": ".claude/agents/approver.md" }
  ]
}
```

Someone else writes three steps (build, one review, owner), or just one (build). Nobody edits obid.

It moves in two steps, smallest first. First, `oby-feed` reads its steps from this file instead of `ACTIVE` and `STEPS` (B00-31): the pipeline is data, proven on the real factory, with no change to obid. Then obid runs a project's workflow itself (B00-32), so anyone with Oby has one, not only this repository's feeder.

How this differs from [docs/workflows.md](../workflows.md) (2026-10-18): that design rebuilds obid's own internals as typed-node flows, with a model router and a model loop of obid's own. This plan needs neither. It adds a list the owner writes around the job's existing lifecycle, and leaves every model call to the agent CLIs ("Dispatcher, not harness", 2026-11-05).

### 5. Order of work

| Task | What the owner gets | Size | Depends on |
|---|---|---|---|
| [B00-27](../../factory/queue/tasks/B00-27.md) | This repository's paths and folders leave obid's code for its own `oby.json`; other repositories stop inheriting them; the factory reads the same file | S | none |
| [B00-28](../../factory/queue/tasks/B00-28.md) | Every rule in §2 is one setting per repository, changed in the owner's own app, each saying where its value came from | M | B00-27 |
| [B00-29](../../factory/queue/tasks/B00-29.md) | obid's fixed words stop naming this repository's habits: commit style from the rules, a neutral example, the repository's own branch | S | B00-27 |
| [B00-30](../../factory/queue/tasks/B00-30.md) | Hooks: `afterWorktree` and `beforePublish`, set by the owner, offered by `oby.json` | S | B00-28 |
| [B00-31](../../factory/queue/tasks/B00-31.md) | The factory's pipeline is a workflow in `oby.json`, and `oby-feed` follows it | M | B00-27 |
| [B00-32](../../factory/queue/tasks/B00-32.md) | obid runs a project's workflow itself, so any owner can write one | L (intake may split it) | B00-31, B00-06, A00-70 |

B00-27 goes first: it's small, it fixes a real leak, and it puts in place the file and the two rules everything after it relies on (read from the base; a repository only adds protection). After it, B00-28, B00-29 and B00-31 can run side by side.

B00-28 and B00-32 each need a screen of Sangam's before the app shows them: a Rules section in a repository's settings, and a workflow's steps beside his factory screens 14 and 15. obid serves the data first; the screens follow his design and words.

Each task is the dispatcher's lane: which rules hold a job, which step runs next, what state the work is in. None reads, edits or runs anything for an agent, and none adds a model call to obid.

### 6. Not in this plan

- **Lanes, boxes and logins** (LANES-1 to 18; "Logins and secrets"). They're how the non-negotiables are kept, not house rules.
- **A drawn graph editor**, as OpenBot has. A list covers the factory; a graph can come when someone needs a branch a list can't say.
- **obid running steps on another machine.** Compute sharing is retired; the factory keeps doing it in its own tooling.
- **The phone apps**, frozen since 2026-11-13.

### 7. For the owner to decide

1. **Approve B00-27 first.** It's small, and every later task builds on its file.
2. **The file's name: `oby.json`** at the repository's root, as T3 Code has `t3.json`. `.obi/` is obid's own folder inside a worktree, so a repository's file shouldn't live there.
3. **Two defaults change for other repositories** (§2): the Mac-only and two-review paths shrink to the general ones, and agents are told no commit style unless the repository or the owner names one. This repository keeps all of it.
4. **One change to B00-06 before it's built**: its `steps` take the ids the project's workflow names instead of a fixed list; its `when` (which lens runs for which surface) moves to the workflow's steps; and its `rounds` moves to the rules (`reviews.rounds`). Then each fact lives in one place: the workflow says which steps run and when, `factory.json` who runs them, the rules what must hold before a merge. B00-31 and B00-32 rely on it.
