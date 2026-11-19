# The owner's agent skills

The business behavior contract for using the skills the owner installed for their own Claude Code or Codex (their own skills, a plugin's) from Oby's message box, as T3 Code does, from [the template](../business-contract-template.md). Scenarios are numbered `SKILLS-n`; the tests that prove them are named after them: obid's in `core/crates/obid/src/skills.rs`, `core/crates/obid/tests/skills_api.rs` and `core/crates/obid/tests/long_jobs.rs`. The box's side is [composer.md](composer.md) (COMPOSER-25 to 28); the API is [docs/api/skills.md](../api/skills.md).

## Feature and status

- **Outcome:** The owner who installed a plugin of skills for Claude Code (`/plugin install …`) or a skill for Codex types `/` in Oby's message box, finds the skill by its name or what it does, picks it, and their words reach the agent as a skill call, as in their terminal. A skill installed while a conversation's agent waits is there at the next message, and ⌘K's Restart agent starts the agent again for anything else that changed.
- **Status:** **Implemented.** No screen of Sangam's yet: the `/` list keeps the box's own panel (COMPOSER-16's), and its words are his to change.
- **Scope:** obid's listing of each account's skills, the `skill` the box sends with the owner's words to a conversation or a task at work, the form each agent gets it in, and starting a waiting agent again. **Out of scope:** installing, updating or removing a skill or plugin (the agent's own `/plugin`, `codex` commands, in the owner's terminal); the agents' built-in commands (`/compact`, `/model`); Claude Code's claude.ai-synced plugins and `CLAUDE_CODE_PLUGIN_DIRS`, Codex's plug-ins (off in a conversation's Codex); Cursor; a task's follow-up (a task no longer at work); the iPhone.

## Behavior

### Actors and ownership

- **The owner** types `/`, picks a skill, sends words, and presses ⌘K › Restart agent.
- **obid** lists each account's skills from the account's own folder or its agent's own listing, decides where they'd run, checks the picked skill, and writes the words in the agent's form. It runs nothing of a skill's, installs nothing and writes nothing of the agent's.
- **The agent** (Claude Code, Codex) runs the skill as it runs one in the owner's terminal: its hooks, scripts and `allowed-tools` are the owner's settings' (LANE-1, LANES-12, LANES-18).
- **The app** asks obid what to offer and sends the name the owner picked beside their words (composer.md).

### What an account has (SKILLS-1 to 4)

- **Claude Code** reads, from the account's own folder only (its `configDir`, or `~/.claude` for the default login; never another account's), as Claude Code does: the Mac's managed folder's `.claude/skills`, the account's `skills/`, a task's worktree's `.claude/skills`, then each plugin `settings.json`'s `enabledPlugins` turns on (`true` only), installed in `plugins/installed_plugins.json` for the account (`user`, `managed`) or for that worktree (`project`, `local`), its skills under `skills/`, its own `SKILL.md`, or the folders its `.claude-plugin/plugin.json` names inside it. A plugin's skill is `<plugin>:<skill>`, the plugin named by its manifest.
- A skill's `SKILL.md` frontmatter gives its description and whether the owner may start it (`user-invocable: false`: only the agent does). One whose frontmatter can't be read is left out; `skillOverrides: "off"` in the settings leaves one out (a settings file holding a value Claude Code doesn't know is read as having none, as Claude Code reads it). The first of a name wins: managed, the account's, the project's, the plugins'.
- **Codex** lists its own (`skills/list` from an `app-server` on the account's own `CODEX_HOME`, plug-ins off as a conversation's runs): those on, its short description first, its scope as where it came from.
- A listing is kept until a file it was read from changes (its folders, `SKILL.md`s, settings, the plugins' records and manifests, Codex's `config.toml` and skill folders); obid reads only names and times to tell.

### Where they're offered (SKILLS-5)

- Only where they'd run: the turn or task whose agent runs on the owner's own settings: their Claude Code conversation on macOS (LANES-12), their Codex conversation in Bypass on a `codex login` (LANES-13), a Claude Code task at work that's theirs (LANE-1), a Codex one in Bypass (LANES-18). Anywhere else (another Oby's conversation, a task marked untrusted, Oby's review, Codex outside Bypass or on a ChatGPT login, Cursor, a task not at work) obid lists none and says why in one sentence.
- A conversation's are those of the account its next turn would talk through: the one the owner chose, else the one its session is on, else whichever has room, of its profile's. Home's box with no conversation yet: a new conversation's.

### The words with a skill (SKILLS-6 to 8, 11, 12)

- The box sends the skill's name as `skill`, beside the words, never in them. A skill picked with no words is a message of its own: the agent gets `/<name>` (`$<name>`) alone; where it doesn't run, nothing is sent and the answer says so (`Nothing was sent: the skill <name> doesn't run here (<why>).`).
- **Claude Code** gets one user message whose last text block is `/<name> <words>`, everything obid adds before the owner's words (its notes, quoted text, earlier turns) in a block before it: the one place Claude Code starts a skill.
- **Codex** gets `$<name> <words>` where the owner's words go.
- A conversation's turn whose agent hasn't the skill there (it moved to another account, the turn is in Oby's box, the skill went, or only the agent may start it) gets the words alone, and its answer says so in one line: `Your words went without the skill <name>: <why>.`
- A task at work takes words with a skill as it takes words (TASKCONV-33 to 37), `/name` or `$name` before them. obid refuses (`422`, why) a skill for a task not at work, in Oby's box, or whose agent hasn't it.
- A name is letters, digits, `:`, `_` and `-`, starting with a letter or digit, holding a letter, at most 128: anything else is refused (`422`) and nothing of the words is kept.

### A new install, and Restart agent (SKILLS-9, 10)

- A conversation's agent that waits for its next turn (docs/fast-replies.md), or one started ahead for the next new conversation, carries the mark of its account's skills, plugins and settings as they were when it started. A turn finds it with another mark (something was installed, turned on or off, or changed): it ends, and the turn starts the agent again on the same session (`--resume`, Codex's `thread/resume`).
- **Restart agent** (⌘K, a conversation open) ends the conversation's waiting agent now; one a turn under way would keep doesn't wait after it. The next turn starts it again on the same session. obid answers whether one waited.

### Failures and recovery

- A listing that can't be read (a folder that isn't there, Codex that doesn't answer) is empty: the box says `/` has nothing, as COMPOSER-16 does.
- A skill the turn's agent hasn't: the words go alone, said so (above).
- A restart for a conversation obid doesn't have, or another profile's: `404`.

### Invariants and boundaries

- Only the `skill` the owner's box sends picks a skill. obid never turns text into a skill call: an issue's, a page's, another Oby's, a file's, an earlier turn's or a report's text goes where it always did, inside obid's own notes and fences before the owner's words, never opening the block Claude Code starts a skill from. Words the owner types starting `/` reach their Claude Code as typed, as in their terminal.
- Skills, and anything they run, reach only an agent that already runs on the owner's own settings (the own lane). A boxed run gets a bare login copy and none of them: obid lists none there and gives none there.
- obid reads an account's skills from that account's own folder only, reads names, descriptions and switches, and writes nothing.

## Interfaces and dependencies

- `GET /v1/skills?conversation=` / `?job=` / `?account=` / neither, `skill` on `POST /v1/plans` and `POST /v1/jobs/{id}/messages`, `POST /v1/conversations/{id}/restart` ([docs/api/skills.md](../api/skills.md)).
- `GET /v1/status`: `skills-v1` in `capabilities` ([docs/api/capabilities.md](../api/capabilities.md)).
- Lanes: [lanes.md](lanes.md) (LANE-1, LANES-12, 13, 18).

## Acceptance scenarios

- **SKILLS-1:** Given a Claude Code account's own folder with skills, when obid lists its skills, then each is listed by its folder's name, with its description and whether the owner may start it; another account's folder isn't read. Test: `skills_1_claude_codes_own_skills_from_the_accounts_folder` (`src/skills.rs`).
- **SKILLS-2:** Given plugins installed and turned on in the account's settings, then their skills are `<plugin>:<skill>`, named by the manifest, its declared folders too; one turned off, or installed for another project, gives none. Test: `skills_2_an_enabled_plugins_skills_are_plugin_colon_skill`.
- **SKILLS-3:** Given a skill turned off in the settings, unreadable frontmatter, or the same name in the managed folder and the account's, then the first is off the list, the second skipped, the managed one listed. Test: `skills_3_settings_and_frontmatter_decide_what_is_offered`.
- **SKILLS-4:** Given Codex's own listing, then those on are listed, short description first, by scope. Test: `skills_4_codex_lists_its_own`.
- **SKILLS-5:** Given the owner's own Claude Code, then `GET /v1/skills` lists the account's skills; a conversation in Oby's box lists none and says why; an unknown conversation, job or account is `404`. Test: `skills_5_the_box_lists_the_agents_skills_where_they_run` (`tests/skills_api.rs`).
- **SKILLS-6:** Given words sent with a skill the account has, then Claude Code gets `/name` and the words opening the last text block, all else before it; a skill sent with no words, `/name` alone. Tests: `skills_6_a_picked_skill_reaches_claude_code_as_slash_name`, `skills_6_and_7_each_agent_gets_its_own_form` (`src/skills.rs`).
- **SKILLS-7:** Given words sent with a skill to Codex, then it gets `$name` and the words. Test: `skills_6_and_7_each_agent_gets_its_own_form`.
- **SKILLS-8:** Given a skill the turn's account hasn't, or only the agent may start, then the words go alone and the answer says so (with no words, nothing is sent); words that themselves say `/name` start none. Test: `skills_6_a_picked_skill_reaches_claude_code_as_slash_name`.
- **SKILLS-9:** Given a conversation's agent waiting, when a skill is installed on its account, then the next turn starts the agent again on the same session; with nothing installed, the same one takes it. Tests: `skills_9_a_waiting_agent_started_before_an_install_starts_again`, `skills_9_the_mark_moves_with_what_is_installed` (`src/skills.rs`).
- **SKILLS-10:** Given a conversation's agent waiting, when the owner restarts it, then it ends and the next turn starts it again on the same session; again with none waiting answers `false`; an unknown conversation is `404`. Test: `skills_10_restart_agent_starts_the_next_turn_afresh`.
- **SKILLS-11:** Given a Claude Code task at work, when the owner's words come with a skill its account has, then its agent hears `/name` and the words; a skill it hasn't, or a task no longer at work, is `422`. Test: `skills_11_a_picked_skill_reaches_a_task_at_work` (`tests/long_jobs.rs`).
- **SKILLS-12:** Given a `skill` that isn't a skill's name, then the words are refused (`422`) and nothing is kept. Tests: `skills_12_a_skill_that_isnt_a_name_is_refused`, `skills_6_and_7_each_agent_gets_its_own_form`.

### Test it when it's done (real Mac)

1. With a Claude Code plugin of skills installed, type `/` in a conversation: its skills are listed with what they do; type part of one's name: the list narrows. Pick one and send words: the agent runs that skill on them, as in the terminal.
2. Install another skill in the terminal while the conversation's agent waits, then send words with it: it's there and runs.
3. ⌘K › Restart agent, then send words: the answer comes, the conversation goes on.
4. In a task at work, `/`, pick a skill, send: the task's agent takes it.
5. In a conversation on Cursor, or Codex outside Bypass, type `/`: the list says why there's none.
