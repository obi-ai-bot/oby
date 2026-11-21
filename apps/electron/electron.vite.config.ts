import { defineConfig } from 'electron-vite';
import renderer from './vite.config';
import { buildInfo } from './buildInfo';
// @ts-expect-error a plain script, shared with vite.config.ts
import { localOnly } from './scripts/local-only.mjs';

// the commit, and a scratch build's obid, baked into main and the page (src/shared/build.ts)
const define = buildInfo();

export default defineConfig({
  // the app ships no node_modules (electron-builder.yml): main's packages, the updater and the
  // js-yaml that parses the feed as it does (shared/updates.ts, verifyFeed), go in its bundle
  main: { define, plugins: [localOnly()], build: { externalizeDeps: { exclude: ['electron-updater', 'js-yaml'] } } },
  // the window is sandboxed (main/index.ts): a sandboxed preload can't be an ES module, and may
  // require only electron's own renderer modules (its imports are bundled in)
  preload: { build: { rollupOptions: { output: { format: 'cjs' } } } },
  renderer: { ...renderer, define: { ...renderer.define, ...define } },
});
