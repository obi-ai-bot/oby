# Parked: Cursor conversations run commands (LANES-28)

Parked 2026-11-21 by the owner's word, relayed by the orchestrator: at most 4 sessions, only the browser and
Artifacts now. The PR isn't opened; nothing is pushed. Branch `claude/cursor-own-conversations`, from `dev` at
2a6eeaf20; the work is commit cb0db7dc8, and this note is the commit after it (drop it before a PR).

## The decision (orchestrator, 2026-11-21): option A, macOS first
Like Claude Code's own lane (the owner's 2026-11-16 decision; "lanes match T3"). Conditions:
1. lanes.md's old line 69 says exactly what holds: the file tools are denied obid's secret places through the
   room's `.cursor/cli.json`; the shell runs as the owner's user under the owner's Cursor settings. Cite the
   2026-11-16 decision; no "proven sandbox" wording. (Done.)
2. Only the owner's own work. Another Oby's, untrusted ones and reviews stay boxed with their login copy. (Done
   for conversations. Jobs stay boxed, see "What's left".)
3. The owner's `~/.cursor` is never written; only the room's `.cursor/` is. (Done, with a link test.)
4. Tests: red to green for the deny (read and edit refused), and a test that the room's settings reach
   cursor-agent. Don't claim the shell can't read the vault. (Done.)
5. Linux Cursor stays boxed until measured; the contract says so. Don't sign anyone in on obybuild. (Done.)

## Done (cb0db7dc8)
- `jobs/lanes.rs`:
  - `conversation_own_cursor_launch`: unboxed, the account's own folder and login, no proxy, no `--sandbox`.
  - `cursor_room_settings`: writes the room's `.cursor/cli.json` and never follows a link.
  - `cursor_denials`: Read and Write of `conversation_secret_places`, plus Write of the room's `.cursor`.
- `acp.rs`: `cursor::own_argv`.
- `brain/harness.rs`: `own_conversation` puts Cursor in the own lane on macOS.
- `brain/harness_cursor.rs`: the launch, the socket-room check and the first prompt follow the lane.
- `tests/agents/fake-cursor-harness`: `READ` and `EDIT` keep a project's `cli.json` denies as cursor-agent
  2026.11.13 does.
- `tests/conversation_agent.rs`:
  - three LANES-28 tests;
  - CONVERSATION-AGENT-6, 8, 9 and 41 moved onto a planted conversation of another Oby's (`obid_boxed`);
  - `cursor_turns` also reads the account folder.
- Docs: `docs/contracts/lanes.md` (LANES-28 and the old line 69), `docs/contracts/conversation-agent.md`,
  `docs/cursor.md`, and `AGENTS.md` plus `core/AGENTS.md` with their `docs/agents-rules-manifest.md`
  entries. That makes it a security path: two reviews or the owner's click.

## Measured
- **Real cursor-agent 2026.11.13 on macOS**, the owner's login, with settings only in the working folder's
  `.cursor/`:
  - `cli.json` `deny` `Read`/`Write` stops Cursor's read tool ("Permission denied") and its edit tool
    (nothing written).
  - Its shell, even with `--sandbox enabled`, `cat`s the denied file.
  - A per-repo `sandbox.json` `readBoundary: workspace` refuses the shell's read outside the workspace, but
    the owner's home too.
  - Details are in the shared notes, `lanes-port-t3-cursor.md`.
- **Red then green on the Mac**:

| Broken on purpose | Result |
|---|---|
| Cursor not own | 3 LANES-28 tests fail |
| no denies | the file-tools test fails: the token was read |
| the link followed | the link test fails |
| green | all 13 Cursor conversation tests pass, and CONVERSATION-AGENT-43 |

- **Linux, obybuild** (same `lanes.rs`/`harness.rs` hashes as here; log in `.parked/linux-run.log`):
  - The conversation binaries, `brain::`, `jobs::lanes`, `acp::` and `proofs::`: 346 of 347 passed. The one
    failure, `conversation_harness a_new_conversation_finds_its_claude_code_started`, is a Claude Code test
    on a path this change doesn't touch. It wasn't rerun, so it's unchecked whether it's a flake: rerun it
    against `dev` first on resume.
  - The conversations obid-spec workflows: 68 of 68 passed.
  - Preflight's output was lost when the SSH connection to the VM dropped: run it again on resume.

## What's left
- On resume: rebase onto `dev`, then rerun the Mac Cursor tests (alone) and the Linux set.
- Fill RESULTS and CHECKS in `cursor-commands-pr-body.md`, add the `full-ci` and `full-suite` labels, open
  the PR and send the head.
- Cursor jobs stay boxed (LANES-11). A job's working folder is the owner's repository, where a `cli.json` of
  obid's would be committed or overwrite the project's: that needs its own decision.
- Cursor's skills still show "Cursor runs in Oby's box here, without your skills." in both lanes: words
  for Sangam, a follow-up.
- Linux: measure a signed-in cursor-agent there before giving it a lane.
