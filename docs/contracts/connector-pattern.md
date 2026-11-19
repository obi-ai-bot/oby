# Connectors: one pattern every code host, tracker, knowledge source and agent follows

The rule every connector in obid follows, from [the template](../business-contract-template.md): how it is laid out, what it must offer, how it signs in, what it may send, the conformance suite it must pass, and how a change to one is made. It is binding: [core/AGENTS.md](../../core/AGENTS.md#from-outside) points every agent here ([why](../decisions/2026-11-19-one-connector-pattern.md)).

Settings › Connectors, the page that lists the owner's connectors, is a different contract: [connectors.md](connectors.md) (`CONNECTORS-n`). This one's scenarios are `CONNECTOR-n`.

Think of a power socket: every appliance has a different motor inside, but they all fit the same plug, and an electrician tests each one the same way before it is sold. Each connector here is an appliance; this page is the plug and the test.

## Feature and status

- **Outcome:** Oby can add a code host, an issue tracker, a knowledge source or an agent by writing one definition and one adapter, and the new one behaves like the others: it signs in the same way, it is recognized, mapped, paged and parsed the same way, its text reaches agents the same way, and what it sends is scanned the same way. A bug fixed once in the pattern is fixed for every connector.
- **Status:** In force: the rules marked **In force** below (sign-in, a returned text is data, the signed Confirm, the add-only API), which obid already keeps for GitHub, Tetrix and the three agents. **Proposed**, not built: the layout, the definitions, the interfaces, the capability flags, the outgoing gate, the conformance suite, words from the definition, the Obysphere naming no service, and the recipe. Each proposed rule names the lane of wave 3 that builds it; a lane moves its scenarios to **Implemented** in the pull request that builds them.
- **Scope:** every connector of the four kinds below, today's and later ones. Outside it: what each connector does for the owner (its own contract: [github-work.md](github-work.md), [tetrix-url.md](tetrix-url.md), [agents.md](agents.md) and the like), Settings › Connectors ([connectors.md](connectors.md)), and the owner's own MCP servers and claude.ai connectors, which their Claude Code loads itself and Oby only lists.

### The four kinds

| Kind | Today | Later |
| --- | --- | --- |
| Code host (`CodeHost`) | GitHub | GitLab, Bitbucket, Azure DevOps, Forgejo |
| Issue tracker (`IssueTracker`) | GitHub Issues | Linear, Jira |
| Knowledge (`Knowledge`, MCP) | Tetrix | any MCP server |
| Agent (`Agent`) | Claude Code, Codex, Cursor | any ACP agent |

### Where the pattern comes from

The shape is T3 Code's: a *definition* and a *driver* per service, with a registry, in its packages [`source-control-core`](https://github.com/pingdotgg/t3code/tree/cdf5a6c0fc9f0ad68bc2bd462a5f7ce626fdd622/packages/source-control-core), [`source-control-definitions`](https://github.com/pingdotgg/t3code/tree/cdf5a6c0fc9f0ad68bc2bd462a5f7ce626fdd622/packages/source-control-definitions), [`source-control-testing`](https://github.com/pingdotgg/t3code/tree/cdf5a6c0fc9f0ad68bc2bd462a5f7ce626fdd622/packages/source-control-testing) and [`provider-core`](https://github.com/pingdotgg/t3code/tree/cdf5a6c0fc9f0ad68bc2bd462a5f7ce626fdd622/packages/provider-core) (read at `cdf5a6c`).

The shape alone is not enough. In T3 Code's history, about 45 to 55% of the commits to each host package are fixes (GitLab 55 of 100, Forgejo 43 of 87, GitHub 61 of 132, Azure DevOps 31 of 69, the shared core 49 of 110; Bitbucket, 20 of 56, is lower). The fixes repeat a few kinds of bug: recognizing a host (an SSH remote that isn't the web host, an Enterprise hostname), mapping states (merged, closed, "rebase required"), lists past 100, and parsing merges. Its testing package holds only two fakes, and nothing makes a new host behave like the others. So this pattern adds what T3 Code lacks: one conformance suite, seeded with those bugs, that every adapter must pass ("The conformance suite", below).

## Behavior

### Actors and ownership

- **The definition** (one per connector) says what the connector is: no I/O, only data.
- **The adapter** (one per connector, in obid) does the I/O: it runs the service's CLI or calls its API, and turns what comes back into the kind's types.
- **obid** decides everything else, as for any work ("Dispatcher, not harness" in [AGENTS.md](../../AGENTS.md#dispatcher-not-harness)): which connector a repository or task uses, when it signs in, what goes out and when, and what the owner is asked. Rules decide; no model call of obid's does.
- **The apps** show what obid answers, in Sangam's words, with the connector's own nouns from its definition.

### The rules

1. **One place per connector.** A crate with no I/O, `core/crates/oby-connectors`, holds the definitions, the interfaces, the capability flags and the conformance suite. Adapters live in obid, under `core/crates/obid/src/connectors/<kind>/<service>.rs` (`code_host/github.rs`, `issue_tracker/linear.rs`, `knowledge/tetrix.rs`, `agent/codex.rs`); `src/connectors.rs`, Settings › Connectors' list today, becomes their parent module. The crate starts no process, opens no socket and reads no file, so its tests run anywhere in milliseconds. **Proposed** (lane 0.4 creates the crate with the suite; phase A moves GitHub into it). (CONNECTOR-1)
2. **One definition per connector.** It names the connector: its id, kind, display name, nouns (pull request or merge request, issue or ticket), its web and API URLs, how a git remote is recognized as it, how it signs in, and its capabilities. A service's URL, host name or noun appears in obid's production code nowhere but its definition. **Proposed** (lane A1). (CONNECTOR-2)
3. **One interface per kind**, `CodeHost`, `IssueTracker`, `Knowledge` and `Agent`, each listed with what it must offer below ("What each kind offers"). An adapter offers its kind's whole interface; what its service can't do is a capability set to false (rule 4), never a missing function or a panic. Each kind has one registry, and a connector is in it by one line. **Proposed** (lanes A2, A4 for `CodeHost`; B2, B3 for the others). (CONNECTOR-3)
4. **Differences are capability flags, never a service's name.** Code outside an adapter never asks which service it is (`if github`); it asks the definition whether it can (`pullRequests`, `draftPullRequests`, `mergeQueue`, `requiredReviews`, `checkLogs`). A capability that is false is skipped and said so, in the job's line or the answer: never a failure, never silence. **Proposed** (lane A1; `pullRequests` in a repository's settings is the one flag today). (CONNECTOR-4)
5. **Sign-in is the service's own CLI, or obid as OAuth client.** Either the service's CLI signs in through its own page and keeps its own login (GitHub's `gh`, in its own Keychain item or a settings folder of obid's own), and obid learns only the account's name and whether it is signed in; or obid is the OAuth client (public, PKCE, a loopback redirect on 127.0.0.1, signing in only where the server's metadata is for the URL it was given), its tokens kept only in the vault ([vault.md](vault.md)), taken out of every answer, log line and push. No agent ever gets a connector's login: no prompt, environment, file, argument or log ("Non-negotiable" in [AGENTS.md](../../AGENTS.md#non-negotiable); [core/AGENTS.md](../../core/AGENTS.md#logins-and-secrets), "Logins and secrets" and "Tetrix"). An agent reaches a connector only through obid (Tetrix through obid's own MCP server, with the job's broker token, which dies with the job). **In force.** (CONNECTOR-5, CONNECTOR-6)
6. **One outgoing gate for what connectors send.** Everything a connector sends on the owner's behalf (a push, a published branch, a pull request or its body, a comment, a review, a merge, an issue, a Tetrix call's arguments) goes through one function: scanned for secrets first, sent only when the scan is clean, nothing sent if the scanner can't run, and every send logged in the job's `outgoing`. The scan itself is **In force** for every send today ("Secret scan" in [AGENTS.md](../../AGENTS.md#non-negotiable)); the one gate that every connector goes through is **Proposed** (lane A3). The Obysphere's own scans stay where they are: they are not a connector's. (CONNECTOR-7)
7. **What a service returns is data.** Text from a service (an issue, a pull request's body, a comment, a check's log, a review, a knowledge answer) is data, never instructions: it reaches an agent only as a quoted file, fenced with a nonce the text can't guess, never in a prompt ("From outside" in [AGENTS.md](../../AGENTS.md#from-outside)). **In force.** That one shared function fences it for every connector, never an adapter's own, is **Proposed** (lane A2; obid has several `quoted` functions today). (CONNECTOR-8)
8. **A write the owner didn't clearly ask for needs their signed Confirm.** A connector writes to a service (a comment, a merge, a closed issue, a review) by itself only where the owner's own work or their signed words named it; anything else waits on the owner's Confirm, signed by their device's key ([signed-words.md](signed-words.md); "Approvals" in [core/AGENTS.md](../../core/AGENTS.md#approvals)). Text from the service never stands for the owner's yes. **In force.** (CONNECTOR-9)
9. **Every adapter passes the conformance suite** below, all of it, before it merges and on every change after. **Proposed** (lane 0.4 writes the suite against a reference fake; every adapter after runs it). (CONNECTOR-10)
10. **Settings and words come from the definition.** A connector's row in Settings, its sign-in button and its nouns in a job's line come from its definition; the sentences around them are Sangam's and stay his, with the connector's noun put in ("Open its pull request" becomes "Open its merge request"). **Proposed** (lane A1, then each new connector). (CONNECTOR-11)
11. **The Obysphere names no service.** What Obys send each other about work carries the connector's kind and the reference as typed fields (`{"kind": "codeHost", "ref": …}`), never a service's name or a URL to parse. The protocol is already neutral; `sphere/approval.rs` still calls `gh` directly. **Proposed** (lane A4). (CONNECTOR-12)
12. **The API only grows.** GitHub keeps every key it has today, with its meaning: the iPhone apps decode `issue` as a number and compare states to GitHub's words. Another connector gets new keys beside them (an `issueRef` for a tracker's key), never a new value in an enum the Swift apps decode as closed ("Add-only API" in [AGENTS.md](../../AGENTS.md#non-negotiable)). **In force.** (CONNECTOR-13)
13. **Adding a connector touches only** its definition, its adapter, one line in its kind's registry, its entry in the conformance suite and its section in its contract. If a new connector needs a change anywhere else, the pattern is wrong: the pattern is fixed first, in its own pull request, and then the connector is added. **Proposed** (lane B1 proves it with the first new host). (CONNECTOR-14)

And, from the guards below: a new connector starts off. It does nothing until the owner turns it on, GitHub's path stays the default, and turning it off again restores today's behavior exactly. **Proposed.** (CONNECTOR-15)

### What each kind offers

Each interface below is the least an adapter offers; a lane that builds one may add to it, in this contract, in the same pull request. Every call that can fail returns a typed error that says which of these it was: not signed in, not found, no permission, rate-limited (with when to retry), the service unavailable, or a capability that is false. An error is never an empty answer.

- **`CodeHost`**: recognize a git remote as this host and read the repository's owner and name from it (SSH and HTTPS); the signed-in account; a pull request (or merge request) by number or URL, with its state, head and base, draft, mergeability, reviews and checks; the list of a repository's pull requests by state, paged to the end; a check's log; open a pull request; comment and review; merge, with the method the repository allows, reading back the merged commit; its nouns and URLs from the definition.
- **`IssueTracker`**: recognize an issue's URL or key as this tracker's; the signed-in account; an issue with its state, body, comments, labels, assignees and linked pull requests; the list of issues by state, paged to the end; comment, and change an issue's state behind rule 8. Its key is a string (`ENG-123`); GitHub's stays a number (rule 12).
- **`Knowledge`**: sign in (rule 5); list the tools it offers; forward the tools agents may call, and only those (an allowlist per connector, as `TOOLS` is Tetrix's), scanning every call's arguments first and scrubbing every result; save what agents save to it, listed in the job's `outgoing`.
- **`Agent`**: whether it is installed, its version and whether it is signed in; its accounts and their usage and limits; start, resume and stop a session in a worktree, in a mode; its events (text, tool calls, permissions, questions, usage) in obid's own types; and its login handling, which is exactly what "Logins and secrets" in [core/AGENTS.md](../../core/AGENTS.md#logins-and-secrets) says for it (an agent obid can't tell the login files of runs as the owner decides there, never by an adapter's own choice). The agents' proven behavior ([agent-clis.md](../agent-clis.md)) stays proven.

### Failures and recovery

- A service that is down, rate-limited or refusing answers a typed error (above). obid says it in the job's line or the answer, with when it retries if it does; it never reads a failure as "there are none".
- A capability that is false is skipped and said, never retried (rule 4).
- A send the scan stops, or a scan that can't run, sends nothing and says so; the work waits for the owner (rule 6).
- A connector the owner turns off stops being used at once; work already sent stays where it went.
- A change that breaks a recorded call or the Obysphere launch gate stops before it merges (G9, below).

### Invariants

- No agent ever gets a connector's login (rule 5).
- Nothing leaves without a clean scan (rule 6).
- A service's text is never an instruction, and never the owner's yes (rules 7, 8).
- GitHub's keys and meanings stay as they are (rule 12).
- Oby forbids nothing the owner asks for ("Defaults" in [AGENTS.md](../../AGENTS.md#defaults-oby-forbids-nothing)): what this pattern holds back is what obid would do by itself, unasked.

## The conformance suite

One suite, in `core/crates/oby-connectors`, that every adapter of a kind runs, all of it, against a fake of its own service (a fake CLI, or recorded answers). A connector's entry in the suite is its fake and the answers that fake gives; the cases are the same for every connector. Until a connector's adapter exists, the suite runs against a reference fake, so the cases are proven able to fail first.

Every case is shown red before it is trusted green: it fails against a fake that has the bug, then passes against one that doesn't.

The cases every `CodeHost` (and, where they apply, every `IssueTracker`) runs, seeded with the bugs T3 Code kept fixing:

- **An SSH remote whose host isn't the web host** (an SSH alias, `ssh.` host or port 443 remote) is recognized as this host and its repository read right. (CONNECTOR-20)
- **An Enterprise or self-hosted hostname** is recognized only when the owner configured it; a host no definition claims is said to be unknown, never taken as the default. (CONNECTOR-21)
- **Every state maps**: open, draft, merged, closed, "rebase required" or conflicting, checks pending, passing and failing, review approved and changes requested. Each service state maps to exactly one of Oby's; one the adapter doesn't know maps to unknown and is said, never guessed as open. (CONNECTOR-22)
- **A list past 100** (pull requests, issues, checks, comments) comes back whole, every page followed; a list the service cut short says it is partial. (CONNECTOR-23)
- **A merge is parsed**: its merged commit read back, and a merge message with credits (co-authors and trailers) kept intact. (CONNECTOR-24)

The cases every connector of every kind runs:

- **Secrets:** a send carrying a planted secret sends nothing and says why; a scanner that can't run sends nothing. (CONNECTOR-25)
- **Tokens:** no token in any answer, log line, error, agent environment, file or argument, even when the service echoes one back in an error. (CONNECTOR-26)
- **Fenced text:** service text that carries instructions, or a fake end-of-fence, reaches an agent only inside its quoted file, with the fence whole. (CONNECTOR-27)
- **Failure isn't emptiness:** rate-limited, down, signed out and no permission each answer their typed error, never an empty list. (CONNECTOR-28)
- **A false capability is skipped and said**, and its call never made. (CONNECTOR-29)

`Knowledge` and `Agent` add their own cases here in the lanes that build their interfaces (B3 brings ACP's suite on a fake agent).

## How a connector change is made

Every change to a connector, today's GitHub, Tetrix and agents included, is made under these guards. A change that can't meet them doesn't merge.

- **G1 · Record before moving.** Every `gh` call obid makes today, with its arguments and what it gets back, is recorded against a fake `gh`, and every API answer that carries GitHub data is snapshotted. A move must reproduce them byte for byte (lane 0.2). (CONNECTOR-30)
- **G2 · One conformance suite, seeded with T3 Code's bugs:** the suite above, which every adapter runs.
- **G3 · A move changes nothing, and proves it.** A pull request that moves code changes no behavior: the recordings (G1), obid's tests, the BLOK workflows and the Obysphere launch gate all pass unchanged. A behavior change goes in a pull request of its own, never mixed into a move.
- **G4 · The Obysphere launch gate runs on every change that touches obid.** GitHub approvals feed the Sphere (`sphere/approval.rs`), so a connector change can break it without touching its files.
- **G5 · The coverage gate runs on every pull request that changes a contract** (#2564), so a scenario left out of the suite's scope can't merge and turn `dev` red.
- **G6 · One lane at a time where files are shared.** Lanes running together share no files; a lane that would share one waits. No stacked pull requests: a squash merge breaks the stack.
- **G7 · New connectors start off** (CONNECTOR-15).
- **G8 · A real-Mac check after each phase.** Oby.app is rebuilt and GitHub's whole path driven on a scratch repository: an issue to a task, its pull request, Fix CI, the merge. A phase isn't done until it passes.
- **G9 · The stop rule.** If a recording shows a difference a pull request didn't intend, or the launch gate turns red, that pull request stops: it is fixed or reverted before anything else merges.
- **G10 · Proven by driving it.** Every change is driven from outside before it merges, as [docs/verification.md](../verification.md) says: obid's behavior by a BLOK workflow against the real obid, the window by a BLOK workflow driving the real app, the Mac's own surfaces with agent-desktop, the Obysphere by the launch gate. Each check is shown red before green.

### Adding a connector

1. Write its section in its kind's contract, with its scenarios, and the owner's word that it is wanted.
2. Write its definition in `core/crates/oby-connectors`: id, kind, name, nouns, URLs, remote recognition, sign-in, capabilities.
3. Write its fake, and add it to the conformance suite. Run the suite: it fails, since there's no adapter yet.
4. Write its adapter in `core/crates/obid/src/connectors/<kind>/<service>.rs`, until the suite passes.
5. Add one line to its kind's registry, with the connector off by default (G7).
6. Run the scope report: nothing outside these files changed (rule 13). If something had to, stop and fix the pattern first.
7. Drive it from outside (G10), then on a real Mac (G8).

## Interfaces and dependencies

- **New:** the crate `core/crates/oby-connectors` (definitions, interfaces, capability flags, the suite); `core/crates/obid/src/connectors/<kind>/` (adapters).
- **Unchanged:** obid's API keeps every key ([runner-api.md](../runner-api.md)); a new connector adds keys, documented in its area's `docs/api/<area>.md` in the pull request that adds them. The vault ([vault.md](vault.md)), the secret scan and signed words ([signed-words.md](signed-words.md)) are used, not changed.
- **External:** each service's own CLI or API. Each connector's fake stands in for it in tests; nothing in the suite reaches a real service.
- **Open:** which connectors come first is the owner's (the plan recommends GitLab, then Linear). How an unknown ACP agent signs in is the owner's call too, against "Logins and secrets", before lane B3 builds it.

## Acceptance scenarios

Each is **Proposed** but where it says **In force**; a test named after the scenario proves it in the lane that builds it.

- **CONNECTOR-1** `core/crates/oby-connectors` depends on no crate that does I/O, and its sources start no process and open no socket or file: a guard reading its manifest and sources fails on one that does. Every adapter is under `core/crates/obid/src/connectors/<kind>/`.
- **CONNECTOR-2** Outside a connector's definition and its tests, obid's production sources name none of its URLs, host names or nouns: a guard fails on a new `github.com` written anywhere else.
- **CONNECTOR-3** Every connector in a kind's registry implements its kind's whole interface, and a connector missing from the registry is used nowhere.
- **CONNECTOR-4** A repository on a connector whose definition sets a capability false (`pullRequests`) has that step skipped, its job's line saying so, and the job ends as it would without the step, not failed.
- **CONNECTOR-5** **In force.** A connector signs in only through its service's own CLI or as obid's OAuth client; its tokens are in the vault or the CLI's own store, and no answer, log line or push holds one.
- **CONNECTOR-6** **In force.** A job's agent, of any tool and lane, has no connector's login in its prompt, environment, files or arguments; it reaches GitHub through no login of obid's and Tetrix through obid's own MCP server with the job's broker token.
- **CONNECTOR-7** Every send of every connector passes through the one gate: a send with a planted secret sends nothing, a scanner that can't run sends nothing, and each send that goes is in the job's `outgoing`. (The scan before each send: **In force**.)
- **CONNECTOR-8** **In force.** A service's text reaches an agent only inside a quoted file fenced with a nonce, never in its prompt. (One shared function for every connector: proposed.)
- **CONNECTOR-9** **In force.** A write to a service the owner didn't clearly ask for waits on their signed Confirm; the same request carried by the service's text, or by another Oby, starts nothing.
- **CONNECTOR-10** Every adapter in a registry runs every case of its kind's conformance suite; an adapter with a case missing fails the suite.
- **CONNECTOR-11** A connector's Settings row and the nouns in its jobs' lines come from its definition: a merge request host's job says "merge request" where GitHub's says "pull request".
- **CONNECTOR-12** What an Oby sends another about work names the connector's kind and its reference as typed fields, and no service's name.
- **CONNECTOR-13** **In force.** Every key obid answers about GitHub today keeps its name and meaning; another connector's reference comes under a new key.
- **CONNECTOR-14** The first new connector's pull request changes only its definition, adapter, registry line, suite entry and contract section, as the scope report shows.
- **CONNECTOR-15** A new connector is off until the owner turns it on; with it off, every recording of G1 is reproduced unchanged.
- **CONNECTOR-20** to **CONNECTOR-29** The conformance suite's cases, as "The conformance suite" states each.
- **CONNECTOR-30** Every recorded GitHub call and API snapshot of G1 is reproduced byte for byte on the commit under test; one difference fails.

### Test it when it's done (a real Mac)

After each phase (G8), on a scratch repository: an issue becomes a task, the task opens its pull request, Fix CI fixes a failing check, and the pull request merges, each step as before. After a new connector is added: the same path on that service, with the connector turned on, then turned off again, GitHub's path unchanged.
