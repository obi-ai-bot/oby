### obid only: the owner's own commands

The message box's `/` lists the commands and skills the owner's own Claude Code takes, and the owner's words run one in the conversation's session (docs/contracts/composer.md, COMPOSER-16 to 21). obid doesn't hold them: an own conversation's session (LANES-12) names them in Claude Code's stream-json `system`/`init` line, and obid keeps their names in `home/commands.json` (0600), replaced whole by the next own session's. A boxed session's list and a job's never replace it. Not on the gateway's allowlist.

**`GET /v1/commands`** answers `{"claude": {"seenAt": "2026-11-17T15:00:00Z", "commands": [{"name": "simplify", "skill": true}, {"name": "compact", "skill": false}]}}`, in Claude Code's order, or `{"claude": null}` before any own session named them (or when the file doesn't read):

- `seenAt`: when obid last saw the list.
- `name`: as Claude Code names it, without the `/` (a plugin's may hold a `:`, as `vercel:deploy`). Claude Code's `init` `slash_commands`, but its `terminal_slash_commands` (they run only in a terminal) and names starting with `_`.
- `skill`: Claude Code's `skills` names it too.

Nothing a command or skill says or does is kept or answered: only names. A Claude Code whose `init` names no `slash_commands` (before 2.1.317) leaves the last list as it was.

**Running one** has no endpoint: the owner's words do it, on `POST /v1/plans` (docs/api/conversations.md). Words an app of the owner's signed, one message alone (not answered together, not an `answer`), with no `attachments` and no `jobs`, whose first word is `/` and a name this list holds, in a turn that runs on an own Claude Code session, are that turn's whole prompt, unchanged, so Claude Code runs the command; its answer is Oby's turn as any other. Any other words starting with a listed name get `These words aren't a command:` on a line before them, so they stay words. The job reports a command's turn doesn't tell go with the owner's next words.
