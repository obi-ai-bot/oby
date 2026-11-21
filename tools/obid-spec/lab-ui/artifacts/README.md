# Oby Artifacts, in the lab

The plan: the Oby Artifacts backend plan (§3 to §7). The design it serves: the Oby Artifacts canvas,
whose files are `design/project/` here (people's names replaced).

- `src/contract.ts`: the TypeScript twin of `src/lab/artifacts_contract.rs`, the routes and shapes
  the app will talk to. The Rust file is the source; `the_typescript_twin_matches` fails until the
  two agree.
- Oby's own runtime of Claude Design's `.dc.html` format, its canvas host and React 18.3.1 moved
  to obid with the port plan's P4: `core/assets/oby-dc/` (its README). The lab's view
  (`src/lab/artifactview.rs`) reads them from there.
- `design/project/`: the canvas, as render fixtures (`design/README.md`).
- `scripts/`: the workflows' generators (`workflows/lab/artifacts/`, `workflows/lab/artifactview/`).
  Edit them, not the JSON.

Claude's own runtime is not here and never will be. `lab.artifactview.vsclaude` compares pictures
with it only when `OBID_SPEC_CLAUDE_DC_RUNTIME` names a copy of it somewhere else.
