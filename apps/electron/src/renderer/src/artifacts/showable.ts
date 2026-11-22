import { marked } from 'marked';
import { reactShown } from './react';

// What Sandpack's `static` template is given for one version of an artifact (docs/contracts/
// artifacts-app.md, ARTIFACTSAPP-1), as LibreChat gives it: the template always opens `index.html`,
// so a page is its own files, and a document or a picture gets an `index.html` written around it.
// Main reads the version's files from obid and hands them over (main/artifactPreview.ts); this
// decides what the preview shows, in the preview's host page (artifacts/host.ts).

/** One file of a version, as main read it: text, or bytes that aren't UTF-8 as base64. */
export type ReadFile = { text: string } | { base64: string };
export type Version = { kind: string; title: string; files: Record<string, ReadFile> };
/** What Sandpack's static template takes: each path from `/`, its text or its bytes. */
export type Shown = Record<string, { code: string | Uint8Array }>;

export const bytes = (f: ReadFile): Uint8Array => 'text' in f ? new TextEncoder().encode(f.text) : Uint8Array.from(atob(f.base64), (c) => c.charCodeAt(0));
export const text = (f: ReadFile): string => 'text' in f ? f.text : new TextDecoder().decode(bytes(f));
export const escaped = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
export const page = (title: string, style: string, body: string, head = '') =>
  `<!doctype html>\n<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>${escaped(title)}</title><style>${style}</style>${head}</head><body>${body}</body></html>`;

const DOCUMENT = 'body{margin:0;padding:24px 28px;font:15px/1.55 -apple-system,BlinkMacSystemFont,"Helvetica Neue",sans-serif;color:#1d1d1f;background:#fff}pre,code{font:13px/1.45 ui-monospace,Menlo,monospace}pre{background:#f5f5f7;padding:12px;border-radius:8px;overflow:auto}img{max-width:100%}table{border-collapse:collapse}td,th{border:1px solid #d2d2d7;padding:4px 8px}';
const PICTURE = 'html,body{margin:0;height:100%;background:#fff}body{display:flex;align-items:center;justify-content:center}img{max-width:100%;max-height:100%}';

const TYPES: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml', avif: 'image/avif' };
const ext = (p: string) => p.slice(p.lastIndexOf('.') + 1).toLowerCase();

// A page's Tailwind CDN script outside a comment (ARTIFACTSAPP-21): the Play CDN, or @tailwindcss/browser
// on jsDelivr or unpkg. Its src becomes the bundled Tailwind main's relay serves, and the page's own
// `tailwind.config = …` gets a `tailwind` to set. The hosts stay escaped, so the build's localOnly passes.
const TAILWIND_CDN = /<!--[\s\S]*?-->|(<script\b[^>]*?\ssrc\s*=\s*)(["']?)https:\/\/(?:cdn\.tailwindcss\.com(?:[/?#][^"'\s>]*)?|(?:cdn\.jsdelivr\.net\/npm|unpkg\.com)\/@tailwindcss\/browser(?:@[^/?#"'\s>]*)?(?:[/?#][^"'\s>]*)?)\2(?=[\s>])/gi;
const bundledTailwind = (html: string) => html.replace(TAILWIND_CDN, (m, tag?: string, q = '') => tag === undefined ? m : `<script>window.tailwind ??= {};</script>${tag}${q}/__csb_vendor/tailwindcss-browser/index.global.js${q}`);

/** A React artifact (ARTIFACTSAPP-13): an `html` version with an App.tsx or App.jsx and no index.html. */
export const isReact = (v: Version): boolean => v.kind === 'html' && !v.files['index.html'] && !!(v.files['App.tsx'] || v.files['App.jsx']);

/** The files the preview shows for a version, every path from `/`. */
export function showable(v: Version): Shown {
  const paths = Object.keys(v.files).sort();
  const first = (test: (p: string) => boolean) => paths.find(test);
  if (v.kind === 'markdown') {
    const md = first((p) => /\.(md|markdown)$/i.test(p)) ?? paths[0];
    // marked leaves raw HTML in: it runs in the preview's own frame, under its CSP, as a page would
    return { '/index.html': { code: page(v.title, DOCUMENT, md ? marked.parse(text(v.files[md]), { async: false }) as string : '') } };
  }
  if (v.kind === 'image') {
    const pic = first((p) => ext(p) in TYPES) ?? paths[0];
    const shown: Shown = { '/index.html': { code: page(v.title, PICTURE, pic ? `<img src="./${encodeURI(pic)}" alt="${escaped(v.title)}">` : '') } };
    if (pic) shown[`/${pic}`] = { code: bytes(v.files[pic]) };
    return shown;
  }
  // a React artifact, compiled
  if (isReact(v)) return reactShown(v);
  // a page, a canvas, a design (drawn by Oby's runtime later, P4): its own files, index.html first
  const shown: Shown = {};
  for (const p of paths) {
    const f = v.files[p];
    shown[`/${p}`] = { code: 'text' in f ? (v.kind === 'html' && /\.html$/i.test(p) ? bundledTailwind(f.text) : f.text) : bytes(f) };
  }
  if (!shown['/index.html']) {
    const list = paths.map((p) => `<li><a href="./${encodeURI(p)}">${escaped(p)}</a></li>`).join('');
    shown['/index.html'] = { code: page(v.title, DOCUMENT, `<h1>${escaped(v.title)}</h1><ul>${list}</ul>`) };
  }
  return shown;
}
