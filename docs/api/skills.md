# Skills: the owner's agent skills in the message box

The skills the owner installed for their own Claude Code or Codex (user skills, plugins), offered in the message box by `/` and handed to the agent in the form it understands, as T3 Code does. Contract: [docs/contracts/skills.md](../contracts/skills.md) (SKILLS-1 to 12) and [composer.md](../contracts/composer.md) (COMPOSER-16, 25 to 28). Listed in `GET /v1/status` as `skills-v1` ([capabilities.md](capabilities.md)): an app asks for none and sends no `skill` until obid lists it.

## The skill object

```json
{"name": "review:check-diff", "description": "Reads the diff and says what's risky.",
 "userInvocable": true, "source": "plugin", "plugin": "review"}
```

- `name`: what the agent calls it: a user or project skill's folder name; a Claude Code plugin's skill as `<plugin>:<skill>`; Codex's own name for it. Letters, digits, `:`, `_` and `-`, starting with a letter or digit.
- `description`: its own, from its `SKILL.md` (Codex: its short description first), or absent.
- `userInvocable`: false for a Claude Code skill whose `SKILL.md` says `user-invocable: false` (only the agent starts it). The app offers only `true` ones; obid refuses the rest as a `skill`.
- `source`: `user`, `project`, `plugin`, `managed` (Claude Code's managed folder) or `system` (Codex's own). A string an app shows as it is: more may come.
- `plugin`: the plugin's name, for `source: "plugin"` only.

A skill turned off in the owner's settings (Claude Code's `skillOverrides: "off"`, Codex's `enabled: false`) isn't listed.

## `GET /v1/skills?conversation=<id>` / `?job=<id>` / `?account=<id>` / (neither)

The skills of the agent that would get the owner's next words there, and only where they'd run: `200`

```json
{"tool": "claude", "account": "claude-default", "skills": [ … ]}
```

- `?conversation=<id>`: the conversation's next turn's agent and account (the one the owner chose, else the one its session is on, else whichever has room, of its profile's accounts).
- `?job=<id>`: the task's agent and account, while it's at work (`running`): a running task takes words at once (TASKCONV-33 to 37).
- `?account=<id>`: the account's agent's own skills, wherever its turns would run (none of a project's; `[]` for Cursor).
- Neither: a new conversation of the current profile (Home's box).
- `skills` is `[]`, with `why` (one sentence for the owner), where skills wouldn't run: the turn or task runs in Oby's box (another Oby's conversation, a task marked untrusted, Oby's review, Codex on a ChatGPT login, Cursor), the task isn't at work, or no account can talk. `tool` and `account` are `null` when no account would. `404` for a conversation, job or account obid doesn't have (or another profile's).
- Read from the account's own folder only, never another's: Claude Code's `configDir` (or `~/.claude`), with its `settings.json`, its `plugins/installed_plugins.json` and each plugin's `.claude-plugin/plugin.json`, the Mac's managed folder, and a task's worktree's `.claude/skills`; Codex's own `skills/list`, from an `app-server` on the account's own `CODEX_HOME`, as its own run would have it. Kept until any of those files change.

## `skill` on `POST /v1/plans` and `POST /v1/jobs/{id}/messages`

`"skill": "<name>"`, the skill the owner picked in the box, sent beside `text` (never inside it). obid checks its shape (`422` otherwise: a name as above, at most 128 characters) and, as the words reach the agent, that the agent there has it and the owner may start it:

- Claude Code gets the words as one user message whose last text block is `/<name> <words>` (all obid adds before it, its first block), the only place Claude Code expands a skill.
- Codex gets `$<name> <words>` where the words go.
- A skill with no words (`said` or `text` empty) is a message of its own: `/<name>` (`$<name>`) alone. Where it doesn't run, a conversation's turn sends nothing and answers why.
- A conversation's turn whose agent doesn't have it there (another account it moved to, Oby's box) gets the words alone, and its answer says the skill wasn't used and why.
- A job's words: `422` when the task isn't at work in the own lane or its agent hasn't the skill.

obid makes a skill call only from `skill`: quoted text, files, an earlier turn, a report, another Oby's question and anything from outside go where they always did, in obid's own notes and fences before the owner's words, never opening the block Claude Code starts a skill from. Words the owner types starting `/` reach their Claude Code as typed, as in their terminal.

## `POST /v1/conversations/{id}/restart`

Ends the conversation's agent waiting for its next turn, if one waits: the next turn starts it again on the same session (`--resume`, Codex's `thread/resume`), with the skills, plugins and settings installed since. `200` `{"restarted": true}` (`false` when none waited), `404` for a conversation obid doesn't have. A turn under way isn't touched. ⌘K's **Restart agent** sends it.

obid also does this by itself: an agent waiting for a conversation's next turn whose account's skills, plugins or settings changed since it started is ended, and the turn starts one afresh (SKILLS-9).
