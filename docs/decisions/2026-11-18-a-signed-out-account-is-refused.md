# A task named to a signed-out account is refused, not started to fail

- **Date:** 2026-11-18
- **Status:** In force
- **Replaced:** ACCOUNTS-20's "a job naming it still goes there" for an account that isn't signed in, and ACCOUNTS-32's rule that refused only an account whose CLI isn't on this Mac: a task named to a signed-out account started, and its run failed a moment later saying why (ACCOUNTS-19)
- **Changed by:** Nothing
- **Contracts and scenarios:** [accounts.md](../contracts/accounts.md) (ACCOUNTS-20, ACCOUNTS-32, ACCOUNTS-35), [conversation-agent.md](../contracts/conversation-agent.md) (CONVERSATION-AGENT-40)

## The owner's words

> Yes, refuse it.

The owner's words of 2026-11-18, on #2487's open half, as the session that started this work relayed them.

## Where it is stated

- [accounts.md](../contracts/accounts.md), "Where a task goes", ACCOUNTS-20, ACCOUNTS-32 and ACCOUNTS-35
- [conversation-agent.md](../contracts/conversation-agent.md), CONVERSATION-AGENT-40

## Notes

A waiter who knows the kitchen is out of salmon says so when you order it, not after you've waited twenty minutes. obid already knows an account isn't signed in (`GET /v1/status`, `signedIn: false`); now a task naming it gets a `422` saying so and what to do, as one naming a tool that isn't installed already did. A follow-up keeps its first task's account, so it is refused the same way once that account is signed out. An account whose agent only failed to start a moment ago (`cantRun`) still takes a task named to it.
