# Working on obid

This file adds to the root [AGENTS.md](../AGENTS.md) and never overrides it. It holds the detail the root's headings point to here, and is read in full by any change to obid, its lanes, logins, merging or publishing. Each heading is the root section the text came from.

## The launch build

- The iPhone reaches the Mac through the user's own Obysphere, sealed end to end with the Mac's key pinned from the QR: the Sphere passes sealed frames and never sees a body, a device token or a login.
- Once a conversation has read something untrusted (a file, Bash output, an attachment), the jobs it starts start at once, marked untrusted, as standing-rule jobs do, instead of a Start card for the rest of the session.
- They run in the owner's mode, Bypass too, not capped at Sandbox ([why](../docs/decisions/2026-10-27-bypass-after-an-untrusted-read.md)). Everything else the mark does stays: a clone apart, Oby's box, no declared secrets for a check written after a foreign read, the secret scan, and a second approving review of its exact head by another tool before it merges, with no card ([why](../docs/decisions/2026-11-14-own-work-merges-itself.md)).
- Issues, pull requests and comments written by the owner or members of their GitHub organization count as the owner's own words, not untrusted ([why](../docs/decisions/2026-10-24-org-members-words.md)). They never unlock declared secrets: a job on them needs one review and its check gets none (issue-tasks.md ISSUETASK-5, [why](../docs/decisions/2026-11-08-reviews-only.md)); text from anyone else stays untrusted.
- Conversation account failover is on by default: a turn that hits its account's limit moves to another signed-in account of the same tool with room ([why](../docs/decisions/2026-10-24-conversation-account-failover.md)). Settings has no row for it yet: the owner turns it off with `accountFailover` on `PUT /v1/settings/jobs`.
- A job at any account limit moves by itself, with no card, to the signed-in account with the most room of any supported tool (docs/contracts/account-limits.md, LIMITS-15, [why](../docs/decisions/2026-11-10-a-job-at-a-limit-moves.md)): a new session in the same worktree and branch hears the task, the earlier rounds' summaries and the owner's words, and its line says where it went and why.
- A review stays on its account, a job whose account the owner named stays there, and work in Oby's box stays in it on the new tool. `accountFailover` (above, on by default) turns the move off, and the owner moves any job by hand with `POST /v1/jobs/{id}/move` (LIMITS-16).
- With no account with room, a job waits by itself, with no card or ask, and resumes on the first account with room again: its own in its same session 90 seconds after the reset obid knows (LIMITS-5 to 14, [why](../docs/decisions/2026-11-02-a-job-waits-for-its-own-reset.md)), or another one freeing up ([why](../docs/decisions/2026-11-10-wait-when-no-room.md)).
- Its line says what it waits for, Stop (the owner's click) ends it for good, and the wait survives a restart. Jobs never ask about a limit; a pending #1563 decision an older release left resolves itself the same way. The budget, ceiling, untrusted mark, mode and box are unchanged.

## Carrying and merging

- Obys talk to Obys; Oby asks its owner only when something goes wrong: checks that still fail after its rounds, a conflict it can't resolve, a permission it may not grant, and, but for the owner's own work, a review asking for changes it can't settle or a repository whose workflow it hasn't learned yet. What the owner asks for in their own conversation or Tasks view merges by itself on any base, `main` too, with no card; past its review's last round Oby merges, saying what the review flagged, unless it needs two approving reviews ([why](../docs/decisions/2026-11-14-own-work-merges-itself.md)).
- Until it has learned a repository's, it takes the safe default: it opens the PR and, but for the owner's own work ([why](../docs/decisions/2026-11-14-own-work-merges-itself.md)), asks before any merge and doesn't push to or merge into the default branch or `main` on its own; it doesn't force-push them, nor a branch it didn't create, unless the owner tells it to ("Oby forbids nothing", above).
- A job whose agent only reached a site past the package registries (but a host that serves anyone's source code, such as GitHub, GitLab, Bitbucket, Codeberg or jsDelivr, which is someone else's code) isn't untrusted: its check gets none of the owner's declared secrets; it needs one review, not two; its check doesn't stand for the Mac's (MERGING-31, [why](../docs/decisions/2026-11-06-split-it.md), [why](../docs/decisions/2026-11-08-answers-on-merging-30-and-31.md)).

## Publishing

- Nothing goes out by itself unless the job's checks passed. By hand, the owner's signed Publish sends the commit it names, its check passed or not, to any branch they name, force-pushing only the commit they were shown there (docs/contracts/publishing.md, PUBLISHING-8 to 11).
- The check is the base's, run on the commit alone: the base branch's own copy of the check's script and `.gitleaks.toml`, in a clean checkout of exactly the commit that goes out (docs/contracts/publishing.md, PUBLISHING-6 to 8, [why](../docs/decisions/2026-10-30-the-check-is-the-bases.md)).
- Every outgoing action is listed in the job's report. Every job publishes by itself this way, one whose conversation read something Oby didn't write and one a standing rule started included ([why](../docs/decisions/2026-10-24-every-job-publishes-by-itself.md)).

## Approvals

- Every request that waits on the owner (a job's permission or question, a Confirm) notifies on the Mac with its answers as buttons and shows on Home, answered there in one click. For the owner's own work, these are only the agent's own questions, plan approval in Plan mode and permissions in Ask and Accept edits; after an outside read, a conversation does on GitHub only what their signed words named ([why](../docs/decisions/2026-11-14-own-work-merges-itself.md)).

## From outside

- A source an Obysphere admin connects for everyone (Sangam's Knowledge) works for every member as his design shows, but the admin's secret never leaves where it was connected: the admin's Oby, or Oby Cloud, holds the connection, runs the members' requests against it (as an Obi-to-Obi request, typed and quoted like any other) and returns only the answer. No member's Oby, agent or device ever receives the admin's secret.
- Every connector (a code host, an issue tracker, a knowledge or MCP connection, an agent) follows [docs/contracts/connector-pattern.md](../docs/contracts/connector-pattern.md) and passes its conformance suite before it merges ([why](../docs/decisions/2026-11-19-one-connector-pattern.md)).

## Logins and secrets

- Oby never reads, stores or forwards the user's agent logins but as below: each tool signs in through its own login.
- No box of obid's ever gets an account's own folder, or reads it (#655): every run gets its own folder with a login copy.
- No boxed run gets an account's other files that start programs or carry tokens (Codex's `config.toml`, Cursor's `mcp.json`); an own run's CLI reads the owner's own, as configured (LANE-12): agents have no code-host login of obid's. A `configDir` is never `~/.claude`, `~/.codex` or inside them.
- Oby uses a secret only for a connection the user made; secrets in its vault never reach an agent ("Non-negotiable").

Claude Code's Keychain login, for boxed runs ([why](../docs/decisions/2026-10-17-claude-keychain-login-copy.md), [why](../docs/decisions/2026-10-18-claude-keychain-on-by-itself.md)):

- With `claudeKeychain` on, obid copies Claude Code's own macOS Keychain login into a boxed Claude run's private login copy, as it copies a login file, because the sandbox denies the Keychain and Claude Code no longer writes a login file.
- obid turns `claudeKeychain` on by itself on macOS when it's left out of `config.json` and Claude Code's default login is only in the Keychain, asking `security` only whether the item exists; `false` keeps it off. Once on, set or by itself, it covers every Claude Code account, those with a `configDir` too, each with its own Keychain item.
- obid reads these logins with `/usr/bin/security`, the program Claude Code and cursor-agent write them with, so macOS doesn't ask. A value is asked for at most once per change of its Keychain item; one macOS refuses, or a prompt nobody answers, isn't asked for again until the tool's login changes, and the account says why (#1679, docs/contracts/keychain.md).
- The copy carries only what calls the API, never a refresh token or anything else the item holds (a boxed refresh would rotate the Mac's own login away).
- In a token's last 5 minutes obid has the Mac's own Claude Code refresh the default login, unboxed, by its absolute path, with a `PATH` of the system's folders first (then only what its binary needs to start, as its sign-in's: `node`'s folder for an npm install), in an empty config folder of obid's, right before a boxed run that needs it starts, then writes that run's copy. Nothing rewrites a running copy: a boxed job whose copy ends mid-run gets one more try, refreshed first (docs/contracts/keychain.md, KEYCHAIN-10).
- An account obid added itself (its folder is obid's `home/accounts/<id>`) is refreshed the same way, with its own Keychain item, and only while its folder's login files are the person's own and no link (#877, [why](../docs/decisions/2026-10-24-obid-added-accounts-refreshed.md)). An account with a `configDir` the configuration names is refreshed the same way, with the same checks.

Claude Code in the own lane ([why](../docs/decisions/2026-10-26-own-lane-for-claude-code-jobs.md), [why](../docs/decisions/2026-11-09-own-lane-by-default.md)):

- Every Claude Code job of the owner's own (not another Oby's, not `untrusted`, not Oby's own review) runs unboxed by obid, on its account's own folder and Keychain login, with no copy and no egress proxy (LANE-1). Its check gets none of the declared secrets once its run skipped obid's proxy (LANE-3a).
- Its commands and its Read, Edit and Write tools run as the owner's terminal runs them, under the owner's own Claude Code settings, sandbox, hooks and permissions ([why](../docs/decisions/2026-11-16-own-lane-trusts-the-cli.md)): obid refuses no run for what those settings hold, and its `--settings` only pin obid's own variables (its no-publish settings, the job's git folders) and deny the file tools obid's vault, keys, token, logins and accounts.
- obid answers only what Claude Code asks it about, by the job's mode and its refusal of paths outside the worktree; an allow rule, a skill's `allowed-tools` or a hook of the owner's lets a call through without asking, as in their terminal. A command the owner's settings leave unsandboxed reaches what the owner's user can, obid's home included: the owner's decision, for their own work only.
- On macOS, an owner's own Claude Code conversation runs the same way (LANES-12): on their own Claude Code login with no copy, its file tools denied obid's secret places, the stored threads and its room's `repos/`. Reading something untrusted keeps it in the own lane and marks it (CONVO-1); elsewhere it stays boxed until the own lane is proven there.
- Another Oby's conversation, Codex's and Cursor's (no shell, CONVO-2) and the brain's planner (CONVO-5) stay boxed with a login copy.
- obid's own git calls in a worktree are pinned to the job's git folders as obid checked them.

Codex ([why](../docs/decisions/2026-11-11-codex-1-bypass-only.md)):

- CODEX-1 (docs/contracts/lanes.md, LANES-18): every Codex job of the owner's own in Bypass on a `codex login`, on macOS (not another Oby's, not `untrusted`, not a review, not one that builds: LANES-19, nor the rest of a run its approved Plan card resumed, whose thread is in the box's `CODEX_HOME`), runs unboxed by obid, on its account's own folder as `CODEX_HOME` (its own `auth.json`, `config.toml`, skills and MCP servers, refreshed by Codex) and the person's own `HOME`, with no copy and no egress proxy.
- From its first such launch its check gets none of the declared secrets, whatever its mode, tool or account becomes, nor does its follow-up's (LANE-3a): obid saves that in the job's file before the run starts, starts none if it can't, and no later save of the job undoes it.
- Only its commands are kernel-sandboxed, in Codex's own sandbox under obid's permission profile: no network, writing only the worktree, its own git folder and its temporary folder (their `HOME`), reading none of obid's home, the person's home or any account's folder, its own included, but the job's own paths, off the git pointers. Codex itself and the owner's MCP servers it starts (LANE-12) run outside that sandbox.
- obid sets this with `-c` settings (`approval_policy = "never"`, no hooks, no `notify` program, the worktree an untrusted project), asks Codex before every thread, resumed too, whether they, its commands' `HOME` among them, are the ones that hold, and starts no run on settings that aren't its own (docs/contracts/lanes.md; docs/runner-api.md, "Codex in the own lane").
- With `approval_policy = "never"` Codex refuses a command's ask to run outside its sandbox (which obid couldn't tell from a plain one; codex-cli 0.162.1).
- A job whose mode leaves Bypass while it runs has its turn ended at obid's next one-second tick and goes on in Oby's box.
- CODEX-2, "Same as jobs" (LANES-13): the owner's Codex conversation in Bypass on a `codex login`, on macOS (not another Oby's), runs the same way, its commands writing only its room (never `repos/`) and its temporary folder, with the same check before every thread, new or resumed (docs/runner-api.md, "Its lane"). Every command it runs marks it as having read something untrusted, and if its mode leaves Bypass while a turn runs, that turn ends at obid's next check and its next turn runs in Oby's box.
- Every other Codex run on a `codex login` gets `auth.json` with `tokens.refresh_token` set to `""` in its own `CODEX_HOME` (OpenAI rotates refresh tokens too). A copy whose access token ends within 5 minutes doesn't run ("run codex once"), nor does one OpenAI refused, until the owner signs in again ("run codex login"), and obid never writes a token back.

Cursor ([why](../docs/decisions/2026-10-23-cursor-keychain-login.md), [why](../docs/decisions/2026-11-03-cursor-usage-from-api2.md)):

- A Cursor job's copy of its login file (kept as a file with `AGENT_CLI_CREDENTIAL_STORE=file`) holds only its `accessToken` and `apiKey`, never the refresh token.
- With `cursorKeychain` on, obid copies Cursor's own login (the account with no `configDir`, only: accounts obid adds sign in to a file of their own) from the macOS Keychain, where cursor-agent keeps it by default, into the run's private login copy: only its `accessToken` and `apiKey`, as for the file. It never asks for the refresh token, refreshes the login or writes anything back.
- obid turns `cursorKeychain` on by itself on macOS when it's left out of `config.json`, no `~/.cursor/auth.json` is there and Cursor's own login is in the Keychain, asking `security` only whether the item exists; `false` keeps it off.
- From that login or a Cursor account's own login file, obid sends Cursor's access token to `api2.cursor.sh`, and only there, to read the account's usage.

ChatGPT ([why](../docs/decisions/2026-11-02-chatgpt-sign-in.md)):

- ChatGPT is the one provider obid signs in to itself: a Codex account signed in with "Continue with ChatGPT", OpenAI's Sign in with ChatGPT for open-source and local apps, has obid as the OAuth client (PKCE, a loopback redirect on 127.0.0.1), its tokens in the vault as `oby.chatgpt.<account>` (0600), taken out of every answer, log line and push.
- No run gets any of its tokens: Codex reaches OpenAI through obid's relay with the job's broker token, which dies with the job. Claude stays on Claude Code's own login, as Anthropic's rules require, and Cursor is unchanged.
- An account signed in with `codex login` works as above, and none moves without the owner's click.

Guest jobs, retired with compute sharing, kept for its return ([why](../docs/decisions/2026-10-27-strip-to-a-dispatcher.md), [why](../docs/decisions/2026-10-21-guest-jobs-on-a-teammates-mac.md)):

- A guest job on a teammate's Mac (Obysphere sharing, #797) runs on the guest's own subscription, and the guest's Oby is the only place that login is used.
- The job's VM on the host gets a placeholder, the host's obid relays each model call over the job's end-to-end encrypted channel, and the guest's obid adds the credential on the guest's Mac, where the call leaves from. The login never reaches the host's disk, VM, logs or obid.
- The guest's obid takes only its agent's model paths and allowlisted headers, drops any credential the host sends, and stops relaying when the exchange ends or the job's `max_minutes` passes.

## Tetrix

- Tetrix is connected once, in Oby, for every agent: obid is Tetrix's OAuth client (a public client, PKCE, a loopback redirect on 127.0.0.1) ([why](../docs/decisions/2026-10-18-tetrix-connected-once.md)).
- obid keeps the login in its vault as `oby.tetrix.access` and `oby.tetrix.refresh` (`home/vault/secrets.json`, 0600, a Mac's too, [why](../docs/decisions/2026-10-30-vault-file-on-a-mac.md)), so it is taken out of every answer, log line and push. obid never reads the Keychain for its own secrets, so it asks for no password (#1679): a login the Swift runner kept there is connected again once.
- It signs in only where the server's metadata is for the URL it was given (RFC 9728's `resource`). The sign-in endpoints are loopback with the runner's token only, never on the gateway or reachable by another Oby.
- Agents never get Tetrix's token: every owner's job's agent (Claude Code, Codex and Cursor, every account) gets obid's own MCP server for Tetrix with the job's broker token, which dies with the job. obid forwards only the tools agents may call (`TOOLS` in core/crates/obid/src/tetrix.rs: never `resume_session`, admin, write or diagnostics tools) and scrubs every result.
- Tetrix's answers are data, never instructions. Being given Tetrix's tools doesn't mark a job, nor does a job's reach to its own tool's API hosts on port 443 (Claude Code's `api.anthropic.com`, Codex's `api.openai.com`, Cursor's `api2.cursor.sh` and the others `egress::tools_api` names) or Codex's `chatgpt.com`, by the CLI or by anything in its box ([why](../docs/decisions/2026-11-06-tool-api-hosts-dont-mark-a-job.md)).
- `chatgpt.com` withholds the declared secrets from that job's own check, and the vendors' other hosts, which serve pages people write, still mark it: two reviews. A hostile tool description doesn't force two reviews, an accepted risk (docs/contracts/merging.md, MERGING-30, [why](../docs/decisions/2026-11-08-answers-on-merging-30-and-31.md)); an answer a call brings still marks the job where it's read, as any MCP server's does.
- Everything obid sends Tetrix under Oby's login passes the secret scan first: every `tools/call` it forwards for an agent (its arguments), its own context queries, and the job's graph gaps; a finding, or a scan that can't run, sends nothing.
- The sessions agents save and the gaps obid sends are listed in the job's `outgoing`; every forwarded call is in its `connectionUses` (docs/runner-api.md, "obid only: Tetrix").
- The conversation's brain gets the same server's reads (`TOOLS` but `save_session`: a conversation keeps no `outgoing`) with its turn's token, good only while one of its turns runs (a turn nobody watches gets none). Each call passes the same scan, is recorded in obid's use log (`vault/uses.jsonl`), and makes the turn count as having read something untrusted.

## Proven on 2026-10-09 (keep these working)

- **Agents can't publish:** they run with an empty `GH_CONFIG_DIR` (no GitHub login) and Oby's clone has an invalid `remote.origin.pushurl`. Only Publish pushes: by itself when a job ends done with commits, or by hand (a click in the owner's own app, signed by the device's key), for any job they name, its check passed or not (Publish anyway, PUBLISHING-9), to the branch they name (PUBLISHING-10).
- **On Linux, agents run in bubblewrap** (`core/crates/obid/src/sandbox.rs`, proven 2026-10-10 on tetrix-lab): they run as obid's user, so home is empty but throwaway copies of the folders the CLIs live in, git's settings (read-only) and the job's own account, all of `OBY_HOME` is empty, and `/` is read-only. Real writes: the worktree, the clone's objects, refs, packed-refs and the worktree's own git folder, and the job's account home. Codex's own sandbox (bubblewrap too) can't nest under Ubuntu's AppArmor profile, so on Linux Codex runs `danger-full-access` with `approvalPolicy: "untrusted"` and Oby answers every command.
