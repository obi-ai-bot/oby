# Oby Artifacts: the port from the lab into obid and the app

A plan, checked against `origin/dev` at `820d33c4` (2026-11-21); P1 (#2776), P2 (#2798), P3 (#2799) and L1 to L4 are merged, and the
order changed on 2026-11-21: the local loop, below, comes first. It turns the port step
of the Oby Artifacts backend plan (its §7.6) into small pull requests, each with its contract's
scenarios, the files it owns, its place in the order, and the lab workflow that becomes its Blok
proof (OBIDSPEC-16). Nothing here is code. Each PR below can become a task file
(`factory/queue/tasks/B00-<n>.md`, sprint 0) once the owner approves the plan, as
[owner-workflows.md](owner-workflows.md) did.

Think of the lab as a workshop where the whole thing was built and tested on a bench: the store,
the viewer, sharing and the public link all work there, each proven red before green. The port
moves it into the house piece by piece: a few walls at a time, the lights staying on, each piece
proven in place by the same test that proved it on the bench, now pointed at the real obid or the
real app.

## First: the local loop, as LibreChat does it (2026-11-21)

The owner, 2026-11-21: "Yes, do it like LibreChat and Sandpack and then we try to make some
changes to be able to achieve what we wanted to oby but first make it work!!!!!!"

So the next PRs build the loop LibreChat has, and nothing else, until it works in the owner's
daily drive:

1. an agent makes an artifact in a conversation;
2. the owner sees it in a side panel, with Code and Preview, and edits it;
3. each change is a version, and the owner can go back to any version;
4. export and import.

Preview is drawn by Sandpack (CodeSandbox's in-browser runner), in a sandboxed frame inside
Sangam's artifact screens. The rest of this plan comes after that loop works: sharing, encryption,
public links, the invite, and Claude Design boards drawn by Oby's runtime.

Think of LibreChat's artifacts as a recipe that is already proven. We cook it as written, in our
own kitchen and with our own ingredients: P1's store is the pantry, and nothing comes from a
shop outside. Only once the dish comes out right do we change the recipe.

### What LibreChat and Sandpack do

Read from `danny-avila/LibreChat` `main` and `codesandbox/sandpack` `main`, 2026-11-21.

- **LibreChat's panel** (`client/src/components/Artifacts/`):
  - The panel is `Artifacts.tsx`. Its tabs are `ArtifactTabs.tsx`: Code is `ArtifactCodeEditor.tsx`
    and Preview is `ArtifactPreview.tsx`.
  - Versions are `ArtifactVersion.tsx`, a menu of the same artifact's versions, shown only when
    there are two or more.
  - Downloading is `DownloadArtifact.tsx`. The card in a message, which opens the panel, is
    `ArtifactButton.tsx`.
- **The message tags.** A model writes `:::artifact{identifier type title}` in its message, and
  LibreChat parses it as a directive. Oby needs no tags: an agent already publishes through its
  room's `.obi/artifacts/` or `oby_artifact`, P1's store keeps every version, and the conversation
  shows a card per artifact.
- **How LibreChat drives Sandpack** (`client/src/utils/artifacts.ts`):
  - Each type maps to a Sandpack template. `text/html`, markdown, code, SVG and plain text use
    `static`; `application/vnd.react` uses `react-ts`.
  - A React artifact gets a fixed set of dependencies (React, `lucide-react`, Radix, `recharts`,
    shadcn's files).
  - The preview is `SandpackProvider` plus `SandpackPreview`, with "Open in CodeSandbox" and
    refresh hidden.
  - `buildSandpackOptions` sets `bundlerURL` from `SANDPACK_BUNDLER_URL`, or
    `SANDPACK_STATIC_BUNDLER_URL` for `static`.
  - Edits are saved by `useEditArtifact`, debounced, into the message that made the artifact.
- **LibreChat isn't local.** It still uses hosted services even with its own bundler:
  - With no bundler URL set, it uses CodeSandbox's hosted bundler.
  - It always loads Tailwind from `https://cdn.tailwindcss.com/3.4.17` (`externalResources`).
  - The `react-ts` bundler fetches npm packages from a hosted CDN.
  - Its editor, `@monaco-editor/react`, loads Monaco from jsDelivr unless told otherwise.
- **Sandpack's clients** (`sandpack-client/src/clients/`):
  - `static` serves a template's files through `static-browser-server`. A hidden relay frame at
    `<random id>-<bundler host>/__csb_relay/` registers a service worker, which answers the preview
    frame's requests from the files in memory. The bundler host defaults to
    `preview.sandpack-static-server.codesandbox.io`, and `bundlerURL` changes it.
  - `runtime` (the React templates) is the bundler that runs in the browser. Both bundlers fetch
    every npm package from a CDN of CodeSandbox's: the classic one from
    `prod-packager-packages.codesandbox.io`, the newer `sandpack-bundler` from
    `sandpack-cdn-staging.blazingly.io`.
  - `node` runs on Nodebox.

### Licences, from their own files

| What | Licence | What we may do with it |
|---|---|---|
| LibreChat | MIT (`LICENSE`) | Read it and follow its design. Any code copied keeps its MIT notice in the file. |
| `@codesandbox/sandpack-react` 2.20.0, `sandpack-client` 2.19.8 | Apache-2.0 (`LICENSE`) | Use as packages. The app's notices carry their licence. |
| `static-browser-server` 1.0.3 (the relay) | Apache-2.0, said in its `package.json`. Neither the repository nor the package has a `LICENSE` file. | Ship its built relay, changed (see below), with the Apache-2.0 text beside it. |
| `codesandbox/sandpack-bundler` | Apache-2.0 | Allowed, but not used: it fetches npm packages from a hosted CDN and hasn't changed since 2024-11. |
| `codesandbox/codesandbox-client`, the classic bundler, and LibreChat's self-hosting fork `LibreChat-AI/codesandbox-client` | GPLv3, except `packages/common`, `packages/sandpack-core` and `packages/app/src/sandbox` (Apache-2.0) | **Not copied or shipped.** Its build mixes GPLv3 code into the bundler. |
| `@codesandbox/nodebox` 0.1.8 | **Sustainable Use License**, not open source: "only for your own internal business purposes or for non-commercial or personal use" | **Never in Oby's build.** `sandpack-client` depends on it for its `node` client, so the app replaces it with an empty stub of our own through npm's `overrides`. A check fails the build if any of it is in `out/`. |
| `sucrase` 3.35.1, `@tailwindcss/browser` 4.3.5, React | MIT | Vendored for React artifacts (L5). |

### How Sandpack runs fully local in Oby

**Yes, it can**, with one change from LibreChat: every template is `static`, and React artifacts
are compiled in the app, not by a hosted bundler.

- **The relay is ours, served by the app.**
  - `static-browser-server`'s package already holds the built relay: `out/preview/`, four files,
    about 38 KB, with no address outside itself.
  - The app vendors it under `apps/electron/resources/sandpack-relay/` and adds a CSP: nothing
    from outside, `connect-src 'self'`, no `frame-ancestors` but the app.
  - Electron's main serves it on 127.0.0.1, on a port it picks, for any host
    `<id>-preview.localhost`. Chromium sends `*.localhost` to loopback by itself, and treats it as
    a secure context, so the service worker can register.
  - `bundlerURL` is always that address, so Sandpack never reaches `codesandbox.io`.
- **Nodebox never ships.** See the table.
- **React artifacts with no CDN** (L5): this is the change from LibreChat.
  - The app compiles `App.tsx` and its files with `sucrase`.
  - It writes an `index.html` with an import map onto vendored ESM builds of `react`, `react-dom`,
    `lucide-react` and `@tailwindcss/browser`, served by the same relay server.
  - It hands those files to Sandpack's `static` template.
  - An import outside the vendored set shows a plain error in the preview and fetches nothing.
    The set grows by vendoring, one package and its licence at a time.
- **The editor** is Sandpack's own `SandpackCodeEditor` (CodeMirror 6, MIT), which loads nothing.
  We don't take Monaco's CDN loader.
- **Proof** in each app PR: a network log of the preview shows every request on loopback, and the
  build check finds no `codesandbox.io`, `blazingly.io`, `cdn.tailwindcss.com`, jsDelivr, unpkg or
  Nodebox code in the built app.

### The loop's PRs, in order

L1 to L6 come next, before P4 to P13. Each app PR has its scenarios in a new
`docs/contracts/artifacts-app.md` (ARTIFACTSAPP-n), each a test named after it: vitest for the
renderer, and an `app.*` workflow under `tools/obid-spec/workflows/app/`. Only the `app.*`
workflows run on the Mac, alone; everything else runs on Azure. Each obid PR's Blok proof is a
route workflow, red before green. Sangam's artifact screens (the Oby Artifacts canvas, boards 1a to
1c) are what the app draws; the words come from his boards, never from a contract.

#### L1. Sandpack, local: the relay, the card, and Preview beside the conversation

- **Lane:** dispatcher (the app shows what was made). **Depends:** P1 (merged).
- **Built** (2026-11-21): the card and the pane came into L1 from L2, so a real artifact shows in
  the real window now. The preview is in a view of main's own, not the page's frame.
- **What:**
  - `@codesandbox/sandpack-client` 2.19.8, pinned exactly: its static client's chunk alone is built
    in, never the hosted `runtime` bundler or the Nodebox `node` client. `sandpack-react` comes
    with Code (L2).
  - `@codesandbox/nodebox` replaced by `apps/electron/stubs/nodebox/`, a dependency the override
    points at.
  - The relay, vendored, and main's relay server (`*-preview.localhost`, 127.0.0.1 only), its worker
    sending other origins to the network and putting the preview's CSP on every answer.
  - The preview in a `WebContentsView` of its own partition (`oby-artifact-preview`), sandboxed,
    with no preload; its session lets go only its host page and the relay. It is laid over the pane's
    box, and main reads the version's files from obid.
  - The card under the task that made it (board 1b's words), and the pane in the browser's place.
  - A `page` (HTML), `markdown` (drawn by `marked`, as LibreChat does), `image` and `canvas` show;
    a `design` shows its files until P4.
- **Scenarios:**
  - ARTIFACTSAPP-1: a page artifact's version draws in Sandpack's preview, and every request the
    preview makes is to loopback.
  - ARTIFACTSAPP-2: the built app holds no Nodebox code and no hosted bundler or CDN address.
  - ARTIFACTSAPP-3: the preview can't reach off its origin, and is sandboxed in its own partition
    with no preload, no Node and no bridge.
  - ARTIFACTSAPP-4: the card under its task opens the pane on the latest version.
- **Files:** `apps/electron/package.json`, `package-lock.json`, `electron.vite.config.ts`,
  `vite.config.ts`, `stubs/nodebox/`, `resources/sandpack-relay/`, `scripts/local-only.mjs`,
  `scripts/test-artifacts.mjs`, `src/main/{artifactPreview,previewRules,sandpackRelay}.ts`,
  `src/main/index.ts`, `src/preload/index.ts`, `src/renderer/preview.html`,
  `src/renderer/src/artifacts/`, `ChatView.tsx`, `Home.tsx`, `obid.ts`;
  `docs/contracts/{artifacts-app,ipc}.md`.

#### L2. The panel's Code, following the agent

- **Lane:** dispatcher. **Depends:** L1, P3 (merged).
- **What:**
  - **Code.** Code and Preview tabs in the pane. Code is CodeMirror 6, the editor Sandpack's
    `SandpackCodeEditor` wraps, read-only here, with a tab per file. `sandpack-react` itself isn't
    built in: it names CodeSandbox's hosted services (its Open in CodeSandbox, its client's hosted
    `runtime`), which the build's guard refuses.
  - **Following the agent.** An `artifact` event for the open conversation opens or moves the
    pane to the new version. LibreChat's panel follows the latest the same way.
  - **Asking for a change.** Words said while the pane is open carry `artifact: {id, version}`,
    P3's key, so the agent changes this artifact.
- **Scenarios:**
  - ARTIFACTSAPP-5: Code shows each of the version's files.
  - ARTIFACTSAPP-6: a new version from the agent moves the open pane to it.
  - ARTIFACTSAPP-7: words said with the pane open reach the artifact's conversation with its
    `artifact`.
- **Files:** `src/renderer/src/artifacts/` (`Code.tsx`, `Pane.tsx`, `store.ts`), `Composer.tsx`'s
  send (the key), `shared/words.ts`.

#### L3a. obid: the owner's edit is a new version

- **Lane:** dispatcher. **Depends:** P1. **Can run beside:** L1, L2.
- **What:** `POST /v1/artifacts/{id}/versions` with `{base, files: {path: text}}`, signed in the
  app's new `Edit` shape (signed-words).
  - The files are the base version's, with the changes laid over them.
  - They go through the same checks as any publish: kind, limits, names, and the secret scan. The
    new version is the next one.
  - It records `by: "owner"` (add-only, a version's `by`) and sends the `artifact` event.
  - The answer is `409` when `base` isn't the latest, so two edits never pass each other silently.
    It is `404` for no such artifact, and `401` unsigned.
- **Scenarios** (`artifacts.md`):
  - ARTIFACTS-16: an edit makes the next version, holding the base's files with the changes, and
    `by: "owner"`.
  - ARTIFACTS-17: an edit on an old base is `409`, and an unsigned one `401`.
  - ARTIFACTS-18: an edit with a secret, or over a limit, is refused saying why, and nothing is
    kept.
- **Files:** `core/crates/obid/src/artifacts/edit.rs` (new), `api.rs` (one route),
  `docs/contracts/{artifacts,signed-words}.md`, `docs/api/{artifacts.md,routes.txt}`,
  `tools/obid-spec/workflows/routes/artifacts/post-artifacts-id-versions--*.json`.

#### L3b. The app: edit, save, and versions

- **Lane:** dispatcher. **Depends:** L2, L3a.
- **What:**
  - **Editing.** Code becomes editable. Preview redraws from the edited files as the owner types,
    which is Sandpack's own live update.
  - **Saving.** Save sends L3a's edit, and the result shows as the new version. LibreChat saves
    edits automatically after a pause; Oby takes Sangam's control here.
  - **Versions.** A version menu sits in the panel, shown from two versions on, as LibreChat's
    `ArtifactVersion` is. An older version opens read-only, and saving from it is an edit on that
    base, which obid answers `409` until the owner reloads the latest.
- **Scenarios:**
  - ARTIFACTSAPP-8: an edit shows in Preview before it's saved, and Save makes version n+1.
  - ARTIFACTSAPP-9: the version menu opens any version, and the latest is marked.
  - ARTIFACTSAPP-10: an edit that obid refuses shows why, and the edit stays in Code.
- **Files:** `src/renderer/src/artifacts/` (`Code.tsx`, `Versions.tsx`, `store.ts`),
  `shared/words.ts` (`Edit`).

#### L4. Export and import

- **Lane:** dispatcher. **Depends:** L2, P2 (export and import in obid, built, opens next).
- **What:**
  - **Export.** The panel's menu has Download, as a zip or an HTML file, saved where the owner
    picks, and Copy, for one file's text.
  - **Import.** A project zip or an HTML file dropped into a conversation becomes an artifact of
    that conversation (P2's import), and its panel opens.
- **Scenarios:**
  - ARTIFACTSAPP-11: Download saves the version's zip, which equals obid's export byte for byte,
    or its single HTML.
  - ARTIFACTSAPP-12: a dropped zip becomes an imported artifact with its panel open, and its export
    equals the zip.
- **Files:** `src/renderer/src/artifacts/` (`Menu.tsx`, the drop), `src/main/` (the save dialog),
  `shared/words.ts` (`Import`, as P2 signs it).

#### L5. React artifacts, still local

- **Lane:** dispatcher. **Depends:** L1.
- **What:**
  - `sucrase` compiles `.tsx`/`.jsx` in the renderer, and the import map points at the vendored
    `react`, `react-dom`, `lucide-react` and `@tailwindcss/browser`. Those are served by L1's relay
    server, from `resources/sandpack-vendor/`, with each licence beside it.
  - An artifact whose entry is `App.tsx` with no `index.html` gets one written by the app.
  - obid's side is checked first: whether `check_kind` keeps such a folder as a `page`. If it
    doesn't, an add-only change in its own small obid PR, with its Blok proof.
- **Scenarios:**
  - ARTIFACTSAPP-13: a React artifact using `lucide-react` and Tailwind classes draws, every
    request on loopback.
  - ARTIFACTSAPP-14: an import outside the vendored set shows its error in the preview and fetches
    nothing.
- **Files:** `resources/sandpack-vendor/`, `src/renderer/src/artifacts/react.ts` (new),
  `package.json` (`sucrase`).

#### L6. The real drive

- On the owner's Mac, with their own agents, through the `human-drive` skill:
  - asking for a page in a conversation opens its panel;
  - asking for a change moves the panel to v2;
  - an edit is saved as v3, and v1 opens from the version menu;
  - a download, then an import of that zip;
  - a React artifact draws;
  - the network log of the whole drive shows the preview on loopback only.
- Each finding is a bug issue the same day, checked against boards 1a to 1c.

### What waits until the loop works

- **P2** (export and import in obid) and **P3** (the turn's artifact, #2799) are part of the loop:
  they land as planned.
- **P4** (Oby's runtime for Claude Design boards), **P5** (agent tools), **P6** and **P7** (the
  artifact tab in obid's browser API, and the `oby-artifact://` view) and **P9** to **P13**
  (sharing, the Sphere, public links, Share in the app, Settings) wait, as the owner said.
  - P4 and P9 are built and held (P9 is #2796, disarmed).
  - P7's `oby-artifact://` protocol stays the plan for designs. Sandpack's per-preview origin
    already gives each preview its own origin.
- **P8**'s pieces moved into the loop: the card (L2), Export and Import (L4) and the `artifact` on
  the owner's words (L2). What's left of it, the artifact under its conversation in the sidebar,
  waits with P7.
- Once the loop works in the owner's daily drive, a new plan says what to change to reach the rest
  of Oby's design. That covers designs on Oby's runtime, sharing, and moving the panel from
  Sandpack's frame to the `oby-artifact://` view if that is still wanted.

## What the lab proved

Four lab PRs, all in `tools/obid-spec`, each scenario a workflow red with its rule off:

| Lab PR | Lab file | What it proved | Workflows |
|---|---|---|---|
| #2682 | `src/lab/artifacts.rs`, `artifacts_contract.rs` | store v2: the `design` kind byte for byte, `/_blob/<id>` inside its version, font and video types, a token-like upload id hashed, rename event, versions across a conversation's jobs, Claude's limits, export zip and HTML, import, the turn's artifact | `workflows/lab/artifacts/` (LAB-ARTIFACTS-1 to 7, 19, 23) |
| #2689 | `src/lab/artifactview.rs`, `lab-ui/artifacts/runtime/` | Oby's own `.dc.html` runtime and canvas host (within 0.06% of Claude's own pictures on the Oby Artifacts canvas), the view's CSP, the app's navigation rule, the artifact tab, `oby_artifact_check` | `workflows/lab/artifactview/` (LAB-ARTVIEW-8 to 11) |
| #2709 | `src/lab/artifactsphere.rs`, `lab-ui/artifacts/public/` | sharing in age (one upload, a key per member), the recipient's checks and quarantine, answers and edits back, public links with the key in the `#` | `workflows/lab/artifactsphere/` (LAB-ARTSPHERE-12 to 19, 21, 22) |
| #2713 | `src/lab/artifacts.rs` (spike), `lab-ui/artifacts/guide.md` | headless Claude Code offers no Artifact tool (2.1.325); from Oby's guide alone it makes a design Oby draws with no error ($0.58, 55 s) | `workflows/lab/artifacts/claude-headless.json` (LAB-ARTIFACTS-20) |

## How the port works

- **One PR, one piece, one owner of its files.** No two PRs below own the same file, so sessions
  can run side by side where the order allows; `api.rs` and `shared/words.ts` are touched by
  several, in the order given, never at once.
- **Contracts first in each PR.** A PR moves its scenarios into `docs/contracts/<area>.md` as
  `implemented`, each a test named after it, and its routes into `docs/api/artifacts.md` and
  `docs/api/routes.txt`; `tools/obid-spec/scope.json` reads the new contracts.
- **The Blok proof is the lab workflow, pointed at obid.** Each lab workflow named below is
  rewritten under `tools/obid-spec/workflows/routes/artifacts/` (or `routes/browser/`,
  `workflows/app/`), its steps calling the real obid (`obid-start`, `call`) instead of the lab's
  server, its assertions the lab's. The lab workflow stays, as the lab's own proof.
- **Add-only, as obid's API always is.** Every key and route here is new; none is renamed.

## The port's PRs, P1 to P13

Each says its lane ("Dispatcher, not harness"), what it depends on, its scenarios (new ids, with
the lab scenario each comes from), the files it owns, and its Blok proof.

### P1. Store v2: the design kind, uploads, types, limits, versions across jobs

- **Lane:** dispatcher. **Depends:** none.
- **What:** `design` (Claude Design's `canvas.json` checked and kept byte for byte), obid's and
  Swift's `canvas` shapes, `_blob/<id>` kept under `assets/` with the version's `blobs` map and
  served back inside the version only, a token-like id hashed, `.ttf .otf .mp4 .webm .avif` types,
  Claude's limits (16 MB text, 15 MB binary, 2 MB SVG, 512 files, 256 MB a version; 4 GB a job's
  or a conversation's; 50 versions), a follow-up job of the artifact's conversation versioning it,
  and `artifact {renamed: true}` on a rename. Add-only keys on artifact.json: `profile`, `origin`,
  `shares`, `public`; on a version `blobs`, `job`, `by`.
- **Scenarios** (`docs/contracts/artifacts.md`, new):
  - ARTIFACTS-1: a Claude Design canvas publishes as `design`, its files byte for byte (LAB-ARTIFACTS-1).
  - ARTIFACTS-2: both canvas shapes publish; a design with a missing board, a bad page id, a stray order or launch, no `<x-dc>`, two logic scripts or a missing design system, or holding a link, is refused saying which (LAB-ARTIFACTS-2).
  - ARTIFACTS-3: `{version}/_blob/<id>` serves the upload of that version only; any other id, version or path out is `404` (LAB-ARTIFACTS-3).
  - ARTIFACTS-4: fonts, video and AVIF are served with their types under obid's CSP (LAB-ARTIFACTS-4).
  - ARTIFACTS-5: an upload id that looks like a token is kept under a hashed name, never refused (LAB-ARTIFACTS-1).
  - ARTIFACTS-6: a rename sends `artifact {renamed, title}` (LAB-ARTIFACTS-5).
  - ARTIFACTS-7: a follow-up job of the artifact's conversation makes its next version; another conversation's is refused (LAB-ARTIFACTS-6).
  - ARTIFACTS-8: each of Claude's limits takes its value and refuses one byte or one file over, saying which (LAB-ARTIFACTS-23).
- **Files:** `core/crates/obid/src/artifacts.rs` becomes `artifacts/mod.rs` with `artifacts/design.rs`
  (`check_design`, `check_canvas`), `artifacts/blobs.rs` (`ingest_blobs`, `resolve`, `long_run`) and
  `artifacts/limits.rs`; `docs/contracts/artifacts.md` (new); `docs/api/artifacts.md`;
  `tools/obid-spec/scope.json`. Not `jobs.rs` (it may only shrink): the job's end already calls
  `Artifacts::publish`.
- **Blok proof:** `workflows/lab/artifacts/{design,shapes,blobs,types,rename,versions,limits}.json`
  rewritten as `workflows/routes/artifacts/` against obid (publishing through a conversation's
  `oby_artifact` and a job's `.obi/artifacts/`).

### P2. Export and import

- **Lane:** dispatcher. **Depends:** P1.
- **What:** `GET /v1/artifacts/{id}/export/{version}?as=zip|html` (under `export/`, so a file named
  `export` stays served) and `POST /v1/artifacts/import` (a Claude Design project zip, a page or a
  document; signed by its `Import` label). The zip is deterministic, so an export of an import is
  the import. The phone's gateway may read the export.
- **Scenarios** (`artifacts.md`): ARTIFACTS-9, an import's export equals it entry for entry and byte for byte, and a second round gives the same zip (LAB-ARTIFACTS-1); ARTIFACTS-10, the standalone HTML holds every file and reaches nothing outside itself (LAB-ARTIFACTS-1); ARTIFACTS-11, an import is `origin: import`, titled from `canvas.json`, refused unsigned, and a zip with a link or a path out is refused (LAB-ARTIFACTS-1, 19).
- **Files:** `core/crates/obid/src/artifacts/export.rs` (new); `api.rs` (two routes);
  `gateway.rs` (the export `GET`); `docs/contracts/signed-words.md` (the `Import` shape, WORDS-21);
  `docs/api/artifacts.md`; `docs/api/routes.txt`; `core/Cargo.toml` (`zip`, deflate only).
- **Blok proof:** `workflows/lab/artifacts/design.json`'s import, round trip and export steps, and
  `signed.json`'s import step, as `workflows/routes/artifacts/{post-artifacts-import,get-artifact-export}.json`.

### P3. The turn's artifact

- **Lane:** dispatcher. **Depends:** P1.
- **What:** `POST /v1/plans`' `Request` (`deny_unknown_fields`) gains `artifact: {id, version}`;
  words said while an artifact is open land in its conversation, whatever conversation the app had
  open; an artifact with none gets one on its first words (TASKCONV-4's rule). The brain's briefing
  names the open artifact.
- **Scenarios** (`artifacts.md`): ARTIFACTS-12, words with `artifact` land in its conversation; an import's first words give it one, and its next land there; words naming no artifact are `404` (LAB-ARTIFACTS-7).
- **Files:** `core/crates/obid/src/brain/mod.rs` (the `Request`, the routing);
  `docs/contracts/task-conversations.md` (TASKCONV-4's note); `docs/api/artifacts.md`. The app's
  `wordsOf` shape gains `artifact` in P8, not here.
- **Blok proof:** `workflows/lab/artifacts/turn.json` as `workflows/routes/everything-else/post-plans-artifact.json`.

### P4. Oby's runtime, served beside a design

- **Lane:** dispatcher. **Depends:** P1.
- **What:** Oby's runtime moves from the lab (`lab-ui/artifacts/runtime/`: `dc-runtime.js`,
  `canvas.html`, React 18.3.1 and ReactDOM vendored, MIT) to `core/assets/oby-dc/`, and obid serves,
  for a `design` version, `support.js`, `__dc/boards.js`, `__dc/logic/<board>.js` and the canvas
  host `__canvas.html` (built, never stored), with `BOARD_POLICY` (obid's CSP, `frame-ancestors
  'self'`) and `HOST_POLICY`. A board's `data-props` defaults are its props.
- **Scenarios** (`artifacts.md`): ARTIFACTS-13, every board of a canvas draws within its size with no error, holes, `sc-for`, `sc-if`, `dc-import` and links between boards played in place (LAB-ARTVIEW-8); ARTIFACTS-14, the canvas host shows each board of a page in its own sandboxed frame and switches pages (LAB-ARTVIEW-8); ARTIFACTS-15, a board can't fetch, load from outside or open a window (LAB-ARTVIEW-10, the CSP's part).
- **Files:** `core/assets/oby-dc/` (new); `core/crates/obid/src/artifacts/view.rs` (new: `serve`,
  `board_parts`, `boards_js`, `logic_js`, `canvas_html`); the file route in `artifacts/mod.rs`.
- **Blok proof:** `workflows/lab/artifactview/render.json` (the lab's own fixture and the Oby
  Artifacts canvas) as `workflows/routes/artifacts/get-artifact-design.json`, drawn in the lab's
  pinned chrome-headless-shell through a per-version origin (`lab.artifactview` pointed at obid).
  `claude.json` (the comparison with Claude's runtime) stays a lab workflow: Claude's runtime is
  never in the repository.
- **Note:** the runtime's paths are root-relative (`/__dc/…`, a board's `/_blob/<id>`), which holds
  only on a per-version origin: the app's protocol (P7). Through `/v1/artifacts/{id}/{v}/` (the API,
  the phone) a design shows as its files, not drawn, until the phone has its own per-version scheme.

### P5. Agent tools: every agent can make, check and read an artifact

- **Lane:** dispatcher. **Depends:** P4.
- **What:** jobs get the artifact tools, not only conversations: `oby_artifact` on the job broker
  (`Server::Broker`), the broker written into Claude jobs' `obi-mcp.json`;
  `oby_artifact_check` (draws a folder in the browser engine's chrome-headless-shell, answers a
  picture of each board and each board's mistake by name); `oby_artifact_read` (copies a version
  into the room; reading another Oby's work or a version `by` someone else marks the turn
  untrusted, its text quoted); `oby_artifact_open` (asks the app to show it); the guide
  (`lab-ui/artifacts/guide.md`) as the resource `oby://guide/artifacts`, inlined where an agent
  can't read resources.
- **Scenarios** (`docs/contracts/agent-mcp.md`): AGENTMCP-8, a job's agent lists the four artifact tools and the guide; AGENTMCP-9, `oby_artifact_check` answers a picture per board and a board's thrown and unparsed mistakes by name (LAB-ARTVIEW-11); AGENTMCP-10, `oby_artifact_read` of another Oby's artifact or of a version by another Oby marks the turn and quotes its text (LAB-ARTSPHERE-14, the `by` half of LAB-ARTSPHERE-21).
- **Files:** `core/crates/obid/src/brain/tools.rs`, `mcp.rs`, `connections.rs`,
  `jobs/owner_run.rs`; `core/assets/oby-guide/artifacts.md` (new, from the lab's guide);
  `docs/contracts/agent-mcp.md`. `jobs.rs` may only shrink (`jobs_rs_limit`): Claude jobs'
  `obi-mcp.json` is written there today, so this PR moves that writing into `jobs/owner_run.rs`
  (adding the broker there), and the prompt's one line about `.obi/artifacts/` names the guide in
  place of what it says. `jobs.rs` ends shorter.
- **Blok proof:** `workflows/lab/artifactview/check.json` as `workflows/routes/artifacts/mcp-artifact-check.json`;
  `workflows/lab/artifactsphere/taint.json`'s read steps (proven on a version `by` another Oby until
  P10 brings received artifacts). `claude-headless.json` stays informational in the lab.

### P6. The artifact tab in obid's browser API

- **Lane:** dispatcher. **Depends:** none (the browser's own ports first: the orchestrator sequences it).
- **What:** the wire `Tab` gains `kind` (`page` | `artifact`, missing reads as `page`) and `artifact:
  {id, version, latest}`; `POST /v1/browser/tabs {artifact}` opens or brings forward its tab, signed;
  `…/{id}/version` pins a version or follows the latest; `…/{id}/close`. Take, give back, stream,
  input, a secret and the ways out are refused `artifact_tab`; agents' tools never reach it
  (BROWSER-18's `in_box`); tabs are kept over a restart; a rename retitles a tab, a delete closes it.
- **Scenarios** (`docs/contracts/browser.md`): BROWSER-29, an artifact opens as the person's tab of kind `artifact` on its latest version, the same tab again on a second open, refused unsigned; BROWSER-30, take, stream and input are refused `artifact_tab`, and an agent's token reaches nothing; BROWSER-31, a new version moves a tab following the latest, a pinned one stays; BROWSER-32, the tab is kept over obid's restart and closes when its artifact is deleted (all LAB-ARTVIEW-9).
- **Files:** `core/crates/obid/src/browser/contract/tab.rs`, `contract/artifact_ref.rs` and
  `contract/open_artifact_body.rs` (new), `contract/mod.rs` (the routes and the reason),
  `browser/routes.rs`, `browser/service.rs`; `docs/contracts/browser.md`; `docs/api/browser.md`.
- **Blok proof:** `workflows/lab/artifactview/tab.json` as `workflows/routes/browser/post-browser-tabs-artifact.json`.

### P7. The app: the artifact tab, drawn by the app

- **Lane:** dispatcher. **Depends:** P4, P6.
- **What:** Electron's main serves `oby-artifact://<id>-v<n>/<path>` (`protocol.handle`) from obid
  with the token main holds, one origin per version (the orchestrator accepted it on 2026-11-20:
  a board's root-relative `/_blob/<id>` and `/Board.dc.html` stay inside its version), passing
  obid's CSP through; the tab is a `WebContentsView` (`sandbox`, `contextIsolation`, no preload,
  partition `oby-artifacts`); every navigation, window and request outside the version is denied,
  and a link to an address outside is asked of the person first (the lab's `navigation` rule). The
  browser panel draws a `kind: artifact` tab with that view instead of the frame stream, full width
  by default, its address line and strip as Sangam's boards 1b and 1c have them.
- **Scenarios** (`docs/contracts/browser-app.md`): BROWSERAPP-21, an artifact tab shows its version from `oby-artifact://<id>-v<n>/`, a design drawn by Oby's runtime; BROWSERAPP-22, a board can't navigate the view, open a window or reach outside, and a link out is asked first (LAB-ARTVIEW-10); BROWSERAPP-23, opening an artifact asks for full width; BROWSERAPP-24, the address line and strip read as the board says.
- **Files:** `apps/electron/src/main/artifacts.ts` (new: the protocol, the view, the navigation
  rule), `main/browser.ts`, `main/index.ts`; `renderer/src/browser/Panel.tsx`, `BrowserPane.tsx`,
  `store.tsx`, `live.ts`; `shared/words.ts` (open, pin, close); `docs/contracts/browser-app.md`.
- **Blok proof:** `workflows/lab/artifactview/sandbox.json` as `workflows/app/artifact-tab-*.json`
  (`app.launch` on a scratch obid, `app.read`, `app.compare` with boards 1b and 1c of the Oby
  Artifacts canvas).

### P8. The app: artifacts in their conversation, Export and Import

- **Lane:** dispatcher. **Depends:** P2, P3, P7.
- **What:** artifacts nested under the conversation that made them (`nested.ts`, `kidsOf`, from
  `GET /v1/artifacts?conversation=`); words said while an artifact is open carry `artifact`
  (`wordsOf` gains the key); the tab's Export menu (zip, HTML) and Import by dragging a project zip
  into a conversation; the artifact card in a conversation (board 1a).
- **Scenarios** (`browser-app.md`): BROWSERAPP-25, an artifact sits under its conversation; BROWSERAPP-26, words said with an artifact open reach its conversation; BROWSERAPP-27, Export saves the zip or the HTML where the person picks, signed; BROWSERAPP-28, a dropped project zip becomes an imported design.
- **Files:** `apps/electron/src/renderer/src/nested.ts`, the composer's send, the browser
  panel's menu, a new `renderer/src/artifacts/` for the card; `shared/words.ts` (the turn's
  `artifact`, Export, Import).
- **Blok proof:** `workflows/lab/artifacts/turn.json` and `design.json`'s export steps, driven
  through the app (`workflows/app/artifact-*.json`), compared with board 1a.

### P9. Sharing in obid: the format and the recipient's checks

- **Lane:** between owners. **Depends:** P1. No Sphere change.
- **What:** the pure half of sharing: a version as a deterministic tar; age (the `age` crate, see
  "Decided") with one file key wrapped per recipient; `unpack` (no link, no path out); the
  recipient's checks in order (hash of the blob, decrypt, hash of the tar, Claude's limits, the
  secret scan, a scan that can't run refusing); files that instruct agents set apart in
  `home/artifacts/quarantine/<id>/`; quoting what another Oby sent.
- **Scenarios** (`docs/contracts/artifact-sharing.md`, new): ARTSHARE-1, one blob for a group, a wrapped key per member, opened by each member and nobody else (LAB-ARTSPHERE-12); ARTSHARE-2, a blob whose hash differs, a link, a path out, a file over a limit or a found secret is refused before anything is kept, saying why (LAB-ARTSPHERE-13); ARTSHARE-3, `CLAUDE.md`, `AGENTS.md` and `.claude/` are set apart, the rest kept (LAB-ARTSPHERE-13).
- **Files:** `core/crates/obid/src/artifacts/seal.rs` (new, pure); `docs/contracts/artifact-sharing.md`
  (new); `core/Cargo.toml` (`age` 0.11, `tar`).
- **Blok proof:** unit tests named after the scenarios (the lab's `artifactsphere` core tests,
  moved); `workflows/lab/artifactsphere/receive.json`'s forged offers wait for P10, which has a
  Sphere to put them on.

### P10. Sharing over the Sphere: offers, updates, answers, edits back

- **Lane:** between owners. **Depends:** P9, and the Obysphere's Artifacts server, merged into
  oby-sphere's `dev` as `d2ed70bf` (`oby-sphere#109`, which carries #107 and #108 and the fixes of
  an independent security review). Until a member's Sphere offers it, the feature is off for that
  member: obid offers no `artifacts` and every share route refuses in words.
- **The server's names, which replace the lab's.** P10 takes these, not the lab's:
  - the feature is `artifacts`, as `/v0.1/features` lists it (the lab's `artifacts.v1`);
  - a share is sealed to the recipient card's X25519 key;
  - a blob's id is the Sphere's UUID;
  - expiry is RFC 3339;
  - a refusal by the Sphere's rules is `policy_denied`;
  - a recipient's Oby confirms what it kept with the new action `artifact_receipt`;
  - a blob goes up in parts of at most 64 MiB (option B, below), checked in order, part 1 first.
- **Its own PR first:** oby's `e2e/sphere.ref` moves to `d2ed70bf`, so the Blok workflows below
  run against the real server.
- **What:** the actions `artifact_offer`, `artifact_update`, `artifact_version`, `artifact_revoke`,
  `artifact_deleted`, `artifact_receipt` and the replies `answer`, `decline`; `shares.json` per artifact; the routes
  `GET/POST /v1/artifacts/{id}/shares`, `DELETE …/shares/{to}`, `POST …/answers`, `POST …/sendBack`;
  `DELETE /v1/artifacts/{id}` revoking every share and the public link first; a received artifact
  kept with `origin: sphere` in the thread with its owner; an inbox item needing the owner only
  when it carries an ask; taint for the room's received artifacts (`harness.rs`'s list).
- **Scenarios** (`artifact-sharing.md`): ARTSHARE-4, sending scans first and asks before files that instruct agents (LAB-ARTSPHERE-13); ARTSHARE-5, a new version reaches every share following the latest in one blob, a pinned one stays, a revoke deletes the blob and the copy says taken back (LAB-ARTSPHERE-15); ARTSHARE-6, a delete revokes every share and the public link first (LAB-ARTIFACTS-5); ARTSHARE-7, an answer shows quoted in the owner's conversation and member thread, view-only can't answer, a decline and an expiry say so (LAB-ARTSPHERE-16); ARTSHARE-8, a member without `artifacts` and a policy refusal (`policy_denied`) are said in words naming the admin (LAB-ARTSPHERE-18); ARTSHARE-9, every share act is refused unsigned (LAB-ARTSPHERE-19); ARTSHARE-10, a member given edit sends a version back, checked like anything received, kept `by` their Oby, nothing of it reaching the owner's agents, view-only refused both sides (LAB-ARTSPHERE-21).
- **Files:** `core/crates/obid/src/sphere/artifacts.rs` (new), `sphere/link.rs` (`FEATURES` gains
  `artifacts`), `sphere/routes.rs`, `brain/harness.rs` (the taint list),
  `core/crates/obi-protocol/src/types.rs` (the actions, add-only); `api.rs` (the share routes);
  `gateway.rs` (`GET …/shares`); `docs/contracts/artifact-sharing.md`; `docs/api/artifacts.md`;
  `docs/api/routes.txt`; `docs/contracts/signed-words.md` (WORDS-22: `Share with …`, `Remove`,
  the recipient's `{text}`).
- **Blok proof:** `workflows/lab/artifactsphere/{share,receive,taint,update,answer,refusals,signed,edit}.json`
  as `workflows/routes/artifacts/*-shares*.json`, two scratch obids on the run's local Sphere (the
  pinned `e2e/sphere.ref`, at `d2ed70bf` from its own PR above).

### P11. Public links in obid

- **Lane:** between owners. **Depends:** P10, and the Sphere's public links, merged with its
  Artifacts server (`d2ed70bf`, `oby-sphere#109`).
- **The server's side, as built.**
  - **Its own feature:** public links are `public_links` in `/v0.1/features`, apart from
    `artifacts`. obid makes no link for a member whose Sphere doesn't list it, and says so.
  - **Their own origin:** links are served only on the Sphere's separate `PUBLIC_ORIGIN`, never
    on its API's.
  - **The URL:** the `PUT`'s answer gives the link's `url`; obid adds `#<key>` itself, so the key
    never reaches the Sphere.
  - **Expiry:** `expires_at` defaults to 30 days.
  - **Quota:** 64 links and 1 GiB a member.
  - **The viewer:** the Sphere serves its own `viewer.js`, pinned by SRI
    `sha384-3KvRtu6DBv1CGhAUw/retmki3mwiTYD2tirg/BHT1YOVYjJDxhrWM5XsgcuHCtuq`.
- **What:** `POST /v1/artifacts/{id}/public {acknowledged: "anyone-with-the-link"}`, signed: a key
  pair per link, the version sealed to it, only ciphertext uploaded, the whole link answered once
  and its key kept only in the vault (`oby.artifact.<id>.link`, 0600), never in artifact.json; a new
  version going up again under the same link and key; `DELETE …/public`. The viewer is the Sphere's `viewer.js` at the SRI above, the lab's
  (`lab-ui/artifacts/public/`: the page, its script, typage vendored, BSD-3) as it was taken there;
  obid checks the pin on the first view, and a different hash is a link it refuses to show.
- **Scenarios** (`artifact-sharing.md`): ARTSHARE-11, no link without `acknowledged`; the Sphere holds one age file and no plaintext, and never receives the key; the vault keeps it, artifact.json never (LAB-ARTSPHERE-17); ARTSHARE-12, the link opens in a browser, decrypts there, shows the files and only the invite, and says so once taken back (LAB-ARTSPHERE-17, 22).
- **Files:** `core/crates/obid/src/sphere/public.rs` (new); `api.rs` (two routes);
  `docs/contracts/artifact-sharing.md`; `docs/api/artifacts.md`; `docs/api/routes.txt`;
  `docs/contracts/signed-words.md` (WORDS-23: `Share and make a link` with `acknowledged`).
- **Blok proof:** `workflows/lab/artifactsphere/public.json` as `workflows/routes/artifacts/post-artifact-public.json`.

### P12. The app: Share, Shared, and what others shared

- **Lane:** between owners. **Depends:** P7, P10, P11.
- **What:** the tab's Share menu (one member, everyone, a public link after its warning, view,
  comment or edit, follow the latest), the Sphere's Shared group, a member's Oby showing what was
  shared with it (board 2a), answers and versions in the artifact's conversation, and the public
  link's copy and take-back. Every word on these screens is Sangam's (below).
- **Scenarios** (`browser-app.md`): BROWSERAPP-29 to 32, one per board (1c, 1e, 2a, 2c's warning).
- **Files:** `apps/electron/src/renderer/src/artifacts/` (the Share menu, the Shared rows),
  `renderer/src/PeerThread.tsx` and `Home.tsx` (what a member was shared, board 2a),
  `renderer/src/SphereTile.tsx` (the Shared group), `shared/words.ts` (the share shapes).
- **Blok proof:** `workflows/lab/artifactsphere/{share,answer,public}.json` driven through the app,
  compared with boards 1c, 1e, 2a and 2c.

### P13. Settings › Artifacts

- **Lane:** dispatcher. **Depends:** P5.
- **What:** `GET/PUT /v1/settings/artifacts` (`default: oby | claude`, `agent: auto | <account>`,
  `openFullWidth`), signed by its control's label; `claude` refused `claude_unavailable` while
  `lab.artifacts.claude_headless` says Claude Code offers no Artifact tool headless, and the app says
  why; board 2b.
- **Scenarios** (`artifacts.md`): new ids (ARTIFACTS-16 to 18 are L3a's, built): the settings read and change signed, and `claude` refused `claude_unavailable` in words.
- **Files:** `core/crates/obid/src/artifacts/settings.rs` (new); `api.rs`; the app's Settings
  section; `docs/api/artifacts.md`.
- **Blok proof:** new: the lab never built settings, so its workflow is written with this PR, red
  first (`workflows/routes/artifacts/settings-artifacts.json`).

## The order

| PR | Depends on | Can run beside | State |
|---|---|---|---|
| P1 Store v2 | none | | merged (#2776) |
| P2 Export and import | P1 | L1, L2, L3a | merged (#2798) |
| P3 The turn’s artifact | P1 | L1, L3a | merged (#2799) |
| **L1** Sandpack, local, the card and Preview | P1 | P2, L3a | merged (#2807) |
| **L2** Code, following the agent | L1, P3 | L3a | merged (#2827) |
| **L3a** The owner's edit (obid) | P1 | L1, L2 | merged (#2809) |
| **L3b** Edit, save, versions (app) | L2, L3a | L5 | merged (#2836) |
| **L4** Export and import (app) | L2, P2 | L5 | merged (#2847) |
| **L5** React artifacts, local | L1 | L3b, L4 | |
| **L6** The real drive | L1 to L5 | none | |
| P4 Oby's runtime | P1 | | after the loop (built, held) |
| P5 Agent tools | P4 | | after the loop |
| P6 The artifact tab (obid) | the browser's own ports | | after the loop |
| P7 The artifact tab (app) | P4, P6 | | after the loop |
| P8 What's left: under its conversation | P7 | | after the loop |
| P9 Sharing: format and checks | P1 | | after the loop (#2796, disarmed) |
| P10 Sharing over the Sphere | P9, the Sphere's server (merged, `d2ed70bf`) | | after the loop |
| P11 Public links | P10, the Sphere's public links (merged, `d2ed70bf`) | | after the loop |
| P12 Share, Shared (app) | P7, P10, P11 | | after the loop |
| P13 Settings › Artifacts | P5 | | after the loop |

- In review, no promise: #2845 (a change asked with an artifact open becomes its next version, ARTIFACTS-19) and the artifact card task.
- Now: P2 and P3 land as planned; L1 and L3a side by side, each owning its files; then L2, L3b, L4
  and L5; L6 last. `api.rs` is touched by P2 and L3a, and `shared/words.ts` by L2, L3b and L4:
  those land one after another, never at once.
- Every PR runs `scripts/preflight.sh` before its push; each obid PR's Blok
  proof runs in its own PR, red before its code and green after.

## Decided

- **The local loop first, like LibreChat, with Sandpack** (the owner, 2026-11-21): L1 to L6 before
  P4 to P13. Sandpack runs local only: our relay, the `static` template, React compiled in the app,
  no Nodebox, no hosted bundler or CDN.

- **`age`, the crate** (the orchestrator, 2026-11-20): obid takes `age` 0.11, not its own writing of
  the format: hand-written cryptography is the riskier way to save crates. The cost: `age` 0.11
  requires `i18n-embed` and `i18n-embed-fl` (they aren't optional in its Cargo.toml), 68 packages in
  the lock, and a future-Rust warning from `proc-macro-error2`, a dependency's.
- **One origin per version** for the app's protocol, `oby-artifact://<id>-v<n>/` (the orchestrator,
  2026-11-20), with the reason above (P7).
- **The blob cap: B, parts of at most 64 MiB** (built by the Sphere, `oby-sphere#109`). A version
  may be 256 MB (Claude's limit, the owner's decision). obid cuts its ciphertext into parts of at
  most 64 MiB, named in order in the offer (`blobs`, add-only). The Sphere checks them in order,
  part 1 first, and obid checks each part's hash and joins them before it decrypts. It shapes P10's
  offer and P11's upload.
- **The Claude bridge stays the project zip** (the owner, 2026-11-20); #2713 found no Artifact tool
  headless on Claude Code 2.1.325, and its workflow checks again on each release.

## Open

- **Sangam: the invite's words and motion, and every screen's words.** The public link's invite
  (board 2c: Oby hopping and blinking, "Get Oby for Mac", "I have Oby: open it there", Reduce
  Motion respected) is his to design; the lab's words and animation are proposals, and so are the
  words on boards 1a to 1e, 2a and 2b. P7, P8, P11 and P12 take his words and his screens; their
  scenarios name the board, never the words.

## Sources

- LibreChat (`danny-avila/LibreChat`, MIT): `client/src/components/Artifacts/`,
  `client/src/utils/artifacts.ts`; Sandpack (`codesandbox/sandpack`, Apache-2.0):
  `sandpack-client/src/clients/static/`; `codesandbox/static-browser-server` 1.0.3; the licences of
  `codesandbox-client`, `sandpack-bundler`, `@codesandbox/nodebox` 0.1.8; all read 2026-11-21.

- The Oby Artifacts backend plan (§3, §6, §7, §8) and its design canvas (boards 1a to 2c).
- The lab: #2682, #2689, #2709, #2713 (`tools/obid-spec/src/lab/artifacts*.rs`, `artifactview.rs`,
  `artifactsphere.rs`, `lab-ui/artifacts/`, `workflows/lab/artifacts*/`).
- `origin/dev` at `820d33c4`: `core/crates/obid/src/{artifacts.rs, api.rs, gateway.rs, brain/{mod,tools,harness}.rs, mcp.rs, connections.rs, jobs.rs, browser/**, sphere/**}`,
  `core/crates/obi-protocol/src/types.rs`, `apps/electron/src/{main,renderer/src/browser,shared}/`,
  `docs/contracts/{browser,browser-app,agent-mcp,signed-words,obid-spec,task-conversations}.md`,
  `docs/api/{artifacts.md,routes.txt}`, `tools/obid-spec/scope.json`, `core/crates/obid/src/jobs/owner_run.rs`.
