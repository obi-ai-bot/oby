# A task named to a signed-out account is refused, not started to fail

- **Date:** 2026-11-18
- **Status:** In force
- **Replaced:** ACCOUNTS-20's "a job naming it still goes there" for an account the owner names that isn't signed in: such a task started, and its run failed a moment later saying why (ACCOUNTS-19)
- **Changed by:** Nothing
- **Contracts and scenarios:** [accounts.md](../contracts/accounts.md) (ACCOUNTS-20, ACCOUNTS-32, ACCOUNTS-35), [keychain.md](../contracts/keychain.md) (KEYCHAIN-3)

## The owner's words

> Yes, refuse it.

The owner's words of 2026-11-18, on #2487's open half, as the session that started this work relayed them.

## Where it is stated

- [accounts.md](../contracts/accounts.md), "Where a task goes", ACCOUNTS-20, ACCOUNTS-32 and ACCOUNTS-35
- [keychain.md](../contracts/keychain.md), KEYCHAIN-3

## Notes

A waiter who knows the kitchen is out of salmon says so when you order it, not after you've waited twenty minutes. obid already knows an account isn't signed in (`GET /v1/status`, `signedIn: false`); now a task the owner names it for, in `POST /v1/jobs`, gets a `422` saying so and what to do, as one naming a tool that isn't installed already did. A follow-up keeps its first task's account, so the owner's words to such a task that ended are refused the same way once that account is signed out.

The decision covers the owner naming the account, and nothing else: a conversation's dispatch, a routine's run, a review (another Oby's included) and the owner's words to a task whose account they didn't name, that carry a signed-out account, still start there, as Auto passes signed-out accounts over and ChatGPT's relay logins keep their own wait for the owner's yes (CHATGPT-20). An account whose agent only failed to start a moment ago (`cantRun`) still takes a task named to it.
