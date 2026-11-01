import { readFileSync, existsSync, statSync } from 'node:fs';
import { resolve, extname, join } from 'node:path';
import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';

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
const csp = (): Plugin => ({
  name: 'csp',
  apply: 'build',
  transformIndexHtml: () => [{ tag: 'meta', attrs: { 'http-equiv': 'Content-Security-Policy', content: CSP }, injectTo: 'head-prepend' }],
});

export default defineConfig({
  root: resolve(__dirname, 'src/renderer'),
  base: './',
  plugins: [react(), oby(), csp()],
  server: { port: Number(process.env.PORT) || 5175, strictPort: true },
  // the favicons, from Oby's icon folder (apps/icon/exports/web): served in dev, copied by a build
  publicDir: resolve(__dirname, '../icon/exports/web'),
  build: { outDir: resolve(__dirname, 'out/renderer'), emptyOutDir: true },
});
