# Lanes: the owner's work on their own login, everyone else's in Oby's box

The business behavior contract for where a job's agent runs, from [the template](../business-contract-template.md). Scenarios are numbered `LANES-n`; the tests that prove them will be named after them.

## Feature and status

- **Outcome:** The work the owner asked for runs as it would in their terminal: their own Claude Code or Codex, on their own login, which refreshes itself, with their environment, gateways, settings, skills and MCP servers, under the CLI's own sandbox. Work another Oby asked for, or work marked untrusted, stays in Oby's box as it does today. AGENTS.md, "Dispatcher, not harness" (the owner's decision of 2026-11-05), names the two lanes; this contract says what each one does.
- **Status:** **Proposed**, not built. The owner's decisions of 2026-11-09 lead, and are followed exactly:
  - only another Oby's job or an `untrusted` one is boxed, not every foreign mark;
  - Oby's own review stays boxed;
  - own runs skip obid's egress proxy;
  - the check stays boxed, with no network and declared secrets as today (the setup step has the network);
  - own jobs load Claude Code's user, project and local settings, as the terminal does; foreign ones load none;
  - Tetrix is always wired;
  - Cursor stays boxed until a vendor sandbox is proven;
  - `claudeOwnSandbox` stays a key obid reads and ignores;
  - branches may land in the shared clone; a stash stays in the job's worktree.

  **What runs today:** every job runs in Oby's box (`sandbox.rs`) with a private copy of its login, obid's egress proxy, `--setting-sources ""` and `--strict-mcp-config`, except a Claude Code account listed in `claudeOwnSandbox` (opt-in, the owner's decision of 2026-10-26), which runs on its own folder under Claude Code's own sandbox; "Rules Oby never breaks" in AGENTS.md says what happens until each step lands, and each step changes those rules in the same pull request.
- **Scope:** which lane a job's and a conversation's agent runs in, and what each lane gives it: login, environment, settings, network, git, the check, a resume command. **Out of scope:** what Bypass and the other modes ask ([permissions.md](permissions.md)); how obid publishes, reviews and merges ([publishing.md](publishing.md), [merging.md](merging.md)); a Codex account obid signed in to ChatGPT itself ([chatgpt-sign-in.md](chatgpt-sign-in.md): its tokens never reach a run, in either lane); the words a person sees (Sangam's).

## Behavior

### Actors and ownership

- **obid** decides a job's lane when it launches the job's run, starts the agent there, keeps the lane on the job, and builds the resume command.
- **The agent's CLI** (Claude Code, Codex) enforces its own sandbox in the own lane and refreshes its own login.
- **The Electron app** shows the job as obid reports it, and its resume command. It decides nothing.

### The two lanes

| | Own lane | Foreign lane (today's) |
|---|---|---|
| Whose work | the owner's | another Oby's request, or a job marked `untrusted` |
| Sandbox | the CLI's own | Oby's box |
| Login | the account's own folder and Keychain item, refreshed by the CLI | a private copy with no refresh token |
| Environment | the owner's login shell's, less code-host credentials | obid's allowlist |
| Claude Code settings | user, project and local | none |
| MCP servers | the owner's (no code host's) and Tetrix | Tetrix only (none for another Oby's job) |
| Network | the CLI's sandbox, no obid proxy; Codex none | obid's egress proxy |
| Conversation shell | the CLI's own (Bash, Codex's shell) | obid's `oby_shell` |
| Cursor | Oby's box | Oby's box |

### Decisions

- **The lane comes from whose work it is, fixed at launch.** A job is in the foreign lane when another Oby asked for it (`Job.obi`) or it's marked `untrusted`; every other job is in the own lane. The other marks (`foreign` text from someone else's issue, `reached`, `unsigned`, `memory_from_others`) keep what they do today (reviews, no declared secrets) and don't move a job to the box. The lane is decided when a run launches and doesn't change while it runs: a mark that arrives during a run takes effect on the job's next run (a follow-up, Try again, the run after an answer), which launches boxed. A job that was foreign never goes back to the own lane. Oby's own review runs boxed in either case, as today (`review_may_run`).
- **Agents publish in neither lane.** obid publishes; an agent never pushes, opens or merges a pull request, or writes to a code host. In the own lane this holds by:
  - no code-host login: the run's environment leaves out `GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN`, `GITLAB_TOKEN`, `GL_TOKEN`, Bitbucket's tokens, `SSH_AUTH_SOCK`, `GIT_ASKPASS` and `SSH_ASKPASS`, whatever the login shell sets; `GH_CONFIG_DIR` is empty; git has no global or system settings and an empty `credential.helper`, as `own_sandbox_env` does today; the sandbox denies reads of `~/.config/gh`, `~/.ssh`, `~/.git-credentials` and `~/.netrc`;
  - no code host on the network: Claude Code's sandbox denies `CODE_HOSTS`, as today; Codex runs with no network at all;
  - **no code-host tools from the owner's settings:** an MCP server in the owner's Claude Code settings, `.mcp.json`, a plugin or Codex's `config.toml` whose name, command, package or URL is a code host's (GitHub's, GitLab's, Bitbucket's: `github-mcp-server`, `@modelcontextprotocol/server-github`, `api.githubcopilot.com/mcp`, and the like) never starts for an agent: Claude Code gets its tools as deny rules (`mcp__<server>__*`) in the settings obid passes on the command line, which only managed settings outrank, and Codex gets `mcp_servers.<server>.enabled=false`. MCP servers run outside the CLI's sandbox with their own tokens, so leaving them out is the only fence; the owner's other servers start (LANES-5);
  - obid's refusals of publishing commands (`policy::publishes`: `git push`, `gh pr`, `gh api` and the rest) go in the same command-line settings as deny rules, so an allow rule in the owner's settings never lets one through.

  In the foreign lane it holds as today: Oby's box, no login, `--strict-mcp-config`.
- **Own Claude Code runs on its own folder and refreshes itself.** `CLAUDE_CONFIG_DIR` is the account's folder (`~/.claude` for the default account, its `configDir` otherwise) and the login is the account's own Keychain item. obid writes no login copy for it and doesn't refresh it: Claude Code does, as in the terminal. Claude Code's sandbox keeps its commands out of obid's home, every other account's folder and the git pointers, as `own_sandbox_settings` does today. `claudeOwnSandbox` stays in `config.json` as a key obid reads and ignores: every own Claude Code job is in this lane, and a list there changes nothing.
- **Own runs get the owner's environment.** obid reads the owner's login shell's environment (`$SHELL -l`), as the terminal starts with it, and gives it to own runs, less the code-host credentials above and obid's own variables (`OBY_*`). So a gateway reaches the agent as the owner set it (`ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, `AWS_*`, `OPENAI_BASE_URL`), and a provider's model id (Bedrock's `us.anthropic.…`, Vertex's) is passed to the CLI as written, never mapped to another name. A job with no model picked passes no `--model`, so the owner's own default holds. obid reads the environment again when the owner's shell files change; a shell that doesn't answer in 10 seconds leaves own runs with obid's allowlist and says so on the job. The foreign lane keeps obid's allowlist (`process::sandbox_env`).
- **Own runs get the owner's settings, skills and MCP servers.** Claude Code gets `--setting-sources user,project,local` and no `--strict-mcp-config`, so it loads the owner's settings, skills, plugins, hooks and MCP servers as the terminal does, with obid's `obi-mcp.json` beside them. obid's command-line settings keep the sandbox on (`failIfUnavailable`, `allowUnsandboxedCommands: false`), its deny rules (above) and its permission mode; the owner's settings can't turn them off. Codex reads its own `config.toml`, skills and MCP servers from its `CODEX_HOME`. The foreign lane loads none of them, as today. **Tetrix is always wired**, as today: every job but another Oby's gets it, in either lane.
- **The own run's network, and its check.** Own runs skip obid's egress proxy: no `HTTP(S)_PROXY` of obid's. Claude Code's commands reach the network through its own sandbox, which obid's settings keep off the code hosts; the sites it asks about come to obid as any permission does and follow the job's mode. Codex has none (below). What the run reaches is still known: WebFetch, WebSearch, a site Claude Code's sandbox asks about past the package registries, and a call to an MCP server that isn't obid's mark the job `reached`, as the proxy does today, with the same consequences for its check (MERGING-31). **The check stays boxed** in Oby's box in both lanes, with no network (loopback only for a job no other Oby asked for, `check_keeps_loopback`) and declared secrets as today (`check_refuses_secrets`); the repository's setup step runs boxed with the network through the egress door, as today.
- **Own Codex runs on its own `CODEX_HOME` in `workspace-write`.** `CODEX_HOME` is the account's folder (`~/.codex`, or its `configDir`), its login the account's own `auth.json`, refreshed by Codex. obid sets `sandbox_mode = "workspace-write"` with `sandbox_workspace_write.network_access = false`, and the writable roots: the worktree and the worktree's own git folder (`<clone>/.git/worktrees/<name>`), which commit and stash need; `/private/tmp` is writable in `workspace-write` already. **Its network is off** because the spike of 2026-11-09 showed a command in `workspace-write` can read `~/.codex/auth.json`, refresh token included: with no network it can't send it anywhere, and the secret scan before publishing looks for the account's own tokens, read from its `auth.json` at scan time, in the change. Approvals still come to obid (`approvalPolicy`, as today).
- **Stash and branches work; hooks and config stay closed.** In the own lane:
  - `git stash` and `git stash pop` work, and the stash stays in the job's worktree: its entries live in the worktree's own git folder, never in the clone's shared `refs/stash`, so a job never sees or pops another job's or the owner's stash;
  - `git branch`, `git switch -c` and `git checkout -b` work, and the branch may land in the shared clone's refs; what obid publishes and what it bases a job on come from its own records and the remote's refs, never from a branch an agent moved there;
  - the clone's hooks (`hooks/`, `core.hooksPath`), its `config` and `config.worktree`, and the pointers (`.git`, `commondir`, `gitdir`) stay read-only, so nothing an agent writes runs later in obid's or the owner's git. obid's own git calls keep `git::SAFE`.

  The foreign lane keeps today's: a detached HEAD, the clone read-only, objects in the job's own folder.
- **A resume command.** An own job with a session shows the command that continues it in the owner's terminal: `cd '<worktree>' && claude --resume <session>` (with `CLAUDE_CONFIG_DIR='<folder>'` first for an account that isn't the default) or `cd '<worktree>' && codex resume <session>` (with `CODEX_HOME`). obid gives it to the app on this Mac only: the gateway leaves it out, as it does `session`. A foreign job has none: its session lives in Oby's copy of the login, which the terminal can't open.
- **Foreign work is unchanged.** A job another Oby asked for or marked `untrusted` runs exactly as every job runs today: Oby's box, a private login copy with no refresh token, obid's egress proxy, obid's allowlisted environment, `--setting-sources ""` and `--strict-mcp-config`, Codex's `danger-full-access` inside the box, the conversation's `oby_shell`.
- **Cursor stays boxed**, in both lanes, with its login copy as today, until a sandbox of Cursor's own is proven to hold what Claude Code's and Codex's do (commands kept off obid's home, other accounts' folders, the git pointers and the code hosts). Proving it is a change to this contract.
- **Own Claude Code conversations get their own Bash.** A conversation the owner started runs its Claude Code turns with Claude Code's own Bash, under its sandbox, writing only the conversation's room and its temporary folder, instead of `oby_shell`. A conversation that has read something untrusted (`read_untrusted`), or one with another Oby, runs its next turns foreign: `oby_shell`, as today.
- **Own Codex conversations get their own shell.** The same for Codex: its shell tool on, in `workspace-write` on the room, with no network (Codex's rule above).

### Failures and recovery

- Claude Code's sandbox unavailable (`failIfUnavailable`): the run fails before its first command and says so; obid never runs it unsandboxed or moves it to the box by itself.
- A Codex too old for `workspace-write`'s settings obid passes: the run fails and names the version it needs.
- An own account whose own login is signed out or expired: the run fails with the CLI's own words and how to sign in again ("run claude", "run codex login"); obid copies nothing and refreshes nothing.
- The login shell's environment unreadable: LANES-4's fallback (obid's allowlist), said on the job.
- obid restarts during a run: the run's lane is the one kept on the job; it's never decided again for that run.

### Invariants

- Agents publish in neither lane (LANES-2); obid publishes.
- A boxed job never gets an account's own folder (#655); only the own lane runs on it, and only under the CLI's own sandbox.
- Secrets in Oby's vault never reach an agent, in either lane. The owner's login shell's environment is the owner's, not the vault's.
- The secret scan, the merge rules (`merge::SECURITY_PATHS`, two reviews for an untrusted job) and the check's secret rules hold in both lanes.
- obid's API stays add-only: the lane and the resume command are new keys.

## Interfaces and dependencies

- `GET /v1/jobs/{id}` (and the job in its lists): proposed `lane` (`own` | `foreign`) and `resumeCommand` (a string, or `null`), added, never renamed; the gateway leaves `resumeCommand` out ([runner-api.md](../runner-api.md) gets them when built).
- `config.json`: `claudeOwnSandbox` read and ignored.
- Claude Code: `--setting-sources`, `--settings`, `--mcp-config`, its sandbox settings, its deny rules. Codex: `sandbox_mode`, `sandbox_workspace_write.network_access`, `sandbox_workspace_write.writable_roots`, `mcp_servers.<name>.enabled`, `codex resume`.

**Open questions:**

- Whether Claude Code in `-p` mode sends a sandboxed command's new site to the permission prompt tool, which LANES-6's `reached` needs; if it doesn't, obid lists only the registries in `allowedDomains` and an own run reaches no other site from a command.
- Whether a call to the owner's own MCP server should mark `reached` (LANES-6 says yes, the cautious reading of "as today"): with it, an own job that used one gets no declared secrets for its check.
- Whether own Claude Code loads the worktree's `CLAUDE.md` and `AGENTS.md` as the terminal does, or keeps today's quoting of the base branch's.
- How a code-host MCP server is recognised past the known names (LANES-2): a server the owner named otherwise, with a code-host token, would start.

## Acceptance scenarios

- **LANES-1** The owner's task, another Oby's request and a job marked `untrusted` launch: the first is `own`, the other two `foreign`; a job marked only `foreign` (someone else's issue text) or `reached` is `own`. A mark that arrives while an own run works leaves that run own, and its next run is `foreign`. Oby's review of an own job runs boxed.
- **LANES-2** In each lane, with the owner's settings giving Claude Code `Bash(git push:*)` as an allow rule and Codex a GitHub MCP server that can push and merge: the agent's `git push` and `gh pr create` are refused, the GitHub MCP server doesn't start (its tools aren't offered), `GH_TOKEN` and `SSH_AUTH_SOCK` set in the login shell aren't in the run's environment, and the job's branch reaches GitHub only when obid publishes it.
- **LANES-3** An own Claude Code job on the default account runs with `CLAUDE_CONFIG_DIR` at `~/.claude` and no login copy in the job's folder; with its token in its last minutes, Claude Code refreshes it and the Mac's own `claude` still works afterwards. With `claudeOwnSandbox` listing nothing, or another account, the job is the same.
- **LANES-4** The owner's `~/.zprofile` exports `ANTHROPIC_BASE_URL`, `CLAUDE_CODE_USE_BEDROCK=1` and `ANTHROPIC_MODEL=us.anthropic.claude-…`: an own Claude Code job's environment has all three, unchanged, and passes no `--model`; a foreign job's has none of them.
- **LANES-5** The owner has a skill and an MCP server in `~/.claude`, a project setting in `.claude/settings.json` and a local one in `.claude/settings.local.json`: an own job sees all of them and Tetrix's tools; a foreign one sees Tetrix's only (another Oby's, none). The owner's `sandbox.enabled: false` doesn't turn the sandbox off.
- **LANES-6** An own Claude Code job's environment has no `HTTPS_PROXY` of obid's; its `curl https://example.com` follows the job's mode, and once it ran the job is `reached`; its `curl https://github.com` fails. Its check runs in Oby's box with no network, and gets declared secrets only as today.
- **LANES-7** An own Codex job runs with `CODEX_HOME` at `~/.codex` in `workspace-write`: it commits in its worktree, writes `/private/tmp`, can't write outside them, and its `curl https://example.com` fails; a change holding `auth.json`'s refresh token isn't published.
- **LANES-8** An own job runs `git stash`, `git stash pop`, and `git switch -c try`: all three work, the clone's `refs/stash` is unchanged, and `try` is in the clone's branches. Its writes to `.git/hooks/pre-commit`, the clone's `config`, `config.worktree` and `commondir` fail.
- **LANES-9** An own Claude Code job with a session shows `cd '<worktree>' && claude --resume <session>`; run in a terminal, it opens that session. An own Codex job shows `codex resume <session>`. A foreign job's `resumeCommand` is `null`, and the phone never gets one.
- **LANES-10** A foreign job runs exactly as before this contract: today's lane tests pass unchanged (the box, the login copy, the proxy, `--setting-sources ""`, `--strict-mcp-config`).
- **LANES-11** An own Cursor job runs in Oby's box with its login copy, as a foreign one does.
- **LANES-12** An own Claude Code conversation's turn runs `git status` in its room with Claude Code's own Bash, not `oby_shell`; after the conversation reads something untrusted, its next turn uses `oby_shell`.
- **LANES-13** An own Codex conversation's turn runs a command with Codex's own shell in `workspace-write` on the room, with no network.

**Test it when it's done (real Mac):** with Claude Code on the default account and a Bedrock gateway in the login shell, a task finishes on Bedrock and its resume command opens the session in Terminal; a Codex task commits and stashes with no card about its folders; another Oby's request runs boxed; GitHub shows no push but obid's.
