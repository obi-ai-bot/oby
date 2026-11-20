# The own lane on Linux too

- **Date:** 2026-11-19
- **Status:** In force
- **Replaced:** "Own on macOS only": the owner's own Claude Code conversation stayed boxed off macOS until the own lane was proven elsewhere (LANES-12, the "ponytail" in `brain::harness::own_conversation`)
- **Changed by:** Nothing
- **Contracts and scenarios:** [lanes.md](../contracts/lanes.md), LANES-12 and LANES-26

## The owner's words

Asked whether the own lane, which ran the owner's own Claude Code conversations on their own login only on macOS, should run the same way on a Linux computer:

> Extend the lanes to Linux too

The owner's words, as the orchestrator session relayed them, said on 2026-11-19 in the owner's own chat.

## Where it is stated

- [lanes.md](../contracts/lanes.md): LANES-12 (own Claude Code conversations on macOS and Linux) and LANES-26 (an own run reaches obid on loopback)
- AGENTS.md, "Dispatcher, not harness" (the two lanes) and "Logins and secrets"; core/AGENTS.md, "Claude Code in the own lane"

## Notes

- The lab proved it first (#2594, LAB-LANES-2 and 5, on an isolated Ubuntu 24.04 machine and on the Mac): an own launch is the same on both, and an own run reaches Oby's tools at `127.0.0.1`, never at `10.0.2.2`, the address only obid's Linux box reaches. Without that rewrite an own conversation on Linux couldn't reach Oby's tools ("failed twice", tetrix-lab, 2026-11-11), which is why it stayed boxed.
- Own Claude Code jobs already ran in the own lane on Linux; this brings the owner's conversations along. Codex's own lane ("Yes, match T3", the same day) is its own change.
- Still to come on Linux: LANES-16's sweep of what an own run's commands leave running (the lab's subreaper sweep, LAB-LANES-3).
