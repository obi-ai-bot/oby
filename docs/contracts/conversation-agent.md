# Talking on any connected agent

The business behavior contract for which account a conversation talks through.

## Feature and status

- **Outcome:** The owner connects any number of accounts, sees each one's percentage in the chat window, picks which one Oby talks through with one click (or leaves it on Auto), and when that one fills Oby moves by itself, says so in a line, and the chat keeps going.
- **Status:** **Implemented**, on obid and in the Electron app, proven in the real app on real Claude Code, Codex 0.162 and cursor-agent 2026.10.01 (2026-11-02): each answers, knows the earlier turns and keeps its session; Cursor's `oby_jobs` runs, its own shell is refused, a job started from it runs; a full account moves. The Mac it ran on is one, on a debug build.
- **Scope:** A conversation's turns, and the turns nobody watches (a supervisor's, an event's). **Out of scope:** where jobs run (`defaultAccount`, routing, #1563), saying "talk on Codex" in words.

## What the design shows (proposed: Sangam's to change)

In the message box, by send, on Home and in a conversation: the chosen account's icon and an arrow, or the word "Auto" and an arrow on Auto, which has no icon, as in Cursor's picker, with a dot past 80% (yellow) and 90% or held full (red), as the composer's gauge. A click opens a list over it, where everything else is; each row names the tool, and the account too once its tool has more than one ("Claude · Personal"), and its percentage is the fuller of its week and its 5-hour session, the limit that hits first: "Auto" ("Whichever has room") first, then every connected account with its icon, who it is and that percentage ("Full" when held full, "No usage" with no figure), the chosen one checked, and "Add account" last. In a conversation a choice is that conversation's, at its next turn; on Home it's the setting a new conversation follows, the same as Settings › Agents' "New conversations talk through".

## Behavior

### Actors and ownership

- **obid** decides which account each turn runs on, moves it, keeps the choice and says what happened. Everything here is obid's.
- **The Electron app** shows obid's accounts and figures, and sends the owner's choice. It decides nothing: which account the picker names follows `talkAccount`, `talkedOn`, `movedOff`, `fullUntil` and `talkNext`.
- **`conversation.agent`** in `config.json` stays the fallback: the tool Auto looks at first, Claude Code when it's left out. It still takes `claude` or `codex` only.

### What a turn runs on

1. The conversation's own choice (`talkAccount`: an account's id or `auto`), else the setting's (`talkAccount` on `PUT /v1/settings/jobs`), else Auto. A choice whose account has gone is Auto.
2. The accounts it may try: signed in, not being changed, not marked full, of every tool that can talk; best first within a tool (#874). Those with room come first, whatever the tool; then those held back by their window or weekly reserve; Cursor last, whatever its usage reads (the owner's decision of 2026-11-03: a plan can go on past its included usage, docs/contracts/usage.md, USAGE-9), since its turn marks the conversation for good. Within each, `conversation.agent`'s tool comes before the others.
3. The chosen account first; else the one its session is on while that has room, so it keeps its session; then the rest as above, those of the first's tool first within each. At most three in one turn, at most two of a tool, so a full tool can't keep another from its turn.
4. An account whose turn hit its limit, or that couldn't run at all (its login, its box, its session failing twice), hands the turn to the next. One whose turn failed for what it was asked (too long for the model, a security rule that ended it) doesn't. Neither does one whose turn already did anything but read: started a job, merged, wrote on GitHub, ran a command, changed a setting, made a card. Given the same words, another agent would do it again. Reading is `oby_jobs`, `oby_job`, `oby_conversations`, `oby_inbox`, `oby_github`, `oby_read`, `oby_search`, `oby_list`, `oby_tasks`, `oby_routines` and `oby_shell_output`.
5. `accountFailover` off: one account only. A chosen account that can't run is said, and nothing else is tried.

### A session belongs to one agent

A session can't be picked up on another tool or account, so a turn there starts a new one with the earlier turns handed over, quoted, as after a lost session (the last 20, `first_prompt`). What the conversation read stays with it: `session.json`'s taint and its foreign mark are the conversation's, on whichever agent.

### Cursor

Cursor's own reads and edits never ask and no flag takes its tools away (docs/cursor.md), so Claude Code's and Codex's way of keeping a conversation's agent from reading its login and sending it out can't hold. Instead:

- A turn on Cursor counts from its start as having read something untrusted. Once its session has started, it marks the conversation for the owner's secrets, and that stays after it leaves Cursor; one that never started read nothing and leaves no mark. What the owner asks through Oby's tools there becomes a Confirm card, as after any untrusted read.
- Its only way off the Mac is obid's egress proxy, by a door of the conversation's own, to Cursor's own services (`tools_own`) and nothing else, with no owner's card. So it has no web of its own but what Cursor's servers do for it. Where the box can't hold it to the proxy (bubblewrap's, Linux), it doesn't talk: `talk-needs-egress-box`.
- Every command asks, and obid's rule refuses it (`oby_shell` is the conversation's shell). A call is an MCP tool by its title only when it's of no other kind: a command titled like one is a command.
- A Cursor that can't load its session (no `loadSession`) fails that attempt, and the next starts a new session with the earlier turns handed over.
- What it plants is gone before its next start: `.cursor`, `.grok`, `.cursorrules` and the rest in its room; its settings, sandbox file, hooks, MCP servers, rules, skills (`skills`, `skills-cursor`, `cloud-skills`, `~/.grok`) and a shell's start-up files in its `HOME`. That list is what cursor-agent 2026.10.01 loads; a later version may add names.
- A turn nobody watches never runs on it.
- Auto or a move can reach it unasked, only behind every Claude Code and Codex account the turn could try. The line then says what that costs.
- Known limits:
  - **What Cursor's servers carry.** Cursor's own web search runs on its servers, and reaches obid only as a `search`. A Cursor that's been talked into it by what it read can put what it read in a search: the room's memories, and its login copy's `accessToken` and `apiKey`. obid can't gate that, and hasn't seen it against a real `cursor-agent`. The login copy holds no refresh token and goes when the turn ends, which bounds it, and doesn't stop it. So the rest of the box and proxy keep what it read from any other host, not from Cursor's.
  - Cursor's MCP calls time out after about a minute (docs/cursor.md), so an `oby_shell` that waits longer for the owner's OK ends for it while the card stays.
  - A read it does isn't shown as its own step.
  - A chosen Cursor that can't run is tried first on every turn, twice, before the turn moves; the line says so once.

### Failures and recovery

Nothing can run: the turn says why in one line, naming what holds each signed-in account back, of every tool ("Claude Code · Personal hit its usage limit until …; Codex · Work is at 97% of its week"), or with none signed in why the first tool has none (docs/conversation-harness.md); the owner chooses another or adds an account. A choice survives the account filling: when it has room again, turns go back to it, in a new session.

### Invariants

- The API is add-only: no key or route renamed or removed, no new value in a closed enum. `talkAccount`, `talkedOn` and `talkNext` are new keys; older apps ignore them. No new gateway route: `PATCH /v1/conversations/{id}` and `PUT /v1/settings/jobs` were on its allowlist already, as the phones may change agents' settings.
- No box ever gets an account's folder: each run has a copy of the login, in its own folder, taken out when the turn ends (with no refresh token). The turn's MCP token goes over Cursor's stdin, in no argument or environment.
- Per-work taint, foreign marks and owners_word rules are as they were: nothing here clears one.

## Interfaces and dependencies

- `PUT`/`GET /v1/settings/jobs`: `talkAccount` (an id, or null for Auto; `auto` on `PUT` is null). `422` for an account there isn't, or one that can't talk here (`reason: talk-needs-egress-box`).
- `PATCH /v1/conversations/{id}`: `talkAccount` (an id, `auto`, or null to follow the setting), same `422`s. A conversation's JSON gains them once there's something to say: `talkAccount`, `talkedOn`, the account its last answer came from, and `movedOff`, the account its turns have moved off, gone when one answers from it again or the owner chooses again. Each goes out in the `conversation` event, `talkedOn` with the answer's.
- `GET /v1/status`: `talkAccount`, and `talkNext`, the account a new conversation's first turn would use now. obid works it out off its runtime, since whether an account can run can ask the macOS Keychain.
- Kept in `home/settings.json` and the conversation's own file, across restarts. Cursor's folder: `home/logins/t-<8 of its id>`, short for its sockets, gone with the conversation.

## Acceptance scenarios

- **CONVERSATION-AGENT-1:** The setting (Auto or an account) and a conversation's choice outlive a restart. An account there isn't is `422`. A conversation with no choice follows the setting; `auto` is Auto whatever the setting says.
- **CONVERSATION-AGENT-2:** A choice takes effect at the next turn, on another tool in a new session with the earlier turns handed over, saying nothing of moving. Auto then stays where the session is while it has room.
- **CONVERSATION-AGENT-3:** The chosen account fills, can't run, or was full when chosen: the turn is answered on another account, any tool, with a line that says so, once, until it answers on that account again or the owner chooses again. The choice stays. Cursor moves off the same way. A turn that already did anything but read stays where it is: nothing runs twice.
- **CONVERSATION-AGENT-4:** `accountFailover` off: one account only, across tools too, and the turn says why.
- **CONVERSATION-AGENT-5:** What a conversation read goes with it to another agent: its work starts marked untrusted there.
- **CONVERSATION-AGENT-6:** Cursor talks: in its box, one session per conversation picked up with `session/load`, its words streaming and never what a load replays, told what it is in its first prompt, with Oby's tools, and the turn's token only in `session/new`. One that can't load gets a new session with the earlier turns handed over.
- **CONVERSATION-AGENT-7:** A turn on Cursor counts as untrusted from its start, and that stays after it leaves. One that never started leaves no mark. One reached without the owner choosing it says what that costs.
- **CONVERSATION-AGENT-8:** A conversation's Cursor gets out through the proxy only, to Cursor's own services only; with no such box, choosing it is `422` with `talk-needs-egress-box`.
- **CONVERSATION-AGENT-9:** Cursor's commands are refused, what it plants is gone before its next start, its login copy goes when the turn ends, and its folder goes with the conversation.
- **CONVERSATION-AGENT-10:** A turn nobody watches never runs on Cursor, and runs on Claude Code or Codex, whichever has room.
- **CONVERSATION-AGENT-11:** An account that's removed is no choice any more: the setting and every conversation's choice are cleared, and that conversation talks on Auto.
- **CONVERSATION-AGENT-12:** The picker and its list show each account's percentage (the fuller of its week and its session), one held full "Full", one with no figure "No usage" (Cursor's when its read gives none), the dot yellow past 80% and red past 90%, red when held full, and every connected account under its tool.
- **CONVERSATION-AGENT-13:** The picker names the chosen account, at once; moved off it or on Auto, where it last talked, else what obid says is next, each only while it can run.
- **CONVERSATION-AGENT-14:** Settings › Agents offers Auto and every connected account for new conversations, and a new conversation's first turn runs on that choice.
- **CONVERSATION-AGENT-15:** Cursor with a figure and with room stays last on Auto, after every account held back, and first only when the owner chose it.
- **`conversation_agent_auto_goes_where_there_is_room`:** Auto takes an account with room before a held one, whatever the tool, and either before Cursor.
- **`conversation_agent_nothing_can_run_names_what_holds_each_account`:** with nothing to try, the turn names every signed-in account's hold, of every tool.

The order of accounts and what a turn counts as off are tested in `brain/talk.rs` (`conversation_agent_*`).

### Test it on a real Mac

1. With Claude Code and Codex connected: open a conversation, click the picker by send, pick Codex. The next answer comes from Codex and knows what was said.
2. Run `cursor-agent login`, pick Cursor. It answers, `oby_jobs` works, `oby_dispatch` starts a job marked untrusted, and a run of `echo hi` by its own shell is refused.
3. Fill an account (or mark it full): the chat keeps going on another, and says so once.
