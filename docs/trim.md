# Trimming Oby to its four cores

The owner's decision of 2026-10-25: Oby had become a patchwork, so the Mac app and obid go back to four cores. Sangam's design (`design/app`) stays exactly as it is: it's the demo of the whole idea, and nothing in it is trimmed. Mapped on dev at 7859f051.

1. **Connect** Claude Code, Codex and Cursor: find what's signed in, each tool with its own login.
2. **Dispatch** work between them and between accounts; when one account runs out, another carries on.
3. **Don't stop:** finish the work, ask only at the step that's truly yours, show what was done. What Oby asks waits on Home as one stack of cards, answered right there (#1442): "1/3" says how many decisions wait, the owner cycles through them, and each card links to its task.
4. **Obys talk:** review requests between teammates' Obys first, working together later. GitHub is the only outside connection; Obys do the talking, not Slack.

Under all four, **Tetrix**: Oby exists to promote it. Every agent gets Tetrix's graph through Oby's one connection, and that path has to be flawless.

The Mac app comes first. The iPhone waited, untouched, until the Mac app was right; since Oby 0.4.1 (2026-10-26) it's back, showing what the Mac's launch build shows and pairing through the Obysphere.

What's cut leaves the code, not the history: it stays in git, and Sangam's design keeps the picture for when it comes back.

## What stays

- obid's agents and jobs: `accounts`, `agent`, `acp` (Cursor), `codex`, `conn`, `job`, `jobs`, `limits`, `usage`, `meter`, `process`, `store`, `settings`, `obi_dir`.
- Git and publishing: `git`, `github`, `github_login`, `github_work`, `repos`, `publish`, `scan`, `merge`, `review`.
- Tetrix: `tetrix`, `decisions` (top level), and the `/v1/agent/mcp/{job}` endpoint in `connections` that hands agents its tools.
- Asking and telling: `permissions`, `policy`, `triage`, `transcript`, `detail`, `activity`, `facts`, `failures`, `events`, `clear`, `conversation`, the learned-workflow part of `memory` (merge uses it).
- Routines: obid's `routines` module and its routes, kept by the owner's decision of 2026-11-13 (see Decided).
- Plumbing: `api`, `routes`, `config`, `app`, `logs`, `token`, `secrets` (redaction, used by 36 files), `vault` (Tetrix's login lives there).
- The Obysphere: `sphere/link`, `exchange`, `inbox`, `decisions`, `membership`, `pins`, `routes`, `approval`, `answering`, `sharing` (the rules for answering other Obys, not compute sharing). (`channel` and `sealed` went with compute sharing on 2026-10-27; the phone relay's frames are `relay/frame.rs`.)
- The conversation as a Claude Code (or Codex) session with an `oby` MCP server: `brain/harness`, `harness_codex`, `tools`, `actions`, `plan` (its checks). This already exists; only the plan loop around it goes.
- The Mac app's window, side panel, tasks, conversations, decisions, accounts, models, Tetrix, Obysphere, first launch, updates.
- The iPhone app and what serves it (`relay`, `push`, `gateway`, `devices`, `approval`, `PairingView`): back after 0.4.1, pairing through the Obysphere with no Tailscale (#1421's work), hiding what the Mac's launch build hides. The Watch app was removed from the iPhone build on 2026-11-13 (the owner's decision, docs/decisions/2026-11-13-roadmap-answers.md, R03-07); git history is its archive.

## Batches

One pull request per batch, obid and the Mac app together, so an endpoint and its caller leave in the same change. Each keeps the workspace and the app building. Lines are source only; their tests go with them.

| # | Batch | obid | Mac app | About | Status |
|---|---|---|---|---|---|
| 0 | **Freeze.** Land the open core PRs (#1420, #1422–#1439), then start no new work outside the cores | | | | done (#1420, #1422–#1439) |
| 1 | Dead code | | `JobView` (keep `Publishing`, `MovePanel`), `ChatComposer`, `TrayRow`, `JobChip`, `LiveView`'s body, `MessageBubbleLayout`, `ThinkingLine` | 1k | done (#1441; the rest of the Mac app went with #1860) |
| 2 | Slack and the other connectors | first move `create_review`, `post_review_with_gh`, `decided`, `APPROVE` from `listen.rs` to `github.rs`; then `slack`, `listen`, `watch`, `sentry`, `hooks`, `event_work`, `rules`, and the rest of `connections`. Routines are kept (2026-11-13, see Decided) | `Hooks.swift`, Slack, Sentry, watches and connections in Settings and `RunnerFeed` | 13k | done (#1443, #1446; `event_work` in #1512, the rest of `connections` in #1516, the Mac app's rest with #1860): the review functions went to `review.rs`; `connections.rs` keeps the `/v1/agent/mcp/{job}` endpoint (What stays) |
| 3 | Plug-ins (after the connectors: hooks, Sentry and Slack items reach the inbox as plug-in events) | the plug-in host, its routes and its events (crates `obi-plugin-api` and `obi-plugin-host`, `pluginEvent` inbox items, `plugins/`). Keep, with local types, what writes Tetrix's and teammates' context into a job (`write_context`, `context_note`) and the Claude MCP config that carries obid's Tetrix server. Tetrix's plug-in fallback (an agent signing in by itself when Oby isn't) goes: Oby's one sign-in is the path | plug-in rows in Connectors | 9.6k | done (#1512; the Mac app's plug-in rows went with #1860) |
| 4 | Remote access through Cloudflare (the Obysphere's relay replaces it) | `tunnel` | `RemoteAccess` (move `WebSignIn` to `Sphere` first), the cloudflared phase, its Settings page | 1k | done (#1510; the Mac app's Remote access side went with #1860) |
| 5 | Voice | | `Voice`, `TalkKey`, `TalkPill`, `ListeningLine`, `Setup.Permissions`, the fn step, the microphone keys and entitlement | 1k | done (#1860: the Swift app's voice went with it on 2026-11-02) |
| 6 | Compute sharing | `sphere/vm`, `vm_image`, `guest`, `hosting`, `shares`, `core/obi-vm`, `runs_on` (~60 places) | `SharingSettings`, the obi-vm phase, run-on-another-Mac in the tray and message box | 6.2k | done in part (#1515; the Mac app's side went with #1860): `core/obi-vm` is still in the tree |
| 7 | Mac checks | `/v1/checks` | `MacChecks`, its Settings rows | 0.5k | done (#1516; the Mac app's side went with #1860) |
| 8 | The plan loop | first move `nonce`, `fence`, `fence_rule`, `line` and the quick-model call out of `brain/`; then `turn`, `planner`, `rules`, `lead`, `research`, `read`, `find`, `recap`, `orchestrator`, most of `briefing` | | 11.5k | done in part (#1553, #1946, #1948, #1952): `planner`, `rules`, `lead`, `research` and `orchestrator` are gone; `turn.rs`, `read.rs`, `find.rs`, `recap.rs` and `briefing.rs` (with `nonce`, `fence`, `fence_rule`) are still in `brain/` |
| 9 | Oby's own box | `sandbox.rs`, the login copies and Keychain keeper (`jobs.rs:9749–11240`): agents run with the account's own config folder and the tool's own sandbox (Codex `workspace-write`, Claude Code's sandbox settings, Cursor's sandbox). Keep sign-in detection | | 3.3k rewritten | done in part (#2057, #2099): jobs pick a lane and the owner's own Claude Code and Codex work runs outside Oby's box (docs/contracts/lanes.md); the Keychain keeper went; `sandbox.rs` and the login copies stay for the foreign lane |
| 10 | The Swift engine | | the Mac stops compiling `Engine/` (its models come from `apps/shared`, a few helpers move into the app); then `Engine/`, `Runner/`, `RunnerTests/` retire | 28k Swift | done (#1860, 2026-11-02: `Engine/`, `Runner/`, `RunnerTests/` gone) |

About 45k of obid's 131k source lines, and about 33k lines of Swift, leave.

## Decided

- **Tetrix stays** and has to be flawless: it's why Oby exists (the owner, 2026-10-25).
- **The iPhone waited** until the Mac app was right (the owner, 2026-10-25); after 0.4.1 it's back, as the Mac's launch build (the owner, 2026-10-26).
- **Slack goes:** GitHub is the only outside connection, and Obys do the talking (the owner, 2026-10-25).
- **`apps/web` goes:** nothing has served it since #1510, so it leaves the repository and git history is its archive, as for the Swift Mac app. A future browser client is the Electron renderer running on a web runtime: the same UI, only the bridge differs. The owner closes #1290, #1291, #1198, #1170 and #1114 and retargets #1228 to the iPhone and Electron ([R03-06](decisions/2026-11-13-roadmap-answers.md#r03-06-q008); the owner, 2026-11-13).
- **The Watch app goes:** removed from the iPhone build, with its rows in the iPhone's Settings and nothing in their place (the owner, 2026-11-13, R03-07).
- **Routines stay:** obid's `routines` and their routes are kept, and batch 2 no longer cuts them; the [answer](decisions/2026-11-13-roadmap-answers.md#r13-01-q011) notes they should work as OpenBot's do (the owner, 2026-11-13).

## Still open

- **Batch 5:** voice isn't in the four cores. The Swift app's voice and fn-to-talk went with it on 2026-11-02; Sangam's Voice comes back later.
- **Batch 9:** what replaces obid's read-only box for an Obysphere review and for answering another Oby (the trust boundary of core 4), and the no-network box a repository's check runs in. Candidates: Codex's `read-only` sandbox, Claude Code's sandbox with writes denied and no network. Cursor's sandbox falls back to an allowlist on some Macs and fails with zsh on macOS 26, so Cursor stays out of reviews until it holds.
- **Memory:** the learned workflow (where a PR merges, who reviews) is core 3. Settings › Memory as its own page isn't.
