# An own Codex run's commands never read obid's secrets, but in Bypass

- **Date:** 2026-11-20
- **Status:** In force
- **Replaced:** Nothing
- **Changed by:** Nothing
- **Contracts and scenarios:** [lanes.md](../contracts/lanes.md), LANES-27 (and LANES-13, LANES-18)

## The owner's words

Asked about one consequence of "Yes, match T3" ([2026-11-19-codex-like-t3.md](2026-11-19-codex-like-t3.md)): with obid's own `oby_own` profile gone, an own Codex run's commands could read obid's home, its vault, keys, token and logins among them, where an own Claude Code run's file tools are denied those by obid's `--settings`:

> Merge, add guard after

The owner's words, as the orchestrator session relayed them, said on 2026-11-20 in the owner's own chat.

## Where it is stated

- [lanes.md](../contracts/lanes.md): "Own Codex runs as T3 Code runs it" (its commands never read obid's secret places, but in Bypass), "Invariants", LANES-27
- AGENTS.md, "Logins and secrets" (Codex); core/AGENTS.md, "Codex"

## Notes

- **The guard:** in Ask, Plan, Accept edits and Sandbox an own Codex run's profile is obid's `oby`, which `extends` the mode's built-in profile (T3's mapping stays: `:read-only` or `:workspace`) and denies reading the places an own Claude Code run's file tools are denied: for a job, obid's `vault`, `keys`, `api-token`, `logins` and `accounts`; for a conversation, the conversation's longer list. obid checks before every thread that Codex merged exactly that profile.
- **Bypass:** T3's full access is `:danger-full-access`, no sandbox at all, so nothing can deny a read there; the contract says so beside Claude Code's matching line (an own Claude Code run's commands run as the owner's user unless the owner's own settings sandbox them).
- **Measured** on codex-cli 0.163.0 (2026-11-20), on macOS (Seatbelt) and on Ubuntu 24.04 (bubblewrap): a profile extending `:read-only` or `:workspace` with those paths set to `none` refuses `cat` of each, keeps the worktree (inside obid's hidden home) readable and, for `:workspace`, writable; `*` and `**` in a path match on macOS (a folder's contents need `/**` when the name has a `*`), never on Linux, so obid also names each place a pattern matches when the run starts; on Linux a place denied together with one inside it (a folder and its own `/**`) keeps the app-server from starting a thread, so obid names each place once (measured again for #2698 with `thread/start`, which `codex sandbox` alone never ran); `config/read` reports each as `"deny"`, every key from the session's flags; a thread comes back on `{id: "oby", extends: <base>}`.
