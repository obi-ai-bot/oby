## What

The owner's own Cursor conversation on a Mac can now run commands. Until now, every Cursor conversation ran in Oby's box: commands were refused, the login was a copy, and every connection went through obid's proxy. Now the owner's own conversation runs Cursor as their terminal would:
- on the account's own folder and login, with no copy and no proxy;
- with no `--sandbox` flag of obid's, so its shell runs as the owner's user under the owner's own Cursor settings.

Think of it as a house key with one locked drawer. Cursor's own read and edit tools can't open obid's secret places (the vault, its keys, its token, other conversations' logins). obid writes those locks into the conversation's own room, in `.cursor/cli.json`, each time a session starts. The shell isn't held by those locks; that's the owner's own settings' job, exactly as it is today for an own Claude Code conversation's Bash.

| | Read and edit tools | Shell |
|---|---|---|
| The owner's own Cursor conversation on macOS (new) | denied obid's secret places by the room's `.cursor/cli.json` | the owner's user, under the owner's own Cursor settings: obid promises nothing |
| Another Oby's Cursor conversation | Oby's box | refused (CONVO-2) |
| Cursor's jobs | Oby's box (LANES-11) | refused |
| Cursor conversations on Linux | boxed, until measured there; they don't run today, since the box can't hold them to the proxy | — |

- **`jobs/lanes.rs`:** `conversation_own_cursor_launch` (unboxed, the account's own folder, the owner's shell environment); `cursor_room_settings`, which writes the room's `.cursor/cli.json` and never follows a link in its place; `cursor_denials`, which turns `conversation_secret_places` into `Read`/`Write` rules, plus a `Write` deny of the room's own `.cursor`.
- **`acp.rs`:** `cursor::own_argv`, `cursor-agent acp` with no `--sandbox`.
- **`brain/harness.rs` and `brain/harness_cursor.rs`:**
  - `own_conversation` puts Cursor in the own lane on macOS.
  - The launch, the socket-room check and the first prompt follow the lane; an own session hears that its shell is its own.
  - The mark stays: obid still can't see Cursor's reads.
- **Tests:**
  - The stand-in `fake-cursor-harness` gains `READ`/`EDIT`, which keep a project's `cli.json` denies as cursor-agent 2026.11.13 does.
  - Three new LANES-28 tests.
  - CONVERSATION-AGENT-6, 8, 9 and 41, which are about the box, now talk in another Oby's conversation (planted, as `conversation_codex.rs` does).
- **Docs:**
  - `docs/contracts/lanes.md`: the old line 69, the table, CONVO-2's list, and a new LANES-28.
  - `docs/contracts/conversation-agent.md`: the Cursor section and scenarios 6, 8 and 9 say which conversation is boxed.
  - `docs/cursor.md`: the measurements.
  - `AGENTS.md` and `core/AGENTS.md`: the lane lines, with their entries in `docs/agents-rules-manifest.md`. AGENTS.md's own rule is that each lane step changes those lines in the same PR.

## Why

The owner's decision of 2026-11-16 for own runs ([why](docs/decisions/2026-11-16-own-lane-trusts-the-cli.md)): an own run's file tools are denied obid's secret places, and its commands run as the owner's user unless the owner's own settings sandbox them. Plus the owner's "lanes match T3". T3 Code runs Cursor with Cursor's own sandbox as the only limit. The orchestrator chose this option (A, macOS first) on 2026-11-21, over B, a workspace read boundary. B also takes the owner's whole home away from every command (measured below), and Cursor can't express "everything but obid's home".

Gate 0 ("Dispatcher, not harness"):
- Which lane and folder a conversation runs in is the dispatcher's.
- The shell's limits stay the CLI's own.
- obid adds no state, endpoint, tool or model call. The room's `cli.json` is written fresh each session from what obid already knows.

## How

### How we proved it

**Red, then green** (macOS, on the Mac, alone: these Cursor tests need Seatbelt and are `cfg(target_os = "macos")`). Each red changes only the one guard:

| Broken on purpose | Result |
|---|---|
| Cursor never in the own lane (`dev`'s behaviour) | all three LANES-28 tests **fail**; the shell gets `reject-once` |
| the room's `cli.json` denies nothing | the file-tools test **fails**: the stand-in read `api-token` (`read: [redacted: api-token]`) |
| a link in the room's `.cursor` place followed (the link check and `scrub`'s `.cursor` both taken out) | the link test **fails**: a file written through the link |
| green | all 4 pass, CONVERSATION-AGENT-43 with them; all 13 Cursor conversation tests pass |

**The real cursor-agent 2026.11.13 on macOS**, on the owner's own login. Settings were only in the working folder's `.cursor/`, the owner's `~/.cursor` untouched, with a fake vault file outside the folder:

| `.cursor/` of the working folder | read tool | edit tool | shell, `--sandbox enabled` |
|---|---|---|---|
| `cli.json` `deny: [Read(<vault>/**), Write(<vault>/**)]` | **refused** ("Permission denied") | **refused**, nothing written | `cat` **printed it** |
| `sandbox.json` `{"readBoundary": "workspace"}` (option B) | — | — | `cat` refused, but so was the owner's home (`~/.zshenv` → `~/.cargo/env`) |

So the contract claims what holds and no more: the file tools are denied, the shell isn't held by obid.

**Linux** (Azure obybuild, a fresh slot): RESULTS

**Checks before the push:** CHECKS

Task and scenarios: the lanes port, PR 2 ("Cursor conversations run commands"); LANES-28, CONVO-2, CONVERSATION-AGENT-6, 8 and 9.

Dispatcher, not harness: `dispatcher`.

Failure modes:
- Cursor's edit tool rewrites the room's `cli.json`: denied by its own `Write` rule, and the file is written fresh at each session.
- A link left in the room's `.cursor` place: `scrub` removes it before the run, and `cursor_room_settings` never writes through one (`lanes_28_a_link_in_the_rooms_cursor_place_is_never_followed`).
- The owner's user settings allow a read of the vault: a deny beats every allow in cursor-agent 2026.11.13.
- **Known limits, stated:**
  - The shell reads what the owner's Cursor settings let it, obid's home included, as an own Claude Code conversation's Bash.
  - Cursor matches rules on the path as given, so a link the shell made into obid's home could take the read tool past a deny. The shell could read that place anyway.
  - A later cursor-agent could change how it finds or matches `cli.json`. The deny is Cursor's to keep.
- The words a person sees about Cursor's skills ("Cursor runs in Oby's box here, without your skills.") are unchanged. Cursor's skills aren't read in either lane yet: a follow-up for Sangam's words.

Security path: **yes**, `AGENTS.md` and `core/AGENTS.md` (their lane lines). It needs two approving reviews of its exact head, or the owner's click (MERGING-13 to 16).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
