import { transform, type Transform } from 'sucrase';
import { bytes, escaped, page, text, type Shown, type Version } from './showable';

// A React artifact's page (docs/contracts/artifacts-app.md, ARTIFACTSAPP-13 and 14): an `html`
// version with an App.tsx or App.jsx and no index.html, compiled here in the preview's host page,
// as LibreChat's react-ts template draws one, but on what main's relay serves (/__csb_vendor/),
// never a CDN. Each source becomes its `.js`; the written index.html's import map sends React's
// names to the vendored builds and each file's own URL, with and without its extension, to its
// `.js`, so the artifact's imports run as they are written. An import outside the map is the
// browser's own resolve error, shown in the page; a bare name outside it is never fetched.

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
/** An error's words wrap and sit inset from the frame's edge, never cut off at it. */
const ERROR = 'padding:16px;margin:0;white-space:pre-wrap';
const WRAP = `pre{${ERROR}}`;
/** What a stylesheet's import gets: nothing, its file linked from the head instead. */
const EMPTY = '/__oby_empty.js';

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
  const imports: Record<string, string> = { ...VENDORED };
  const sheets: string[] = [];
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
      return { '/index.html': { code: page(v.title, WRAP, `<pre data-oby-error>${escaped(e instanceof Error ? e.message : String(e))}</pre>`) } };
    }
    imports[`/${p}`] = js;
    imports[`/${base}`] = js;
    // ponytail: a folder's `/dir` only; an import map's `/dir/` key is a prefix and must map to a folder
    const folder = /^(.+)\/index$/.exec(base);
    if (folder) imports[`/${folder[1]}`] = js;
  }
  if (sheets.length) shown[EMPTY] = { code: 'export {};\n' };
  const head = `<script type="importmap">${JSON.stringify({ imports }).replace(/</g, '\\u003c')}</script>`
    + sheets.map((p) => `<link rel="stylesheet" href="/${escaped(encodeURI(p))}">`).join('')
    + '<script src="/__csb_vendor/tailwindcss-browser/index.global.js"></script>';
  shown['/index.html'] = { code: page(v.title, '', `<div id="root"></div><script type="module">${MOUNT}</script>`, head) };
  return shown;
}
