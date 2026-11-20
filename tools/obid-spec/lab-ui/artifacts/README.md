# Oby Artifacts, in the lab

The plan: the Oby Artifacts backend plan (§3 to §7). The design it serves: the Oby Artifacts canvas,
whose files are `design/project/` here (people's names replaced).

- `src/contract.ts`: the TypeScript twin of `src/lab/artifacts_contract.rs`, the routes and shapes
  the app will talk to. The Rust file is the source; `the_typescript_twin_matches` fails until the
  two agree.
- `runtime/`: Oby's own runtime of Claude Design's `.dc.html` format (`dc-runtime.js`), written
  from the format alone, and the canvas host (`canvas.html`). obid serves them beside a `design`
  version (`src/lab/artifactview.rs`): `support.js` is React 18.3.1 and ReactDOM (`runtime/vendor/`,
  MIT, from npm's `react@18.3.1` and `react-dom@18.3.1`, their production UMD builds unchanged) then
  `dc-runtime.js`. No Babel, nothing from a CDN.
- `design/project/`: the canvas, as render fixtures (`design/README.md`).
- `scripts/`: the workflows' generators (`workflows/lab/artifacts/`, `workflows/lab/artifactview/`).
  Edit them, not the JSON.

Claude's own runtime is not here and never will be. `lab.artifactview.vsclaude` compares pictures
with it only when `OBID_SPEC_CLAUDE_DC_RUNTIME` names a copy of it somewhere else.
