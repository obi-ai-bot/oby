# The `/` menu, factory settings, and the workflow gaps

A plan, not built, for the owner and Sangam to decide on. Sangam owns the UI: where his screens or words differ, he leads. It covers three connected pieces.

1. **The `/` menu.** Typing `/` in the message box opens one menu for the factory, your agents, your agent CLIs' skills, routines and Oby's own commands.
2. **Factory settings.** For each step of a project's factory, which account and model runs it. It is one JSON file, edited visually or as text.
3. **The workflow gaps.** What Oby still lacks for different people, checked against `origin/dev` at `76ec3518`.

Think of `/` as the box's shortcut drawer, and of factory settings as the duty roster on the factory's wall. The roster says who works each station, and the drawer is how you call any of them without leaving the conversation.

Everything here keeps the approved Home layout: Sangam's mini sidebar, the profile as the panel's title, the Projects | Agents switch, agents as coloured Obys, and the "Auto" pill in the message box. The canvas is the owner-approved one, extended with a second row: <https://claude.ai/artifact/TZV6boSX9eS89U7ManLTH7> (screens 11 to 18). Screenshots are 1280 wide, with mock data, rendered with headless Chrome.

| Screen | What it shows |
|---|---|
| `11-slash.png` | `/` typed: the factory first, then agents, skills, routines and Oby's own commands, a few of each. |
| `12-slash-filter.png` | `/rev` typed: one list, best match first. Each row says where it comes from: Oby, Claude Code on Deskree Team, a routine, an agent. |
| `13-slash-factory.png` | `/factory` picked: Run a task (with the tasks that can start), What's running, Add a task, Pause, Settings. |
| `14-factory-settings.png` | Settings › Factory: presets with time and room, then each step under its stage, each with an account, a model and a hint. One picker is open. |
| `15-factory-json.png` | The same file as JSON. It is checked as you type, the problem is said on its line, and Save waits until the file is valid. |
| `16-project-settings.png` | A project's settings, as T3 Code does it: where conversations work (this folder, or a new worktree each), what finishing offers when there is no remote (Commit, then Publish repository), the factory on or off, and whether to ask before anything merges. |
| `17-waiting.png` | Waiting for you: everything across the profile's projects that needs your word, as one list with Merge, Allow and Answer. |
| `18-new-profile.png` | A new profile: a place of its own, with the accounts and code-host login it may use. |

Three screens, not more, answer the six personas. The rest of the gaps are obid work with nothing new to see.

---

## 1. Research: what T3 Code, OpenBot, Claude Code and Codex do

Paths are relative to each project's own root: T3 Code at `304cf149c8`, OpenBot `0.69.0`. OpenBot is PolyForm Noncommercial, so nothing here copies its code, only what it does.

### T3 Code

| Feature | Evidence | Oby has it? |
|---|---|---|
| A `/` menu in the composer. The word under the cursor picks the menu: `/` commands, `#` PRs, `$` skills, `@` mentions. | `apps/web/src/composer-logic.ts:258-300` | **Gap.** Oby has `@` for Obys (`apps/electron/src/renderer/src/Composer.tsx:62-66,143-153`) and the `+` panel (`Composer.tsx:476-488`), but no `/`. |
| Four built-in commands: `/model`, `/side`, `/btw`, `/clear`. | `apps/web/src/components/chat/useComposerMenuItems.ts:237-274` | Partial. Oby's talk pill is `/model`; New conversation is `/clear`. |
| The agent's own commands and skills in the same `/` menu (`/name`, `/skill:name`). A command is hidden when a skill has the same name. | `useComposerMenuItems.ts:275-312` | **Gap.** |
| Agent commands only at the start of the message. | `apps/web/src/composerSlashCommandSearch.ts:16-31` | Proposed the same (§2). |
| Ranking: exact, then prefix, word start, contains, fuzzy. The description counts less. Ties go built-in, then agent command, then skill. | `composerSlashCommandSearch.ts:33-117`; `packages/shared/src/searchRanking.ts:22,88-138` | Proposed the same. |
| Keyboard: ↑/↓ wrap, Enter or Tab picks, Esc closes. | `apps/web/src/components/chat/composerCommandKey.ts:241-266` | Oby's `@` list already works this way (`Composer.tsx:62-66`). |
| Claude commands come from the CLI itself (the SDK's `initializationResult().commands`), never by reading `~/.claude/commands`. | `apps/server/src/provider/ClaudeProvider.ts:536,600,393-415` | **Gap.** obid reads no init event (no `slash_commands` in `core/crates/obid/src`). |
| Claude skills are read from the skill folders. | `apps/server/src/provider/Drivers/ClaudeSkills.ts:721-780` | Gap. |
| Codex skills come from the app-server's `skills/list`. | `apps/server/src/provider/CodexProvider.ts:735,840,880` | **Gap, cheap.** obid already speaks `codex app-server` (`core/crates/obid/src/codex.rs:1,54-75`). |
| `showSkillsInSlashMenu` setting; skills with `userInvocable:false` are hidden. | `packages/contracts/src/settings.ts:528`; `packages/client-runtime/src/providerSkills.ts:40-62` | Gap (nice). |
| The contract carries commands and skills: `ServerProviderSlashCommand`, `ServerProviderSkill`, `ServerProviderWorkspaceSnapshot`, with a 5-minute snapshot TTL. | `packages/contracts/src/server.ts:103-143,274-278` | The model for `GET /v1/commands` (§2.4). |
| A `mod+k` palette: actions, threads, projects, settings, message hits. | `packages/shared/src/keybindings.ts:86`; `apps/web/src/components/CommandPalette.logic.ts:774-790` | Gap. Not proposed now: `/` plus the panel's search covers it (the lean rule). |
| Per-project scripts (`t3.json`) run in the terminal. | `packages/contracts/src/project.ts:35-47` | Gap. Not proposed: that is the agent's work (Dispatcher, not harness). |
| Queue or steer while a turn runs. | `packages/client-runtime/src/state/composerDispatch.ts:4-16` | Planned elsewhere (R05, steering). |
| Send to several models at once, one worktree each. | `useMultipleModelSend.ts:20-24` | Gap. Not proposed now. |

### OpenBot

| Feature | Evidence | Oby has it? |
|---|---|---|
| No `/` menu. The picker matches only `@` and `$`. | `packages/ui/src/features/conversation/ComposerEditor.tsx:703` | — |
| Text starting with `/` goes to the CLI verbatim, and OpenBot then adds none of its own context. | `src/backend/claude-client.ts:1640-1643`; `src/backend/acp-client.ts:2223-2225`; `src/backend/agent/turn-context.ts:73-78` | **Gap.** Proposed: a CLI skill or command picked in `/` is passed through as typed (§2.3). |
| Skills: `~/.agents/skills/<slug>/SKILL.md` plus each provider's folders, capped at 200 skills and 256 KB each. A skill in a folder the CLI doesn't read is flagged. | `src/backend/home-skills.ts:9,88-101`; `src/main/skill-folder-discovery.ts:22-55` | Gap. Oby doesn't host skills (§2.2). |
| One descriptor table per provider: skill folders, instruction files, steer support, usage reporting. | `packages/contracts/src/agent-providers.ts:134-200,292-473` | Planned as R08 (one interface for every agent CLI). Skills folders belong in that table. |
| `$` picks a skill. It becomes a chip and is sent as a hint; the CLI loads the skill itself. | `ComposerEditor.tsx:307-320,726-743`; `src/backend/agent/delivery-content.ts:479-497` | Proposed in `/` instead of a second key. |
| `@` lists agents, files, browser tabs and side threads, in titled sections. | `ComposerEditor.tsx:321-347,698-723` | Oby's `@` lists Obys only. |
| A Cmd+K global search with tabs: Agents, Channels, Messages, Files, Routines. | `packages/ui/src/components/GlobalSearch.tsx:45,98,699-726` | Gap; not proposed now. |
| Routines per agent: schedules, missed-run and limit policies, webhooks. | `packages/contracts/src/ipc-routines.ts:17-37,121-128` | Oby has routines (`docs/api/routines.md:65-75`). They are listed in `/` with Run now. |
| Workflows: `llm` and `condition` nodes each carry `provider`, `model` and `effort`. | `src/backend/workflows/workflow-graph.ts:30-32` | The closest thing to a model per factory step. **Gap** (§3). |
| Backup models: `{automatic, order[≤8 of {provider, model, accountId}]}`, used on a usage limit. | `packages/contracts/src/ipc-agents.ts:162-175` | Partial. Oby routes by room (`docs/api/jobs.md:256`). `whenFull` in §3 covers the factory. |
| Cost hints: only a "Free" badge and usage-limit marks. | `packages/contracts/src/agent-providers.ts:857-896` | Oby can do better: time and room from its own history (§3.4). |

### Claude Code and Codex, as the owner uses them

| Feature | Where | Oby has it? |
|---|---|---|
| Claude Code built-ins (`/clear`, `/compact`, `/model`, `/review`, `/init`, `/agents`, `/mcp`, `/memory`, `/permissions`). Many only make sense in its terminal. | Claude Code's own help | Oby lists only what works in a headless run (§2.2). |
| Claude Code skills: `.claude/skills/<name>/SKILL.md` in the project and the user's folder, run as `/name` or by the model's `Skill` tool. Older custom commands live in `.claude/commands/`. Plugins add `/plugin:name`. | This repository's own: `.claude/skills/{factory,ship,triage,plan-feature,human-drive}`. obid already lets an own run use `Skill` (`core/crates/obid/src/jobs/lanes.rs:177-183`). | **Gap:** Oby can't list or offer them. |
| Codex built-ins (`/model`, `/approvals`, `/review`, `/new`, `/init`, `/compact`, `/diff`, `/status`, `/mcp`). Custom prompts in `~/.codex/prompts/` as `/prompts:name`. Skills in `<CODEX_HOME>/skills`, including the bundled `.system` ones. | `docs/contracts/permissions.md:77` (PERMISSIONS-13 names `.codex/skills/.system/openai-docs`) | **Gap.** |
| Both CLIs pick a model and effort per session; neither has a per-step pipeline. | `GET /v1/settings/models/options` already lists each tool's models and efforts (`docs/api/agents.md:1-7`) | Oby has the options, but no per-step choice. |

### Sangam's own `@`

His design leads, so `/` has to fit around what he already drew.

| What | Evidence | What it means for `/` |
|---|---|---|
| `@` typed at the start of a word opens a list of the other Obys over the box. It narrows as you type: arrows move, Enter or Tab picks, Esc closes. The pick goes into the words as `@Name`, blue on a blue tint. | Mockup: `design/app/src/renderer/src/Composer.tsx:34-67,183`, `MessageEditor.tsx:19-29` (`AT = /(?:^|\s)@(\w*)$/`), `textTokens.ts:17-18`. v1: `apps/electron/src/renderer/src/Composer.tsx:62-66,143-153,317-334`. | `/` reuses this panel, its keys and its token look. It adds nothing new to learn. |
| The `+` panel: Add files or photos, Add other Obys (`@` hint), Attach tasks, Run on another Mac, Choose a repository, Select working folder. | `apps/electron/src/renderer/src/Composer.tsx:476-488` | Attaching stays in `+`; calling something goes in `/`. |
| `@` names Obys only: teammates' Obys, brought into a conversation to ask them or hand them work. Its list is `OBYS`, the people whose Obys you share an Obysphere with, matched by name prefix, then by contains. It names no agent, tool, factory or command. | Every one of Sangam's commits, read pre-squash: `4a7eae366` (the mockup: `obysFor`, `Composer.tsx:369-373`; `OBYS`, `mock.ts:434`), and `a037454fb`, `de3d6bce3`, `91ecb8fa1`, `605a31eda`, `c14f1bfe4`, `8fe65b4da`, `538495d8a` (v1, #1564: none touches `@`). Also checked: every branch, his Slack messages since October, the Oby design system. | So `@` is **between owners** (people's Obys) and `/` is **your own** (factory, agents, skills, routines). The two don't overlap. If Sangam planned more `@` commands, they aren't committed anywhere; they would have to come from him. |

**The one-line takeaway.** T3 Code shows that a single `/` list, fed by what each CLI says it has, works well. OpenBot shows that the CLI should run its own skills, with the app only offering them and passing the text through. No one has a model per factory step with honest time and cost hints. That part is Oby's own.

---

## 2. The `/` menu

### 2.1 What it lists, in this order

| Group | Its rows | What a pick does | Who owns it |
|---|---|---|---|
| **Factory** · the conversation's project | `/factory`, with Run a task (the tasks that can start, each `ready` or `waits for …`), What's running, Add a task, Pause / Resume, Settings | Run starts the task in the project's factory. What's running opens the factory beside the conversation (screen 5). Add turns the words after it into a task. Pause finishes what runs and starts nothing new. Settings opens screen 14. | obid (A00-73 gives a project its jobs) |
| **Agents** (the profile's helpers, A00-77) | `/<agent-name>`, with its coloured Oby | The words go to that agent's conversation, which opens. | obid |
| **Claude Code**, **Codex**, and each tool connected later, one title each, with its account | That tool's own `/<name>`: its skills (`skill · this project`) and the built-in commands that work without its terminal (`command`). A tool that isn't connected, or reports nothing, gets no title. | Inserts `/name ` and sends the words to that tool's CLI as typed. The CLI loads and runs the command or skill. | **The agent CLI.** obid only reports what the CLI says it has. |
| **Routines** | `/<routine-name>`, "Run it now", next run | `POST /v1/routines/{id}/run` (exists). | obid |
| **Oby** | `/plan` (plan first), `/review <pr>` (Oby's review, by two tools), `/every <when> <words>` (make a routine), `/model` (the talk pill), `/project <name>`, `/new`, `/usage`, `/settings` | Each is one of the app's existing actions. | obid / the app |

Projects are reached through `/project` rather than a group of their own: the project chip already switches projects, and lean wins.

### 2.2 Rules

- **It opens as T3 Code's does** (the owner's answer, 2026-11-15). A word that starts with `/` opens it anywhere in the message; a word with a second `/` or a `\` is a path and opens nothing (`packages/shared/src/composerTrigger.ts:50-62`). As the very first character, it offers everything. Mid-text it offers only what can sit inside a sentence: the agents and Oby's `/model`. The factory, the tools' commands and skills, routines and Oby's other commands need the start of the message, because the CLI reads a command only there (`composerSlashCommandSearch.ts:15-31`; `useComposerMenuItems.ts:308-311`).
- **It narrows as you type**, ranked exact, then prefix, word start, contains, fuzzy (T3's order). Matched letters are bold. Ties go Oby, then Factory, Agents, Skills, Routines. The empty menu shows at most three rows per group; typing reaches the rest.
- **Keyboard.** ↑/↓ wrap, Tab or ↵ picks, → opens a row's own list (like `/factory`), Esc closes and keeps the words. This is the `@` list's behaviour, kept.
- **The same name twice** (Oby's `/review` and Claude Code's `/review`): both rows show, Oby's first. Each row carries its source, and the token inserted names its tool, so nothing is ambiguous.
- **A skill belongs to its tool.** A Claude Code skill runs on Claude Code. When the talk pill is on another tool, the row says which account it will run on (`Deskree Team`), and that one turn goes there. Cursor reports no skills today, so it shows none.
- **Which CLI commands are listed** (the owner's answer: "create a title and put them under it, only show based on what is connected"). Under each connected tool's title: its skills, then its built-in commands that work in a headless run (Claude Code's `/review`, `/compact`; Codex's `/review`, `/init`). Never the terminal-only ones (`/login`, `/exit`, `/config`, `/vim`): obid filters them with a short list kept beside the provider's row (R08). A tool added later (Gemini, say) gets its own title the day it's connected.
- **Lanes.** Skills reach only own-lane runs, which already load the owner's own `.claude`/`CODEX_HOME` (`AGENTS.md`, "Claude Code in the own lane", CODEX-1). A boxed run gets no account files that start programs (`core/AGENTS.md`). So in a boxed conversation (another Oby's, untrusted, Cursor), skills rows are hidden, not offered and then refused.

### 2.3 Dispatcher, not harness

| Piece | Whose | Why |
|---|---|---|
| What a skill does, its text, running it | **The agent's** | Claude Code and Codex already run skills. Oby copying them would be a second harness. |
| Which skills exist for this account and folder | **The dispatcher's** (a cached report) | obid already starts these CLIs. It records what each said it has, and reads nothing it wouldn't otherwise read. |
| `/factory`, agents, routines, Oby's commands | **The dispatcher's** | These are obid's own things. |
| `/` text arriving from another Oby or from GitHub | **Between owners** | It is never a command. Quoted, as today: only the owner's own app's words open the menu or run its actions. |

### 2.4 What obid adds, and what the app only shows

**obid adds (add-only):**

- **`GET /v1/commands?conversation=<id>`**: everything `/` may list for that conversation, in one answer. The app filters and ranks it locally, since the list stays small: a few hundred at most, and skills are capped at 200 per tool as OpenBot caps them.

  ```json
  {
    "revision": 7,
    "commands": [
      {"id": "factory", "group": "factory", "name": "factory", "about": "Run a task, see what's running, add one",
       "badge": "2 running · 1 needs you", "project": "p_9c1", "children": true},
      {"id": "agent:h_31", "group": "agents", "name": "researcher", "about": "Hand these words to Researcher", "color": "#c3b5fd"},
      {"id": "skill:claude:deskree-team:ship", "group": "skills", "name": "ship", "about": "Ship a pull request into dev",
       "tool": "claude", "account": "deskree-team", "scope": "project", "argumentHint": "<pr>", "seenAt": "2026-11-15T18:02:11Z"},
      {"id": "routine:r_7", "group": "routines", "name": "nightly-check", "about": "Run it now", "next": "2026-11-16T02:00:00-05:00"},
      {"id": "oby:plan", "group": "oby", "name": "plan", "about": "Plan first, work once you approve"}
    ]
  }
  ```

  `group` is an open set: an app shows a group it doesn't know under "More", so a new group is never a breaking change.
- **`GET /v1/commands/factory?project=<id>`**: the factory's actions and the tasks that can start, each with `state` (`ready`, `waits`) and `waitsFor`.
- **`POST /v1/projects/{id}/factory/run`** `{task?, words?, clientRequestId}`, **`.../factory/pause`** and **`.../factory/resume`**. A run with neither `task` nor `words` takes the next ready task. Pause is what the owner does by hand today when the factory drains.
- **Skill reports from the CLI itself**, kept per `(tool, account, folder)` in the profile's home and refreshed at each session start:
  - Claude Code: the `system`/`init` event of its stream-json output. A spike confirms first that it lists commands and skills in `-p` mode; if it doesn't, obid falls back to reading `.claude/skills` and `.claude/commands` names and frontmatter in the project and the account's own folder, own-lane only.
  - Codex: `skills/list` on the `app-server` obid already runs.
  - Cursor: none until it reports any.
- **Passthrough.** Words starting with `/<skill>` reach that tool's CLI unchanged. obid adds no context block to that turn, as OpenBot does, so the CLI sees its own command.

**The app only shows:** the menu, its filtering and keyboard, and the token chip. Each pick calls an endpoint obid already has, or one of the above. A new shape goes in `src/shared/obid-wire.ts` (`apps/electron/AGENTS.md`, "Wire types").

---

## 3. Factory settings: a model per step

### 3.1 What there is today

The repository has two factories.

- **Oby's own factory.** `factory/bin/oby-feed` feeds task files to the factory's obid. It starts one `build` job per task on a single account (`--account`, default `deskree-team`; `factory/bin/oby-feed:259-268,530`). obid's own reviewer rule picks the reviews: another tool's account first, never Cursor (`docs/cursor.md:19`, with no reason given), a second review by a tool that hasn't approved (`core/crates/obid/src/jobs.rs:11576-11640`). The owner's answer (2026-11-15): every connected account may review, Cursor and the tools added later included (B00-07). No model is chosen per step.
- **The Claude Code factory** (`/factory run`). `factory/config.json` holds `routing` (risk → the code reviewer's model), `surface_reviewers` (ui, security, proofs, model → lenses and models), `proofs` (`blocking`, 600 s), `gate_shards`, `remote_gate` and `concurrency`. `factory/bin/route` turns intake's risk and surfaces into reviewers (`factory/bin/route:30-47`). Each agent's default model is its `.claude/agents/*.md` frontmatter: intake `fable`, spec-writer `opus`, builder `opus`, reviewer-code `haiku`, reviewer-ux `sonnet`, reviewer-ui `haiku`, reviewer-security `opus`, reviewer-proofs `opus`, approver `opus`.
- **What obid already offers.** A job takes `agent`, `account` and `model` (`docs/api/jobs.md:256-257`), and `effort` comes from the account (`docs/api/accounts.md:22`). `GET /v1/settings/models/options` lists each tool's models and efforts (`docs/api/agents.md:1-7`).

So the pieces exist. What is missing is one file that says, per step, who runs it, plus a screen over that file.

### 3.2 The file: `factory.json`, schema `oby.factory/1`

One file per project, in the profile's home: `profiles/<profile>/projects/<project>/factory.json`, mode 0600, written whole by rename. Steps follow the stages Sangam's factory view shows: Prepare (intake, spec), Work (build), Check (gate, prove), Review (the lenses, then the second review), Merge (approver).

```json
{
  "schema": "oby.factory/1",
  "preset": "careful",
  "steps": {
    "intake":  { "tool": "claude", "account": "deskree-team", "model": "claude-fable-5-1", "effort": "high" },
    "spec":    { "tool": "claude", "account": "deskree-team", "model": "claude-opus-5-5",  "effort": "high" },
    "build":   { "tool": "claude", "account": "deskree-team", "model": "claude-opus-5-5",  "effort": "xhigh",
                 "byRisk": { "low": { "model": "claude-sonnet-5-5" } } },
    "gate":    { "runsOn": "gate-vm", "shards": { "linux": 4, "mac": 2 } },
    "prove":   { "runsOn": "gate-vm", "mode": "blocking", "timeoutS": 600 },
    "review": {
      "code":     { "tool": "claude", "model": "claude-sonnet-5-5", "byRisk": { "low": { "model": "claude-haiku-5-5" } } },
      "ux":       { "when": ["ui"],       "tool": "claude", "model": "claude-sonnet-5-5" },
      "ui":       { "when": ["ui"],       "tool": "claude", "model": "claude-haiku-5-5" },
      "security": { "when": ["security"], "tool": "claude", "model": "claude-opus-5-5" },
      "proofs":   { "when": ["proofs", "model"], "tool": "claude", "model": "claude-opus-5-5" },
      "second":   { "tool": "codex", "account": "chatgpt", "model": "gpt-6-sol", "effort": "high" }
    },
    "approve": { "tool": "claude", "account": "deskree-team", "model": "claude-opus-5-5", "effort": "high" }
  },
  "whenFull": "wait",
  "concurrency": 60,
  "rounds": 3
}
```

- **Every model step** takes `tool` (`claude`, `codex`, `cursor`, `auto`), `account` (an account id in this profile, or `auto` for the one with the most room, USAGE-11), `model` (one of that tool's options, or `auto`), and `effort`. Left out, each falls back as today: the account's own model and effort, then the agent's frontmatter.
- **`byRisk`** overrides a step for `low`, `medium` or `high` risk. It replaces `routing`.
- **`when`** runs a lens only for those surfaces. It replaces `surface_reviewers`.
- **`gate` and `prove`** are machines, not models. They say where they run and how; `mode`, `timeoutS` and `shards` keep today's meaning.
- **`second`** is Oby's second review, by another tool. Two reviews by the same tool are allowed (Oby forbids nothing). The screen then says that a security path waits for the owner's click (MERGING-13 to 16), and that is all.
- **`whenFull`**: `wait` (the step waits for its account's room) or `any` (today's routing picks another signed-in account).
- **`preset`**: `fast`, `careful`, `cheapest` or `custom`. Any change to a step makes it `custom`.
- **The machines' own facts** stay where they are: `remote_gate`, `mac_worker`, `ports`, `prices_usd_per_mtok`, `protected_paths` and `sprint`. The JSON view shows only this file.

**The checks on save, each one a 422 with the JSON path, never a silent fix:**

- an unknown key or tool;
- an account that isn't in this profile, or isn't that tool's;
- a model that isn't in that tool's options for that account (screen 15 shows this one);
- an effort that model doesn't take;
- a `when` surface the factory doesn't know.

### 3.3 Presets

| Preset | Who runs what | Hint (this project's last 20 tasks) |
|---|---|---|
| **Fast** | Sonnet builds; Haiku reviews code and UI; Codex second at medium | ≈ 20 min · 6% of a week |
| **Careful** (today's) | Fable intake, Opus spec, build, security, proofs and approver; Sonnet code; Codex second at high | ≈ 45 min · 11% of a week |
| **Cheapest** | Haiku everywhere it can; Cursor Auto builds; the second review on the cheapest other tool (Cursor Auto, else Codex at low effort) | ≈ 30 min · 3% of a week |
| **Custom** | Whatever the steps say | — |

A preset changes who runs each step and how hard it thinks, **never a gate**: `gate`, `prove`, `protected_paths`, the two reviews a security path needs, and "ask before anything merges" stay as they are. This is what every product checked does (the research below). When the profile has only one tool connected, so no second tool can review, a security-path change waits for the owner's click, whatever the preset.

**How other platforms do it** (researched 2026-11-15 for the owner's question 10; vendor docs, dates where given):

| Product | Its cost or speed setting | Does a cheaper setting relax a merge gate? |
|---|---|---|
| GitHub Copilot code review | Effort Lite / Balanced / Max (Max uses several high-reasoning models) | No. Its reviews are comments by default and count as approvals only where an admin allows it, for paths an admin lists. |
| GitHub rulesets | Required approvals, code owners, required reviewers by path | No notion of model or cost at all. |
| Cursor (Auto, Bugbot) | Auto: Cost / Balance / Intelligence; Bugbot effort Low to Smart | No. Bugbot comments; blocking is a separate org setting. |
| Claude Code | `opus`, `sonnet`, `haiku`, `opusplan`; effort per session, subagent and skill | No. Effort never changes what asks permission. |
| Factory Droids | Model by complexity; autonomy Off to High | No. What needs approval follows the action's risk, not the model. |
| Amp | One dial, low to ultra | Speed and depth only. |
| CodeRabbit | Auto-approval within risk limits | Path overrides hold approval "regardless of risk"; a cheaper plan lacks a check rather than loosening one. |

Two models agreeing is something teams do by hand (Claude, Codex and Gemini each approving); no product sells it as a tier. One analysis warns that two models agreeing is correlated, not independent, which is why Oby keeps the checks and proofs as the gate and the second review as one more look. Sources: docs.github.com (Copilot code review, rulesets), cursor.com/docs (models, Bugbot), code.claude.com/docs (model config), docs.factory.ai (autonomy), ampcode.com/news/the-dial, docs.coderabbit.ai (automatic approvals).

### 3.4 Hints that are true

Each step shows its median minutes and its share of the account's week, from this project's last 20 jobs of that step. These are numbers obid already has: job times, and the account's `sevenDay` usage before and after (`docs/api/accounts.md:15`). Before there are 5 samples, the hint says "no history yet", never a guess. An account billed by the token (a key, not a plan) shows dollars from `prices_usd_per_mtok` instead of a share of the week. Gate and prove show "no account".

### 3.5 Who reads it

| Reader | How |
|---|---|
| **obid** | `GET /v1/projects/{id}/factory/settings` returns `{settings, revision, options, hints}`. `options` holds each tool's accounts in this profile, with models and efforts, from the existing models options. `PUT` with `{settings, revision}` returns `200 {revision}`, `409` on a stale revision, or `422 {problems:[{path, message}]}`. A factory job started for a step takes `tool`, `account`, `model` and `effort` from it. The reviewer rule takes `review.second` first when it is signed in and has room, else follows `whenFull`. |
| **oby-feed** | Reads the settings at each pass instead of `--account`, which stays as an override for one run. It passes `agent`, `account`, `model` and `effort` in `POST /v1/jobs`, and starts nothing while the factory is paused. |
| **The Claude Code factory** (`/factory run`) | `factory/config.json` gains the same `steps` block. `bin/route` reads `review.*` (`byRisk`, `when`) instead of `routing` and `surface_reviewers`, which stay readable while both exist. Each agent is spawned with `steps.<step>.model` over its frontmatter. `factory/config.json` is a protected path, so this change waits for the owner. |
| **The app** | Settings › Factory (screens 14 and 15): the Steps tab writes through `PUT`, and the JSON tab shows the same file and saves only when it's valid. |

Today's `config.json` maps onto `steps` as follows: `routing.low.code` → `review.code.byRisk.low.model`; `surface_reviewers.ui` → `review.ux`/`review.ui` with `when: ["ui"]`; `surface_reviewers.security` → `review.security`; `proofs` → `steps.prove`; `gate_shards` → `steps.gate.shards`; and each agent's frontmatter model → its step's `model`.

---

## 4. Workflow gaps, by persona

Read from `origin/dev` (`76ec3518`) and `docs/contracts/`. No code was run.

| Persona | Works today | Missing | How important | Planned? | Screen |
|---|---|---|---|---|---|
| **Solo developer, GitHub** | Sign in with gh's device flow (`docs/api/github.md`); one PR per job (`docs/contracts/publishing.md:8`); own work merges by itself into any base, `main` too (MERGING-44 to 49, `core/crates/obid/src/merge.rs:3985-4157`); a merged job frees its worktree (A00-69). | No `/` menu or skills (§2); no model per step (§3). A head the merge queue dropped isn't followed (A00-98). Only github.com, no GitHub Enterprise. `README.md` still says merging into main is "still being built". | Important | A00-98 | 11 to 15 |
| **Manager or CTO who only reviews** | AskStack on Home stacks what needs you (`apps/electron/src/renderer/src/AskStack.tsx`); a review request lands as `needs_you` (`docs/api/inbox.md:125`); a "Ready to merge" card (MERGING-17); the "Since you last looked" recap (`docs/api/routines.md:89`). | No single list across projects with Merge in it. No "ask me before anything merges": own work merges with no card (MERGING-45). iPhone approvals: the phone apps are frozen (`docs/decisions/2026-11-13-roadmap-answers.md`) and pairing needs an Obysphere (`SettingsView.tsx:285`). | Important; a **blocker** for approving on the phone | none | 16, 17 |
| **A team with several Obys** | Join by invite link, members, Leave (SPHERE-1 to 4, 15; A00-89); a teammate's Oby asked for a review (TEAMREVIEW-1, 2); ask an Oby, peer threads. | Create an Obysphere or invite from the app (SPHERE-14 stays on the mock). TEAMREVIEW-3 to 8 are proposed. One Obysphere per Oby (`core/crates/obid/src/sphere/membership.rs:1`). No compare-keys UI. | Important | A00-87; R10-01 to 04, R10-10 | none (Sangam's Obysphere screens) |
| **No GitHub, local folders** | Picking folders: obid checks them and keeps them on the conversation (`docs/contracts/folders.md:8`, `core/crates/obid/src/folders.rs`); a non-GitHub remote gets a push, no PR (`docs/api/configuration.md:25`). | A folder with no remote can't run its own job (`file://` only behind a test flag). No way to work in the folder itself, and no Commit / Publish repository finish (B00-11, as T3 Code does). No code-host adapter: publishing, merging and the inbox call gh directly, though `AGENTS.md` says GitHub is one adapter. | **Blocker** | none | 16 |
| **Conversations only, no factory** | Conversations on any connected account, with the talk pill (`docs/contracts/conversation-agent.md:8`). | No switch to keep a project's conversations from starting jobs, and no per-project setting. | Important | none (A00-73/74 add the factory, not an off switch) | 16 |
| **Several companies (profiles)** | List, create and rename profiles (PROFILES-1, 13 to 18); the panel's menu switches (A00-76). | **Profiles hold nothing yet** (`docs/contracts/profiles.md:13`): conversations, jobs, accounts, settings, permissions and the vault are the install's. Routines (A00-82), the Obysphere (A00-83) and alerts (A00-84) aren't per profile. One gh login per Mac. No delete. | **Blocker** | A00-79, 82, 83, 84 | 18 |
| **Codex-only or Cursor-only** | Conversations and jobs on either. | Inbox triage, memory and the recap use "the first Claude Code account" (`core/crates/obid/src/brain/calls.rs:1-5,57`; `core/crates/obid/src/triage.rs:461`). Oby's review never runs on Cursor (`docs/cursor.md:19`), so a Cursor-only owner's own work may never merge by itself (inferred, not tested). | Important | none | none |
| **Not English, or VoiceOver** | `aria-label`s on most controls. | No i18n: every string is English, in the app and in obid. R07-10 (stable codes) is the prerequisite. VoiceOver isn't audited. | Important (later) | R07-10 | none |

**The top 5 gaps:**

1. **Profiles separate nothing yet.** Someone with several companies sees all of them mixed. A00-79 is the fix; screen 18 is its sheet.
2. **No folder without a remote can run work.** Someone with no GitHub can't use the factory or get a PR, and there is no "branch in the folder" (screen 16). Not planned.
3. **No `/` and no per-step models.** The factory, agents and the CLIs' own skills are hard to reach, and every factory step runs on one account (§2, §3). Not planned.
4. **No list for whoever reviews, and no "ask before merging".** A CTO's approvals are mixed into the stack, and own work merges with no gate (screens 16, 17). The phone is frozen.
5. **No GitHub, no Claude Code, no English.** No code-host adapter; Codex-only or Cursor-only owners lose triage, memory, recap and possibly merging; no i18n.

Found in passing, for the owner rather than for these tasks:

- A00-75 is in `factory/queue/done/` but shipped only PROF-3a: `docs/contracts/profiles.md:13` still says a second profile holds nothing.
- A00-69 and A00-75 are in both `tasks/` and `done/` on `origin/dev`.
- A00-87 depends on A00-86, which is in neither folder.

---

## 5. The tasks this becomes (drafts)

These are drafts, not task files. The owner chose the prefix B00 on 2026-11-15. B00-01 is taken (the reader-race fix, PR #2384), so these start at B00-02: B00-02 to 05 are the `/` menu, B00-06 to 10 factory settings, and B00-11 to 15 the gaps. `factory/bin/job add` writes only A00 keys, so the task files are written by hand in the same header format. Each scenario is written to become a test named after it.

### B00-02 · feat(obid): `GET /v1/commands`, what `/` may list for a conversation

- **What.** Oby has no single place that says what can be called from the message box. obid answers one list: the project's factory, the profile's agents, its routines, Oby's commands, and the skills the conversation's CLIs reported.
- **Why.** Without it, each menu would hard-code its own lists and drift from obid, the way the mock already does.
- **How.** A new route, read-only, built from what obid holds (A00-73 projects, A00-77 helpers, routines, the skill reports of B00-03). `group` is an open set. The shape goes in `docs/api/commands.md` and `src/shared/obid-wire.ts`.
- **Scenarios:**
  - COMMANDS-1: a conversation in project P with 2 helpers, 1 routine and no skill reports lists `factory`, 2 `agents`, 1 `routines` and Oby's fixed commands, and no `skills`.
  - COMMANDS-2: a conversation with no project lists no `factory` row.
  - COMMANDS-3: a conversation boxed for another Oby or marked untrusted lists no `skills`.
  - COMMANDS-4: an unknown conversation answers `404`, and the route is not on the gateway's allowlist.

### B00-03 · feat(obid): each CLI's own skills and commands, reported and passed through

- **What.** obid records, per tool, account and folder, the skills and commands the CLI says it has. Words starting with one of them reach that CLI unchanged.
- **Why.** The skills are the owner's and the CLI's (Dispatcher, not harness). Oby should offer them, not copy them.
- **How.**
  - Spike first: does Claude Code's stream-json `system`/`init` list commands and skills in `-p`?
  - Codex uses `skills/list` on `codex app-server`.
  - Reports are kept in the profile's home, capped at 200 per tool, names and descriptions only, never a file's body.
  - Terminal-only commands are filtered by a list on the provider's row (R08).
- **Scenarios:**
  - COMMANDS-10: after a Codex own-lane turn in folder F, `GET /v1/commands` lists `openai-docs` with `tool: "codex"`, the account and `seenAt`.
  - COMMANDS-11: Claude Code's `/review` and `/compact` are listed under `tool: "claude"` with `kind: "command"`; `/login`, `/exit` and `/config` are never listed.
  - COMMANDS-15: with no Cursor account signed in, nothing is listed under `tool: "cursor"`.
  - COMMANDS-12: words `/ship 2341` to a Claude Code own-lane conversation reach its CLI byte-identical, with no context block added on that turn.
  - COMMANDS-13: a skill report never holds a `SKILL.md` body.
  - COMMANDS-14: a boxed run is never given the reported skills' folders.

### B00-04 · feat(electron): the `/` menu in the message box

- **What.** Typing `/` first in the box opens Sangam's panel over it (screens 11 to 13).
- **Why.** It is one key to reach the factory, agents, skills and routines without leaving the conversation.
- **How.** It reuses the `@` list's panel and keys (`Composer.tsx`). Ranking is local (exact, prefix, word start, contains, fuzzy). Each pick calls B00-02, B00-05 or an existing endpoint.
- **Scenarios:**
  - SLASH-1: `/` as the first character offers every group; `ask /res` mid-sentence offers only agents and `/model`; `src/a/b` opens nothing.
  - SLASH-2: `/rev` lists Oby's `/review` before Claude Code's `/review`, both with their source.
  - SLASH-3: ↑/↓ wrap, Tab and ↵ pick, Esc closes and keeps `/rev` in the box.
  - SLASH-4: picking a skill inserts `/ship ` and sends it on that tool's account.
  - SLASH-5: on a conversation with no project, no Factory group shows.
  - SLASH-6: with Claude Code and Codex connected, the menu has a `Claude Code` and a `Codex` title, each with its account; with Codex signed out, no `Codex` title.

### B00-05 · feat(obid): `/factory`: run, what's running, add, pause

- **What.** These are the factory's actions for a project, reached from `/factory`: run the next or a named ready task, open what's running, add a task from words, pause and resume.
- **Why.** Today the factory is driven from a terminal (`oby-feed`, `/factory run`). The owner wants it in the box.
- **How.** It needs A00-73. The routes are `POST /v1/projects/{id}/factory/run|pause|resume` and `GET /v1/commands/factory`. Pause is a flag obid keeps and oby-feed reads. "Add a task" writes into a queue obid keeps for the project, in the profile's home, never into the project's own files (the owner's answer: "done on obid to not mess with people's project"). A repository that keeps its own task files, as Oby's does, can still be read as the queue.
- **Scenarios:**
  - FACTORY-CMD-1: with tasks T1 (ready) and T2 (waits for T1), `GET /v1/commands/factory` lists T1 `ready` and T2 `waits` with `waitsFor: ["T1"]`.
  - FACTORY-CMD-2: `run` with no task starts T1 and answers its job.
  - FACTORY-CMD-3: `run` with T2 answers `409` naming T1.
  - FACTORY-CMD-4: after `pause`, `run` answers `409 paused`, running jobs go on, and oby-feed starts nothing.
  - FACTORY-CMD-5: a repeated `clientRequestId` answers the first job (R02).
  - FACTORY-CMD-6: `/factory add Fix the queue order` adds a task to obid's queue for the project, and `git status` in the project's folder is unchanged.

### B00-06 · feat(obid): a project's `factory.json` and `/v1/projects/{id}/factory/settings`

- **What.** One file per project says who runs each step (§3.2). obid reads it, checks it, and writes it whole.
- **Why.** The owner mixes Claude, Codex and Cursor accounts per step, and today every step runs on one account.
- **How.** The schema is `oby.factory/1`, and the API is as in §3.5. When the file is missing, the answer is the Careful preset built from today's `config.json` and frontmatter.
- **Scenarios:**
  - FACTORYSET-1: `GET` on a project with no file answers the Careful steps with `revision: 0`.
  - FACTORYSET-2: `PUT` with `review.second.model: "gpt-6"` on an account whose options lack it answers `422` with path `/steps/review/second/model`, and the file is byte-identical.
  - FACTORYSET-3: `PUT` with a stale `revision` answers `409`.
  - FACTORYSET-4: an account of another profile answers `422`.
  - FACTORYSET-5: the file is 0600 and replaced by rename.

### B00-07 · feat(obid): factory jobs and Oby's review follow the settings

- **What.** A step's job runs on its `tool`, `account`, `model` and `effort`, and the second review on `review.second`.
- **Why.** Choosing has no effect until the jobs honour it.
- **How.** The `POST /v1/jobs` fields already exist. `reviewer()` (`core/crates/obid/src/jobs.rs:11581`) takes `second` first, then applies `whenFull`. It drops "never Cursor": every connected account may review, and a tool added later reviews the day it's connected (the owner's answer). A spike first checks that a Cursor review gives a verdict reliably; until then, a Cursor review that gives none counts as no review, as today for any tool.
- **Scenarios:**
  - FACTORYSET-10: with `build` on Codex / ChatGPT / `gpt-6-sol`, a factory build job has those three.
  - FACTORYSET-11: with `second` on Codex and room, the second review runs on Codex.
  - FACTORYSET-12: with `second`'s account full and `whenFull: "wait"`, the review waits and says why. With `"any"`, today's rule picks.
  - FACTORYSET-13: with `second` on the same tool as the first review, a security-path PR waits for the owner's click (MERGING-14).
  - FACTORYSET-14: with only Claude Code and Cursor signed in, the second review of a Claude Code job runs on Cursor.

### B00-08 · feat(factory): oby-feed and the Claude Code factory read the steps

- **What.** `oby-feed` takes each job's tool, account and model from B00-06. `bin/route` and the agent spawn read `steps` from `factory/config.json`.
- **Why.** Both factories should follow one roster.
- **How.** `--account` stays as a one-run override. `routing` and `surface_reviewers` stay readable while both exist. This touches `factory/config.json`, a protected path: the owner merges.
- **Scenarios** (stdlib tests in `factory/tests`):
  - FEED-STEPS-1: with settings `build` on `codex`/`chatgpt`, oby-feed's `POST /v1/jobs` body carries them.
  - FEED-STEPS-2: with the factory paused, a pass starts nothing.
  - ROUTE-STEPS-1: `steps.review.code.byRisk.low.model: "haiku"` routes a low-risk job's code review to Haiku.
  - ROUTE-STEPS-2: no `steps` routes exactly as today.

### B00-09 · feat(electron): Settings › Factory, steps and JSON

- **What.** Screens 14 and 15: presets, steps under their stages with account, model and hint, and the file as JSON.
- **Why.** The owner asked for the JSON file changed visually, with the raw view for power users.
- **How.** It reads and writes B00-06. The pickers list `options`. JSON is checked by the same `PUT` with a dry-run flag before Save is enabled.
- **Scenarios:**
  - FACTORYUI-1: picking Fast sets every step to Fast's and marks the preset `fast`.
  - FACTORYUI-2: changing one step marks the preset `custom`.
  - FACTORYUI-3: invalid JSON shows the problem on its line, and Save stays off.
  - FACTORYUI-4: two reviews on one tool show the security-path line.
  - FACTORYUI-5: a stale save (409) reloads and says so.

### B00-10 · feat(obid): each step's hint, from this project's own history

- **What.** Median minutes and share of the week per step, over the last 20 jobs of that step (§3.4).
- **Why.** Choosing a model without knowing its cost in time and room is a guess.
- **How.** It uses job times and the account's `sevenDay` before and after. A key-billed account shows dollars from `prices_usd_per_mtok`.
- **Scenarios:**
  - FACTORYSET-20: with 4 samples the hint is `null` ("no history yet").
  - FACTORYSET-21: with 20 samples it is their median.
  - FACTORYSET-22: gate and prove have no `weekShare`.

### B00-11 · feat(obid): a project works in its own folder or a new worktree, and a folder with no remote can still finish

- **What.** As T3 Code does it (the owner's answer, 2026-11-15). Each conversation picks where it works: **This folder** (the project's own checkout, edited in place) or **A new worktree** (its own branch and copy). It's chosen when the conversation starts and kept after. Finishing offers what the folder can do. With a remote: Commit, Push, Create pull request, as today. With no remote: only **Commit**, and once everything is committed, **Publish repository**, which makes the remote on the code host. A folder that isn't a git repository always works in place and offers **Initialize Git**.
- **Why.** Someone with no GitHub can't use Oby's work at all today: the top gap for that persona. T3 Code shows a simple way that needs no local merge.
- **How.**
  - Following T3, there is **no local merge** of a worktree branch: T3 has none, and merging happens on the code host once the repository is published. Its evidence: `packages/contracts/src/environment.ts:54` (modes `local` and `worktree`), `apps/web/src/components/BranchToolbar.logic.ts:95-113` (the labels), `GitActionsControl.logic.ts:147-149,288-308` (no remote: Commit only, then Publish repository), `packages/shared/src/projectSettings.ts:210-215` (not git: always local).
  - Default: a conversation works in **this folder** (T3's default). A factory job always gets its own worktree, because the factory runs several at once, as T3 makes a new worktree per model when one message goes to several.
  - **Lanes change, the owner's word:** today the owner's own Claude Code conversation writes only its room (LANES-12) and Codex's never `repos/` (CODEX-2). Working in this folder means the conversation's commands may write that folder, and only that folder. `docs/contracts/lanes.md` changes in the same pull request. The five non-negotiables are untouched: no secret, login or vault value reaches the folder.
  - Worktree clean-up follows T3's rules: a worktree is removed with its conversation, and once merged (A00-69 already frees a merged job's).
- **Scenarios:**
  - WORKSPACE-1: a conversation started with "This folder" edits files in the project's folder, and its commands can't write outside that folder.
  - WORKSPACE-2: a conversation started with "A new worktree" works on its own branch, and the folder's own files are untouched.
  - WORKSPACE-3: the choice can't change after the first message.
  - WORKSPACE-4: in a folder with no remote, finishing offers Commit only, and no gh call is made; once clean, it offers Publish repository.
  - WORKSPACE-5: in a folder that isn't a git repository, work happens in place, and finishing offers Initialize Git.
  - WORKSPACE-6: every factory job gets its own worktree, whatever the conversation's choice.

### B00-12 · feat(obid): a project's factory switch and "ask before anything merges"

- **What.** These are two per-project settings. `factory: false` means conversations don't turn words into tasks; they answer and work where the conversation works (B00-11). `askBeforeMerge: true` means even the owner's own work gets a Ready-to-merge card.
- **Why.** Some people want conversations only, and some want to approve every merge (the CTO).
- **How.** They are project settings beside `factory.json`. The brain's planner reads `factory` and merging reads `askBeforeMerge`. Both are the owner's word, overriding defaults.
- **Scenarios:**
  - PROJECTSET-1: with `factory: false`, words in a project conversation start no job unless the owner asks for one by name.
  - PROJECTSET-2: with `askBeforeMerge: true`, own work that passed its reviews gets a merge card and doesn't merge by itself.
  - PROJECTSET-3: the default keeps today's behaviour (MERGING-45).

### B00-13 · feat(electron): Waiting for you, one list across the profile's projects

- **What.** A panel row with a count opens one list: merges, permissions and questions across projects, each with its summary and action (screen 17).
- **Why.** Someone who mostly reviews needs one queue, not cards mixed into Home.
- **How.** It is built over `GET /v1/inbox` (`triage: needs_you`) and the merge cards. No new obid state.
- **Scenarios:**
  - WAITING-1: three `needs_you` items in two projects show as three rows, longest waiting first.
  - WAITING-2: Merge on a row sends the signed Merge (signed words) and the row leaves.
  - WAITING-3: the count matches the rows.

### B00-14 · feat(electron): the New profile sheet with its accounts and code-host login

- **What.** Screen 18: name, colour, the accounts this profile may use, and its GitHub login.
- **Why.** Someone running several companies needs each one's accounts kept apart.
- **How.** After A00-79. It uses `POST /v1/profiles` plus account assignment, and needs per-profile gh logins: the "one gh login per Mac" gap.
- **Scenarios:**
  - PROFILESUI-1: creating Acme with only Acme Max on lists only Acme Max in Acme's pickers.
  - PROFILESUI-2: an account on in Default and off in Acme never runs Acme's jobs.

### B00-15 · fix(obid): triage, memory and the recap run on any signed-in account

- **What.** These use "the first Claude Code account" today (`core/crates/obid/src/brain/calls.rs:57`; `core/crates/obid/src/triage.rs:461`).
- **Why.** A Codex-only owner loses them silently.
- **How.** Pick by room across tools, as job routing does.
- **Scenarios:**
  - TRIAGE-ANY-1: with only a Codex account, a new inbox item is sorted within 2 minutes.
  - TRIAGE-ANY-2: with none signed in, items stay `pending` and say why.

---

## 6. The owner's decisions (2026-11-15)

| # | Question | The owner's answer | What it changed |
|---|---|---|---|
| 1 | The tasks' prefix | B00 (in the parent session); B00-01 is taken | Drafts are B00-02 to B00-15. Neither T3 Code nor OpenBot numbers tasks, so there was nothing to copy. |
| 2 | Where `/` opens | "Like T3 does" | Anywhere a word starts with `/`; mid-text only agents and `/model` (§2.2, SLASH-1). |
| 3 | The CLIs' own built-ins | "Create a title and put them under it. Only show based on what is connected" | One title per connected tool, with its skills and headless commands (§2.1, screen 11, COMMANDS-11, 15, SLASH-6). |
| 4 | Where factory settings live | "Yes, your proposal" | One `factory.json` per project in obid (§3.2). |
| 5 | Where `/factory add` writes | "On obid, to not mess with people's project" | A queue obid keeps per project (B00-05, FACTORY-CMD-6). |
| 6 | Factory off: edit the folder directly? | "Follow what T3 does" | This folder or a new worktree, per conversation, this folder by default; a lanes change (B00-11). |
| 7 | No remote: merge locally? | "What T3 does" | No local merge: Commit, then Publish repository (B00-11, screen 16). |
| 8 | Hints | "Sure, show it that way" | % of the week for plan accounts, dollars for key-billed ones (§3.4). |
| 9 | Second review on Cursor | "Yes, consider every account; others will be added" | Every connected account may review (B00-07, FACTORYSET-14). |
| 10 | May Cheapest skip the second review? | "Research how other platforms do it" | No product lets a cheaper tier relax a gate, so a preset never changes a gate; Cheapest keeps the second review on a cheap model of another tool (§3.3). |

Nothing is left open. B00-02 to B00-15 are written as task files in `factory/queue/tasks/`, by hand in `job add`'s header format (`SPRINT: 0`, the ad-hoc milestone), each depending on the A00 tasks it builds on.
