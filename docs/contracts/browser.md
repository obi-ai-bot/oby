# Browser: a browser of each job's own, served by obid to every agent, the person in control

The business behavior contract for Oby's browser in obid: the engine that drives it, the API the app reads and the person acts through, and the tools every agent is given, from [the template](../business-contract-template.md). Proven first in the Blok lab (`tools/obid-spec/src/lab/browser.rs`, `browserapi.rs`, `browsersphere.rs`, `browser_contract.rs`; #2593, #2599, #2601, #2602), then ported. Its screens are the app's (`apps/electron`), from the approved design (https://claude.ai/artifact/Cs7t741EJKCD7JXNfzSRXb); the lab's are `tools/obid-spec/lab-ui/browser` (#2600).

## Feature and status

- **Outcome:** An agent can open, read and drive web pages in a browser of its job's own, while the person watches it live, takes control whenever they want, types a password the agent never sees, and keeps what the agent downloads apart from their own files. What a page says reaches the agent as quoted outside data, never as instructions. Another Oby's task gets a browser only inside Oby's box.
- **Status:** **Proposed.** Built and proven in the lab; the port into obid lands in the pull requests BROWSER-n names (contract, then the core, then the engine and API, then the agents' tools, then the Obysphere's rules). Each scenario says `(proposed)` until its test passes against the real obid. BROWSER-23 and 24 also wait for the owner's decisions on how the browser reaches a Mac and where it runs. The owner's decisions behind the rest: [the browser](../decisions/2026-11-19-the-browser.md).
- **Scope:** obid's `browser` module: agent-browser driven per job, the person's hold, refs, deadlines, outside data, downloads, the live stream, the API under `/v1/browser`, its events, the agents' MCP server, its gates (settings, lanes, the box's edge, the secret scan on what goes back through the Sphere). Outside it: the app's screens and words (Sangam's design; the next session's port), desktop control (agent-desktop, a contract of its own), another computer's browser (Remote computers, after the launch), and anything agent-browser itself does inside a page.

## Behavior

### Actors and ownership

- **obid** owns each job's browser (one agent-browser session and profile, its Chrome), every tab's state, the person's hold, the event log, asks, downloads, and the gates on every tool. Rules decide; no model call of obid's does.
- **Agents** (Claude Code, Codex, Cursor; own lane and foreign) call `oby_browser_*` tools on obid's MCP server with their job's broker token.
- **The owner's apps** read `GET /v1/browser`, its events and a tab's stream with obid's token, and act (take, give back, type a password, answer an ask, save a copy, change the settings) with the device's signed words (`X-Oby-Words`, [signed-words.md](signed-words.md)).
- **agent-browser** (0.43.0, pinned) drives Chrome for Testing; obid never uses the person's own Chrome, its profile or `--auto-connect`.

### Rules

- **On by default.** "Agents can use the browser" and "Agents can use the Mac" are on until the owner turns them off in Settings › General (`PUT /v1/browser/settings`, signed) ([why](../decisions/2026-11-19-the-browser.md)). With the first off, every tool refuses `browser_off`.
- **A browser of the job's own.** Each job's agent gets its own agent-browser session and profile (`--session`, `--profile`, `--download-path <profile>/Downloads`, `--no-auto-dialog`), its own `HOME` and socket folder under obid's home. A job sees and acts on only its own tabs (`not_your_tab`).
- **Started under its own allowance.** A job's browser starts on its first tool that reaches a page, within 60 s (`BROWSER_START_ALLOWANCE_MS`) apart from the command's own deadline; past it the answer is `browser_starting`, never a bare timeout.
- **The person's hold.** Only the owner's registered device takes a tab, with its signed words; while held, the agent's acts are refused (`human_control`) and its reads go on; only the holder gives it back. An act let through before a take never runs after it (`control_changed`); a take that lands while an act runs waits for it to end. Nothing is half-applied.
- **The person's input** on a tab's stream comes only from the device that holds the tab (`not_holder` from any other).
- **Stale refs.** A ref dies on navigation, a new snapshot or a handover; obid checks the URL right before each act that names one (`stale_ref`).
- **Deadlines.** Every command has one (15 s by default, `deadlineMs`); a command past it answers `timed_out` and is never replayed; its tab settles first, refusing calls (`settling`) rather than queueing them.
- **Outside data.** Page words, titles, dialogs, snapshots and `evaluate`'s value reach the agent only under `outside` with `marksTurnUntrusted`; reading them marks the turn as having read something untrusted ([why](../decisions/2026-11-19-the-browser.md)).
- **Passwords** go from the owner's device through `POST …/secret` straight into the page's field. The value is in no answer, event, log line or file obid writes, and the agent never gets it ([why](../decisions/2026-11-19-the-browser.md)).
- **Downloads.** An agent's download stays in its profile; Save a copy (signed) copies it to the person's Downloads and leaves the agent's; one the person starts is delivered ([why](../decisions/2026-11-19-the-browser.md)).
- **The live stream** sends the newest frame only: a slow reader drops frames, never queues them.
- **Events.** Every change is one event with a `seq`; `GET /v1/browser` and `GET /v1/browser/downloads` carry the last `seq` they reflect, so a reader never takes a list older than an event it applied. A reader reconnecting with `Last-Event-ID` gets what it missed while obid holds it (2000 events).
- **Every agent, both lanes.** Every job's agent is given obid's browser server, whatever its tool, account or lane, while the first switch is on. Every other outside MCP tool stays refused ([permissions](permissions.md), `policy::fixed_denial_as`).
- **The foreign lane.** A job's origin comes from obid's own records (the incoming exchange whose `job` is this job), never from what a request says. Another Oby's task, or one marked `untrusted`, gets a browser in Oby's box (`box-<job>`), apart from the owner's: it opens only `http` and `https` pages, never `file:`, browser-internal pages or the host's own services on loopback (`outside_box`), and its Chrome runs behind the box's egress proxy ([why](../decisions/2026-11-19-the-browser.md)).
- **Asks** go to the host's own devices only; another owner's device can't take a tab or answer one.
- **What goes back** through the Sphere passes the secret scan first; a finding, or a scanner that can't run, sends nothing with text ("Non-negotiable").

### Failures and recovery

- agent-browser or Chrome missing or not matching its pin: the tool answers `browser` with why, and nothing runs (BROWSER-23).
- A browser that died: its job's next tool starts it again under the allowance; its tabs are gone (`tabClosed`).
- A job that ends: its token dies, its browser and every process it started stop, its tabs leave the list.
- obid restarting: browsers left from before are stopped; no tab survives a restart.

## Interfaces and dependencies

- **API:** [docs/api/browser.md](../api/browser.md). Every path is the lab contract's (`browser_contract.rs`), add-only from there on.
- **The agents' server:** `POST /v1/browser/mcp` with the job's broker token; the server is named `browser`, its tools `oby_browser_*`, permission rules `mcp__browser__<tool>`.
- **Events:** the browser's own stream, `GET /v1/browser/events`; nothing new on `/v1/events`.
- **Persistence:** none across restarts but the two settings (obid's settings file) and each job's profile folder until the job is cleaned up.
- **External:** agent-browser 0.43.0 and Chrome for Testing, each pinned by SHA-256 per platform.

## Acceptance scenarios

Each becomes a test named after it: a lab workflow pointed at the real obid (`tools/obid-spec/workflows/browser/`), or a unit test where it says so.

- **BROWSER-1** (proposed). Given a job, when its agent lists the browser server's tools, then they are the 11 `oby_browser_*` tools in the contract's order, each with `readOnlyHint` (list_tabs, snapshot, screenshot, wait_for and ask_person: true, since none changes a page; the rest false). Lab: LAB-BROWSERAPI-4.
- **BROWSER-2** (proposed). Given a new job, when `GET /v1/browser` is read before any tool, then its browser is `notStarted`; when its first `oby_browser_open` runs, then the events say `browser` `starting` then `ready`, and a launch slower than the command's deadline still runs the command (the allowance). Lab: LAB-BROWSERAPI-1; unit: `a_slow_browser_start_has_its_own_allowance`.
- **BROWSER-3** (proposed). Given a job that opened the fixture, when `GET /v1/browser` is read, then the tab is the agent's, `ready`, on this Mac, with the page's title; when the agent snapshots, types and clicks, then the events carry each `action` with its words (`Clicking Sign in`) and outcome, in order; closing a tab says `tabClosed`. Without obid's token the list is refused. Lab: LAB-BROWSERAPI-1.
- **BROWSER-4** (proposed). Given a list answer with `seq` N, when an event changes a tab, then the event's `seq` is N+1 and a reader with `Last-Event-ID: N` gets exactly it; a list answer older than an event the app applied doesn't roll the app back. Lab: LAB-UI-LIVE-6; unit: `the_lists_carry_the_last_event_they_reflect`.
- **BROWSER-5** (proposed). Given a tab, when an unsigned take, a take signed by another key or by an unknown device arrives, then each is refused (`unsigned`, `bad_signature`, `unknown_device`); when the registered device takes it, then the agent's act is refused `human_control`, its snapshot goes on, another device can't give it back, and the holder's give back lets the agent act again. Lab: LAB-BROWSERAPI-2.
- **BROWSER-6** (proposed). Given an agent's act let through and waiting to run, when the owner's device takes the tab, then the act is refused `control_changed` and the page is unchanged. Lab: LAB-BROWSERAPI-2 (`race`).
- **BROWSER-7** (proposed). Given a tab held by device A, when device B (registered, signed) sends input on its stream, then it is refused `not_holder` and nothing reaches the page; device A's input reaches it.
- **BROWSER-8** (proposed). Given a held tab, when the holder's device puts a value into `#pw` through `POST …/secret`, then Oby's own read of the field has it, and the value is in no answer, event, log line or file under obid's home. Lab: LAB-BROWSERAPI-3.
- **BROWSER-9** (proposed). Given a page with steering words, when the agent snapshots it or evaluates an expression on it, then the result carries the page's words only under `outside.data`, with its note and `marksTurnUntrusted`, and the turn is marked. Lab: LAB-BROWSERAPI-4 and 5.
- **BROWSER-10** (proposed). Given a snapshot's ref, when the page navigates and the agent acts on that ref, then the act is refused `stale_ref` and never reaches agent-browser. Lab: lab-browser `drive`.
- **BROWSER-11** (proposed). Given a page that answers in 8 s, when the agent navigates with `deadlineMs` 2000, then the answer is `timed_out` within the deadline, the navigation isn't run again, the tab is `timedOut`, and calls on it answer `settling` until it settles. Lab: lab-browser `deadlines`, LAB-BROWSERAPI page-states.
- **BROWSER-12** (proposed). Given a navigation to a refused port, when it fails, then the tab is `failed` (list and event); a later good load clears it, at the same address too. Lab: LAB-BROWSERAPI page-states.
- **BROWSER-13** (proposed). Given the agent clicks a download, then it is listed `profile` and stays there; Save a copy unsigned is refused, signed it copies into the person's Downloads and the agent's file stays. Lab: LAB-BROWSERAPI-6.
- **BROWSER-14** (proposed). Given a tab's stream with a fast and a stuck reader, then the fast one reads frames in order and the stuck one is handed the newest only, frames dropped; input on a tab nobody holds is refused `not_held`. Lab: LAB-BROWSERAPI-7.
- **BROWSER-15** (proposed). Given the settings, when read first, then both switches are on; when "Agents can use the browser" is turned off with signed words, then every tool refuses `browser_off` ("Finish without a browser" for another Oby's task, "Ask them" for the owner's); turned on again, they work. Lab: LAB-BROWSERAPI-4 (`off-*`), LAB-BROWSERSPHERE-5.
- **BROWSER-16** (proposed). Given the browser server, when a call comes with no token, a made-up one, or an ended job's, then it is refused `no_token` or `bad_token`.
- **BROWSER-17** (proposed). Given a job of each tool (Claude Code, Codex, Cursor) in the own lane and in the box, when it starts, then its agent's MCP configuration has obid's `browser` server with its broker token and its permission rules `mcp__browser__<tool>` for the 11 tools; an `mcp__browser__` name not among them, and any other outside MCP tool, is still refused by `fixed_denial_as`. Unit: `policy::tests`; workflow with stand-in agents.
- **BROWSER-18** (proposed). Given another Oby's handoff accepted by the owner, when its job's agent uses the browser, then obid's records name the origin (`peer`, `peerOwner`), the browser is `box-<job>`, page data reaches it only quoted, and it can't open obid, its gateway, the browser's API or `file:` (`outside_box`); the owner's own conversation can open obid. Lab: LAB-BROWSERSPHERE-1.
- **BROWSER-19** (proposed). Given another Oby's task, when it asks the person to sign in, then the ask is on the host's devices, the password goes from the host's device into the field, and the other owner's device can neither take the tab nor answer (`unknown_device`); nothing of the ask is in what goes back. Lab: LAB-BROWSERSPHERE-2.
- **BROWSER-20** (proposed). Given owner B, when they try every browser route on A's obid with no token, their obid's token, their device's words or a made-up token, then none reaches A's tabs (401 or 404), and B's task lists only its own tab. Lab: LAB-BROWSERSPHERE-3.
- **BROWSER-21** (proposed). Given the owner's conversation signed in to a site, when another Oby's task opens the same site, then it is signed out, its `document.cookie` has no session and it gets `not_your_tab` on the owner's tab. Lab: LAB-BROWSERSPHERE-4.
- **BROWSER-22** (proposed). Given another Oby's task whose report carries a token-like string from a page, when the report would go back through the Sphere, then the scan finds it and nothing is sent; the same report without it goes. A scanner that can't run sends nothing. Lab: LAB-BROWSERSPHERE-6.
- **BROWSER-23** (proposed; waits on the owner's decision). Given agent-browser or Chrome for Testing that doesn't match its pinned SHA-256, when a job's browser would start, then nothing runs and the tool answers `browser` with why.
- **BROWSER-24** (proposed; waits on the owner's decision). Given more jobs using the browser than its limit, or a browser idle past its limit, then the extra job waits (`browser_starting`) and the idle browser closes.
- **BROWSER-25** (proposed). Given a job that used the browser, when it ends (or obid restarts), then no agent-browser daemon or Chrome it started is still alive and its tabs left the list. Lab: LAB-BROWSERAPI cleanup.
