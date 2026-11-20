# The Oby Artifacts canvas, as render fixtures

`project/` is the Claude Design canvas the Oby Artifacts plan serves, exported at version
`1795193792-f4a5`: its `canvas.json`, its twelve `.dc.html` boards, their stylesheets and the `oby`
design system's copy. People's names are replaced (Ana, Ben), a board named after one is renamed
`BenHome.dc.html`, and its claude.ai links are cut; nothing else changed.

`lab.artifacts.fixture` with `name: canvas` reads it into a workflow's folder, so the
`artifactview` workflows publish it and draw each board with Oby's own runtime
(`../runtime/dc-runtime.js`). Claude's runtime is not here and never will be: Oby's is written from
the format alone.
