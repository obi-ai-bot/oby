# What a React artifact runs on, served by the app

A React artifact's page (docs/contracts/artifacts-app.md, ARTIFACTSAPP-13) imports these files from
its own origin: main's relay serves each `.mjs` and `.js` here at `/__csb_vendor/<folder>/<file>`
(`src/main/sandpackRelay.ts`), so nothing of it comes from a CDN. Each folder holds its package's
own `LICENSE`, copied from the package.

| Folder | Package | Version | Licence | Files |
|---|---|---|---|---|
| `react/` | react | 19.3.0 | MIT | `react.mjs`, `jsx-runtime.mjs` |
| `react-dom/` | react-dom | 19.3.0 | MIT | `react-dom.mjs`, `client.mjs` |
| `scheduler/` | scheduler (built into `react-dom/client.mjs`) | 0.28.0 | MIT | its `LICENSE` |
| `lucide-react/` | lucide-react (its main entry: no `DynamicIcon`, no `dynamicIconImports`) | 1.55.0 | ISC, with MIT for the icons derived from Feather, both in its `LICENSE` | `lucide-react.mjs` |
| `tailwindcss-browser/` | @tailwindcss/browser (its own `dist/index.global.js`, byte for byte) | 4.3.5 | MIT | `index.global.js` |

The versions are `package-lock.json`'s. React ships CommonJS only, so each `.mjs` is built from the
installed packages with Vite's `build()` in library mode (ES, minified, `NODE_ENV` production);
every file but `react.mjs` keeps `react` outside it, and `client.mjs` `react-dom` too, so a page
has one React. To make them again, after the packages change:

    npm run vendor:sandpack

The same packages give the same bytes (`artifactsReact.test.ts` builds them again and compares).
The set grows only by vendoring: one package and its licence at a time.
