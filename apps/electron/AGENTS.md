# Working on the Electron app

This file adds to the root [AGENTS.md](../../AGENTS.md) and never overrides it. Paths are relative to `apps/electron/`; `design/oby`, `tools/brain` and `apps/mac/` are from the repository root.

## Design

Sangam's design leads this app's screens, words and flows, as the root file's [Sangam's design leads](../../AGENTS.md#sangams-design-leads) says.

## Wire types

obid's API shapes for this app are declared in `src/shared/obid-wire.ts`, shared by the main process and the renderer. A new obid shape or key is added there, not redeclared in a component or in `mock.ts`. The Brain tool (`tools/brain`) checks it against obid's serde structs. obid's API only grows ([add-only](../../AGENTS.md#non-negotiable)).

## The mock

`src/renderer/src/mock.ts` holds the demo data shown in a browser (`npm run web`) or with `?everything`. Production renderer files also import from it: view types (`Job`, `Message`, ...), constants (`AGENTS`, `ago`, ...) and some demo lists (`JOBS`, `TURNS`, `OBYS`, ...). On obid, `OBYS`, `PLACES` and `SHARED_WITH_ME` are empty (`onObid` in `src/renderer/src/lean.ts`). Moving the view types out of `mock.ts` is follow-up work, not done yet.

## Build and run

`npm install && npm run dev` (the window), or `npm run web` for the renderer alone in a browser; `npm run typecheck`. To see a screen on a given port: `npm run web -- --port <port> --strictPort` (the renderer on its mock). `?everything` shows all of Sangam's design; [README.md](README.md) maps it.

## Packaging

The app isn't sandboxed (it runs the user's CLIs) and ships with Developer ID, not the App Store. Its packaging (`scripts/helpers.sh`) still calls `apps/mac/app/Tools/bundle-{obid,gh,gitleaks}.sh`, the only files left under `apps/mac`.

## The character in the window

`oby.html` (Sangam's copy, `design/oby`) in a frame on the window's Home (`src/renderer/src/Oby.tsx`). Oby has no always-on panel; it follows the pointer anywhere on screen (Sangam's design) only while its window is shown: main stops watching the pointer the moment the window is hidden, minimized or closed (`track` in `src/main/index.ts`), so with the window closed and nothing running it should sit at about 0% CPU (check it in Activity Monitor over 1 minute). The Swift app's floating talk pill and its menu bar item have no Electron counterpart yet (contract note, 2026-11-02): add them only with Sangam's screen.
