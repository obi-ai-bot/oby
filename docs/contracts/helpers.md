# Helpers: an Oby of its own for one kind of general work

The business behavior contract for helpers, from [the template](../business-contract-template.md). The API is [docs/api/helpers.md](../api/helpers.md); the code is `core/crates/obid/src/helpers.rs` (the record, its file, the routes and the pure check `may_use`), with the turn's part in `brain/harness.rs` (`run`, `mcp_config`, `claude_flags`, `orientation`), `brain/talk.rs`, `brain/harness_codex.rs`, `brain/harness_cursor.rs`, `brain/tools.rs` (`handshake`, `call`), `connections.rs` (`agent_mcp`), `tetrix.rs` (`converse`) and `conversation.rs`. Scenarios are numbered `HELPERS-n`; the tests that prove them are named after them (`helpers_<n>_…` in `core/crates/obid/tests/helpers_api.rs` and in `helpers.rs`'s unit tests), with the Kani proof `helpers_8_may_use_exactly_what_was_granted` in `helpers.rs` and the Quint model `docs/models/helpers.qnt`. Task A00-77. The Electron app shows them as Agents (HELPERS-12 to 20, task A00-78): `apps/electron/src/renderer/src/` `AgentsList.tsx`, `AgentSheet.tsx`, `helpers.ts`, `Sidebar.tsx`, `ChatView.tsx` and `Home.tsx`, tested in `test/agentsPanel.test.tsx`.

## Feature and status

- **Outcome:** The owner has a named assistant for one kind of general work, as OpenBot has: a name, a colour, what it helps with (its standing instructions), the tool it runs on and the connectors it may use, in one profile, with one ongoing conversation. Words sent to it run a turn on its tool, told its instructions, reaching only what it was given.
- **Status:** **Implemented** (every scenario unless it says it's proposed): obid, and the Electron app, which shows them as Agents (docs/design/oby-home/, screens 08 to 10, task A00-78). The iPhone and Watch apps don't show them yet. The words of the refusals and of the helper's section are obid's own, Sangam's to change.
- **Scope:** the record and its file, `GET` and `POST /v1/helpers`, `PATCH` and `DELETE /v1/helpers/{id}`, the helper's conversation, and its turns' tool, instructions and connectors. In the Electron app: Agents beside Projects in the side panel, the New agent sheet, and an agent's conversation (A00-78). **Left out, for later tasks:** a helper's routine and the line saying one ran; Gmail, Slack or any other connector; the iPhone and Watch screens; editing, recolouring, renaming or deleting an agent from the app; a gateway or phone-relay route; changing a helper's profile after it's made; scoping conversations, jobs or accounts to a profile (PROF-3).

## Behavior

### Actors and ownership

- **obid** owns the helpers (`home/helpers.json`, 0600), checks every change, makes and deletes each one's conversation, and decides each of its turns' tool, orientation and connectors. Every grant decision is `helpers::may_use(granted, asked)`: true exactly when `asked` is in `granted`.
- **Clients** send the routes on the Mac. The Electron app shows helpers as Agents (In the app, below); the iPhone and Watch apps don't yet.
- **The agent** (the user's own Claude Code, Codex or Cursor) runs the turn as for any conversation; obid reaches GitHub and Tetrix for it with its own login and the turn's token, as today.

### Triggers and prerequisites

- `POST /v1/helpers` makes one: `name`, `color`, `instructions` and `tool` are required; `connectors` is `[]` when left out; `profile` is the default profile's when left out and must name a profile in `home/profiles.json` when sent. It gets a lower-case UUID, `created` now, and its conversation, titled after its name, with no turn.
- `PATCH /v1/helpers/{id}` changes any of `name`, `color`, `instructions`, `tool` and `connectors`. Its profile never changes.
- `DELETE /v1/helpers/{id}` deletes it and its conversation.
- `POST /v1/plans` with `"conversation": "<its conversation>"` runs a turn in it.

### The checks (the same on `POST` and `PATCH`)

- `name`: one line of 1 to 100 characters after trimming, with no control or line-breaking character: the rule a conversation's title has on `PATCH /v1/conversations/{id}`.
- `color`: `#` and 6 hex digits.
- `instructions`: 1 to 4,000 characters after trimming.
- `tool`: `claude`, `codex` or `cursor`.
- `connectors`: each of `github`, `tetrix` or `web`; repeats collapse, and the list is kept as `github`, `tetrix`, `web`.
- A `cursor` helper has `web` (`422`, reason `cursor-needs-web`): Cursor's web search runs on Cursor's servers and obid can't take it away (conversation-agent.md, "Cursor").
- At most 50 helpers.
- A body that isn't a JSON object of these keys, has another key or a key of the wrong type is `400`; every other refusal is `422`, one plain sentence naming the rule. `PATCH` with no key, or with `profile`, is `422`; an unknown id is `404`. A refused request keeps nothing and makes no conversation.

### States and transitions

- **No file:** no helpers. **File kept:** the helpers as kept, across restarts.
- **Its conversation:** made with it; `"helper": "<id>"` in its summary. Like the conversation of other Obys' questions, it isn't counted or forgotten among the owner's 200, making it forgets none of theirs, and it's never the latest: words with neither `conversation` nor `new` go elsewhere. `DELETE /v1/conversations/{id}` on it, and the brain's confirmed `oby_delete_conversations` naming it, are `409` (`helper-conversation`), and it stays.
- **Deleted:** the helper goes and the turn under way in its conversation is cancelled; that conversation is refused every connector from then on (`Helpers::allows` fails closed for it, and no new turn starts there, since it goes with its helper). Then its conversation goes as `DELETE /v1/conversations/{id}` takes one: its turns and attached files go, the offers made in it close and its sessions end. The jobs it started stay.

### A helper's turn

- **Its tool.** The accounts tried are only its tool's, as `talk::accounts` orders them; the conversation's chosen account counts only when it's of that tool. No account of another tool is ever tried; with none that can run, the turn says why in one line. Everything else is a conversation's on that tool: its lane, session, failover within the tool, stop, untrusted marks and the Cursor rules.
- **Its instructions.** On every tool, the orientation (Claude Code's `--append-system-prompt`, Codex's instructions, Cursor's first prompt) ends with a section that names the helper, gives its instructions as the owner's standing instructions and says which of Oby's connectors it has, with the vault's secrets taken out; a vault that can't be read starts no turn. A waiting Claude Code started with other instructions or grants isn't given the turn.
- **Only granted connectors.** The turn reads its helper's grants as it starts, and each decision is `may_use`:
  - **`tetrix`:** obid's Tetrix server is in its MCP configuration only while Tetrix is usable and the turn was granted it. `/v1/agent/tetrix/conversation-<id>` answers its `tools/list` with `[]` and refuses its `tools/call` while the helper, as kept at that moment, lacks it.
  - **`github`:** `oby_github`, `oby_github_write` and `oby_merge` are left out of its `tools/list`, during a turn and for a waiting session, and a call of one is refused before its handler runs, while the helper, as kept at that moment, lacks it.
  - **`web`:** without it, Claude Code's `--tools` leaves out `WebSearch` and `WebFetch`, and Codex runs with `web_search="disabled"`. Cursor always has it.
- Taking `github` or `tetrix` away refuses those calls in a turn under way too; taking `web` away, and new instructions, apply from the next turn.

### Failures and recovery

| What happens | What obid does | What the owner sees |
|---|---|---|
| `helpers.json` doesn't parse | Logged; left as it is; answered as no helpers; `POST`, `PATCH` and `DELETE` answer `503` | No helpers until the file reads |
| `helpers.json` can't be written | The change is undone (a new helper's conversation goes with it); `500` | The request failed, nothing changed |
| The vault can't be read at a turn | No turn starts | The turn's one line saying why |
| No account of its tool can run | Nothing else is tried | The turn's one line saying why |

### Invariants and boundaries

- No vault secret reaches the tool: nothing new is put in a prompt, environment, file, argument or log but the helper's section, scrubbed, and connector choices that take things away. GitHub and Tetrix stay reached by obid with its own login and the turn's token. Every `/v1/helpers` answer goes through the `/v1` scrub.
- A helper's profile is one profile on this Mac, set once.
- API add-only: four new routes, `helper` on a conversation, the reasons `cursor-needs-web` and `helper-conversation`; no value is added to an enum the Swift apps decode as closed.
- Mac only: no helper route is on the gateway's allowlist.

### Assumptions and known limits

- **Connectors are Oby's, not the CLI's.** In the own lane the owner's own Claude Code or Codex loads the owner's own MCP servers and settings (CONVO-4, CODEX-2), and its commands keep today's network rules: a helper doesn't take those away.
- **The `github` connector is Oby's three GitHub tools.** Jobs a helper starts (`oby_dispatch`) are obid's own dispatch, with their own rules, and the rest of Oby's tools stay; the owner may want to tighten this.
- **Codex's switch-off value** (`web_search="disabled"`) is checked against the stand-in only; a real Mac's Codex helper without the web is a "Test it when it's done" item.
- **Cursor keeps the web**, since obid can't gate Cursor's own search.
- **The instructions are the owner's standing words**, set only through the loopback routes with the runner's token, as a routine's prompt is; they don't change whether a turn counts as the owner's signed word or as untrusted.
- **Limits picked here**, Sangam's or the owner's to change: names as conversation titles, instructions up to 4,000 characters, colours as `#rrggbb`, at most 50 helpers.

### In the app

- **The switch.** In the side panel's chats, under the profile's name, one switch shows Projects (Oby's row and the conversations, as before) or Agents; the arrow keys move between the two. The choice is the panel's, kept on this Mac as `oby.panelSide` (`projects` by default), across a restart. A helper's conversation is under Agents only: it's no row on Projects, Projects' search doesn't find it, and it's never Oby's row's line or where the app opens. Agents has no search: the search button shows on Projects only.
- **The list.** New agent first, then the open profile's helpers (every one when obid lists no profiles): those that need you under "Needs you" with their count, then the rest under "Agents", each group most recently moved first, one whose conversation isn't listed last. A row is the agent's Oby in its colour, its name, its line (what its turn is on, else its last words, blank when nothing is loaded) and one mark: orange when something in its conversation needs you (a question back, a Confirm, an offer, a limit, or one of its tasks), grey while it works, else when it last moved. The panel reads obid's list as it starts, each time Agents shows, and after a create; with none (or an obid without helpers, `404`, or not answering) it shows New agent and "No agents yet", with no error.
- **The sheet.** New agent asks for its Oby's colour (the first no agent of this profile has), its name, what it helps with, what it runs on (Claude Code by default) and what it can use (none by default). Create agent sends `POST /v1/helpers` with exactly `name` and `instructions` trimmed, `color`, `tool` and `connectors` (`github`, `web`), and `profile` only when obid lists profiles. Made, its conversation opens once obid's event brings it; refused, obid's sentence shows in the sheet, which keeps everything as it was.
- **Gmail and Slack** are drawn as on screen 10 but can't be chosen on obid, and are never sent; on the app's mock all four can be. Tetrix has no chip.
- **Its conversation.** It opens in the app's conversation view: its Oby in its colour beside its words and, with its name, in the bar (no ⋮: it goes with its helper), and the box reads "Message <name>…". Its steps, cards and question back are drawn as in any conversation, and words go to `POST /v1/plans` with its conversation.

## Interfaces and dependencies

- Persistence: `home/helpers.json` (0600), `{"helpers": [{id, profile, name, color, instructions, tool, connectors, conversation, created}]}`; a helper's conversation keeps `helper` in its own file.
- API: [docs/api/helpers.md](../api/helpers.md); `helper` and `409 helper-conversation` in [docs/api/conversations.md](../api/conversations.md).
- Profiles: `Profiles::id_of` ([profiles.md](profiles.md)).
- Model: `docs/models/helpers.qnt` checks the design (only granted connectors, no secret reaching the tool, one profile), not the code.

## Acceptance scenarios

- **HELPERS-1** (`helpers_1_make_list_restart`, `helpers_1_a_helper_s_json_has_exactly_its_keys`): `POST` of Inbox helper on `claude` with `["web","github","web"]` is `201` with its nine keys, the default profile and `["github","web"]`; `GET /v1/helpers` lists it; `GET /v1/conversations` lists its conversation with `helper` and its name as title; after a restart it's the same; `helpers.json` is 0600. **Implemented.**
- **HELPERS-2** (`helpers_2_refusals_keep_nothing`, `helpers_2_each_refusal_names_its_rule`): an empty, two-line or 101-character name, colour `blue`, empty or 4,001-character instructions, tool `gemini`, connector `gmail` and a 51st helper are each `422` with its reason; an unknown key or `connectors` that isn't a list of strings is `400`; after each, both lists are unchanged. **Implemented.**
- **HELPERS-3** (`helpers_3_one_profile`): an unknown `profile` is `422`; an existing one is kept; `PATCH` with `profile` is `422` and the profile is unchanged. **Implemented.**
- **HELPERS-4** (`helpers_4_change`): `PATCH` of each key is `200` with the change, under HELPERS-2's checks; `{}` is `422`; an unknown id is `404`; after `tool` becomes `codex`, the next turn runs on Codex. **Implemented.**
- **HELPERS-5** (`helpers_5_delete`): `DELETE` of its conversation is `409` `helper-conversation` and it stays; `DELETE` of the helper is `204`, both are gone and a job its conversation started stays; a second `DELETE` is `404`. **Implemented.**
- **HELPERS-6** (`helpers_6_its_tool_and_its_instructions`): with the Claude Code account full, a Claude helper's turn runs no Codex or Cursor and says why in one line; with room it runs on Claude Code, its `--append-system-prompt` holding its name and instructions; a Codex helper's turn runs on Codex with them in its instructions. **Implemented.**
- **HELPERS-7** (`helpers_7_only_granted_connectors`, `helpers_7_a_turn_under_way_when_its_helper_is_deleted_gets_no_connector`, `helpers_7_a_deleted_helper_s_conversation_gets_no_connector`, `helpers_7_a_call_during_its_helper_s_delete_gets_no_connector`): with Tetrix usable, a Claude helper with no connectors gets only the `oby` server and `--tools` without the web's; its `tools/list` (during a turn and waiting) leaves out the three GitHub tools and `oby_github` is refused; Tetrix refuses its turn's call; a Codex helper without the web runs with `web_search="disabled"`; given all three, its next turn has them all; a turn under way when its helper is deleted gets no `oby_github` or Tetrix call through, and a deleted helper's conversation is refused every connector, even one it was granted; a call that comes while the delete is being saved is refused too. **Implemented.**
- **HELPERS-8** (`helpers_8_may_use_is_true_exactly_for_what_was_granted`, Kani `helpers_8_may_use_exactly_what_was_granted`): `may_use` is true exactly when the connector is in the set, for all 8 sets and 3 connectors. **Implemented.**
- **HELPERS-9** (`helpers_9_no_secret_reaches_the_tool`): a vault value in a helper's instructions is in neither its turn's arguments nor its MCP configuration, nor in any `/v1/helpers` answer. **Implemented.**
- **HELPERS-10** (`helpers_10_cursor_keeps_the_web`): a `cursor` helper without `web` is `422` `cursor-needs-web`, on `POST`, on a `PATCH` taking `web` away and on one switching a helper without it to `cursor`; with `web` it's `201`. **Implemented.**
- **HELPERS-11** (`helpers_11_the_conversation_stays_and_is_never_the_latest`): with a helper made first and then more than 200 other conversations, its conversation is still there; after it had the latest turn, the latest conversation is another one. **Implemented.**

- **HELPERS-12** (`helpers_12_the_switch`): the panel shows Projects and Agents under the title. Agents shows the agents list; Projects shows Oby's row and the chats. The choice is kept as `oby.panelSide` and holds across a restart. The arrow keys move between the two, and Agents has no search. **Implemented** (`apps/electron/src/renderer/src/test/agentsPanel.test.tsx`).
- **HELPERS-13** (`helpers_13_needs_you_first_in_the_open_profile`): four helpers, three in the open profile (one needs you, one working, one idle) and one in another profile. The open profile's three are listed, "Needs you" `1` first with the needing one, then "Agents" with the working one (grey mark) and the idle one (its time). The other profile's helper is absent. **Implemented** (`agentsPanel.test.tsx`).
- **HELPERS-14** (`helpers_14_a_helper_s_conversation_is_not_among_the_chats`): a conversation with `helper` is not a row on Projects, is not found by Projects' search, and its words are not Oby's row's line. **Implemented** (`agentsPanel.test.tsx`).
- **HELPERS-15** (`helpers_15_the_sheet_sends_what_it_shows`): fill in the sheet, choose GitHub, The web and Codex, then Create agent. The request is `POST /v1/helpers` with exactly `{name, color, instructions, tool: "codex", connectors: ["github","web"], profile}`. On `201` the sheet closes and the new conversation opens. **Implemented** (`agentsPanel.test.tsx`).
- **HELPERS-16** (`helpers_16_gmail_and_slack_are_never_sent_on_obid`): on obid, Gmail and Slack are disabled, clicking them chooses nothing, and the request's `connectors` holds neither. **Implemented** (`agentsPanel.test.tsx`).
- **HELPERS-17** (`helpers_17_a_refusal_keeps_the_sheet`): `422` `{"error": "…", "reason": "cursor-needs-web"}`. obid's sentence shows in the sheet, the sheet stays open with the name, instructions and choices as they were, and the list is unchanged. **Implemented** (`agentsPanel.test.tsx`).
- **HELPERS-18** (`helpers_18_an_agent_s_conversation`): an agent's conversation shows the agent's face in its colour beside its words and in the bar with its name, and the box says `Message <name>…`. Words sent go to `POST /v1/plans` with its conversation's id. **Implemented** (`agentsPanel.test.tsx`).
- **HELPERS-19** (`helpers_19_an_agent_waiting_on_two_questions`): an agent whose conversation holds two Question cards is under "Needs you" with the orange mark, and its conversation shows both cards with their options and Later. **Implemented** (`agentsPanel.test.tsx`).
- **HELPERS-20** (`helpers_20_an_obid_without_helpers`): `GET /v1/helpers` answers `404`, or obid can't be reached. Agents shows "New agent", no rows and "No agents yet", no error shows, and Projects is as it was. **Implemented** (`agentsPanel.test.tsx`).

**Test it when it's done (real Mac):**

1. Make a helper on Codex with no connectors (`curl -X POST …/v1/helpers`), send it words that ask it to search the web, and see it say it can't.
2. Make one on Claude Code with `["github"]`, ask it for the pull requests waiting for your review, then `PATCH` its connectors to `[]` and ask again: it says it wasn't given GitHub.
3. In the app, switch to Agents.
4. Create an agent on Claude Code with The web, and see its conversation open.
5. Ask it something, and see its face, the "Message <name>…" box and its row's working mark, then its time.
6. Quit and reopen: Agents is still the side shown.
