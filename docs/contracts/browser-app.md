# The browser, in the app

The business behavior contract for the browser panel in the Electron app, from [the template](../business-contract-template.md). Scenarios are numbered `BROWSERAPP-n`; the `app.*` workflows that prove them are `tools/obid-spec/workflows/app/browser-*.json` (written by `apps/electron/scripts/browser-workflows.py`), one per design board, and the app's unit tests are named after them.

## Feature and status

- **Outcome:** The owner watches their agents use a browser beside the conversation, takes control of a tab at any time and gives it back, answers what an agent asks (a sign-in, a password, a choice), and turns the browser off in Settings. A password typed in Oby goes straight into the page, never to an agent.
- **Status:** BROWSERAPP-1 to 18 **implemented**: the panel and its tabs, on the demo data and on obid's browser, with Take control and Give back, the agents' asks and the password card, page states, a script's mark, downloads, another computer and Settings › General's two switches. On obid they wait for its `/v1/browser` routes (browser port 1): until obid answers, nothing shows. BROWSERAPP-19 and 20 **proposed**, in their own pull request: the Mac's pill, Stop and the first-time sheet (19, 20).
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
- **One panel at the window's right at a time**: a task's Changes opening puts the browser away, and the browser opening closes Changes.
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
- **BROWSERAPP-3** (boards 3, 4) The conversation keeps 420px: at 994px the panel is 574px; at 1440px with the side panel docked at 362px, the panel is 600px and the conversation 478px; where the side panel would leave the browser under 360px, the side panel floats (`browserFit`); a task's Changes and the browser are never open together (`browserApp.test.tsx`).
- **BROWSERAPP-4** (board 3) Tabs from several agents each carry their agent's logo (Claude Code's, Codex's, Cursor's), the owner's own a **Y**, and a working dot while an agent uses it; a tab clicked is the one shown and its strip names its agent.
- **BROWSERAPP-5** (boards 2, 5) **Full width** puts the page across the window (1280px), the conversation folded to **Sign-up flow works end to end · Signed up and in; now inviting a teammate** with **Show the conversation**; that, or **Back beside the conversation** (pressed), puts the panel back at 640px.
- **BROWSERAPP-6** (board 17) With no tabs, the address says **Type an address**, the page **No pages yet. When an agent opens one, you see it here, live.**, and the first-time note's Close removes the note only.
- **BROWSERAPP-7** On obid, the globe shows only once obid answers `GET /v1/browser`; an obid that answers 404 shows no globe and no panel and nothing is followed; nothing of the demo data shows on obid (`browserApp.test.tsx`).
- **BROWSERAPP-8** On obid, main follows `GET /v1/browser/events` and each tab's `…/stream` with obid's token and the page draws each tab's newest frame; what an agent does stays on its tab 1.5 s after it ends; a list answer older than an event already applied never rolls the panel back (`seq`); every call the page makes is a `/v1/browser` one through main (`browserLive.test.ts`; live in the real app: `browser-101-live-on-obid.json`, expected red until obid serves the routes).
- **BROWSERAPP-9** (board 6) **Take control** and **Give back** are signed by their own labels, `{}` bodies on `…/take` and `…/giveBack` (a take is also held by **Take control to answer** and **I’ll choose in the page**); while held, the strip says **You have this tab. Codex waits until you give it back.** on `--held`, the page is ringed and the tab's dot is `--unread`; Give back returns the strip to the agent.
- **BROWSERAPP-10** The owner's mouse and keys reach the page (`…/input`) only while they hold the tab, each signed by main only within 2 seconds of a real input of the same kind it saw (the very key for a key; a press, a release or a key once each), and never without one (`browserLive.test.ts`, `device.test.ts`).
- **BROWSERAPP-11** (board 7) An agent's sign-in ask shows its card over the message box of its job's conversation, never over it (**Take control**, **Type the password**, **Skip**), its heading the agent's first sentence (a statement's full stop dropped) and the rest leading its words, with **1 of n**; the strip says **Codex asks you to sign in here** and the tab's dot is `--needs`.
- **BROWSERAPP-12** (board 8) **Type the password** opens the password card (the strip: **You’re filling the password. Codex waits.**); **Fill in** (or Enter in its box) has main put the value into the page's field: on the owner's own press and a real input main saw, main takes the tab if this device doesn't hold it, sends `…/secret` `{field, value}`, answers the ask and gives back only the hold it took, each signed. The card keeps no copy, the page's field shows **From Oby**, and the value is in no log, the window or a box left behind (`browserapp_12_…` in `device.test.ts`, red without the click check).
- **BROWSERAPP-13** (board 9) Each ask kind (a CAPTCHA, a choice, a code, a passkey) shows its card with the board's words, one at a time; a choice's answer (`{choice}`) is signed by its own label, **Skip** and **Skip this page** answer `{}`, **I’ll choose in the page** takes control, and a code is filled as a password is (Fill in).
- **BROWSERAPP-14** (board 10) A page that didn't simply load shows its state's words in the panel: loading (the strip **Codex · Waiting for the page**, Stop), failed, timed out, the page's own dialog over the page (answerable once you hold the tab), an untrusted certificate (its address marked), crashed. On the demo data each shows its way out as the board does (Try again, Go back, Go on anyway, Reload, Leave, Stay); on obid the words show alone until obid has the owner's routes for them (Open).
- **BROWSERAPP-15** (board 11) A script an agent ran in the page (obid's `scriptRun`) marks the page **Ran a script** (with how long it took, where known) and the strip says **Codex ran a script in this page · done**, until the agent does something else there. Its result as outside text is the agent's log under its card (TaskLog), not the panel's.
- **BROWSERAPP-16** (board 12) Downloads sit on the panel's shelf, each saying whose it is and where; an agent's stays in its browser until **Save a copy** (`…/saveCopy`, `{}`, signed by that label) puts a copy in Downloads; **Show in Finder** has main show that file, by its name, in the owner's Downloads and nowhere else.
- **BROWSERAPP-17** (board 14) A browser on another computer says so under its tabs (**This browser runs on devbox. Its localhost is devbox’s, not this Mac’s.**), in its address (**on devbox**) and on its New tab button.
- **BROWSERAPP-18** (board 13) Settings › General's **Browser** group (marked **New**, after Permissions on this Mac, where the browser is offered) has **Agents can use the browser** and **Agents can use this Mac**, both on by default, each with the board's words, and its footer; each is turned with `PUT /v1/browser/settings` of that one key, a boolean, signed by the switch's own label (held by its `aria-label`, a switch having no words of its own). With the browser off, its globe goes from the title bar.
- **BROWSERAPP-19** (proposed, board 15) While an agent uses the Mac, Oby's pill says so with **Stop**; Stop, or Esc twice, ends it.
- **BROWSERAPP-20** (proposed, board 16) The first time an agent would use the Mac, Oby says why macOS will ask, once.

### Not matched within 4px, and why

Every browser piece is held to its board within 4px by `app.compare`. What it reports as today's app (the title bar, the conversation, the side panel: the app's own, where the boards draw a demo one) is never held. Left out of one board's compare, each for its reason:

- **The title bar is 52px**, where macOS puts the traffic lights (`--bar`); the boards draw 48px. Every browser piece sits 4px lower than its board, within the tolerance.
- **Board 3**: the board leaves Full width out of a panel beside a docked side panel; the app keeps one set of controls on every tab, so the tabs have one button's room less (`.br-tab`, `.br-tabs-end .icon-button`).
- **Board 4**: the board draws a smaller set (no page's menu); the app's address is 30px shorter (`.br-address`).
- **Boards 11, 12 and 14**: the board draws a smaller set for a tab nobody is using (no Full width, no page's menu, Take control without its hand); the app keeps board 2's set, so the tabs' end has one more button, the address is 30px shorter and the strip's Take control 20px wider (`.br-tabs-end .icon-button`, `.br-address`, `.br-control-text`, `.br-control .tc-option`).
- **Boards 7 and 8**: the decision card sits over the app's own message box, whose place is today's app, not the board's: its words are held to the board, its places reported (`.decision-card`, `.tc-ask-line`, `.tc-ask-text`, `.secret-row`, `.secret-note`).
- **Board 13**: Settings › General is today's page, with more groups than the board draws: the Browser group is held by its words, its switches and its place after Permissions on this Mac, not by the board's places.
- **Boards 9 and 10** are sheets of pieces side by side: their cards and small browsers are checked one at a time in the app, word by word, not by the sheet's places.
- **Board 5**: across the window the tabs start after the traffic lights, where macOS put them, 5px right of the board's, as the app's own panel toggle is (`.br-tab`).

### Open

- **The owner's ways out of a page state** (Try again, Reload, Go back, Go on anyway; a page dialog's Leave and Stay) need routes obid doesn't have for the owner: until then, on obid, a state's words show alone.
- **Skip answers the ask** (`{}`, which `docs/api/browser.md` calls Done): the agent goes on without what it asked for. Whether Skip should say so to the agent in other words is the owner's or Sangam's.
- **Asks live** (BROWSERAPP-11 to 13 on a real obid) need an agent's ask: they come with a stand-in agent that asks through `/v1/browser/mcp`, with browser port 1's routes.
- **⇧⌘B** (board 1's tip) needs a menu item whose words aren't on a board; it waits for the owner's or Sangam's words.
- **Take control live** (BROWSERAPP-9 and 10 in the real app on a real obid) needs a tab an agent opened: it comes with a stand-in agent that opens one through obid's `/v1/browser/mcp`, with browser port 1's routes.

### Test it when it's done (a real Mac)

1. Open `?everything&browser=2` in a development build: the panel shows beside the conversation as board 2 does; Full width and back; the globe hides and shows it.
2. Narrow the window to its narrowest: the conversation stays 420px and the panel gives way.
