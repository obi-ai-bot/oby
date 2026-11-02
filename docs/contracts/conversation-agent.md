# Talking on any connected agent

The business behavior contract for which account a conversation talks through.

## Feature and status

- **Outcome:** The owner connects any number of accounts, sees each one's percentage in the chat window, picks which one Oby talks through with one click (or leaves it on Auto), and when that one fills Oby moves by itself, says so in a line, and the chat keeps going.
- **Status:** **Implemented**, on obid and in the Electron app, proven against stand-in agents. **Not yet run against a real `cursor-agent`** (CONVERSATION-AGENT-6 to 9): the login on the Mac it was built on had ended (`cursor-agent models`: "Authentication failed"). The owner's decision of 2026-11-02, in his words: "Oby should use any of the connected agents to talk. Show a way for people to click and select the conversation agent, like on MausBot. Put that on the chat window so we can choose which agent we want to use for talking. What I need is to easily connect my accounts, to connect any amount of them, to choose which to use, and to easily see the percentage. Easy to see, easy to use, easy to change, proactive to act." Replaces docs/conversation-harness.md's one configured agent, and #874's failover within one tool.
- **Scope:** A conversation's turns, and the turns nobody watches (a supervisor's, an event's). **Out of scope:** where jobs run (`defaultAccount`, routing, #1563), a Settings row for the default (the setting has its API; the menu changes one conversation), saying "talk on Codex" in words.

## What the design shows (proposed: Sangam's to change)

The chat window's header, on the right, a pill like MausBot's model pill: the agent, the account and its percentage ("Claude Code · Personal 38%"), with a dot yellow past 80% and red past 90% of its session or week, as the composer's gauge. A click opens a menu: "Auto" ("Whichever has room") first, then every connected account under its tool with its week's percentage and a meter, the chosen one checked, and "Add account" last (Settings › Agents). Cursor reports no usage, so it says "No usage reported", never a number. When the turn moved, Oby's answer ends in a line: "Personal hit its limit, so I moved to Codex · Personal." (or "Personal couldn't run, so I moved to …" when it wasn't its limit).

## Behavior

### Actors and ownership

- **obid** decides which account each turn runs on, moves it, keeps the choice and says what happened. Everything here is obid's.
- **The Electron app** shows obid's accounts and figures, and sends the owner's choice. It decides nothing: which account the pill names follows `talkAccount`, `talkedOn`, `fullUntil` and `talkNext`.
- **`conversation.agent`** in `config.json` stays the fallback: the tool Auto looks at first, Claude Code when it's left out. It still takes `claude` or `codex` only.

### What a turn runs on

1. The conversation's own choice (`talkAccount`: an account's id or `auto`), else the setting's (`talkAccount` on `PUT /v1/settings/jobs`), else Auto. A choice whose account has gone is Auto.
2. The accounts it may try: signed in, not being changed, not marked full, of every tool that can talk; best first within a tool (#874). Tools in order: `conversation.agent`, then the others, Cursor last (it reports no usage).
3. The chosen account first; else the one its session is on while that has room, so it keeps its session; then the others of the first's tool, then the rest. At most three in one turn, at most two of a tool, so a full tool can't keep another from its turn.
4. An account whose turn hit its limit, or that couldn't run at all (its login, its box, its session failing twice), hands the turn to the next. One whose turn failed for what it was asked (too long for the model, a security rule that ended it) doesn't, and neither does one whose turn already started a job.
5. `accountFailover` off: one account only. A chosen account that can't run is said, and nothing else is tried.

### A session belongs to one agent

A session can't be picked up on another tool or account, so a turn there starts a new one with the earlier turns handed over, quoted, as after a lost session (the last 20, `first_prompt`). What the conversation read stays with it: `session.json`'s taint and its foreign mark are the conversation's, on whichever agent.

### Cursor

Cursor's own reads and edits never ask and no flag takes its tools away (docs/cursor.md), so Claude Code's and Codex's way of keeping a conversation's agent from reading its login and sending it out can't hold. Instead:

- A turn on Cursor counts from its start as having read something untrusted, and marks the conversation for the owner's secrets. It stays marked after it leaves Cursor. What the owner asks through Oby's tools there becomes a Confirm card, as after any untrusted read.
- Its only way off the Mac is obid's egress proxy, by a door of the conversation's own, to Cursor's own services (`tools_own`) and nothing else, with no owner's card. So it has no web of its own but what Cursor's servers do for it. Where the box can't hold it to the proxy (bubblewrap's, Linux), it doesn't talk: `talk-needs-egress-box`.
- Every command asks, and obid's rule refuses it (`oby_shell` is the conversation's shell).
- What it plants is gone before its next start: `.cursor`, `.cursorrules` and the rest in its room; its settings, hooks, MCP servers, rules and a shell's start-up files in its `HOME`.
- A turn nobody watches never runs on it.
- Known limits: Cursor's MCP calls time out after about a minute (docs/cursor.md), so an `oby_shell` that waits longer for the owner's OK ends for it while the card stays; a read it does isn't shown as its own step.

### Failures and recovery

Nothing can run: the turn says why in one line, as before (docs/conversation-harness.md), and the owner chooses another or adds an account. A choice survives the account filling: when it has room again, turns go back to it, in a new session.

### Invariants

- The API is add-only: no key or route renamed or removed, no new value in a closed enum. `talkAccount`, `talkedOn` and `talkNext` are new keys; older apps ignore them. No new gateway route: `PATCH /v1/conversations/{id}` and `PUT /v1/settings/jobs` were on its allowlist already, as the phones may change agents' settings.
- No box ever gets an account's folder: each run has a copy of the login, in its own folder, taken out when the turn ends (with no refresh token). The turn's MCP token goes over Cursor's stdin, in no argument or environment.
- Per-work taint, foreign marks and owners_word rules are as they were: nothing here clears one.

## Interfaces and dependencies

- `PUT`/`GET /v1/settings/jobs`: `talkAccount` (an id, or null for Auto; `auto` on `PUT` is null). `422` for an account there isn't, or one that can't talk here (`reason: talk-needs-egress-box`).
- `PATCH /v1/conversations/{id}`: `talkAccount` (an id, `auto`, or null to follow the setting), same `422`s. Every conversation's JSON: `talkAccount`, and `talkedOn`, the account its last answer came from. A change sends the `conversation` event.
- `GET /v1/status`: `talkAccount`, and `talkNext`, the account a new conversation's first turn would use now.
- Kept in `home/settings.json` and the conversation's own file, across restarts. Cursor's folder: `home/logins/talk-<12 of its id>`, short for its sockets, gone with the conversation.

## Acceptance scenarios

- **CONVERSATION-AGENT-1:** The setting (Auto or an account) and a conversation's choice outlive a restart. An account there isn't is `422`. A conversation with no choice follows the setting; `auto` is Auto whatever the setting says.
- **CONVERSATION-AGENT-2:** A choice takes effect at the next turn, on another tool in a new session with the earlier turns handed over, saying nothing of moving. Auto then stays where the session is while it has room.
- **CONVERSATION-AGENT-3:** The chosen account fills, or can't run: the turn is answered on another account, any tool, with a line that says so, once. The choice stays. Cursor moves off the same way.
- **CONVERSATION-AGENT-4:** `accountFailover` off: one account only, across tools too, and the turn says why.
- **CONVERSATION-AGENT-5:** What a conversation read goes with it to another agent: its work starts marked untrusted there.
- **CONVERSATION-AGENT-6:** Cursor talks: in its box, one session per conversation picked up with `session/load`, its words streaming and never what a load replays, told what it is in its first prompt, with Oby's tools, and the turn's token only in `session/new`.
- **CONVERSATION-AGENT-7:** A turn on Cursor counts as untrusted from its start, and that stays after it leaves.
- **CONVERSATION-AGENT-8:** A conversation's Cursor gets out through the proxy only, to Cursor's own services only; with no such box, choosing it is `422` with `talk-needs-egress-box`.
- **CONVERSATION-AGENT-9:** Cursor's commands are refused, what it plants is gone before its next start, its login copy goes when the turn ends, and its folder goes with the conversation.
- **CONVERSATION-AGENT-10:** A turn nobody watches never runs on Cursor, and runs on Claude Code or Codex, whichever has room.
- **CONVERSATION-AGENT-11:** An account that's removed is no choice any more: the setting's cleared, and a conversation that chose it talks on Auto.
- **CONVERSATION-AGENT-12:** The pill and the menu show each account's percentage (its week, else its session), Cursor "No usage reported", the dot yellow past 80% and red past 90%, and every connected account under its tool.
- **CONVERSATION-AGENT-13:** The pill names the chosen account, at once; moved off it or on Auto, where it last talked, else what obid says is next.

The order of accounts and when a turn says it moved are tested in `brain/talk.rs` (`conversation_agent_*`).

### Test it on a real Mac

1. With Claude Code and Codex connected: open a conversation, click the pill, pick Codex. The next answer comes from Codex and knows what was said.
2. Run `cursor-agent login`, pick Cursor. It answers, `oby_jobs` works, `oby_dispatch` starts a job marked untrusted, and a run of `echo hi` by its own shell is refused.
3. Fill an account (or mark it full): the chat keeps going on another, and says so once.
