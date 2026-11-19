# The own lane trusts the CLI, as T3 Code does

- **Date:** 2026-11-16
- **Status:** In force
- **Replaced:** [2026-11-10-lane-6a.md](2026-11-10-lane-6a.md), [2026-11-11-lane-6b.md](2026-11-11-lane-6b.md), [2026-11-11-lanes-4a.md](2026-11-11-lanes-4a.md) and [2026-11-11-lanes-ask.md](2026-11-11-lanes-ask.md), for the owner's own Claude Code runs; the refusal of the owner's user settings that LANES-5 described, and the managed-hooks refusal of LANES-17
- **Changed by:** Nothing
- **Contracts and scenarios:** [lanes.md](../contracts/lanes.md) (LANES-4, LANES-5, LANES-14, LANES-17, LANES-22, LANES-23)

## The owner's words

> CAN WE STRIP OBID DOWN TO TRUST THE CLI EXACTLY LIKE T3CODE?

The owner's words, as lanes.md quoted them.

> DO THAT RIGHT NOW

The owner's words, as lanes.md quoted them.

## Where it is stated

- [lanes.md](../contracts/lanes.md), "Decisions" and LANES-22, LANES-23

## Notes

What the owner saw: one harmless key in their own `~/.claude/settings.json` stopped every own Claude Code run (#2423), and most of the day's failures were obid refusing setups the owner's own terminal runs. T3 Code runs the same CLIs with the account's folder (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`), the CLI's own login, the owner's whole environment and settings, and the CLI's own permissions as the limit.

For the owner's own Claude Code jobs and conversations this means:

- obid refuses no run for what the owner's settings, `.claude.json` or managed settings hold (hooks, `permissions.additionalDirectories`, `sandbox.excludedCommands`, allow rules, any `env` key);
- the owner's login shell's `CLAUDE_*` and `ANTHROPIC_*` variables all reach the run (LANES-4a's allow-list is gone); only the account's folder, which is the dispatcher's choice, and a launching Claude Code session's own variables stay out;
- Claude Code loads every settings source, user, project and local, as the terminal does (LANE-6a's `--setting-sources user` is gone);
- obid's `--settings` no longer turn the sandbox on, hooks off or every tool into an `ask` (LANE-6b, LANES-ASK): the owner's own sandbox, hooks and permissions decide, and obid answers only what Claude Code asks it, by the job's mode, as T3 Code answers `canUseTool`.

What stays, because they are the non-negotiables or the dispatcher's:

- no agent gets obid's vault: obid's `--settings` still deny the file tools obid's secret places (its vault, keys, token, logins, accounts, other conversations' logins), and give an agent no secret of the vault's;
- obid's own paths still publish nothing (LANES-2): an empty `GH_CONFIG_DIR`, git's no-publish settings and the clone's invalid push URL, pinned in `--settings`' `env`; obid publishes, after the secret scan;
- approvals still come only from the owner's own apps;
- the API only grows;
- the foreign lane (another Oby's work, `untrusted`, Oby's own review) stays in Oby's box exactly as it was.

What this widens, said plainly: an own Claude Code run's commands are no longer held by a sandbox of obid's. Unless the owner's own settings sandbox them, they reach what the owner's user can reach, as in the owner's terminal: their files, the network, the code hosts with whatever credentials the owner's own setup gives them, and obid's home, its vault file included, which only a command (not a file tool) could read. A token the owner puts in their own settings' `env` reaches the run, as an MCP server of theirs already does (LANE-12). An own run's leftovers that left its process group are no longer swept (LANES-16 needs the sandbox's tag) unless the owner's sandbox is on.
