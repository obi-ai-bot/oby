# Words to a Claude Code task at work reach it in the turn under way, by default

- **Date:** 2026-11-17
- **Status:** In force
- **Replaced:** TASKCONV-14's "as its next turn" for a Claude Code task at work (the words waited for the agent's whole turn to end, then for Claude Code to start again), and R05-07's proposal that words steer a running job only with `steer: true` on `POST /v1/jobs/{id}/messages`, queue kept as the default
- **Changed by:** Nothing
- **Contracts and scenarios:** [task-conversations.md](../contracts/task-conversations.md) (TASKCONV-14, TASKCONV-29 to 31)

## The owner's words

> Keep it default

The owner's answer of 2026-11-17 to "Keep words to a Claude Code task on by default, or put them behind R05-07's opt-in flag?", in this session.

## Where it is stated

- [task-conversations.md](../contracts/task-conversations.md), TASKCONV-29 to 31
- [docs/api/jobs.md](../api/jobs.md), `POST /v1/jobs/{id}/messages`, "Running"

## Notes

Telling a working task something used to be a note left on a desk: the agent read it only after everything it was doing, then obid started Claude Code again from cold. On a scratch obid (six 15-second commands, words sent while the second ran) that was 108 to 122 s; delivered into the running Claude Code, the agent answered right after the command it was on (19.9 s and 34.7 s). Sangam's line for words to a working task already said "It'll pick that up after its current step". Codex's `turn/steer` (R05-07's other half) isn't built yet; Codex and Cursor tasks still hear words as their next turn.
