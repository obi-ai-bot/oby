# Permission modes: Bypass asks nothing

The business behavior contract for how much a job asks its owner, from [the template](../business-contract-template.md). Scenarios are numbered `BYPASS-n`; the tests that prove them name the scenario.

## Feature and status

- **Outcome:** The owner picks how freely their agents work, in Settings and on each task, with Sangam's four modes, and Bypass, the default, asks nothing but secrets. On the 2026-10-28 call every task asked for permissions although Settings showed Bypass ([#1570](https://github.com/obi-ai-bot/oby/issues/1570)): obid wasn't in Bypass, and the app's Mode menu did nothing.
- **Status:** **Implemented** by [#1592](https://github.com/obi-ai-bot/oby/pull/1592) (Bypass holds where the brain, the conversation's shell and approved plans lost it; only what touches secrets asks) and by the change that added this contract (a new install starts in Bypass, the Mode menu, hooks, Question cards). Sangam's design (General › Permissions: Ask, Accept edits, Plan, Bypass · "Asks nothing", Bypass the default; the task card's Mode button) and the owner's decision of 2026-10-27 lead.
- **Scope:** obid's default mode, the modes the Electron app offers, what Bypass still asks, the agent's own questions. **Out of scope:** the words of the secrets card (Sangam's to design), the Swift apps (frozen; they keep decoding every mode), Linux's network reading.

## Behavior

### Actors and ownership

- **obid** keeps the default mode (`home/permissions.json`), gives each new job its mode, decides every call by its fixed rules, the owner's saved rules and the mode, and asks the owner what's left.
- **The Electron app** shows and sets the default (Settings › General › Permissions on this Mac) and a task's mode (its Mode menu), shows what a job asks on its card, and sends the owner's answers.
- **The owner** answers what's asked, from the app.

### The modes

| Mode | Asks the owner about |
|---|---|
| Ask | every edit and command |
| Accept edits | every command |
| Plan | nothing: it only reads, then presents its plan to approve |
| Bypass | only what touches secrets, a login or the vault |
| Sandbox (obid's old default) | the network beyond the model's API, a read outside the worktree, what obid can't read, secrets |

The app offers the first four. Sandbox stays in obid's API, add-only, for the installs that saved it and the jobs that have it: Settings shows it only while it's the saved one, and a task in it shows `Sandbox` on its Mode button.

### Decisions

- **A new install starts in Bypass.** No `permissions.json`, or one with rules and no `mode`, means `bypass`. A mode the owner saved is never changed by an upgrade, `sandbox` included.
- **Bypass asks nothing but secrets.** Commands, edits, the network, the conversation's shell, a repository's hooks (Cursor's), approved plans and the brain's own Bypass jobs all go ahead. What touches secrets, a login or the vault asks in every mode, Bypass too, and no mode change or rule settles that card.
- **The agent's own question is not a permission.** Claude Code's `AskUserQuestion` (and Codex's and Cursor's questions) waits for the owner in every mode, because the agent chose to ask; the app shows it as Sangam's Question card, the question as its headline and the agent's options as buttons, never as a Permission card with Allow and Deny.
- **A task's Mode menu changes that task only**, from now on (`POST /v1/jobs/{id}/mode`): a waiting permission the new mode allows is answered; a secret or a question keeps waiting.

### Failures and recovery

- A `permissions.json` that doesn't read is taken as empty: Bypass, and obid logs it.
- A Mode or an answer obid refuses (an unknown job, a mode that isn't one, a question already answered) shows obid's words in the app, and the card stays.

### Invariants

- The box, the fences (the worktree only, no publishing by the agent, no background work) and the secret scan hold in every mode. Bypass changes what obid asks, never what the box allows.
- obid's API stays add-only for the Swift iPhone and Watch apps: `sandbox` stays a mode they may see.

## Interfaces

`GET`/`PUT /v1/settings/permissions`, `GET /v1/status` (`permissions.mode`), `POST /v1/jobs/{id}/mode`, `POST /v1/jobs/{id}/permission` (`{"id", "answer"}` for a question), the `agentQuestion` inbox item ([runner-api.md](../runner-api.md)).

## Acceptance scenarios

- **BYPASS-1** A new install, no `permissions.json` or one with rules and no mode: Settings and `GET /v1/status` say `bypass`, a new job is in `bypass`, and its `npm install` runs without a card. `jobs_api::a_new_install_starts_in_bypass_and_a_saved_mode_stays`, `status.rs` (`permissions` of a fresh home), `permissions::tests` (`Kept::default()`), `jobs::tests::an_approved_plan_goes_on_in_the_owners_default_mode`.
- **BYPASS-2** An install that saved `sandbox` (or `bypass`) upgrades: it keeps it, and a Sandbox job asks before `npm install`. `jobs_api::a_new_install_starts_in_bypass_and_a_saved_mode_stays`.
- **BYPASS-3** A Cursor job in Bypass on a repository with hooks starts without a card; in any other mode it asks, as before. `permissions::tests::a_repositorys_hooks_ask_in_every_mode_but_bypass`, `adapters::cursor_asks_you_before_it_runs_a_repositorys_hooks`.
- **BYPASS-4** A Bypass job that reads `.env` asks; changing its mode to any other doesn't settle the card. `jobs_api::a_mode_change_never_settles_a_secret`, `permissions::tests::only_what_touches_secrets_asks_in_bypass` (#1592).
- **BYPASS-5** An agent asks "Which store?" with SQLite and Postgres: the task's card is a Question card with that headline and those buttons; a pick answers the agent. `apps/electron/src/renderer/src/test/asks.test.ts` (`bypass_5_…`); obid's side, `jobs::tests::the_agents_question_waits_for_the_owners_answer` (#858).
- **BYPASS-6** On a running task in Accept edits, the owner opens Mode and picks Bypass: the menu offers Ask, Accept edits, Plan and Bypass, the task shows Bypass, and its waiting command card is answered. obid's side: `jobs_api::a_mode_change_never_settles_a_secret` (the route, every mode); the menu: checked on the mock in the browser.
- **BYPASS-7** The conversation's shell, an approved plan and the brain's Bypass jobs ask nothing in Bypass. `brain::shell::tests::installs_and_the_network_ask_in_every_mode_but_bypass`, `jobs::tests::the_owners_words_carry_a_jobs_bypass_into_its_follow_up` (#1592).

**Test it when it's done (real Mac):** on a fresh obid home, Settings › Permissions shows Bypass; a task that runs `npm install` finishes with no card; the task page's Mode menu changes a running task to Ask and its next command asks; a Claude Code task told to ask a question with options shows a Question card, and a pick lets it go on.
