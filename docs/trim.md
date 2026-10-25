# Trimming Oby to its four cores

The owner's decision of 2026-10-25: Oby had become a patchwork, so the Mac app and obid go back to four cores. Sangam's design (`design/app`) stays exactly as it is: it's the demo of the whole idea, and nothing in it is trimmed. Mapped on dev at 7859f051.

1. **Connect** Claude Code, Codex and Cursor: find what's signed in, each tool with its own login.
2. **Dispatch** work between them and between accounts; when one account runs out, another carries on.
3. **Don't stop:** finish the work, ask only at the step that's truly yours, show what was done. What Oby asks waits on Home as one stack of cards, answered right there (#1442): "1/3" says how many decisions wait, the owner cycles through them, and each card links to its task.
4. **Obys talk:** review requests between teammates' Obys first, working together later. GitHub is the only outside connection; Obys do the talking, not Slack.

Under all four, **Tetrix**: Oby exists to promote it. Every agent gets Tetrix's graph through Oby's one connection, and that path has to be flawless.

The Mac app comes first. The iPhone matters, but it waits, untouched, until the Mac app is right.

What's cut leaves the code, not the history: it stays in git, and Sangam's design keeps the picture for when it comes back.

## What stays

- obid's agents and jobs: `accounts`, `agent`, `acp` (Cursor), `codex`, `conn`, `job`, `jobs`, `limits`, `usage`, `meter`, `process`, `store`, `settings`, `obi_dir`.
- Git and publishing: `git`, `github`, `github_login`, `github_work`, `repos`, `publish`, `scan`, `merge`, `review`.
- Tetrix: `tetrix`, `decisions` (top level), and the `/v1/agent/mcp/{job}` endpoint in `connections` that hands agents its tools.
- Asking and telling: `permissions`, `policy`, `triage`, `transcript`, `detail`, `activity`, `facts`, `failures`, `events`, `clear`, `conversation`, the learned-workflow part of `memory` (merge uses it).
- Plumbing: `api`, `routes`, `config`, `app`, `logs`, `token`, `secrets` (redaction, used by 36 files), `vault` (Tetrix's login lives there).
- The Obysphere: `sphere/link`, `exchange`, `inbox`, `decisions`, `membership`, `pins`, `routes`, `approval`, `answering`, `sharing` (the rules for answering other Obys, not compute sharing), `channel` and `sealed`.
- The conversation as a Claude Code (or Codex) session with an `oby` MCP server: `brain/harness`, `harness_codex`, `tools`, `actions`, `plan` (its checks). This already exists; only the plan loop around it goes.
- The Mac app's window, side panel, tasks, conversations, decisions, accounts, models, Tetrix, Obysphere, first launch, updates.
- Parked, not cut and not worked on: the iPhone and Watch apps and what serves them (`relay`, `push`, `gateway`, `devices`, `approval`, `PairingView`). #1421 waits with them.

## Batches

One pull request per batch, obid and the Mac app together, so an endpoint and its caller leave in the same change. Each keeps the workspace and the app building. Lines are source only; their tests go with them.

| # | Batch | obid | Mac app | About |
|---|---|---|---|---|
| 0 | **Freeze.** Land the open core PRs (#1420, #1422–#1439), then start no new work outside the cores | | | |
| 1 | Dead code | | `JobView` (keep `Publishing`, `MovePanel`), `ChatComposer`, `TrayRow`, `JobChip`, `LiveView`'s body, `MessageBubbleLayout`, `ThinkingLine` | 1k |
| 2 | Slack and the other connectors | first move `create_review`, `post_review_with_gh`, `decided`, `APPROVE` from `listen.rs` to `github.rs`; then `slack`, `listen`, `watch`, `sentry`, `hooks`, `event_work`, `rules`, `routines`, and the rest of `connections` | `Hooks.swift`, Slack, Sentry, watches, routines and connections in Settings and `RunnerFeed` | 13k |
| 3 | Plug-ins (after the connectors: hooks, Sentry and Slack items reach the inbox as plug-in events) | the plug-in host, its routes and its events (crates `obi-plugin-api` and `obi-plugin-host`, `pluginEvent` inbox items, `plugins/`). Keep, with local types, what writes Tetrix's and teammates' context into a job (`write_context`, `context_note`) and the Claude MCP config that carries obid's Tetrix server. Tetrix's plug-in fallback (an agent signing in by itself when Oby isn't) goes: Oby's one sign-in is the path | plug-in rows in Connectors | 9.6k |
| 4 | Remote access through Cloudflare (the Obysphere's relay replaces it) | `tunnel` | `RemoteAccess` (move `WebSignIn` to `Sphere` first), the cloudflared phase, its Settings page | 1k |
| 5 | Voice | | `Voice`, `TalkKey`, `TalkPill`, `ListeningLine`, `Setup.Permissions`, the fn step, the microphone keys and entitlement | 1k |
| 6 | Compute sharing | `sphere/vm`, `vm_image`, `guest`, `hosting`, `shares`, `core/obi-vm`, `runs_on` (~60 places) | `SharingSettings`, the obi-vm phase, run-on-another-Mac in the tray and message box | 6.2k |
| 7 | Mac checks | `/v1/checks` | `MacChecks`, its Settings rows | 0.5k |
| 8 | The plan loop | first move `nonce`, `fence`, `fence_rule`, `line` and the quick-model call out of `brain/`; then `turn`, `planner`, `rules`, `lead`, `research`, `read`, `find`, `recap`, `orchestrator`, most of `briefing` | | 11.5k |
| 9 | Oby's own box | `sandbox.rs`, the login copies and Keychain keeper (`jobs.rs:9749–11240`): agents run with the account's own config folder and the tool's own sandbox (Codex `workspace-write`, Claude Code's sandbox settings, Cursor's sandbox). Keep sign-in detection | | 3.3k rewritten |
| 10 | The Swift engine | | the Mac stops compiling `Engine/` (its models come from `apps/shared`, a few helpers move into the app); then `Engine/`, `Runner/`, `RunnerTests/` retire | 28k Swift |

About 45k of obid's 131k source lines, and about 33k lines of Swift, leave.

## Decided

- **Tetrix stays** and has to be flawless: it's why Oby exists (the owner, 2026-10-25).
- **The iPhone waits** until the Mac app is right; its code stays as it is (the owner, 2026-10-25).
- **Slack goes:** GitHub is the only outside connection, and Obys do the talking (the owner, 2026-10-25).

## Still open

- **Batch 5:** voice isn't in the four cores, but fn-to-talk was in the launch plan.
- **Batch 9:** what replaces obid's read-only box for an Obysphere review and for answering another Oby (the trust boundary of core 4), and the no-network box a repository's check runs in. Candidates: Codex's `read-only` sandbox, Claude Code's sandbox with writes denied and no network. Cursor's sandbox falls back to an allowlist on some Macs and fails with zsh on macOS 26, so Cursor stays out of reviews until it holds.
- **Memory:** the learned workflow (where a PR merges, who reviews) is core 3. Settings › Memory as its own page isn't.
