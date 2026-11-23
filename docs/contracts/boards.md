# The real window held to the browser boards

The business behavior contract for `apps/electron/scripts/boards.mjs`, from [the template](../business-contract-template.md). Scenarios are numbered `BOARDS-n`; the tests named after them are in `apps/electron/scripts/boards.test.mjs` (`node --test scripts/boards.test.mjs` from `apps/electron`).

## Feature and status

- **Outcome:** Anyone with this checkout and a Mac holds the built app's real window to the 17 browser boards of the approved design, with no obid-spec and no Rust: one command launches each board's demo screen, compares it with its board piece by piece and word for word, and says exactly which piece is off, by how much, or which words differ. A later macOS CI job runs the same script.
- **Status:** Each scenario says whether it's implemented. The browser's own behavior, and why each piece is left out of a board, stay in [browser-app.md](browser-app.md).
- **Scope:** the browser boards only (`tools/obid-spec/lab-ui/browser/design/boards/`, the 17 of BROWSERAPP-1 to 20). **Out of scope:** the Artifacts boards, which draw their screens from the board's own script, which `boardFile()` doesn't run; obid-spec's `app.*` workflows (`tools/obid-spec/workflows/app/browser-*.json`), which keep running as they do.

## Behavior

### Actors and ownership

- **`boards.mjs`** (a plain node script) runs every board and judges it. It adds no state, endpoint, tool or model call to obid: no obid runs, and the window shows the app's demo data.
- **`drive-app.mjs`**, spawned unchanged, one helper per board, launches the built app hidden and answers one JSON line per request, as it does for obid-spec's `app.*` nodes.
- **`board-compare.mjs`** draws a board as a plain page and judges the window's pieces against it (`judge()`), for this script and for obid-spec's `app.compare` alike.

### What it runs

`node scripts/boards.mjs <out-folder> [board numbers…]`, from `apps/electron`. With no out folder it prints its usage and exits 1. The board numbers limit the run to those boards, matched by number (`01` is board 1). A number that names no board (`18`, a typo) is refused with `no board <arg>` and the usage line, exit 1, before anything launches: a run never passes on boards it didn't run.

For each board of `apps/electron/scripts/boards/browser.json`, in order:

1. It spawns `node scripts/drive-app.mjs` with its own environment and launches the app on the demo data (`mock`) at the board's `query` (`browser=<n>`), `size` and `storage` (`oby.chatPanelOpen`).
2. For each check: its `before` steps (a drive-app `click`, `press` or `type`, sent as written, or a `wait`, a `read` asked again for up to 10 s until something is found), then `compare` with the board, its `state`, `tolerance` (4px) and `skip`, asked again for up to 10 s while it doesn't hold, the last answer kept; then a side-by-side picture.
3. A board with no check runs its `before` steps and takes only its picture.
4. `close`; the helper has ended before the next board starts.

### What it reads

- `apps/electron/scripts/boards/browser.json`: the 17 boards, each with its checks or a `gap` saying why it has none.
- The design folder: `tools/obid-spec/lab-ui/browser/design` (its `boards/`, `oby.css` and `blobs/`), or the folder `OBY_BOARDS_DIR` names, read once when `board-compare.mjs` loads. A copy with one word changed is how a miss is shown on the real window.
- The built app: `out/main/index.js`, `out/renderer/index.html` and Electron's binary in `node_modules`.

### What it writes

- `<out>/<nn>-<Board>[-<state>].json` for each check: `n`, `board`, `state`, `skip`, the judge's `result` (or the `error` a helper failure gave) and the `pageErrors` `close` returned.
- `<out>/<nn>-<Board>.json` for a board with no check: `n`, `board`, `gap` and `pageErrors`.
- `<out>/<nn>-<Board>[-<state>]-beside.png`: the board left, the window right.
- `<out>/result.json`: the port, the scratch home, every check's pass or fail with its miss lines, and the known gaps.
- One line per check (`holds`, or each of its misses) and one line per known gap, printed.

### Readiness and the port

- Before anything launches, the build must be there and current: both `out/` files and Electron's binary exist, and the older `out/` file is not older than any file under `src/` (but `node_modules`, where vitest keeps its cache). Otherwise it exits 1 saying to run `npm ci` and `npm run build` in `apps/electron`.
- The port the windows are given for obid is a free one of 127.0.0.1, taken as obid-spec's `idle_port()` takes one, or the one `OBY_BOARDS_PORT` names. 7777 is the owner's obid and is refused; so are a port something listens on and an `OBY_BOARDS_PORT` that isn't a port. Each says to unset `OBY_BOARDS_PORT` or name a free port other than 7777, and nothing launches.
- The scratch home is a new folder under the system's temporary folder: `no-obid/` (obid's home for every launch) and one `<nn>/` per board with its own `userData/`, so no board inherits another's stored choices.

### Judging

- Every held piece (in `PIECES`, or in `HELD` for that board alone, not in `TODAY`, not in the check's `skip`) both sides draw is compared box by box, within the tolerance, and word for word; a held piece drawn a different number of times on each side is a miss too.
- A check where no held piece was compared fails as **nothing compared** (`within: false`, `nothingCompared: true`) instead of passing. obid-spec's `app.compare` shares `judge()` through `drive-app.mjs`, so a workflow's compare that compares nothing fails there too.
- **Board 1 (Closed) holds the globe's tip** (`HELD` in `board-compare.mjs`: `.tip-under`, held on that board alone), the one piece it draws that isn't today's app. Its words and its place match the board: it ends 8px from the window's right edge.

### Failures and recovery

- A request the helper refuses (`ok: false`) fails that check, and every later check of its board, with its error; the board still ends in `close` and the run goes on to the next board.
- It exits 0 when every check holds and removes the scratch home. Otherwise it exits 1 after printing every miss line, and keeps the scratch home, printing its path.
- Each miss line starts with the board's number and name, and its state in brackets: a piece off (`.br-tab[0] 6px off`, with both boxes' x, y, w and h), words that differ (both texts), counts that differ (both counts), `nothing compared`, or the helper's error.

### Known gaps

- **Skipped pieces**, by board, each for the reason [browser-app.md](browser-app.md#not-matched-within-4px-and-why) gives: board 3 `.br-tabs-end .icon-button`, `.br-tab`; board 4 `.br-address`; board 5 `.br-tab`; board 7 `.decision-card`, `.tc-ask-line`, `.tc-ask-text`; board 8 `.decision-card`, `.tc-ask-line`, `.secret-row`, `.secret-note`; boards 11, 12 and 14 `.br-tabs-end .icon-button`, `.br-address`, `.br-control-text`, `.br-control .tc-option`. Every run prints them.
- **Boards with no check**: 9 (Asks) and 10 (PageStates), sheets of pieces side by side; 13 (SettingsBrowser), today's Settings with more groups than the board draws; 15 (Desktop), the Mac's own screen around the pill. Each is pictured, and its words are checked in its obid-spec workflow.
- **The skip lists exist twice**, in `browser.json` and in `BOARD_SKIPS` of `apps/electron/scripts/browser-workflows.py`, until `browser-workflows.py` reads `browser.json`. BOARDS-1's test holds them equal.

### Invariants

- The app is launched only through `drive-app.mjs`, hidden, never on port 7777 and never on the owner's obid home.
- The design folder is read, never written; the board pages are written under each board's scratch.
- Nothing a run writes is meant for git: its out folder is the caller's.

## Interfaces and dependencies

`drive-app.mjs`'s JSON lines (`launch`, `click`, `press`, `type`, `read`, `compare`, `sidebyside`, `close`), `board-compare.mjs`'s `DESIGN`, `boardFile()` and `judge()`, `drive-lib.mjs`'s `appRoot` and `electronBinary`, and `playwright-core` and Electron from `apps/electron`'s own packages. No runner API changes. It needs macOS, where the built app runs, and no Rust. The contract gets no line in `tools/obid-spec/scope.json`: the coverage gate reads only the contracts listed there.

## Acceptance scenarios

- **BOARDS-1** (implemented) Given a built app, when `boards.mjs` runs, then every one of the 17 boards in `browser.json` is launched and pictured, every check is judged, and it exits 0 with today's skips listed.
- **BOARDS-2** (implemented) Given a board whose word differs from the window's in a held piece, when it runs, then it exits 1 naming the board, the piece and both words.
- **BOARDS-3** (implemented) Given a held piece more than its tolerance off, when it runs, then it exits 1 naming the board, the piece, the pixels and both boxes.
- **BOARDS-4** (implemented) Given a check where no held piece is on either side, when it is judged, then it fails as "nothing compared" instead of passing.
- **BOARDS-5** (implemented) Given no build, a build older than `src/`, port 7777 or a port something listens on, when it starts, then it exits 1 before launching anything, saying what to do.
