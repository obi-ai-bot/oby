### obid only: helpers

A helper is an assistant of the owner's for one kind of general work (docs/contracts/helpers.md): a name, a colour, standing instructions, the one tool it runs on and the connectors it may use, in one profile, with one conversation of its own. The API says "helper"; the app calls them Agents. Mac only: none of these routes is on the gateway's allowlist, so a paired phone gets the gateway's usual refusal. The Swift runner has none: `404`.

**Where.** `home/helpers.json` (0600), written whole: `{"helpers": [<helper>, …]}`, oldest first. No file is no helpers. A file that doesn't read is logged and left as it is: every route answers as if there were no helpers, and `POST`, `PATCH` and `DELETE` answer `503` with a plain reason instead of writing over it.

**A helper:**

```json
{"id": "8b1c…", "profile": "3f0c…", "name": "Inbox helper", "color": "#3a5a8c",
 "instructions": "Sort my inbox.", "tool": "claude", "connectors": ["github", "web"],
 "conversation": "5d2e…", "created": "2026-11-15T15:30:00Z"}
```

- `id`: a lower-case UUID obid makes. `profile`: a profile's id from `GET /v1/profiles`, set when it's made and never changed. `created`: RFC 3339.
- `name`: one line of 1 to 100 characters after trimming, as a conversation's title on `PATCH /v1/conversations/{id}`. `color`: `#` and 6 hex digits. `instructions`: 1 to 4,000 characters after trimming.
- `tool`: `claude`, `codex` or `cursor`.
- `connectors`: a list from `github` (Oby's GitHub tools `oby_github`, `oby_github_write` and `oby_merge`), `tetrix` (obid's Tetrix server) and `web` (the tool's own web search and fetch), always in that order, each once. An open list: read a value you don't know as nothing to show.
- `conversation`: its one conversation's id (`helper` on it, below).

**`GET /v1/helpers`**: every helper, oldest first; `[]` with none.

**`POST /v1/helpers`** `{name, color, instructions, tool, connectors?, profile?}`: `201` with the helper. `connectors` is `[]` when left out; `profile` is the default profile's when left out. Its conversation is made with it, titled after its name, with no turn.

**`PATCH /v1/helpers/{id}`** with any of `name`, `color`, `instructions`, `tool` and `connectors`: `200` with the helper as changed. A new tool, instructions or web grant applies from the conversation's next turn; taking `github` or `tetrix` away also refuses those calls in a turn under way.

**`DELETE /v1/helpers/{id}`**: `204`. The helper goes, then its conversation as `DELETE /v1/conversations/{id}` deletes one: its turns and attached files go, offers made in it close and its sessions end. The jobs it started stay.

**Refusals.** The checks are the same on `POST` and `PATCH` (on `PATCH`, for each key sent). A refused request keeps nothing and makes no conversation.

| Status | When | `reason` |
|---|---|---|
| `400` | The body isn't a JSON object of these keys, has another key, or a key of the wrong JSON type (`connectors` not a list of strings) | |
| `422` | `name`, `color`, `instructions` or `tool` breaks its rule above (on `POST`, one left out too); a connector that isn't `github`, `tetrix` or `web`; a 51st helper; `profile` naming no profile on `POST`, or sent at all on `PATCH`; `PATCH` with no key | |
| `422` | A `cursor` helper without `web`: obid can't take Cursor's own web search away | `cursor-needs-web` |
| `404` | `PATCH` or `DELETE` of an id that isn't there | |
| `503` | `helpers.json` doesn't read | |

Each refusal is `{"error": "<one plain sentence naming the rule>"}`, with `reason` where the table says.

**A helper's conversation.** `GET /v1/conversations` shows it with `"helper": "<its id>"` (add-only, left out on every other conversation). Words reach it as any conversation's: `POST /v1/plans` with `"conversation": "<its id>"`. It isn't counted or forgotten among the 200 conversations obid keeps, and it's never the latest, so words with neither `conversation` nor `new` go elsewhere. `DELETE /v1/conversations/{id}` on it, and the brain's confirmed `oby_delete_conversations` naming it, are `409` with `reason` `helper-conversation`: it goes with its helper.

**Its turns.** A turn in it runs only on accounts of the helper's tool, as a conversation on that tool would (its lane, session, failover within the tool, stop, marks); with none that can run, it says why in one line. Its orientation gains a section naming the helper, its instructions and its connectors, with the vault's secrets taken out. Only its granted connectors are offered and allowed: without `tetrix`, no Tetrix server in its MCP configuration, `tools/list` on `/v1/agent/tetrix/conversation-<id>` answers `[]` and a `tools/call` there is refused; without `github`, `tools/list` on `/v1/agent/mcp/conversation-<id>` leaves out Oby's three GitHub tools and a call of one is refused (`isError: true`); without `web`, Claude Code's `--tools` leaves out `WebSearch` and `WebFetch`, and Codex runs with `-c web_search="disabled"`.
