# AGENTS.md's rules, and where each one is

Every line of the root `AGENTS.md` at `3ff84b79` that states a rule (each bullet and paragraph, not a heading or a table row), in that file's order, under its line number there, with the file that now holds it. Each entry quotes the line as the root wrote it, its links relative to the repository root. `scripts/oby-check.sh` reads only the entry lines and fails when a line is no longer in the file its entry names (`docs/contracts/ci.md`, CI-42 to 44): a change to a rule changes its entry in the same change.

### L3
- `AGENTS.md`: Oby is a Mac app in a window that hands coding work to the user's own Claude Code, Codex and Cursor CLIs (Gemini later). Its desktop app, begun as a copy of Sangam's design, is the Electron app on obid, which stays in Rust ("How we work"). The product is in [README.md](README.md); what is built, and in which order, is the roadmap ("How we work"; the F1 to F16 milestones are retired).

### L5
- `AGENTS.md`: Each rule here is stated once; its "why" links the decision that holds its history ([docs/decisions/](docs/decisions/README.md)). Don't argue by citing this file: say what a rule protects and link its decision.

### L9
- `AGENTS.md`: The owner's word in the app doesn't change these five ([why](docs/decisions/2026-11-13-roadmap-answers.md#r13-03-q019)).

### L11
- `AGENTS.md`: **Secret scan.** Everything that leaves is scanned for secrets first, and nothing goes out unless the scan is clean; if the scanner can't run, nothing goes out.

### L12
- `AGENTS.md`: **Approvals.** Approvals come only from the owner's own apps (the Electron app on this Mac, paired devices), signed by the device's key, never from another Oby, a job, GitHub text or anything received.

### L13
- `AGENTS.md`: **Add-only API.** obid's API only grows: new keys and values, never a renamed or removed one, and no new value in an enum the Swift apps decode as closed.

### L14
- `AGENTS.md`: **Licence.** Oby is open source under Apache-2.0 ([LICENSE](LICENSE)); the factory's own files are GPLv3, as [factory/README.md](factory/README.md) says.

### L15
- `AGENTS.md`: **Logins and vault secrets.** Secrets in Oby's vault never reach an agent (no prompt, environment, file, argument or log), and obid reads, stores or forwards an agent's login only as [Logins and secrets](#logins-and-secrets) states.

### L19
- `AGENTS.md`: Oby is an orchestrator, never a gatekeeper. Whatever the owner tells it to do, with a click or in words in their own app, Oby does or hands to an agent: merge into `main`, push to the default branch, force-push, anything ([why](docs/decisions/2026-10-31-oby-forbids-nothing.md)).

### L20
- `AGENTS.md`: The limits are the agents' own (the LLM, Claude Code's, Codex's, Cursor's or Antigravity's own permissions and refusals) and the code host's (GitHub's branch protection and required reviews). When one of those stops a request, Oby tells the owner plainly what stopped it and how to change it; it never refuses the owner on its own account.

### L21
- `AGENTS.md`: What Oby holds back is only what it would do by itself, unasked: those are its defaults, each overridden by the owner's word. The five non-negotiables are not defaults.

### L22
- `AGENTS.md`: Never write "forbidden", "never, even on the owner's click" or "in every case" for something the owner asks for, in a contract, a brief, a card or code.

### L23
- `AGENTS.md`: Its defaults, each stated once below: no force-pushing, nor merging into `main` but the owner's own work, on its own ([why](docs/decisions/2026-11-14-own-work-merges-itself.md)); the safe merge default until it has learned a repository's workflow; nothing going out by itself unless the job's checks passed; two approving reviews, or the owner's click, for an untrusted or security-path pull request; and the like, below.

### L27
- `AGENTS.md`: Every decision, here and in a contract, is checked against these three, for Oby, the Obysphere and Oby Cloud ([why](docs/decisions/2026-10-28-the-social-contract.md)):

### L29
- `AGENTS.md`: **Vendor agnostic, no lock-in.** Oby works with any agent, model and code host (GitHub is one adapter, never a requirement); the Obysphere is a protocol any agent can speak, not only Oby; Oby Cloud, when it comes, is one registry among others. Nothing is hard-coded to one provider.

### L30
- `AGENTS.md`: **Privacy first, local first.** Oby works fully with no account and nothing stored off the Mac (like Obsidian): no sign-in, no user name, its data under obid's home. An Oby is a UUID with the name it's given, never a global identity; Obyspheres are joined by private links. Tying an Oby to an identity (Oby Cloud: settings sync, a global registry) is the owner's choice, later, never forced.

### L31
- `AGENTS.md`: **A movement, not a product.** All three are open source for others to build on, as Cursor was built on VS Code; a stranger can read obid, run it and change it.

### L33
- `AGENTS.md`: Three directions follow, in order: **Oby as a flexible dispatcher** the owner adjusts through Oby itself (after the MVP: Home sections a person shapes, such as runtime artifacts for a developer, manager or CTO; basic features adjusted over an opinionated UX); **the Obysphere**, the rules for agents working together (Dmytro's); **Oby Cloud**, an insanely secure global registry of agents (deferred). For now the MVP stays small: what launches is what works; anything extra that breaks goes.

### L37
- `AGENTS.md`: Oby dispatches the user's agents and joins them to the Obysphere, not competing with their harnesses: what no harness has is the trust between different people's agents ([why](docs/decisions/2026-11-05-dispatcher-not-harness.md)). Before adding state, an endpoint, a tool or a model call to obid, say which it is:

### L39
- `AGENTS.md`: **The agent's.** Reading, editing, running commands, a turn's own retries, its context. It stays in the CLI. That Claude Code does something is a reason to leave it there, not to copy it.

### L40
- `AGENTS.md`: **The dispatcher's.** Which agent, account and worktree; start, stop and resume; what state the work is in; asking the owner; showing what was done. Rules decide what happens to a job, not a model call of obid's.

### L41
- `AGENTS.md`: **Between owners.** Another Oby's request, or text from outside. Typed, quoted and boxed: this is what Oby's box and the rules below it are for.

### L43
- `AGENTS.md`: So work runs in two lanes, step by step in [docs/contracts/lanes.md](docs/contracts/lanes.md) (LANES-1 to 18, some still proposed) and run by run in "Logins and secrets".

### L45
- `AGENTS.md`: In the **own lane**, work the owner asked for runs on their own login under the CLI's own sandbox, with no login copy (LANE-1, LANES-12). It holds every Claude Code and Codex job and conversation of the owner's own, on macOS and Linux (LANES-26, LANES-18, LANES-13), but Codex on a ChatGPT login.

### L46
- `AGENTS.md`: In the **foreign lane**, work another Oby asked for or marked untrusted stays in Oby's box. Until their steps land, that is another Oby's job or conversation, one marked `untrusted`, Oby's own review, Codex work on a ChatGPT login, and every Cursor job and conversation.

### L47
- `AGENTS.md`: `claudeOwnSandbox` is accepted and ignored (LANE-9). Each step changes those rules in the same pull request.

### L48
- `AGENTS.md`: `jobs.rs` may only shrink (`jobs_rs_limit` in `scripts/oby-check.sh`); a raise of it comes in a pull request of its own, a `# payback:` line directly above it naming the issue or task whose cut repays it (docs/contracts/ci.md, CI-39 to 41). The Rust core's total lines are reported, never limited ([why](docs/decisions/2026-11-16-obid-lines-reported.md)).

### L49
- `AGENTS.md`: obid's tests may only lose `thread::sleep`/`time::sleep` calls (`tests_sleep_limit` in `scripts/oby-check.sh`, docs/contracts/ci.md): a test waits for state with `support::within`.

### L50
- `AGENTS.md`: A failed test is retried only when on the flaky list in `core/.config/nextest.toml`; the list may only shrink, and a new entry comes with its fixing task file in the same change (docs/contracts/ci.md, CI-26 to 28).

### L54
- `AGENTS.md`: Branch from `dev` and open pull requests into `dev`; `dev` goes into `main` as "Carrying and merging" says.

### L55
- `AGENTS.md`: A merge into `dev` goes through GitHub's merge queue and needs its `Check` (docs/ci.md, "How a merge lands"): GitHub's limit, which the owner sets in the repository's settings. The Mac's check still gates each pull request before approval, and the nightly full Mac run (R01-14) checks `dev` as merged.

### L56
- `AGENTS.md`: Planning runs in layers: Sangam's design (his screens in `apps/electron`, run on the mock) → the board "Oby — Roadmap" (Now, Next, Later, In review, Done; a weekly Sprint field, Area and Priority) → an issue with its business contract (`feature.yml`, `bug.yml`) → a pull request with its tests ([why](docs/decisions/2026-10-29-planning-in-layers.md)). Dates and sprints live on the board only; take work from Now by priority, and read an issue's spikes first.

### L57
- `AGENTS.md`: The roadmap is the task files in `factory/queue/` (180 tasks in 13 milestones and 10 sprints, `factory/queue/README.md`), built by the software factory, and the board "Oby — Roadmap" is cleared, its issues closed ([why](docs/decisions/2026-11-12-roadmap-is-the-task-files.md)). Where this file says the board, Now or an issue's contract, read the task file and the active sprint (`sprint` in `factory/config.json`).

### L58
- `AGENTS.md`: A task's contract stays in its file while proposed and moves to `docs/contracts/<area>.md` in its own pull request. Anything that breaks while we use Oby is a GitHub bug issue the same day.

### L59
- `AGENTS.md`: By default, an agent starts work on a GitHub feature issue only once the owner, who alone sets it, has set its `approved` label, unless they tell it to start; a task file in `factory/queue/` in the active sprint is already the owner's approved work and needs no label, bugs and chores aren't covered, and the issue's Lane field ("Dispatcher, not harness") says whose work it is ([why](docs/decisions/2026-11-13-roadmap-answers.md#r13-10-q023)).

### L60
- `AGENTS.md`: The week: Monday, Sangam shows for 30 minutes what he designed since the last, and 5 to 8 ready issues go into the sprint; daily, we use Oby for real work; Friday, a demo on a real Mac, then 15 minutes on what to change.

### L61
- `AGENTS.md`: **Ready** means a contract with numbered scenarios (`FOLDERS-3`), the Sangam screen it serves, what obid owns and what the app shows, and the files it touches. The words a person sees come from his screen, never from the contract.

### L62
- `AGENTS.md`: **Done** means every scenario is a test named after it; `scripts/oby-check.sh`, gitleaks and the Brain tool's tests (once it exists) pass on the same commit; `docs/api/<area>.md` matches the code; and the issue's real-Mac steps ran.

### L63
- `AGENTS.md`: Borrowed from Dieta Flexível: a rule is written once and imported, never copied; every rule gets a test that fails without it; each workstream lists the files it owns.

### L64
- `AGENTS.md`: Plans live in [obi-ai-bot/oby-plan](https://github.com/obi-ai-bot/oby-plan) (internal), not here, and stay dateless: the Obysphere roadmap and its epics, their research, and unscheduled ideas ([why](docs/decisions/2026-10-26-plans-live-in-oby-plan.md)). Features are still issues here and on the board; when a plan's epic has a piece that lands in Oby, follow its order and its "done when".

### L65
- `AGENTS.md`: The desktop client is the Electron app, `apps/electron`, on obid's API, and obid (`core/`) stays the engine ([why](docs/decisions/2026-10-26-electron-is-the-desktop-app.md)).

### L66
- `AGENTS.md`: The Swift Mac app, with `Engine/`, `Runner/` and `RunnerTests/`, is deleted, git history its archive ([why](docs/decisions/2026-11-02-swift-mac-app-deleted.md)).

### L67
- `AGENTS.md`: The iPhone and Apple Watch apps stay in Swift on obid's API, so that API stays add-only ("Non-negotiable", above).

### L68
- `AGENTS.md`: Work here is Oby only: Dmytro owns the Obysphere (obi-ai-bot/obi-sphere: its server, spec and deploys) ([why](docs/decisions/2026-10-26-work-here-is-oby-only.md)). Don't open pull requests there or deploy it unless the owner asks.

### L69
- `AGENTS.md`: Track the work at every step, in the same turn: its task file, or a GitHub issue's `status:*` labels and links, and the board where an item still has a row. The steps are in [.claude/skills/ship/SKILL.md](.claude/skills/ship/SKILL.md).

### L70
- `AGENTS.md`: Commits follow Conventional Commits. Agents commit unsigned in their worktree (they have no signing key); Oby signs commits when it pushes them. Report the checks you actually ran and what they printed.

### L74
- `AGENTS.md`: Every new feature has a business behavior contract before it's built, using [docs/business-contract-template.md](docs/business-contract-template.md). While proposed it lives in the feature issue (`feature.yml`), linked to Sangam's screen; once built it moves to `docs/contracts/<area>.md`, with numbered scenarios (`<AREA>-<n>`) its tests are named after, and its runner API goes in `docs/api/<area>.md` ("Build and run").

### L75
- `AGENTS.md`: Link the contract from the issue and the code; update it when the owner's decisions change. Mark proposed behavior apart from what is implemented. A bug fix that changes or clarifies a promised behavior updates its contract.

### L76
- `AGENTS.md`: The contract names the user outcome, triggers, states, decisions, observable results, failure and recovery, invariants and acceptance scenarios, and what obid owns and what clients show or send. Existing security, API compatibility and Sangam design rules apply; a contract doesn't override them.

### L77
- `AGENTS.md`: A contract states what is true now; its history goes to the decision log ([docs/business-contract-template.md](docs/business-contract-template.md), "Current behavior, not history").

### L78
- `AGENTS.md`: **Clean Architecture:** business decisions live in obid's domain and application logic, apart from UI, transport, persistence and provider details; outer adapters translate through explicit boundaries, and dependencies point toward the business rules. Use the existing architecture where it serves; add boundaries that reduce coupling, not layers for their own sake.

### L79
- `AGENTS.md`: **Clean Code:** clear names, focused responsibilities, explicit state and errors, small cohesive changes. Avoid duplicated policy and hidden side effects; keep behavior outside the contract as it is. Test observable outcomes and meaningful failures at the right boundary.

### L83
- `AGENTS.md`: Sangam owns Oby's UI, UX and DX: his screens, words, flows and tooling are followed as he made them ([why](docs/decisions/2026-10-29-sangam-owns-ui-ux-dx.md)). His v1 lives in `apps/electron` (#1564: `FirstRun.tsx`, Home and the side panel, the message box, Settings; [apps/electron/AGENTS.md](apps/electron/AGENTS.md)); `design/app` (the `sangam-design` configuration in `.claude/launch.json`) holds his earlier mockup.

### L84
- `AGENTS.md`: His design is the source of truth for how Oby looks and behaves, on the Mac and the iPhone: its screens, words and logic. Where this file or a doc disagrees with it on what the user sees or does, the design wins and the doc changes ([why](docs/decisions/2026-10-18-sangams-design-leads.md)).

### L85
- `AGENTS.md`: The rules below still hold, as how his design is built, never as a reason to change what the user sees: keep his experience exactly, with the protection underneath. When a rule can't be kept without changing his design, ask the owner; don't weaken either on your own.

### L86
- `AGENTS.md`: When his design needs something obid lacks, obid adds it. A choice his screen keeps "locally: obid has no setting for it yet" is obid work: an issue linked to his screen, never a reason to hide, trim or roll back his control.

### L87
- `AGENTS.md`: Where his design and an earlier decision differed, his wins: a message to a finished task starts a follow-up with the same kind, agent, account and repository; Oby on Home follows the pointer anywhere on screen while its window is visible; a new user is set up by his first run (`FirstRun.tsx`: Hi, Tools, Knowledge, Obysphere, then Home), not in a conversation; the window has no voice button.

### L91
- `docs/launch-build.md`: Oby launches lean: where this differs from Sangam's design, this wins until the owner says otherwise ([why](docs/decisions/2026-10-23-the-launch-build.md)). The plan, workstreams and checklist: https://claude.ai/artifact/GUdcsv2sLN3CqviHVDSYCE.

### L93
- `docs/launch-build.md`: What launches: Home, conversations, tasks and the sidebar; Sangam's Voice comes back later, and the microphone rule below holds.

### L94
- `docs/launch-build.md`: Mac share (Settings › Sharing), Runner share (Run on another Mac, Shared with you, Use X's share) and Remote access are hidden on the Mac and the iPhone, but for the next bullet. obid's side went with the strip to a dispatcher, and Sangam's screens stay the target when they return ([why](docs/decisions/2026-10-27-strip-to-a-dispatcher.md)). A `showEverything` default, with no UI, shows them on Deskree's own Macs.

### L95
- `docs/launch-build.md`: Settings › Remote access shows in the Electron app with pairing an iPhone through the owner's Obysphere (Pair a device, Paired devices) ([why](docs/decisions/2026-11-01-remote-access-in-electron.md)). "Runner" is never a word a person sees, on the Mac or the iPhone.

### L96
- `docs/launch-build.md`: Connectors show GitHub and Tetrix, and Slack is cut (docs/trim.md, [why](docs/decisions/2026-10-25-slack-cut.md)). Sangam's first run promises GitLab, Bitbucket and any MCP server in Settings later: obid work once he designs those screens.

### L97
- `docs/launch-build.md`: Oby answers in text, never aloud, and the user's words show as they talk.

### L98
- `core/AGENTS.md`: The iPhone reaches the Mac through the user's own Obysphere, sealed end to end with the Mac's key pinned from the QR: the Sphere passes sealed frames and never sees a body, a device token or a login.

### L99
- `docs/launch-build.md`: Anyone hosts an Obysphere from a Deploy to Cloudflare button (obi-ai-bot/obi-sphere, public once its `dev` is in `main`), with a workers.dev address or their own domain.

### L100
- `docs/launch-build.md`: A new user goes from download to a first task in about 5 minutes: obid on by default, the agent logins already on the Mac picked up without a click, and every repository their GitHub login can push to usable without editing `config.json`.

### L101
- `docs/launch-build.md`: After the core audit (the plan's "Core audit" tab, [why](docs/decisions/2026-10-23-core-audit.md)):

### L102
- `core/AGENTS.md`: Once a conversation has read something untrusted (a file, Bash output, an attachment), the jobs it starts start at once, marked untrusted, as standing-rule jobs do, instead of a Start card for the rest of the session.

### L103
- `core/AGENTS.md`: They run in the owner's mode, Bypass too, not capped at Sandbox ([why](docs/decisions/2026-10-27-bypass-after-an-untrusted-read.md)). Everything else the mark does stays: a clone apart, Oby's box, no declared secrets for a check written after a foreign read, the secret scan, and a second approving review of its exact head by another tool before it merges, with no card ([why](docs/decisions/2026-11-14-own-work-merges-itself.md)).

### L104
- `core/AGENTS.md`: Issues, pull requests and comments written by the owner or members of their GitHub organization count as the owner's own words, not untrusted ([why](docs/decisions/2026-10-24-org-members-words.md)). They never unlock declared secrets: a job on them needs one review and its check gets none (issue-tasks.md ISSUETASK-5, [why](docs/decisions/2026-11-08-reviews-only.md)); text from anyone else stays untrusted.

### L105
- `docs/launch-build.md`: A task ends honestly: "Pull request open · waiting for your review on GitHub", only for a pull request Oby doesn't review by itself: one published by hand ([why](docs/decisions/2026-10-24-honest-end-for-hand-published-prs.md)). Settings makes no promise about merging, and Checks on this Mac (this repository's Mac checks) is hidden and off by default.

### L106
- `docs/launch-build.md`: Settings › Vault is hidden for launch: under obid it does nothing. General › Permissions shows and is live, with Sangam's four modes (Ask, Accept edits, Plan, Bypass) and Bypass the default.

### L107
- `docs/launch-build.md`: The launch download leaves out cloudflared and obi-vm, strips obid and drops the bundled web app; Deskree's Macs get them in their own builds or on demand.

### L108
- `docs/launch-build.md`: Deskree's Obysphere relays phones for its members only; anyone else pairs through their own Sphere.

### L109
- `core/AGENTS.md`: Conversation account failover is on by default: a turn that hits its account's limit moves to another signed-in account of the same tool with room ([why](docs/decisions/2026-10-24-conversation-account-failover.md)). Settings has no row for it yet: the owner turns it off with `accountFailover` on `PUT /v1/settings/jobs`.

### L110
- `core/AGENTS.md`: A job at any account limit moves by itself, with no card, to the signed-in account with the most room of any supported tool (docs/contracts/account-limits.md, LIMITS-15, [why](docs/decisions/2026-11-10-a-job-at-a-limit-moves.md)): a new session in the same worktree and branch hears the task, the earlier rounds' summaries and the owner's words, and its line says where it went and why.

### L111
- `core/AGENTS.md`: A review stays on its account, a job whose account the owner named stays there, and work in Oby's box stays in it on the new tool. `accountFailover` (above, on by default) turns the move off, and the owner moves any job by hand with `POST /v1/jobs/{id}/move` (LIMITS-16).

### L112
- `core/AGENTS.md`: With no account with room, a job waits by itself, with no card or ask, and resumes on the first account with room again: its own in its same session 90 seconds after the reset obid knows (LIMITS-5 to 14, [why](docs/decisions/2026-11-02-a-job-waits-for-its-own-reset.md)), or another one freeing up ([why](docs/decisions/2026-11-10-wait-when-no-room.md)).

### L113
- `core/AGENTS.md`: Its line says what it waits for, Stop (the owner's click) ends it for good, and the wait survives a restart. Jobs never ask about a limit; a pending #1563 decision an older release left resolves itself the same way. The budget, ceiling, untrusted mark, mode and box are unchanged.

### L119
- `docs/contracts/merging.md`: Oby carries a task the whole way by itself: it publishes the PR, reviews it itself (its own review of the exact commit counts for the owner's own pull requests) or, where a repository's rules need another person's approval (Tetrix's), gets it approved by a teammate's Oby over the Obysphere, and merges it with the owner's own GitHub account as the repository's workflow says, as Claude Code does ([why](docs/decisions/2026-10-24-oby-carries-a-task-the-whole-way.md)).

### L120
- `core/AGENTS.md`: Obys talk to Obys; Oby asks its owner only when something goes wrong: checks that still fail after its rounds, a conflict it can't resolve, a permission it may not grant, and, but for the owner's own work, a review asking for changes it can't settle or a repository whose workflow it hasn't learned yet. What the owner asks for in their own conversation or Tasks view merges by itself on any base, `main` too, with no card; past its review's last round Oby merges, saying what the review flagged, unless it needs two approving reviews ([why](docs/decisions/2026-11-14-own-work-merges-itself.md)).

### L121
- `docs/contracts/merging.md`: How work merges is learned, not fixed: Oby learns each person's and repository's workflow (where a PR merges, rebasing first, who reviews) from what they say or do, saves it as memory only after asking "Can I remember that?" and hearing yes, and follows it, saying so ("Merged into dev, as you like"; docs/onboarding.md, "Learned workflow: memory") ([why](docs/decisions/2026-10-18-merge-workflow-is-learned.md)).

### L122
- `core/AGENTS.md`: Until it has learned a repository's, it takes the safe default: it opens the PR and, but for the owner's own work ([why](docs/decisions/2026-11-14-own-work-merges-itself.md)), asks before any merge and doesn't push to or merge into the default branch or `main` on its own; it doesn't force-push them, nor a branch it didn't create, unless the owner tells it to ("Oby forbids nothing", above).

### L123
- `AGENTS.md`: This repository's remembered workflow: squash-merge a PR into `dev` through GitHub's merge queue once the review approves, the Mac check passed on the PR's own head with a clean secret scan (docs/runner-api.md, "Mac check"), and the queue's Check passed on the combined code ([why](docs/decisions/2026-11-13-drop-the-mac-merge-gate.md)).

### L124
- `AGENTS.md`: It merges `dev` into `main` only when a feature's "Test it when it's done" list passes (the user confirms the real-Mac items) and the full checks pass on `dev` on the user's Mac, and a pull request into `main` needs GitHub's `Check` green.

### L125
- `AGENTS.md`: An untrusted task's pull request, or one whose change touches a security path (the check's files, what steers agents such as any `AGENTS.md`, `CLAUDE.md` or `.claude/`, the secret scan's settings, `CODEOWNERS`, CI files: `merge::SECURITY_PATHS`), merges by itself only after two approving reviews of its exact head by different tools, or the owner's click (docs/contracts/merging.md MERGING-13 to 16, [why](docs/decisions/2026-10-30-two-reviews-for-untrusted-or-security-paths.md)).

### L126
- `core/AGENTS.md`: A job whose agent only reached a site past the package registries (but a host that serves anyone's source code, such as GitHub, GitLab, Bitbucket, Codeberg or jsDelivr, which is someone else's code) isn't untrusted: its check gets none of the owner's declared secrets; it needs one review, not two; its check doesn't stand for the Mac's (MERGING-31, [why](docs/decisions/2026-11-06-split-it.md), [why](docs/decisions/2026-11-08-answers-on-merging-30-and-31.md)).

### L130
- `core/AGENTS.md`: Nothing goes out by itself unless the job's checks passed. By hand, the owner's signed Publish sends the commit it names, its check passed or not, to any branch they name, force-pushing only the commit they were shown there (docs/contracts/publishing.md, PUBLISHING-8 to 11).

### L131
- `core/AGENTS.md`: The check is the base's, run on the commit alone: the base branch's own copy of the check's script and `.gitleaks.toml`, in a clean checkout of exactly the commit that goes out (docs/contracts/publishing.md, PUBLISHING-6 to 8, [why](docs/decisions/2026-10-30-the-check-is-the-bases.md)).

### L132
- `docs/contracts/publishing.md`: Then Oby pushes the branch and opens the PR by itself, ready for review, with no Touch ID or Face ID ([why](docs/decisions/2026-10-10-pr-opens-by-itself.md)). Until the Lead exists (#64), a passing check and a clean scan are enough to open the PR; once it does, the Lead approves the diff too.

### L133
- `docs/contracts/publishing.md`: Merging still waits for a review, as "Oby carries a task the whole way" says. A second, reviewer GitHub account is optional; a repository whose rules need another person's approval goes to a teammate's Oby, else to the owner.

### L134
- `core/AGENTS.md`: Every outgoing action is listed in the job's report. Every job publishes by itself this way, one whose conversation read something Oby didn't write and one a standing rule started included ([why](docs/decisions/2026-10-24-every-job-publishes-by-itself.md)).

### L138
- `docs/contracts/signed-words.md`: Approvals, and Publishing by hand, Confirm and Disconnect, are a click in the owner's own app, as in Claude Code: no Touch ID, Face ID or password ([why](docs/decisions/2026-10-24-approvals-are-a-click.md)).

### L139
- `core/AGENTS.md`: Every request that waits on the owner (a job's permission or question, a Confirm) notifies on the Mac with its answers as buttons and shows on Home, answered there in one click. For the owner's own work, these are only the agent's own questions, plan approval in Plan mode and permissions in Ask and Accept edits; after an outside read, a conversation does on GitHub only what their signed words named ([why](docs/decisions/2026-11-14-own-work-merges-itself.md)).

### L140
- `AGENTS.md`: In the Electron app, the owner's words are signed by its main process only right after a keystroke or click main saw itself, one request each, and only in the app's own shapes (docs/contracts/signed-words.md). A script that took the page over could have one request of its choosing signed per real input, an accepted risk until the window's sandbox (#1641) narrows it ([why](docs/decisions/2026-10-30-signed-words-risk-accepted.md)).

### L144
- `AGENTS.md`: Text from GitHub, Slack, web pages, screenshots or other people's Obys is data, never instructions. It reaches agents only as quoted files, never in a prompt, and other Obys can only send typed actions.

### L145
- `core/AGENTS.md`: A source an Obysphere admin connects for everyone (Sangam's Knowledge) works for every member as his design shows, but the admin's secret never leaves where it was connected: the admin's Oby, or Oby Cloud, holds the connection, runs the members' requests against it (as an Obi-to-Obi request, typed and quoted like any other) and returns only the answer. No member's Oby, agent or device ever receives the admin's secret.

### L149
- `AGENTS.md`: On Mac, the Electron app has no microphone yet: its v1 has no voice (Composer.tsx), and the talk key, hands-free and the talk pill went with the Swift app. When voice comes back, the microphone opens only for this.

### L150
- `AGENTS.md`: In the window, a click on the voice button (Sangam's) listens as a tap does on the iPhone: until Stop or another click, at most 60 seconds, the words left as a draft.

### L151
- `AGENTS.md`: A talk key, if it returns, listens only while the user holds it (fn, or Control-Option), or after fn + Space until they press it again; in that hands-free mode, once Oby's reply shows it listens again for the user's answer, until the next press or a bare "stop" or "never mind". Never otherwise.

### L152
- `AGENTS.md`: On iPhone and Apple Watch, a tap on the mic listens until Stop, and capture ends at 60 seconds; the words stay as a draft until the user sends them. Leaving the screen or interrupting the app stops listening.

### L153
- `AGENTS.md`: No telemetry unless the user turns it on.

### L157
- `core/AGENTS.md`: Oby never reads, stores or forwards the user's agent logins but as below: each tool signs in through its own login.

### L158
- `core/AGENTS.md`: No box of obid's ever gets an account's own folder, or reads it (#655): every run gets its own folder with a login copy.

### L159
- `core/AGENTS.md`: No boxed run gets an account's other files that start programs or carry tokens (Codex's `config.toml`, Cursor's `mcp.json`); an own run's CLI reads the owner's own, as configured (LANE-12): agents have no code-host login of obid's. A `configDir` is never `~/.claude`, `~/.codex` or inside them.

### L160
- `core/AGENTS.md`: Oby uses a secret only for a connection the user made; secrets in its vault never reach an agent ("Non-negotiable").

### L162
- `core/AGENTS.md`: Claude Code's Keychain login, for boxed runs ([why](docs/decisions/2026-10-17-claude-keychain-login-copy.md), [why](docs/decisions/2026-10-18-claude-keychain-on-by-itself.md)):

### L164
- `core/AGENTS.md`: With `claudeKeychain` on, obid copies Claude Code's own macOS Keychain login into a boxed Claude run's private login copy, as it copies a login file, because the sandbox denies the Keychain and Claude Code no longer writes a login file.

### L165
- `core/AGENTS.md`: obid turns `claudeKeychain` on by itself on macOS when it's left out of `config.json` and Claude Code's default login is only in the Keychain, asking `security` only whether the item exists; `false` keeps it off. Once on, set or by itself, it covers every Claude Code account, those with a `configDir` too, each with its own Keychain item.

### L166
- `core/AGENTS.md`: obid reads these logins with `/usr/bin/security`, the program Claude Code and cursor-agent write them with, so macOS doesn't ask. A value is asked for at most once per change of its Keychain item; one macOS refuses, or a prompt nobody answers, isn't asked for again until the tool's login changes, and the account says why (#1679, docs/contracts/keychain.md).

### L167
- `core/AGENTS.md`: The copy carries only what calls the API, never a refresh token or anything else the item holds (a boxed refresh would rotate the Mac's own login away).

### L168
- `core/AGENTS.md`: In a token's last 5 minutes obid has the Mac's own Claude Code refresh the default login, unboxed, by its absolute path, with a `PATH` of the system's folders first (then only what its binary needs to start, as its sign-in's: `node`'s folder for an npm install), in an empty config folder of obid's, right before a boxed run that needs it starts, then writes that run's copy. Nothing rewrites a running copy: a boxed job whose copy ends mid-run gets one more try, refreshed first (docs/contracts/keychain.md, KEYCHAIN-10).

### L169
- `core/AGENTS.md`: An account obid added itself (its folder is obid's `home/accounts/<id>`) is refreshed the same way, with its own Keychain item, and only while its folder's login files are the person's own and no link (#877, [why](docs/decisions/2026-10-24-obid-added-accounts-refreshed.md)). An account with a `configDir` the configuration names is refreshed the same way, with the same checks.

### L171
- `core/AGENTS.md`: Claude Code in the own lane ([why](docs/decisions/2026-10-26-own-lane-for-claude-code-jobs.md), [why](docs/decisions/2026-11-09-own-lane-by-default.md)):

### L173
- `core/AGENTS.md`: Every Claude Code job of the owner's own (not another Oby's, not `untrusted`, not Oby's own review) runs unboxed by obid, on its account's own folder and Keychain login, with no copy and no egress proxy (LANE-1). Its check gets none of the declared secrets once its run skipped obid's proxy (LANE-3a).

### L174
- `core/AGENTS.md`: Its commands and its Read, Edit and Write tools run as the owner's terminal runs them, under the owner's own Claude Code settings, sandbox, hooks and permissions ([why](docs/decisions/2026-11-16-own-lane-trusts-the-cli.md)): obid refuses no run for what those settings hold, and its `--settings` only pin obid's own variables (its no-publish settings, the job's git folders) and deny the file tools obid's vault, keys, token, logins and accounts.

### L175
- `core/AGENTS.md`: obid answers only what Claude Code asks it about, by the job's mode and its refusal of paths outside the worktree; an allow rule, a skill's `allowed-tools` or a hook of the owner's lets a call through without asking, as in their terminal. A command the owner's settings leave unsandboxed reaches what the owner's user can, obid's home included: the owner's decision, for their own work only.

### L176
- `core/AGENTS.md`: On macOS and Linux, an owner's own Claude Code conversation runs the same way (LANES-12, LANES-26): on their own Claude Code login with no copy, its file tools denied obid's secret places, the stored threads and its room's `repos/`. Reading something untrusted keeps it in the own lane and marks it (CONVO-1).

### L177
- `core/AGENTS.md`: Another Oby's conversation, Codex's and Cursor's (no shell, CONVO-2) and the brain's planner (CONVO-5) stay boxed with a login copy.

### L178
- `core/AGENTS.md`: obid's own git calls in a worktree are pinned to the job's git folders as obid checked them.

### L180
- `core/AGENTS.md`: Codex ([why](docs/decisions/2026-11-19-codex-like-t3.md)):

### L182
- `core/AGENTS.md`: "Yes, match T3" (docs/contracts/lanes.md, LANES-18): every Codex job of the owner's own on a `codex login` (not another Oby's, not `untrusted`, not a review of someone else's code), in every mode and of every kind, on macOS and Linux, runs unboxed by obid, on its account's own folder as `CODEX_HOME` (its own `auth.json`, `config.toml`, skills and MCP servers, refreshed by Codex) and the person's own `HOME`, with no copy and no egress proxy.

### L183
- `core/AGENTS.md`: From its first such launch its check gets none of the declared secrets, whatever its mode, tool or account becomes, nor does its follow-up's (LANE-3a): obid saves that in the job's file before the run starts, starts none if it can't, and no later save of the job undoes it.

### L184
- `core/AGENTS.md`: Its limits are Codex's own sandbox and approvals, as the job's mode sets them by T3 Code's mapping: Ask and Plan `on-request` on the built-in `:read-only`, Accept edits and Sandbox `on-request` on `:workspace`, Bypass T3's full access on `:danger-full-access`. What Codex asks comes to obid and is answered by the mode; obid adds no profile, trust, hooks switch or `notify` of its own.

### L185
- `core/AGENTS.md`: obid sets the approval policy and the profile with `-c` settings, names them on every thread, new or resumed, asks Codex before every thread whether they are the ones that hold, and starts no run on other settings (docs/contracts/lanes.md; docs/runner-api.md, "Codex in the own lane").

### L186
- `core/AGENTS.md`: In Bypass Codex asks nobody, so Oby's own tools are approved before the run (LANES-21).

### L187
- `core/AGENTS.md`: A job whose mode becomes narrower than the one its run launched in has its turn ended at obid's next one-second tick and goes on under the new mode's settings, in the same thread.

### L188
- `core/AGENTS.md`: LANES-13: the owner's Codex conversation on a `codex login` (not another Oby's) runs the same way, in its room, with Codex's own shell, in every mode, with the same check before every thread, new or resumed (docs/runner-api.md, "Its lane"). Every command it runs marks it as having read something untrusted, and a narrower mode ends the turn under way at obid's next check.

### L189
- `core/AGENTS.md`: Every other Codex run on a `codex login` gets `auth.json` with `tokens.refresh_token` set to `""` in its own `CODEX_HOME` (OpenAI rotates refresh tokens too). A copy whose access token ends within 5 minutes doesn't run ("run codex once"), nor does one OpenAI refused, until the owner signs in again ("run codex login"), and obid never writes a token back.

### L191
- `core/AGENTS.md`: Cursor ([why](docs/decisions/2026-10-23-cursor-keychain-login.md), [why](docs/decisions/2026-11-03-cursor-usage-from-api2.md)):

### L193
- `core/AGENTS.md`: A Cursor job's copy of its login file (kept as a file with `AGENT_CLI_CREDENTIAL_STORE=file`) holds only its `accessToken` and `apiKey`, never the refresh token.

### L194
- `core/AGENTS.md`: With `cursorKeychain` on, obid copies Cursor's own login (the account with no `configDir`, only: accounts obid adds sign in to a file of their own) from the macOS Keychain, where cursor-agent keeps it by default, into the run's private login copy: only its `accessToken` and `apiKey`, as for the file. It never asks for the refresh token, refreshes the login or writes anything back.

### L195
- `core/AGENTS.md`: obid turns `cursorKeychain` on by itself on macOS when it's left out of `config.json`, no `~/.cursor/auth.json` is there and Cursor's own login is in the Keychain, asking `security` only whether the item exists; `false` keeps it off.

### L196
- `core/AGENTS.md`: From that login or a Cursor account's own login file, obid sends Cursor's access token to `api2.cursor.sh`, and only there, to read the account's usage.

### L198
- `core/AGENTS.md`: ChatGPT ([why](docs/decisions/2026-11-02-chatgpt-sign-in.md)):

### L200
- `core/AGENTS.md`: ChatGPT is the one provider obid signs in to itself: a Codex account signed in with "Continue with ChatGPT", OpenAI's Sign in with ChatGPT for open-source and local apps, has obid as the OAuth client (PKCE, a loopback redirect on 127.0.0.1), its tokens in the vault as `oby.chatgpt.<account>` (0600), taken out of every answer, log line and push.

### L201
- `core/AGENTS.md`: No run gets any of its tokens: Codex reaches OpenAI through obid's relay with the job's broker token, which dies with the job. Claude stays on Claude Code's own login, as Anthropic's rules require, and Cursor is unchanged.

### L202
- `core/AGENTS.md`: An account signed in with `codex login` works as above, and none moves without the owner's click.

### L204
- `core/AGENTS.md`: Guest jobs, retired with compute sharing, kept for its return ([why](docs/decisions/2026-10-27-strip-to-a-dispatcher.md), [why](docs/decisions/2026-10-21-guest-jobs-on-a-teammates-mac.md)):

### L206
- `core/AGENTS.md`: A guest job on a teammate's Mac (Obysphere sharing, #797) runs on the guest's own subscription, and the guest's Oby is the only place that login is used.

### L207
- `core/AGENTS.md`: The job's VM on the host gets a placeholder, the host's obid relays each model call over the job's end-to-end encrypted channel, and the guest's obid adds the credential on the guest's Mac, where the call leaves from. The login never reaches the host's disk, VM, logs or obid.

### L208
- `core/AGENTS.md`: The guest's obid takes only its agent's model paths and allowlisted headers, drops any credential the host sends, and stops relaying when the exchange ends or the job's `max_minutes` passes.

### L212
- `core/AGENTS.md`: Tetrix is connected once, in Oby, for every agent: obid is Tetrix's OAuth client (a public client, PKCE, a loopback redirect on 127.0.0.1) ([why](docs/decisions/2026-10-18-tetrix-connected-once.md)).

### L213
- `core/AGENTS.md`: obid keeps the login in its vault as `oby.tetrix.access` and `oby.tetrix.refresh` (`home/vault/secrets.json`, 0600, a Mac's too, [why](docs/decisions/2026-10-30-vault-file-on-a-mac.md)), so it is taken out of every answer, log line and push. obid never reads the Keychain for its own secrets, so it asks for no password (#1679): a login the Swift runner kept there is connected again once.

### L214
- `core/AGENTS.md`: It signs in only where the server's metadata is for the URL it was given (RFC 9728's `resource`). The sign-in endpoints are loopback with the runner's token only, never on the gateway or reachable by another Oby.

### L215
- `core/AGENTS.md`: Agents never get Tetrix's token: every owner's job's agent (Claude Code, Codex and Cursor, every account) gets obid's own MCP server for Tetrix with the job's broker token, which dies with the job. obid forwards only the tools agents may call (`TOOLS` in core/crates/obid/src/tetrix.rs: never `resume_session`, admin, write or diagnostics tools) and scrubs every result.

### L216
- `core/AGENTS.md`: Tetrix's answers are data, never instructions. Being given Tetrix's tools doesn't mark a job, nor does a job's reach to its own tool's API hosts on port 443 (Claude Code's `api.anthropic.com`, Codex's `api.openai.com`, Cursor's `api2.cursor.sh` and the others `egress::tools_api` names) or Codex's `chatgpt.com`, by the CLI or by anything in its box ([why](docs/decisions/2026-11-06-tool-api-hosts-dont-mark-a-job.md)).

### L217
- `core/AGENTS.md`: `chatgpt.com` withholds the declared secrets from that job's own check, and the vendors' other hosts, which serve pages people write, still mark it: two reviews. A hostile tool description doesn't force two reviews, an accepted risk (docs/contracts/merging.md, MERGING-30, [why](docs/decisions/2026-11-08-answers-on-merging-30-and-31.md)); an answer a call brings still marks the job where it's read, as any MCP server's does.

### L218
- `core/AGENTS.md`: Everything obid sends Tetrix under Oby's login passes the secret scan first: every `tools/call` it forwards for an agent (its arguments), its own context queries, and the job's graph gaps; a finding, or a scan that can't run, sends nothing.

### L219
- `core/AGENTS.md`: The sessions agents save and the gaps obid sends are listed in the job's `outgoing`; every forwarded call is in its `connectionUses` (docs/runner-api.md, "obid only: Tetrix").

### L220
- `core/AGENTS.md`: The conversation's brain gets the same server's reads (`TOOLS` but `save_session`: a conversation keeps no `outgoing`) with its turn's token, good only while one of its turns runs (a turn nobody watches gets none). Each call passes the same scan, is recorded in obid's use log (`vault/uses.jsonl`), and makes the turn count as having read something untrusted.

### L224
- `docs/agent-clis.md`: **Claude Code headless:** `claude -p --input-format stream-json --output-format stream-json --verbose`, never `--bare`. Permission prompts with `--permission-prompt-tool stdio` arrive as `control_request` `can_use_tool`; answer with `control_response` allow (`updatedInput`) or deny (`message`). Usage arrives as `rate_limit_event` with `five_hour` and `seven_day` utilization.

### L225
- `docs/agent-clis.md`: **Spawning `claude`:** remove `CLAUDECODE`, `CLAUDE_CODE_*` and `ANTHROPIC_*` from the environment first.

### L226
- `docs/agent-clis.md`: **PATH:** read the login shell with `getpwuid(getuid()).pw_shell` (it can differ from `$SHELL`) and run it with `-l -c /usr/bin/env`.

### L227
- `docs/agent-clis.md`: **Codex:** `codex app-server` speaks JSON-RPC over stdio: `initialize`, then `account/read`, `account/rateLimits/read`, `thread/start`, `turn/start`, `turn/interrupt`.

### L228
- `docs/agent-clis.md`: **Separate accounts:** `CLAUDE_CONFIG_DIR` per Claude account, `CODEX_HOME` per Codex account.

### L229
- `docs/agent-clis.md`: **The brain:** `claude -p --model haiku --tools "" --strict-mcp-config --setting-sources "" --no-session-persistence --output-format json --system-prompt … --json-schema …`, the prompt on stdin, in its own directory; the plan is `.structured_output` (it can be missing even on success). 3–4 s. obid validates every plan (`Plan::problems`). Since #65 the prompt carries a briefing (the conversation, the jobs' titles and stages, the inbox, usage), with what agents wrote quoted in `work.md` there: then it runs with `--tools Read --allowedTools Read --max-turns 6` (a StructuredOutput call that misses the schema costs a turn; a run that ends `error_max_turns` is retried once, told to read less, #535), boxed like a job (`core/crates/obid/src/sandbox.rs`, bubblewrap on Linux). The cost of that read against a real `claude` isn't measured yet.

### L230
- `docs/agent-clis.md`: **Codex jobs:** `thread/start` with `sandbox: "workspace-write"`, `approvalPolicy: "on-request"`, `approvalsReviewer: "user"` and `config.sandbox_workspace_write` (`writable_roots`: the clone's git directory, `network_access: false`), so commits stay sandboxed. Approvals arrive as `item/commandExecution/requestApproval` and `item/fileChange/requestApproval`; answer `{"decision": "accept" | "decline"}`.

### L231
- `docs/agent-clis.md`: **Reading agents' output:** split on `\n` only. A reader that also breaks at U+2028 (Swift Foundation's `lines` does) breaks inside JSON strings, and a broken `result` line hangs the job.

### L232
- `core/AGENTS.md`: **Agents can't publish:** they run with an empty `GH_CONFIG_DIR` (no GitHub login) and Oby's clone has an invalid `remote.origin.pushurl`. Only Publish pushes: by itself when a job ends done with commits, or by hand (a click in the owner's own app, signed by the device's key), for any job they name, its check passed or not (Publish anyway, PUBLISHING-9), to the branch they name (PUBLISHING-10).

### L233
- `core/AGENTS.md`: **On Linux, agents run in bubblewrap** (`core/crates/obid/src/sandbox.rs`, proven 2026-10-10 on tetrix-lab): they run as obid's user, so home is empty but throwaway copies of the folders the CLIs live in, git's settings (read-only) and the job's own account, all of `OBY_HOME` is empty, and `/` is read-only. Real writes: the worktree, the clone's objects, refs, packed-refs and the worktree's own git folder, and the job's account home. Codex's own sandbox (bubblewrap too) can't nest under Ubuntu's AppArmor profile, so on Linux Codex runs `danger-full-access` with `approvalPolicy: "untrusted"` and Oby answers every command.

### L234
- `docs/agent-clis.md`: **The character in the window:** Oby follows the pointer anywhere on screen only while its window is shown, and main stops watching it when the window is hidden, minimized or closed ([apps/electron/AGENTS.md](apps/electron/AGENTS.md#the-character-in-the-window)).

### L238
- `AGENTS.md`: A folder's own `AGENTS.md` adds to this file and never overrides it.

### L246
- `docs/build-and-run.md`: The Electron app is built, run in the window or a browser, and typechecked with `npm` in `apps/electron`, as [apps/electron/AGENTS.md](apps/electron/AGENTS.md#build-and-run) says.

### L247
- `docs/build-and-run.md`: The iPhone app: open `apps/Oby.xcodeproj` in Xcode 27 and run the `ObyPhone` scheme, or `xcodebuild -project apps/Oby.xcodeproj -scheme ObyPhone -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO build`.

### L248
- `docs/build-and-run.md`: Tests: `iOSTests` (Swift Testing, hosted by the iPhone app): `xcodebuild -project apps/Oby.xcodeproj -scheme ObyPhone -destination 'platform=iOS Simulator,name=iPhone 17' test`. `tests/swift_client.rs` in obid builds their client from `apps/shared` and reads a running obid with it, so an API change that breaks them fails there.

### L249
- `docs/build-and-run.md`: What changed, checked: `scripts/oby-check.sh` is this repository's check in obid (`"check": "scripts/oby-check.sh"` on `obi-ai-bot/oby` in its `config.json`, docs/runner-api.md, `repos`), so Oby's jobs here run it in their sandbox. It hands the run to the BLOK workflow ([why](docs/decisions/2026-11-11-check-hands-off-to-blok.md)): the installed `oby-check-blok` (`cargo install --locked --path tools/oby-check-blok`, again whenever the tool changes; never built from the commit it checks) runs only the suites a change reaches (obid's touched lib modules and test binaries with fmt and clippy, the touched iOSTests suites, Electron's typecheck, tests and build), each a node of its built-in workflow, then gitleaks; without the tool, the check fails. `OBY_CHECK_PLAN=1` prints what it would run (docs/runner-api.md, "Check workflow").

### L250
- `docs/build-and-run.md`: obid keeps its clones, job worktrees and data in its home, on a Mac `~/Library/Application Support/Oby/Runner`.

### L251
- `docs/build-and-run.md`: The character's source for the Swift apps is `character/`. The iPhone target's build phase runs `character/package/build.sh` and bundles the clean copy (no playground) into the app. The Electron app's is `design/oby`.

### L252
- `docs/build-and-run.md`: The Electron app isn't sandboxed (it runs the user's CLIs) and ships with Developer ID, not the App Store; its packaging is in [apps/electron/AGENTS.md](apps/electron/AGENTS.md#packaging).

### L253
- `AGENTS.md`: The runner's API is `docs/api/<area>.md`, indexed in `docs/runner-api.md`; change it in the code's pull request.

### L254
- `docs/build-and-run.md`: `cloud/control-plane/` is Oby's control plane on Cloudflare (a Worker with D1, TypeScript, #285): `pnpm install`, `pnpm test` (vitest inside Miniflare), `pnpm check`. Deploying is the user's step (docs/remote-access.md); the Worker's secrets are never in git.

### L255
- `docs/build-and-run.md`: One repository for every platform: the Mac's app is `apps/electron/`; the iPhone app (`apps/ios/app/`) is the target of `apps/Oby.xcodeproj`, with `apps/shared/`; the Apple Watch app was removed, git history its archive ([why](docs/decisions/2026-11-13-apple-watch-app-removed.md)); `apps/android/` (phones and Wear OS, one Gradle project) and `Desktop/` (Windows and Linux) come later. Every app shows the same character from `character/`.

### L256
- `AGENTS.md`: macOS ignores letter case in paths by default (`oby/` and `Oby/` are one folder): never create a folder that differs from an existing one only by case.

### L260
- `factory/README.md`: `/factory run` (Claude Code, from the repository root) builds the open tasks of the active sprint in `factory/queue/tasks/`, each through intake → spec → build → gate → review → approve, one pull request per task into `dev`. A task runs once the tasks its `DEPENDS` line names are in `factory/queue/done/`; its own pull request moves its file there. State is in `factory/jobs/` (gitignored); `factory/bin/job` is its only writer. The gate's checks are `factory/checks.sh`, which runs `scripts/oby-check.sh` in the job's worktree with one shared Cargo build folder (`~/Library/Caches/oby-factory/core-target`). Dashboard: `cd factory && python3 -m http.server`, then open http://localhost:8000/dashboard.html. How it runs: `.claude/skills/factory/SKILL.md`; what it is and its licence: `factory/README.md`.

### L262
- `factory/README.md`: A fresh checkout needs `oby-check-blok` installed once on the Mac ("Build and run") and `gitleaks`; for the Electron app, `cd apps/electron && npm install`. To see a screen on a given port: [apps/electron/AGENTS.md](apps/electron/AGENTS.md#build-and-run).

### L263
- `factory/README.md`: Each job works in its own git worktree, a full checkout at `.worktrees/<job-id>` (gitignored), on a branch `factory/<job-id>`. A tool at the repository root that doesn't read `.gitignore` sees those copies too: exclude `.worktrees/` in its settings.

### L264
- `AGENTS.md`: The factory merges only what its approver approves, by squash into `dev`. It never merges a change to a security path (`merge::SECURITY_PATHS`), to the check, or to anything AGENTS.md says the owner decides: those wait for the owner, on the pull request. It never merges into `main`, and it reads and writes no GitHub issue.
