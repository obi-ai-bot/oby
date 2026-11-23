# Oby's runtime of Claude Design's `.dc.html` format

What obid serves beside a `design` artifact's version so its boards draw
([docs/contracts/artifacts.md](../../../docs/contracts/artifacts.md), ARTIFACTS-13 to 15; the code is
`core/crates/obid/src/artifacts/view.rs`, which includes these files in obid at build time). Claude
Design's own runtime can't run under obid's artifact policy (it evaluates strings as code and
fetches its board), so Oby has its own, written from the format alone, never from another
runtime's code.

- `dc-runtime.js`: Oby's runtime (Apache-2.0, as Oby is): holes, `sc-for`, `sc-if`, `dc-import`,
  `x-import`, helmets, links between boards played in place, `class Component extends DCLogic`.
- `canvas.html`: the canvas host (Apache-2.0): every board of a page in a sandboxed frame of its own,
  its pages, notes and Play. obid puts the version's `canvas.json` in it.
- `react.production.min.js` and `react-dom.production.min.js`: React 18.3.1 and ReactDOM 18.3.1, the
  production UMD builds of npm's `react@18.3.1` and `react-dom@18.3.1`, unchanged (MIT,
  `LICENSE-react`). Their SHA-256s are pinned by `the_vendored_react_is_react_18_3_1` in `view.rs`:
  - `react.production.min.js` `d949f1c3687aedadcedac85261865f29b17cd273997e7f6b2bfc53b2f9d4c4dd`
  - `react-dom.production.min.js` `35f4f974f4b2bcd44da73963347f8952e341f83909e4498227d4e26b98f66f0d`

They were proven in the Blok lab first (`tools/obid-spec/src/lab/artifactview.rs`, #2689), which now
reads them from here: one copy, obid's.
