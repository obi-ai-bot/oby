# Sandpack's relay, served by the app

These four files are `static-browser-server` 1.0.3's built relay (`out/preview/` in its npm
package), byte for byte: the version `@codesandbox/sandpack-client` 2.19.8 pins. Its package.json
says Apache-2.0, and the package has no licence file, so `LICENSE` here is the Apache-2.0 text.

Electron's main serves them on 127.0.0.1 for `<id>-preview.localhost` (`src/main/sandpackRelay.ts`,
docs/contracts/artifacts-app.md), so Sandpack's `static` template never reaches CodeSandbox's
hosted copy. Main changes two lines of the service worker as it serves it, never this copy: a
request to another origin goes to the network (where the preview's CSP and its partition stop it)
instead of being answered from the artifact's files, and every answer carries the preview's CSP.
`sandpackRelay.test.ts` fails if these files differ from the package's, or if either change no
longer finds its line.
