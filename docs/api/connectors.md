### obid only: the owner's own connectors

Settings › Connectors shows, after GitHub and Tetrix, the MCP servers and claude.ai connectors the owner's own Claude Code has (docs/contracts/connectors.md). obid doesn't hold them: an own conversation's session (LANES-12) names them in Claude Code's `init` line, and obid keeps each one's name and status, but its own `oby` and `tetrix`, in `home/connectors.json` (0600), replaced whole by the next own session's. A boxed session's list never replaces it. Not on the gateway's allowlist.

**`GET /v1/connectors`** answers `{"claude": {"seenAt": "2026-11-17T15:00:00Z", "servers": [{"name": "claude.ai Slack", "status": "connected"}, {"name": "rea", "status": "failed"}]}}`, the servers in Claude Code's order, or `{"claude": null}` before any own session named them:

- `seenAt`: when obid last saw the list.
- `name`: as Claude Code says it; a claude.ai connector's starts with `claude.ai `.
- `status`: Claude Code's own word, an open string: `connected`, `needs-auth`, `pending`, `failed` and `disabled` today. Read one you don't know as not connected.

No address, command, argument or environment is kept or answered: Claude Code's `init` line has none.
