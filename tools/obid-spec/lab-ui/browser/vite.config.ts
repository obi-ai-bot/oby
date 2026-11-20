// The lab's browser screens (lane B of the browser plan): a Vite + React app on the real app's own
// styles and pieces, read straight from apps/electron (read-only), so the components can move into
// the app as files. Two more things it serves:
//   /board/<Name>.html  a design board (design/boards, from the approved Claude Design canvas)
//                       rendered offline, for the side-by-side picture and the box-by-box compare;
//   /v1/*               lane A's browser API (live mode), its token and the device's signature
//                       added here, never in the page.
import { readFileSync, existsSync } from 'node:fs';
import { createHash, createPrivateKey, sign } from 'node:crypto';
import { Readable } from 'node:stream';
import { resolve, join, basename } from 'node:path';
import { defineConfig, type Connect, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';

const HERE = __dirname;
const APP = resolve(HERE, '../../../../apps/electron/src/renderer/src');
const DESIGN = resolve(HERE, 'design');

// The boards' images, by the id the canvas stored them under
const BLOBS: Record<string, string> = {
  '3446da5a8daa6f8aa4ff0d9358936562': 'oby',
  b6a520d0d66c6d8e93fb0c106328e021: 'codex',
  b9591b423efbb6b607314ffefffd24be: 'claude',
  e57918fdb33aeaf1d73c6e7aeb4175ae: 'cursor',
};

// A board's states, for the one board with logic (Hold: Give back and Take control on the canvas)
const STATES: Record<string, Record<string, Record<string, unknown>>> = {
  Hold: {
    held: { held: true, free: false, gaveBack: false, metaLine: 'Waiting for you · Codex · acme-web', cardDot: 'is-held', viewHeld: 'is-held', pwFocus: 'is-focus' },
    gaveBack: { held: false, free: true, gaveBack: true, metaLine: 'Running · Codex · acme-web', cardDot: 'is-working', viewHeld: '', pwFocus: '' },
  },
};

/** A .dc.html board as a plain page: no canvas runtime, its images local, the dark theme. */
export function boardPage(name: string, state?: string): string | null {
  const file = join(DESIGN, 'boards', `${basename(name)}.dc.html`);
  if (!existsSync(file)) return null;
  const src = readFileSync(file, 'utf8');
  const dc = src.split('<x-dc>')[1]?.split('</x-dc>')[0] ?? '';
  const helmet = (dc.match(/<helmet>([\s\S]*?)<\/helmet>/)?.[1] ?? '')
    // no web fonts: the system font stands first in Oby's stack, on the Mac it is the one drawn
    .replace(/<link[^>]*fonts\.googleapis[^>]*>/g, '');
  let body = dc.replace(/<helmet>[\s\S]*?<\/helmet>/, '');
  const vals: Record<string, unknown> = { theme: '', true: true, false: false, ...(STATES[name]?.[state ?? Object.keys(STATES[name] ?? {})[0]] ?? {}) };
  // sc-if: kept when its value holds, dropped when not (innermost first)
  for (let i = 0; i < 6; i++) {
    body = body.replace(/<sc-if value="\{\{(\w+)\}\}"[^>]*>((?:(?!<sc-if)[\s\S])*?)<\/sc-if>/g, (_m, k: string, inner: string) => (vals[k] ? inner : ''));
  }
  body = body.replace(/\s(onClick)="\{\{\w+\}\}"/g, '').replace(/\{\{(\w+)\}\}/g, (_m, k: string) => String(vals[k] ?? ''));
  body = body.replace(/\/_blob\/([0-9a-f]{32})/g, (_m, id: string) => `/board/blobs/${id}.svg`);
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>${name} (design board)</title>
<link rel="stylesheet" href="/board/oby.css">${helmet}</head><body>${body}</body></html>`;
}

function boards(): Plugin {
  const serve: Connect.NextHandleFunction = (req, res, next) => {
    const url = new URL(req.url ?? '/', 'http://lab');
    const path = decodeURIComponent(url.pathname);
    if (!path.startsWith('/board/')) return next();
    const rest = path.slice('/board/'.length);
    if (rest === 'oby.css') {
      res.setHeader('Content-Type', 'text/css');
      return res.end(readFileSync(join(DESIGN, 'oby.css')));
    }
    const blob = rest.match(/^blobs\/([0-9a-f]{32})\.svg$/);
    if (blob && BLOBS[blob[1]]) {
      res.setHeader('Content-Type', 'image/svg+xml');
      return res.end(readFileSync(join(DESIGN, 'blobs', `${blob[1]}.svg`)));
    }
    const page = rest.match(/^([A-Za-z-]+)\.html$/);
    const html = page && boardPage(page[1], url.searchParams.get('state') ?? undefined);
    if (!html) return next();
    res.setHeader('Content-Type', 'text/html; charset=utf-8');
    res.end(html);
  };
  return {
    name: 'design-boards',
    configureServer: (server) => { server.middlewares.use(serve); },
    configurePreviewServer: (server) => { server.middlewares.use(serve); },
  };
}

// Live mode: this server stands where the app's main process will (the plan's gate 5): it holds
// lane A's owner token and a device key of the owner's, so the page never sees either, and it signs
// the owner's acts on the browser (X-Oby-Words: the device's P-256 key over the exact body, as
// keys.rs's words_header and the contract say). In the lab it signs what the page sends; the app's
// main signs only right after a keystroke or click it saw (docs/contracts/signed-words.md).
const API = process.env.LAB_BROWSER_API;
const TOKEN = process.env.LAB_BROWSER_TOKEN;
const DEVICE = process.env.LAB_BROWSER_DEVICE; // "<id> <secret hex> <public key base64>"

function deviceKey() {
  if (!DEVICE) return null;
  const [id, secret, pub] = DEVICE.split(' ');
  const point = Buffer.from(pub, 'base64'); // 0x04 ‖ x ‖ y
  const b64u = (b: Buffer) => b.toString('base64url');
  const key = createPrivateKey({ key: { kty: 'EC', crv: 'P-256', d: b64u(Buffer.from(secret, 'hex')), x: b64u(point.subarray(1, 33)), y: b64u(point.subarray(33, 65)) }, format: 'jwk' });
  return { id, key };
}

/** X-Oby-Words for one request: `<device> <signedAt> <signature>`. */
function words(method: string, path: string, body: Buffer): string | null {
  const d = deviceKey();
  if (!d) return null;
  const at = new Date().toISOString();
  const digest = createHash('sha256').update(body).digest('hex');
  const message = `oby-words\n${method}\n${path.split('?')[0]}\n${digest}\n${at}`;
  const signature = sign('sha256', Buffer.from(message), { key: d.key, dsaEncoding: 'der' }).toString('base64');
  return `${d.id} ${at} ${signature}`;
}

function live(): Plugin {
  const serve: Connect.NextHandleFunction = (req, res, next) => {
    if (!req.url?.startsWith('/v1/')) return next();
    if (!API || !TOKEN) {
      res.statusCode = 503;
      res.setHeader('Content-Type', 'application/json');
      return res.end(JSON.stringify({ error: 'This lab server has no browser API (LAB_BROWSER_API).', reason: 'not_found' }));
    }
    const chunks: Buffer[] = [];
    req.on('data', (c: Buffer) => chunks.push(c));
    req.on('end', async () => {
      const body = Buffer.concat(chunks);
      const method = req.method ?? 'GET';
      const headers: Record<string, string> = { Authorization: `Bearer ${TOKEN}` };
      if (req.headers['content-type']) headers['Content-Type'] = String(req.headers['content-type']);
      if (req.headers.accept) headers.Accept = String(req.headers.accept);
      // the owner's acts are signed; reads are not
      if (method !== 'GET') { const w = words(method, req.url!, body); if (w) headers['X-Oby-Words'] = w; }
      try {
        const ctl = new AbortController();
        res.on('close', () => ctl.abort());
        const r = await fetch(`${API.replace(/\/$/, '')}${req.url}`, { method, headers, body: method === 'GET' ? undefined : body, signal: ctl.signal });
        res.statusCode = r.status;
        r.headers.forEach((v, k) => { if (!['content-length', 'transfer-encoding', 'connection'].includes(k)) res.setHeader(k, v); });
        // an event stream goes out as it comes: the preview server's compression would hold small
        // events back until its buffer fills
        if (r.headers.get('content-type')?.includes('text/event-stream')) { res.setHeader('Cache-Control', 'no-cache, no-transform'); res.flushHeaders(); }
        if (!r.body) return res.end();
        Readable.fromWeb(r.body as never).on('error', () => res.end()).pipe(res);
      } catch (e) {
        if (!res.headersSent) { res.statusCode = 502; res.setHeader('Content-Type', 'application/json'); }
        res.end(JSON.stringify({ error: String((e as Error).message), reason: 'not_found' }));
      }
    });
  };
  return {
    name: 'lab-live',
    configureServer: (server) => { server.middlewares.use(serve); },
    configurePreviewServer: (server) => { server.middlewares.use(serve); },
  };
}

export default defineConfig({
  root: HERE,
  base: '/',
  plugins: [react(), boards(), live()],
  resolve: {
    alias: { '@app': APP },
    // the app's files import react and the icon sets: from this app's node_modules, one copy
    dedupe: ['react', 'react-dom', 'lucide-react', '@phosphor-icons/react'],
  },
  // 127.0.0.1 only; the app's styles and pieces are read from apps/electron, never written
  server: { host: '127.0.0.1', port: Number(process.env.PORT) || 5190, strictPort: false, fs: { allow: [HERE, APP] } },
  preview: { host: '127.0.0.1' },
  build: { outDir: process.env.LAB_UI_OUT || resolve(HERE, 'dist'), emptyOutDir: true },
});
