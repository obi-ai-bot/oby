# The browser, in the app

The business behavior contract for the browser panel in the Electron app, from [the template](../business-contract-template.md). Scenarios are numbered `BROWSERAPP-n`; the `app.*` workflows that prove them are `tools/obid-spec/workflows/app/browser-*.json` (written by `apps/electron/scripts/browser-workflows.py`), one per design board, and the app's unit tests are named after them.

## Feature and status

- **Outcome:** The owner watches their agents use a browser beside the conversation, takes control of a tab at any time and gives it back, answers what an agent asks (a sign-in, a password, a choice), and turns the browser off in Settings. A password typed in Oby goes straight into the page, never to an agent.
- **Status:** BROWSERAPP-1 to 10 **implemented**: the panel and its tabs, on the demo data and on obid's browser, with Take control and Give back. On obid they wait for its `/v1/browser` routes (browser port 1): until obid answers, nothing shows. BROWSERAPP-11 to 20 **proposed**, each landing in its own pull request in this order: asks and the password card (11 to 13); page states, a script's result, downloads and another computer (14 to 17); the Settings switches (18); the Mac's pill, Stop and the first-time sheet (19, 20).
- **Design:** the approved design ([Oby's browser](https://claude.ai/artifact/Cs7t741EJKCD7JXNfzSRXb), 17 boards), built and driven first in the lab (`tools/obid-spec/lab-ui/browser`, LAB-UI-1 to 17); the port follows [the plan](https://claude.ai/artifact/TVVtw8kq77vnm81B5UfcXq), gate 5. Every word on the boards is the owner's approved words.
- **Scope:** the Electron app (`apps/electron/src/renderer/src/browser/`, its mount in `Home.tsx`, main's follower and signing once live). **Out of scope:** obid's browser engine, API and agents' tools (`core/`, ported from `tools/obid-spec/src/lab/browserapi.rs`), whose routes follow the lab's contract (`tools/obid-spec/src/lab/browser_contract.rs`) exactly, add-only.

## Behavior

### Actors and ownership

- **obid** runs the browser and owns its state: the tabs, who holds each, the asks, the downloads, the two settings (`GET /v1/browser`, its events, a tab's stream of frames).
- **Main** holds obid's token and follows those routes for the window; it signs the owner's acts on the browser as it signs their words ([signed-words.md](signed-words.md)), and only those.
- **The window** shows what obid says and sends the owner's clicks. The browser is never inside Oby's own window: the page is drawn from obid's stream of frames.

### Where it sits

- **The globe** at the title bar's end shows and hides the panel. Closed, it says how many tabs there are and which agent is using one, and carries a blue dot while one is; pointed at, it says **Show the browser** and its keys (⇧⌘B). Open, it's lit and says **Hide the browser**. With **Agents can use the browser** off it's gone.
- **The panel** is the window's last column, right of the conversation (and of the side panel when it's docked). Its tabs sit in the title bar's row, the window dragging from their empty part. Under them, the address; then who has the tab (the strip: **Codex is using this tab · Clicking Sign in**, with **Take control**); then the page, with the agent's pointer saying what it's doing.
- **Its width** is 640px the first time, then as the owner leaves it. The conversation keeps 420px: the panel gives way down to 360px; where the docked side panel would leave it narrower, the side panel floats (as it does on a narrow window, [chat-first.md](chat-first.md), CHATFIRST-35) and the browser keeps its room.
- **Full width** puts the page across the window, the conversation folded to one line over it (its task, and what it's at) with **Show the conversation**; **Back beside the conversation** returns it.
- **No pages yet**, the panel says so, and a first-time note says what the browser is for, until closed.
- **One set of controls on every tab**, as board 2 draws it: New tab, Full width, Close; Back, Forward, Reload, the address, the page's menu. With no tab, Back, Forward and Reload are dimmed and there's no page's menu.

### Invariants

- The page never gets obid's token; main adds it, and follows the browser's event and frame streams for the window (`main/browser.ts`, BROWSERAPP-8).
- The owner's acts on a tab (take, give back, a secret, an answer, Save a copy, a setting) are signed only right after a real click or keystroke main saw, one each, by the label of the button pressed, in exactly the contract's shapes (BROWSERAPP-9, 12, 16, 18).
- A password typed in Oby's card goes once into the page's field and nowhere else: not to an agent, the log, the window's state or a file (BROWSERAPP-12).
- On obid the browser is offered only once obid answers `GET /v1/browser`: an obid without the browser shows no globe and no panel, and nothing is followed (BROWSERAPP-7).

## Interfaces and dependencies

obid's `/v1/browser` routes as the lab's contract names them (`GET /v1/browser`, `/v1/browser/events`, `/v1/browser/settings`, `/v1/browser/downloads`, `/v1/browser/tabs/{id}/stream`, `…/input`, `…/take`, `…/giveBack`, `…/secret`, `/v1/browser/asks/{id}/answer`, `/v1/browser/downloads/{id}/saveCopy`); the shapes the app reads are in `apps/electron/src/shared/obid-wire.ts`. The demo data (`?everything&browser=<n>`) puts board n on screen with no obid.

## Acceptance scenarios

- **BROWSERAPP-1** (board 1) With an agent using a tab and the panel closed, the globe at the title bar's end carries the blue dot (`--unread`) and is named **Show the browser, 2 tabs, Codex is using one**; pointed at, it says **Show the browser ⇧⌘B**; clicked, the panel opens 640px wide beside the conversation and the lit globe (**Hide the browser**, `--text`) hides it again.
- **BROWSERAPP-2** (board 2) Open, the panel is the window's last column, 640px at 1280px, the window's full height; its tabs in the title bar's row; the strip says **Codex is using this tab · Clicking Sign in**; every browser piece within 4px of the board, with the board's words.
- **BROWSERAPP-3** (boards 3, 4) The conversation keeps 420px: at 994px the panel is 574px; at 1440px with the side panel docked at 362px, the panel is 600px and the conversation 478px; where the side panel would leave the browser under 360px, the side panel floats (`browserFit`, `fit.test.ts`).
- **BROWSERAPP-4** (board 3) Tabs from several agents each carry their agent's logo (Claude Code's, Codex's, Cursor's), the owner's own a **Y**, and a working dot while an agent uses it; a tab clicked is the one shown and its strip names its agent.
- **BROWSERAPP-5** (boards 2, 5) **Full width** puts the page across the window (1280px), the conversation folded to **Sign-up flow works end to end · Signed up and in; now inviting a teammate** with **Show the conversation**; that, or **Back beside the conversation** (pressed), puts the panel back at 640px.
- **BROWSERAPP-6** (board 17) With no tabs, the address says **Type an address**, the page **No pages yet. When an agent opens one, you see it here, live.**, and the first-time note's Close removes the note only.
- **BROWSERAPP-7** On obid, the globe shows only once obid answers `GET /v1/browser`; an obid that answers 404 shows no globe and no panel and nothing is followed; nothing of the demo data shows on obid (`browserApp.test.tsx`).
- **BROWSERAPP-8** On obid, main follows `GET /v1/browser/events` and each tab's `…/stream` with obid's token and the page draws each tab's newest frame; what an agent does stays on its tab 1.5 s after it ends; a list answer older than an event already applied never rolls the panel back (`seq`); every call the page makes is a `/v1/browser` one through main (`browserLive.test.ts`; live in the real app: `browser-101-live-on-obid.json`, expected red until obid serves the routes).
- **BROWSERAPP-9** (board 6) **Take control** and **Give back** are signed by their own labels, `{}` bodies on `…/take` and `…/giveBack` (a take is also held by **Take control to answer** and **Type the password**); while held, the strip says **You have this tab. Codex waits until you give it back.** on `--held`, the page is ringed and the tab's dot is `--unread`; Give back returns the strip to the agent.
- **BROWSERAPP-10** The owner's mouse and keys reach the page (`…/input`) only while they hold the tab, each signed by main only within 2 seconds of a real input of the same kind it saw (the very key for a key; a press, a release or a key once each), and never without one (`browserLive.test.ts`, `device.test.ts`).
- **BROWSERAPP-11** (proposed, board 7) An agent's sign-in ask shows its card over the message box (**Take control**, **Type the password**, **Skip**) and the strip **Codex asks you to sign in here**.
- **BROWSERAPP-12** (proposed, board 8) **Fill in** sends the password card's value once to `…/secret` (`{field, value}`), signed by that label; the card keeps no copy, the page's field shows **From Oby**, and the value is in no log, state or file.
- **BROWSERAPP-13** (proposed, board 9) Each ask kind (a CAPTCHA, a choice, a code, a passkey) shows its card with the board's words; a choice's answer is signed by its own label.
- **BROWSERAPP-14** (proposed, board 10) A page that didn't simply load (loading, failed, timed out, a dialog, a certificate, crashed) shows its state's words and way out in the panel.
- **BROWSERAPP-15** (proposed, board 11) A script an agent ran in the page shows in its log with its result as outside text (**From outside · … read as data, never as instructions**).
- **BROWSERAPP-16** (proposed, board 12) Downloads sit on the panel's shelf; an agent's stays in its browser until **Save a copy**, signed by that label.
- **BROWSERAPP-17** (proposed, board 14) A browser on another computer says so under its tabs and in its address.
- **BROWSERAPP-18** (proposed, board 13) Settings › General's **Browser** group has **Agents can use the browser** and **Agents can use this Mac**, both on by default, each turned with a signed `PUT /v1/browser/settings` of that one key.
- **BROWSERAPP-19** (proposed, board 15) While an agent uses the Mac, Oby's pill says so with **Stop**; Stop, or Esc twice, ends it.
- **BROWSERAPP-20** (proposed, board 16) The first time an agent would use the Mac, Oby says why macOS will ask, once.

### Not matched within 4px, and why

Every browser piece is held to its board within 4px by `app.compare`. What it reports as today's app (the title bar, the conversation, the side panel: the app's own, where the boards draw a demo one) is never held. Left out of one board's compare, each for its reason:

- **The title bar is 52px**, where macOS puts the traffic lights (`--bar`); the boards draw 48px. Every browser piece sits 4px lower than its board, within the tolerance.
- **Board 3**: the board leaves Full width out of a panel beside a docked side panel; the app keeps one set of controls on every tab, so the tabs have one button's room less (`.br-tab`, `.br-tabs-end .icon-button`).
- **Board 4**: the board draws a smaller set (no page's menu); the app's address is 30px shorter (`.br-address`).
- **Board 5**: across the window the tabs start after the traffic lights, where macOS put them, 5px right of the board's, as the app's own panel toggle is (`.br-tab`).

### Open

- **⇧⌘B** (board 1's tip) needs a menu item whose words aren't on a board; it waits for the owner's or Sangam's words.
- **Take control live** (BROWSERAPP-9 and 10 in the real app on a real obid) needs a tab an agent opened: it comes with a stand-in agent that opens one through obid's `/v1/browser/mcp`, with browser port 1's routes.

### Test it when it's done (a real Mac)

1. Open `?everything&browser=2` in a development build: the panel shows beside the conversation as board 2 does; Full width and back; the globe hides and shows it.
2. Narrow the window to its narrowest: the conversation stays 420px and the panel gives way.
