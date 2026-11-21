/// <reference types="vitest/config" />
import { readFileSync, existsSync, statSync } from 'node:fs';
import { resolve, extname, join } from 'node:path';
import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
// @ts-expect-error a plain script, shared with electron.vite.config.ts
import { localOnly } from './scripts/local-only.mjs';

// Oby lives in design/oby (Sangam's playground copy, ahead of character/) and
// stays standalone. In dev it's served at /oby/; a build copies the page and
// three.js in next to the app.
const OBY = resolve(__dirname, '../../design/oby');
const OBY_FILES = ['oby.html', 'obys.html', 'vendor/three.module.js'];
const TYPES: Record<string, string> = { '.html': 'text/html', '.js': 'text/javascript' };

function oby(): Plugin {
  return {
    name: 'oby',
    configureServer(server) {
      server.middlewares.use('/oby', (req, res, next) => {
        const file = join(OBY, decodeURIComponent((req.url || '/').split('?')[0]));
        if (!file.startsWith(OBY) || !existsSync(file) || !statSync(file).isFile()) return next();
        res.setHeader('Content-Type', TYPES[extname(file)] || 'application/octet-stream');
        res.end(readFileSync(file));
      });
    },
    generateBundle() {
      for (const f of OBY_FILES) this.emitFile({ type: 'asset', fileName: `oby/${f}`, source: readFileSync(join(OBY, f)) });
    },
  };
}

// The page's Content-Security-Policy, in what a build ships (the dev server's own inline script
// needs more): its own scripts only, no remote content, no plugins, no other base. The page reaches
// obid only through main (preload), never over the network itself. Inline styles: React's style={}.
const CSP = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'none'; frame-src 'self'; object-src 'none'; base-uri 'none'";
// The artifact preview's host page (preview.html, main/artifactPreview.ts), in its own view and
// partition: its own script, and frames only from main's relay on loopback (`<id>-preview.localhost`,
// main/sandpackRelay.ts). It reaches nothing else, obid neither.
const PREVIEW_HOST_CSP = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; frame-src http://*.localhost:*; object-src 'none'; base-uri 'none'; form-action 'none'";
const csp = (): Plugin => ({
  name: 'csp',
  apply: 'build',
  transformIndexHtml: (_html, ctx) => [{ tag: 'meta', attrs: { 'http-equiv': 'Content-Security-Policy', content: ctx.filename.endsWith('preview.html') ? PREVIEW_HOST_CSP : CSP }, injectTo: 'head-prepend' }],
});

// Sandpack's static client alone (docs/contracts/artifacts-app.md): the chunk sandpack-client's own
// index loads for the `static` template, built in by itself, so neither its `runtime` client (a hosted
// bundler) nor its `node` client (Nodebox) is part of the app. Found from the index's own imports.
const SANDPACK = resolve(__dirname, 'node_modules/@codesandbox/sandpack-client/dist');
function sandpackStatic(): string {
  const index = existsSync(join(SANDPACK, 'index.mjs')) ? readFileSync(join(SANDPACK, 'index.mjs'), 'utf8') : '';
  const chunks = [...index.matchAll(/import\('\.\/(index-[0-9a-f]+\.mjs)'\)/g)].map((m) => m[1]);
  return chunks.length === 1 ? join(SANDPACK, chunks[0]) : join(SANDPACK, 'no-static-client.mjs');
}

export default defineConfig({
  root: resolve(__dirname, 'src/renderer'),
  base: './',
  plugins: [localOnly(), react(), oby(), csp()],
  resolve: { alias: { 'oby-sandpack-static': sandpackStatic() } },
  server: { port: Number(process.env.PORT) || 5175, strictPort: true },
  // the favicons, from Oby's icon folder (apps/icon/exports/web): served in dev, copied by a build
  publicDir: resolve(__dirname, '../icon/exports/web'),
  build: { outDir: resolve(__dirname, 'out/renderer'), emptyOutDir: true, rollupOptions: { input: { index: resolve(__dirname, 'src/renderer/index.html'), preview: resolve(__dirname, 'src/renderer/preview.html') } } },
  // a test's first import of the app (a cold transform, a busy Mac) can outlast vitest's 5 s, and
  // what the timed-out test goes on to call lands in the next test's (#1839); a beforeAll that does
  // that first import (personCard's) gets as long, past vitest's 10 s for hooks
  test: { testTimeout: 30_000, hookTimeout: 30_000 },
});
