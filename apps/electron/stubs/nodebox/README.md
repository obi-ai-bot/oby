# Not Nodebox

`@codesandbox/sandpack-client` depends on `@codesandbox/nodebox` for its `node` client. Nodebox is
under the Sustainable Use License, which isn't open source, so it is never part of Oby: npm's
`overrides` in `apps/electron/package.json` put this empty package in its place, and nothing is
downloaded. Oby uses only Sandpack's `static` client (docs/contracts/artifacts-app.md,
ARTIFACTSAPP-2), and the build fails if Nodebox's code, or a hosted bundler's or CDN's address,
reaches `out/` (`scripts/local-only.mjs`).
