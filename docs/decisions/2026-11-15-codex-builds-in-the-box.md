# CODEX-BUILDS: the owner's Codex job that builds runs in Oby's box

- **Date:** 2026-11-15
- **Status:** Replaced by [2026-11-19-codex-like-t3.md](2026-11-19-codex-like-t3.md)
- **Replaced:** Nothing lanes.md names
- **Changed by:** [2026-11-19-codex-like-t3.md](2026-11-19-codex-like-t3.md)
- **Contracts and scenarios:** [lanes.md](../contracts/lanes.md) (LANES-18, LANES-19)

## The owner's words

> Build in Oby's box (Recommended): a Codex job that needs to build runs in Oby's box, which was measured to build fine and still hides your home. Changes the lane rule (LANES-18).

The owner's answer in the factory session of 2026-11-15, on task A00-65.

> CODEX-BUILDS ([why](../decisions/2026-11-15-codex-builds-in-the-box.md)): the owner's own Codex job that builds (`build`, `fix` or `test`) runs in Oby's box, in Bypass too, where it builds, while obid's home and the person's home stay hidden (LANES-19);

lanes.md's wording, not a quote.

## Where it is stated

- [lanes.md](../contracts/lanes.md), "Feature and status" (Status), the own Codex decision, LANES-18 and LANES-19
- AGENTS.md, "Dispatcher, not harness" (the foreign lane), and core/AGENTS.md, CODEX-1

## Notes

It amends [CODEX-1](2026-11-11-codex-1-bypass-only.md): an own Codex job in Bypass whose command builds couldn't resolve its own worktree under obid's permission profile, which hides the folders above it and has no way to show them by name only (codex-cli 0.163.0).
