# Files a released obid wrote

`tests/released_files.rs` starts today's obid on each folder here (docs/contracts/released-files.md, RELEASED-1 to 3). Each tag's folder holds:

- `jobs/`: three job files, `home/jobs/<id>.json`, each named after its `id`: one `done` with its branch, pull request number and URL, one `failed`, and one `queued` with a `waiting` object.
- `config.json`: the configuration that obid read, with two repositories and two agents with accounts. The test makes only its machine-specific keys its own (`home`, `listen`, each agent's `bin`, each account's `configDir`, each repository's `remote`, and `allowFileRemotes`); every other key is read as released.
- `usage.json`: the accounts' usage as that obid kept it (`home/usage.json`), which keeps the waiting job waiting (below).

Every value is made up: no token, login, person's path or real repository.

## Where they come from

No tag's obid was run: every file is written by hand from that tag's own structs, with the keys and shape they serialize, retired keys included (every `Option` field of `Job` without `skip_serializing_if` is present, as `null` when empty; those with it are left out when empty).

| Tag | Commit | `job.rs` | `config.rs` | `usage.rs` |
| --- | --- | --- | --- | --- |
| v0.4.1 | c0c02093d43dbaf5b2713140219861013030e066 | v0.4.1's | v0.4.1's | v0.4.1's |
| v0.4.2 | 129032928739dc43faf6342013dcf158e643d492 | v0.4.2's, the same as v0.4.1's | v0.4.2's, the same as v0.4.1's | v0.4.2's |
| v0.4.3 | 4af0651fd87f66ca4886ff7c9e4e624396780ee5 | v0.4.3's, the same as v0.4.1's | v0.4.3's, the same as v0.4.1's | v0.4.3's |
| v0.4.4 | 5c1ed2e39deeea8deefdc8bb36578792f2b7cac4 | v0.4.4's, the same as v0.4.1's | v0.4.4's, the same as v0.4.1's | v0.4.4's |
| v0.4.5 | 7a77f0c07a0db694c5eaaf6cd2b8d479745ec715 | v0.4.5's, the same as v0.4.1's | v0.4.5's (it ignores an unknown top-level key; the file's keys are the same) | v0.4.5's |

`job.rs` didn't change from v0.4.1 to v0.4.5, so the jobs differ only in their ids and dates. The configuration uses keys every one of these tags knew and today's obid has retired (`turnBudget`, `orchestratorDelay`, `guestRequestEvery`): v0.4.1 to v0.4.4 refused any key they didn't know, so none is there that they didn't. No tag from v0.4.1 to v0.4.5 is skipped. Tags before v0.4.1 are out of scope.

## The waiting job

Those tags had two kinds of wait: `cap` (a slot or memory) and `limit` (an account with room, with `reserved` naming the accounts whose weekly reserve held it). Today's obid starts both as soon as there's room: a `cap` wait at once, and a `limit` wait because a weekly reserve no longer holds work the owner asked for. So the waiting job is a `limit` wait for a Claude Code account, `tool: "claude"`, and `usage.json` has both Claude Code accounts at their limit (`fullUntil`) until 2100-01-01, as a tool's report would leave them: a reset far in the future, so it still holds whenever the test runs. The test signs those accounts in with a stand-in login, so their usage, not a missing login, is what keeps the job waiting.
