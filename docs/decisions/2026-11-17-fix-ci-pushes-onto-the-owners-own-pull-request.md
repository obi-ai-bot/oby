# Fix CI pushes onto the owner's own pull request, as T3 Code does

- **Date:** 2026-11-17
- **Status:** In force
- **Replaced:** a Fix CI job on the owner's own pull request published a branch of its own (`oby/fix-<n>`) and opened a pull request into his, since obid sent a branch the remote already had only on the owner's signed Publish ([publishing.md](../contracts/publishing.md), PUBLISHING-13): "It never pushes onto your branch: when its check passes, it opens a pull request into it."
- **Changed by:** Nothing
- **Contracts and scenarios:** [task-conversations.md](../contracts/task-conversations.md) (TASKCONV-31), [publishing.md](../contracts/publishing.md) (PUBLISHING-13)

## The owner's words

> as simple as T3 Code; where a safety step adds friction, T3's way wins

The owner's standing word, as the session that ran this work relayed it on 2026-11-17.

## Where it is stated

- [task-conversations.md](../contracts/task-conversations.md), "Rules", and TASKCONV-31
- [publishing.md](../contracts/publishing.md), PUBLISHING-13, its exception for TASKCONV-31
- [github.md](../api/github.md), the `CI failed` offer's words

## Notes

Only the owner's own pull request: one GitHub lists as his (`author:@me`), on a branch of the repository itself, never a fork's. Oby's own pull requests get no Fix CI (the job that opened one follows its checks), and anyone else's pull request keeps the old way: a pull request into their branch, since pushing onto someone else's branch isn't the owner's to do by default.

The push is a fast-forward only. If the branch moved on GitHub since the job started, nothing goes out and the job says so; obid never force-pushes, rebases, merges or deletes that branch, and opens or edits no pull request, so the owner's own description stays his. Oby's review of the job doesn't run: the pull request's own checks run again on the push, and they are the judge. The job's `onto` is never set from the API.
