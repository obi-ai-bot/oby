# Oby's browser, in the lab (lane B: the screens)

The approved design ([Oby's browser](https://claude.ai/artifact/Cs7t741EJKCD7JXNfzSRXb), 17 boards)
built as React screens on the app's own styles, and driven by Blok workflows, before any of it is
ported into the app ([the build plan](https://claude.ai/artifact/TVVtw8kq77vnm81B5UfcXq), lane B).
Nothing here is Oby's own code: `apps/electron` is only read.

## Click through it

```bash
cd tools/obid-spec/lab-ui/browser && npm ci && npm run dev
```

Then open the URL it prints (`http://127.0.0.1:5190/` unless that port is taken): the index lists
the 17 states, each at its board's size, with its buttons working (Watch, the globe, Take control,
Give back, Full width and back, Type the password and Fill in, Save a copy, the switches, Stop and
Esc twice, the tip's ×). `/board/<Name>.html` shows the design board itself, rendered offline, for
comparing by eye.

Live, on lane A's browser API: start it with
`OBID_SPEC_LAB_BROWSERAPI_SERVE_MINUTES=20 tools/obid-spec/target/debug/obid-spec run --only lab-browserapi --workflow serve --obid /usr/bin/true`,
then start this app with `LAB_BROWSER_API`, `LAB_BROWSER_TOKEN` and `LAB_BROWSER_DEVICE`
(`<id> <key> <publicKey>`) from `~/Library/Caches/obid-spec/browserapi-serve.json`, and open
`/#/2?live` (or 7, 12).

## What is where

- `src/main.tsx` imports the app's `tokens.css`, `app.css` and `liveTask.css` from
  `apps/electron/src/renderer/src` (Vite's `server.fs` allows that folder, read-only), and the app's
  `brands.tsx` and `ObyFace.tsx` for the agents' logos and Oby's face.
- `src/browser.css`: the browser's new pieces (`.br-*`, the pointer, the cards' fields, the Mac's
  pill), copied as they are from the design's stylesheet (`design/oby.css`): every rule a proposal.
  `src/boards.css`: each board's own `<style>`. `src/frame.css`: what Electron and macOS draw around
  the app (the window's size, the traffic lights), the lab's only look of its own.
- `src/pieces.tsx`, `src/browser.tsx`, `src/cards.tsx`, `src/sites.tsx`: the pieces, on the app's
  real markup (Composer, TaskLog's `.ss-entries`, Sidebar's rows) where the app has one.
- `src/scenes.tsx`, `src/desktop.tsx`: the 17 screens; `src/boards.tsx`: each board's number, size,
  screen and fixture.
- `src/store.tsx`: two modes, one interface (`BrowserApi`). Fixtures put every state on screen with
  no server; live (`src/live.ts`) reads lane A's API through `src/contract.ts` (lane A's file,
  never edited here): `GET /v1/browser` and every event after it, the page from agent-browser's
  stream (`event: frame`, the newest drawn), and, while you hold the tab, your mouse and keys on it
  sent to the page (`POST …/tabs/{id}/input`, in the page's px). In the lab the server adds the
  token, so the page reads the SSE routes with `EventSource`; in the app, main does that part.
- `vite.config.ts`: in live mode this app's server stands where the app's main process will: it holds
  the owner token and a device key, so the page never sees either, and signs the owner's acts
  (`X-Oby-Words`, P-256 over the exact body). It also serves the design boards at `/board/`.
- `design/`: the canvas's boards (`*.dc.html`), its stylesheet and images, from version
  `1795132893-b200`.
- `scripts/drive-lab.mjs`: the Playwright helper behind the `lab.ui.*` nodes (`src/lab/ui.rs`), as
  `apps/electron/scripts/drive-app.mjs` is behind `app.*`: Chrome for Testing from obid-spec's own
  cache, headless, its profile in the workflow's folder; never the person's Chrome or screen.
  `compare` measures every piece the screen and its board share by class, box by box (within 4px)
  and word by word. `sidebyside` writes the board and the screen as one PNG.
- `scripts/check.mjs`: every board beside its screen without obid-spec (`node scripts/check.mjs
  [out-folder] [boards…]`). `scripts/workflows.py` writes `workflows/lab/ui/*.json`; edit it, not
  the JSON.

## The workflows

`tools/obid-spec/target/debug/obid-spec run --only lab-ui --obid /usr/bin/true` (the screens need no
obid). One per board (`01-…` to `17-…`, `LAB-UI-n`): opens its state, checks its words, its sizes
and colours (computed styles against the app's tokens), drives what the board shows, compares the
screen with the board (every browser piece within 4px, the same words), and saves a screenshot and
a side-by-side PNG (`OBID_SPEC_SHOTS=<folder>` keeps them). Five run live on lane A's API (`101-` to
`105-`, `LAB-UI-LIVE-n`): open and drive, Take control and Give back (with your click and typing
reaching the page's field), the password card, page states
(failed, a page's alert, timed out), downloads. `00-cleanup` proves the cleanup; `15-the-mac-stop`
is board 15's Stop on agent-desktop, skipped without its permissions or off a desktop Space.

`OBID_SPEC_LAB_UI=<another copy of this folder>` drives another build, to watch a workflow fail on
the screens before a change.

## Not matched within 4px, and why

Every browser piece (the panel, its tabs, address, strip, page, pointer, shelf, the new cards, the
sheets, the Mac's pill and modal) is within 2px of its board on all 17 boards. What differs is
today's app, drawn by the app's own CSS where the boards restated it slightly differently; these
are reported by `compare` (`today`), never held to the board, and are for the owner and Sangam:

- **What it did, under its card**: the app's rows are 6px apart (`.ss-entries`, `.lt-log`), the
  boards 2px; the task card's stages bar sits 8px under its words in the app as in the boards, but
  the gaps add up to ≤29px on the conversation's messages above (boards 1, 2, 6, 7, 8, 12, 14).
- **The title bar**: the app's `.crumb` has 6px of padding pulled back by its margin (same text
  place, a box 12px wider), so the project chip sits 14px further right than on the boards; the
  bar's end keeps the app's 20px gutter where the boards use 12px, so the globe is 8px further left.
- **The side panel** (3, 13): the rows are the app's real Sidebar markup (`.convo-nest`,
  `.sidebar-list`), up to 28px off the boards' simplified rows; Settings' items are 34px apart in
  the app, 36px on the board.
- **Settings' select and the sphere tile** (13): the board's `.set-select` and tile border are the
  design's restatement; the app draws them its own way.
- **A short conversation sits at the foot of the thread** on every board; the app starts it at the
  top today. The lab follows the boards (`frame.css`).

Open questions, also for the owner and Sangam:

- An agent's act runs in about 40 ms, so "Clicking Sign in" would never be seen: in live mode what
  the agent did stays on screen 1.5 s after it ends (`LINGER_MS`, `src/live.ts`).
- After Save a copy, the agent's row reads "Codex downloaded it · in Downloads" with Show in Finder,
  words taken from the board's two rows; the board shows no after-state.
- The board's Light proposals (2, 6, 10, 13) aren't built: the app is dark only.
- Live mode draws the stream's frame scaled to the panel; the agent's pointer is placed in the
  page's own pixels, unscaled.
