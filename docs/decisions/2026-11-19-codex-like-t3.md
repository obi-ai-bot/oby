# The owner's Codex runs as T3 Code runs it: own lane, every mode, Codex's own limits

- **Date:** 2026-11-19
- **Status:** In force
- **Replaced:** [2026-11-11-codex-1-bypass-only.md](2026-11-11-codex-1-bypass-only.md) (CODEX-1, "Yes, Bypass only"), [2026-11-12-codex-2.md](2026-11-12-codex-2.md) (CODEX-2, "Same as jobs", as to its Bypass-only and macOS-only parts and obid's profile), [2026-11-15-codex-builds-in-the-box.md](2026-11-15-codex-builds-in-the-box.md) (CODEX-BUILDS, "Build in Oby's box")
- **Changed by:** Nothing
- **Contracts and scenarios:** [lanes.md](../contracts/lanes.md), LANES-13, LANES-18, LANES-19 (retired), LANES-21

## The owner's words

Asked whether the owner's own Codex work should run as T3 Code runs Codex (the own lane in every mode, Codex's own permissions as the limit, no `oby_own` profile, builds not boxed) rather than in Bypass only, on macOS only, under obid's own permission profile:

> Yes, match T3

The owner's words, as the orchestrator session relayed them, said on 2026-11-19 in the owner's own chat, with "Extend the lanes to Linux too" ([2026-11-19-lanes-on-linux.md](2026-11-19-lanes-on-linux.md)).

## Where it is stated

- [lanes.md](../contracts/lanes.md): the two lanes' table, "Own Codex runs as T3 Code runs it", LANES-13, LANES-18, LANES-19 (retired), LANES-21
- AGENTS.md, "Dispatcher, not harness" (the two lanes) and "Logins and secrets" (Codex); core/AGENTS.md, "Codex"

## Notes

- **The mapping** is T3 Code's (`codexRuntimeModeTurnDefaults` in its CodexAdapterV2.ts, read at T3 Code 420d1b5900, and proven in the lab, #2594, LAB-LANES-1): Ask and Plan get `approval_policy = "on-request"` and the built-in profile `:read-only`; Accept edits and Sandbox `on-request` and `:workspace` (the working folder and the temporary folders, no network); Bypass T3's full access, a granular policy that asks nobody but for an MCP server's own question, and `:danger-full-access`. T3 sends `untrusted` for its approval-required mode; codex-cli 0.163.0 refuses that in its configuration ("no longer supported"), so Ask and Plan ask on request in a sandbox that writes nothing.
- **What goes:** obid's `oby_own` profile (its folder list, its hidden homes, its read-only git pointers), the worktree as an untrusted project, `features.hooks = false`, `notify = []`, the commands' `HOME` setting, the Bypass-only and macOS-only gates, and boxed builds. The repository's own `.codex/` and the owner's hooks and `notify` load as Codex loads them in the owner's terminal, as an own Claude Code run's settings do since [2026-11-16-own-lane-trusts-the-cli.md](2026-11-16-own-lane-trusts-the-cli.md).
- **What stays:** a ChatGPT login stays boxed (its run reaches OpenAI only through obid's relay); another Oby's, an untrusted job, Oby's review and Cursor stay boxed; obid still checks, before every thread, that the approval policy and profile Codex merged are its mode's (a managed layer can't widen them silently), and names both on every thread, new or resumed; a mode that lets a running own turn go less far ends that turn, which goes on under the new mode's settings in the same thread; its check gets no declared secrets (LANE-3a).
- **Measured** on codex-cli 0.163.0 with a scratch `CODEX_HOME` and no login (2026-11-20): for each mode's `-c` settings `config/read` answers that policy and profile from the session's flags, and `thread/start` with them in its `config` comes back with `approvalPolicy` and `activePermissionProfile` set to them, the granular policy included.
- **A consequence the owner accepted with it:** in Bypass an own Codex run's commands run with no sandbox, as the owner's user, as an own Claude Code run's do unless the owner's settings sandbox them; in Ask and Plan Codex's read-only sandbox reads what the owner's user can read.
