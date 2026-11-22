import { transform, type Transform } from 'sucrase';
import { bytes, escaped, page, text, type Shown, type Version } from './showable';

// A React artifact's page (docs/contracts/artifacts-app.md, ARTIFACTSAPP-13 and 14): an `html`
// version with an App.tsx or App.jsx and no index.html, compiled here in the preview's host page,
// as LibreChat's react-ts template draws one, but on what main's relay serves (/__csb_vendor/),
// never a CDN. Each source becomes its `.js`; the written index.html's import map sends React's
// names and shadcn's components to the vendored builds and each file's own URL, with and without its extension, to its
// `.js`, so the artifact's imports run as they are written. A bare import outside the map is named
// before anything runs, with what the preview has (ARTIFACTSAPP-28); a relative, absolute or URL one
// is the browser's and the CSP's to answer.

const TRANSFORMS: Record<string, Transform[]> = { tsx: ['typescript', 'jsx'], ts: ['typescript'], jsx: ['jsx'], js: ['jsx'] };
const VENDORED: Record<string, string> = {
  react: '/__csb_vendor/react/react.mjs',
  'react/jsx-runtime': '/__csb_vendor/react/jsx-runtime.mjs',
  'react-dom': '/__csb_vendor/react-dom/react-dom.mjs',
  'react-dom/client': '/__csb_vendor/react-dom/client.mjs',
  'lucide-react': '/__csb_vendor/lucide-react/lucide-react.mjs',
  recharts: '/__csb_vendor/recharts/recharts.mjs',
  clsx: '/__csb_vendor/clsx/clsx.mjs',
  'tailwind-merge': '/__csb_vendor/tailwind-merge/tailwind-merge.mjs',
  'class-variance-authority': '/__csb_vendor/class-variance-authority/class-variance-authority.mjs',
};
/** shadcn/ui's components, as LibreChat ships them (ARTIFACTSAPP-27), built into `shadcn/` beside their `cn`. */
const SHADCN = ['accordion', 'alert', 'alert-dialog', 'avatar', 'badge', 'breadcrumb', 'button', 'card', 'checkbox', 'collapsible', 'dialog', 'dropdown-menu', 'input', 'label', 'menubar', 'navigation-menu', 'pagination', 'popover', 'progress', 'radio-group', 'select', 'separator', 'skeleton', 'slider', 'switch', 'table', 'tabs', 'textarea', 'toast', 'toaster', 'toggle', 'toggle-group', 'tooltip', 'use-toast'];
const COMPONENTS: Record<string, string> = {
  ...Object.fromEntries(SHADCN.map((n) => [`/components/ui/${n}`, `/__csb_vendor/shadcn/ui/${n}.mjs`])),
  '/lib/utils': '/__csb_vendor/shadcn/lib/utils.mjs',
};
/** An error's words wrap and sit inset from the frame's edge, never cut off at it. */
const ERROR = 'padding:16px;margin:0;white-space:pre-wrap';
const WRAP = `pre{${ERROR}}`;
/** What a stylesheet's import gets: nothing, its file linked from the head instead. */
const EMPTY = '/__oby_empty.js';

// MIT License
//
// Copyright (c) 2024 Hassan El Mghari
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
//
// From Nutlope/llamacoder, `lib/preview/html.ts` (`findMissingPreviewModules`) at commit 5d57b70: a
// static or re-exported import's specifier; the second, a literal dynamic import's, is Oby's
const IMPORTED = [/(?:import|export)\s+(?:[^"']*?\s+from\s+)?["']([^"']+)["']/g, /import\(\s*["']([^"']+)["']\s*\)/g];
/** A specifier the import map must hold: not a path (`/`, `./`, `../`) nor a URL (`<scheme>:`). */
const bare = (s: string) => !/^(?:\/|\.\.?\/|[a-z][a-z\d+.-]*:)/i.test(s);
const names = Object.keys(VENDORED).filter((k) => !k.includes('/'));
// ponytail: proposed words, for Sangam to change
const missing = (gone: string[]) => `This preview has no ${gone.map((n) => `\`${n}\``).join(', ')}. It has: ${names.join(', ')} and shadcn/ui at /components/ui/<name>.`;

// App's default export mounted in #root, committed at once, then the bridge told on a timer, as
// LlamaCoder's page tells it (ARTIFACTSAPP-22); its errors are the bridge's to say, the page left as
// it drew. An App.js that won't load shows its own text in a <pre>, marked as the page's error
const MOUNT = `import { createElement } from 'react';
import { flushSync } from 'react-dom';
import { createRoot } from 'react-dom/client';
window.__obyReact = true;
import('/App.js').then((app) => { flushSync(() => { createRoot(document.getElementById('root')).render(createElement(app.default)); }); setTimeout(() => { window.__obyRendered?.(); }, 0); }, (e) => { const pre = document.createElement('pre'); pre.style.cssText = ${JSON.stringify(ERROR)}; pre.setAttribute('data-oby-error', ''); pre.textContent = e instanceof Error ? e.message : String(e); document.body.replaceChildren(pre); window.__obyRendered?.(); });`;

/** The files the preview shows for a React artifact's version, every path from `/`. */
export function reactShown(v: Version): Shown {
  const shown: Shown = {};
  const imports: Record<string, string> = { ...VENDORED, ...COMPONENTS };
  const sheets: string[] = [];
  const asked: string[] = [];
  for (const p of Object.keys(v.files).sort()) {
    const f = v.files[p];
    const source = /^(.*)\.(tsx|ts|jsx|js)$/i.exec(p);
    if (!source) {
      shown[`/${p}`] = { code: 'text' in f ? f.text : bytes(f) };
      if (/\.css$/i.test(p)) { imports[`/${p}`] = EMPTY; sheets.push(p); }
      continue;
    }
    const [, base, ext] = source;
    const js = `/${base}.js`;
    try {
      shown[js] = { code: transform(text(f), { transforms: TRANSFORMS[ext.toLowerCase()], jsxRuntime: 'automatic', production: true, filePath: p }).code };
    } catch (e) {
      return refused(v, e instanceof Error ? e.message : String(e));
    }
    // sucrase's output: its react/jsx-runtime import added, TypeScript's type-only ones gone
    const found = IMPORTED.flatMap((re) => [...(shown[js].code as string).matchAll(re)]).sort((a, b) => a.index - b.index);
    asked.push(...found.map((m) => m[1]).filter(bare));
    imports[`/${p}`] = js;
    imports[`/${base}`] = js;
    // ponytail: a folder's `/dir` only; an import map's `/dir/` key is a prefix and must map to a folder
    const folder = /^(.+)\/index$/.exec(base);
    if (folder) imports[`/${folder[1]}`] = js;
  }
  // ponytail: `@/` for shadcn's keys only, one key each, to the vendored file or the artifact's own
  for (const k of Object.keys(COMPONENTS)) imports[`@${k}`] = imports[k];
  const gone = [...new Set(asked.filter((s) => !Object.hasOwn(imports, s)))];
  if (gone.length) return refused(v, missing(gone));
  if (sheets.length) shown[EMPTY] = { code: 'export {};\n' };
  const head = `<script type="importmap">${JSON.stringify({ imports }).replace(/</g, '\\u003c')}</script>`
    + sheets.map((p) => `<link rel="stylesheet" href="/${escaped(encodeURI(p))}">`).join('')
    + '<script src="/__csb_vendor/tailwindcss-browser/index.global.js"></script>';
  shown['/index.html'] = { code: page(v.title, '', `<div id="root"></div><script type="module">${MOUNT}</script>`, head) };
  return shown;
}

/** Only a page saying why nothing runs, marked as its error (ARTIFACTSAPP-22). */
function refused(v: Version, why: string): Shown {
  return { '/index.html': { code: page(v.title, WRAP, `<pre data-oby-error>${escaped(why)}</pre>`) } };
}
