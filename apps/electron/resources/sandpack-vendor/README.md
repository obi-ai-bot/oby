# What a React artifact runs on, served by the app

A React artifact's page (docs/contracts/artifacts-app.md, ARTIFACTSAPP-13) imports these files from
its own origin: main's relay serves each `.mjs` and `.js` here at `/__csb_vendor/<folder>/<file>`
(`src/main/sandpackRelay.ts`), so nothing of it comes from a CDN. Each folder holds its package's
own `LICENSE`, copied from the package. Each built folder (not `scheduler/` or `tailwindcss-browser/`)
also holds `THIRD_PARTY_LICENSES`: a section for every package its build took in, found from the
build's module ids (the nearest `package.json` with a name and a version), sorted by name, each
headed `--- <name>@<version> (<licence>) ---` and holding that package's own licence file.

| Folder | Package | Version | Licence | Files |
|---|---|---|---|---|
| `react/` | react | 19.3.0 | MIT | `react.mjs`, `jsx-runtime.mjs` |
| `react-dom/` | react-dom | 19.3.0 | MIT | `react-dom.mjs`, `client.mjs` |
| `scheduler/` | scheduler (built into `react-dom/client.mjs`) | 0.28.0 | MIT | its `LICENSE` |
| `lucide-react/` | lucide-react (its main entry: no `DynamicIcon`, no `dynamicIconImports`) | 1.55.0 | ISC, with MIT for the icons derived from Feather, both in its `LICENSE` | `lucide-react.mjs` |
| `tailwindcss-browser/` | @tailwindcss/browser (its own `dist/index.global.js`, byte for byte) | 4.3.5 | MIT | `index.global.js` |
| `recharts/` | recharts (with react-is 19.3.0, the d3 parts through `victory-vendor`, redux, `@reduxjs/toolkit`, react-redux, immer, reselect, es-toolkit and the rest its `THIRD_PARTY_LICENSES` lists) | 3.11.0 | MIT; the d3 parts ISC (victory-vendor, which re-exports them, says `MIT AND ISC AND BSD-3-Clause`), the rest MIT | `recharts.mjs` |
| `clsx/` | clsx | 2.1.1 | MIT | `clsx.mjs` |
| `tailwind-merge/` | tailwind-merge | 3.7.1 | MIT | `tailwind-merge.mjs` |
| `class-variance-authority/` | class-variance-authority (with its own clsx 2.1.1) | 0.7.1 | Apache-2.0 | `class-variance-authority.mjs` |
| `shadcn/` | LibreChat's shadcn/ui sources (`danny-avila/LibreChat` at `4700c44`, after shadcn/ui, in `../shadcn-src/`): accordion, alert, alert-dialog, avatar, badge, breadcrumb, button, card, checkbox, collapsible, dialog, dropdown-menu, input, label, menubar, navigation-menu, pagination, popover, progress, radio-group, select, separator, skeleton, slider, switch, table, tabs, textarea, toast, toaster, toggle, toggle-group, tooltip and use-toast, and `cn`; built with the 23 `@radix-ui/react-*` devDependencies and what they bundle (the rest of Radix, `@floating-ui/*`, react-remove-scroll, aria-hidden, tslib and the rest its `THIRD_PARTY_LICENSES` lists) | `4700c44` | MIT (LibreChat's and shadcn/ui's), what it bundles MIT, tslib 0BSD | `ui/<name>.mjs`, `lib/utils.mjs`, `chunks/` |

The versions are `package-lock.json`'s. React ships CommonJS only, so each `.mjs` is built from the
installed packages with Vite's `build()` in library mode (ES, minified, `NODE_ENV` production);
an ES package's entry is `export * from` it, as its own `package.json` says it is ES. Every file but
`react.mjs` keeps `react` outside it, and `client.mjs` and `recharts.mjs` `react-dom` too, so a page
has one React. `shadcn/` is one build with an entry for each component and one for `cn`, so the
Radix code they share is in `chunks/` once (one `DismissableLayer`); react, react-dom,
`react/jsx-runtime`, lucide-react, clsx, tailwind-merge and class-variance-authority stay outside it,
each the one copy above. Its `LICENSE` is `../shadcn-src/LICENSE`, LibreChat's MIT licence and then
shadcn/ui's. react-remove-scroll-bar 2.3.8's npm files hold no licence: its section is its
repository's `LICENSE` (MIT, at `7301c16`), kept in `../licences/react-remove-scroll-bar/`. To make
them again, after the packages change:

    npm run vendor:sandpack

It prints, as one line of JSON, each built folder's packages, the list its `THIRD_PARTY_LICENSES`
was written from.

The same packages give the same bytes (`artifactsReact.test.ts` builds them again and compares).
The set grows only by vendoring: one package and its licence at a time.
