# Oby's browser, as T3 Code has it: the port map

A plan, not built (2026-11-21), checked against `origin/dev` at `c7d23aee` and T3 Code at
`304cf149`. The owner decided it, in their words: "MAKE IT AS THE SAME AS T3CODE IF POSSIBLE" and
"NO FALLBACK REMOVE CHROME ENTIRELY". Nothing here is code. It maps T3 Code's pieces to Oby's
files, gives the protocol between the window and obid, lists what goes and what stays, and orders
the pull requests of both halves: the window's (this plan's author) and obid's (the browser-obid
session, whose notes are `browser-t3-obid.md` in the sessions' shared folder).

Today obid downloads its own Chrome (chrome-headless-shell, driven by agent-browser), runs it out of
sight and streams pictures of the page to the window. After the port, the page is a real Electron
`<webview>` inside Oby's window, the way a browser tab is: the agent drives it through the window,
and the person sees and uses the page itself, not a picture of it. Think of a shared car with two
sets of controls, rather than a remote-controlled car watched on a monitor.

## How T3 Code does it

Three parts (paths in T3 Code; line numbers at `304cf149`):

- **The server** (`apps/server/src/preview/`) owns every tab and the agents' tools. It drives a
  tab with Playwright, `connectOverCDP` to a one-tab loopback WebSocket (`DesktopBrowserChannel.ts:165-225`,
  `ServerBrowserContexts.ts:280-290`). Opening a tab is not a channel command: the server publishes a
  preview session on its normal event stream, waits up to 10 s for `attached`, and announces again every
  second meanwhile (`ServerBrowser.ts:115, 1040-1055`). With no desktop it falls back to a headless
  Chromium it streams (`ServerBrowser.ts:1100-1131`). **Oby deliberately has no fallback**, the
  owner's own words: "NO FALLBACK REMOVE CHROME ENTIRELY!". With no window, an agent's call is
  refused `no_window`; nobody adds a headless browser back.
- **Electron main** (`apps/desktop/src/preview/`) attaches `webContents.debugger` ("1.3") to each
  `<webview>` on `did-attach-webview` (`Manager.ts:1133-1146, 2042-2071`), and relays CDP between it
  and the server (`DesktopBrowserHost.ts`, `CdpRelay.ts`, `GuestInput.ts`). It fakes a one-page
  browser for Playwright (`CdpRelay.ts:66-210`), sends each command with `debugger.sendCommand` and
  echoes its id (`CdpRelay.ts:256-291`), and keeps a few special cases: `Page.reload`/`stopLoading`
  become `wc.reload()`/`stop()`, keys and `insertText` into a guest with no focused frame go through
  `sendInputEvent`/`insertText`, mouse presses are serialized, a 64 MB cap closes the relay
  (`GuestInput.ts:99-216`, `CdpRelay.ts:46, 83-103`). DevTools opening detaches and closing
  re-attaches; the debugger's own `detach` is recovered (`Manager.ts:1114-1137, 2252-2267`).
- **The renderer** (`apps/web/src/browser/`) mounts every hosted tab's `<webview>` once, in a
  fixed-position wrapper, keyed and sorted by tab so none ever moves in the DOM (Electron destroys a
  moved webview's guest, `ElectronBrowserHost.tsx:59-63`). Panels and the mini player draw a
  `BrowserSurfaceSlot` that publishes its rectangle; the webview follows it (`BrowserSurfaceSlot.tsx:28-78`,
  `HostedBrowserWebview.tsx:90-103, 344-359`). A crashed guest is remounted at its last address,
  at most 3 tries in 30 s (`webviewCrashRecovery.ts`). The agent's cursor is drawn from `pointer`
  (`AgentBrowserCursor.tsx`). On a native webview there is no take-control lock: the person just
  clicks the page (`ServerBrowser.ts:1872-1873`).

The relay protocol is `packages/contracts/src/desktopBrowser.ts` (48 lines), copied below as it is.

## The protocol, agreed with obid's half

Every message is keyed `{threadId, tabId}`: `threadId` is obid's job id. T3 carries it over two
inherited pipes (fd 6 and 7, NDJSON, `DesktopBackendConfiguration.ts:548-549`); Oby carries it over
the window's existing connection to obid, on Electron main's runner token in both directions:

- **obid → window:** a stream of its own, `GET /v1/browser/relay` (SSE, runner token): `event:` the
  command's type, `data:` its own JSON, `type` included, with no ids and no replay. A reconnecting
  main gets only new commands, so it never re-runs a stale click or navigate, and a burst of CDP
  never pushes the screens' events out of `/v1/browser/events`' replay log. While main follows it,
  obid counts a window as there; when it drops, every call in flight fails `no_window`. The commands:
  - `cdp {threadId, tabId, message}`: one raw CDP JSON message, page level, its `id` obid's, no `sessionId`.
  - `release {threadId, tabId}`: drop the tab's relay; the debugger stays attached (T3's `release`).
  - `pointer {threadId, tabId, phase: "move" | "click", x, y}`: only draws the agent's cursor, in CSS px; no input.
  - `turn {threadId, active}`: an agent turn runs on the job, so its tabs keep focus emulation on between steps.
  - A new tab is the existing `tab` event on `GET /v1/browser/events`, with two add-only fields:
    `url`, and `partition` (below), sent before obid waits for `attached`. The window mounts a
    `<webview>` for every agent tab it sees, as T3's renderer does; `tabClosed` unmounts it. Both stay
    on the events stream, whose replay is right for them: a reconnecting window mounts the tabs it missed.
- **window → obid:** `POST /v1/browser/relay {"events": [...]}`, in order, one request in flight per
  window, so CDP stays ordered and a burst of page events is one request. Each event is one of:
  - `attached {threadId, tabId}`: the debugger is attached and the first page settled (`dom-ready`,
    `did-stop-loading` or a real load failure, 5 s at most, `DesktopBrowserHost.ts:569-606`).
  - `detached {threadId, tabId}`: closed, crashed, swapped, refused, or DevTools took the debugger.
  - `cdp {threadId, tabId, message}`: a reply `{id, result | error}` or a CDP event `{method, params}`.
  - `loadFailed {threadId, tabId, url, code, description}`: the main frame's `did-fail-load` (not `-3`), replayed on attach.
  - `shown {threadId, tabId, shown}`: the webview started or stopped being on screen.
- **Reconnect:** when main's relay stream reconnects, its next relay POST carries `attached` and
  `shown` again for every live tab (T3's `announceAll`, `DesktopBrowserHost.ts:668-700`); no command
  runs again.
- **No window:** with nobody following `GET /v1/browser/relay`, an agent's browser tool is refused
  with a new reason, `no_window` (add-only). obid waits up to 10 s for `attached` after a new tab, as T3 does.
- **obid-spec's test window** (O3) is a headless Chrome behind the same relay, standing in for the
  webview's debugger so obid's half is proven on Azure with no Mac. It is the tests' only: the product
  has no browser of its own.
- **Signing:** the relay is the window's, on the runner token, like the events stream. The owner's
  own acts that reach obid stay signed as today (an ask answered, Save a copy, the settings).

## Partitions: obid decides, main checks

Which session a page lives in is the lane's question, and obid decides lanes (`lanes::launches_own`,
the Sphere exchange's origin). So obid names it, in the `tab` event's `partition`:

- `persist:oby-agent-<claude | codex | cursor>` for the owner's own jobs: one per agent, so a site the
  owner signed in to for Codex stays signed in for Codex's next job (board 7's sign-in, per agent).
- `oby-box-<job>`, not persistent, for another Oby's job or an `untrusted` one: dropped when the
  job's last tab is released or the job ends.

Main accepts only those two shapes, in `will-attach-webview` too, and answers `detached` for any
other; a page never lands in the default session or the app's own. T3 shares one partition per
profile with the user's tabs (`BrowserSession.ts:558-574`); Oby's are stricter, as BROWSER-18 to 22 need.

## The window's guards, beyond T3

- **A CDP domain allowlist in main.** obid speaks page-level CDP only, so main needs no fake browser:
  it forwards `Page`, `Runtime`, `DOM`, `DOMSnapshot`, `Accessibility`, `Input`, `Network`,
  `Emulation` and `Log`, and answers any other domain (`Browser`, `Target`, `SystemInfo`, `Storage`,
  ...) or any `sessionId` with a CDP error. T3 allowlists only its fake browser level (`CdpRelay.ts:70-75`).
- **Webview hardening as T3's** (`DesktopWindow.ts:620-645`): `will-attach-webview` forces
  `sandbox`, no Node in any frame, no preload of the page's, and the partition check above. Oby's own
  window keeps `sandbox`, `contextIsolation` and no Node, and gains `webviewTag: true`.
- **Downloads land where obid says, and nowhere else.** obid sends `Page.setDownloadBehavior
  {behavior: "allow", downloadPath}` with a path under its home; main's `will-download` saves there
  only when the path is inside `<obid home>/browser/downloads/`. obid keeps today's rule: an agent's
  download stays in its folder until the owner's Save a copy (BROWSERAPP-16).
- **The password never reaches obid.** Fill in (board 8) is main's: on the owner's own press, main
  focuses the field and types the value into the webview through its own debugger, between the
  agent's commands, and tells obid only that the ask was answered. Today the value goes to obid's
  `…/secret`; after the port it never leaves the window.

## What goes

- **obid:** `browser/driver/` (agent-browser, chrome-headless-shell, its processes, frame streaming,
  person input, the reaper; about 3,079 lines), `browser/install/` (the download and its pins, 929),
  the frame slot, the certificate page and Go on anyway's SPKI trust (#2674, disarmed), the stuck-page
  guards. Routes the window no longer calls go as `core/AGENTS.md` and the add-only rule allow
  (obid's half decides, PR O2).
- **The window:** the live frame (`LiveFrame` in `Panel.tsx`), main's per-tab `…/stream` followers
  (`browser-watch`, `browser-unwatch`, `browser-frame`), the owner's input over obid
  (`main/browserInput.ts`, `…/input` signing), and `fillSecret`'s path through obid (`main/device.ts`).
- **App PR 7** (the ways out of a page state on obid, and Go on anyway over `…/proceed`) is dropped:
  the webview does those itself.

## What carries over

- **Sangam's 17 boards are the UI.** The panel, tabs, strip, asks, password card, page states,
  downloads shelf, Settings' two switches and the Mac's pill stay, with the boards' words; what was
  the live picture becomes the webview in the board's place.
- **The agents' MCP browser tools** keep their 11 `oby_browser_*` names, the lanes wiring that gives
  each agent the server (5b, folded into obid's PR O1), refs that die on navigation, deadlines,
  outside-data quoting and the asks. Only what runs them changes: CDP through the relay.
- **Per-agent sign-ins** are the per-agent partitions above.
- **Take control is clicking the page**, as on T3's native webview: no lock, no hold to give back.
- **Page states** come from the webview: a load failure is `did-fail-load` (board 10's words, its
  Try again reloads the webview); a crash is `render-process-gone` (remount, as T3); the page's own
  dialog stays an agent tool and a board 10 card; an untrusted certificate is main's
  `certificate-error` for that partition (board 10's words, Go back and Go on anyway, approved per
  partition, host and fingerprint, as T3's `BrowserSession.ts:83-121` keeps them).
- **The Mac's pill** (BROWSERAPP-19, 20) is untouched: it is desktop control, not the browser.

## The pull requests

Two halves, each its own files; the protocol above is the only thing they share. The window's PRs
use vitest for main and the renderer, and the real app (`app.*` workflows, macOS only, run alone)
once a webview is live; obid's use Blok workflows (`tools/obid-spec`, on Azure). The human drive
(`.claude/skills/human-drive`) closes the port on real pages in the real window.

### O1 (obid). The relay and a CDP driver

- Files: `core/crates/obid/src/browser/` (new `relay/`, `cdp/`; `contract/` add-only: `relay`,
  the new events, `tab.url`, `tab.partition`, `no_window`), `routes.rs`, `docs/api/browser.md`,
  `docs/api/routes.txt`, `docs/contracts/browser.md`, plus 5b's lanes wiring and its test updates.
- Proof: Blok, against a stand-in window (O3) for each tool, `no_window`, the partition per lane.

### W1 (window). Main: the webview host and the relay

- **What:** `webviewTag: true`; `will-attach-webview` hardening and the partition check;
  `did-attach-webview` attaches the debugger; a host keyed `job\0tab` that reads obid's `cdp`,
  `release`, `pointer` and `turn` from `GET /v1/browser/relay` and POSTs `attached`, `detached`, `cdp`,
  `loadFailed` and `shown` in order; the domain allowlist; T3's input and reload special cases; the
  64 MB cap; detach recovery and DevTools; re-announce on reconnect; per-partition permissions
  (T3's allowlist, `BrowserSession.ts:28-39`), downloads and popups.
- **Files:** `apps/electron/src/main/browserHost.ts` (new), `main/cdpRelay.ts` (new),
  `main/browserGuest.ts` (new: attach, partitions, permissions, downloads, certificates),
  `main/browser.ts`, `main/index.ts`, `main/ipc.ts` (`browser-register`, `browser-shown`),
  `preload/index.ts`, `docs/contracts/ipc.md`, `docs/contracts/browser-app.md` (BROWSERAPP-8 rewritten).
- **Tests:** vitest with a fake debugger and a fake events stream: ordering, the allowlist, the
  special cases, the cap, the partition check, re-announce, detach. Depends on nothing but the protocol.

### W2 (window). The renderer: webviews in Sangam's panel

- **What:** one host at the app root mounting each agent tab's `<webview>` in a fixed wrapper, keyed
  and sorted by tab; a slot in the panel (and Full width) the webview follows; `registerWebview` on
  attach and `shown` on visibility; crash remount; the agent cursor from `pointer`; the strip without
  Take control and Give back; tabs from the `tab` event's `url` and `partition`.
- **Files:** `apps/electron/src/renderer/src/browser/Webviews.tsx` (new), `browser/Slot.tsx` (new),
  `browser/Panel.tsx`, `browser/BrowserPane.tsx`, `browser/live.ts`, `browser/store.tsx`,
  `browser/view.ts`, `browser/browser.css`, `renderer/src/Home.tsx` (the host's mount), their tests.
- **Proof:** vitest; then a new live app workflow (`browser-104`; `browser-102`, the streamed picture's Take control, retired with it) so a
  stand-in agent opens a page through obid (O1) and the real app draws it in a real webview.

### W3 (window). Delete the window's Chrome-era pieces

- **What:** the live frame, the stream followers, `main/browserInput.ts`, the Take control / Give
  back signing shapes (`shared/words.ts`, the preload's held labels), and their tests and docs.
- **Files:** `main/browser.ts`, `main/browserInput.ts` (deleted), `browser/Panel.tsx`,
  `shared/words.ts`, `preload/index.ts`, `docs/contracts/signed-words.md`, `docs/contracts/browser-app.md`.
- Lands with or right after obid's O2, so neither side calls what the other deleted.

### O2 (obid). Delete Chrome

- `browser/driver/`, `browser/install/`, the frame stream, person input, certificate pages, the
  stuck-page guards; routes as decided; the docs and decisions that named chrome-headless-shell.

### W4 (window). Fill in, downloads and page states on the webview

- **What:** Fill in typed by main into the webview (the ask answered to obid, the value never sent);
  agent downloads saved by `will-download` into obid's folder; board 10's states from the webview
  (`did-fail-load`, `render-process-gone`, `certificate-error` with Go on anyway per partition).
- **Files:** `main/device.ts` (`fillSecret`), `main/browserGuest.ts`, `browser/Panel.tsx`,
  `browser/Asks.tsx`, `docs/contracts/browser-app.md` (BROWSERAPP-12, 14, 16).
- **Proof:** vitest; the board 8, 10 and 12 app workflows on live webviews.

### O3 and O4 (obid). A stand-in window for Blok, and BROWSER-18 to 22 on partitions

- O3: a `browser-window` node that follows `/v1/browser/events` and answers `cdp` from a small page
  model, so every tool is red then green on Azure without a Mac. O4: the box's partitions.

### The order

1. O1 and W1 side by side: they share only the protocol, and each proves its half alone (Blok
   against the stand-in window; vitest against a fake debugger).
2. W2 once O1 and W1 are merged: the first real page in the real window.
3. O2 and W3 together: Chrome goes on both sides.
4. W4 and O3/O4.
5. The human drive on real pages.

## Decided since

- **The window's guards are agreed with obid's half (2026-11-21):** the CDP domain allowlist (obid
  uses `Page`, `Runtime`, `DOM`, `Accessibility`, `Input`, `Emulation`; `Network` stays for cookies),
  downloads only under `<obid home>/browser/downloads/` (obid names `<job>` there and hears
  `Page.downloadWillBegin` and `downloadProgress` through the relay), and Fill in typed by main, so
  `POST …/secret` retires and obid only hears the ask answered.
- **No holds, for any job.** Take control is clicking the page; take, give back, input, the stream,
  `human_control` and `control_changed` go in O2 and W3. The box's foreign jobs are kept apart by
  their partition and the box's edge, not by a hold. No `person` event at first, as T3.
- **Sangam's words stay; new lines are the plainest true sentence, marked his to change.** The
  buttons keep their labels (Take control, Give back, Go on anyway). Where a board's line stops being
  true (board 6's "Codex waits until you give it back"; Go on anyway's "starts this task's browser
  again"), the PR that changes the behavior writes the plainest true sentence and marks it, in the
  code and the PR, as Sangam's to change.

## Open

- **A "person is driving" signal**, if agents and the owner's clicks fight: a throttled
  `person {threadId, tabId}` event, window → obid.

## Sources

- T3 Code `304cf149`: `packages/contracts/src/desktopBrowser.ts`; `apps/desktop/src/preview/`
  (`Manager.ts`, `DesktopBrowserHost.ts`, `CdpRelay.ts`, `GuestInput.ts`, `BrowserSession.ts`),
  `apps/desktop/src/window/DesktopWindow.ts`; `apps/server/src/preview/` (`DesktopBrowserChannel.ts`,
  `ServerBrowser.ts`, `ServerBrowserPage.ts`); `apps/web/src/browser/` (`ElectronBrowserHost.tsx`,
  `HostedBrowserWebview.tsx`, `BrowserSurfaceSlot.tsx`, `webviewCrashRecovery.ts`).
- Oby: `core/crates/obid/src/browser/`, `apps/electron/src/main/browser.ts`,
  `apps/electron/src/renderer/src/browser/`, `docs/contracts/browser.md`, `docs/contracts/browser-app.md`.
- obid's half: `browser-t3-obid.md` in the sessions' shared notes ("Relay, agreed").
