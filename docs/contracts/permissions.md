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
| Plan | a command or a site that doesn't only read (what only reads goes ahead; an edit is refused until the owner approves its plan) |
| Bypass | only what touches secrets, a login or the vault |
| Sandbox (obid's old default) | the network beyond the model's API, a read outside the worktree, what obid can't read, secrets |

The app offers the first four. Sandbox stays in obid's API, add-only, for the installs that saved it and the jobs that have it: Settings shows it only while it's the saved one, and a task in it shows `Sandbox` on its Mode button.

### Decisions

- **A new install starts in Bypass.** No `permissions.json`, or one with rules and no `mode`, means `bypass`. A mode the owner saved is never changed by an upgrade, `sandbox` included. An install that never saved a mode (most 0.4.x installs, which ran in Sandbox by default) is in Bypass after the upgrade too: the owner's decision of 2026-10-30, said in the release notes. A saved mode obid can't read (a newer obid's, a hand edit), or a file that doesn't read, fails closed: Ask, as a job whose mode obid can't read does.
- **Bypass asks nothing but secrets.** Commands, edits, the network, the conversation's shell, a repository's hooks (Cursor's), approved plans and the brain's own Bypass jobs all go ahead. What touches secrets, a login or the vault asks in every mode, Bypass too, and no mode change or rule settles that card. A repository's hooks are judged as the agent's commands are (`permissions::decide`), when the job starts and when its mode changes alike, so a hook that touches secrets asks in Bypass too, with no Always.
- **The agent's own question is not a permission.** Claude Code's `AskUserQuestion` (and Codex's and Cursor's questions) waits for the owner in every mode, because the agent chose to ask; the app shows it as Sangam's Question card, the question as its headline and the agent's options as buttons, never as a Permission card with Allow and Deny. His Tell it something follows the options and opens the card's field for the owner's own words, so a question with no options is answered too. A pick or the words answer the inbox item itself (`POST /v1/inbox/{id}/answer`), never the job's permission, which can lag behind the inbox between a call's questions.
- **A task's Mode menu changes that task only**, from now on (`POST /v1/jobs/{id}/mode`): a waiting permission the new mode allows is answered; a secret or a question keeps waiting.

### Failures and recovery

- A `permissions.json` that can't be read (its permissions, a folder in its place) or doesn't parse is logged and taken as no rules and Ask, until the owner picks a mode again. Only a missing file is a new install.
- A Mode or an answer obid refuses (an unknown job, a mode that isn't one, a question already answered) shows obid's words in the app, and the card stays.

### Invariants

- The box, the fences (the worktree only, no publishing by the agent, no background work) and the secret scan hold in every mode. Bypass changes what obid asks, never what the box allows.
- obid's API stays add-only for the Swift iPhone and Watch apps: `sandbox` stays a mode they may see.

## Interfaces

`GET`/`PUT /v1/settings/permissions`, `GET /v1/status` (`permissions.mode`), `POST /v1/jobs/{id}/mode`, the `agentQuestion` inbox item and `POST /v1/inbox/{id}/answer` ([runner-api.md](../runner-api.md)).

## Acceptance scenarios

- **BYPASS-1** A new install, no `permissions.json` or one with rules and no mode: Settings and `GET /v1/status` say `bypass`, a new job is in `bypass`, and its `npm install` runs without a card. A saved mode obid can't read, or a file that doesn't read, is Ask. `jobs_api::bypass_1_2_a_new_install_starts_in_bypass_and_a_saved_mode_stays`, `permissions::tests::bypass_1_a_missing_mode_is_bypass_and_an_unreadable_one_is_ask`; `status.rs`'s fresh home says `bypass` too.
- **BYPASS-2** An install that saved `sandbox` (or `bypass`) upgrades: it keeps it, and a Sandbox job asks before `npm install`. `jobs_api::bypass_1_2_a_new_install_starts_in_bypass_and_a_saved_mode_stays`.
- **BYPASS-3** A Cursor job in Bypass on a repository with hooks starts without a card; in any other mode it asks, as before; a hook that touches secrets asks in every mode. `permissions::tests::bypass_3_a_repositorys_hooks_ask_in_every_mode_but_bypass`, `adapters::bypass_3_cursor_asks_you_before_it_runs_a_repositorys_hooks`.
- **BYPASS-4** A Bypass job that reads `.env` asks; changing its mode to any other doesn't settle the card. `jobs_api::bypass_4_a_mode_change_never_settles_a_secret`, `permissions::tests::bypass_4_only_what_touches_secrets_asks_in_bypass` (#1592).
- **BYPASS-5** An agent asks "Which store?" with SQLite and Postgres: the task's card is a Question card with that headline, those buttons and Tell it something; a pick, or the owner's words, answers that inbox item. A question with no options offers Tell it something. `apps/electron/src/renderer/src/test/asks.test.ts` (`bypass_5_…`); obid's side, `jobs::tests::bypass_5_the_agents_question_waits_for_the_owners_answer` (#858).
- **BYPASS-6** On a running task in Accept edits, the owner opens Mode and picks Bypass: the menu offers Ask, Accept edits, Plan and Bypass, the task shows Bypass, and its waiting command card is answered. The app: `asks.test.ts` (`bypass_6_changes_the_jobs_mode_through_obid`: `POST /v1/jobs/{id}/mode`), and the menu checked on the mock in the browser. obid: `runner_api::bypass_6_modes_answer_what_they_allow_and_stopping_drops_the_permission`.
- **BYPASS-8** A job's background helper (`Agent` or `Task` with `run_in_background`) runs in Bypass, where nothing asks for a permission; in any other mode it's refused, as before, and one run remotely (`isolation: remote`, outside Oby's box) in every mode. `policy::tests::bypass_8_a_background_helper_runs_in_bypass`.
- **BYPASS-9** A Cursor job on a repository whose `.cursor/cli.json` allows commands without asking obid asks the owner with a card, as for its hooks (BYPASS-3), where it used to refuse to start: Allow starts it, Deny says why, Bypass answers it. `adapters::bypass_9_a_repository_allowlist_asks_you_before_cursor_starts`.
- **PERMISSIONS-1** git that stays on the Mac runs in a job (`submodule`, `lfs` but its push, `gc`, `sparse-checkout`, `bundle`, `format-patch`); what writes to a code host is refused in every mode, because agents keep no login, and the agent hears that obid pushes its branch and the owner can tell Oby to push, merge or post anything else. `policy::tests::permissions_1_local_git_runs_and_a_code_host_write_says_how`.
- **PERMISSIONS-2** A job's agent reaching telemetry elsewhere than its tool's vendor (Datadog, Sentry) is decided as any site is (its rules, its mode, the owner's card), not refused; a plain `http://` request is decided the same way and forwarded alone, without the job's token, instead of refused. `jobs::tests::permissions_2_the_egress_proxy_lets_through_asks_and_refuses_by_site`, `egress::tests::permissions_2_a_plain_request_names_its_site_and_goes_on_alone`.
- **PERMISSIONS-4** A `permissions.json` that doesn't read (a hand edit, a file a newer obid wrote) is Ask with no rules, as before, and the owner's next change (a mode, an Always) moves it aside to `permissions.json.unreadable-<time>` before writing: the rules it holds are never written over. When it can't be moved, nothing is written and the change fails. `permissions::tests::permissions_4_an_unreadable_file_is_kept_aside_never_written_over` (#1652).
- **PERMISSIONS-5** Plan asks before a command that doesn't only read, WebFetch included, and refuses an edit until the plan is approved (Sangam's "Plans first, and works once you approve the plan"); what only reads never asks. `permissions::tests::permissions_5_each_mode_allows_or_asks` (#1652).
- **BYPASS-7** (#1592) In Bypass, the conversation's shell runs an install with the network (`brain::shell::tests::bypass_7_installs_and_the_network_ask_in_every_mode_but_bypass`; outside Bypass it asks, `conversation_shell::bypass_7_an_install_asks_outside_bypass`); an approved plan goes on in the owner's default, and a Bypass the brain names, continues or carries on asks for no Confirm where Bypass is the default (`jobs::tests::bypass_7_an_approved_plan_goes_on_in_the_owners_default_mode`, which checks `Jobs::bypass_widens`); the owner's words to a finished Bypass job start its follow-up in Bypass (`jobs::tests::bypass_7_the_owners_words_carry_a_jobs_bypass_into_its_follow_up`).

**Test it when it's done (real Mac):** on a fresh obid home, Settings › Permissions shows Bypass; a task that runs `npm install` finishes with no card; the task page's Mode menu changes a running task to Ask and its next command asks; a Claude Code task told to ask a question with options shows a Question card, and a pick lets it go on; one with no options takes Tell it something's words.
